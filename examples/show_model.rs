//! Shows a 3D model in the terminal with the Ratty graphics protocol, as a
//! program would, to try Flow's models (and Ratty's) by hand: it asks the
//! terminal whether it speaks the protocol, sends the file in base64 chunks,
//! makes room below the prompt and places the model there, then exits; the
//! terminal keeps it, turning it with `--animate`. `--clip n` plays a glTF's
//! animation `n` (Flow plays the first unless told, `-1` none), and
//! `--clicks` asks Flow for clicks on the model and prints each answer
//! until a key is pressed (both Flow's own keys, which Ratty ignores).
//!
//!   cargo run --release --example show_model -- <model.obj|model.glb> [--cols 40] [--rows 12]
//!       [--animate] [--color 7ce7ff] [--depth 2] [--rx 0] [--ry 0] [--rz 0] [--id 1]
//!       [--clip 0] [--clicks]
//!
//! On the Mac and Linux it finds the cursor with `stty` and `CSI 6n`, so it
//! needs a Unix tty. On Windows, where ConPTY drops the protocol's APCs, it
//! sends them down the tab's pipe for images (`FLOW_IMAGE_PIPE`, through
//! Flow-SDK's `Pipe`), finds the cursor through the console, and reads the
//! clicks' answers with its console in VT-input mode, the one mode ConPTY
//! passes them on in (`docs/design/models-through-conpty.md`).
use std::io::Write;

use anyhow::{bail, Context};
use base64::Engine as _;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let number = |name: &str, default: u32| value(name).and_then(|v| v.parse().ok()).unwrap_or(default);
    let Some(path) = args.first().filter(|a| !a.starts_with("--")) else {
        bail!("usage: show_model <model.obj|model.glb> [--cols 40] [--rows 12] [--animate] [--color rrggbb] [--depth d] [--rx/--ry/--rz deg] [--id n] [--clip n] [--clicks]");
    };
    let format = match path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("obj") => "obj",
        Some("glb") => "glb",
        _ => bail!("{path}: a model is .obj or .glb"),
    };
    let bytes = std::fs::read(path).with_context(|| format!("reading {path}"))?;
    let (cols, rows, id) = (number("--cols", 40).max(1), number("--rows", 12).max(1), number("--id", 1));

    // Whether it speaks the protocol, and where the cursor is once there's room.
    let mut out = std::io::stdout();
    write!(out, "{}", "\n".repeat(rows as usize))?;
    out.flush()?;
    let (mut commands, row) = terminal::connect()?;

    // The file, in chunks of 3 KB (4 KB of base64).
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let chunks: Vec<&[u8]> = bytes.chunks(3072).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        let data = base64::engine::general_purpose::STANDARD.encode(chunk);
        commands.send(&format!("r;id={id};fmt={format};source=payload;more={more};name={name};{data}"))?;
    }

    // In the rows just made, from the left edge: the anchor is the middle
    // cell, or the one past the middle. Rows count from 0, the cursor's from 1.
    let top = (row - 1).saturating_sub(rows);
    let mut place = format!("p;id={id};row={};col={};w={cols};h={rows}", top + (rows - 1).div_ceil(2), (cols - 1).div_ceil(2));
    if args.iter().any(|a| a == "--animate") {
        place.push_str(";animate=1");
    }
    for key in ["color", "depth", "rx", "ry", "rz", "clip"] {
        if let Some(v) = value(&format!("--{key}")) {
            place.push_str(&format!(";{key}={v}"));
        }
    }
    let clicks = args.iter().any(|a| a == "--clicks");
    if clicks {
        place.push_str(";clicks=1");
    }
    commands.place(&place)?;
    if clicks {
        println!("Click the model (any key ends)");
        terminal::listen()?;
    }
    Ok(())
}

/// What one answer read so far is, from the bytes or letters read since the
/// last: a whole answer (`ESC _ … ESC \`, its words), one on its way, or a
/// key, which ends the listening.
enum Heard {
    Answer(String),
    Waiting,
    Key,
}

fn heard(pending: &str) -> Heard {
    match pending.find("\x1b\\") {
        Some(end) if pending.starts_with("\x1b_") => Heard::Answer(pending[2..end].to_string()),
        None if pending.starts_with("\x1b_") || pending == "\x1b" => Heard::Waiting,
        _ => Heard::Key,
    }
}

/// The Mac's and Linux's: everything goes through the terminal.
#[cfg(unix)]
mod terminal {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    use anyhow::{bail, Context};

    use super::{heard, Heard};

    /// Ratty's commands, written to the terminal.
    pub struct Commands;

    impl Commands {
        pub fn send(&mut self, command: &str) -> anyhow::Result<()> {
            let mut out = std::io::stdout();
            write!(out, "\x1b_ratty;g;{command}\x1b\\")?;
            Ok(out.flush()?)
        }

        pub fn place(&mut self, command: &str) -> anyhow::Result<()> {
            self.send(command)
        }
    }

    /// Asks whether the terminal speaks the protocol and where the cursor
    /// is (its row, from 1).
    pub fn connect() -> anyhow::Result<(Commands, u32)> {
        let reply = ask("\x1b_ratty;g;s\x1b\\\x1b[6n", b'R')?;
        if !reply.contains("ratty;g;s;v=") {
            bail!("this terminal doesn't speak the Ratty graphics protocol");
        }
        let at = reply.rsplit("\x1b[").next().and_then(|r| r.strip_suffix('R')).and_then(|r| r.split_once(';'));
        let Some((row, _)) = at.and_then(|(r, c)| Some((r.parse::<u32>().ok()?, c.parse::<u32>().ok()?))) else {
            bail!("the terminal didn't say where the cursor is");
        };
        Ok((Commands, row))
    }

