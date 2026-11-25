//! What `flowapp first` puts ahead of the system's commands in Flow's tabs,
//! and how each tab's shell is told of it.
//!
//! Each choice is kept in `apps/first.json` ([`Record`]): a program (an
//! app's, where it's named as `<app>/<program>`), the shells it's first in
//! (all, some, or all but some, by [`SHELLS`]' names), how (first on the
//! PATH, so scripts find it too, or at the prompt alone, an alias the hook
//! makes), and what it takes away of what stands ahead of the PATH there:
//! an alias, a builtin, a function, an applet ([`Take`]; by default
//! PowerShell's alias and BusyBox's applet, as before there were choices).
//! A later choice takes the place of an earlier one on the shells it names.
//!
//! Every pane has a folder of its own, `apps/first/panes/<flow pid>-<session
//! id>/`, made as its shell starts ([`open_pane`]) and gone with it
//! ([`close_pane`]), which is its `FLOW_FIRST`, first on its PATH. It
//! holds, for its shell (`.shell` says which): the programs first on the
//! PATH, linked by their names; those first at the prompt alone, linked in
//! `.prompt/`, which the hook makes aliases of; what the hook takes away,
//! as lines of `<what> <name>` in `.take`; and the pane's own choices
//! (`--here`, [`Here`]) in `.here`. The hooks (`shell.rs`) read its listing,
//! `.prompt/`'s and `.take` before each prompt, and the `*` they list by
//! passes over the dotfiles. `flowapp first`, `last`, `remove` and an
//! install write every pane's folder afresh ([`rewrite_panes`]), so open
//! tabs follow at their next prompt; the first pane a Flow opens sweeps
//! away the folders of Flows that are gone ([`sweep`]).
//!
//! What stands ahead of the PATH in each shell, as the shells have it
//! unchanged by anyone's startup files, is in tables here ([`ahead`]), for
//! `flowapp first` to say what still comes first and what it took away.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{exe, Installed};
use crate::shell::{BashFlavor, ShellKind, ShellProfile};

/// The record of choices, beside `installed.json`.
const RECORD: &str = "first.json";

/// In a pane's folder: the shell it's for.
const SHELL_FILE: &str = ".shell";
/// In a pane's folder: what its hook takes away, a `<what> <name>` a line.
const TAKE_FILE: &str = ".take";
/// In a pane's folder: the programs first at the prompt alone.
const PROMPT_DIR: &str = ".prompt";
/// In a pane's folder: its own choices.
const HERE_FILE: &str = ".here";

/// The shells a choice can name (`--shell`, `--except`), by the names of
/// what Flow's hooks do in them: `bash` is every real bash (the Mac's,
/// Linux's, Git Bash, MSYS2), `busybox` the BusyBox ash that comes with
/// Flow on Windows, `sh` a plain sh with no hook of Flow's.
pub const SHELLS: [&str; 6] = ["zsh", "bash", "sh", "powershell", "cmd", "busybox"];

/// The name [`SHELLS`] gives a shell of `kind` and `flavor`.
pub fn shell_name(kind: ShellKind, flavor: BashFlavor) -> &'static str {
    match kind {
        ShellKind::Zsh => "zsh",
        ShellKind::Bash if flavor == BashFlavor::BusyBox => "busybox",
        ShellKind::Bash => "bash",
        ShellKind::Sh => "sh",
        ShellKind::PowerShell => "powershell",
        ShellKind::Cmd => "cmd",
    }
}

/// The shells Flow offers on this system, for `flowapp first` to say what's
/// first in each.
pub fn system_shells() -> &'static [&'static str] {
    if cfg!(windows) {
        &["powershell", "cmd", "bash", "busybox"]
    } else {
        &["zsh", "bash"]
    }
}

/// Whether `shell` has a hook of Flow's before each prompt, which a command
/// first at the prompt alone needs: cmd's prompt runs nothing, and a plain
/// sh has none of Flow's.
pub fn has_prompt_hook(shell: &str) -> bool {
    !matches!(shell, "cmd" | "sh")
}

/// The shells a choice is for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shells {
    #[default]
    All,
    Only(Vec<String>),
    Except(Vec<String>),
}

impl Shells {
    /// Whether `shell` is one of them.
    pub fn has(&self, shell: &str) -> bool {
        match self {
            Shells::All => true,
            Shells::Only(names) => names.iter().any(|n| n == shell),
            Shells::Except(names) => !names.iter().any(|n| n == shell),
        }
    }

    /// Those of them that aren't `other`'s; none where nothing's left.
    pub fn without(&self, other: &Shells) -> Option<Shells> {
        let left = match (self, other) {
            (_, Shells::All) => return None,
            (Shells::All, Shells::Only(b)) => Shells::Except(b.clone()),
            (Shells::Only(a), Shells::Only(b)) => Shells::Only(a.iter().filter(|n| !b.contains(n)).cloned().collect()),
            (Shells::Except(a), Shells::Only(b)) => Shells::Except(union(a, b)),
            (Shells::All, Shells::Except(b)) => Shells::Only(b.clone()),
            (Shells::Only(a), Shells::Except(b)) => Shells::Only(a.iter().filter(|n| b.contains(n)).cloned().collect()),
            (Shells::Except(a), Shells::Except(b)) => Shells::Only(b.iter().filter(|n| !a.contains(n)).cloned().collect()),
        };
        left.some()
    }

    /// Those of them that are `other`'s too; none where there are none.
    pub fn meet(&self, other: &Shells) -> Option<Shells> {
        let both = match (self, other) {
            (Shells::All, x) | (x, Shells::All) => x.clone(),
            (Shells::Only(a), Shells::Only(b)) => Shells::Only(a.iter().filter(|n| b.contains(n)).cloned().collect()),
            (Shells::Only(a), Shells::Except(b)) | (Shells::Except(b), Shells::Only(a)) => Shells::Only(a.iter().filter(|n| !b.contains(n)).cloned().collect()),
            (Shells::Except(a), Shells::Except(b)) => Shells::Except(union(a, b)),
        };
        both.some()
    }

    /// Itself, or none where it names no shell at all.
    fn some(self) -> Option<Shells> {
        match &self {
            Shells::Only(names) if names.is_empty() => None,
            Shells::Except(names) if SHELLS.iter().all(|s| names.iter().any(|n| n == s)) => None,
            _ => Some(self),
        }
    }
}

/// `a` and those of `b` it hasn't, in order.
fn union(a: &[String], b: &[String]) -> Vec<String> {
    let mut out = a.to_vec();
    out.extend(b.iter().filter(|n| !a.contains(n)).cloned());
    out
}

/// How a program is first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    /// First on the PATH, which scripts run from the tab inherit too.
    #[default]
    Path,
    /// First at the prompt alone, as an alias the hook makes, so scripts
    /// keep the system's command.
    Prompt,
}

impl How {
    fn is_path(&self) -> bool {
        *self == How::Path
    }
}

/// What a program put first takes away of what stands ahead of the PATH
/// in a shell, which is given back once it's no longer first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Take {
    /// PowerShell's own alias of its name (`ls`, `cat`), and the aliases
    /// of it the user's files make in bash, zsh and BusyBox's ash.
    Alias,
    /// bash's and zsh's builtin of its name (`echo`, `printf`): bash's
    /// `enable -n`, zsh's `disable`.
    Builtin,
    /// PowerShell's own function of its name (`mkdir`, `more`).
    Function,
    /// BusyBox's applet of its name, which BusyBox runs ahead of the PATH
    /// but for the names in `BB_OVERRIDE_APPLETS`.
    Applet,
}

