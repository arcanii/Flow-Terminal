# Flow as an MCP server (0.1.42)

## Context

Asked for on 2026-11-16: Flow as an MCP server that apps outside it can
use, with the user giving each its permissions. The second item of 0.1.42,
after skills from flowapp packages. Outside apps (Claude Desktop, Claude
Code, Cursor) should drive Flow, with the user deciding what each may do.

What was decided:
- **What an app may do:** read tabs, run commands, open and close tabs and
  panes, and change what `flow-settings` may.
- **Permission per app, asked once:** the first time an app connects, a card
  names it and what it asks for. Each kind is allowed or denied, remembered,
  and listed in Settings to change or revoke. Commands still ask each time
  unless the app is allowed to run them.
- **A `flow-mcp` program** ships with Flow. An app's MCP config runs it as a
  stdio server, and it talks to the running Flow over a socket only the user
  can open (a named pipe on Windows). Nothing listens on the network.
- **Flow not running:** `flow-mcp` starts it and waits.
- **Where commands run:** only in the app's own panes: a pane beside the
  user's, labeled with the app's name, or the tabs and panes it opened. It
  may read the user's panes but never types into them.
- **High-risk commands** (and `tools::always_asks`) still wait for the user's
  yes in a card, even for an app allowed to run commands.
- **Off until turned on:** Flow listens only once Settings' switch is on.

## How it works

**1. `flow-mcp`, the stdio server** (`src/bin/flow-mcp.rs`, a fifth binary;
the protocol in core, `core/src/mcp/`)
- It speaks MCP's JSON-RPC 2.0 over stdin and stdout, a message a line, written
  by hand as the AI clients are, with no SDK. It handles `initialize`
  (answering the client's protocol version where it's one Flow knows, else
  the newest: 2025-06-18, 2025-03-26, 2024-11-05), `notifications/initialized`,
  `ping`, `tools/list` and `tools/call`. Logs go to stderr only.
- Each `tools/call` goes to Flow over the bridge, and the answer comes back as
  MCP content (`{"type":"text"}` plus `isError`).
- **Starting Flow:** where the socket isn't there, it starts the Flow it ships
  with. On the Mac that's `open -g` on its own bundle (Helpers' parent); on
  Windows, `flow.exe` beside its `games\` folder; on Linux, `bin/flow` of its
  prefix. It waits up to 20 s for the socket. If Flow is running with the
  server off, the answer says to turn on Settings → Outside apps.

**2. The bridge** (`core/src/mcp/bridge.rs`)
- Unix (Mac and Linux, in core): a socket at `<config_dir>/mcp/flow.sock`,
  its folder 0700. Flow binds it when the switch is on, after removing a stale
  one that no longer answers. A second Flow finds one answering and serves
  nothing.
- Windows: `Platform::mcp_listen` (`windows/src/mcp.rs`, beside `pipe.rs`).
  The pipe is `\\.\pipe\flow-mcp-<user>`, `PIPE_REJECT_REMOTE_CLIENTS`, and
  its first instance has `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a second Flow,
  or anything that took the name first, gets none. An elevated Flow
  (`Platform::elevated`) never serves.
