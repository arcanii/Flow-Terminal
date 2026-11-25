//! Streaming scanner for the shell-integration OSC sequences, the progress
//! some programs report (OSC 9;4, as in Windows Terminal and ConEmu), kitty's
//! graphics commands (APC `_G`) and the queries the emulator doesn't answer
//! (XTVERSION, the cell size in pixels), those it does that Flow's pipe for
//! images answers too (`crate::pipe`: the text area's size, DA1, and the
//! colors OSC 4, 10, 11 and 12 ask for), the effects programs ask the glass
//! for (OSC 7731), and the sounds they ask for (OSC 7731's `fx=sound`, and
//! the VT520's notes, DECPS). It also finds DEC's line sizes (`ESC # 3` to
//! `ESC # 6`), which the emulator keeps no trace of (`crate::line_size`),
//! sixel pictures (a DCS, `crate::sixel`) and what programs ask of them
//! (XTSMGRAPHICS) and of the modes they go by (DECSDM, sixelScrollsRight,
//! which Flow keeps itself: [`Modes`]), iTerm2's inline images (OSC 1337,
//! `crate::iterm`, whole or in parts), and what lets the pictures of a
//! screen go (the alternate screen left, the screen erased whole, RIS). A
//! picture a program inside tmux sends wrapped for it (`DCS tmux; …` with
//! its ESCs doubled) is found as it would be unwrapped.
//! All bytes are still forwarded to the terminal emulator (it ignores these,
//! or answers them itself), but for the body of an OSC 1337
//! ([`Piece::Drop`]); the scanner only reports where each marker sits in
//! the byte stream.

use crate::line_size::LineSize;
use crate::sound::Cue;

#[derive(Debug, Clone, PartialEq)]
pub enum Marker {
    /// Prompt is about to be drawn.
    PromptStart,
    /// Prompt finished; the user's input starts here.
    PromptEnd,
    /// Command output starts, and the nonce after the mark where the hook
    /// sends one (zsh's, and bash's from 4.4: `133;C;<nonce>`), which says
    /// the hook marks the start of every command it runs.
    CommandStart { nonce: String },
    /// Previous command finished, with exit code if the shell reports one,
    /// and the nonce after it where the hook sends one (zsh's, bash's and
    /// PowerShell's: `133;D;<code>;<nonce>`). Anything printed in a tab can
    /// send the marks, so where the hook says its marks with the nonce the
    /// session takes a "done" without it, and a prompt drawn while a
    /// command runs, as a program's output that only looks like the hook's.
    CommandDone { code: Option<i32>, nonce: String },
    /// The command line as the shell read it (OSC 633;E), for the history,
    /// and the nonce after it. Anything printed in a tab can send one, so
    /// the session takes it only with its pane's own nonce, which only the
    /// shell's hook knows (`shell::NONCE`). `newest` (`;newest` after the
    /// nonce, bash's) where it's only the newest line of the shell's
    /// history, which may not be the line run: bash's history may have
    /// left that out (HISTCONTROL, HISTIGNORE).
    CommandLine { line: String, nonce: String, newest: bool },
    Cwd(String),
    /// bash's hook (OSC 633;P;FlowMetaKeys=1, before a prompt): the line
    /// editor at this prompt may take each byte past ASCII as Meta and a
    /// key, not a letter. readline 8.1 and later look at the locale again
    /// as each line is read, and where it has changed since the last one,
    /// put back that locale's 8-bit settings over the hook's `bind`: a C
    /// or POSIX one's (or none's) take such bytes as keys. It needs no
    /// nonce: anything printing it only has Flow type less.
    MetaKeys,
    /// A program's progress: `None` clears it.
    Progress(Option<Progress>),
    /// A kitty graphics command: the APC body after its `G`.
    Graphics(Vec<u8>),
    /// A Ratty graphics protocol command (3D models, `crate::model`): the
    /// APC body after `ratty;g;`.
    Model(Vec<u8>),
    /// XTVERSION (`CSI > q`): which terminal this is.
    VersionQuery,
    /// `CSI 16 t`: a cell's size in pixels.
    CellSizeQuery,
    /// `CSI 14 t`: the text area's size in pixels. The emulator answers it in
    /// the terminal; the pipe answers it too.
    AreaSizeQuery,
    /// DA1 (`CSI c`): what the terminal is. The emulator answers it in the
    /// terminal; down the pipe its answer marks the end of the answers.
    DeviceQuery,
    /// OSC 4, 10, 11 or 12 asking for colors (`ESC ] 11 ; ? BEL`): the
    /// colors asked, in turn, as the emulator numbers them (0 to 255 the
    /// palette's, 256 the text's, 257 the background's, 258 the cursor's),
    /// and whether BEL ended the question, as its answers must be ended.
    /// The emulator answers it in the terminal; the pipe answers it too.
    ColorQuery { colors: Vec<usize>, bell: bool },
    /// A program asks for an effect on the glass.
    Fx(FxRequest),
    /// A program asks for a sound.
    Sound(SoundRequest),
    /// A program washes the glass in a color while it runs, or (none) stops.
    Tint(Option<Wash>),
    /// A program asks about Flow's settings, or to change one (`flow-settings`,
    /// OSC 7732): the request as it came, base64 of a JSON object, which the
    /// app reads and answers the same way (`App::settings_answer`).
    Settings(String),
    /// DEC's line size for the cursor's line: `ESC # 3` and `ESC # 4` the top
    /// and bottom halves of a double-height line (DECDHL), `ESC # 6` double
    /// width (DECDWL), `ESC # 5` single again (DECSWL).
    LineSize(LineSize),
    /// A sixel picture (`ESC P <P1> ; <P2> ; <P3> q <data> ESC \`,
    /// `crate::sixel`): its parameters, an empty one 0, and its data after
    /// the `q`.
    Sixel { params: Vec<u16>, data: Vec<u8> },
    /// An iTerm2 inline image asked to show, `inline=1` (`ESC ] 1337 ; File
    /// = <args> : <base64> BEL`, `crate::iterm`): its arguments, and the
    /// file, still base64. The emulator never sees its body.
    ITermImage { args: crate::iterm::Args, data: Vec<u8> },
    /// The start of an iTerm2 file sent in parts, as iTerm2 3.5's `imgcat`
    /// sends a big one: `ESC ] 1337 ; MultipartFile = <args> BEL`, its
    /// arguments as `File=`'s, shown or not at its end as they say.
    ITermFileStart(crate::iterm::Args),
    /// A part of it, `ESC ] 1337 ; FilePart = <base64> BEL`, still base64.
    ITermFilePart(Vec<u8>),
    /// Its end, `ESC ] 1337 ; FileEnd BEL`.
    ITermFileEnd,
    /// DEC private modes Flow keeps for sixels, which the emulator doesn't
    /// know, and whether the alternate screen was asked for or left
    /// (`CSI ? Pm h` sets, `l` resets); or a full reset (RIS, `ESC c`),
    /// which resets them all.
    Modes(Modes),
    /// DECRQM (`CSI ? Ps $ p`) asking whether one of the sixel modes of
    /// [`Modes`] is set: 80 or 8452.
    ModeQuery(u16),
    /// The screen erased whole (`CSI 2 J`), or its scrollback
    /// (`CSI 3 J`, which `clear` sends after it).
    Erased { scrollback: bool },
    /// XTSMGRAPHICS (`CSI ? Pi ; Pa ; Pv S`), asking about sixel pictures:
    /// item `Pi` (1 the color registers, 2 the largest picture shown whole,
    /// 3 ReGIS's), action `Pa` (1 read, 2 reset, 3 set to `Pv`, 4 read the
    /// most there may be).
    GraphicsQuery { item: u16, action: u16 },
}

/// What a `CSI ? Pm h` or `l` says of the modes Flow keeps itself, each
/// `None` where it leaves that one alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modes {
    /// DECSDM (80), sixel display mode: a sixel goes at the screen's top
    /// left, the cursor staying where it is and nothing scrolling.
    pub sixel_display: Option<bool>,
    /// xterm's sixelScrollsRight (8452): the cursor goes right of a sixel's
    /// last row, not to the start of the line below it.
    pub sixel_right: Option<bool>,
    /// The alternate screen (1049), the one alacritty_terminal switches to.
    pub alternate: Option<bool>,
}

