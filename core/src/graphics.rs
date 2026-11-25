//! The kitty graphics protocol: programs send images in APC escapes
//! (`ESC _G <keys>;<base64> ESC \`), and Flow shows them in the terminal.
//!
//! Every image on screen is a block of Unicode placeholder cells (U+10EEEE,
//! the protocol's own way through tmux), whether the program wrote them or
//! Flow did for it: the image id rides in the cells' foreground color, the
//! placement id in their underline color, and the row and column in combining
//! marks. So images scroll, clear and go to scrollback just as text does,
//! and one drawing path shows them all. The spec:
//! <https://sw.kovidgoyal.net/kitty/graphics-protocol/>.
//!
//! Text rewraps when the grid narrows, which would split a picture's rows,
//! so the main screen's are cut at the new edge instead
//! ([`crop_pictures`]) and filled out again as it widens
//! ([`regrow_pictures`]).
//!
//! Pictures sent another way, a sixel (`crate::sixel`) or an iTerm2 inline
//! image (`crate::iterm`, whole or in parts), are kept here too, under ids
//! of Flow's own, and placed at the cursor as kitty's are
//! ([`ImageStore::show`]). Nothing names them again, so once none of their
//! cells can show they're let go ([`ImageStore::let_go_unseen`]), not only
//! when the quota is reached.
//!
//! A GIF or a WebP that moves (`crate::frames`) is one picture holding its
//! frames, each a picture of its own ([`Image::frames`]), every frame
//! counted in the quota, and all of them shown through one texture (their
//! reel, [`Image::reel`]). Kitty's own animation commands (`a=f`, `a=a`,
//! `a=c`, in [`animation`]) make a picture one of frames the same way and
//! play it.

mod animation;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Grid, GridCell, Row};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, Rgb};
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::Engine;

use crate::line_size::{self, LineSize};

/// The character a placeholder cell holds.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// Combining marks that number a placeholder's row and column, and carry the
/// top byte of a large image id: kitty's table, in order, so placeholders
/// that programs write mean the same here.
const DIACRITICS: [u32; 297] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F, 0x1B70, 0x1B71,
    0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5,
    0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7,
    0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE,
    0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1,
    0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186, 0x1D187,
    0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243, 0x1D244,
];

/// The largest side of an image, in pixels (as in kitty).
pub(crate) const MAX_SIDE: u32 = 10_000;
/// Image data one terminal keeps at most (as in kitty); the oldest images,
/// those no longer shown first, make room for new ones.
const QUOTA: usize = 320 << 20;
/// The most one image's data may grow to while it's sent.
pub(crate) const MAX_DATA: usize = 128 << 20;
/// The most cells an image may span each way.
const MAX_CELLS: u32 = 1000;

/// Ids for images and placements Flow numbers itself, kept clear of the
/// small numbers programs pick (placement ids fit 24 bits).
const OWN_IDS: u32 = 0x8000_0000;
const OWN_PLACEMENTS: u32 = 0x00F0_0000;

static SERIAL: AtomicU64 = AtomicU64::new(1);

/// Serials of images that are no longer anywhere (sent again under their
/// id, deleted, or scrolled out of the store), whose textures the renderer
/// frees at its next frame (`take_gone`) rather than after it hasn't drawn
/// them for a while: a picture sent anew each frame, as an animation is,
/// would otherwise keep seconds of its frames on the GPU. At most
/// `GONE_KEPT` wait, should frames stop; one left out goes the usual way.
static GONE: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
const GONE_KEPT: usize = 4096;

/// The serials of the images gone since the last call.
pub fn take_gone() -> Vec<u64> {
    std::mem::take(&mut *GONE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Tells the renderer that the texture known by `serial` can go.
pub(crate) fn note_gone(serial: u64) {
    let mut gone = GONE.lock().unwrap_or_else(|e| e.into_inner());
    if gone.len() < GONE_KEPT {
        gone.push(serial);
    }
}

/// A serial no image has had: an animation's reel takes one too, so its
/// texture's id is never a picture's own.
pub(crate) fn next_serial() -> u64 {
    SERIAL.fetch_add(1, Ordering::Relaxed)
}

/// A decoded image: premultiplied RGBA, 8 bits a channel.
pub struct Image {
    /// Unique across all terminals, so the GPU's copy can be found by it.
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// A 3D model placed by a program (`crate::model`), which the renderer
    /// draws into it rather than taking `pixels`.
    pub model: Option<Arc<crate::model::Shown>>,
    /// An animation's frames (a GIF's, a WebP's, kitty's), each a picture
    /// of this one's size, drawn in its place as [`Image::frame_at`] says;
    /// its own `pixels` are none.
    pub frames: Option<crate::frames::Frames>,
    /// For a frame of an animation, the id of the animation's one texture
    /// (`Frames::reel`), which shows whichever frame is drawn.
    pub(crate) reel: Option<u64>,
}

impl Image {
    /// An image from premultiplied RGBA pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self { serial: next_serial(), width, height, pixels, model: None, frames: None, reel: None }
    }

    /// The id of the texture it's shown through: its own, or for a frame
    /// its animation's, which every frame of it shares.
    pub fn texture(&self) -> u64 {
        self.reel.unwrap_or(self.serial)
    }

    /// A picture `width` × `height` pixels that moves, shown as `frames`.
    pub fn animated(width: u32, height: u32, frames: crate::frames::Frames) -> Self {
        let mut image = Self::new(width, height, Vec::new());
        image.frames = Some(frames);
        image
    }

    /// What it holds, decoded: its pixels, or its frames'.
    pub fn bytes(&self) -> usize {
        self.pixels.len() + self.frames.as_ref().map_or(0, |f| f.each().map(|frame| frame.pixels.len()).sum())
    }

    /// The frame an animation shows at `now`, and when it gives way to the
    /// next (none once it's played out); none for a picture that's still.
    pub fn frame_at(&self, now: std::time::Instant) -> Option<(&Arc<Image>, Option<std::time::Instant>)> {
        self.frames.as_ref().map(|f| f.at(now))
    }

    /// A picture `width` × `height` pixels of a placed model.
    pub fn model(width: u32, height: u32, shown: Arc<crate::model::Shown>) -> Self {
        let mut image = Self::new(width, height, Vec::new());
        image.model = Some(shown);
        image
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // A frame, or the picture holding them, has no texture of its own:
        // the animation's goes with its reel.
        if self.reel.is_none() && self.frames.is_none() {
            note_gone(self.serial);
        }
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image #{} {}×{}", self.serial, self.width, self.height)?;
        match &self.frames {
            Some(frames) => write!(f, " of {} frames", frames.len()),
            None => Ok(()),
        }
    }
}

/// Where an image shows: over `cols` × `rows` cells, the part `src` of it (x,
/// y, w, h in its pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub cols: u32,
    pub rows: u32,
    pub src: [u32; 4],
    /// Shown at its own pixel size (no cells asked for), never enlarged.
    pub natural: bool,
    /// Stretched to fill its cells exactly, however their shape differs
    /// from its own: Flow's own picture on a page of its color (a start
    /// screen's `{disk}`), a hair off its shape rather than a sliver short
    /// of the page round it.
    pub fills: bool,
}

/// One graphics command, as its keys say. Unknown keys are left out. Kitty's
/// animation commands read some keys as other things (its spec's tables):
/// for `a=f` (a frame), `x` and `y` are where its data goes, `s` and `v` the
/// data's size, `r` the frame it edits and `c` the frame a new one starts
/// from; for `a=a` (playing), `s` is how it plays, `v` how many times, `r`
/// the frame whose gap `z` gives and `c` the frame to show; for `a=c` (a
/// rectangle of one frame laid over another), `r` is the frame it's from,
/// `c` the one it goes on, `x`, `y`, `w` and `h` where it goes and its size,
/// and `C=1` that it replaces what's there.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    /// `a`: t transmit, T transmit and show, p show, d delete, q query, f
    /// an animation's frame, a play an animation, c compose its frames.
    pub action: u8,
    /// `q`: 1 keeps OK replies back, 2 errors too.
    pub quiet: u32,
    /// `f`: 24 RGB, 32 RGBA, 100 PNG.
    pub format: u32,
    /// `t`: d in the escape, f a file, t a temporary file, s shared memory.
    pub medium: u8,
    /// `o=z`: zlib-compressed.
    pub compressed: bool,
    /// `s`, `v`: the size of raw pixels.
    pub width: u32,
    pub height: u32,
    /// `S`, `O`: how much of a file to read, and from where.
    pub size: u32,
    pub offset: u32,
    /// `i`, `I`, `p`: image id, image number, placement id.
    pub id: u32,
    pub number: u32,
    pub placement: u32,
    /// `m=1`: more chunks follow.
    pub more: bool,
    /// `x`, `y`, `w`, `h`: the part of the image to show.
    pub src: [u32; 4],
    /// `c`, `r`: the cells to show it over.
    pub cols: u32,
    pub rows: u32,
    /// `C=1`: the cursor stays where it is.
    pub stay: bool,
    /// `U=1`: a virtual placement, for placeholders the program writes.
    pub virtual_: bool,
    /// `d`: what to delete.
    pub delete: u8,
    /// `z`: an animation frame's gap in milliseconds, below zero none (a
    /// gapless frame), 0 left as it is. (A placement's z-index, which Flow
    /// doesn't heed yet.)
    pub z: i32,
    /// `X`, `Y`: for a frame (`a=f`), 1 laying it in place of what's there
    /// rather than over it, and a new frame's background (RGBA, 32 bits);
    /// for `a=c`, the left and top of the rectangle taken. (A placement's
    /// offset within its cell, which Flow doesn't heed yet.)
    pub big_x: u32,
    pub big_y: u32,
    /// The payload, still base64.
    pub data: Vec<u8>,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            action: b't',
            quiet: 0,
            format: 32,
            medium: b'd',
            compressed: false,
            width: 0,
            height: 0,
            size: 0,
            offset: 0,
            id: 0,
            number: 0,
            placement: 0,
            more: false,
            src: [0; 4],
            cols: 0,
            rows: 0,
            stay: false,
            virtual_: false,
            delete: b'a',
            z: 0,
            big_x: 0,
            big_y: 0,
            data: Vec::new(),
        }
    }
}

/// Reads a command from an APC body after its `G`: `key=value,...;payload`.
pub fn parse(body: &[u8]) -> Option<Command> {
    let (keys, data) = match body.iter().position(|&b| b == b';') {
        Some(i) => (&body[..i], &body[i + 1..]),
        None => (body, &[][..]),
    };
    let mut cmd = Command { data: data.to_vec(), ..Command::default() };
    for pair in keys.split(|&b| b == b',').filter(|p| !p.is_empty()) {
        let [key, b'=', value @ ..] = pair else { return None };
        let signed = || std::str::from_utf8(value).ok().and_then(|v| v.parse::<i64>().ok());
        let number = || signed().map(|n| n.clamp(0, u32::MAX as i64) as u32);
        let letter = value.first().copied();
        match *key {
            b'a' => cmd.action = letter?,
            b't' => cmd.medium = letter?,
            b'o' => cmd.compressed = letter == Some(b'z'),
            b'd' => cmd.delete = letter?,
            b'q' => cmd.quiet = number()?,
            b'f' => cmd.format = number()?,
            b's' => cmd.width = number()?,
            b'v' => cmd.height = number()?,
            b'S' => cmd.size = number()?,
            b'O' => cmd.offset = number()?,
            b'i' => cmd.id = number()?,
            b'I' => cmd.number = number()?,
            b'p' => cmd.placement = number()?,
            b'm' => cmd.more = number()? == 1,
            b'x' => cmd.src[0] = number()?,
            b'y' => cmd.src[1] = number()?,
            b'w' => cmd.src[2] = number()?,
            b'h' => cmd.src[3] = number()?,
            b'c' => cmd.cols = number()?,
            b'r' => cmd.rows = number()?,
            b'C' => cmd.stay = number()? == 1,
            b'U' => cmd.virtual_ = number()? == 1,
            b'z' => cmd.z = signed()?.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            b'X' => cmd.big_x = number()?,
            b'Y' => cmd.big_y = number()?,
            // Relative placement: not yet.
            _ => {}
        }
    }
    Some(cmd)
}

/// What the terminal needs to know to place an image at the cursor.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    /// A cell's size in pixels.
    pub cell: (u32, u32),
    /// Columns from the cursor to the right edge.
    pub cols_left: u32,
}

