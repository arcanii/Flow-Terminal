//! GIF and WebP pictures, still or moving, as iTerm2's inline images and
//! kitty's (`f=100`, which Flow reads as either too) may send them, and the
//! frames of any picture that moves: kitty's own animations too, which a
//! program builds frame by frame (`crate::graphics`'s `animation`). A
//! file's frames are decoded at once, each laid over those before as its
//! file says (a GIF's disposal methods; image-webp lays a WebP's), and kept
//! as pictures of their own ([`Image`]s, each with its own serial, so the
//! renderer can tell when the frame it shows has changed). The picture
//! that holds them ([`Image::frames`]) says which shows when
//! ([`Frames::at`]): the drawing asks as it draws the picture's cells, so it
//! plays wherever it is, and the window wakes only as a frame gives way to
//! the next. Its frames share one texture on the GPU, known by their reel
//! (`Image::reel`), which the renderer writes afresh only as the frame
//! shown changes, so a moving picture holds a frame's worth there.
//!
//! It plays as kitty plays its own: a frame with no time of its own (a
//! gapless one) is passed over; it can be stopped on any frame, played
//! round and round, or played and held at its last frame while more are
//! coming ([`State`]); and each change goes on from the frame it's at, as
//! kitty's do, rather than starting it over.
//!
//! Every frame counts in the store's quota. An animation stops being read
//! past [`MAX_DATA`] of frames or [`MAX_FRAMES`] of them, and plays the
//! frames read; a file with one frame is a still picture.

use std::io::Cursor;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::graphics::{premultiply, Image, MAX_DATA, MAX_SIDE};

/// The most frames an animation keeps.
pub const MAX_FRAMES: usize = 1000;
/// How long a frame shows that says nothing of it (a GIF's 0 or 1
/// hundredths of a second, a WebP's 10 ms or less), as browsers show it.
const UNSET: Duration = Duration::from_millis(100);

/// How an animation plays, as kitty's `a=a,s=` has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Held on the frame it's at (`s=1`): where kitty's own animations
    /// begin, until a program plays them.
    Stopped,
    /// Playing, but held at its last frame rather than going round again,
    /// as more frames are coming (`s=2`).
    Loading,
    /// Playing, round again after its last frame as many times as it plays
    /// (`s=3`): a GIF's and a WebP's, from the start.
    Running,
}

/// The id an animation's one texture is known by, told to the renderer as
/// gone (`crate::graphics::take_gone`) once the last copy of its frames is.
struct Reel(u64);

impl Drop for Reel {
    fn drop(&mut self) {
        crate::graphics::note_gone(self.0);
    }
}

/// An animation's frames, how long each shows, and how it plays. A change
/// is made to a copy (they're cheap: the frames are shared), which takes
/// the place of the picture holding the old, so a frame being drawn stays
/// as it was; the copies share their reel.
#[derive(Clone)]
pub struct Frames {
    /// Each frame, and how long it shows before the next: none for a
    /// gapless frame, which is passed over.
    frames: Vec<(Arc<Image>, Duration)>,
    /// Where each frame's time ends in one play: its gap and those before
    /// it added up, so the frame at a time is found by halving.
    ends: Vec<Duration>,
    /// None for forever; once played out, the last frame stays.
    plays: Option<u32>,
    state: State,
    /// Where it was when last changed: the frame it goes on from, when that
    /// began to show, and the plays finished by then.
    from: usize,
    since: Instant,
    played: u32,
    reel: Arc<Reel>,
}

/// Where an animation is at a time.
struct Position {
    index: usize,
    /// When frame `index` began to show; for one held at an end, as though
    /// its time ended just then, so a frame that comes after it shows at
    /// once, as in kitty.
    began: Instant,
    /// When it gives way to the next; none while it's held.
    until: Option<Instant>,
    played: u32,
}

