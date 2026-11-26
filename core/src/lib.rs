//! Flow's shared core: the terminal, the assistant, the liquid and the UI.
//!
//! Nothing here calls an OS API. Each OS crate (`windows`, `macos`, `linux`)
//! implements [`Platform`], and the `flow` binary hands it to [`run`].

mod about_cube;
mod access;
pub mod ai;
pub mod apps;
mod app;
mod box_drawing;
mod caps_lock;
pub mod combining;
mod command_palette;
mod config;
pub mod event;
mod find;
pub mod flow_command;
mod frames;
mod fx;
mod gallery;
mod graphics;
pub mod hardware;
pub mod help;
mod history;
pub mod hotkey;
pub mod keys;
pub mod i18n;
pub mod identity;
mod input;
pub mod instance;
mod iterm;
mod line_size;
mod marks;
pub mod mcp;
pub mod link;
pub mod logos;
mod looks;
/// Theme packages' form, for the example that writes Flow's own.
pub use looks::packaged as themes;
mod model;
mod mosaic;
mod mouse;
mod notify;
mod osc;
mod panel_input;
mod panes;
pub mod pipe;
pub mod platform;
pub mod power;
mod record;
mod render;
mod scrollbar;
pub mod resources;
mod secrets;
pub mod session;
mod settings;
mod settings_search;
mod sixel;
pub mod shell;
pub mod sound;
mod text;
mod ui;
pub mod version;

use winit::event_loop::EventLoop;

pub use config::config_dir;
pub use platform::Platform;

/// Display name, used in the title bar, dialogs and Claude's system prompt.
pub const APP_NAME: &str = "Flow";
/// Who makes Flow, as About's credits name them, and the page there they
/// link to.
pub const MAKER: &str = "Arcanii";
pub const MAKER_PAGE: &str = "https://github.com/arcanii/";
/// Used for the config folder and the credential store entry.
pub const APP_ID: &str = "flow-terminal";
/// The argument naming the task an administrator's Flow is opened for, to
/// offer at once (`Platform::relaunch_elevated`).
pub const CARRY_ON: &str = "--carry-on";
/// The public half of Flow's own Ed25519 update key (examples/update_key.rs
/// made it). Updates are accepted only when signed with it: by WinSparkle on
/// Windows, and on macOS as Sparkle's SUPublicEDKey.
pub const UPDATE_PUBLIC_KEY: &str = "2emwe9ELY5aktN4yvpnu8bIBdwemm7wqgZz6Fexmdwo=";

/// Runs Flow on `platform` until the window closes.
pub fn run(platform: &'static dyn Platform) -> anyhow::Result<()> {
    // Before the logger, which truncates the release log. The macOS
    // packaging scripts read this to check what they bundle.
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", version::answer(version::say_updates()));
        return Ok(());
    }
    // A Flow started for a command (`flow --window`, or `flow` with no tab
    // to ask) is a guest beside the one running, and keeps its own log, so
    // that one's isn't emptied.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let guest = flow_command::read_guest(&args);
    // A pane's secret is its own programs': a Flow started from one of
    // its tabs (`open` passes the tab's variables on) mustn't hand it on.
    std::env::remove_var(flow_command::TAB_KEY);
    // A guest's words after its `--` are its command's, never Flow's own.
    let own_args = guest.is_none();
    let mut args = std::env::args().skip(1).filter(move |_| own_args);
    let mut snapshot = None;
    let mut record = false;
    let mut carry_on = None;
    let mut carrying_on = false;
    while let Some(arg) = args.next() {
        if arg == "--snapshot" || arg == "--record" {
            record = arg == "--record";
            snapshot = Some(std::path::PathBuf::from(args.next().unwrap_or_else(|| "snapshots".into())));
        } else if arg == CARRY_ON {
            carrying_on = true;
            carry_on = args.next().and_then(|id| id.parse::<i64>().ok());
        }
    }
    // This Flow started again while it runs brings the running one
    // forward and quits, before the logger, which would empty that one's
    // log (docs/design/put-away.md).
    let takes_part = instance::takes_part(guest.is_some(), snapshot.is_some(), carrying_on, platform.elevated());
    let start = std::env::current_exe()
        .ok()
        .filter(|_| takes_part)
        .map(|program| platform.hand_to_running(&instance::id(&program, &config::config_dir())));
    if start == Some(platform::Start::Handed) {
        return Ok(());
    }
    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"));
    // Release builds have no console (Windows) or aren't started from one
    // (Flow.app on macOS), so log to a file next to the config. A Flow
    // beside the one running keeps its own, as a guest does, and so does a
    // snapshot or a recording, which leaves the user's Flow's log alone.
    if cfg!(not(debug_assertions)) {
        let dir = config::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let apart = guest.is_some() || snapshot.is_some() || start == Some(platform::Start::Beside);
        let name = if apart { "flow-window.log" } else { "flow.log" };
        if let Ok(file) = std::fs::File::create(dir.join(name)) {
            logger.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    logger.init();
    log::info!("{APP_NAME} {}", version::display());
    if start == Some(platform::Start::Beside) {
        log::warn!("this Flow is running already and didn't come forward, so this one runs beside it");
    }
    platform.mark_running();
    platform.prepare_input();
    let event_loop = EventLoop::<event::UserEvent>::with_user_event().build()?;
    let mut app = app::App::new(event_loop.create_proxy(), snapshot, record, platform);
    if start == Some(platform::Start::First) {
        app.listen_for_starts();
    }
    if let Some(task) = carry_on {
        app.carry_on_at_start(task);
    }
    if let Some(guest) = guest {
        // Not the command itself, which may hold what the log shouldn't.
        log::info!("a guest Flow, started for a command");
        app.as_guest(guest);
    }
    event_loop.run_app(&mut app)?;
    Ok(())
}
