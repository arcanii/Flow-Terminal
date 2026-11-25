//! Writes the package of Flow's own machines for `flowapp`
//! (`flow-core-themes`, `flow_core::themes::core_themes`): its `themes.json`
//! in a folder, read back as Flow reads a package, and that folder packed as
//! a `.tar.gz` owned by no one, as the apps' downloads are, ready to be
//! signed (`windows/scripts/sign-release.sh`) and written into the catalog
//! (`examples/app_catalog.rs --themes --platform any`).
//!
//!   cargo run --example theme_package -- --version 0.1.0 [--out target/themes]

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context};
use flow_core::themes;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut version, mut out) = (None, PathBuf::from("target/themes"));
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--version" => version = Some(value()?),
            "--out" => out = PathBuf::from(value()?),
            other => bail!("unknown argument {other} (see the top of examples/theme_package.rs)"),
        }
    }
    let Some(version) = version else { bail!("--version is needed") };
    let file = themes::core_themes(&version);
    let name = format!("{}-{version}", themes::CORE);
    let stage = out.join(&name);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).with_context(|| format!("making {}", stage.display()))?;
    std::fs::write(stage.join(themes::FILE), serde_json::to_string_pretty(&file)? + "\n")?;
    // Read back as Flow will, so a package that wouldn't wear isn't made.
    let read = themes::read(&stage).map_err(|e| anyhow::anyhow!("{}: {e}", stage.display()))?;
    let tarball = std::fs::canonicalize(&out)?.join(format!("{name}.tar.gz"));
    let mut tar = if cfg!(target_os = "macos") {
        // The system's tar (bsdtar), without the Mac's own metadata.
        let mut tar = Command::new("/usr/bin/tar");
        tar.env("COPYFILE_DISABLE", "1").args(["--no-mac-metadata", "--uid", "0", "--gid", "0", "--uname", "", "--gname", ""]);
        tar
    } else {
        let mut tar = Command::new("tar");
        tar.args(["--owner=0", "--group=0", "--numeric-owner"]);
        tar
    };
    let status = tar.arg("-czf").arg(&tarball).arg("-C").arg(&stage).arg(".").status().context("running tar")?;
    if !status.success() {
        bail!("tar failed");
    }
    let bytes = std::fs::read(&tarball)?;
    println!("{}: {} themes, {} bytes, SHA-256 {}", tarball.display(), read.len(), bytes.len(), flow_core::apps::sha256(&bytes));
    Ok(())
}
