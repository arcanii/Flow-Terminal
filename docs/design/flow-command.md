# The `flow` command (0.1.43)

Planned on Windows on 2026-11-16, and built on the Mac the same day (see
As built, at the end), then tried on Linux; Windows is to build and try
it.
BACKLOG's 0.1.43 and "Starting Flow from a tab" hold the ask; this is the
design as agreed, kept up to date as it's built.

## Context

`flow ssh -N -L 7445:127.0.0.1:7445 …`, typed in a Windows tab to keep a
tunnel open apart from that tab, got PowerShell's "The term 'flow' is not
recognized". Flow's programs folder is
on every tab's PATH (`games\`, `Contents/Helpers`), but Flow itself isn't
in it, and Flow reads no command from its arguments.

What was decided:
- **A new tab, not a new Flow:** `flow <command>` asks the Flow the tab
  belongs to, and a new Flow comes only with `--window`.
- **Like the asking tab:** its shell, theme and folder, so the command
  runs in the shell it was typed for.
- **All four options for 0.1.43:** `--split`, `--behind`, `--close`, and
  `--shell` / `--theme`.

## What it does

```
flow [options] [--] <command> [args…]   a new tab that runs the command
flow [options]                          a new tab here, at its prompt
flow --window [options] [--] <command> [args…]
flow --help | --version
```

- `--split`: a pane beside the asking one, as ⌘D (Ctrl+Shift+D) opens,
  in the asking pane's tab, even while another tab is in front.
- `--behind`: the tab (or pane) opens without coming to the front, and
  the keys stay where they were. Without it the new one comes forward,
  since ssh may ask for a password there.
- `--close`: the tab closes by itself once its command ends well (below).
- `--shell <name>`, `--theme <name>`: another shell or theme for it, by
  id or label as MCP's `open_tab` takes them (`App::profiles`;
  `looks::ids`, `looks::name`), an unknown one an error that lists them.
- `--window`: a new Flow with that one tab (see A new Flow).
- The first word that isn't an option begins the command, and `--` ends
  the options (so `flow -- --odd-name`). An option `flow` doesn't know is
  an error, before anything opens.

`flow` prints nothing when the tab opens and returns at once, before the
command runs: it can't know when the command ends or how. On an error it
says why on stderr and exits 1; where no Flow answered, 2, as
`flow-settings` does, saying the key it sent may be another pane's
(below).

## The new tab

- **Like the asking pane:** its shell profile (`Pane::profile`), the
  theme it wears (`Tab::look_of`: its look id and its own whole look),
  and the folder `flow` was run in (its current directory, which in Git
  Bash and BusyBox is a Windows path, as `flow.exe` sees it), unless
  `--shell` or `--theme` say otherwise. It opens right after the asking
  tab in the bar, not at the end.
- **The command is typed** once the shell waits at its prompt, as an
  update's is (`tend_update_tab`: `Session::at_prompt`, else after
  `UPDATE_TAB_WAIT`), through `Session::run_typed`. So it goes into the
  history as the user's, the shell stays when it ends, and its output is
  the tab's as if typed. A theme's start screen is typed first as in any
  new tab. One that waits for a key (`{wait}`) goes on by itself, since
  the command stands in for the key.
