//! Recording the terminal (`Command::Record`): the terminal area as Flow
//! draws it, the liquid, glass, tube and frames in it, taken from each frame
//! the renderer shows (`render::capture`) at most `FPS` a second, and
//! encoded on a `flow-record` thread into a looping animated WebP
//! (`webp::AnimEncoder`) as each frame comes, so nothing raw piles up. A
//! frame not taken (nothing changed, or too soon after the last) only lasts
//! longer in the file, so an idle tab costs nothing. Where every pixel
//! changes (a theme's tube, a large window) and the encoder can't keep up,
//! frames are taken only as it's ready for them (`Recorder::ready`), so
//! those kept are evenly spaced, and it hurries (`Pace`) at libwebp's
//! fastest method until it keeps up again. What's here is pure but the
//! thread: the frames' conversion and shrinking, the canvas, the pacing,
//! the prompt's names and the file's name, all tested without a GPU.

mod webp;

use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

/// Frames taken a second, at most.
pub const FPS: f32 = 24.0;
/// A recording stops by itself after this long.
pub const MAX_LENGTH: Duration = Duration::from_secs(10 * 60);
/// The widest a recording is, in pixels: the window's points, no more.
pub const MAX_WIDTH: u32 = 1600;
/// The widest at full size: a 4K screen's width.
pub const MAX_FULL_WIDTH: u32 = 3840;
/// Frames waiting for the encoder; past it, one is dropped.
const QUEUE: usize = 8;
/// Frames waiting for the encoder from which it hurries whatever its pace,
/// as when frames come faster than `Recorder::ready` would take them.
const BEHIND: usize = 2;
/// Frames encoded in a hurry before one is tried at the chosen method
/// again, to see whether it keeps up now: two seconds' worth.
const RETRY: u32 = 48;
/// Settings' Quality choices (`record.quality`), smallest file first.
pub const QUALITIES: [&str; 4] = ["smaller", "balanced", "sharp", "lossless"];
/// Settings' Size choices (`record.size`).
pub const SIZES: [&str; 2] = ["points", "pixels"];
/// Settings' Countdown choices (`record.countdown`), in seconds; 0 starts
/// at once.
pub const COUNTDOWNS: [u32; 4] = [0, 3, 5, 10];

/// How a recording is encoded, from Settings' Quality.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Encoding {
    /// Lossy's quality (0 to 100); lossless's effort, lower faster.
    pub quality: f32,
    /// libwebp's method: 0 fast to 6 small.
    pub method: i32,
    pub lossless: bool,
    /// Frames between whole frames, which put right what a lossy
    /// encoder's small changes left behind.
    pub keyframes: i32,
}

/// The encoding a Quality choice means; one Flow doesn't know is
/// Balanced. Higher qualities take smaller changes as changes and draw a
/// whole frame more often, so a slowly moving liquid stays clean, at the
/// cost of a larger file; lossless takes every change. Sharp's method is
/// lower to keep up, and libwebp's sharper colors (`use_sharp_yuv`), which
/// more than doubled each frame's time, are left off. On 4 s of the oil
/// style's own frames (`encoding_keeps_up_with_frames_that_change_all_over`,
/// on an Apple silicon Mac), at 1100 × 700 each takes about 21, 24, 23 and 19 ms
/// a frame, in a hurry 14, 15, 19 and 19, so each keeps up with `FPS`; at
/// every pixel of 2200 × 1400, 52, 59, 60 and 47, in a hurry 26, 29, 41 and
/// 46, so lossless falls behind there and is taken as it's ready. The
/// worst 16 × 16 block was 25, 11, 6 and 1 levels off (q80 with no whole
/// frames, as at first: 30), hurried or not, in 0.4, 0.7, 1.6 and 16 MB,
/// hurried a third more.
pub fn encoding(quality: &str) -> Encoding {
    let lossy = |quality, method, keyframes| Encoding { quality, method, lossless: false, keyframes };
    match quality {
        "smaller" => lossy(70.0, 4, 48),
        "sharp" => lossy(95.0, 2, 12),
        "lossless" => Encoding { quality: 0.0, method: 0, lossless: true, keyframes: 24 },
        _ => lossy(85.0, 4, 24),
    }
}
/// A frame drawn this much short of the interval still counts as due, as
/// vsync's frames come a little either side of it.
const SLACK: Duration = Duration::from_millis(4);

