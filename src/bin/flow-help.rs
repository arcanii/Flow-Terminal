//! `flow-help`: Flow's help pages printed in a tab, on every tab's PATH as
//! the games are (`shell::set_games_dir`). `flow-help` prints every page,
//! `flow-help settings` one, `flow-help --list` their names. The pages and
//! how they're printed are core's (`help::for_terminal`); this finds the
//! language, Flow's keys and the assistant's provider as config.toml says
//! them, the terminal's width and whether it takes colors.

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use flow_core::ai::provider;
use flow_core::{help, i18n, keys};

/// Wider than this, lines get hard to follow from one to the next.
const MOST_COLUMNS: usize = 100;

fn main() -> ExitCode {
    // Flow's own language, which it gives its tabs, then the system's.
    let preferred: Vec<String> = ["FLOW_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).collect();
    i18n::set_language(i18n::AUTO, &preferred);
    keys::set(keys::from_config_file());
    // Its keys page names the assistant as Flow does ("Ask GPT").
    provider::set_in_use(provider::from_config_file());
    let stdout = std::io::stdout();
    let terminal = stdout.is_terminal();
    // A terminal that doesn't say its width (a pty never sized) says 0.
    let cols = terminal.then(crossterm::terminal::size).and_then(Result::ok).map(|(w, _)| w as usize).filter(|w| *w > 0).unwrap_or(80);
    let styled = terminal && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match help::for_terminal(&args, cols.clamp(20, MOST_COLUMNS), styled) {
        Ok(text) => {
            // A pager that quits early closes the pipe; that's no error.
            let _ = stdout.lock().write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}