- **Quoted for the new tab's shell** (`shell::quote`, new): each word as
  `flow` got it, after the asking shell expanded its variables, globs and
  quotes. A word of plain characters only goes as it is, so `ssh -N -L
  7445:127.0.0.1:7445 user@host` is typed as written. Any other word is
  quoted so that the new shell expands nothing again. As built:
  - zsh, bash and BusyBox (`posix_word`): plain is letters, digits and
    `-_./:@,+%=`, but not a word that begins with `=` (zsh's `=cmd`),
    nor a first word with a `=` (an assignment). A `~` anywhere is
    quoted: bash expands one after a `=` or a `:` in any word that looks
    like an assignment (`--prefix=~/x`), not only at a word's start.
    Else single quotes, a `'` written as `'\''`. Letters past ASCII go
    as letters, which Flow's bash hook has readline take as letters
    before each prompt, whatever the locale (`builtin bind 'set
    convert-meta off' …`). Where the new shell hasn't shown the prompt
    Flow's hook draws when the line is typed (`FLOW_TAB_WAIT` passed: its
    startup files `exec`ed another shell, which the hook never reached),
    or bash's hook said its line editor may take them as keys there (the
    first prompt after a change into a locale that isn't UTF-8, which
    readline 8.1 and later put back over the bind, the hook having
    brought the environment a readline apart from bash reads it from up
    to date with `: "$(:)"` before each prompt), or can't run before its
    prompts (a read-only `PROMPT_COMMAND`: no "done" with the nonce has
    come, `ShellState::letters_may_be_keys`), `tend_flow_tabs`
    types `Opening::typed_ascii` instead (`shell::quote_ascii_for`):
    into zsh and bash, each run of letters past ASCII as its bytes,
    `$'\xc4\x92'`, beside the single quotes of the rest, which no line
    editor takes as keys and which reads back as the word.
  - PowerShell (`powershell_word`): plain is letters, digits and
    `-_./:\=+@`, but not a word that begins with `@` (a splat), nor a
    `-` word with a `.` or `:` in it, which PowerShell may split; `,`
    (an array) and `~` aren't plain. Else single quotes, each of
    PowerShell's four single quotes (`'` and `‘’‚‛`) doubled, and a
    quoted first word gets `& ` in front so it runs.
  - Windows PowerShell 5.1 (`quote_for`, the profile's program
    `powershell.exe`; found on the PC, 2026-11-16): it hands a program
    its words the old way, which PowerShell 7.3 left
    (`$PSNativeCommandArgumentPassing`): each as it is, in double quotes
    only where it holds a space, an inner `"` left bare, an empty word
    dropped, and a backslash before its closing quote left single (so
    `C:\my dir\` came out `C:\my dir"`, an empty word went missing, and
    `a"b&c` ran on into the words after it). So each word after the
    first that any of that would change goes to it already quoted as a
    Windows program reads its words (`windows_word`), inside
    PowerShell's single quotes (`'"C:\my dir\\"'`, `'""'` for an empty
    one), which it passes on as they are. PowerShell 7's own (`pwsh`)
    are left to it, as above. A cmdlet given such a word would see the
    quotes; `flow`'s commands are a program's.
  - cmd (`windows_word`, `cmd_carets`): plain is letters, digits and
    `-_./:\@=,+~%`. Else double quotes by Windows' rules (a `"` as `\"`,
    the backslashes before it and at the word's end doubled), then `^`
    before each of cmd's own `& | < > ^` that cmd reads outside quotes,
    over the whole line: cmd knows no `\"`, so every `"` turns its quotes
    on or off, and that runs on from one word into the next. And `^`
    before every `%` (found on the PC, where `%PATH%` came back as the
    PATH, split at its spaces): cmd's prompt expands `%NAME%` before it
    reads quotes or carets, but leaves a name it doesn't know (`NAME^`)
    as it is, and takes the caret away after. Inside quotes, where a
    caret is a letter, the quotes are closed around it (`"50"^%" off"`),
    which a Windows program reads as the one word `50% off`; cmd's own
    `echo`, which prints its line as written, shows them.
  With `--shell`, the quoting follows the new shell, not the one `flow`
  was typed in.
- **`--close`:** Flow watches the pane's next command to end (the
  `Finished` that `watch_commands` takes) and closes the pane, or its tab
  where it's the only one, once that command ends with 0. A failure,
  Ctrl+C's 130 among them, leaves it open so its error can be read. cmd
  reports no exit code, so there it closes whenever the command ends.
  It's closed as ⌘W closes, without asking, since the shell is at its
  prompt again.

## How `flow` asks

- **The same road as `flow-settings`:** `flow_sdk::settings::ask`, which
  sends OSC 7732 to the tab's terminal on the Mac and Linux and down the
  tab's pipe on Windows, with a new request:
  `{"do": "open-tab", "pass": …, "command": ["ssh", …], "cwd": …,
  "split": false, "behind": false, "close": false, "shell": null,
  "theme": null}`. Flow answers `{"ok": true, "pane": <id>}` once the tab
  is open, not once its command is typed, or `{"ok": false, "error": …}`
  in Flow's language. The SDK carries any request as it is, so 0.1.43
  needs no new tag of it. Its docs list the new `do` at its next one.
