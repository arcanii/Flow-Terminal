//! What Linux says about itself: the user's languages and locale, the
//! computer for the startup logo's lines, its memory banks and buses for the
//! Workstation's PROM, and the distribution's logo.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use flow_core::hardware::Hardware;
use flow_core::platform::Machine;

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// The user's languages, most wanted first ("ja-JP"), as gettext reads
/// them: LANGUAGE's list, then the locale (LC_ALL, LC_MESSAGES or LANG).
pub fn languages() -> Vec<String> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(var);
    languages_from(var("LANGUAGE").as_deref(), locale.as_deref())
}

fn languages_from(language: Option<&str>, locale: Option<&str>) -> Vec<String> {
    // The C locale's messages are English, and gettext passes over LANGUAGE
    // in it. Said outright, or the core would go by LANG, past an LC_ALL or
    // LC_MESSAGES of C.
    if locale.is_some_and(|l| bcp47(l).is_none()) {
        return vec!["en".into()];
    }
    let mut out: Vec<String> = Vec::new();
    let list = if locale.is_none() { None } else { language };
    for tag in list.into_iter().flat_map(|l| l.split(':')).chain(locale).filter_map(bcp47) {
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// "ja_JP.UTF-8", "de_DE@euro" or "pt" as "ja-JP", "de-DE" or "pt"; None for
/// C and POSIX, which name no language.
fn bcp47(locale: &str) -> Option<String> {
    let name = locale.split(['.', '@']).next().unwrap_or_default();
    if name.is_empty() || name == "C" || name == "POSIX" {
        return None;
    }
    Some(name.replace('_', "-"))
}

/// A UTF-8 locale for the shells when Flow was started with none, as by a
/// launcher that cleared the environment: programs would fall back to
/// ASCII otherwise. C.UTF-8 is built into glibc and musl.
pub fn shell_env() -> Vec<(String, String)> {
    if ["LC_ALL", "LC_CTYPE", "LANG"].into_iter().any(|v| var(v).is_some()) {
        return Vec::new();
    }
    vec![("LANG".into(), "C.UTF-8".into())]
}

fn os_release() -> Option<String> {
    ["/etc/os-release", "/usr/lib/os-release"].into_iter().find_map(|p| std::fs::read_to_string(p).ok())
}

/// The computer: the distribution's name, the processor and the memory,
/// read once.
pub fn machine() -> Machine {
    static MACHINE: OnceLock<Machine> = OnceLock::new();
    MACHINE.get_or_init(read_machine).clone()
}

/// The memory banks, read once and kept, and the buses' slots, read
/// afresh, so a tab opened after a device is plugged in shows it: from
/// udev's database and sysfs (`flow_core::hardware::linux`).
pub fn hardware() -> Hardware {
    use flow_core::hardware::{linux, Bank};
    static BANKS: OnceLock<Vec<Bank>> = OnceLock::new();
    let root = Path::new("/");
    Hardware { banks: BANKS.get_or_init(|| linux::banks(root)).clone(), probes: linux::probes(root) }
}

fn read_machine() -> Machine {
    let read = |path: &str| std::fs::read_to_string(path).ok();
    let os = os_release()
        .and_then(|text| os_release_value(&text, "PRETTY_NAME").or_else(|| os_release_value(&text, "NAME")))
        .unwrap_or_else(|| "Linux".into());
    let chip = read("/proc/cpuinfo")
        .and_then(|text| cpu_name(&text))
        // Arm boards name the board, not the processor, in the device tree.
        .or_else(|| read("/sys/firmware/devicetree/base/model").map(|m| tidy(m.trim_end_matches('\0'))).filter(|m| !m.is_empty()))
        .unwrap_or_default();
    let memory_gb = installed_gb().or_else(|| read("/proc/meminfo").and_then(|text| memory_gb(&text))).unwrap_or(0);
    Machine { os, chip, memory_gb }
}

/// The memory installed, in whole gigabytes, from the kernel's online memory
/// blocks: MemTotal leaves out what the firmware and the kernel keep.
fn installed_gb() -> Option<u64> {
    let dir = Path::new("/sys/devices/system/memory");
    let block = u64::from_str_radix(std::fs::read_to_string(dir.join("block_size_bytes")).ok()?.trim(), 16).ok()?;
    let online = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("memory"))
        .filter(|e| std::fs::read_to_string(e.path().join("online")).is_ok_and(|s| s.trim() == "1"))
        .count() as u64;
    let gb = (block * online) as f64 / (1u64 << 30) as f64;
    (gb >= 1.0).then(|| gb.round() as u64)
}

/// A value in os-release, which is shell-like: `NAME="Ubuntu"`, quoted or
/// not, with `\"`, `\\`, `\$` and `` \` `` escaped inside double quotes.
fn os_release_value(text: &str, key: &str) -> Option<String> {
    let line = text.lines().find_map(|l| l.trim().strip_prefix(key)?.strip_prefix('='))?.trim();
    let value = if let Some(inner) = line.strip_prefix('"').and_then(|l| l.strip_suffix('"')) {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match (c, chars.clone().next()) {
                ('\\', Some(next @ ('"' | '\\' | '$' | '`'))) => {
                    out.push(next);
                    chars.next();
                }
                _ => out.push(c),
            }
        }
        out
    } else if let Some(inner) = line.strip_prefix('\'').and_then(|l| l.strip_suffix('\'')) {
        inner.to_string()
    } else {
        line.to_string()
    };
    (!value.is_empty()).then_some(value)
}

