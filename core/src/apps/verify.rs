//! `flowapp verify`: an installed app looked at against the download it
//! came from. That's fetched again and checked with Flow's key as an
//! install checks it, unpacked aside in the system's temp folder (never in
//! the apps' folder), and compared file by file with what's in
//! `<name>/<version>/` and the copies of its programs in `apps/bin/`.
//!
//! A file changed or gone fails. One added since the install, as Python
//! adds its `__pycache__` beside a package it runs, is said but doesn't
//! fail. The download is the one `installed.json` kept, or for an app
//! installed by a flowapp that kept none, the catalog's of the same
//! version, saying so (`docs/design/flowapp-signatures.md`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{check, exe, fetch_with, take_away, untar, Catalog, Download, Failed, Kept, Step};

/// What's installed against its download.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// How many files of the download are as it has them.
    pub same: usize,
    /// Files whose bytes (or a link's target) differ, by their place in
    /// the app's folder, `/` between, or `bin/<program>` for a copy in
    /// `apps/bin/`.
    pub changed: Vec<String>,
    /// Files of the download that aren't there.
    pub missing: Vec<String>,
    /// Files there that the download hasn't.
    pub added: Vec<String>,
}

impl Diff {
    /// Whether everything the download holds is there as it holds it.
    pub fn ok(&self) -> bool {
        self.changed.is_empty() && self.missing.is_empty()
    }

    /// How many files the download holds, its programs' copies counted.
    pub fn files(&self) -> usize {
        self.same + self.changed.len() + self.missing.len()
    }
}

/// A file as the comparison sees it.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    /// Its SHA-256.
    File(String),
    /// Where the link points.
    Link(PathBuf),
}

/// Every file under `dir`, by its place under it, links not followed.
fn files_under(dir: &Path) -> BTreeMap<String, Seen> {
    fn walk(dir: &Path, at: &str, out: &mut BTreeMap<String, Seen>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let place = if at.is_empty() { name } else { format!("{at}/{name}") };
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if meta.file_type().is_symlink() {
                if let Ok(to) = std::fs::read_link(&path) {
                    out.insert(place, Seen::Link(to));
                }
            } else if meta.is_dir() {
                walk(&path, &place, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.insert(place, Seen::File(super::sha256(&bytes)));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, "", &mut out);
    out
}

/// What's in `unpacked`, a download as it comes, against `installed`,
/// where it was put, and each of `programs` in it against its copy in
/// `bin`.
pub fn compare(unpacked: &Path, installed: &Path, bin: &Path, programs: &[String]) -> Diff {
    let want = files_under(unpacked);
    let mut have = files_under(installed);
    let mut diff = Diff::default();
    for (place, seen) in &want {
        match have.remove(place) {
            Some(there) if there == *seen => diff.same += 1,
            Some(_) => diff.changed.push(place.clone()),
            None => diff.missing.push(place.clone()),
        }
    }
    diff.added = have.into_keys().collect();
    for program in programs {
        let file = exe(program);
        let place = format!("bin/{file}");
        let copy = bin.join(&file);
        match (std::fs::read(unpacked.join("bin").join(&file)), std::fs::read(&copy)) {
            (Ok(want), Ok(have)) if want == have => diff.same += 1,
            (Ok(_), Ok(_)) => diff.changed.push(format!("apps/{place}")),
            (Ok(_), Err(_)) => diff.missing.push(format!("apps/{place}")),
            // The download hasn't it: the install would have refused it.
            (Err(_), _) => {}
        }
    }
    diff
}

/// Which download an install is looked at against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Against {
    /// The one `installed.json` kept as it installed it.
    Kept,
    /// The catalog's of the same version, as nothing was kept.
    Catalog,
}

/// What `verify` found of one app.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Its download, fetched and checked, against what's there.
    Checked { download: Box<Download>, against: Against, diff: Diff },
    /// Installed from a file of the user's own, which no one signed.
    OwnFile,
    /// Nothing says which download it came from: `installed.json` kept
    /// none, and the catalog has no download of its version for here.
    NoDownload,
}

/// The download `kept` came from: the one it kept, else the catalog's of
/// its version for this system.
pub fn download_of(name: &str, kept: &Kept, catalog: Option<&Catalog>) -> Option<(Download, Against)> {
    if let Some(download) = &kept.download {
        return Some((download.clone(), Against::Kept));
    }
    let download = catalog.and_then(|c| c.get(name)).and_then(super::App::here).filter(|d| d.version == kept.version)?;
    Some((download.clone(), Against::Catalog))
}