    /// Prints each click Flow reports on the model (`ESC _ ratty;g;click;…
    /// ESC \`) until a key is pressed.
    pub fn listen() -> anyhow::Result<()> {
        let mut tty = std::fs::OpenOptions::new().read(true).open("/dev/tty").context("opening the terminal")?;
        let saved = stty(&["-g"])?;
        stty(&["-icanon", "-echo", "min", "1", "time", "0"])?;
        let result = (|| -> anyhow::Result<()> {
            let (mut pending, mut byte) = (Vec::new(), [0u8; 1]);
            while tty.read(&mut byte)? == 1 {
                pending.push(byte[0]);
                match heard(&String::from_utf8_lossy(&pending)) {
                    Heard::Answer(words) => {
                        println!("{words}");
                        pending.clear();
                    }
                    Heard::Waiting => {}
                    Heard::Key => break,
                }
            }
            Ok(())
        })();
        stty(&[&saved])?;
        result
    }

    fn stty(args: &[&str]) -> anyhow::Result<String> {
        let tty = std::fs::File::open("/dev/tty")?;
        let out = std::process::Command::new("stty").args(args).stdin(tty).output().context("running stty")?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Writes `query` to the terminal and reads its answers up to `end`, with
    /// the tty raw for a moment (through `stty`, so nothing to link).
    fn ask(query: &str, end: u8) -> anyhow::Result<String> {
        let mut tty = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").context("opening the terminal")?;
        let saved = stty(&["-g"])?;
        stty(&["-icanon", "-echo", "min", "0", "time", "1"])?;
        let result = (|| -> anyhow::Result<String> {
            tty.write_all(query.as_bytes())?;
            tty.flush()?;
            let (mut reply, start) = (Vec::new(), Instant::now());
            let mut byte = [0u8; 1];
            while start.elapsed() < Duration::from_secs(2) {
                if tty.read(&mut byte)? == 1 {
                    reply.push(byte[0]);
                    if byte[0] == end {
                        break;
                    }
                }
            }
            Ok(String::from_utf8_lossy(&reply).into_owned())
        })();
        stty(&[&saved])?;
        result
    }
}

/// Windows': ConPTY drops the protocol's APCs, so the commands go down the
/// tab's pipe for images, and the clicks' answers come through the console
/// in VT-input mode.
#[cfg(windows)]
mod terminal {
    use std::ffi::c_void;

    use anyhow::{bail, Context};
    use flow_sdk::pipe::{Pipe, DA1};

    use super::{heard, Heard};

    /// Ratty's commands, down Flow's pipe.
    pub struct Commands(Pipe);

    impl Commands {
        pub fn send(&mut self, command: &str) -> anyhow::Result<()> {
            Ok(self.0.send(format!("\x1b_ratty;g;{command}\x1b\\").as_bytes())?)
        }

        /// The placement, once Flow has read it: Flow places it once ConPTY
        /// has painted the rows made for it, and the clicks' words printed
        /// after come below them.
        pub fn place(&mut self, command: &str) -> anyhow::Result<()> {
            Ok(self.0.send_and_wait(format!("\x1b_ratty;g;{command}\x1b\\").as_bytes())?)
        }
    }

    /// Asks Flow's pipe whether it speaks the protocol, and the console where
    /// the cursor is (its row, from 1), which ConPTY keeps as Flow's screen
    /// shows it.
    pub fn connect() -> anyhow::Result<(Commands, u32)> {
        let Some(mut pipe) = Pipe::open() else {
            bail!("no pipe for images here (FLOW_IMAGE_PIPE): run it in a Flow tab");
        };
        let reply = pipe.ask(&[&b"\x1b_ratty;g;s\x1b\\"[..], DA1].concat());
        if !String::from_utf8_lossy(&reply).contains("ratty;g;s;v=") {
            bail!("this Flow doesn't speak the Ratty graphics protocol down its pipe");
        }
        let (_, row) = crossterm::cursor::position().context("finding the cursor")?;
        Ok((Commands(pipe), u32::from(row) + 1))
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> *mut c_void;
        fn GetConsoleMode(console: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(console: *mut c_void, mode: u32) -> i32;
        fn ReadConsoleW(console: *mut c_void, buf: *mut u16, len: u32, read: *mut u32, control: *const c_void) -> i32;
    }

    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    /// `ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT`.
    const COOKED: u32 = 0x7;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x200;

    /// Prints each click Flow reports on the model (`ESC _ ratty;g;click;…
    /// ESC \`) until a key is pressed, with the console in VT-input mode,
    /// its mode put back after.
    pub fn listen() -> anyhow::Result<()> {
        // SAFETY: this process's own console input, read into a buffer that
        // outlives each call, its mode put back before returning.
        unsafe {
            let input = GetStdHandle(STD_INPUT_HANDLE);
            let mut saved = 0u32;
            if GetConsoleMode(input, &mut saved) == 0 {
                bail!("the input isn't a console");
            }
            SetConsoleMode(input, (saved & !COOKED) | ENABLE_VIRTUAL_TERMINAL_INPUT);
            let (mut pending, mut buf, mut n) = (String::new(), [0u16; 256], 0u32);
            'reading: while ReadConsoleW(input, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null()) != 0 {
                for letter in char::decode_utf16(buf[..n as usize].iter().copied()).map(|c| c.unwrap_or('\u{FFFD}')) {
                    pending.push(letter);
                    match heard(&pending) {
                        Heard::Answer(words) => {
                            println!("{words}");
                            pending.clear();
                        }
                        Heard::Waiting => {}
                        Heard::Key => break 'reading,
                    }
                }
            }
            SetConsoleMode(input, saved);
        }
        Ok(())
    }
}
