//! The `flow` command (`docs/design/flow-command.md`): `flow <command>`
//! typed in one of Flow's tabs asks the Flow that tab belongs to for a new
//! tab like it (its shell, its theme, the folder `flow` ran in), where the
//! command is typed once the shell waits at its prompt. `flow --window`,
//! or `flow` with no tab to ask, starts a Flow of its own for it instead,
//! a guest beside the one running ([`Guest`]).
//!
//! The ask goes the way `flow-settings`' do (OSC 7732, down Windows' pipe),
//! as an `open-tab` request. Flow takes OSC 7732 from whatever writes to a
//! tab, a remote host's output and a file being `cat`ed among it, so each
//! pane has a secret ([`TAB_KEY`], [`new_key`]) that only programs started
//! in it see, and Flow opens a tab only for a request carrying the asking
//! pane's own, answering any other not at all ([`tab_act`]), since its
//! answer would be typed into the shell's input. So [`run`] says itself
//! what may be wrong where nothing answers.
//!
//! What's here is everything but the asking itself, which is the SDK's
//! (`flow_sdk::settings::ask`, which core takes only for its tests): the
//! programs hand it in ([`run`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::i18n::{t, tf};
use crate::shell::{BashFlavor, ShellProfile};

/// The variable holding a pane's own secret, which its shell and every
/// program under it inherit.
pub const TAB_KEY: &str = "FLOW_TAB_KEY";

/// The request's `do`, beside `flow_sdk::settings`' own.
pub const OPEN_TAB: &str = "open-tab";

/// Flow's own first argument for a Flow started for a command (a guest),
/// followed by [`CWD`]; as the user types it, `flow --window`.
pub const WINDOW: &str = "--window";

/// The folder a guest's tab starts in, among Flow's own arguments.
const CWD: &str = "--cwd";

/// How long Flow is given to answer: it answers once the tab is open,
/// which is at once.
pub const WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// What `flow`'s arguments ask for.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    Help,
    Version,
    Open(Open),
}

/// A tab to open, and the command to type in it (none: a tab at its
/// prompt).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Open {
    pub command: Vec<String>,
    /// A pane beside the asking one, in its tab, rather than a tab.
    pub split: bool,
    /// Opened without coming to the front, the keys left where they were.
    pub behind: bool,
    /// Closed once its command ends well.
    pub close: bool,
    /// A Flow of its own (`--window`), rather than a tab in this one.
    pub window: bool,
    /// Another shell or theme than the asking tab's, by id or name.
    pub shell: Option<String>,
    pub theme: Option<String>,
}

/// Reads `flow`'s arguments (after the program's name): its options, up to
/// the first word that isn't one, or `--`, then the command. An option it
/// doesn't know is an error, in words, before anything opens, and so is a
/// command holding a control character ([`untypable`]), before a tab or a
/// Flow of its own is asked for.
pub fn parse(args: &[String]) -> Result<Asked, String> {
    let mut open = Open::default();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let (option, given) = match word.split_once('=') {
            Some((option, value)) if option.starts_with("--") => (option, Some(value.to_string())),
            _ => (word.as_str(), None),
        };
        let switch = matches!(option, "--split" | "--behind" | "--close" | "--window" | "--help" | "--version");
        if given.is_some() && switch {
            return Err(tf("flow_command.unknown_option", &[("option", &written_out(word))]));
        }
        match option {
            "--" => {
                open.command.extend(words.cloned());
                break;
            }
            "--help" | "-h" => return Ok(Asked::Help),
            "--version" | "-V" => return Ok(Asked::Version),
            "--split" => open.split = true,
            "--behind" => open.behind = true,
            "--close" => open.close = true,
            "--window" => open.window = true,
            "--shell" | "--theme" => {
                let value = given.or_else(|| words.next().cloned()).filter(|v| !v.is_empty());
                let value = value.ok_or_else(|| tf("flow_command.needs_value", &[("option", &written_out(option))]))?;
                match option {
                    "--shell" => open.shell = Some(value),
                    _ => open.theme = Some(value),
                }
            }
            o if o.starts_with('-') && o.len() > 1 => return Err(tf("flow_command.unknown_option", &[("option", &written_out(o))])),
            _ => {
                open.command.push(word.clone());
                open.command.extend(words.cloned());
                break;
            }
        }
    }
    crate::shell::typable(&open.command).map_err(untypable)?;
    Ok(Asked::Open(open))
}

/// Why Flow won't type a command, in words: its word that holds a control
/// character, counted from 1, and that character's code
/// (`shell::Untypable`).
pub fn untypable(why: crate::shell::Untypable) -> String {
    tf("flow_command.control", &[("word", &(why.word + 1)), ("character", &why.code())])
}

