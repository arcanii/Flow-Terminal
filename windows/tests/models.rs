//! 3D models through ConPTY (`docs/design/models-through-conpty.md`): ConPTY
//! drops the Ratty protocol's APCs, so a program sends its model down the
//! tab's pipe for images, and Flow writes the model's cells itself, which
//! ConPTY doesn't know of. They have to outlast ConPTY's repaints (a
//! full-screen program leaving, a resize), and a click on a model that asked
//! for them has to reach the program through ConPTY's input, which it does
//! for one reading its console in VT-input mode.
#![cfg(windows)]

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use base64::Engine as _;
use flow_core::session::{CommandResult, GridSize, Session, Waker};

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Types `command` at the prompt and waits for its result.
fn run(session: &Session, command: &str) -> CommandResult {
    wait_for("the prompt", Duration::from_secs(10), || session.state.lock().at_prompt);
    session.run_command(command);
    let mut result = None;
    wait_for(command, Duration::from_secs(20), || {
        result = session.poll_command(Duration::from_secs(20));
        result.is_some()
    });
    result.unwrap()
}

/// Has the tabs opened here keep their panes' folders in an apps' folder of
/// the tests' own, never the user's.
fn own_apps() {
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-models-apps-{}", std::process::id())));
}

/// A square, a model as small as one gets.
const SQUARE: &[u8] = b"v -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nf 1 2 3\nf 1 3 4\n";

/// A BusyBox tab, session `id` (each test its own, so no two share a pane's
/// folder), at its prompt, and its pipe for images opened as a program in it
/// would open it.
fn tab_and_pipe(id: u32) -> (Session, std::fs::File) {
    own_apps();
    flow_core::pipe::set_platform(&flow_windows::PLATFORM);
    let profile = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    let session = Session::spawn(id, profile, GridSize { cols: 100, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {})).unwrap();
    wait_for("busybox prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    let output = run(&session, "echo $FLOW_IMAGE_PIPE").output;
    let name = output.lines().map(str::trim).find(|l| l.contains("pipe/flow-images-")).unwrap_or_else(|| panic!("{output:?}"));
    let pipe = std::fs::OpenOptions::new().read(true).write(true).open(name).expect("the pipe opens");
    (session, pipe)
}

/// Sends `commands` (Ratty's, each without its `ESC _ ratty;g;` and `ESC \`)
/// down the pipe, then DA1, and waits for its answer: Flow has read them all.
fn send(pipe: &mut std::fs::File, commands: &[String]) {
    let mut bytes: String = commands.iter().map(|c| format!("\x1b_ratty;g;{c}\x1b\\")).collect();
    bytes.push_str("\x1b[c");
    pipe.write_all(bytes.as_bytes()).unwrap();
    let mut answer = Vec::new();
    while !answer.ends_with(b"c") {
        let mut buf = [0u8; 64];
        let n = pipe.read(&mut buf).unwrap();
        assert!(n > 0, "the pipe closed");
        answer.extend_from_slice(&buf[..n]);
    }
}

/// Registers the square as model `id`.
fn register(id: u32) -> String {
    format!("r;id={id};fmt=obj;source=payload;more=0;{}", base64::engine::general_purpose::STANDARD.encode(SQUARE))
}

/// How many picture cells are on the screen, once ConPTY has gone quiet:
/// the model's, as no picture else is shown in these tabs.
fn model_cells(session: &Session) -> usize {
    wait_for("ConPTY to go quiet", Duration::from_secs(10), || {
        session.state.lock().last_output_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400))
    });
    let emu = session.emu.lock();
    let grid = emu.term.grid();
    (0..grid.screen_lines() as i32).map(|l| (0..grid.columns()).filter(|&c| grid[Line(l)][Column(c)].c == '\u{10EEEE}').count()).sum()
}

