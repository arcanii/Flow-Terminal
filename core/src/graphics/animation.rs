//! Kitty's animation commands (its spec's "Animation"), on the store's
//! pictures: `a=f` sends a frame, a new one or one edited, laid over a
//! frame before it, a color or itself; `a=a` plays it, stops it, has it
//! loop, shows a frame and sets a frame's gap; `a=c` lays a rectangle of
//! one frame over another; and `a=d,d=f` deletes a frame. A still picture is made one of frames
//! (`crate::frames`, its own pixels the first, kitty's root frame) by the
//! first command that needs it, and plays as kitty's do. Each change is
//! made to a copy of its frames, which a new picture holding them puts in
//! the old one's place, under its id and with its placements: so a frame
//! the renderer is drawing stays as it was, and a command that fails
//! changes nothing. Frames are counted from 1, the picture's own pixels, as
//! kitty counts them, and every one counts in the store's quota.
//!
//! Pixels are kept premultiplied, as all of Flow's pictures are, so laying
//! one over another is the usual "over": the top's color, and the one
//! under it as much as the top lets through.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{answer, load, outcome, premultiply, Command, Image, ImageStore, Response, MAX_DATA};
use crate::frames::{Frames, State, MAX_FRAMES};

/// How long a new frame shows that's given no gap, as in kitty.
const GAP: Duration = Duration::from_millis(40);

/// The gap `z` gives: below zero none (a gapless frame, never shown, a base
/// for others), 0 none given.
fn gap(z: i32) -> Option<Duration> {
    match z {
        0 => None,
        z if z < 0 => Some(Duration::ZERO),
        ms => Some(Duration::from_millis(ms as u64)),
    }
}

/// Whether an animation of `count` frames holding `held` bytes has room for
/// one more of `frame` bytes.
fn has_room(count: usize, held: usize, frame: usize) -> bool {
    count < MAX_FRAMES && held + frame <= MAX_DATA
}

impl ImageStore {
    /// `a=f`: a frame for the picture the command names, its data (PNG or
    /// raw pixels, as a picture's are sent) laid at `x`, `y` over what was
    /// there: with `r` the frame it edits, else a new one after the last,
    /// begun from frame `c`, or from the color `Y` (transparent at first).
    /// `X=1` lays it in place of what's there rather than over it, and `z`
    /// gives the frame's gap (a new one's is 40 ms). Raw pixels sent with no
    /// size fill the picture from where they go. The answer names the
    /// frame, as kitty's does.
    pub(super) fn frame(&mut self, mut cmd: Command, data: Option<Vec<u8>>) -> Response {
        let id = self.named(&cmd);
        let found = self.images.get(&id).filter(|i| i.model.is_none()).map(|i| (i.width, i.height, i.frames.as_ref().map_or(1, Frames::len)));
        let Some((width, height, count)) = found else {
            return Response { reply: answer(&cmd, id, None, "ENOENT:no such image"), ..Response::default() };
        };
        // A frame past the last is a new one, as in kitty.
        let number = match cmd.rows as usize {
            n @ 1.. if n <= count => n,
            _ => count + 1,
        };
        if cmd.format != 100 {
            let (x, y) = (cmd.src[0].min(width), cmd.src[1].min(height));
            if cmd.width == 0 {
                cmd.width = width - x;
            }
            if cmd.height == 0 {
                cmd.height = height - y;
            }
        }
        let result = load(&cmd, data, &mut self.file_failures).and_then(|sent| self.lay_frame(&cmd, id, number - 1, sent));
        if result.is_ok() {
            self.make_room(id);
        }
        Response { reply: answer(&cmd, id, Some(number), &outcome(&result)), ..Response::default() }
    }