- **Only the user, at both ends (0.1.46).** Each end asks the system who's
  at the other before a word goes. The socket's folder is usually the
  user's own, but `FLOW_MCP_SOCKET` may name any path, and a config path
  too long for a socket falls back to `/tmp/flow-mcp-<user>/flow.sock`,
  whose folder another user could make first; and every user shares
  Windows' pipes' names, which are easy to guess. Root and administrators,
  who can act as anyone, are out of scope.
  - Unix, by uid. `bridge::peer_user` reads the effective uid at the other
    end (the Mac's `getpeereid`, Linux's `SO_PEERCRED`, through the
    `linux_cred` that `linux_peer` shares), and `of_user` takes only
    `Some` of the user's own (`this_user`, `geteuid`), so a connection the
    system can't name is refused.
    - Flow's end: `listen_unix`'s accept loop closes, unread, a connection
      from another uid, before `peer_of`, `Conns` or `serve`, so nothing
      reaches the window.
    - `flow-mcp`'s end: `connect_unix` connects, then refuses a listener of
      another uid before anything is written, as `bridge::NotUsersFlow`
      (an `io::Error` of kind `PermissionDenied` that `NotUsersFlow::is`
      tells apart from the system's own `EACCES` and `EPERM`).
    - The folder: `listen_as` makes it, then takes it only where it's a
      real folder (not a link) owned by the user, before it chmods it 0700
      or removes an old socket there, else `outside.folder_not_own`'s
      toast. A link planted at `/tmp/flow-mcp-<user>` could otherwise have
      Flow chmod another of the user's folders, or remove a `flow.sock`
      in it.
    - `listen_as` and `connect_as` take the uid expected, so the tests
      play another user without root; nothing a user or program sets
      changes which uid is taken.
  - Windows, by the pipe and the token (`windows/src/pipe.rs`, which both
    of Flow's pipes share, and `windows/src/mcp.rs`).
    - Its security: every instance is made under `pipe::UserOnly`,
      `O:<sid>D:P(A;;GA;;;<sid>)`, which names the user as the pipe's
      owner as well as the only one let in, administrators and the system
      included. An elevated Flow's (its token at high integrity) is
      labelled `S:(ML;;NW;;;HI)` too, so the user's programs that aren't
      elevated can't write to it; unlabelled it would count as medium. A
      label no higher than its maker's needs no privilege.
    - `pipe::owned_by` (`GetSecurityInfo` on the handle, the owner, and
      `EqualSid`), at both ends. Only the user can make themselves a
      pipe's owner (anyone else would need SeRestorePrivilege), so a pipe
      another user made never passes, whatever process id it gives. Flow
      checks each instance it makes (`UserOnly::owns`, a later instance
      joining whatever pipe has the name), and `flow-mcp` its own end.
    - `pipe::serve_each`, one serving loop for both pipes: an instance is
      made only while another of Flow's holds the name, the next before
      a failed one (`ERROR_NO_DATA`, a client gone before the wait began)
      is dropped. So the name is never free while Flow serves it, for
      someone to claim with `FILE_FLAG_FIRST_PIPE_INSTANCE`, which Flow's
      next instance would join. Where no next instance can be made
      (`CreateNamedPipeW` failing on the user's own pipe, out of
      resources, say), the pipe stops serving (a warning in the log
      where it fails after a program connected, only a debug line where
      the program had gone before Flow waited), and its name goes free
      once its connections end, though the tab or the Outside apps
      switch still lives and `FLOW_IMAGE_PIPE` still names it:
      `flow-mcp`'s checks below still hold then, and the image pipe's
      clients are as after a tab closes (What it leaves).
    - `flow-mcp` opens the pipe through `flow_windows::mcp::open` at the
      level of identification (`as_client`: std's `security_qos_flags`
      with `SECURITY_IDENTIFICATION`, std adding `SECURITY_SQOS_PRESENT`).
      Asked for no level, as std's `OpenOptions` asks by default, a pipe's
      client lets whatever serves it act as them (once it has read what
      the client wrote, as `ImpersonateNamedPipeClient` asks), and the
      open comes before any check can: a program that took the name under
      an account holding SeImpersonatePrivilege (LOCAL SERVICE, NETWORK
      SERVICE, an IIS app pool's) could take on the user's token and start
      programs as them. Before 0.1.46 `flow-mcp` took any `flow.exe` as
      Flow, so a squatter so named read its Hello and could. From 0.1.46
      it writes nothing to a pipe it refuses, so whether the open alone
      lends the token isn't shown (it couldn't be tried on the Mac); asked
      for identification, nothing it writes lends more than who it is,
      should a check ever pass what it shouldn't. Flow never impersonates
      its clients, and `GetNamedPipeClientProcessId` works at any level,
      so Flow's side is unchanged.
    - Then `served_by_flow` takes the pipe only where, in order: the user
      owns it (`owned_by` on `flow-mcp`'s own handle, which a reused
      process id can't fool, since `GetNamedPipeServerProcessId` names the
      process that made the instance even once it's gone and its id given
      to another); the process that serves it
      (`GetNamedPipeServerProcessId`) runs as the user at medium integrity
      or above (`pipe::User::of_process`, so an AppContainer's or another
      sandbox's program can't pass for them); and it's a `flow.exe`.
      Anything else is `NotUsersFlow` too.
    - Flow's end checks no user of its own: the access list, which the
      kernel checks at the open, lets in the user alone, and remote
      clients are turned away. It reads who connected only to know the
      app (below).
  - `flow-mcp` tells the model which of the two kept it out (`kept_out`):
    what answers isn't the user's own Flow (`not_flows`, the refusals
    above), or the system won't let it in (`not_let_in`: another user's
    address, or an app that runs `flow-mcp` in a sandbox). Either way it
    starts no Flow, which couldn't take the address back or get past the
    sandbox.
  - Older Flows and `flow-mcp`s still meet: an older Flow's pipe has no
    `O:`, so it's owned by its token's default, the user under UAC, which
    a new `flow-mcp` takes; an older `flow-mcp` checks only the name.
  - What it leaves:
    - The pipe's name is `USERNAME` filtered to ASCII letters, digits and
      `-_.` (`bridge::address`), so every all-Japanese name is
      `flow-mcp-`, and names that differ only in what the filter drops
      share one. Those users no longer reach each other's Flow, but only
      the first one's can listen; the others hear `not_flows` or
      `not_let_in`. Naming it by the user's SID, or a hash of it, would
      change the address both ends work out, so Flow and `flow-mcp` would
      change together.
    - A low-integrity program of the user's own could make the pipe while
      Flow isn't listening (owned by the user, then), and win a process
      id's reuse against the user's `flow.exe`. Reading the pipe's own
      mandatory label on `flow-mcp`'s handle (`GetSecurityInfo` with
      `LABEL_SECURITY_INFORMATION`) and wanting medium or above would
      close it.
    - Before Flow-SDK 0.1.4, the image pipe's clients checked nothing
      of who served it, and opened it at the default level, as
      `flow-mcp` did. Every one Flow carries goes through Flow-SDK's
      `Pipe::open` (`src/pipe.rs`; at `v0.1.3`, std's `OpenOptions`
      reading and writing, no level asked), `flow-settings` and the `flow` command by
      `flow_sdk::settings::ask`, `flowapp`'s splash directly, and so do
      the games, the demos and the SDK's C library and Python package, by
      its `detect`. While Flow serves the pipe, `serve_each` keeps its
      random name Flow's. Once the tab closes, or the pipe stops serving
      (above), the name goes free when its connections end, and a program
      from that tab still running (one started with `start`, a detached
      SDK or Python program) that opens it again meets whoever took it
      next. That tab's `FLOW_TAB_KEY` opens nothing by then, but what it
      sends isn't the harm: the program writes as soon as it's open, so a
      squatter holding SeImpersonatePrivilege could act as the user, as
      above. Flow-SDK 0.1.4, the tag Flow takes from 0.1.46, closes it:
      `Pipe::open` asks `security_qos_flags(SECURITY_IDENTIFICATION)`
      (std adding `SECURITY_SQOS_PRESENT`), then reads its handle's
      owner and writes nothing unless that's the user
      (`owner_is_user`), so another user's pipe hears nothing. A check after the
      open alone wouldn't close it, since the level is given at the
      open. A shell's own redirection to it (`> %FLOW_IMAGE_PIPE%`) opens
      at the default level too, which nothing of Flow's can change.
    - `flow_running()` on Windows reads `tasklist`, which sees other
      users' Flows too; it only decides whether to start Flow.