- **The pass.** Flow takes OSC 7732 from whatever writes to the tab,
  which includes a remote host's output over ssh and a file being
  `cat`ed. Opening a tab that runs a command would let any such output
  run commands on this computer. So each pane gets a secret as its
  session launches: 128 bits from the system's random source
  (`getrandom`, already in the lock), as hex in `FLOW_TAB_KEY`, which
  `shell::build_command` gives the shell. Flow answers an `open-tab` only
  when its `pass` is the asking pane's own, compared in constant time.
  Restart Tab's new shell gets a new one.
  - **No answer without it** (from 0.1.46). At first a wrong pass and
    none got the same refusal, `{"ok": false, "error": …}`. But Flow
    writes its answer into the asking terminal, where it's the shell's
    input: a file `cat`ed could have Flow type a fixed refusal at the
    prompt up to sixteen times a frame, which a line editor in vi mode
    reads as commands (bash's vi-command `v` opens `$EDITOR`). So
    `tab_act` gives none (no answer at all) for a request without the
    asking pane's own secret, and `settings_act`'s
    `SettingsAct::Unanswered` does the same for settings, one that
    doesn't read among them, since it can't show the secret.
    `watch_settings` writes nothing for either, notes it in the log at
    debug, and down Windows' pipe drops the reply, which ends the pipe's
    wait at once. The programs say why themselves: `flow` and
    `flow-settings` say, where nothing answers, that the key they sent
    may be another pane's (or one an ssh session or a script brought
    along), besides Flow being too old or tmux in the way; and
    `flow-settings` with no `FLOW_TAB_KEY` asks nothing and says so at
    once (`flow` with none starts a Flow of its own, below). The
    assistant's and outside apps' `flow_settings` tool, which the user
    allowed, still gets its refusals in words.
  - This said at first that for settings no pass was needed, since
    `open_to_programs` keeps every key that could run something out. It
    was wrong: without one, printed output could read the config back
    into the tab (`list`, `get`), change and save the look, sound, the
    window and the shell's settings (`set`, `reset all`), reset the tab,
    and restart Flow, ending every program in every tab, which two
    `restart`s within five seconds did even while programs ran, the
    second taken for the user's yes. From 0.1.46 every settings request
    from a pane needs the pass too (`settings_act`'s
    `SettingsAsker::Program`), `flow-settings` sends it, and a program's
    `restart` restarts only while every other pane waits at its prompt,
    never counting as the user's confirmation.
  - Only programs started in that pane, on this computer, have it. The
    environment doesn't cross ssh unless the user's `SendEnv` names it.
  - Windows' pipe is the tab's own and local already. The rule is the
    same there, so it's one rule, tested once in core.
- **Through tmux or screen,** which don't pass OSC 7732 on, no answer
  comes (as for `flow-settings`), and `flow` says Flow didn't answer,
  naming them among the reasons.

## Where `flow` is

- **Windows:** a console program, `src/bin/flow-command.rs`, shipped as
  `games\flow.exe` beside `flow-settings.exe` (`release.ps1`,
  `installer.iss`). Flow's own `flow.exe`, a windowed program a shell
  can't wait on or print through, stays where it is, off the PATH.
- **Mac:** the same program as `Contents/Helpers/flow` (`bundle.sh`).
- **Linux:** `bin/flow` is Flow itself, found ahead of the programs'
  folder. `flow_core::run` hands its arguments to the same code first
  (`flow_command`), which asks the running Flow when it's in a Flow tab
  (`FLOW_TAB_KEY` set), and goes on to open a window only for
  `--window` or where there's no tab to ask. So on Linux a bare `flow`
  in a tab opens a tab, where it opened a new window before.
- **Another `flow` ahead on the PATH,** such as the JavaScript type
  checker's (`flow-bin` from npm), wins, since Flow's programs come last.
  Flow's is still there by its folder's path. Not solved in 0.1.43.

## A new Flow (`--window`, or no Flow to ask)

