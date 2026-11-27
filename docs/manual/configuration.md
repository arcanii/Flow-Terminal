[← Flow's manual](../../README.md#documentation)

# Configuration

`%APPDATA%\flow-terminal\config.toml` (Windows),
`~/Library/Application Support/flow-terminal/config.toml` (macOS) or
`~/.config/flow-terminal/config.toml` (Linux, under `$XDG_CONFIG_HOME` if
set), written on first run. On Windows Flow goes by `APPDATA` in its
environment (from 0.1.43), so a Flow started with another, a test copy's
say, keeps everything of its own in that folder and leaves yours alone.
Token usage is kept beside it in `usage.json`,
and the assistant's [memory](assistant.md#memory) in `assistant.sqlite3` and
`assistant-memory.txt`, the files it changed, kept for Undo, in
`assistant-undo`, and your [instructions and
skills](assistant.md#your-instructions-and-skills) in `assistant-instructions.txt` and
`assistant-skills`.
On Linux, whether Flow checks for updates daily, and when it last did, are in
`updates.json`, and the tabs open when Flow quit, for the next start, are
in `tabs.json`, since Flow writes those by itself and never writes
`config.toml` unasked. Release builds write their log to `flow.log` in the
same folder.

Notable settings:

- **Assistant:**
  - `ai.provider` (`anthropic`, `openai`, `gemini`; default `anthropic`)
  - each provider's model: `ai.model` for Claude (default `claude-opus-5-5`),
    `ai.openai_model` (default `gpt-6.1-sol`), `ai.gemini_model` (default
    `gemini-4-argon`)
  - `ai.effort` (`low` … `max`, default `medium`; Gemini's top level is `high`)
  - `ai.auto_mode`, `ai.web_search` (Claude and GPT), `ai.token_cap` (0 = no cap)
  - `ai.memory` (the assistant's [memory](assistant.md#memory), on at first)
  - `ai.pane` (`yours` or `own`), `ai.helpers`, `ai.helper_model` ([helpers](assistant.md#helpers))
  - `ai.own`, `ai.own_minutes`, `ai.own_tokens` ([working on its own](assistant.md#working-on-its-own): on, 60, 500000 at first)
  - `ai.skills_on` (the installed packages' [skills](assistant.md#your-instructions-and-skills) turned on, each `<package>:<name>`, such as `flow-skills:flowapp`; none at first),
    `ai.skills_off` (your own skills switched off, by name)
  - `ai.mcp_servers_on` (the [MCP servers](assistant.md#mcp-servers) of `mcp-servers.json` connected, by name; none at first),
    `ai.mcp_tools_on` (their tools turned on, each `<server>:<tool>`; none at first),
    `ai.mcp_approval` (a table of each server's approval: `ask`, `read_only` or `allowed`; `ask` where absent),
    `ai.mcp_resources_on` (the servers whose resources the assistant may read, by name; none at first)
  - `ai.fallbacks` (Claude), `ai.command_timeout_secs`
  - each provider's endpoint: `ai.base_url` for Claude, `ai.openai_base_url`
    (any server with OpenAI's Responses API), `ai.gemini_base_url`; or
    `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL`, `GEMINI_BASE_URL` in the environment
- **Outside apps:** `mcp.enabled` (whether [outside apps](outside-apps.md) may connect through `flow-mcp`; off at first). What each may do is kept in `mcp.json` beside it.
- **Liquid:**
  - `fx.style` (`clouds`, `water`, `oil`, `rain`, `none`), `fx.water_view` (`above`, `below`)
  - `fx.sun`, `fx.electricity`, `fx.liquid` (false freezes the liquid),
    `fx.reactions` (rings, splashes, the progress tank, tab glows),
    `fx.program_effects` (the effects programs ask for with OSC 7731)
  - `fx.speed` (1 = normal), `fx.ambient` (idle flow), `fx.rain` (how hard
    it rains, 0 to 1: a drizzle to 0.3, then rain, heavy rain, and from 0.8
    a downpour; 0.15 at first), `fx.caustics` (light ribbons)
- **Glass:**
  - `fx.text_ripple` (how far text rides the flow, px), `fx.opacity`
    (Glass opacity, 0.2 to 1: 1, opaque, at first, the liquid still filling
    the window, and lower lets the desktop show through; from 0.1.36, whose
    first start takes a config at Flow's old 0.55 to 1), `fx.glow` (0 to 2,
    1 = normal), `fx.cursor_glide` (false steps the cursor
    along as you type), `cursor.speed` (how fast it glides, 0.5 to 4,
    1 = normal), `fx.crt` (the CRT's strength, 0 to 1, 0 = off)
- **CRT:**
  - `crt.scanlines`, `crt.grille`, `crt.vignette`, `crt.curvature` (0 to 1),
    `crt.glow` (0 to 2), `crt.decay` (seconds, 0 to 0.3), `crt.interlace`
  - `[[crt.presets]]`: the presets saved under a name, each with `name`,
    `strength` and the parts above
  - `fx.backdrop` (`acrylic`, `mica`, `blur`, `none`; macOS and Linux have `blur`
    and `none`, and on Linux only some compositors blur, as KDE Plasma's does), `fx.palette` (`aurora`, `ocean`, `ember`, `mono`)
- **Terminal:**
  - `ui.language` (`auto`, `en`, `ja`; default `auto`) and `ui.logo` (the startup
    logo: `flow`, `system`, `chip`, `off`; default `flow`)
  - `ui.look`, the **Theme** (`flow`, `home`, `workstation`, `graphics`,
    `green`, `amber`, `teletype`, `school`, `pc`, `instruments`, `amiga`,
    `own`, or
    `saved:` and a saved theme's name; default `flow`), with
    `ui.look_start`, `ui.look_prompt`, `ui.look_font`, `ui.look_crt`,
    `ui.look_colors` and `ui.look_frame` (default true; an old machine's
    switches in Theme details, the first two of which your own theme
    doesn't use, the last the Flow notanAmiga CLI's alone), and
    `theme.start`,
    `theme.baud`, `theme.prompt`, `theme.sounds` and `theme.frame` (default
    false: the Flow notanAmiga CLI's window frame round each pane) for your
    own, which Settings sets too (see [Themes](themes.md))
  - `[[themes.saved]]`: the themes saved under a name (**Save as theme…**),
    each a `name` and the look's sections as a preset file holds them, with
    what a theme sets: `[themes.saved.colors]` (all of it),
    `[themes.saved.cursor]` (`shape`, `blink`), `[themes.saved.font]`
    (`families`), `[themes.saved.fx]` (`style`, `water_view`, `sun`,
    `electricity`, `speed`, `ambient`, `text_ripple`, `caustics`, `palette`,
    `opacity`, `glow`, `crt`), `[themes.saved.crt]` (its parts, not its
    presets) and `[themes.saved.theme]` (its own `start`, `baud`, `prompt`,
    `sounds` and `frame`, which it brings in place of `[theme]`'s without
    changing them). Wearing one changes only what it holds, so one written by hand
    can hold a few; the text's size, the liquid's switches, the cursor's
    speed and the backdrop stay yours
  - `ui.closed_sections`, Settings' sections shown closed (`general`, `theme`,
    `theme_details`, `liquid`, `glass`, `text`, `crt`, `assistant`, `skills`,
    `outside_apps`, `mcp_servers`, `terminal`, `keys`, `recording`, `apps`,
    `sound`, `about`; default `theme_details`, `crt` and `sound`)
  - `ui.efficient` (default false): [Efficient mode](settings.md)
  - `ui.efficient_on_battery` (default false): efficient mode while the
    computer runs on battery or saves power, without changing `ui.efficient`
    ([Settings](settings.md))
  - `window.width` and `window.height` (default 1100 × 700 points): the size
    Flow opens at, which Settings' **Fit window to** doesn't change
  - `window.sizes` (default empty): screens of your own for **Fit window to**
    and **Fit text to**, as `132x43, 40x24` (Settings' **Your sizes**)
  - `window.refresh_rate` (the most frames a second; 0, the default, keeps to
    the display's rate) and `window.vsync` (false shows each frame without
    waiting for the display: **Unlimited** with a rate of 0)
  - `window.hotkey` (default empty, none): the key taken system-wide that
    brings Flow forward and puts it away, as modifiers and a key joined by
    `+` (`` ctrl+` ``, `alt+space`, `f12` but on Windows, `ctrl+space`, which Settings offers,
    or by hand `ctrl`, `alt`, `shift` and `cmd` (the Windows key, Super)
    with a letter, a digit, `space`, `` ` `` or F1 to F12; **Press a key…**
    writes them so too), and `window.drop_down` (default true): it drops
    the window over the top of the screen, as much of it as
    `window.drop_down_height` says (`third`, `half`, the default,
    `two_thirds` or `all`), sliding down and back up with
    `window.drop_down_slide` (default true), put away when Flow loses the
    keys with `window.drop_down_hides` (default false), and on the Mac over
    full-screen apps with `window.drop_down_over_full_screen` (default
    false) ([Settings](settings.md))
  - `window.on_top` (default false): the window stays in front of every
    other (Settings' **Always on top**, the title bar's **on-top**; not on
    Wayland)
  - `ui.notify_after` (seconds, default 30; 0 is off): a command that ran
    this long and finishes while Flow isn't in front tells you so through
    the system ([The glass reacts](liquid-styles.md#the-glass-reacts))
  - `font.families`: a list where the first installed font wins and the rest
    are fallbacks. Picking a **Font** in Settings moves it to the front.
  - `font.size`, `font.line_height`, `shell.default`, `shell.scrollback`,
    `shell.option_as_meta` (macOS), `shell.mouse_reporting` (default true),
    `shell.click_moves_cursor` (default true: a click on the line typed at
    the prompt moves the cursor there, [Using it](keys.md)),
    `shell.reopen_tabs` (default true: a start opens the tabs open at quit),
    `shell.scrollbar` (`scrolling`, `always`, `never`; default `scrolling`:
    [the scrollbar](terminal.md#scrolling-back))
  - `[keys]`: Flow's keys where they differ from its own, each as its name
    and its key (`new_tab = "cmd+n"`, `record = "none"`): see [Changing
    Flow's keys](keys.md#changing-flows-keys)
  - `history.enabled` (default true): keep the commands run, for the
    [command history](terminal.md#command-history)
  - `record.quality` (`smaller`, `balanced`, `sharp`, `lossless`; default
    `balanced`), `record.size` (`points`, `pixels`; default `points`) and
    `record.countdown` (seconds before it starts: 0, 3, 5 or 10; default
    3): how [a recording of the terminal](terminal.md#recording-the-terminal) is kept
  - `apps.check_at_start` (default false) and `apps.check` (`off`,
    `daily`, `weekly`; default `off`): when Flow reads flowapp's catalog to
    say an app has a newer version ([Apps: flowapp](flowapp.md)). The
    version of Flow that last started is kept in `updates.json` beside
    `config.toml` (`ran`), for the reminder after Flow updates
  - `sound.enabled` (default false), `sound.volume` (0.05 to 1; default 0.6),
    `sound.bell` (`system`, `flow`, `off`; default `system`), and
    `sound.cues`, `sound.effects`, `sound.programs`, `sound.liquid` and
    `sound.machines` (default true), `sound.bell_flash` (default false), and
    `sound.muted`, the machines' sounds Theme details turned off, each as its
    theme and its sound (`pc.fan`, `teletype.punch`): see [Sound](sound.md)
  - `colors.text`, `colors.cursor` and `colors.background` (`#RRGGBB`; defaults
    `#E4E9F2`, `#8FE3FF` and `#080C16`), `colors.border` (empty, or a
    color for a wide margin round the text, as the Home computer theme has),
    and `colors.ansi`, programs' sixteen colors (black, red, green, yellow,
    blue, magenta, cyan, white, then the bright ones), as an old machine's
    theme sets them; empty for Flow's own; and `colors.phosphor`, the one
    color everything is shown in (`#RRGGBB`, the green and amber screens'
    own), empty for full color
  - `cursor.shape` (`block`, `underline`, `bar`; default `block`) and
    `cursor.blink` (default false)