    /// Lays `sent` as frame `index` of picture `id`, as [`ImageStore::frame`]
    /// says.
    fn lay_frame(&mut self, cmd: &Command, id: u32, index: usize, sent: Image) -> Result<(), String> {
        if sent.frames.is_some() {
            return Err("EINVAL:a frame can't be an animation".into());
        }
        self.animate(id, |frames, (width, height)| {
            if sent.width > width || sent.height > height {
                return Err("EINVAL:a frame larger than its picture".into());
            }
            let new = index == frames.len();
            if new && !has_room(frames.len(), frames.bytes(), width as usize * height as usize * 4) {
                return Err("ENOSPC:no room for another frame".into());
            }
            let mut canvas = match (frames.frame(index), cmd.cols as usize) {
                (Some(frame), _) => frame.pixels.clone(),
                (None, 0) => filled((width, height), cmd.big_y),
                (None, base) => frames.frame(base - 1).ok_or("ENOENT:no such frame")?.pixels.clone(),
            };
            let size = (sent.width, sent.height);
            lay(&mut canvas, (width, height), (&sent.pixels, sent.width), (0, 0), size, (cmd.src[0], cmd.src[1]), cmd.big_x == 1);
            let frame = Image::new(width, height, canvas);
            let now = Instant::now();
            if new {
                frames.push(frame, gap(cmd.z).unwrap_or(GAP), now);
            } else {
                frames.put(index, frame);
                if let Some(gap) = gap(cmd.z) {
                    frames.set_gap(index, gap, now);
                }
            }
            Ok(())
        })
    }

    /// `a=a`: how the picture the command names plays. `r` and `z` give a
    /// frame's gap (the first frame's is none until given), `c` shows a
    /// frame, `s` stops it (1), plays it holding its last frame while more
    /// are coming (2) or plays it round (3), and `v` how many times it
    /// plays (1 forever, n n − 1 times). Kitty answers only what goes
    /// wrong, and passes over a frame that isn't there.
    pub(super) fn play(&mut self, cmd: &Command) -> Response {
        let id = self.named(cmd);
        let result = self.animate(id, |frames, _| {
            let now = Instant::now();
            let count = frames.len();
            let frame = |n: u32| (n as usize).checked_sub(1).filter(|&i| i < count);
            if let (Some(index), Some(gap)) = (frame(cmd.rows), gap(cmd.z)) {
                frames.set_gap(index, gap, now);
            }
            if let Some(index) = frame(cmd.cols) {
                frames.show(index, now);
            }
            let state = match cmd.width {
                1 => Some(State::Stopped),
                2 => Some(State::Loading),
                3 => Some(State::Running),
                _ => None,
            };
            if let Some(state) = state {
                frames.set_state(state, now);
            }
            match cmd.height {
                0 => {}
                1 => frames.set_plays(None, now),
                n => frames.set_plays(Some(n - 1), now),
            }
            Ok(())
        });
        Response { reply: result.err().and_then(|e| answer(cmd, id, None, &e)), ..Response::default() }
    }

    /// `a=c`: a rectangle of frame `r` of the picture the command names, at
    /// `X`, `Y` in it, laid over frame `c` at `x`, `y`, `w` × `h` pixels
    /// (the whole picture for none), in place of what's there with `C=1`.
    /// Answered with ENOENT for a frame that isn't there, and EINVAL for a
    /// rectangle past the picture's edge or one overlapping itself.
    pub(super) fn compose(&mut self, cmd: &Command) -> Response {
        let id = self.named(cmd);
        let result = self.animate(id, |frames, (width, height)| {
            let count = frames.len();
            let index = |n: u32| (n as usize).checked_sub(1).filter(|&i| i < count);
            let from = index(cmd.rows).ok_or("ENOENT:no such source frame")?;
            let to = index(cmd.cols).ok_or("ENOENT:no such destination frame")?;
            let or = |given: u32, whole: u32| if given == 0 { whole } else { given };
            let size = (or(cmd.src[2], width), or(cmd.src[3], height));
            let (at, take) = ((cmd.src[0], cmd.src[1]), (cmd.big_x, cmd.big_y));
            let within = |(x, y): (u32, u32)| x as u64 + size.0 as u64 <= width as u64 && y as u64 + size.1 as u64 <= height as u64;
            if !within(at) {
                return Err("EINVAL:the destination rectangle is out of bounds".into());
            }
            if !within(take) {
                return Err("EINVAL:the source rectangle is out of bounds".into());
            }
            let apart = |a: u32, b: u32, side: u32| a.max(b) >= a.min(b) + side;
            if from == to && !apart(at.0, take.0, size.0) && !apart(at.1, take.1, size.1) {
                return Err("EINVAL:the rectangles overlap in the same frame".into());
            }
            let (Some(source), Some(onto)) = (frames.frame(from).cloned(), frames.frame(to)) else { return Ok(()) };
            let mut canvas = onto.pixels.clone();
            lay(&mut canvas, (width, height), (&source.pixels, width), take, size, at, cmd.stay);
            frames.put(to, Image::new(width, height, canvas));
            Ok(())
        });
        Response { reply: answer(cmd, id, None, &outcome(&result)), ..Response::default() }
    }