impl Frames {
    /// Frames `frames` (two or more) begin to play now, `plays` times or
    /// forever.
    pub fn new(frames: Vec<(Image, Duration)>, plays: Option<u32>) -> Self {
        let frames = frames.into_iter().map(|(image, delay)| (image, delay.max(Duration::from_millis(1)))).collect();
        Self::begun(frames, plays, State::Running)
    }

    /// The frames of a kitty picture a program makes move, begun from its
    /// own pixels (kitty's root frame), which has no gap until the program
    /// gives it one; stopped on it, as kitty's are until a program plays
    /// them.
    pub fn kitty(root: Image) -> Self {
        Self::begun(vec![(root, Duration::ZERO)], None, State::Stopped)
    }

    fn begun(frames: Vec<(Image, Duration)>, plays: Option<u32>, state: State) -> Self {
        let reel = Arc::new(Reel(crate::graphics::next_serial()));
        let frames = frames.into_iter().map(|(image, gap)| (adopt(image, reel.0), gap)).collect();
        let mut made = Self { frames, ends: Vec::new(), plays: plays.map(|p| p.max(1)), state, from: 0, since: Instant::now(), played: 0, reel };
        made.add_up();
        made
    }

    fn add_up(&mut self) {
        let mut end = Duration::ZERO;
        self.ends = self
            .frames
            .iter()
            .map(|(_, gap)| {
                end += *gap;
                end
            })
            .collect();
    }

    /// One play's length, the frames' gaps added up: zero for frames that
    /// are all gapless, which don't play.
    fn period(&self) -> Duration {
        self.ends.last().copied().unwrap_or_default()
    }

    /// Where frame `index`'s time begins in a play.
    fn start_of(&self, index: usize) -> Duration {
        index.checked_sub(1).map_or(Duration::ZERO, |i| self.ends[i])
    }

    /// The last frame with a time of its own, which an animation played out,
    /// or held while more are coming, stays on.
    fn last_shown(&self) -> usize {
        self.frames.iter().rposition(|(_, gap)| !gap.is_zero()).unwrap_or(self.frames.len() - 1)
    }

    /// The frame shown at `now`, and when it gives way to the next; none
    /// while it's held: stopped, played out, or waiting for more frames.
    pub fn at(&self, now: Instant) -> (&Arc<Image>, Option<Instant>) {
        let at = self.position(now);
        (&self.frames[at.index].0, at.until)
    }

    fn position(&self, now: Instant) -> Position {
        let held = |index: usize, played: u32| {
            let gap = self.frames[index].1;
            Position { index, began: now.checked_sub(gap).unwrap_or(now), until: None, played }
        };
        let period = self.period();
        if self.state == State::Stopped {
            return Position { index: self.from, began: self.since, until: None, played: self.played };
        }
        if period.is_zero() || self.plays.is_some_and(|plays| self.played >= plays) {
            return held(self.from, self.played);
        }
        // Into the play `from` is in, counted from that play's start.
        let total = (self.start_of(self.from) + now.saturating_duration_since(self.since)).as_nanos();
        let more = u32::try_from(total / period.as_nanos()).unwrap_or(u32::MAX);
        if more > 0 && self.state == State::Loading {
            return held(self.last_shown(), self.played);
        }
        let played = self.played.saturating_add(more);
        if let Some(plays) = self.plays.filter(|&plays| played >= plays) {
            return held(self.last_shown(), plays);
        }
        let into = Duration::from_nanos((total % period.as_nanos()) as u64);
        // A gapless frame's time ends where the one before it ends, so it's
        // never the first to end after `into`.
        let index = self.ends.partition_point(|end| *end <= into);
        let began = now.checked_sub(into - self.start_of(index)).unwrap_or(now);
        Position { index, began, until: Some(now + (self.ends[index] - into)), played }
    }

