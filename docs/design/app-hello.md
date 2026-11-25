# An app introducing itself as it's installed

Asked for on 2026-11-17: once flow-say is installed, it should introduce
itself. Three ways were weighed that day (a canned clip named in the
catalog, the voice fetched at every install, flowapp playing a WAV), and
none was taken, so it stayed on the backlog. It was begun for 0.1.46 on
2026-11-23, as below: the app asks first, then speaks in its real voice
(decided 2026-11-23).

What stands in the way: flowapp runs nothing of an app's after installing
it, and flow-say's voice isn't in its download but fetched the first time
it speaks.

## What the user sees

In a tab, `flowapp install flow-say`:

```
✓ flow-say 0.1.2  16.7 MB  checked
  flow-say introduces itself:
```

What follows is the app's own: flow-say asks whether to fetch its voice,
and greets you aloud once it's there, or says how to hear it later.

From **Settings → Apps**, Install does the same in a tab of its own:
Settings closes and Flow opens a tab in its own look, typing `flowapp
hello flow-say` there once its shell is at its prompt.

## Flow's part

- **The download says what introduces it**, so the command is covered by
  Flow's signature, never by the catalog, which nothing signs: a
  `flowapp.json` at the download's root, `{"hello": ["<program>",
  "<argument>", …]}`. The first word is one of the app's programs, which
  the download's `bin/` must hold; the rest are its arguments. A
  `flowapp.json` that doesn't read refuses the install, as a
  `themes.json` that doesn't read does: it's a mistake in the download,
  which its scripts' tries should have caught.
- **`apps::hello(root, name)`** reads the installed version's
  `flowapp.json` into a `Hello` (the program's path in the version's
  `bin/`, and its arguments), none where there's none.
- **`flowapp install`** runs it after a fresh install alone (no version
  was installed), from the catalog alone (signed; a file of the user's
  own holds no programs anyway), and only where standard input and output
  are both terminals: never from a script, a pipe or the assistant. It's
  run by its path, not through the PATH (on the Mac the system's `say`
  comes first), with flowapp's own terminal, and flowapp waits. How it
  ends never fails the install. `upgrade` and `keep_carried` never run
  it.
- **`flowapp hello <name>`** runs an installed app's hello any time,
  saying so where it has none or isn't installed.
- **Settings' Apps:** where an Install (`Doing::Install`, so the app
  wasn't installed) succeeds and the app has a hello, `App::apps_done`
  closes Settings and opens a tab in Flow's look (`open_tab_in` with
  `flow`), typing `flowapp hello <name>` (quoted for its shell) once it's
  at its prompt, as the .deb's update tab types apt
  (`UpdateTab`, `tend_update_tab`). The toast says it's installed as
  before. Snapshots never do it.

## The app's part

flow-say, from its own repository, arcanii/Flow-Say, names its hello in
the `flowapp.json` its download carries; what the hello says and asks is
its own. That's flow-say 0.1.2.

## Versions

flowapp's part is flowapp 0.2.2, which can go out before Flow 0.1.46 (it
updates itself). Settings' tab is Flow 0.1.46's. A flowapp before 0.2.2
reads past `flowapp.json`, so flow-say 0.1.2 installs in any flowapp.

## Tests

- `apps::hello`: none without the file; read with its arguments; refused
  where its program isn't the app's or isn't in `bin/`, or the file
  doesn't read.
- An install refusing a `flowapp.json` that doesn't read, and keeping one
  that does.
- flowapp's choice to run it (`hello_runs`): fresh and in a terminal
  only.
- On the Mac: flow-say 0.1.2's download built ad hoc and signed locally,
  installed through `FLOWAPP_CATALOG` in a scratch HOME, in a tab and
  from Settings in a test copy.
