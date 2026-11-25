//! `flowapp`: Flow's apps kept up to date from the shell, as Homebrew keeps
//! its formulae, on every tab's PATH as `flow-help` and `flow-settings` are.
//! `flowapp update` reads the catalog and says what's newer, `upgrade`
//! installs it, `install` and `remove` take an app on or off, and `list`,
//! `outdated`, `info` and `search` say what there is (`flow_core::apps`).
//! Those four go by the catalog `update` last read; `install` and `upgrade`
//! read it again first, as Homebrew does before it installs. Every download
//! is checked against the catalog's size and SHA-256 and Flow's own update
//! key before anything is put in place, and one that needs a newer Flow
//! than the one it runs in is held back, saying so, until Flow is updated.
//!
//! flowapp has a version of its own and is an app of the catalog too
//! (`flow_core::apps::itself`, docs/design/flowapp-updates.md): `upgrade`
//! installs a newer copy into the apps folder, which runs in Flow's tabs
//! ahead of the one Flow carries. Whichever copy runs first hands over to
//! the other where that one is newer, so a newer Flow's own flowapp wins
//! over an older one installed, and the Flow it runs in is the tab's.
//!
//! `list` and `search` say who signed each download, with which key and
//! when (`flow_core::apps::signing`), `info` how to check that with OpenSSL
//! alone and, on the Mac, who signed its programs, and `verify` fetches an
//! installed app's download again and looks at what's installed against it
//! (`flow_core::apps::verify`).
//!
//! In a terminal what it lists are tables fitted to the terminal's width
//! and a download is one line redrawn as it comes; piped, or with
//! `NO_COLOR`, they're plain lines (`flow_core::apps::table`, which lays
//! them out), and with `--json` JSON. In Flow the liquid splashes as an app
//! lands.
//!
//! `first` and `last` choose what comes ahead of the system's commands in
//! Flow's tabs (`flow_core::apps::first`): in which shells, on the PATH or
//! at the prompt alone, what of each shell's own it takes away, or in the
//! tab it runs in alone; each tab open follows at its next prompt.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use flow_core::apps::first::{self, Ahead, Choice, Here, How, Record, Shells, Take, Unnamed};
use flow_core::apps::itself;
use flow_core::apps::signing::{self, CodeGroup};
use flow_core::apps::table::{self, Fetch, Look, Shown};
use flow_core::apps::verify::{Against, Verdict};
use flow_core::apps::{self, App, Catalog, Installed, Kept, Read, Standing, Step};
use flow_core::i18n::{self, t, tf};

/// Wider than this, a table's rows get hard to follow, as `flow-help`'s
/// lines do.
const MOST_COLUMNS: usize = 100;

/// How often at most a download's line is drawn again.
const REDRAW: Duration = Duration::from_millis(100);

/// How a list of apps is printed: as a table laid out for where it goes,
/// or, with `--json`, as JSON.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Table(Look),
    Json,
}

