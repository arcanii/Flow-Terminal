//! Built-in looks: Flow's own, and some after old machines (two 8-bit home
//! computers, a workstation, a graphics workstation, a minicomputer at a
//! green screen, an amber screen or a teletype's paper, a school's micro, a
//! PC, and a 16-bit home computer's shell window). A look sets what a
//! preset file holds (`config::PRESET_SECTIONS`): the colors, the cursor,
//! the fonts, the liquid and the CRT. An old machine also has a start
//! screen, typed out at the speed of its line before a tab's first prompt,
//! and a prompt it gives the shells (`shell::set_prompt`).
//!
//! The machines are named for how they look, not for their makers, and
//! their start screens are written in their spirit rather than copied. Their
//! words are the machines' own, in no language but theirs, so they aren't
//! translated. Fonts of their time are tried first by name, and one is
//! built into Flow for each (`render/atlas.rs`): VT323, Bedstead, Gallant,
//! SGI's Screen, Flow Teletype (Flow's own, as no free font has the
//! teletype's letters), Flow Instruments (Flow's own too, the home
//! computer's dots), Flow notaTopaz (Flow's own, after the 16-bit home
//! computer's letters, whose own font isn't free) and unscii's two sizes.
//! The home computer and the PC try their makers' own first where the user
//! has them installed.
//!
//! A start screen's lines can be colored and made double as teletext's
//! were: the school computer's starts with a page of colored teletext, its
//! title double height over a picture in mosaics (see [`start_screen`]).
//! Colors written as `#rrggbb` color them too, as the cyan home computer's
//! bars are its own video chip's.
//!
//! The user's own looks come after these: their own theme (`[theme]` over
//! whatever's set), and the themes they saved under a name
//! (`themes.saved`, [`save_as`]), each what was worn then, as a preset
//! file holds it, worn again as a built-in look is.

use unicode_width::UnicodeWidthChar;

use crate::config::{Config, CrtConfig, FontConfig, FxConfig, SavedTheme, ThemeConfig};
use crate::hardware::{shown_at, shown_kind, shown_name, shown_path, Bank, Hardware, Probe};
use crate::line_size::LineSize;
use crate::render::palette;
use crate::shell::{BashFlavor, ShellKind, ZSH_LITERALS};

pub mod packaged;

/// One look.
#[derive(Debug)]
pub struct Look {
    pub id: &'static str,
    /// Fonts of its time to try first, by name.
    fonts: &'static [&'static str],
    text: palette::Rgb8,
    cursor: palette::Rgb8,
    background: palette::Rgb8,
    /// The window's margin, round the text, in a color of its own.
    border: Option<palette::Rgb8>,
    /// Programs' sixteen colors as its terminal had them (black, red,
    /// green, yellow, blue, magenta, cyan, white, then the bright ones); none
    /// keeps Flow's, made for dark glass.
    ansi: Option<[palette::Rgb8; 16]>,
    /// The one color its tube showed everything in, a monochrome screen's
    /// phosphor (`colors.phosphor`); none for a screen in color.
    phosphor: Option<palette::Rgb8>,
    cursor_shape: &'static str,
    blink: bool,
    /// How fast its cursor glides (`cursor.speed`): a teletype's print head
    /// heavy, a PC's quick, as the slider's steps of a half go.
    cursor_speed: f32,
    /// The built-in CRT preset it's seen through, if any.
    crt: Option<&'static str>,
    /// Its start screen, a line each, as [`start_screen`] reads them: one
    /// starting with `^` is centered, `{tall}` makes one double height,
    /// `{red}` and the like color it, and `{version}`, `{os}`, `{chip}`,
    /// `{memory}` ("16 GB"), `{mb}`, `{bytes}` and `{shell}` stand for what
    /// they say. `{banks}` and `{probes}`, each a line alone, become the
    /// PROM's lines probing the memory banks and the buses' slots.
    start: &'static [&'static str],
    /// The bits a second it's typed at, ten to a letter; 0 shows it at once.
    pub baud: u32,
    /// The prompt it gives the shells: `\n` a new line, `{cwd}` the folder,
    /// `{n}` the command's number in the history.
    pub prompt: Option<&'static str>,
    /// What the machine sounded like, for its sounds while its look is on.
    pub sounds: Sounds,
    /// Its screen, as columns and rows of characters, which Settings' Fit
    /// window to offers first; none for Flow's.
    pub screen: Option<(usize, usize)>,
    /// A shell window's frame round each pane wearing it, as its desktop
    /// drew one (the Flow notanAmiga CLI's): its title bar, the tab's name
    /// and three lines, and an outline (Theme details' Window frame).
    pub frame: bool,
    /// More of the look's settings (`THEME_KEYS`, as a saved theme holds
    /// them), put on after the rest: a packaged theme's (`packaged`), none
    /// for the built-in ones.
    keys: Option<&'static serde_json::Map<String, serde_json::Value>>,
}

/// What an old machine sounded like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sounds {
    /// Nothing of its own: Flow's look.
    Quiet,
    /// A tube's hum, and its keys' clicks.
    Tube,
    /// A teletype: each letter it prints, and each key struck, clacks; its
    /// bell rings as a line typed nears the end, its carriage runs back at
    /// each return, and its paper tape punch chatters as a command's output
    /// comes.
    Teletype,
    /// A tube's hum and its keys' clicks, and a disk drive seeking as the
    /// machine starts.
    Disk,
    /// A disk drive's, and a fan whirring with the hum.
    Fan,
    /// A tube's hum and its keys' clicks, and a printer printing each line
    /// of a command's output.
    Printer,
    /// A video terminal: a tube's hum, its own speaker's sharper key
    /// clicks, and its beep for the bell.
    Terminal,
    /// A tube's hum and its keys' clicks, and a chord as the machine starts.
    Chime,
    /// A home computer's sound chip through the television: a tube's hum
    /// and its keys' clicks, a high beep as the machine starts, as the key
    /// that began it was taken, and a low honk, which met a mistake, for
    /// the bell.
    Tones,
    /// A home computer's floppy drive beside its monitor: a tube's hum and
    /// its keys' clicks, the drive seeking as the machine starts and as its
    /// disk goes in, and clicking while it's empty, as its first screen
    /// waits for the disk.
    Floppy,
}

const PAPER: palette::Rgb8 = [0xEF, 0xE8, 0xD8];
const INK: palette::Rgb8 = [0x26, 0x24, 0x22];
const GREEN: palette::Rgb8 = [0x41, 0xFF, 0x6E];
const AMBER: palette::Rgb8 = [0xFF, 0xB0, 0x00];
const TERMINAL_FONTS: &[&str] = &["VT323", "Glass TTY VT220", "IBM 3270", "3270 Nerd Font Mono"];

/// Sixteen colors written as 0xRRGGBB.
const fn sixteen(hex: [u32; 16]) -> [palette::Rgb8; 16] {
    let mut out = [[0; 3]; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = [(hex[i] >> 16) as u8, (hex[i] >> 8) as u8, hex[i] as u8];
        i += 1;
    }
    out
}

/// The home computer's own colors, a shade lighter, as its text is, to read
/// on its blue through the tube.
const HOME_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000000, 0x9A6759, 0x9AD284, 0xB8C76F, 0xA49BF5, 0xB07FC8, 0x70A4B2, 0xD0D0D0, //
    0x6C6C6C, 0xC08B7E, 0xB8E6A4, 0xD8E28F, 0xC4BDFF, 0xC9A0DC, 0x98C8D4, 0xFFFFFF,
]);
/// A workstation console's, dark enough for its white: "white" is the
/// strongest grey, since white on white can't be read.
const WORKSTATION_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000000, 0xB00000, 0x007A00, 0x8A6A00, 0x0000C0, 0xA000A0, 0x007F80, 0x707070, //
    0x505050, 0xE00000, 0x00A000, 0xB08800, 0x3050FF, 0xD000D0, 0x00A0A0, 0x303030,
]);
/// A graphics workstation's terminal on its warm grey, deep as its window
/// colors were.
const GRAPHICS_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000000, 0xA01818, 0x1E6E1E, 0x7A5A00, 0x1F3FA0, 0x7A2A7A, 0x106A6A, 0x5A5A5A, //
    0x404040, 0xC82020, 0x2A8C2A, 0x9C7400, 0x2F5FD0, 0xA040A0, 0x188A8A, 0x2A2A2A,
]);
/// One phosphor, in its shades from dim to bright.
const GREEN_COLORS: [palette::Rgb8; 16] = sixteen([
    0x0A2A12, 0x2FAF4F, 0x41FF6E, 0x7CFF9A, 0x2A8C44, 0x36C95A, 0x5CFF86, 0x9CFFB4, //
    0x1E5A2C, 0x4FD873, 0x6BFF8F, 0xACFFC2, 0x3AA35A, 0x5FE07F, 0x86FFA6, 0xD0FFDC,
]);
const AMBER_COLORS: [palette::Rgb8; 16] = sixteen([
    0x2A1A00, 0xB87A00, 0xFFB000, 0xFFC640, 0xA06A00, 0xD89400, 0xFFBE30, 0xFFD27A, //
    0x5A3C00, 0xE09A00, 0xFFC020, 0xFFD860, 0x8A5C00, 0xF0A800, 0xFFCC50, 0xFFE6A8,
]);
/// A teletype's ribbon: its ink, with red where a ribbon had it.
const TELETYPE_COLORS: [palette::Rgb8; 16] = sixteen([
    0x262422, 0x8B2A1E, 0x262422, 0x262422, 0x262422, 0x262422, 0x262422, 0x5A554C, //
    0x6A645A, 0xA83A2A, 0x111111, 0x111111, 0x111111, 0x111111, 0x111111, 0x111111,
]);
/// Teletext's eight, bright and plain alike.
const SCHOOL_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000000, 0xFF0000, 0x00FF00, 0xFFFF00, 0x0000FF, 0xFF00FF, 0x00FFFF, 0xFFFFFF, //
    0x000000, 0xFF0000, 0x00FF00, 0xFFFF00, 0x0000FF, 0xFF00FF, 0x00FFFF, 0xFFFFFF,
]);
/// The cyan home computer's video chip's colors, each made as much darker
/// as it takes to read on its cyan: the plain ones by 4.5 to 1, the bright
/// by 3. Its "white" is a grey, its bright white the strongest, as the
/// workstation's are.
const INSTRUMENTS_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000000, 0xA03D3A, 0x146C24, 0x665D28, 0x4A4BD1, 0x8D4083, 0x1C656A, 0x5C5C5C, //
    0x797979, 0xD54747, 0x178B2E, 0x86784B, 0x6F69E1, 0xB652A8, 0x25848A, 0x2A2A2A,
]);
/// The cyan home computer's screen, and its border round it.
const INSTRUMENTS_CYAN: palette::Rgb8 = [0x42, 0xEC, 0xF6];
/// A row of its title screen's bars: sixteen of its colors two columns
/// each, in its own order.
const BARS: &str = "^{on-#d5524d}  {on-#5edd78}  {on-#000000}  {on-#e7cf81}  {on-#21b13b}  {on-#ca5bbb}  {on-#ffffff}  {on-#5455ee}  {on-#21c942}  {on-#ca5bbb}  {on-#fd5554}  {on-#cdcdcd}  {on-#7d76fd}  {on-#ff7978}  {on-#d5c254}  {on-#d5524d}  ";

/// The PC's sixteen, as its color adapter made them.
const PC_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA, 0xAAAAAA, //
    0x555555, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
]);

/// The sixteen-bit home computer's desktop of 1988 had four colors: its
/// blue, its white, its black and its orange, each a step of its sixteen
/// to a channel.
const WORKBENCH_BLUE: palette::Rgb8 = [0x00, 0x55, 0xAA];
const WORKBENCH_WHITE: palette::Rgb8 = [0xFF, 0xFF, 0xFF];
const WORKBENCH_ORANGE: palette::Rgb8 = [0xFF, 0x88, 0x00];
/// Programs' sixteen on its blue: its own black, and its orange for yellow,
/// as its shell's third and fourth pens were; the rest steps of its sixteen
/// made as much lighter as it takes to read on the blue, the plain ones
/// and the bright black by 3 to 1, the other bright ones by 4.5.
const NOTANAMIGA_COLORS: [palette::Rgb8; 16] = sixteen([
    0x000022, 0xFF8877, 0x44CC44, 0xFF8800, 0x88AAFF, 0xFF77FF, 0x44CCCC, 0xCCCCCC, //
    0x99AACC, 0xFFBBBB, 0x66EE66, 0xFFCC66, 0xBBCCFF, 0xFFBBFF, 0x66EEEE, 0xFFFFFF,
]);
/// Flow's look first, then the machines.
pub static LOOKS: [Look; 11] = [
    Look {
        id: "flow",
        fonts: &[],
        text: palette::FOREGROUND,
        cursor: palette::CURSOR,
        background: palette::BACKGROUND,
        border: None,
        ansi: None,
        phosphor: None,
        cursor_shape: "block",
        blink: false,
        cursor_speed: 1.0,
        crt: None,
        start: &[],
        baud: 0,
        prompt: None,
        sounds: Sounds::Quiet,
        screen: None,
        frame: false,
        keys: None,
    },
    // Light blue on blue inside a light blue border, on a television.
    Look {
        id: "home",
        fonts: &["C64 Pro Mono", "Pet Me 64", "PetMe64", "Commodore 64 Pixelized", "unscii-8"],
        // A shade lighter than the machine's own, to read through the tube.
        text: [0xA4, 0x9B, 0xF5],
        cursor: [0xA4, 0x9B, 0xF5],
        background: [0x40, 0x31, 0x8D],
        border: Some([0x8C, 0x7F, 0xE2]),
        ansi: Some(HOME_COLORS),
        phosphor: None,
        cursor_shape: "block",
        blink: true,
        cursor_speed: 1.5,
        crt: Some("television"),
        start: &["", "^**** FLOW HOME COMPUTER ****", "", "^{memory} RAM SYSTEM  {bytes} BASIC BYTES FREE", ""],
        baud: 0,
        prompt: Some("READY.\n"),
        sounds: Sounds::Disk,
        screen: Some((40, 25)),
        frame: false,
        keys: None,
    },
    // Black on white, as a workstation's console.
    Look {
        id: "workstation",
        fonts: &["Gallant Raster Term"],
        text: [0x00, 0x00, 0x00],
        cursor: [0x00, 0x00, 0x00],
        background: [0xFF, 0xFF, 0xFF],
        border: None,
        ansi: Some(WORKSTATION_COLORS),
        phosphor: None,
        cursor_shape: "block",
        blink: false,
        cursor_speed: 2.0,
        crt: None,
        // As a SPARCstation's PROM booted it: the computer's own memory
        // banks and buses probed, its maker's emblem beside the banner, then
        // `ok boot`. Its help is Flow's, `flow-help` in the tab.
        start: &[
            "{banks}",
            "{probes}",
            "",
            "{logo}{beside}Flow notaSun Workstation, Keyboard Present",
            "{beside}ROM Rev. {version}, {mb} MB memory installed, Serial #{serial}.",
            "{beside}Ethernet address {ether}, Host ID: {hostid}.",
            "{beside}{chip}",
            "",
            "Type  flow-help  for more information",
            "ok boot",
            "Boot device: disk  File and args: {shell}",
            "",
        ],
        baud: 9600,
        prompt: Some("% "),
        sounds: Sounds::Printer,
        // Its PROM's console, on a screen 1152 pixels wide.
        screen: Some((80, 34)),
        frame: false,
        keys: None,
    },
    // Dark text on warm grey, a graphics workstation's terminal.
    Look {
        id: "graphics",
        fonts: &["SGI Screen"],
        text: [0x1A, 0x1A, 0x1A],
        cursor: [0x3B, 0x5B, 0xA9],
        background: [0xD6, 0xD2, 0xC4],
        border: None,
        ansi: Some(GRAPHICS_COLORS),
        phosphor: None,
        cursor_shape: "block",
        blink: true,
        cursor_speed: 2.0,
        crt: None,
        // As its PROM started up: the cube beside the words, as on its
        // screen, then IRIX's banner, here Flow's.
        start: &[
            "Running power-on diagnostics...",
            "",
            "{logo}{beside}Starting up the system...",
            "{beside}",
            "{beside}To perform system maintenance instead, press <Esc>",
            "{beside}",
            "",
            "Flow Release {version} IP35 Version {serial} System V",
            "{chip}, Main memory size: {mb} Mbytes",
            "Ethernet (ec0) address {sgi_ether}",
            "",
            "The system is coming up.",
            "",
        ],
        baud: 19200,
        prompt: Some("{n}% "),
        sounds: Sounds::Chime,
        // Its terminal window as it first opened.
        screen: Some((80, 40)),
        frame: false,
        keys: None,
    },
    // A minicomputer's terminal, green phosphor.
    Look {
        id: "green",
        fonts: TERMINAL_FONTS,
        text: GREEN,
        cursor: GREEN,
        background: [0x02, 0x0B, 0x04],
        border: None,
        ansi: Some(GREEN_COLORS),
        phosphor: Some(GREEN),
        cursor_shape: "block",
        blink: true,
        cursor_speed: 1.5,
        crt: Some("phosphor"),
        start: &["FLOW-11 MINICOMPUTER", "{memory} OF CORE", "", "BOOTING TIME-SHARING SYSTEM ...", ""],
        baud: 1200,
        prompt: Some("$ "),
        sounds: Sounds::Terminal,
        screen: Some((80, 24)),
        frame: false,
        keys: None,
    },
    // The same at an amber terminal.
    Look {
        id: "amber",
        fonts: TERMINAL_FONTS,
        text: AMBER,
        cursor: AMBER,
        background: [0x12, 0x09, 0x00],
        border: None,
        ansi: Some(AMBER_COLORS),
        phosphor: Some(AMBER),
        cursor_shape: "block",
        blink: true,
        cursor_speed: 1.5,
        crt: Some("classic"),
        start: &["FLOW VIDEO TERMINAL   SET-UP: 9600 BAUD, NO PARITY", "", "CONNECTED TO FLOW-11 ({os})", ""],
        baud: 2400,
        prompt: Some("$ "),
        sounds: Sounds::Terminal,
        screen: Some((80, 24)),
        frame: false,
        keys: None,
    },
    // Dark ink on a teletype's paper.
    Look {
        id: "teletype",
        fonts: &["Flow Teletype", "Special Elite"],
        text: INK,
        cursor: INK,
        background: PAPER,
        border: None,
        ansi: Some(TELETYPE_COLORS),
        phosphor: None,
        cursor_shape: "underline",
        blink: false,
        cursor_speed: 0.5,
        crt: None,
        start: &["FLOW TIME-SHARING SERVICE", "LINE 1 ON LINE AT 300 BAUD", ""],
        baud: 300,
        prompt: Some("% "),
        sounds: Sounds::Teletype,
        // A line of the Model 33's paper, 72 letters long; paper has no
        // rows, so a terminal's 24.
        screen: Some((72, 24)),
        frame: false,
        keys: None,
    },
    // A British school's micro in its teletext mode: white on black, on the
    // classroom's monitor, its letters in a teletext font where one's
    // installed. (A television's curve would bend its top-left corner, where
    // the text begins.)
    Look {
        id: "school",
        fonts: &["Bedstead", "ModeSeven", "Mode Seven"],
        text: [0xFF, 0xFF, 0xFF],
        cursor: [0xFF, 0xFF, 0xFF],
        background: [0x00, 0x00, 0x00],
        border: None,
        ansi: Some(SCHOOL_COLORS),
        phosphor: None,
        cursor_shape: "underline",
        blink: true,
        cursor_speed: 1.0,
        crt: Some("classic"),
        // A page of colored teletext: the machine's name double height on a
        // band of blue, over a boat at sea in mosaics (the sea's foot in
        // separated ones). Then its disc system and BASIC, as it started.
        start: &[
            "",
            "^{tall}{on-blue}{yellow} Flow School Computer 32K ",
            "",
            "^            {white}🬵█🬹🬏     🬏🬏        {yellow}🬹██🬹     ",
            "^          {white}🬞🬻████🬺🬱   🬁  🬢🬃     {yellow}🬎██🬎     ",
            "^        {red}🬁🬎███████🬝🬆                     ",
            "^{cyan}🬵🬻█🬺🬱🬵🬻█🬺🬱🬵🬻█🬺🬱🬵🬻█🬺🬱🬵🬻█🬺🬱🬵🬻█🬺🬱🬵🬻█🬺🬱🬵🬻█🬺🬱",
            "^{blue}𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏𜺏",
            "",
            "Flow DFS",
            "",
            "BASIC",
            "",
        ],
        baud: 0,
        prompt: Some(">"),
        sounds: Sounds::Tube,
        // Teletext's 40 columns, and its 25 rows.
        screen: Some((40, 25)),
        frame: false,
        keys: None,
    },
    // An eighties personal computer: grey on black, its BIOS's memory test
    // as it starts, and its hard disk's prompt.
    Look {
        id: "pc",
        fonts: &["Px437 IBM VGA8", "PxPlus IBM VGA8", "Perfect DOS VGA 437", "Web437 IBM VGA8", "unscii-16"],
        text: [0xAA, 0xAA, 0xAA],
        cursor: [0xAA, 0xAA, 0xAA],
        background: [0x00, 0x00, 0x00],
        border: None,
        ansi: Some(PC_COLORS),
        phosphor: None,
        cursor_shape: "underline",
        blink: true,
        cursor_speed: 2.5,
        crt: Some("classic"),
        start: &["Flow Personal Computer BIOS Version {version}", "Memory Test: {kb}K OK", "", "Starting Flow-DOS...", ""],
        baud: 4800,
        prompt: Some("C:\\>"),
        sounds: Sounds::Fan,
        screen: Some((80, 25)),
        frame: false,
        keys: None,
    },
    // An early-eighties home computer on a television: black on cyan, its
    // title screen between bands of its colors with a logo in the middle,
    // then its BASIC, ready.
    Look {
        id: "instruments",
        fonts: &["Flow Instruments", "unscii-8"],
        text: [0x00, 0x00, 0x00],
        cursor: [0x00, 0x00, 0x00],
        background: INSTRUMENTS_CYAN,
        // Its border was its screen's own color.
        border: Some(INSTRUMENTS_CYAN),
        ansi: Some(INSTRUMENTS_COLORS),
        phosphor: None,
        cursor_shape: "block",
        blink: true,
        cursor_speed: 1.5,
        crt: Some("television"),
        // Its title screen row for row on its 32 × 24, the startup logo
        // where its maker's was, waiting for a key as it said; then the
        // screen cleared for its BASIC's first words at its foot, the
        // prompt a line below.
        start: &[
            BARS,
            BARS,
            BARS,
            "",
            "^{logo}",
            "",
            "",
            "",
            "",
            "^FLOW INSTRUMENTS",
            "",
            "^HOME COMPUTER",
            "",
            "",
            "",
            "",
            "^READY-PRESS ANY KEY TO BEGIN",
            "",
            BARS,
            BARS,
            BARS,
            "",
            "^©1981  FLOW INSTRUMENTS",
            "{wait}",
            "{clear}{foot}",
            " FLOW BASIC READY",
            "",
        ],
        baud: 0,
        prompt: Some(">"),
        sounds: Sounds::Tones,
        screen: Some((32, 24)),
        frame: false,
        keys: None,
    },
    // A sixteen-bit home computer's shell window, on its desktop of 1988:
    // white on its blue, an orange block for the cursor, letters of 8 by 8
    // tall dots, on its own monitor.
    Look {
        id: "amiga",
        fonts: &["Flow notaTopaz", "unscii-16"],
        text: WORKBENCH_WHITE,
        cursor: WORKBENCH_ORANGE,
        background: WORKBENCH_BLUE,
        border: None,
        ansi: Some(NOTANAMIGA_COLORS),
        phosphor: None,
        cursor_shape: "block",
        blink: false,
        cursor_speed: 1.5,
        crt: Some("monitor"),
        // As it started: its first screen, white, a hand holding out the
        // desktop's disk, which a key puts in (its empty drive clicking
        // meanwhile), in the middle of its 32 rows; then its blue again, the
        // shell's copyright with the ball beside it, and its prompt.
        start: &[
            "{paper-white}{clear}",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "{disk}",
            "",
            "^{black}Flow notanAmiga Kickstart {version}",
            "^{black}Press any key to insert the Workbench disk",
            "{wait}",
            "{paper-plain}{clear}{logo}{beside}Copyright © 1985, 1986, 1987, 1988 Flow notanAmiga.",
            "{beside}All rights reserved.",
            "{beside}Release {version}",
            "{beside}",
            "",
        ],
        baud: 9600,
        // Its shell's prompt: the shell's number, then the folder.
        prompt: Some("1.{cwd}> "),
        sounds: Sounds::Floppy,
        // A PAL screen's 256 lines, eight to a row.
        screen: Some((80, 32)),
        frame: true,
        keys: None,
    },
];