impl Modes {
    /// What RIS leaves: every mode reset, the main screen in use.
    pub const RESET: Modes = Modes { sixel_display: Some(false), sixel_right: Some(false), alternate: Some(false) };
}

/// A color the glass is washed in while a program runs, as a game's night:
/// `ESC ] 7731 ; fx=tint ; color=<RRGGBB> ; strength=<0..1> BEL`, the glass
/// shifting toward the color and the liquid dimming as far as `strength`
/// says; `strength=0` washes it off. It holds while the program keeps the
/// alternate screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wash {
    pub color: [u8; 3],
    pub strength: f32,
}

/// A sound a program asks for: one of Flow's by name, with
/// `ESC ] 7731 ; fx=sound ; sound=<name> [; at=<col>,<row>] [; strength=<0..1>]`,
/// or the VT520's notes with DECPS, `CSI Pv ; Pd ; Pn … , ~`.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundRequest {
    /// Its column places it left or right.
    Named { cue: Cue, col: Option<u16>, strength: f32, volume: f32 },
    /// `volume` 0 (silent) to 7, each note `duration` 32nds of a second,
    /// notes from 1 (C5) to 25 (C7) and 0 for a rest.
    Notes { volume: u8, duration: u8, notes: Vec<u8> },
}

/// What an effect sounds like, while its sounds are on: its kind's (a drop
/// for a ripple, a splash for a burst, thunder for a bolt, a rush of air
/// for a stir), none (`sound=none`), or one of Flow's by name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FxSound {
    Kind,
    Quiet,
    Named(Cue),
}

/// An effect a program asks for with OSC 7731:
/// `ESC ] 7731 ; fx=<kind> ; at=<col>,<row> [; to=<col>,<row>]
/// [; color=<rrggbb>] [; size=<cells>] [; dir=<dx>,<dy>] [; strength=<0..1>]
/// [; sound=<name>|none]`
/// ended by BEL or ST, with cells counted from the screen's top-left from 0.
/// Unknown keys are ignored, and so is a request of an unknown kind or
/// without what its kind needs (a bolt's `to`, a stir's `dir`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxRequest {
    pub kind: FxKind,
    pub at: (u16, u16),
    /// Where a bolt ends.
    pub to: Option<(u16, u16)>,
    pub color: Option<[u8; 3]>,
    /// How far it reaches, in columns.
    pub size: Option<f32>,
    /// Which way a stir pushes, as a unit vector (y down).
    pub dir: Option<(f32, f32)>,
    /// 0..1; 0.5 unless given.
    pub strength: f32,
    pub sound: FxSound,
    /// How loud its sound is, 0..1, against what its strength makes it: a
    /// program's own volume for its effects, which leaves how they look.
    pub volume: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxKind {
    /// Lightning from `at` to `to`.
    Bolt,
    /// A splash, in `color` where the liquid takes color.
    Burst,
    /// A ring spreading from `at`.
    Ripple,
    /// The liquid pushed along `dir`.
    Stir,
}

/// What OSC 9;4 says about a running program's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub state: ProgressState,
    /// 0..=100; meaningless while `Busy`.
    pub percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    Normal,
    Error,
    /// Going, but with no figure to show.
    Busy,
    Paused,
}

#[derive(Debug, PartialEq)]
pub enum Piece {
    /// Byte range of the chunk passed to `feed`.
    Data(usize, usize),
    /// Byte range of the chunk kept from the emulator: the body of an OSC
    /// 1337 (iTerm2's), whose file can run to megabytes, which vte would
    /// keep whole and alacritty spell out byte by byte for its log before
    /// asking whether it logs that. Its start (`ESC ] 1337 ;`) and its end
    /// still go, so the emulator reads an empty one.
    Drop(usize, usize),
    Marker(Marker),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Esc,
    Osc,
    OscEsc,
    Apc,
    ApcEsc,
    Csi,
    /// After `ESC #`: a line size, or another of DEC's (DECALN).
    Hash,
    /// A DCS (`ESC P`): a sixel picture's, or another the emulator has.
    Dcs,
    DcsEsc,
}

const MAX_PAYLOAD: usize = 4096;
/// What begins an OSC of iTerm2's, whose body the emulator is spared.
const ITERM: &[u8] = b"1337;";
/// An OSC 1337 may run this long: a file of 48 MB in base64.
const MAX_ITERM: usize = 64 << 20;
/// A sixel picture's DCS may run this long, as an APC may.
const MAX_DCS: usize = 64 << 20;
/// A DCS's parameters, before its `q`, past which it's not read as a sixel's.
const MAX_DCS_PARAMS: usize = 32;
/// What starts a Ratty graphics command's APC.
const RATTY: &[u8] = b"ratty;g;";
/// An APC longer than this (a graphics command's chunk is 4 KB) is dropped.
const MAX_APC: usize = 64 << 20;
/// A CSI's parameters worth reading: the queries here are short, DECPS's
/// notes longer. A longer one is passed over.
const MAX_CSI: usize = 96;

#[derive(Default)]
pub struct OscScanner {
    state: State,
    payload: Vec<u8>,
    /// The OSC grew past [`MAX_PAYLOAD`], or the APC past [`MAX_APC`], and
    /// is skipped: what's left of it could read as something it didn't say.
    /// A DCS that isn't a sixel's is skipped the same way.
    overflow: bool,
    /// The OSC is an OSC 1337, whose body is kept from the emulator.
    dropping: bool,
    /// The DCS has come to the `q` of a sixel picture.
    sixel: bool,
}

