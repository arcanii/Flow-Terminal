//! Lists the Rust crates compiled into flow, with their licenses, for a
//! release's THIRD-PARTY-NOTICES.txt: the same section that
//! windows/scripts/third-party-notices.ps1 writes on Windows.
//! macos/scripts/release.sh puts the Sparkle notice in front of it.
//!
//!   cargo run --example third_party_notices -- --target aarch64-apple-darwin \
//!       [--target x86_64-apple-darwin] [--binary flow] [--package <name>]... \
//!       [--features <package/feature>]... --out notices.txt
//!
//! The crates are those the workspace's root package (flow-terminal) uses, or
//! with --package, those the named packages use (flow-sdk-c and flow-demos,
//! or a game, which, a workspace's own, isn't listed itself). --features
//! turns on what the build turned on, so a crate behind a feature is in the
//! graph. --manifest-path reads another workspace: Flow-SDK's, for flowapp's
//! flow-sdk package, or Flow-Games', whose scripts/app-release.sh runs this
//! with `--manifest-path <Flow-Games>/Cargo.toml --package flow-rogue`.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context};
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    let mut targets = Vec::new();
    let (mut out, mut binary) = (None, "flow".to_string());
    let (mut roots, mut features) = (Vec::new(), Vec::new());
    let mut manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => targets.push(args.next().context("--target needs a value")?),
            "--package" => roots.push(args.next().context("--package needs a value")?),
            "--features" => features.push(args.next().context("--features needs a value")?),
            "--out" => out = Some(args.next().context("--out needs a value")?),
            "--binary" => binary = args.next().context("--binary needs a value")?,
            "--manifest-path" => manifest = args.next().context("--manifest-path needs a value")?.into(),
            other => bail!("unknown argument {other}"),
        }
    }
    let (Some(out), false) = (out, targets.is_empty()) else {
        bail!("usage: third_party_notices --target <triple> [--target <triple>] [--binary <name>] [--package <name>]... [--features <package/feature>]... --out <file>");
    };

    // name+version -> package; the crates are the union over the targets.
    let mut crates: BTreeMap<(String, String), Value> = BTreeMap::new();
    for target in &targets {
        let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["metadata", "--format-version", "1", "--locked", "--filter-platform", target, "--manifest-path"])
            .arg(&manifest)
            .args(features.iter().flat_map(|f| ["--features", f.as_str()]))
            .output()
            .context("running cargo metadata")?;
        if !output.status.success() {
            bail!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        let meta: Value = serde_json::from_slice(&output.stdout)?;
        // Our own workspace crates (flow-core, flow-macos) have no source registry: leave them out.
        for p in shipped(&meta, &roots)?.into_iter().filter(|p| !p["source"].is_null()) {
            let key = (str_field(&p, "name"), str_field(&p, "version"));
            crates.insert(key, p);
        }
    }

    // Identical license texts (most Apache-2.0 copies) are printed once and numbered.
    let mut text_index: HashMap<String, usize> = HashMap::new();
    let mut texts: Vec<(String, Vec<String>)> = Vec::new();
    let mut list = Vec::new();
    for ((name, version), p) in &crates {
        let dir = Path::new(p["manifest_path"].as_str().unwrap_or_default()).parent().map(Path::to_path_buf);
        let mut refs = Vec::new();
        // A crate that builds a C library from its source (libwebp-sys)
        // carries that library's own notices in `vendor/`, beside its own.
        let mut files: Vec<_> = dir
            .into_iter()
            .flat_map(|d| [d.join("vendor"), d])
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file() && is_license_file(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        files.sort();
        for file in files {
            let Ok(raw) = std::fs::read_to_string(&file) else { continue };
            let text = raw.replace("\r\n", "\n").lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let n = *text_index.entry(text.clone()).or_insert_with(|| {
                texts.push((text, Vec::new()));
                texts.len()
            });
            texts[n - 1].1.push(format!("{name} {version}"));
            if !refs.contains(&n) {
                refs.push(n);
            }
        }
        let source = p["repository"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("https://crates.io/crates/{name}/{version}"));
        let license = p["license"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("see {}", p["license_file"].as_str().unwrap_or("the crate")));
        list.push((format!("{name} {version}"), license, source, refs));
    }
    // A crate that ships no license text but may be had under Apache-2.0
    // (draco-core, say) is pointed at that license's own text, which names
    // no author, as another crate shipped it. MIT and the like carry their
    // authors' copyright, so they can't be borrowed.
    let apache = texts.iter().position(|(text, _)| text.starts_with("Apache License") && text.contains("Version 2.0, January 2004"));
    let list: Vec<String> = list
        .into_iter()
        .map(|(crate_, license, source, mut refs)| {
            let borrowed = refs.is_empty() && license.split([' ', '/', '(', ')']).any(|id| id == "Apache-2.0");
            let text_line = match apache {
                Some(n) if borrowed => {
                    texts[n].1.push(format!("{crate_} (which ships no text of its own)"));
                    refs.push(n + 1);
                    format!("[{}], the license's own text (none in the published crate)", n + 1)
                }
                _ if refs.is_empty() => "not in the published crate; see Source".to_string(),
                _ => refs.iter().map(|n| format!("[{n}]")).collect::<Vec<_>>().join(" "),
            };
            format!("{crate_}\n    License: {license}\n    Source:  {source}\n    Text:    {text_line}\n")
        })
        .collect();

    let rule = "-".repeat(79);
    let mut doc = format!("{rule}\nRust crates compiled into {binary} ({})\n{rule}\n\n", crates.len());
    for entry in &list {
        doc.push_str(entry);
        doc.push('\n');
    }
    doc.push_str(&format!("{rule}\nLicense texts (numbered as in the Text: lines above)\n{rule}\n"));
    for (i, (text, users)) in texts.iter().enumerate() {
        doc.push_str(&format!("\n[{}] {}\n\n{text}\n", i + 1, users.join(", ")));
    }
    std::fs::write(&out, doc).with_context(|| format!("writing {out}"))?;
    println!("Wrote {out} ({} crates, {} license texts)", crates.len(), texts.len());
    Ok(())
}