/// `text`, from a request or `flow`'s arguments, as it's put in Flow's
/// words back: its control characters written out (`\u{1b}`), since those
/// words are printed in the asking tab, where an Esc of them would begin
/// an escape.
fn written_out(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { c.escape_default().to_string() } else { c.to_string() }).collect()
}

/// The request asking the tab's Flow for `open`, with the asking pane's
/// secret (`pass`) and the folder `flow` was run in.
pub fn request(open: &Open, pass: &str, cwd: Option<&Path>) -> Value {
    json!({
        "do": OPEN_TAB,
        "pass": pass,
        "command": open.command,
        "cwd": cwd.map(|c| c.to_string_lossy().into_owned()),
        "split": open.split,
        "behind": open.behind,
        "close": open.close,
        "shell": open.shell,
        "theme": open.theme,
    })
}

/// A new pane's secret: 128 bits from the system's random source, as hex.
/// Empty where there's none to be had, which no request matches.
pub fn new_key() -> String {
    random_secret().unwrap_or_else(|| {
        log::warn!("no random numbers for a tab's key: flow can't open tabs from it");
        String::new()
    })
}

/// 128 bits from the system's random source, as hex, if it has them: a
/// pane's key, and its nonce for the shell's hook (`shell::new_nonce`).
pub(crate) fn random_secret() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Whether `given` is the pane's own secret `own`, compared in time that
/// doesn't hang on where they differ; never for an empty one.
pub fn same_key(given: &str, own: &str) -> bool {
    !own.is_empty() && given.len() == own.len() && given.bytes().zip(own.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}

/// Whether a settings request (OSC 7732's base64 of JSON) is an `open-tab`,
/// and if so what it asks.
pub fn open_tab_asked(request: &str) -> Option<Value> {
    read_request(request).filter(|asked| asked["do"] == OPEN_TAB)
}

/// What a settings request (OSC 7732's base64 of JSON) asks, where it
/// reads: its base64 taken as `base64 -d` takes it, whatever spaces and
/// line breaks are in it (GNU's and BusyBox's `base64` break theirs every
/// 76 characters), since its pass is checked after.
pub fn read_request(request: &str) -> Option<Value> {
    use base64::Engine as _;
    let text: String = request.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(text).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The pane asking for a tab, as Flow has it.
#[derive(Debug, Clone, Default)]
pub struct Asker {
    /// Its own secret.
    pub key: String,
    /// Its shell, as an index into the profiles.
    pub profile: usize,
    /// The theme it wears (`Tab::look_of`).
    pub theme: (Option<String>, Option<Value>),
    /// The folder its shell is in, where Flow knows it.
    pub folder: Option<PathBuf>,
}

/// What Flow is to open for an `open-tab` request.
#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    pub profile: usize,
    pub theme: (Option<String>, Option<Value>),
    pub cwd: Option<PathBuf>,
    /// The command as it's typed at the new shell's prompt (`shell::quote_for`).
    pub typed: Option<String>,
    /// As it's typed where that shell hasn't shown the prompt Flow's hook
    /// draws, or bash's hook said it may take letters past ASCII as keys
    /// there (`Session::types_letters`): zsh's and bash's letters past
    /// ASCII as their bytes (`shell::quote_ascii_for`).
    pub typed_ascii: Option<String>,
    pub split: bool,
    pub behind: bool,
    pub close: bool,
}

/// What Flow does with `asked`, an `open-tab` from `asker`: the tab to open,
/// like the asker's (its shell, theme and folder) unless the request names
/// a shell or a theme (an id or a name, `themes` being each theme's id and
/// name), or why it won't, in words. A request without the asking pane's
/// own secret is given no answer at all (none), alike whether it carried
/// none or a wrong one: what printed it may be a file being `cat`ed, and
/// an answer is typed into the shell's input, where a line editor in vi
/// mode reads its letters as commands. A command holding a control character
/// ([`untypable`]) is refused, which `flow` refuses first but a program
/// asking for itself may not: its words may come from anywhere, a file's
/// name among them, and typed at the new prompt a Ctrl+C or a line break
/// in one would be a key, ending its quotes or its line, so that what came
/// after ran as a command of its own. The folder only starts the shell,
/// which nothing types.
pub fn tab_act(asked: &Value, asker: &Asker, profiles: &[ShellProfile], themes: &[(String, String)]) -> Option<Result<Opening, String>> {
    same_key(asked["pass"].as_str().unwrap_or_default(), &asker.key).then(|| opening_for(asked, asker, profiles, themes))
}

/// The tab to open for `asked`, an `open-tab` carrying `asker`'s own
/// secret ([`tab_act`]), or why not, in words.
fn opening_for(asked: &Value, asker: &Asker, profiles: &[ShellProfile], themes: &[(String, String)]) -> Result<Opening, String> {
    let command = match &asked["command"] {
        Value::Null => Vec::new(),
        Value::Array(words) => words.iter().map(|w| w.as_str().map(str::to_string)).collect::<Option<Vec<_>>>().ok_or_else(|| t("flow_command.unreadable").to_string())?,
        _ => return Err(t("flow_command.unreadable").to_string()),
    };
    let profile = match asked["shell"].as_str() {
        Some(name) => shell_named(profiles, name)?,
        None => asker.profile,
    };
    let theme = match asked["theme"].as_str() {
        Some(name) => (Some(theme_named(themes, name)?), None),
        None => asker.theme.clone(),
    };
    let shell = profiles.get(profile).ok_or_else(|| t("flow_command.unreadable").to_string())?;
    // WSL's folders are Linux's, which a Windows path isn't.
    let wsl = shell.flavor == BashFlavor::Wsl;
    let given = asked["cwd"].as_str().map(PathBuf::from).filter(|p| p.is_dir());
    let cwd = given.or_else(|| asker.folder.clone().filter(|p| p.is_dir())).filter(|_| !wsl);
    // A word holding a control character would be keys, not letters, at
    // the new shell's prompt, whatever its quotes.
    let (typed, typed_ascii) = match command.is_empty() {
        true => (None, None),
        false => (
            Some(crate::shell::quote_for(shell, &command).map_err(untypable)?),
            Some(crate::shell::quote_ascii_for(shell, &command).map_err(untypable)?),
        ),
    };
    let on = |key: &str| asked[key].as_bool().unwrap_or(false);
    Ok(Opening { profile, theme, cwd, typed, typed_ascii, split: on("split"), behind: on("behind"), close: on("close") })
}

/// The shell named `name`, by its id or its label, ignoring case; else
/// why not, naming those there are.
pub fn shell_named(profiles: &[ShellProfile], name: &str) -> Result<usize, String> {
    profiles.iter().position(|p| p.id.eq_ignore_ascii_case(name) || p.label.eq_ignore_ascii_case(name)).ok_or_else(|| {
        let shells: Vec<&str> = profiles.iter().map(|p| p.label.as_str()).collect();
        tf("flow_command.no_shell", &[("shell", &written_out(name)), ("shells", &shells.join(", "))])
    })
}

/// The theme named `name`, by its id or its name, ignoring case; else why
/// not, naming those there are.
pub fn theme_named(themes: &[(String, String)], name: &str) -> Result<String, String> {
    themes.iter().find(|(id, shown)| id.eq_ignore_ascii_case(name) || shown.eq_ignore_ascii_case(name)).map(|(id, _)| id.clone()).ok_or_else(|| {
        let ids: Vec<&str> = themes.iter().map(|(id, _)| id.as_str()).collect();
        tf("flow_command.no_theme", &[("theme", &written_out(name)), ("themes", &ids.join(", "))])
    })
}

/// A Flow started for a command (`flow --window`, or `flow` with no tab to
/// ask): one tab, the command typed in it, a guest beside the Flow already
/// running, which keeps the saved tabs, the hotkey, outside apps and
/// updates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Guest {
    pub command: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub shell: Option<String>,
    pub theme: Option<String>,
    pub close: bool,
}