impl OscScanner {
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, &b) in chunk.iter().enumerate() {
            match self.state {
                State::Ground => {
                    if b == 0x1b {
                        self.state = State::Esc;
                    }
                }
                State::Esc => self.escape(b, i, &mut start, &mut out),
                State::Osc => match b {
                    0x07 => {
                        self.end_drop(&mut start, i, &mut out);
                        self.finish(&mut start, i + 1, true, &mut out);
                        self.state = State::Ground;
                    }
                    0x1b => {
                        self.end_drop(&mut start, i, &mut out);
                        self.state = State::OscEsc;
                    }
                    0x18 | 0x1a => {
                        self.end_drop(&mut start, i, &mut out);
                        self.state = State::Ground;
                    }
                    _ => {
                        let room = if self.dropping { MAX_ITERM } else { MAX_PAYLOAD };
                        if self.payload.len() < room {
                            self.payload.push(b);
                        } else {
                            self.overflow = true;
                        }
                        // iTerm2's: what follows is kept from the emulator.
                        if !self.dropping && self.payload == ITERM {
                            out.push(Piece::Data(start, i + 1));
                            start = i + 1;
                            self.dropping = true;
                        }
                    }
                },
                State::OscEsc => {
                    if b == b'\\' {
                        self.finish(&mut start, i + 1, false, &mut out);
                        self.state = State::Ground;
                    } else {
                        // A bare ESC ended the OSC (ConPTY drops the backslash of ST).
                        self.finish(&mut start, i, false, &mut out);
                        self.escape(b, i, &mut start, &mut out);
                    }
                }
                State::Apc => match b {
                    0x1b => self.state = State::ApcEsc,
                    _ if self.payload.len() < MAX_APC => self.payload.push(b),
                    _ => self.overflow = true,
                },
                State::ApcEsc => {
                    let end = if b == b'\\' { i + 1 } else { i };
                    if !std::mem::take(&mut self.overflow) {
                        if self.payload.first() == Some(&b'G') {
                            let body = std::mem::take(&mut self.payload).split_off(1);
                            self.mark(&mut start, end, Marker::Graphics(body), &mut out);
                        } else if self.payload.starts_with(RATTY) {
                            let body = std::mem::take(&mut self.payload).split_off(RATTY.len());
                            self.mark(&mut start, end, Marker::Model(body), &mut out);
                        }
                    }
                    self.payload.clear();
                    self.state = State::Ground;
                    if b != b'\\' {
                        self.escape(b, i, &mut start, &mut out);
                    }
                }
                State::Csi => match b {
                    0x40..=0x7e => {
                        let marker = match (self.payload.as_slice(), b) {
                            _ if std::mem::take(&mut self.overflow) => None,
                            (b">" | b">0", b'q') => Some(Marker::VersionQuery),
                            (b"16", b't') => Some(Marker::CellSizeQuery),
                            (b"14", b't') => Some(Marker::AreaSizeQuery),
                            (b"" | b"0", b'c') => Some(Marker::DeviceQuery),
                            (params, b'~') => parse_decps(params),
                            ([b'?', params @ ..], b'S') => parse_graphics_query(params),
                            ([b'?', params @ ..], b'h' | b'l') => parse_modes(params, b == b'h'),
                            (b"?80$", b'p') => Some(Marker::ModeQuery(80)),
                            (b"?8452$", b'p') => Some(Marker::ModeQuery(8452)),
                            (b"2", b'J') => Some(Marker::Erased { scrollback: false }),
                            (b"3", b'J') => Some(Marker::Erased { scrollback: true }),
                            _ => None,
                        };
                        if let Some(marker) = marker {
                            self.mark(&mut start, i + 1, marker, &mut out);
                        }
                        self.state = State::Ground;
                    }
                    0x1b => self.state = State::Esc,
                    0x20..=0x3f if self.payload.len() < MAX_CSI => self.payload.push(b),
                    0x20..=0x3f => self.overflow = true,
                    // A control character inside a CSI is carried out on its own.
                    _ => {}
                },
                State::Hash => {
                    let size = match b {
                        b'3' => Some(LineSize::Top),
                        b'4' => Some(LineSize::Bottom),
                        b'5' => Some(LineSize::Single),
                        b'6' => Some(LineSize::Wide),
                        _ => None,
                    };
                    if let Some(size) = size {
                        self.mark(&mut start, i + 1, Marker::LineSize(size), &mut out);
                    }
                    self.state = if b == 0x1b { State::Esc } else { State::Ground };
                }
                State::Dcs => match b {
                    0x1b => self.state = State::DcsEsc,
                    0x18 | 0x1a => self.state = State::Ground,
                    _ if self.overflow => {}
                    _ if self.sixel => {
                        if self.payload.len() < MAX_DCS {
                            self.payload.push(b);
                        } else {
                            self.overflow = true;
                        }
                    }
                    b'0'..=b'9' | b';' if self.payload.len() < MAX_DCS_PARAMS => self.payload.push(b),
                    b'q' => {
                        self.sixel = true;
                        self.payload.push(b);
                    }
                    // Another DCS (DECRQSS, XTGETTCAP, tmux's), the emulator's.
                    _ => self.overflow = true,
                },
                State::DcsEsc => {
                    let end = if b == b'\\' { i + 1 } else { i };
                    if std::mem::take(&mut self.sixel) && !std::mem::take(&mut self.overflow) {
                        let marker = sixel_marker(std::mem::take(&mut self.payload));
                        self.mark(&mut start, end, marker, &mut out);
                    }
                    self.payload.clear();
                    self.state = State::Ground;
                    if b != b'\\' {
                        self.escape(b, i, &mut start, &mut out);
                    }
                }
            }
        }
        if start < chunk.len() {
            out.push(if self.dropping { Piece::Drop(start, chunk.len()) } else { Piece::Data(start, chunk.len()) });
        }
        out
    }

    /// Ends what's kept from the emulator of an OSC 1337 at `at`, where its
    /// end begins, which goes to it.
    fn end_drop(&mut self, start: &mut usize, at: usize, out: &mut Vec<Piece>) {
        if std::mem::take(&mut self.dropping) {
            if *start < at {
                out.push(Piece::Drop(*start, at));
            }
            *start = at;
        }
    }

    /// What the byte `b` at `i` after an ESC begins; after a full reset
    /// (RIS, `ESC c`), the modes Flow keeps are reset too.
    fn escape(&mut self, b: u8, i: usize, start: &mut usize, out: &mut Vec<Piece>) {
        if b == b'c' {
            self.mark(start, i + 1, Marker::Modes(Modes::RESET), out);
        }
        self.after_esc(b);
    }

    fn after_esc(&mut self, b: u8) {
        self.state = match b {
            b']' => {
                self.payload.clear();
                self.overflow = false;
                State::Osc
            }
            b'_' => {
                self.payload.clear();
                self.overflow = false;
                State::Apc
            }
            b'[' => {
                self.payload.clear();
                self.overflow = false;
                State::Csi
            }
            b'P' => {
                self.payload.clear();
                self.overflow = false;
                self.sixel = false;
                State::Dcs
            }
            b'#' => State::Hash,
            0x1b => State::Esc,
            _ => State::Ground,
        };
    }

    /// Reports the OSC just ended, at `end`, by BEL or else by ST.
    fn finish(&mut self, start: &mut usize, end: usize, bell: bool, out: &mut Vec<Piece>) {
        let payload = std::mem::take(&mut self.payload);
        if std::mem::take(&mut self.overflow) {
            return;
        }
        let marker = if payload.starts_with(ITERM) { parse_iterm(payload) } else { parse_payload(&payload).or_else(|| parse_color_query(&payload, bell)) };
        if let Some(marker) = marker {
            self.mark(start, end, marker, out);
        }
    }

    /// Reports `marker` after the bytes up to `end`.
    fn mark(&mut self, start: &mut usize, end: usize, marker: Marker, out: &mut Vec<Piece>) {
        if *start < end {
            out.push(Piece::Data(*start, end));
        }
        *start = end;
        out.push(Piece::Marker(marker));
    }
}

fn parse_payload(payload: &[u8]) -> Option<Marker> {
    let text = std::str::from_utf8(payload).ok()?;
    let (code, rest) = text.split_once(';').unwrap_or((text, ""));
    match code {
        "133" | "633" => {
            let (kind, args) = rest.split_once(';').unwrap_or((rest, ""));
            match kind {
                "A" => Some(Marker::PromptStart),
                "B" => Some(Marker::PromptEnd),
                "C" => Some(Marker::CommandStart { nonce: args.to_string() }),
                "D" => {
                    let (code, nonce) = args.split_once(';').unwrap_or((args, ""));
                    Some(Marker::CommandDone { code: code.trim().parse().ok(), nonce: nonce.to_string() })
                }
                "P" if args == "FlowMetaKeys=1" => Some(Marker::MetaKeys),
                "P" => args.strip_prefix("Cwd=").map(|p| Marker::Cwd(p.to_string())),
                // The nonce follows, as VS Code sends it: the command can't
                // hold a bare `;`, which its escaping writes as `\x3b`.
                // Then bash's `;newest`, where it's only that.
                "E" => {
                    let (line, nonce) = args.split_once(';').unwrap_or((args, ""));
                    let (nonce, newest) = match nonce.strip_suffix(";newest") {
                        Some(nonce) => (nonce, true),
                        None => (nonce, false),
                    };
                    Some(Marker::CommandLine { line: unescape(line), nonce: nonce.to_string(), newest })
                }
                _ => None,
            }
        }
        "7" => {
            let url = rest.strip_prefix("file://")?;
            let path = url.find('/').map(|i| &url[i..]).unwrap_or("");
            let mut path = percent_decode(path);
            if cfg!(windows) && path.len() > 2 && path.as_bytes()[2] == b':' {
                path.remove(0);
            }
            Some(Marker::Cwd(path))
        }
        "9" => {
            if let Some(args) = rest.strip_prefix("4;") {
                return parse_progress(args);
            }
            rest.strip_prefix("9;").map(|p| Marker::Cwd(p.trim_matches('"').to_string()))
        }
        "7731" => parse_fx(rest),
        "7732" => Some(Marker::Settings(rest.to_string())),
        _ => None,
    }
}

