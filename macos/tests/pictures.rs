//! Prints a sixel and an iTerm2 inline PNG from zsh and bash in a real PTY,
//! as `img2sixel` and `imgcat` would, then the PNG again in parts and an
//! animated GIF, as iTerm2 3.5's `imgcat` sends them, then a kitty picture
//! made to move with kitty's own animation commands, and checks that Flow
//! keeps each at its own size (an animation with its frames, through one
//! texture) and leaves the cursor where xterm, iTerm2 and kitty do: below a
//! sixel at the left, and after an iTerm2 or kitty picture's last row,
//! right of it. Then that files a shell's programs failed to read stay
//! counted through a terminal reset, which a program holding the tab's
//! key may ask for.
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use flow_core::session::{CommandResult, GridSize, Session, Waker};
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
fn a_sixel_and_an_iterm2_png_printed_by_the_shells_are_shown_with_the_cursor_after_each() {
    let platform = &flow_macos::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());
    // An apps' folder of the test's own, never the user's, which a tab
    // opened there would change (its pane's folder, and an older Flow's
    // `flowapp first` links brought into a first.json).
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-pictures-apps-{}", std::process::id())));
    let dir = std::env::temp_dir().join(format!("flow-pictures-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A red sixel 45 × 36 pixels: in cells of 9 × 18, five columns and two rows.
    let sixel = dir.join("red.six");
    std::fs::write(&sixel, format!("\x1bPq\"1;1;45;36#1;2;100;0;0{}\x1b\\", ["!45~"; 6].join("-"))).unwrap();
    // Kickstart's picture, 130 × 131 pixels: fifteen columns and eight rows.
    let png = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/logos/kickstart.png")).unwrap();
    let file = dir.join("disk.iterm");
    std::fs::write(&file, format!("\x1b]1337;File=name={};size={};inline=1:{}\x07", base64(b"kickstart.png"), png.len(), base64(&png))).unwrap();
    // The same in parts of a thousand letters of base64 each.
    let parts = dir.join("disk.parts");
    let mut sent = format!("\x1b]1337;MultipartFile=name={};size={};inline=1\x07", base64(b"kickstart.png"), png.len());
    for part in base64(&png).as_bytes().chunks(1000) {
        sent.push_str(&format!("\x1b]1337;FilePart={}\x07", std::str::from_utf8(part).unwrap()));
    }
    sent.push_str("\x1b]1337;FileEnd\x07");
    std::fs::write(&parts, sent).unwrap();
    // A GIF 18 × 36 pixels, red then blue, forever: two columns and two rows.
    let mut gif = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut gif, 18, 36, &[255, 0, 0, 0, 0, 255]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for index in [0, 1] {
            let pixels = vec![index; 18 * 36];
            let frame = gif::Frame { width: 18, height: 36, delay: 20, buffer: std::borrow::Cow::Borrowed(&pixels), ..gif::Frame::default() };
            encoder.write_frame(&frame).unwrap();
        }
    }
    let moving = dir.join("moving.iterm");
    std::fs::write(&moving, format!("\x1b]1337;File=name={};size={};inline=1:{}\x07", base64(b"moving.gif"), gif.len(), base64(&gif))).unwrap();
    // A kitty picture of one cell, 9 × 18 pixels, made to move with kitty's
    // own commands, quiet, as `kitten icat` sends a GIF: red, then a blue
    // frame, then a green square laid over the red, the red given a gap
    // and all of it played round.
    let colored = |color: [u8; 4], w: usize, h: usize| base64(&color.repeat(w * h));
    let kitty = dir.join("moving.kitty");
    let commands = [
        format!("a=T,f=32,s=9,v=18,i=77,q=2;{}", colored([255, 0, 0, 255], 9, 18)),
        format!("a=f,i=77,q=2,z=100;{}", colored([0, 0, 255, 255], 9, 18)),
        format!("a=f,i=77,q=2,c=1,x=3,y=6,s=3,v=3;{}", colored([0, 255, 0, 255], 3, 3)),
        "a=a,i=77,q=2,r=1,z=100".into(),
        "a=a,i=77,q=2,s=3,v=1".into(),
    ];
    std::fs::write(&kitty, commands.iter().map(|c| format!("\x1b_G{c}\x1b\\")).collect::<String>()).unwrap();

    for profile in platform.shells() {
        let label = profile.label.clone();
        let session = Session::spawn(7, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
            .unwrap_or_else(|e| panic!("{label}: {e:#}"));
        wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
            let st = session.state.lock();
            st.integrated && st.at_prompt
        });
        let command = format!("printf A; cat '{}'; printf B; cat '{}'; printf 'C\\n'", sixel.display(), file.display());
        let result = run(&session, &label, &command);
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
        let command = format!("printf D; cat '{}'; printf E; cat '{}'; printf 'F\\n'", parts.display(), moving.display());
        let result = run(&session, &label, &command);
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
        let command = format!("printf G; cat '{}'; printf 'H\\n'", kitty.display());
        let result = run(&session, &label, &command);
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");

        let (_, held, pictures) = session.held();
        let sizes: Vec<(u32, u32)> = pictures.iter().map(|&(_, w, h)| (w, h)).collect();
        let count = |size| sizes.iter().filter(|&&s| s == size).count();
        // The PNG twice, and the GIF and the kitty animation once each: one
        // texture for all of an animation's frames.
        assert_eq!((count((45, 36)), count((130, 131)), count((18, 36)), count((9, 18))), (1, 2, 1, 1), "{label}: {sizes:?}");
        // Every frame counts in what the tab holds: the GIF's two and the
        // kitty picture's three.
        assert_eq!(held, (45 * 36 + 2 * 130 * 131 + 2 * 18 * 36 + 3 * 9 * 18) * 4, "{label}");
        // The kitty picture plays, its third frame the red with the green
        // square over it.
        let (picture, _) = session.images.lock().lookup(77, 0).unwrap_or_else(|| panic!("{label}: no kitty picture"));
        let frames = picture.frames.as_ref().unwrap_or_else(|| panic!("{label}: the kitty picture doesn't move"));
        assert!(frames.len() == 3 && picture.frame_at(Instant::now()).is_some_and(|(_, next)| next.is_some()), "{label}: {picture:?}");
        let third = &frames.frame(2).unwrap().pixels;
        let at = |x: usize, y: usize| &third[(y * 9 + x) * 4..(y * 9 + x) * 4 + 4];
        assert_eq!((at(0, 0), at(4, 7)), (&[255, 0, 0, 255][..], &[0, 255, 0, 255][..]), "{label}");
        // Picture cells read as blanks: the A before the sixel, the B on the
        // line below it at the left, and the C after the PNG's eighth row,
        // past its fifteen columns.
        let emu = session.emu.lock();
        let text = emu.text_between(0, emu.cursor_abs_line() + 1);
        drop(emu);
        let lines: Vec<&str> = text.lines().collect();
        let a = lines.iter().rposition(|l| *l == "A").unwrap_or_else(|| panic!("{label}: no A in {text}"));
        assert_eq!((lines.get(a + 1), lines.get(a + 2)), (Some(&""), Some(&"B")), "{label}: {text}");
        assert_eq!(lines.get(a + 9).copied(), Some(format!("{}C", " ".repeat(16)).as_str()), "{label}: {text}");
        assert!(lines[a + 3..a + 9].iter().all(|l| l.is_empty()), "{label}: {text}");
        // The PNG in parts as whole, the E after its last row, and the GIF
        // after the E, its second row's end the F's place.
        let d = lines.iter().rposition(|l| *l == "D").unwrap_or_else(|| panic!("{label}: no D in {text}"));
        assert_eq!(lines.get(d + 7).copied(), Some(format!("{}E", " ".repeat(16)).as_str()), "{label}: {text}");
        assert_eq!(lines.get(d + 8).copied(), Some(format!("{}F", " ".repeat(19)).as_str()), "{label}: {text}");
        // The kitty picture's cell between the G and the H, the cursor after
        // it as kitty leaves it.
        assert!(lines.contains(&"G H"), "{label}: {text}");
        eprintln!("{label}: ok");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A program on another computer can name files on this one and time the
/// failures, so past 8 failures in 10 seconds a tab reads no file it names.
/// A program holding the tab's key (`FLOW_TAB_KEY`, which a remote one may
/// be handed) may ask for a terminal reset (`flow-settings reset
/// terminal`), which takes the tab's pictures away; were the failures
/// forgotten with them, a program could reset after each 8 and go on.
#[test]
fn files_that_failed_to_be_read_stay_counted_through_a_terminal_reset() {
    let platform = &flow_macos::PLATFORM;
    flow_core::shell::set_shell_env(platform.shell_env());
    flow_core::apps::first::set_root(std::env::temp_dir().join(format!("flow-pictures-apps-{}", std::process::id())));
    let dir = std::env::temp_dir().join(format!("flow-pictures-reset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Kickstart's picture, named by its path as kitty's icat sends a PNG,
    // quietly, and eight files that aren't there named the same way.
    let png = dir.join("disk.png");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/logos/kickstart.png"), &png).unwrap();
    let named = |path: &std::path::Path| format!("\x1b_Ga=T,q=2,t=f,f=100;{}\x1b\\", base64(path.as_os_str().as_encoded_bytes()));
    let show = dir.join("show.kitty");
    std::fs::write(&show, named(&png)).unwrap();
    let fail = dir.join("fail.kitty");
    std::fs::write(&fail, (0..8).map(|n| named(&dir.join(format!("missing-{n}.png")))).collect::<String>()).unwrap();

    let profile = platform.shells().into_iter().next().expect("a shell");
    let label = profile.label.clone();
    let session = Session::spawn(8, profile, GridSize { cols: 120, rows: 30 }, (9.0, 18.0), 2000, None, Waker::from_fn(|_| {}))
        .unwrap_or_else(|e| panic!("{label}: {e:#}"));
    wait_for(&format!("{label} first prompt"), Duration::from_secs(30), || {
        let st = session.state.lock();
        st.integrated && st.at_prompt
    });
    let cat = |file: &std::path::Path| {
        let result = run(&session, &label, &format!("cat '{}'", file.display()));
        assert_eq!(result.exit_code, Some(0), "{label}: {result:?}");
    };
    let held = || session.held().1;
    // The picture shows, and a reset takes it away.
    cat(&show);
    assert_eq!(held(), 130 * 131 * 4, "{label}: the picture shows");
    session.reset_terminal();
    assert_eq!(held(), 0, "{label}: a reset takes the picture away");
    // Past eight failures the picture is refused unread, and still after
    // a reset. (Were this to take ten seconds, the failures would be
    // forgotten and the picture shown: how long it took says so.)
    let failed = Instant::now();
    cat(&fail);
    cat(&show);
    assert_eq!(held(), 0, "{label}: refused past eight failures, {:?} after them", failed.elapsed());
    session.reset_terminal();
    cat(&show);
    assert_eq!(held(), 0, "{label}: refused after a reset too, {:?} after the failures", failed.elapsed());
    let _ = std::fs::remove_dir_all(&dir);
}