- **When:** `--window`, or `flow` run with no tab to ask (no
  `FLOW_TAB_KEY`: Windows' Run box, a shortcut, another terminal, a
  script started outside Flow). In a tab whose Flow doesn't answer (tmux,
  a Flow from before 0.1.43, a key that isn't the pane's), `flow` says so
  rather than starting one.
- **Started apart,** so `flow` returns at once and the new Flow outlives
  the tab:
  - Windows: `flow.exe` in the folder above `games\`, detached and in a
    process group of its own.
  - Mac: `open -n -a <Flow.app> --args …`, the bundle two folders up from
    `Helpers`. Without `-n`, Launch Services hands it to the Flow
    already running.
  - Linux: itself again, in a session of its own (`setsid`), its
    standard input, output and error on `/dev/null`.
- **Flow's own arguments** (`flow_core::run`, beside `--snapshot`,
  `--record` and `--carry-on`): `--window --cwd <dir> [--shell <id>]
  [--theme <id>] [--close] -- <command> [args…]`. `--split` and
  `--behind` mean nothing there and aren't passed.
- **Nothing of the tab's goes with it:** `FLOW_TAB_KEY`,
  `FLOW_IMAGE_PIPE`, `FLOW_PROMPT`, `FLOW_LANGUAGE` and
  `FLOW_USER_ZDOTDIR` are taken out of its environment (`TAB_ONLY`), and
  Flow takes `FLOW_TAB_KEY` out of its own as it starts, in case it came
  some other way (`open` passes the caller's variables on, as tried).
  As built, `FLOW_GAMES` and `FLOW_FIRST` stay: a Flow starting reads
  them to take that tab's folders off the PATH it gives its own tabs
  (`shell::path_with`), and gives its tabs their own. zsh's tabs keep
  Flow's folder of hooks as `ZDOTDIR` and the user's in
  `FLOW_USER_ZDOTDIR`, so the new Flow gets the user's `ZDOTDIR` back
  (none where it was the home folder), or its zsh would read no
  `.zshrc` of theirs.
- **A guest beside the Flow you had.** It opens that one tab, its command
  typed as above, and leaves the rest to the Flow already running:
  - It reads no `tabs.json` and writes none, as an administrator's Flow
    does.
  - It takes no drop-down hotkey, so no toast says it's taken.
  - It doesn't listen for outside apps (MCP).
  - It doesn't check for updates, install carried apps or remind of
    apps.
  - It logs to `flow-window.log`, so the first Flow's `flow.log` isn't
    emptied.

  Settings it changes are saved to config.toml as any Flow's are (the
  last to save wins, as with two Flows today). It quits when its last tab
  closes. It's a guest even where no other Flow runs, which keeps the
  rule simple: started for a command, it's for that command.

## Words, help and the docs

- `flow --help` from `flow_command.usage` in every locale, and its errors
  under `flow_command.*`.
- A help page of no screen (`Page::FlowCommand`), linked from Flow's own
  page as `flowapp`'s is. As built it's named `command` (`flow-help
  command`, `help:command`), since `flow` is Flow's own page's name.
- The manual: a section on starting a tab from a tab, beside `flow-settings`'
  ([From the shell](../manual/from-the-shell.md)).
  CLAUDE.md: a paragraph in the architecture.

## Tests

- Core, without a window:
  - argument parsing into a request (each option, `--`, an unknown
    option, no command);
  - `shell::quote` for each kind of shell;
  - the pass: none, wrong, another pane's, right;
  - the decision as a pure function (`tab_act`, as `settings_act` is),
    with the shell, theme and folder it picks.
- Each OS's shell tests in a real PTY: the quoted words typed into zsh,
  bash, PowerShell, cmd, BusyBox and Git Bash come back as the same
  words (a stand-in that prints its arguments one a line). Also: `flow`'s
  request reaches the session with its pass.
- By hand in a live window on each system:
  - `flow ssh …` from each shell, and each option;
  - `--window` from a tab and from outside Flow (Windows' Run box,
    Terminal.app, GNOME's terminal);
  - an `open-tab` printed by `cat` or a remote host, left unanswered
    (from 0.1.46; refused before).

## Not in 0.1.43

- Running the command as the tab's own program (`alacritty -e`'s way):
  `--close` covers a tab that ends with its command.
- `--split` below (⇧⌘D's), and splitting a pane other than the asking
  one.
- WSL tabs, whose programs see the tab's variables only where `WSLENV`
  passes them.
- Waiting for the command and giving its exit code (`--wait`).
- A Flow started a second time by hand, which reopens the same saved tabs
  and saves them as it quits (two Flows run on the PC today).

## As built (the Mac, 2026-11-16)

- Core: `core/src/flow_command.rs` (the arguments, the request, the
  pane's key, `tab_act`, the guest's arguments, `run`, `is_command`,
  `start_apart`), `shell::quote`, `Session::key`, and
  `core/src/app/flow_tabs.rs` (`open_tab_asked`, `tab_asked`,
  `split_asked` through `split_front_pane`, `tend_flow_tabs`,
  `close_flow_tabs`, the guest). The program is
  `src/bin/flow-command.rs`, named `flow` by `bundle.sh` and
  `release.ps1`; Linux's is `src/main.rs`.
- A shell that can't say it's at its prompt gets the command after
  `FLOW_TAB_WAIT` (10 s, longer than the update tab's 3 s, since a
  theme's start screen may still be typing).
- An `open-tab` refused is answered as a settings question is, into the
  asking terminal: a program that doesn't read it (a file `cat`ed) leaves
  the answer typed at the prompt, never run, as `flow-settings`' answers
  do today. (From 0.1.46 one without the pane's own key gets no answer
  at all, above, so a file `cat`ed leaves nothing at the prompt.)
- Tried in a test copy (`com.arcanii.Flow.test`, a scratch home), driven
  through its own `flow-mcp` from a pane of its own: `flow echo …` (a
  tab right after the asking one, in front, the command typed at zsh's
  prompt), `--behind`, `--split`, `--close true` (closed) and `--close
  false` (left open), `--shell bash`, `--theme green` (its start screen,
  then the command), an unknown shell and theme (exit 1, listing them),
  odd words (`it's`, `$HOME`, `*`, an empty one, `a b`, `~`) given back
  as they were, the notanAmiga's and the Flow Instruments' start screens
  going on without a key, a request `cat`ed from a file, one with no key
  and one with another pane's refused (no tab), the pane's own key by
  hand opening one; `flow --window` from a tab and `flow` from outside
  Flow starting a guest (`open -n`) that ran its command, logged to
  `flow-window.log`, closed with `--close` and saved no `tabs.json`, its
  zsh reading the user's own `.zshrc` (ZDOTDIR given back); the test copy
  itself, quit, saving its tabs as before.