/// An image to write into the grid at the cursor as placeholder cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub id: u32,
    pub placement: u32,
    pub cols: u32,
    pub rows: u32,
    /// The cursor goes back to where it was (`C=1`).
    pub stay: bool,
}

/// What a command leads to: a reply for the program, an image to place, and
/// whether to delete the pictures on screen.
#[derive(Debug, Default, PartialEq)]
pub struct Response {
    pub reply: Option<String>,
    pub place: Option<Place>,
    /// `d=a` (false) or `d=A` (true, freeing their data too): the pictures
    /// on screen go, which only the grid knows (`ImageStore::delete_shown`).
    pub clear_screen: Option<bool>,
}

/// A transmission still coming in chunks.
struct Pending {
    cmd: Command,
    data: Vec<u8>,
    error: Option<String>,
}

/// An iTerm2 file still coming in parts (`MultipartFile=`): its arguments
/// and its base64 so far, up to [`MAX_DATA`] of it, past which it's too big
/// and none of it is kept.
struct FileParts {
    args: crate::iterm::Args,
    base64: Vec<u8>,
    too_big: bool,
}

/// The screen a picture [`ImageStore::show`] placed went on: the main one,
/// whose scrollback keeps it, or the alternate one, which keeps nothing
/// once it's left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Alternate,
}

/// One terminal's images and where they show. The reader thread adds to it as
/// commands come; the drawing reads it.
#[derive(Default)]
pub struct ImageStore {
    images: HashMap<u32, Arc<Image>>,
    /// Image numbers (`I`) to the ids given for them.
    numbers: HashMap<u32, u32>,
    /// By image, then placement: a cell naming no placement takes the
    /// image's first, found without looking through the others.
    placements: BTreeMap<(u32, u32), Placement>,
    /// Image ids, oldest first, for making room.
    order: Vec<u32>,
    bytes: usize,
    pending: Option<Pending>,
    /// An iTerm2 file coming in parts.
    file: Option<FileParts>,
    /// The pictures [`ImageStore::show`] placed, which nothing names again,
    /// and the screen each went on: let go once none of their cells show.
    shown_on: HashMap<u32, Screen>,
    /// What the store held after the main screen's last sweep.
    swept_at: usize,
    /// The transmissions naming a file that failed lately.
    file_failures: FileFailures,
    next_id: u32,
    next_placement: u32,
}

impl ImageStore {
    /// Carries out the command in an APC body (after its `G`).
    pub fn handle(&mut self, body: &[u8], ctx: Context) -> Response {
        let Some(cmd) = parse(body) else { return Response::default() };
        // The chunks after the first carry only m and q: they belong to it.
        let cmd = match self.pending.take() {
            Some(mut pending) => {
                if pending.error.is_none() {
                    if let Err(e) = decode_base64(&cmd.data, &mut pending.data) {
                        pending.error = Some(e);
                    }
                }
                if cmd.more {
                    self.pending = Some(pending);
                    return Response::default();
                }
                let Pending { cmd: first, data, error } = pending;
                return self.finish(first, Some(data), error, ctx);
            }
            None => cmd,
        };
        if cmd.more && matches!(cmd.action, b't' | b'T' | b'q' | b'f') {
            let mut data = Vec::new();
            let error = decode_base64(&cmd.data, &mut data).err();
            self.pending = Some(Pending { cmd, data, error });
            return Response::default();
        }
        self.finish(cmd, None, None, ctx)
    }

    fn finish(&mut self, cmd: Command, data: Option<Vec<u8>>, error: Option<String>, ctx: Context) -> Response {
        if let Some(e) = error {
            return Response { reply: reply(&cmd, cmd.id, &e), ..Response::default() };
        }
        match cmd.action {
            b'q' => {
                let result = load(&cmd, data, &mut self.file_failures).map(|_| ());
                Response { reply: reply(&cmd, cmd.id, &outcome(&result)), ..Response::default() }
            }
            b't' | b'T' => {
                let id = match self.store(&cmd, data) {
                    Ok(id) => id,
                    Err(e) => return Response { reply: reply(&cmd, cmd.id, &e), ..Response::default() },
                };
                if cmd.action == b't' {
                    return Response { reply: reply(&cmd, id, "OK"), ..Response::default() };
                }
                let placed = self.place(&cmd, id, ctx);
                Response { reply: reply(&cmd, id, &outcome(&placed)), place: placed.ok().flatten(), clear_screen: None }
            }
            b'p' => {
                let id = self.named(&cmd);
                let placed = self.place(&cmd, id, ctx);
                Response { reply: reply(&cmd, id, &outcome(&placed)), place: placed.ok().flatten(), clear_screen: None }
            }
            b'f' => self.frame(cmd, data),
            b'a' => self.play(&cmd),
            b'c' => self.compose(&cmd),
            b'd' if cmd.delete.eq_ignore_ascii_case(&b'a') => Response { clear_screen: Some(cmd.delete == b'A'), ..Response::default() },
            b'd' => {
                self.delete(&cmd);
                Response::default()
            }
            _ => Response { reply: reply(&cmd, cmd.id, "EINVAL:unsupported action"), ..Response::default() },
        }
    }

    /// The image a command names, by its id or by its number (`I`); 0 for
    /// none.
    fn named(&self, cmd: &Command) -> u32 {
        match cmd.id {
            0 => self.numbers.get(&cmd.number).copied().unwrap_or(0),
            id => id,
        }
    }

    /// Decodes and keeps an image; returns its id.
    fn store(&mut self, cmd: &Command, data: Option<Vec<u8>>) -> Result<u32, String> {
        let image = load(cmd, data, &mut self.file_failures)?;
        let id = match (cmd.id, cmd.number) {
            (0, 0) => self.own_id(),
            (0, number) => {
                let id = self.own_id();
                self.numbers.insert(number, id);
                id
            }
            (id, _) => id,
        };
        self.keep(id, image);
        Ok(id)
    }

    /// Keeps `image` under `id`, in place of what had it, the oldest
    /// making room for it.
    fn keep(&mut self, id: u32, image: Image) {
        self.forget(id, true);
        self.bytes += image.bytes();
        self.images.insert(id, Arc::new(image));
        self.order.push(id);
        self.make_room(id);
    }

    /// Keeps a picture a program sent another way than kitty's (a sixel, an
    /// iTerm2 inline image), which nothing names again, and places it at
    /// the cursor as kitty's `a=T` does: over `cols` × `rows` cells, or at
    /// its own size for 0 and 0, shrunk to the room right of the cursor,
    /// and fitted in its cells, or stretched to fill them with `fills`.
    /// Returns where to write it, on `screen`.
    pub fn show(&mut self, image: Image, (cols, rows): (u32, u32), fills: bool, ctx: Context, screen: Screen) -> Option<Place> {
        let id = self.own_id();
        self.keep(id, image);
        self.shown_on.insert(id, screen);
        let cmd = Command { action: b'T', cols, rows, ..Command::default() };
        let place = self.place(&cmd, id, ctx).ok().flatten()?;
        if let Some(p) = self.placements.get_mut(&(id, place.placement)) {
            p.fills = fills;
        }
        Some(place)
    }

    /// Whether any picture [`ImageStore::show`] placed on `screen` is
    /// still kept.
    pub fn holds_shown(&self, screen: Screen) -> bool {
        self.shown_on.values().any(|s| *s == screen)
    }

    /// Whether the main screen's pictures are worth sweeping
    /// ([`ImageStore::let_go_unseen`]), which means reading its scrollback:
    /// with more than half the quota held, and a sixteenth of it more
    /// than at the last sweep.
    pub fn sweep_due(&self) -> bool {
        self.bytes > QUOTA / 2 && self.bytes >= self.swept_at + QUOTA / 16 && self.holds_shown(Screen::Main)
    }

    /// Lets go of the pictures [`ImageStore::show`] placed on `screen` that
    /// no cell of `seen` (the image ids in the grid's placeholder cells: the
    /// screen's, and the main one's scrollback) shows any more. Kitty's
    /// pictures, which a program may place again, Flow's own (the startup
    /// logo) and models stay.
    pub fn let_go_unseen(&mut self, screen: Screen, seen: &HashSet<u32>) {
        let gone: Vec<u32> = self.shown_on.iter().filter(|&(id, s)| *s == screen && !seen.contains(id)).map(|(id, _)| *id).collect();
        for id in gone {
            self.forget(id, true);
        }
        if screen == Screen::Main {
            self.swept_at = self.bytes;
        }
    }

    /// An iTerm2 file begins to come in parts, in place of any before it
    /// that never ended.
    pub fn start_file(&mut self, args: crate::iterm::Args) {
        self.file = Some(FileParts { args, base64: Vec::new(), too_big: false });
    }

    /// A part of the iTerm2 file coming in parts, still base64; none if
    /// none is coming.
    pub fn file_part(&mut self, part: &[u8]) {
        let Some(file) = self.file.as_mut().filter(|f| !f.too_big) else { return };
        if file.base64.len() + part.len() > MAX_DATA {
            file.too_big = true;
            file.base64 = Vec::new();
        } else {
            file.base64.extend_from_slice(part);
        }
    }

    /// The iTerm2 file that came in parts, now whole: its arguments and its
    /// base64, if it's to be shown (`inline=1`) and not too big.
    pub fn end_file(&mut self) -> Option<(crate::iterm::Args, Vec<u8>)> {
        let file = self.file.take()?;
        if file.too_big {
            // Its name is the program's, escaped so it writes no lines or
            // escapes of its own into the log.
            log::info!("iTerm2 image {} not shown: more than {} MB in parts", file.args.name.as_deref().unwrap_or("without a name").escape_debug(), MAX_DATA >> 20);
            return None;
        }
        file.args.inline.then_some((file.args, file.base64))
    }

    fn own_id(&mut self) -> u32 {
        loop {
            self.next_id = self.next_id.wrapping_add(1);
            let id = OWN_IDS | (self.next_id & !OWN_IDS);
            if !self.images.contains_key(&id) {
                return id;
            }
        }
    }

    /// Drops the oldest images, those no longer shown first, until the data
    /// fits the quota; `keep` stays.
    fn make_room(&mut self, keep: u32) {
        while self.bytes > QUOTA {
            let shown = |id: &u32| self.has_placement(*id);
            // A model's picture holds no pixels here, so letting it go frees nothing.
            let pixels = |id: &u32| self.images.get(id).is_some_and(|i| i.model.is_none());
            let victim = self
                .order
                .iter()
                .copied()
                .find(|id| *id != keep && pixels(id) && !shown(id))
                .or_else(|| self.order.iter().copied().find(|id| *id != keep && pixels(id)));
            let Some(id) = victim else { break };
            self.forget(id, true);
        }
    }

    /// Lets go of image `id`'s data, and its placements too if `placements`.
    fn forget(&mut self, id: u32, placements: bool) {
        if let Some(image) = self.images.remove(&id) {
            self.bytes -= image.bytes();
        }
        self.shown_on.remove(&id);
        self.order.retain(|i| *i != id);
        if placements {
            self.placements.retain(|(i, _), _| *i != id);
        }
    }

    /// Records where image `id` shows; returns the cells to write unless it's
    /// a virtual placement, whose placeholders the program writes itself.
    fn place(&mut self, cmd: &Command, id: u32, ctx: Context) -> Result<Option<Place>, String> {
        let image = self.images.get(&id).ok_or("ENOENT:no such image")?;
        let (w, h) = (image.width, image.height);
        let [x, y, sw, sh] = cmd.src;
        let (x, y) = (x.min(w - 1), y.min(h - 1));
        let sw = if sw == 0 { w - x } else { sw.min(w - x) };
        let sh = if sh == 0 { h - y } else { sh.min(h - y) };
        let (cw, ch) = (ctx.cell.0.max(1) as f64, ctx.cell.1.max(1) as f64);
        let (fw, fh) = (sw as f64, sh as f64);
        let (mut cols, mut rows) = match (cmd.cols, cmd.rows) {
            (0, 0) => ((fw / cw).ceil(), (fh / ch).ceil()),
            (c, 0) => (c as f64, (c as f64 * cw * fh / fw / ch).ceil()),
            (0, r) => ((r as f64 * ch * fw / fh / cw).ceil(), r as f64),
            (c, r) => (c as f64, r as f64),
        };
        // Wider than the room right of the cursor: shrunk to fit it.
        let room = ctx.cols_left.max(1) as f64;
        if !cmd.virtual_ && cols > room {
            rows = (rows * room / cols).ceil();
            cols = room;
        }
        let (cols, rows) = ((cols as u32).clamp(1, MAX_CELLS), (rows as u32).clamp(1, MAX_CELLS));
        let natural = cmd.cols == 0 && cmd.rows == 0;
        let placement = Placement { cols, rows, src: [x, y, sw, sh], natural, fills: false };
        let pid = match cmd.placement {
            0 if !cmd.virtual_ => self.own_placement(),
            p => p & 0x00FF_FFFF,
        };
        self.placements.insert((id, pid), placement);
        if cmd.virtual_ {
            return Ok(None);
        }
        Ok(Some(Place { id, placement: pid, cols, rows, stay: cmd.stay }))
    }