- Frames are a JSON a line. `{"hello": {client}}` comes first (the MCP
  client's name and version from `initialize`), then
  `{"id","tool","args"}` and answers `{"id","ok","result"|"error"}`.
- **Who's asking:** Flow reads the peer's pid (`LOCAL_PEERPID` on the Mac,
  `SO_PEERCRED` on Linux, `GetNamedPipeClientProcessId` on Windows). That's
  `flow-mcp`; its parent is the app that started it. A new
  `Platform::process_info(pid)` gives a process's parent and executable path
  (`proc_pidpath` and `proc_pidinfo`; `/proc/<pid>/exe` and `stat`;
  `QueryFullProcessImageNameW` and Toolhelp). An app is known by its
  client name and that path, so a renamed client from another program asks
  again. The card says this guards against apps by mistake, not against
  malware already running as the user, which could do anything anyway.

**3. In Flow** (`core/src/mcp/` for what needs no `App`, `core/src/app.rs`
for the rest)
- A `flow-mcp` listener thread, then a thread per connection, as `serve_pipe`
  does. Each request becomes `UserEvent::Mcp(McpRequest { conn, id, tool,
  args, reply: crossbeam Sender })` through the `Waker`. It's handled in
  `user_event` and driven from `about_to_wait` (`App::advance_mcp`, as
  `advance_helpers` is), never waiting for a frame.
- **Grants** (`mcp::Grants`, `mcp.json` beside config.toml, written as each
  answer comes, never by snapshots): each app's name, path and its kinds:
  `read`, `tabs`, `settings` on or off, and `commands` Off, Ask or Allowed.
- **The permission card** (`ui::McpAsk`, queued as `model_asks` are,
  `ui::draw_mcp_ask`, `Hit::McpAsk*`): "Claude Code wants to use Flow", the
  host's path under it, a switch for each kind (preset to what the app's
  tools need, commands at Ask), and Allow and Deny (Deny: every kind off,
  remembered). The app's calls wait meanwhile. With Flow behind, it notifies
  (`Platform::notify`). Screen readers get it as a dialog.
