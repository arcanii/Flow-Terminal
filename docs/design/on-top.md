# Always on top, and e-mode

Asked for on 2026-11-17: a mode that keeps Flow's window in front of
every other window, turned on and off from a pill in the title bar, and a
shorter word than "Efficient" on its pill to make the room. What was
decided the same day settles it. It goes out in **0.1.44.1** on every
system, with the notanAmiga frame's gadgets on the Mac.

## e-mode

- The Efficient pill always reads **🌱e-mode** (always, rather than only
  where the title bar ran out of room, as first decided). On the battery
  or in Low Power Mode it's **🌱e-mode · battery** or **· low power**
  where the tabs keep their room, as Efficient's was.
- The seedling is drawn in its own colors, green whether e-mode is on or
  off; the word after it is lit or grey as the pill is. It takes two
  cells and draws in about one, so no space follows it, which left a gap
  as wide as a letter and a half (seen in a snapshot).
- Only the pill changes. Settings' row, the Mac's View menu's Efficient
  Mode, the palette, the toasts, help, readers' words and the config keys
  (`ui.efficient`, `ui.efficient_on_battery`) keep "Efficient". The pill's
  words are `title.efficient`, `efficient_battery` and
  `efficient_low_power`; ja's are 🌱効率 and the like.

## On top

- **What it does:** Flow's window stays in front of every other window,
  through winit's `set_window_level(WindowLevel::AlwaysOnTop)`: the Mac's
  floating level, Windows' `HWND_TOPMOST`, X11's `_NET_WM_STATE_ABOVE`
  (which the window manager may honor). Off, `WindowLevel::Normal`.
- **Wayland** has no protocol for it, and winit does nothing there. GNOME's
  and KDE's window menus have their own Always on Top, which a right click
  on Flow's title bar opens. So on Wayland there's no pill, and Settings'
  row says the window manager's menu does it, as the Hotkey's row says
  there's none (`Platform::on_top_possible`, Linux's from
  `hotkey::ON_WAYLAND`).
- **Kept:** `window.on_top` in the config, off at first, so it comes back at
  the next start. Settings' Window section has its switch, **Always on
  top**.
- **Turned on and off from:**
  - the pill, **on-top**, between the gear and e-mode: always shown, unlit
    while off and lit while on (`Hit::ToggleOnTop`). It never gives way,
    being the quickest way to turn it off: the tabs give up the room
    instead.
  - `Command::ToggleOnTop` in `keys::TABLE` (`Group::Flow`, no key at
    first, which `[keys]` can give it), so the palette lists it and Settings'
    Keys can bind it.
  - the Mac's Window menu, **Float on Top**, checked while on
    (`MenuState`).
  - Settings' row, and `flow-settings` (`window.on_top`, among the
    window's keys open to programs).
- **With the drop-down:** the drop-down follows on-top (decided
  2026-11-18, rather than the pill lighting while it floats). As built
  for 0.1.44.1 it floated by itself (`set_dropped_down` set the Mac's
  floating level, and `HWND_TOPMOST` and X11's above state elsewhere)
  with the pill unlit, which was found on the Mac and on Windows. Now
  `set_dropped_down` never touches the level: the hotkey brings the
  window forward, and `follow_on_top` keeps it on top, dropped down or
  not, only while `window.on_top` is on, so the pill always tells the
  truth. On the Mac, Over full-screen apps still makes Flow an accessory
  app with the window `FullScreenAuxiliary`, at the normal level unless
  on-top is on: to try, since it floated when that was built.
- **Readers** get the pill as a toggle button, as e-mode's.
- **Words:** en "on-top", "Always on top", "Float on Top"; ja 最前面 (as
  Windows' 常に最前面).

## Pictures

Every picture's title bar changes (the pill, and e-mode's word). The
pictures the Mac made are made again on the Mac for 0.1.44.1; those
Windows made (CLAUDE.md's list) on Windows when it has the release.

## Left out

- A key of its own at first (none, as the user may pick one in Keys).
- The Mac's floating level keeps to its own Space and doesn't follow into
  another app's full screen, which only the drop-down's accessory app does.
- Flow's own panels (`rfd`: a recording's save panel, Export…, preset files)
  open while the window floats: on the Mac they're sheets on it; on Windows
  and Linux one could open behind it. To try on each.