    fn own_placement(&mut self) -> u32 {
        self.next_placement = (self.next_placement + 1) % (0x0100_0000 - OWN_PLACEMENTS);
        OWN_PLACEMENTS + self.next_placement
    }

    /// Deletes by image id or number, or an animation's frame (`d=f`,
    /// `delete_frame`). Everything on screen (`d=a`) is `delete_shown`; by
    /// cell, row, column or z-index: not yet.
    fn delete(&mut self, cmd: &Command) {
        let free = cmd.delete.is_ascii_uppercase();
        let id = match cmd.delete.to_ascii_lowercase() {
            b'i' => cmd.id,
            b'n' => self.numbers.get(&cmd.number).copied().unwrap_or(0),
            b'f' => return self.delete_frame(cmd),
            _ => return,
        };
        match cmd.placement {
            0 => self.placements.retain(|(i, _), _| *i != id),
            p => {
                self.placements.remove(&(id, p));
            }
        }
        if free && !self.has_placement(id) {
            self.forget(id, false);
        }
    }

    fn placements_of(&self, id: u32) -> impl Iterator<Item = (&(u32, u32), &Placement)> {
        self.placements.range((id, 0)..=(id, u32::MAX))
    }

    fn has_placement(&self, id: u32) -> bool {
        self.placements_of(id).next().is_some()
    }

    /// Deletes what the screen shows, as kitty's `d=a` asks: the placements in
    /// `shown`, (image, placement) pairs read from the screen's placeholder
    /// cells, placement 0 standing for all of the image's. Pictures in the
    /// scrollback or on the other screen (main or alternate) stay. With
    /// `free`, images left with no placement let go of their data too.
    pub fn delete_shown(&mut self, shown: &HashSet<(u32, u32)>, free: bool) {
        // Models are the Ratty protocol's, which deletes them itself.
        let images = &self.images;
        let on_screen = |(id, placement): &(u32, u32)| {
            images.get(id).is_none_or(|i| i.model.is_none()) && (shown.contains(&(*id, *placement)) || shown.contains(&(*id, 0)))
        };
        let mut gone: Vec<u32> = self.placements.keys().filter(|k| on_screen(k)).map(|(id, _)| *id).collect();
        self.placements.retain(|k, _| !on_screen(k));
        if free {
            gone.sort_unstable();
            gone.dedup();
            for id in gone {
                if !self.has_placement(id) {
                    self.forget(id, false);
                }
            }
        }
    }

    /// Keeps an image Flow shows itself (the startup logo), fitted to `cols` ×
    /// `rows` cells; returns where to write it.
    pub fn show_own(&mut self, image: Image, cols: u32, rows: u32) -> Place {
        let id = self.own_id();
        let src = [0, 0, image.width, image.height];
        self.bytes += image.bytes();
        self.images.insert(id, Arc::new(image));
        self.order.push(id);
        let placement = self.own_placement();
        self.placements.insert((id, placement), Placement { cols, rows, src, natural: false, fills: false });
        Place { id, placement, cols, rows, stay: true }
    }

    /// Shows `image` as [`ImageStore::show_own`] does, stretched to fill
    /// its cells exactly ([`Placement::fills`]).
    pub fn show_own_filling(&mut self, image: Image, cols: u32, rows: u32) -> Place {
        let place = self.show_own(image, cols, rows);
        if let Some(p) = self.placements.get_mut(&(place.id, place.placement)) {
            p.fills = true;
        }
        place
    }

    /// Lets go of an image Flow showed itself, and its placements.
    pub fn forget_own(&mut self, id: u32) {
        self.forget(id, true);
    }

    /// Gives a model's picture (`Image::model`) a new size, `w` × `h`, keeping
    /// its id and placements, so the cells that show it show the new one,
    /// which the renderer draws afresh; false if it's not one, or the size
    /// is the same.
    pub fn resize_model(&mut self, id: u32, (w, h): (u32, u32)) -> bool {
        let Some(old) = self.images.get(&id) else { return false };
        let Some(shown) = old.model.clone().filter(|_| (old.width, old.height) != (w, h)) else { return false };
        self.images.insert(id, Arc::new(Image::model(w, h, shown)));
        for (_, p) in self.placements.range_mut((id, 0)..=(id, u32::MAX)) {
            p.src = [0, 0, w, h];
        }
        true
    }

    /// What its images hold, decoded, and the id and size of each one's
    /// texture, for what the GPU holds of them: an animation's one, which
    /// holds whichever frame shows.
    pub fn held(&self) -> (u64, Vec<(u64, u32, u32)>) {
        let held = self.images.values().map(|i| match &i.frames {
            Some(frames) => (frames.reel(), i.width, i.height),
            None => (i.serial, i.width, i.height),
        });
        (self.bytes as u64, held.collect())
    }

    /// The image and placement a placeholder cell belongs to: placement 0
    /// takes the image's first. Cells that came through ConPTY name none,
    /// since it drops their underline color.
    pub fn lookup(&self, id: u32, placement: u32) -> Option<(Arc<Image>, Placement)> {
        let image = self.images.get(&id)?.clone();
        let found = match placement {
            0 => self.placements_of(id).next().map(|(_, p)| *p),
            p => self.placements.get(&(id, p)).copied(),
        };
        Some((image, found?))
    }

    /// Lets every picture, placement and transmission go, as a terminal
    /// started afresh holds none ([`crate::session::Session::reset_terminal`]),
    /// but keeps the files that failed lately counted: a program holding
    /// the tab's key (`FLOW_TAB_KEY`, which a remote one may be handed) may
    /// ask for a reset (`flow-settings reset terminal`), and one that did
    /// so after each few failures would name files by the thousand again.
    pub(crate) fn reset(&mut self) {
        let file_failures = std::mem::take(&mut self.file_failures);
        *self = ImageStore { file_failures, ..ImageStore::default() };
    }
}

/// The reply a program gets, if it gave an id or number and didn't ask for
/// quiet: `ESC _G i=<id>[,I=<n>][,p=<p>];<message> ESC \`.
fn reply(cmd: &Command, id: u32, message: &str) -> Option<String> {
    answer(cmd, id, None, message)
}

/// As [`reply`], naming the animation frame a command made or edited
/// (`r=<n>`, counted from 1), as kitty answers a frame.
fn answer(cmd: &Command, id: u32, frame: Option<usize>, message: &str) -> Option<String> {
    let ok = message == "OK";
    if (cmd.id == 0 && cmd.number == 0) || (ok && cmd.quiet >= 1) || cmd.quiet >= 2 {
        return None;
    }
    let mut keys = format!("i={id}");
    if cmd.number != 0 {
        keys.push_str(&format!(",I={}", cmd.number));
    }
    if cmd.placement != 0 {
        keys.push_str(&format!(",p={}", cmd.placement));
    }
    if let Some(frame) = frame {
        keys.push_str(&format!(",r={frame}"));
    }
    Some(format!("\x1b_G{keys};{message}\x1b\\"))
}

fn outcome<T>(result: &Result<T, String>) -> String {
    match result {
        Ok(_) => "OK".into(),
        Err(e) => e.clone(),
    }
}

/// The base64 in `text` appended to `out`, padded or not.
pub(crate) fn decode_base64(text: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let text: Vec<u8> = text.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
    if out.len() + text.len() / 4 * 3 > MAX_DATA {
        return Err("EFBIG:image data too large".into());
    }
    LENIENT.decode_vec(&text, out).map_err(|_| "EINVAL:bad base64".to_string())
}

/// What a transmission that names a file answers, whatever went wrong:
/// the file missing, unreadable, not a regular file, in the system's part
/// of the disk, shorter than the pixels need, not a picture, a picture that
/// won't decode, asked for in a way only a temporary file may be, or not
/// read at all after too many failures ([`FileFailures`]). A program on
/// another computer (over ssh) can name files on this one, and any other
/// answer would tell it which of these it found, as kitty's spec warns;
/// ranger takes this answer to mean it should send its pictures in the
/// escape instead.
const FILE_ERROR: &str = "EBADF:could not read the file";

/// The words a temporary file's path must hold, as kitty's spec asks, for
/// Flow to take it for one a program wrote for this and delete it.
const TEMP_MARK: &str = "tty-graphics-protocol";

/// How many transmissions naming a file may fail in [`FILE_FAILURE_WINDOW`]
/// before the rest are refused unread until it's over (see
/// [`FileFailures`]). A program that sends files fails once, asking
/// whether it may; this is room for a few such programs started at once.
const FILE_FAILURES: u32 = 8;

/// How long [`FILE_FAILURES`] are counted for, from the first.
const FILE_FAILURE_WINDOW: Duration = Duration::from_secs(10);

/// How long a transmission naming a file takes when it fails ([`wait_out`]),
/// longer than a missing file, a folder or a file that isn't a picture
/// takes to find out about once the system has them in its cache.
const FILE_FAILURE_TIME: Duration = Duration::from_millis(2);

/// The transmissions naming a file that failed lately. Each answers
/// [`FILE_ERROR`], but finding out takes longer for some than for others:
/// a missing file's fails at once, a file that isn't a picture once it's
/// opened and read from. A program on another computer could name a file
/// by the thousand, quietly (`q=2`), and time a later answer (the cursor's
/// place, say) to learn which it was. So each failure ends
/// [`FILE_FAILURE_TIME`] after it began ([`wait_out`]), and past
/// [`FILE_FAILURES`] of them in [`FILE_FAILURE_WINDOW`] every transmission
/// naming a file is refused without the disk touched until the window is
/// over, a terminal reset ([`ImageStore::reset`]), which a program holding
/// the tab's key may ask for, keeping them counted. A probe then takes as
/// long whatever it names, and what could still show comes a few times a
/// window at most:
/// where finding out takes longer than the wait (a disk slow to find a
/// file the first time, or a file that begins as a picture Flow shows but
/// won't decode, read whole first, so the time says how long it is), or
/// where the system holds the reader back meanwhile. The waiting, awake,
/// holds up the tab's reader and a frame drawn meanwhile, and keeps a
/// processor busy, 16 milliseconds a window at most. A picture read
/// doesn't count, so a program showing a folder of them is never held up.
#[derive(Default)]
struct FileFailures {
    /// When the window began, at its first failure.
    since: Option<Instant>,
    /// How many failed in it.
    count: u32,
}

impl FileFailures {
    /// The picture in the file `name` names, as [`load_file`] reads it, or
    /// [`FILE_ERROR`], why going to the log, which only the user here
    /// reads.
    fn load(&mut self, name: &[u8], cmd: &Command) -> Result<Image, String> {
        let started = Instant::now();
        if self.since.is_some_and(|since| started.duration_since(since) >= FILE_FAILURE_WINDOW) {
            *self = Self::default();
        }
        if self.count >= FILE_FAILURES {
            log::debug!("a picture's file not read: {FILE_FAILURES} failed in the last {} s", FILE_FAILURE_WINDOW.as_secs());
            return Err(FILE_ERROR.into());
        }
        load_file(name, cmd).map_err(|why| {
            self.since.get_or_insert(started);
            self.count += 1;
            log::debug!("a picture's file not read: {why}");
            // All that differs with what was found is done before the
            // wait, letting go of the words why too.
            drop(why);
            wait_out(started);
            FILE_ERROR.to_string()
        })
    }
}

/// Waits until [`FILE_FAILURE_TIME`] has passed since `started`, awake the
/// whole time. A sleep ends when the system's timers say, not at the
/// deadline: the Mac ends one later the longer it was asked for (its
/// timers' leeway), so sleeping out what's left showed the difference the
/// other way round; and where the system holds the thread back (run at the
/// Mac's utility class, a half-millisecond sleep ended 4.5 ms later) or its
/// timers are as coarse as Windows' can be, a sleep ends past the deadline,
/// so what was found showed one to one. Looking at the clock until the
/// deadline ends every failure within a look of it whatever the timers do:
/// two milliseconds of the processor a failure, a few failures a window at
/// most.
fn wait_out(started: Instant) {
    while started.elapsed() < FILE_FAILURE_TIME {
        std::hint::spin_loop();
    }
}

