//! The user's standing instructions to the assistant, and its skills.
//!
//! The instructions are a text file of the user's own beside `config.toml`
//! ([`INSTRUCTIONS`]), sent in the system prompt: their rules and tastes,
//! which Flow's own prompt and approvals still stand above.
//!
//! Skills are playbooks the model reads only when a task needs them: the
//! system prompt lists each one's name and description, and `read_skill`
//! gives its text. They live in a folder beside `config.toml` ([`DIR`]),
//! each a Markdown file, `<name>.md`, or a folder, `<name>/SKILL.md` with
//! files of its own beside it, as Agent Skills are laid out, so skills
//! written for other assistants work here too. Each begins with front
//! matter naming it and saying what it's for:
//!
//! ```text
//! ---
//! name: windows-drivers
//! description: Checking and updating drivers on Windows, a restore point first.
//! ---
//! ```
//!
//! Packages `flowapp` installs may bring skills too, in a `skills/` folder
//! laid out as the user's: an app beside its programs, or a package of
//! nothing but skills ([`FILE`] at its root, as `themes.json` is a package
//! of themes'). Each of a package's skills is named `<package>:<name>`
//! ([`gather`]), so none takes the place of the user's, and is read only
//! once the user has turned it on in Settings (`ai.skills_on`): installing
//! an app never changes what the assistant reads.

use std::path::{Path, PathBuf};

/// The skills' folder, beside `config.toml`.
pub const DIR: &str = "assistant-skills";
/// The instructions' file, beside `config.toml`.
pub const INSTRUCTIONS: &str = "assistant-instructions.txt";
/// The most of the instructions sent, in bytes.
pub const INSTRUCTIONS_BYTES: usize = 8 * 1024;
/// The most skills listed.
const MOST: usize = 50;
/// The longest a name and a description may be, in characters.
const NAME_CHARS: usize = 64;
const DESCRIPTION_CHARS: usize = 300;
/// The most of a skill's text `read_skill` gives, in bytes.
const SKILL_BYTES: usize = 64 * 1024;
/// A folder skill's own file.
const SKILL_FILE: &str = "SKILL.md";
/// What a package of nothing but skills has at its root, naming it.
pub const FILE: &str = "skills.json";
/// The folder of skills in a package's version folder.
pub const PACKAGE_DIR: &str = "skills";
/// The package file's format: one of another is refused.
pub const FORMAT: u32 = 1;

/// A skill, as its front matter names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    /// Its name: the front matter's, or `<package>:<name>` for a package's.
    pub name: String,
    pub description: String,
    /// Its file: `<name>.md`, or `<name>/SKILL.md`.
    pub path: PathBuf,
    /// The package it came in; none for the user's own.
    pub from: Option<String>,
}

/// A package of nothing but skills' [`FILE`]: its name and version, as a
/// package of themes' `themes.json` says them.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub package: String,
    pub version: String,
}

/// The package file in `dir`, read and checked: its format, and skills in
/// its `skills/` that read.
pub fn manifest(dir: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(dir.join(FILE)).map_err(|_| format!("it holds no {FILE}"))?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| format!("{FILE}: {e}"))?;
    if manifest.format != FORMAT {
        return Err(format!("{FILE}: format {} (this Flow reads {FORMAT})", manifest.format));
    }
    if manifest.package.is_empty() || manifest.version.is_empty() {
        return Err(format!("{FILE}: its package and version are needed"));
    }
    if list(&dir.join(PACKAGE_DIR)).is_empty() {
        return Err(format!("its {PACKAGE_DIR} folder holds no skill that reads (each needs front matter with a name and a description)"));
    }
    Ok(manifest)
}