fn main() -> ExitCode {
    if let Some(code) = hand_over() {
        return code;
    }
    let flow = itself::running_flow(std::env::var("TERM_PROGRAM").ok().as_deref(), std::env::var("TERM_PROGRAM_VERSION").ok().as_deref(), || itself::flow_dir().and_then(|dir| apps::version_of(&itself::flow_program(&dir))));
    if let Some(flow) = flow {
        itself::set_flow_version(flow);
    }
    let preferred: Vec<String> = ["FLOW_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).collect();
    i18n::set_language(i18n::AUTO, &preferred);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut words: Vec<&str> = args.iter().map(String::as_str).collect();
    // `--json`, anywhere after a command that lists apps, prints them as
    // JSON; after any other it's a mistake.
    let json = words.contains(&"--json");
    words.retain(|w| *w != "--json");
    if json && !matches!(words.first(), Some(&("update" | "list" | "outdated" | "info" | "search" | "first"))) {
        eprintln!("{}", t("flowapp.usage"));
        return ExitCode::from(2);
    }
    let root = apps::dir();
    // What a Flow before 0.1.35 put first, into the record of choices.
    first::migrate(&root);
    let look = look();
    let shape = if json { Shape::Json } else { Shape::Table(look) };
    let result = match words.as_slice() {
        ["update"] => update(&root, shape),
        ["upgrade", names @ ..] => upgrade(&root, names, look),
        ["install", names @ ..] if !names.is_empty() => install(&root, names, look),
        ["remove" | "uninstall", names @ ..] if !names.is_empty() => remove(&root, names),
        ["first"] => {
            say_firsts(&root, shape);
            Ok(())
        }
        ["first", words @ ..] if !json => put_first(&root, words),
        ["last", words @ ..] if !words.is_empty() && !json => put_last(&root, words),
        ["list"] => list(&root, shape),
        ["outdated"] => outdated(&root, shape),
        ["info", name] => info(&root, name, shape),
        ["search", words @ ..] => search(&root, words, shape),
        ["verify", names @ ..] if !json => verify(&root, names, look),
        ["hello", name] if !json => hello(&root, name),
        ["--version"] => {
            println!("flowapp {} (build {})", flow_core::version::FLOWAPP, flow_core::version::BUILD);
            Ok(())
        }
        [] | ["--help" | "-h" | "help"] => {
            println!("{}", t("flowapp.usage"));
            Ok(())
        }
        _ => {
            eprintln!("{}", t("flowapp.usage"));
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // An error may quote what anyone's file says (a theme's
            // fields), whose escapes the terminal mustn't be given.
            eprintln!("flowapp: {}", apps::plain(&error));
            ExitCode::from(1)
        }
    }
}

/// How to print, as `flow-help` decides: in a terminal, styled and fitted
/// to its width (a pty never sized says 0, taken as 80); with `NO_COLOR`
/// there, plain but still fitted; piped, plain and whole.
fn look() -> Look {
    let terminal = std::io::stdout().is_terminal();
    let cols = terminal.then(crossterm::terminal::size).and_then(Result::ok).map(|(w, _)| w as usize).filter(|w| *w > 0).unwrap_or(80);
    let styled = terminal && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
    Look { styled, width: terminal.then_some(cols.clamp(20, MOST_COLUMNS)) }
}

/// Writes `text` out whole; a pager that quits early closes the pipe, and
/// that's no error.
fn show(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

/// The folder of the programs Flow's release carries, where its own
/// `flowapp` is, which isn't this one's for a copy in the apps folder.
fn bundled_dir() -> Option<PathBuf> {
    itself::flow_dir()
}

/// Hands over to the other copy of flowapp where it's newer
/// ([`itself::other_copy`], [`itself::hand_to`]), with the same arguments,
/// giving back what it ends with; none where this one runs. On Unix the
/// other takes this one's place; on Windows this one waits for it.
fn hand_over() -> Option<ExitCode> {
    let handed = std::env::var_os(itself::HANDED).is_some();
    if handed {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let (other, version) = itself::other_copy(exe.parent()?, &apps::dir())?;
    if !itself::hand_to(flow_core::version::FLOWAPP, &version, handed) {
        return None;
    }
    let mut command = std::process::Command::new(&other);
    command.args(std::env::args_os().skip(1)).env(itself::HANDED, "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Only returns where the other won't start: then this one runs.
        let _ = command.exec();
        None
    }
    #[cfg(not(unix))]
    {
        let status = command.status().ok()?;
        Some(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
    }
}

/// The catalog `update` last read, or read now where it never has.
fn catalog(root: &Path) -> Result<Read, String> {
    apps::catalog_as_saved(root, &apps::catalog_place())
}

/// The catalog read again, before a download; where it won't read, the
/// one `update` last read, saying so.
fn catalog_afresh(root: &Path) -> Result<Catalog, String> {
    let read = apps::catalog_afresh(root, &apps::catalog_place())?;
    if let Some(error) = &read.failed {
        eprintln!("{}", table::offline(error, read.age));
    }
    Ok(read.catalog)
}

/// Under a table in a terminal, that the catalog it's from is old.
fn say_if_stale(read: &Read, shape: Shape) {
    let Shape::Table(look) = shape else { return };
    if let Some(line) = table::stale(read.age, look) {
        println!("{line}");
    }
}

/// Each of `names` as the tables show it: what the catalog says of it, and
/// where it stands here.
fn shown<'a>(root: &Path, catalog: &Catalog, names: impl IntoIterator<Item = &'a str>) -> Vec<Shown> {
    let installed = Installed::load(root);
    let bundled = bundled_dir();
    names
        .into_iter()
        .map(|name| {
            let app = catalog.get(name);
            let kept = installed.apps.get(name);
            let mut shown = Shown::new(name, app, apps::standing(&installed, name, app, bundled.as_deref())).noting(kept, app);
            shown.themes = catalog.is_themes(name) || kept.is_some_and(|kept| kept.themes());
            shown.skills = catalog.is_skills(name) || kept.is_some_and(|kept| kept.skills());
            shown
        })
        .collect()
}

/// Each app of `catalog` that's installed (or comes with Flow) and has a
/// newer version for this system.
fn newer(root: &Path, catalog: &Catalog) -> Vec<Shown> {
    shown(root, catalog, catalog.names()).into_iter().filter(Shown::newer).collect()
}

fn update(root: &Path, shape: Shape) -> Result<(), String> {
    let catalog = apps::read_catalog(root, &apps::catalog_place())?.catalog;
    if let Shape::Table(_) = shape {
        println!("{}", tf("flowapp.updated", &[("n", &catalog.names().count())]));
    }
    print_newer(&newer(root, &catalog), shape);
    Ok(())
}

fn print_newer(newer: &[Shown], shape: Shape) {
    let Shape::Table(look) = shape else { return show(&table::json(newer)) };
    if newer.is_empty() {
        println!("{}", t("flowapp.up_to_date"));
        return;
    }
    show(&table::outdated(newer, look));
    for app in newer.iter().filter(|app| app.held) {
        println!("{}", table::held_back(app));
    }
    if newer.iter().any(|app| !app.held) {
        println!("{}", t("flowapp.to_upgrade"));
    }
}

fn outdated(root: &Path, shape: Shape) -> Result<(), String> {
    let read = catalog(root)?;
    print_newer(&newer(root, &read.catalog), shape);
    say_if_stale(&read, shape);
    Ok(())
}

/// Installs each of `names`, or each app that has a newer version, but
/// those that need a newer Flow than this one, which it says of: where
/// they were named, as an error once the rest are in.
fn upgrade(root: &Path, names: &[&str], look: Look) -> Result<(), String> {
    let catalog = catalog_afresh(root)?;
    let (held, due): (Vec<Shown>, Vec<Shown>) = newer(root, &catalog).into_iter().filter(|app| names.is_empty() || names.contains(&app.name.as_str())).partition(|app| app.held);
    if let Some(name) = names.iter().find(|n| catalog.get(n).is_none()) {
        return Err(tf("flowapp.no_such_app", &[("name", name)]));
    }
    // One named that's a package of your own isn't the catalog's to
    // upgrade (a bare upgrade passes over them: `Shown::newer`).
    let installed = Installed::load(root);
    if let Some(why) = names.iter().find_map(|name| apps::in_the_way(&installed, name, false, Some(&catalog))) {
        return Err(why);
    }
    if due.is_empty() && held.is_empty() {
        println!("{}", t("flowapp.up_to_date"));
        return Ok(());
    }
    let held: Vec<String> = held.iter().map(table::held_back).collect();
    if names.is_empty() {
        for line in &held {
            println!("{line}");
        }
    }
    for app in &due {
        put_in(root, &app.name, catalog.get(&app.name).expect("in the catalog"), look)?;
    }
    match held.is_empty() || names.is_empty() {
        true => Ok(()),
        false => Err(held.join("\n")),
    }
}

fn install(root: &Path, names: &[&str], look: Look) -> Result<(), String> {
    // A theme package of your own, from a file: no catalog, nor signature.
    let (files, names): (Vec<&str>, Vec<&str>) = names.iter().partition(|n| n.ends_with(".tar.gz") || n.ends_with(".tgz"));
    // Read now, for a file as for the catalog's: a package of your own
    // mustn't take a name the catalog has added since it was last read,
    // and install_file goes by the copy this saves. Where it won't read,
    // the copy saved before, saying so; offline with none, what's
    // installed and flowapp's own name are all it can know.
    let read = catalog_afresh(root);
    for file in files {
        let (name, kept) = apps::install_file(root, Path::new(file))?;
        println!("{}", tf("flowapp.installed_file", &[("name", &name), ("version", &kept.version)]));
        say_skills(root, &name);
        splash();
    }
    if names.is_empty() {
        return Ok(());
    }
    let catalog = read?;
    let installed = Installed::load(root);
    for name in &names {
        let app = catalog.get(name).ok_or_else(|| tf("flowapp.no_such_app", &[("name", name)]))?;
        let here = app.here().ok_or_else(|| tf("flowapp.no_download_of", &[("name", name), ("platform", &apps::platform())]))?;
        // A package of your own of its name isn't it, at whatever version.
        if let Some(why) = apps::in_the_way(&installed, name, false, Some(&catalog)) {
            return Err(why);
        }
        if installed.apps.get(*name).is_some_and(|kept| !apps::newer(&here.version, &kept.version)) {
            println!("{}", tf("flowapp.already", &[("name", name), ("version", &kept_version(&installed, name))]));
            continue;
        }
        if let Some(needs) = here.needs_newer_flow(&itself::flow_version()) {
            return Err(apps::needs_flow(name, &here.version, needs));
        }
        put_in(root, name, app, look)?;
        // An app new here introduces itself, where its download says how
        // and someone's there to hear it.
        let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        if apps::hello_runs(!installed.apps.contains_key(*name), terminal) {
            if let Some(hello) = apps::hello(root, name) {
                println!("  {}", tf("flowapp.introduces", &[("name", name)]));
                if let Err(error) = run_hello(&hello) {
                    eprintln!("flowapp: {error}");
                }
            }
        }
    }
    Ok(())
}

/// `flowapp hello <name>`: the installed app introduces itself again,
/// where its download says how.
fn hello(root: &Path, name: &str) -> Result<(), String> {
    if !Installed::load(root).apps.contains_key(name) {
        return Err(tf("flowapp.not_installed", &[("name", &name)]));
    }
    let hello = apps::hello(root, name).ok_or_else(|| tf("flowapp.no_hello", &[("name", &name)]))?;
    run_hello(&hello)
}

/// Runs an app's hello in this terminal, waiting for it, and says how it
/// ended where that wasn't well.
fn run_hello(hello: &apps::Hello) -> Result<(), String> {
    let program = hello.program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let status = std::process::Command::new(&hello.program).args(&hello.args).status().map_err(|e| format!("{program}: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(tf("flowapp.hello_ended", &[("program", &program), ("code", &code)])),
        None => Err(tf("flowapp.hello_stopped", &[("program", &program)])),
    }
}

fn kept_version(installed: &Installed, name: &str) -> String {
    installed.apps.get(name).map(|k| k.version.clone()).unwrap_or_default()
}

/// Downloads, checks and installs `name`, and says where it went: in a
/// terminal on one line drawn again as the download comes, a mark once it's
/// checked, and in Flow a splash as it lands.
fn put_in(root: &Path, name: &str, app: &App, look: Look) -> Result<(), String> {
    let version = app.here().map(|d| d.version.clone()).unwrap_or_default();
    let kept = if look.styled {
        let (cols, total) = (look.width.unwrap_or(80), app.here().map_or(0, |d| d.size));
        let installed = apps::install_with(root, name, app, flow_core::UPDATE_PUBLIC_KEY, &mut drawing_steps(name, &version, total, Fetch::Install, cols));
        match installed {
            Ok(kept) => {
                redraw(&table::checked(name, &kept.version, total, true, Fetch::Install, cols));
                splash();
                println!();
                kept
            }
            Err(error) => {
                redraw(&table::failed(name, &version, Fetch::Install, cols));
                println!();
                return Err(table::failure(name, &error));
            }
        }
    } else {
        println!("{}", tf("flowapp.installing", &[("name", &name), ("version", &version)]));
        let kept = apps::install(root, name, app, flow_core::UPDATE_PUBLIC_KEY).map_err(|e| table::failure(name, &e))?;
        println!("{}", tf("flowapp.installed", &[("name", &name), ("version", &kept.version)]));
        kept
    };
    // Programs of the system's commands' names stay behind them until put
    // first.
    let path = std::env::var_os("PATH");
    let firsts = Record::load(root).names();
    let shared: Vec<&String> = kept.programs.iter().filter(|p| !firsts.contains(p) && apps::system_command(p, path.as_deref(), &flows_folders(root)).is_some()).collect();
    if !shared.is_empty() {
        let shared: Vec<&str> = shared.iter().map(|p| p.as_str()).collect();
        println!("  {}", tf("flowapp.can_go_first", &[("names", &shared.join(", ")), ("app", &name)]));
    }
    // An app with a library (the SDK's) says where it is.
    let at = apps::app_dir(root, name, &kept.version);
    for (folder, key) in [("lib", "flowapp.its_lib"), ("include", "flowapp.its_include"), ("python", "flowapp.its_python")] {
        if at.join(folder).is_dir() {
            println!("  {}", tf(key, &[("dir", &at.join(folder).display())]));
        }
    }
    say_skills(root, name);
    Ok(())
}

/// What draws a download's line again as `install_with` and `verify` tell
/// how it comes: how much of its `total`, how fast, and a mark once it's
/// checked, in the words for why it's `fetch`ed.
fn drawing_steps<'a>(name: &'a str, version: &'a str, total: u64, fetch: Fetch, cols: usize) -> impl FnMut(Step) + 'a {
    let start = Instant::now();
    let mut drawn: Option<Instant> = None;
    redraw(&table::fetching(name, version, 0, total, None, fetch, cols));
    move |step| match step {
        Step::Fetched(got) => {
            let now = Instant::now();
            if got < total && drawn.is_some_and(|at| now - at < REDRAW) {
                return;
            }
            drawn = Some(now);
            // A speed once there's been time to tell one.
            let secs = (now - start).as_secs_f64();
            let speed = (secs > 0.25).then(|| got as f64 / secs);
            redraw(&table::fetching(name, version, got, total, speed, fetch, cols));
        }
        Step::Checked => redraw(&table::checked(name, version, total, false, fetch, cols)),
    }
}

/// Says how many skills for Flow's assistant `name` brought, where it
/// brought any, and how many the assistant reads: each of a package's is
/// off until the user turns it on in Settings.
fn say_skills(root: &Path, name: &str) {
    let Some((_, dir)) = apps::skill_dirs(root).into_iter().find(|(n, _)| n == name) else { return };
    let skills = flow_core::ai::skills::list(&dir);
    if skills.is_empty() {
        return;
    }
    let turned_on = flow_core::ai::skills::turned_on();
    let on = skills.iter().filter(|s| turned_on.contains(&format!("{name}:{}", s.name))).count();
    let n = skills.len();
    let line = if on == 0 { tf("flowapp.skills_off", &[("name", &name), ("n", &n)]) } else { tf("flowapp.skills_on", &[("on", &on), ("n", &n)]) };
    println!("  {line}");
}

/// The line the cursor's on, drawn again as `line`.
fn redraw(line: &str) {
    show(&format!("\r{line}\x1b[K"));
}

/// In Flow, the liquid splashes green at the start of the cursor's line,
/// where an app's mark is: down Flow's pipe on Windows, which ConPTY
/// would drop, and to the terminal elsewhere. The cursor's line is asked
/// of the terminal, which Flow answers at once.
fn splash() {
    if !flow_sdk::in_flow() {
        return;
    }
    let Ok((_, row)) = crossterm::cursor::position() else { return };
    let escape = flow_sdk::fx::Effect::burst((1, row)).color(0x5fd75f).size(5.0).strength(0.6).escape();
    match flow_sdk::pipe::Pipe::open() {
        Some(mut pipe) => {
            let _ = pipe.send(escape.as_bytes());
        }
        None => show(&escape),
    }
}

/// Flow's own folders, which `first` never puts a program ahead of: the
/// apps' and the games' (`FLOW_GAMES`).
fn flows_folders(root: &Path) -> Vec<PathBuf> {
    let mut folders = vec![root.to_path_buf()];
    folders.extend(std::env::var_os("FLOW_GAMES").iter().flat_map(std::env::split_paths));
    folders.extend(bundled_dir());
    folders
}

/// The shells `--shell` and `--except` name, and the rest of what `first`
/// and `last` are told beside the names.
#[derive(Debug, Default)]
struct Options {
    names: Vec<String>,
    only: Vec<String>,
    except: Vec<String>,
    here: bool,
    prompt: bool,
    take: Option<Vec<Take>>,
    all: bool,
}

impl Options {
    fn shells(&self) -> Shells {
        match (self.only.is_empty(), self.except.is_empty()) {
            (false, _) => Shells::Only(self.only.clone()),
            (_, false) => Shells::Except(self.except.clone()),
            _ => Shells::All,
        }
    }
}

/// Reads `first`'s (or `last`'s) words: names, and `--shell`, `--except`
/// and `--here`, with `--prompt` and `--take` for `first` and `--all` for
/// `last`, each value after it or after `=`, a list parted by commas.
fn options(command: &str, words: &[&str]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let (flag, mut value) = match word.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (*word, None),
        };
        let mut list = |value: &mut Option<String>| -> Result<Vec<String>, String> {
            let value = value.take().or_else(|| words.next().map(|w| w.to_string())).ok_or_else(|| tf("flowapp.bad_option", &[("option", &flag), ("command", &command)]))?;
            Ok(value.split(',').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect())
        };
        match flag {
            "--shell" => o.only.extend(list(&mut value)?),
            "--except" => o.except.extend(list(&mut value)?),
            "--here" if value.is_none() => o.here = true,
            "--prompt" if command == "first" && value.is_none() => o.prompt = true,
            "--all" if command == "last" && value.is_none() => o.all = true,
            "--take" if command == "first" => {
                let mut takes = o.take.take().unwrap_or_default();
                for word in list(&mut value)? {
                    match Take::parse(&word) {
                        Some(take) => takes.push(take),
                        None if word == "none" => {}
                        None => return Err(tf("flowapp.bad_take", &[("name", &word)])),
                    }
                }
                o.take = Some(takes);
            }
            flag if flag.starts_with('-') => return Err(tf("flowapp.bad_option", &[("option", &flag), ("command", &command)])),
            name => o.names.push(name.to_string()),
        }
    }
    let shells = first::SHELLS.join(", ");
    if let Some(name) = o.only.iter().chain(&o.except).find(|n| !first::SHELLS.contains(&n.as_str())) {
        return Err(tf("flowapp.no_such_shell", &[("name", name), ("names", &shells)]));
    }
    if !o.only.is_empty() && !o.except.is_empty() {
        return Err(t("flowapp.shell_and_except").into());
    }
    if o.here && (!o.only.is_empty() || !o.except.is_empty()) {
        return Err(t("flowapp.here_and_shell").into());
    }
    Ok(o)
}

/// The tab `flowapp` runs in, where it's one of Flow's: its folder of
/// what's first (the last part of `FLOW_FIRST`, the same however its
/// shell spells the path) and the shell it's for.
fn this_pane(root: &Path) -> Option<(PathBuf, String)> {
    let named = PathBuf::from(std::env::var_os("FLOW_FIRST")?);
    let pane = first::panes_dir(root).join(named.file_name()?);
    let shell = first::pane_shell(&pane)?;
    Some((pane, shell))
}

/// BusyBox's applets: those of the BusyBox beside Flow, where there's one
/// (on Windows), else those Flow's has.
fn busybox_applets() -> Vec<String> {
    let beside = bundled_dir().into_iter().flat_map(|dir| [dir.join(apps::exe("busybox")), dir.join("..").join(apps::exe("busybox"))]);
    for busybox in beside.filter(|b| b.is_file()) {
        if let Ok(out) = std::process::Command::new(&busybox).arg("--list").output() {
            let listed: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
            if out.status.success() && !listed.is_empty() {
                return listed;
            }
        }
    }
    first::known_applets()
}

/// Where a choice of `shells` is first, in words; or in this tab alone.
fn where_first(shells: &Shells, here: bool) -> String {
    let named = |names: &[String]| names.iter().map(|n| table::shell_word(n)).collect::<Vec<_>>().join(", ");
    match (here, shells) {
        (true, _) => t("flowapp.where_here").into(),
        (false, Shells::All) => t("flowapp.where_all").into(),
        (false, Shells::Only(names)) => tf("flowapp.where_only", &[("shells", &named(names))]),
        (false, Shells::Except(names)) => tf("flowapp.where_except", &[("shells", &named(names))]),
    }
}

/// Each name first in each shell, how, what it took away and what still
/// comes ahead of it, from the record (and this tab's own choices).
fn say_firsts(root: &Path, shape: Shape) {
    let shells = first::shells_to_show(root);
    let shells: Vec<&str> = shells.iter().map(String::as_str).collect();
    let pane = this_pane(root);
    let rows = first::rows(root, &shells, pane.as_ref().map(|(dir, shell)| (dir.as_path(), shell.as_str())), &busybox_applets());
    match shape {
        Shape::Json => show(&table::firsts_json(&rows)),
        Shape::Table(_) if rows.is_empty() => println!("{}", t("flowapp.first_none")),
        Shape::Table(look) => {
            show(&table::firsts(&rows, look));
            println!("{}", t("flowapp.user_aliases"));
        }
    }
}

/// The choices `names` stand for, of what's installed: an app's programs,
/// one by its name, or `<app>/<program>`.
fn choices_named(installed: &Installed, names: &[String]) -> Result<Vec<Choice>, String> {
    let mut choices: Vec<Choice> = Vec::new();
    for name in names {
        let named = first::resolve(installed, name).map_err(|unnamed| match unnamed {
            Unnamed::Nothing => tf("flowapp.no_program", &[("name", name)]),
            Unnamed::Shared(apps) => tf("flowapp.two_apps", &[("apps", &apps.join(", ")), ("name", name), ("example", &format!("{}/{name}", apps[0]))]),
        })?;
        for choice in named {
            if !choices.iter().any(|c| c.program == choice.program) {
                choices.push(choice);
            }
        }
    }
    Ok(choices)
}

/// What still comes ahead of a name first in `shell`, as a line to say.
fn still_ahead(shell: &str, name: &str, ahead: Ahead) -> Option<String> {
    let shell = table::shell_word(shell);
    let line = match ahead {
        Ahead::Builtin => tf("flowapp.ahead_builtin", &[("shell", &shell), ("name", &name)]),
        Ahead::Alias(of) => tf("flowapp.ahead_alias", &[("shell", &shell), ("name", &name), ("of", &of)]),
        Ahead::Function => tf("flowapp.ahead_function", &[("shell", &shell), ("name", &name)]),
        Ahead::Applet => tf("flowapp.ahead_applet", &[("shell", &shell), ("name", &name)]),
        Ahead::Own => tf("flowapp.ahead_own", &[("shell", &shell), ("name", &name)]),
        // zsh's which hands it plain uses, as it should.
        Ahead::Which => return None,
    };
    Some(line)
}

/// Puts each of the programs the names stand for ahead of the system's
/// commands of their names, where and how the options say: in every Flow
/// tab, or in this one alone.
fn put_first(root: &Path, words: &[&str]) -> Result<(), String> {
    let o = options("first", words)?;
    if o.names.is_empty() {
        return Err(t("flowapp.first_needs_name").into());
    }
    let shells = o.shells();
    let installed = Installed::load(root);
    let pane = match o.here {
        true => Some(this_pane(root).ok_or_else(|| t("flowapp.here_outside").to_string())?),
        false => None,
    };
    // The shells it says what still comes first in: this system's that it's
    // for, or this tab's.
    let scope: Vec<String> = match &pane {
        Some((_, shell)) => vec![shell.clone()],
        None => first::system_shells().iter().filter(|s| shells.has(s)).map(|s| s.to_string()).collect(),
    };
    // At the prompt alone needs a shell with a hook there, of those named.
    let named: Vec<String> = match &pane {
        Some((_, shell)) => vec![shell.clone()],
        None => first::SHELLS.iter().filter(|s| shells.has(s)).map(|s| s.to_string()).collect(),
    };
    if o.prompt && named.iter().all(|s| !first::has_prompt_hook(s)) {
        let named: Vec<&str> = named.iter().map(|s| table::shell_word(s)).collect();
        return Err(tf("flowapp.prompt_nowhere", &[("shells", &named.join(", "))]));
    }
    let mut choices = choices_named(&installed, &o.names)?;
    for choice in &mut choices {
        choice.shells = shells.clone();
        choice.how = if o.prompt { How::Prompt } else { How::Path };
        choice.take = o.take.clone();
    }
    // A builtin no program can do the work of is refused, before anything
    // changes.
    if o.take.as_ref().is_some_and(|t| t.contains(&Take::Builtin)) {
        for choice in &choices {
            let own = scope.iter().find(|s| Take::Builtin.in_shell(s) && first::is_builtin(s, &choice.program) && !first::STANDS_IN.contains(&choice.program.as_str()));
            if let Some(shell) = own {
                return Err(tf("flowapp.not_a_program_builtin", &[("name", &choice.program), ("shell", &shell), ("names", &first::STANDS_IN.join(", "))]));
            }
        }
    }
    let mut record = Record::load(root);
    let mut here = pane.as_ref().map(|(dir, _)| Here::load(dir));
    let path = std::env::var_os("PATH");
    let applets = busybox_applets();
    let at = where_first(&shells, o.here);
    for choice in choices {
        let name = choice.program.clone();
        let over = apps::system_command(&name, path.as_deref(), &flows_folders(root));
        match (&over, choice.how) {
            (Some(over), How::Path) => println!("{}", tf("flowapp.put_first_over", &[("name", &name), ("where", &at), ("path", &over.display())])),
            (None, How::Path) => println!("{}", tf("flowapp.put_first", &[("name", &name), ("where", &at)])),
            (Some(over), How::Prompt) => println!("{}", tf("flowapp.put_first_prompt", &[("name", &name), ("where", &at), ("path", &over.display())])),
            (None, How::Prompt) => println!("{}", tf("flowapp.put_first_prompt_none", &[("name", &name), ("where", &at)])),
        }
        match &mut here {
            Some(here) => here.put(choice.clone()),
            None => {
                let ours = first::whose(&installed, &choice);
                for replaced in record.put(choice.clone()) {
                    if let Some(theirs) = first::whose(&installed, &replaced).filter(|theirs| ours.as_ref() != Some(theirs)) {
                        println!("{}", tf("flowapp.first_instead", &[("app", &theirs), ("name", &name), ("where", &where_first(&replaced.shells, false))]));
                    }
                }
            }
        }
        for shell in &scope {
            if !choice.in_shell(shell) {
                println!("{}", tf("flowapp.no_prompt_hook", &[("shell", &table::shell_word(shell)), ("name", &name)]));
                continue;
            }
            for ahead in first::ahead(shell, &name, choice.how, &choice.takes_in(shell), &applets) {
                if let Some(line) = still_ahead(shell, &name, ahead) {
                    println!("{line}");
                }
            }
        }
    }
    save_firsts(root, &record, pane.as_ref().map(|(dir, _)| dir.as_path()).zip(here.as_ref()))?;
    println!("{}", t(if o.here { "flowapp.first_here_now" } else { "flowapp.first_open_tabs" }));
    Ok(())
}

/// Keeps what changed, the record or this tab's own choices (`here`, with
/// its folder), and writes the panes' folders afresh from it: every pane's,
/// or this one's.
fn save_firsts(root: &Path, record: &Record, here: Option<(&Path, &Here)>) -> Result<(), String> {
    match here {
        Some((pane, here)) => {
            here.save(pane)?;
            first::rewrite_pane(root, pane);
        }
        None => {
            record.save(root)?;
            first::rewrite_panes(root);
        }
    }
    // The links a Flow before 0.1.35 kept, now that the record has them.
    first::forget_old_links(root);
    Ok(())
}

/// Takes each of the programs the names stand for (or, with `--all`, every
/// one) out from ahead of the system's commands, where the options say.
fn put_last(root: &Path, words: &[&str]) -> Result<(), String> {
    let o = options("last", words)?;
    if o.names.is_empty() != o.all {
        return Err(t("flowapp.last_needs_name").into());
    }
    let shells = o.shells();
    let installed = Installed::load(root);
    let pane = match o.here {
        true => Some(this_pane(root).ok_or_else(|| t("flowapp.here_outside").to_string())?),
        false => None,
    };
    let mut record = Record::load(root);
    let mut here = pane.as_ref().map(|(dir, _)| Here::load(dir));
    let first_here = |record: &Record, here: &Here| pane.as_ref().map(|(_, shell)| first::in_pane(record, here, shell)).unwrap_or_default();
    // Each program, and whether it was named itself (rather than as one of
    // an app's, which needn't each have been first).
    let programs: Vec<(String, bool)> = match (&here, o.all) {
        (Some(here), true) => first_here(&record, here).into_keys().map(|p| (p, false)).collect(),
        (None, true) => record.names().into_iter().map(|p| (p, false)).collect(),
        (_, false) => {
            let mut programs = Vec::new();
            for name in &o.names {
                // One no longer installed is taken out by its name all the same.
                let named = first::resolve(&installed, name).map(|cs| cs.into_iter().map(|c| c.program).collect::<Vec<_>>()).unwrap_or_else(|_| vec![name.rsplit('/').next().unwrap_or(name).to_string()]);
                let alone = named.len() == 1;
                programs.extend(named.into_iter().map(|p| (p, alone)));
            }
            programs
        }
    };
    let path = std::env::var_os("PATH");
    let at = where_first(&shells, o.here);
    let mut any = false;
    for (program, alone) in &programs {
        let was = match &mut here {
            Some(here) => {
                let was = first_here(&record, here).contains_key(program);
                here.put_last(program);
                was
            }
            None => record.put_last(program, &shells),
        };
        if !was {
            if *alone {
                println!("{}", tf("flowapp.not_first", &[("name", program), ("where", &at)]));
            }
            continue;
        }
        any = true;
        match apps::system_command(program, path.as_deref(), &flows_folders(root)) {
            Some(back) => println!("{}", tf("flowapp.put_last_over", &[("name", program), ("where", &at), ("path", &back.display())])),
            None => println!("{}", tf("flowapp.put_last", &[("name", program), ("where", &at)])),
        }
    }
    if !any && programs.iter().all(|(_, alone)| !alone) {
        println!("{}", t("flowapp.first_none"));
    }
    save_firsts(root, &record, pane.as_ref().map(|(dir, _)| dir.as_path()).zip(here.as_ref()))
}

fn remove(root: &Path, names: &[&str]) -> Result<(), String> {
    let catalog = apps::saved_catalog(root).map(|read| read.catalog).unwrap_or_default();
    let bundled = bundled_dir();
    for name in names {
        if apps::remove(root, name)? {
            println!("{}", tf("flowapp.removed", &[("name", name)]));
            continue;
        }
        let standing = apps::standing(&Installed::default(), name, catalog.get(name), bundled.as_deref());
        let key = if matches!(standing, Standing::Bundled(_)) { "flowapp.comes_with_flow" } else { "flowapp.not_installed" };
        println!("{}", tf(key, &[("name", name)]));
    }
    Ok(())
}

fn list(root: &Path, shape: Shape) -> Result<(), String> {
    // The catalog says which games Flow carries; offline, what's installed.
    let read = catalog(root).ok();
    let catalog = read.as_ref().map(|read| read.catalog.clone()).unwrap_or_default();
    let installed = Installed::load(root);
    // What flowapp installed, and the games Flow's release carries that
    // the catalog knows of.
    let mut names: Vec<&str> = installed.apps.keys().map(String::as_str).collect();
    names.extend(catalog.names().filter(|n| !installed.apps.contains_key(*n)));
    let had: Vec<Shown> = shown(root, &catalog, names).into_iter().filter(|app| app.standing != Standing::Absent).collect();
    // The apps, then the packages of themes and of skills, each under a
    // heading of its own.
    let (themes, rest): (Vec<Shown>, Vec<Shown>) = had.iter().cloned().partition(|app| app.themes);
    let (skills, apps): (Vec<Shown>, Vec<Shown>) = rest.into_iter().partition(|app| app.skills);
    match shape {
        Shape::Json => show(&table::json(&had)),
        Shape::Table(_) if had.is_empty() => println!("{}", t("flowapp.none_installed")),
        Shape::Table(look) => {
            let mut before = false;
            if !apps.is_empty() {
                show(&table::list(&apps, look));
                before = true;
            }
            for (heading, group) in [("flowapp.themes_heading", &themes), ("flowapp.skills_heading", &skills)] {
                if group.is_empty() {
                    continue;
                }
                if before {
                    println!();
                }
                println!("{}", t(heading));
                show(&table::list(group, look));
                before = true;
            }
        }
    }
    if let Some(read) = &read {
        say_if_stale(read, shape);
    }
    Ok(())
}

fn info(root: &Path, name: &str, shape: Shape) -> Result<(), String> {
    let read = catalog(root)?;
    if read.catalog.get(name).is_none() {
        return Err(tf("flowapp.no_such_app", &[("name", &name)]));
    }
    let app = &shown(root, &read.catalog, [name])[0];
    let code = match &app.standing {
        Standing::Installed(version) => code_signatures(&apps::app_dir(root, name, version)),
        _ => Vec::new(),
    };
    match shape {
        Shape::Json => show(&table::json_one(app, &code)),
        Shape::Table(look) => show(&table::info(app, &apps::platform(), read.age, &code, look)),
    }
    Ok(())
}

/// Who signed each program and library in `dir`, an installed app's
/// version folder, as `codesign` says: the Mac's alone.
#[cfg(target_os = "macos")]
fn code_signatures(dir: &Path) -> Vec<CodeGroup> {
    fn files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_dir() => files(&path, out),
                Ok(meta) if meta.is_file() => out.push(path),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    files(dir, &mut found);
    found.sort();
    let mach_o = found.into_iter().filter(|path| {
        let mut head = [0u8; 4];
        std::fs::File::open(path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head)).is_ok_and(|()| signing::is_mach_o(&head))
    });
    signing::group(mach_o.filter_map(|path| std::process::Command::new("codesign").arg("-dvv").arg(&path).output().ok().map(|out| signing::code_signature(&String::from_utf8_lossy(&out.stderr)))))
}

#[cfg(not(target_os = "macos"))]
fn code_signatures(_: &Path) -> Vec<CodeGroup> {
    Vec::new()
}

/// Looks at each of `names`, or every app flowapp installed, against the
/// download it came from, fetched and checked again, and says what it
/// found; an error where anything isn't as its download has it, or
/// wouldn't fetch or check.
fn verify(root: &Path, names: &[&str], look: Look) -> Result<(), String> {
    let installed = Installed::load(root);
    let catalog = catalog(root).ok().map(|read| read.catalog);
    let names: Vec<String> = match names.is_empty() {
        true => installed.apps.keys().cloned().collect(),
        false => names.iter().map(|n| n.to_string()).collect(),
    };
    if names.is_empty() {
        println!("{}", t("flowapp.none_installed"));
        return Ok(());
    }
    let bundled = bundled_dir();
    let mut failed = 0;
    for name in &names {
        let Some(kept) = installed.apps.get(name) else {
            let app = catalog.as_ref().and_then(|c| c.get(name));
            match apps::standing(&installed, name, app, bundled.as_deref()) {
                Standing::Bundled(version) => println!("{}", tf("flowapp.verify_with_flow", &[("name", name), ("version", &version)])),
                _ => {
                    println!("{}", tf("flowapp.not_installed", &[("name", name)]));
                    failed += 1;
                }
            }
            continue;
        };
        if !verify_one(root, name, kept, catalog.as_ref(), look) {
            failed += 1;
        }
    }
    match failed {
        0 => Ok(()),
        1 => Err(t("flowapp.verify_failed_one").to_string()),
        n => Err(tf("flowapp.verify_failed", &[("n", &n)])),
    }
}

/// Most of the files added since an install that `verify` names.
const ADDED_SHOWN: usize = 5;

/// [`verify`] for one app, `kept`: whether all is as its download has it.
fn verify_one(root: &Path, name: &str, kept: &Kept, catalog: Option<&Catalog>, look: Look) -> bool {
    let version = &kept.version;
    let total = apps::verify::download_of(name, kept, catalog).map_or(0, |(d, _)| d.size);
    let verdict = match look.styled && !kept.own_file && total > 0 {
        true => {
            let cols = look.width.unwrap_or(80);
            let verdict = apps::verify::verify(root, name, kept, catalog, flow_core::UPDATE_PUBLIC_KEY, &mut drawing_steps(name, version, total, Fetch::Again, cols));
            match &verdict {
                Ok(_) => redraw(&table::checked(name, version, total, true, Fetch::Again, cols)),
                Err(_) => redraw(&table::failed(name, version, Fetch::Again, cols)),
            }
            println!();
            verdict
        }
        false => apps::verify::verify(root, name, kept, catalog, flow_core::UPDATE_PUBLIC_KEY, &mut |_| {}),
    };
    let paint = |text: String, ink: table::Ink| table::painted(&text, ink, look.styled);
    match verdict {
        Err(error) => {
            println!("{}", table::failure(name, &error));
            false
        }
        Ok(Verdict::OwnFile) => {
            println!("{}", tf("flowapp.verify_own_file", &[("name", &name), ("version", version)]));
            true
        }
        Ok(Verdict::NoDownload) => {
            println!("{}", tf("flowapp.verify_no_download", &[("name", &name), ("version", version)]));
            true
        }
        Ok(Verdict::Checked { download, against, diff }) => {
            let who = signing::signer(flow_core::UPDATE_PUBLIC_KEY).unwrap_or_default();
            let key = signing::short(&signing::fingerprint(flow_core::UPDATE_PUBLIC_KEY).unwrap_or_default());
            let signed = match download.signed.is_empty() {
                true => tf("flowapp.signed_by_key", &[("who", &who), ("key", &key)]),
                false => tf("flowapp.signed_by_key_on", &[("who", &who), ("key", &key), ("date", &download.signed)]),
            };
            let differ = diff.changed.len() + diff.missing.len();
            let key = match (diff.ok(), diff.files(), differ) {
                (true, 1, _) => "flowapp.verify_ok_one",
                (true, _, _) => "flowapp.verify_ok",
                (false, _, 1) => "flowapp.verify_differs_one",
                (false, _, _) => "flowapp.verify_differs",
            };
            let n = if diff.ok() { diff.files() } else { differ };
            let line = tf(key, &[("name", &name), ("version", version), ("signed", &signed), ("n", &n)]);
            println!("{}", paint(line, if diff.ok() { table::Ink::Newer } else { table::Ink::Bad }));
            if against == Against::Catalog {
                println!("  {}", paint(tf("flowapp.verify_by_catalog", &[("version", version)]), table::Ink::Dim));
            }
            for file in &diff.changed {
                println!("  {}", paint(tf("flowapp.verify_changed", &[("file", file)]), table::Ink::Bad));
            }
            for file in &diff.missing {
                println!("  {}", paint(tf("flowapp.verify_missing", &[("file", file)]), table::Ink::Bad));
            }
            if !diff.added.is_empty() {
                let added = match diff.added.len() {
                    1 => t("flowapp.verify_added_one").to_string(),
                    n => tf("flowapp.verify_added", &[("n", &n)]),
                };
                println!("  {}", paint(added, table::Ink::Dim));
                for file in diff.added.iter().take(ADDED_SHOWN) {
                    println!("    {}", paint(file.clone(), table::Ink::Dim));
                }
                if diff.added.len() > ADDED_SHOWN {
                    println!("    {}", paint(tf("flowapp.verify_more", &[("n", &(diff.added.len() - ADDED_SHOWN))]), table::Ink::Dim));
                }
            }
            diff.ok()
        }
    }
}

fn search(root: &Path, words: &[&str], shape: Shape) -> Result<(), String> {
    let read = catalog(root)?;
    let words: Vec<String> = words.iter().map(|w| w.to_string()).collect();
    let found = shown(root, &read.catalog, read.catalog.search(&words));
    match shape {
        Shape::Json => show(&table::json(&found)),
        Shape::Table(_) if found.is_empty() => println!("{}", t("flowapp.nothing_found")),
        Shape::Table(look) => show(&table::search(&found, look)),
    }
    say_if_stale(&read, shape);
    Ok(())
}