/// The image a command sends: its bytes from where `t` says, decompressed
/// and decoded. `data` is the payload already gathered from chunks.
fn load(cmd: &Command, data: Option<Vec<u8>>, files: &mut FileFailures) -> Result<Image, String> {
    let bytes = match data {
        Some(d) => d,
        None => {
            let mut d = Vec::new();
            decode_base64(&cmd.data, &mut d)?;
            d
        }
    };
    match cmd.medium {
        b'd' => unpack(cmd, bytes),
        b'f' | b't' => files.load(&bytes, cmd),
        _ => Err("EINVAL:unsupported transmission medium".into()),
    }
}

/// The picture in `bytes`, inflated first if the command says they're
/// compressed.
fn unpack(cmd: &Command, bytes: Vec<u8>) -> Result<Image, String> {
    if !cmd.compressed {
        return decode(cmd, &bytes);
    }
    let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes, MAX_DATA).map_err(|_| "EINVAL:bad zlib data".to_string())?;
    decode(cmd, &bytes)
}

/// The picture in the file a transmission names (`name`, its path): a file
/// (`t=f`), or a temporary one the program wrote for this (`t=t`), deleted
/// once opened. A program on another computer can name any file on this
/// one, so the answer may tell it only whether a path holds a picture (a
/// PNG, a GIF or a WebP, which kitty's icat sends by its path), read whole
/// from its start. Raw pixels, compression and a part of a file (`O`, `S`)
/// would tell it whether any file is there, how long it is and bits of
/// what it holds, so they come only from a temporary file, which it can't
/// name: in a temporary folder, with [`TEMP_MARK`] in its path and a
/// random name, as kitty's icat and viu write theirs. How long finding out
/// takes is [`FileFailures`]' to hide. The error is why, for the log, the
/// program's path in it escaped as Rust writes a string, so it can't write
/// lines or escapes of its own there; the program gets [`FILE_ERROR`] for
/// every one.
fn load_file(name: &[u8], cmd: &Command) -> Result<Image, String> {
    let name = std::str::from_utf8(name).map_err(|_| "a path that isn't UTF-8".to_string())?;
    let path = Path::new(name);
    if !is_local_path(path) {
        return Err(format!("{name:?} isn't a whole path on this computer"));
    }
    // Where the path as sent can't be a temporary file's, what only one may
    // give is refused before anything on the disk is touched.
    let may_be_temp = cmd.medium == b't' && name.contains(TEMP_MARK);
    if !may_be_temp {
        only_a_picture(cmd)?;
    }
    let real = path.canonicalize().map_err(|e| format!("{name:?}: {e}"))?;
    if is_sensitive(&real) {
        return Err(format!("{real:?} is the system's"));
    }
    let temp = may_be_temp && is_temp(&real);
    if may_be_temp && !temp {
        only_a_picture(cmd)?;
    }
    let meta = std::fs::metadata(&real).map_err(|e| format!("{real:?}: {e}"))?;
    if !meta.is_file() || meta.len() > MAX_DATA as u64 {
        return Err(format!("{real:?} isn't a regular file of at most {} MB", MAX_DATA >> 20));
    }
    let mut file = std::fs::File::open(&real).map_err(|e| format!("{real:?}: {e}"))?;
    let read = match file.metadata() {
        // The one opened, should the path have changed since.
        Ok(m) if m.is_file() && temp => read_part(&mut file, cmd),
        Ok(m) if m.is_file() => read_picture(&mut file),
        _ => Err("not a regular file once opened".into()),
    };
    drop(file);
    // A temporary file goes once opened, a picture or not, as in kitty, so
    // none is left behind.
    if temp {
        let _ = std::fs::remove_file(&real);
    }
    unpack(cmd, read.map_err(|e| format!("{real:?}: {e}"))?)
}

/// What may be asked of a file that isn't a temporary one: a picture
/// (`f=100`), not compressed, from its start. Its `S` isn't heeded, since
/// the whole file is read, so how long it is never changes the answer.
fn only_a_picture(cmd: &Command) -> Result<(), String> {
    if cmd.format == 100 && !cmd.compressed && cmd.offset == 0 {
        return Ok(());
    }
    Err("raw pixels, compressed data or a part of a file, which only a temporary file may give".into())
}

/// A temporary file's bytes: from `O` on, `S` of them at most, or all for
/// 0, as kitty reads them (icat's `S` can be more than the file holds).
fn read_part(file: &mut std::fs::File, cmd: &Command) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(cmd.offset as u64)).map_err(|e| e.to_string())?;
    let read = match cmd.size {
        0 => file.read_to_end(&mut bytes),
        n => file.by_ref().take(n as u64).read_to_end(&mut bytes),
    };
    read.map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The bytes a picture's file begins with that say which it is (a WebP's
/// twelve are the most), and for a PNG how big it is: its signature, then
/// its header's length, `IHDR`, its width and its height.
const PICTURE_HEAD: u64 = 24;

/// A file that isn't a temporary one: a picture, read whole. Its first
/// bytes are looked at before the rest is read, so a long one that isn't a
/// picture takes no longer to refuse, nor does a PNG whose header is
/// missing or says it's larger than Flow shows ([`MAX_SIDE`]), which
/// [`decode_png`] would refuse only once all of it was read, the time
/// that took saying how long the file is.
fn read_picture(file: &mut std::fs::File) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    file.by_ref().take(PICTURE_HEAD).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    if !png && !crate::frames::is_gif(&bytes) && !crate::frames::is_webp(&bytes) {
        return Err("not a PNG, a GIF or a WebP".into());
    }
    if png {
        let side = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
        match (bytes.get(12..16), side(16), side(20)) {
            (Some(b"IHDR"), Some(w), Some(h)) if w <= MAX_SIDE && h <= MAX_SIDE => {}
            _ => return Err(format!("a PNG that doesn't begin by saying it's at most {MAX_SIDE} pixels a side")),
        }
    }
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Whether `path` is a whole path to a file on this computer: not one
/// relative to wherever Flow was started, and on Windows one on a drive,
/// not a share on another computer (`\\server\share`), whose opening has
/// Windows sign in to that computer as the user, nor a device (`\\.\`).
pub(crate) fn is_local_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    path.is_absolute()
        && match path.components().next() {
            Some(Component::Prefix(p)) => matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
            _ => true,
        }
}

/// Whether `real` is in the system's part of the disk, which holds no
/// pictures and where merely reading can do things: /proc, /sys and /dev,
/// as kitty refuses them, but for /dev/shm, a temporary folder, where
/// kitty's icat writes its files on Linux.
pub(crate) fn is_sensitive(real: &Path) -> bool {
    ["/proc", "/sys", "/dev"].iter().any(|p| real.starts_with(p)) && !real.starts_with("/dev/shm")
}

/// Whether `real` is a temporary file a program wrote for this, as kitty's
/// spec has it: in a temporary folder (the system's, /tmp, /dev/shm, or
/// the Mac's under /var/folders), with [`TEMP_MARK`] in its path.
fn is_temp(real: &Path) -> bool {
    let mut folders = vec![std::env::temp_dir()];
    if cfg!(unix) {
        folders.extend(["/tmp", "/dev/shm", "/var/folders"].map(PathBuf::from));
    }
    // A folder that isn't there is none, and the root holds every path.
    let mut folders = folders.into_iter().filter_map(|f| f.canonicalize().ok()).filter(|f| f.parent().is_some());
    real.to_string_lossy().contains(TEMP_MARK) && folders.any(|f| real.starts_with(f))
}

/// Pixels from PNG (f=100) or raw RGB/RGBA (f=24/32, sized by s and v),
/// premultiplied for drawing. A file sent as f=100 that's a GIF or a WebP
/// is read as one, moving or not (`crate::frames`), as kitty's spec leaves
/// open and programs that send them hope.
fn decode(cmd: &Command, bytes: &[u8]) -> Result<Image, String> {
    let (width, height, mut rgba) = match cmd.format {
        100 if crate::frames::is_gif(bytes) || crate::frames::is_webp(bytes) => {
            return crate::frames::decode(bytes).map_err(|e| format!("EINVAL:{e}"));
        }
        100 => decode_png(bytes)?,
        24 | 32 => {
            let (w, h) = (cmd.width, cmd.height);
            let channels = if cmd.format == 24 { 3 } else { 4 };
            let need = w as usize * h as usize * channels;
            if w == 0 || h == 0 || bytes.len() < need {
                return Err("ENODATA:not enough pixel data".into());
            }
            let rgba = if channels == 4 {
                bytes[..need].to_vec()
            } else {
                bytes[..need].as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
            };
            (w, h, rgba)
        }
        _ => return Err("EINVAL:unsupported format".into()),
    };
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err("EFBIG:image too large".into());
    }
    premultiply(&mut rgba);
    Ok(Image::new(width, height, rgba))
}

/// RGBA's colors scaled by its alpha, as pictures are drawn.
pub(crate) fn premultiply(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        for c in &mut px[..3] {
            *c = (*c as u16 * a / 255) as u8;
        }
    }
}

pub(crate) fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let bad = |_| "EINVAL:bad PNG".to_string();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(bad)?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err("EFBIG:image too large".into());
    }
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or("EFBIG:image too large")?];
    let info = reader.next_frame(&mut buf).map_err(bad)?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("EINVAL:bad PNG".into()),
    };
    Ok((info.width, info.height, rgba))
}

// ---- placeholder cells ------------------------------------------------------

fn diacritic(n: u32) -> char {
    char::from_u32(DIACRITICS[(n as usize).min(DIACRITICS.len() - 1)]).unwrap_or('\u{0305}')
}

fn diacritic_index(c: char) -> Option<u32> {
    DIACRITICS.binary_search(&(c as u32)).ok().map(|i| i as u32)
}

/// A number in a color: 24 bits of RGB.
fn number_color(n: u32) -> Color {
    Color::Spec(Rgb { r: (n >> 16) as u8, g: (n >> 8) as u8, b: n as u8 })
}

fn color_number(color: Color) -> Option<u32> {
    match color {
        Color::Spec(c) => Some((c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32),
        Color::Indexed(i) => Some(i as u32),
        Color::Named(_) => None,
    }
}

/// The placeholder cell for row `row`, column `col` of placement `placement`
/// of image `id`.
pub fn placeholder_cell(id: u32, placement: u32, row: u32, col: u32) -> Cell {
    let mut cell = Cell { c: PLACEHOLDER, fg: number_color(id & 0x00FF_FFFF), ..Cell::default() };
    if placement != 0 {
        cell.set_underline_color(Some(number_color(placement)));
    }
    cell.push_zerowidth(diacritic(row));
    cell.push_zerowidth(diacritic(col));
    if id >> 24 != 0 {
        cell.push_zerowidth(diacritic(id >> 24));
    }
    cell
}

/// What a placeholder cell says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub id: u32,
    pub placement: u32,
    /// None where the cell leaves it out, to be taken from the cell before.
    pub row: Option<u32>,
    pub col: Option<u32>,
}

/// Reads a placeholder cell; None for any other cell.
pub fn read_placeholder(cell: &Cell) -> Option<Mark> {
    if cell.c != PLACEHOLDER {
        return None;
    }
    let marks = cell.zerowidth().unwrap_or(&[]);
    let mut nth = marks.iter().map(|&c| diacritic_index(c));
    let row = nth.next().flatten();
    let col = nth.next().flatten();
    let high = nth.next().flatten().unwrap_or(0);
    let id = color_number(cell.fg)? | high << 24;
    let placement = cell.underline_color().and_then(color_number).unwrap_or(0);
    (id != 0).then_some(Mark { id, placement, row, col })
}

// ---- rewrapping --------------------------------------------------------------

