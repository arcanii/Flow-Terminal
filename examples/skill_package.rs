//! Writes a package of skills for `flowapp` (`flow-skills`, Flow's own, by
//! default): the skills in a folder (`assets/skills/`, each `<name>.md` or
//! `<name>/SKILL.md` with its files) copied under `skills/`, beside the
//! `skills.json` naming the package, read back as Flow reads a package
//! (`flow_core::ai::skills`), and that folder packed as a `.tar.gz` owned
//! by no one, as the apps' downloads are, ready to be signed
//! (`windows/scripts/sign-release.sh`) and written into the catalog
//! (`examples/app_catalog.rs --skills --platform any`). A skill that
//! wouldn't read (no front matter with a name and a description, a name
//! that isn't lowercase letters, digits and hyphens) stops it.
//!
//!   cargo run --example skill_package -- --version 0.1.0 [--name flow-skills] \
//!       [--from assets/skills] [--out target/skills]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};
use flow_core::ai::skills;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut version, mut name, mut from, mut out) = (None, "flow-skills".to_string(), PathBuf::from("assets/skills"), PathBuf::from("target/skills"));
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--version" => version = Some(value()?),
            "--name" => name = value()?,
            "--from" => from = PathBuf::from(value()?),
            "--out" => out = PathBuf::from(value()?),
            other => bail!("unknown argument {other} (see the top of examples/skill_package.rs)"),
        }
    }
    let Some(version) = version else { bail!("--version is needed") };
    let stage_name = format!("{name}-{version}");
    let stage = out.join(&stage_name);
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).with_context(|| format!("making {}", stage.display()))?;
    let manifest = skills::Manifest { format: skills::FORMAT, package: name.clone(), version: version.clone() };
    std::fs::write(stage.join(skills::FILE), serde_json::to_string_pretty(&manifest)? + "\n")?;
    copy(&from, &stage.join(skills::PACKAGE_DIR)).with_context(|| format!("copying {}", from.display()))?;
    // Read back as Flow will: every skill there, so none is left out unseen.
    skills::manifest(&stage).map_err(|e| anyhow::anyhow!("{}: {e}", stage.display()))?;
    let read = skills::list(&stage.join(skills::PACKAGE_DIR));
    let there = std::fs::read_dir(&from)?.flatten().filter(|e| e.path().is_dir() || e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("md"))).count();
    if read.len() != there {
        bail!("{} of the {there} skills in {} read: {}", read.len(), from.display(), read.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    let tarball = std::fs::canonicalize(&out)?.join(format!("{stage_name}.tar.gz"));
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
    let names: Vec<&str> = read.iter().map(|s| s.name.as_str()).collect();
    println!("{}: {} skills ({}), {} bytes, SHA-256 {}", tarball.display(), read.len(), names.join(", "), bytes.len(), flow_core::apps::sha256(&bytes));
    Ok(())
}

/// Copies the folder `from` to `to`, files and folders alike.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()))?;
        } else {
            std::fs::copy(&path, to.join(entry.file_name()))?;
        }
    }
    Ok(())
}