    /// Notes where it is at `now` as where it goes on from, before what
    /// it's made of or how it plays changes.
    fn settle(&mut self, now: Instant) {
        let at = self.position(now);
        (self.from, self.since, self.played) = (at.index, at.began, at.played);
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Each frame, as the store counts them.
    pub fn each(&self) -> impl Iterator<Item = &Arc<Image>> {
        self.frames.iter().map(|(frame, _)| frame)
    }

    /// Frame `index`, counted from 0 (kitty counts from 1).
    pub fn frame(&self, index: usize) -> Option<&Arc<Image>> {
        self.frames.get(index).map(|(frame, _)| frame)
    }

    /// How long frame `index` shows: zero for a gapless one.
    pub fn gap(&self, index: usize) -> Option<Duration> {
        self.frames.get(index).map(|(_, gap)| *gap)
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// How many times it plays: none for forever.
    pub fn plays(&self) -> Option<u32> {
        self.plays
    }

    /// What its frames hold, decoded.
    pub fn bytes(&self) -> usize {
        self.each().map(|frame| frame.pixels.len()).sum()
    }

    /// The id its one texture is known by, which each frame carries.
    pub fn reel(&self) -> u64 {
        self.reel.0
    }

    /// A frame after the last, shown for `gap` (zero: a gapless one, never
    /// shown, a base for others): playing, it comes in its turn, and one
    /// held at its last frame while more are coming goes on to it at once.
    pub fn push(&mut self, frame: Image, gap: Duration, now: Instant) {
        self.settle(now);
        self.frames.push((adopt(frame, self.reel.0), gap));
        self.add_up();
    }

    /// Frame `index` with new pixels, `frame`, which show at once where
    /// it's the frame shown: a picture of their own, which the renderer
    /// writes over the one its texture held.
    pub fn put(&mut self, index: usize, frame: Image) {
        let reel = self.reel.0;
        if let Some(slot) = self.frames.get_mut(index) {
            slot.0 = adopt(frame, reel);
        }
    }

    /// Takes frame `index` out, as kitty's `d=f` does, the frames after it
    /// moving down; never the last one left. The frame shown stays shown
    /// where it's still there, and where it's the one taken out, the frame
    /// now in its place (or the last, where it was the last) shows from
    /// now. False where nothing was taken.
    pub fn remove(&mut self, index: usize, now: Instant) -> bool {
        if self.frames.len() < 2 || index >= self.frames.len() {
            return false;
        }
        self.settle(now);
        self.frames.remove(index);
        self.add_up();
        if self.from > index {
            self.from -= 1;
        } else if self.from == index {
            (self.from, self.since) = (index.min(self.frames.len() - 1), now);
        }
        true
    }

    /// Frame `index` shows for `gap` from now on (zero: it's passed over).
    pub fn set_gap(&mut self, index: usize, gap: Duration, now: Instant) {
        if index < self.frames.len() {
            self.settle(now);
            self.frames[index].1 = gap;
            self.add_up();
        }
    }

    /// Frame `index` is the one shown, from now: playing, it goes on from
    /// there.
    pub fn show(&mut self, index: usize, now: Instant) {
        if index < self.frames.len() {
            self.settle(now);
            (self.from, self.since) = (index, now);
        }
    }

    /// It plays as `state` says from now, from the frame it's at, its
    /// plays counted afresh, as kitty counts them.
    pub fn set_state(&mut self, state: State, now: Instant) {
        self.settle(now);
        if self.state == State::Stopped && state != State::Stopped {
            self.since = now;
        }
        (self.state, self.played) = (state, 0);
    }

    /// It plays `plays` times in all (none: forever), counting those it
    /// has played already.
    pub fn set_plays(&mut self, plays: Option<u32>, now: Instant) {
        self.settle(now);
        self.plays = plays.map(|p| p.max(1));
    }
}

/// `image` as a frame of the animation whose texture is `reel`'s.
fn adopt(mut image: Image, reel: u64) -> Arc<Image> {
    image.reel = Some(reel);
    Arc::new(image)
}

/// Whether `bytes` are a GIF's.
pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

/// Whether `bytes` are a WebP's.
pub fn is_webp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
}

/// The picture in a GIF or a WebP, premultiplied: still, or moving with
/// its frames. What's wrong, for the log, otherwise.
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    if is_gif(bytes) {
        decode_gif(bytes)
    } else if is_webp(bytes) {
        decode_webp(bytes)
    } else {
        Err("neither a GIF nor a WebP".into())
    }
}