/// A frame as the GPU hands it back: the terminal area's pixels,
/// premultiplied, four bytes each, rows packed, `bgra` as the surface is.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: bool,
    pub pixels: Vec<u8>,
    /// Since the recording began.
    pub at: Duration,
}

enum Msg {
    Frame(Frame),
    End(Duration),
}

/// A recording under way: its thread, and what it was handed.
pub struct Recorder {
    tx: Sender<Msg>,
    done: Receiver<Result<Vec<u8>, String>>,
    pub taken: u32,
    pub dropped: u32,
}

impl Recorder {
    /// Starts the thread, for a file `canvas` in size, encoded as `how`
    /// says.
    pub fn start(canvas: (u32, u32), how: Encoding) -> std::io::Result<Recorder> {
        let (tx, rx) = bounded(QUEUE);
        let (done_tx, done) = bounded(1);
        std::thread::Builder::new().name("flow-record".into()).spawn(move || {
            let result = encode(canvas, how, rx);
            let _ = done_tx.send(result);
        })?;
        Ok(Recorder { tx, done, taken: 0, dropped: 0 })
    }

    /// Whether the thread is ready for another frame: none waiting for it,
    /// so a frame taken now goes in as soon as the one it's on is done.
    /// Otherwise the frame is better owed and taken a little later, so the
    /// frames kept are evenly spaced however fast the encoder goes.
    pub fn ready(&self) -> bool {
        self.tx.is_empty()
    }

    /// Hands a frame to the thread, or drops it while the thread is behind.
    pub fn add(&mut self, frame: Frame) {
        match self.tx.try_send(Msg::Frame(frame)) {
            Ok(()) => self.taken += 1,
            Err(TrySendError::Full(_)) => self.dropped += 1,
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Ends the recording at `end`; the file's bytes (or why there are none)
    /// come on what's handed back once the thread has caught up.
    pub fn finish(self, end: Duration) -> Receiver<Result<Vec<u8>, String>> {
        if self.dropped > 0 {
            log::info!("recording: {} frames taken, {} dropped while the encoder was behind", self.taken, self.dropped);
        }
        let _ = self.tx.send(Msg::End(end));
        self.done
    }
}

/// The thread's work: each frame onto the canvas and into the encoder, and
/// the file at the end.
fn encode(canvas: (u32, u32), how: Encoding, rx: Receiver<Msg>) -> Result<Vec<u8>, String> {
    let mut encoder = webp::AnimEncoder::new(canvas.0, canvas.1, &how)?;
    let mut failed: Option<String> = None;
    let mut last = None;
    let mut pace = Pace::default();
    let (mut hurried, mut frames) = (0u32, 0u32);
    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Frame(frame) => {
                let ms = millis(frame.at);
                if failed.is_some() || last.is_some_and(|last| ms <= last) {
                    continue;
                }
                let began = std::time::Instant::now();
                let hurry = pace.hurry() || rx.len() >= BEHIND;
                let rgba = onto_canvas(&frame, canvas);
                match encoder.add(&rgba, ms, hurry) {
                    Ok(()) => last = Some(ms),
                    Err(e) => failed = Some(e),
                }
                pace.took(began.elapsed(), hurry);
                frames += 1;
                hurried += hurry as u32;
            }
            Msg::End(end) => {
                if hurried > 0 {
                    log::info!("recording: {hurried} of {frames} frames encoded in a hurry, to keep up");
                }
                return match failed {
                    Some(e) => Err(e),
                    None => encoder.finish(millis(end)),
                };
            }
        }
    }
    Err("the recording ended without its end".into())
}

/// Whether the encoder hurries: once a frame at the chosen method takes
/// longer than the time between frames (an average of the last few, so one
/// slow frame doesn't), it encodes at libwebp's fastest until, tried at the
/// chosen method again every `RETRY` frames, a frame keeps up with room to
/// spare. Its frames are a little larger, not worse to look at.
#[derive(Debug, Default)]
struct Pace {
    /// A frame's time at the chosen method, averaged, in seconds.
    usual: Option<f32>,
    hurrying: bool,
    /// Frames hurried since the chosen method was last tried.
    since: u32,
}