- **A kind not allowed** answers with an error naming where to allow it
  (Settings → Outside apps).
- **Commands:** with Ask, or where `tools::command_risk` says High or
  `tools::always_asks` matches, a command card shows the app, the command
  and its risk, with Run and Skip (`ui::McpCommand`, the same queue). Run
  goes to the app's own pane (`PaneFor::Outside(app)`, opened by
  `open_side_pane` in the tab in front and kept for the app's next
  commands), or to a pane the app opened. `Session::run_command` and
  `poll_command` run it, with the app's timeout, at most 600 s (default
  `ai.command_timeout_secs`). The answer is its output (cut at
  `MAX_TOOL_OUTPUT`), exit code, folder, and whether it's still running.
  One command a pane at a time, since a session keeps one tracker.
- **Tools** (`mcp::tools::list`, with their JSON schemas):
  - `list_tabs` (read): each tab, its panes (id, label, shell, folder, at
    its prompt, the one in front, the app's own).
  - `read_pane` (read): a pane's screen and scrollback, the last `lines`
    (default 200, at most 2000), through `Emulator::recent_text`.
  - `run_command` (commands): `command`, optional `pane` (the app's own),
    `timeout_secs`.
  - `open_tab` (tabs): optional shell, theme and folder; the tab and pane
    are the app's own.
  - `split_pane` (tabs): splits one of the app's panes.
  - `close_pane` (tabs): only a pane or tab the app opened, never the
    user's.
  - `focus_pane` (tabs): brings any pane to the front.
  - `flow_settings` (settings): `settings_answer` on the tab in front, the
    same requests as `flow-settings` (`open_to_programs` still decides),
    but for `restart` and `reset-terminal`, which end what runs and which
    `settings_act` refuses Flow's tools (`SettingsAsker::Tool`) from
    0.1.46; it needs no pane's secret, the app's grant standing for it.
  - `flow_help` (read): Flow's help pages.
- The assistant's own panes, approvals and `ToolBatch` stay apart: an
  outside app's commands use their own state (`App::mcp`), as helpers do.

**4. Settings: Outside apps** (`Section::OutsideApps` after Skills)
- **Let outside apps connect** (`mcp.enabled`, off at first, closed to
  programs). Turning it on starts the listener, and off stops it and ends
  connections.
- **Connect an app:** `flow-mcp`'s path (`Platform::games_dir`), with Copy
  for Claude Code (`claude mcp add flow -- <path>`) and Copy for a JSON
  `mcpServers` entry (Claude Desktop, Cursor).
- For each app remembered: its name with **Forget**, then a switch each for
  Read tabs, Open and close tabs and Change settings, and Run commands as
  Off / Ask / Without asking. The connected ones say so.

**5. Shipping**
- `flow-mcp` beside `flow-settings` and `flowapp`: the bundle's Helpers
  (`bundle.sh --game`, `release.sh`'s `games`), Linux's `package.sh`,
  `install.sh`'s uninstall and the .deb, Windows' `release.ps1` and
  `installer.iss`.

**6. Words, docs and snapshots**
- en and ja for the card, the command card, Settings, the errors and
  `flow-mcp --help`; a help page (`Page::OutsideApps`); the manual (a page on
  [outside apps](../manual/outside-apps.md) with the setup lines, the keys); CLAUDE.md; BACKLOG
  (built).
- Snapshots, made up: `mcp-ask.png` (the permission card), `mcp-command.png`
  (a command card), `settings-mcp.png`.

## Files

- New: `src/bin/flow-mcp.rs`; `core/src/mcp/mod.rs`, `protocol.rs` (MCP
  JSON-RPC, pure), `tools.rs` (the list and schemas), `bridge.rs` (socket,
  framing, peer pid), `grants.rs` (`mcp.json`); `windows/src/mcp.rs`.
- Changed: `core/src/app.rs` (`App::mcp`, `advance_mcp`, cards, tools),
  `core/src/event.rs` (`UserEvent::Mcp`), `core/src/ui.rs` (two cards,
  `PaneFor::Outside`), `core/src/settings.rs`, `core/src/config.rs`
  (`mcp.enabled`), `core/src/platform.rs` (`process_info`, `serve_mcp`),
  `macos/src/processes.rs`, `core/src/resources.rs` (Linux), the release
  scripts, locales and docs.
- Reused: `Session::run_command`/`poll_command`, `Emulator::recent_text`,
  `open_side_pane`, `settings_answer`, `tools::command_risk`/`always_asks`,
  the `ModelAsk` card pattern, `serve_pipe`'s request-with-reply shape,
  `Platform::notify`, `Platform::games_dir`.

## Tests

- `protocol`: initialize and version choice, `tools/list` names and schemas,
  `tools/call` through a stand-in bridge, JSON-RPC errors (unknown method,
  bad params, not initialized), notifications answered with nothing.
- `bridge`: framing over a socket pair; the peer pid on Unix; a stale socket
  replaced; a live one left alone. From 0.1.46 the peer checks too: the uid
  at the other end, a socket another user listens at refused (`connect_as`),
  another user's connection closed unread, the system's own refusal told
  apart, and no folder that's a link or another's listened in; on Windows
  (run on a PC) `pipe.rs`'s instances owned by the user and never leaving
  the name free, and `mcp.rs`'s identification-level open
  (`what_serves_flows_pipe_may_know_who_opened_it_but_never_act_as_them`).
- `grants`: kept and read back; what each kind allows; commands Off, Ask and
  Allowed, with High risk and `always_asks` still asking.
- `process_info` on this OS: this process's parent and path.
- `flow-mcp` end to end (`tests/mcp.rs` in the root package): the binary
  against a stand-in Flow on a scratch socket (`FLOW_MCP_SOCKET`), with
  initialize, tools/list and a call over its stdin and stdout.
- Settings' section rows; i18n; `cargo test --workspace`; clippy at
  flow-core's 8.

## Verification

- Snapshots of the two cards and Settings' section, looked at.
- A test Flow.app with the switch on, connected to Claude Code or Claude
  Desktop with the copied line. It asks once, reads a tab, runs a command
  (asking, then allowed), opens and closes a tab, sets a theme, and is
  refused a kind turned off. Then Flow is quit, to see flow-mcp start it.
- The Windows pipe and Linux's socket built and tried on those systems
  from master (Windows' code isn't built on the Mac).
- Nothing published or released until a release is agreed.

## As built (2026-11-16, the Mac)

Built to this plan, with these choices made on the way:
- The words of the cards and Settings are under `outside.*` and
  `settings.mcp_*` in the locales; the help page is `help:outside`.
- The permission card starts with Read on and whatever the app's first
  call needs, the rest off; commands, switched on there, ask each time.
- `flow-mcp` sends one call at a time, as each connection is served in
  order; the app's own pane is opened on its first command.
- Forget, in Settings, is one press: the app is only asked again.
- `close_pane` on a tab's last pane closes the tab as Flow next waits; the
  last tab of all stays open.
- The tools' words, which go to the app's model, are English; `--help` and
  the cards are in the user's language.
- Windows' pipe (`windows/src/mcp.rs`) was written on the Mac and is built
  and tried on the PC first.
- Flow is started without a tab's variables
  (`flow_command::leave_the_tab`, 0.1.43): an app running in a Flow tab
  that outlived its Flow (under tmux) hands `flow-mcp` the tab's
  `FLOW_TAB_KEY`, and Linux's `bin/flow` with a key and no arguments is
  the flow command, which only asked a Flow that wasn't there for a tab.
  Found and fixed on Linux, 2026-11-16, where `flow-mcp` starting Flow
  was tried for the first time.
