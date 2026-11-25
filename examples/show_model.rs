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
//! It finds the cursor with `stty` and `CSI 6n`, so it needs a Unix tty; on
//! Windows, where ConPTY drops the protocol's APCs, it doesn't work yet.
use std::io::{Read, Write};
use std::time::{Duration, Instant};

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
    let reply = ask("\x1b_ratty;g;s\x1b\\\x1b[6n", b'R')?;
    if !reply.contains("ratty;g;s;v=") {
        bail!("this terminal doesn't speak the Ratty graphics protocol");
    }
    let at = reply.rsplit("\x1b[").next().and_then(|r| r.strip_suffix('R')).and_then(|r| r.split_once(';'));
    let Some((row, _)) = at.and_then(|(r, c)| Some((r.parse::<u32>().ok()?, c.parse::<u32>().ok()?))) else {
        bail!("the terminal didn't say where the cursor is");
    };

    // The file, in chunks of 3 KB (4 KB of base64).
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let chunks: Vec<&[u8]> = bytes.chunks(3072).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        let data = base64::engine::general_purpose::STANDARD.encode(chunk);
        write!(out, "\x1b_ratty;g;r;id={id};fmt={format};source=payload;more={more};name={name};{data}\x1b\\")?;
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
    write!(out, "\x1b_ratty;g;{place}\x1b\\")?;
    out.flush()?;
    if clicks {
        listen()?;
    }
    Ok(())
}

/// Prints each click Flow reports on the model (`ESC _ ratty;g;click;…
/// ESC \`) until a key is pressed.
fn listen() -> anyhow::Result<()> {
    let mut tty = std::fs::OpenOptions::new().read(true).open("/dev/tty").context("opening the terminal")?;
    let stty = |args: &[&str]| -> anyhow::Result<String> {
        let tty = std::fs::File::open("/dev/tty")?;
        let out = std::process::Command::new("stty").args(args).stdin(tty).output().context("running stty")?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    println!("Click the model (any key ends)");
    let saved = stty(&["-g"])?;
    stty(&["-icanon", "-echo", "min", "1", "time", "0"])?;
    let result = (|| -> anyhow::Result<()> {
        let (mut pending, mut byte) = (Vec::new(), [0u8; 1]);
        while tty.read(&mut byte)? == 1 {
            pending.push(byte[0]);
            let text = String::from_utf8_lossy(&pending).into_owned();
            match text.find("\x1b\\") {
                Some(end) if text.starts_with("\x1b_") => {
                    println!("{}", &text[2..end]);
                    pending.clear();
                }
                // An answer on its way.
                None if text.starts_with("\x1b_") || text == "\x1b" => {}
                // Anything else is a key.
                _ => break,
            }
        }
        Ok(())
    })();
    stty(&[&saved])?;
    result
}

/// Writes `query` to the terminal and reads its answers up to `end`, with
/// the tty raw for a moment (through `stty`, so nothing to link).
fn ask(query: &str, end: u8) -> anyhow::Result<String> {
    let mut tty = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").context("opening the terminal")?;
    let stty = |args: &[&str]| -> anyhow::Result<String> {
        let tty = std::fs::File::open("/dev/tty")?;
        let out = std::process::Command::new("stty").args(args).stdin(tty).output().context("running stty")?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
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