/// The user's own look's id: whatever colors, fonts and CRT are set, with
/// the start screen, prompt and sounds of `[theme]` in the config.
pub const OWN: &str = "own";

/// What a theme saved under a name has for an id before its name:
/// `saved:Night shift`. No built-in look's id has a colon.
pub const SAVED: &str = "saved:";

/// What a theme sets, as a preset file names it: each of its sections and
/// the settings in it, past its start screen, prompt and sounds, which a
/// saved theme keeps whole (`SavedTheme::theme`). The font's size, the
/// liquid's switches and the window's backdrop stay as they are, as they do
/// for a built-in look, so a tab wearing one keeps the window's.
pub const THEME_KEYS: [(&str, &[&str]); 5] = [
    ("colors", &["text", "cursor", "background", "border", "ansi", "phosphor"]),
    ("cursor", &["shape", "blink", "speed"]),
    ("font", &["families"]),
    ("fx", &["style", "water_view", "sun", "electricity", "speed", "ambient", "rain", "text_ripple", "caustics", "palette", "opacity", "glow", "crt"]),
    ("crt", &["scanlines", "interlace", "glow", "decay", "grille", "vignette", "curvature"]),
];

/// What a tab owns of the config, its whole look: everything in Settings'
/// Theme, Theme details, Liquid, Glass, Text and CRT sections (the font's
/// size and the Startup logo among them, a CRT's saved presets and the
/// machines' muted sounds not), as a preset file names them. Settings on a
/// tab changes these for it alone (`App`'s `Tab::own`), and the rest of
/// the config is Flow's, every tab's; [`THEME_KEYS`] are among them.
pub const TAB_KEYS: [(&str, &[&str]); 7] = [
    ("ui", &["look", "logo", "look_start", "look_prompt", "look_font", "look_crt", "look_colors", "look_frame", "look_real_numbers"]),
    ("theme", &["start", "baud", "prompt", "sounds", "frame"]),
    ("colors", &["text", "cursor", "background", "border", "ansi", "phosphor"]),
    ("cursor", &["shape", "blink", "speed"]),
    ("font", &["families", "size", "line_height"]),
    ("fx", &["style", "water_view", "sun", "electricity", "speed", "ambient", "rain", "text_ripple", "caustics", "palette", "opacity", "glow", "crt", "liquid", "reactions", "program_effects", "backdrop", "cursor_glide"]),
    ("crt", &["scanlines", "interlace", "glow", "decay", "grille", "vignette", "curvature"]),
];

/// `config`'s whole look ([`TAB_KEYS`]), as JSON: a section's object of
/// those of its keys, for a tab to keep as its own. A key the config
/// leaves out where it's unset (programs' sixteen colors, Flow's own) is
/// null, so putting it on unsets it too.
pub fn tab_part(config: &Config) -> serde_json::Value {
    let all = serde_json::to_value(config).unwrap_or_default();
    let part: serde_json::Map<_, _> = TAB_KEYS
        .iter()
        .filter_map(|(section, keys)| {
            let from = all.get(*section)?;
            let keys: serde_json::Map<_, _> = keys.iter().map(|k| (k.to_string(), from.get(*k).cloned().unwrap_or_default())).collect();
            Some((section.to_string(), serde_json::Value::Object(keys)))
        })
        .collect();
    serde_json::Value::Object(part)
}

/// Puts a tab's whole look (`part`, as [`tab_part`] wrote it) on `config`,
/// keeping the rest; nothing, where a value in it doesn't fit (a
/// `tabs.json` edited by hand, or from a Flow whose settings differ).
pub fn put_tab_part(config: &mut Config, part: &serde_json::Value) {
    let Ok(mut all) = serde_json::to_value(&*config) else { return };
    for (section, keys) in TAB_KEYS {
        let (Some(from), Some(serde_json::Value::Object(into))) = (part.get(section), all.get_mut(section)) else { continue };
        for key in keys {
            if let Some(value) = from.get(*key) {
                into.insert(key.to_string(), value.clone());
            }
        }
    }
    match serde_json::from_value(all) {
        Ok(worn) => *config = worn,
        Err(err) => log::warn!("a tab's own look doesn't fit: {err}"),
    }
}

/// The look `id` names, or Flow's: a packaged theme's where one has that
/// id, in a built-in one's place too.
pub fn find(id: &str) -> &'static Look {
    packaged::get(id).map(|p| p.look).or_else(|| LOOKS.iter().find(|l| l.id == id)).unwrap_or(&LOOKS[0])
}

/// The id of the theme saved under `name`.
pub fn saved_id(name: &str) -> String {
    format!("{SAVED}{name}")
}

/// The name of the saved theme `id` names, if it names one.
pub fn saved_name(id: &str) -> Option<&str> {
    id.strip_prefix(SAVED)
}

/// Every look a tab can wear, in Settings' order: the built-in ones (a
/// packaged one in the place of one with its id), the packaged ones of
/// ids of their own, the user's own, then the themes `config` keeps under
/// a name.
pub fn ids(config: &Config) -> Vec<String> {
    let builtin = LOOKS.iter().map(|l| l.id.to_string());
    let packaged = packaged::all().iter().filter(|p| !p.replaces()).map(|p| p.look.id.to_string());
    builtin.chain(packaged).chain([OWN.to_string()]).chain(config.themes.saved.iter().map(|t| saved_id(&t.name))).collect()
}

/// A look's name, as Settings and the new-tab picker show it: a saved
/// theme's own, a packaged one's (in a built-in one's place, the built-in
/// one's), else the words for it.
pub fn name(id: &str) -> String {
    match (saved_name(id), packaged::get(id).filter(|p| !p.replaces())) {
        (Some(name), _) => name.to_string(),
        (None, Some(p)) => p.name(),
        (None, None) => crate::i18n::t(&format!("settings.theme.{id}")).to_string(),
    }
}

/// `base` wearing look `id`: a built-in one put on; the user's own, which
/// keeps the colors, fonts and CRT as they are and takes `[theme]`'s start
/// screen, prompt and sounds, begun from the old machine worn last if it
/// has none yet; or a theme `base` keeps under a name, which puts on what
/// it holds and brings its own start screen, prompt and sounds
/// ([`InUse::of`]), leaving `[theme]`, your own, as it is (a name it no
/// longer keeps changes nothing but the id, and is Flow's look).
pub fn wear(base: &Config, id: &str) -> Config {
    let mut config = base.clone();
    if id == OWN {
        let last = find(&config.ui.look);
        if config.theme == crate::config::ThemeConfig::default() && last.machine() {
            config.theme = last.as_theme();
            // Its frame as Theme details left it.
            config.theme.frame &= config.ui.look_frame;
        }
        config.ui.look = OWN.into();
    } else if let Some(name) = saved_name(id) {
        if let Some(saved) = base.themes.named(name) {
            put_on(saved, &mut config);
        }
        config.ui.look = id.into();
    } else {
        find(id).apply(&mut config);
    }
    config
}

/// Puts back into `worn` the whole look as `base` has it (every one of
/// [`TAB_KEYS`], which `wear` and a tab's own look change), keeping what
/// else changed: what's saved while a tab wears a look of its own is the
/// user's config, not that look.
pub fn keep_look(base: &Config, worn: &mut Config) {
    put_tab_part(worn, &tab_part(base));
}

/// What `config` wears now, to keep as a theme under `name`: every one of
/// [`THEME_KEYS`] as a preset file writes it, programs' sixteen colors as
/// they show (Flow's own where a config from before them has none), and
/// the start screen, prompt and sounds of the look in use, where Theme
/// details leave them on.
pub fn save_as(config: &Config, name: &str) -> SavedTheme {
    let theme = match InUse::of(config) {
        InUse::Builtin(look) => {
            let mut theme = look.as_theme();
            if !look.machine() || !config.ui.look_start {
                theme.start.clear();
            }
            if !look.machine() || !config.ui.look_prompt {
                theme.prompt.clear();
            }
            theme.frame &= config.ui.look_frame;
            theme
        }
        InUse::Own(_, theme) => theme.clone(),
    };
    let mut config = config.clone();
    config.colors.ansi.get_or_insert_with(Vec::new);
    config.colors.phosphor.get_or_insert_with(String::new);
    let all = serde_json::to_value(&config).unwrap_or_default();
    let look = THEME_KEYS
        .iter()
        .filter_map(|(section, keys)| {
            let from = all.get(*section)?;
            let part: serde_json::Map<_, _> = keys.iter().filter_map(|k| Some((k.to_string(), from.get(*k)?.clone()))).collect();
            Some((section.to_string(), crate::config::tidy(serde_json::Value::Object(part))))
        })
        .collect();
    SavedTheme { name: name.to_string(), theme, look }
}

/// Puts on `config` what `saved` holds of [`THEME_KEYS`], keeping the rest,
/// `[theme]` among it; nothing, if a value in it doesn't fit (a config
/// edited by hand).
fn put_on(saved: &SavedTheme, config: &mut Config) {
    if let Err(err) = put_keys(&saved.look, config) {
        log::warn!("the theme “{}” doesn't fit: {err}", saved.name);
    }
}

/// Puts on `config` what `look` holds of [`THEME_KEYS`] (sections, each of
/// its keys), keeping the rest; nothing, if a value in it doesn't fit.
fn put_keys(look: &serde_json::Map<String, serde_json::Value>, config: &mut Config) -> Result<(), String> {
    let mut all = serde_json::to_value(&*config).map_err(|e| e.to_string())?;
    for (section, keys) in THEME_KEYS {
        let (Some(from), Some(serde_json::Value::Object(into))) = (look.get(section), all.get_mut(section)) else { continue };
        for key in keys {
            if let Some(value) = from.get(*key) {
                into.insert(key.to_string(), value.clone());
            }
        }
    }
    *config = serde_json::from_value(all).map_err(|e| e.to_string())?;
    Ok(())
}

/// Mutes the sounds of look `to` that `muted` mutes for look `from`, in
/// place of what it muted for `to` before: a theme saved from a machine's
/// keeps its sounds as they were.
pub fn carry_mutes(muted: &mut Vec<String>, from: &str, to: &str) {
    if from == to {
        return;
    }
    muted.retain(|m| muted_part(m, to).is_none());
    let carried: Vec<String> = muted.iter().filter_map(|m| muted_part(m, from)).map(|part| format!("{to}.{part}")).collect();
    muted.extend(carried);
}

/// The part of look `look`'s sounds that `key` (one of `sound.muted`'s,
/// [`Part::key`]) mutes, if it's that look's. A saved theme's name may
/// hold dots, and a part's id never does, so the look is all before the
/// last: "Amber v1.1"'s parts are never "Amber v1"'s.
pub fn muted_part<'a>(key: &'a str, look: &str) -> Option<&'a str> {
    key.rsplit_once('.').filter(|(of, _)| *of == look).map(|(_, part)| part)
}

/// A config from before programs' colors were a theme's: a built-in
/// machine worn with its colors on, and no `colors.ansi` at all (an empty
/// one is Flow's own, on purpose), takes its machine's (in memory, saved
/// with the next change). So with `colors.phosphor`, from before 0.1.34.
pub fn fill_in_colors(config: &mut Config) {
    let look = find(&config.ui.look);
    let wearing = look.id == config.ui.look && config.ui.look_colors;
    if wearing && config.colors.ansi.is_none() {
        config.colors.ansi = Some(look.ansi.map(|ansi| ansi.iter().map(|c| palette::hex(*c)).collect()).unwrap_or_default());
    }
    // A phosphor came in 0.1.34: the green and amber screens worn from
    // before take theirs, and every other config none, so the key is there
    // for `flow-settings` to set.
    if config.colors.phosphor.is_none() {
        config.colors.phosphor = Some(look.phosphor.filter(|_| wearing).map(palette::hex).unwrap_or_default());
    }
}

/// Whether the built-in look `id` has a frame ([`Look::frame`]), which a
/// package's theme taking its place keeps.
pub(crate) fn builtin_frame(id: &str) -> bool {
    LOOKS.iter().any(|l| l.id == id && l.frame)
}

/// Whether `config` wears a look with a frame, each pane wearing it drawing
/// one round its terminal: a machine's (or a package's) with Theme
/// details' Its window frame on, or your own or a saved theme's with its
/// Window frame on (`ThemeConfig::frame`).
pub fn framed(config: &Config) -> bool {
    match InUse::of(config) {
        InUse::Builtin(look) => config.ui.look_frame && look.frame,
        InUse::Own(_, theme) => theme.frame,
    }
}

