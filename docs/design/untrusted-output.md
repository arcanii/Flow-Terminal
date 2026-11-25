# Untrusted output (0.1.46)

Before Flow's source opens, an audit (2026-11-23) looked at what text
Flow didn't write could make it do, and the fixes below were settled and
built on the Mac on 2026-11-23 and 24, in several passes, each attacked
and fixed again. This is the design of the whole pass, kept up to date as
it changes: what each part does, why it's done so, and what it leaves.
The manual's pages (from-the-shell, shells, images, assistant,
outside-apps, flowapp, terminal, keys) say what each part does for the
user; this says why.

## What it's for

A terminal takes escapes from whatever is printed in it, and Flow reads
more of them than most: its own settings (OSC 7732), effects and sounds
(OSC 7731), the shells' marks (OSC 133 and 633), pictures (kitty's, sixel,
iTerm2's), 3D models (Ratty's), links (OSC 8) and the folder (OSC 7). So
anything that can print into a tab is a sender:

- a file being `cat`ed, a log being followed, a page `curl` prints;
- a remote host over ssh, and every program there;
- a theme's start screen, typed through the same scanner as a program's
  output, and its prompt, which the shells draw before each command;
- a package flowapp installs, and a `.tar.gz` of the user's own from
  anyone.

And text reaches Flow by other ways than the terminal, each with its own
trust:

- outside apps, through `flow-mcp` and the bridge's socket or pipe;
- the assistant, whose commands come from a model that reads the screen,
  files and pages, any of which may be written to steer it;
- the image pipe each Windows tab has, since ConPTY drops APCs.

The rule throughout: what a sender can make Flow do must be what that
sender could do anyway. A program in the tab can run programs as the
user, so it's trusted with what the tab's own key opens; text it merely
prints is not, and neither is a remote host, a theme, a package or a
model.

## OSC 7732: the pane's key, and a program's restart