/// An OSC 1337 that shows a file, `1337;File=<args>:<base64>` with
/// `inline=1`, or one of the three a file sent in parts comes in
/// (`MultipartFile=<args>`, `FilePart=<base64>`, `FileEnd`); its other
/// commands (and downloads) are passed over.
fn parse_iterm(mut payload: Vec<u8>) -> Option<Marker> {
    const FILE: &[u8] = b"File=";
    const MULTIPART: &[u8] = b"MultipartFile=";
    const PART: &[u8] = b"FilePart=";
    let command = payload.get(ITERM.len()..)?;
    if let Some(args) = command.strip_prefix(MULTIPART) {
        return Some(Marker::ITermFileStart(crate::iterm::parse_args(std::str::from_utf8(args).ok()?)));
    }
    if command.starts_with(PART) {
        payload.drain(..ITERM.len() + PART.len());
        return Some(Marker::ITermFilePart(payload));
    }
    if command == b"FileEnd" {
        return Some(Marker::ITermFileEnd);
    }
    let rest = command.strip_prefix(FILE)?;
    let colon = rest.iter().position(|&b| b == b':')?;
    let args = crate::iterm::parse_args(std::str::from_utf8(&rest[..colon]).ok()?);
    if !args.inline {
        return None;
    }
    payload.drain(..ITERM.len() + FILE.len() + colon + 1);
    Some(Marker::ITermImage { args, data: payload })
}

/// A sixel DCS's parameters and data, from its body: what came before its
/// `q` (digits and `;` alone) and after.
fn sixel_marker(mut payload: Vec<u8>) -> Marker {
    let q = payload.iter().position(|&b| b == b'q').unwrap_or(0);
    let params = payload[..q].split(|&b| b == b';').map(|p| std::str::from_utf8(p).ok().and_then(|p| p.parse().ok()).unwrap_or(0)).collect();
    payload.drain(..=q);
    Marker::Sixel { params, data: payload }
}

/// What a `CSI ? Pm h` (`set`) or `l` does to the modes Flow keeps, after
/// its `?`: none where it names none of them.
fn parse_modes(params: &[u8], set: bool) -> Option<Marker> {
    let mut modes = Modes::default();
    for mode in params.split(|&b| b == b';') {
        match mode {
            b"80" => modes.sixel_display = Some(set),
            b"8452" => modes.sixel_right = Some(set),
            b"1049" => modes.alternate = Some(set),
            _ => {}
        }
    }
    (modes != Modes::default()).then_some(Marker::Modes(modes))
}

/// XTSMGRAPHICS's item and action, after its `?`; a value to set is passed
/// over, since Flow's are fixed.
fn parse_graphics_query(params: &[u8]) -> Option<Marker> {
    let text = std::str::from_utf8(params).ok()?;
    let mut nums = text.split(';').map(|n| n.parse::<u16>().ok());
    let (item, action) = (nums.next()??, nums.next()??);
    Some(Marker::GraphicsQuery { item, action })
}

/// OSC 4, 10, 11 or 12 asking for colors, read as the emulator reads them
/// (vte's `osc_dispatch`), so the pipe answers what the terminal would: OSC 4
/// takes pairs of an index and `?` (or a color, which sets it and isn't
/// asked), and OSC 10, 11 and 12 take a `?` (or a color) for that color and
/// each after it, to the cursor's. None if nothing is asked.
fn parse_color_query(payload: &[u8], bell: bool) -> Option<Marker> {
    let text = std::str::from_utf8(payload).ok()?;
    // vte keeps the first 16 (its `MAX_OSC_PARAMS`, the code among them), so
    // OSC 4 asking for eight colors or more comes to it as an even number,
    // which it drops unanswered.
    let mut params = text.split(';').take(16);
    let code = params.next()?;
    let params: Vec<&str> = params.collect();
    // vte's numbers are a byte's.
    let number = |n: &str| (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.parse::<u8>().ok()).flatten();
    let mut colors = Vec::new();
    match code {
        "4" if !params.is_empty() && params.len().is_multiple_of(2) => {
            for pair in params.chunks(2) {
                if let (Some(index), "?") = (number(pair[0]), pair[1]) {
                    colors.push(index as usize);
                }
            }
        }
        "10" | "11" | "12" => {
            let first = 256 + number(code)? as usize - 10;
            for (index, param) in (first..=258).zip(&params) {
                if *param == "?" {
                    colors.push(index);
                }
            }
        }
        _ => {}
    }
    (!colors.is_empty()).then_some(Marker::ColorQuery { colors, bell })
}

/// The `key=value` fields of OSC 7731 (see [`FxRequest`]).
fn parse_fx(args: &str) -> Option<Marker> {
    let pair = |v: &str| -> Option<(f32, f32)> {
        let (a, b) = v.split_once(',')?;
        let (a, b) = (a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?);
        (a.is_finite() && b.is_finite()).then_some((a, b))
    };
    let cell = |v: &str| -> Option<(u16, u16)> {
        let (a, b) = v.split_once(',')?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    };
    let mut kind = None;
    let mut fx = FxRequest { kind: FxKind::Burst, at: (0, 0), to: None, color: None, size: None, dir: None, strength: 0.5, sound: FxSound::Kind, volume: 1.0 };
    let mut at = None;
    let mut sound_only = false;
    let mut tint = false;
    for field in args.split(';') {
        let Some((key, value)) = field.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "fx" if value == "sound" => sound_only = true,
            "fx" if value == "tint" => tint = true,
            "sound" => {
                fx.sound = match value {
                    "none" | "off" | "0" => FxSound::Quiet,
                    name => Cue::named(name).map_or(FxSound::Kind, FxSound::Named),
                }
            }
            "fx" => {
                kind = Some(match value {
                    "bolt" => FxKind::Bolt,
                    "burst" => FxKind::Burst,
                    "ripple" => FxKind::Ripple,
                    "stir" => FxKind::Stir,
                    _ => return None,
                })
            }
            "at" => at = cell(value),
            "to" => fx.to = cell(value),
            "color" => {
                let hex = value.strip_prefix('#').unwrap_or(value);
                let six = hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit());
                fx.color = u32::from_str_radix(hex, 16).ok().filter(|_| six).map(|c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]);
            }
            "size" => fx.size = value.parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(0.5, 40.0)),
            "dir" => {
                fx.dir = pair(value).and_then(|(x, y)| {
                    let len = x.hypot(y);
                    (len > 1e-6).then(|| (x / len, y / len))
                })
            }
            "strength" => {
                if let Some(v) = value.parse::<f32>().ok().filter(|v| v.is_finite()) {
                    fx.strength = v.clamp(0.0, 1.0);
                }
            }
            "volume" => {
                if let Some(v) = value.parse::<f32>().ok().filter(|v| v.is_finite()) {
                    fx.volume = v.clamp(0.0, 1.0);
                }
            }
            _ => {}
        }
    }
    if tint {
        let wash = fx.color.filter(|_| fx.strength > 0.0).map(|color| Wash { color, strength: fx.strength });
        return Some(Marker::Tint(wash));
    }
    if sound_only {
        let FxSound::Named(cue) = fx.sound else { return None };
        return Some(Marker::Sound(SoundRequest::Named { cue, col: at.map(|(col, _)| col), strength: fx.strength, volume: fx.volume }));
    }
    fx.kind = kind?;
    fx.at = at?;
    let incomplete = match fx.kind {
        FxKind::Bolt => fx.to.is_none(),
        FxKind::Stir => fx.dir.is_none(),
        FxKind::Burst | FxKind::Ripple => false,
    };
    (!incomplete).then_some(Marker::Fx(fx))
}

/// DECPS's parameters, `Pv ; Pd ; Pn …` and its `,`: a volume, a length
/// and at least one note. A note past 25 is a rest.
fn parse_decps(params: &[u8]) -> Option<Marker> {
    let text = std::str::from_utf8(params.strip_suffix(b",")?).ok()?;
    let mut nums = text.split(';').map(|n| if n.is_empty() { Some(0u16) } else { n.parse::<u16>().ok() });
    let volume = nums.next()??.min(7) as u8;
    let duration = nums.next()??.min(255) as u8;
    let notes: Vec<u8> = nums.map(|n| n.map(|n| if n <= 25 { n as u8 } else { 0 })).collect::<Option<_>>()?;
    (!notes.is_empty()).then_some(Marker::Sound(SoundRequest::Notes { volume, duration, notes }))
}