/// The glass a machine's theme sets: opaque, as its screen was.
pub const MACHINE_OPACITY: f32 = 1.0;

/// The glass a machine's theme set before 0.1.36: nearly opaque.
const OPACITY_BEFORE: f32 = 0.96;

/// Flow's own glass before 0.1.36: more than half see-through.
pub const FLOW_GLASS_BEFORE: f32 = 0.55;

/// Whether this is the first start of 0.1.36 or later over a Flow before
/// it: the version that ran last (`updates.json`'s `ran`) older, or none
/// written where a config was there already (a Flow before 0.1.35).
pub fn first_start_since_glass(ran: &str, older_flow: bool) -> bool {
    if ran.trim().is_empty() { older_flow } else { crate::apps::newer("0.1.36", ran) }
}

/// A config from a Flow before 0.1.36, worn with Flow's own look and the
/// glass it had then (55%), takes Flow's glass now, opaque, at 0.1.36's
/// first start alone ([`first_start_since_glass`]): 55% is a step Glass
/// opacity can be set to, which a check at every start would take back.
/// True where it changed.
pub fn bring_glass_up(config: &mut Config) -> bool {
    let up = config.ui.look == "flow" && config.fx.opacity == FLOW_GLASS_BEFORE;
    if up {
        config.fx.opacity = FxConfig::default().opacity;
    }
    up
}

/// [`bring_glass_up`] for a tab's or a pane's own look in `tabs.json`.
pub fn bring_own_glass_up(own: &mut serde_json::Value) -> bool {
    let flows = own.pointer("/ui/look").and_then(|v| v.as_str()) == Some("flow");
    let up = own.pointer_mut("/fx/opacity").filter(|o| flows && o.as_f64().is_some_and(|o| o as f32 == FLOW_GLASS_BEFORE));
    if let Some(opacity) = up {
        *opacity = serde_json::json!(FxConfig::default().opacity);
        return true;
    }
    false
}

/// A config from before 0.1.36, a machine worn with the glass its theme
/// set then (0.96 exactly), takes its glass now, opaque (in memory, saved
/// with the next change). An opacity the user set stays, and so does your
/// own theme's or a saved one's.
pub fn fill_in_opacity(config: &mut Config) {
    let look = find(&config.ui.look);
    if look.id == config.ui.look && look.machine() && config.fx.opacity == OPACITY_BEFORE {
        config.fx.opacity = MACHINE_OPACITY;
    }
}

/// [`fill_in_opacity`] for a tab's or a pane's own look in `tabs.json`
/// ([`tab_part`]'s JSON, which names the look it wears).
pub fn fill_in_own_opacity(own: &mut serde_json::Value) {
    let machine = own.pointer("/ui/look").and_then(|v| v.as_str()).is_some_and(|id| {
        let look = find(id);
        look.id == id && look.machine()
    });
    if let Some(opacity) = own.pointer_mut("/fx/opacity").filter(|o| machine && o.as_f64().is_some_and(|o| o as f32 == OPACITY_BEFORE)) {
        *opacity = serde_json::json!(MACHINE_OPACITY);
    }
}

/// The fonts a machine's theme set before Flow drew its own for it.
const FONTS_BEFORE: [(&str, &[&str]); 1] = [("instruments", &["unscii-8"])];

/// A config from before a machine had a font of Flow's own: the machine
/// worn with its font on, and the fonts it set then, as they were, takes
/// its own now (in memory, saved with the next change). Fonts the user
/// picked stay.
pub fn fill_in_fonts(config: &mut Config) {
    let look = find(&config.ui.look);
    let Some((_, before)) = FONTS_BEFORE.iter().find(|(id, _)| *id == look.id) else { return };
    let set_then: Vec<String> = before.iter().map(|f| f.to_string()).chain(FontConfig::default().families).collect();
    if look.id == config.ui.look && config.ui.look_font && config.font.families == set_then {
        look.apply_font(config);
    }
}

impl Sounds {
    /// As `[theme]` names it: "tube", "terminal", "disk", "fan", "floppy",
    /// "chime", "printer", "teletype", "tones", else quiet.
    pub fn named(name: &str) -> Sounds {
        match name {
            "tube" => Sounds::Tube,
            "teletype" => Sounds::Teletype,
            "disk" => Sounds::Disk,
            "fan" => Sounds::Fan,
            "printer" => Sounds::Printer,
            "terminal" => Sounds::Terminal,
            "chime" => Sounds::Chime,
            "tones" => Sounds::Tones,
            "floppy" => Sounds::Floppy,
            _ => Sounds::Quiet,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Sounds::Quiet => "quiet",
            Sounds::Tube => "tube",
            Sounds::Teletype => "teletype",
            Sounds::Disk => "disk",
            Sounds::Fan => "fan",
            Sounds::Printer => "printer",
            Sounds::Terminal => "terminal",
            Sounds::Chime => "chime",
            Sounds::Tones => "tones",
            Sounds::Floppy => "floppy",
        }
    }

    /// Whether a tube hums and its keys click.
    pub fn tube(self) -> bool {
        matches!(self, Sounds::Tube | Sounds::Disk | Sounds::Fan | Sounds::Printer | Sounds::Terminal | Sounds::Chime | Sounds::Tones | Sounds::Floppy)
    }

    /// The machine's own bell, and its size, where it had one: a teletype's
    /// bell, a terminal's or a PC speaker's beep, a sound chip's honk.
    pub fn bell(self) -> Option<(crate::sound::Cue, f32)> {
        match self {
            Sounds::Teletype => Some((crate::sound::Cue::Margin, 0.0)),
            Sounds::Terminal => Some((crate::sound::Cue::Beep, 0.0)),
            Sounds::Fan => Some((crate::sound::Cue::Beep, 1.0)),
            Sounds::Tones => Some((HONK, 0.0)),
            _ => None,
        }
    }

    /// What it plays as the machine starts, where that's a tone of its
    /// sound chip's: the high beep.
    pub fn start_tone(self) -> Option<crate::sound::Cue> {
        (self == Sounds::Tones).then_some(ACCEPT)
    }

    /// Whether a disk drive seeks as the machine starts.
    pub fn disk(self) -> bool {
        matches!(self, Sounds::Disk | Sounds::Fan | Sounds::Floppy)
    }

    /// Whether its drive clicks while it's empty: while a start screen
    /// waits for a key, which puts the disk in.
    pub fn empty_drive(self) -> bool {
        self == Sounds::Floppy
    }

    /// The sounds it makes, each of which its theme's details turn off on
    /// its own.
    pub fn parts(self) -> &'static [Part] {
        match self {
            Sounds::Quiet => &[],
            Sounds::Tube => &[Part::Hum, Part::Keys],
            Sounds::Teletype => &[Part::Clatter, Part::Keys, Part::Margin, Part::Punch, Part::Bell],
            Sounds::Disk => &[Part::Hum, Part::Keys, Part::Disk],
            Sounds::Fan => &[Part::Hum, Part::Fan, Part::Keys, Part::Disk, Part::Bell],
            Sounds::Printer => &[Part::Hum, Part::Keys, Part::Printer],
            Sounds::Terminal => &[Part::Hum, Part::Keys, Part::Bell],
            Sounds::Chime => &[Part::Hum, Part::Keys, Part::Chime],
            Sounds::Tones => &[Part::Hum, Part::Keys, Part::Tone, Part::Bell],
            Sounds::Floppy => &[Part::Hum, Part::Keys, Part::Disk, Part::Drive],
        }
    }
}

/// The sound chip's high beep, as the key that began it was taken.
const ACCEPT: crate::sound::Cue = crate::sound::Cue::Note { hz: 1397.0, secs: 0.15 };
/// Its low honk, at a mistake.
const HONK: crate::sound::Cue = crate::sound::Cue::Note { hz: 218.0, secs: 0.3 };

/// One of the sounds an old machine makes (`Sounds::parts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A tube's hum.
    Hum,
    /// The keys struck: clicks, a terminal's ticks, a teletype's clack.
    Keys,
    /// A PC's fan, laid with the hum.
    Fan,
    /// A disk drive seeking as the machine starts.
    Disk,
    /// An empty floppy drive's clicks, while the machine waits for its disk.
    Drive,
    /// A graphics workstation's chord as it starts.
    Chime,
    /// A sound chip's beep as the machine starts.
    Tone,
    /// A teletype printing its start screen, and its carriage running back.
    Clatter,
    /// A teletype's bell, near the line's end.
    Margin,
    /// A teletype's paper tape punch, as a command's output comes.
    Punch,
    /// A printer printing each line of a command's output.
    Printer,
    /// Its own bell for a program's, in place of Flow's.
    Bell,
}

impl Part {
    /// Its name in `sound.muted` and in the words.
    pub fn id(self) -> &'static str {
        match self {
            Part::Hum => "hum",
            Part::Keys => "keys",
            Part::Fan => "fan",
            Part::Disk => "disk",
            Part::Drive => "drive",
            Part::Chime => "chime",
            Part::Tone => "tone",
            Part::Clatter => "clatter",
            Part::Margin => "margin",
            Part::Punch => "punch",
            Part::Printer => "printer",
            Part::Bell => "bell",
        }
    }

    /// Its name in `sound.muted` for the look `look`: `pc.fan`.
    pub fn key(self, look: &str) -> String {
        format!("{look}.{}", self.id())
    }
}

/// The look new tabs and the machines' sounds go by: a built-in one, or
/// one of the user's own (`ui.look = "own"`, which brings `[theme]`'s
/// start screen, prompt and sounds to whatever colors, fonts and CRT are
/// set, or a theme saved under a name, which brings its own,
/// `SavedTheme::theme`). The user's own carry their id.
#[derive(Clone, Copy)]
pub enum InUse<'a> {
    Builtin(&'static Look),
    Own(&'a str, &'a ThemeConfig),
}

impl<'a> InUse<'a> {
    /// What `config` wears: a saved theme it no longer keeps is Flow's
    /// look, as any id it doesn't know is.
    pub fn of(config: &'a Config) -> Self {
        let id = config.ui.look.as_str();
        match saved_name(id).map(|name| config.themes.named(name)) {
            Some(Some(saved)) => InUse::Own(id, &saved.theme),
            Some(None) => InUse::Builtin(find(id)),
            None if id == OWN => InUse::Own(id, &config.theme),
            None => InUse::Builtin(find(id)),
        }
    }

    /// The look's id: a built-in look's, `own`, or a saved theme's.
    pub fn id(&self) -> &str {
        match self {
            InUse::Builtin(look) => look.id,
            InUse::Own(id, _) => id,
        }
    }

    /// An old machine's (or the user's own), which brings a start screen
    /// and a prompt; not Flow's.
    pub fn machine(&self) -> bool {
        match self {
            InUse::Builtin(look) => look.machine(),
            InUse::Own(..) => true,
        }
    }

    pub fn prompt(&self) -> Option<&str> {
        match self {
            InUse::Builtin(look) => look.prompt,
            InUse::Own(_, theme) => Some(theme.prompt.as_str()).filter(|p| !p.is_empty()),
        }
    }

    pub fn baud(&self) -> u32 {
        match self {
            InUse::Builtin(look) => look.baud,
            InUse::Own(_, theme) => theme.baud,
        }
    }

    pub fn sounds(&self) -> Sounds {
        match self {
            InUse::Builtin(look) => look.sounds,
            InUse::Own(_, theme) => Sounds::named(&theme.sounds),
        }
    }

    /// Its start screen for a terminal `cols` wide (see [`Look::start_screen`]).
    pub fn start_screen(&self, facts: &Facts, cols: usize) -> Option<String> {
        match self {
            InUse::Builtin(look) => look.start_screen(facts, cols),
            InUse::Own(_, theme) => start_screen(theme.start.iter().map(String::as_str), facts, cols),
        }
    }

    /// Whether its start screen shows a serial or an Ethernet address, which
    /// Theme details' Its real numbers may make the computer's own.
    pub fn shows_numbers(&self) -> bool {
        match self {
            InUse::Builtin(look) => look.shows_numbers(),
            InUse::Own(_, theme) => theme.start.iter().any(|l| has_numbers(l)),
        }
    }

    /// Whether its start screen probes the computer's memory or buses
    /// (`{banks}`, `{probes}`), and so needs `Platform::hardware`.
    pub fn reads_hardware(&self) -> bool {
        let probes = |line: &str| matches!(line.trim(), "{banks}" | "{probes}");
        match self {
            InUse::Builtin(look) => look.start.iter().any(|l| probes(l)),
            InUse::Own(_, theme) => theme.start.iter().any(|l| probes(l)),
        }
    }
}

/// Whether a start screen's line shows a serial or an Ethernet address.
fn has_numbers(line: &str) -> bool {
    ["{serial}", "{ether}", "{hostid}", "{sgi_ether}"].iter().any(|n| line.contains(n))
}

/// What a start screen can say about the computer.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub version: String,
    pub os: String,
    pub chip: String,
    pub memory_gb: u64,
    pub shell: String,
    /// Where a start screen's made-up numbers come from (`{serial}`,
    /// `{ether}`, `{hostid}`, `{sgi_ether}`): the same on one computer, from
    /// its id hashed (`identity::seed`), never the id itself.
    pub seed: u64,
    /// The computer's own address and serial, where Theme details' Its real
    /// numbers asks for them, in place of made-up ones; else none.
    pub real: crate::identity::RealNumbers,
    /// The memory banks and buses' slots `{banks}` and `{probes}` probe;
    /// empty where the system didn't say, and they probe made-up ones.
    pub hardware: Hardware,
}

/// The processor snapshots name: never the computer's own.
const MADE_UP_CHIP: &str = "Flow 64-bit processor";

impl Facts {
    /// A made-up computer, for snapshots, so the README's pictures never
    /// depend on the computer that made them: 16 GB, Flow's own processor,
    /// the same numbers each time, and made-up banks and buses.
    pub fn made_up(version: String, os: String, shell: String) -> Facts {
        Facts { version, os, chip: MADE_UP_CHIP.into(), memory_gb: 16, shell, seed: 7, real: Default::default(), hardware: Hardware::default() }
    }

    /// The memory installed: the banks' where they agree with the system's
    /// total, which is rounded, else that total.
    fn megabytes(&self) -> u64 {
        if self.hardware.banks_agree(self.memory_gb) {
            self.hardware.megabytes()
        } else {
            self.memory_gb.saturating_mul(1024)
        }
    }
}

/// Where a start screen's `{logo}` puts the startup logo (or its theme's),
/// four rows tall at the line's start, and how its `{beside}` lines go past
/// it; private-use characters, which no start screen has of its own.
pub const LOGO_HERE: char = '\u{E000}';
pub const BESIDE_LOGO: char = '\u{E001}';
/// Where a centered line's `{logo}` puts it, four rows tall in the middle
/// of the line, with nothing beside it.
pub const LOGO_CENTERED: char = '\u{E002}';
/// Where a start screen's `{wait}` waits for the tab's first key ("press
/// any key"), which it takes, before typing the rest.
pub const KEY_WAIT: char = '\u{E003}';
/// Where a start screen's `{disk}` puts the Flow notanAmiga's first
/// screen's picture, the hand holding out its disk
/// (`crate::logos::kickstart`), in the middle of the screen, the cursor
/// left on its last row.
pub const PICTURE_HERE: char = '\u{E004}';

/// Whether a start screen shows the logo itself.
pub fn shows_logo(text: &str) -> bool {
    text.contains([LOGO_HERE, LOGO_CENTERED])
}

/// Whether a start screen shows the hand holding out the disk.
pub fn shows_picture(text: &str) -> bool {
    text.contains(PICTURE_HERE)
}

/// The PROM's lines probing made-up memory, where the system didn't say
/// what its banks are: `memory_gb` in two banks, or four from 4 GB, of
/// eight, the rest "Nothing there".
fn memory_banks(memory_gb: u64) -> Vec<String> {
    let mb = if memory_gb == 0 { 64 } else { memory_gb * 1024 };
    let filled = if mb >= 4096 { 4 } else { 2 };
    (0..8).map(|i| if i < filled { format!("Probing Memory Bank #{i} {} Megabytes", mb / filled) } else { format!("Probing Memory Bank #{i} Nothing there") }).collect()
}

/// A SPARCstation 5's buses, which the PROM probes where the system didn't
/// say what its own are. They're Sun's lines, so they keep their width.
const MADE_UP_PROBES: [&str; 6] = [
    "Probing /iommu@0,10000000/sbus@0,10001000 at 5,0  espdma esp sd st SUNW,bpp ledma le",
    "Probing /iommu@0,10000000/sbus@0,10001000 at 4,0  SUNW,CS4231 power-management",
    "Probing /iommu@0,10000000/sbus@0,10001000 at 1,0  Nothing there",
    "Probing /iommu@0,10000000/sbus@0,10001000 at 2,0  Nothing there",
    "Probing /iommu@0,10000000/sbus@0,10001000 at 3,0  SUNW,tcx",
    "Probing /iommu@0,10000000/sbus@0,10001000 at 0,0  Nothing there",
];

/// The most lines either of the PROM's lists takes; a longer one shows one
/// fewer and says what it left out.
const MOST_LINES: usize = 8;
/// The widest a PROM's line probing the computer's own buses may be, so it
/// fits a terminal of 80 columns.
const WIDEST: usize = 79;
const NOTHING_THERE: &str = "Nothing there";

/// Which of a list's items the PROM shows: all of them where they fit and
/// none `must_cut`, else one line fewer than the most, the filled ones first
/// and then the empty ones, each in their order, shown in their own order.
fn keep(filled: &[bool], must_cut: bool) -> Vec<usize> {
    if filled.len() <= MOST_LINES && !must_cut {
        return (0..filled.len()).collect();
    }
    let full = (0..filled.len()).filter(|&i| filled[i]);
    let empty = (0..filled.len()).filter(|&i| !filled[i]);
    let mut kept: Vec<usize> = full.chain(empty).take(MOST_LINES - 1).collect();
    kept.sort_unstable();
    kept
}

