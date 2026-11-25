---
name: flowapp
description: Using flowapp, Flow's app manager: installing, upgrading, listing and removing apps and packages of themes or skills, putting an app's program ahead of the system's command per shell (first, last), Settings → Apps, and making a small package of skills by hand.
---

# flowapp, Flow's app manager

flowapp installs Flow's apps (its games, Flow-Utils, the SDK) and packages of themes and of skills, and keeps them up to date, as Homebrew does. It's on the PATH of Flow's tabs, and only there. Run it with `run_command` in the user's tab. Reading (`list`, `outdated`, `info`, `search`, `first` alone) is low risk; `install`, `upgrade`, `remove`, `first` and `last` change the user's apps (medium). Where a detail isn't below, read `flowapp --help` or the `apps` help page (`flow_help` with page `apps`) rather than guess.

## Quick reference

```bash
flowapp update                # read the catalog again, and say what's newer
flowapp outdated              # what has a newer version (by the catalog update last read)
flowapp upgrade               # install everything newer (or: flowapp upgrade flow-rogue)
flowapp install flow-sdk      # install an app, a package of themes or one of skills
flowapp install mine.tar.gz   # install a package of the user's own from its file (.tar.gz or .tgz)
flowapp remove flow-sdk       # take off one flowapp installed (uninstall works too)
flowapp list                  # what's installed: apps, then Themes, then Skills
flowapp info flow-rogue       # what one app is
flowapp search puzzle         # what the catalog has (no words: all of it)
```

- `--json` after `update`, `list`, `outdated`, `info`, `search` or a bare `first` prints JSON. Each entry has `name`, `installed`, `latest`, `newer`, `kind` (`app`, `themes` or `skills`), `needs_flow`, `held` and more. Use it when you'll parse the output.
- `list`, `outdated`, `info` and `search` go by the catalog `update` last saved, so they work offline. `install` and `upgrade` read it again first.
- Every download is checked against the catalog's size, SHA-256 and Flow's signature before it's unpacked. One that needs a newer Flow is held back: `install` refuses it and `upgrade` skips it, saying which Flow it needs. The fix is updating Flow first, not trying again.
- A download that isn't there (a 404) can mean the catalog is a few minutes behind a release. Wait and try again.
- Installed apps live in `apps/` beside Flow's `config.toml`. Their programs go in `apps/bin`, which Flow's tabs have on their PATH after the system's commands. If a tab doesn't find a new program, open a new tab.
- `remove` takes off only what flowapp installed, and leaves an app's saves.

## Settings → Apps

The same in Flow's Settings: each app and package installed or in the catalog, its version, whether a newer one is out (or waits for a newer Flow), and Install, Upgrade and Remove buttons (Remove is pressed twice). Catalog says when it was last read, and Check now reads it again. Check apps at start, and Check apps (Daily or Weekly), only read the catalog and say what's newer; both are off at first. Flow never upgrades an app by itself. Point the user there when they'd rather click than type.

## First and last

An app's programs come after the system's commands of the same names, so an app's `ls` never replaces the system's unasked. `first` puts them ahead, in Flow's tabs only; `last` puts the system's back.

```bash
flowapp first flow-utils             # all its programs, in every shell
flowapp first ls --shell zsh,bash    # in these shells alone
flowapp first ls --except powershell # in every shell but these
flowapp first tree --prompt          # at the prompt alone: scripts keep the system's tree
flowapp first echo --take builtin    # bash's and zsh's own echo put aside while it's first
flowapp first ls --take alias        # the user's own alias of ls put aside too
flowapp first ls --take none         # keep even PowerShell's alias and BusyBox's applet ahead
flowapp first which --here           # in this tab alone, until it closes
flowapp first flow-utils/ls          # one app's ls, where two apps have one
flowapp first                        # what's first in each shell, and what still comes ahead
flowapp last ls --shell bash         # the system's ls again in bash
flowapp last --all                   # the system's commands first again
```