/// Whether `name` is a skill's: lowercase letters, digits and hyphens, at
/// most [`NAME_CHARS`].
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= NAME_CHARS && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A skill's front matter (its name and description) and the text after
/// it; none where it has no front matter with both.
fn front_matter(text: &str) -> Option<(String, String, &str)> {
    let text = text.trim_start_matches('\u{feff}');
    let rest = text.strip_prefix("---")?.trim_start_matches([' ', '\t']);
    let rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'))?;
    let end = rest.find("\n---")?;
    let (head, body) = (&rest[..end], &rest[end + 4..]);
    let body = body.split_once('\n').map_or("", |(_, b)| b);
    let field = |key: &str| {
        head.lines().find_map(|line| {
            let (k, v) = line.split_once(':')?;
            (k.trim() == key).then(|| v.trim().trim_matches(['"', '\'']).trim().to_string())
        })
    };
    let (name, description) = (field("name")?, field("description")?);
    (!description.is_empty()).then_some((name, description, body))
}

/// The skills in `dir`, by name, at most [`MOST`]: each `<name>.md` and
/// `<name>/SKILL.md` whose front matter has a valid name and a description
/// of at most [`DESCRIPTION_CHARS`]; others are passed over.
pub fn list(dir: &Path) -> Vec<Skill> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut skills: Vec<Skill> = read
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let file = if path.is_dir() {
                path.join(SKILL_FILE)
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) {
                path
            } else {
                return None;
            };
            let text = std::fs::read_to_string(&file).ok()?;
            let (name, description, _) = front_matter(&text)?;
            (valid_name(&name) && description.chars().count() <= DESCRIPTION_CHARS).then_some(Skill { name, description, path: file, from: None })
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills.dedup_by(|a, b| a.name == b.name);
    skills.truncate(MOST);
    skills
}

/// The skills the assistant may read: the user's own in `dir` but those
/// they switched `off`, then each package's in `packages` (its name and
/// its skills' folder) that they turned `on`, by `<package>:<name>`; at
/// most [`MOST`] in all, the user's own first, so a package's never pushes
/// one of theirs out.
pub fn gather(dir: &Path, packages: &[(String, PathBuf)], off: &[String], on: &[String]) -> Vec<Skill> {
    let mut skills: Vec<Skill> = list(dir).into_iter().filter(|s| !off.contains(&s.name)).collect();
    for (package, folder) in packages {
        for mut skill in list(folder) {
            skill.name = format!("{package}:{}", skill.name);
            if on.contains(&skill.name) {
                skill.from = Some(package.clone());
                skills.push(skill);
            }
        }
    }
    skills.truncate(MOST);
    skills
}

/// The packages' skills the user turned on in Settings, as config.toml
/// says (`ai.skills_on`, each `<package>:<name>`), for `flowapp`, which
/// reads it without loading the rest; none where it says none or doesn't
/// read.
pub fn turned_on() -> Vec<String> {
    let text = std::fs::read_to_string(crate::config::config_path()).unwrap_or_default();
    let read: toml::Value = toml::from_str(&text).unwrap_or(toml::Value::Table(Default::default()));
    read.get("ai")
        .and_then(|ai| ai.get("skills_on"))
        .and_then(|from| from.as_array())
        .map(|from| from.iter().filter_map(|n| n.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// What `read_skill` gives of skill `name` among `skills`: its text after
/// the front matter, at most [`SKILL_BYTES`], and for a folder skill the
/// paths of its other files, for `read_file`.
pub fn read(skills: &[Skill], name: &str) -> Result<String, String> {
    let Some(skill) = skills.iter().find(|s| s.name == name) else {
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        return Err(match names.as_slice() {
            [] => format!("There's no skill \"{name}\": there are none."),
            _ => format!("There's no skill \"{name}\". The skills: {}.", names.join(", ")),
        });
    };
    let text = std::fs::read_to_string(&skill.path).map_err(|e| e.to_string())?;
    let body = front_matter(&text).map_or(text.as_str(), |(.., body)| body).trim();
    let mut out = format!("# {}\n{}\n\n", skill.name, skill.description);
    let mut cut = body.len().min(SKILL_BYTES);
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    out.push_str(&body[..cut]);
    if cut < body.len() {
        out.push_str("\n\n(The rest of this skill was cut.)");
    }
    if skill.path.file_name().is_some_and(|f| f == SKILL_FILE) {
        let folder = skill.path.parent().unwrap_or(&skill.path);
        let mut others: Vec<String> = walk(folder).into_iter().filter(|p| *p != skill.path).map(|p| p.display().to_string()).collect();
        others.sort();
        if !others.is_empty() {
            out.push_str(&format!("\n\nIts other files, which read_file reads:\n- {}", others.join("\n- ")));
        }
    }
    Ok(out)
}

/// The files under `folder`, a few levels down.
fn walk(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 3 {
                    dirs.push((path, depth + 1));
                }
            } else if found.len() < 100 {
                found.push(path);
            }
        }
    }
    found
}