/// "s" for any number but one.
fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// The PROM's lines probing the memory banks the firmware lists, by their
/// slots' numbers: size, kind and speed, or "Nothing there". Where there are
/// none, or they don't add up to the memory the system reports, made-up
/// ones.
fn bank_lines(facts: &Facts) -> Vec<String> {
    let banks = &facts.hardware.banks;
    if !facts.hardware.banks_agree(facts.memory_gb) {
        return memory_banks(facts.memory_gb);
    }
    let filled: Vec<bool> = banks.iter().map(|b| b.megabytes > 0).collect();
    let kept = keep(&filled, false);
    let mut lines: Vec<String> = kept.iter().map(|&i| bank_line(i, &banks[i])).collect();
    if kept.len() < banks.len() {
        let left: Vec<&Bank> = banks.iter().enumerate().filter(|(i, _)| !kept.contains(i)).map(|(_, b)| b).collect();
        let (n, mb) = (left.len(), left.iter().fold(0u64, |sum, b| sum.saturating_add(b.megabytes)));
        let what = if mb == 0 { NOTHING_THERE.to_string() } else { format!("{mb} Megabytes") };
        lines.push(format!("Probing {n} more Memory Bank{}  {what}", plural(n)));
    }
    lines
}

/// One bank, `i` its slot: "Probing Memory Bank #1 16384 Megabytes of
/// DDR4-3200", its kind (and so its speed) left out where unknown.
fn bank_line(i: usize, bank: &Bank) -> String {
    if bank.megabytes == 0 {
        return format!("Probing Memory Bank #{i} {NOTHING_THERE}");
    }
    let mut line = format!("Probing Memory Bank #{i} {} Megabytes", bank.megabytes);
    if shown_kind(&bank.kind) {
        line += &format!(" of {}", bank.kind);
        if bank.speed > 0 {
            line += &format!("-{}", bank.speed);
        }
    }
    line
}

/// The PROM's lines probing the buses' slots, at most eight of at most 79
/// columns; made-up ones where the system didn't say.
fn probe_lines(hardware: &Hardware) -> Vec<String> {
    let probes = &hardware.probes;
    if probes.is_empty() {
        return MADE_UP_PROBES.iter().map(|l| l.to_string()).collect();
    }
    let lines: Vec<Option<String>> = probes.iter().map(probe_line).collect();
    // Only a probe with a line can be shown; one without is left out.
    let showable: Vec<usize> = (0..probes.len()).filter(|&i| lines[i].is_some()).collect();
    let filled: Vec<bool> = showable.iter().map(|&i| !probes[i].names.is_empty()).collect();
    let kept: Vec<usize> = keep(&filled, showable.len() < probes.len()).into_iter().map(|k| showable[k]).collect();
    let mut out: Vec<String> = kept.iter().filter_map(|&i| lines[i].clone()).collect();
    if kept.len() < probes.len() {
        let left: Vec<&Probe> = probes.iter().enumerate().filter(|(i, _)| !kept.contains(i)).map(|(_, p)| p).collect();
        let n = left.len();
        let devices: usize = left.iter().map(|p| p.names.len()).sum();
        let what = if devices == 0 { NOTHING_THERE.to_string() } else { format!("{devices} device{}", plural(devices)) };
        out.push(format!("Probing {n} more slot{}  {what}", plural(n)));
    }
    out
}

/// One slot's line, "Probing /iommu@fed91000/pci@0,0 at 1c,0  r8169",
/// within 79 columns: a long path loses its leading parts to "/...", and
/// names that don't fit say how many more there were. None where the path
/// or the place on the bus can't be shown, or won't fit.
fn probe_line(probe: &Probe) -> Option<String> {
    if !shown_path(&probe.path) || !shown_at(&probe.at) {
        return None;
    }
    let head_of = |path: &str| format!("Probing {path} at {}  ", probe.at);
    let mut head = head_of(&probe.path);
    if head.len() + NOTHING_THERE.len() > WIDEST {
        let parts: Vec<&str> = probe.path.split('/').filter(|p| !p.is_empty()).collect();
        head = (1..parts.len()).map(|cut| head_of(&format!("/...{}", parts[cut..].iter().map(|p| format!("/{p}")).collect::<String>()))).find(|h| h.len() + NOTHING_THERE.len() <= WIDEST)?;
    }
    if probe.names.is_empty() {
        return Some(head + NOTHING_THERE);
    }
    let room = WIDEST - head.len();
    let shown: Vec<&str> = probe.names.iter().map(String::as_str).filter(|n| shown_name(n)).collect();
    let all = shown.join(" ");
    if shown.len() == probe.names.len() && all.len() <= room {
        return Some(head + &all);
    }
    // As many as fit, then how many more, those that can't be shown among
    // them; or, where none fits, how many there are, since "Nothing there"
    // would be false.
    let names = (1..=shown.len()).rev().map(|k| format!("{} +{} more", shown[..k].join(" "), probe.names.len() - k)).find(|text| text.len() <= room);
    let names = names.unwrap_or_else(|| format!("{} device{}", probe.names.len(), plural(probe.names.len())));
    (names.len() <= room).then(|| head + &names)
}

/// A start screen's numbers: its serial number, its Ethernet address as
/// Sun wrote one with the host ID that follows from it, and the same as
/// IRIX wrote it; the computer's own where `real` has them, else made up
/// from `seed`, in Sun's block (8:0:20) and SGI's (08:00:69).
fn numbers(seed: u64, real: &crate::identity::RealNumbers) -> (String, String, String, String) {
    let (made_serial, made_ether, made_hostid, made_sgi) = made_up(seed);
    let serial = real.serial.clone().unwrap_or(made_serial);
    match real.ether {
        Some([a, b, c, d, e, f]) => (serial, format!("{a:x}:{b:x}:{c:x}:{d:x}:{e:x}:{f:x}"), format!("80{d:02x}{e:02x}{f:02x}"), format!("{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{f:02x}")),
        None => (serial, made_ether, made_hostid, made_sgi),
    }
}

/// Made-up numbers from `seed`: a serial number, an Ethernet address in
/// Sun's own block (8:0:20) with the host ID that follows from it, and the
/// same address in SGI's (08:00:69), as IRIX wrote it.
fn made_up(seed: u64) -> (String, String, String, String) {
    let mut x = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let serial = 1_000_000 + next() % 9_000_000;
    let [a, b, c] = [next() as u8, next() as u8, next() as u8];
    (serial.to_string(), format!("8:0:20:{a:x}:{b:x}:{c:x}"), format!("80{a:02x}{b:02x}{c:02x}"), format!("08:00:69:{a:02x}:{b:02x}:{c:02x}"))
}

impl Look {
    /// An old machine's, not Flow's.
    pub fn machine(&self) -> bool {
        self.id != "flow"
    }

    /// Puts this look on `config`: its colors, cursor, fonts, glass and CRT.
    /// The font's size, the liquid's other settings and the user's CRT
    /// presets stay as they are.
    pub fn apply(&self, config: &mut Config) {
        self.apply_colors(config);
        config.cursor.shape = self.cursor_shape.into();
        config.cursor.blink = self.blink;
        config.cursor.speed = self.cursor_speed;
        self.apply_font(config);
        let fx = FxConfig::default();
        if self.machine() {
            // Plain glass, opaque, and no glow: the screens of the time
            // didn't shine through. Glass opacity dials it back.
            config.fx.style = "none".into();
            config.fx.opacity = MACHINE_OPACITY;
            config.fx.glow = 0.0;
        } else {
            config.fx.style = fx.style;
            config.fx.opacity = fx.opacity;
            config.fx.glow = fx.glow;
        }
        self.apply_crt(config);
        config.ui.look = self.id.into();
        if let Some(keys) = self.keys {
            if let Err(err) = put_keys(keys, config) {
                log::warn!("the theme “{}” doesn't fit: {err}", self.id);
            }
        }
    }

    /// Whether its start screen shows a serial or an Ethernet address, for
    /// Theme details' Its real numbers.
    pub fn shows_numbers(&self) -> bool {
        self.start.iter().any(|line| has_numbers(line))
    }

    /// Whether it's seen through a CRT preset of its own.
    pub fn has_crt(&self) -> bool {
        self.crt.is_some()
    }

    /// Whether `config` wears a machine's part: its details' switch for it
    /// (`ui.look_font` and the like) is on. Flow's look always wears its own.
    fn wears(&self, switch: bool) -> bool {
        switch || !self.machine()
    }

    /// Its colors, programs' sixteen among them, or with Theme details'
    /// Colors off, Flow's.
    pub fn apply_colors(&self, config: &mut Config) {
        let look = if self.wears(config.ui.look_colors) { self } else { &LOOKS[0] };
        let colors = &mut config.colors;
        colors.text = palette::hex(look.text);
        colors.cursor = palette::hex(look.cursor);
        colors.background = palette::hex(look.background);
        colors.border = look.border.map(palette::hex).unwrap_or_default();
        colors.ansi = Some(look.ansi.map(|ansi| ansi.iter().map(|c| palette::hex(*c)).collect()).unwrap_or_default());
        colors.phosphor = Some(look.phosphor.map(palette::hex).unwrap_or_default());
    }

    /// Its fonts ahead of Flow's, or with Theme details' Font off, Flow's.
    pub fn apply_font(&self, config: &mut Config) {
        let fonts = if self.wears(config.ui.look_font) { self.fonts } else { &[] };
        config.font.families = fonts.iter().map(|f| f.to_string()).chain(FontConfig::default().families).collect();
    }

    /// Its tube, a built-in CRT preset, or with Theme details' CRT off,
    /// none. The presets the user saved stay.
    pub fn apply_crt(&self, config: &mut Config) {
        let crt = self.crt.filter(|_| self.wears(config.ui.look_crt));
        let presets = std::mem::take(&mut config.crt.presets);
        match crt.and_then(|name| crate::settings::builtin_crt_presets().into_iter().find(|p| p.name == name)) {
            Some(p) => {
                config.fx.crt = p.strength;
                config.crt = CrtConfig {
                    scanlines: p.scanlines,
                    interlace: p.interlace,
                    glow: p.glow,
                    decay: p.decay,
                    grille: p.grille,
                    vignette: p.vignette,
                    curvature: p.curvature,
                    presets: Vec::new(),
                };
            }
            None => {
                config.fx.crt = 0.0;
                config.crt = CrtConfig::default();
            }
        }
        config.crt.presets = presets;
    }

    /// Its start screen for a terminal `cols` wide, with `facts` put in, in
    /// the terminal's line endings; none for Flow's look.
    pub fn start_screen(&self, facts: &Facts, cols: usize) -> Option<String> {
        start_screen(self.start.iter().copied(), facts, cols)
    }

    /// The user's own theme, begun from this look: its start screen, the
    /// speed it's typed at, its prompt and its sounds.
    pub fn as_theme(&self) -> ThemeConfig {
        let start = self.start.iter().map(|l| l.to_string()).collect();
        ThemeConfig { start, baud: self.baud, prompt: self.prompt.unwrap_or("").into(), sounds: self.sounds.name().into(), frame: self.frame }
    }
}

/// A start screen of `lines`, with `facts` put in, for a terminal `cols`
/// wide, in the terminal's line endings; none without lines.
///
/// A line starting with `^` is centered. One starting (after any `^`) with
/// `{tall}` is typed twice, as the top and bottom halves of a double-height
/// line, and one with `{wide}` is double width (`crate::line_size`); either
/// is centered in the half of the columns it shows. Teletext's colors color
/// what follows them to the line's end: `{red}`, `{green}`, `{yellow}`,
/// `{blue}`, `{magenta}`, `{cyan}`, `{white}` and `{black}` the text,
/// `{on-red}` and the like its background, and `{plain}` back to the look's
/// own; any other color is written as `{#d5524d}` (`{on-#d5524d}` behind).
/// `{paper-white}` (or any other) is the page's color: each line after it
/// starts on it, its centering too, and goes back to it at its end and at
/// `{plain}`, so a `{clear}` after it fills the screen with it, until
/// `{paper-plain}` (or the start screen's end). They count for nothing in
/// the centering, nor do escapes written in the line itself. A centered line holding `{logo}` alone has the logo in its
/// middle ([`LOGO_CENTERED`]). A line of `{wait}` alone waits there for the
/// tab's first key ([`KEY_WAIT`]); `{clear}` clears the screen, leaving the
/// cursor at its top, and `{foot}` puts the cursor on its last line, as a
/// home computer's BASIC began at the screen's foot. `{disk}` alone on a
/// line is the Flow notanAmiga's first screen's picture ([`PICTURE_HERE`]).
fn start_screen<'a>(lines: impl IntoIterator<Item = &'a str>, facts: &Facts, cols: usize) -> Option<String> {
    let lines: Vec<&str> = lines.into_iter().collect();
    if lines.is_empty() {
        return None;
    }
    // `{banks}` and `{probes}`, each a line alone, are the PROM's lines
    // probing the memory and the buses.
    let (banks, probes) = (bank_lines(facts), probe_lines(&facts.hardware));
    let lines: Vec<&str> = lines
        .iter()
        .flat_map(|line| match line.trim() {
            "{banks}" => banks.iter().map(String::as_str).collect(),
            "{probes}" => probes.iter().map(String::as_str).collect(),
            _ => vec![*line],
        })
        .collect();
    let (serial, ether, hostid, sgi_ether) = numbers(facts.seed, &facts.real);
    let bytes = facts.memory_gb.saturating_mul(1 << 30);
    // The page's color (`{paper-…}`), as an escape, which lines start on.
    let mut paper = String::new();
    let lines: Vec<String> = lines
        .iter()
        .map(|line| {
            let (center, line) = match line.strip_prefix('^') {
                Some(rest) => (true, rest),
                None => (false, *line),
            };
            if center && line.trim() == "{logo}" {
                return LOGO_CENTERED.to_string();
            }
            if line.trim() == "{wait}" {
                return KEY_WAIT.to_string();
            }
            if line.trim() == "{disk}" {
                return PICTURE_HERE.to_string();
            }
            let text = line
                .replace("{version}", &facts.version)
                .replace("{os}", &facts.os)
                .replace("{chip}", &facts.chip)
                .replace("{memory}", &format!("{} GB", facts.memory_gb))
                .replace("{mb}", &facts.megabytes().to_string())
                .replace("{bytes}", &bytes.to_string())
                .replace("{kb}", &(facts.memory_gb * 1024 * 1024).to_string())
                .replace("{shell}", &facts.shell)
                .replace("{serial}", &serial)
                .replace("{ether}", &ether)
                .replace("{hostid}", &hostid)
                .replace("{sgi_ether}", &sgi_ether)
                .replace("{logo}", &LOGO_HERE.to_string())
                .replace("{beside}", &BESIDE_LOGO.to_string())
                .replace("{clear}", "\x1b[2J\x1b[H")
                .replace("{foot}", "\x1b[999;1H");
            let (size, text) = match (text.strip_prefix("{tall}"), text.strip_prefix("{wide}")) {
                (Some(rest), _) => (LineSize::Top, rest),
                (_, Some(rest)) => (LineSize::Wide, rest),
                _ => (LineSize::Single, text.as_str()),
            };
            let (text, width) = colored(text, &mut paper);
            let room = size.shown(cols);
            let pad = if center && width < room { " ".repeat((room - width) / 2) } else { String::new() };
            match size {
                LineSize::Top => format!("\x1b#3{pad}{text}\r\n\x1b#4{pad}{text}"),
                LineSize::Wide => format!("\x1b#6{pad}{text}"),
                _ => format!("{pad}{text}"),
            }
        })
        .collect();
    // Whatever comes after starts on the look's own colors again.
    let end = if paper.is_empty() { "" } else { "\x1b[0m" };
    Some(lines.join("\r\n") + "\r\n" + end)
}

/// Teletext's eight colors, by the names a start screen gives them.
const TELETEXT: [(&str, palette::Rgb8); 8] = [
    ("black", [0x00, 0x00, 0x00]),
    ("red", [0xFF, 0x00, 0x00]),
    ("green", [0x00, 0xFF, 0x00]),
    ("yellow", [0xFF, 0xFF, 0x00]),
    ("blue", [0x00, 0x00, 0xFF]),
    ("magenta", [0xFF, 0x00, 0xFF]),
    ("cyan", [0x00, 0xFF, 0xFF]),
    ("white", [0xFF, 0xFF, 0xFF]),
];

/// The escape for a start screen's color `name` (see [`start_screen`]).
fn color_escape(name: &str) -> Option<String> {
    if name == "plain" {
        return Some("\x1b[0m".into());
    }
    let (layer, name) = match name.strip_prefix("on-") {
        Some(name) => (48, name),
        None => (38, name),
    };
    let [r, g, b] = match name.strip_prefix('#') {
        Some(hex) => hex_color(hex)?,
        None => TELETEXT.iter().find(|(n, _)| *n == name)?.1,
    };
    Some(format!("\x1b[{layer};2;{r};{g};{b}m"))
}

/// A color written as six hex digits, `d5524d`.
fn hex_color(hex: &str) -> Option<palette::Rgb8> {
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// A start screen's line with its colors (`{red}`) as escapes, back to the
/// look's own at its end (or to `paper`, the page's color as an escape,
/// which `{paper-…}` sets), and how many columns it shows.
fn colored(line: &str, paper: &mut String) -> (String, usize) {
    let (mut out, mut width, mut colors) = (String::new(), 0, false);
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        let color = rest.strip_prefix('{').and_then(|r| r.split_once('}')).and_then(|(name, after)| {
            let escape = match name.strip_prefix("paper-") {
                Some("plain") => {
                    paper.clear();
                    "\x1b[0m".to_string()
                }
                Some(color) => {
                    *paper = color_escape(&format!("on-{color}")).filter(|_| !color.starts_with("on-"))?;
                    format!("\x1b[0m{paper}")
                }
                None if name == "plain" => format!("\x1b[0m{paper}"),
                None => color_escape(name)?,
            };
            Some((escape, after))
        });
        if let Some((escape, after)) = color {
            out.push_str(&escape);
            colors = true;
            rest = after;
            continue;
        }
        // An escape of the line's own shows nothing.
        let len = if c == '\x1b' { escape_len(rest) } else { c.len_utf8() };
        if c != '\x1b' {
            width += c.width().unwrap_or(0);
        }
        out.push_str(&rest[..len]);
        rest = &rest[len..];
    }
    if colors {
        out.push_str("\x1b[0m");
        out.push_str(paper);
    }
    (out, width)
}

/// How many bytes of `text`, which starts with ESC, its escape takes: a CSI
/// to its final byte, `ESC #`, `ESC (` and `ESC )` with the character
/// after, another with one character.
fn escape_len(text: &str) -> usize {
    let mut chars = text.char_indices().skip(1);
    let end = |(i, c): (usize, char)| i + c.len_utf8();
    match chars.next() {
        Some((_, '[')) => chars.find(|(_, c)| ('\x40'..='\x7e').contains(c)).map_or(text.len(), end),
        Some((_, '#' | '(' | ')')) => chars.next().map_or(text.len(), end),
        Some(next) => end(next),
        None => text.len(),
    }
}