- Shells: `zsh`, `bash` (every bash, Git Bash and MSYS2 too), `sh`, `powershell`, `cmd`, `busybox`. Lists are comma-separated. `--shell` and `--except` don't go together, and neither goes with `--here`.
- `--take` takes `alias`, `builtin`, `function`, `applet` or `none`. Without it, PowerShell's own alias and BusyBox's applet are taken, nothing else. `--take builtin` refuses builtins no program can stand in for (`cd`, `export`, `read`).
- A later choice replaces an earlier one in the shells it names and leaves the rest.
- The tab it ran in follows at its next prompt, and other open tabs once their prompt comes back. A BusyBox tab follows once `flowapp` runs in it, or in a new tab. cmd's own commands (`dir`, `type`, `echo`) always come first, and `--prompt` puts nothing first in cmd.
- `--here` changes only the pane the command runs in. If your command ran in a pane of your own, tell the user to run it in theirs.
- Check with `flowapp first`, then `type <name>` in the tab (`Get-Command <name> -All` in PowerShell): flowapp can't see the user's own aliases and functions.

## Skills that come with packages

A package may bring skills for Flow's assistant: an app with a `skills/` folder beside its programs, or a package of nothing but skills. Settings → Skills lists every one of them under "Skills from <package>", each with a switch of its own, off until the user turns it on (All on and All off turn a package's all at once). You can't turn them on: `flow_settings` doesn't reach the assistant's settings, and don't edit `config.toml` to get round that. Tell the user where the switches are. Once on, each is listed for you as `<package>:<name>` (this one as `flow-skills:flowapp`), and `read_skill` takes that name. `/skills` in the panel lists the skills on, and each package's still off.

## Recipes

**Install an app.**
```bash
flowapp search
flowapp install flow-utils
```
Read what it prints: programs that share names with the system's commands (and the `flowapp first` that would put them ahead), where a library is, and whether it brought skills.

**See what's outdated, then upgrade.**
```bash
flowapp update
flowapp upgrade flow-rogue
```
Ask before a bare `flowapp upgrade`, which upgrades everything newer.

**Put a program first for one shell only.**
```bash
flowapp first tree --shell zsh
flowapp first
```
The user's zsh tabs run the app's `tree` from their next prompt. `flowapp last tree --shell zsh` undoes it.

**Install a package of the user's own.** A `.tar.gz` of themes (Settings' Export…, under Your themes, writes one) or of skills. It isn't signed, so flowapp refuses one holding programs (a `bin/` folder). Install only a file the user named.
```bash
flowapp install mine.tar.gz
```
Themes join Settings' Theme row and the new-tab picker once Flow is in front again.

**Make a small package of skills.** One skill for the user alone needs no package: Settings → Skills → Your skills opens their own skills folder. A package is for sharing, or keeping several together. Write its files with `write_file`, which makes the folders:

```
my-skills/
  skills.json
  skills/
    deploy-site.md
    backups/SKILL.md      (a folder skill may keep other files beside it)
```

`skills.json` names it:
```json
{"format": 1, "package": "my-skills", "version": "1.0.0"}
```

Each skill begins with front matter, then the playbook:
```
---
name: deploy-site
description: Deploying the site from a clean checkout, tests first.
---
```

- `name`: lowercase letters, digits and hyphens, at most 64 characters. `description`: one sentence, at most 300 characters. A skill without both is passed over, and a package with no skill that reads is refused.
- The package's name has no dots or slashes and isn't `bin` or `first`. Keep it lowercase with hyphens.

Pack it from the folder above `my-skills`, owned by 0 with no names, so the file names no one's account:
```bash
COPYFILE_DISABLE=1 /usr/bin/tar --no-mac-metadata --uid 0 --gid 0 --uname "" --gname "" -czf my-skills.tar.gz -C my-skills .   # macOS
tar --owner=0 --group=0 --numeric-owner -czf my-skills.tar.gz -C my-skills .   # Linux, Git Bash (GNU tar)
tar -czf my-skills.tar.gz -C my-skills .   # Windows' own tar, in PowerShell or cmd: it writes no owner's name
```

Install it, then tell the user to turn its skills on:
```bash
flowapp install my-skills.tar.gz
```
It says how many skills it has, each off until the user turns it on in Settings → Skills, under Skills from my-skills. To change it, raise its version, pack it and install it again. `flowapp remove my-skills` takes it off.
