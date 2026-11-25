//! `flow` in a tab, on the Mac and Windows (`Contents/Helpers/flow`,
//! `games\flow.exe`): opens a tab in the running Flow like the one it was
//! typed in and types a command there (`flow ssh -N -L …`), or with
//! `--window`, or outside Flow's tabs, starts a Flow of its own for it.
//! Its work is `flow_core::flow_command`'s; it asks Flow as
//! `flow-settings` does, with OSC 7732 (down Flow's pipe on Windows). On
//! Linux `flow` is Flow itself, which reads the same arguments first
//! (`src/main.rs`). Flow's own `flow.exe`, which a shell can't wait on or
//! print through, stays off the PATH.

use std::process::ExitCode;

use flow_core::flow_command;
use flow_core::i18n;

fn main() -> ExitCode {
    // A guest Flow started from here would otherwise hold what this
    // program prints to open as long as it runs (a script capturing it).
    #[cfg(windows)]
    flow_windows::keep_std_handles();
    // Flow's own language, which it gives its tabs, then the system's.
    let preferred: Vec<String> = ["FLOW_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).collect();
    i18n::set_language(i18n::AUTO, &preferred);
    let args: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    let code = flow_command::run(&args, &|request| flow_sdk::settings::ask(request, flow_command::WAIT), false);
    ExitCode::from(code as u8)
}