impl Pace {
    /// Whether the next frame is hurried.
    fn hurry(&self) -> bool {
        self.hurrying && self.since < RETRY
    }

    /// A frame took `took`, `hurried` or not.
    fn took(&mut self, took: Duration, hurried: bool) {
        let budget = interval().as_secs_f32();
        let took = took.as_secs_f32();
        if hurried {
            self.since += 1;
            return;
        }
        if self.hurrying {
            // The chosen method tried again: it stays unless it keeps up
            // with room to spare.
            self.usual = Some(took);
            self.hurrying = took > 0.8 * budget;
            self.since = 0;
            return;
        }
        let usual = self.usual.map_or(took, |u| u * 0.75 + took * 0.25);
        self.usual = Some(usual);
        if usual > 0.9 * budget {
            self.hurrying = true;
            self.since = 0;
        }
    }
}

fn millis(at: Duration) -> i32 {
    at.as_millis().min(i32::MAX as u128) as i32
}

/// The file's size for a terminal area `crop` pixels on a display of
/// `scale`, at Settings' Size: its points, at most `MAX_WIDTH` wide, or
/// its every pixel ("pixels"), at most `MAX_FULL_WIDTH`; the same shape.
pub fn canvas_for(crop: (u32, u32), scale: f32, size: &str) -> (u32, u32) {
    let (scale, widest) = if size == "pixels" { (1.0, MAX_FULL_WIDTH) } else { (scale.max(1.0), MAX_WIDTH) };
    let (mut w, mut h) = (crop.0 as f32 / scale, crop.1 as f32 / scale);
    if w > widest as f32 {
        h *= widest as f32 / w;
        w = widest as f32;
    }
    ((w.round() as u32).max(1), (h.round() as u32).max(1))
}

/// Whether a frame drawn `since` the last one taken is due (none taken
/// yet: it is).
pub fn due(since: Option<Duration>) -> bool {
    since.is_none_or(|since| since + SLACK >= interval())
}

/// The time between frames taken.
pub fn interval() -> Duration {
    Duration::from_secs_f32(1.0 / FPS)
}

/// A frame onto the canvas: straight RGBA, shrunk to it, or, where the
/// window has changed size since the start, fitted inside it at its own
/// shape (never enlarged), transparent round it.
fn onto_canvas(frame: &Frame, canvas: (u32, u32)) -> Vec<u8> {
    let mut pixels = frame.pixels.clone();
    straighten(&mut pixels, frame.bgra);
    let (w, h) = (frame.width, frame.height);
    if (w, h) == canvas {
        return pixels;
    }
    let (x, y, fw, fh) = fit((w, h), canvas);
    let fitted = if (fw, fh) == (w, h) { pixels } else { shrink(&pixels, w, h, fw, fh) };
    if (fw, fh) == canvas {
        return fitted;
    }
    let mut out = vec![0u8; canvas.0 as usize * canvas.1 as usize * 4];
    for row in 0..fh as usize {
        let to = ((y as usize + row) * canvas.0 as usize + x as usize) * 4;
        out[to..to + fw as usize * 4].copy_from_slice(&fitted[row * fw as usize * 4..(row + 1) * fw as usize * 4]);
    }
    out
}

/// Where a picture `size` goes in `canvas`: as large as fits at its own
/// shape but no larger than it is, in the middle.
fn fit(size: (u32, u32), canvas: (u32, u32)) -> (u32, u32, u32, u32) {
    let factor = (canvas.0 as f32 / size.0.max(1) as f32).min(canvas.1 as f32 / size.1.max(1) as f32).min(1.0);
    let w = ((size.0 as f32 * factor).round() as u32).clamp(1, canvas.0);
    let h = ((size.1 as f32 * factor).round() as u32).clamp(1, canvas.1);
    ((canvas.0 - w) / 2, (canvas.1 - h) / 2, w, h)
}