/// Looks at `name`, installed under `root` as `kept`, against its download,
/// checked with `key` (base64), telling `step` how the download comes.
pub fn verify(root: &Path, name: &str, kept: &Kept, catalog: Option<&Catalog>, key: &str, step: &mut dyn FnMut(Step)) -> Result<Verdict, Failed> {
    if kept.own_file {
        return Ok(Verdict::OwnFile);
    }
    let Some((download, against)) = download_of(name, kept, catalog) else { return Ok(Verdict::NoDownload) };
    let bytes = fetch_with(&download.url, download.size, &mut |got| step(Step::Fetched(got)))?;
    check(&bytes, &download, key).map_err(|e| format!("{name}: {e}"))?;
    step(Step::Checked);
    // A folder of this check's own, as two may run at once.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let aside = std::env::temp_dir().join(format!("flowapp-verify-{}-{n}-{name}", std::process::id()));
    take_away(&aside).map_err(|e| format!("{name}: {e}"))?;
    std::fs::create_dir_all(&aside).map_err(|e| format!("{name}: {e}"))?;
    let unpacked = untar(&bytes, &aside).map(|()| compare(&aside, &root.join(name).join(&kept.version), &root.join("bin"), &kept.programs));
    let _ = take_away(&aside);
    let diff = unpacked.map_err(|e| format!("{name}: {e}"))?;
    Ok(Verdict::Checked { download: Box::new(download), against, diff })
}

#[cfg(test)]
mod tests {
    use super::super::tests::{key, package, tempdir};
    use super::super::{install_bytes, install_with, App, Installed};
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_folder_like_its_download_is_the_same_and_each_difference_is_said() {
        let (want, have, bin) = (tempdir(), tempdir(), tempdir());
        // The program as each system names it: tool.exe on Windows.
        let tool = exe("tool");
        let in_bin = format!("apps/bin/{tool}");
        for dir in [&want, &have] {
            write(&dir.join("bin").join(&tool), "#!/bin/sh\necho tool\n");
            write(&dir.join("lib/libtool.dylib"), "library");
            write(&dir.join("share/man/man1/tool.1"), ".TH TOOL 1\n");
        }
        write(&bin.join(&tool), "#!/bin/sh\necho tool\n");
        let programs = vec!["tool".to_string()];
        let same = compare(&want, &have, &bin, &programs);
        assert_eq!(same, Diff { same: 4, ..Diff::default() });
        assert!(same.ok());
        assert_eq!(same.files(), 4);

        write(&have.join("lib/libtool.dylib"), "changed");
        std::fs::remove_file(have.join("share/man/man1/tool.1")).unwrap();
        write(&have.join("lib/__pycache__/x.pyc"), "cache");
        write(&bin.join(&tool), "#!/bin/sh\necho another\n");
        let diff = compare(&want, &have, &bin, &programs);
        assert_eq!(diff.changed, ["lib/libtool.dylib", in_bin.as_str()]);
        assert_eq!(diff.missing, ["share/man/man1/tool.1"]);
        assert_eq!(diff.added, ["lib/__pycache__/x.pyc"]);
        assert_eq!((diff.same, diff.files()), (1, 4));
        assert!(!diff.ok());

        std::fs::remove_file(bin.join(&tool)).unwrap();
        assert_eq!(compare(&want, &have, &bin, &programs).missing, ["share/man/man1/tool.1", in_bin.as_str()]);
    }

