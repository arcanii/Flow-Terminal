#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! The `flow` binary: runs the shared core on the OS crate this build is for.
//! Adding an OS is a new crate that implements `flow_core::Platform`, plus a
//! line here and in Cargo.toml.

#[cfg(windows)]
use flow_windows as os;

#[cfg(target_os = "macos")]
use flow_macos as os;

#[cfg(all(unix, not(target_os = "macos")))]
use flow_linux as os;

fn main() -> anyhow::Result<()> {
    // On Linux `flow` is also the command that opens a tab in the running
    // Flow (`flow ssh …` in a tab, `flow --window`): its arguments are read
    // for that first, and Flow starts only where they're Flow's own.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let in_tab = std::env::var_os(flow_core::flow_command::TAB_KEY).is_some_and(|k| !k.is_empty());
        if flow_core::flow_command::is_command(&args, in_tab) {
            let preferred: Vec<String> = ["FLOW_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).collect();
            flow_core::i18n::set_language(flow_core::i18n::AUTO, &preferred);
            let args: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
            let ask = |request: &str| flow_sdk::settings::ask(request, flow_core::flow_command::WAIT);
            std::process::exit(flow_core::flow_command::run(&args, &ask, true));
        }
    }
    flow_core::run(&os::PLATFORM)
}