/// Packages reached from the root, or from the packages named in `roots`,
/// through normal dependencies. A name is the workspace's package, else one
/// it depends on. Build and dev dependencies don't ship, and neither do
/// proc-macro crates or what only they pull in.
fn shipped(meta: &Value, roots: &[String]) -> anyhow::Result<Vec<Value>> {
    let packages: HashMap<&str, &Value> = meta["packages"]
        .as_array()
        .context("no packages in cargo metadata")?
        .iter()
        .map(|p| (p["id"].as_str().unwrap_or_default(), p))
        .collect();
    let nodes: HashMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .context("no resolve graph in cargo metadata")?
        .iter()
        .map(|n| (n["id"].as_str().unwrap_or_default(), n))
        .collect();
    let members: Vec<&str> = meta["workspace_members"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let starts: Vec<&str> = if roots.is_empty() {
        vec![meta["resolve"]["root"].as_str().context("no root package")?]
    } else {
        roots
            .iter()
            .map(|name| {
                let named = |id: &&str| packages.get(id).is_some_and(|p| p["name"] == name.as_str());
                members
                    .iter()
                    .copied()
                    .find(named)
                    .or_else(|| nodes.keys().copied().find(named))
                    .with_context(|| format!("no package named {name}"))
            })
            .collect::<anyhow::Result<_>>()?
    };
    let is_proc_macro = |id: &str| {
        packages.get(id).and_then(|p| p["targets"].as_array()).is_some_and(|targets| {
            targets.iter().any(|t| t["kind"].as_array().is_some_and(|k| k.iter().any(|k| k == "proc-macro")))
        })
    };
    let (mut seen, mut queue) = (HashSet::new(), VecDeque::from(starts.clone()));
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        for dep in nodes.get(id).and_then(|n| n["deps"].as_array()).into_iter().flatten() {
            let normal = dep["dep_kinds"].as_array().is_some_and(|kinds| kinds.iter().any(|k| k["kind"].is_null()));
            let pkg = dep["pkg"].as_str().unwrap_or_default();
            if normal && !is_proc_macro(pkg) {
                queue.push_back(pkg);
            }
        }
    }
    for id in &starts {
        seen.remove(id);
    }
    Ok(seen.into_iter().filter_map(|id| packages.get(id).map(|p| (*p).clone())).collect())
}

fn is_license_file(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE", "PATENTS"].iter().any(|p| upper.starts_with(p))
}

fn str_field(p: &Value, key: &str) -> String {
    p[key].as_str().unwrap_or_default().to_string()
}