    #[test]
    fn files_added_since_the_install_are_said_but_dont_fail() {
        let (want, have, bin) = (tempdir(), tempdir(), tempdir());
        write(&want.join("python/flow_sdk/__init__.py"), "");
        write(&have.join("python/flow_sdk/__init__.py"), "");
        write(&have.join("python/flow_sdk/__pycache__/__init__.cpython-313.pyc"), "cache");
        let diff = compare(&want, &have, &bin, &[]);
        assert!(diff.ok());
        assert_eq!(diff.added.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_the_same_while_it_points_where_the_downloads_does() {
        let (want, have, bin) = (tempdir(), tempdir(), tempdir());
        for dir in [&want, &have] {
            write(&dir.join("lib/libtool.1.0.dylib"), "library");
            std::os::unix::fs::symlink("libtool.1.0.dylib", dir.join("lib/libtool.dylib")).unwrap();
        }
        assert!(compare(&want, &have, &bin, &[]).ok());
        std::fs::remove_file(have.join("lib/libtool.dylib")).unwrap();
        std::os::unix::fs::symlink("/elsewhere/libtool.dylib", have.join("lib/libtool.dylib")).unwrap();
        assert_eq!(compare(&want, &have, &bin, &[]).changed, ["lib/libtool.dylib"]);
    }

    #[test]
    fn an_install_is_verified_against_the_download_it_kept_and_a_change_is_found() {
        let root = tempdir();
        let (signing, public) = key();
        let (_, download) = package(&["flow-tool"], "0.2.0", &signing);
        let mut app = App { version: "0.2.0".into(), programs: vec!["flow-tool".into()], ..App::default() };
        app.downloads.insert(super::super::platform(), download.clone());
        let kept = install_with(&root, "flow-tool", &app, &public, &mut |_| {}).unwrap();
        assert_eq!(kept.download.as_ref(), Some(&download), "the install keeps its download");
        assert!(!kept.checked.is_empty() && !kept.own_file);
        assert_eq!(Installed::load(&root).apps["flow-tool"], kept);

        let mut steps = Vec::new();
        let Verdict::Checked { against, diff, .. } = verify(&root, "flow-tool", &kept, None, &public, &mut |s| steps.push(s)).unwrap() else { panic!("not checked") };
        assert_eq!(against, Against::Kept);
        assert!(diff.ok() && diff.added.is_empty(), "{diff:?}");
        assert_eq!(steps.last(), Some(&Step::Checked));

        write(&root.join("flow-tool/0.2.0/bin").join(exe("flow-tool")), "#!/bin/sh\necho changed\n");
        let Verdict::Checked { diff, .. } = verify(&root, "flow-tool", &kept, None, &public, &mut |_| {}).unwrap() else { panic!("not checked") };
        assert_eq!(diff.changed, [format!("bin/{}", exe("flow-tool"))]);

        // Checked with another key, it isn't Flow's download.
        let (_, other) = key_of(9);
        assert!(verify(&root, "flow-tool", &kept, None, &other, &mut |_| {}).is_err());
    }

    fn key_of(seed: u8) -> (ed25519_dalek::SigningKey, String) {
        use base64::Engine;
        let signing = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        let public = base64::engine::general_purpose::STANDARD.encode(signing.verifying_key().to_bytes());
        (signing, public)
    }

    #[test]
    fn an_install_that_kept_no_download_goes_by_the_catalogs_of_its_version() {
        let root = tempdir();
        let (signing, public) = key();
        let (bytes, download) = package(&["flow-tool"], "0.2.0", &signing);
        let kept = install_bytes(&root, "flow-tool", "0.2.0", &["flow-tool".into()], &bytes).unwrap();
        assert!(kept.download.is_none() && !kept.own_file);
        assert_eq!(verify(&root, "flow-tool", &kept, None, &public, &mut |_| {}).unwrap(), Verdict::NoDownload);

        let mut catalog = Catalog::default();
        let mut app = App { version: "0.2.0".into(), ..App::default() };
        app.downloads.insert(super::super::platform(), download);
        catalog.apps.insert("flow-tool".into(), app.clone());
        let Verdict::Checked { against, diff, .. } = verify(&root, "flow-tool", &kept, Some(&catalog), &public, &mut |_| {}).unwrap() else { panic!("not checked") };
        assert_eq!(against, Against::Catalog);
        assert!(diff.ok());

        // A catalog that has moved on to another version names nothing.
        app.downloads.values_mut().for_each(|d| d.version = "0.3.0".into());
        catalog.apps.insert("flow-tool".into(), app);
        assert_eq!(verify(&root, "flow-tool", &kept, Some(&catalog), &public, &mut |_| {}).unwrap(), Verdict::NoDownload);
    }

    #[test]
    fn a_file_of_the_users_own_has_nothing_to_be_checked_against() {
        let kept = Kept { version: "1.0".into(), own_file: true, ..Kept::default() };
        assert_eq!(verify(&tempdir(), "mine", &kept, None, crate::UPDATE_PUBLIC_KEY, &mut |_| {}).unwrap(), Verdict::OwnFile);
    }
}