- Tried on Linux (Ubuntu 26.04.1, GNOME, 2026-11-16):
  `linux/tests/shells.rs`' real-PTY test in zsh, bash and
  BusyBox's ash (`base64 -w0`, as GNU's wraps); in a copy installed with
  `install.sh --prefix` in a scratch folder with a scratch HOME, driven
  through its own `flow-mcp`: `bin/flow` with no arguments outside a tab
  (as the launcher runs it, and from GNOME Terminal) being Flow,
  `--version`, `--help` and `--snapshot` still Flow's, every option, odd
  words both ways between bash and zsh, the refusals, `flow --window`
  (`setsid`: a session of its own, outliving the tab and the Flow it came
  from, `flow-window.log`, no `tabs.json`, ZDOTDIR given back), `flow`
  from outside a tab, on Wayland and on X11, and the .deb's `usr/`
  layout, unpacked.
- `start_apart`'s taking out of a tab's variables is
  `leave_the_tab`, which `flow-mcp` calls too as it starts Flow: an app
  in a Flow tab that outlived its Flow (under tmux) handed `flow-mcp` the
  tab's key, and on Linux `flow` with a key and no arguments is this
  command, which asked a Flow that wasn't there for a tab instead of
  starting one (found on Linux).
- Two guests share `flow-window.log`, so the second to start empties the
  first's: noted, not changed.
- Not tried yet: Windows (`games\flow.exe`, PowerShell's and cmd's
  quoting in a real console, Git Bash's and BusyBox's paths, `flow.exe`
  started detached).