/// The processor's name from /proc/cpuinfo: x86's "model name", or what
/// some Arm kernels give as "Model" or "Hardware".
fn cpu_name(text: &str) -> Option<String> {
    let field = |name: &str| {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == name).then(|| tidy(v)).filter(|v| !v.is_empty())
        })
    };
    field("model name").or_else(|| field("Model")).or_else(|| field("Hardware"))
}

/// Single spaces only: Intel's names pad with runs of them.
fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// /proc/meminfo's MemTotal in whole gigabytes (GiB, as macOS counts them),
/// rounded up, since it's short of what's installed.
fn memory_gb(text: &str) -> Option<u64> {
    let kb: u64 = text.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().strip_suffix("kB")?.trim().parse().ok()?;
    Some(kb.div_ceil(1 << 20))
}

/// The distribution's logo, premultiplied: os-release's LOGO, an icon name,
/// else the icon themes' "distributor-logo", found as a PNG (Flow draws no
/// SVG), the size Flow's own logo is first.
pub fn logo() -> Option<(Vec<u8>, u32, u32)> {
    let mut names = os_release().and_then(|text| os_release_value(&text, "LOGO")).into_iter().collect::<Vec<_>>();
    names.push("distributor-logo".into());
    let dirs = data_dirs();
    names.iter().flat_map(|name| logo_paths(&dirs, name)).find_map(|path| read_png(&path))
}