/// Keeps a skill the assistant wrote as `<name>.md` in `dir`: what to tell
/// the model.
pub fn save(dir: &Path, name: &str, description: &str, body: &str) -> Result<String, String> {
    if !valid_name(name) {
        return Err(format!("A skill's name is lowercase letters, digits and hyphens, at most {NAME_CHARS}: \"{name}\" isn't."));
    }
    let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if description.is_empty() || description.chars().count() > DESCRIPTION_CHARS {
        return Err(format!("A skill's description is one sentence of at most {DESCRIPTION_CHARS} characters."));
    }
    if body.trim().is_empty() {
        return Err("The skill has no text.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // A folder skill of that name is the user's: never written over.
    if dir.join(name).is_dir() {
        return Err(format!("The skill \"{name}\" is a folder of the user's own, which isn't written over."));
    }
    let path = dir.join(format!("{name}.md"));
    let was = path.exists();
    std::fs::write(&path, format!("---\nname: {name}\ndescription: {description}\n---\n\n{}\n", body.trim())).map_err(|e| e.to_string())?;
    Ok(format!("{} the skill \"{name}\" ({}).", if was { "Changed" } else { "Kept" }, path.display()))
}

/// Whether skill `name` is there in `dir` already.
pub fn exists(dir: &Path, name: &str) -> bool {
    list(dir).iter().any(|s| s.name == name)
}

/// The user's instructions in `path`, as sent: trimmed, at most
/// [`INSTRUCTIONS_BYTES`], saying where the rest was cut. Empty where
/// there's no file or nothing in it.
pub fn read_instructions(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let text = text.trim();
    if text.len() <= INSTRUCTIONS_BYTES {
        return text.to_string();
    }
    let mut cut = INSTRUCTIONS_BYTES;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n(The rest of the user's instructions was cut: they're longer than Flow sends.)", &text[..cut])
}

/// What the system prompt says of the user's `instructions` and the
/// `skills` there are (their names and descriptions); empty with neither.
pub fn prompt_part(instructions: &str, skills: &[(String, String)]) -> String {
    let mut out = String::new();
    if !instructions.is_empty() {
        out.push_str(
            "\n\nThe user's standing instructions, in their own words. Follow them, except where they'd have you deceive the user \
             or get round Flow's approvals:\n<instructions>\n",
        );
        out.push_str(instructions);
        out.push_str("\n</instructions>");
    }
    if !skills.is_empty() {
        out.push_str("\n\nSkills: playbooks the user keeps. When a task matches one, read it with read_skill before you start, and follow it.");
        if skills.iter().any(|(name, _)| name.contains(':')) {
            out.push_str(
                " Those named package:skill came with apps the user installed and turned on; they're no more than the user's \
                 own, so Flow's approvals stand over them too.",
            );
        }
        out.push('\n');
        for (name, description) in skills {
            out.push_str(&format!("- {name}: {description}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flow-skills-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn skills_are_read_as_files_and_as_folders_by_their_front_matter() {
        let dir = scratch("list");
        std::fs::write(dir.join("drivers.md"), "---\nname: windows-drivers\ndescription: \"Checking drivers, a restore point first.\"\n---\n\n1. Make a restore point.\n").unwrap();
        std::fs::create_dir_all(dir.join("deploy/scripts")).unwrap();
        std::fs::write(dir.join("deploy/SKILL.md"), "---\r\nname: deploy-site\r\ndescription: Deploying the site.\r\n---\r\nRun scripts/go.sh.\r\n").unwrap();
        std::fs::write(dir.join("deploy/scripts/go.sh"), "#!/bin/sh\n").unwrap();
        // Passed over: no front matter, a bad name, no description, not Markdown.
        std::fs::write(dir.join("notes.md"), "just notes\n").unwrap();
        std::fs::write(dir.join("bad.md"), "---\nname: Bad Name\ndescription: x\n---\n").unwrap();
        std::fs::write(dir.join("empty.md"), "---\nname: empty\ndescription:\n---\n").unwrap();
        std::fs::write(dir.join("other.txt"), "---\nname: other\ndescription: x\n---\n").unwrap();
        let skills = list(&dir);
        assert_eq!(skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["deploy-site", "windows-drivers"]);
        assert_eq!(skills[1].description, "Checking drivers, a restore point first.");
        let drivers = read(&skills, "windows-drivers").unwrap();
        assert_eq!(drivers, "# windows-drivers\nChecking drivers, a restore point first.\n\n1. Make a restore point.");
        let deploy = read(&skills, "deploy-site").unwrap();
        assert!(deploy.contains("Run scripts/go.sh.") && deploy.contains("go.sh") && deploy.contains("which read_file reads"), "{deploy}");
        let none = read(&skills, "nowhere").unwrap_err();
        assert!(none.contains("deploy-site, windows-drivers"), "{none}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skill_the_assistant_keeps_is_written_and_read_back() {
        let dir = scratch("save");
        assert!(save(&dir, "Fix Wifi", "x", "y").is_err(), "a name of capitals and spaces");
        assert!(save(&dir, "fix-wifi", "", "y").is_err());
        assert!(save(&dir, "fix-wifi", &"x".repeat(301), "y").is_err());
        assert!(save(&dir, "fix-wifi", "x", " ").is_err());
        assert!(!exists(&dir, "fix-wifi"));
        assert!(save(&dir, "fix-wifi", "Bringing the Wi-Fi back\n after sleep.", "1. Turn it off and on.").unwrap().starts_with("Kept"));
        assert!(exists(&dir, "fix-wifi"));
        assert_eq!(list(&dir)[0].description, "Bringing the Wi-Fi back after sleep.");
        assert!(read(&list(&dir), "fix-wifi").unwrap().ends_with("1. Turn it off and on."));
        assert!(save(&dir, "fix-wifi", "Again.", "2.").unwrap().starts_with("Changed"));
        std::fs::create_dir_all(dir.join("mine")).unwrap();
        assert!(save(&dir, "mine", "x", "y").is_err(), "a folder skill is the user's own");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn packages_skills_come_after_the_users_own_named_by_their_package() {
        let dir = scratch("gather");
        let mine = dir.join("mine");
        let utils = dir.join("utils");
        let sdk = dir.join("sdk");
        for d in [&mine, &utils, &sdk] {
            std::fs::create_dir_all(d).unwrap();
        }
        let skill = |at: &Path, name: &str| std::fs::write(at.join(format!("{name}.md")), format!("---\nname: {name}\ndescription: About {name}.\n---\nDo {name}.\n")).unwrap();
        skill(&mine, "tree");
        skill(&utils, "tree");
        skill(&utils, "find-files");
        std::fs::create_dir_all(sdk.join("effects/examples")).unwrap();
        std::fs::write(sdk.join("effects/SKILL.md"), "---\nname: effects\ndescription: Effects from a program.\n---\nSend OSC 7731.\n").unwrap();
        std::fs::write(sdk.join("effects/examples/burst.py"), "print()\n").unwrap();
        let packages = [("flow-utils".to_string(), utils.clone()), ("flow-sdk".to_string(), sdk.clone())];
        // A package's skills are off until each is turned on, by the name
        // it's listed by.
        assert_eq!(gather(&mine, &packages, &[], &[]).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["tree"]);
        let on: Vec<String> = ["flow-utils:find-files", "flow-utils:tree", "flow-sdk:effects"].map(String::from).into();
        let skills = gather(&mine, &packages, &[], &on);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["tree", "flow-utils:find-files", "flow-utils:tree", "flow-sdk:effects"], "the user's own first, then each package's, by name");
        assert_eq!((skills[0].from.as_deref(), skills[2].from.as_deref()), (None, Some("flow-utils")));
        // Read by the name the list gives, the user's own and a package's
        // apart; a package's folder skill names its other files.
        assert!(read(&skills, "tree").unwrap().ends_with("Do tree."));
        assert!(read(&skills, "flow-utils:tree").unwrap().starts_with("# flow-utils:tree\nAbout tree."));
        let effects = read(&skills, "flow-sdk:effects").unwrap();
        assert!(effects.contains("Send OSC 7731.") && effects.contains("burst.py"), "{effects}");
        // One of a package's turned on alone, and the user's own switched
        // off, each by its name.
        let names: Vec<String> = gather(&mine, &packages, &["tree".to_string()], &["flow-utils:tree".to_string()]).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["flow-utils:tree"], "the user's tree off, the package's on");
        // At most MOST in all, the user's own kept.
        let mut on = on.clone();
        for i in 0..MOST {
            skill(&utils, &format!("more-{i:02}"));
            on.push(format!("flow-utils:more-{i:02}"));
        }
        let many = gather(&mine, &packages, &[], &on);
        assert_eq!((many.len(), many[0].name.as_str()), (MOST, "tree"));
        // The prompt says where a package's came from.
        let pairs: Vec<(String, String)> = skills.iter().map(|s| (s.name.clone(), s.description.clone())).collect();
        let part = prompt_part("", &pairs);
        assert!(part.contains("came with apps the user installed") && part.contains("- flow-sdk:effects: Effects from a program.\n"), "{part}");
        assert!(!prompt_part("", &pairs[..1]).contains("came with apps"), "said only where there are some");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_package_of_skills_is_named_by_its_file_and_needs_a_skill_that_reads() {
        let dir = scratch("manifest");
        assert!(manifest(&dir).unwrap_err().contains("skills.json"));
        std::fs::write(dir.join(FILE), r#"{"format":1,"package":"flow-skills","version":"0.1.0"}"#).unwrap();
        assert!(manifest(&dir).unwrap_err().contains("no skill that reads"));
        std::fs::create_dir_all(dir.join(PACKAGE_DIR)).unwrap();
        std::fs::write(dir.join(PACKAGE_DIR).join("notes.md"), "no front matter\n").unwrap();
        assert!(manifest(&dir).is_err());
        std::fs::write(dir.join(PACKAGE_DIR).join("theme.md"), "---\nname: theme-package\ndescription: Making a package of themes.\n---\n1.\n").unwrap();
        assert_eq!(manifest(&dir).unwrap(), Manifest { format: 1, package: "flow-skills".into(), version: "0.1.0".into() });
        std::fs::write(dir.join(FILE), r#"{"format":2,"package":"flow-skills","version":"0.1.0"}"#).unwrap();
        assert!(manifest(&dir).unwrap_err().contains("format 2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_instructions_are_sent_trimmed_and_cut_past_their_room() {
        let dir = scratch("instructions");
        let path = dir.join(INSTRUCTIONS);
        assert_eq!(read_instructions(&path), "");
        std::fs::write(&path, "\n  Always use winget.\n\n").unwrap();
        assert_eq!(read_instructions(&path), "Always use winget.");
        std::fs::write(&path, "é".repeat(INSTRUCTIONS_BYTES)).unwrap();
        let cut = read_instructions(&path);
        assert!(cut.len() < INSTRUCTIONS_BYTES + 200 && cut.ends_with("than Flow sends.)"));
        assert_eq!(prompt_part("", &[]), "");
        let part = prompt_part("Always use winget.", &[("fix-wifi".into(), "Bringing the Wi-Fi back.".into())]);
        assert!(part.contains("<instructions>\nAlways use winget.\n</instructions>") && part.contains("- fix-wifi: Bringing the Wi-Fi back.\n"), "{part}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