/// The most bytes of a prompt a shell is given. A theme's prompt is a line
/// or two; one far longer (a package's themes.json may be a megabyte, and
/// bash's form of a `$` is five bytes) would pass what the system lets an
/// environment variable hold, and no tab in that theme could open.
const MOST_PROMPT: usize = 4096;

/// The prompt `template` (see [`Look::prompt`]) as `kind` of shell writes
/// it, for the hooks to take from `FLOW_PROMPT`: bash's and ash's `PS1`,
/// zsh's (with `%` doubled, and the letters its options expand brought by
/// `%v`), the string PowerShell's prompt expands, or cmd's `PROMPT` codes.
/// A shell without the history number (cmd, and BusyBox's ash, the bash
/// `flavor` Flow brings on Windows) leaves it out.
///
/// A theme's prompt is text, never shell code (it comes from packages, a
/// user's own `.tar.gz` among them): every letter of it shows as itself,
/// whatever the user's shell options and locale, and nothing in it is
/// expanded. So each shell's special letters go as that shell shows them
/// literally; the controls that don't show are left out (a NUL, which no
/// variable can hold, and the ^A BusyBox's backslashes travel as), and so
/// is every escape but a color's, so a prompt drawn before each command
/// can't send Flow's own marks or questions (OSC 133, 633, 7732); a new
/// line and a tab stay. A prompt past [`MOST_PROMPT`] is cut, at a whole
/// letter.
pub fn prompt_for(kind: ShellKind, flavor: BashFlavor, template: &str) -> String {
    let busybox = flavor == BashFlavor::BusyBox;
    let mut out = String::new();
    // Whether what bash shows of the piece before may end in a byte that,
    // in a locale such as Shift JIS or GB18030, begins a letter whose
    // second byte is a backslash: a letter past ASCII, or the folder.
    let mut lead = false;
    let mut rest = template;
    while !rest.is_empty() {
        let (written, after) = if rest.starts_with('\x1b') {
            let len = whole_escape_len(rest);
            let escape = &rest[..len];
            rest = &rest[len..];
            if is_color(escape) {
                (escape.to_string(), false)
            } else {
                (String::new(), lead)
            }
        } else {
            let (written, piece) = written_for(kind, busybox, lead, &mut rest);
            let after = match piece {
                Piece::Cwd => true,
                Piece::Number => false,
                Piece::Char(_) if written.is_empty() => lead,
                Piece::Char(c) => !c.is_ascii(),
            };
            (written, after)
        };
        if out.len() + written.len() > MOST_PROMPT {
            log::warn!("a theme's prompt is cut at {MOST_PROMPT} bytes, as {kind:?} writes it");
            break;
        }
        out.push_str(&written);
        lead = after;
    }
    out
}

/// The next piece of `rest`, taken off it, and what `kind` of shell is
/// given for it (see [`prompt_for`]; `lead`, whether bash's text before
/// may end in a byte that begins a letter).
fn written_for(kind: ShellKind, busybox: bool, lead: bool, rest: &mut &str) -> (String, Piece) {
    let (piece, len) = if rest.starts_with("{cwd}") {
        (Piece::Cwd, 5)
    } else if rest.starts_with("{n}") {
        (Piece::Number, 3)
    } else {
        let c = rest.chars().next().unwrap_or(' ');
        (Piece::Char(c), c.len_utf8())
    };
    *rest = &rest[len..];
    let written = match (kind, piece) {
        (_, Piece::Char(c)) if c.is_control() && !matches!(c, '\n' | '\t') => String::new(),
        (ShellKind::Zsh, Piece::Cwd) => "%~".into(),
        (ShellKind::Zsh, Piece::Number) => "%h".into(),
        (ShellKind::Zsh, Piece::Char('%')) => "%%".into(),
        // What prompt_subst or prompt_bang would expand, whether either is
        // on now or comes on before the prompt is drawn: brought from the
        // end of psvar, where zsh's hook puts them, by %v, whose text zsh
        // never expands. So PS1 holds none of them.
        (ShellKind::Zsh, Piece::Char(c)) if ZSH_LITERALS.contains(&c) => {
            format!("%-{}v", ZSH_LITERALS.iter().rev().position(|&l| l == c).unwrap_or(0) + 1)
        }
        (ShellKind::PowerShell, Piece::Cwd) => "$($executionContext.SessionState.Path.CurrentLocation)".into(),
        (ShellKind::PowerShell, Piece::Number) => "$($MyInvocation.HistoryId)".into(),
        (ShellKind::PowerShell, Piece::Char('\n')) => "`n".into(),
        // What PowerShell expands in double quotes, and its curly double
        // quotes, each made plain by a backtick. Not the straight one:
        // ExpandString, which the hook hands the prompt to, keeps it as it
        // is, but drops all that follows a backtick before it (5.1 and 7.7
        // alike), while a curly one left bare ends the text there.
        (ShellKind::PowerShell, Piece::Char(c @ ('$' | '`' | '\u{201c}' | '\u{201d}' | '\u{201e}'))) => format!("`{c}"),
        (ShellKind::Cmd, Piece::Cwd) => "$P".into(),
        (ShellKind::Cmd, Piece::Number) => String::new(),
        (ShellKind::Bash, Piece::Number) if busybox => String::new(),
        (ShellKind::Cmd, Piece::Char(c)) => match c {
            '\n' => "$_".into(),
            '$' => "$$".into(),
            '>' => "$G".into(),
            '<' => "$L".into(),
            '|' => "$B".into(),
            '=' => "$Q".into(),
            '&' => "$A".into(),
            '(' => "$C".into(),
            ')' => "$F".into(),
            c => c.to_string(),
        },
        (_, Piece::Cwd) => "\\w".into(),
        (_, Piece::Number) => "\\!".into(),
        // ash expands its prompt as double-quoted text before its line
        // editor reads the escapes, so its backslash is doubled twice,
        // and $ and ` go as octal escapes, which only the line editor
        // reads: ash finds nothing to expand.
        (ShellKind::Bash, Piece::Char('\\')) if busybox => "\\\\\\\\".into(),
        (ShellKind::Bash, Piece::Char('$')) if busybox => "\\044".into(),
        (ShellKind::Bash, Piece::Char('`')) if busybox => "\\140".into(),
        // bash reads its escapes, then expands what's left (unless
        // promptvars is off; and in posix mode a ! is the history
        // number), but never what an escape gave: it puts a backslash
        // before each $ and ` of that. \D{…} gives strftime's text,
        // which without a % is the format as it is. That backslash
        // would be read as the second byte of a letter begun by the
        // byte before it, so after a piece that may end in one, \001
        // and \002 go first: an empty stretch of the prompt readline
        // doesn't count, else two controls the terminal shows nothing
        // of, and neither ever a letter's second byte.
        (ShellKind::Bash | ShellKind::Sh, Piece::Char(c @ ('\\' | '$' | '`' | '!'))) if !busybox => {
            format!("{}\\D{{{c}}}", if lead { "\\001\\002" } else { "" })
        }
        (_, Piece::Char(c)) => c.to_string(),
    };
    (written, piece)
}

/// How many bytes of `text`, which starts with ESC, its escape takes, as
/// [`escape_len`] counts them, and a string's (OSC, DCS, APC, PM and SOS:
/// `ESC ]`, `P`, `_`, `^`, `X`) to its end, BEL or `ESC \`, or the text's.
fn whole_escape_len(text: &str) -> usize {
    match text.as_bytes().get(1) {
        Some(b']' | b'P' | b'_' | b'^' | b'X') => {
            let body = &text[2..];
            // BEL or ST ends it; another escape begins there.
            let end = body.find(['\x07', '\x1b']).map_or(body.len(), |i| match &body[i..] {
                b if b.starts_with("\x1b\\") => i + 2,
                b if b.starts_with('\x07') => i + 1,
                _ => i,
            });
            2 + end
        }
        _ => escape_len(text),
    }
}

/// Whether `escape` sets colors and nothing else (SGR: `ESC [`, numbers
/// parted by `;` or `:`, `m`).
fn is_color(escape: &str) -> bool {
    escape.strip_prefix("\x1b[").and_then(|e| e.strip_suffix('m')).is_some_and(|e| e.bytes().all(|b| b.is_ascii_digit() || b == b';' || b == b':'))
}

#[derive(Clone, Copy)]
enum Piece {
    Cwd,
    Number,
    Char(char),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A theme saved from a config set apart from the default in every one
    /// of [`THEME_KEYS`], a start screen and a prompt among them.
    fn unlike_any(name: &str) -> SavedTheme {
        let colors = crate::config::ColorConfig { text: "#123456".into(), cursor: "#654321".into(), background: "#010203".into(), border: "#102030".into(), ansi: Some(vec!["#FFFFFF".into(); 16]), phosphor: Some("#204060".into()) };
        let mut c = Config { colors, ..Config::default() };
        (c.cursor.shape, c.cursor.blink, c.cursor.speed) = ("bar".into(), !c.cursor.blink, 3.0);
        c.font.families = vec!["Mine Mono".into()];
        let fx = &mut c.fx;
        (fx.style, fx.water_view, fx.palette) = ("water".into(), "below".into(), "ember".into());
        (fx.sun, fx.electricity, fx.speed, fx.ambient, fx.text_ripple, fx.caustics, fx.opacity, fx.glow, fx.crt, fx.rain) = (0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95);
        c.crt = CrtConfig { scanlines: 0.1, interlace: false, glow: 0.2, decay: 0.03, grille: 0.3, vignette: 0.4, curvature: 0.5, presets: Vec::new() };
        c.theme = ThemeConfig { start: vec!["MINE".into()], baud: 1200, prompt: "$(mine) ".into(), sounds: "tube".into(), frame: false };
        c.ui.look = OWN.into();
        save_as(&c, name)
    }

    #[test]
    fn the_home_computer_worn_from_before_takes_the_font_flow_drew_for_it() {
        let mut config = Config::default();
        find("instruments").apply(&mut config);
        assert_eq!(config.font.families[0], "Flow Instruments");
        // As a config from before had it, its fonts are brought up to date.
        let before = |config: &mut Config| config.font.families = std::iter::once("unscii-8".to_string()).chain(FontConfig::default().families).collect();
        before(&mut config);
        fill_in_fonts(&mut config);
        assert_eq!(config.font.families[..2], ["Flow Instruments".to_string(), "unscii-8".into()]);
        // Fonts the user picked, or a theme's fonts turned off, stay.
        config.font.families = vec!["unscii-8".into()];
        fill_in_fonts(&mut config);
        assert_eq!(config.font.families, ["unscii-8"]);
        before(&mut config);
        config.ui.look_font = false;
        fill_in_fonts(&mut config);
        assert_eq!(config.font.families[0], "unscii-8");
    }

    #[test]
    fn flows_glass_is_opaque_and_a_config_at_its_old_glass_takes_it_at_the_first_start_since() {
        assert_eq!(FxConfig::default().opacity, 1.0);
        // The first start of 0.1.36 or later over an older Flow, once.
        for (ran, older, first) in [("0.1.35", true, true), ("0.1.34.1", true, true), ("", true, true), ("", false, false), ("0.1.36", true, false), ("0.1.37", true, false)] {
            assert_eq!(first_start_since_glass(ran, older), first, "{ran:?} {older}");
        }
        // Flow's own look at the old 55% alone; a machine, your own theme
        // and a glass the user dialed stay.
        for (look, was, now) in [("flow", 0.55, 1.0), ("flow", 0.7, 0.7), ("green", 0.55, 0.55), ("own", 0.55, 0.55)] {
            let mut config = Config::default();
            (config.ui.look, config.fx.opacity) = (look.into(), was);
            assert_eq!(bring_glass_up(&mut config), was != now, "{look} at {was}");
            assert_eq!(config.fx.opacity, now, "{look} at {was}");
        }
        // A tab's own look in tabs.json, the same.
        let mut own = tab_part(&Config::default());
        own["fx"]["opacity"] = serde_json::json!(0.55_f32);
        assert!(bring_own_glass_up(&mut own));
        assert_eq!(own["fx"]["opacity"], serde_json::json!(1.0));
        own["ui"]["look"] = "amber".into();
        own["fx"]["opacity"] = serde_json::json!(0.55_f32);
        assert!(!bring_own_glass_up(&mut own), "a machine's own look keeps what it holds");
    }

    #[test]
    fn a_machines_glass_is_opaque_and_one_worn_from_before_takes_it() {
        let mut config = Config::default();
        find("amiga").apply(&mut config);
        assert_eq!(config.fx.opacity, 1.0);
        find("flow").apply(&mut config);
        assert_eq!(config.fx.opacity, FxConfig::default().opacity);
        // A machine worn from before, its glass as its theme set it then,
        // takes it opaque; one the user dialed, or Flow's own, stays.
        find("green").apply(&mut config);
        for (look, was, now) in [("green", 0.96, 1.0), ("green", 0.8, 0.8), ("flow", 0.96, 0.96), ("own", 0.96, 0.96)] {
            config.ui.look = look.into();
            config.fx.opacity = was;
            fill_in_opacity(&mut config);
            assert_eq!(config.fx.opacity, now, "{look} at {was}");
        }
        // So does a tab's own look in tabs.json, which names its own.
        let mut own = tab_part(&config);
        own["ui"]["look"] = "amber".into();
        own["fx"]["opacity"] = serde_json::json!(0.96_f32);
        fill_in_own_opacity(&mut own);
        assert_eq!(own["fx"]["opacity"], serde_json::json!(1.0));
        own["fx"]["opacity"] = serde_json::json!(0.7_f32);
        fill_in_own_opacity(&mut own);
        assert_eq!(own["fx"]["opacity"].as_f64().map(|o| o as f32), Some(0.7));
    }

    #[test]
    fn a_start_screen_waits_for_a_key_and_clears_to_its_foot_where_it_says() {
        let facts = Facts::default();
        let text = start_screen(["^PRESS ANY KEY", "{wait}", "{clear}{foot}", " READY"], &facts, 20).unwrap();
        assert_eq!(text, format!("   PRESS ANY KEY\r\n{KEY_WAIT}\r\n\x1b[2J\x1b[H\x1b[999;1H\r\n READY\r\n"));
        // The machine that said "press any key" waits for one.
        let instruments = LOOKS.iter().find(|l| l.id == "instruments").unwrap();
        assert!(instruments.start_screen(&facts, 32).unwrap().contains(KEY_WAIT));
        assert!(LOOKS.iter().filter(|l| l.start.contains(&"{wait}")).map(|l| l.id).eq(["instruments", "amiga"]));
    }

    #[test]
    fn a_tabs_whole_look_is_every_key_settings_look_sections_change_and_puts_back_on() {
        // Every key names a setting there is, and a saved theme's are among
        // them.
        let mut every = Config::default();
        every.colors.ansi = Some(Vec::new());
        let all = serde_json::to_value(every).unwrap();
        for (section, keys) in TAB_KEYS {
            for key in keys {
                assert!(all.get(section).and_then(|s| s.get(key)).is_some(), "{section}.{key}");
            }
        }
        for (section, keys) in THEME_KEYS {
            let tab = TAB_KEYS.iter().find(|(s, _)| *s == section).map(|(_, k)| *k).unwrap_or_default();
            assert!(keys.iter().all(|k| tab.contains(k)), "{section}");
        }
        // A tab's look taken off one config and put on another is worn
        // whole there, the rest of that one's kept.
        let mut mine = wear(&Config::default(), "teletype");
        (mine.font.size, mine.fx.cursor_glide, mine.ui.logo) = (21.0, false, "chip".into());
        let part = tab_part(&mine);
        let mut other = Config::default();
        other.ui.language = "ja".into();
        other.crt.presets.push(crate::config::CrtPreset { name: "kept".into(), ..Default::default() });
        put_tab_part(&mut other, &part);
        assert_eq!(tab_part(&other), part);
        assert_eq!((other.ui.look.as_str(), other.font.size, other.ui.language.as_str(), other.crt.presets.len()), ("teletype", 21.0, "ja", 1));
        // A look that doesn't fit changes nothing.
        let before = tab_part(&other);
        put_tab_part(&mut other, &serde_json::json!({ "font": { "size": "big" } }));
        assert_eq!(tab_part(&other), before);
    }

    #[test]
    fn a_change_worn_over_the_users_config_splits_into_the_tabs_look_and_flows_settings() {
        // Settings on a tab changes its liquid and its text's size, and the
        // language, which is Flow's.
        let user = Config::default();
        let mut worn = user.clone();
        (worn.fx.style, worn.font.size, worn.ui.language) = ("rain".into(), 22.0, "ja".into());
        // The tab keeps its look, the language not among it...
        let own = tab_part(&worn);
        assert_ne!(own, tab_part(&user));
        let mut tab = user.clone();
        put_tab_part(&mut tab, &own);
        assert_eq!((tab.fx.style.as_str(), tab.font.size, tab.ui.language.as_str()), ("rain", 22.0, "auto"));
        // ...and the user's config takes the language, keeping its own look.
        let mut kept = worn.clone();
        keep_look(&user, &mut kept);
        assert_eq!((kept.fx.style.as_str(), kept.font.size, kept.ui.language.as_str()), (user.fx.style.as_str(), user.font.size, "ja"));
    }

    #[test]
    fn a_look_worn_in_a_tab_is_taken_off_whole_before_saving() {
        // Some of the user's own, a machine worn, their own theme and one
        // they saved, unlike anything else, too.
        let mut base = Config::default();
        base.fx.style = "rain".into();
        base.font.size = 17.0;
        base.colors.text = "#c0ffee".into();
        base.themes.saved.push(unlike_any("Mine"));
        for start in ["flow", "teletype", OWN, "saved:Mine"] {
            let base = wear(&base, start);
            for id in ids(&base) {
                // Something else changed while the look was worn stays.
                let mut worn = wear(&base, &id);
                worn.ui.efficient = !base.ui.efficient;
                keep_look(&base, &mut worn);
                let mut want = base.clone();
                want.ui.efficient = !base.ui.efficient;
                assert_eq!(toml::to_string(&worn).unwrap(), toml::to_string(&want).unwrap(), "{id} over {start}");
            }
        }
    }

