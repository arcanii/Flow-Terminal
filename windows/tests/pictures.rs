//! Prints a sixel and an iTerm2 inline PNG from each Windows shell through
//! ConPTY, as `img2sixel` and `imgcat` would, and checks what reaches Flow.
//! Windows' own ConPTY passes OSC 1337 on but drops a sixel's DCS whole, so
//! only the iTerm2 picture shows, and it says what the sixel came to.
//! ConPTY never learns the picture's room and keeps its cursor on the row
//! the picture began on, so Flow has the picture rise from there over the
//! lines above (`session::place_image`): the text after it goes right of
//! its last row, as iTerm2 leaves it, and the next command's prompt and
//! output below it, the picture whole, in every shell.
#![cfg(windows)]

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::vte::ansi::{Color, Rgb};
use flow_core::session::{CommandResult, GridSize, Session, Waker};
use flow_core::shell::{BashFlavor, ShellKind};
use flow_core::Platform;

fn wait_for(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < timeout, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run(session: &Session, label: &str, command: &str) -> CommandResult {
    wait_for(&format!("{label} prompt"), Duration::from_secs(10), || session.state.lock().at_prompt);
    session.run_command(command);
    let mut result = None;
    wait_for(&format!("{label}: {command}"), Duration::from_secs(20), || {
        result = session.poll_command(Duration::from_secs(20));
        result.is_some()
    });
    result.unwrap()
}

/// Has the tabs opened here keep their panes' folders (and any `flowapp
/// first` record) in an apps' folder of the tests' own, never the user's,
/// which a tab opened there would change. Each test calls it before its
/// first shell; the folder is the process's, so they all name the same.
fn own_apps() {
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-pictures-apps-{}", std::process::id())));
}

/// Base64, as imgcat sends a file.
fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { LETTERS[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[test]
fn an_iterm2_png_printed_through_conpty_shows_with_the_text_after_it() {
    own_apps();
    let platform = &flow_windows::PLATFORM;
    let dir = std::env::temp_dir().join(format!("flow-pictures-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A red sixel 45 × 36 pixels, then Kickstart's picture, 130 × 131 pixels:
    // in cells of 9 × 18, fifteen columns and eight rows.
    let sixel = format!("\x1bPq\"1;1;45;36#1;2;100;0;0{}\x1b\\", ["!45~"; 6].join("-"));
    let png = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/logos/kickstart.png")).unwrap();
    let iterm = format!("\x1b]1337;File=name={};size={};inline=1:{}\x07", base64(b"kickstart.png"), png.len(), base64(&png));
    let file = dir.join("pictures.txt");
    std::fs::write(&file, format!("A{sixel}B{iterm}C\r\n")).unwrap();
    let path = file.display().to_string();

    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    for profile in platform.shells().into_iter().chain([busybox]) {
        let label = profile.label.clone();
        let kind = profile.kind;
        let command = match kind {
            ShellKind::Cmd => format!("type \"{path}\""),
            ShellKind::PowerShell => format!("Write-Host -NoNewline (Get-Content -Raw '{path}')"),
            ShellKind::Bash => format!("cat '{}'", path.replace('\\', "/")),
            _ => continue,
        };
        let session = Session::spawn(7, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        run(&session, &label, &command);
        let (_, _, pictures) = session.held();
        let sizes: Vec<(u32, u32)> = pictures.iter().map(|&(_, w, h)| (w, h)).collect();
        assert!(sizes.contains(&(130, 131)), "{label}: {sizes:?}");
        eprintln!("{label}: the sixel {}", if sizes.contains(&(45, 36)) { "came through" } else { "was dropped" });
        // Picture cells read as blanks: the A and B with the picture right
        // of them, and the C after its eighth row, past its fifteen columns.
        let emu = session.emu.lock();
        let text = emu.text_between(0, emu.cursor_abs_line() + 1);
        drop(emu);
        let lines: Vec<&str> = text.lines().collect();
        let a = lines.iter().rposition(|l| *l == "AB").unwrap_or_else(|| panic!("{label}: no AB in {text}"));
        assert!(lines[a + 1..a + 7].iter().all(|l| l.is_empty()), "{label}: {text}");
        assert_eq!(lines.get(a + 7).copied(), Some(format!("{}C", " ".repeat(17)).as_str()), "{label}: {text}");
        // The next command comes below the picture, which stays whole:
        // ConPTY, which never learns its room, has its cursor on the C's
        // row, where Flow has the picture end. Read by the grid's rows, as
        // the command's line wraps.
        run(&session, &label, "echo after");
        let emu = session.emu.lock();
        let grid = emu.term.grid();
        let lines = -(grid.history_size() as i32)..grid.screen_lines() as i32;
        let words = |line: i32| (0..grid.columns()).map(|c| grid[Line(line)][Column(c)].c).filter(|&c| c != '\u{10EEEE}').collect::<String>();
        let ab = lines.clone().rev().find(|&l| words(l).starts_with("AB")).unwrap_or_else(|| panic!("{label}: no AB in {text}"));
        let shown: Vec<usize> = (ab..ab + 8).map(|l| (2..17).filter(|&c| grid[Line(l)][Column(c)].c == '\u{10EEEE}').count()).collect();
        let after = lines.clone().find(|&l| words(l).trim() == "after");
        let text = emu.text_between(0, emu.cursor_abs_line() + 1);
        drop(emu);
        assert_eq!(shown, [15; 8], "{label}: the picture's cells on each of its rows\n{text}");
        assert!(after.is_some_and(|l| l > ab + 7), "{label}: {text}");
        eprintln!("{label}: ok");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// ConPTY drops a sixel written to the terminal, so a sixel program's
/// output goes down the tab's pipe for images instead (`img2sixel a.png >
/// %FLOW_IMAGE_PIPE%`), from each shell. Flow shows it in lines of its own
/// above the shell's prompt, which, with ConPTY's cursor, stays where it
/// was: the line the command printed first, the picture whole, then the
/// prompt and the next command's output below it.
#[test]
fn a_sixel_sent_down_the_pipe_from_each_shell_shows_below_the_commands_text() {
    own_apps();
    flow_core::pipe::set_platform(&flow_windows::PLATFORM);
    let dir = std::env::temp_dir().join(format!("flow-pipe-sixel-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A red sixel 45 × 36 pixels: in cells of 9 × 18, five columns and two
    // rows.
    let sixel = format!("\x1bPq\"1;1;45;36#1;2;100;0;0{}\x1b\\", ["!45~"; 6].join("-"));
    let file = dir.join("red.six");
    std::fs::write(&file, &sixel).unwrap();
    let path = file.display().to_string();
    let modes = dir.join("sdm.txt");
    std::fs::write(&modes, "\x1b[?80h").unwrap();
    let modes = modes.display().to_string();
    let busybox = flow_windows::bundled_busybox().expect("windows/third_party/busybox/busybox64u.exe");
    for profile in flow_windows::PLATFORM.shells().into_iter().chain([busybox]) {
        let label = profile.label.clone();
        let unix = |p: &str| p.replace('\\', "/");
        let (command, modes, name) = match profile.kind {
            ShellKind::Cmd => (format!("echo before & type \"{path}\" > %FLOW_IMAGE_PIPE%"), format!("type \"{modes}\""), "echo %FLOW_IMAGE_PIPE%"),
            ShellKind::PowerShell => (
                format!("Write-Output before; [IO.File]::WriteAllBytes($env:FLOW_IMAGE_PIPE, [IO.File]::ReadAllBytes('{path}'))"),
                // Not Write-Host, which PowerShell 7.7 strips of escapes
                // under NO_COLOR.
                format!("[Console]::Write([IO.File]::ReadAllText('{modes}'))"),
                "$env:FLOW_IMAGE_PIPE",
            ),
            // WSL's programs see no Windows pipe.
            ShellKind::Bash if profile.flavor == BashFlavor::Wsl => continue,
            // BusyBox opens the pipe itself. Git Bash's own redirection
            // can't open a Windows pipe ("Invalid argument", and "No such
            // file or directory" as //./pipe/…), so cmd writes it there.
            ShellKind::Bash if profile.flavor == BashFlavor::BusyBox => {
                (format!("echo before; cat '{}' > \"$FLOW_IMAGE_PIPE\"", unix(&path)), format!("cat '{}'", unix(&modes)), r#"echo "$FLOW_IMAGE_PIPE""#)
            }
            ShellKind::Bash => (format!("echo before; cmd //c 'type {path} > %FLOW_IMAGE_PIPE%'"), format!("cat '{}'", unix(&modes)), r#"echo "$FLOW_IMAGE_PIPE""#),
            _ => continue,
        };
        let session = Session::spawn(7, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        run(&session, &label, &command);
        // Placed once ConPTY's output has stopped, a quarter of a second at most.
        wait_for(&format!("{label}: the sixel"), Duration::from_secs(5), || session.held().2.iter().any(|&(_, w, h)| (w, h) == (45, 36)));
        std::thread::sleep(Duration::from_millis(300));
        run(&session, &label, "echo after");
        let emu = session.emu.lock();
        let grid = emu.term.grid();
        let lines = -(grid.history_size() as i32)..grid.screen_lines() as i32;
        let words = |line: i32| (0..grid.columns()).map(|c| grid[Line(line)][Column(c)].c).filter(|&c| c != '\u{10EEEE}').collect::<String>();
        let cells = |line: i32| (0..grid.columns()).filter(|&c| grid[Line(line)][Column(c)].c == '\u{10EEEE}').count();
        let picture: Vec<i32> = lines.clone().filter(|&l| cells(l) > 0).collect();
        let before = lines.clone().rev().find(|&l| words(l).trim() == "before");
        let after = lines.clone().rev().find(|&l| words(l).trim() == "after");
        let widths: Vec<usize> = picture.iter().map(|&l| cells(l)).collect();
        let text = emu.text_between(0, emu.cursor_abs_line() + 1);
        drop(emu);
        assert_eq!(widths, [5, 5], "{label}: the picture's rows\n{text}");
        assert!(before.is_some_and(|b| b < picture[0]) && after.is_some_and(|a| a > picture[1]), "{label}: {before:?} {picture:?} {after:?}\n{text}");
        // DECSDM written to the terminal comes through ConPTY, so the sixels
        // sent down the pipe after it go at the screen's top left: DECRQM
        // asked there says so. (BusyBox has the pipe's name with slashes.)
        run(&session, &label, &modes);
        let output = run(&session, &label, name).output;
        let name = output.lines().map(|l| l.trim().replace('/', r"\")).find(|l| l.starts_with(r"\\.\pipe\flow-images-")).unwrap_or_else(|| panic!("{label}: {output:?}"));
        let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(&name).expect("the pipe opens");
        pipe.write_all(b"\x1b[?80$p\x1b[c").unwrap();
        let mut answer = Vec::new();
        while !answer.ends_with(b"c") {
            let mut buf = [0u8; 64];
            let n = pipe.read(&mut buf).unwrap();
            assert!(n > 0, "the pipe closed");
            answer.extend_from_slice(&buf[..n]);
        }
        assert_eq!(answer, b"\x1b[?80;1$y\x1b[?62;4;22c", "{label}");
        eprintln!("{label}: ok");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// ConPTY drops kitty's APCs, so a program sends an animation down the
/// tab's pipe for images, as it sends a still picture (`FLOW_IMAGE_PIPE`):
/// the frames with kitty's own commands, then a virtual placement whose
/// bare placeholder cells it writes through the terminal. The same picture
/// as the Mac's test plays from the pipe, every frame held and one texture
/// for all of them.
#[test]
fn a_kitty_animation_sent_down_the_pipe_plays_through_one_texture() {
    own_apps();
    flow_core::pipe::set_platform(&flow_windows::PLATFORM);
    let profile = flow_windows::PLATFORM.shells().into_iter().find(|p| p.id == "powershell").expect("Windows PowerShell");
    let session = Session::spawn(7, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap();
    wait_for("PowerShell first prompt", Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    let output = run(&session, "PowerShell", "$env:FLOW_IMAGE_PIPE").output;
    let name = output.lines().map(str::trim).find(|l| l.starts_with(r"\\.\pipe\flow-images-")).unwrap_or_else(|| panic!("{output:?}"));
    let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(name).expect("the pipe opens");

    // A picture of one cell, 9 × 18 pixels, in a virtual placement: red,
    // then a blue frame, then a green square laid over the red, the red
    // given a gap and all of it played round, quiet, then DA1, whose answer
    // comes once Flow has read the rest.
    let id = 0x4D;
    let colored = |color: [u8; 4], w: usize, h: usize| base64(&color.repeat(w * h));
    let commands = [
        format!("a=T,U=1,f=32,s=9,v=18,i={id},c=1,r=1,q=2;{}", colored([255, 0, 0, 255], 9, 18)),
        format!("a=f,i={id},q=2,z=100;{}", colored([0, 0, 255, 255], 9, 18)),
        format!("a=f,i={id},q=2,c=1,x=3,y=6,s=3,v=3;{}", colored([0, 255, 0, 255], 3, 3)),
        format!("a=a,i={id},q=2,r=1,z=100"),
        format!("a=a,i={id},q=2,s=3,v=1"),
    ];
    let sent: String = commands.iter().map(|c| format!("\x1b_G{c}\x1b\\")).collect();
    pipe.write_all(format!("{sent}\x1b[c").as_bytes()).unwrap();
    let mut answer = Vec::new();
    while !answer.ends_with(b"c") {
        let mut buf = [0u8; 64];
        let n = pipe.read(&mut buf).unwrap();
        assert!(n > 0, "the pipe closed");
        answer.extend_from_slice(&buf[..n]);
    }
    assert_eq!(answer, b"\x1b[?62;4;22c");

    // It plays, its third frame the red with the green square over it.
    let (picture, _) = session.images.lock().lookup(id, 0).expect("the picture and its placement");
    let frames = picture.frames.as_ref().expect("the picture moves");
    assert!(frames.len() == 3 && picture.frame_at(Instant::now()).is_some_and(|(_, next)| next.is_some()), "{picture:?}");
    let third = &frames.frame(2).unwrap().pixels;
    let at = |x: usize, y: usize| &third[(y * 9 + x) * 4..(y * 9 + x) * 4 + 4];
    assert_eq!((at(0, 0), at(4, 7)), (&[255, 0, 0, 255][..], &[0, 255, 0, 255][..]));
    // Every frame counts in what the tab holds, and one texture shows them.
    let (_, held, pictures) = session.held();
    assert_eq!(held, 3 * 9 * 18 * 4);
    assert_eq!(pictures.iter().filter(|&&(_, w, h)| (w, h) == (9, 18)).count(), 1, "{pictures:?}");

    // Its bare cell, written through ConPTY between a G and an H, finds it.
    run(&session, "PowerShell", &format!(r#"if ($PSStyle) {{ $PSStyle.OutputRendering = 'Ansi' }}; $e = [char]27; $p = [char]::ConvertFromUtf32(0x10EEEE); Write-Host "G$e[38;2;0;0;{id}m$p$e[0mH""#));
    let emu = session.emu.lock();
    let grid = emu.term.grid();
    let row = (0..grid.screen_lines() as i32).find(|&l| grid[Line(l)][Column(0)].c == 'G' && grid[Line(l)][Column(2)].c == 'H').expect("the line with the picture");
    let cell = &grid[Line(row)][Column(1)];
    assert_eq!((cell.c, cell.fg), ('\u{10EEEE}', Color::Spec(Rgb { r: 0, g: 0, b: id as u8 })));
}