/// `st;pr` of OSC 9;4: state 0 clears, 1 is normal, 2 an error, 3 busy with
/// no figure, 4 paused; pr is a percentage.
fn parse_progress(args: &str) -> Option<Marker> {
    let mut parts = args.split(';');
    let state = match parts.next()?.trim() {
        "0" => return Some(Marker::Progress(None)),
        "1" => ProgressState::Normal,
        "2" => ProgressState::Error,
        "3" => ProgressState::Busy,
        "4" => ProgressState::Paused,
        _ => return None,
    };
    let percent = parts.next().and_then(|p| p.trim().parse::<u32>().ok()).unwrap_or(0).min(100) as u8;
    Some(Marker::Progress(Some(Progress { state, percent })))
}

/// Works on bytes: slicing the `&str` after a `%` could split a multibyte
/// character and panic the reader thread.
/// OSC 633;E's escaping, as VS Code reads it: `\\` is a backslash and
/// `\xHH` a byte, so the command can hold `;`, newlines and other controls.
fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let hex = |i: usize| bytes.get(i).and_then(|&b| (b as char).to_digit(16));
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if bytes.get(i + 1) == Some(&b'\\') {
                out.push(b'\\');
                i += 2;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'x') {
                if let (Some(hi), Some(lo)) = (hex(i + 2), hex(i + 3)) {
                    out.push((hi * 16 + lo) as u8);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratty_commands_are_found_in_the_stream_beside_kitty_ones() {
        let mut s = OscScanner::default();
        let pieces = s.feed(b"a\x1b_ratty;g;p;id=1;row=2;col=3;w=4;h=5\x1b\\b\x1b_Gi=1\x1b\\\x1b_other\x1b\\");
        let found: Vec<Marker> = markers(&pieces);
        assert_eq!(found, vec![Marker::Model(b"p;id=1;row=2;col=3;w=4;h=5".to_vec()), Marker::Graphics(b"i=1".to_vec())]);
        // Split across reads.
        let mut s = OscScanner::default();
        assert!(markers(&s.feed(b"\x1b_ratty;g;")).is_empty());
        assert_eq!(markers(&s.feed(b"s\x1b\\")), vec![Marker::Model(b"s".to_vec())]);
    }

    #[test]
    fn the_command_line_comes_unescaped_apart_from_its_nonce() {
        let said = |payload: &[u8]| match parse_payload(payload) {
            Some(Marker::CommandLine { line, nonce, newest: false }) => (line, nonce),
            other => panic!("{other:?}"),
        };
        let line = |payload: &[u8]| said(payload).0;
        assert_eq!(said(b"633;E;git status"), ("git status".into(), String::new()), "a program's, with none");
        assert_eq!(said(br"633;E;echo a\x3b echo b;nonce123"), ("echo a; echo b".into(), "nonce123".into()));
        assert_eq!(said(b"633;E;ls;0f1e;more").1, "0f1e;more", "what follows the nonce makes it another, which no pane's is");
        assert_eq!(said(b"633;E;;abc"), (String::new(), "abc".into()));
        assert_eq!(line(br"633;E;dir C:\\Users"), r"dir C:\Users");
        assert_eq!(line(br"633;E;for i in 1 2\x0ado echo $i\x0adone"), "for i in 1 2\ndo echo $i\ndone");
        assert_eq!(line("633;E;echo こんにちは".as_bytes()), "echo こんにちは");
        assert_eq!(line(br"633;E;a\xZZ \"), r"a\xZZ \", "what isn't an escape stays");
        assert_eq!(line(b"633;E"), "");
        // Every control character comes escaped, as the hooks write them: a
        // CAN or a SUB would end the OSC early.
        assert_eq!(said(br"633;E;a\x18b\x1ac\x7f\x01;5eed"), ("a\x18b\x1ac\x7f\x01".into(), "5eed".into()));
        // bash's newest line, which may not be the one run.
        let newest = |payload: &[u8]| match parse_payload(payload) {
            Some(Marker::CommandLine { line, nonce, newest: true }) => (line, nonce),
            other => panic!("{other:?}"),
        };
        assert_eq!(newest(br"633;E;ls -l\x3b true;5eed;newest"), ("ls -l; true".into(), "5eed".into()));
        assert_eq!(newest(b"633;E;;5eed;newest"), (String::new(), "5eed".into()));
        assert_eq!(newest(b"633;E;ls;;newest"), ("ls".into(), String::new()), "with no nonce, which no pane's is");
        assert_eq!(said(b"633;E;ls;5eed;newest;x").1, "5eed;newest;x", "only at the end");
    }

    #[test]
    fn bash_says_a_prompt_may_take_letters_past_ascii_as_keys() {
        assert_eq!(parse_payload(b"633;P;FlowMetaKeys=1"), Some(Marker::MetaKeys));
        assert_eq!(parse_payload(b"633;P;FlowMetaKeys=0"), None);
        assert_eq!(parse_payload(b"633;P;Cwd=/tmp"), Some(Marker::Cwd("/tmp".into())));
    }

    #[test]
    fn a_command_starts_and_ends_with_the_nonce_after_its_mark() {
        let start = |payload: &[u8]| match parse_payload(payload) {
            Some(Marker::CommandStart { nonce }) => nonce,
            other => panic!("{other:?}"),
        };
        let done = |payload: &[u8]| match parse_payload(payload) {
            Some(Marker::CommandDone { code, nonce }) => (code, nonce),
            other => panic!("{other:?}"),
        };
        assert_eq!(start(b"133;C"), "", "a program's, or a shell's that sends none");
        assert_eq!(start(b"133;C;0f1e"), "0f1e");
        assert_eq!(done(b"133;D"), (None, String::new()));
        assert_eq!(done(b"133;D;2"), (Some(2), String::new()));
        assert_eq!(done(b"133;D;130;0f1e"), (Some(130), "0f1e".into()));
        assert_eq!(done(b"133;D;;0f1e"), (None, "0f1e".into()), "cmd's, with no code");
        assert_eq!(done(b"133;D;1;0f1e;aid=3").1, "0f1e;aid=3", "what follows the nonce makes it another");
        assert_eq!(done(b"633;D;4;0f1e"), (Some(4), "0f1e".into()));
    }

    fn markers(pieces: &[Piece]) -> Vec<Marker> {
        pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Marker(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn finds_bel_terminated_markers() {
        let mut s = OscScanner::default();
        let input = b"out\x1b]133;D;2\x07\x1b]633;P;Cwd=C:\\Users\\me\x07C:\\Users\\me>\x1b]133;B\x07";
        let pieces = s.feed(input);
        assert_eq!(
            markers(&pieces),
            vec![
                Marker::CommandDone { code: Some(2), nonce: String::new() },
                Marker::Cwd("C:\\Users\\me".into()),
                Marker::PromptEnd
            ]
        );
        covers(&pieces, input.len());
    }

    /// Checks the ranges, given and kept from the emulator, cover a chunk
    /// `len` bytes long in order.
    fn covers(pieces: &[Piece], len: usize) {
        let mut covered = 0;
        for p in pieces {
            if let Piece::Data(a, b) | Piece::Drop(a, b) = p {
                assert_eq!(*a, covered, "{pieces:?}");
                assert!(a < b, "an empty range: {pieces:?}");
                covered = *b;
            }
        }
        assert_eq!(covered, len, "{pieces:?}");
    }

    /// What of `chunk` the emulator gets.
    fn given(pieces: &[Piece], chunk: &[u8]) -> Vec<u8> {
        pieces.iter().filter_map(|p| if let Piece::Data(a, b) = p { Some(&chunk[*a..*b]) } else { None }).flatten().copied().collect()
    }

    #[test]
    fn sixel_pictures_are_found_with_their_parameters_and_data_which_still_go_on() {
        let mut s = OscScanner::default();
        let input = b"a\x1bP0;1;0q\"1;1;2;6#0~~\x1b\\b\x1bPq~-~\x1b\\c\x1bP;;9q?\x1b[0m";
        let pieces = s.feed(input);
        assert_eq!(
            markers(&pieces),
            vec![
                Marker::Sixel { params: vec![0, 1, 0], data: b"\"1;1;2;6#0~~".to_vec() },
                Marker::Sixel { params: vec![0], data: b"~-~".to_vec() },
                Marker::Sixel { params: vec![0, 0, 9], data: b"?".to_vec() },
            ],
            "the last ended by a bare ESC"
        );
        covers(&pieces, input.len());
        assert_eq!(given(&pieces, input), input, "every byte still goes to the emulator");
        // Split across reads.
        let mut s = OscScanner::default();
        assert!(markers(&s.feed(b"\x1bP")).is_empty());
        assert!(markers(&s.feed(b"q#1~")).is_empty());
        assert!(markers(&s.feed(b"~\x1b")).is_empty());
        assert_eq!(markers(&s.feed(b"\\")), vec![Marker::Sixel { params: vec![0], data: b"#1~~".to_vec() }]);
        // Other DCSs are the emulator's: DECRQSS, XTGETTCAP, a synchronized
        // update, tmux's; and one cancelled by CAN.
        assert!(markers(&s.feed(b"\x1bP$qm\x1b\\\x1bP+q544e\x1b\\\x1bP=1s\x1b\\\x1bPtmux;q\x1b\\\x1bPq~\x18\x1b\\")).is_empty());
        // Still read after them.
        assert_eq!(markers(&s.feed(b"\x1b[16t")), vec![Marker::CellSizeQuery]);
    }

    #[test]
    fn an_iterm2_image_is_found_and_its_body_kept_from_the_emulator() {
        let mut s = OscScanner::default();
        let input = b"x\x1b]1337;File=name=YS5wbmc=;inline=1;width=2:QUJD\x07y";
        let pieces = s.feed(input);
        let args = crate::iterm::Args { name: Some("a.png".into()), width: crate::iterm::Extent::Cells(2), inline: true, ..crate::iterm::Args::default() };
        assert_eq!(markers(&pieces), vec![Marker::ITermImage { args, data: b"QUJD".to_vec() }]);
        covers(&pieces, input.len());
        assert_eq!(given(&pieces, input), b"x\x1b]1337;\x07y", "an empty OSC 1337 for the emulator");
        // Split across reads, ended by ST: each read's share of the body
        // is kept back, and the ST goes on.
        let mut s = OscScanner::default();
        let reads: [&[u8]; 4] = [b"\x1b]13", b"37;File=inli", b"ne=1:QUJD\x1b", b"\\z"];
        let mut emulator = Vec::new();
        let mut found = Vec::new();
        for read in reads {
            let pieces = s.feed(read);
            covers(&pieces, read.len());
            emulator.extend(given(&pieces, read));
            found.extend(markers(&pieces));
        }
        assert_eq!(emulator, b"\x1b]1337;\x1b\\z");
        assert!(matches!(found.as_slice(), [Marker::ITermImage { data, .. }] if data == b"QUJD"), "{found:?}");
        // Longer than other OSCs may be, it's still read.
        let long = format!("\x1b]1337;File=inline=1:{}\x07", "QUJD".repeat(5000));
        assert!(matches!(markers(&s.feed(long.as_bytes())).as_slice(), [Marker::ITermImage { data, .. }] if data.len() == 20_000));
        // A download, or another of iTerm2's commands, is no picture, but
        // its body is kept back all the same; and other OSCs go as before.
        let others = b"\x1b]1337;File=size=3:QUJD\x07\x1b]1337;SetMark\x07\x1b]2;title\x07";
        let pieces = s.feed(others);
        assert!(markers(&pieces).is_empty());
        assert_eq!(given(&pieces, others), b"\x1b]1337;\x07\x1b]1337;\x07\x1b]2;title\x07");
        // Cancelled by CAN, which goes on to cancel it in the emulator too.
        let cancelled = b"\x1b]1337;File=inline=1:QU\x18ok";
        let pieces = s.feed(cancelled);
        assert!(markers(&pieces).is_empty());
        assert_eq!(given(&pieces, cancelled), b"\x1b]1337;\x18ok");
    }

    #[test]
    fn programs_ask_what_sixel_pictures_may_hold_with_xtsmgraphics() {
        let mut s = OscScanner::default();
        let found = markers(&s.feed(b"\x1b[?1;1S\x1b[?2;1S\x1b[?2;4;0S\x1b[?1;3;256S\x1b[2S\x1b[?1S"));
        let asked = |item, action| Marker::GraphicsQuery { item, action };
        assert_eq!(found, vec![asked(1, 1), asked(2, 1), asked(2, 4), asked(1, 3)], "scrolling up isn't asking, and an item alone isn't either");
    }

    #[test]
    fn the_sixel_modes_and_the_alternate_screen_are_found_set_reset_and_asked_about() {
        let mut s = OscScanner::default();
        let input = b"\x1b[?80h\x1b[?8452;25l\x1b[?1049;80l\x1b[?25h\x1b[?80$p\x1b[?8452$p\x1b[?25$p\x1b[80h";
        let pieces = s.feed(input);
        let modes = |sixel_display, sixel_right, alternate| Marker::Modes(Modes { sixel_display, sixel_right, alternate });
        assert_eq!(
            markers(&pieces),
            vec![
                modes(Some(true), None, None),
                modes(None, Some(false), None),
                modes(Some(false), None, Some(false)),
                Marker::ModeQuery(80),
                Marker::ModeQuery(8452),
            ],
            "a mode Flow doesn't keep, or one that isn't DEC's, is the emulator's alone"
        );
        assert_eq!(given(&pieces, input), input, "every byte still goes to the emulator");
        // A full reset resets them, after the bare ESC that ends a string too.
        assert_eq!(markers(&s.feed(b"a\x1bcb\x1b]2;t\x1bc")), vec![Marker::Modes(Modes::RESET), Marker::Modes(Modes::RESET)]);
        assert_eq!(markers(&s.feed(b"\x1b(c\x1b[c")), vec![Marker::DeviceQuery], "ESC ( c isn't RIS");
        // The screen erased whole, not part of it.
        assert_eq!(markers(&s.feed(b"\x1b[2J\x1b[J\x1b[1J\x1b[3J\x1b[2K")), vec![Marker::Erased { scrollback: false }, Marker::Erased { scrollback: true }]);
    }

    #[test]
    fn an_iterm2_file_sent_in_parts_is_found_part_by_part() {
        let mut s = OscScanner::default();
        let input = b"\x1b]1337;MultipartFile=name=YS5wbmc=;inline=1;width=2\x07\x1b]1337;FilePart=QU\x07\x1b]1337;FilePart=JD\x1b\\\x1b]1337;FileEnd\x07z";
        let pieces = s.feed(input);
        let args = crate::iterm::Args { name: Some("a.png".into()), width: crate::iterm::Extent::Cells(2), inline: true, ..crate::iterm::Args::default() };
        assert_eq!(markers(&pieces), vec![Marker::ITermFileStart(args), Marker::ITermFilePart(b"QU".to_vec()), Marker::ITermFilePart(b"JD".to_vec()), Marker::ITermFileEnd]);
        covers(&pieces, input.len());
        assert_eq!(given(&pieces, input), b"\x1b]1337;\x07\x1b]1337;\x07\x1b]1337;\x1b\\\x1b]1337;\x07z", "each an empty OSC 1337 for the emulator");
        // A download's start is found too: its end shows nothing.
        assert_eq!(markers(&s.feed(b"\x1b]1337;MultipartFile=size=3\x07")), vec![Marker::ITermFileStart(crate::iterm::Args { size: Some(3), ..crate::iterm::Args::default() })]);
    }

    #[test]
    fn pictures_wrapped_for_tmux_are_found_as_they_would_be_unwrapped() {
        // As a program inside tmux sends them, for tmux to pass on: in
        // `DCS tmux; … ST`, with every ESC of what's inside doubled.
        let wrap = |inner: &[u8]| {
            let mut out = b"\x1bPtmux;".to_vec();
            for &b in inner {
                out.extend(if b == 0x1b { &b"\x1b\x1b"[..] } else { std::slice::from_ref(&b) });
            }
            out.extend(b"\x1b\\");
            out
        };
        let mut s = OscScanner::default();
        let input = [
            wrap(b"\x1bP0;1q\"1;1;2;6#0~~\x1b\\"),
            b"a".to_vec(),
            wrap(b"\x1b_Ga=T,f=24,s=1,v=1;AAAA\x1b\\"),
            wrap(b"\x1b]1337;File=inline=1:QUJD\x07"),
            b"\x1b[16t".to_vec(),
        ]
        .concat();
        let pieces = s.feed(&input);
        let found = markers(&pieces);
        assert_eq!(found[..2], [Marker::Sixel { params: vec![0, 1], data: b"\"1;1;2;6#0~~".to_vec() }, Marker::Graphics(b"a=T,f=24,s=1,v=1;AAAA".to_vec())]);
        assert!(matches!(&found[2..], [Marker::ITermImage { data, .. }, Marker::CellSizeQuery] if data == b"QUJD"), "{found:?}");
        covers(&pieces, input.len());
        // Split across reads, the same.
        let mut s = OscScanner::default();
        let found: Vec<Marker> = input.chunks(5).flat_map(|read| markers(&s.feed(read))).collect();
        assert_eq!(found.len(), 4, "{found:?}");
    }

    #[test]
    fn survives_chunk_split_and_bare_esc() {
        let mut s = OscScanner::default();
        let a = s.feed(b"\x1b]13");
        let b = s.feed(b"3;D\x1b\x1b]133;A\x07");
        assert!(markers(&a).is_empty());
        assert_eq!(markers(&b), vec![Marker::CommandDone { code: None, nonce: String::new() }, Marker::PromptStart]);
    }

    #[test]
    fn osc_9_4_reports_progress_and_9_9_still_reports_the_folder() {
        let progress = |state, percent| Some(Marker::Progress(Some(Progress { state, percent })));
        assert_eq!(parse_payload(b"9;4;1;45"), progress(ProgressState::Normal, 45));
        assert_eq!(parse_payload(b"9;4;2;100"), progress(ProgressState::Error, 100));
        assert_eq!(parse_payload(b"9;4;3"), progress(ProgressState::Busy, 0));
        assert_eq!(parse_payload(b"9;4;4;250"), progress(ProgressState::Paused, 100));
        assert_eq!(parse_payload(b"9;4;0;0"), Some(Marker::Progress(None)));
        assert_eq!(parse_payload(b"9;4;9;10"), None);
        assert_eq!(parse_payload(b"9;9;\"C:\\work\""), Some(Marker::Cwd("C:\\work".into())));
    }

    #[test]
    fn programs_ask_about_flows_settings_with_osc_7732() {
        // As the SDK writes it, found among other output, kept as it came.
        let asked = flow_sdk::settings::request(r#"{"do":"list"}"#);
        let mut scanner = OscScanner::default();
        let bytes = [b"before".as_slice(), &asked, b"after"].concat();
        let found: Vec<Marker> = scanner.feed(&bytes).into_iter().filter_map(|p| if let Piece::Marker(m) = p { Some(m) } else { None }).collect();
        assert!(matches!(found.as_slice(), [Marker::Settings(request)] if request == "eyJkbyI6Imxpc3QifQ=="), "{found:?}");
    }

    #[test]
    fn programs_ask_for_effects_with_osc_7731() {
        let fx = |text: &str| match parse_payload(text.as_bytes()) {
            Some(Marker::Fx(fx)) => Some(fx),
            _ => None,
        };
        let burst = fx("7731;fx=burst;at=12,3;color=ff8000;size=2.5;strength=0.9").unwrap();
        assert_eq!((burst.kind, burst.at, burst.color, burst.size, burst.strength), (FxKind::Burst, (12, 3), Some([255, 128, 0]), Some(2.5), 0.9));
        let bolt = fx("7731;fx=bolt;at=0,0;to=40,10").unwrap();
        assert_eq!((bolt.to, bolt.strength), (Some((40, 10)), 0.5));
        let stir = fx("7731;fx=stir;at=5,5;dir=3,-4;strength=7").unwrap();
        assert_eq!((stir.dir, stir.strength), (Some((0.6, -0.8)), 1.0));
        // Out of range values are brought in, bad ones left out, unknown keys skipped.
        let odd = fx("7731;fx=ripple;at=1,2;size=900;color=nope;speed=3;strength=x").unwrap();
        assert_eq!((odd.size, odd.color, odd.strength), (Some(40.0), None, 0.5));
        assert_eq!(fx("7731;fx=burst;at=1,2;color=#a0e0ff").unwrap().color, Some([0xa0, 0xe0, 0xff]));
        for bad in ["+12345", "##ff8000", "ff800", "ff80000", "-12345"] {
            assert_eq!(fx(&format!("7731;fx=burst;at=1,2;color={bad}")).unwrap().color, None, "{bad}");
        }
        // Unknown kinds, and kinds without what they need, are ignored.
        assert_eq!(fx("7731;fx=quake;at=1,1"), None);
        assert_eq!(fx("7731;fx=burst"), None);
        assert_eq!(fx("7731;fx=burst;at=-1,2"), None);
        assert_eq!(fx("7731;fx=bolt;at=1,1"), None);
        assert_eq!(fx("7731;fx=stir;at=1,1;dir=0,0"), None);
        // In the stream, ended by BEL or ST.
        let mut s = OscScanner::default();
        let found = markers(&s.feed(b"a\x1b]7731;fx=ripple;at=3,4\x07b\x1b]7731;fx=burst;at=5,6\x1b\\c"));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_program_washes_the_glass_in_a_color_and_off_again() {
        assert_eq!(parse_payload(b"7731;fx=tint;color=0c1838;strength=0.55"), Some(Marker::Tint(Some(Wash { color: [0x0c, 0x18, 0x38], strength: 0.55 }))));
        assert_eq!(parse_payload(b"7731;fx=tint;strength=0"), Some(Marker::Tint(None)));
        assert_eq!(parse_payload(b"7731;fx=tint;color=nope"), Some(Marker::Tint(None)), "without a color, none");
        assert_eq!(parse_payload(b"7731;fx=tint;color=ff0000;strength=7"), Some(Marker::Tint(Some(Wash { color: [0xff, 0, 0], strength: 1.0 }))));
    }

    #[test]
    fn a_program_sets_its_effects_sound_volume_apart_from_their_strength() {
        let fx = |text: &str| match parse_payload(text.as_bytes()) {
            Some(Marker::Fx(fx)) => Some(fx),
            _ => None,
        };
        let quieter = fx("7731;fx=burst;at=1,2;strength=0.9;volume=0.3").unwrap();
        assert_eq!((quieter.strength, quieter.volume), (0.9, 0.3));
        assert_eq!(fx("7731;fx=burst;at=1,2").unwrap().volume, 1.0, "full unless given");
        assert_eq!(fx("7731;fx=burst;at=1,2;volume=5").unwrap().volume, 1.0);
        assert_eq!(parse_payload(b"7731;fx=sound;sound=chime;volume=0.5"), Some(Marker::Sound(SoundRequest::Named { cue: Cue::Chime, col: None, strength: 0.5, volume: 0.5 })));
    }

    #[test]
    fn programs_ask_for_sounds_by_name_and_play_decps_notes() {
        let ripple = match parse_payload(b"7731;fx=ripple;at=1,2;sound=boing") {
            Some(Marker::Fx(fx)) => fx,
            other => panic!("{other:?}"),
        };
        assert_eq!(ripple.sound, FxSound::Named(Cue::Boing));
        let quiet = parse_payload(b"7731;fx=burst;at=1,2;sound=none");
        assert!(matches!(quiet, Some(Marker::Fx(FxRequest { sound: FxSound::Quiet, .. }))));
        let unknown = parse_payload(b"7731;fx=burst;at=1,2;sound=kazoo");
        assert!(matches!(unknown, Some(Marker::Fx(FxRequest { sound: FxSound::Kind, .. }))), "an unknown name keeps its kind's");
        assert_eq!(
            parse_payload(b"7731;fx=sound;sound=chime;at=30,4;strength=0.8"),
            Some(Marker::Sound(SoundRequest::Named { cue: Cue::Chime, col: Some(30), strength: 0.8, volume: 1.0 }))
        );
        assert_eq!(parse_payload(b"7731;fx=sound;sound=kazoo"), None, "a sound alone needs a name Flow knows");
        // DECPS, in the stream: volume, length, notes; a note past 25 rests.
        let mut s = OscScanner::default();
        let found = markers(&s.feed(b"\x1b[7;8;1;5;8;30,~\x1b[3;16;13,~\x1b[5;4,~"));
        assert_eq!(
            found,
            vec![
                Marker::Sound(SoundRequest::Notes { volume: 7, duration: 8, notes: vec![1, 5, 8, 0] }),
                Marker::Sound(SoundRequest::Notes { volume: 3, duration: 16, notes: vec![13] }),
            ],
            "the last has no notes"
        );
        // Other CSIs ending in ~ (keys, bracketed paste) aren't notes.
        assert!(markers(&s.feed(b"\x1b[3~\x1b[200~x\x1b[201~")).is_empty());
        // A CSI too long to read is passed over, not read cut short.
        let long = format!("\x1b[7;8;{}1,~", "1;".repeat(60));
        assert!(markers(&s.feed(long.as_bytes())).is_empty());
        assert_eq!(markers(&s.feed(b"\x1b[16t")), vec![Marker::CellSizeQuery]);
    }

    #[test]
    fn an_osc_too_long_to_keep_is_dropped_rather_than_read_cut_short() {
        // One byte too long: cut at the limit, it would read as a burst at
        // row 1, not 12.
        let long = format!("\x1b]7731;fx=burst;{}at=0,12\x07", "x=y;".repeat(1019));
        assert_eq!(long.len() - 3, MAX_PAYLOAD + 1);
        let mut s = OscScanner::default();
        assert!(markers(&s.feed(long.as_bytes())).is_empty());
        // And the next one is read as usual.
        assert_eq!(markers(&s.feed(b"\x1b]133;A\x07")), vec![Marker::PromptStart]);
    }

    #[test]
    fn graphics_commands_and_queries_are_found_across_chunks() {
        let mut s = OscScanner::default();
        let a = s.feed(b"x\x1b_Ga=q,i=1;QU");
        let b = s.feed(b"JD\x1b\\y\x1b[>q\x1b[16t\x1b[1;31m\x1b[>0q");
        assert!(markers(&a).is_empty());
        assert_eq!(
            markers(&b),
            vec![
                Marker::Graphics(b"a=q,i=1;QUJD".to_vec()),
                Marker::VersionQuery,
                Marker::CellSizeQuery,
                Marker::VersionQuery,
            ]
        );
        // Other APCs, and a bare ESC ending one (ConPTY), leave no marker.
        assert!(markers(&s.feed(b"\x1b_Xother\x1b\\\x1b_Gm=0\x1b[0m")).len() == 1);
        // The pipe's own: the text area's size and DA1, not DA2 or a cursor.
        let pipe = s.feed(b"\x1b[14t\x1b[c\x1b[0c\x1b[>c\x1b[14;2t\x1b[2c");
        assert_eq!(markers(&pipe), vec![Marker::AreaSizeQuery, Marker::DeviceQuery, Marker::DeviceQuery]);
    }

    #[test]
    fn colors_asked_for_are_found_with_how_the_question_ended() {
        let asked = |colors: &[usize], bell| Marker::ColorQuery { colors: colors.to_vec(), bell };
        let mut s = OscScanner::default();
        // The background, by BEL and by ST, and split across reads.
        assert_eq!(markers(&s.feed(b"\x1b]11;?\x07\x1b]11;?\x1b\\")), vec![asked(&[257], true), asked(&[257], false)]);
        assert!(markers(&s.feed(b"\x1b]1")).is_empty());
        assert_eq!(markers(&s.feed(b"1;?\x1b\\")), vec![asked(&[257], false)]);
        // The text's and the cursor's; each `?` after the first asks for the
        // next color, to the cursor's, as the emulator reads it.
        assert_eq!(markers(&s.feed(b"\x1b]10;?\x07\x1b]12;?\x07\x1b]10;?;?\x07\x1b]11;?;?;?\x07")), vec![
            asked(&[256], true),
            asked(&[258], true),
            asked(&[256, 257], true),
            asked(&[257, 258], true),
        ]);
        // The palette's, in pairs, those set passed over.
        assert_eq!(markers(&s.feed(b"\x1b]4;1;?;2;rgb:ff/00/00;255;?\x07")), vec![asked(&[1, 255], true)]);
        // Seven pairs at most, as the emulator keeps 16 parameters: eight
        // reach it cut to an even number, which it doesn't answer.
        assert_eq!(markers(&s.feed(b"\x1b]4;0;?;1;?;2;?;3;?;4;?;5;?;6;?\x07")), vec![asked(&[0, 1, 2, 3, 4, 5, 6], true)]);
        assert!(markers(&s.feed(b"\x1b]4;0;?;1;?;2;?;3;?;4;?;5;?;6;?;7;?\x07")).is_empty());
        // Setting colors, or asking for something else, isn't asking for them.
        assert!(markers(&s.feed(b"\x1b]11;rgb:ff/ff/ff\x07\x1b]4;1\x07\x1b]4;256;?\x07\x1b]13;?\x07\x1b]2;?\x07\x1b]11\x07")).is_empty());
    }

    #[test]
    fn line_sizes_are_found_after_their_escape_even_split_across_reads() {
        let mut s = OscScanner::default();
        let input = b"\x1b#3Big\r\n\x1b#4Big\r\n\x1b#6Wide\x1b#5\x1b#8";
        let pieces = s.feed(input);
        let sizes = vec![Marker::LineSize(LineSize::Top), Marker::LineSize(LineSize::Bottom), Marker::LineSize(LineSize::Wide), Marker::LineSize(LineSize::Single)];
        assert_eq!(markers(&pieces), sizes, "DECALN (`ESC # 8`) is the emulator's");
        // Each comes after its own bytes, which still go to the emulator.
        assert_eq!(pieces[0], Piece::Data(0, 3));
        assert_eq!(pieces[2], Piece::Data(3, 11));
        let mut s = OscScanner::default();
        assert!(markers(&s.feed(b"\x1b")).is_empty());
        assert!(markers(&s.feed(b"#")).is_empty());
        assert_eq!(markers(&s.feed(b"6x")), vec![Marker::LineSize(LineSize::Wide)]);
        // Not inside a string, nor after another escape.
        assert!(markers(&s.feed(b"\x1b]2;a#3\x07\x1b_Xa#3\x1b\\\x1b(#3")).is_empty());
    }

    #[test]
    fn osc7_cwd() {
        assert_eq!(
            parse_payload(b"7;file://host/home/me/my%20dir"),
            Some(Marker::Cwd("/home/me/my dir".into()))
        );
    }

    #[test]
    fn percent_decoding_keeps_multibyte_text_and_stray_percents() {
        // A '%' and one hex digit before a multibyte character used to panic.
        assert_eq!(percent_decode("/tmp/%aé/50%"), "/tmp/%aé/50%");
        assert_eq!(percent_decode("/caf%C3%A9/%é"), "/café/%é");
        // Only two hex digits count; from_str_radix also took a '+' sign.
        assert_eq!(percent_decode("/a%+1b"), "/a%+1b");
    }
}