**What it does.** Every settings request from a pane (`list`, `describe`,
`get`, `set`, `reset`, `reset-terminal`, `restart`, and any `do` Flow
doesn't know) must carry that pane's own secret, `Session::key`
(`FLOW_TAB_KEY` in its shell's environment, 128 bits from `getrandom`), as
its `pass`, as `open-tab` (the `flow` command) already did. The check is
the first thing `settings_act` does with a `SettingsAsker::Program`,
before it reads the `do` or the config, compared by
`flow_command::same_key` in constant time; an empty key matches nothing.
A request without the key, with a wrong one, or one that doesn't decode,
gets no answer at all (`SettingsAct::Unanswered`; `tab_act`'s `None`),
on the terminal and down Windows' pipe alike, where the reply's channel
is dropped so `serve_pipe` stops waiting at once. Flow logs one line at
debug. `flow-settings` sends the pass with every request; with no
`FLOW_TAB_KEY` (sudo, `env -i`, ssh) it asks nothing and says why
(`flow_settings.no_key`, exit 2), and where nothing answers it says the
key may be another pane's, or that tmux or screen is in the way.

A program's `restart` never stands for the user's yes:
`restart_for_program` restarts at once only while every other pane waits
at its prompt (`others_wait`, the other panes of its own tab among them),
else it answers `ok: false`, shows a toast pointing to Settings' Restart,
and never reads or arms `restart_asked`, the user's own Restart Flow
confirmation. The assistant's and outside apps' `flow_settings` tool
(`SettingsAsker::Tool`, already allowed by the user's approval or grant)
needs no pass, but may not `restart` or `reset-terminal`.

**Why this way.** Before, `list`, `get` and `describe` wrote the whole
config back into the pty as base64, `set` changed and saved settings, and
two restarts printed within five seconds counted as the user confirming.
One gate in the pure decision function covers every `do`, present and to
come, and is tested without a window. No answer, rather than a refusal,
because Flow's answer goes into the shell's input: a line editor in vi
mode reads its letters as commands (bash's `v` opens `$EDITOR`), and a
`cat`ed file could send sixteen asks a frame. The programs that ask know
best why nothing came, so they say it.

**What it leaves.**
- Code running in the pane has the key, by design; so does anything it
  hands the key to (a script, an ssh session it's passed along to).
- A pane's own recorded output (`script`, asciinema) replayed in that
  same live pane is accepted again.
- A program built on Flow-SDK before 0.1.4 sends no pass, so it gets no
  answer until it's built again. From 0.1.4, `ask` adds the pass from
  `FLOW_TAB_KEY`, from Rust, C and Python alike.
- PowerShell, cmd and BusyBox say no command's start, so a pane there may
  count as at its prompt while something runs, and a program's restart
  may go ahead.

## A theme's prompt is text in every shell

**What it does.** `looks::prompt_for` writes a theme's prompt for each
shell so that no shell can expand it, now or later, there or in a shell
it starts:
- every control character but `\n` and `\t` goes, and of the escapes only
  colors (SGR) stay, so a prompt can't send OSC 133, 633 or 7732 before
  each command;
- it stops at `MOST_PROMPT` (4096 bytes), at a whole piece;
- bash gets `$`, backtick, `\` and `!` as `\D{c}` (strftime with no `%`),
  whose `$` and backtick bash protects itself, with `\001\002` before it
  where the byte before may begin a two-byte letter (Shift JIS, GBK);
  `__flow_prompt` unexports `PS1` last while a theme is worn;
- zsh gets them as `%-4v` to `%-1v`, which `__flow_precmd` (under
  `emulate -L zsh`) puts last in `psvar` before each prompt; zsh never
  expands what `%v` brings, whatever options come on later
  (`prompt_subst` in a widget), and an exported zsh `PS1` holds nothing a
  child shell would run;
- BusyBox ash gets `\044` and `\140`, `PS1` is unexported before and
  after, and a bash reading the ash hook through `$ENV` keeps its own
  prompt;
- PowerShell gets a backtick before `$`, a backtick and the curly double
  quotes, but leaves the straight one bare: `ExpandString`, which the hook
  hands the prompt to, keeps a bare `"` as it is and drops everything after
  a backticked one (5.1 and 7.7 alike, found on the PC), while a bare curly
  one ends the text there; cmd was already safe.

**Why this way.** A theme comes from a built-in look, a catalog package,
an unsigned `.tar.gz`, a preset file or a saved theme, and its prompt was
handed to `PS1` nearly raw: `$(…)` ran at every prompt in bash and ash,
and in zsh under `prompt_subst`, which oh-my-zsh turns on. Each form is
one its shell guarantees to show as written in every mode. Turning
`prompt_subst` on for every zsh user was weighed and refused: `print -P`
then runs `$(…)` in data, a new hole in the user's own plugins.

**What it leaves.** zsh run as sh shows `%-4v` as written; a widget that
changes `psvar`'s end shows wrong characters, never runs any; a `PS1`
exported again earlier on the same command line reaches a shell started
on that line.

## Links and folders on this computer only, and Windows' opener

**What it does.** One check says whether a path names this computer:
`link::on_this_computer` (its rules in `local(text, windows)`, so both
systems' are tested anywhere, reading only the text). It refuses any
path beginning with two separators in any mix (`//h/s`, `\\h\s`,
`\\?\UNC\…`, `\\.\pipe\…`), and on Windows takes only a drive's path or
the verbatim form exactly as `canonicalize` writes it (`\\?\C:\…`, in
backslashes, with no `.` or `..`). It's applied wherever a path a program
printed becomes one Flow touches:
- `file:` links (`link::target`, `file_path`): one to another computer is
  no link at all, no underline and no click;
- the shell's folder (OSC 7, 633;P, 9;9) for new tabs, splits, Restart
  Tab, `tabs.json`, Attach and the flow command;
- a 3D model's path, read only after the user's yes and only from a
  regular file (`model::Ask::load`);
- the assistant's file tools (`tools::file_elsewhere`), refused without
  a card in either mode, judged by one read of the folder that the
  decision and the open share.

What reaches the system's opener is a web or mail address
(`link::for_the_opener`: http or https with a host, `mailto` with only
`to`, `cc`, `bcc`, `subject`, `body` and `in-reply-to`), or a `file:` URL
of Flow's own. Windows opens it with `ShellExecuteExW` (`shell_open`), not
`explorer.exe`'s command line, whose switches a `,` or `"` could reach. An
elevated Flow opens nothing in another program. MCP sign-ins and links
must be web addresses too.

**Why this way.** On Windows, merely touching `\\host\share` (an
`exists()`, Explorer's `/select,`) signs in to that host with the user's
NTLM hash, and any program can print a `file:` link, and plain text that
looks like one is a link too. Win32 normalizes every `?` or `.` device
prefix but the exact verbatim one, and its `..` climbs from `C:` to
`UNC`, so only the exact form is let through.

**What it leaves.** Mapped and `subst` drives, which are the user's own;
a guide-mode card approved after the folder moved to another local folder
opens from there; bidi and look-alike characters in addresses (display
only); commands naming a share are asked about on Windows
(`names_a_share`), a check by pattern that misses a bare host name and a
share reached through a variable.

## Kitty's files

**What it does.** A kitty transmission that names a file (`t=f`, `t=t`)
tells the program nothing about this computer's files but whether a path
holds a picture Flow shows:
- every failure gets the same answer, `EBADF:could not read the file`,
  decoding and inflating included (`FileFailures::load`, `load_file`);
- a file that isn't a temporary one is read only as a whole PNG, GIF or
  WebP from its start (`O` and compression refused, `S` ignored), and a
  PNG whose header is missing or over `MAX_SIDE` is refused after its
  first `PICTURE_HEAD` (24) bytes;
- raw pixels, zlib and windows come only from a `t=t` temporary file,
  with `TEMP_MARK` (`tty-graphics-protocol`) in the path as sent and its
  real path, under a temporary folder, deleted once opened;
- `/proc`, `/sys` and `/dev` but `/dev/shm`, relative paths and Windows'
  UNC and device paths are refused;
- every failure takes `FILE_FAILURE_TIME` (2 ms), spun out on the clock
  (`wait_out`), and past `FILE_FAILURES` (8) in `FILE_FAILURE_WINDOW`
  (10 s) every file-named transmission in that pane is refused unread
  until the window ends, kept through a terminal reset.

**Why this way.** Specific errors told a remote program whether any file
existed, its size by binary search, and bits of its content; kitty's spec
asks for one answer. The same answer still left the time a lookup took,
an existence oracle by the thousand with `q=2`; a sleep ends when the
system's timers say (4.5 ms late under the Mac's utility class), so only
looking at the clock evens it out, and the budget bounds what's left and
the cost. Clients (`kitten icat`, ranger, viu) fall back to sending the
picture directly, which shows.

**What it leaves.** Whether a path holds a picture, and its size once
shown; a cold disk, or a long GIF or WebP or a PNG with a good header and
bad data, read whole before failing, can still show in the time, a few
times a window at most; a reader thread the system holds back during the
wait.

## The bridge's and the image pipe's peers

**What it does.** Both ends of the outside apps' bridge ask the system
who is at the other end before sending anything:
- Unix: `peer_user` (`getpeereid`, Linux's `SO_PEERCRED`); `flow-mcp`
  connects with `connect_unix`, which refuses a listener of another user
  (`NotUsersFlow`) before writing, and Flow closes a connection from
  another user unread. The socket's folder must be the user's own real
  folder before Flow changes or removes anything in it.
- Windows: both pipes are made with a descriptor naming the user alone as
  owner and only entry (`pipe::UserOnly`), labelled high integrity in an
  elevated Flow, served by one loop (`pipe::serve_each`) that never lets
  a name go free while Flow serves it. `flow-mcp` opens the pipe at
  identification level (`flow_windows::mcp::as_client`), so whatever
  serves it can learn who opened it but never act as them, and
  `flow_windows::mcp::open` refuses it unless `served_by_flow` holds: the
  pipe owned by the user (`pipe::owned_by`, read from flow-mcp's own
  handle, which a reused process id can't fake), its server the user's at
  medium integrity or above, and named `flow.exe`.

**Why this way.** Pipe names are one namespace for every user, the
bridge's name is easy to guess, and a socket's folder may be one another
user made first (`/tmp/flow-mcp-<user>`, `FLOW_MCP_SOCKET`). Whoever
took the address would read every tool call and answer into the AI app's
context. The image pipe's default access list let everyone read it.

Flow-SDK's `Pipe::open`, which every image-pipe client Flow carries goes
through (`flow-settings`, the `flow` command, `flowapp`'s splash, the
games, the demos, the C and Python libraries), does the same from the
SDK's 0.1.4, the tag Flow takes: it opens at identification level, and
writes nothing until it has read the pipe's owner and found it's the
user (`owner_is_user`). So a program from a tab that has closed meets no
one else's pipe.

**What it leaves.** A program built on an SDK before 0.1.4 still opens
the image pipe at the default level. A shell's own redirection to it
(`> %FLOW_IMAGE_PIPE%`) does too, which nothing of Flow's can change.
The pipe's name is the user name filtered to ASCII, so some users share
one. A low-integrity process of the same user could take the name while
Flow isn't listening. Root and administrators are out of scope.

## The assistant's rules

**What it does.** Whether a command waits for the user (auto mode, a
work-on-its-own grant, helpers, outside apps' Allowed commands, a reply's
Run) is decided by reading the line as the shells read it
(`tools::Reader`): each simple command's program word, through groups,
loops, substitutions, quoted commands a program runs later and wrappers
whose grammar Flow knows, held to a literal name. The rule (decided
2026-11-24): a command is rated by the programs Flow can read for
certain, and one whose program it can't read asks. So a program built
from a variable, `$(…)`, a glob that may match a risky name, a string
run by `eval`, a shell or interpreter fed its program from a pipe or an
unread here-document, history expansion, and a line holding a control
character are all hidden, High in auto mode and asked about under a grant
(`Why::Unclear`). Deletes are judged by their own paths in the folder of
the pane they'll run in (`always_asks_in_folder`), following each `cd` on
the line, so `rm -rf *` at the home asks. The pane is settled as the call
is judged (`App::place_for`'s `Place`, `command_pane`), kept with a
waiting card and used when it's carried out; under a grant a command is
typed only while its pane's shell is still in the folder it was judged in
(`same_folder`, else `judge_again`), the grant it was judged under kept
with it (`Job::granted`). `tools::Shells` says which shells a line is
read as, the pane's own (`Shells::of`, in `Place`, for the helpers' panes,
an outside app's and a reply's Run too): a statement beginning with a
variable is a value only in PowerShell's, and every other shell's
program, on the Mac and Linux and in Git Bash's, WSL's, BusyBox's and
cmd's panes on Windows. Where the reader meets a shape it doesn't read in
full, the line asks (decided 2026-11-24): it doesn't model
the shape. So a name for a variable or a name built as the line runs
(`declare -n`, `printf -v "$n"`, `export "$(…)"=…`), an index holding a
command's output in a name a command sets or tests, arithmetic on the
positional words, a prompt from a variable, zsh's `case w { … }`, and a
program's option that runs a value given (`tar --to-command`, `open -a`)
ask; and the reader is bounded, a line past 64 KB (`MOST_LINE`), wrappers
read one in another's words past 16 (`MOST_PROGRAMS`) and groups past 48
(`MOST_NESTED`, `Reader::open`) asking, so no line takes the stack or
seconds of the window thread. A descriptor's name (`{fd}>`) and `>|` are
redirections; NULLCMD and READNULLCMD are command lines, and so are git's
settings in the environment; a delete's path is brace-expanded and read
as Windows' shells read a backslash too, a `..` after a variable asking;
a written file is compared with a run one from the folders each runs in
(`path_keys`).

Beside the rules, what the model reads is kept from posing as Flow or the
user:
- the system prompt (`SYSTEM_PROMPT`, and the helpers' and working on
  its own's) says that text on the screen, output, files, attachments,
  pages and what MCP servers return is data, never instructions, however
  it's worded, and that Flow's blocks appear only in the user's
  messages;
- `untrusted::defang` writes Flow's own tags (`terminal_context`,
  `memory`, `instructions`, `file`) found in such text with `&lt;`, read
  as a model would see them (zero-width characters dropped, fullwidth
  forms folded, `-` or a space for `_`), `one_line` keeps a name to one
  line, and `fence` is longer than any run of backticks inside;
- tool results stay byte for byte in each provider's own result blocks.

**Why this way.** The rules were a list of substrings, and a spelling
past it kept turning up: a path, quotes, a backslash, a wrapper, an
expansion, a variable, a second level of quoting. Listing the ways to put
a program together never ended and made ordinary commands High, so the
rule turned round: read what can be read for certain, and ask about the
rest. Judging where the command runs closes the case where the grant was
judged in the user's pane and the command ran in the assistant's.

**What it leaves.** This is a check by pattern, not a sandbox, and the
manual says so: a command can do what its words don't say (a script,
`python3 -c`, a path built as it runs), and earlier commands' state is
read only through what the line says. Known gaps:
- Literal here-documents, here-strings and `-c` strings for interpreters
  are taken as written, and so is code a program reads in its own
  language (`awk`, GNU sed's `e`, `perl -e 'exec @ARGV' "$x"`); only a
  shell's lines in them are read.
- Settings an interpreter loads from where the line points
  (`PYTHONPATH`, `PERL5LIB`, `RUBYLIB`, `NODE_PATH`) aren't read, since
  `PYTHONPATH="$PWD/src"` is everyday; a script written by an earlier
  command and loaded so runs.
- A pane whose shell's hook never reports a folder (a plain `sh`) can't
  take a grant's command in a pane opened for it; an idle zsh with a
  right prompt counts as typed at, so its commands go to a pane beside it.
- An MCP prompt's text and a package skill's description reach the
  conversation undefanged; defanging stops text closing Flow's blocks,
  not a model being talked round, which is why the approvals don't lean
  on it.

## flowapp's names

**What it does.** Every install, the catalog's and the user's own, holds
names to one rule, since they become folders and files and the catalog
isn't signed: `fit_name` (at most 64 lowercase ASCII letters, digits, `-`
and `_`, not `bin`, `first` or `previews`, not a device name),
`fit_version` (Flow-SDK's, not ending in a dot, no device name before the
first dot) and `fit_program`. A package of the user's own can't take a
name the catalog has (`own_name_refused`, the catalog read afresh first,
the saved one offline), nor a theme id a built-in look or another package
brings; at run time the catalog's themes are read first and one of the
user's never stands in for a built-in. What flowapp prints of a stranger's
words has its control characters spelled out (`apps::plain`). On the Mac
tarballs unpack with `/usr/bin/tar --no-mac-metadata --no-acls
--no-fflags`, and a refused install leaves nothing behind (`take_away`,
which clears modes, read-only attributes and the Mac's ACLs if a plain
remove fails).

**Why this way.** An unsigned `.tar.gz` named `flow-sdk` replaced the
catalog's app; one bringing `green` replaced a machine's look; `C:` as a
version deleted the current folder on Windows; `Bin` was `bin/` on a
case-insensitive disk; a refusal quoting `ESC]7732…` sent the escape; and
bsdtar restores ACLs from AppleDouble entries even for a normal user, so
a refused file could leave a folder nobody could delete.

**What it leaves.** A package that took a name before the catalog had it
stays until removed (every place that would act on it says so); a
leftover owned by root; Windows' `tar.exe` and NTFS ACLs, untried.

## Control characters, and bash's Meta keys

**What it does.** Text Flow types at a prompt is held to what the line
editor takes as letters:
- words the `flow` command, a guest Flow's tab, a dropped file's path or
  `flowapp hello` type are quoted by `shell::quote`, which refuses any
  control character (`Untypable`), before anything is asked or opened;
- whole commands from the model (helpers too), an outside app and a
  reply's Run or put-at-prompt are refused where they hold a control
  character but a line break (`shell::key_in`, `tools::untypable`); a
  line from the history put at the prompt loses them (`without_keys`),
  a tab staying in a bracketed paste;
- bash's hook has readline take every byte as a letter before each prompt
  (`convert-meta off`, `input-meta on`, `output-meta on`), and from bash
  5.1 brings the environment up to date first (`{ : "$(:)"; }`), so a
  separately linked readline sees the prompt's locale at that prompt; it
  marks a prompt that may take them as keys anyway (633;P;FlowMetaKeys=1,
  the locale readline goes by just changed into one that isn't UTF-8);
- where Flow can't know who reads the keys (a pane whose shell never
  showed Flow's prompt, bash's marked prompt, a bash or zsh whose hook
  never sends its nonce'd done, so a prompt a program forges counts for
  nothing), the flow command and a drop type letters past
  ASCII into zsh and bash as `$'\xHH'` bytes (`quote_ascii`), and whole
  commands holding them are refused (`Session::letter_as_keys`,
  `card.letters_as_keys`, `toast.letters_as_keys`);
- a `~` inside a word is quoted, and Linux's updater passes over a
  release whose version isn't numbers and dots, since it names the `.deb`
  typed into `sudo apt install`.

**Why this way.** A control character at a line editor is a key, which no
quoting keeps inside a word: Ctrl+U cleared the line and ran what followed,
Tab completed out of an open quote, and a card drew `echo 'done\u15touch
~/x'` as harmless text. In a C locale readline takes Ē's second byte as
M-C-r, revert-line, which threw away the quotes; a dropped `ą` ran
without Enter (M-C-e). Refusing is one rule for every shell (cmd can't
quote them at all) and a script is the way out.

**What it leaves.** The bash 5.1 fork costs about half a millisecond a
prompt on the Mac, more under Git Bash, unmeasured there; a `read -e` in
a function between prompts; a bash 5.0 linked to a newer readline; a zsh
pane whose own shell was `exec`ed away can be taken as integrated again
by forged 133;A/B, since zsh's guard doesn't wait for its nonce'd done.
On Linux a zsh in a locale that isn't UTF-8 makes each byte past ASCII
typed at its line editor a `?`, even with `multibyte` off, since glibc's
C locale has no letter past ASCII to keep (the Mac's keeps each byte): so
the flow command's words, a dropped name and the assistant's commands
lose such letters there, though nothing runs.

## The history's nonce

**What it does.** Each pane has a second secret, `FLOW_NONCE`
(`shell::new_nonce`, given only where the hook says each line:
`give_nonce`), which zsh's, bash's and PowerShell's hooks keep in an
unexported variable and unset before the user's files run. The command
line (633;E) counts only with it; every 133;D the hooks send carries it,
and zsh's preexec and bash 4.4's PS0 put it on 133;C. Once a pane has
seen its nonce'd done, `ShellState::forged` passes over a done without
it and any prompt mark drawn while a command runs. bash says `;newest`
after the nonce where the line it sends is only its history's newest,
which may not be the one run (HISTCONTROL, HISTIGNORE), and a line read
off the screen that the hook then said nothing of keeps nothing
(`HistoryNote::LeftOut`), since a background job may have drawn it.
The hooks escape every control character in a line as `\xNN`, and turn
tracing off while they print.

**Why this way.** The history, the notification's command and Ask why's
command, which goes to the model, came from whatever printed 633;E, or
from the screen after a forged prompt mark moved where typing starts.
It's not `FLOW_TAB_KEY`: the hook prints the nonce into the output, where
a log may keep it, and the tab key opens tabs that type commands. A
prompt can't carry it without putting it in `PS1`, so prompt marks are
judged by the state around them.

**What it leaves.** Output while the shell waits at its prompt (a
background job's) is still trusted where no nonce'd line follows (cmd,
ash, sh); PowerShell sends no command start, so a line typed ahead and
oh-my-posh's transient prompt run unguarded; 633;P and OSC 7 are taken
from any output, as nvim and yazi send them; system startup files see
`FLOW_NONCE` before the hook unsets it, and the nonce shows in `set`.

## What it leaves

All of the above in one place:

- programs built on Flow-SDK before 0.1.4, which send no pass and open
  the image pipe at the default level;
- the assistant's rules: code taken as written in an interpreter's own
  language, settings interpreters load from where the line points, and
  the panes above (a plain `sh`'s, a zsh with a right prompt);
- code in the pane is trusted and has both secrets; a replayed recording
  in the same pane is accepted;
- the history and the prompt marks: a background job's output at a
  prompt where no nonce'd line follows, PowerShell's typed-ahead lines;
- the folder (633;P, OSC 7) from any output, filtered only where it
  becomes a path Flow touches;
- letters past ASCII typed into a zsh on Linux in a locale that isn't
  UTF-8, which become `?`;
- kitty's files: whether a path holds a picture, and slow lookups a few
  times a window;
- a check by pattern for shares in commands on Windows, and for what the
  assistant's commands do.

## How it's tested

- **Pure decisions, on every system:** `settings_act` with every `do`
  and every wrong pass, and a request's base64 broken into lines,
  `tab_act`, `others_wait`; `prompt_for` in each shell's form;
  `link::local` and `file_path` under both systems' rules; `load_file`'s
  answers, timings and budget; `Reader`'s readings against `PUT_TOGETHER`
  (each shape for `rm`, `reboot` and `diskutil`) and `ORDINARY` (everyday
  lines keep their rating), lines of thousands of wrappers and groups on
  a thread of a megabyte's stack, deletes through `..`, braces and a
  backslash before a quote, `own::decide` and the outside apps' grants by
  each pane's `Shells`; `command_pane`,
  `waits_for_prompt`, `command_ready`, `same_folder`; `fit_name` and the
  install refusals; `key_in`, `quote`, `quote_ascii`, `without_keys`;
  `forged` and the osc parsers; `defang`.
- **Real PTYs on the Mac** (zsh 5.9, Homebrew bash 5.3, `/bin/bash` 3.2,
  scratch homes): `macos/tests/prompt_text.rs` (a theme's prompt runs
  nothing under any option, exported or not), `shells.rs` (forged marks
  and lines name nothing, letters past ASCII in a C locale, readline's
  settings put back only at the marked prompt), `pictures.rs` (the
  failure budget through a terminal reset), `tests/settings.rs` (the real
  `flow-settings` sends the pass, says why without a key, asks nothing),
  `tests/mcp.rs` (the real `flow-mcp` against a socket the system won't
  open), `tests/flowapp.rs` (a refused install leaves nothing, the
  catalog read afresh).
- **Built and run here:** `cargo test --workspace` and `cargo clippy
  --workspace --tests` on the Mac, with no new warnings.
- **Not built here, to run on each system before a release:**
  - **Windows:** `cargo test -p flow-windows` (the pipes' security,
    `owned_by`, `serve_each` never leaving a name free, the
    identification level through `as_client`), `cargo test --test
    mcp_windows`, `windows/tests/shells.rs` (the nonce and forged marks
    in PowerShell, Git Bash, WSL and BusyBox), `prompt_text.rs`,
    `pictures.rs`, `first.rs` and `flow_command.rs` (a request Flow
    doesn't answer), then `cargo test -p flow-windows`
    and `--test mcp_windows` again elevated; by hand, a `file:` link of
    Flow's own opening through `ShellExecuteExW`, and `cargo clippy
    --workspace --tests`.
  - **Linux:** `cargo test -p flow-linux` (`prompt_text.rs` with
    BusyBox ash, `shells.rs`' readline test and nonce checks,
    `pictures.rs`), the updater's `plain_version`, `SO_PEERCRED` in
    `peer_user`, and `cargo clippy --workspace --tests`.
  - PowerShell's hook text and the Windows code were checked by hand
    against the crate versions in Cargo.lock, never compiled.