## As built on Windows (the PC, 2026-11-16)

- What the Mac wrote built here as it was: `apart`
  (`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`), `release.ps1`'s and
  `installer.iss`' `games\flow.exe`.
- `windows/tests/flow_command.rs`: the odd words (`it's $HOME`, `*`,
  `a b  c`, `""`, `a"b&c`, `x&y`, `%PATH%`, `$env:HOME`, `a,b`, `@x`,
  `-D.x`, `C:\my dir\`, an empty one, `--`, `ünï`, the ssh words) quoted
  by `shell::quote_for` and typed with `Session::run_typed` into cmd,
  PowerShell 7.7, Windows PowerShell 5.1, Git Bash and the bundled
  BusyBox through ConPTY, given back as they were by a copy of the test
  program (`stand_in`, printing its arguments as a Windows program reads
  them, since cmd's `echo` prints its line as typed); and in each, the
  `flow` program's own ask (`flow_command::run` with
  `flow_sdk::settings::ask`) down the tab's pipe carrying the pane's
  `FLOW_TAB_KEY`, reaching the session as a settings ask, its answer
  going back to `flow`, which exits 0 or 1 by it.
- Two of the quoting rules the real consoles disagreed with, changed
  (above): cmd's `%`, now kept with `^`, and Windows PowerShell 5.1's
  old way of handing words to programs (`shell::quote_for`, which
  `tab_act` and the guest's tab use).
- Tried live in the portable files (build 665) unpacked to
  `%USERPROFILE%\Flow-0.1.43-test`, a scratch profile beside it
  (`APPDATA`, which Flow on Windows now goes by: `config::config_dir`)
  and an MCP pipe of its own (`FLOW_MCP_SOCKET`), driven through its own
  `games\flow-mcp.exe` with a grant written in its `mcp.json`. From cmd,
  PowerShell 7.7, Git Bash and BusyBox tabs (a
  second instance with `FLOW_SHELL=busybox`): `flow ssh -N -L
  7445:127.0.0.1:7445 user@host` (typed as written; ssh couldn't resolve
  `host`), `ping -n 20` in a tab of its own, `--split` (a pane beside,
  in front), `--behind` (the front tab kept), `--close` with a command
  that ended well (closed; `true`, `cmd /c exit 0`) and one that didn't
  (kept; `false`, `cmd /c exit 1`), `--shell cmd`, `--shell pwsh`,
  `--theme green` (its start screen, then the command), an unknown
  shell, theme and option (exit 1, naming those there are), words with
  quotes, spaces, `&` and `%PATH%` given back; each new tab right after
  the asking one, in its folder. `flow --window` from a tab started a
  guest (`flow.exe --window --cwd … -- …`, detached) that outlived the
  tab, which was closed, and one with `--close` that ran its command and
  quit; so did `flow` from a terminal outside Flow (no `FLOW_TAB_KEY`)
  and `flow` started through the shell as the Run box starts it
  (`Start-Process`, which gives it a console of its own; the Run box
  itself wasn't driven). The guests logged to `flow-window.log`, and no
  `tabs.json` was written. An `open-tab` with a wrong key `type`d from a
  file in cmd opened nothing, and with the pane's own key one opened, so
  ConPTY passes OSC 7732 on; the refusal's answer, which the Mac leaves
  typed at the prompt, doesn't show at cmd's: ConPTY's reader of what's
  typed takes it as a sequence. (From 0.1.46 there is none to show.)
- Two guests at once share `flow-window.log` here too, the second
  emptying the first's at its start.
- Found with every Flow closed, after the release script's smoke test
  passed (it checks the installed `games\flow.exe --version`): a guest
  held what started it open, a script's capture of `flow --window`'s
  output never ending while the guest ran, since Windows hands a child
  every inheritable handle and the flow command's standard handles come
  so. `src/bin/flow-command.rs` takes the mark off them first
  (`flow_windows::keep_std_handles`); `flow --window ping …` captured
  then returned in 0.1 s. And `flow-mcp`, finding `games\flow.exe`
  beside it, started the flow command (a guest, listening for no outside
  app) in place of Flow; it looks for a release's own layout first now.
