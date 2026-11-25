# Fonts

The fonts Flow builds in (`core/src/render/atlas.rs`), so the old machines'
themes look as they should without installing anything. Only fonts whose
licenses allow it go here, and fonts Flow draws itself, which are Flow's
under its [MIT License](../../LICENSE): their scripts, and the `.ttf` files
they write, whose copyright names "the Flow authors" as LICENSE does.
`NOTICES.txt` is their section of a release's THIRD-PARTY-NOTICES.txt,
which each OS's release script adds, followed by the notice on the makers'
logos Flow shows (`core/src/logos.rs`, `assets/logos/`).

- `VT323-Regular.ttf`: VT323 2.000, SIL Open Font License 1.1
  (`VT323-OFL.txt`), from <https://github.com/google/fonts/tree/main/ofl/vt323>.
  The Green screen and Amber screen themes.
- `bedstead.otf`: Bedstead 3.261 by Ben Harris, public domain, from
  <https://bjh21.me.uk/bedstead/>. The School computer theme, and teletext's
  mosaic characters.
- `gallant.ttf`: Gallant Raster Term 1.007 by Jens Schweikhardt, the Sun
  console's Gallant, BSD 2-Clause License (`gallant-LICENSE.txt`), from
  <https://github.com/NanoBillion/gallant>. The Workstation theme.
- `unscii-8.ttf` and `unscii-16.ttf`: unscii by Viznut, public domain (or
  CC0), from <https://github.com/viznut/unscii> (`fontfiles/`; not
  `unscii-16-full`, which is GPL). Both name their family "unscii", so Flow
  loads them as `unscii-8` and `unscii-16`. The Home computer and Personal
  computer themes.
- `sgi-screen.ttf`: SGI Screen, Silicon Graphics' Screen font (MIT License,
  `sgi-screen-LICENSE.txt`), from Nicholas Riley's conversion of the SGI
  ProPack's fonts, <http://sabi.net/nriley/software/screen/> (`ScreenMedium.ttf`).
  That file holds only bitmaps, which Flow's text can't draw, so
  `bitmap2ttf.py` made its 18-pixel strike outlines, each pixel a square:
  `python3 -I bitmap2ttf.py ScreenMedium.ttf sgi-screen.ttf 18 "SGI Screen" "<copyright>"`.
  The Graphics workstation theme.
- `flow-teletype.ttf`: Flow Teletype, Flow's own, drawn for Flow after the
  type a Teletype Model 33 printed, since no free font has it. The Teletype
  theme. `teletype.py` makes it with plain Python, from strokes along center
  lines traced by a round pen (the round ends and the ink in the corners of
  the Model 33's printouts), and can write a sample sheet of every glyph:
  `python3 -I teletype.py flow-teletype.ttf [sheet.png]`. It has the Model
  33's 64 characters, lowercase as small capitals, the rest of ASCII and
  Latin-1, the arrows, and the quotation marks and dashes programs print;
  `^` and `_` are a caret and an underscore, and the Model 33's ↑ and ← are
  at their own code points. Every glyph is 0.6 em wide.
- `flow-instruments.ttf`: Flow Instruments, Flow's own too, the letters
  of the Flow Instruments Home Computer theme: an early-eighties home
  computer's, which drew its text on a television from cells of 8 by 8
  dots, capitals and digits five dots by seven with lowercase as small
  capitals and no descenders. `instruments.py` holds each glyph's dots as
  text and writes the font, each dot a square of the outline, and can
  write a sample sheet: `python3 -I instruments.py flow-instruments.ttf
  [sheet.png]`. It has ASCII and the © its title screen carries (other
  letters come from fallback fonts); every glyph is a cell as wide as it
  is tall, with unscii-8's line gap, which it took over from.
- `flow-notatopaz.ttf`: Flow notaTopaz, Flow's own too, the letters of the
  Flow notanAmiga CLI theme, drawn in the manner of the Amiga's Topaz,
  which isn't free to ship, but none of its glyphs: a 16-bit home
  computer's, which drew its shell's text from cells of 8 by 8 dots on a
  screen whose dots were twice as tall as wide, so uprights are two dots
  wide and strokes across one; capitals and digits seven rows tall, the
  lowercase five, with tails in the eighth row, below the baseline.
  `notatopaz.py` holds each glyph's dots as text and writes the font, each
  dot a rectangle of the outline half as wide as it's tall, and can write
  a sample sheet: `python3 -I notatopaz.py flow-notatopaz.ttf
  [sheet.png]`. It has ASCII and the © its start screen carries (other
  letters come from unscii-16, whose 8 × 16 cells are the same shape, after
  it in the theme's fonts); every glyph is half an em wide, and the font
  has no line gap.
- `SpecialElite-Regular.ttf`: Special Elite 1.001 by Astigmatic, Apache
  License 2.0 (`SpecialElite-LICENSE.txt`), from
  <https://github.com/google/fonts/tree/main/apache/specialelite>, a
  typewriter's letters. The Teletype theme names it after Flow Teletype, and
  it stays in Settings' fonts. Its letters differ in width, so Flow sets each
  in the middle of an M-wide cell.
