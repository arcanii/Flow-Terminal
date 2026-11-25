//! Writes an app's entry into `flowapp`'s catalog (`apps.json`,
//! `flow_core::apps`): its line, its programs and, for one system and chip,
//! its download, whose size and SHA-256 it works out from the file and whose
//! signature it checks against Flow's update key before writing anything.
//! The other systems' downloads stay as they were; the app's version is the
//! newest of its downloads'. With `--themes`, it's a package of themes
//! (`catalog.themes`), no programs, and `--platform any`, as
//! `examples/theme_package.rs` makes one; with `--skills`, a package of
//! skills for the assistant (`catalog.skills`), the same, as
//! `examples/skill_package.rs` makes one. An app with programs may carry
//! skills too, in a `skills/` folder in its download, and needs neither. `--flow 0.1.34` says the
//! download needs that Flow or a newer one, which an older Flow's flowapp
//! holds it back for (`--flow none` takes that off); without it, the
//! download keeps what this system's before it said, since an app seldom
//! needs less of Flow than it did.
//!
//! The download's `signed` is today, a UTC date, or `--signed 2026-11-18`:
//! the catalog's word for when it was signed, which `flowapp list` and
//! `search` show. Beside the file it writes `<file>.sig`, the signature's
//! 64 bytes, to go on the release beside it, so anyone can check it with
//! OpenSSL alone (`docs/design/flowapp-signatures.md`).
//!
//!   cargo run --example app_catalog -- --catalog apps.json --app flow-rogue \
//!       --version 0.1.45 --summary "…" --programs flow-rogue [--bundled flow-rogue] \
//!       --platform macos-aarch64 --url https://…/flow-rogue-0.1.45-macos-aarch64.tar.gz \
//!       --file target/apps/flow-rogue-0.1.45-macos-aarch64.tar.gz --signature <base64> [--flow 0.1.34] [--signed 2026-11-18]

use std::path::PathBuf;

use anyhow::{bail, Context};
use base64::Engine;
use flow_core::apps::{self, App, Catalog, Download};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut catalog, mut name, mut version, mut summary, mut programs, mut bundled) = (None, None, None, None, Vec::new(), None);
    let (mut platform, mut url, mut file, mut signature, mut themes, mut flow) = (None, None, None, None, false, None);
    let (mut skills, mut signed) = (false, None);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--catalog" => catalog = Some(PathBuf::from(value()?)),
            "--app" => name = Some(value()?),
            "--version" => version = Some(value()?),
            "--summary" => summary = Some(value()?),
            "--programs" => programs = value()?.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
            "--bundled" => bundled = Some(value()?),
            "--platform" => platform = Some(value()?),
            "--url" => url = Some(value()?),
            "--file" => file = Some(PathBuf::from(value()?)),
            "--signature" => signature = Some(value()?),
            "--themes" => themes = true,
            "--skills" => skills = true,
            "--flow" => flow = Some(value()?),
            "--signed" => signed = Some(value()?),
            other => bail!("unknown argument {other} (see the top of examples/app_catalog.rs)"),
        }
    }
    let (Some(catalog_path), Some(name), Some(version), Some(platform), Some(url), Some(file), Some(signature)) = (catalog, name, version, platform, url, file, signature) else {
        bail!("--catalog, --app, --version, --platform, --url, --file and --signature are all needed");
    };
    // Each becomes a folder or a file where flowapp installs it, which it
    // refuses outside these, so no download goes out that none installs.
    if !apps::fit_name(&name) {
        bail!("--app {name}: lowercase letters, digits, - and _, from a letter or digit, at most 64, not bin, first, previews nor a Windows device's name");
    }
    if !apps::fit_version(&version) {
        bail!("--version {version}: letters, digits, ., -, _ and +, at most 64, neither starting nor ending with a dot, nor a Windows device's name before its first dot");
    }
    if let Some(program) = programs.iter().chain(&bundled).find(|p| !apps::fit_program(p)) {
        bail!("{program}: a program is a plain file's name in bin/, not hidden, a path nor a drive");
    }
    let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
    let signed = match signed {
        Some(date) => apps::date(&date).with_context(|| format!("--signed {date}: a date, such as 2026-11-18"))?,
        None => apps::today(),
    };
    let mut download = Download { version: version.clone(), url, size: bytes.len() as u64, sha256: apps::sha256(&bytes), signature: signature.trim().to_string(), flow: String::new(), signed };
    if let Err(e) = apps::check(&bytes, &download, flow_core::UPDATE_PUBLIC_KEY) {
        bail!("{}: {e}. Sign this exact file with Flow's key.", file.display());
    }
    // The signature as a file of its own, for the release, which OpenSSL
    // checks the download with.
    let sig_path = PathBuf::from(format!("{}.sig", file.display()));
    let raw = base64::engine::general_purpose::STANDARD.decode(&download.signature).context("the signature's base64")?;
    std::fs::write(&sig_path, raw).with_context(|| format!("writing {}", sig_path.display()))?;
    let mut catalog = match std::fs::read_to_string(&catalog_path) {
        Ok(text) => Catalog::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", catalog_path.display()))?,
        Err(_) => Catalog { format: apps::FORMAT, ..Catalog::default() },
    };
    if themes && skills {
        bail!("a package is of themes or of skills, not both");
    }
    let app = if themes {
        catalog.themes.entry(name.clone())
    } else if skills {
        catalog.skills.entry(name.clone())
    } else {
        catalog.apps.entry(name.clone())
    }
    .or_insert_with(App::default);
    if let Some(summary) = summary {
        app.summary = summary;
    }
    if !programs.is_empty() {
        app.programs = programs;
    }
    if bundled.is_some() {
        app.bundled = bundled;
    }
    let package = themes || skills;
    if app.programs.is_empty() && !package {
        bail!("{name} names no programs (--programs)");
    }
    if package && (!app.programs.is_empty() || platform != apps::ANY) {
        bail!("a package of {} has no programs, and one download, --platform {}", if themes { "themes" } else { "skills" }, apps::ANY);
    }
    download.flow = match flow.as_deref().map(|f| f.trim().trim_start_matches('v')) {
        Some("" | "none") => String::new(),
        Some(f) if f.split('.').all(|part| part.parse::<u64>().is_ok()) => f.to_string(),
        Some(f) => bail!("--flow {f}: a Flow version, such as 0.1.34, or none"),
        None => app.downloads.get(&platform).map(|d| d.flow.clone()).unwrap_or_default(),
    };
    let needs = if download.flow.is_empty() { String::new() } else { format!(", needing Flow {} or later", download.flow) };
    let signed_on = download.signed.clone();
    app.downloads.insert(platform.clone(), download);
    app.version = app.downloads.values().map(|d| d.version.clone()).reduce(|a, b| if apps::newer(&b, &a) { b } else { a }).unwrap_or_default();
    let text = serde_json::to_string_pretty(&catalog)? + "\n";
    std::fs::write(&catalog_path, text).with_context(|| format!("writing {}", catalog_path.display()))?;
    println!("{}: {name} {version} for {platform} ({} bytes{needs}), signature verified, signed {signed_on}", catalog_path.display(), bytes.len());
    println!("{}: its signature, to go on the release beside it", sig_path.display());
    Ok(())
}
