---
name: theme-package
description: Making a package of themes for Flow: a themes.json of colors, fonts, CRT, start screen, prompt, sounds and logo, packed as a .tar.gz and installed with flowapp, or a saved theme exported from Settings. Use when the user wants to make, share or install a theme package.
---

A theme package is a `.tar.gz` with `themes.json` at its top, plus the font
files and PNG logos it names. It is only data (a start screen only types
text), so one's own needs no signature. Flow checks every theme as it
installs and refuses the whole package if any one is wrong.

## The quick way: export a saved theme

If the look is already worn in Flow, the user does this (you can't click
Settings for them):

1. Settings, Theme section, **Your themes** row: **Save as theme…** keeps
   what's worn under a name.
2. With that theme chosen in the Theme row, **Export…** on the same row
   writes `<package>-1.0.tar.gz` ("Night shift" gives
   `night-shift-theme-1.0.tar.gz`, theme id `night-shift`).
3. Its fonts are named, not carried. To add font files or a logo, unpack it
   into an empty folder, edit `themes.json` as below, and pack it again.

## Writing one by hand

1. Make an empty folder for the package. `themes.json` goes at its top;
   put fonts in e.g. `fonts/` and logos in e.g. `logos/`.
2. Write `themes.json` with `write_file`. Every field of a theme may be left
   out but `id`; left out, it's Flow's own colors, a block cursor, no CRT,
   no start screen, quiet. A minimal valid one:

```json
{
  "format": 1,
  "package": "night-themes",
  "version": "1.0",
  "themes": [
    {
      "id": "night-terminal",
      "name": "Night terminal",
      "names": { "ja": "夜の端末" },
      "fonts": ["VT323"],
      "text": "#7FE0C0",
      "cursor": "#7FE0C0",
      "background": "#05100C",
      "crt": "classic",
      "start": ["^{tall}NIGHT TERMINAL", "", "{memory} OK", ""],
      "baud": 1200,
      "prompt": "{cwd} $ ",
      "sounds": "terminal",
      "screen": [80, 24]
    }
  ]
}
```

What each field takes (Flow checks the ones marked "refused"):

- `format`: `1` (refused otherwise). `package`: its name for flowapp, with
  no `.`, `/` or `\`, not `bin` or `first` (refused); not a catalog app's
  (`flowapp search`), as it replaces an installed one of that name.
  `version`: not empty, no slashes, not starting with `.`.
- `id`: lowercase a-z, digits and `-`, at most 40, unique in the package,
  never `flow` or `own` (refused). A built-in machine's id (`home`,
  `workstation`, `graphics`, `green`, `amber`, `teletype`, `school`, `pc`,
  `instruments`, `amiga`) takes that machine's place: only do that on
  purpose. If two installed packages share an id, only one is worn.
- `name`, and `names` by language code (`ja`): what Settings shows.
- `text`, `cursor`, `background`: `#RRGGBB`, never a color's name (refused).
  `border`: `#RRGGBB`, or `""` for none. `phosphor`: `""` for a color
  screen, or `#RRGGBB` to show everything as shades of that one phosphor.
- `ansi`: programs' sixteen colors, exactly 16 `#RRGGBB` or `[]` for Flow's
  own (any other count is refused).
- `fonts`: family names to try first, as the font itself names its family
  (not the file name); Flow's own fonts follow. `font_files`: font files in
  the package, loaded so `fonts` can name them. Ship only fonts whose
  license lets them be passed on.
- `cursor_shape`: `block`, `underline` or `bar` (else refused). `blink`:
  true or false. `cursor_speed`: 0.5 to 4.
- `crt`: `""` for no tube, or a built-in preset: `subtle`, `classic`,
  `arcade`, `phosphor`, `television`, `monitor` (any other is refused).
- `start`: the start screen, a line each, at most 200 lines of 400 bytes
  (refused past that). `^` centers a line; `{tall}`, `{wait}`, `{clear}`,
  `{logo}`, colors and fields like `{memory}` and `{os}` work as in
  Settings' start screen. Run `flow-help settings` (Theme) for the full list
  rather than guessing. `baud`: how fast it's typed, 0 for at once, up to
  1000000 (300, 1200, 2400, 9600 suit old machines).