    #[test]
    fn a_theme_saved_holds_what_is_worn_and_nothing_else() {
        let theme = unlike_any("Mine");
        assert_eq!(theme.name, "Mine");
        // Every key a theme sets, by section, and none but those.
        // As sets: the order JSON keeps them in is the order they were put.
        let mut held: Vec<(&str, Vec<&str>)> = theme.look.iter().map(|(section, keys)| (section.as_str(), { let mut k: Vec<&str> = keys.as_object().unwrap().keys().map(String::as_str).collect(); k.sort(); k })).collect();
        held.sort();
        let mut want: Vec<(&str, Vec<&str>)> = THEME_KEYS.iter().map(|(section, keys)| (*section, { let mut k = keys.to_vec(); k.sort(); k })).collect();
        want.sort();
        assert_eq!(held, want);
        assert_eq!(theme.look["fx"]["opacity"], serde_json::json!(0.7), "fractions as they're set");
        assert_eq!(theme.theme, ThemeConfig { start: vec!["MINE".into()], baud: 1200, prompt: "$(mine) ".into(), sounds: "tube".into(), frame: false }, "its own start screen, prompt and sounds");
        // A machine's start screen, prompt and sounds where Theme details
        // leave them on; Flow's look has none, and programs' sixteen colors
        // are Flow's own where a config from before them has none.
        let mut green = wear(&Config::default(), "green");
        green.ui.look_prompt = false;
        let theme = save_as(&green, "Green");
        assert_eq!(theme.theme.start, find("green").start);
        assert_eq!((theme.theme.prompt.as_str(), theme.theme.sounds.as_str()), ("", "terminal"));
        assert_eq!(theme.look["colors"]["text"], serde_json::json!("#41FF6E"));
        let mut old = Config::default();
        old.colors.ansi = None;
        let theme = save_as(&old, "Flow's");
        assert_eq!((theme.look["colors"]["ansi"].clone(), theme.theme.start.len()), (serde_json::json!([]), 0));
    }

    #[test]
    fn a_saved_theme_is_worn_as_a_built_in_one_is() {
        let mut base = Config::default();
        base.font.size = 19.0;
        base.theme = find("teletype").as_theme();
        base.themes.saved.push(unlike_any("Mine"));
        assert_eq!(ids(&base)[LOOKS.len()..], [OWN, "saved:Mine"], "after the built-in ones and your own");
        assert_eq!((name("saved:Mine"), saved_name("saved:Mine"), saved_name("green")), ("Mine".to_string(), Some("Mine"), None));
        let worn = wear(&base, "saved:Mine");
        assert_eq!((worn.colors.text.as_str(), worn.fx.style.as_str(), worn.crt.curvature, worn.font.families.as_slice()), ("#123456", "water", 0.5, ["Mine Mono".to_string()].as_slice()));
        // The text's size and the liquid's switches stay the user's.
        assert_eq!((worn.font.size, worn.fx.liquid), (19.0, base.fx.liquid));
        // It brings its start screen, prompt and sounds, under its own id.
        let in_use = InUse::of(&worn);
        assert_eq!((in_use.id(), in_use.prompt(), in_use.sounds(), in_use.baud()), ("saved:Mine", Some("$(mine) "), Sounds::Tube, 1200));
        assert!(matches!(in_use, InUse::Own(..)) && in_use.start_screen(&facts(), 80).unwrap().starts_with("MINE"));
        assert_eq!((worn.worn_theme(), &worn.theme), (&base.themes.saved[0].theme, &base.theme), "your own stays as it was");
        // Your own chosen after it brings your own again.
        let own = wear(&worn, OWN);
        assert_eq!((InUse::of(&own).prompt(), own.colors.text.as_str()), (find("teletype").prompt, "#123456"));
        // Saved again, it's the same theme.
        assert_eq!(save_as(&worn, "Mine"), base.themes.saved[0]);
        // One no longer kept changes nothing but the id; one edited by hand
        // to hold a few changes those, and one that doesn't fit, nothing.
        let gone = wear(&base, "saved:Gone");
        assert_eq!((gone.ui.look.as_str(), gone.colors.text.as_str()), ("saved:Gone", base.colors.text.as_str()));
        assert_eq!((InUse::of(&gone).id(), gone.worn_theme()), ("flow", &base.theme), "and is Flow's look");
        let mut few = SavedTheme { name: "Few".into(), ..SavedTheme::default() };
        few.look.insert("fx".into(), serde_json::json!({ "style": "oil", "liquid": false }));
        few.look.insert("font".into(), serde_json::json!({ "size": 30.0 }));
        base.themes.saved.push(few);
        let worn = wear(&base, "saved:Few");
        assert_eq!((worn.fx.style.as_str(), worn.fx.liquid, worn.font.size, worn.colors.text.as_str()), ("oil", true, 19.0, base.colors.text.as_str()));
        base.themes.saved[1].look.insert("cursor".into(), serde_json::json!({ "blink": "often" }));
        assert_eq!(wear(&base, "saved:Few").fx.style, base.fx.style);
    }

    #[test]
    fn a_machines_muted_sounds_carry_to_the_theme_saved_from_it() {
        let mut muted = vec!["pc.fan".to_string(), "pc.disk".into(), "green.hum".into(), "saved:Mine.keys".into()];
        carry_mutes(&mut muted, "pc", "saved:Mine");
        assert_eq!(muted, ["pc.fan", "pc.disk", "green.hum", "saved:Mine.fan", "saved:Mine.disk"]);
        // Saved over itself, it keeps its own.
        carry_mutes(&mut muted, "saved:Mine", "saved:Mine");
        assert_eq!(muted.len(), 5);
        // A name that's another's and more after a dot is a theme apart.
        let mut muted = vec!["saved:Amber v1.1.hum".to_string(), "saved:Amber v1.keys".into()];
        carry_mutes(&mut muted, "saved:Amber v1", "saved:Amber");
        assert_eq!(muted, ["saved:Amber v1.1.hum", "saved:Amber v1.keys", "saved:Amber.keys"]);
        carry_mutes(&mut muted, "pc", "saved:Amber v1");
        assert_eq!(muted, ["saved:Amber v1.1.hum", "saved:Amber.keys"]);
        assert_eq!((muted_part("saved:Amber v1.1.hum", "saved:Amber v1.1"), muted_part("saved:Amber v1.1.hum", "saved:Amber v1")), (Some("hum"), None));
    }

    fn facts() -> Facts {
        Facts { version: "0.1.22".into(), os: "macOS 27.0".into(), chip: "Apple M3".into(), memory_gb: 16, shell: "zsh".into(), seed: 7, real: Default::default(), hardware: Hardware::default() }
    }

    #[test]
    fn a_machine_with_a_bell_of_its_own_rings_it_and_the_others_ring_flows() {
        use crate::sound::Cue;
        let bell = |id: &str| find(id).sounds.bell().map(|(cue, _)| cue);
        assert_eq!(bell("teletype"), Some(Cue::Margin));
        assert_eq!((bell("green"), bell("amber"), bell("pc")), (Some(Cue::Beep), Some(Cue::Beep), Some(Cue::Beep)));
        assert_eq!((bell("home"), bell("workstation"), bell("graphics"), bell("school"), bell("flow")), (None, None, None, None, None));
        // The 16-bit home computer flashed its screen for the bell.
        assert_eq!(bell("amiga"), None);
        assert_eq!(find("pc").sounds.bell().map(|(_, size)| size), Some(1.0), "a PC speaker's higher beep");
        // The cyan home computer's sound chip honks low, and beeps high as
        // it starts; no other machine beeps then.
        let tones = find("instruments").sounds;
        assert!(matches!((tones.bell(), tones.start_tone()), (Some((Cue::Note { hz: low, .. }, _)), Some(Cue::Note { hz: high, .. })) if low < 300.0 && high > 1000.0));
        assert!(LOOKS.iter().filter(|l| l.sounds.start_tone().is_some()).map(|l| l.id).eq(["instruments"]));
        assert_eq!((Sounds::named(tones.name()), tones.parts().contains(&Part::Tone), tones.tube()), (Sounds::Tones, true, true));
    }

    #[test]
    fn every_look_has_its_name_and_finds_itself() {
        for look in &LOOKS {
            assert!(std::ptr::eq(find(look.id), look));
            // A key no language has comes back as itself.
            let key = format!("settings.theme.{}", look.id);
            assert_ne!(crate::i18n::t(&key), key.as_str(), "{}", look.id);
        }
        assert_eq!(find("no such look").id, "flow");
        assert!(!find("flow").machine() && find("teletype").machine());
    }

    #[test]
    fn a_look_sets_the_glass_and_keeps_the_users_size_and_presets() {
        let mut cfg = Config::default();
        cfg.font.size = 19.0;
        cfg.crt.presets.push(crate::config::CrtPreset { name: "mine".into(), ..Default::default() });
        find("green").apply(&mut cfg);
        assert_eq!((cfg.colors.text.as_str(), cfg.fx.style.as_str(), cfg.ui.look.as_str()), ("#41FF6E", "none", "green"));
        assert_eq!(cfg.fx.crt, 0.9, "the phosphor preset");
        assert_eq!(cfg.font.families[0], "VT323");
        assert_eq!(cfg.font.families.last(), FontConfig::default().families.last(), "Flow's own fonts after");
        assert_eq!((cfg.font.size, cfg.crt.presets.len()), (19.0, 1));
        find("home").apply(&mut cfg);
        assert_eq!(cfg.colors.border, "#8C7FE2");
        find("flow").apply(&mut cfg);
        let default = Config::default();
        assert_eq!((cfg.colors.text, cfg.colors.border, cfg.fx.style, cfg.fx.crt), (default.colors.text, default.colors.border, default.fx.style, 0.0));
        assert_eq!(cfg.font.families, default.font.families);
    }

    #[test]
    fn each_look_sets_its_own_cursor_speed_on_the_sliders_steps_and_takes_it_off_again() {
        let (lo, hi, step) = crate::settings::CURSOR_SPEED;
        for look in &LOOKS {
            let speed = look.cursor_speed;
            assert!((lo..=hi).contains(&speed) && ((speed - lo) / step).fract() == 0.0, "{}: {speed}", look.id);
        }
        // A teletype's heavier than a PC's, and Flow's glides as it always has.
        assert!(find("teletype").cursor_speed < find("pc").cursor_speed);
        assert_eq!(find("flow").cursor_speed, crate::config::CursorConfig::default().speed);
        let mut mine = Config::default();
        mine.cursor.speed = 3.5;
        let mut worn = wear(&mine, "teletype");
        assert_eq!(worn.cursor.speed, 0.5);
        keep_look(&mine, &mut worn);
        assert_eq!(worn.cursor.speed, 3.5, "what's saved is the user's own");
    }