    /// `a=d,d=f`: deletes frame `r` of the picture `i` or `I` names, as
    /// kitty does (its spec says no more than "Delete animation frames"):
    /// counted from 1, none given the first, past the last the last; the
    /// frames after it move down, and the frame shown stays shown where
    /// it's still there. A picture of one frame keeps it, and with `d=F`
    /// is deleted whole instead. As every delete, it's never answered.
    /// Every frame Flow keeps is whole, so none is lost with another, as
    /// a frame kitty keeps as a difference could be.
    pub(super) fn delete_frame(&mut self, cmd: &Command) {
        let id = self.named(cmd);
        let Some(count) = self.images.get(&id).filter(|i| i.model.is_none()).map(|i| i.frames.as_ref().map_or(1, Frames::len)) else { return };
        if count < 2 {
            if cmd.delete == b'F' {
                self.forget(id, true);
            }
            return;
        }
        let index = (cmd.rows as usize).clamp(1, count) - 1;
        let now = Instant::now();
        let _ = self.animate(id, |frames, _| {
            frames.remove(index, now);
            Ok(())
        });
    }

    /// Changes the frames of picture `id` with `change`, given its size; a
    /// still one is first made one of a single frame, its own pixels. The
    /// frames changed go into a new picture in its place, and the store's
    /// count of what it holds follows; if `change` fails, nothing changes.
    fn animate(&mut self, id: u32, change: impl FnOnce(&mut Frames, (u32, u32)) -> Result<(), String>) -> Result<(), String> {
        let image = self.images.get(&id).filter(|i| i.model.is_none()).ok_or("ENOENT:no such image")?;
        let size = (image.width, image.height);
        let mut frames = match &image.frames {
            Some(frames) => frames.clone(),
            None => Frames::kitty(Image::new(size.0, size.1, image.pixels.clone())),
        };
        change(&mut frames, size)?;
        let moving = Image::animated(size.0, size.1, frames);
        self.bytes += moving.bytes();
        if let Some(old) = self.images.insert(id, Arc::new(moving)) {
            self.bytes -= old.bytes();
        }
        Ok(())
    }
}

/// A picture `size` pixels all of `rgba` (kitty's 32 bits, red highest and
/// alpha lowest), premultiplied.
fn filled((width, height): (u32, u32), rgba: u32) -> Vec<u8> {
    let mut pixel = rgba.to_be_bytes();
    premultiply(&mut pixel);
    pixel.repeat(width as usize * height as usize)
}