/// Where icons live: XDG_DATA_DIRS, or /usr/local/share and /usr/share.
fn data_dirs() -> Vec<PathBuf> {
    let list = var("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    list.split(':').filter(|d| !d.is_empty()).map(PathBuf::from).collect()
}

/// The PNGs an icon named `name` may be, in the order to try them: each
/// theme's (hicolor first) at the sizes nearest 256 pixels, then pixmaps.
fn logo_paths(dirs: &[PathBuf], name: &str) -> Vec<PathBuf> {
    const SIZES: [&str; 11] = [
        "256x256", "128x128@2x", "512x512", "256x256@2x", "192x192", "128x128", "96x96", "64x64@2x", "64x64", "48x48@2x", "48x48",
    ];
    let file = format!("{name}.png");
    let mut out = Vec::new();
    for dir in dirs {
        let icons = dir.join("icons");
        let mut themes: Vec<PathBuf> =
            std::fs::read_dir(&icons).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        themes.sort_by_key(|t| (t.file_name() != Some("hicolor".as_ref()), t.clone()));
        for size in SIZES {
            for theme in &themes {
                for kind in ["apps", "places"] {
                    out.push(theme.join(size).join(kind).join(&file));
                }
            }
        }
        out.push(dir.join("pixmaps").join(&file));
    }
    out.retain(|p| p.is_file());
    out
}

/// A PNG of at most 1024 pixels a side as premultiplied RGBA.
fn read_png(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let file = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    let premultiplied = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = p[3] as u16;
            [(p[0] as u16 * a / 255) as u8, (p[1] as u16 * a / 255) as u8, (p[2] as u16 * a / 255) as u8, p[3]]
        })
        .collect();
    Some((premultiplied, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_come_from_language_then_the_locale() {
        assert_eq!(languages_from(Some("ja:en_GB:en"), Some("ja_JP.UTF-8")), ["ja", "en-GB", "en", "ja-JP"]);
        assert_eq!(languages_from(None, Some("de_DE@euro")), ["de-DE"]);
        assert_eq!(languages_from(Some("fr:"), Some("pt_BR.utf8")), ["fr", "pt-BR"]);
        // The C locale is English, whatever LANGUAGE says, as for gettext.
        assert_eq!(languages_from(Some("ja"), Some("C.UTF-8")), ["en"]);
        assert_eq!(languages_from(None, Some("POSIX")), ["en"]);
        // With no locale at all, nothing to go by.
        assert!(languages_from(Some("ja"), None).is_empty());
    }

    #[test]
    fn os_release_values_lose_their_quotes_and_escapes() {
        let text = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 26.04.1 LTS\"\nID=ubuntu\nLOGO='ubuntu-logo'\nX=\"a \\\"b\\\" \\$c\"\nEMPTY=\"\"\n";
        assert_eq!(os_release_value(text, "PRETTY_NAME").as_deref(), Some("Ubuntu 26.04.1 LTS"));
        assert_eq!(os_release_value(text, "NAME").as_deref(), Some("Ubuntu"));
        assert_eq!(os_release_value(text, "ID").as_deref(), Some("ubuntu"));
        assert_eq!(os_release_value(text, "LOGO").as_deref(), Some("ubuntu-logo"));
        assert_eq!(os_release_value(text, "X").as_deref(), Some("a \"b\" $c"));
        assert_eq!(os_release_value(text, "EMPTY"), None);
        assert_eq!(os_release_value(text, "VERSION"), None);
    }

    #[test]
    fn the_processor_and_memory_come_from_proc() {
        let x86 = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz\n";
        assert_eq!(cpu_name(x86).as_deref(), Some("Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz"));
        let pi = "processor\t: 0\nBogoMIPS\t: 108.00\n\nRevision\t: c03114\nModel\t\t: Raspberry Pi 4 Model B Rev 1.4\n";
        assert_eq!(cpu_name(pi).as_deref(), Some("Raspberry Pi 4 Model B Rev 1.4"));
        assert_eq!(cpu_name("processor\t: 0\nmodel name\t:\n"), None);
        assert_eq!(memory_gb("MemTotal:       32803932 kB\nMemFree:        1 kB\n"), Some(32));
        assert_eq!(memory_gb("MemTotal:        8058400 kB\n"), Some(8));
        assert_eq!(memory_gb("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn this_computer_says_what_it_is() {
        let m = machine();
        eprintln!("{m:?}");
        assert!(!m.os.is_empty());
        if Path::new("/proc/meminfo").exists() {
            assert!(m.memory_gb > 0, "{m:?}");
        }
    }

    #[test]
    fn this_computer_probes_its_memory_and_buses() {
        let started = std::time::Instant::now();
        let h = flow_core::hardware::linux::read(Path::new("/"));
        let took = started.elapsed();
        // Only clean data: sizes, kinds, bus addresses and drivers' names.
        eprintln!("{h:?} in {took:?}");
        assert!(took < std::time::Duration::from_millis(250), "{took:?}");
        // Older systemd and boards without SMBIOS list no banks. Those listed
        // are what's installed, which may be more than Linux was given (an
        // APU's graphics take theirs first), but never less.
        if !h.banks.is_empty() {
            assert!(h.banks_agree(machine().memory_gb), "{h:?} {:?}", machine());
        }
        // The banks are read once and kept, the buses' slots afresh for each
        // new tab: with nothing plugged in meanwhile, two reads agree.
        assert_eq!(hardware(), hardware());
        assert_eq!(hardware().banks, h.banks);
    }

    #[test]
    fn a_logo_is_found_and_read_where_the_system_has_one() {
        // Not every system has one as a PNG; where there is one, it decodes.
        if let Some((pixels, w, h)) = logo() {
            eprintln!("logo: {w}×{h}");
            assert_eq!(pixels.len(), (w * h * 4) as usize);
            assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
        }
    }
}