    #[test]
    fn the_school_micro_and_the_personal_computer_start_as_theirs_did() {
        let school = find("school");
        let screen = school.start_screen(&facts(), 80).unwrap();
        let lines: Vec<&str> = screen.split("\r\n").collect();
        // Its name double height, in yellow on blue, centered in the 40
        // columns a double line shows.
        let title = "       \x1b[48;2;0;0;255m\x1b[38;2;255;255;0m Flow School Computer 32K \x1b[0m";
        assert_eq!((lines[1], lines[2]), (format!("\x1b#3{title}").as_str(), format!("\x1b#4{title}").as_str()));
        // The picture's lines line up, each 40 columns centered in 80.
        let picture = &lines[4..9];
        assert!(picture.iter().all(|l| l.starts_with("                    ") && l.ends_with("\x1b[0m")), "{picture:?}");
        assert!(picture.iter().all(|l| colored(l, &mut String::new()).1 == 60), "the escapes take no room");
        assert!(screen.ends_with("\r\n\r\nFlow DFS\r\n\r\nBASIC\r\n\r\n"));
        assert_eq!((school.prompt, school.sounds), (Some(">"), Sounds::Tube));
        let pc = find("pc").start_screen(&facts(), 80).unwrap();
        assert!(pc.contains("Memory Test: 16777216K OK\r\n"), "{pc}");
        // Its prompt's backslash, as each shell writes one.
        let prompt = find("pc").prompt.unwrap();
        assert_eq!(prompt, "C:\\>");
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::Native, prompt), "C:\\D{\\}>");
        assert_eq!(prompt_for(ShellKind::Cmd, BashFlavor::Native, prompt), "C:\\$G");
        assert_eq!(prompt_for(ShellKind::PowerShell, BashFlavor::Native, prompt), "C:\\>");
    }

    #[test]
    fn the_cyan_home_computers_title_screen_lies_as_its_did_between_bars_of_its_colors() {
        let look = find("instruments");
        let screen = look.start_screen(&facts(), 32).unwrap();
        let lines: Vec<&str> = screen.split("\r\n").collect();
        // Its 32 × 24 title screen, row for row, waiting for a key on its
        // last; then the screen cleared for its BASIC's words at its foot,
        // and a line for the prompt below them.
        assert_eq!((lines.len(), look.screen, look.prompt), (28, Some((32, 24)), Some(">")));
        let bars = [0, 1, 2, 18, 19, 20];
        for row in bars {
            let (text, width) = colored(BARS.strip_prefix('^').unwrap(), &mut String::new());
            assert_eq!((lines[row], width), (text.as_str(), 32), "row {row}, the screen's width across");
        }
        // Sixteen colors two columns each, its red first, then its light
        // green and its black, in the chip's own colors.
        assert!(lines[0].starts_with("\x1b[48;2;213;82;77m  \x1b[48;2;94;221;120m  \x1b[48;2;0;0;0m  "), "{:?}", lines[0]);
        assert_eq!(lines[0].matches("\x1b[48;2;").count(), 16);
        // The logo alone in the middle of its line, four rows over the name.
        assert_eq!((lines[4], lines[5..9].iter().all(|l| l.is_empty())), (LOGO_CENTERED.to_string().as_str(), true));
        assert!(shows_logo(&screen) && !screen.contains(LOGO_HERE));
        let centered = |row: usize, words: &str| assert_eq!(lines[row].trim_start(), words, "row {row}");
        centered(9, "FLOW INSTRUMENTS");
        centered(11, "HOME COMPUTER");
        centered(16, "READY-PRESS ANY KEY TO BEGIN");
        centered(22, "©1981  FLOW INSTRUMENTS");
        assert_eq!(lines[16], "  READY-PRESS ANY KEY TO BEGIN", "as its was, two columns in");
        assert_eq!(&lines[23..], [KEY_WAIT.to_string().as_str(), "\x1b[2J\x1b[H\x1b[999;1H", " FLOW BASIC READY", "", ""]);
        // Wider, all of it stays in the middle.
        let wide = look.start_screen(&facts(), 80).unwrap();
        assert!(wide.split("\r\n").nth(9).unwrap().starts_with(&" ".repeat(32)));
        assert!(wide.split("\r\n").next().unwrap().starts_with(&format!("{}\x1b[48;2;213;82;77m", " ".repeat(24))));
    }

    #[test]
    fn a_paper_color_is_the_pages_till_plain_its_centering_and_clears_on_it() {
        let white = "\x1b[48;2;255;255;255m";
        let screen = start_screen(["{paper-white}{clear}", "^x", "{red}y{plain}z", "{paper-plain}w", "v"], &facts(), 5).unwrap();
        // Each line after it starts on it and goes back to it, `{plain}` too.
        assert_eq!(screen, format!("\x1b[0m{white}\x1b[2J\x1b[H\x1b[0m{white}\r\n  x\r\n\x1b[38;2;255;0;0my\x1b[0m{white}z\x1b[0m{white}\r\n\x1b[0mw\x1b[0m\r\nv\r\n"));
        // One still on at the end is taken off there.
        let blue = start_screen(["{paper-blue}x"], &facts(), 5).unwrap();
        assert_eq!(blue, "\x1b[0m\x1b[48;2;0;0;255mx\x1b[0m\x1b[48;2;0;0;255m\r\n\x1b[0m");
        for wrong in ["{paper-on-red}", "{paper-#12}", "{paper-}"] {
            assert_eq!(colored(wrong, &mut String::new()), (wrong.to_string(), wrong.chars().count()), "{wrong}");
        }
    }

    #[test]
    fn the_notanamiga_asks_for_its_disk_then_starts_its_shell_beside_the_ball() {
        let look = find("amiga");
        let screen = look.start_screen(&facts(), 80).unwrap();
        let (page, shell) = screen.split_once(&format!("{KEY_WAIT}\r\n")).expect("a wait for the disk");
        let lines: Vec<&str> = page.split("\r\n").collect();
        // A white page, as its first screen was.
        let white = "\x1b[48;2;255;255;255m";
        assert_eq!(lines[0], format!("\x1b[0m{white}\x1b[2J\x1b[H\x1b[0m{white}"));
        // The hand holding the disk out, as its first screen drew it; then
        // what it is and what to do, in black.
        let words = |line: &str| line.trim_start().replace(white, "").replace("\x1b[38;2;0;0;0m", "").replace("\x1b[0m", "");
        // On its 32 rows, the picture's 13 (`session::PICTURE_ROWS`) and the
        // two lines of words a row below sit in the middle.
        assert!(lines[1..8].iter().all(|line| line.is_empty()));
        assert_eq!((lines[8], lines[9]), (PICTURE_HERE.to_string().as_str(), ""));
        assert!(shows_picture(&screen) && !LOOKS.iter().filter(|l| l.id != "amiga").any(|l| l.start.contains(&"{disk}")));
        assert_eq!(words(lines[10]), "Flow notanAmiga Kickstart 0.1.22");
        assert_eq!(words(lines[11]), "Press any key to insert the Workbench disk");
        assert!(lines[11].contains("\x1b[38;2;0;0;0mPress"));
        assert_eq!(lines.len(), 13, "the key awaited on the line after");
        let (above, below) = (8usize, 32 - (8 + crate::session::PICTURE_ROWS as usize + 3));
        assert!(above.abs_diff(below) <= 1, "{above} rows above, {below} below");
        // Then its blue again, the screen cleared, the ball at its top left
        // with the shell's words beside it, and a line for the prompt.
        let shell: Vec<&str> = shell.split("\r\n").collect();
        assert_eq!(shell[0], format!("\x1b[0m\x1b[2J\x1b[H{LOGO_HERE}{BESIDE_LOGO}Copyright © 1985, 1986, 1987, 1988 Flow notanAmiga.\x1b[0m"));
        assert_eq!(&shell[1..5], [format!("{BESIDE_LOGO}All rights reserved."), format!("{BESIDE_LOGO}Release 0.1.22"), BESIDE_LOGO.to_string(), String::new()]);
        assert!(shows_logo(&screen) && !screen.ends_with("\x1b[0m"));
        // Its prompt, the shell's number and the folder, in every shell.
        let prompt = look.prompt.unwrap();
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::Native, prompt), "1.\\w> ");
        assert_eq!(prompt_for(ShellKind::Zsh, BashFlavor::Native, prompt), "1.%~> ");
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::BusyBox, prompt), "1.\\w> ");
        assert_eq!(prompt_for(ShellKind::PowerShell, BashFlavor::Native, prompt), "1.$($executionContext.SessionState.Path.CurrentLocation)> ");
        assert_eq!(prompt_for(ShellKind::Cmd, BashFlavor::Native, prompt), "1.$P$G ");
        // Its drive seeks as it starts and as the disk goes in, and clicks
        // while it's empty; no other machine's does.
        assert!(look.sounds.disk() && look.sounds.tube() && look.sounds.parts().contains(&Part::Drive));
        assert!(LOOKS.iter().filter(|l| l.sounds.empty_drive()).map(|l| l.id).eq(["amiga"]));
        assert_eq!((Sounds::named("floppy"), look.screen, look.crt), (Sounds::Floppy, Some((80, 32)), Some("monitor")));
        assert!(crate::settings::builtin_crt_presets().iter().any(|p| p.name == "monitor" && !p.interlace));
    }

    /// How many times as light as `b` color `a` is, as WCAG counts it.
    fn contrast(a: palette::Rgb8, b: palette::Rgb8) -> f32 {
        let lum = |c: palette::Rgb8| {
            let lin = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn the_notanamigas_colors_read_on_its_blue_but_its_own_black() {
        let look = find("amiga");
        let (ansi, blue) = (look.ansi.unwrap(), look.background);
        assert_eq!((ansi[0], ansi[3], look.cursor), ([0x00, 0x00, 0x22], WORKBENCH_ORANGE, WORKBENCH_ORANGE), "its black, and its orange for yellow and the cursor");
        for (i, color) in ansi.iter().enumerate().skip(1) {
            let least = if i < 9 { 3.0 } else { 4.5 };
            assert!(contrast(*color, blue) >= least, "{i}: {color:?} at {}", contrast(*color, blue));
        }
        assert!(contrast(look.text, blue) > 7.0);
        // Steps of its sixteen to a channel, as its colors were.
        assert!([look.text, look.cursor, blue].iter().chain(&ansi).flatten().all(|v| v % 0x11 == 0));
    }

    #[test]
    fn a_start_screen_takes_any_color_written_in_hex() {
        assert_eq!(colored("{#d5524d}A{on-#00FF7f}B{plain}C", &mut String::new()), ("\x1b[38;2;213;82;77mA\x1b[48;2;0;255;127mB\x1b[0mC\x1b[0m".to_string(), 3));
        // What isn't six hex digits is left as it's written.
        for wrong in ["{#d5524}", "{#d5524dd}", "{#gg0000}", "{on-#}", "{#}"] {
            assert_eq!(colored(wrong, &mut String::new()), (wrong.to_string(), wrong.chars().count()), "{wrong}");
        }
        // A logo on a centered line with words isn't the centered logo.
        let screen = start_screen(["^{logo} and more", "^{logo}"], &facts(), 40).unwrap();
        assert!(screen.contains(&format!("{LOGO_HERE} and more")) && screen.contains(&format!("\r\n{LOGO_CENTERED}\r\n")), "{screen:?}");
    }

    #[test]
    fn your_own_theme_brings_its_start_screen_prompt_and_sounds_begun_from_a_machine() {
        let mut cfg = Config::default();
        assert!(!InUse::of(&cfg).machine(), "Flow's look");
        cfg.theme = find("teletype").as_theme();
        cfg.ui.look = OWN.into();
        let own = InUse::of(&cfg);
        assert!(own.machine());
        assert_eq!((own.prompt(), own.baud(), own.sounds()), (Some("% "), 300, Sounds::Teletype));
        assert!(own.start_screen(&facts(), 80).unwrap().starts_with("FLOW TIME-SHARING SERVICE\r\n"));
        // Edited: its own lines, and no prompt of its own keeps the shell's.
        cfg.theme.start = vec!["^MY MACHINE ({shell})".into()];
        cfg.theme.prompt.clear();
        let own = InUse::of(&cfg);
        assert_eq!(own.start_screen(&facts(), 20).unwrap(), "  MY MACHINE (zsh)\r\n");
        assert_eq!(own.prompt(), None);
        cfg.theme.start.clear();
        assert!(InUse::of(&cfg).start_screen(&facts(), 80).is_none());
    }

    #[test]
    fn a_start_screen_says_what_the_computer_has() {
        let home = find("home").start_screen(&facts(), 40).unwrap();
        let lines: Vec<&str> = home.split("\r\n").collect();
        assert_eq!(lines[1], "      **** FLOW HOME COMPUTER ****", "centered on 40 columns");
        assert!(lines[3].contains("16 GB RAM SYSTEM  17179869184 BASIC BYTES FREE"));
        assert!(home.ends_with("\r\n"));
        let sun = find("workstation").start_screen(&facts(), 80).unwrap();
        // The PROM's boot: the memory probed in banks, the bus, the banner
        // beside the logo with made-up numbers, then `ok boot`. Where the
        // system doesn't say what its banks and buses are, they're made up:
        // the memory in four banks, and a SPARCstation 5's buses.
        assert!(sun.starts_with("Probing Memory Bank #0 4096 Megabytes\r\n") && sun.contains("Probing Memory Bank #7 Nothing there\r\n"));
        let lines: Vec<&str> = sun.split("\r\n").collect();
        assert_eq!(lines[..8], memory_banks(16));
        assert_eq!(&lines[8..14], &MADE_UP_PROBES[..]);
        assert_eq!(lines[14], "");
        let (serial, ether, hostid, sgi_ether) = made_up(7);
        assert!(ether.starts_with("8:0:20:") && hostid.starts_with("80") && hostid.len() == 8 && serial.len() == 7);
        assert_eq!(made_up(7), (serial.clone(), ether.clone(), hostid.clone(), sgi_ether.clone()));
        assert_ne!(made_up(8).1, ether);
        assert!(sun.contains(&format!("{LOGO_HERE}{BESIDE_LOGO}Flow notaSun Workstation, Keyboard Present\r\n{BESIDE_LOGO}ROM Rev. 0.1.22, 16384 MB memory installed, Serial #{serial}.\r\n")));
        assert!(sun.contains(&format!("Ethernet address {ether}, Host ID: {hostid}.")) && sun.contains("\r\nType  flow-help  for more information\r\nok boot\r\nBoot device: disk  File and args: zsh\r\n"));
        // SGI's: the cube beside the PROM's words, then IRIX's banner with
        // the same made-up numbers, the address in SGI's block.
        let sgi = find("graphics").start_screen(&facts(), 80).unwrap();
        assert!(sgi.starts_with("Running power-on diagnostics...\r\n\r\n"));
        assert!(sgi.contains(&format!("{LOGO_HERE}{BESIDE_LOGO}Starting up the system...\r\n{BESIDE_LOGO}\r\n{BESIDE_LOGO}To perform system maintenance instead, press <Esc>\r\n")));
        assert_eq!(sgi_ether, format!("08:00:69:{}:{}:{}", &hostid[2..4], &hostid[4..6], &hostid[6..]));
        assert!(sgi.contains(&format!("Flow Release 0.1.22 IP35 Version {serial} System V\r\n")) && sgi.contains(&format!("Ethernet (ec0) address {sgi_ether}\r\n")));
        assert!(find("flow").start_screen(&facts(), 80).is_none());
    }

    #[test]
    fn the_workstations_own_numbers_go_in_only_where_theyre_given() {
        let real = crate::identity::RealNumbers { ether: Some([0xa4, 0x83, 0xe7, 0x0f, 0x02, 0xc1]), serial: Some("C02XL0GH".into()) };
        let own = Facts { real, ..facts() };
        let sun = find("workstation").start_screen(&own, 80).unwrap();
        assert!(sun.contains("Serial #C02XL0GH.") && sun.contains("Ethernet address a4:83:e7:f:2:c1, Host ID: 800f02c1."), "{sun}");
        let sgi = find("graphics").start_screen(&own, 80).unwrap();
        assert!(sgi.contains("Version C02XL0GH System V") && sgi.contains("Ethernet (ec0) address a4:83:e7:0f:02:c1"));
        // An address alone: the serial stays made up.
        let half = Facts { real: crate::identity::RealNumbers { serial: None, ..own.real.clone() }, ..facts() };
        assert!(find("workstation").start_screen(&half, 80).unwrap().contains(&format!("Serial #{}.", made_up(7).0)));
        // Only the workstations show numbers.
        assert!(find("workstation").shows_numbers() && find("graphics").shows_numbers() && !find("teletype").shows_numbers());
    }

    fn probe(path: &str, at: &str, names: &[&str]) -> Probe {
        Probe { path: path.into(), at: at.into(), names: names.iter().map(|n| n.to_string()).collect() }
    }

    /// An Apple silicon Mac's memory and buses, as `crate::hardware` reads
    /// them.
    fn mac() -> Hardware {
        let apcie = "/arm-io@10F00000/apcie0@80000000";
        let apciec = |i: u8| probe(&format!("/arm-io@10F00000/apciec{i}@30000000"), "0,0", &[]);
        Hardware {
            banks: vec![Bank { megabytes: 32768, kind: "LPDDR5".into(), speed: 0 }],
            probes: vec![probe(apcie, "0,0", &["wlan", "bluetooth-pcie"]), probe(apcie, "1,0", &["pcie-sdreader"]), apciec(0), apciec(1), apciec(2)],
        }
    }

    #[test]
    fn the_workstation_probes_the_computers_own_memory_and_buses() {
        let apple = Facts { memory_gb: 32, hardware: mac(), ..facts() };
        let sun = find("workstation").start_screen(&apple, 80).unwrap();
        let boot = "\
Probing Memory Bank #0 32768 Megabytes of LPDDR5\r
Probing /arm-io@10F00000/apcie0@80000000 at 0,0  wlan bluetooth-pcie\r
Probing /arm-io@10F00000/apcie0@80000000 at 1,0  pcie-sdreader\r
Probing /arm-io@10F00000/apciec0@30000000 at 0,0  Nothing there\r
Probing /arm-io@10F00000/apciec1@30000000 at 0,0  Nothing there\r
Probing /arm-io@10F00000/apciec2@30000000 at 0,0  Nothing there\r
\r
";
        assert!(sun.starts_with(boot), "{sun}");
        assert!(sun.contains(", 32768 MB memory installed, Serial #"), "{sun}");
        // A PC's banks, kinds and speeds, with an empty slot and one whose
        // kind isn't known (nor so its speed).
        let banks = vec![
            Bank { megabytes: 16384, kind: "DDR4".into(), speed: 3200 },
            Bank { megabytes: 16384, kind: "DDR4".into(), speed: 3200 },
            Bank::default(),
            Bank { megabytes: 8192, kind: String::new(), speed: 2400 },
        ];
        let pc = Facts { memory_gb: 40, hardware: Hardware { banks, probes: Vec::new() }, ..facts() };
        let lines = bank_lines(&pc);
        assert_eq!(lines, ["Probing Memory Bank #0 16384 Megabytes of DDR4-3200", "Probing Memory Bank #1 16384 Megabytes of DDR4-3200", "Probing Memory Bank #2 Nothing there", "Probing Memory Bank #3 8192 Megabytes"]);
        assert_eq!(pc.megabytes(), 40960);
        // An APU whose graphics were given most of its memory: the system
        // sees 16 GB of 64, and the PROM probes what's installed.
        let banks = vec![Bank { megabytes: 16384, kind: "LPDDR5X".into(), speed: 8000 }; 4];
        let apu = Facts { memory_gb: 16, hardware: Hardware { banks, probes: Vec::new() }, ..facts() };
        assert_eq!(bank_lines(&apu)[3], "Probing Memory Bank #3 16384 Megabytes of LPDDR5X-8000");
        assert!(find("workstation").start_screen(&apu, 80).unwrap().contains(", 65536 MB memory installed,"));
    }

    /// How many more items, and of what, a cut list's last line says it left out.
    fn left_out(line: &str, item: &str) -> (usize, String) {
        let rest = line.strip_prefix("Probing ").unwrap();
        let (n, rest) = rest.split_once(' ').unwrap();
        let what = rest.split_once("  ").unwrap().1;
        assert!(rest.starts_with(&format!("more {item}")), "{line}");
        (n.parse().unwrap(), what.to_string())
    }

    #[test]
    fn a_long_list_is_cut_to_eight_lines_and_says_what_it_left_out() {
        // 32 banks, every other one filled.
        let banks: Vec<Bank> = (0..32).map(|i| if i % 2 == 0 { Bank { megabytes: 8192, kind: "DDR5".into(), speed: 4800 } } else { Bank::default() }).collect();
        let server = Facts { memory_gb: 128, hardware: Hardware { banks, probes: Vec::new() }, ..facts() };
        let lines = bank_lines(&server);
        assert_eq!(lines.len(), MOST_LINES);
        // The filled ones first, each by its own slot's number.
        assert_eq!(lines[1], "Probing Memory Bank #2 8192 Megabytes of DDR5-4800");
        assert_eq!(lines[6], "Probing Memory Bank #12 8192 Megabytes of DDR5-4800");
        let (n, what) = left_out(&lines[7], "Memory Banks");
        assert_eq!((n, what.as_str()), (25, "73728 Megabytes"));
        assert_eq!(7 * 8192 + 73728, server.hardware.megabytes());
        assert_eq!(lines[7], "Probing 25 more Memory Banks  73728 Megabytes");
        // Left out empty, they're nothing there.
        let banks: Vec<Bank> = (0..9).map(|i| Bank { megabytes: if i < 2 { 8192 } else { 0 }, ..Default::default() }).collect();
        let empty = Facts { memory_gb: 16, hardware: Hardware { banks, probes: Vec::new() }, ..facts() };
        assert_eq!(bank_lines(&empty).last().unwrap(), "Probing 2 more Memory Banks  Nothing there");
        // 20 slots, half of them filled with one name or two.
        let probes: Vec<Probe> = (0..20u32)
            .map(|i| {
                let names: Vec<String> = (0..i % 3).map(|j| format!("drv{i}x{j}")).collect();
                Probe { path: "/pci@0,0".into(), at: format!("{i:x},0"), names }
            })
            .collect();
        let total: usize = probes.iter().map(|p| p.names.len()).sum();
        let lines = probe_lines(&Hardware { banks: Vec::new(), probes });
        assert_eq!(lines.len(), MOST_LINES);
        let shown: usize = lines[..7].iter().map(|l| l.split_once("  ").unwrap().1.split(' ').count()).sum();
        let (n, what) = left_out(&lines[7], "slots");
        let devices: usize = what.strip_suffix(" devices").unwrap().parse().unwrap();
        assert_eq!((n + 7, shown + devices), (20, total));
        assert!(lines[..7].iter().all(|l| !l.ends_with(NOTHING_THERE)), "the filled ones first: {lines:?}");
        // Eight fit as they are; one line that can't be shown cuts the list
        // to seven and says so.
        let eight: Vec<Probe> = (0..8).map(|i| probe("/pci@0,0", &format!("{i},0"), &[])).collect();
        assert_eq!(probe_lines(&Hardware { banks: Vec::new(), probes: eight.clone() }).len(), 8);
        let mut cut = eight;
        cut[3].path = "/Alex's".into();
        cut[4].names = vec!["nvme".into()];
        let lines = probe_lines(&Hardware { banks: Vec::new(), probes: cut });
        assert_eq!(lines.len(), 8);
        assert!(lines.iter().any(|l| l.ends_with("at 4,0  nvme")), "{lines:?}");
        assert_eq!(lines[7], "Probing 1 more slot  Nothing there");
    }

    #[test]
    fn no_probe_line_passes_79_columns() {
        let line = |p: Probe| probe_line(&p);
        // Names that don't fit: as many as do, then how many more.
        let many = line(probe("/iommu@fed91000/pci@0,0", "1c,4", &["snd_hda_intel"; 20])).unwrap();
        assert!(many.len() <= WIDEST && many.starts_with("Probing /iommu@fed91000/pci@0,0 at 1c,4  snd_hda_intel snd_hda_intel "), "{many}");
        let shown = many.matches("snd_hda_intel").count();
        assert!(many.ends_with(&format!(" +{} more", 20 - shown)), "{many}");
        // A deep path loses its leading parts.
        let deep = "/arm-io@10F00000/apcie0@80000000/pci-bridge0@0/pci-switch@0/pci-port@1/thunderbolt@0/pci@0";
        let shortened = line(probe(deep, "0,0", &["xhci"])).unwrap();
        assert!(shortened.len() <= WIDEST && shortened.starts_with("Probing /.../") && shortened.ends_with("/thunderbolt@0/pci@0 at 0,0  xhci"), "{shortened}");
        // One whose last part alone won't fit can't be shown.
        assert_eq!(line(probe(&format!("/{}", "a".repeat(70)), "0,0", &[])), None);
        // Names none of which may be shown: how many there are, never
        // "Nothing there".
        assert_eq!(line(probe("/pci@0,0", "3,0", &["Alex's iPhone"])).unwrap(), "Probing /pci@0,0 at 3,0  1 device");
        assert_eq!(line(probe("/pci@0,0", "3,0", &["My Mac", ""])).unwrap(), "Probing /pci@0,0 at 3,0  2 devices");
        assert_eq!(line(probe("/pci@0,0", "3,0", &["wlan", "My Mac"])).unwrap(), "Probing /pci@0,0 at 3,0  wlan +1 more");
        assert_eq!(line(probe("/pci@0,0", "3,0", &[&"x".repeat(31), &"y".repeat(31)])).unwrap(), format!("Probing /pci@0,0 at 3,0  {} +1 more", "x".repeat(31)));
        assert_eq!((line(probe("/pci@0,0", "{n}", &[])), line(probe("pci@0,0", "0,0", &[]))), (None, None));
    }

    #[test]
    fn hardware_that_could_name_someone_never_reaches_the_screen() {
        let uuid = "123e4567-e89b-12d3-a456-426614174000";
        let long = "a".repeat(40);
        let hostile = ["Alex's iPhone", "My Mac", "00:11:22:33:44:55", uuid, "C02XL0GHJGH5", "\x1b[2J", "{red}", "{serial}", "{logo}", "x\u{202e}y", &long];
        let names: Vec<&str> = hostile.iter().copied().chain(["", "wlan", "snd_hda_intel", "pci8086,a706", "nvme", "bluetooth-pcie"]).collect();
        let deep = format!("/{}", ["arm-io@10F00000", "apcie0@80000000", "pci-bridge0@0", "pci@0", "pci@1"].repeat(4).join("/"));
        let wide = format!("/{}", "x".repeat(130));
        let paths = ["/pci@0,0", "/iommu@fed91000/pci@0,0", "/arm-io@10F00000/apcie0@80000000", "/pci@0,0 ", "/Alex's", "{logo}", "/\x1b[31m", "/a:b", &deep, &wide];
        let ats = ["0,0", "1c,4", "{n}", "ZZ", "", "1234567890", "a,b"];
        let kinds = ["DDR4", "LPDDR5X", "{red}", "Alex's", "\x1b[2J", "RAM", "", "a:b"];
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |n: usize| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % n as u64) as usize
        };
        for _ in 0..400 {
            let banks: Vec<Bank> = (0..next(65)).map(|_| Bank { megabytes: [0, 4096, 8192, 16384, 1 << 30][next(5)], kind: kinds[next(kinds.len())].into(), speed: [0, 3200, 99999][next(3)] as u32 }).collect();
            let probes: Vec<Probe> = (0..1 + next(64)).map(|_| Probe { path: paths[next(paths.len())].into(), at: ats[next(ats.len())].into(), names: (0..next(7)).map(|_| names[next(names.len())].to_string()).collect() }).collect();
            let random = Facts { memory_gb: [0, 16, 64][next(3)], hardware: Hardware { banks, probes }, ..facts() };
            let (banks, probes) = (bank_lines(&random), probe_lines(&random.hardware));
            assert!(banks.len() <= MOST_LINES && probes.len() <= MOST_LINES, "{banks:?} {probes:?}");
            for line in banks.iter().chain(&probes) {
                assert!(line.len() <= WIDEST && line.is_ascii(), "{line:?}");
                assert!(!line.contains(['{', '\x1b', ':', '\'']), "{line:?}");
                assert!(hostile.iter().all(|h| !line.contains(h)), "{line:?}");
            }
            let screen = find("workstation").start_screen(&random, 80).unwrap();
            assert!(hostile.iter().all(|h| !screen.contains(h)), "{screen}");
            assert_eq!(screen.matches('{').count() + screen.matches('\x1b').count(), 0, "{screen}");
        }
    }

    #[test]
    fn banks_that_dont_add_up_fall_back_to_the_made_up_ones() {
        let banks = vec![Bank { megabytes: 8192, kind: "DDR4".into(), speed: 3200 }];
        let short = Facts { memory_gb: 64, hardware: Hardware { banks, probes: Vec::new() }, ..facts() };
        assert_eq!(bank_lines(&short), memory_banks(64));
        assert_eq!(short.megabytes(), 65536);
        assert!(find("workstation").start_screen(&short, 80).unwrap().contains(", 65536 MB memory installed,"));
        let absurd = Facts { hardware: Hardware { banks: vec![Bank { megabytes: u64::MAX, ..Default::default() }], probes: Vec::new() }, ..facts() };
        assert_eq!((bank_lines(&absurd), absurd.megabytes()), (memory_banks(16), 16384));
    }

    #[test]
    fn snapshots_probe_made_up_hardware() {
        let made_up = Facts::made_up("0.1.27".into(), "macOS 27.0".into(), "zsh".into());
        assert_eq!((made_up.hardware.clone(), made_up.memory_gb, made_up.chip.as_str(), made_up.seed), (Hardware::default(), 16, MADE_UP_CHIP, 7));
        let fallback = Facts { version: "0.1.27".into(), chip: MADE_UP_CHIP.into(), ..facts() };
        let screen = find("workstation").start_screen(&made_up, 80).unwrap();
        assert_eq!(screen, find("workstation").start_screen(&fallback, 80).unwrap());
        assert!(screen.contains(&MADE_UP_PROBES.join("\r\n")) && screen.contains("16384 MB memory installed") && screen.contains(MADE_UP_CHIP));
    }

    #[test]
    fn the_workstations_start_screen_reads_the_hardware_and_the_others_dont() {
        let workstation = InUse::Builtin(find("workstation"));
        assert!(workstation.reads_hardware());
        let theme = find("workstation").as_theme();
        assert!(theme.start.iter().any(|l| l == "{probes}") && InUse::Own(OWN, &theme).reads_hardware());
        for id in ["graphics", "home", "flow", "pc"] {
            assert!(!InUse::Builtin(find(id)).reads_hardware(), "{id}");
            assert!(!InUse::Own(OWN, &find(id).as_theme()).reads_hardware(), "{id}");
        }
        // An own theme begun before `{probes}` keeps its six lines, as its own text.
        let mut old = theme.clone();
        old.start = MADE_UP_PROBES.iter().map(|l| l.to_string()).chain(theme.start.iter().filter(|l| *l != "{banks}" && *l != "{probes}").cloned()).collect();
        assert!(!InUse::Own(OWN, &old).reads_hardware());
        assert!(InUse::Own(OWN, &old).start_screen(&facts(), 80).unwrap().starts_with(&MADE_UP_PROBES[..2].join("\r\n")));
    }

    #[test]
    fn a_start_screen_line_takes_teletexts_colors_and_double_sizes() {
        let screen = |lines: &[&str], cols| start_screen(lines.iter().copied(), &facts(), cols).unwrap();
        assert_eq!(screen(&["{red}A{on-cyan}B{plain}C"], 80), "\x1b[38;2;255;0;0mA\x1b[48;2;0;255;255mB\x1b[0mC\x1b[0m\r\n");
        // Unknown names stay as they're written, and so does a line without colors.
        assert_eq!(screen(&["{teal}{shell} {x"], 80), "{teal}zsh {x\r\n");
        // A double-width line centers in half the columns; a tall one twice.
        assert_eq!(screen(&["^{wide}{green}Hi"], 20), "\x1b#6    \x1b[38;2;0;255;0mHi\x1b[0m\r\n");
        assert_eq!(screen(&["^{tall}Hi"], 21), "\x1b#3    Hi\r\n\x1b#4    Hi\r\n");
        // An escape the line holds itself takes no room either.
        assert_eq!(screen(&["^\x1b[1mBold\x1b[0m"], 10), "   \x1b[1mBold\x1b[0m\r\n");
        assert_eq!(escape_len("\x1b#é!"), 4);
        assert_eq!(escape_len("\x1b"), 1);
    }

    #[test]
    fn a_prompt_is_written_in_each_shells_own_way() {
        let t = "{n}% {cwd}\n$ ";
        let native = BashFlavor::Native;
        // bash shows a $ that an escape gave as itself (strftime's, with no
        // % in it), and expands none of it.
        assert_eq!(prompt_for(ShellKind::Bash, native, t), "\\!% \\w\n\\D{$} ");
        assert_eq!(prompt_for(ShellKind::Zsh, native, t), "%h%% %~\n%-4v ");
        assert_eq!(prompt_for(ShellKind::PowerShell, native, t), "$($MyInvocation.HistoryId)% $($executionContext.SessionState.Path.CurrentLocation)`n`$ ");
        assert_eq!(prompt_for(ShellKind::Cmd, native, t), "% $P$_$$ ");
        assert_eq!(prompt_for(ShellKind::Bash, native, "a\\b"), "a\\D{\\}b");
        assert_eq!(prompt_for(ShellKind::Zsh, native, "READY.\n"), "READY.\n");
        // BusyBox's ash shows no history number: its prompt has no `\!`. It
        // expands the prompt before reading its escapes, so its $ is an
        // octal escape only the line editor reads.
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::BusyBox, t), "% \\w\n\\044 ");
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::BusyBox, "C:\\>"), "C:\\\\\\\\>");
    }

    #[test]
    fn a_themes_prompt_is_text_that_no_shell_expands() {
        // What a shell would expand, run or read as an escape, a quote of
        // each kind PowerShell knows, controls that don't show, and a color.
        let t = "$(id) `id` ${HOME} \\ ! % \"q\" \u{201c}q\u{201d}\u{201e} \u{0}\u{1}\r\t\x1b[1m.";
        let native = BashFlavor::Native;
        let bash = "\\D{$}(id) \\D{`}id\\D{`} \\D{$}{HOME} \\D{\\} \\D{!} % \"q\" \u{201c}q\u{201d}\u{201e} \t\x1b[1m.";
        assert_eq!(prompt_for(ShellKind::Bash, native, t), bash);
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::GitBash, t), bash);
        assert_eq!(prompt_for(ShellKind::Sh, native, t), bash);
        // zsh's own escapes, %, are doubled, and what its options would
        // expand comes from the end of psvar, where its hook puts $ ` \ !.
        let zsh = prompt_for(ShellKind::Zsh, native, t);
        assert_eq!(zsh, "%-4v(id) %-3vid%-3v %-4v{HOME} %-2v %-1v %% \"q\" \u{201c}q\u{201d}\u{201e} \t\x1b[1m.");
        assert!(!zsh.contains(ZSH_LITERALS));
        // ash holds no $ or ` at all, and nothing that turns into the ^A its
        // backslashes travel as.
        let ash = prompt_for(ShellKind::Bash, BashFlavor::BusyBox, t);
        assert_eq!(ash, "\\044(id) \\140id\\140 \\044{HOME} \\\\\\\\ ! % \"q\" \u{201c}q\u{201d}\u{201e} \t\x1b[1m.");
        assert!(!ash.contains(['$', '`', '\u{1}']));
        assert_eq!(
            prompt_for(ShellKind::PowerShell, native, t),
            "`$(id) ``id`` `${HOME} \\ ! % \"q\" `\u{201c}q`\u{201d}`\u{201e} \t\x1b[1m."
        );
        // cmd reads its $ codes alone, every $ of the theme's doubled.
        assert_eq!(prompt_for(ShellKind::Cmd, native, t), "$$$Cid$F `id` $${HOME} \\ ! % \"q\" \u{201c}q\u{201d}\u{201e} \t\x1b[1m.");
        // No variable can hold a NUL, so a theme's would keep the tab from
        // opening; it's left out with the rest.
        for kind in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Sh, ShellKind::PowerShell, ShellKind::Cmd] {
            assert!(!prompt_for(kind, native, t).contains(['\0', '\u{1}', '\r']), "{kind:?}");
        }
    }

    #[test]
    fn a_backslash_bash_is_given_never_follows_a_byte_that_may_begin_a_letter() {
        // ぁ ends in 0x81, which in Shift JIS or GB18030 begins a letter
        // whose second byte may be the backslash bash puts before a $ an
        // escape gave, so that $ would be expanded after all; and the
        // folder's name may end in such a byte too. \001\002 parts them.
        let native = BashFlavor::Native;
        let sep = "\\001\\002";
        assert_eq!(
            prompt_for(ShellKind::Bash, native, "ぁ$ ぁ` ぁ\\ ぁ! a$ {cwd}$ {n}$"),
            format!("ぁ{sep}\\D{{$}} ぁ{sep}\\D{{`}} ぁ{sep}\\D{{\\}} ぁ{sep}\\D{{!}} a\\D{{$}} \\w{sep}\\D{{$}} \\!\\D{{$}}")
        );
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::GitBash, "ぁ$"), format!("ぁ{sep}\\D{{$}}"));
        // What's left out between them parts nothing; a color ends in m.
        assert_eq!(prompt_for(ShellKind::Bash, native, "ぁ\u{1}$"), format!("ぁ{sep}\\D{{$}}"));
        assert_eq!(prompt_for(ShellKind::Bash, native, "ぁ\x1b]0;x\x07$"), format!("ぁ{sep}\\D{{$}}"));
        assert_eq!(prompt_for(ShellKind::Bash, native, "ぁ\x1b[0m$"), "ぁ\x1b[0m\\D{$}");
        // The other shells' escapes need nothing of the kind.
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::BusyBox, "ぁ$"), "ぁ\\044");
        assert_eq!(prompt_for(ShellKind::Zsh, native, "ぁ$"), "ぁ%-4v");
    }

    #[test]
    fn a_prompts_only_escapes_are_its_colors() {
        // Flow's own marks and questions, a clear, a string ended by the
        // next escape, a DCS, and an escape left open at the end.
        let t = "\x1b[1;38;5;196mgo\x1b[0m\x1b]133;A\x07\x1b]7732;e30=\x1b\\\x1b[2J\x1b]633;P;Cwd=/tmp\x1b[38:2::1:2:3m>\x1bP1;2q#0\x1b\\ \x1b";
        assert_eq!(prompt_for(ShellKind::Zsh, BashFlavor::Native, t), "\x1b[1;38;5;196mgo\x1b[0m\x1b[38:2::1:2:3m> ");
        for kind in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Sh, ShellKind::PowerShell, ShellKind::Cmd] {
            let written = prompt_for(kind, BashFlavor::Native, t);
            for gone in ["133", "7732", "633", "[2J", "q#0"] {
                assert!(!written.contains(gone), "{kind:?}: {written:?}");
            }
        }
    }

    #[test]
    fn a_prompt_too_long_for_a_variable_is_cut_at_a_whole_piece() {
        let written = prompt_for(ShellKind::Bash, BashFlavor::Native, &"$".repeat(10_000));
        assert!(written.len() <= MOST_PROMPT && written.len() > MOST_PROMPT - 5, "{}", written.len());
        assert_eq!(written, "\\D{$}".repeat(written.len() / 5));
        let colors = prompt_for(ShellKind::Zsh, BashFlavor::Native, &"\x1b[0m".repeat(10_000));
        assert!(colors.len() <= MOST_PROMPT && colors.ends_with('m'));
        // A theme's own prompt is far within it.
        assert_eq!(prompt_for(ShellKind::Bash, BashFlavor::Native, "{cwd}> "), "\\w> ");
    }

    #[test]
    fn a_machines_colors_font_and_tube_each_follow_their_switch() {
        let mut cfg = Config::default();
        find("graphics").apply(&mut cfg);
        let ink = cfg.colors.ink();
        assert_eq!(ink.ansi, Some(GRAPHICS_COLORS), "programs' sixteen, its own");
        assert_eq!(palette::color_for_index(3, ink), GRAPHICS_COLORS[3], "its yellow, dark enough for its grey");
        assert!(cfg.font.families[0] == "SGI Screen");
        // Its colors off: Flow's, and the rest of it still worn.
        cfg.ui.look_colors = false;
        find("graphics").apply_colors(&mut cfg);
        assert_eq!(cfg.colors.ink().ansi, None);
        assert_eq!(cfg.colors.background, palette::hex(palette::BACKGROUND));
        assert_eq!(cfg.font.families[0], "SGI Screen");
        // Its font off, and the PC's tube off.
        cfg.ui.look_font = false;
        find("graphics").apply_font(&mut cfg);
        assert_eq!(cfg.font.families, FontConfig::default().families);
        cfg.ui.look_crt = false;
        find("pc").apply(&mut cfg);
        assert_eq!((cfg.fx.crt, cfg.ui.look.as_str()), (0.0, "pc"));
        // Flow's own look is always Flow's, whatever the switches say.
        find("flow").apply(&mut cfg);
        assert_eq!(cfg.colors.ink().ansi, None);
    }

    #[test]
    fn a_machine_worn_before_its_colors_were_its_own_takes_them() {
        let mut cfg: Config = toml::from_str("[ui]\nlook = \"pc\"\n").unwrap();
        assert!(cfg.colors.ansi.is_none());
        fill_in_colors(&mut cfg);
        assert_eq!(cfg.colors.ink().ansi, Some(PC_COLORS));
        // Flow's own sixteen, written on purpose under the PC, stay.
        let mut cfg: Config = toml::from_str("[ui]\nlook = \"pc\"\n[colors]\nansi = []\n").unwrap();
        fill_in_colors(&mut cfg);
        assert_eq!(cfg.colors.ink().ansi, None);
        // Flow's look, your own, or the machine's colors turned off: nothing.
        for (look, colors) in [("flow", true), ("own", true), ("pc", false)] {
            let mut cfg = Config::default();
            (cfg.ui.look, cfg.ui.look_colors) = (look.into(), colors);
            fill_in_colors(&mut cfg);
            assert!(cfg.colors.ansi.as_ref().is_none_or(|a| a.is_empty()), "{look}");
        }
    }

    #[test]
    fn the_green_and_amber_screens_show_everything_in_their_phosphor() {
        let mut cfg = Config::default();
        find("green").apply(&mut cfg);
        assert_eq!(cfg.colors.ink().phosphor, Some(GREEN));
        find("amber").apply(&mut cfg);
        assert_eq!(cfg.colors.ink().phosphor, Some(AMBER));
        // A machine in color has none, nor does Flow's look.
        for id in ["pc", "instruments", "flow"] {
            find(id).apply(&mut cfg);
            assert_eq!((id, cfg.colors.ink().phosphor), (id, None));
        }
        // With Theme details' Colors off, the green screen is in color.
        cfg.ui.look_colors = false;
        find("green").apply(&mut cfg);
        assert_eq!(cfg.colors.ink().phosphor, None);
    }

    #[test]
    fn a_phosphors_own_shades_keep_their_places_between_black_and_full() {
        // Each becomes the phosphor's shade exactly, purer where it was
        // paler, as a monochrome monitor has no paler green but a brighter
        // one: as bright as its signal's luma, white the phosphor at full,
        // so the paler shades show brighter than the text and none whiter.
        let unit = |c: palette::Rgb8| palette::luma(c.map(|v| v as f32 / 255.0));
        for (colors, look, phosphor) in [(GREEN_COLORS, "green", GREEN), (AMBER_COLORS, "amber", AMBER)] {
            let full = unit(phosphor);
            for c in colors.into_iter().chain([find(look).background, find(look).text]) {
                let shown = palette::on_phosphor(c, phosphor);
                assert!((unit(c) - unit(shown) / full).abs() <= 0.02, "{} showed as {}", palette::hex(c), palette::hex(shown));
            }
        }
    }

    #[test]
    fn a_green_screen_worn_from_before_its_phosphor_takes_it() {
        let mut cfg: Config = toml::from_str("[ui]\nlook = \"green\"\n").unwrap();
        assert!(cfg.colors.phosphor.is_none());
        fill_in_colors(&mut cfg);
        assert_eq!(cfg.colors.ink().phosphor, Some(GREEN));
        // Set to none on purpose, it stays none; and every other config
        // gets the key, empty.
        let mut cfg: Config = toml::from_str("[ui]\nlook = \"green\"\n[colors]\nphosphor = \"\"\n").unwrap();
        fill_in_colors(&mut cfg);
        assert_eq!(cfg.colors.ink().phosphor, None);
        let mut cfg = Config::default();
        fill_in_colors(&mut cfg);
        assert_eq!(cfg.colors.phosphor.as_deref(), Some(""));
    }

    #[test]
    fn a_saved_theme_keeps_its_phosphor() {
        let mut cfg = wear(&Config::default(), "amber");
        cfg.colors.phosphor = Some("#E8F0F0".into());
        let saved = save_as(&cfg, "White");
        let mut base = Config::default();
        base.themes.saved.push(saved);
        let worn = wear(&base, &saved_id("White"));
        assert_eq!(worn.colors.ink().phosphor, Some([0xE8, 0xF0, 0xF0]));
        // And it saves to the config file and reads back.
        let text = toml::to_string(&base).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(wear(&back, &saved_id("White")).colors.ink().phosphor, Some([0xE8, 0xF0, 0xF0]));
    }
}