/// Before the grid narrows to `cols`: blanks the picture cells past that
/// on the main screen, scrollback and all. alacritty_terminal rewraps the
/// main screen's lines, and a row of a picture wider than the grid would
/// otherwise go on at the start of a row of its own, splitting the
/// picture; cut instead, each row stays one row, showing as much as fits.
/// Text after a picture on its row keeps its place behind the blanks, so it
/// wraps as before, and [`regrow_pictures`] can fill them in again. A double
/// line whose text flows rewraps by its half (`line_size::flows`), so it's
/// cut at its new half. The alternate screen isn't rewrapped: there
/// alacritty cuts every row itself.
pub fn crop_pictures<L: EventListener>(term: &mut Term<L>, cols: usize) {
    if cols >= term.columns() {
        return;
    }
    on_main_screen(term, |grid| {
        for line in -(grid.history_size() as i32)..grid.screen_lines() as i32 {
            let from = if line_size::flows(grid, Line(line)) { LineSize::Wide.shown(cols) } else { cols };
            let row = &mut grid[Line(line)];
            for c in from..row.len() {
                if read_placeholder(&row[Column(c)]).is_some() {
                    row[Column(c)] = Cell::default();
                }
            }
        }
    });
}

/// After the grid widened from `old_cols`: a picture on the main screen cut
/// at that edge (by [`crop_pictures`], or placed wider than the grid then
/// was) is filled out again into the blank cells now beside it, as far as
/// its placement goes, and on a double line as far as the half of its
/// columns that it shows (`crate::line_size`), whose last cell holds its
/// size. A double line's picture was cut at its old half if the line
/// wrapped, and at the old edge if not, the cells past its half hidden; so
/// it's grown from whichever of those it reaches. One whose row goes on
/// with text, or that ends there, is left as it is.
pub fn regrow_pictures<L: EventListener>(term: &mut Term<L>, old_cols: usize, store: &ImageStore) {
    let cols = term.columns();
    if old_cols == 0 || cols <= old_cols {
        return;
    }
    on_main_screen(term, |grid| {
        for line in -(grid.history_size() as i32)..grid.screen_lines() as i32 {
            let row = &mut grid[Line(line)];
            let size = line_size::of_row(row);
            let edges = if size.doubled() { [size.shown(old_cols) - 1, old_cols - 1] } else { [old_cols - 1; 2] };
            regrow_from(row, edges[0], size.shown(cols), store);
            if edges[1] != edges[0] {
                regrow_from(row, edges[1], size.shown(cols), store);
            }
        }
    });
}

/// Fills out the picture whose cell on `row` is at `edge` into the blank
/// cells after it, up to `shown`, as far as its placement goes.
fn regrow_from(row: &mut Row<Cell>, edge: usize, shown: usize, store: &ImageStore) {
    let Some(mark) = (edge < row.len()).then(|| read_placeholder(&row[Column(edge)])).flatten() else { return };
    // The marks the cell leaves out go on from the cells before it, from the
    // start of its run.
    let same = |c: &Cell| read_placeholder(c).is_some_and(|m| (m.id, m.placement) == (mark.id, mark.placement));
    let start = (0..edge).rev().take_while(|&c| same(&row[Column(c)])).last().unwrap_or(edge);
    let (mut r, mut col) = (0, 0);
    for c in start..=edge {
        let m = read_placeholder(&row[Column(c)]).unwrap_or(mark);
        r = m.row.unwrap_or(if c == start { 0 } else { r });
        col = m.col.unwrap_or(if c == start { 0 } else { col + 1 });
    }
    let Some((_, placement)) = store.lookup(mark.id, mark.placement) else { return };
    let mut template = row[Column(edge)].clone();
    template.flags.remove(Flags::WRAPLINE);
    for (k, c) in (edge + 1..shown).enumerate() {
        let col = col + 1 + k as u32;
        if col >= placement.cols || !row[Column(c)].is_empty() {
            break;
        }
        // A cell that names its column gets its own; one that leaves it out
        // to follow on from the cell before is copied.
        row[Column(c)] = match mark.col {
            Some(_) => placeholder_cell(mark.id, mark.placement, r, col),
            None => template.clone(),
        };
    }
}

/// Runs `f` on the main screen's grid, whichever screen is in use.
/// alacritty_terminal keeps the hidden one to itself, and swapping to it
/// and back is the only way in; coming back clears the alternate screen
/// and moves its cursor, so it's put back as it was.
pub(crate) fn on_main_screen<L: EventListener>(term: &mut Term<L>, f: impl FnOnce(&mut Grid<Cell>)) {
    if !term.mode().contains(TermMode::ALT_SCREEN) {
        return f(term.grid_mut());
    }
    let alternate = term.grid().clone();
    term.swap_alt();
    f(term.grid_mut());
    term.swap_alt();
    *term.grid_mut() = alternate;
}

/// The part of cell (`row`, `col`) an image covers, as a rectangle from the
/// placement's top-left (x, y, w, h in pixels) and the matching part of the
/// image (u0, v0, u1, v1). `cell` is a cell's size in pixels.
pub fn tile(image: (u32, u32), p: &Placement, cell: (f32, f32), row: u32, col: u32) -> Option<([f32; 4], [f32; 4])> {
    let (cw, ch) = cell;
    let [sx, sy, sw, sh] = p.src.map(|v| v as f32);
    let (kx, ky, ox, oy) = fit(p, cell);
    let (x0, y0) = ((col as f32 * cw).max(ox), (row as f32 * ch).max(oy));
    let (x1, y1) = (((col + 1) as f32 * cw).min(ox + sw * kx), ((row + 1) as f32 * ch).min(oy + sh * ky));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (iw, ih) = (image.0 as f32, image.1 as f32);
    let u = |x: f32| (sx + (x - ox) / kx) / iw;
    let v = |y: f32| (sy + (y - oy) / ky) / ih;
    Some(([x0, y0, x1 - x0, y1 - y0], [u(x0), v(y0), u(x1), v(y1)]))
}

/// How a placement's part of its image sits in its cells: the scale each
/// way and where it starts (pixels from the cells' top left).
fn fit(p: &Placement, (cw, ch): (f32, f32)) -> (f32, f32, f32, f32) {
    let [_, _, sw, sh] = p.src.map(|v| v as f32);
    let (bw, bh) = (p.cols as f32 * cw, p.rows as f32 * ch);
    let fit = (bw / sw).min(bh / sh);
    // At its own size it starts top-left and is never enlarged; fitted to
    // cells it's centered in them. One that falls short of its cells by
    // less than a pixel fills them: cells are a fraction of a pixel wide, so
    // an image made in whole pixels for its cells would otherwise leave a
    // hairline where images laid side by side meet.
    if p.natural {
        (fit.min(1.0), fit.min(1.0), 0.0, 0.0)
    } else if p.fills || (bw - sw * fit < 1.0 && bh - sh * fit < 1.0) {
        (bw / sw, bh / sh, 0.0, 0.0)
    } else {
        (fit, fit, (bw - sw * fit) / 2.0, (bh - sh * fit) / 2.0)
    }
}

/// The point of the image (its pixels, from its top left) shown at `at`
/// (pixels from the placement's top-left cell's corner), `tile` the other
/// way round; None where its cells show none of it.
pub fn picture_point(p: &Placement, cell: (f32, f32), at: (f32, f32)) -> Option<(f32, f32)> {
    let [sx, sy, sw, sh] = p.src.map(|v| v as f32);
    let (kx, ky, ox, oy) = fit(p, cell);
    let (x, y) = ((at.0 - ox) / kx, (at.1 - oy) / ky);
    ((0.0..=sw).contains(&x) && (0.0..=sh).contains(&y)).then_some((sx + x, sy + y))
}

