//! On Windows, embeds the icon and version info into flow.exe. This has to be
//! the binary's own build script: embed-resource links the resource only into
//! the binaries of the package that compiles it. The version numbers the app
//! shows come from core/build.rs, through the same build-number helper.

use std::path::{Path, PathBuf};

#[path = "core/build_number.rs"]
mod build_number;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/flow.ico");
    println!("cargo:rerun-if-changed=core/build_number.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
        let build = build_number::build_number(&manifest_dir);
        let mut shown = build_number::shown_version(&manifest_dir, &version);
        if build_number::dev(&manifest_dir) {
            shown.push_str(".DEV");
        }
        embed_windows_resources(&manifest_dir, &version, &shown, build);
    }
}

/// `version` gives the numbers the installer and the updater compare
/// (0.1.25.<build>); `shown`, with a patch release's number, or DEV while
/// it's still worked toward, the product's.
fn embed_windows_resources(manifest_dir: &Path, version: &str, shown: &str, build: u32) {
    let icon = manifest_dir.join("assets").join("icon").join("flow.ico");
    let icon = icon.to_string_lossy().replace('\\', "\\\\");
    let mut parts = version.split(['.', '-']).map(|p| p.parse::<u16>().unwrap_or(0));
    let (major, minor, patch) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    // "Flow" matches APP_NAME in core/src/lib.rs.
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},{build}
PRODUCTVERSION {major},{minor},{patch},{build}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Flow"
      VALUE "FileVersion", "{version}.{build}"
      VALUE "InternalName", "flow"
      VALUE "OriginalFilename", "flow.exe"
      VALUE "ProductName", "Flow"
      VALUE "ProductVersion", "{shown} (build {build})"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("flow.rc");
    std::fs::write(&out, rc).expect("write flow.rc");
    // Without the Windows SDK's rc.exe this is skipped and the exe just has no icon.
    embed_resource::compile(&out, embed_resource::NONE).manifest_optional().expect("compile flow.rc");
}