/// The frames read, as a picture: one is a still.
fn finish(width: u32, height: u32, mut frames: Vec<(Image, Duration)>, plays: Option<u32>) -> Result<Image, String> {
    match frames.len() {
        0 => Err("a picture with no frames".into()),
        1 => Ok(frames.remove(0).0),
        _ => Ok(Image::animated(width, height, Frames::new(frames, plays))),
    }
}

/// Whether one more frame of `frame` bytes goes past what an animation
/// keeps, with `held` kept so far in `count` frames; if so, the log says so.
fn too_many(count: usize, held: usize, frame: usize, kind: &str) -> bool {
    let over = count == MAX_FRAMES || held + frame > MAX_DATA;
    if over {
        log::info!("{kind} of more frames than Flow keeps: its first {count} play");
    }
    over
}

fn decode_gif(bytes: &[u8]) -> Result<Image, String> {
    let bad = |e: gif::DecodingError| format!("a GIF that won't decode: {e}");
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(NonZeroU64::new(MAX_DATA as u64).unwrap_or(NonZeroU64::MIN)));
    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(bad)?;
    let (width, height) = (decoder.width() as u32, decoder.height() as u32);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("a GIF {width} × {height}, which Flow doesn't show"));
    }
    // GIF's loop count says how many times it plays again after the first.
    let plays = match decoder.repeat() {
        gif::Repeat::Infinite => None,
        gif::Repeat::Finite(again) => Some(again as u32 + 1),
    };
    let size = (width, height);
    let mut canvas = vec![0u8; width as usize * height as usize * 4];
    let mut frames: Vec<(Image, Duration)> = Vec::new();
    // How the last frame goes before the next is laid over: its rectangle
    // cleared, or the canvas put back as it was before it.
    let mut dispose: Option<(gif::DisposalMethod, [u32; 4], Option<Vec<u8>>)> = None;
    loop {
        let frame = match decoder.read_next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(e) if !frames.is_empty() => {
                log::info!("a GIF cut short after {} frames: {e}", frames.len());
                break;
            }
            Err(e) => return Err(bad(e)),
        };
        if too_many(frames.len(), frames.len() * canvas.len(), canvas.len(), "a GIF") {
            break;
        }
        match dispose.take() {
            Some((gif::DisposalMethod::Background, rect, _)) => clear(&mut canvas, size, rect),
            Some((gif::DisposalMethod::Previous, _, Some(before))) => canvas = before,
            _ => {}
        }
        let rect = [frame.left as u32, frame.top as u32, frame.width as u32, frame.height as u32];
        let before = (frame.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        lay(&mut canvas, size, &frame.buffer, rect);
        let delay = if frame.delay < 2 { UNSET } else { Duration::from_millis(frame.delay as u64 * 10) };
        let mut pixels = canvas.clone();
        premultiply(&mut pixels);
        frames.push((Image::new(width, height, pixels), delay));
        dispose = Some((frame.dispose, rect, before));
    }
    finish(width, height, frames, plays)
}

/// Lays a GIF frame's pixels (RGBA, `rect`'s width by its height) over
/// `canvas`, `size` pixels, at `rect`'s corner, as far as the canvas goes:
/// where they aren't transparent, since a GIF's pixels are seen or not.
fn lay(canvas: &mut [u8], (width, height): (u32, u32), pixels: &[u8], [x, y, w, h]: [u32; 4]) {
    for (row, line) in pixels.chunks(w as usize * 4).take(h as usize).enumerate() {
        let cy = y as usize + row;
        if cy >= height as usize {
            break;
        }
        for (col, px) in line.as_chunks::<4>().0.iter().enumerate() {
            let cx = x as usize + col;
            if cx >= width as usize {
                break;
            }
            if px[3] != 0 {
                let at = (cy * width as usize + cx) * 4;
                canvas[at..at + 4].copy_from_slice(px);
            }
        }
    }
}