impl Take {
    pub const ALL: [Take; 4] = [Take::Alias, Take::Builtin, Take::Function, Take::Applet];

    /// Its name, as `--take` and `.take` write it.
    pub fn word(self) -> &'static str {
        match self {
            Take::Alias => "alias",
            Take::Builtin => "builtin",
            Take::Function => "function",
            Take::Applet => "applet",
        }
    }

    pub fn parse(word: &str) -> Option<Take> {
        Take::ALL.into_iter().find(|t| t.word() == word)
    }

    /// Whether `shell` has such things, which its hook can take away.
    pub fn in_shell(self, shell: &str) -> bool {
        match self {
            Take::Alias => matches!(shell, "zsh" | "bash" | "busybox" | "powershell"),
            Take::Builtin => matches!(shell, "zsh" | "bash"),
            Take::Function => shell == "powershell",
            Take::Applet => shell == "busybox",
        }
    }
}

/// One choice of `flowapp first`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    /// The program, by its name (without `.exe`).
    pub program: String,
    /// The app whose program it is, where it was named as `<app>/<program>`
    /// (linked from that app's version); none for the one in `bin/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default)]
    pub shells: Shells,
    #[serde(default, skip_serializing_if = "How::is_path")]
    pub how: How,
    /// What it takes away, where that was asked; none for the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub take: Option<Vec<Take>>,
}

impl Choice {
    /// `program`, first on the PATH in every shell, taking away what it
    /// takes by default.
    pub fn new(program: &str) -> Self {
        Choice { program: program.to_string(), app: None, shells: Shells::All, how: How::Path, take: None }
    }

    /// Whether it's first in `shell`: one of its shells, and where it's
    /// first at the prompt alone, a shell with a hook there.
    pub fn in_shell(&self, shell: &str) -> bool {
        self.shells.has(shell) && (self.how == How::Path || has_prompt_hook(shell))
    }

    /// What it takes away in `shell`: what was asked of what `shell` has,
    /// or by default PowerShell's alias and BusyBox's applet; first at the
    /// prompt alone, nothing, as the alias the hook makes comes ahead of
    /// all of them.
    pub fn takes_in(&self, shell: &str) -> Vec<Take> {
        if self.how == How::Prompt {
            return Vec::new();
        }
        let mut takes: Vec<Take> = match &self.take {
            None => match shell {
                "powershell" => vec![Take::Alias],
                "busybox" => vec![Take::Applet],
                _ => Vec::new(),
            },
            Some(asked) => asked.iter().copied().filter(|t| t.in_shell(shell)).collect(),
        };
        takes.sort();
        takes.dedup();
        takes
    }
}

/// Every choice made, in the order made, kept in `apps/first.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    pub firsts: Vec<Choice>,
}

impl Record {
    fn path(root: &Path) -> PathBuf {
        root.join(RECORD)
    }

    /// The choices made under `root`; none where none were.
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::path(root)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Kept whole, then put in place, so a crash leaves the one before.
    pub fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let new = root.join(format!("{RECORD}.new"));
        std::fs::write(&new, text).map_err(|e| e.to_string())?;
        std::fs::rename(&new, Self::path(root)).map_err(|e| e.to_string())
    }

    /// Puts `choice` first on its shells, in place of the choices of its
    /// name there, which it says, each with the shells where it takes its
    /// place.
    pub fn put(&mut self, choice: Choice) -> Vec<Choice> {
        let mut replaced = Vec::new();
        let mut kept = Vec::new();
        for old in std::mem::take(&mut self.firsts) {
            if old.program != choice.program {
                kept.push(old);
                continue;
            }
            if let Some(both) = old.shells.meet(&choice.shells) {
                replaced.push(Choice { shells: both, ..old.clone() });
            }
            if let Some(left) = old.shells.without(&choice.shells) {
                kept.push(Choice { shells: left, ..old });
            }
        }
        kept.push(choice);
        self.firsts = kept;
        replaced
    }

    /// Takes `program` out from first in `shells`. Whether it was first in
    /// any of them.
    pub fn put_last(&mut self, program: &str, shells: &Shells) -> bool {
        let mut was = false;
        let mut kept = Vec::new();
        for old in std::mem::take(&mut self.firsts) {
            if old.program != program {
                kept.push(old);
                continue;
            }
            was |= old.shells.meet(shells).is_some();
            if let Some(left) = old.shells.without(shells) {
                kept.push(Choice { shells: left, ..old });
            }
        }
        self.firsts = kept;
        was
    }

    /// What's first in `shell`: a choice for each name, the last made
    /// winning.
    pub fn in_shell(&self, shell: &str) -> BTreeMap<String, Choice> {
        let mut chosen = BTreeMap::new();
        for choice in self.firsts.iter().filter(|c| c.in_shell(shell)) {
            chosen.insert(choice.program.clone(), choice.clone());
        }
        chosen
    }

    /// Every name it puts first anywhere, each once.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.firsts.iter().map(|c| c.program.clone()).collect();
        names.sort();
        names.dedup();
        names
    }

    /// Forgets the choices of programs no longer `installed`: those of an
    /// app removed, or one an upgrade's version no longer has. Whether it
    /// forgot any.
    pub fn forget_gone(&mut self, installed: &Installed) -> bool {
        let before = self.firsts.len();
        self.firsts.retain(|c| match &c.app {
            Some(app) => installed.apps.get(app).is_some_and(|kept| kept.programs.contains(&c.program)),
            None => installed.apps.values().any(|kept| kept.programs.contains(&c.program)),
        });
        self.firsts.len() != before
    }
}

/// A pane's own choices (`--here`), kept in its folder's `.here`, over the
/// record's for its shell: what it put first, and what it put last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Here {
    pub firsts: Vec<Choice>,
    pub lasts: Vec<String>,
}

