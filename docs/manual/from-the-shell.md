[← Flow's manual](../../README.md#documentation)

# From the shell

Two programs on the PATH of every Flow tab reach the Flow they run in:
`flow-settings` for its settings, and the `flow` command for a tab of its own.

## Settings from the shell

In a Flow tab, `flow-settings` sees and changes Flow's settings from the
shell, by the names `config.toml` gives them (see [Configuration](configuration.md)):

```bash
flow-settings                       # every setting a program may see, with its value
flow-settings get fx.style          # one of them
flow-settings set fx.style rain     # changes one: rain, 16, true, "#00ff00", '["VT323", "Menlo"]'
flow-settings reset font.size       # puts one back to its default (reset all: every one)
flow-settings reset terminal        # starts this tab afresh: its screen, scrollback and modes
flow-settings restart               # restarts Flow, as Settings' Restart does
flow-settings --help                # this, and what each setting takes, as Flow says
```

Flow applies a change at once, as Settings does, saves it and says so in a
toast, so nothing changes unseen. As Settings opened on the tab does, the
look's settings (the theme, liquid, glass, CRT, colors, cursor and fonts)
are the tab's it runs in, which keeps them when Flow opens it again, and a
theme comes with its colors, fonts and CRT; the rest are Flow's, for
every tab; a value of the wrong kind, one Settings
doesn't offer (a style it has no name for, a number past a slider's ends,
a color not written `#RRGGBB`), or a name it doesn't know, it refuses and
says why. `--help` lists what each setting takes where Settings offers
choices or a range (`fx.style`: clouds, water, oil, rain or none;
`fx.rain`: a number from 0 to 1), which Flow answers itself (a
`describe` question), so it's never out of date. Programs may change the look (the
liquid, glass, CRT, colors, cursor and fonts), sound, the window, the
terminal's settings and the theme worn, but never the assistant's, nor
your own theme or the themes you saved (whose prompts your shell shows
before each command you type; nor which of them is chosen, nor choose
one), nor show the computer's real numbers: those are Settings' alone.
`restart` restarts at once while every other pane waits at its prompt,
the other panes of its own tab among them; otherwise Flow doesn't
restart, says so in a toast, and `flow-settings` says why and exits 1:
restarting then is yours to do, with Settings' Restart, About's Restart
Flow, or Restart Flow from the Mac's Flow menu or the palette (which asks
again while something runs). A program asking again is never taken for
your yes. Decimals list as they were set (`crt.decay = 0.06`). It asks
Flow with an escape of its own (OSC 7732), which any program can send
through the SDK (`flow_sdk::settings`, and from C and Python
`flow_settings_ask` and `flow_sdk.settings`). Without a `FLOW_TAB_KEY`
(outside Flow's tabs, or under `sudo` or `env -i`) it says so at once and
asks nothing; with one that nothing answers (tmux, another pane's key, a
terminal started from a Flow tab), it says so after its wait (below). It
asks the tab itself, not its own input and output, so what it prints can
be piped, sent to a file or kept in a variable (`style=$(flow-settings
get fx.style)`). It sits with `flow-help` and the games at the end of the
tabs' PATH.

Anything printed in a tab could ask what `flow-settings` asks, a file
being `cat`ed or a remote host's output over ssh among it, and the
answers would be typed back into the tab. So from 0.1.46 Flow answers a
request, whatever it asks (`list` and `describe` too), only when it
carries the asking pane's own secret, `FLOW_TAB_KEY`, as its `pass`, as
[the `flow` command](#a-tab-from-a-tab-the-flow-command)'s requests must.
Without it, or with a wrong one, Flow doesn't answer at all, not even to
say no: an answer is typed into the shell's input, where a line editor in
vi mode would read its letters as commands. So `flow-settings`, which
sends the secret from its own environment, says why itself. Where there's
none (run through `sudo`, which leaves it out, with `env -i`, or over
ssh), it asks nothing and says so at once; where its secret goes
unanswered, it says after its wait that tmux or screen may be in the way,
which don't pass Flow's questions on, or that the secret may be another's
(another pane's, or one an ssh session or a script brought along), or
that this isn't a Flow tab. Both exit 2. From the SDK's 0.1.4, its `ask`
(Rust's `flow_sdk::settings::ask`, C's `flow_settings_ask`, Python's
`flow_sdk.settings.ask` and the helpers over it) puts the secret in from
`FLOW_TAB_KEY` where a request has none, as `{"do": "get", "key":
"fx.style", "pass": "<FLOW_TAB_KEY>"}`. Where nothing answers, a program
should say why as `flow-settings` does. Flow reads a request's base64 as
`base64 -d` does, line breaks and all, so one GNU's `base64` wrapped
reads too. A program built on an SDK before 0.1.4 sends no secret, so
Flow doesn't answer it until it's built again.
Flow's assistant and outside apps change settings through their own
`flow_settings` tool, which needs no secret but can't restart Flow or
reset a terminal.

## A tab from a tab: the flow command

In a Flow tab, `flow` opens a new tab in the same Flow and types a
command there, so a long-running one (a tunnel, a server, a build) goes
on in a tab of its own while you go on in this one (from 0.1.43):

```bash
flow ssh -N -L 7445:127.0.0.1:7445 me@host   # a tab of its own for the tunnel
flow                                         # a tab here, at its prompt
flow --split npm run dev                     # a pane beside this one instead
flow --behind cargo build --release          # opened without coming to the front
flow --close make test                       # closed once it ends well; a failure stays to be read
flow --shell bash --theme green htop         # another shell or theme, by id or name
flow --window ./serve.sh                     # a Flow of its own for it
flow --help
```

The new tab is like the one `flow` ran in: the same shell, the same theme
and that folder, right after it in the tab bar and in front. The command
is typed once its shell waits at its prompt, as if you'd typed it: it
goes into the command history, and the shell stays when it ends. A
theme's start screen comes first, as in any new tab, and one that waits
for a key goes on by itself. Each word goes as the shell you typed it in
gave it to `flow`, its variables, globs and quotes already expanded, and
is quoted for the new shell so nothing in it is expanded again, cmd's
`%NAME%` included. A word holding a control character (a tab, a line
break, Esc, Ctrl+C and the like) is refused, and `flow` says which: typed
at a prompt it would be a key, not a letter, which could end the word's
quotes or its line and run what came after it as a command of its own (a
file's name can hold one). Put a command that needs one in a script, and
run that. Letters past ASCII go as letters, so a Japanese file's name
shows at the prompt and in the history as it's written: bash's line
editor in a locale that isn't UTF-8 (`LC_ALL=C`, which scripts often set)
would take each of their bytes as a key, which could throw away the
word's quotes as Ctrl+U would, but Flow's bash hook has it take them as
letters (see [Shells](shells.md)). Where the new shell hasn't shown
Flow's prompt when the command is typed (another its startup files start
in its place, which Flow's hook never reached), or bash's hook says its
prompt may take them as keys (its locale has just changed) or can't run
before its prompts (a read-only `PROMPT_COMMAND`), they go into zsh and
bash as their bytes, `$'\xc3\xbc'` for ü, beside the quotes of the rest,
which the program gets as the word it was. A word holding a `~` is quoted, since
bash expands one after a `=` or a `:` even in an argument
(`--prefix=~/x`). The first word that isn't an
option begins the command; `--` ends the options. `flow` returns once the
tab is open, before its command runs; it can't say how the command ends.
`--close` closes the tab (or pane) once the command ends with 0; a
failure, Ctrl+C among them, leaves it open, and cmd, which can't say,
closes whenever the command ends.

`flow --window`, or `flow` run outside Flow's tabs (another terminal, a
script, a shortcut, Windows' Run box), starts a new Flow with that one
tab, a guest beside the Flow you have: it opens none of your saved tabs
and saves none, takes no hotkey, doesn't listen for outside apps, checks
for no updates, logs to `flow-window.log`, and quits when its last tab
closes.

Anything printed in a tab could ask what `flow` asks, a file being `cat`ed
or a remote host's output over ssh among it. So each pane has a secret,
`FLOW_TAB_KEY`, that only programs started in it have (it doesn't cross
ssh unless your `SendEnv` names it), and Flow opens a tab, as it answers
`flow-settings`, only for a request carrying the asking pane's own; any
other it doesn't answer at all. `flow` asks as `flow-settings`
does (OSC 7732, down the tab's pipe on Windows), so where nothing
answers, it says Flow didn't, and that the secret it sent may be another
pane's, or that tmux or screen, which don't pass it on, is in the way
(exit 2). It's
`Contents/Helpers/flow` on the Mac and `games\flow.exe` on Windows, at the
end of the tabs' PATH, so another `flow` earlier on the PATH (the
JavaScript type checker's) runs instead. On Linux `bin/flow` is Flow
itself, which reads the same arguments first: a bare `flow` in a Flow tab
opens a tab, and from anywhere else starts Flow as before.