/// Flow's own arguments for a guest started for `open`, in `cwd`:
/// `--window --cwd <dir> [--shell <id>] [--theme <id>] [--close] --
/// <command>`.
pub fn guest_args(open: &Open, cwd: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![WINDOW.into(), CWD.into(), cwd.into()];
    if let Some(shell) = &open.shell {
        args.extend(["--shell".into(), shell.into()]);
    }
    if let Some(theme) = &open.theme {
        args.extend(["--theme".into(), theme.into()]);
    }
    if open.close {
        args.push("--close".into());
    }
    args.push("--".into());
    args.extend(open.command.iter().map(OsString::from));
    args
}

/// The guest Flow's own arguments ask for (after the program's name), as
/// [`guest_args`] wrote them; none unless they begin `--window --cwd`, so
/// `flow --window` as typed is never one.
pub fn read_guest(args: &[OsString]) -> Option<Guest> {
    let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let [window, cwd, dir, rest @ ..] = args.as_slice() else { return None };
    if window != WINDOW || cwd != CWD {
        return None;
    }
    let mut guest = Guest { cwd: Some(PathBuf::from(dir)), ..Guest::default() };
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--shell" => guest.shell = words.next().cloned(),
            "--theme" => guest.theme = words.next().cloned(),
            "--close" => guest.close = true,
            "--" => {
                guest.command = words.cloned().collect();
                break;
            }
            _ => {}
        }
    }
    Some(guest)
}

