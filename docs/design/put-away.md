# A put-away Flow, found again

Found on 2026-11-18, in Windows 0.1.44.2: Flow started with a toast,
"Outside apps can't connect: another Flow already lets outside apps
connect, or the pipe's name is taken". Two Flows were running from
Program Files.

- The update's setup had started Flow at 9:05.
- Ctrl+Space then put that Flow away. Its window was hidden at the
  drop-down's place, with no taskbar button.
- At 9:11 Flow was opened from Explorer, which started a second Flow.
  The first still held the outside-apps pipe and the hotkey.

A put-away Flow showed nowhere in the taskbar, so two things were
designed:

1. Starting Flow again brings the running one forward instead of opening a
   second.
2. While Flow is put away, an icon in the notification area shows it's
   there and brings it back.

Both are for Windows. As decided the same day, the icon shows whenever
Flow is put away, with no setting, and the rest is as drafted. They go
out as **0.1.44.3**, on Windows alone, since nothing else changes.

## Why it happens

Putting away hides the window outright (`Platform::put_away`, by default
`set_visible(false)`, Windows' `SW_HIDE`), so the drop-down is out of the
taskbar and Alt+Tab, as a console's should be. The taskbar lists only
visible windows, so a put-away Flow is seen only in the process list, and
the hotkey is the one way back to it. Starting Flow from Start, the
taskbar's pin or Explorer opens a second Flow. The second can't take the
pipe (`FILE_FLAG_FIRST_PIPE_INSTANCE`, `windows/src/mcp.rs`) or the
hotkey, so it toasts that it can't, the pipe's toast covering the hotkey's. Both
Flows' assistants then talk to the first through the `flow` MCP server.
`run` also truncates `flow.log` at the second start, which loses the
first Flow's log. Worse, each Flow holds the whole config in memory and
writes its whole copy whenever it saves. So whichever saves last undoes
the other's changes: the hotkey had been made Ctrl+\`, but after both
Flows were ended, config.toml was written at 9:53 with the old
Ctrl+Space. Once Flow started again, Ctrl+\` did nothing until it was set
again (2026-11-18).

The Mac hasn't this gap. Launch Services hands a second start to the
running app, and a click on the Dock icon unhides a hidden app. Linux has
the same gap as Windows. It's left for later: see "Left out".

## 1. Starting Flow again

**The same Flow** is the same program, started with the same config
folder. Its id is a hash of `current_exe()` and `config::config_dir()`,
each lowercased, since Windows' paths don't heed case. So a `cargo run`
build, a portable copy in another folder, and a test copy with a scratch
`APPDATA` each still start on their own, as today.

**Which starts take part:** a plain start of Flow. These are left out:

- a guest (`flow --window`, or `flow` with no tab to ask: `read_guest`);
- `--snapshot` and `--record`, which run beside the user's Flow;
- `--carry-on`, the Flow opened as administrator for a task, which runs
  beside the one that opened it;
- an elevated Flow at all (`Platform::elevated`). It neither hands over
  nor takes starts, so a start from Start goes to the user's own Flow,
  never the administrator's window. Its objects would refuse a plain
  process anyway.

`--version` returns before any of it, as now.

**How (Windows), first thing in `flow_core::run`, before the logger:**

- A new `Platform::hand_to_running()` gives a `Start`:
  - `First`: this Flow runs, and claims the name.
  - `Handed`: the running one came forward, and this one quits with 0.
  - `Beside`: it didn't answer, and this one starts as today.

  The default is `First`, so the Mac and Linux are unchanged. `run` parses
  its arguments before the logger, to know whether the start takes part.
- Windows creates the mutex `Local\Flow.Instance.<id>`, beside
  `Flow.Running`, which stays for the installer's AppMutex. Where it
  already exists (`ERROR_ALREADY_EXISTS`), another start of this Flow
  runs. This start then:
  1. calls `AllowSetForegroundWindow(ASFW_ANY)`, since the user's click
     gave it the right to come forward and the running Flow needs it;
  2. sets the auto-reset event `Local\Flow.Again.<id>`;
  3. waits up to `AGAIN_WAIT` (10 s, so a Flow still starting has time)
     for the event `Local\Flow.Forward.<id>`. If it comes, the start is
     `Handed`; if not, it's `Beside`.
- The first Flow creates both events with its mutex, before the logger.
  A start arriving while it's still starting therefore leaves the event
  set until the first Flow gets to it.
- A `Beside` start logs to `flow-window.log`, as a guest does, so the
  running Flow's `flow.log` is kept. It shows the pipe's toast as today,
  which is then true of a Flow that didn't answer.

**The running Flow:**

- A `flow-again` thread, started from `App::new` for a Flow that took
  part, waits on `Flow.Again`. For each start it sends
  `UserEvent::StartedAgain` through the waker.
- `App::come_forward` does what the hotkey does for a Flow not in
  front, without ever putting it away (`on_hotkey`'s second half):
  - It drops down, sliding where Slide says, where the window was
    dropped down or the hotkey would drop it: with Drop down on and a
    hotkey held. Drop down is on at first, so without the hotkey a
    window that was never dropped just comes forward where it was.
  - It comes forward and gets the keys (`Platform::bring_forward`, which
    also shows and unminimizes it).
  - A window in front already just keeps the keys.
  - Settings, a card or a dialog open in it stays as it was.
- Then `Platform::came_forward()` sets `Flow.Forward`. It's set from the
  window's thread, after the window has come forward, so a Flow whose
  window has hung doesn't answer, and the new start opens beside it.

**Edges:**

- **Two starts at once:** each sets the event; the running Flow comes
  forward for each and answers each. A start that misses its answer
  starts beside, as a hung Flow would make it.
- **An update:** the setup waits for `Flow.Running` to go before
  installing, so the old Flow is gone before the new one starts.
- **Restart Flow** and **Restart Tab** happen within the one process.
- **flow-mcp** starts Flow only where none runs. With outside apps off in
  a running Flow, it says so rather than starting another.
- **Another program of the user's** can open these objects, as it could
  anything of the user's. Setting the event only brings Flow forward.
  Holding the mutex makes each start wait `AGAIN_WAIT`, then start beside
  as today.

## 2. An icon while put away

- **While put away**, Flow's icon sits in the notification area. It's
  added when the window is put away (the hotkey's, Put away when
  unfocused, or the frame's send-to-back while dropped down, all
  through `Platform::put_away`), and taken away as soon as the window
  shows again (`Platform::bring_forward`) and as Flow quits.
- **Its tooltip:** "Flow is put away. Ctrl+Space or a click brings it
  back", with the hotkey as Settings writes it.
- **Its left click** (or Enter or Space on it from the keyboard,
  `NIN_SELECT` and `NIN_KEYSELECT`) brings Flow back as a second start
  does (`come_forward`).
- **Its right click** opens a menu: **Show Flow** (the default, in bold),
  then **Quit Flow**, which closes the window as its ✕ does, keeping the
  tabs in `tabs.json` as any quit does.
- **How:** a `flow-tray` thread with a hidden top-level window of its own.
  - It isn't message-only, since a message-only window never hears
    `TaskbarCreated`, which re-adds the icon after Explorer restarts.
  - It uses `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4`, the icon
    identified by that window and an id rather than a GUID, since a GUID
    ties the icon to one exe path.
  - The icon is flow.exe's own (resource 1) at the notification area's
    size (`LoadIconMetric`).
  - Its clicks come to the window as `UserEvent::AwayIconShow` and `AwayIconQuit` through the waker
    `install_menus` keeps, as a notification's click does.
  - `NIM_DELETE` runs from Windows' `Platform::exiting`, which
    `App::exiting` calls. Windows otherwise leaves a dead process's icon
    until the pointer passes over it.
- **Core's side:** `Platform::away_icon(Some(tip))` follows each
  `Platform::put_away`, and `away_icon(None)` each `bring_forward`
  (`App::bring_forward`), the window getting the keys, and quitting. The
  waker the icon's clicks go through is the one `install_menus` is
  given.
- Windows 11 puts a new program's icon in the overflow (the ^) until the
  user pins it in the taskbar's settings. The tooltip and the second start
  still work there.
- **No setting:** it always shows while Flow is put away, as decided.

## Words

New keys go in `en.json` and `ja.json`. Drafts:

- `tray.tip`: "Flow is put away. {key} or a click brings it back" / "Flow
  はしまってあります。{key} かクリックで戻ります"
- `tray.tip_no_key` (no hotkey, Put away from the frame or a setting
  changed): "Flow is put away. A click brings it back" / "Flow
  はしまってあります。クリックで戻ります"
- `tray.show`: "Show Flow" / "Flow を表示"
- `tray.quit`: "Quit Flow" / "Flow を終了"

## Docs

- **The docs:** the hotkey and drop-down in [Settings](../manual/settings.md), for a
  second start and the icon, and [Status](../status.md), for what was tried on Windows.
- **CLAUDE.md:** Notifications and the hotkey, for the start and the icon.

## Tests

- **Pure, in core:**
  - the instance id from two paths: case, the same, another config
    folder, another program;
  - which starts take part, from the arguments and whether elevated.
- **Windows (`windows/tests` or the crate's own):** the handshake under a
  scratch id between two threads, each as a process would be:
  - one claims, the next hands over, and its answer comes;
  - a claim with no one answering is `Beside` within the wait.

  Also the icon's add and delete calls succeeding, with no window shown.
- **Live,** in a portable test copy with a scratch `APPDATA`
  and its own `FLOW_MCP_SOCKET`, never the installed Flow:
  1. Drop it down and put it away; start it again from its folder. It
     drops down with the keys, with no second process.
  2. With Drop down off, the hotkey hides it, and a start brings it back
     where it was.
  3. While put away, the icon shows with its tooltip, and a click brings
     the window back.
  4. The menu's Show and Quit work, and Quit keeps the tabs.
  5. Restart Explorer while put away; the icon comes back.
  6. Quit while put away; no icon is left.
  7. A copy in another folder still starts beside.
  8. A `--snapshot` run beside it still runs.

## Left out

- **Linux** has the same gap. A launcher's start would need the same
  handover, through a socket in the config folder as the bridge's, and a
  tray icon through StatusNotifierItem over D-Bus. Later, with its own
  tries on X11, where Flow has its hotkey.
- **One Flow across copies:** another build or copy still starts beside,
  with the pipe's toast.
- **Minimizing in place of hiding,** which would keep a taskbar button but
  show the drop-down in Alt+Tab and the taskbar, against what it's for.
- **An icon all the time Flow runs.** It shows only while put away.

## Release

0.1.44.3 is `patch = 3` on the 0.1.44 that's out, with `dev` staying
false. It's Windows' alone: the Mac's and Linux's code is untouched, and
Linux's 0.1.44.2 is built from the commit before the version goes to
0.1.44.3.