/// Clears `rect` of `canvas`, `size` pixels, to transparent, as browsers
/// take a GIF's "background".
fn clear(canvas: &mut [u8], (width, height): (u32, u32), [x, y, w, h]: [u32; 4]) {
    let (x0, x1) = (x.min(width) as usize, x.saturating_add(w).min(width) as usize);
    for cy in y.min(height)..y.saturating_add(h).min(height) {
        let row = cy as usize * width as usize;
        canvas[(row + x0) * 4..(row + x1) * 4].fill(0);
    }
}

fn decode_webp(bytes: &[u8]) -> Result<Image, String> {
    let bad = |e: image_webp::DecodingError| format!("a WebP that won't decode: {e}");
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(bad)?;
    decoder.set_memory_limit(MAX_DATA);
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("a WebP {width} × {height}, which Flow doesn't show"));
    }
    let size = decoder.output_buffer_size().ok_or("a WebP too large to hold")?;
    let alpha = decoder.has_alpha();
    let rgba = |buf: &[u8]| -> Vec<u8> {
        let mut pixels = if alpha { buf.to_vec() } else { buf.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect() };
        premultiply(&mut pixels);
        pixels
    };
    let mut buf = vec![0u8; size];
    if !decoder.is_animated() {
        decoder.read_image(&mut buf).map_err(bad)?;
        return Ok(Image::new(width, height, rgba(&buf)));
    }
    // A WebP's loop count is how many times it plays.
    let plays = match decoder.loop_count() {
        image_webp::LoopCount::Forever => None,
        image_webp::LoopCount::Times(n) => Some(n.get() as u32),
    };
    let frame = width as usize * height as usize * 4;
    let mut frames: Vec<(Image, Duration)> = Vec::new();
    for _ in 0..decoder.num_frames() {
        if too_many(frames.len(), frames.len() * frame, frame, "a WebP") {
            break;
        }
        let ms = match decoder.read_frame(&mut buf) {
            Ok(ms) => ms,
            Err(e) if !frames.is_empty() => {
                log::info!("a WebP cut short after {} frames: {e}", frames.len());
                break;
            }
            Err(e) => return Err(bad(e)),
        };
        let delay = if ms <= 10 { UNSET } else { Duration::from_millis(ms as u64) };
        frames.push((Image::new(width, height, rgba(&buf)), delay));
    }
    finish(width, height, frames, plays)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::borrow::Cow;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    /// A frame of [`gif`]: its pixels' indices, rectangle, delay in
    /// hundredths, disposal and transparent index.
    pub(crate) type GifFrame<'a> = (&'a [u8], [u16; 4], u16, gif::DisposalMethod, Option<u8>);

    /// A GIF 4 × 2 of the palette red, green, blue (2 its transparent
    /// index where a frame says so), from frames given as their pixels'
    /// indices, rectangle, delay in hundredths and disposal, played
    /// `repeat` as GIF counts it.
    pub(crate) fn gif(frames: &[GifFrame<'_>], repeat: gif::Repeat) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut out, 4, 2, &[255, 0, 0, 0, 255, 0, 0, 0, 255]).unwrap();
            encoder.set_repeat(repeat).unwrap();
            for &(indices, [left, top, width, height], delay, dispose, transparent) in frames {
                let frame = gif::Frame { left, top, width, height, delay, dispose, transparent, buffer: Cow::Borrowed(indices), ..gif::Frame::default() };
                encoder.write_frame(&frame).unwrap();
            }
        }
        out
    }

    /// The pixel at `x`, `y` of a picture `width` wide.
    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        image.pixels[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn an_animated_gif_has_its_frames_each_laid_over_the_last() {
        use gif::DisposalMethod::{Keep, Previous};
        // All red for 50 ms; then a blue patch at (1, 1), for the 100 ms of
        // a delay unset; then, put back after, green over the left half.
        let file = gif(&[(&[0; 8], [0, 0, 4, 2], 5, Keep, None), (&[2, 2], [1, 1, 2, 1], 0, Previous, None), (&[1; 4], [0, 0, 2, 2], 20, Keep, None)], gif::Repeat::Infinite);
        let image = decode(&file).unwrap();
        let frames = image.frames.as_ref().expect("moving");
        assert_eq!((image.width, image.height, frames.len(), image.bytes()), (4, 2, 3, 3 * 32));
        let shown: Vec<&Arc<Image>> = frames.each().collect();
        assert_eq!((pixel(shown[0], 1, 1), pixel(shown[1], 1, 1), pixel(shown[1], 0, 0)), (RED, BLUE, RED));
        // The blue patch was to be put back: the third lays green over red.
        assert_eq!((pixel(shown[2], 1, 1), pixel(shown[2], 2, 1), pixel(shown[2], 0, 0)), ([0, 255, 0, 255], RED, [0, 255, 0, 255]));
        // Which shows when, and when it gives way, round and round.
        let start = frames.since;
        let at = |ms: u64| {
            let now = start + Duration::from_millis(ms);
            let (frame, next) = frames.at(now);
            (shown.iter().position(|f| Arc::ptr_eq(f, frame)).unwrap(), next.map(|t| (t - start).as_millis()))
        };
        assert_eq!([at(0), at(49), at(50), at(149), at(150), at(349), at(350 + 60)], [(0, Some(50)), (0, Some(50)), (1, Some(150)), (1, Some(150)), (2, Some(350)), (2, Some(350)), (1, Some(500))]);
    }

    #[test]
    fn a_gif_plays_as_many_times_as_it_says_and_once_without_its_loop() {
        use gif::DisposalMethod::Keep;
        let two = |repeat| gif(&[(&[0; 8], [0, 0, 4, 2], 10, Keep, None), (&[2; 8], [0, 0, 4, 2], 10, Keep, None)], repeat);
        let image = decode(&two(gif::Repeat::Finite(1))).unwrap();
        let frames = image.frames.as_ref().unwrap();
        let later = |ms: u64| frames.at(frames.since + Duration::from_millis(ms));
        // Played twice, it stays on its last frame, and wakes no one.
        assert_eq!((pixel(later(300).0, 0, 0), later(300).1.is_some()), (BLUE, true));
        assert_eq!((pixel(later(400).0, 0, 0), later(400).1), (BLUE, None));
        let once = decode(&two(gif::Repeat::Finite(0))).unwrap();
        let frames = once.frames.as_ref().unwrap();
        assert_eq!(frames.at(frames.since + Duration::from_millis(250)).1, None);
    }

    #[test]
    fn a_gifs_frame_cleared_to_its_background_leaves_it_clear_and_transparency_shows_through() {
        use gif::DisposalMethod::{Background, Keep};
        // Red cleared after it shows; then blue on the right half, its
        // index 1 (green) transparent.
        let file = gif(&[(&[0; 8], [0, 0, 4, 2], 10, Background, None), (&[2, 1, 2, 1], [2, 0, 2, 2], 10, Keep, Some(1))], gif::Repeat::Infinite);
        let image = decode(&file).unwrap();
        let second = image.frames.as_ref().unwrap().each().nth(1).unwrap().clone();
        assert_eq!((pixel(&second, 0, 0), pixel(&second, 2, 0), pixel(&second, 3, 0)), ([0; 4], BLUE, [0; 4]));
    }

    #[test]
    fn a_gif_of_one_frame_is_a_still_and_a_broken_one_is_refused() {
        let file = gif(&[(&[2; 8], [0, 0, 4, 2], 0, gif::DisposalMethod::Keep, None)], gif::Repeat::Infinite);
        let image = decode(&file).unwrap();
        assert!(image.frames.is_none());
        assert_eq!((image.width, pixel(&image, 3, 1)), (4, BLUE));
        assert!(decode(b"GIF89a\x01\x00\x01\x00").unwrap_err().starts_with("a GIF that won't decode"));
        assert_eq!(decode(b"\x89PNG").unwrap_err(), "neither a GIF nor a WebP");
    }

    /// A WebP 2 × 2 of `frames`, each of one color for so many
    /// milliseconds, played `loops` times (0 forever): each frame a still
    /// from image-webp's encoder, in an ANMF chunk of its own.
    pub(crate) fn webp(frames: &[([u8; 4], u32)], loops: u16) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, name: &[u8], data: &[u8]) {
            out.extend(name);
            out.extend((data.len() as u32).to_le_bytes());
            out.extend(data);
            if data.len() % 2 == 1 {
                out.push(0);
            }
        }
        let side = |n: u32| (n - 1).to_le_bytes()[..3].to_vec();
        let mut chunks = Vec::new();
        // Animated, with alpha; the canvas's sides less one, 24 bits each.
        chunk(&mut chunks, b"VP8X", &[[0x12, 0, 0, 0].as_slice(), &side(2), &side(2)].concat());
        chunk(&mut chunks, b"ANIM", &[[0, 0, 0, 0].as_slice(), &loops.to_le_bytes()].concat());
        for &(color, ms) in frames {
            let mut still = Vec::new();
            image_webp::WebPEncoder::new(&mut still).encode(&color.repeat(4), 2, 2, image_webp::ColorType::Rgba8).unwrap();
            // At 0, 0, laid without blending, its own VP8L chunk after.
            let header = [[0u8; 6].as_slice(), &side(2), &side(2), &ms.to_le_bytes()[..3], &[0b10]].concat();
            chunk(&mut chunks, b"ANMF", &[header.as_slice(), &still[12..]].concat());
        }
        [b"RIFF".as_slice(), &(chunks.len() as u32 + 4).to_le_bytes(), b"WEBP", &chunks].concat()
    }

    #[test]
    fn an_animated_webp_plays_its_frames_and_a_still_one_is_a_still() {
        let image = decode(&webp(&[(RED, 40), (BLUE, 0)], 3)).unwrap();
        let frames = image.frames.as_ref().expect("moving");
        assert_eq!((frames.len(), frames.period(), frames.plays), (2, Duration::from_millis(140), Some(3)));
        let shown: Vec<[u8; 4]> = frames.each().map(|f| pixel(f, 1, 1)).collect();
        assert_eq!(shown, [RED, BLUE]);
        // Half see-through, premultiplied.
        let mut still = Vec::new();
        image_webp::WebPEncoder::new(&mut still).encode(&[255, 255, 255, 128].repeat(4), 2, 2, image_webp::ColorType::Rgba8).unwrap();
        let image = decode(&still).unwrap();
        assert!(image.frames.is_none());
        assert_eq!(pixel(&image, 0, 0), [128, 128, 128, 128]);
    }

    const GREEN: [u8; 4] = [0, 255, 0, 255];

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A picture 1 × 1 of `color`.
    fn one(color: [u8; 4]) -> Image {
        Image::new(1, 1, color.to_vec())
    }

    /// The color `frames` shows `n` ms after `t`, and when it gives way, in
    /// ms after `t`.
    fn shown(frames: &Frames, t: Instant, n: u64) -> ([u8; 4], Option<u128>) {
        let (frame, next) = frames.at(t + ms(n));
        (pixel(frame, 0, 0), next.map(|next| (next - t).as_millis()))
    }

    #[test]
    fn kittys_frames_begin_stopped_and_play_on_from_the_frame_they_are_at() {
        let t = Instant::now();
        let mut frames = Frames::kitty(one(RED));
        frames.push(one(BLUE), ms(40), t);
        let at = |frames: &Frames, n| shown(frames, t, n);
        // Stopped on its first frame, which wakes no one.
        assert_eq!(at(&frames, 1000), (RED, None));
        // Played, the first, gapless until given a gap, is passed over: the
        // blue plays round alone.
        frames.set_state(State::Running, t);
        assert_eq!([at(&frames, 0), at(&frames, 50)], [(BLUE, Some(40)), (BLUE, Some(80))]);
        // Given a gap 50 ms in, the blue showing has its whole time first.
        frames.set_gap(0, ms(20), t + ms(50));
        assert_eq!([at(&frames, 60), at(&frames, 85), at(&frames, 100)], [(BLUE, Some(80)), (RED, Some(100)), (BLUE, Some(140))]);
        // Shown from the first, it goes on from there; stopped, it stays.
        frames.show(0, t + ms(200));
        assert_eq!(at(&frames, 210), (RED, Some(220)));
        frames.set_state(State::Stopped, t + ms(230));
        assert_eq!(at(&frames, 5000), (BLUE, None));
        assert_eq!((frames.len(), frames.bytes()), (2, 8));
    }

    #[test]
    fn played_while_frames_are_coming_it_holds_the_last_and_goes_on_to_the_next_at_once() {
        let t = Instant::now();
        let mut frames = Frames::kitty(one(RED));
        frames.set_gap(0, ms(30), t);
        frames.push(one(BLUE), ms(30), t);
        frames.set_state(State::Loading, t);
        let at = |frames: &Frames, n| shown(frames, t, n);
        assert_eq!([at(&frames, 10), at(&frames, 40), at(&frames, 100)], [(RED, Some(30)), (BLUE, Some(60)), (BLUE, None)]);
        // The next frame shows as it comes, for its whole time, and is held.
        frames.push(one(GREEN), ms(30), t + ms(100));
        assert_eq!([at(&frames, 100), at(&frames, 120), at(&frames, 500)], [(GREEN, Some(130)), (GREEN, Some(130)), (GREEN, None)]);
        // Played round, it starts again from the first.
        frames.set_state(State::Running, t + ms(500));
        assert_eq!(at(&frames, 500), (RED, Some(530)));
    }

    #[test]
    fn played_out_it_holds_its_last_frame_until_asked_to_play_on() {
        let t = Instant::now();
        let mut frames = Frames::kitty(one(RED));
        frames.set_gap(0, ms(50), t);
        frames.push(one(BLUE), ms(50), t);
        frames.set_plays(Some(2), t);
        frames.set_state(State::Running, t);
        let at = |frames: &Frames, n| shown(frames, t, n);
        assert_eq!([at(&frames, 150), at(&frames, 250)], [(BLUE, Some(200)), (BLUE, None)]);
        // Asked to play forever, it goes on to the next at once.
        frames.set_plays(None, t + ms(300));
        assert_eq!(at(&frames, 300), (RED, Some(350)));
    }

    #[test]
    fn a_gapless_frame_is_passed_over_and_never_the_last_one_held() {
        let t = Instant::now();
        let mut frames = Frames::kitty(one(RED));
        frames.set_gap(0, ms(30), t);
        frames.push(one(GREEN), Duration::ZERO, t);
        frames.push(one(BLUE), ms(30), t);
        frames.push(one(GREEN), Duration::ZERO, t);
        frames.set_state(State::Running, t);
        let at = |frames: &Frames, n| shown(frames, t, n);
        assert_eq!([at(&frames, 10), at(&frames, 40), at(&frames, 70)], [(RED, Some(30)), (BLUE, Some(60)), (RED, Some(90))]);
        frames.set_state(State::Loading, t + ms(70));
        assert_eq!(at(&frames, 500), (BLUE, None));
        // All gapless, nothing plays.
        let mut still = Frames::kitty(one(RED));
        still.push(one(BLUE), Duration::ZERO, t);
        still.set_state(State::Running, t);
        assert_eq!(shown(&still, t, 100), (RED, None));
    }

    #[test]
    fn an_animation_keeps_no_more_frames_than_flow_holds() {
        let many: Vec<GifFrame<'_>> = vec![(&[0; 8], [0, 0, 4, 2], 2, gif::DisposalMethod::Keep, None); MAX_FRAMES + 5];
        let image = decode(&gif(&many, gif::Repeat::Infinite)).unwrap();
        assert_eq!(image.frames.as_ref().unwrap().len(), MAX_FRAMES);
    }
}
