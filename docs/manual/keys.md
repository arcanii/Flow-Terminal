[← Flow's manual](../../README.md#documentation)

# Flow's keys

These are Flow's keys as it has them. Any of them can be changed, in
Settings' **Keys** ([Changing Flow's keys](#changing-flows-keys)), and
About's **Keys** link, or `flow-help keys` in a tab, lists them as
they're bound.

| Windows and Linux | macOS | What it does |
| --- | --- | --- |
| `Ctrl+,` or the ⚙ button | `⌘,` or the ⚙ button | Settings: look, font, assistant, token cap, default shell |
| `Ctrl+Space` | `⌃Space` | Open / focus / close the assistant panel |
| `Win+.` (Windows) | `⌃⌘Space` or `fn E` | The system's emoji picker: what you pick is typed at the prompt or in the assistant's input |
| `Enter` (panel) | `Enter` (panel) | Send your request; with a command waiting, run it |
| `Shift+Enter` (panel) | `⇧Enter` or `⌥Enter` (panel) | A new line in your request |
| `↑` / `↓` (panel) | same | Between the input's rows; past its first or last, the requests you sent before, then back to what you were typing |
| click, drag, `Shift`+arrows (panel) | same | Put the caret there, or select; double-click a word, triple-click a line |
| `Ctrl+←/→`, `Home` / `End`, `Ctrl+Home` / `Ctrl+End` (panel) | `⌥←/→`, `⌘←/→`, `⌘↑/↓` (panel) | By word, to the row's ends, to the input's ends |
| `Ctrl+A` / `Ctrl+C` / `Ctrl+X` / `Ctrl+V` (panel) | `⌘A` / `⌘C` / `⌘X` / `⌘V` (panel) | Select all, copy, cut and paste in the input |
| `Ctrl+Z` / `Ctrl+Y` (panel) | `⌘Z` / `⇧⌘Z` (panel) | Undo / redo in the input |
| `E` / `S` (panel, command waiting) | same | Edit the command first / skip it |
| typing + `Enter` while a command waits | same | Decline it and tell Claude why |
| `Tab` (panel) | `Tab` (panel) | Switch guide ↔ auto mode |
| `Esc` (panel) | `Esc` (panel) | Stop Claude, let go of a selection, clear the input, or go back to the terminal |
| `Ctrl+L` (panel) | `⌃L` (panel) | Start a new conversation |
| `Ctrl+Shift+T` | `⌘T` | New tab with the default shell (+ in the title bar too; … beside it picks the shell and the theme) |
| `Ctrl+Shift+W` | `⌘W` | Close the pane in front, or the tab (the last one closes Flow); where something runs in it, a toast says what, and the key again within a few seconds closes it |
| double-click a tab, or none at first ([Keys](settings.md)) | double-click a tab, or none at first, **Shell → Rename Tab** | [Name the tab](terminal.md#naming-a-tab) in front, typed in its pill: Enter keeps the name, Esc puts back the one before, and an empty name gives the tab its shell's again |
| `Ctrl+Shift+S` | `⇧⌘S`, **View → Tab Sidebar** | The [tab sidebar](terminal.md#the-tab-sidebar) out over the terminal, or away again; its own keys are on that page |
| none at first ([Keys](settings.md)) | none at first, **Shell → New Group** | Put the tab in front into a [group](terminal.md#groups-of-tabs) of its own (the palette moves it into another, or out) |
| `Ctrl+Shift+1..9` | `⌘1`…`⌘9` | Show tab 1–9 |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | `⌃Tab` / `⌃⇧Tab`, `⇧⌘]` / `⇧⌘[` | Next / previous tab |
| `Ctrl+Shift+C` / `Ctrl+Shift+V` | `⌘C` / `⌘V` | Copy / paste (on Windows and Linux, `Ctrl+C` and `Ctrl+Insert` copy when text is selected, `Shift+Insert` pastes, and right-click copies or pastes) |
| | `⌘A` / `⌘K` | Select all / clear the scrollback, keeping the prompt line |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | `⌘+` / `⌘-` / `⌘0` | Font size (a toast says what the window then holds) |
| `Ctrl+Shift+K` | `⇧⌘K` | Change the API key |
| `Shift+PageUp/PageDown`, mouse wheel | `PageUp/PageDown` (`fn+↑/↓`), mouse wheel | Scroll back |
| `Ctrl+Shift+End` / `Ctrl+Shift+Home` | `⌘End` / `⌘Home` (`⌘fn→` / `⌘fn←`) | Back to the bottom / to the top of the scrollback (also [the pill and the scrollbar](terminal.md#scrolling-back)) |
| `Ctrl+Shift+↑` / `Ctrl+Shift+↓` | `⌘↑` / `⌘↓` | The command before / after in the scrollback ([marked on the scrollbar](terminal.md#scrolling-back)) |
| `Shift` + click, drag or wheel | `⇧` + click, drag or wheel | Select, open the menu or scroll back over a program that takes the mouse |
| `Ctrl` + click a link | `⌘` + click a link | Open a web or mail link a program wrote (OSC 8), or an address in the text, which the pointer underlines; a link to a file on this computer shows it in its folder |
| `Ctrl+Shift+R` | `⌘R` | Search the command history |
| `Ctrl+Shift+F`, then `F3` / `Shift+F3` | `⌘F`, then `⌘G` / `⇧⌘G` | Find in the scrollback, then the match before or after (also ↑ and ↓ in the find bar; `Alt+R` / `⌥⌘R` there looks for a pattern) |
| `Ctrl+Shift+P` | `⇧⌘P` | The command palette: every command, setting, theme, page of help and shell, found by typing |
| `Ctrl+Shift+D` / `Ctrl+Alt+Shift+D` | `⌘D` / `⇧⌘D` | Split the pane in front side by side / one above the other |
| `Ctrl+Alt` + arrows | `⌥⌘` + arrows | Move between a split tab's panes (a click on one does too) |
| `Ctrl+Shift+E` | `⇧⌘E` | Efficient mode on or off (also **🌱e-mode** in the title bar) |
| none at first ([Keys](settings.md)) | none at first, **Window → Float on Top** | Keep Flow's window on top of every other, or not (also **on-top** in the title bar, and Settings' **Always on top**) |
| `Ctrl+Alt+Shift+R` (Windows), `Ctrl+Alt+Shift+V` (Linux) | `⇧⌘R` | [Record the terminal](terminal.md#recording-the-terminal) to an animated WebP, and stop (also the clock in the title bar while it runs) |
| `Ctrl+Shift+B` | `⇧⌘B` | Restart the tab in front: its shell ends and starts again in its place, with its theme, the other tabs left as they are (asking first while a program runs in it); Settings' **Restart** restarts all of Flow |
| `F1` over one of Flow's screens, `Ctrl+Shift+?` anywhere | `F1` over one of Flow's screens, `⌘?` anywhere | Help for the screen in front |
| click the Flow logo | click the Flow logo | About Flow |

On macOS these commands are also in the menu bar (Flow, Shell, Edit, View,
Assistant, Window, Help), and right-click or Control-click opens a menu with Copy,
Paste, Select All, Clear Buffer and Ask Claude (Ask GPT or Ask Gemini, with that
provider chosen). The usual text-editing keys work
at the prompt: `⌥←` / `⌥→` move by word, `⌘←` / `⌘→` jump to the start or end
of the line, and `⌘⌫` deletes back to the start. Option types characters such
as ƒ and ©; turn on **Option as Meta** in Settings to have it send Esc+key for
shell and editor shortcuts instead.

On Linux, text you select with the mouse is also the primary selection, as in
other Linux programs: a middle click pastes it, in Flow or elsewhere, and a
middle click in Flow pastes what was last selected anywhere. Flow draws its
own title bar, so drag it to move the window, double-click it to maximize,
and, on Wayland, right-click it for the window manager's menu (always on top,
move to another workspace…).

Programs that ask for the mouse, such as games, editors and `htop`, get its
clicks, drags and wheel over the terminal, as in xterm (with any of
`?1000h`, `?1002h` and `?1003h`, in SGR form with `?1006h`). Over them the
pointer is an arrow rather than a text caret, and Shift gives the mouse back to
Flow: Shift-drag selects, Shift-right-click opens the menu, and Shift with the
wheel scrolls back. A button pressed over the program comes up as the
program's wherever the pointer goes. Turn off **Mouse reporting** in Settings
to keep the mouse for Flow alone.

At the shell's prompt, a click on the line you're typing puts the cursor
there, as the arrow keys would: Flow sends the shell ← or → for each
letter between. It's for that line alone (its rows, where a long command
wraps), and only while the shell waits at its prompt, so a click on the
prompt goes to the input's start, one past what's typed to its end, and a
click anywhere else, or a drag, selects as always. A suggestion drawn after
the cursor (zsh-autosuggestions') is taken where a click lands in it, as →
takes it. Turn off **Click to move the cursor** in Settings
(`shell.click_moves_cursor`) to keep clicks for selecting.

Programs can write links (OSC 8: `ESC ] 8 ; ; <address> ESC \`, the words,
then `ESC ] 8 ; ; ESC \`), as `flowapp`'s tables do with each app's name.
With the pointer over one, Flow underlines it and shows where it goes at
the terminal's foot, and ⌘-click (Ctrl+click on Windows and Linux) opens it
in your browser or mail app, even over a program that takes the mouse. A
program can put any words over a link, so Flow opens only `http`, `https`
and `mailto` links, written as addresses are: one holding a space, a
control character or a backslash isn't one, and a quote, `<`, `>`, `{`,
`}`, `|`, `^` or `` ` `` in one is sent as a browser sends it (`%7C` for
`|`), since the system's opener or your browser could read them as more
than the address. A `mailto` link may fill in only `to`, `cc`, `bcc`,
`subject`, `body` and `in-reply-to`, never a file to attach. A `file:`
link to a file on this computer, as
`ls --hyperlink` and Flow-Utils' `ls`, `tree`, `du` and `df` write them, is
never opened: ⌘-click shows the file selected in its folder (the Finder,
Explorer, or the file manager) and runs nothing. One to another computer
(`file:////server/share/…`, or any path beginning with two slashes or
backslashes) isn't a link, and on Windows only one on a drive
(`file:///C:/…`) is: on Windows, merely looking at a file on another
computer hands it your sign-in. A Flow running as administrator (or root)
follows no link: what it opened would run as administrator too. Other
links stay plain
text. An address in the text itself that no program marked is a link too,
underlined under the pointer and opened the same way: an `http://`,
`https://`, `mailto:` or `file://` one, without the full stop or bracket
of the sentence around it, its wrapped rows and all.

Programs can make a line double width (`ESC # 6`, DECDWL) or double height
(`ESC # 3` on the line for its top half and `ESC # 4` on the next for its
bottom, DECDHL), as on DEC's terminals, and single again with `ESC # 5`. A
double line shows the first half of its columns, each twice as wide, and the
cursor, selecting and the clicks programs hear go by those. As on DEC's
terminals, a line made double loses what was written past its half, so
made single again it's blank there (xterm keeps it hidden and shows it
again). Its right margin
is there, as on DEC's terminals and in xterm: text wraps at the last of those
columns (or, with autowrap off, writes over it), the cursor goes no further,
and inserting, deleting and erasing characters work within them. Pictures
and 3D models on it are drawn as its text is, each cell twice as wide, and
each half of a double height showing its half, so a picture over rows of
different sizes doesn't line up across them; a program shows a picture
double height as it does text, with the same cells on both halves (the
`ESC # 3` line's and the `ESC # 4` line's). It scrolls and goes into the
scrollback as it is, and copying it copies its text: text that wrapped at
its half copies as one line with what it wrapped onto, as a single line's
does, and so does a command typed there for the history. Erasing within it
keeps its size, while a clear, or an erase from its start down, makes it
single. Resizing the window rewraps text that wrapped at a double line's
half together with what it wrapped onto, as one line by the double line's
width: narrower, the letters it no longer has room for go on at the start
of the next line (a wide letter whole), and wider, it takes back as many as
it now has room for, and a line left empty goes. So it reads, copies and
goes into the history as it did, the cursor stays by its letter, and a
picture cut at the double line's edge grows back as the window widens, as
on a single line. A line a program made double without wrapping text at
its half, such as a title or the two halves of a double height, isn't
rewrapped, as xterm rewraps no line: narrower, it shows fewer of its
letters and keeps the rest hidden (cutting off any past the new width),
and wider it shows them again. A single line that wrapped onto a double
one rewraps on its own and still runs on into it, and so do the letters a
narrower window moves on from a double line that wrapped onto another. The
alternate screen, which programs draw again themselves, is cut rather than
rewrapped, double lines and all.
Block graphics and box drawing Flow draws itself, from the cell's own
pixels, so that they join up whatever the font: the block elements (halves,
quarters and eighths, such as `▀`, `▄`, `█`, `▌` and `▚`, but not the
shades) and teletext's mosaics as Unicode has them, the sextants
(U+1FB00–U+1FB3B) and the separated sextants (U+1CE51–U+1CE8F); and every
line of box drawing (U+2500–U+257F: light, heavy and double, each junction
between them, the dashed lines, the rounded corners, the half lines and
the diagonals), each stroke meeting the next cell's at the same pixels, a
light one as heavy as the font's letters (an eighth of a cell at least),
and bending with the liquid as the text does. Most of the themes' fonts
have no box drawing, and a frame drawn in whatever font had it came out as
dashes and brackets. The squares are drawn too, black and white in four
sizes: `■` and `□` as big as the font's x-height, then `◼` `◻`, `◾` `◽`
and `▪` `▫`, each white one its black one's outline, in the middle of the
cell and the same size in every cell, since a fallback font's could be
wider than the cell, and a progress bar such as `[■■■□□□]` half drawn by
Flow and half by another font wouldn't match.

A letter's combining marks are drawn on it, shaped with it in its cell:
an accent written as a mark (`é` as `e` and U+0301), and the vowel signs
and viramas of Hindi and the other scripts of India and around it. On the
Mac, a spacing mark such as Devanagari's vowel sign `ा` goes in the cell
of the letter before it, squeezed in beside it, since the Mac's shells
measure it as taking no room: given a cell of its own, as alacritty and
Linux give it, a long command with Hindi in it lost letters as it was
typed in zsh and bash and came back garbled with ↑. On Linux, whose shells
give it a cell, it keeps one. In zsh, Flow turns on its COMBINING_CHARS
option before your own files, as macOS does, so the line you type shows a
mark on its letter rather than as `<0301>`; `unsetopt combining_chars` in
your `.zshrc` turns it off. Each cell is shaped by itself, so a
conjunct across two cells (`स्त`) shows its virama rather than joining. A
shell on another computer, over ssh, measures as that one does: from a
Mac, a Linux shell's line with such a mark in it can still come back
garbled.

Japanese, Chinese and Korean are typed through the system's input method: at
the prompt, in the assistant's input and in a CRT preset's name. What it's
composing shows underlined where it will go, more heavily under the part being
converted, with its candidates beside it, and only what you commit reaches the
shell. Settings' rows and the key dialog take keys as they are, so the input
method is off there. On the Mac a held letter repeats, as in Terminal, instead
of opening the accent picker; `defaults write com.arcanii.Flow
ApplePressAndHoldEnabled -bool true` brings the picker back. If `⌃Space`
switches input sources on your Mac, macOS takes it before Flow sees it, so open
the assistant from the title bar or the Assistant menu.

What the assistant writes can be copied and run from its panel. Under each
block of code in a reply are **Copy**, which copies it, and for commands
(a block with no language named, or a shell's), **Run**, which runs them in
the tab in front as if typed there and Enter pressed, and puts the focus
there; it goes into the command history as yours. In a shell session's block
only the lines with a prompt run (`$ `, `% `, `PS> `, and `> ` for a line
that goes on), without it, and not the output between them. A command Flow
rates high-risk, such as `rm -rf`, one for a shell Flow can't follow to its
prompt, and several lines in a shell that takes no bracketed paste (cmd,
macOS's own bash), which get joined as it runs them one after another, are
put at the prompt instead, for you to look at and run with Enter. While a
program is running in the tab, or holds its screen, nothing is typed into
it, and Flow says so. Nor is a block holding a control character (a tab, a
carriage return, Esc, Ctrl+C and the like), which the panel can't show and
which would be typed as a key, not a letter: Flow says which, and **Copy**
still copies it. Nor is one holding letters past ASCII where the shell
may take them as keys, not letters (bash just after its locale changed
into one that isn't UTF-8, or a shell where Flow's hook doesn't run). A
line that's nothing but something in backticks (a paragraph's, or a list item's after its bullet or number: `` `config.toml` ``,
`` `brew install ripgrep` ``) has a small **Copy** at its end, on the same
line, which copies what's between the backticks as written. It has no Run,
since a file's name, a path or a setting is written so as often as a
command; the assistant is told to put a command for you to run in a block of
its own, which has both. The card of a command the assistant ran, or waits for you
to run (beside **Run**, **Edit** and **Skip**), has the same **Copy** for
its command line.
With the pointer over a reply, **Copy reply** on its first line without a
**Copy** of its own copies the whole of it as written. And a drag over the conversation selects its words,
as in the terminal, to copy with `⌘C` or `Ctrl+Shift+C`, the menu's Copy or a
right click, while the panel has the keys; on Linux it's the primary
selection too.

Panel commands: `/model` (a list of the models to choose from with ↑↓ and
Enter; `/model sonnet` or `/model 2` puts one in use at once, and another
provider's starts a new conversation),
`/clear`, `/auto`, `/guide`, `/usage`, `/memory` (what the
assistant remembers), `/undo` (puts back the files the task changed),
`/skills` (lists your skills), `/settings`, `/about`, `/key`, `/forget`
(removes the stored key).

## Changing Flow's keys

Every key in the table above, and the commands with none (New
Conversation, Ask Why, Save as Theme, About, Restart Flow and the rest), can
be given whatever key you want. In Settings' **Keys** each one shows its
key: press Enter on it, or click **Change**, then the key it's to be. Esc
keeps the key it had, Backspace leaves it none, and **Reset** (or **Reset
All**) puts Flow's back. **Add Key** gives a command another key beside
those it has: both work, and the menus show the first. Going to tab 1 to 9 and moving between panes are
one row each, and take the modifiers held with the digit or the arrow you
press. Keys follow the keyboard's layout, as the menus do: `⌘?` is `⇧⌘ß`
on a German keyboard.

A key another command has is asked about: **Swap** them, **Take It**
(leaving the other without it), or **Keep Mine** (a key being added can't
be swapped). A key that would be typed
(no `Ctrl`, `Alt` or `⌘`, bar an F key), F1 alone, the system's (`⌘Q`,
`⌘Tab`, `Alt+F4`, `Alt+Space`, the Windows key) and the Hotkey are refused, and one the
shell would otherwise get (`Ctrl` and a letter) is taken with a word that
the shell no longer gets it. On Windows, `Alt+F4` closes Flow as it always
has, but not while Settings waits for a key, which refuses it as it does
the system's other keys. On the Mac, the menus show the keys as they're
bound, and while Settings waits for a key the menus give theirs up, so
`⌘T` can be pressed for another command. The keys inside Flow's own
screens (Esc, Enter, the arrows, the panel's `⌃L`, the history's `⌘⌫`)
stay as they are.

Only the keys you change are kept, under `[keys]` in `config.toml`:

```toml
[keys]
new_tab = "cmd+n"
record = "none"
bigger_text = ["cmd+plus", "cmd+="]
go_to_tab = "ctrl"
```

Each is a key's name (as `flow-help keys` lists them, `new_tab`,
`search_history`, `move_between_panes`) and its key: modifiers (`ctrl`,
`alt`, `shift`, `cmd`, the Windows key or Super elsewhere) and a key joined
by `+` (a letter, digit or symbol, `plus`, `space`, `tab`, `enter`,
`backspace`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`,
`left`, `right`, `up`, `down`, `escape`, `f1` to `f24`), a few in a list,
`none`, or for the two families their modifiers alone. A name or a key Flow
can't read is passed over, keeping its default. Programs can't change them
through `flow-settings`, since one could take Copy or Paste.