/// `text` without placeholder cells (the character and its marks), for
/// copying and for the assistant.
pub fn strip(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_placeholder = false;
    for c in text.chars() {
        if c == PLACEHOLDER {
            in_placeholder = true;
        } else if !(in_placeholder && diacritic_index(c).is_some()) {
            in_placeholder = false;
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::test::TermSize;
    use alacritty_terminal::term::Config as TermConfig;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    pub(crate) const CTX: Context = Context { cell: (10, 20), cols_left: 80 };

    pub(crate) fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let pixels: Vec<u8> = (0..w * h).flat_map(|_| [255, 0, 0, 128]).collect();
        writer.write_image_data(&pixels).unwrap();
        drop(writer);
        out
    }

    #[test]
    fn a_picture_gone_from_everywhere_is_told_to_the_renderer_once() {
        take_gone();
        let image = Arc::new(Image::new(1, 1, vec![0; 4]));
        let serial = image.serial;
        let held = image.clone();
        drop(image);
        assert!(!take_gone().contains(&serial), "still held, as a frame holds what it draws");
        drop(held);
        assert!(take_gone().contains(&serial));
        assert!(!take_gone().contains(&serial));
    }

    #[test]
    fn keys_are_read_and_unknown_ones_left_out() {
        let cmd = parse(b"a=T,f=100,i=7,q=2,c=10,C=1,U=1,z=-1,Y=3,X=4294967296,H=2;QUJD").unwrap();
        assert_eq!((cmd.action, cmd.format, cmd.id, cmd.quiet, cmd.cols), (b'T', 100, 7, 2, 10));
        assert!(cmd.stay && cmd.virtual_);
        // An animation's keys, a gap below zero among them; a number past
        // what 32 bits hold is held to them.
        assert_eq!((cmd.z, cmd.big_x, cmd.big_y), (-1, u32::MAX, 3));
        assert_eq!(cmd.data, b"QUJD");
        assert_eq!(parse(b"a=q").unwrap().data, b"");
        assert!(parse(b"a").is_none());
    }

    #[test]
    fn a_query_is_answered_and_keeps_nothing() {
        let mut store = ImageStore::default();
        let body = format!("a=q,i=31,s=1,v=1,f=24,t=d;{}", b64(&[0, 0, 0]));
        let r = store.handle(body.as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=31;OK\x1b\\"));
        assert!(store.images.is_empty());
        // Shared memory isn't taken yet, so programs fall back to direct.
        let r = store.handle(b"a=q,i=32,s=1,v=1,f=24,t=s;bmFtZQ==", CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=32;EINVAL:unsupported transmission medium\x1b\\"));
    }

    #[test]
    fn a_png_sent_in_chunks_is_shown_at_its_own_size() {
        let mut store = ImageStore::default();
        let data = b64(&png(25, 30));
        let (a, b) = data.split_at(8);
        assert_eq!(store.handle(format!("a=T,f=100,i=5,m=1;{a}").as_bytes(), CTX), Response::default());
        let r = store.handle(format!("m=0;{b}").as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=5;OK\x1b\\"));
        let place = r.place.unwrap();
        assert_eq!((place.id, place.cols, place.rows, place.stay), (5, 3, 2, false));
        let (image, p) = store.lookup(5, place.placement).unwrap();
        assert_eq!((image.width, image.height, p.natural), (25, 30, true));
        // Premultiplied: half-transparent red.
        assert_eq!(&image.pixels[..4], &[128, 0, 0, 128]);
    }

    #[test]
    fn raw_pixels_compressed_with_zlib_decode() {
        let mut store = ImageStore::default();
        let raw: Vec<u8> = [10u8, 20, 30].repeat(4);
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let body = format!("a=t,f=24,s=2,v=2,o=z,i=9,q=1;{}", b64(&packed));
        assert_eq!(store.handle(body.as_bytes(), CTX), Response::default());
        assert_eq!(&store.images[&9].pixels[..4], &[10, 20, 30, 255]);
        // Too little data is an error the program hears about.
        let r = store.handle(format!("a=t,f=32,s=4,v=4,i=10;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        assert!(r.reply.unwrap().contains("ENODATA"));
    }

    /// A folder of a test's own in the temporary folder, gone after it.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("flow-graphics-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// What `keys` with `t` naming `path` answer, as image 9.
    fn ask_for_file(path: &Path, keys: &str) -> Option<String> {
        let mut store = ImageStore::default();
        store.handle(format!("i=9,{keys};{}", b64(path.as_os_str().as_encoded_bytes())).as_bytes(), CTX).reply
    }

    const REFUSED: &str = "\x1b_Gi=9;EBADF:could not read the file\x1b\\";

    #[test]
    fn a_missing_file_an_unreadable_one_and_one_that_isnt_a_picture_answer_alike() {
        let dir = Scratch::new("alike");
        let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&[7; 4096], 6);
        let missing = dir.0.join("missing.png");
        let text = dir.file("notes.txt", &[b'x'; 4096]);
        let packed = dir.file("packed.bin", &zlib);
        let cut = dir.file("cut.png", &png(4, 4)[..30]);
        let locked = dir.file("locked.png", &png(2, 2));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        // Run as root, it's read all the same, and is a picture.
        let unreadable = std::fs::read(&locked).is_err();
        let mut paths = vec![missing, text, packed, cut, dir.0.clone(), PathBuf::from("notes.txt")];
        if unreadable {
            paths.push(locked);
        }
        if cfg!(unix) {
            paths.extend(["/dev/zero", "/etc/hosts"].map(PathBuf::from));
        }
        // Each way of asking a program might use to learn whether a file
        // is there, how long it is or what it begins with: raw pixels of
        // a size, a window of it, compressed data, a picture.
        let asks = ["f=100", "f=24,s=1,v=1", "f=24,s=1,v=1000", "f=24,s=1,v=1,O=4096", "f=24,s=1,v=1,S=3", "f=32,s=32,v=32,o=z", "f=100,O=1", "f=100,S=8"];
        for path in &paths {
            for keys in asks {
                for medium in ["t=f", "t=t"] {
                    for action in ["a=q", "a=t", "a=T"] {
                        let keys = format!("{action},{medium},{keys}");
                        assert_eq!(ask_for_file(path, &keys).as_deref(), Some(REFUSED), "{} with {keys}", path.display());
                    }
                }
            }
        }
        // None of them was taken for a temporary file and deleted.
        assert!(dir.0.join("notes.txt").exists() && dir.0.join("packed.bin").exists());
    }

    #[test]
    fn a_picture_named_by_its_path_is_read_whole_from_its_start() {
        let dir = Scratch::new("picture");
        let path = dir.file("red.png", &png(25, 30));
        let mut store = ImageStore::default();
        // As kitty's icat sends a PNG it needn't change; `S` isn't heeded.
        for (i, keys) in [(4, "a=T,f=100,t=f"), (5, "a=t,f=100,t=f,S=10")] {
            let r = store.handle(format!("i={i},{keys};{}", b64(path.as_os_str().as_encoded_bytes())).as_bytes(), CTX);
            assert_eq!(r.reply, Some(format!("\x1b_Gi={i};OK\x1b\\")), "{keys}");
            assert_eq!((store.images[&i].width, store.images[&i].height), (25, 30));
        }
        // Not a temporary file, so it stays, though `t=t` asks for one.
        assert_eq!(ask_for_file(&path, "a=q,f=100,t=t").as_deref(), Some("\x1b_Gi=9;OK\x1b\\"));
        assert!(path.exists());
        // A GIF too, which moves.
        use gif::DisposalMethod::Keep;
        let gif = crate::frames::tests::gif(&[(&[0; 8], [0, 0, 4, 2], 5, Keep, None), (&[2; 8], [0, 0, 4, 2], 5, Keep, None)], gif::Repeat::Infinite);
        let gif = dir.file("moving.gif", &gif);
        let r = store.handle(format!("a=t,f=100,t=f,i=6;{}", b64(gif.as_os_str().as_encoded_bytes())).as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=6;OK\x1b\\"));
        assert_eq!(store.images[&6].frames.as_ref().map(|f| f.len()), Some(2));
    }

    #[test]
    fn a_png_too_large_to_show_is_refused_from_its_first_bytes_unread() {
        let dir = Scratch::new("large");
        // A header saying it's 20000 pixels wide, then a megabyte; and the
        // PNG's signature with nothing after it but noughts, as a program
        // might write to time how long a file it names is.
        let mut wide = png(2, 2);
        wide[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        wide.resize(wide.len() + (1 << 20), 0);
        let mut noughts = b"\x89PNG\r\n\x1a\n".to_vec();
        noughts.resize(1 << 20, 0);
        for (i, bytes) in [wide, noughts].iter().enumerate() {
            let path = dir.file(&format!("large-{i}.png"), bytes);
            let mut file = std::fs::File::open(&path).unwrap();
            assert!(read_picture(&mut file).is_err(), "{i}");
            // Nothing past its first bytes was read.
            assert_eq!(file.stream_position().unwrap(), PICTURE_HEAD, "{i}");
            assert_eq!(ask_for_file(&path, "a=q,t=f,f=100").as_deref(), Some(REFUSED), "{i}");
        }
        // One Flow shows is read to its end.
        let path = dir.file("red.png", &png(25, 30));
        let mut file = std::fs::File::open(&path).unwrap();
        assert_eq!(read_picture(&mut file).unwrap(), std::fs::read(&path).unwrap());
    }

    #[test]
    fn raw_pixels_compression_and_parts_come_from_a_temporary_file_deleted_once_opened() {
        let dir = Scratch::new("temporary");
        // How kitty's icat asks whether it may send files: a pixel in a
        // temporary file of its own, its `S` the length of the file's name.
        let query = dir.file("kitty-tty-graphics-protocol-query", &[1, 2, 3]);
        assert_eq!(ask_for_file(&query, "a=q,t=t,f=24,s=1,v=1,S=48").as_deref(), Some("\x1b_Gi=9;OK\x1b\\"));
        assert!(!query.exists(), "deleted once opened");
        // As ranger asks, in a file without the words: refused, and left.
        let plain = dir.file("tmpquery", &[255; 3]);
        assert_eq!(ask_for_file(&plain, "a=q,t=f,f=24,s=1,v=1,S=3").as_deref(), Some(REFUSED));
        assert_eq!(ask_for_file(&plain, "a=q,t=t,f=24,s=1,v=1,S=3").as_deref(), Some(REFUSED));
        assert!(plain.exists());
        // Compressed pixels from a part of one.
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&[10, 20, 30].repeat(4), 6);
        let file = dir.file("tty-graphics-protocol-frame", &[b"junk".as_slice(), &packed, b"more"].concat());
        let mut store = ImageStore::default();
        let keys = format!("a=t,i=2,t=t,f=24,s=2,v=2,o=z,O=4,S={}", packed.len());
        let r = store.handle(format!("{keys};{}", b64(file.as_os_str().as_encoded_bytes())).as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=2;OK\x1b\\"));
        assert_eq!(&store.images[&2].pixels[..4], &[10, 20, 30, 255]);
        assert!(!file.exists());
        // One too short for its pixels answers as a missing file does, and
        // goes all the same.
        let short = dir.file("tty-graphics-protocol-short", &[1, 2]);
        assert_eq!(ask_for_file(&short, "a=q,t=t,f=24,s=1,v=1").as_deref(), Some(REFUSED));
        assert!(!short.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_with_the_words_to_a_file_without_them_is_no_temporary_file() {
        let dir = Scratch::new("link");
        let target = dir.file("secret", &[1, 2, 3]);
        let link = dir.0.join("tty-graphics-protocol-link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(ask_for_file(&link, "a=q,t=t,f=24,s=1,v=1").as_deref(), Some(REFUSED));
        assert!(target.exists() && link.exists());
    }

    #[test]
    fn a_failure_takes_as_long_whether_the_file_is_missing_or_only_not_a_picture() {
        let dir = Scratch::new("time");
        let text = dir.file("notes.txt", b"not a picture");
        for path in [dir.0.join("missing.png"), text, dir.0.clone()] {
            let started = Instant::now();
            assert_eq!(ask_for_file(&path, "a=q,t=f,f=100").as_deref(), Some(REFUSED));
            assert!(started.elapsed() >= FILE_FAILURE_TIME, "{}", path.display());
        }
    }

    #[test]
    fn past_a_few_failures_every_file_is_refused_unread_until_they_are_old() {
        let dir = Scratch::new("failures");
        let picture = dir.file("red.png", &png(2, 2));
        let missing = dir.0.join("missing.png");
        let mut store = ImageStore::default();
        let mut ask = |path: &Path, keys: &str| store.handle(format!("i=9,{keys};{}", b64(path.as_os_str().as_encoded_bytes())).as_bytes(), CTX).reply;
        const READ: &str = "\x1b_Gi=9;OK\x1b\\";
        // A picture read doesn't count, however many.
        for _ in 0..2 * FILE_FAILURES {
            assert_eq!(ask(&picture, "a=q,t=f,f=100").as_deref(), Some(READ));
        }
        for _ in 0..FILE_FAILURES {
            assert_eq!(ask(&missing, "a=q,t=f,f=100").as_deref(), Some(REFUSED));
        }
        // Now a picture is refused as a missing file is, and a temporary
        // file isn't even opened, so it's still there.
        let temp = dir.file("tty-graphics-protocol-pixel", &[1, 2, 3]);
        assert_eq!(ask(&picture, "a=q,t=f,f=100").as_deref(), Some(REFUSED));
        assert_eq!(ask(&temp, "a=q,t=t,f=24,s=1,v=1").as_deref(), Some(REFUSED));
        assert!(temp.exists());
        // Once the window is over, files are read again.
        store.file_failures.since = Instant::now().checked_sub(FILE_FAILURE_WINDOW);
        let mut ask = |path: &Path, keys: &str| store.handle(format!("i=9,{keys};{}", b64(path.as_os_str().as_encoded_bytes())).as_bytes(), CTX).reply;
        assert_eq!(ask(&picture, "a=q,t=f,f=100").as_deref(), Some(READ));
        assert_eq!(ask(&temp, "a=q,t=t,f=24,s=1,v=1").as_deref(), Some(READ));
        assert!(!temp.exists());
    }

    #[test]
    fn a_terminal_reset_leaves_the_failures_counted() {
        let dir = Scratch::new("reset");
        let picture = dir.file("red.png", &png(2, 2));
        let missing = dir.0.join("missing.png");
        let mut store = ImageStore::default();
        let ask = |store: &mut ImageStore, path: &Path| store.handle(format!("i=9,a=t,t=f,f=100;{}", b64(path.as_os_str().as_encoded_bytes())).as_bytes(), CTX).reply;
        assert_eq!(ask(&mut store, &picture).as_deref(), Some("\x1b_Gi=9;OK\x1b\\"));
        for _ in 0..FILE_FAILURES {
            assert_eq!(ask(&mut store, &missing).as_deref(), Some(REFUSED));
        }
        // As `flow-settings reset terminal` has it: the picture goes, and
        // the failures stay counted, so a picture is still refused unread.
        store.reset();
        assert!(store.images.is_empty() && store.held().0 == 0);
        assert_eq!(ask(&mut store, &picture).as_deref(), Some(REFUSED));
        assert!(store.images.is_empty());
    }

    /// Times failures as a program on another computer would, to learn
    /// what they found: a file that isn't a picture and one that isn't
    /// there, by names as long, asked in turn, the failures counted afresh
    /// every [`FILE_FAILURES`] so none is refused unread. What's timed is
    /// [`FileFailures::load`], all a transmission does that depends on the
    /// file it names; reading the command before and answering after are
    /// the same for both, and only add the system's noise. Their times must
    /// not differ by a microsecond, however late the system's timers would
    /// end a sleep. Ignored, since it takes seconds and a busy computer
    /// could upset it; Flow's releases are optimized, so run it so too:
    /// `cargo test --release -p flow-core a_failure_takes_the_same_time --
    /// --ignored --nocapture`, with `FLOW_FAILURE_PROBES` for more than
    /// 1280 of each, and on the Mac under `taskpolicy -c utility` (or `-c
    /// background`) too, where a sleep ends milliseconds late.
    #[test]
    #[ignore]
    fn a_failure_takes_the_same_time_whatever_it_found() {
        let dir = Scratch::new("timing");
        let text = dir.file("notes.txt", &[b'x'; 4096]);
        let missing = dir.0.join("notes.txx");
        let cmd = parse(b"i=9,a=q,t=f,f=100").unwrap();
        let probes: usize = std::env::var("FLOW_FAILURE_PROBES").ok().and_then(|n| n.parse().ok()).unwrap_or(1280);
        let mut times = [Vec::with_capacity(probes), Vec::with_capacity(probes)];
        for block in 0..probes.div_ceil(FILE_FAILURES as usize) {
            // Which comes first takes turns too, so neither is always the
            // one asked after a pause.
            for which in [block % 2, 1 - block % 2] {
                let name = [&text, &missing][which].as_os_str().as_encoded_bytes();
                let mut failures = FileFailures::default();
                for _ in 0..FILE_FAILURES {
                    let started = Instant::now();
                    let answer = failures.load(name, &cmd);
                    times[which].push(started.elapsed().as_secs_f64() * 1e6);
                    assert_eq!(answer.err().as_deref(), Some(FILE_ERROR));
                }
            }
        }
        // The middle 80 % of each, as a program would weigh them, past
        // the times the system held the thread up.
        let trimmed = |t: &[f64]| {
            let cut = t.len() / 10;
            t[cut..t.len() - cut].iter().sum::<f64>() / (t.len() - 2 * cut) as f64
        };
        let mut seen = Vec::new();
        for (what, t) in ["not a picture", "not there"].iter().zip(&mut times) {
            t.sort_by(f64::total_cmp);
            let at = |share: f64| t[((t.len() - 1) as f64 * share) as usize];
            println!("{what}: {} probes, median {:.2} µs, trimmed mean {:.2}, p10 {:.2}, p90 {:.2}, p99 {:.2}, max {:.2}", t.len(), at(0.5), trimmed(t), at(0.1), at(0.9), at(0.99), at(1.0));
            seen.push((at(0.5), trimmed(t)));
        }
        let (median, mean) = (seen[0].0 - seen[1].0, seen[0].1 - seen[1].1);
        println!("not a picture less not there: median {median:+.3} µs, trimmed mean {mean:+.3} µs");
        assert!(median.abs() < 1.0 && mean.abs() < 1.0, "median {median:+.3} µs, trimmed mean {mean:+.3} µs");
    }

    #[test]
    fn only_a_whole_path_on_this_computer_names_a_file() {
        // Relative ones everywhere; on Windows, shares on other computers
        // and devices, which Flow never opens.
        for path in [r"\\server\share\x.png", r"\\?\UNC\server\share\x.png", r"\\.\pipe\x", "x.png", r"C:x.png"] {
            assert!(!is_local_path(Path::new(path)), "{path}");
        }
        if cfg!(windows) {
            assert!(!is_local_path(Path::new("//server/share/x.png")));
            assert!(is_local_path(Path::new(r"C:\Users\x.png")) && is_local_path(Path::new(r"\\?\C:\Users\x.png")));
        } else {
            assert!(is_local_path(Path::new("/home/x.png")));
        }
    }

    #[test]
    fn an_image_wider_than_the_room_left_shrinks_to_fit() {
        let mut store = ImageStore::default();
        store.handle(format!("a=t,f=100,i=1,q=2;{}", b64(&png(400, 100))).as_bytes(), CTX);
        let narrow = Context { cell: (10, 20), cols_left: 20 };
        let place = store.handle(b"a=p,i=1,q=2", narrow).place.unwrap();
        assert_eq!((place.cols, place.rows), (20, 3));
        // Asked for columns, it keeps its shape over them.
        let place = store.handle(b"a=p,i=1,c=10,q=2", CTX).place.unwrap();
        assert_eq!((place.cols, place.rows), (10, 2));
        // A virtual placement writes nothing: the program does.
        assert_eq!(store.handle(b"a=p,i=1,U=1,c=8,r=4,p=3,q=2", CTX).place, None);
        assert_eq!(store.lookup(1, 3).unwrap().1.cols, 8);
    }

    #[test]
    fn a_picture_sent_another_way_is_kept_and_placed_as_kitty_would() {
        let mut store = ImageStore::default();
        let place = store.show(Image::new(25, 30, vec![0; 25 * 30 * 4]), (0, 0), false, CTX, Screen::Main).unwrap();
        assert_eq!((place.cols, place.rows, place.stay), (3, 2, false));
        assert!(place.id >= OWN_IDS, "an id of Flow's own");
        let (image, p) = store.lookup(place.id, place.placement).unwrap();
        assert_eq!((image.width, p.natural, p.fills), (25, true, false));
        // Wider than the room left, it shrinks; stretched, it fills its cells.
        let narrow = Context { cell: (10, 20), cols_left: 2 };
        let place = store.show(Image::new(40, 20, vec![0; 40 * 20 * 4]), (8, 2), true, narrow, Screen::Main).unwrap();
        assert_eq!((place.cols, place.rows), (2, 1));
        assert!(store.lookup(place.id, place.placement).unwrap().1.fills);
    }

    #[test]
    fn pictures_no_cell_shows_any_more_are_let_go_by_the_screen_they_went_on() {
        let mut store = ImageStore::default();
        let picture = || Image::new(10, 20, vec![0; 10 * 20 * 4]);
        let main = store.show(picture(), (0, 0), false, CTX, Screen::Main).unwrap().id;
        let game = [0, 1].map(|_| store.show(picture(), (0, 0), false, CTX, Screen::Alternate).unwrap().id);
        let logo = store.show_own(picture(), 1, 1).id;
        store.handle(format!("a=T,f=24,s=1,v=1,i=7,q=2;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        // The game's second covered its first.
        store.let_go_unseen(Screen::Alternate, &HashSet::from([game[1]]));
        let kept = |store: &ImageStore, id| store.images.contains_key(&id);
        assert!(!kept(&store, game[0]) && kept(&store, game[1]) && kept(&store, main));
        // The alternate screen left, nothing of it shows.
        store.let_go_unseen(Screen::Alternate, &HashSet::new());
        assert!(!kept(&store, game[1]) && !store.holds_shown(Screen::Alternate));
        // Gone from the main screen and its scrollback: only what Flow
        // placed for a program goes, not its own logo nor kitty's.
        assert!(store.holds_shown(Screen::Main));
        store.let_go_unseen(Screen::Main, &HashSet::new());
        assert!(!kept(&store, main) && kept(&store, logo) && kept(&store, 7));
        assert_eq!(store.bytes, 10 * 20 * 4 + 4);
    }

    #[test]
    fn the_main_screen_is_swept_past_half_the_quota_and_not_again_until_it_grows() {
        let mut store = ImageStore::default();
        store.show(Image::new(1, 1, vec![0; 4]), (0, 0), false, CTX, Screen::Main).unwrap();
        assert!(!store.sweep_due(), "far from the quota");
        store.bytes = QUOTA / 2 + 1;
        assert!(store.sweep_due());
        let seen = store.shown_on.keys().copied().collect();
        store.let_go_unseen(Screen::Main, &seen);
        assert!(!store.sweep_due(), "swept, and every picture still showed");
        store.bytes += QUOTA / 16;
        assert!(store.sweep_due());
    }

    #[test]
    fn an_iterm2_file_in_parts_comes_whole_at_its_end_unless_too_big_or_a_download() {
        use crate::iterm::{parse_args, Args};
        let mut store = ImageStore::default();
        store.file_part(b"QUJD");
        assert_eq!(store.end_file(), None, "no file was begun");
        store.start_file(parse_args("inline=1;width=3"));
        store.file_part(b"QU");
        store.file_part(b"JD");
        let (args, base64) = store.end_file().unwrap();
        assert_eq!((args.width, base64.as_slice()), (crate::iterm::Extent::Cells(3), &b"QUJD"[..]));
        assert_eq!(store.end_file(), None, "once");
        // A download shows nothing; one begun anew takes the place of one
        // that never ended.
        store.start_file(Args::default());
        store.file_part(b"QUJD");
        assert_eq!(store.end_file(), None);
        store.start_file(parse_args("inline=1;width=9"));
        store.start_file(parse_args("inline=1"));
        assert_eq!(store.end_file().map(|(args, _)| args.width), Some(crate::iterm::Extent::Auto));
        // Past what a picture may hold, none of it is kept.
        store.start_file(parse_args("inline=1"));
        store.file.as_mut().unwrap().base64 = vec![b'A'; MAX_DATA - 2];
        store.file_part(b"QUJD");
        store.file_part(b"QU");
        assert!(store.file.as_ref().is_some_and(|f| f.too_big && f.base64.is_empty()));
        assert_eq!(store.end_file(), None);
    }

    #[test]
    fn a_kitty_picture_sent_as_a_gif_moves_and_every_frame_counts() {
        use gif::DisposalMethod::Keep;
        let mut store = ImageStore::default();
        let file = crate::frames::tests::gif(&[(&[0; 8], [0, 0, 4, 2], 5, Keep, None), (&[2; 8], [0, 0, 4, 2], 5, Keep, None)], gif::Repeat::Infinite);
        let r = store.handle(format!("a=T,f=100,i=3;{}", b64(&file)).as_bytes(), CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=3;OK\x1b\\"));
        let (image, _) = store.lookup(3, 0).unwrap();
        assert_eq!((image.width, image.frames.as_ref().map(|f| f.len()), store.bytes), (4, Some(2), 2 * 32));
        // One texture, its reel's, shows whichever frame is drawn: the
        // picture holding them has none, nor has each frame.
        let (_, held) = store.held();
        let reel = image.frames.as_ref().unwrap().reel();
        assert_eq!(held, [(reel, 4, 2)]);
        assert!(image.frames.as_ref().unwrap().each().all(|frame| frame.texture() == reel && frame.serial != reel));
        // A WebP, still, and a broken GIF, which the program hears of.
        let mut webp = Vec::new();
        image_webp::WebPEncoder::new(&mut webp).encode(&[0, 0, 255, 255], 1, 1, image_webp::ColorType::Rgba8).unwrap();
        assert_eq!(store.handle(format!("a=T,f=100,i=4,q=1;{}", b64(&webp)).as_bytes(), CTX).reply, None);
        assert_eq!(store.lookup(4, 0).unwrap().0.pixels, [0, 0, 255, 255]);
        let r = store.handle(format!("a=T,f=100,i=5;{}", b64(b"GIF89a\x01")).as_bytes(), CTX);
        assert!(r.reply.unwrap().starts_with("\x1b_Gi=5;EINVAL:a GIF that won't decode"));
    }

    #[test]
    fn deleting_frees_images_and_numbers_get_ids() {
        let mut store = ImageStore::default();
        let r = store.handle(format!("a=T,f=24,s=1,v=1,I=4;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        let id = r.place.unwrap().id;
        assert_eq!(r.reply, Some(format!("\x1b_Gi={id},I=4;OK\x1b\\")));
        store.handle(b"a=d,d=n,I=4", CTX);
        assert!(store.lookup(id, 0).is_none() && store.images.contains_key(&id));
        store.handle(b"a=d,d=I,i=1", CTX);
        assert!(store.images.contains_key(&id));
        // Deleting everything on screen is the session's to do, with what
        // the grid shows.
        assert_eq!(store.handle(b"a=d,d=A", CTX).clear_screen, Some(true));
        assert_eq!(store.handle(b"a=d", CTX).clear_screen, Some(false));
        assert!(store.images.contains_key(&id));
        let r = store.handle(b"a=p,i=99", CTX);
        assert_eq!(r.reply.as_deref(), Some("\x1b_Gi=99;ENOENT:no such image\x1b\\"));
    }

    #[test]
    fn deleting_what_the_screen_shows_leaves_the_rest() {
        let mut store = ImageStore::default();
        for i in [1, 2, 3] {
            store.handle(format!("a=T,f=24,s=1,v=1,i={i},p=1,q=2;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        }
        // Image 1 shows by its placement, image 2 by a cell naming none.
        store.delete_shown(&HashSet::from([(1, 1), (2, 0)]), false);
        assert!(store.lookup(1, 1).is_none() && store.lookup(2, 0).is_none());
        assert!(store.images.contains_key(&1), "d=a keeps the data");
        assert!(store.lookup(3, 1).is_some(), "not on screen: it stays");
        store.handle(b"a=p,i=1,p=1,q=2", CTX);
        store.delete_shown(&HashSet::from([(1, 1)]), true);
        assert!(!store.images.contains_key(&1) && store.images.contains_key(&3));
    }

    #[test]
    fn a_bare_placeholder_finds_its_image_by_the_id_alone() {
        let mut store = ImageStore::default();
        for (i, p) in [(7, 9), (70, 1), (6, 2)] {
            store.handle(format!("a=T,U=1,f=24,s=1,v=1,i={i},p={p},c={p},r=1,q=2;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        }
        store.handle(b"a=p,U=1,i=7,p=4,c=4,r=1,q=2", CTX);
        // What ConPTY leaves of a cell: the character and its color, with
        // no marks for the row and column, and no placement.
        let cell = Cell { c: PLACEHOLDER, fg: number_color(7), ..Cell::default() };
        let mark = read_placeholder(&cell).unwrap();
        assert_eq!(mark, Mark { id: 7, placement: 0, row: None, col: None });
        // The image's first placement, not another image's.
        assert_eq!(store.lookup(mark.id, mark.placement).unwrap().1.cols, 4);
        store.handle(b"a=d,d=i,i=7,p=4,q=2", CTX);
        assert_eq!(store.lookup(7, 0).unwrap().1.cols, 9);
        store.handle(b"a=d,d=I,i=7,q=2", CTX);
        assert!(store.lookup(7, 0).is_none() && !store.images.contains_key(&7));
        assert!(store.lookup(70, 0).is_some() && store.lookup(6, 0).is_some());
    }

    #[test]
    fn placeholder_cells_carry_their_image_placement_row_and_column() {
        let cell = placeholder_cell(0x1234_5678, 0x00AB_CDEF, 7, 200);
        let mark = read_placeholder(&cell).unwrap();
        assert_eq!(mark, Mark { id: 0x1234_5678, placement: 0x00AB_CDEF, row: Some(7), col: Some(200) });
        let mut bare = Cell { c: PLACEHOLDER, fg: Color::Indexed(42), ..Cell::default() };
        assert_eq!(read_placeholder(&bare), Some(Mark { id: 42, placement: 0, row: None, col: None }));
        bare.c = 'x';
        assert_eq!(read_placeholder(&bare), None);
        assert_eq!(strip(&format!("a{PLACEHOLDER}\u{0305}\u{030D}{PLACEHOLDER}\u{0305}b \u{0305}")), "ab \u{0305}");
    }

    #[test]
    fn a_fitted_image_is_centered_and_a_natural_one_starts_top_left() {
        let fitted = Placement { cols: 4, rows: 2, src: [0, 0, 100, 100], natural: false, fills: false };
        // It fills the 40×40 px of cells at 0.4×.
        let (rect, uv) = tile((100, 100), &fitted, (10.0, 20.0), 0, 1).unwrap();
        assert_eq!(rect, [10.0, 0.0, 10.0, 20.0]);
        assert_eq!(uv, [0.25, 0.0, 0.5, 0.5]);
        let natural = Placement { cols: 3, rows: 1, src: [0, 0, 25, 10], natural: true, fills: false };
        let (rect, _) = tile((25, 10), &natural, (10.0, 20.0), 0, 2).unwrap();
        assert_eq!(rect, [20.0, 0.0, 5.0, 10.0]);
        assert!(tile((25, 10), &natural, (10.0, 20.0), 1, 0).is_none());
    }

    #[test]
    fn an_image_a_fraction_of_a_pixel_short_of_its_cells_fills_them() {
        // Two cells of 16.54 × 38 px hold a 33 × 38 image: fitted, it would
        // leave 0.08 px of the cells bare.
        let tile_of = Placement { cols: 2, rows: 1, src: [0, 0, 33, 38], natural: false, fills: false };
        let (left, _) = tile((33, 38), &tile_of, (16.54, 38.0), 0, 0).unwrap();
        let (right, uv) = tile((33, 38), &tile_of, (16.54, 38.0), 0, 1).unwrap();
        assert_eq!(left[0], 0.0);
        assert!((right[0] + right[2] - 33.08).abs() < 1e-4);
        assert!((uv[2] - 1.0).abs() < 1e-6);
        // A pixel or more short, it's centered as before, unless it fills
        // its cells whatever its shape.
        let narrow = Placement { cols: 2, rows: 1, src: [0, 0, 30, 38], natural: false, fills: false };
        let (rect, _) = tile((30, 38), &narrow, (16.54, 38.0), 0, 0).unwrap();
        assert!((rect[0] - 1.54).abs() < 1e-4);
        let filling = Placement { fills: true, ..narrow };
        let (left, _) = tile((30, 38), &filling, (16.54, 38.0), 0, 0).unwrap();
        let (right, uv) = tile((30, 38), &filling, (16.54, 38.0), 0, 1).unwrap();
        assert_eq!((left[0], left[3]), (0.0, 38.0));
        assert!((right[0] + right[2] - 33.08).abs() < 1e-4 && (uv[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_point_in_the_cells_finds_the_pixel_of_the_picture_shown_there() {
        // 100 × 100 fitted to 4 × 2 cells of 10 × 20: 40 × 40 at 0.4.
        let fitted = Placement { cols: 4, rows: 2, src: [0, 0, 100, 100], natural: false, fills: false };
        assert_eq!(picture_point(&fitted, (10.0, 20.0), (20.0, 10.0)), Some((50.0, 25.0)));
        // A taller picture, centered: the bands either side show none of it.
        let tall = Placement { cols: 4, rows: 2, src: [0, 0, 50, 100], natural: false, fills: false };
        assert_eq!(picture_point(&tall, (10.0, 20.0), (20.0, 20.0)), Some((25.0, 50.0)));
        assert_eq!(picture_point(&tall, (10.0, 20.0), (5.0, 20.0)), None);
        // Each tile's corners land where `tile` puts them.
        let (rect, uv) = tile((50, 100), &tall, (10.0, 20.0), 1, 2).unwrap();
        let (x, y) = picture_point(&tall, (10.0, 20.0), (rect[0], rect[1])).unwrap();
        assert!((x / 50.0 - uv[0]).abs() < 1e-6 && (y / 100.0 - uv[1]).abs() < 1e-6);
    }

    #[test]
    fn a_models_picture_takes_a_new_size_under_the_same_id() {
        let mut store = ImageStore::default();
        let shown = Arc::new(crate::model::Shown::new(Arc::default(), crate::model::Style::default(), 20.0));
        let place = store.show_own(Image::model(40, 60, shown), 4, 3);
        let before = store.lookup(place.id, 0).unwrap().0.serial;
        assert!(!store.resize_model(place.id, (40, 60)), "the same size");
        assert!(store.resize_model(place.id, (48, 72)));
        let (image, placement) = store.lookup(place.id, place.placement).unwrap();
        assert_eq!((image.width, image.height, placement.src, placement.cols), (48, 72, [0, 0, 48, 72], 4));
        assert_ne!(image.serial, before, "a picture of its own, for the renderer to draw");
        assert!(image.model.is_some());
        let logo = store.show_own(Image::new(1, 1, vec![0; 4]), 1, 1);
        assert!(!store.resize_model(logo.id, (2, 2)), "not a model's");
    }

    /// What a cell shows, for comparing screens: its character, or which
    /// cell of which picture.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Shows {
        Text(char),
        Picture(u32, u32),
    }

    /// Every line from the top of the scrollback, without trailing blanks.
    fn lines_of(grid: &Grid<Cell>) -> Vec<Vec<Shows>> {
        (-(grid.history_size() as i32)..grid.screen_lines() as i32)
            .map(|l| {
                let row = &grid[Line(l)];
                let mut shows: Vec<Shows> = (0..grid.columns())
                    .map(|c| match read_placeholder(&row[Column(c)]) {
                        Some(m) => Shows::Picture(m.row.unwrap_or(99), m.col.unwrap_or(99)),
                        None => Shows::Text(row[Column(c)].c),
                    })
                    .collect();
                while shows.last() == Some(&Shows::Text(' ')) {
                    shows.pop();
                }
                shows
            })
            .collect()
    }

    /// Checks each of the picture's 3 rows is on one line, from column 2,
    /// its cells in order: as many as fit in `cols`.
    fn whole_rows(grid: &Grid<Cell>, cols: usize) {
        let lines = lines_of(grid);
        let mut seen = Vec::new();
        for line in &lines {
            let cells: Vec<(usize, u32, u32)> =
                line.iter().enumerate().filter_map(|(c, s)| if let Shows::Picture(r, k) = s { Some((c, *r, *k)) } else { None }).collect();
            let Some(&(first, r, _)) = cells.first() else { continue };
            assert_eq!(first, 2, "a row starting anywhere but its column: {line:?}");
            assert!(cells.iter().enumerate().all(|(i, &(c, row, k))| c == 2 + i && row == r && k == i as u32), "a row in pieces: {line:?}");
            assert_eq!(cells.len(), 30.min(cols - 2), "as much as fits");
            seen.push(r);
        }
        assert_eq!(seen, [0, 1, 2], "each row once, in order: {lines:?}");
    }

    /// A terminal 40 wide with a picture 30 columns by 3 rows at column 2,
    /// text after its last row, and a long line below; narrowed to 20
    /// columns by `resize` and widened back, with [`crop_pictures`] before
    /// and [`regrow_pictures`] after, as `Session::resize` does: each row of
    /// the picture stays whole, and back at 40 all is as it was.
    pub(crate) fn picture_through_a_rewrap(resize: impl Fn(&mut Term<VoidListener>, usize)) {
        let mut store = ImageStore::default();
        let place = store.show_own(Image::new(300, 60, vec![0; 300 * 60 * 4]), 30, 3);
        let mut term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &TermSize::new(40, 10), VoidListener);
        let mut processor = Processor::<StdSyncHandler>::new();
        processor.advance(&mut term, b"$ show\r\n\n\n\n");
        for r in 0..3 {
            for c in 0..30 {
                term.grid_mut()[Line(1 + r as i32)][Column(2 + c as usize)] = placeholder_cell(place.id, place.placement, r, c);
            }
        }
        processor.advance(&mut term, format!("\x1b[4;33Habc\r\n{}\r\n$ ", "y".repeat(38)).as_bytes());
        let before = lines_of(term.grid());
        whole_rows(term.grid(), 40);

        crop_pictures(&mut term, 20);
        resize(&mut term, 20);
        regrow_pictures(&mut term, 40, &store);
        whole_rows(term.grid(), 20);
        let text: String = lines_of(term.grid()).concat().iter().filter_map(|s| if let Shows::Text(c) = s { Some(*c) } else { None }).collect();
        assert!(text.contains("abc") && text.matches('y').count() == 38, "the text rewrapped: {text:?}");

        crop_pictures(&mut term, 40);
        resize(&mut term, 40);
        regrow_pictures(&mut term, 20, &store);
        assert_eq!(lines_of(term.grid()), before, "filled out again");
    }

    #[test]
    fn a_pictures_rows_stay_whole_as_the_grid_narrows_and_widens() {
        picture_through_a_rewrap(|term, cols| term.resize(TermSize::new(cols, 10)));
    }

    #[test]
    fn the_main_screens_pictures_are_cut_while_a_program_holds_the_alternate_one() {
        let mut store = ImageStore::default();
        let place = store.show_own(Image::new(300, 20, vec![0; 300 * 20 * 4]), 30, 1);
        let mut term = Term::new(TermConfig { scrolling_history: 100, ..TermConfig::default() }, &TermSize::new(40, 6), VoidListener);
        let mut processor = Processor::<StdSyncHandler>::new();
        for c in 0..30 {
            term.grid_mut()[Line(0)][Column(c)] = placeholder_cell(place.id, place.placement, 0, c as u32);
        }
        processor.advance(&mut term, b"\r\n$ \x1b[?1049h\x1b[3;5Hgame");
        let cursor = term.grid().cursor.point;

        crop_pictures(&mut term, 20);
        term.resize(TermSize::new(20, 6));
        regrow_pictures(&mut term, 40, &store);
        // The alternate screen, and its cursor, as the program left them.
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(lines_of(term.grid())[2], "    game".chars().map(Shows::Text).collect::<Vec<_>>());
        assert_eq!(term.grid().cursor.point, cursor);
        // Back on the main screen, the picture is cut, not wrapped.
        processor.advance(&mut term, b"\x1b[?1049l");
        let lines = lines_of(term.grid());
        assert_eq!((lines[0].len(), lines[0][19], lines[1].first()), (20, Shows::Picture(0, 19), Some(&Shows::Text('$'))), "{lines:?}");

        // Widened while the program holds the screen again, it fills out.
        processor.advance(&mut term, b"\x1b[?1049h");
        crop_pictures(&mut term, 40);
        term.resize(TermSize::new(40, 6));
        regrow_pictures(&mut term, 20, &store);
        processor.advance(&mut term, b"\x1b[?1049l");
        let lines = lines_of(term.grid());
        assert_eq!((lines[0].len(), lines[0][29]), (30, Shows::Picture(0, 29)));
    }

    #[test]
    fn a_bare_picture_cut_at_the_edge_fills_out_after_the_cell_before() {
        // Cells that name neither row nor column, as through ConPTY: each
        // takes its column from the one to its left.
        let mut store = ImageStore::default();
        store.handle(format!("a=T,U=1,f=24,s=1,v=1,i=7,p=1,c=10,r=1,q=2;{}", b64(&[1, 2, 3])).as_bytes(), CTX);
        let mut term = Term::new(TermConfig::default(), &TermSize::new(6, 3), VoidListener);
        let bare = Cell { c: PLACEHOLDER, fg: number_color(7), ..Cell::default() };
        for c in 2..6 {
            term.grid_mut()[Line(0)][Column(c)] = bare.clone();
        }
        term.resize(TermSize::new(9, 3));
        regrow_pictures(&mut term, 6, &store);
        let filled = (0..9).filter(|&c| term.grid()[Line(0)][Column(c)] == bare).count();
        assert_eq!(filled, 7, "from column 2 to the edge, short of the picture's 10");
    }
}