- `prompt`: `{cwd}` the folder, `{n}` the command's number, `\n` a new line.
  Leave it out to keep the shell's own prompt. Everything else shows as
  written in every shell: `$(...)`, `%n` or `\u` aren't expanded, so don't
  use them for a git branch or the user's name. Only color escapes (SGR)
  are kept, and a prompt past 4096 bytes is cut.
- `sounds`: `quiet`, `tube`, `terminal`, `disk`, `fan`, `floppy`, `chime`,
  `tones`, `printer` or `teletype`. Any other name is quiet, silently.
- `screen`: `[columns, rows]` that Fit window to offers first; 10 to 500
  columns, 4 to 300 rows (refused otherwise).
- `frame`: `true` draws a shell window's frame round each pane.
- `logo`: a PNG in the package shown as the startup logo; leave it out for
  the one Settings picks.
- Paths in `font_files` and `logo` are plain relative paths inside the
  package, such as `fonts/mine.ttf`: never `./fonts/mine.ttf`, `../` or an
  absolute path, and the file must be there (refused otherwise).
- `keys`: more of the look, as config.toml's sections. Only these are
  taken: `colors`, `cursor` (`shape`, `blink`, `speed`), `font.families`,
  `fx` (`style`, `water_view`, `sun`, `electricity`, `speed`, `ambient`,
  `rain`, `text_ripple`, `caustics`, `palette`, `opacity`, `glow`, `crt`)
  and `crt` (`scanlines`, `interlace`, `glow`, `decay`, `grille`,
  `vignette`, `curvature`). A theme starts on plain, opaque glass with no
  glow, as the old machines do; for a liquid add e.g.
  `"keys": { "fx": { "style": "water", "water_view": "below" } }`. A wrong
  kind of value isn't refused, only left off when worn: check what each key
  takes with `flow-settings --help` in a Flow tab.

3. Pack the folder's contents, not the folder, so `themes.json` is at the
   archive's top. Flow packs its own owned by no one, so a shared file names
   nobody's account (and on the Mac without `._` files):

```bash
# macOS
COPYFILE_DISABLE=1 /usr/bin/tar --no-mac-metadata --uid 0 --gid 0 --uname "" --gname "" -czf night-themes-1.0.tar.gz -C night-themes .
# Linux (GNU tar)
tar --owner=0 --group=0 --numeric-owner -czf night-themes-1.0.tar.gz -C night-themes .
```

   On Windows, `tar.exe` takes the Mac's owner flags, without
   `COPYFILE_DISABLE` and `--no-mac-metadata`. Install doesn't check owners:
   if those flags fail, pack without them and say the file names the account.

4. Check it, then install it. `tar -tzf` should list `./themes.json` and
   the files it names, and no `bin/` (a package holding programs is
   refused). The name must end in `.tar.gz` or `.tgz`, or flowapp looks for
   a catalog app of that name.

```bash
tar -tzf night-themes-1.0.tar.gz
flowapp install night-themes-1.0.tar.gz
```

   An error names the problem (often a theme's id first). Fix `themes.json`,
   pack again and install again: the same version installs over itself.
   `flowapp list` shows it among the packages of themes, and
   `flowapp remove night-themes` takes it off.

## Wearing it

- Installed from a tab, its themes come in once Flow is in front again: ask
  the user to switch to another app and back.
- Then choose it in Settings' **Theme** row, after the built-in themes. It
  goes on the tab Settings is on, or on new tabs with **New tabs** chosen in
  the row under Settings' title. The new-tab picker (the **…** after the tab
  bar's +) opens a tab in it. **Theme gallery** (**Browse…**) previews it.
- **Theme details** turns each part (start screen, prompt, font, CRT,
  colors, frame, sounds) off or on. `flow-help gallery` and `flow-help apps`
  say more.