impl Here {
    /// The pane's in `pane`, its folder; none where it made none.
    pub fn load(pane: &Path) -> Self {
        std::fs::read_to_string(pane.join(HERE_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, pane: &Path) -> Result<(), String> {
        let file = pane.join(HERE_FILE);
        if self.firsts.is_empty() && self.lasts.is_empty() {
            let _ = std::fs::remove_file(&file);
            return Ok(());
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(file, text).map_err(|e| e.to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.firsts.is_empty() && self.lasts.is_empty()
    }

    /// Puts `choice` first in this pane alone, whatever shells it named.
    pub fn put(&mut self, choice: Choice) {
        self.lasts.retain(|n| *n != choice.program);
        self.firsts.retain(|c| c.program != choice.program);
        self.firsts.push(Choice { shells: Shells::All, ..choice });
    }

    /// Takes `program` out from first in this pane alone.
    pub fn put_last(&mut self, program: &str) {
        self.firsts.retain(|c| c.program != program);
        if !self.lasts.iter().any(|n| n == program) {
            self.lasts.push(program.to_string());
        }
    }
}

/// Why a name `flowapp first` was given stands for no program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unnamed {
    /// No app installed is called that, nor has a program called that.
    Nothing,
    /// These apps each have a program of that name, which must then be
    /// named as `<app>/<program>`.
    Shared(Vec<String>),
}

/// The programs `name` stands for, of what's `installed`, each a choice on
/// every shell: `<app>/<program>`, that app's; an app, each of its
/// programs (as its own where another app has one of the name); a program,
/// by its own name, where one app alone has it.
pub fn resolve(installed: &Installed, name: &str) -> Result<Vec<Choice>, Unnamed> {
    let owners = |program: &str| -> Vec<String> { installed.apps.iter().filter(|(_, kept)| kept.programs.iter().any(|p| p == program)).map(|(app, _)| app.clone()).collect() };
    if let Some((app, program)) = name.split_once('/') {
        let has = installed.apps.get(app).is_some_and(|kept| kept.programs.iter().any(|p| p == program));
        return match has {
            true => Ok(vec![Choice { app: Some(app.to_string()), ..Choice::new(program) }]),
            false => Err(Unnamed::Nothing),
        };
    }
    if let Some(kept) = installed.apps.get(name).filter(|kept| !kept.programs.is_empty()) {
        return Ok(kept.programs.iter().map(|p| Choice { app: (owners(p).len() > 1).then(|| name.to_string()), ..Choice::new(p) }).collect());
    }
    match owners(name).as_slice() {
        [] => Err(Unnamed::Nothing),
        [_] => Ok(vec![Choice::new(name)]),
        many => Err(Unnamed::Shared(many.to_vec())),
    }
}

/// The app whose program `choice` runs: the one it names, else the one app
/// that has a program of its name.
pub fn whose(installed: &Installed, choice: &Choice) -> Option<String> {
    if choice.app.is_some() {
        return choice.app.clone();
    }
    let mut owners = installed.apps.iter().filter(|(_, kept)| kept.programs.contains(&choice.program));
    match (owners.next(), owners.next()) {
        (Some((app, _)), None) => Some(app.clone()),
        _ => None,
    }
}

/// What's first in a pane of `shell` whose own choices are `here`: the
/// record's for its shell, less what it put last, with what it put first.
pub fn in_pane(record: &Record, here: &Here, shell: &str) -> BTreeMap<String, Choice> {
    let mut chosen = record.in_shell(shell);
    for name in &here.lasts {
        chosen.remove(name);
    }
    for choice in here.firsts.iter().filter(|c| c.in_shell(shell)) {
        chosen.insert(choice.program.clone(), choice.clone());
    }
    chosen
}

/// The program `choice` names, as a path under `root`: in its app's
/// version where it names an app, else in `bin/`. None where it isn't
/// installed.
fn target(root: &Path, installed: &Installed, choice: &Choice) -> Option<PathBuf> {
    let file = exe(&choice.program);
    let at = match &choice.app {
        Some(app) => {
            let kept = installed.apps.get(app).filter(|kept| kept.programs.contains(&choice.program))?;
            Path::new(app).join(&kept.version).join("bin").join(&file)
        }
        None => Path::new("bin").join(&file),
    };
    root.join(&at).is_file().then_some(at)
}

/// The folder of every pane's own folder, under `root` (the apps').
pub fn panes_dir(root: &Path) -> PathBuf {
    root.join("first").join("panes")
}

/// The folder of the pane whose session is `id`, in the Flow whose
/// process is `pid`.
pub fn pane_dir(root: &Path, pid: u32, id: u32) -> PathBuf {
    panes_dir(root).join(format!("{pid}-{id}"))
}

/// The apps' folder whose record the panes go by, unless a test named
/// another ([`set_root`]).
static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Names the apps' folder whose record and panes' folders Flow's tabs use,
/// in place of the user's own, as a test does.
pub fn set_root(root: PathBuf) {
    let _ = ROOT.set(root);
}

/// The apps' folder the panes go by.
pub fn root() -> PathBuf {
    ROOT.get().cloned().unwrap_or_else(super::dir)
}

/// A pane's folder, taken away when it's dropped: as its pane closes, or
/// its shell fails to start.
#[derive(Debug)]
pub struct PaneFolder(PathBuf);

impl PaneFolder {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PaneFolder {
    fn drop(&mut self) {
        close_pane(&self.0);
    }
}

/// Opens the folder of this Flow's pane whose session is `id`, as its shell
/// (`profile`) starts: the `FLOW_FIRST` it's given. None where it can't be
/// made.
pub fn open_pane(id: u32, profile: &ShellProfile) -> Option<PaneFolder> {
    open_pane_in(&root(), std::process::id(), id, shell_name(profile.kind, profile.flavor)).map(PaneFolder)
}

/// Opens the folder under `root` of the pane `id` of the Flow `pid`, for
/// `shell`, from the record. The first opened under a `root` also brings an
/// older Flow's choices into the record ([`migrate`]) and, on a thread of
/// its own, sweeps away the folders of Flows that are gone.
pub fn open_pane_in(root: &Path, pid: u32, id: u32, shell: &str) -> Option<PathBuf> {
    static SEEN: parking_lot::Mutex<Vec<PathBuf>> = parking_lot::const_mutex(Vec::new());
    {
        let mut seen = SEEN.lock();
        if !seen.iter().any(|r| r == root) {
            seen.push(root.to_path_buf());
            migrate(root);
            let swept = root.to_path_buf();
            let _ = std::thread::Builder::new().name("flow-first".into()).spawn(move || sweep(&swept));
        }
    }
    let dir = pane_dir(root, pid, id);
    // Afresh: a folder of that name is another session's, gone.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join(SHELL_FILE), shell).ok()?;
    build(root, &dir, &Record::load(root), &Installed::load(root));
    Some(dir)
}

/// Takes a pane's folder away as its pane closes. A program from it still
/// running on Windows keeps its link there, which a later start sweeps.
pub fn close_pane(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// The panes' folders under `root`, each with the shell it's for.
pub fn panes(root: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(panes_dir(root)) else { return Vec::new() };
    let mut panes: Vec<(PathBuf, String)> = entries.flatten().filter_map(|e| Some((e.path(), pane_shell(&e.path())?))).collect();
    panes.sort();
    panes
}

/// The shell a pane's folder is for, where it's a pane's folder.
pub fn pane_shell(pane: &Path) -> Option<String> {
    std::fs::read_to_string(pane.join(SHELL_FILE)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Writes every pane's folder under `root` afresh from the record, its own
/// choices kept, so open tabs follow at their next prompt.
pub fn rewrite_panes(root: &Path) {
    let (record, installed) = (Record::load(root), Installed::load(root));
    for (pane, _) in panes(root) {
        build(root, &pane, &record, &installed);
    }
}

/// Writes one pane's folder afresh, as after its own choices changed.
pub fn rewrite_pane(root: &Path, pane: &Path) {
    build(root, pane, &Record::load(root), &Installed::load(root));
}

/// Makes `pane`, a pane's folder, what `record` and its own choices put
/// first in its shell: links where there were others, what's no longer
/// first taken out, and `.take` written where it changed. Never makes the
/// folder itself, which a pane closing meanwhile takes away.
fn build(root: &Path, pane: &Path, record: &Record, installed: &Installed) {
    let Some(shell) = pane_shell(pane) else { return };
    let here = Here::load(pane);
    let (mut path, mut prompt, mut take) = (BTreeMap::new(), BTreeMap::new(), Vec::new());
    for (name, choice) in in_pane(record, &here, &shell) {
        let Some(at) = target(root, installed, &choice) else { continue };
        match choice.how {
            How::Path => {
                take.extend(choice.takes_in(&shell).into_iter().map(|t| format!("{} {name}", t.word())));
                path.insert(exe(&name), at);
            }
            How::Prompt => {
                prompt.insert(exe(&name), at);
            }
        }
    }
    take.sort();
    link_all(root, pane, &path);
    let prompts = pane.join(PROMPT_DIR);
    if prompt.is_empty() {
        let _ = std::fs::remove_dir_all(&prompts);
    } else if std::fs::create_dir_all(&prompts).is_ok() {
        link_all(root, &prompts, &prompt);
    }
    let text: String = take.iter().map(|line| format!("{line}\n")).collect();
    if std::fs::read_to_string(pane.join(TAKE_FILE)).ok().as_deref() != Some(&text) {
        let new = pane.join(format!("{TAKE_FILE}.new"));
        if std::fs::write(&new, &text).is_ok() {
            let _ = std::fs::rename(&new, pane.join(TAKE_FILE));
        }
    }
}

/// Makes `dir` hold a link by each of `want`'s names to its program (a path
/// under `root`), and nothing else but its dotfiles. On Unix a link by
/// name, relative, so the apps' folder can move; on Windows a hard link (a
/// copy across drives), put in place by renaming, the one it takes the
/// place of put aside first, as Windows won't rename over a program that's
/// running.
fn link_all(root: &Path, dir: &Path, want: &BTreeMap<String, PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') && !want.contains_key(&name) && std::fs::remove_file(entry.path()).is_err() {
                let _ = std::fs::rename(entry.path(), dir.join(format!(".{name}.old")));
            }
        }
    }
    for (file, at) in want {
        let to = dir.join(file);
        let new = dir.join(format!(".{file}.new"));
        let _ = std::fs::remove_file(&new);
        #[cfg(unix)]
        let linked = {
            let depth = dir.strip_prefix(root).map(|p| p.components().count()).unwrap_or(0);
            let from = Path::new(&"../".repeat(depth)).join(at);
            if std::fs::read_link(&to).is_ok_and(|was| was == from) {
                continue;
            }
            std::os::unix::fs::symlink(&from, &new)
        };
        #[cfg(not(unix))]
        let linked = {
            let from = root.join(at);
            let same = |a: &Path, b: &Path| matches!((std::fs::metadata(a), std::fs::metadata(b)), (Ok(a), Ok(b)) if a.len() == b.len() && a.modified().ok() == b.modified().ok());
            if same(&to, &from) {
                continue;
            }
            if to.exists() {
                let aside = dir.join(format!(".{file}.old"));
                let _ = std::fs::remove_file(&aside);
                let _ = std::fs::rename(&to, &aside);
            }
            std::fs::hard_link(&from, &new).or_else(|_| std::fs::copy(&from, &new).map(|_| ()))
        };
        if linked.is_ok() {
            let _ = std::fs::rename(&new, &to);
        }
    }
    super::sweep_aside(dir);
}

/// Brings the choices of a Flow before 0.1.35 into the record, once: its
/// `first/` held a link of each program put first, which every tab had
/// first on its PATH, so each is first in every shell, as it was. The links
/// stay, for an older Flow still running meanwhile, until `flowapp` next
/// changes what's first ([`forget_old_links`]).
pub fn migrate(root: &Path) {
    if Record::path(root).exists() {
        return;
    }
    let old = root.join("first");
    let Ok(entries) = std::fs::read_dir(&old) else { return };
    let mut files: Vec<String> = entries.flatten().filter(|e| e.file_type().is_ok_and(|t| !t.is_dir())).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| !n.starts_with('.')).collect();
    if files.is_empty() {
        return;
    }
    files.sort();
    let suffix = std::env::consts::EXE_SUFFIX;
    let firsts = files.iter().map(|f| Choice::new(f.strip_suffix(suffix).filter(|_| !suffix.is_empty()).unwrap_or(f.as_str()))).collect();
    let _ = (Record { firsts }).save(root);
}

/// Takes away the links a Flow before 0.1.35 kept in `first/`, once the
/// record holds them.
pub fn forget_old_links(root: &Path) {
    if !Record::path(root).exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root.join("first")) else { return };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| !t.is_dir()) && !entry.file_name().to_string_lossy().starts_with('.') {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Takes away the panes' folders of Flows no longer running, which a Flow
/// that quit without closing its tabs, or crashed, left.
pub fn sweep(root: &Path) {
    sweep_with(root, alive);
}

fn sweep_with(root: &Path, alive: impl Fn(u32) -> bool) {
    let own = std::process::id();
    let Ok(entries) = std::fs::read_dir(panes_dir(root)) else { return };
    let mut seen: BTreeMap<u32, bool> = BTreeMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(pid) = name.split('-').next().and_then(|p| p.parse::<u32>().ok()) else { continue };
        if pid != own && !*seen.entry(pid).or_insert_with(|| alive(pid)) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Whether the process `pid` is running; where that can't be told, it
/// counts as running, so a live Flow's folders are never taken away.
#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(all(unix, not(target_os = "linux")))]
fn alive(pid: u32) -> bool {
    let quiet = std::process::Stdio::null;
    std::process::Command::new("kill").args(["-0", &pid.to_string()]).stdout(quiet()).stderr(quiet()).status().map_or(true, |s| s.success())
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]).creation_flags(CREATE_NO_WINDOW).output();
    out.map_or(true, |o| !o.status.success() || String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
}

/// What stands ahead of the PATH in a shell, by the tables Flow keeps of
/// each shell as it comes (what the user's own files add, Flow can't see).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ahead {
    /// A builtin of bash's or zsh's, which `--take builtin` puts aside.
    Builtin,
    /// zsh's own `which`, which hands a plain `which` of programs to the
    /// one first, and keeps the rest.
    Which,
    /// One of PowerShell's own aliases, of what it names, which `--take
    /// alias` puts aside.
    Alias(&'static str),
    /// One of PowerShell's own functions, which `--take function` puts aside.
    Function,
    /// One of BusyBox's applets, which `--take applet` puts aside.
    Applet,
    /// A command of the shell's own that no program can take the place of:
    /// cmd's built-in commands, BusyBox's shell's builtins.
    Own,
}

/// What still stands ahead of `name`, first in `shell` in the way `how`,
/// having taken away `takes`; `applets` are BusyBox's. First at the prompt
/// alone, nothing: the alias the hook makes comes ahead of all of it.
pub fn ahead(shell: &str, name: &str, how: How, takes: &[Take], applets: &[String]) -> Vec<Ahead> {
    if how == How::Prompt && has_prompt_hook(shell) {
        return Vec::new();
    }
    let took = |t: Take| takes.contains(&t);
    let mut out = Vec::new();
    match shell {
        "zsh" if name == "which" && !took(Take::Builtin) => out.push(Ahead::Which),
        "zsh" | "bash" if is_builtin(shell, name) && !took(Take::Builtin) => out.push(Ahead::Builtin),
        "powershell" => {
            if let Some((_, to)) = PS_ALIASES.iter().find(|(a, _)| a.eq_ignore_ascii_case(name)).filter(|_| !took(Take::Alias)) {
                out.push(Ahead::Alias(to));
            }
            if PS_FUNCTIONS.iter().any(|f| f.eq_ignore_ascii_case(name)) && !took(Take::Function) {
                out.push(Ahead::Function);
            }
        }
        "busybox" if ASH_BUILTINS.contains(&name) => out.push(Ahead::Own),
        "busybox" if applets.iter().any(|a| a == name) && !took(Take::Applet) => out.push(Ahead::Applet),
        "cmd" if CMD_OWN.iter().any(|c| c.eq_ignore_ascii_case(name)) => out.push(Ahead::Own),
        _ => {}
    }
    out
}

/// Whether `name` is a builtin of `shell` (bash's or zsh's).
pub fn is_builtin(shell: &str, name: &str) -> bool {
    match shell {
        "zsh" => ZSH_BUILTINS.contains(&name),
        "bash" => BASH_BUILTINS.contains(&name),
        _ => false,
    }
}

/// The builtins a program can stand in for, which alone `--take builtin`
/// puts aside: the rest work on the shell's own state (`cd`, `export`,
/// `read`), which no program can, and Flow's hooks need them.
pub const STANDS_IN: [&str; 11] = ["[", "echo", "false", "help", "kill", "log", "printf", "pwd", "test", "true", "which"];

/// BusyBox's applets, as Flow's BusyBox for Windows has them, where `busybox
/// --list` can't be asked.
pub fn known_applets() -> Vec<String> {
    BUSYBOX_APPLETS.iter().map(|a| a.to_string()).collect()
}

/// zsh 5.9's builtins.
const ZSH_BUILTINS: &[&str] = &[
    ".", ":", "[", "alias", "autoload", "bg", "bindkey", "break", "builtin", "bye", "cd", "chdir", "command", "compadd", "comparguments", "compcall", "compctl", "compdescribe", "compfiles", "compgroups", "compquote", "compset", "comptags", "comptry", "compvalues", "continue", "declare", "dirs", "disable", "disown", "echo", "echotc", "echoti", "emulate", "enable", "eval", "exec", "exit", "export", "false", "fc", "fg", "float", "functions", "getln", "getopts", "hash", "history", "integer", "jobs", "kill", "let", "limit", "local", "log", "logout", "noglob", "popd", "print", "printf", "private", "pushd", "pushln", "pwd", "r", "read", "readonly", "rehash", "return", "sched", "set", "setopt", "shift", "source", "suspend", "test", "times", "trap", "true", "ttyctl", "type", "typeset", "ulimit", "umask", "unalias", "unfunction", "unhash", "unlimit", "unset", "unsetopt", "vared", "wait", "whence", "where", "which", "zcompile", "zformat", "zle", "zmodload", "zparseopts", "zregexparse", "zstyle",
];

/// bash 5's builtins (3.2's are the same but `compopt`, `mapfile` and
/// `readarray`).
const BASH_BUILTINS: &[&str] = &[
    ".", ":", "[", "alias", "bg", "bind", "break", "builtin", "caller", "cd", "command", "compgen", "complete", "compopt", "continue", "declare", "dirs", "disown", "echo", "enable", "eval", "exec", "exit", "export", "false", "fc", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let", "local", "logout", "mapfile", "popd", "printf", "pushd", "pwd", "read", "readarray", "readonly", "return", "set", "shift", "shopt", "source", "suspend", "test", "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias", "unset", "wait",
];

/// BusyBox ash's builtins, which come ahead of its applets and the PATH,
/// and which ash can't put aside.
const ASH_BUILTINS: &[&str] = &[
    ".", ":", "[", "[[", "alias", "bg", "break", "cd", "chdir", "command", "continue", "echo", "eval", "exec", "exit", "export", "false", "fg", "getopts", "hash", "help", "history", "jobs", "kill", "let", "local", "printf", "pwd", "read", "readonly", "return", "set", "shift", "source", "test", "times", "trap", "true", "type", "ulimit", "umask", "unalias", "unset", "wait",
];

/// The applets of the BusyBox for Windows that comes with Flow.
const BUSYBOX_APPLETS: &[&str] = &[
    "[", "[[", "ar", "arch", "ascii", "ash", "awk", "base32", "base64", "basename", "bash", "bc", "bunzip2", "busybox", "bzcat", "bzip2", "cal", "cat", "chattr", "chmod", "cksum", "clear", "cmp", "comm", "cp", "cpio", "crc32", "cut", "date", "dc", "dd", "df", "diff", "dirname", "dos2unix", "dpkg", "dpkg-deb", "du", "echo", "ed", "egrep", "env", "expand", "expr", "factor", "false", "fgrep", "find", "fold", "free", "fsync", "ftpget", "ftpput", "getopt", "grep", "groups", "gunzip", "gzip", "hd", "head", "hexdump", "httpd", "iconv", "id", "inotifyd", "install", "ipcalc", "jn", "kill", "killall", "less", "link", "ln", "logname", "ls", "lsattr", "lzcat", "lzma", "lzop", "lzopcat", "make", "man", "md5sum", "mkdir", "mktemp", "mv", "nc", "nl", "nproc", "od", "paste", "patch", "pdpmake", "pgrep", "pidof", "pipe_progress", "pkill", "printenv", "printf", "ps", "pwd", "readlink", "realpath", "reset", "rev", "rm", "rmdir", "rpm", "rpm2cpio", "sed", "seq", "sh", "sha1sum", "sha256sum", "sha3sum", "sha512sum", "shred", "shuf", "sleep", "sort", "split", "ssl_client", "stat", "strings", "su", "sum", "sync", "tac", "tail", "tar", "tee", "test", "time", "timeout", "touch", "tr", "true", "truncate", "ts", "tsort", "ttysize", "uname", "unexpand", "uniq", "unix2dos", "unlink", "unlzma", "unlzop", "unxz", "unzip", "uptime", "usleep", "uudecode", "uuencode", "vi", "watch", "wc", "wget", "which", "whoami", "whois", "xargs", "xxd", "xz", "xzcat", "yes", "zcat",
];

/// PowerShell's own aliases of names a program might have, and what each
/// stands for (Windows PowerShell 5.1's; PowerShell 7 on Windows has them
/// but `curl` and `wget`).
const PS_ALIASES: &[(&str, &str)] = &[
    ("ac", "Add-Content"), ("cat", "Get-Content"), ("cd", "Set-Location"), ("chdir", "Set-Location"), ("clear", "Clear-Host"), ("cls", "Clear-Host"), ("copy", "Copy-Item"), ("cp", "Copy-Item"), ("curl", "Invoke-WebRequest"), ("del", "Remove-Item"), ("diff", "Compare-Object"), ("dir", "Get-ChildItem"), ("echo", "Write-Output"), ("erase", "Remove-Item"), ("fc", "Format-Custom"), ("fl", "Format-List"), ("foreach", "ForEach-Object"), ("ft", "Format-Table"), ("gc", "Get-Content"), ("gci", "Get-ChildItem"), ("gl", "Get-Location"), ("gm", "Get-Member"), ("gps", "Get-Process"), ("group", "Group-Object"), ("h", "Get-History"), ("history", "Get-History"), ("iex", "Invoke-Expression"), ("iwr", "Invoke-WebRequest"), ("kill", "Stop-Process"), ("lp", "Out-Printer"), ("ls", "Get-ChildItem"), ("man", "help"), ("md", "mkdir"), ("measure", "Measure-Object"), ("mount", "New-PSDrive"), ("move", "Move-Item"), ("mv", "Move-Item"), ("popd", "Pop-Location"), ("ps", "Get-Process"), ("pushd", "Push-Location"), ("pwd", "Get-Location"), ("r", "Invoke-History"), ("rd", "Remove-Item"), ("ren", "Rename-Item"), ("rm", "Remove-Item"), ("rmdir", "Remove-Item"), ("select", "Select-Object"), ("set", "Set-Variable"), ("sleep", "Start-Sleep"), ("sort", "Sort-Object"), ("start", "Start-Process"), ("tee", "Tee-Object"), ("type", "Get-Content"), ("wget", "Invoke-WebRequest"), ("where", "Where-Object"), ("write", "Write-Output"),
];

/// PowerShell's own functions of names a program might have.
const PS_FUNCTIONS: &[&str] = &["help", "mkdir", "more", "oss", "pause"];

/// cmd's built-in commands, which come ahead of every program and which
/// nothing can put aside.
const CMD_OWN: &[&str] = &[
    "assoc", "break", "call", "cd", "chdir", "cls", "color", "copy", "date", "del", "dir", "echo", "endlocal", "erase", "exit", "for", "ftype", "goto", "if", "md", "mkdir", "mklink", "move", "path", "pause", "popd", "prompt", "pushd", "rd", "rem", "ren", "rename", "rmdir", "set", "setlocal", "shift", "start", "time", "title", "type", "ver", "verify", "vol",
];

/// One line of what `flowapp first` lists: a name first in a shell, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub shell: String,
    /// The pane's own, as `--here` made it, in the tab `flowapp` runs in.
    pub here: bool,
    pub name: String,
    /// The app whose program it is.
    pub app: Option<String>,
    pub how: How,
    /// What it took away there.
    pub took: Vec<Take>,
    /// What still comes ahead of it there.
    pub ahead: Vec<Ahead>,
}

/// What's first in each of `shells`, as the record says, and, where
/// `flowapp` runs in a pane (`pane`, its folder and shell) whose own
/// choices change it, that pane's too.
pub fn rows(root: &Path, shells: &[&str], pane: Option<(&Path, &str)>, applets: &[String]) -> Vec<Row> {
    let record = Record::load(root);
    let installed = Installed::load(root);
    let row = |shell: &str, here: bool, choice: &Choice| {
        let took = choice.takes_in(shell);
        Row { shell: shell.to_string(), here, name: choice.program.clone(), app: whose(&installed, choice), how: choice.how, ahead: ahead(shell, &choice.program, choice.how, &took, applets), took }
    };
    let mut out = Vec::new();
    for shell in shells {
        out.extend(record.in_shell(shell).values().map(|c| row(shell, false, c)));
    }
    if let Some((pane, shell)) = pane {
        let here = Here::load(pane);
        if !here.is_empty() {
            out.extend(in_pane(&record, &here, shell).values().map(|c| row(shell, true, c)));
        }
    }
    out
}

/// The shells to say what's first in: this system's, and any an open pane
/// is in.
pub fn shells_to_show(root: &Path) -> Vec<String> {
    let mut shells: Vec<String> = system_shells().iter().map(|s| s.to_string()).collect();
    for (_, shell) in panes(root) {
        if !shells.contains(&shell) && SHELLS.contains(&shell.as_str()) {
            shells.push(shell);
        }
    }
    shells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Kept;

    fn tempdir() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("flow-first-unit-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn only(names: &[&str]) -> Shells {
        Shells::Only(names.iter().map(|n| n.to_string()).collect())
    }

    fn except(names: &[&str]) -> Shells {
        Shells::Except(names.iter().map(|n| n.to_string()).collect())
    }

    /// `programs` installed under `root` as `app` at `version`: in its
    /// version's `bin/`, and in `bin/` in place of another's of a name, as
    /// an install puts them.
    fn install(root: &Path, app: &str, version: &str, programs: &[&str]) {
        let mut installed = Installed::load(root);
        for dir in [root.join(app).join(version).join("bin"), root.join("bin")] {
            std::fs::create_dir_all(&dir).unwrap();
            for p in programs {
                std::fs::write(dir.join(exe(p)), format!("{app} {version} {p}")).unwrap();
            }
        }
        installed.apps.insert(app.into(), Kept { version: version.into(), programs: programs.iter().map(|p| p.to_string()).collect(), kind: String::new(), ..Kept::default() });
        installed.save(root).unwrap();
    }

    /// What a pane's link runs: the text of the stand-in it reaches.
    fn runs(pane: &Path, name: &str) -> Option<String> {
        std::fs::read_to_string(pane.join(exe(name))).ok()
    }

    /// The names in a folder but its dotfiles, without `.exe`.
    fn listed(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| !n.starts_with('.')).map(|n| n.trim_end_matches(".exe").to_string()).collect()).unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn shells_less_and_shared_name_what_they_should() {
        assert_eq!(Shells::All.without(&only(&["zsh"])), Some(except(&["zsh"])));
        assert_eq!(only(&["zsh", "bash"]).without(&only(&["zsh"])), Some(only(&["bash"])));
        assert_eq!(only(&["zsh"]).without(&only(&["zsh"])), None, "nothing left");
        assert_eq!(except(&["cmd"]).without(&only(&["zsh"])), Some(except(&["cmd", "zsh"])));
        assert_eq!(Shells::All.without(&except(&["powershell"])), Some(only(&["powershell"])));
        assert_eq!(except(&["cmd"]).without(&except(&["cmd", "zsh"])), Some(only(&["zsh"])));
        assert_eq!(only(&["zsh"]).without(&Shells::All), None);
        assert_eq!(Shells::All.meet(&only(&["bash"])), Some(only(&["bash"])));
        assert_eq!(only(&["zsh", "bash"]).meet(&except(&["zsh"])), Some(only(&["bash"])));
        assert_eq!(except(&["zsh"]).meet(&except(&["cmd"])), Some(except(&["zsh", "cmd"])));
        assert_eq!(only(&["zsh"]).meet(&only(&["bash"])), None);
        let every: Vec<&str> = SHELLS.to_vec();
        assert_eq!(Shells::All.without(&only(&every)), None, "all but every one is none");
        assert!(except(&["cmd"]).has("zsh") && !except(&["cmd"]).has("cmd") && only(&["cmd"]).has("cmd"));
    }

    #[test]
    fn a_later_choice_takes_an_earlier_ones_place_only_on_its_shells() {
        let mut record = Record::default();
        assert!(record.put(Choice::new("ls")).is_empty());
        let zsh = Choice { app: Some("other".into()), shells: only(&["zsh"]), take: Some(vec![Take::Alias]), ..Choice::new("ls") };
        let replaced = record.put(zsh.clone());
        assert_eq!(replaced, vec![Choice { shells: only(&["zsh"]), ..Choice::new("ls") }], "the earlier one, where the later takes its place");
        assert_eq!(record.firsts, vec![Choice { shells: except(&["zsh"]), ..Choice::new("ls") }, zsh.clone()]);
        assert_eq!(record.in_shell("zsh")["ls"], zsh);
        assert_eq!(record.in_shell("bash")["ls"].app, None);
        // Put last in bash alone, then everywhere.
        assert!(record.put_last("ls", &only(&["bash"])));
        assert!(!record.in_shell("bash").contains_key("ls") && record.in_shell("powershell").contains_key("ls") && record.in_shell("zsh").contains_key("ls"));
        assert!(!record.put_last("cat", &Shells::All), "it never was");
        assert!(record.put_last("ls", &Shells::All));
        assert!(record.firsts.is_empty());
    }

    #[test]
    fn the_record_reads_back_and_says_only_what_isnt_the_default() {
        let root = tempdir();
        let mut record = Record::default();
        record.put(Choice::new("ls"));
        record.put(Choice { app: Some("flow-utils".into()), shells: except(&["cmd"]), how: How::Prompt, take: Some(vec![]), ..Choice::new("tree") });
        record.save(&root).unwrap();
        assert_eq!(Record::load(&root), record);
        let text = std::fs::read_to_string(root.join("first.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["firsts"][0], serde_json::json!({ "program": "ls", "shells": "all" }));
        assert_eq!(json["firsts"][1], serde_json::json!({ "program": "tree", "app": "flow-utils", "shells": { "except": ["cmd"] }, "how": "prompt", "take": [] }));
        assert_eq!(Record::load(&root.join("nowhere")), Record::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn what_a_name_takes_away_is_powershells_alias_and_busyboxs_applet_unless_asked() {
        let ls = Choice::new("ls");
        assert_eq!(ls.takes_in("powershell"), [Take::Alias]);
        assert_eq!(ls.takes_in("busybox"), [Take::Applet]);
        assert!(ls.takes_in("zsh").is_empty() && ls.takes_in("bash").is_empty() && ls.takes_in("cmd").is_empty());
        let asked = Choice { take: Some(vec![Take::Builtin, Take::Alias, Take::Function, Take::Builtin]), ..Choice::new("echo") };
        assert_eq!(asked.takes_in("zsh"), [Take::Alias, Take::Builtin], "what zsh has of it");
        assert_eq!(asked.takes_in("powershell"), [Take::Alias, Take::Function]);
        assert_eq!(asked.takes_in("busybox"), [Take::Alias]);
        assert!(asked.takes_in("cmd").is_empty(), "cmd has nothing to take away");
        let none = Choice { take: Some(vec![]), ..Choice::new("ls") };
        assert!(none.takes_in("powershell").is_empty(), "--take none keeps PowerShell's alias");
        let prompt = Choice { how: How::Prompt, take: Some(vec![Take::Builtin]), ..Choice::new("echo") };
        assert!(prompt.takes_in("zsh").is_empty(), "at the prompt the alias comes ahead of everything");
        assert!(prompt.in_shell("zsh") && !prompt.in_shell("cmd") && !prompt.in_shell("sh"), "no hook at cmd's or sh's prompt");
    }

    #[test]
    fn a_panes_folder_holds_what_its_shell_has_first_and_follows_the_record() {
        let root = tempdir();
        install(&root, "flow-utils", "0.2.0", &["ls", "tree", "echo"]);
        install(&root, "other", "1.0.0", &["ls"]);
        let mut record = Record::default();
        record.put(Choice::new("ls"));
        record.put(Choice { shells: only(&["zsh", "powershell"]), how: How::Prompt, ..Choice::new("tree") });
        record.put(Choice { shells: only(&["zsh"]), take: Some(vec![Take::Builtin, Take::Alias]), ..Choice::new("echo") });
        record.save(&root).unwrap();
        // This process's, which the sweep a first pane sets off leaves.
        let pid = std::process::id();
        let zsh = open_pane_in(&root, pid, 7, "zsh").unwrap();
        let bash = open_pane_in(&root, pid, 8, "bash").unwrap();
        let cmd = open_pane_in(&root, pid, 9, "cmd").unwrap();
        let ps = open_pane_in(&root, pid, 10, "powershell").unwrap();
        assert_eq!(zsh, root.join(format!("first/panes/{pid}-7")));
        assert_eq!(listed(&zsh), ["echo", "ls"]);
        assert_eq!(listed(&zsh.join(".prompt")), ["tree"], "first at the prompt alone, apart");
        assert_eq!(std::fs::read_to_string(zsh.join(".take")).unwrap(), "alias echo\nbuiltin echo\n");
        assert_eq!(runs(&zsh, "ls").as_deref(), Some("other 1.0.0 ls"), "bin/'s, the later installed");
        assert_eq!(runs(&zsh.join(".prompt"), "tree").as_deref(), Some("flow-utils 0.2.0 tree"));
        assert_eq!(listed(&bash), ["ls"]);
        assert!(!bash.join(".prompt").exists() && std::fs::read_to_string(bash.join(".take")).unwrap().is_empty());
        assert_eq!(listed(&cmd), ["ls"], "no prompt hook in cmd");
        assert_eq!(std::fs::read_to_string(ps.join(".take")).unwrap(), "alias ls\n", "PowerShell's alias of it goes by default");
        assert_eq!(std::fs::read_to_string(ps.join(".shell")).unwrap(), "powershell");
        #[cfg(unix)]
        assert_eq!(std::fs::read_link(zsh.join("ls")).unwrap(), Path::new("../../../bin/ls"), "relative, so the folder can move");
        // `<app>/<program>`: that app's, from its version.
        record.put(Choice { app: Some("flow-utils".into()), ..Choice::new("ls") });
        record.save(&root).unwrap();
        rewrite_panes(&root);
        assert_eq!(runs(&bash, "ls").as_deref(), Some("flow-utils 0.2.0 ls"));
        // A pane's own: in it alone, and kept when the others are written afresh.
        let mut here = Here::load(&bash);
        here.put(Choice { shells: only(&["cmd"]), ..Choice::new("tree") });
        here.put_last("ls");
        here.save(&bash).unwrap();
        rewrite_pane(&root, &bash);
        assert_eq!(listed(&bash), ["tree"]);
        rewrite_panes(&root);
        assert_eq!(listed(&bash), ["tree"], "its own kept");
        assert_eq!(listed(&cmd), ["ls"]);
        // Put last everywhere: out of every folder.
        record.put_last("ls", &Shells::All);
        record.put_last("echo", &Shells::All);
        record.save(&root).unwrap();
        rewrite_panes(&root);
        assert!(listed(&zsh).is_empty() && listed(&cmd).is_empty());
        assert!(std::fs::read_to_string(zsh.join(".take")).unwrap().is_empty());
        assert_eq!(listed(&zsh.join(".prompt")), ["tree"]);
        // A program no longer installed isn't linked; its choice is forgotten.
        std::fs::remove_file(root.join("bin").join(exe("tree"))).unwrap();
        rewrite_panes(&root);
        assert!(!zsh.join(".prompt").exists() && listed(&bash).is_empty());
        let mut installed = Installed::load(&root);
        installed.apps.remove("flow-utils");
        assert!(record.forget_gone(&installed));
        assert!(record.firsts.is_empty(), "flow-utils' tree went with it");
        // Closed, the folder goes.
        close_pane(&zsh);
        assert!(!zsh.exists());
        assert_eq!(panes(&root).len(), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_stands_for_an_apps_programs_or_one_of_them() {
        let root = tempdir();
        install(&root, "flow-utils", "0.2.0", &["ls", "tree"]);
        install(&root, "other", "1.0.0", &["ls"]);
        let installed = Installed::load(&root);
        let mine = |p: &str| Choice { app: Some("flow-utils".into()), ..Choice::new(p) };
        assert_eq!(resolve(&installed, "flow-utils"), Ok(vec![mine("ls"), Choice::new("tree")]), "its own ls, as another app has one");
        assert_eq!(resolve(&installed, "tree"), Ok(vec![Choice::new("tree")]));
        assert_eq!(resolve(&installed, "ls"), Err(Unnamed::Shared(vec!["flow-utils".into(), "other".into()])));
        assert_eq!(resolve(&installed, "other/ls"), Ok(vec![Choice { app: Some("other".into()), ..Choice::new("ls") }]));
        assert_eq!(resolve(&installed, "other/tree"), Err(Unnamed::Nothing));
        assert_eq!(resolve(&installed, "cat"), Err(Unnamed::Nothing));
        assert_eq!(whose(&installed, &Choice::new("tree")).as_deref(), Some("flow-utils"));
        assert_eq!(whose(&installed, &Choice::new("ls")), None, "two have it, and bin/ says not whose");
        assert_eq!(whose(&installed, &mine("ls")).as_deref(), Some("flow-utils"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_older_flows_first_folder_comes_into_the_record() {
        let root = tempdir();
        install(&root, "flow-utils", "0.1.0", &["ls", "which"]);
        let old = root.join("first");
        std::fs::create_dir_all(&old).unwrap();
        for p in ["ls", "which"] {
            std::fs::copy(root.join("bin").join(exe(p)), old.join(exe(p))).unwrap();
        }
        std::fs::write(old.join(".ls.old"), "").unwrap();
        migrate(&root);
        assert_eq!(Record::load(&root).firsts, vec![Choice::new("ls"), Choice::new("which")], "first in every shell, as they were");
        assert!(old.join(exe("ls")).exists(), "kept for an older Flow still running");
        // Once: a record there already is left alone.
        Record::default().save(&root).unwrap();
        migrate(&root);
        assert!(Record::load(&root).firsts.is_empty());
        // flowapp takes the old links away, not the panes' folders.
        std::fs::create_dir_all(panes_dir(&root).join("1-1")).unwrap();
        forget_old_links(&root);
        assert!(!old.join(exe("ls")).exists() && !old.join(exe("which")).exists() && panes_dir(&root).join("1-1").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sweep_takes_only_the_folders_of_flows_that_are_gone() {
        let root = tempdir();
        let own = std::process::id();
        for (pid, id) in [(own, 1), (4_000_001, 1), (4_000_001, 2), (4_000_002, 1)] {
            std::fs::create_dir_all(pane_dir(&root, pid, id)).unwrap();
        }
        std::fs::create_dir_all(panes_dir(&root).join("not-a-pane")).unwrap();
        let asked = std::cell::RefCell::new(Vec::new());
        sweep_with(&root, |pid| {
            asked.borrow_mut().push(pid);
            pid == 4_000_002
        });
        // In the order the folder lists them, which is by name on the Mac's
        // APFS and by hash on ext4.
        let mut asked = asked.into_inner();
        asked.sort();
        assert_eq!(asked, [4_000_001, 4_000_002], "each Flow asked of once, never this one");
        let mut left: Vec<String> = std::fs::read_dir(panes_dir(&root)).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        let mut want = vec![format!("{own}-1"), "4000002-1".into(), "not-a-pane".into()];
        want.sort();
        assert_eq!(left, want);
        assert!(alive(own), "this process runs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn what_stands_ahead_is_what_each_shell_has_less_what_was_taken() {
        let applets = known_applets();
        let ahead = |shell: &str, name: &str, takes: &[Take]| super::ahead(shell, name, How::Path, takes, &applets);
        assert_eq!(ahead("zsh", "echo", &[]), [Ahead::Builtin]);
        assert!(ahead("zsh", "echo", &[Take::Builtin]).is_empty());
        assert_eq!(ahead("zsh", "which", &[]), [Ahead::Which], "zsh's which hands plain uses over");
        assert!(ahead("bash", "which", &[]).is_empty() && ahead("bash", "ls", &[]).is_empty());
        assert_eq!(ahead("bash", "printf", &[]), [Ahead::Builtin]);
        assert!(ahead("powershell", "ls", &Choice::new("ls").takes_in("powershell")).is_empty(), "its alias goes by default");
        assert_eq!(ahead("powershell", "ls", &[]), [Ahead::Alias("Get-ChildItem")]);
        assert_eq!(ahead("powershell", "mkdir", &[Take::Alias]), [Ahead::Function]);
        assert!(ahead("powershell", "mkdir", &[Take::Function]).is_empty());
        assert_eq!(ahead("busybox", "ls", &[]), [Ahead::Applet]);
        assert!(ahead("busybox", "ls", &[Take::Applet]).is_empty());
        assert_eq!(ahead("busybox", "echo", &[Take::Applet]), [Ahead::Own], "ash's own builtin");
        assert_eq!(ahead("cmd", "DIR", &[]), [Ahead::Own]);
        assert!(ahead("cmd", "tree", &[]).is_empty());
        assert!(super::ahead("zsh", "echo", How::Prompt, &[], &applets).is_empty(), "the prompt's alias comes ahead of all");
        assert_eq!(super::ahead("cmd", "dir", How::Prompt, &[], &applets), [Ahead::Own]);
        assert!(STANDS_IN.iter().all(|b| is_builtin("zsh", b) || is_builtin("bash", b)));
        assert!(!STANDS_IN.contains(&"cd"));
    }

    #[test]
    fn rows_say_each_shells_names_and_the_panes_own() {
        let root = tempdir();
        install(&root, "flow-utils", "0.2.0", &["ls", "echo"]);
        let mut record = Record::default();
        record.put(Choice { shells: except(&["cmd"]), ..Choice::new("ls") });
        record.put(Choice { shells: only(&["zsh"]), how: How::Prompt, ..Choice::new("echo") });
        record.save(&root).unwrap();
        let pane = open_pane_in(&root, std::process::id(), 3, "bash").unwrap();
        let mut here = Here::default();
        here.put(Choice::new("echo"));
        here.save(&pane).unwrap();
        let rows = rows(&root, &["zsh", "bash", "cmd", "powershell"], Some((&pane, "bash")), &known_applets());
        let said: Vec<(&str, bool, &str, How)> = rows.iter().map(|r| (r.shell.as_str(), r.here, r.name.as_str(), r.how)).collect();
        assert_eq!(said, [("zsh", false, "echo", How::Prompt), ("zsh", false, "ls", How::Path), ("bash", false, "ls", How::Path), ("powershell", false, "ls", How::Path), ("bash", true, "echo", How::Path), ("bash", true, "ls", How::Path)]);
        assert_eq!(rows[3].took, [Take::Alias]);
        assert_eq!(rows[1].app.as_deref(), Some("flow-utils"), "the app whose program it is");
        assert_eq!(rows[4].ahead, [Ahead::Builtin], "bash's echo still comes ahead");
        assert!(shells_to_show(&root).contains(&"bash".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