/// What the `flow` program does with its arguments (after its name):
/// asks the tab's Flow through `ask` (`flow_sdk::settings::ask`), or
/// starts a Flow apart, printing what went wrong on stderr. Gives the exit
/// code: 0, 1 for an error, 2 where no Flow answered, as `flow-settings`
/// does. `flow_itself` is Linux's, where `flow` is Flow: a guest starts as
/// this program again.
pub fn run(args: &[String], ask: &dyn Fn(&str) -> std::io::Result<Option<String>>, flow_itself: bool) -> i32 {
    let open = match parse(args) {
        Ok(Asked::Help) => {
            println!("{}", t("flow_command.usage"));
            return 0;
        }
        Ok(Asked::Version) => {
            println!("{}", crate::version::answer(crate::version::say_updates()));
            return 0;
        }
        Ok(Asked::Open(open)) => open,
        Err(error) => {
            eprintln!("flow: {error}");
            return 1;
        }
    };
    let cwd = std::env::current_dir().ok();
    let pass = std::env::var(TAB_KEY).ok().filter(|k| !k.is_empty());
    let Some(pass) = pass.filter(|_| !open.window) else {
        // A Flow of its own, from the folder it was run in.
        let dir = cwd.or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("."));
        return match start_apart(&guest_args(&open, &dir), flow_itself) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("flow: {}", tf("flow_command.could_not_start", &[("error", &error)]));
                1
            }
        };
    };
    let request = request(&open, &pass, cwd.as_deref()).to_string();
    let answer = match ask(&request) {
        Ok(Some(answer)) => answer,
        Ok(None) => {
            eprintln!("flow: {}", t("flow_command.no_answer"));
            return 2;
        }
        Err(error) => {
            eprintln!("flow: {error}");
            return 2;
        }
    };
    let answer: Value = serde_json::from_str(&answer).unwrap_or_default();
    if answer["ok"] != Value::Bool(true) {
        eprintln!("flow: {}", answer["error"].as_str().unwrap_or_default());
        return 1;
    }
    0
}

/// On Linux, where `flow` is Flow itself: whether its arguments (after its
/// name) are the `flow` command's rather than Flow's own. Flow's own are
/// none outside a tab (the launcher's), `--snapshot`, `--record`,
/// `--carry-on` and a guest's ([`read_guest`]); anything else is the
/// command's, and so is no argument at all in one of Flow's tabs, which
/// opens a tab beside it.
pub fn is_command(args: &[OsString], in_tab: bool) -> bool {
    let first = args.first().map(|a| a.to_string_lossy().into_owned());
    match first.as_deref() {
        None => in_tab,
        Some("--snapshot" | "--record" | crate::CARRY_ON) => false,
        Some(_) => read_guest(args).is_none(),
    }
}

/// What a tab gives its programs that a Flow started from it mustn't take
/// for its own: its secret, its pipe, its prompt and its language. Its
/// `FLOW_GAMES` and `FLOW_FIRST` stay, since a Flow starting reads them to
/// take that tab's folders off the PATH it gives its own tabs
/// (`shell::path_with`), and they're its tabs' own then.
const TAB_ONLY: [&str; 5] = [TAB_KEY, crate::pipe::VAR, "FLOW_PROMPT", "FLOW_LANGUAGE", "FLOW_USER_ZDOTDIR"];

/// Takes what a tab gave its programs out of `cmd`'s environment, for a
/// Flow started from one: by `flow --window` ([`start_apart`]), or by
/// `flow-mcp` for an app running in a tab. On Linux `flow` with a tab's
/// key and no arguments is this command, not Flow, so a Flow started with
/// the key would only ask the tab's Flow for a tab, where there may be
/// none.
pub fn leave_the_tab(cmd: &mut std::process::Command) {
    let user = std::env::var_os("FLOW_USER_ZDOTDIR");
    for (var, value) in leaving_the_tab(user.as_deref(), dirs::home_dir().as_deref()) {
        match value {
            Some(value) => cmd.env(var, value),
            None => cmd.env_remove(var),
        };
    }
}

