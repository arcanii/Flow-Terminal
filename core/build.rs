//! Stamps the version and build number into the core for `version.rs`.
//! The marketing version is `version` under `[workspace.package]` in the
//! root Cargo.toml, with a patch release's number after it when there is one
//! (`[workspace.metadata.flow]`), and whether it's still being worked
//! toward (`dev`, shown as 0.1.40.DEV); the build number is the git commit
//! count. And whether the build updates itself: `FLOW_UPDATES=1` while core
//! builds, which Flow's release scripts set only with Flow's update key at
//! hand (docs/design/updates-for-releases.md).

mod build_number;

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_number.rs");
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let build = build_number::build_number(&dir);
    let root = dir.parent().expect("core/ sits in the workspace");
    println!("cargo:rustc-env=FLOW_VERSION={}", build_number::shown_version(root, &version));
    println!("cargo:rustc-env=FLOW_DEV={}", build_number::dev(root));
    println!("cargo:rustc-env=FLOW_BUILD={build}");
    println!("cargo:rustc-env=FLOW_VERSION_FULL={version}.{build}");
    println!("cargo:rustc-env=FLOWAPP_VERSION={}", build_number::flowapp_version(root).expect("[workspace.metadata.flow] names flowapp's version"));
    println!("cargo:rerun-if-env-changed=FLOW_UPDATES");
    println!("cargo:rustc-env=FLOW_UPDATES={}", std::env::var("FLOW_UPDATES").as_deref() == Ok("1"));
}
