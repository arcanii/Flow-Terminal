//! `flowapp` run for real, in a scratch home of its own with the catalog a
//! file (`FLOWAPP_CATALOG`): what the order of its own steps decides, which
//! `flow_core::apps`' tests, calling those steps one by one, can't show.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use flow_core::apps::{self, itself, Catalog, Installed};
use flow_core::themes::{ThemeEntry, ThemeFile, FILE};

/// A home of its own and a copy of `flowapp` in a folder of its own: run
/// from the build's folder, flowapp would take the `flow` built beside it
/// for the Flow it runs in and ask it its version.
struct Home {
    dir: PathBuf,
    flowapp: PathBuf,
}

impl Home {
    fn new(name: &str) -> Home {
        let dir = std::env::temp_dir().join(format!("flowapp-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Laid out as Linux's install is, so Flow's own program beside it
        // (`lib/flow-terminal/games`' `../../../bin/flow`) is one that isn't there.
        let programs = dir.join("programs/lib/flow-terminal/games");
        std::fs::create_dir_all(&programs).unwrap();
        let flowapp = programs.join("flowapp");
        std::fs::copy(env!("CARGO_BIN_EXE_flowapp"), &flowapp).unwrap();
        std::fs::create_dir_all(dir.join("home")).unwrap();
        Home { dir, flowapp }
    }

    /// The apps' folder flowapp keeps under this home.
    fn apps(&self) -> PathBuf {
        let config = if cfg!(target_os = "macos") { self.dir.join("home/Library/Application Support") } else { self.dir.join("home/.config") };
        config.join(flow_core::APP_ID).join("apps")
    }

    /// `flowapp` with `args`, the catalog at `catalog`, in English, outside
    /// any tab and never handing over to another copy.
    fn run(&self, catalog: &Path, args: &[&str]) -> Output {
        Command::new(&self.flowapp)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.dir.join("home"))
            .env("FLOW_LANGUAGE", "en")
            .env("FLOWAPP_CATALOG", catalog)
            .env(itself::HANDED, "1")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    /// A package of themes of one's own called `package`, as its file.
    fn own_package(&self, package: &str) -> PathBuf {
        let from = self.dir.join(format!("{package}-files"));
        std::fs::create_dir_all(&from).unwrap();
        let theme = ThemeEntry { id: format!("{package}-look"), name: format!("The {package} look"), ..Default::default() };
        let themes = ThemeFile { format: flow_core::themes::FORMAT, package: package.into(), version: "1.0".into(), themes: vec![theme] };
        std::fs::write(from.join(FILE), serde_json::to_string(&themes).unwrap()).unwrap();
        let tarball = self.dir.join(format!("{package}-1.0.tar.gz"));
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&from).arg(".").status().unwrap().success());
        tarball
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn said(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// A catalog naming `names`, each an app with no download.
fn catalog_of(names: &[&str]) -> String {
    let mut catalog = Catalog { format: apps::FORMAT, ..Catalog::default() };
    for name in names {
        catalog.apps.insert(name.to_string(), apps::App { summary: format!("{name}, of Flow's"), ..apps::App::default() });
    }
    serde_json::to_string(&catalog).unwrap()
}

#[test]
fn a_package_of_your_own_cant_take_a_name_the_catalog_added_since_flowapp_last_read_it() {
    let home = Home::new("names");
    let place = home.dir.join("apps.json");
    // `update` reads the catalog before it has Flow Utils, and saves it.
    std::fs::write(&place, catalog_of(&["flow-sdk"])).unwrap();
    let updated = home.run(&place, &["update"]);
    assert!(updated.status.success(), "{}", said(&updated));
    assert!(home.apps().join("catalog.json").is_file());
    // The catalog has it now: `install` reads it before a file of one's
    // own too, so the name is known to be Flow's.
    std::fs::write(&place, catalog_of(&["flow-sdk", "flow-utils"])).unwrap();
    let file = home.own_package("flow-utils");
    let refused = home.run(&place, &["install", file.to_str().unwrap()]);
    assert!(!refused.status.success() && said(&refused).contains("flow-utils is the name of one of Flow's own"), "{}", said(&refused));
    assert!(Installed::load(&home.apps()).apps.is_empty() && !home.apps().join("flow-utils").exists());
    // Offline, the copy that read saved still knows it, and says it's that.
    let nowhere = home.dir.join("nowhere.json");
    let offline = home.run(&nowhere, &["install", file.to_str().unwrap()]);
    assert!(!offline.status.success() && said(&offline).contains("The catalog didn't read") && said(&offline).contains("one of Flow's own"), "{}", said(&offline));
    // A name that isn't Flow's installs, offline as well.
    let mine = home.own_package("my-looks");
    let installed = home.run(&nowhere, &["install", mine.to_str().unwrap()]);
    assert!(installed.status.success() && said(&installed).contains("Installed my-looks 1.0"), "{}", said(&installed));
    assert_eq!(Installed::load(&home.apps()).apps.keys().collect::<Vec<_>>(), ["my-looks"]);
}