/// Premultiplied pixels made straight RGBA in place, from BGRA where the
/// surface is.
fn straighten(pixels: &mut [u8], bgra: bool) {
    for p in pixels.as_chunks_mut::<4>().0 {
        if bgra {
            p.swap(0, 2);
        }
        let a = p[3] as u16;
        if a == 0 {
            p[..3].fill(0);
        } else if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u16 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// An area-average shrink (every source pixel counts, weighted by how much
/// of it falls in each new one), which keeps text and the liquid's fine
/// detail smooth; a whole factor (a Retina screen's 2) averages squares.
pub fn shrink(src: &[u8], w: u32, h: u32, ow: u32, oh: u32) -> Vec<u8> {
    if ow == 0 || oh == 0 {
        return Vec::new();
    }
    if w.is_multiple_of(ow) && h.is_multiple_of(oh) && w / ow == h / oh {
        return shrink_by(src, w, w / ow, ow, oh);
    }
    let (sx, sy) = (w as f32 / ow as f32, h as f32 / oh as f32);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for oy in 0..oh {
        let (y0, y1) = (oy as f32 * sy, (oy + 1) as f32 * sy);
        for ox in 0..ow {
            let (x0, x1) = (ox as f32 * sx, (ox + 1) as f32 * sx);
            let mut acc = [0f32; 4];
            let mut total = 0.0;
            for y in y0 as u32..(y1.ceil() as u32).min(h) {
                let wy = (y1.min(y as f32 + 1.0) - y0.max(y as f32)).max(0.0);
                for x in x0 as u32..(x1.ceil() as u32).min(w) {
                    let wx = (x1.min(x as f32 + 1.0) - x0.max(x as f32)).max(0.0);
                    let weight = wx * wy;
                    let i = ((y * w + x) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += src[i + c] as f32 * weight;
                    }
                    total += weight;
                }
            }
            let o = ((oy * ow + ox) * 4) as usize;
            for c in 0..4 {
                out[o + c] = if total > 0.0 { (acc[c] / total).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
        }
    }
    out
}

/// `shrink` by a whole factor `k`: each new pixel the mean of a k × k square.
fn shrink_by(src: &[u8], w: u32, k: u32, ow: u32, oh: u32) -> Vec<u8> {
    let (w, k, ow, oh) = (w as usize, k as usize, ow as usize, oh as usize);
    let n = (k * k) as u32;
    let mut out = vec![0u8; ow * oh * 4];
    let mut row = vec![0u32; ow * 4];
    for oy in 0..oh {
        row.fill(0);
        for y in oy * k..(oy + 1) * k {
            let line = &src[y * w * 4..(y + 1) * w * 4];
            for (x, p) in line.as_chunks::<4>().0.iter().enumerate() {
                let o = (x / k) * 4;
                for c in 0..4 {
                    row[o + c] += p[c] as u32;
                }
            }
        }
        for (o, sum) in out[oy * ow * 4..(oy + 1) * ow * 4].iter_mut().zip(&row) {
            *o = ((sum + n / 2) / n) as u8;
        }
    }
    out
}

/// Whether a prompt's text shows the user's name or the computer's (its
/// short name, before the first dot), ignoring case. Names shorter than
/// three letters are passed over, as they'd be found in any word.
pub fn shows_names(prompt: &str, user: Option<&str>, host: Option<&str>) -> bool {
    let prompt = prompt.to_lowercase();
    let host = host.map(|h| h.split('.').next().unwrap_or(h));
    [user, host].into_iter().flatten().map(str::trim).filter(|name| name.chars().count() >= 3).any(|name| prompt.contains(&name.to_lowercase()))
}

/// The user's name, as every system's shells have it.
pub fn user_name() -> Option<String> {
    ["USER", "USERNAME", "LOGNAME"].into_iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.trim().is_empty()))
}

/// The file's name, by the local date and time it ended:
/// `Flow-2026-11-11 15.30.12.webp`.
pub fn file_name() -> String {
    let now = jiff::Zoned::now();
    format!("Flow-{}.webp", now.strftime("%Y-%m-%d %H.%M.%S"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, p: [u8; 4]) -> Vec<u8> {
        p.repeat((w * h) as usize)
    }

    /// Every frame of a WebP as RGBA, with its time, as image-webp reads
    /// them (frames.rs's reader), which gives RGB where nothing is clear.
    fn frames_of(bytes: &[u8]) -> (u32, u32, bool, Vec<(Vec<u8>, u32)>) {
        let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        let (w, h) = decoder.dimensions();
        let forever = matches!(decoder.loop_count(), image_webp::LoopCount::Forever);
        let alpha = decoder.has_alpha();
        let mut buf = vec![0u8; decoder.output_buffer_size().unwrap()];
        let mut out = Vec::new();
        for _ in 0..decoder.num_frames() {
            let ms = decoder.read_frame(&mut buf).unwrap();
            let rgba = if alpha { buf.clone() } else { buf.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect() };
            out.push((rgba, ms));
        }
        (w, h, forever, out)
    }

    fn near(a: &[u8], b: [u8; 4], by: u8) -> bool {
        a.as_chunks::<4>().0.iter().all(|p| p.iter().zip(b).all(|(&x, y)| x.abs_diff(y) <= by))
    }

    #[test]
    fn frames_go_into_a_looping_webp_each_lasting_until_the_next() {
        let canvas = (32, 16);
        let mut rec = Recorder::start(canvas, encoding("balanced")).unwrap();
        let red = [200, 30, 40, 255];
        let blue = [20, 40, 210, 255];
        rec.add(Frame { width: 32, height: 16, bgra: false, pixels: solid(32, 16, red), at: Duration::ZERO });
        rec.add(Frame { width: 32, height: 16, bgra: false, pixels: solid(32, 16, blue), at: Duration::from_millis(100) });
        // Nothing changed for a while: no frames, then a last one.
        rec.add(Frame { width: 32, height: 16, bgra: false, pixels: solid(32, 16, red), at: Duration::from_millis(1100) });
        let bytes = rec.finish(Duration::from_millis(1300)).recv().unwrap().unwrap();
        let (w, h, forever, frames) = frames_of(&bytes);
        assert_eq!((w, h, forever), (32, 16, true));
        let times: Vec<u32> = frames.iter().map(|f| f.1).collect();
        assert_eq!(times, [100, 1000, 200]);
        let firsts: Vec<&[u8]> = frames.iter().map(|f| &f.0[..4]).collect();
        assert!(near(&frames[0].0, red, 12) && near(&frames[1].0, blue, 12) && near(&frames[2].0, red, 12), "{firsts:?}");
    }

    #[test]
    fn premultiplied_bgra_from_the_gpu_comes_out_straight_rgba() {
        let mut p = vec![10, 20, 100, 128, 0, 0, 0, 0, 1, 2, 3, 255];
        straighten(&mut p, true);
        assert_eq!(p, [199, 40, 20, 128, 0, 0, 0, 0, 3, 2, 1, 255]);
    }

    #[test]
    fn a_retina_frame_shrinks_to_its_points_by_averaging_squares() {
        // A 4 × 2 picture, its left half white and right half black.
        let mut src = Vec::new();
        for _ in 0..2 {
            src.extend([255, 255, 255, 255].repeat(2));
            src.extend([0, 0, 0, 255].repeat(2));
        }
        assert_eq!(shrink(&src, 4, 2, 2, 1), [255, 255, 255, 255, 0, 0, 0, 255]);
        // A factor that isn't whole still averages by area.
        let out = shrink(&src, 4, 2, 3, 1);
        assert_eq!(out.len(), 12);
        assert!(out[0] == 255 && (100..=155).contains(&out[4]) && out[8] == 0);
    }

    #[test]
    fn the_canvas_is_the_terminal_in_points_at_most_so_wide() {
        assert_eq!(canvas_for((2400, 1400), 2.0, "points"), (1200, 700));
        assert_eq!(canvas_for((1200, 700), 1.0, "points"), (1200, 700));
        assert_eq!(canvas_for((4000, 2000), 2.0, "points"), (1600, 800));
        assert_eq!(canvas_for((6400, 1000), 1.0, "points"), (1600, 250));
        // Full size keeps a Retina screen's every pixel, up to 4K's width.
        assert_eq!(canvas_for((2400, 1400), 2.0, "pixels"), (2400, 1400));
        assert_eq!(canvas_for((7680, 2000), 2.0, "pixels"), (3840, 1000));
        // A size Flow doesn't know is points.
        assert_eq!(canvas_for((2400, 1400), 2.0, "huge"), (1200, 700));
    }

    #[test]
    fn each_quality_is_an_encoding_and_better_ones_draw_whole_frames_more_often() {
        let all: Vec<Encoding> = QUALITIES.iter().map(|q| encoding(q)).collect();
        assert!(all[0].quality < all[1].quality && all[1].quality < all[2].quality);
        assert!(all[0].keyframes > all[1].keyframes && all[1].keyframes > all[2].keyframes);
        assert!(all[3].lossless && !all[2].lossless);
        assert_eq!(encoding("nonsense"), encoding("balanced"));
    }

    #[test]
    fn every_quality_writes_a_webp_that_reads_back() {
        for quality in QUALITIES {
            let mut rec = Recorder::start((24, 12), encoding(quality)).unwrap();
            for (i, p) in [[200, 30, 40, 255], [20, 40, 210, 255], [30, 200, 60, 255]].into_iter().enumerate() {
                rec.add(Frame { width: 24, height: 12, bgra: false, pixels: solid(24, 12, p), at: Duration::from_millis(100 * i as u64) });
            }
            let bytes = rec.finish(Duration::from_millis(300)).recv().unwrap().unwrap();
            let (w, h, forever, frames) = frames_of(&bytes);
            assert_eq!((w, h, forever, frames.len()), (24, 12, true, 3), "{quality}");
            // Lossless gives back what went in: libwebp's own decoder reads
            // it exactly, while image-webp's rounds a step as it lays a
            // frame over the canvas.
            if quality == "lossless" {
                assert!(near(&frames[1].0, [20, 40, 210, 255], 1), "{quality}");
            }
        }
    }

    #[test]
    fn a_frame_of_another_size_is_fitted_in_the_middle_never_enlarged() {
        assert_eq!(fit((200, 100), (100, 100)), (0, 25, 100, 50));
        assert_eq!(fit((40, 20), (100, 100)), (30, 40, 40, 20));
        let frame = Frame { width: 4, height: 2, bgra: false, pixels: solid(4, 2, [9, 9, 9, 255]), at: Duration::ZERO };
        let out = onto_canvas(&frame, (4, 4));
        // Rows 1 and 2 hold the picture; rows 0 and 3 are clear.
        assert!(out[..16].iter().all(|&b| b == 0) && out[48..].iter().all(|&b| b == 0));
        assert!(near(&out[16..48], [9, 9, 9, 255], 0));
    }

    #[test]
    fn the_encoder_hurries_once_it_falls_behind_and_tries_again_later() {
        let ms = Duration::from_millis;
        let mut pace = Pace::default();
        // Keeping up, and one slow frame among quick ones: no hurry.
        for _ in 0..10 {
            pace.took(ms(20), false);
        }
        pace.took(ms(80), false);
        assert!(!pace.hurry());
        // Frames that change all over, slower than the interval: it hurries.
        pace.took(ms(200), false);
        assert!(pace.hurry());
        // Hurried frames until the chosen method is tried again.
        for _ in 0..RETRY {
            assert!(pace.hurry());
            pace.took(ms(20), true);
        }
        assert!(!pace.hurry(), "tried again");
        // Still too slow: back to hurrying.
        pace.took(ms(50), false);
        assert!(pace.hurry());
        for _ in 0..RETRY {
            pace.took(ms(20), true);
        }
        // Now it keeps up with room to spare: no more hurry.
        pace.took(ms(25), false);
        assert!(!pace.hurry());
    }

    #[test]
    fn the_recorder_is_ready_only_with_nothing_waiting() {
        let mut rec = Recorder::start((8, 8), encoding("balanced")).unwrap();
        assert!(rec.ready());
        // The thread takes each frame from the queue at once, so it's ready
        // again once it has.
        rec.add(Frame { width: 8, height: 8, bgra: false, pixels: solid(8, 8, [1, 2, 3, 255]), at: Duration::ZERO });
        let start = std::time::Instant::now();
        while !rec.ready() && start.elapsed() < Duration::from_secs(5) {
            std::thread::yield_now();
        }
        assert!(rec.ready());
        let _ = rec.finish(Duration::from_millis(100)).recv().unwrap().unwrap();
    }

    /// How long each quality takes a frame, hurried and not, on frames
    /// `--record` wrote (a style's folder of `NNNN.png`, in
    /// `FLOW_RECORD_FRAMES`), shrunk to their points as a recording is:
    /// `FLOW_RECORD_FRAMES=<dir>/oil cargo test --release -p flow-core
    /// encoding_keeps_up -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn encoding_keeps_up_with_frames_that_change_all_over() {
        let Ok(dir) = std::env::var("FLOW_RECORD_FRAMES") else { return };
        let scale: u32 = std::env::var("FLOW_RECORD_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut paths: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
        paths.sort();
        let frames: Vec<Frame> = paths
            .iter()
            .take(96)
            .enumerate()
            .map(|(i, p)| {
                let (w, h, mut rgba) = crate::graphics::decode_png(&std::fs::read(p).unwrap()).unwrap();
                crate::graphics::premultiply(&mut rgba);
                Frame { width: w, height: h, bgra: false, pixels: rgba, at: interval() * i as u32 }
            })
            .collect();
        let canvas = (frames[0].width / scale, frames[0].height / scale);
        println!("{} frames of {} × {}, onto {} × {}", frames.len(), frames[0].width, frames[0].height, canvas.0, canvas.1);
        for quality in QUALITIES {
            for hurry in [false, true] {
                let mut encoder = webp::AnimEncoder::new(canvas.0, canvas.1, &encoding(quality)).unwrap();
                let (mut total, mut worst) = (Duration::ZERO, Duration::ZERO);
                for frame in &frames {
                    let began = std::time::Instant::now();
                    let rgba = onto_canvas(frame, canvas);
                    encoder.add(&rgba, millis(frame.at), hurry).unwrap();
                    let took = began.elapsed();
                    total += took;
                    worst = worst.max(took);
                }
                let bytes = encoder.finish(millis(interval() * frames.len() as u32)).unwrap();
                let mean = total.as_secs_f32() * 1000.0 / frames.len() as f32;
                // The worst 16 × 16 block, by its mean difference from what
                // went in, over every frame read back.
                let (_, _, _, back) = frames_of(&bytes);
                let mut off = 0f32;
                for (frame, (got, _)) in frames.iter().zip(&back) {
                    let want = onto_canvas(frame, canvas);
                    for by in 0..canvas.1 / 16 {
                        for bx in 0..canvas.0 / 16 {
                            let mut sum = 0u32;
                            for y in by * 16..by * 16 + 16 {
                                let i = ((y * canvas.0 + bx * 16) * 4) as usize;
                                sum += want[i..i + 64].iter().zip(&got[i..i + 64]).map(|(a, b)| a.abs_diff(*b) as u32).sum::<u32>();
                            }
                            off = off.max(sum as f32 / (16.0 * 16.0 * 4.0));
                        }
                    }
                }
                println!("{quality:>9} {}: {mean:5.1} ms a frame, worst {:5.1}, {:.2} MB, worst block {off:4.1} levels off", if hurry { "hurried" } else { "chosen " }, worst.as_secs_f32() * 1000.0, bytes.len() as f32 / 1e6);
            }
        }
    }

    #[test]
    fn a_frame_is_due_once_the_interval_has_nearly_passed() {
        assert!(due(None));
        assert!(!due(Some(Duration::from_millis(16))));
        assert!(due(Some(Duration::from_millis(40))));
        assert!(due(Some(Duration::from_millis(50))));
    }

    #[test]
    fn a_prompt_showing_the_users_or_computers_name_is_found() {
        assert!(shows_names("pat@mymac ~ % ", Some("pat"), Some("mymac.local")));
        assert!(shows_names("[MyMac:~] $ ", Some("someone"), Some("mymac.lan")));
        assert!(shows_names("/home/pat/src $ ", Some("pat"), None));
        assert!(!shows_names("bash-5.3$ ", Some("pat"), Some("mymac")));
        assert!(!shows_names("~ % ", Some("pat"), Some("mymac")));
        // Too short to tell from any word.
        assert!(!shows_names("ab$ ", Some("ab"), None));
        assert!(!shows_names("anything", None, None));
    }

    #[test]
    fn the_file_is_named_by_its_date_and_time() {
        let name = file_name();
        assert!(name.starts_with("Flow-") && name.ends_with(".webp"), "{name}");
        assert_eq!(name.len(), "Flow-2026-11-11 15.30.12.webp".len(), "{name}");
    }
}