/// What [`leave_the_tab`] changes, each variable with its new value (none
/// taking it out): every one of [`TAB_ONLY`], and the user's own `ZDOTDIR`
/// back from `user_zdotdir` (`FLOW_USER_ZDOTDIR`). zsh's tabs keep Flow's
/// folder of its hooks as `ZDOTDIR`, so a Flow started from one would give
/// its zsh tabs no `.zshrc` of the user's; none where it's `home`, zsh's
/// own default.
fn leaving_the_tab(user_zdotdir: Option<&std::ffi::OsStr>, home: Option<&Path>) -> Vec<(&'static str, Option<OsString>)> {
    let mut changes: Vec<(&'static str, Option<OsString>)> = TAB_ONLY.iter().map(|&var| (var, None)).collect();
    if let Some(user) = user_zdotdir {
        let at_home = home.is_some_and(|home| home.as_os_str() == user);
        changes.push(("ZDOTDIR", (!at_home).then(|| user.to_os_string())));
    }
    changes
}

/// Starts a Flow apart with `args` (a guest's, [`guest_args`]), so `flow`
/// returns at once and the new Flow outlives the tab: on the Mac the
/// bundle `flow` is in, through `open -n`; on Windows `flow.exe` above
/// `games\`, detached; on Linux this program again (`flow_itself`), in a
/// session of its own. Run from a build folder, the `flow` beside it.
pub fn start_apart(args: &[OsString], flow_itself: bool) -> std::io::Result<()> {
    let me = std::env::current_exe()?;
    let here = me.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut cmd = if cfg!(target_os = "macos") && here.ends_with("Contents/Helpers") {
        // Without -n, Launch Services hands it to the Flow already running.
        let bundle = here.parent().and_then(Path::parent).unwrap_or(&here);
        let mut open = std::process::Command::new("/usr/bin/open");
        open.arg("-n").arg("-a").arg(bundle).arg("--args").args(args);
        open
    } else {
        let flow = if flow_itself {
            me.clone()
        } else if cfg!(windows) {
            // games\ is beside flow.exe, which is above it.
            let up = if here.file_name().is_some_and(|n| n.eq_ignore_ascii_case("games")) { here.parent().unwrap_or(&here) } else { &here };
            up.join("flow.exe")
        } else {
            here.join("flow")
        };
        let mut flow = std::process::Command::new(flow);
        flow.args(args);
        flow
    };
    leave_the_tab(&mut cmd);
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    apart(&mut cmd);
    cmd.spawn().map(drop)
}

/// A process of its own, which the tab closing doesn't end: on Windows
/// detached from the console in a group of its own, elsewhere in a session
/// of its own.
#[cfg(windows)]
fn apart(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt as _;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(all(unix, not(target_os = "macos")))]
fn apart(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt as _;
    // SAFETY: setsid is async-signal-safe and touches no memory.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
}

/// The Mac: `open` starts it apart already, and a build folder's `flow`
/// goes in a group of its own.
#[cfg(target_os = "macos")]
fn apart(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt as _;
    cmd.process_group(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::ShellKind;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }

    fn opened(line: &str) -> Open {
        match parse(&words(line)) {
            Ok(Asked::Open(open)) => open,
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn the_options_come_before_the_command_and_the_rest_is_the_commands_own() {
        let open = opened("ssh -N -L 7445:127.0.0.1:7445 user@host");
        assert_eq!(open, Open { command: words("ssh -N -L 7445:127.0.0.1:7445 user@host"), ..Open::default() });
        let open = opened("--split --behind --close --shell zsh --theme=green ls --split");
        assert!(open.split && open.behind && open.close && !open.window);
        assert_eq!((open.shell.as_deref(), open.theme.as_deref()), (Some("zsh"), Some("green")));
        assert_eq!(open.command, ["ls", "--split"], "a word past the command is the command's");
        assert_eq!(opened("-- --odd-name x").command, ["--odd-name", "x"]);
        assert_eq!(opened("--window top"), Open { command: vec!["top".into()], window: true, ..Open::default() });
        assert_eq!(opened(""), Open::default(), "no command: a tab at its prompt");
        assert_eq!(parse(&words("--help ls")), Ok(Asked::Help));
        assert_eq!(parse(&words("--version")), Ok(Asked::Version));
    }

    #[test]
    fn an_option_flow_doesnt_know_or_one_missing_its_value_is_an_error_before_anything_opens() {
        assert!(parse(&words("--spilt ls")).unwrap_err().contains("--spilt"));
        assert!(parse(&words("-x ls")).unwrap_err().contains("-x"));
        assert!(parse(&words("--shell")).unwrap_err().contains("--shell"));
        assert!(parse(&words("--theme=")).is_err());
        assert!(parse(&words("--close=yes ls")).is_err(), "a switch takes no value");
    }

    #[test]
    fn a_request_carries_the_pass_the_command_and_the_folder() {
        let open = Open { command: words("echo hi"), close: true, theme: Some("green".into()), ..Open::default() };
        let asked = request(&open, "k", Some(Path::new("/tmp")));
        assert_eq!(asked["do"], OPEN_TAB);
        assert_eq!(asked["pass"], "k");
        assert_eq!(asked["command"], json!(["echo", "hi"]));
        assert_eq!((asked["cwd"].clone(), asked["close"].clone(), asked["split"].clone()), (json!("/tmp"), json!(true), json!(false)));
        assert_eq!((asked["theme"].clone(), asked["shell"].clone()), (json!("green"), Value::Null));
        // As OSC 7732's base64 carries it, it's an open-tab; a settings
        // request isn't.
        use base64::Engine as _;
        let b64 = |v: &Value| base64::engine::general_purpose::STANDARD.encode(v.to_string());
        assert_eq!(open_tab_asked(&b64(&asked)), Some(asked.clone()));
        assert_eq!(open_tab_asked(&b64(&json!({ "do": "list" }))), None);
        assert_eq!(open_tab_asked("not base64!"), None);
        // Broken every 76 characters, as GNU's `base64` writes it.
        let whole = b64(&asked);
        let wrapped = whole.as_bytes().chunks(76).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("\n");
        assert!(wrapped.contains('\n'));
        assert_eq!(open_tab_asked(&format!("{wrapped}\n")), Some(asked));
    }

    #[test]
    fn a_panes_key_is_128_random_bits_and_matches_only_itself() {
        let (a, b) = (new_key(), new_key());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(same_key(&a, &a));
        assert!(!same_key(&b, &a), "another pane's");
        assert!(!same_key("", &a), "none");
        assert!(!same_key(&a[..31], &a), "part of it");
        assert!(!same_key("", ""), "an empty key matches nothing");
    }

    fn profiles() -> Vec<ShellProfile> {
        let p = |id: &'static str, label: &str, kind, flavor| ShellProfile { id, label: label.into(), kind, program: PathBuf::from(id), flavor };
        vec![p("zsh", "zsh", ShellKind::Zsh, BashFlavor::Native), p("bash", "Bash", ShellKind::Bash, BashFlavor::Native), p("wsl", "Ubuntu", ShellKind::Bash, BashFlavor::Wsl)]
    }

    fn themes() -> Vec<(String, String)> {
        vec![("flow".into(), "Flow".into()), ("green".into(), "Green screen".into())]
    }

    /// What Flow does with `asked` from `asker`, which carries its key, so
    /// it's answered.
    fn answered(asked: &Value, asker: &Asker, profiles: &[ShellProfile], themes: &[(String, String)]) -> Result<Opening, String> {
        tab_act(asked, asker, profiles, themes).expect("the pane's own key, answered")
    }

    #[test]
    fn a_tab_is_opened_only_for_the_asking_panes_own_key_and_any_other_ask_goes_unanswered() {
        let asker = Asker { key: new_key(), profile: 0, theme: (None, None), folder: None };
        let other = new_key();
        // Printed by a file being cat'ed or a remote host: no answer at all,
        // not even a refusal typed back into the shell's input.
        for pass in [Value::Null, json!(""), json!(other), json!("wrong"), json!(1)] {
            let asked = json!({ "do": OPEN_TAB, "pass": pass, "command": ["ls"] });
            assert_eq!(tab_act(&asked, &asker, &profiles(), &themes()), None, "{pass}: alike for none and a wrong one");
            let asked = json!({ "do": OPEN_TAB, "pass": pass, "shell": "fish", "command": [1] });
            assert_eq!(tab_act(&asked, &asker, &profiles(), &themes()), None, "{pass}: nothing said of what it asks");
        }
        let asked = json!({ "do": OPEN_TAB, "pass": asker.key, "command": ["ls"] });
        assert!(answered(&asked, &asker, &profiles(), &themes()).is_ok());
        // A pane whose secret is empty (no random numbers) answers nothing.
        let keyless = Asker { key: String::new(), ..asker };
        assert_eq!(tab_act(&json!({ "do": OPEN_TAB, "pass": "", "command": ["ls"] }), &keyless, &profiles(), &themes()), None);
    }

    #[test]
    fn the_new_tab_is_like_the_asking_one_unless_the_request_says_otherwise() {
        let here = std::env::temp_dir();
        let own = json!({ "fx": { "style": "rain" } });
        let asker = Asker { key: "k".repeat(32), profile: 1, theme: (Some("green".into()), Some(own.clone())), folder: Some(here.clone()) };
        let ask = |extra: Value| {
            let mut asked = json!({ "do": OPEN_TAB, "pass": asker.key, "command": ["ssh", "-N", "user@host"] });
            asked.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            answered(&asked, &asker, &profiles(), &themes())
        };
        let like = ask(json!({})).unwrap();
        assert_eq!(like, Opening { profile: 1, theme: (Some("green".into()), Some(own)), cwd: Some(here.clone()), typed: Some("ssh -N user@host".into()), typed_ascii: Some("ssh -N user@host".into()), split: false, behind: false, close: false });
        // The folder flow ran in, where it's one; else the asker's.
        let root = PathBuf::from("/");
        assert_eq!(ask(json!({ "cwd": root })).unwrap().cwd, Some(root));
        assert_eq!(ask(json!({ "cwd": "/no/such/folder" })).unwrap().cwd, Some(here.clone()));
        // Another shell and theme by id or name, any case; its quoting the
        // new shell's.
        let other = ask(json!({ "shell": "ZSH", "theme": "green screen", "split": true, "behind": true, "close": true })).unwrap();
        assert_eq!((other.profile, other.theme, other.split, other.behind, other.close), (0, (Some("green".into()), None), true, true, true));
        assert_eq!(ask(json!({ "shell": "ubuntu" })).unwrap().cwd, None, "WSL's folders are Linux's");
        let no_shell = ask(json!({ "shell": "fish" })).unwrap_err();
        assert!(no_shell.contains("fish") && no_shell.contains("zsh, Bash, Ubuntu"), "{no_shell}");
        let no_theme = ask(json!({ "theme": "amber" })).unwrap_err();
        assert!(no_theme.contains("amber") && no_theme.contains("flow, green"), "{no_theme}");
        // No command: a tab at its prompt.
        assert_eq!(answered(&json!({ "do": OPEN_TAB, "pass": asker.key }), &asker, &profiles(), &themes()).unwrap().typed, None);
        assert!(answered(&json!({ "do": OPEN_TAB, "pass": asker.key, "command": [1] }), &asker, &profiles(), &themes()).is_err());
    }

    /// Every one of C0's, DEL and C1's.
    fn controls() -> Vec<char> {
        (0u32..0x20).chain(0x7f..0xa0).filter_map(char::from_u32).collect()
    }

    #[test]
    fn a_command_holding_a_control_character_is_refused_before_anything_is_asked() {
        for c in controls() {
            let command = vec!["vim".to_string(), format!("notes{c}.txt")];
            let refused = parse(&command).unwrap_err();
            assert_eq!(refused, untypable(crate::shell::Untypable { word: 1, character: c }), "{c:?}");
            assert!(refused.contains(&format!("U+{:04X}", u32::from(c))), "{refused}");
            assert!(!refused.chars().any(char::is_control), "{c:?}: the character itself isn't printed back");
            // Nor is a Flow of its own started for it, after -- or not.
            for options in [&["--window"][..], &["--window", "--"], &["--split", "--close", "--"]] {
                let args: Vec<String> = options.iter().map(|o| o.to_string()).chain(command.iter().cloned()).collect();
                assert_eq!(parse(&args), Err(refused.clone()), "{args:?}");
            }
        }
        assert_eq!(parse(&["--help".to_string(), "a\nb".to_string()]), Ok(Asked::Help), "help is no command");
        assert!(parse(&words("printf <%s> ünï 日本")).is_ok());
        // The word is counted from 1, as the user counts them.
        let fifth = untypable(crate::shell::Untypable { word: 4, character: '\u{1b}' });
        assert!(fifth.contains('5') && fifth.contains("U+001B"), "{fifth}");
    }

    #[test]
    fn a_request_whose_command_holds_a_control_character_opens_nothing_whatever_the_shell() {
        let p = |id: &'static str, kind, program: &str| ShellProfile { id, label: id.into(), kind, program: PathBuf::from(program), flavor: BashFlavor::Native };
        let shells = vec![
            p("zsh", ShellKind::Zsh, "/bin/zsh"),
            p("bash", ShellKind::Bash, "/bin/bash"),
            p("sh", ShellKind::Sh, "/bin/sh"),
            p("pwsh", ShellKind::PowerShell, r"C:\Program Files\PowerShell\7\pwsh.exe"),
            p("powershell", ShellKind::PowerShell, r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            p("cmd", ShellKind::Cmd, r"C:\Windows\System32\cmd.exe"),
        ];
        let asker = Asker { key: new_key(), ..Asker::default() };
        for c in controls() {
            for shell in ["zsh", "bash", "sh", "pwsh", "powershell", "cmd"] {
                // A file's name, say, whose Ctrl+C or line break would end
                // its quotes or its line and run what came after it.
                let asked = json!({ "do": OPEN_TAB, "pass": asker.key, "shell": shell, "command": ["vim", format!("x{c}touch pwned #")] });
                let refused = answered(&asked, &asker, &shells, &themes());
                assert_eq!(refused, Err(untypable(crate::shell::Untypable { word: 1, character: c })), "{shell}: {c:?}");
            }
        }
        // Without the pane's key it isn't answered at all, so nothing of
        // the command is said.
        let asked = json!({ "do": OPEN_TAB, "pass": "wrong", "command": ["\u{3}"] });
        assert_eq!(tab_act(&asked, &asker, &shells, &themes()), None);
        // A tab at its prompt, and a command of letters, still open.
        assert!(answered(&json!({ "do": OPEN_TAB, "pass": asker.key, "command": [] }), &asker, &shells, &themes()).is_ok_and(|o| o.typed.is_none()));
        let asked = json!({ "do": OPEN_TAB, "pass": asker.key, "command": ["echo", "ünï a\u{a0}b"] });
        assert_eq!(answered(&asked, &asker, &shells, &themes()).unwrap().typed.as_deref(), Some("echo 'ünï a\u{a0}b'"));
        // Into bash too, whose readline Flow's hook has take them as
        // letters, not keys (Ē's second byte would be M-C-r, revert-line, in
        // a C locale); as their bytes into a shell that hasn't shown the
        // hook's prompt, which may be a bash the hook never reached.
        let asked = json!({ "do": OPEN_TAB, "pass": asker.key, "shell": "bash", "command": ["vim", "Ētouch pwned;#.txt", "日本語.txt"] });
        let opening = answered(&asked, &asker, &shells, &themes()).unwrap();
        assert_eq!(opening.typed.as_deref(), Some("vim 'Ētouch pwned;#.txt' '日本語.txt'"));
        assert_eq!(opening.typed_ascii.as_deref(), Some(r"vim $'\xc4\x92''touch pwned;#.txt' $'\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e''.txt'"));
    }

    #[test]
    fn a_name_or_an_option_flow_doesnt_know_comes_back_with_its_control_characters_written_out() {
        let printed = |words: &str, written: &str| assert!(!words.chars().any(char::is_control) && words.contains(written), "{words}");
        printed(&shell_named(&profiles(), "zsh\u{1b}]0;x\u{7}").unwrap_err(), r"zsh\u{1b}]0;x\u{7}");
        printed(&theme_named(&themes(), "\u{9b}2J\n").unwrap_err(), r"\u{9b}2J\n");
        printed(&parse(&["--\u{1b}[2J".to_string()]).unwrap_err(), r"--\u{1b}[2J");
        printed(&parse(&["--close=\u{1b}".to_string()]).unwrap_err(), r"--close=\u{1b}");
        // As the request carries them.
        let asker = Asker { key: new_key(), ..Asker::default() };
        let asked = json!({ "do": OPEN_TAB, "pass": asker.key, "theme": "\u{1b}[31m", "command": ["ls"] });
        printed(&answered(&asked, &asker, &profiles(), &themes()).unwrap_err(), r"\u{1b}[31m");
    }

    #[test]
    fn a_guest_flow_reads_back_the_arguments_it_was_started_with() {
        let open = Open { command: words("ssh -N host"), close: true, shell: Some("bash".into()), theme: Some("green".into()), window: true, ..Open::default() };
        let args = guest_args(&open, Path::new("/work"));
        let guest = read_guest(&args).unwrap();
        assert_eq!(guest, Guest { command: words("ssh -N host"), cwd: Some(PathBuf::from("/work")), shell: Some("bash".into()), theme: Some("green".into()), close: true });
        // A command with Flow's own words in it stays the command's.
        let odd = Open { command: words("echo --close --shell x"), ..Open::default() };
        assert_eq!(read_guest(&guest_args(&odd, Path::new("/"))).unwrap().command, words("echo --close --shell x"));
        let typed: Vec<OsString> = ["--window", "top"].map(OsString::from).to_vec();
        assert_eq!(read_guest(&typed), None, "flow --window as typed isn't a guest's");
    }

    #[test]
    fn on_linux_flow_is_the_command_unless_its_arguments_are_flows_own() {
        let args = |line: &str| line.split_whitespace().map(OsString::from).collect::<Vec<_>>();
        assert!(!is_command(&args(""), false), "the launcher's");
        assert!(is_command(&args(""), true), "a bare flow in a tab opens a tab");
        assert!(is_command(&args("ssh host"), false));
        assert!(is_command(&args("--window top"), true));
        assert!(is_command(&args("--help"), false));
        assert!(!is_command(&args("--snapshot dir"), true));
        assert!(!is_command(&args("--carry-on 4"), true));
        assert!(!is_command(&guest_args(&Open::default(), Path::new("/")), true));
    }

    #[test]
    fn a_flow_started_from_a_tab_leaves_the_tabs_variables_and_gets_the_users_zdotdir_back() {
        let home = Path::new("/home/me");
        let changed = |user: Option<&str>| leaving_the_tab(user.map(std::ffi::OsStr::new), Some(home));
        // Every one of the tab's own goes, its key first: on Linux a Flow
        // started with it would be the flow command.
        let bash = changed(None);
        assert_eq!(bash.iter().map(|(var, _)| *var).collect::<Vec<_>>(), TAB_ONLY);
        assert!(bash.iter().all(|(_, value)| value.is_none()));
        // zsh's: ZDOTDIR is Flow's folder of hooks there, so the user's comes
        // back, or none where it was the home folder.
        let own = changed(Some("/home/me/.config/zsh"));
        assert_eq!(own.last(), Some(&("ZDOTDIR", Some(OsString::from("/home/me/.config/zsh")))));
        assert_eq!(changed(Some("/home/me")).last(), Some(&("ZDOTDIR", None)));
    }
}
