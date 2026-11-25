---
name: flow-settings
description: See and change Flow's own settings from a shell or a script with flow-settings, or with the flow_settings tool: themes, colors, fonts, the liquid, the CRT, sound, the window. Use it to script a look (a theme per project, a green screen for a demo) or to read a setting.
---

# Flow's settings from the shell

`flow-settings` asks the Flow tab it runs in, by the names `config.toml` gives the settings (`fx.style`, `font.size`, `ui.look`). It only works inside a Flow tab, from Flow 0.1.30 on. It sends the tab's own key (`FLOW_TAB_KEY`, which only programs started in that tab have) with each request, and from 0.1.46 Flow doesn't answer a request without it at all, not even to refuse. So it doesn't work through `sudo`, which leaves the variable out, `env -i`, or over ssh: with no key it asks nothing and says so at once, and where its key goes unanswered (another pane's, one carried into ssh or a script, outside Flow, or through tmux) it says Flow didn't answer and that the key may be wrong. Both exit 2. Run it in the user's own tab rather than through `sudo`.

## Quick reference

```bash
flow-settings                     # every setting a program may see, with its value (also: list)
flow-settings get fx.style        # one value, bare, so $(…) can keep it
flow-settings set fx.style rain   # change one; prints "key = value"
flow-settings reset font.size     # back to its default
flow-settings reset all           # every open setting back to its default: ask the user first
flow-settings reset terminal      # this tab's screen, scrollback and modes start afresh
flow-settings restart             # restarts Flow, as Settings' Restart does
flow-settings --help              # usage, and what each setting takes, as Flow says
```

- Values are read as JSON where they can be (`16`, `true`, `'["VT323", "Menlo"]'`), else as text (`rain`). Quote colors, since a shell may read `#` as a comment: `"#00ff00"`.
- Exit codes: 0 done, 1 Flow refused (its reason on stderr), 2 no tab key, no answer or bad usage.
- Flow applies each change at once, saves it and shows a toast naming the tab and the key. So nothing changes unseen, and a script that sets on every prompt makes a toast every prompt. Compare with `get` first.
- `restart` restarts at once only while every other pane waits at its prompt (the other panes of its own tab too); otherwise Flow doesn't restart, exits 1 with the reason, and tells the user in a toast, who can restart from Settings if they want. Asking again changes nothing: only the user confirms. It closes every tab, this one included, ending what runs there, then opens them again. Make it the last thing a script does.

## What takes what

Never guess a value. `flow-settings --help` asks Flow (a `describe` request) and lists each setting's choices or range, so it's never out of date: `fx.style` takes clouds, water, oil, rain or none; `fx.rain` a number from 0 to 1. Colors are `#RRGGBB`; `colors.border` and `colors.phosphor` also take `""` for none. A value of the wrong kind, one Settings doesn't offer, or an unknown key is refused with the reason. For what a setting means, read `flow-help settings` (or the `flow_help` tool's `settings` page).

`ui.look` is the theme. Built-in ids include `flow` (Flow's own), `green`, `amber`, `home`, `teletype`, `pc`, `amiga`; installed theme packages add more. Take the list from `flow-settings --help`.

## What may and may not change

Open to programs: the look (`fx.*` the liquid and glass, `crt.*`, `colors.*`, `cursor.*`, `font.*`, `ui.look` and the Theme details switches `ui.look_start`, `ui.look_prompt`, `ui.look_font`, `ui.look_crt`, `ui.look_colors`, `ui.look_frame`, and `ui.logo`), `sound.*`, `window.*`, `shell.*`, `record.*`, and `ui.efficient`, `ui.efficient_on_battery`, `ui.language`, `ui.notify_after`.

Never open, only Settings may change them:
- The assistant's settings (`ai.*`: provider, models, token cap, auto mode) and the history's.
- The user's own theme (`[theme]`: its start screen, prompt and sounds) and the themes saved under a name (`themes`). Every shell shows their prompts before each command the user types, so a program could put its own words there. A saved theme can't even be seen or chosen as `ui.look`; `get ui.look` fails while one is worn.
- The computer's real numbers (`ui.look_real_numbers`), keys, apps and Settings' own state.

If Flow says a key is closed, tell the user to change it in Settings. Don't edit `config.toml` behind Flow's back to get around it.

## The per-tab rule

The look's settings change the tab the command runs in (on a split tab, that pane), as Settings opened on that tab would, and Flow keeps them when it opens the tab again. Setting `ui.look` puts the theme on whole, with its colors, fonts and CRT, so set any color or font after the theme. Everything else (sound, window, shell, recording, efficient mode, language, notifications) changes Flow for every tab.

## Recipes

A theme per project folder, in `~/.zshrc` or `~/.bashrc`. It checks before setting, so a `cd` inside the project makes no toast, and puts back what the tab wore on the way out:

```bash
flow_project_theme() {
  [ -n "$FLOW_LANGUAGE" ] && command -v flow-settings >/dev/null || return 0   # Flow's tabs only
  local want= now
  case "$PWD/" in
    */mainframe/*) want=green ;;
    */arcade/*)    want=amber ;;
  esac
  if [ -n "$want" ]; then
    now=$(flow-settings get ui.look 2>/dev/null) || return 0   # a saved theme is worn: leave it
    [ "$now" = "$want" ] && return 0
    : "${flow_look_before:=$now}"
    flow-settings set ui.look "$want" >/dev/null
  elif [ -n "$flow_look_before" ]; then
    flow-settings set ui.look "$flow_look_before" >/dev/null
    flow_look_before=
  fi
}
# zsh:  autoload -Uz add-zsh-hook && add-zsh-hook chpwd flow_project_theme
# bash: cd() { builtin cd "$@" && flow_project_theme; }
```

A green screen for a demo, put back however the script ends:

```bash
look=$(flow-settings get ui.look) || exit 1
size=$(flow-settings get font.size)
trap 'flow-settings set ui.look "$look" >/dev/null; flow-settings set font.size "$size" >/dev/null' EXIT
flow-settings set ui.look green
flow-settings set font.size 22
./demo.sh
```

Rain for a moment, then Flow's whole window quieter (efficient mode is every tab's):

```bash
flow-settings set fx.style rain && flow-settings set fx.rain 0.8
flow-settings set ui.efficient true
```

## From the assistant

The `flow_settings` tool makes the same requests: `do` is `list`, `describe`, `get`, `set` or `reset`, with `key` (dotted, `all` to reset every one) and, for `set`, a JSON `value` (`18`, `"rain"`, `"#00ff00"`). It acts on the pane in front of the tab in front, and in guide mode `set` and `reset` wait for the user's yes. Prefer it to `run_command` for a look: a command may run in a pane of the assistant's own, and a look set there changes that pane, not the user's. The tool can't restart Flow or reset a terminal. For those, ask the user first, then run `flow-settings restart` or `flow-settings reset terminal` in their tab. When unsure what a key takes, call `describe` first.
