//! Version shown to people and to the updater (stamped by core/build.rs).

/// Marketing version from `[workspace.package]` in the root Cargo.toml, with
/// a patch release's number after it (`[workspace.metadata.flow]`), e.g.
/// "0.1.0" or "0.1.0.1": numbers alone, as programs (XTVERSION,
/// `TERM_PROGRAM_VERSION`) and flowapp's catalog compare it, DEV or not.
pub const VERSION: &str = env!("FLOW_VERSION");
/// The work toward `VERSION` goes on (`dev = true` under
/// `[workspace.metadata.flow]`): not released, shown as "0.1.40.DEV".
pub const DEV: bool = matches!(env!("FLOW_DEV").as_bytes(), b"true");
/// The build number: the git commit count at build time plus `build_offset`
/// under `[workspace.metadata.flow]`. On macOS it is also the app bundle's
/// CFBundleVersion, which Sparkle compares against the appcast.
pub const BUILD: &str = env!("FLOW_BUILD");
/// flowapp's own version (`flowapp` under `[workspace.metadata.flow]`),
/// apart from Flow's: flowapp is an app in its own catalog too, whose copy
/// there comes out between Flow's releases (docs/design/flowapp-updates.md).
pub const FLOWAPP: &str = env!("FLOWAPP_VERSION");
/// "0.1.0.42": what WinSparkle compares against the appcast's
/// sparkle:version. The marketing version without the patch, so the build
/// number stays the fourth number and keeps rising.
pub const FULL: &str = env!("FLOW_VERSION_FULL");

/// The version as people see it: `VERSION`, with ".DEV" after it while
/// it's still worked toward.
pub fn shown() -> String {
    if DEV { format!("{VERSION}.DEV") } else { VERSION.to_string() }
}

/// Made as one of Flow's own releases, whose updater reads Flow's feeds:
/// `FLOW_UPDATES=1` as core built, which Flow's release scripts set only
/// with Flow's update key at hand (docs/design/updates-for-releases.md). A
/// plain `cargo build --release`, a fork's among them, has no updater, so
/// it never offers Flow's own release in its place.
pub const UPDATES: bool = matches!(env!("FLOW_UPDATES").as_bytes(), b"true");

/// Whether Flow starts its updater: a release build made for updates, or
/// any build `FLOW_UPDATER=1` asks it of, for testing; never otherwise a
/// debug build, which would offer to "update" to the last release.
pub fn updater_on(updates: bool, debug: bool, forced: bool) -> bool {
    forced || (updates && !debug)
}

/// `flow --version`'s answer: "Flow 0.1.0 (build 42)", and with
/// `say_updates` (`FLOW_SAY_UPDATES=1`) a line after it saying whether the
/// build updates itself, which the release scripts read to check that what
/// they package is as they decided. Asked without it, a Flow says only its
/// version, as the updaters of the Flows out already read it.
pub fn answer(say_updates: bool) -> String {
    let version = format!("{} {}", crate::APP_NAME, display());
    if !say_updates {
        return version;
    }
    format!("{version}\nupdates: {}", if UPDATES { "on" } else { "off" })
}

/// Whether `flow --version` is asked to say if the build updates itself.
pub fn say_updates() -> bool {
    std::env::var("FLOW_SAY_UPDATES").as_deref() == Ok("1")
}

/// "0.1.0 (build 42)", "0.1.0.1 (build 42)" for a patch release, or
/// "0.1.40.DEV (build 590)" while it's worked toward.
pub fn display() -> String {
    format!("{} (build {BUILD})", shown())
}

/// Flow-releases' list of releases, every OS's.
pub const RELEASES: &str = "https://github.com/arcanii/Flow-releases/releases";

/// This version's release page, for About's link. Each OS tags its own by
/// the version shown, a patch's fourth number and all: Windows `v0.1.27`
/// and `v0.1.25.1`, the Mac `v0.1.31.1-macos`, Linux `v0.1.31.1-linux`.
/// Not `released` (a build without the updater a release carries, as from
/// `cargo run`, whose version may not be out yet for its OS), or a DEV
/// build, it's the list, whose page is always there.
pub fn release_page(released: bool) -> String {
    if !released || DEV {
        return RELEASES.to_string();
    }
    let tag = if cfg!(target_os = "macos") {
        format!("v{VERSION}-macos")
    } else if cfg!(windows) {
        format!("v{VERSION}")
    } else {
        format!("v{VERSION}-linux")
    };
    format!("{RELEASES}/tag/{tag}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowapps_own_version_is_numbers_alone_and_above_every_flow_before_it() {
        assert!(FLOWAPP.split('.').all(|n| n.parse::<u32>().is_ok()), "{FLOWAPP}: numbers alone");
        // The flowapps already out say Flow's version (0.1.45 at most), and
        // the catalog's flowapp must read as newer than all of them.
        assert!(crate::apps::newer(FLOWAPP, "0.1.45"), "{FLOWAPP}");
    }

    #[test]
    fn only_a_release_made_for_updates_or_a_build_asked_to_starts_the_updater() {
        assert!(updater_on(true, false, false), "Flow's own release");
        assert!(!updater_on(false, false, false), "a plain cargo build --release, a fork's");
        assert!(!updater_on(true, true, false), "a debug build would offer the last release");
        assert!(updater_on(false, true, true) && updater_on(false, false, true), "FLOW_UPDATER=1, for testing");
    }

    #[test]
    fn the_version_says_whether_the_build_updates_only_when_asked() {
        assert_eq!(answer(false), format!("Flow {}", display()), "as the updaters out already read it");
        let said = answer(true);
        let (first, second) = said.split_once('\n').expect("two lines");
        assert_eq!(first, answer(false));
        assert_eq!(second, if UPDATES { "updates: on" } else { "updates: off" });
    }

    #[test]
    fn a_dev_build_says_so_after_its_version_and_only_there() {
        assert!(VERSION.split('.').all(|n| n.parse::<u32>().is_ok()), "{VERSION}: numbers alone");
        assert_eq!(shown().ends_with(".DEV"), DEV);
        assert!(shown().starts_with(VERSION));
        assert_eq!(display(), format!("{} (build {BUILD})", shown()));
    }

    #[test]
    fn a_release_links_to_its_own_page_and_a_build_from_the_tree_to_the_list() {
        assert_eq!(release_page(false), RELEASES);
        if DEV {
            // Not released yet: the list, wherever it's built.
            assert_eq!(release_page(true), RELEASES);
            return;
        }
        let page = release_page(true);
        let tag = page.strip_prefix(&format!("{RELEASES}/tag/v")).expect("a tag's page");
        assert!(tag.starts_with(env!("CARGO_PKG_VERSION")), "{tag}");
        assert!(tag.starts_with(VERSION), "{tag}: the version shown, a patch's number and all");
        assert_eq!(tag.ends_with("-macos"), cfg!(target_os = "macos"));
        assert_eq!(tag.ends_with("-linux"), cfg!(all(unix, not(target_os = "macos"))));
    }
}