/// A model placed down the pipe in rows the program made for it, above the
/// prompt, as `show_model` places one, outlasts a full-screen program that
/// drew over it and a resize, each of which ConPTY follows with a repaint of
/// the screen it knows, which has no model in it. (Below the cursor, ConPTY
/// paints its blank screen again as a line there wraps, with no repaint
/// due, so a program makes its rows first.)
#[test]
fn a_model_sent_down_the_pipe_outlasts_full_screen_programs_and_resizes() {
    let (mut session, mut pipe) = tab_and_pipe(61);
    run(&session, r"printf '\n\n\n\n\n\n'");
    wait_for("the prompt", Duration::from_secs(10), || session.state.lock().at_prompt);
    // Rows 2 to 5 above the prompt's, centered on the fourth.
    let prompt = session.emu.lock().term.grid().cursor.point.line.0;
    send(&mut pipe, &[register(9), format!("p;id=9;row={};col=70;w=8;h=4", prompt - 3)]);
    assert_eq!(model_cells(&session), 32, "placed, 8 by 4");

    run(&session, r"printf '\033[?1049h\033[H'; for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do printf '%098d\n' 0; done; sleep 1; printf '\033[?1049l'");
    assert_eq!(model_cells(&session), 32, "a full-screen program took the model away");
    for (cols, rows) in [(110, 30), (100, 30)] {
        let sent = Instant::now();
        session.resize(GridSize { cols, rows }, (9.0, 18.0));
        wait_for(&format!("the repaint at {cols}x{rows}"), Duration::from_secs(10), || session.state.lock().last_output_at.is_some_and(|t| t > sent));
        assert_eq!(model_cells(&session), 32, "resizing to {cols}x{rows} took the model away");
    }
    run(&session, "clear");
    assert_eq!(model_cells(&session), 0, "clear left the model");
}

/// Set for this test binary when the test below starts it in a tab, to read
/// its console as a program asking for clicks does.
const CLICK_READER: &str = "FLOW_TEST_CLICK_READER";

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> *mut std::ffi::c_void;
    fn GetConsoleMode(console: *mut std::ffi::c_void, mode: *mut u32) -> i32;
    fn SetConsoleMode(console: *mut std::ffi::c_void, mode: u32) -> i32;
    fn ReadConsoleW(console: *mut std::ffi::c_void, buf: *mut u16, len: u32, read: *mut u32, control: *const std::ffi::c_void) -> i32;
}

/// Reads its console in VT-input mode, line input, echo and processed input
/// off, until a whole APC has come, and prints it.
fn read_a_click() {
    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const VT_INPUT: u32 = 0x200;
    // SAFETY: this process's own console input, its mode put back below.
    unsafe {
        let input = GetStdHandle(STD_INPUT_HANDLE);
        let mut mode = 0u32;
        GetConsoleMode(input, &mut mode);
        SetConsoleMode(input, (mode & !0x7) | VT_INPUT);
        println!("flow-click-reader-ready");
        let (mut got, mut buf, mut n) = (String::new(), [0u16; 256], 0u32);
        let start = Instant::now();
        while !(got.contains("\x1b_") && got.ends_with("\x1b\\")) && start.elapsed() < Duration::from_secs(15) {
            if ReadConsoleW(input, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null()) == 0 {
                break;
            }
            got.push_str(&String::from_utf16_lossy(&buf[..n as usize]));
        }
        SetConsoleMode(input, mode);
        println!("flow-click-got {}", got.replace('\x1b', "ESC"));
    }
}

/// A click on a model that asked for clicks is answered as on the Mac and
/// Linux, written to the tab's input as `Session::click_model` writes it
/// (`report`; core's own tests hold what it says), and through ConPTY it
/// reaches a program reading its console in VT-input mode whole. Started in
/// a tab with `FLOW_TEST_CLICK_READER` set, this test binary is that
/// program.
#[test]
fn a_click_on_a_model_reaches_a_program_reading_its_console_in_vt_input_mode() {
    if std::env::var_os(CLICK_READER).is_some() {
        read_a_click();
        return;
    }
    let (session, mut pipe) = tab_and_pipe(62);
    let exe = std::env::current_exe().unwrap().to_string_lossy().replace('\\', "/");
    session.run_command(&format!("{CLICK_READER}=1 '{exe}' a_click_on_a_model_reaches_a_program_reading_its_console_in_vt_input_mode --exact --nocapture"));
    wait_for("the reader", Duration::from_secs(20), || session.emu.lock().recent_text(30).contains("flow-click-reader-ready"));
    send(&mut pipe, &[register(4), "p;id=4;row=20;col=50;w=8;h=4;clicks=1".into()]);
    assert_eq!(model_cells(&session), 32);
    session.report(b"\x1b_ratty;g;click;id=4;button=left;x=0.1250;y=-0.2500;z=0.0000\x1b\\");
    wait_for("the answer", Duration::from_secs(20), || session.emu.lock().recent_text(30).contains("flow-click-got"));
    let text = session.emu.lock().recent_text(30);
    let got = text.lines().find(|l| l.contains("flow-click-got")).unwrap_or_default();
    assert!(got.contains("ESC_ratty;g;click;id=4;button=left;"), "{got:?}");
    assert!(got.trim_end().ends_with("ESC\\"), "{got:?}");
}