/// Lays the rectangle `size` at `from` in `over` (premultiplied pixels of a
/// picture `over.1` wide) onto `under` (a picture `under_size`) at `at`, as
/// far as `under` goes: in place of what's there with `replace`, else over
/// it.
fn lay(under: &mut [u8], (under_width, under_height): (u32, u32), (over, over_width): (&[u8], u32), from: (u32, u32), (w, h): (u32, u32), at: (u32, u32), replace: bool) {
    let cols = w.min(under_width.saturating_sub(at.0)) as usize;
    let rows = h.min(under_height.saturating_sub(at.1)) as usize;
    for row in 0..rows {
        let taken = ((from.1 as usize + row) * over_width as usize + from.0 as usize) * 4;
        let into = ((at.1 as usize + row) * under_width as usize + at.0 as usize) * 4;
        let (taken, into) = (&over[taken..taken + cols * 4], &mut under[into..into + cols * 4]);
        if replace {
            into.copy_from_slice(taken);
            continue;
        }
        for (below, above) in into.as_chunks_mut::<4>().0.iter_mut().zip(taken.as_chunks::<4>().0) {
            let through = 255 - above[3] as u16;
            for (b, a) in below.iter_mut().zip(above) {
                *b = (*a as u16 + (*b as u16 * through + 127) / 255).min(255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{b64, CTX};
    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];

    /// A store holding picture 1, 2 × 2 pixels of `color`, placed.
    fn store_of(color: [u8; 4]) -> ImageStore {
        let mut store = ImageStore::default();
        let r = store.handle(format!("a=T,f=32,s=2,v=2,i=1,q=2;{}", b64(&color.repeat(4))).as_bytes(), CTX);
        assert!(r.place.is_some());
        store
    }

    /// Picture 1's frames.
    fn frames(store: &ImageStore) -> &Frames {
        store.images[&1].frames.as_ref().expect("moving")
    }

    /// Frame `n` (from 1) of picture 1: its pixels, by rows.
    fn pixels(store: &ImageStore, n: usize) -> Vec<[u8; 4]> {
        frames(store).frame(n - 1).unwrap().pixels.as_chunks::<4>().0.to_vec()
    }

    #[test]
    fn a_frame_sent_with_a_f_is_a_new_one_after_the_last_and_answered_with_its_number() {
        let mut store = store_of(RED);
        let still = store.images[&1].serial;
        let r = store.handle(format!("a=f,i=1,f=32,s=2,v=2;{}", b64(&BLUE.repeat(4))).as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=1,r=2;OK\x1b\\"));
        assert_eq!((frames(&store).len(), pixels(&store, 1)[0], pixels(&store, 2)[3]), (2, RED, BLUE));
        // Stopped on its first frame, the picture's own, as kitty's are until
        // a program plays them; under its id, in place of the still one.
        let (image, _) = store.lookup(1, 0).unwrap();
        let (shown, next) = image.frame_at(Instant::now()).unwrap();
        assert_eq!((shown.pixels[..4].to_vec(), next, frames(&store).state()), (RED.to_vec(), None, State::Stopped));
        assert_ne!(image.serial, still);
        // The first frame has no gap until given one; a new one's is 40 ms.
        assert_eq!((frames(&store).gap(0), frames(&store).gap(1)), (Some(Duration::ZERO), Some(GAP)));
        // A frame past the last is the next; quiet, it's not answered.
        let r = store.handle(format!("a=f,i=1,r=9,s=1,v=1,z=70;{}", b64(&BLUE)).as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=1,r=3;OK\x1b\\"));
        assert_eq!(frames(&store).gap(2), Some(Duration::from_millis(70)));
        assert_eq!(store.handle(format!("a=f,i=1,q=1,z=-5;{}", b64(&BLUE.repeat(4))).as_bytes(), CTX).reply, None);
        assert_eq!(frames(&store).gap(3), Some(Duration::ZERO), "gapless");
        // By its number, as a=p finds one.
        let r = store.handle(format!("a=t,f=24,s=1,v=1,I=4;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        let id = store.numbers[&4];
        assert_eq!(r.reply, Some(format!("\x1b_Gi={id},I=4;OK\x1b\\")));
        let r = store.handle(format!("a=f,I=4,f=24;{}", b64(&[4, 5, 6])).as_bytes(), CTX);
        assert_eq!(r.reply, Some(format!("\x1b_Gi={id},I=4,r=2;OK\x1b\\")));
    }

    #[test]
    fn a_frame_sent_in_chunks_comes_whole_at_its_end() {
        let mut store = store_of(RED);
        let data = b64(&BLUE.repeat(4));
        let (a, b) = data.split_at(8);
        assert_eq!(store.handle(format!("a=f,i=1,s=2,v=2,m=1;{a}").as_bytes(), CTX), Response::default());
        assert!(store.images[&1].frames.is_none(), "not until its end");
        // Kitty's later chunks name a=f again, which changes nothing.
        let r = store.handle(format!("a=f,m=0;{b}").as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=1,r=2;OK\x1b\\"));
        assert_eq!(pixels(&store, 2), [BLUE; 4]);
    }

    #[test]
    fn a_frame_is_laid_over_a_frame_before_it_or_a_color_or_in_place_of_it() {
        let mut store = store_of(RED);
        let half_blue = b64(&[0, 0, 255, 128]);
        // One pixel at (1, 0), on transparent, on the first frame, and on it
        // in place of what's there.
        for keys in ["", ",c=1", ",c=1,X=1"] {
            store.handle(format!("a=f,i=1,x=1,y=0,s=1,v=1{keys};{half_blue}").as_bytes(), CTX);
        }
        let blue = [0, 0, 128, 128];
        assert_eq!(pixels(&store, 2), [[0; 4], blue, [0; 4], [0; 4]]);
        assert_eq!(pixels(&store, 3), [RED, [127, 0, 128, 255], RED, RED], "blended over red");
        assert_eq!(pixels(&store, 4), [RED, blue, RED, RED]);
        // On a color of its own: opaque red, and green part see-through,
        // premultiplied as every picture is.
        store.handle(format!("a=f,i=1,x=1,y=1,s=1,v=1,Y=4278190335;{half_blue}").as_bytes(), CTX);
        assert_eq!(pixels(&store, 5)[..3], [RED; 3]);
        store.handle(format!("a=f,i=1,s=1,v=1,X=1,Y=16711816;{half_blue}").as_bytes(), CTX);
        assert_eq!(pixels(&store, 6), [blue, [0, 136, 0, 136], [0, 136, 0, 136], [0, 136, 0, 136]]);
        // Raw pixels sent with no size fill the picture from where they go;
        // past its edge they're cut off.
        store.handle(format!("a=f,i=1,x=1,f=24;{}", b64(&[0, 0, 255].repeat(2))).as_bytes(), CTX);
        assert_eq!(pixels(&store, 7), [[0; 4], BLUE, [0; 4], BLUE]);
        store.handle(format!("a=f,i=1,x=1,y=1,s=2,v=2,c=1;{}", b64(&BLUE.repeat(4))).as_bytes(), CTX);
        assert_eq!(pixels(&store, 8), [RED, RED, RED, BLUE]);
    }

    #[test]
    fn editing_a_frame_with_r_lays_its_data_over_what_it_was_and_shows_at_once() {
        let mut store = store_of(RED);
        let shown = |store: &ImageStore| store.lookup(1, 0).unwrap().0.frame_at(Instant::now()).unwrap().0.clone();
        store.handle(format!("a=f,i=1,q=2;{}", b64(&BLUE.repeat(4))).as_bytes(), CTX);
        let before = shown(&store);
        let r = store.handle(format!("a=f,i=1,r=1,x=1,y=1,s=1,v=1,X=1,z=500;{}", b64(&BLUE)).as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=1,r=1;OK\x1b\\"));
        // The frame showing (stopped on the first) is a new picture of the
        // same animation, so the renderer writes it over the old.
        let after = shown(&store);
        assert_eq!(after.pixels.as_chunks::<4>().0, [RED, RED, RED, BLUE]);
        assert!(after.serial != before.serial && after.texture() == before.texture());
        assert_eq!((frames(&store).len(), frames(&store).gap(0)), (2, Some(Duration::from_millis(500))));
        // Its base names no frame of its own: the frame edited is the base.
        store.handle(format!("a=f,i=1,r=2,c=1,s=1,v=1,X=1;{}", b64(&RED)).as_bytes(), CTX);
        assert_eq!(pixels(&store, 2), [RED, BLUE, BLUE, BLUE]);
    }

    #[test]
    fn a_frame_too_big_for_its_picture_or_naming_what_isnt_there_is_refused_and_changes_nothing() {
        let mut store = store_of(RED);
        let one = b64(&BLUE);
        let errors = [
            (format!("a=f,i=9,s=1,v=1;{one}"), "\x1b_Gi=9;ENOENT:no such image\x1b\\"),
            (format!("a=f,i=1,c=5,s=1,v=1;{one}"), "\x1b_Gi=1,r=2;ENOENT:no such frame\x1b\\"),
            (format!("a=f,i=1,s=3,v=1;{}", b64(&BLUE.repeat(3))), "\x1b_Gi=1,r=2;EINVAL:a frame larger than its picture\x1b\\"),
            (format!("a=f,i=1,s=2,v=2;{one}"), "\x1b_Gi=1,r=2;ENODATA:not enough pixel data\x1b\\"),
        ];
        for (sent, heard) in errors {
            assert_eq!(store.handle(sent.as_bytes(), CTX).reply.as_deref(), Some(heard), "{sent}");
        }
        assert!(store.images[&1].frames.is_none(), "still as it was");
        assert_eq!(store.bytes, 16);
        // Quiet for errors too, and without an id nothing is answered.
        assert_eq!(store.handle(format!("a=f,i=9,q=2;{one}").as_bytes(), CTX).reply, None);
        assert_eq!(store.handle(format!("a=f;{one}").as_bytes(), CTX).reply, None);
    }

    #[test]
    fn playing_with_a_a_runs_stops_loops_and_shows_a_frame_answering_only_errors() {
        let mut store = store_of(RED);
        store.handle(format!("a=f,i=1,q=2,z=400;{}", b64(&BLUE.repeat(4))).as_bytes(), CTX);
        // Gapless at first, the first frame is passed over: only the blue
        // plays, round and round.
        assert_eq!(store.handle(b"a=a,i=1,s=3", CTX).reply, None, "kitty answers only what goes wrong");
        let start = Instant::now();
        let at = |store: &ImageStore, ms: u64| {
            let (image, _) = store.lookup(1, 0).unwrap();
            let (frame, next) = image.frame_at(start + Duration::from_millis(ms)).unwrap();
            (frame.pixels[..4].to_vec(), next.is_some())
        };
        assert_eq!((at(&store, 100), at(&store, 900)), ((BLUE.to_vec(), true), (BLUE.to_vec(), true)));
        assert_eq!(frames(&store).state(), State::Running);
        // Given a gap, the first shows in its turn.
        store.handle(b"a=a,i=1,r=1,z=400", CTX);
        assert_eq!(frames(&store).gap(0), Some(Duration::from_millis(400)));
        // Shown and stopped on the first, it stays there.
        store.handle(b"a=a,i=1,c=1,s=1", CTX);
        assert_eq!((at(&store, 100), at(&store, 5000), frames(&store).state()), ((RED.to_vec(), false), (RED.to_vec(), false), State::Stopped));
        // Played twice from the first: red, blue, red, blue, then blue
        // stays; then forever, as v=1 asks.
        store.handle(b"a=a,i=1,s=3,v=3", CTX);
        assert_eq!(frames(&store).plays(), Some(2));
        let start = Instant::now();
        let at = |store: &ImageStore, ms: u64| store.lookup(1, 0).unwrap().0.frame_at(start + Duration::from_millis(ms)).unwrap().0.pixels[..4].to_vec();
        assert_eq!([at(&store, 200), at(&store, 600), at(&store, 1000), at(&store, 1400), at(&store, 5000)], [RED, BLUE, RED, BLUE, BLUE].map(|c| c.to_vec()));
        store.handle(b"a=a,i=1,v=1", CTX);
        assert_eq!(frames(&store).plays(), None);
        // Loading: held at its last frame until more come.
        store.handle(b"a=a,i=1,s=2", CTX);
        assert_eq!(frames(&store).state(), State::Loading);
        // A frame that isn't there is passed over; an image that isn't is
        // an error, unless the program asked for quiet.
        assert_eq!(store.handle(b"a=a,i=1,c=9,r=9,z=5", CTX).reply, None);
        assert_eq!(store.handle(b"a=a,i=8,s=3", CTX).reply.as_deref(), Some("\x1b_Gi=8;ENOENT:no such image\x1b\\"));
        assert_eq!(store.handle(b"a=a,i=8,s=3,q=2", CTX).reply, None);
    }

    #[test]
    fn a_still_picture_played_is_one_frame_that_keeps_its_place() {
        let mut store = store_of(RED);
        let placement = *store.placements.keys().next().unwrap();
        store.handle(b"a=a,i=1,s=3,v=1", CTX);
        assert_eq!((frames(&store).len(), frames(&store).state(), store.bytes), (1, State::Running, 16));
        let (image, at) = store.lookup(placement.0, placement.1).unwrap();
        assert_eq!((image.frame_at(Instant::now()).unwrap().1, at.cols), (None, 1), "nothing to play: no frames come for it");
        // A virtual placement, whose cells the program writes, finds it too.
        store.handle(b"a=p,i=1,U=1,p=7,c=3,r=2,q=2", CTX);
        assert!(store.lookup(1, 7).unwrap().0.frames.is_some());
    }

    #[test]
    fn composing_with_a_c_lays_a_rectangle_of_one_frame_over_another() {
        let mut store = store_of(RED);
        store.handle(format!("a=f,i=1,q=2;{}", b64(&[BLUE, [0, 0, 255, 128], BLUE, BLUE].concat())).as_bytes(), CTX);
        // Frame 2's top right pixel, half blue, over the first's bottom left.
        let r = store.handle(b"a=c,i=1,r=2,c=1,x=0,y=1,X=1,Y=0,w=1,h=1", CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=1;OK\x1b\\"));
        assert_eq!(pixels(&store, 1), [RED, RED, [127, 0, 128, 255], RED]);
        // In place of what's there, and the whole frame for no size.
        store.handle(b"a=c,i=1,r=2,c=1,X=1,Y=0,w=1,h=1,C=1,q=1", CTX);
        assert_eq!(pixels(&store, 1)[0], [0, 0, 128, 128]);
        store.handle(b"a=c,i=1,r=2,c=1,C=1", CTX);
        assert_eq!(pixels(&store, 1), pixels(&store, 2));
        // Within one frame, rectangles apart.
        store.handle(b"a=c,i=1,r=2,c=2,x=1,X=0,w=1,C=1", CTX);
        assert_eq!(pixels(&store, 2), [BLUE, BLUE, BLUE, BLUE]);
        let errors = [
            ("a=c,i=1,r=3,c=1", "\x1b_Gi=1;ENOENT:no such source frame\x1b\\"),
            ("a=c,i=1,r=1", "\x1b_Gi=1;ENOENT:no such destination frame\x1b\\"),
            ("a=c,i=1,r=1,c=2,x=1", "\x1b_Gi=1;EINVAL:the destination rectangle is out of bounds\x1b\\"),
            ("a=c,i=1,r=1,c=2,w=1,h=1,X=2", "\x1b_Gi=1;EINVAL:the source rectangle is out of bounds\x1b\\"),
            ("a=c,i=1,r=2,c=2,x=1,y=1,X=0,Y=0,w=2,h=1", "\x1b_Gi=1;EINVAL:the destination rectangle is out of bounds\x1b\\"),
            ("a=c,i=1,r=2,c=2,x=0,y=1,X=0,Y=0,w=2,h=1,C=1", "\x1b_Gi=1;OK\x1b\\"),
            ("a=c,i=1,r=2,c=2,x=1,y=0,X=0,Y=0,w=2,h=1", "\x1b_Gi=1;EINVAL:the destination rectangle is out of bounds\x1b\\"),
            ("a=c,i=1,r=1,c=1,x=1,y=1,X=0,Y=0,w=1,h=1", "\x1b_Gi=1;OK\x1b\\"),
            ("a=c,i=1,r=1,c=1,x=0,y=0,X=0,Y=0,w=2,h=1", "\x1b_Gi=1;EINVAL:the rectangles overlap in the same frame\x1b\\"),
            ("a=c,i=9,r=1,c=1", "\x1b_Gi=9;ENOENT:no such image\x1b\\"),
        ];
        for (sent, heard) in errors {
            assert_eq!(store.handle(sent.as_bytes(), CTX).reply.as_deref(), Some(heard), "{sent}");
        }
    }

    #[test]
    fn every_frame_counts_in_the_quota_and_goes_with_its_picture() {
        let mut store = store_of(RED);
        for _ in 0..3 {
            store.handle(format!("a=f,i=1,q=2;{}", b64(&BLUE.repeat(4))).as_bytes(), CTX);
        }
        // Four frames of 16 bytes, through one texture of one frame's size.
        let (bytes, held) = store.held();
        assert_eq!((bytes, held), (64, vec![(frames(&store).reel(), 2, 2)]));
        // Sent anew under its id, it's still again, its frames gone.
        store.handle(format!("a=t,f=32,s=1,v=1,i=1,q=2;{}", b64(&RED)).as_bytes(), CTX);
        assert_eq!((store.bytes, store.images[&1].frames.is_none()), (4, true));
        store.handle(format!("a=f,i=1,q=2;{}", b64(&BLUE)).as_bytes(), CTX);
        store.handle(b"a=d,d=I,i=1", CTX);
        assert_eq!((store.bytes, store.held().1.len()), (0, 0));
        // No more than an animation may hold, in frames or in pixels.
        assert!(has_room(MAX_FRAMES - 1, 0, 4) && !has_room(MAX_FRAMES, 0, 4));
        assert!(has_room(1, MAX_DATA - 4, 4) && !has_room(1, MAX_DATA - 3, 4));
    }

    #[test]
    fn deleting_a_frame_with_d_f_moves_those_after_it_down_and_keeps_the_last() {
        const GREEN: [u8; 4] = [0, 255, 0, 255];
        let mut store = store_of(RED);
        for color in [BLUE, GREEN] {
            store.handle(format!("a=f,i=1,q=2;{}", b64(&color.repeat(4))).as_bytes(), CTX);
        }
        let firsts = |store: &ImageStore| (0..frames(store).len()).map(|i| frames(store).frame(i).unwrap().pixels[..4].to_vec()).collect::<Vec<_>>();
        // Frame 2, blue: green moves down; nothing's answered.
        assert_eq!(store.handle(b"a=d,d=f,i=1,r=2", CTX), Response::default());
        assert_eq!(firsts(&store), [RED.to_vec(), GREEN.to_vec()]);
        assert_eq!(store.bytes, 32, "the quota counts what's left");
        // None given is the first, which the next takes the place of.
        store.handle(b"a=d,d=f,i=1", CTX);
        assert_eq!(firsts(&store), [GREEN.to_vec()]);
        // The last left stays, unless d=F deletes the picture whole.
        store.handle(b"a=d,d=f,i=1,r=1", CTX);
        assert_eq!(firsts(&store), [GREEN.to_vec()]);
        store.handle(b"a=d,d=F,i=1", CTX);
        assert!(!store.images.contains_key(&1) && store.lookup(1, 0).is_none());
        assert_eq!(store.bytes, 0);
        // A still picture is one frame too; one that isn't there is passed over.
        let mut store = store_of(RED);
        store.handle(b"a=d,d=f,i=1", CTX);
        assert!(store.images.contains_key(&1));
        store.handle(b"a=d,d=f,i=7", CTX);
    }

    #[test]
    fn the_frame_shown_stays_shown_as_others_are_deleted() {
        const GREEN: [u8; 4] = [0, 255, 0, 255];
        let mut store = store_of(RED);
        for color in [BLUE, GREEN] {
            store.handle(format!("a=f,i=1,q=2;{}", b64(&color.repeat(4))).as_bytes(), CTX);
        }
        let shown = |store: &ImageStore| store.lookup(1, 0).unwrap().0.frame_at(Instant::now()).unwrap().0.pixels[..4].to_vec();
        // Stopped on green, the third: a frame before it goes, green stays.
        store.handle(b"a=a,i=1,c=3,s=1", CTX);
        store.handle(b"a=d,d=f,i=1,r=1", CTX);
        assert_eq!(shown(&store), GREEN.to_vec());
        // Green itself, the last: the one before it, now the last, shows.
        store.handle(b"a=d,d=f,i=1,r=2", CTX);
        assert_eq!(shown(&store), BLUE.to_vec());
        // Past the last is the last.
        store.handle(format!("a=f,i=1,q=2;{}", b64(&GREEN.repeat(4))).as_bytes(), CTX);
        store.handle(b"a=a,i=1,c=1,s=1", CTX);
        store.handle(b"a=d,d=f,i=1,r=9", CTX);
        assert_eq!((frames(&store).len(), shown(&store)), (1, BLUE.to_vec()));
    }

    #[test]
    fn as_many_frames_as_an_animation_keeps_and_no_more() {
        let mut store = store_of(RED);
        let one = b64(&BLUE);
        for _ in 1..MAX_FRAMES {
            store.handle(format!("a=f,i=1,s=1,v=1,q=2;{one}").as_bytes(), CTX);
        }
        assert_eq!(frames(&store).len(), MAX_FRAMES);
        let r = store.handle(format!("a=f,i=1,s=1,v=1;{one}").as_bytes(), CTX);
        assert_eq!(r.reply, Some(format!("\x1b_Gi=1,r={};ENOSPC:no room for another frame\x1b\\", MAX_FRAMES + 1)));
        assert_eq!(store.bytes, MAX_FRAMES * 16);
    }
}
