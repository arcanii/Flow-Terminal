//! Shell discovery and launch. Every shell is started with a small integration
//! hook that reports prompt boundaries, exit codes and the working directory
//! through OSC 133 / OSC 633 sequences (BEL terminated, which survives ConPTY).

use portable_pty::CommandBuilder;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::config::config_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Cmd,
    PowerShell,
    Bash,
    Zsh,
    Sh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BashFlavor {
    Native,
    GitBash,
    Msys2,
    Wsl,
    /// The BusyBox ash that ships with Flow, used when there's no real bash.
    BusyBox,
}

#[derive(Debug, Clone)]
pub struct ShellProfile {
    /// Stable id used in config ("cmd", "powershell", "bash", "zsh").
    pub id: &'static str,
    /// Short label for the title bar.
    pub label: String,
    pub kind: ShellKind,
    pub program: PathBuf,
    pub flavor: BashFlavor,
}

impl ShellProfile {
    /// Name of the shell as Claude should think about it.
    pub fn describe(&self) -> String {
        match (self.kind, self.flavor) {
            (ShellKind::Cmd, _) => "Windows cmd.exe".into(),
            (ShellKind::PowerShell, _) => {
                if self.program.to_string_lossy().to_lowercase().contains("pwsh") {
                    "PowerShell 7 (pwsh)".into()
                } else {
                    "Windows PowerShell 5.1".into()
                }
            }
            (ShellKind::Bash, BashFlavor::GitBash) => "Git Bash (MSYS2 bash on Windows; paths look like /c/Users/...)".into(),
            (ShellKind::Bash, BashFlavor::Msys2) => "MSYS2 bash on Windows".into(),
            (ShellKind::Bash, BashFlavor::Wsl) => "bash inside WSL (Linux; Windows drives under /mnt/c)".into(),
            (ShellKind::Bash, BashFlavor::BusyBox) => "BusyBox ash on Windows, bundled with Flow: a POSIX sh with \
                some bash features but not GNU bash (no arrays). Paths look like C:/Users/...; ls, grep, sed, awk, \
                find and other Unix tools are built in"
                .into(),
            (ShellKind::Bash, BashFlavor::Native) => "bash".into(),
            (ShellKind::Zsh, _) => "zsh".into(),
            (ShellKind::Sh, _) => "POSIX sh".into(),
        }
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|s| s.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.is_file()).cloned()
}

/// The shells a Unix machine has (zsh, bash, else sh), in title-bar order:
/// zsh first unless the login shell is bash. The macOS and Linux crates
/// both offer these.
pub fn unix_profiles() -> Vec<ShellProfile> {
    let mut out = Vec::new();
    let user_shell = std::env::var("SHELL").ok().map(PathBuf::from);
    let zsh = first_existing(&[PathBuf::from("/bin/zsh"), PathBuf::from("/usr/bin/zsh")])
        .or_else(|| which("zsh"));
    // Prefer a modern bash (Homebrew) over macOS's bash 3.2.
    let bash = first_existing(&[
        PathBuf::from("/opt/homebrew/bin/bash"),
        PathBuf::from("/usr/local/bin/bash"),
        PathBuf::from("/bin/bash"),
        PathBuf::from("/usr/bin/bash"),
    ]);
    let push_zsh = |out: &mut Vec<ShellProfile>| {
        if let Some(z) = &zsh {
            out.push(ShellProfile {
                id: "zsh",
                label: "zsh".into(),
                kind: ShellKind::Zsh,
                program: z.clone(),
                flavor: BashFlavor::Native,
            });
        }
    };
    let user_is_bash = user_shell
        .as_ref()
        .map(|s| s.to_string_lossy().ends_with("bash"))
        .unwrap_or(false);
    if !user_is_bash {
        push_zsh(&mut out);
    }
    if let Some(b) = bash {
        out.push(ShellProfile {
            id: "bash",
            label: "bash".into(),
            kind: ShellKind::Bash,
            program: b,
            flavor: BashFlavor::Native,
        });
    }
    if user_is_bash {
        push_zsh(&mut out);
    }
    if out.is_empty() {
        out.push(ShellProfile {
            id: "sh",
            label: "sh".into(),
            kind: ShellKind::Sh,
            program: PathBuf::from("/bin/sh"),
            flavor: BashFlavor::Native,
        });
    }
    out
}

const PS_INTEGRATION: &str = r#"
# The pane's nonce, sent with the command line (OSC 633;E) and each
# command's end (133;D) so Flow tells the prompt's marks from a program's
# output that only looks like them: kept here, out of the environment of
# every program started from here.
$global:__flowNonce = "$env:FLOW_NONCE"
$env:FLOW_NONCE = $null
$global:__flowOrigPrompt = $function:prompt
# What `flowapp first` put ahead of the system's commands, first on the
# PATH, after the profile, which may have put its own folders first.
if ($env:FLOW_FIRST) {
    $env:PATH = (@($env:FLOW_FIRST) + @($env:PATH -split ';' | Where-Object { $_ -and $_ -ne $env:FLOW_FIRST })) -join ';'
}
# PowerShell's own aliases (ls, cat, man, ps) and functions (mkdir) come
# before any program. For each name `flowapp first` takes them away from
# (its folder's .take), and each first at the prompt alone (.prompt), an
# alias of the name to the program, at global scope, comes ahead of them,
# and what the name had is put back once it's no longer asked. The
# folder's dotfiles are passed over, which Get-ChildItem lists.
$global:__flowTaken = @{}
function global:__flowFirst {
    $want = @{}
    $dir = $env:FLOW_FIRST
    if ($dir) {
        $files = @{}
        foreach ($f in @(Get-ChildItem -LiteralPath $dir -File -ErrorAction SilentlyContinue)) {
            if (-not $f.Name.StartsWith('.')) { $files[$f.BaseName] = $f.FullName }
        }
        $take = Join-Path $dir '.take'
        if (Test-Path -LiteralPath $take) {
            foreach ($line in @(Get-Content -LiteralPath $take -ErrorAction SilentlyContinue)) {
                $w, $n = "$line" -split ' ', 2
                if (-not $n -or -not $files.ContainsKey($n)) { continue }
                if (-not $want.ContainsKey($n)) { $want[$n] = @{ Path = $files[$n]; Alias = $false; Function = $false; Prompt = $false } }
                if ($w -eq 'alias') { $want[$n].Alias = $true } elseif ($w -eq 'function') { $want[$n].Function = $true }
            }
        }
        foreach ($f in @(Get-ChildItem -LiteralPath (Join-Path $dir '.prompt') -File -ErrorAction SilentlyContinue)) {
            if (-not $f.Name.StartsWith('.')) { $want[$f.BaseName] = @{ Path = $f.FullName; Alias = $true; Function = $true; Prompt = $true } }
        }
    }
    foreach ($n in @($global:__flowTaken.Keys)) {
        $mine = $global:__flowTaken[$n]
        if ($want.ContainsKey($n) -and $want[$n].Path -eq $mine.Path) { continue }
        if ($mine.Had) {
            Set-Alias -Name $n -Value $mine.Had -Scope Global -Force -ErrorAction SilentlyContinue
        } else {
            Remove-Item -Force -LiteralPath "Alias:$n" -ErrorAction SilentlyContinue
        }
        $global:__flowTaken.Remove($n)
    }
    foreach ($n in @($want.Keys)) {
        if ($global:__flowTaken.ContainsKey($n)) { continue }
        $w = $want[$n]
        $a = Get-Alias -Name $n -ErrorAction SilentlyContinue | Select-Object -First 1
        $fn = Get-Item -LiteralPath "Function:$n" -ErrorAction SilentlyContinue
        # A function asked of, with an alias of the name ahead of it, stays
        # behind that alias, which wasn't asked of.
        if (-not ($w.Prompt -or ($w.Alias -and $a) -or ($w.Function -and $fn -and -not $a))) { continue }
        $had = if ($a) { $a.Definition } else { $null }
        $global:__flowTaken[$n] = @{ Path = $w.Path; Had = $had }
        Set-Alias -Name $n -Value $w.Path -Scope Global -Force -ErrorAction SilentlyContinue
    }
}
__flowFirst
function global:prompt {
    $flowOk = $global:?
    $flowCode = if ($flowOk) { 0 } elseif ($global:LASTEXITCODE) { $global:LASTEXITCODE } else { 1 }
    __flowFirst
    $e = [char]27; $bel = [char]7
    $loc = $executionContext.SessionState.Path.CurrentLocation
    $cwd = if ($loc.Provider.Name -eq 'FileSystem') { $loc.ProviderPath } else { '' }
    # The command line just run, for Flow's history (OSC 633;E, escaped as
    # VS Code does, and the pane's nonce after it): the newest history
    # entry, if it's new. Every control character goes as \xNN: a raw CAN
    # or SUB would end the OSC early, and the rest of it, the nonce among
    # it, would show on the screen.
    $flowNonce = $global:__flowNonce
    $flowLine = ''
    $h = Get-History -Count 1
    if ($h -and $h.Id -ne $global:__flowHistoryId) {
        $global:__flowHistoryId = $h.Id
        $c = $h.CommandLine -replace '\\', '\\' -replace ';', '\x3b'
        $c = [regex]::Replace($c, '[\x00-\x1f\x7f]', [System.Text.RegularExpressions.MatchEvaluator] { param($m) '\x{0:x2}' -f [int]$m.Value[0] })
        if ($c.Length -lt 4000) { $flowLine = "$e]633;E;$c;$flowNonce$bel" }
    }
    # The markers go out before the user's prompt runs: a prompt that draws
    # itself with Write-Host prints before it returns, and its text would
    # otherwise land ahead of the done marker as if the command printed it.
    # The done marker carries the nonce too: once Flow has seen it, a
    # "done" without it, or a prompt drawn while a command runs, is a
    # program's output.
    Write-Host -NoNewline "$flowLine$e]133;D;$flowCode;$flowNonce$bel$e]633;P;Cwd=$cwd$bel$e]133;A$bel"
    # An old machine's look gives its own prompt (Flow writes it for here).
    $body = if ($env:FLOW_PROMPT) { $ExecutionContext.InvokeCommand.ExpandString($env:FLOW_PROMPT) } else { & $global:__flowOrigPrompt }
    "$body$e]133;B$bel"
}
"#;

const BASH_INTEGRATION: &str = r#"
# Flow terminal integration: behave like a login shell, then add prompt markers.
# The pane's nonce, sent with each command line (OSC 633;E) and each
# command's end (133;D), and from bash 4.4 its start (133;C), so Flow tells
# the hook's marks from a program's output that only looks like them: kept
# in the shell, never exported, out of the environment of every program it
# starts, before the user's files start any.
__flow_nonce=${FLOW_NONCE:-}
export -n __flow_nonce
unset FLOW_NONCE
# Bash remembers where it found each command and runs that path again even
# once the program's gone (an app `flowapp remove` took away, whose copy in
# the apps' bin/ came ahead of Flow's own), where zsh and ash look again; so
# it checks a remembered one is still there first. Before the user's files,
# which have the last word.
shopt -s checkhash
# The locale bash started in, each variable with whether it's exported,
# which its line editor may already have read as the user's files run
# (`__flow_prompt` says why it matters); from bash 5.1, whose readline
# looks at it again.
(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 501 )) && __flow_locale="${LC_ALL-}=${LC_ALL+${LC_ALL@a}}|${LC_CTYPE-}=${LC_CTYPE+${LC_CTYPE@a}}|${LANG-}=${LANG+${LANG@a}}"
if [ -z "${FLOW_SKIP_PROFILE:-}" ]; then
  [ -f /etc/profile ] && . /etc/profile
  if [ -f ~/.bash_profile ]; then . ~/.bash_profile
  elif [ -f ~/.bash_login ]; then . ~/.bash_login
  elif [ -f ~/.profile ]; then . ~/.profile
  elif [ -f ~/.bashrc ]; then . ~/.bashrc
  fi
else
  # Bash's own prompt, as with no startup files at all: Debian's bash reads
  # /etc/bash.bashrc before this file, and its prompt names the user and host.
  PS1='\s-\v\$ '
fi
# The games that come with Flow, last on the PATH, after the user's files
# (which may have set nounset, as may the ones below).
if [ -n "${FLOW_GAMES:-}" ]; then
  case ":$PATH:" in
    *":$FLOW_GAMES:"*) ;;
    *) PATH="$PATH:$FLOW_GAMES" ;;
  esac
fi
# What `flowapp first` put ahead of the system's commands, first on the
# PATH, after the user's files, which may have put their own folders first.
if [ -n "${FLOW_FIRST:-}" ]; then
  __flow_path=":$PATH:"
  __flow_path=${__flow_path//":$FLOW_FIRST:"/:}
  __flow_path=${__flow_path#:}
  __flow_path=${__flow_path%:}
  PATH="$FLOW_FIRST${__flow_path:+:$__flow_path}"
  unset __flow_path
fi
# What `flowapp first` puts first in this tab is its folder, $FLOW_FIRST:
# its programs, first on the PATH; those in .prompt, first at the prompt
# alone, as aliases, so scripts keep the system's; and what .take says to
# take away for them, a builtin of bash's (enable -n) or an alias of the
# user's, given back once it's no longer asked. Looked at before each
# prompt without a process started for it, and once it's changed
# (`flowapp first` or `last` in any tab), bash, which remembers where it
# found each command, looks afresh.
__flow_taken=()
__flow_had=
__flow_first() {
  [[ -n ${FLOW_FIRST:-} ]] || return 0
  local fg= t= w n q i
  shopt -q failglob && { fg=1; shopt -u failglob; }
  local f=("$FLOW_FIRST"/*) p=("$FLOW_FIRST"/.prompt/*)
  [[ -n $fg ]] && shopt -s failglob
  [[ -r $FLOW_FIRST/.take ]] && { IFS= read -r -d '' t < "$FLOW_FIRST/.take" || :; }
  [[ "${f[*]-}|${p[*]-}|$t" == "${__flow_firsts-}" ]] && return 0
  __flow_firsts="${f[*]-}|${p[*]-}|$t"
  hash -r
  for ((i = 0; i + 1 < ${#__flow_taken[@]}; i += 2)); do
    case ${__flow_taken[i]} in
      builtin) enable "${__flow_taken[i+1]}" 2>/dev/null ;;
      *) unalias "${__flow_taken[i+1]}" 2>/dev/null ;;
    esac
  done
  [[ -n $__flow_had ]] && eval "$__flow_had"
  __flow_taken=() __flow_had=
  while read -r w n; do
    case $w in
      builtin) enable -n "$n" 2>/dev/null && __flow_taken+=(builtin "$n") ;;
      alias)
        if alias "$n" >/dev/null 2>&1; then
          __flow_had+="$(alias "$n")"$'\n'
          unalias "$n"
          __flow_taken+=(alias "$n")
        fi ;;
    esac
  done <<< "$t"
  for w in ${p[@]+"${p[@]}"}; do
    [[ -f $w ]] || continue
    n=${w##*/}
    n=${n%.exe}
    alias "$n" >/dev/null 2>&1 && __flow_had+="$(alias "$n")"$'\n'
    printf -v q %q "$w"
    alias "$n=$q"
    __flow_taken+=(prompt "$n")
  done
  return 0
}
__flow_precmd() {
  local ec=$? x=
  # Not traced (set -x), which would show the nonce; traced again after.
  [[ $- == *x* ]] && { x=1; set +x; } 2>/dev/null
  __flow_first
  [[ -n ${__flow_old:-} && -z ${__flow_seen+set} ]] && __flow_seen=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null)
  printf '\033]133;D;%s;%s\007\033]633;P;Cwd=%s\007' "$ec" "${__flow_nonce:-}" "$PWD"
  [[ -n $x ]] && set -x
  return $ec
}
# The line as bash keeps it in its history, for Flow's history (OSC 633;E,
# escaped as VS Code does, the nonce after it), sent once a line is read
# and before it runs: a command continued over several lines comes whole.
# From bash 4.4 on it goes from PS0, which bash shows then, in a subshell
# (which has the shell's own variables, the nonce among them). A line that
# has the number HISTCMD had at the prompt is this one. Any other newest
# line goes marked as only that (`;newest` after the nonce), since it may
# not be this one: HISTCONTROL or HISTIGNORE left this one out (the newest
# is then the line before, and a background job's output may have drawn a
# prompt with words of its own on the screen since), erasedups took an
# older copy of it out (the newest is then this one, a number lower), or
# it's a later line of a paste, which bash reads with no prompt between
# them (this one, a number higher). Flow then keeps what the screen showed
# only where it's that same line, and a paste's first line's word. Then
# the start of its output (133;C), as zsh's preexec says it, for every
# line: a line typed ahead while another ran starts once the prompt after
# that one is drawn, so only this tells Flow the shell isn't at its prompt
# any more. PS0 says every line's, so it says it with the nonce: Flow knows
# a command runs from here until the next "done", and that any prompt
# drawn meanwhile is a program's.
__flow_line() {
  local h
  # Not traced (set -x), which would show the nonce: in PS0's subshell, so
  # the shell's own trace is left as it was.
  { set +x; } 2>/dev/null
  h=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null)
  h=${h#"${h%%[! ]*}"}
  if [[ -n ${__flow_histcmd:-} && $h == "$__flow_histcmd  "* ]]; then
    __flow_send "${h#"$__flow_histcmd  "}"
  else
    # After its number, a * where it was edited since.
    h=${h#"${h%%[!0-9]*}"}
    __flow_send "${h#[ *] }" newest
  fi
  printf '\033]133;C;%s\007' "${__flow_nonce:-}"
}
# Says $1 as the command line, and $2 after the nonce where it's only the
# newest in the history (`newest`).
__flow_send() {
  local c=$1 i ch hex x=
  # Not traced (set -x), which would show the nonce; traced again after.
  [[ $- == *x* ]] && { x=1; set +x; } 2>/dev/null
  c=${c//\\/\\\\}
  c=${c//;/\\x3b}
  # Every control character as \xNN, as VS Code's hooks write them: a raw
  # CAN or SUB would end the OSC early, and the rest of it, the nonce among
  # it, would show on the screen.
  for ((i = 1; i < 32; i++)); do
    printf -v hex '\\x%02x' "$i"
    printf -v ch "$hex"
    [[ $c == *"$ch"* ]] && c=${c//"$ch"/"$hex"}
  done
  c=${c//$'\x7f'/\\x7f}
  # A line too long to say isn't said; the newest is said empty, which is
  # no line the screen shows.
  (( ${#c} < 4000 )) || { [[ -n ${2:-} ]] && c=; }
  (( ${#c} < 4000 )) && printf '\033]633;E;%s;%s%s\007' "$c" "${__flow_nonce:-}" "${2:+;$2}"
  [[ -n $x ]] && set -x
  return 0
}
# An older bash (macOS's own is 3.2) has no PS0, and reads HISTCMD as 1
# while PROMPT_COMMAND runs, so it sends the line from the first command the
# line runs, in the shell itself: when the newest history entry isn't the
# one there was at the last prompt (__flow_seen), it's this line's; when it
# is, the history left this one out, and it goes as only the newest, as
# `__flow_line` sends it. Then 133;C, as `__flow_line` sends it, but
# without the nonce: the trap isn't run for a line that starts with a
# subshell, so it can't say every command's start, and Flow counts one as
# running from Enter instead.
__flow_old_line() {
  local h said=
  if h=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null) && [[ -n ${__flow_seen+set} ]]; then
    [[ $h == "$__flow_seen" ]] && said=newest
    __flow_seen=$h
    h=${h#"${h%%[! ]*}"}
    h=${h#"${h%%[!0-9]*}"}
    __flow_send "${h#[ *] }" ${said:+"$said"}
  fi
  printf '\033]133;C\007'
}
if (( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 )); then
  [[ ${PS0:-} == *__flow_line* ]] || PS0='$(__flow_line)'"${PS0:-}"
elif [[ -n ${bash_preexec_imported:-}${__bp_imported:-} ]]; then
  # bash-preexec, where the user has it, owns the DEBUG trap and runs its
  # preexec functions once a line is read.
  __flow_old=1
  [[ " ${preexec_functions[*]:-} " == *" __flow_old_line "* ]] || preexec_functions+=(__flow_old_line)
elif [[ -z $(trap -p DEBUG) ]]; then
  # Otherwise a DEBUG trap, which bash runs before every simple command,
  # those PROMPT_COMMAND runs included: __flow_prompt arms it last before
  # each prompt, and the first command after sends the line. A DEBUG trap of
  # the user's own is left alone, and Flow keeps the grid's text. The trap's
  # "$_", and the same last argument at the end, leave $_ as the line's
  # command had it; bash keeps $? itself.
  __flow_old=1
  __flow_debug() {
    if [[ -n ${__flow_armed:-} ]]; then
      __flow_armed=
      # An empty line runs PROMPT_COMMAND at once, Flow's first, and the
      # hook's own last line comes armed: no command either time.
      [[ $BASH_COMMAND == __flow_precmd* || $BASH_COMMAND == __flow_armed=* ]] || __flow_old_line
    fi
    : "$1"
  }
  trap '__flow_debug "$_"' DEBUG
fi
# Last before each prompt, since a prompt theme's command may set PS1 afresh:
# an old machine's look gives its own prompt, and the marks go round it.
__flow_prompt() {
  local ec=$? l c v
  if [ -n "${FLOW_PROMPT:-}" ]; then PS1="$FLOW_PROMPT"; fi
  case "$PS1" in
    *'133;A'*) ;;
    *) PS1="\[\033]133;A\007\]$PS1\[\033]133;B\007\]" ;;
  esac
  # A look's prompt is written for bash alone, which shows it as text, so
  # it never goes to the programs started here, as PS1 would where the
  # user's files export it (or set -a, which exports it again each time
  # it's set, so this comes last): dash, ksh or zsh would expand it.
  if [ -n "${FLOW_PROMPT:-}" ]; then export -n PS1; fi
  # The number the next line gets in the history, as the user's commands
  # before this one (`history -n`, which shares a history between shells)
  # left it, for `__flow_line`.
  __flow_histcmd=${HISTCMD:-}
  # readline 8.1 and later (bash 5.1) look at the locale again as each
  # line is read, and where it has changed since the line before, put back
  # that locale's 8-bit settings over the bind below, for that line: a C
  # or POSIX one's (or none's) take each byte from 0x80 up as keys again.
  # A readline of its own (Homebrew's, and a Linux's that builds bash so)
  # reads the locale from the C library's environment, which bash brings
  # up to date only as it runs a program or a command substitution: one
  # now, so readline reads the locale this prompt has. Else it would read
  # it at a later prompt, once a line has run a program (Flow's own PS0
  # is a substitution), or one the last line set for a program and took
  # back, and put back that locale's settings at a prompt Flow wasn't
  # told of. So where the locale has changed since the last prompt (or
  # since bash started, the first time), a variable's export counting too,
  # and the one readline goes by (LC_ALL, else LC_CTYPE, else LANG) isn't
  # UTF-8, or none is set, read once over every one set, as bash's own
  # readline reads them, and once over those exported, as one of its own
  # does, Flow is told this prompt may take them as keys, and types none
  # into it: at the next, readline has seen it already, and the bind
  # holds. Anything can print that, which only has Flow type less. Not as
  # the hook starts, whose prompt this isn't, and whose "done" comes after;
  # but brought up to date then too, before the bind starts readline,
  # which reads the locale as it starts. Quiet where bash can't start one
  # (no processes left), as the bind is.
  if (( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 501 )); then
    { : "$(:)"; } 2>/dev/null
    l="${LC_ALL-}=${LC_ALL+${LC_ALL@a}}|${LC_CTYPE-}=${LC_CTYPE+${LC_CTYPE@a}}|${LANG-}=${LANG+${LANG@a}}"
    if [[ -z ${__flow_starting:-} && $l != "${__flow_locale-}" ]]; then
      __flow_locale=$l
      c=
      [[ -z $c && ${LC_ALL+${LC_ALL@a}} == *x* ]] && c=$LC_ALL
      [[ -z $c && ${LC_CTYPE+${LC_CTYPE@a}} == *x* ]] && c=$LC_CTYPE
      [[ -z $c && ${LANG+${LANG@a}} == *x* ]] && c=$LANG
      for v in "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" "$c"; do
        case $v in
          *[Uu][Tt][Ff]8* | *[Uu][Tt][Ff]-8*) ;;
          *) printf '\033]633;P;FlowMetaKeys=1\007'; break ;;
        esac
      done
    fi
  fi
  # Every byte typed is a letter, not Meta and a key: in a C or POSIX
  # locale (one a script or the user's files set, or none at all), or with
  # convert-meta in the user's .inputrc, readline takes each byte from 0x80
  # up as Meta and the key of its low seven bits, so a letter past ASCII
  # in a command Flow types (the assistant's, a reply's, the flow
  # command's words, a dropped file's name) would be keys: in macOS's bash
  # 3.2, Ē's second byte is M-C-r, revert-line, which throws away what's
  # typed so far, quotes and all. Before each prompt, as a `bind -f`, or
  # readline itself once the locale changes (above), could turn it on
  # again; bash's own `bind`, whatever the user's files named so; quiet
  # where there's no line editing, and never a failure, which errexit
  # would end the shell for.
  builtin bind 'set convert-meta off' 'set input-meta on' 'set output-meta on' 2>/dev/null || :
  __flow_armed=1
  return $ec
}
# Flow's first command goes in front of the user's first, where bash-preexec
# looks for its own. Its last goes after everything: as a command of its own
# where bash runs each of an array's (5.1 on; Ubuntu's /etc/profile.d makes
# one), else on a line of its own, since the user's may end in "; " or a
# comment. PROMPT_COMMAND's flags ("-a", "-r", "--") say which, and whether
# it can be changed at all.
__flow_pc="$(declare -p PROMPT_COMMAND 2>/dev/null)"
__flow_pc="${__flow_pc#declare }"
__flow_pc="${__flow_pc%% *}"
if [[ $__flow_pc != *r* && "${PROMPT_COMMAND[*]:-}" != *__flow_precmd* ]]; then
  PROMPT_COMMAND="__flow_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
  if [[ $__flow_pc == *a* ]] && (( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 501 )); then
    PROMPT_COMMAND+=(__flow_prompt)
  else
    PROMPT_COMMAND+=$'\n'__flow_prompt
  fi
fi
unset __flow_pc
# Marked now too, for when PROMPT_COMMAND is read-only; armed only by a
# prompt of its own.
__flow_starting=1
__flow_prompt
__flow_armed=
unset __flow_starting
"#;

/// Read by the bundled BusyBox ash through `$ENV`, since ash has no --rcfile.
const ASH_INTEGRATION: &str = r#"
# Flow terminal integration for the bundled BusyBox ash.
if [ -z "${FLOW_SKIP_PROFILE:-}" ] && [ -f "$HOME/.profile" ]; then
  . "$HOME/.profile"
fi
# The games that come with Flow, last on the PATH, after the user's files
# (which may have set nounset, as may the ones below). BusyBox for Windows
# parts its PATH with semicolons, as Windows does.
if [ -n "${FLOW_GAMES:-}" ]; then
  case ";$PATH;" in
    *";$FLOW_GAMES;"*) ;;
    *) PATH="$PATH;$FLOW_GAMES" ;;
  esac
fi
# What `flowapp first` put ahead of the system's commands, first on the
# PATH, after the user's files.
if [ -n "${FLOW_FIRST:-}" ]; then
  __flow_path=";$PATH;"
  __flow_path=${__flow_path//";$FLOW_FIRST;"/;}
  __flow_path=${__flow_path#;}
  __flow_path=${__flow_path%;}
  PATH="$FLOW_FIRST${__flow_path:+;$__flow_path}"
  unset __flow_path
fi
# BusyBox runs its own applets ahead of the PATH, but for the names in
# BB_OVERRIDE_APPLETS, which it then looks for on the PATH alone: so the
# names whose applets `flowapp first` takes away (its folder's .take) join
# the user's own names there. An alias of a name it takes the alias of
# (the user's, or ls's below) goes while it's first and comes back after,
# and a name first at the prompt alone (.prompt) is an alias. All as the
# tab opens and after flowapp runs in it: ash has no PROMPT_COMMAND to look
# before each prompt, and a command not found is looked up in a child,
# which can't change it.
__flow_bb_own=${BB_OVERRIDE_APPLETS:-}
__flow_taken=
__flow_had=
__flow_first() {
  for __flow_n in $__flow_taken; do unalias "$__flow_n" 2>/dev/null; done
  [ -n "$__flow_had" ] && eval "$__flow_had"
  __flow_taken= __flow_had= __flow_o=
  if [ -n "${FLOW_FIRST:-}" ]; then
    if [ -r "$FLOW_FIRST/.take" ]; then
      while read -r __flow_w __flow_n; do
        case $__flow_w in
          applet) __flow_o="$__flow_o $__flow_n" ;;
          alias)
            if alias "$__flow_n" >/dev/null 2>&1; then
              __flow_had="${__flow_had}alias $(alias "$__flow_n")
"
              unalias "$__flow_n"
              __flow_taken="$__flow_taken $__flow_n"
            fi ;;
        esac
      done < "$FLOW_FIRST/.take"
    fi
    for __flow_f in "$FLOW_FIRST"/.prompt/*; do
      [ -f "$__flow_f" ] || continue
      __flow_n=${__flow_f##*/}
      __flow_n=${__flow_n%.*}
      if alias "$__flow_n" >/dev/null 2>&1; then
        __flow_had="${__flow_had}alias $(alias "$__flow_n")
"
      fi
      alias "$__flow_n=\"$__flow_f\""
      __flow_taken="$__flow_taken $__flow_n"
    done
  fi
  BB_OVERRIDE_APPLETS="$__flow_bb_own$__flow_o"
  export BB_OVERRIDE_APPLETS
  unset __flow_f __flow_n __flow_w __flow_o
}
flowapp() {
  command flowapp "$@"
  __flow_rc=$?
  __flow_first
  return $__flow_rc
}
# Flow's colors for BusyBox's plain defaults, unless the user set their own.
if [ "$PS1" = '\w \$ ' ]; then
  PS1='\[\e[36m\]\w\[\e[0m\] \$ '
fi
alias ls >/dev/null 2>&1 || alias ls='ls --color=auto'
__flow_first
# An old machine's look gives its own prompt, with its backslashes sent as
# ^A (BusyBox would have turned them into slashes). It's written for ash
# alone (bash reads its octal escapes as $ and ` and expands them), so it
# never goes to the programs started here, as PS1 would where the user's
# .profile exports it, or sets -a, which exports it again each time it's
# set: so it's unexported last. busybox-w32 gives a variable that came from
# Windows' environment its value from there again the first time it's
# unexported, so that's done before the look's prompt is set, too. A bash
# that reads this file (as sh, which reads $ENV, started here by its path)
# keeps its own prompt: BASH_VERSINFO, an array, never comes from the
# environment.
__flow_look=
if [ -n "${FLOW_PROMPT:-}" ] && [ -z "${BASH_VERSINFO:-}" ]; then
  __flow_look=1
  export -n PS1
  __flow_a=$'\001' __flow_b='\'
  PS1=${FLOW_PROMPT//$__flow_a/$__flow_b}
  unset __flow_a __flow_b
fi
# ash has no PROMPT_COMMAND, but it expands the prompt every time it shows
# it, so $? and $PWD there are the last exit code and the current folder.
case "$PS1" in
  *'133;A'*) ;;
  *) PS1='\[\e]133;D;$?\a\e]633;P;Cwd=$PWD\a\e]133;A\a\]'"$PS1"'\[\e]133;B\a\]' ;;
esac
if [ -n "$__flow_look" ]; then export -n PS1; fi
unset __flow_look
"#;

const ZSH_ENV: &str = r#"
# Flow terminal integration (zshenv)
# The pane's nonce, sent with each command line (OSC 633;E) and each
# command's start and end (133;C, 133;D) so Flow tells the hook's marks
# from a program's output that only looks like them: kept in the shell,
# never exported, out of the environment of every program it starts,
# before the user's files start any.
__flow_nonce=${FLOW_NONCE:-}
typeset +x __flow_nonce
unset FLOW_NONCE
__flow_zdotdir="$ZDOTDIR"
ZDOTDIR="${FLOW_USER_ZDOTDIR:-$HOME}"
[ -f "$ZDOTDIR/.zshenv" ] && . "$ZDOTDIR/.zshenv"
FLOW_USER_ZDOTDIR="$ZDOTDIR"
ZDOTDIR="$__flow_zdotdir"
"#;

const ZSH_PROFILE: &str = r#"
__flow_zdotdir="$ZDOTDIR"; ZDOTDIR="$FLOW_USER_ZDOTDIR"
[ -f "$ZDOTDIR/.zprofile" ] && . "$ZDOTDIR/.zprofile"
ZDOTDIR="$__flow_zdotdir"
"#;

const ZSH_RC: &str = r#"
__flow_zdotdir="$ZDOTDIR"
# macOS's /etc/zshrc keeps the history in $ZDOTDIR, which is this folder of
# Flow's hooks, named after their text: so it stays where it always was,
# beside them in zsh/, whatever version of them a tab has.
[[ ${HISTFILE:-} == "$__flow_zdotdir/.zsh_history" ]] && HISTFILE="${__flow_zdotdir:h}/zsh/.zsh_history"
# Flow draws a letter's combining marks on it, so the line editor may leave
# them there rather than write each as <0301>, as macOS's /etc/zshrc has it
# do and Ubuntu's /etc/zsh doesn't; before the user's files, which may say not.
setopt combining_chars
ZDOTDIR="$FLOW_USER_ZDOTDIR"
[ -f "$ZDOTDIR/.zshrc" ] && . "$ZDOTDIR/.zshrc"
ZDOTDIR="$__flow_zdotdir"
# The games that come with Flow, last on the PATH, after the user's files
# (which may have set nounset, as may the ones below).
if [ -n "${FLOW_GAMES:-}" ]; then
  case ":$PATH:" in
    *":$FLOW_GAMES:"*) ;;
    *) PATH="$PATH:$FLOW_GAMES" ;;
  esac
fi
# What `flowapp first` put ahead of the system's commands, first on the
# PATH, after the user's files, which may have put their own folders first.
if [ -n "${FLOW_FIRST:-}" ]; then
  __flow_path=":$PATH:"
  __flow_path=${__flow_path//":$FLOW_FIRST:"/:}
  __flow_path=${__flow_path#:}
  __flow_path=${__flow_path%:}
  PATH="$FLOW_FIRST${__flow_path:+:$__flow_path}"
  unset __flow_path
fi
# What `flowapp first` puts first in this tab is its folder, $FLOW_FIRST:
# its programs, first on the PATH; those in .prompt, first at the prompt
# alone, as aliases, so scripts keep the system's; and what .take says to
# take away for them, a builtin of zsh's (disable) or an alias of the
# user's, given back once it's no longer asked. Looked at before each
# prompt without a process started for it ($(<file) is read by zsh
# itself), and once it's changed (`flowapp first` or `last` in any tab),
# zsh, which remembers where it found each command, looks afresh.
typeset -ga __flow_taken
typeset -gA __flow_had
__flow_first() {
  emulate -L zsh
  [[ -n ${FLOW_FIRST:-} ]] || return 0
  local -a f=("$FLOW_FIRST"/*(N)) p=("$FLOW_FIRST"/.prompt/*(N))
  local t= w n i line
  [[ -r $FLOW_FIRST/.take ]] && t=$(<"$FLOW_FIRST/.take")
  [[ "${f[*]}|${p[*]}|$t" == "${__flow_firsts-}" ]] && return 0
  __flow_firsts="${f[*]}|${p[*]}|$t"
  rehash
  for (( i = 1; i < ${#__flow_taken}; i += 2 )); do
    w=${__flow_taken[i]} n=${__flow_taken[i+1]}
    if [[ $w == builtin ]]; then enable -- "$n" 2>/dev/null; else unalias -- "$n" 2>/dev/null; fi
  done
  for n in ${(k)__flow_had}; do aliases[$n]=${__flow_had[$n]}; done
  __flow_taken=() __flow_had=()
  for line in ${(f)t}; do
    w=${line%% *} n=${line#* }
    case $w in
      builtin) (( ${+builtins[$n]} )) && disable -- "$n" && __flow_taken+=(builtin "$n") ;;
      alias)
        if (( ${+aliases[$n]} )); then
          __flow_had[$n]=${aliases[$n]}
          unalias -- "$n"
          __flow_taken+=(alias "$n")
        fi ;;
    esac
  done
  for w in $p; do
    n=${w:t}
    (( ${+aliases[$n]} )) && __flow_had[$n]=${aliases[$n]}
    aliases[$n]=${(q)w}
    __flow_taken+=(prompt "$n")
  done
  return 0
}
# zsh's `which` is a builtin of its own, so a `which` that `flowapp first`
# put ahead (Flow-Utils', which draws in Flow) would never run. While one
# is first, a plain `which` of programs (names, or -a and names) goes to
# it; one of an alias, a function, a builtin or a word of zsh's own, or
# with another of zsh's flags, stays zsh's, as does a `which` the user's
# files made their own. With zsh's put aside (`--take builtin`), every
# `which` is the one first.
if (( ! ${+aliases[which]} && ! ${+functions[which]} )); then
  which() {
    if [[ -n ${FLOW_FIRST:-} && -x $FLOW_FIRST/which ]]; then
      (( ${+dis_builtins[which]} )) && { "$FLOW_FIRST/which" "$@"; return }
      local a
      for a in "$@"; do
        case $a in
          -a) ;;
          -*) builtin which "$@"; return ;;
          *) [[ $(whence -w -- "$a") == *': command' ]] || { builtin which "$@"; return } ;;
        esac
      done
      "$FLOW_FIRST/which" "$@"
    else
      builtin which "$@"
    fi
  }
fi
__flow_precmd() {
  local ec=$?
  # In zsh's own options, as __flow_first is: the user's (glob_assign,
  # ksh_arrays, warn_nested_var) bear on nothing here. Not traced (set -x)
  # either, which would show the nonce.
  emulate -L zsh
  setopt noxtrace
  printf '\033]133;D;%s;%s\007\033]633;P;Cwd=%s\007' "$ec" "${__flow_nonce:-}" "$PWD"
  __flow_first
  # Hooked after the user's, since a prompt theme (promptinit's, say) sets PS1
  # afresh before each prompt: an old machine's look gives its own prompt,
  # and the marks go round it. That prompt is text, whatever the user's
  # options, now or by the time it's drawn (a widget may turn prompt_subst
  # on as the line editor starts): Flow wrote each letter prompt_subst or
  # prompt_bang would expand, $ ` \ and !, as %-4v to %-1v, which zsh fills
  # from the last four of psvar, put there here, and never expands. So PS1
  # holds none of them, and a shell started in the tab with PS1 in its
  # environment (the user's files may export it) finds nothing to expand
  # either. The four put there for the prompt before go first, where
  # they're still last.
  if [[ -n ${FLOW_PROMPT:-} ]]; then
    [[ ${(j::)psvar[-4,-1]} == '$`\!' ]] && psvar[-4,-1]=()
    psvar+=('$' '`' '\' '!')
    PS1="$FLOW_PROMPT"
  fi
  [[ $PS1 == *$'\e]133;A'* ]] || PS1=$'%{\e]133;A\a%}'"$PS1"$'%{\e]133;B\a%}'
  # A theme hooked in since (`prompt <name>`, a .zshrc sourced again) would
  # set PS1 after this, so be last again from the next prompt on.
  [[ ${precmd_functions[-1]} == __flow_precmd ]] || precmd_functions=(${precmd_functions:#__flow_precmd} __flow_precmd)
}
# The command line as typed, for Flow's history (OSC 633;E, escaped as
# VS Code does, the nonce after it), then the start of its output, with the
# nonce too: preexec says every command's, so Flow knows one runs from here
# until the next "done", and that any prompt drawn meanwhile is a program's.
# In zsh's own options, as __flow_first is: the user's glob_assign and
# glob_subst would glob the line as each assignment takes it, and a glob
# qualifier in it, quoted for the command, would run as code here. Not
# traced (set -x) either, which would show the nonce.
__flow_preexec() {
  emulate -L zsh
  setopt noxtrace
  local c=${1//\\/\\\\} i ch hex
  c=${c//;/\\x3b}
  # Every control character as \xNN, as VS Code's hooks write them: a raw
  # CAN or SUB would end the OSC early, and the rest of it, the nonce among
  # it, would show on the screen.
  for (( i = 1; i < 32; i++ )); do
    ch=${(#)i}
    if [[ $c == *"$ch"* ]]; then
      printf -v hex '\\x%02x' $i
      c=${c//"$ch"/$hex}
    fi
  done
  c=${c//$'\x7f'/\\x7f}
  (( ${#c} < 4000 )) && printf '\033]633;E;%s;%s\007' "$c" "${__flow_nonce:-}"
  printf '\033]133;C;%s\007' "${__flow_nonce:-}"
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd __flow_precmd
add-zsh-hook preexec __flow_preexec
"#;

const ZSH_LOGIN: &str = r#"
ZDOTDIR="$FLOW_USER_ZDOTDIR"
[ -f "$ZDOTDIR/.zlogin" ] && . "$ZDOTDIR/.zlogin"
# Flow's prompt hook last, after any the user's .zlogin added.
precmd_functions=(${precmd_functions:#__flow_precmd} __flow_precmd)
"#;

fn integration_dir() -> PathBuf {
    config_dir().join("shell-integration")
}

/// A short name for a hook's text, from its SHA-256: each hook's file (and
/// zsh's folder of them) takes it, so Flows whose hooks differ, an older
/// one and a newer one running at once, or another checkout's tests, never
/// write over the hooks the other's shells are about to read.
fn hook_name(body: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(body.as_bytes()).iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// Where in `dir` the hook `name` (`flow.bash`) with the text `body` is
/// kept: `flow-<its name>.bash`.
fn hook_path(dir: &Path, name: &str, body: &str) -> PathBuf {
    let (stem, ext) = name.split_once('.').unwrap_or((name, ""));
    dir.join(format!("{stem}-{}.{ext}", hook_name(body)))
}

/// Writes `body` to `path` unless it's there already: whole beside it,
/// then put in place, so a shell starting meanwhile in another tab, or
/// another Flow, never reads half of it.
fn put_hook(path: &Path, body: &str) {
    if std::fs::read_to_string(path).is_ok_and(|was| was == body) {
        return;
    }
    let Some(dir) = path.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    let new = dir.join(format!(".{}.{}.new", path.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
    let put = std::fs::write(&new, body).and_then(|_| std::fs::rename(&new, path));
    if let Err(err) = put {
        let _ = std::fs::remove_file(&new);
        log::warn!("could not write shell integration {}: {err}", path.display());
    }
}

/// The hook `name` with the text `body`, written where it's missing, and
/// where its shell finds it.
fn write_integration_file(name: &str, body: &str) -> PathBuf {
    // Scripts run under bash/zsh must use LF line endings.
    let body = body.replace("\r\n", "\n");
    let path = hook_path(&integration_dir(), name, &body);
    put_hook(&path, &body);
    path
}

/// zsh's hooks, a folder of the four files it reads from `ZDOTDIR`, named
/// after all of them, and written where they're missing. The history macOS
/// keeps in `ZDOTDIR` stays in `zsh/` beside them, where the hooks before
/// these were (`ZSH_RC`), so a new version of them keeps it.
fn write_zsh_integration() -> PathBuf {
    let _ = std::fs::create_dir_all(integration_dir().join("zsh"));
    let files = [(".zshenv", ZSH_ENV), (".zprofile", ZSH_PROFILE), (".zshrc", ZSH_RC), (".zlogin", ZSH_LOGIN)];
    let all: String = files.iter().map(|(name, body)| format!("{name}\n{body}")).collect();
    let dir = integration_dir().join(format!("zsh-{}", hook_name(&all)));
    for (name, body) in files {
        put_hook(&dir.join(name), body);
    }
    dir
}

/// `C:\Users\me\x` -> `C:/Users/me/x` (accepted by MSYS) or `/mnt/c/Users/me/x` (WSL).
fn posix_path(path: &Path, flavor: BashFlavor) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if flavor == BashFlavor::Wsl {
        let bytes = s.as_bytes();
        if bytes.len() > 2 && bytes[1] == b':' {
            return format!("/mnt/{}{}", (bytes[0] as char).to_ascii_lowercase(), &s[2..]);
        }
    }
    s
}

fn encode_powershell(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64(&bytes)
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Variables the platform has every shell start with (`Platform::shell_env`).
static SHELL_ENV: OnceLock<Vec<(String, String)>> = OnceLock::new();

/// Sets the platform's extra shell variables, once, before shells start.
pub fn set_shell_env(env: Vec<(String, String)>) {
    let _ = SHELL_ENV.set(env);
}

/// The games' folder (`Platform::games_dir`).
static GAMES: OnceLock<PathBuf> = OnceLock::new();

/// Puts the games' folder on every shell's PATH, once, before shells start,
/// after the folder of the apps `flowapp` installed (`apps::bin_dir`), so an
/// app it upgraded comes ahead of the copy Flow carries. Shells get both as
/// `FLOW_GAMES` too, and the bash, zsh and ash hooks add them again after the
/// user's startup files, which may set PATH afresh.
pub fn set_games_dir(dir: PathBuf) {
    let _ = GAMES.set(dir);
}

/// The folders Flow adds to the end of every shell's PATH, in order: the
/// apps `flowapp` installed, then the games Flow carries, where it carries
/// them (not under `cargo run`). A snapshot's plain shells get Flow's own
/// alone, so none of the user's apps stands in for a command it runs.
fn games_dirs(plain: bool) -> Vec<PathBuf> {
    (!plain).then(crate::apps::bin_dir).into_iter().chain(GAMES.get().cloned()).collect()
}

/// The folder a snapshot's or a recording's shells start in and call home,
/// which the snapshot made; while it's set, every shell starts plain.
static PLAIN_HOME: OnceLock<PathBuf> = OnceLock::new();

/// Has every shell from now on start plain, for a snapshot or a recording
/// (`--snapshot`, `--record`), whose pictures show nothing of whoever makes
/// them: in `home`, a folder the snapshot made, which is its home folder
/// too, so it reads none of the user's history or line editor's settings;
/// without the user's startup files (`FLOW_SKIP_PROFILE`, PowerShell's
/// `-NoProfile`, cmd's `/D`), so its prompt is the shell's own; and with
/// nothing `flowapp` installed or put first.
pub fn make_shells_plain(home: PathBuf) {
    let _ = PLAIN_HOME.set(home);
}

/// The folder plain shells start in and call home, once a snapshot has made
/// them so ([`make_shells_plain`]).
pub fn plain_home() -> Option<&'static Path> {
    PLAIN_HOME.get().map(PathBuf::as_path)
}

/// The prompt an old machine's look gives the shells (`looks::Look::prompt`).
static PROMPT: parking_lot::RwLock<Option<String>> = parking_lot::const_rwlock(None);

/// Gives the shells in new tabs `template`'s prompt in place of their own,
/// or (with none) leaves theirs. They find it in `FLOW_PROMPT`, written for
/// each (`looks::prompt_for`); cmd in its `PROMPT`.
pub fn set_prompt(template: Option<&str>) {
    *PROMPT.write() = template.map(String::from);
}

/// The letters of a theme's prompt that one of zsh's options would expand
/// (prompt_subst's `$`, `` ` `` and `\`, prompt_bang's `!`), which
/// `ZSH_RC`'s `__flow_precmd` puts last in `psvar`, in this order, while a
/// theme's prompt is worn, and `looks::prompt_for` writes as `%-4v` to
/// `%-1v`: zsh shows what `%v` brings as it is, whatever its options.
pub const ZSH_LITERALS: [char; 4] = ['$', '`', '\\', '!'];

/// Flow's own PATH with `first` in front, so what `flowapp first` put
/// there comes ahead of the system's commands, and `dirs` last, so the
/// games never shadow the user's.
fn path_with(first: Option<&Path>, dirs: &[PathBuf]) -> Option<std::ffi::OsString> {
    let inherited: Vec<std::ffi::OsString> = ["FLOW_GAMES", "FLOW_FIRST"].iter().filter_map(std::env::var_os).collect();
    path_for_games(std::env::var_os("PATH"), &inherited, first, dirs)
}

/// `path` with `first` (the pane's folder of what's first) in front and
/// `dirs` last, in their order, and without `inherited`'s others, the
/// folders of a Flow, or a tab, this one was started in: opened from one of
/// another Flow's tabs (`open` passes the tab's variables on), it has that
/// Flow's on its PATH already, and their older versions would run.
fn path_for_games(path: Option<std::ffi::OsString>, inherited: &[std::ffi::OsString], first: Option<&Path>, dirs: &[PathBuf]) -> Option<std::ffi::OsString> {
    let others: Vec<PathBuf> = inherited.iter().flat_map(std::env::split_paths).collect();
    let mut paths: Vec<PathBuf> = path.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    paths.retain(|p| !(others.contains(p) || dirs.contains(p) || Some(p.as_path()) == first));
    if let Some(first) = first {
        paths.insert(0, first.to_path_buf());
    }
    paths.extend(dirs.iter().cloned());
    std::env::join_paths(paths).ok()
}

/// The games' folders as `flavor`'s PATH spells and parts them, for the
/// hooks to look for them there and add them where they're missing.
fn games_paths(dirs: &[PathBuf], flavor: BashFlavor) -> std::ffi::OsString {
    let parted = if flavor == BashFlavor::BusyBox || (cfg!(windows) && !matches!(flavor, BashFlavor::GitBash | BashFlavor::Msys2 | BashFlavor::Wsl)) { ";" } else { ":" };
    let mut out = std::ffi::OsString::new();
    for (i, dir) in dirs.iter().enumerate() {
        if i > 0 {
            out.push(parted);
        }
        out.push(games_path(dir, flavor));
    }
    out
}

/// The games' folder as `flavor`'s PATH spells it, for the hooks to look for
/// it there: Git Bash and MSYS2 turn the PATH they're given into a Unix one
/// (`/c/Program Files/Flow/games`, parted by colons), while BusyBox keeps
/// Windows' and turns the backslashes into slashes itself.
fn games_path(dir: &Path, flavor: BashFlavor) -> std::ffi::OsString {
    if matches!(flavor, BashFlavor::GitBash | BashFlavor::Msys2) {
        let s = posix_path(dir, flavor);
        if let [drive, b':', ..] = s.as_bytes() {
            return format!("/{}{}", (*drive as char).to_ascii_lowercase(), &s[2..]).into();
        }
    }
    dir.as_os_str().to_owned()
}

/// `MANPATH` with the apps' man pages (`apps::man_dirs`) after the system's:
/// a leading colon is the system's own path first, for the Mac's man and
/// man-db alike, so an app's `ls.1` never hides the system's. One the tab
/// inherits keeps its own folders first, without the apps' (a Flow started
/// from another's tab would have them already). None where no app brings
/// pages, and on Windows, which has no man.
fn man_path(inherited: Option<std::ffi::OsString>, dirs: &[PathBuf]) -> Option<std::ffi::OsString> {
    if dirs.is_empty() || cfg!(windows) {
        return None;
    }
    let ours: Vec<String> = dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect();
    let inherited = inherited.map(|m| m.to_string_lossy().into_owned()).unwrap_or_default();
    let kept: Vec<&str> = inherited.split(':').filter(|p| !ours.iter().any(|o| o == p)).collect();
    let mut path = kept.join(":");
    path.push(':');
    path.push_str(&ours.join(":"));
    Some(path.into())
}

/// `words` as typed at a `kind` shell's prompt to run them as they are:
/// each the word a program gets, after the shell `flow` was typed in
/// expanded its own (`flow_command`). A word of plain characters goes as it
/// is, so `ssh -N -L 7445:127.0.0.1:7445 user@host` is typed as written;
/// any other is quoted so the new shell expands nothing in it again: zsh's,
/// bash's and BusyBox's single quotes, PowerShell's (a quoted first word
/// run through `&`), and cmd's double quotes by Windows' rules, with `^`
/// before cmd's own `& | < > ^` where cmd reads them outside quotes, and
/// before each `%`, so that `%NAME%` isn't expanded at its prompt.
/// [`quote_for`] is this for a shell profile, Windows PowerShell 5.1's
/// way of handing words to programs among them. A word holding a control
/// character is refused ([`Untypable`]): no quotes keep one in its word.
/// Letters past ASCII go as letters, to bash too, whose readline in a
/// locale that isn't UTF-8 would take each of their bytes as Meta and a
/// key, but which Flow's hook tells before each prompt to take them as
/// letters (`BASH_INTEGRATION`'s `__flow_prompt`): this is for a line typed
/// at a prompt Flow's hook drew, which `Session::types_letters` says.
/// [`quote_ascii`] is for one whose reader Flow can't know.
pub fn quote(kind: ShellKind, _flavor: BashFlavor, words: &[String]) -> Result<String, Untypable> {
    quote_words(kind, words, false)
}

/// [`quote`] for a line whose reader Flow can't know, so it may reach a
/// bash its hook hasn't told to take letters past ASCII as letters: a
/// file's path dropped on a pane while a program runs there (a shell
/// started inside the pane's, one over ssh), or the flow command's words
/// typed into a shell that never showed the hook's prompt (one its
/// startup files `exec`ed), or at one where bash's hook said its line
/// editor may take them as keys (`Session::types_letters`). Whatever the
/// pane's shell, a bash may be reading, so into zsh and bash (every
/// flavor, BusyBox's ash among them) the line is ASCII alone, each run of
/// letters past ASCII written as its bytes (`posix_word`'s `ascii`),
/// which zsh and bash read back as the word in any locale, and which no
/// line editor takes as keys. PowerShell's and cmd's have no such quotes,
/// nor sh's before dash 0.5.12, which Flow starts only where there's no
/// bash or zsh to start: theirs go as [`quote`] has them.
pub fn quote_ascii(kind: ShellKind, _flavor: BashFlavor, words: &[String]) -> Result<String, Untypable> {
    quote_words(kind, words, matches!(kind, ShellKind::Zsh | ShellKind::Bash))
}

/// [`quote_ascii`] for a shell profile, as [`quote_for`] is [`quote`]'s.
pub fn quote_ascii_for(profile: &ShellProfile, words: &[String]) -> Result<String, Untypable> {
    match profile.kind {
        ShellKind::Zsh | ShellKind::Bash => quote_ascii(profile.kind, profile.flavor, words),
        ShellKind::Sh | ShellKind::PowerShell | ShellKind::Cmd => quote_for(profile, words),
    }
}

fn quote_words(kind: ShellKind, words: &[String], ascii: bool) -> Result<String, Untypable> {
    typable(words)?;
    let quoted: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, word)| match kind {
            ShellKind::Cmd => windows_word(word),
            ShellKind::PowerShell => powershell_word(word, i == 0),
            _ => posix_word(word, i == 0, ascii),
        })
        .collect();
    let line = quoted.join(" ");
    Ok(match kind {
        ShellKind::Cmd => cmd_carets(&line),
        _ => line,
    })
}

/// A word that can't be typed at a prompt, since it holds a control
/// character (`char::is_control`: C0's, DEL and C1's), which a line editor
/// takes as a key, not a letter, whatever quotes it's in. Ctrl+C or
/// Ctrl+U throws away what's typed so far, quotes and all, so the rest is
/// read outside them; Enter (CR, or a line break) runs what's there; Esc
/// begins the editor's own commands; Tab completes, which closes a quote
/// round a name it completes; and Ctrl+D, Ctrl+V and the rest do what each
/// editor binds them to. Its `word`'s place among the words (from 0) and
/// the first such `character` in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Untypable {
    pub word: usize,
    pub character: char,
}

impl Untypable {
    /// The character as Unicode writes it ([`char_code`]).
    pub fn code(&self) -> String {
        char_code(self.character)
    }
}

/// `c` as Unicode writes it, `U+0003`, for a control character, which has
/// no letter to show.
pub fn char_code(c: char) -> String {
    format!("U+{:04X}", u32::from(c))
}

/// Whether every word of `words` can be typed at a prompt as letters
/// alone; else the first that can't ([`Untypable`]).
pub fn typable(words: &[String]) -> Result<(), Untypable> {
    for (word, w) in words.iter().enumerate() {
        if let Some(character) = w.chars().find(|c| c.is_control()) {
            return Err(Untypable { word, character });
        }
    }
    Ok(())
}

/// The first character of `lines`, a command (or a few, a line each) to
/// type at a prompt, that a line editor would take as a key, not a letter:
/// any control character but the line breaks between them (`\n`, or
/// `\r\n`), which each way of typing them takes as the end of a line. So
/// a lone CR, which would be Enter mid-line, a Tab, which completes, and
/// Ctrl+C or Ctrl+U, which throw away what's typed so far, are all found,
/// as [`Untypable`] finds them in a word.
pub fn key_in(lines: &str) -> Option<char> {
    letters_and_keys(lines).find(|(_, key)| *key).map(|(c, _)| c)
}

/// `lines` without what [`key_in`] finds: every control character but the
/// line breaks between them, and a tab where `tabs` (in a bracketed paste,
/// a letter to the line editor), so text put at a prompt to be looked at
/// first (a command from the history, which a program could have forged)
/// is letters there, never keys that run or change the line before Enter,
/// nor ones the terminal's driver acts on even mid-paste (Ctrl+C, Ctrl+Z,
/// Ctrl+S).
pub fn without_keys(lines: &str, tabs: bool) -> String {
    letters_and_keys(lines).filter(|&(c, key)| !key || (tabs && c == '\t')).map(|(c, _)| c).collect()
}

/// Each character of `lines` and whether a line editor would take it as a
/// key ([`key_in`]).
fn letters_and_keys(lines: &str) -> impl Iterator<Item = (char, bool)> + '_ {
    let mut chars = lines.chars().peekable();
    std::iter::from_fn(move || {
        let c = chars.next()?;
        let line_break = c == '\n' || (c == '\r' && chars.peek() == Some(&'\n'));
        Some((c, c.is_control() && !line_break))
    })
}

/// [`quote`] for `profile`'s shell. Windows PowerShell 5.1
/// (`powershell.exe`) hands a program its words the old way, which
/// PowerShell 7.3 left (its `$PSNativeCommandArgumentPassing`): each as it
/// is, in double quotes only where it holds a space, a `"` in it left bare,
/// an empty one dropped and a backslash before its closing quote left
/// single. So each word after the first that any of that would change goes
/// to it already quoted as a Windows program reads its words
/// ([`windows_word`]), which it then passes on as it is.
pub fn quote_for(profile: &ShellProfile, words: &[String]) -> Result<String, Untypable> {
    // Its program's name after the last \ or /, as Windows writes the path,
    // whatever system reads it (a backslash is no separator elsewhere).
    let path = profile.program.to_string_lossy();
    let name = path.rsplit(['\\', '/']).next().unwrap_or_default();
    let old_powershell = profile.kind == ShellKind::PowerShell && (name.eq_ignore_ascii_case("powershell.exe") || name.eq_ignore_ascii_case("powershell"));
    if !old_powershell {
        return quote(profile.kind, profile.flavor, words);
    }
    typable(words)?;
    let quoted: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, word)| match i > 0 && (word.is_empty() || word.contains(['"', ' ', '\t'])) {
            true => powershell_word(&windows_word(word), false),
            false => powershell_word(word, i == 0),
        })
        .collect();
    Ok(quoted.join(" "))
}

/// A word for zsh, bash or ash: as it is where nothing in it is the
/// shell's (no `~`, which bash expands after a `=` or a `:` in a word that
/// looks like an assignment, `a=~` and `--x=~/y` among them; no zsh's `=`
/// to begin it; and in the first word no `=`, which would make it an
/// assignment), else in single quotes. With `ascii` ([`quote_ascii`]),
/// each run of letters past ASCII goes as its bytes in ANSI-C quotes,
/// `$'\xc4\x92'`, beside the single quotes of the rest, so nothing past
/// ASCII is typed. bash's readline in a C or POSIX locale (one a script or
/// a `.bashrc` set, or none at all) takes each byte from 0x80 up as Meta
/// and a key, unless told otherwise as Flow's hook tells it: in macOS's
/// bash 3.2, Ē's second byte is M-C-r, revert-line, which throws away
/// what's typed so far, quotes and all, so the rest of the word would run
/// as a command. bash and zsh read `$'\xHH'` as that byte in any locale
/// (BusyBox's ash where it's built with bash's ways, dash from 0.5.12), so
/// a program still gets the word's own UTF-8. The `$'…'` holds nothing but
/// those escapes, no `'` or `!`, so bash's history expansion reads its
/// quotes as the shell does.
fn posix_word(word: &str, first: bool, ascii: bool) -> String {
    let plain = !word.is_empty()
        && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:@,+%=".contains(c))
        && !word.starts_with('=')
        && !(first && word.contains('='));
    let single = |text: &str| format!("'{}'", text.replace('\'', "'\\''"));
    if plain {
        return word.to_string();
    }
    if !ascii || word.is_ascii() {
        return single(word);
    }
    let chars: Vec<char> = word.chars().collect();
    chars
        .chunk_by(|a, b| a.is_ascii() == b.is_ascii())
        .map(|run| {
            let run: String = run.iter().collect();
            match run.is_ascii() {
                true => single(&run),
                false => format!("$'{}'", run.bytes().map(|b| format!("\\x{b:02x}")).collect::<String>()),
            }
        })
        .collect()
}

/// A word for PowerShell: as it is where nothing in it is PowerShell's
/// (no `,`, which makes an array; no `@` to begin it, a splat; no `-`
/// word with a `.` or `:` in it, which PowerShell may split), else in
/// single quotes, each of its four single quotes doubled. A first word
/// quoted is a string, not a command, so `&` runs it.
fn powershell_word(word: &str, first: bool) -> String {
    let plain = !word.is_empty()
        && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:\\=+@".contains(c))
        && !word.starts_with('@')
        && !(word.starts_with('-') && word.contains(['.', ':']));
    if plain {
        return word.to_string();
    }
    let mut out = String::from(if first { "& '" } else { "'" });
    for c in word.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// A word for cmd: as it is where it's plain, else in double quotes as a
/// program's arguments are read on Windows (a `"` in it as `\"`, the
/// backslashes before it, and at its end, doubled).
fn windows_word(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:\\@=,+~%".contains(c)) {
        return word.to_string();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for c in word.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            c => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// `line` with `^` before each of cmd's `& | < > ^` that cmd reads outside
/// quotes: cmd knows no `\"`, so every `"` turns its quotes on or off, from
/// one word into the next. And `^` before every `%`: cmd's prompt expands
/// `%NAME%` before it reads quotes or carets, but leaves a name it doesn't
/// know (`NAME^`) as it is, and the caret is taken away after. Inside
/// quotes, where a caret is a letter, the quotes are closed around it
/// (`"a"^%"b"`), which a Windows program reads as one word.
fn cmd_carets(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut inside = false;
    for c in line.chars() {
        if c == '"' {
            inside = !inside;
        } else if c == '%' {
            out.push_str(if inside { "\"^%\"" } else { "^%" });
            continue;
        } else if !inside && "&|<>^".contains(c) {
            out.push('^');
        }
        out.push(c);
    }
    out
}

/// The variable a shell's hook takes its pane's nonce from, to send after
/// each command line it says (OSC 633;E), as VS Code's hooks send theirs,
/// and after each command's end (133;D) and, where it says every one, its
/// start (133;C). Anything printed in a tab can send the marks, a file
/// being `cat`ed or a remote host's output over ssh among it: a command
/// line, which the history, Ask why and the notifications would name, or
/// a prompt drawn while a command runs, after which Enter would read what
/// the program drew as a command; only the hook knows the nonce. The
/// hooks keep it in the shell, never exported, and take it out of the
/// environment, zsh's and bash's before the user's files run, PowerShell's
/// once its profile has (which runs before Flow's script), so no program
/// started from the prompt (ssh among them) has it.
pub const NONCE: &str = "FLOW_NONCE";

/// A new pane's nonce ([`NONCE`]): 128 random bits as hex, or none where
/// the system has none to give, and then no command line is taken from
/// the shell, and its marks are taken as they come.
pub fn new_nonce() -> String {
    crate::flow_command::random_secret().unwrap_or_else(|| {
        log::warn!("no random numbers for a pane's nonce: the shell's own word for each command line is passed over");
        String::new()
    })
}

/// Whether `profile`'s hook says each command line itself (OSC 633;E):
/// zsh's, bash's and PowerShell's do; cmd, BusyBox's ash and a plain sh
/// have none, and Flow reads the line off the screen.
fn says_its_lines(profile: &ShellProfile) -> bool {
    match profile.kind {
        ShellKind::Zsh | ShellKind::PowerShell => true,
        ShellKind::Bash => profile.flavor != BashFlavor::BusyBox,
        ShellKind::Cmd | ShellKind::Sh => false,
    }
}

/// Gives the shell `cmd` starts its pane's `nonce` ([`NONCE`]) where its
/// hook says each command line, and none elsewhere (not one from a Flow,
/// or a tab, this one was started in). WSL's Linux gets only the
/// variables `WSLENV` names, so there it's named too.
pub fn give_nonce(cmd: &mut CommandBuilder, profile: &ShellProfile, nonce: &str) {
    if nonce.is_empty() || !says_its_lines(profile) {
        cmd.env_remove(NONCE);
        return;
    }
    cmd.env(NONCE, nonce);
    if profile.flavor == BashFlavor::Wsl {
        let named = cmd.get_env("WSLENV").map(|v| v.to_string_lossy().into_owned()).unwrap_or_default();
        if !named.split(':').any(|n| n.split('/').next() == Some(NONCE)) {
            cmd.env("WSLENV", if named.is_empty() { NONCE.to_string() } else { format!("{named}:{NONCE}") });
        }
    }
}

/// Builds the command line for `profile`, wired up with shell integration,
/// with `first` (the pane's own folder, `apps::first`) first on its PATH
/// as `FLOW_FIRST`.
pub fn build_command(profile: &ShellProfile, cwd: Option<&Path>, first: Option<&Path>) -> CommandBuilder {
    command_for(profile, cwd, first, plain_home())
}

/// [`build_command`], its shell plain where `plain` names the folder it
/// starts in and calls home ([`make_shells_plain`]).
fn command_for(profile: &ShellProfile, cwd: Option<&Path>, first: Option<&Path>, plain: Option<&Path>) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&profile.program);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", crate::APP_ID);
    cmd.env("TERM_PROGRAM_VERSION", crate::version::VERSION);
    // Flow shows the progress programs report (OSC 9;4), and Cargo only
    // reports it to terminals it recognizes.
    cmd.env("CARGO_TERM_PROGRESS_TERM_INTEGRATION", "true");
    // The language Flow speaks, for flow-help to print its pages in.
    cmd.env("FLOW_LANGUAGE", crate::i18n::language());
    for (name, value) in SHELL_ENV.get().into_iter().flatten() {
        cmd.env(name, value);
    }
    let dirs = games_dirs(plain.is_some());
    cmd.env("FLOW_GAMES", games_paths(&dirs, profile.flavor));
    match first {
        Some(first) => cmd.env("FLOW_FIRST", games_path(first, profile.flavor)),
        // Not one from a Flow, or a tab, this one was started in.
        None => cmd.env_remove("FLOW_FIRST"),
    }
    if let Some(path) = path_with(first, &dirs) {
        cmd.env("PATH", path);
    }
    let apps_man = if plain.is_some() { Vec::new() } else { crate::apps::man_dirs(&crate::apps::dir()) };
    if let Some(man) = man_path(std::env::var_os("MANPATH"), &apps_man) {
        cmd.env("MANPATH", man);
    }
    if let Some(home) = plain {
        // The hooks of bash and BusyBox read none of the user's startup
        // files, and bash's prompt is its own (`\s-\v\$ `); every shell's
        // history and line editor's settings are the made-up home's.
        cmd.env("FLOW_SKIP_PROFILE", "1");
        cmd.env("HOME", home);
    }
    let prompt = PROMPT.read().as_deref().map(|t| crate::looks::prompt_for(profile.kind, profile.flavor, t));
    match prompt.as_ref().filter(|_| profile.kind != ShellKind::Cmd) {
        // BusyBox turns every backslash in the variables it starts with into
        // a slash, so there they go as ^A, which its hook turns back.
        Some(prompt) if profile.flavor == BashFlavor::BusyBox => cmd.env("FLOW_PROMPT", prompt.replace('\\', "\u{1}")),
        Some(prompt) => cmd.env("FLOW_PROMPT", prompt),
        // Not one from a Flow this one was started in, wearing a look.
        None => cmd.env_remove("FLOW_PROMPT"),
    }
    match profile.kind {
        ShellKind::Cmd => {
            // Literal BEL terminators: ConPTY mangles `ESC \`.
            let shown = prompt.as_deref().unwrap_or("$P$G");
            cmd.env("PROMPT", format!("$e]133;D\x07$e]133;A\x07$e]633;P;Cwd=$P\x07{shown}$e]133;B\x07"));
            // Plain: none of the AutoRun commands the registry gives it.
            if plain.is_some() {
                cmd.arg("/D");
            }
        }
        ShellKind::PowerShell => {
            // Plain: none of the user's profiles, which may set the prompt.
            if plain.is_some() {
                cmd.arg("-NoProfile");
            }
            cmd.args(["-NoLogo", "-NoExit", "-EncodedCommand"]);
            cmd.arg(encode_powershell(PS_INTEGRATION));
        }
        ShellKind::Bash if profile.flavor == BashFlavor::BusyBox => {
            // Not a login shell (-l): BusyBox would cd to the home folder.
            let rc = write_integration_file("flow.ash", ASH_INTEGRATION);
            cmd.env("ENV", posix_path(&rc, BashFlavor::BusyBox));
            // Pass escape sequences, Flow's markers included, straight through.
            cmd.env("BB_TERMINAL_MODE", "5");
            cmd.args(["sh", "-i"]);
        }
        ShellKind::Bash => {
            let rc = write_integration_file("flow.bash", BASH_INTEGRATION);
            match profile.flavor {
                BashFlavor::Wsl => {
                    // Start in the Linux home directory unless told otherwise.
                    if cwd.is_none() {
                        cmd.args(["--cd", "~"]);
                    }
                    cmd.args(["-e", "bash", "--rcfile"]);
                    cmd.arg(posix_path(&rc, BashFlavor::Wsl));
                    cmd.arg("-i");
                }
                flavor => {
                    if matches!(flavor, BashFlavor::GitBash | BashFlavor::Msys2) {
                        cmd.env("CHERE_INVOKING", "1");
                        cmd.env("MSYSTEM", "MINGW64");
                    }
                    cmd.arg("--rcfile");
                    cmd.arg(posix_path(&rc, flavor));
                    cmd.arg("-i");
                }
            }
        }
        ShellKind::Zsh => {
            let dir = write_zsh_integration();
            // Plain, the user's files are the made-up home's: none.
            let user_zdotdir = match plain {
                Some(home) => home.to_string_lossy().into_owned(),
                None => std::env::var("ZDOTDIR").ok().or_else(|| dirs::home_dir().map(|h| h.to_string_lossy().into_owned())).unwrap_or_default(),
            };
            cmd.env("FLOW_USER_ZDOTDIR", user_zdotdir);
            cmd.env("ZDOTDIR", dir.to_string_lossy().as_ref());
            cmd.arg("-l");
        }
        ShellKind::Sh => {
            cmd.arg("-l");
        }
    }
    if let Some(dir) = folder_to_begin_in(profile, cwd, plain) {
        cmd.cwd(dir);
    }
    cmd
}

/// The folder a shell of `profile` started in `cwd` begins in
/// ([`build_command`]): `cwd`, else the home folder (a snapshot's plain
/// shells' own, [`plain_home`]), but WSL given none, which goes where it
/// will (its own home). The assistant's command for a pane it opens is
/// judged in this folder, before the pane is there.
pub fn start_folder(profile: &ShellProfile, cwd: Option<&Path>) -> Option<PathBuf> {
    folder_to_begin_in(profile, cwd, plain_home())
}

/// [`start_folder`], the home folder `plain`'s where it's given.
fn folder_to_begin_in(profile: &ShellProfile, cwd: Option<&Path>, plain: Option<&Path>) -> Option<PathBuf> {
    match cwd {
        Some(dir) => Some(dir.to_path_buf()),
        None if profile.flavor == BashFlavor::Wsl => None,
        None => plain.map(Path::to_path_buf).or_else(dirs::home_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zshs_hook_puts_last_in_psvar_the_letters_a_themes_prompt_brings_from_there() {
        // looks::prompt_for writes each as %-Nv, counted from the end.
        let quoted: Vec<String> = ZSH_LITERALS.iter().map(|c| format!("'{c}'")).collect();
        assert!(ZSH_RC.contains(&format!("psvar+=({})", quoted.join(" "))));
        // and takes the four from the prompt before away first.
        let all: String = ZSH_LITERALS.iter().collect();
        assert!(ZSH_RC.contains(&format!("[[ ${{(j::)psvar[-4,-1]}} == '{all}' ]] && psvar[-4,-1]=()")));
        assert_eq!(ZSH_LITERALS.len(), 4);
    }

    #[test]
    fn the_games_of_a_flow_this_one_was_opened_from_give_way_to_its_own() {
        let join = |parts: &[&str]| std::env::join_paths(parts).unwrap();
        let ours = "/Users/me/Flow.app/Contents/Helpers";
        let theirs = "/Applications/Flow.app/Contents/Helpers";
        let apps = "/Users/me/Library/Application Support/flow-terminal/apps/bin";
        let dirs = [PathBuf::from(apps), PathBuf::from(ours)];
        let first = "/Users/me/Library/Application Support/flow-terminal/apps/first/panes/7-1";
        let made = |path: &[&str], inherited: Option<&str>| path_for_games(Some(join(path)), &inherited.map(|i| join(&[apps, i])).into_iter().collect::<Vec<_>>(), Some(Path::new(first)), &dirs).unwrap();
        assert_eq!(made(&["/usr/bin", apps, theirs, "/bin"], Some(theirs)), join(&[first, "/usr/bin", "/bin", apps, ours]), "the other Flow's games go, its own come last");
        assert_eq!(made(&["/usr/bin"], None), join(&[first, "/usr/bin", apps, ours]), "what's first ahead of all, the apps flowapp installed ahead of the games Flow carries");
        assert_eq!(made(&["/usr/bin", first, apps, ours], Some(ours)), join(&[first, "/usr/bin", apps, ours]), "its own, inherited from a tab of its own, stay once");
        // Another Flow's first folder, or another tab's, goes, as its games do.
        let theirs_first = "/Users/other/flow-terminal/apps/first/panes/9-2";
        let path = path_for_games(Some(join(&[theirs_first, "/usr/bin"])), &[join(&[theirs_first])], Some(Path::new(first)), &dirs).unwrap();
        assert_eq!(path, join(&[first, "/usr/bin", apps, ours]));
        // A shell with no folder of its own (a test's) has none first.
        let path = path_for_games(Some(join(&[theirs_first, "/usr/bin"])), &[join(&[theirs_first])], None, &dirs).unwrap();
        assert_eq!(path, join(&["/usr/bin", apps, ours]));
    }

    #[test]
    fn the_apps_man_pages_come_after_the_systems() {
        let dirs = [PathBuf::from("/apps/flow-utils/0.1.0/share/man"), PathBuf::from("/apps/flow-rogue/0.1.46/share/man")];
        let path = |inherited: Option<&str>| man_path(inherited.map(Into::into), &dirs).map(|p| p.to_string_lossy().into_owned());
        if cfg!(windows) {
            assert_eq!(path(None), None, "Windows has no man");
            return;
        }
        assert_eq!(path(None).as_deref(), Some(":/apps/flow-utils/0.1.0/share/man:/apps/flow-rogue/0.1.46/share/man"), "the system's own first, by the leading colon");
        assert_eq!(path(Some("/opt/homebrew/share/man")).as_deref(), Some("/opt/homebrew/share/man:/apps/flow-utils/0.1.0/share/man:/apps/flow-rogue/0.1.46/share/man"), "one the user set keeps its own first");
        assert_eq!(path(Some(":/apps/flow-utils/0.1.0/share/man:/apps/flow-rogue/0.1.46/share/man")), path(None), "a Flow started from another's tab doesn't add them twice");
        assert_eq!(man_path(None, &[]), None, "no app brings pages: MANPATH is left as it is");
    }

    #[test]
    fn only_shells_whose_hook_says_its_lines_get_the_panes_nonce() {
        use std::ffi::OsStr;
        let profile = |kind, flavor| ShellProfile { id: "x", label: "x".into(), kind, program: "/bin/sh".into(), flavor };
        let given = |profile: &ShellProfile, nonce: &str| {
            let mut cmd = CommandBuilder::new("/bin/sh");
            // One from a Flow, or a tab, this one was started in.
            cmd.env(NONCE, "inherited");
            cmd.env("WSLENV", "USERPROFILE/p");
            give_nonce(&mut cmd, profile, nonce);
            (cmd.get_env(NONCE).map(OsStr::to_os_string), cmd.get_env("WSLENV").map(OsStr::to_os_string))
        };
        let nonce = "0123456789abcdef0123456789abcdef";
        for (kind, flavor) in [(ShellKind::Zsh, BashFlavor::Native), (ShellKind::Bash, BashFlavor::Native), (ShellKind::Bash, BashFlavor::GitBash), (ShellKind::PowerShell, BashFlavor::Native)] {
            assert_eq!(given(&profile(kind, flavor), nonce), (Some(nonce.into()), Some("USERPROFILE/p".into())), "{kind:?} {flavor:?}");
        }
        for (kind, flavor) in [(ShellKind::Cmd, BashFlavor::Native), (ShellKind::Bash, BashFlavor::BusyBox), (ShellKind::Sh, BashFlavor::Native)] {
            assert_eq!(given(&profile(kind, flavor), nonce).0, None, "{kind:?} {flavor:?} says no line, so has no nonce");
        }
        assert_eq!(given(&profile(ShellKind::Zsh, BashFlavor::Native), "").0, None, "none to give, none inherited");
        // WSL's Linux has only what WSLENV names, once.
        let wsl = profile(ShellKind::Bash, BashFlavor::Wsl);
        assert_eq!(given(&wsl, nonce), (Some(nonce.into()), Some("USERPROFILE/p:FLOW_NONCE".into())));
        let mut cmd = CommandBuilder::new("wsl.exe");
        cmd.env("WSLENV", "FLOW_NONCE");
        give_nonce(&mut cmd, &wsl, nonce);
        assert_eq!(cmd.get_env("WSLENV"), Some(OsStr::new("FLOW_NONCE")));
        cmd.env_remove("WSLENV");
        give_nonce(&mut cmd, &wsl, nonce);
        assert_eq!(cmd.get_env("WSLENV"), Some(OsStr::new("FLOW_NONCE")));
    }

    /// Each hook that says the command line (OSC 633;E) sends the nonce
    /// after it, and with each command's end (133;D), from a variable of
    /// the shell's own that's never exported, and takes the nonce out of
    /// the environment its programs get; zsh's preexec and bash's PS0 send
    /// it with each command's start too (133;C), and bash 3.2's trap,
    /// which misses some, doesn't.
    #[test]
    fn every_hook_that_says_the_command_line_sends_the_nonce_and_keeps_it_from_programs() {
        let line = r#"printf '\033]633;E;%s;%s\007' "$c" "${__flow_nonce:-}""#;
        // bash's, with `;newest` after the nonce where it may not be the line.
        let bash_line = r#"printf '\033]633;E;%s;%s%s\007' "$c" "${__flow_nonce:-}" "${2:+;$2}""#;
        let done = r#"printf '\033]133;D;%s;%s\007\033]633;P;Cwd=%s\007' "$ec" "${__flow_nonce:-}" "$PWD""#;
        let start = r#"printf '\033]133;C;%s\007' "${__flow_nonce:-}""#;
        for (hook, taken, said) in [
            (BASH_INTEGRATION, "__flow_nonce=${FLOW_NONCE:-}\nexport -n __flow_nonce\nunset FLOW_NONCE\n", &[bash_line, done, start][..]),
            (ZSH_ENV, "__flow_nonce=${FLOW_NONCE:-}\ntypeset +x __flow_nonce\nunset FLOW_NONCE\n", &[]),
            (ZSH_RC, "", &[line, done, start]),
            (
                PS_INTEGRATION,
                "$global:__flowNonce = \"$env:FLOW_NONCE\"\n$env:FLOW_NONCE = $null\n",
                &[r#""$e]633;E;$c;$flowNonce$bel""#, r#"$e]133;D;$flowCode;$flowNonce$bel"#],
            ),
        ] {
            assert!(hook.contains(taken), "{hook}");
            assert_eq!(hook.matches("633;E;").count(), usize::from(!said.is_empty()), "{hook}");
            for said in said {
                assert_eq!(hook.matches(said).count(), 1, "{said} in {hook}");
            }
            // Every "done" a hook says carries it.
            assert_eq!(hook.matches("133;D;").count(), hook.matches("133;D;%s;%s").count() + hook.matches("133;D;$flowCode;$flowNonce").count(), "{hook}");
        }
        assert_eq!(BASH_INTEGRATION.matches(r"printf '\033]133;C\007'").count(), 1, "bash 3.2's trap's, without it");
        // Taken before the user's files, which may start programs.
        let before = |hook: &str, at: &str| hook.find("unset FLOW_NONCE").unwrap() < hook.find(at).unwrap();
        assert!(before(BASH_INTEGRATION, ". /etc/profile") && before(ZSH_ENV, ".zshenv"));
        assert_eq!(ASH_INTEGRATION.matches("633;E").count(), 0, "ash says no line, and has no nonce");
        // Every control character goes escaped, CAN and SUB among them,
        // which would end the OSC early and show the nonce.
        for hook in [BASH_INTEGRATION, ZSH_RC] {
            assert!(hook.contains("i < 32; i++") && hook.contains(r"c=${c//$'\x7f'/\\x7f}"), "{hook}");
        }
        assert!(PS_INTEGRATION.contains(r"[regex]::Replace($c, '[\x00-\x1f\x7f]'"));
    }

    /// bash's hook, before each prompt, has its readline take every byte
    /// as a letter, whatever the user's .inputrc says, says where a change
    /// of locale may have readline take them as keys there all the same,
    /// having brought the environment readline reads it from up to date
    /// first, and notes the number the next line gets once the user's own
    /// commands have run; and a line that isn't certainly the one run (the
    /// history left it out, or erasedups took an older copy out) goes as
    /// only the newest.
    #[test]
    fn bash_takes_every_byte_as_a_letter_and_says_a_line_its_history_may_have_left_out_as_only_the_newest() {
        let prompt = &BASH_INTEGRATION[BASH_INTEGRATION.find("__flow_prompt() {").unwrap()..];
        let prompt = &prompt[..prompt.find("\n}\n").unwrap()];
        let at = |text: &str| prompt.find(text).unwrap_or_else(|| panic!("{text}: {prompt}"));
        let bind = at("builtin bind 'set convert-meta off' 'set input-meta on' 'set output-meta on' 2>/dev/null || :");
        let mark = at(r"printf '\033]633;P;FlowMetaKeys=1\007'");
        // From readline 8.1's bash on, the environment brought up to date
        // at every prompt, before the bind, which starts readline the first
        // time, and before the locale is weighed.
        let newer = at("if (( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 501 )); then\n    { : \"$(:)\"; } 2>/dev/null\n");
        assert!(newer < mark && mark < bind, "{prompt}");
        assert_eq!(BASH_INTEGRATION.matches(": \"$(:)\"").count(), 1);
        // The locale, each variable exported or not, as bash started,
        // before the user's files, and as each prompt has it; the hook's own
        // first call, which comes before the first "done", says nothing.
        let locale = "\"${LC_ALL-}=${LC_ALL+${LC_ALL@a}}|${LC_CTYPE-}=${LC_CTYPE+${LC_CTYPE@a}}|${LANG-}=${LANG+${LANG@a}}\"";
        let before = BASH_INTEGRATION.find(&format!("(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 501 )) && __flow_locale={locale}")).unwrap();
        assert!(before < BASH_INTEGRATION.find(". ~/.bash_profile").unwrap());
        assert!(newer < at(&format!("l={locale}")) && at(&format!("l={locale}")) < mark);
        // The one readline goes by, among every one set and among those
        // exported.
        assert!(prompt.contains(r#"for v in "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" "$c"; do"#), "{prompt}");
        assert!(prompt.contains("[[ -z $c && ${LC_CTYPE+${LC_CTYPE@a}} == *x* ]] && c=$LC_CTYPE"), "{prompt}");
        assert!(BASH_INTEGRATION.ends_with("__flow_starting=1\n__flow_prompt\n__flow_armed=\nunset __flow_starting\n"));
        assert!(prompt.contains("__flow_histcmd=${HISTCMD:-}"), "{prompt}");
        assert_eq!(BASH_INTEGRATION.matches("__flow_histcmd=").count(), 1, "noted last before the prompt alone");
        // PS0's, where the number isn't the line's, and the old bash's trap's,
        // where the newest is what it was at the prompt.
        assert_eq!(BASH_INTEGRATION.matches(r#"__flow_send "${h#[ *] }" newest"#).count(), 1);
        assert_eq!(BASH_INTEGRATION.matches(r#"[[ $h == "$__flow_seen" ]] && said=newest"#).count(), 1);
    }

    #[test]
    fn shells_are_told_the_language_flow_speaks_for_flow_help() {
        let sh = ShellProfile { id: "sh", label: "sh".into(), kind: ShellKind::Sh, program: "/bin/sh".into(), flavor: BashFlavor::Native };
        for code in ["ja", "en"] {
            crate::i18n::set_language(code, &[]);
            assert_eq!(build_command(&sh, Some(Path::new("/")), None).get_env("FLOW_LANGUAGE"), Some(std::ffi::OsStr::new(code)));
        }
    }

    #[test]
    fn a_snapshots_shells_start_plain_in_the_home_it_made() {
        use std::ffi::{OsStr, OsString};
        let home = Path::new("/made/up/home");
        let sh = ShellProfile { id: "sh", label: "sh".into(), kind: ShellKind::Sh, program: "/bin/sh".into(), flavor: BashFlavor::Native };
        let plain = command_for(&sh, None, None, Some(home));
        assert_eq!(plain.get_env("FLOW_SKIP_PROFILE"), Some(OsStr::new("1")), "no startup files of the user's");
        assert_eq!(plain.get_env("HOME"), Some(home.as_os_str()), "nor their history");
        assert_eq!(plain.get_cwd(), Some(&OsString::from(home)), "started in the home it made");
        assert_eq!(command_for(&sh, Some(Path::new("/elsewhere")), None, Some(home)).get_cwd(), Some(&OsString::from("/elsewhere")), "or where its tab is");
        let games = plain.get_env("FLOW_GAMES").map(|g| std::env::split_paths(g).collect::<Vec<_>>()).unwrap_or_default();
        assert!(!games.contains(&crate::apps::bin_dir()), "none of flowapp's apps: {games:?}");
        let ps = ShellProfile { id: "powershell", label: "PowerShell".into(), kind: ShellKind::PowerShell, program: "pwsh".into(), flavor: BashFlavor::Native };
        let no_profile = |plain: Option<&Path>| command_for(&ps, None, None, plain).get_argv().iter().any(|a| a == "-NoProfile");
        assert!(no_profile(Some(home)), "PowerShell's profiles left out");
        // A run that isn't a snapshot's starts the user's own shells.
        assert!(!no_profile(None));
        let own = command_for(&sh, None, None, None);
        assert_eq!(own.get_env("HOME"), std::env::var_os("HOME").as_deref());
        assert_eq!(own.get_cwd(), dirs::home_dir().map(OsString::from).as_ref());
    }

    #[test]
    fn each_hook_is_kept_by_a_name_of_its_text_so_flows_never_share_one() {
        let dir = std::env::temp_dir().join(format!("flow-hooks-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = hook_path(&dir, "flow.bash", "echo a\n");
        assert_eq!(a, hook_path(&dir, "flow.bash", "echo a\n"), "the same text keeps its file");
        assert_ne!(a, hook_path(&dir, "flow.bash", "echo b\n"), "another Flow's other text has another");
        assert_ne!(a, hook_path(&dir, "flow.ash", "echo a\n"));
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("flow-") && name.ends_with(".bash") && name.len() == "flow-.bash".len() + 12 && !name.contains(' '), "{name}");
        assert_ne!(hook_name(BASH_INTEGRATION), hook_name(ASH_INTEGRATION));
        put_hook(&a, "echo a\n");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "echo a\n");
        // One left half-written is written whole again, and nothing is left
        // beside it.
        std::fs::write(&a, "ech").unwrap();
        put_hook(&a, "echo a\n");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "echo a\n");
        let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left, [name]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn wsl_paths() {
        assert_eq!(posix_path(Path::new(r"C:\Users\me\rc"), BashFlavor::Wsl), "/mnt/c/Users/me/rc");
        assert_eq!(posix_path(Path::new(r"C:\Users\me\rc"), BashFlavor::GitBash), "C:/Users/me/rc");
    }

    #[test]
    fn git_bash_gets_the_games_folder_as_its_path_spells_it() {
        let games = Path::new(r"C:\Program Files\Flow\games");
        assert_eq!(games_path(games, BashFlavor::GitBash), "/c/Program Files/Flow/games");
        assert_eq!(games_path(games, BashFlavor::Msys2), "/c/Program Files/Flow/games");
        assert_eq!(games_path(games, BashFlavor::BusyBox), games.as_os_str());
        assert_eq!(games_path(games, BashFlavor::Native), games.as_os_str());
        // With the apps' folder ahead of it, parted as each PATH parts them.
        let apps = PathBuf::from(r"C:\Users\me\AppData\Roaming\flow-terminal\apps\bin");
        let both = [apps.clone(), games.to_path_buf()];
        assert_eq!(games_paths(&both, BashFlavor::GitBash), "/c/Users/me/AppData/Roaming/flow-terminal/apps/bin:/c/Program Files/Flow/games");
        let mut busybox = apps.into_os_string();
        busybox.push(";");
        busybox.push(games.as_os_str());
        assert_eq!(games_paths(&both, BashFlavor::BusyBox), busybox);
    }

    #[test]
    fn plain_words_are_typed_as_written_and_the_rest_quoted_so_nothing_expands_again() {
        let w = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        let ssh = w(&["ssh", "-N", "-L", "7445:127.0.0.1:7445", "user@host"]);
        for kind in [ShellKind::Zsh, ShellKind::Bash, ShellKind::Sh, ShellKind::PowerShell, ShellKind::Cmd] {
            assert_eq!(quote(kind, BashFlavor::Native, &ssh).unwrap(), "ssh -N -L 7445:127.0.0.1:7445 user@host", "{kind:?}");
        }
        let odd = w(&["echo", "it's $HOME", "*", "~", "a\\b", "", "x=1"]);
        assert_eq!(quote(ShellKind::Bash, BashFlavor::Native, &odd).unwrap(), r"echo 'it'\''s $HOME' '*' '~' 'a\b' '' x=1");
        assert_eq!(quote(ShellKind::Zsh, BashFlavor::Native, &w(&["FOO=bar", "=ls"])).unwrap(), "'FOO=bar' '=ls'", "an assignment and zsh's =cmd");
        // bash expands a ~ after a = or a : in what looks like an
        // assignment, an argument's too.
        for kind in [ShellKind::Zsh, ShellKind::Bash, ShellKind::Sh] {
            assert_eq!(quote(kind, BashFlavor::Native, &w(&["printf", "a=~", "--x=~/y", "a:~", "x~"])).unwrap(), "printf 'a=~' '--x=~/y' 'a:~' 'x~'", "{kind:?}");
        }
        // PowerShell: a quoted first word runs through &, quotes doubled.
        let ps = w(&["C:\\Program Files\\x.exe", "it's", "a,b", "@x", "-D.x", "$env:HOME", "C:\\dir"]);
        assert_eq!(quote(ShellKind::PowerShell, BashFlavor::Native, &ps).unwrap(), r"& 'C:\Program Files\x.exe' 'it''s' 'a,b' '@x' '-D.x' '$env:HOME' C:\dir");
        // cmd: Windows' quoting, with carets where cmd reads outside quotes.
        assert_eq!(quote(ShellKind::Cmd, BashFlavor::Native, &w(&["echo", "a b", "a&b", r"C:\my dir\", r"C:\x"])).unwrap(), r#"echo "a b" "a&b" "C:\my dir\\" C:\x"#);
        // A `\"` turns cmd's quotes off, and they stay off into the next word.
        assert_eq!(quote(ShellKind::Cmd, BashFlavor::Native, &w(&["echo", r#"say "hi" & go"#, r#"a"b&c"#, "x&y"])).unwrap(), r#"echo "say \"hi\" & go" "a\"b^&c" "x^&y""#);
        // A % is kept from cmd's prompt by a caret, out of quotes.
        assert_eq!(quote(ShellKind::Cmd, BashFlavor::Native, &w(&["%PATH%", "50% a", "a b%"])).unwrap(), r#"^%PATH^% "50"^%" a" "a b"^%"""#);
        // Windows PowerShell 5.1 hands words to programs as they are, so
        // what it would get wrong goes as a program reads it; PowerShell 7's
        // are PowerShell's own.
        let profile = |program: &str| ShellProfile { id: "powershell", label: "PowerShell".into(), kind: ShellKind::PowerShell, program: PathBuf::from(program), flavor: BashFlavor::Native };
        let odd = w(&["C:\\Program Files\\x.exe", "", "\"\"", "a\"b", "a b", r"C:\my dir\", "it's x", "*", "x&y"]);
        assert_eq!(
            quote_for(&profile(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"), &odd).unwrap(),
            r#"& 'C:\Program Files\x.exe' '""' '"\"\""' '"a\"b"' '"a b"' '"C:\my dir\\"' '"it''s x"' '*' 'x&y'"#
        );
        let pwsh = profile(r"C:\Program Files\PowerShell\7\pwsh.exe");
        assert_eq!(quote_for(&pwsh, &odd), quote(ShellKind::PowerShell, BashFlavor::Native, &odd));
    }

    #[test]
    fn a_word_holding_a_control_character_is_typed_into_no_shell() {
        let w = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        // Every one of C0's, DEL and C1's: Enter, a line break, Esc, Tab,
        // Ctrl+C, Ctrl+U, Ctrl+D, the bell and the rest.
        let controls: Vec<char> = (0u32..0x20).chain(0x7f..0xa0).filter_map(char::from_u32).collect();
        assert_eq!(controls.len(), 65);
        let profile = |kind, program: &str, flavor| ShellProfile { id: "x", label: "x".into(), kind, program: PathBuf::from(program), flavor };
        let shells = [
            profile(ShellKind::Zsh, "/bin/zsh", BashFlavor::Native),
            profile(ShellKind::Bash, "/bin/bash", BashFlavor::Native),
            profile(ShellKind::Bash, r"C:\Program Files\Git\bin\bash.exe", BashFlavor::GitBash),
            profile(ShellKind::Bash, "wsl.exe", BashFlavor::Wsl),
            profile(ShellKind::Bash, "busybox", BashFlavor::BusyBox),
            profile(ShellKind::Sh, "/bin/sh", BashFlavor::Native),
            profile(ShellKind::PowerShell, r"C:\Program Files\PowerShell\7\pwsh.exe", BashFlavor::Native),
            profile(ShellKind::PowerShell, r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe", BashFlavor::Native),
            profile(ShellKind::Cmd, r"C:\Windows\System32\cmd.exe", BashFlavor::Native),
        ];
        for shell in &shells {
            let what = shell.program.display();
            for &c in &controls {
                // Within a word that's quoted, and a word of it alone first.
                let words = w(&["echo", "it's", &format!("a{c}b")]);
                let refused = Err(Untypable { word: 2, character: c });
                assert_eq!(quote_for(shell, &words), refused, "{what}: {:?}", c);
                assert_eq!(quote(shell.kind, shell.flavor, &words), refused, "{what}: {:?}", c);
                assert_eq!(quote_for(shell, &[c.to_string()]), Err(Untypable { word: 0, character: c }), "{what}: {:?}", c);
            }
            // The first that holds one is named, by its first.
            assert_eq!(quote_for(shell, &w(&["x", "a\r\n", "\u{3}"])), Err(Untypable { word: 1, character: '\r' }), "{what}");
            // Letters past ASCII, a no-break space and Unicode's line
            // separator are no control characters, and go as letters.
            assert!(quote_for(shell, &w(&["echo", "ünï", "a\u{a0}b", "\u{2028}", "日本", ""])).is_ok(), "{what}");
        }
        assert_eq!(typable(&w(&["ls", "-l"])), Ok(()));
        assert_eq!(Untypable { word: 0, character: '\u{1b}' }.code(), "U+001B");
        assert_eq!(Untypable { word: 0, character: '\u{9b}' }.code(), "U+009B");
    }

    #[test]
    fn letters_past_ascii_go_as_letters_and_into_a_bash_flow_didnt_start_as_bytes() {
        let w = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        // To a shell Flow started, bash among them, whose hook has its
        // readline take every byte as a letter: Japanese names show as
        // written.
        let odd = w(&["vim", "Ētouch pwned;#.txt", "日本語.txt", "it's ü!"]);
        for (kind, flavor) in [(ShellKind::Bash, BashFlavor::Native), (ShellKind::Bash, BashFlavor::GitBash), (ShellKind::Bash, BashFlavor::Wsl), (ShellKind::Zsh, BashFlavor::Native)] {
            assert_eq!(quote(kind, flavor, &odd).unwrap(), r"vim 'Ētouch pwned;#.txt' '日本語.txt' 'it'\''s ü!'", "{kind:?} {flavor:?}");
        }
        // Where what reads them may be a bash Flow didn't start (a file
        // dropped while a program runs in the pane, a shell started inside
        // it among them), each run of them as its bytes, in a zsh pane as
        // in a bash one: Ē is C4 92, whose 92 bash's readline in a C
        // locale reads as M-C-r, revert-line; ü is C3 BC and ï C3 AF.
        assert_eq!(quote_ascii(ShellKind::Bash, BashFlavor::Native, &w(&["vim", "Ētouch pwned;#.txt"])).unwrap(), r"vim $'\xc4\x92''touch pwned;#.txt'");
        assert_eq!(quote_ascii(ShellKind::Bash, BashFlavor::Native, &w(&["ünï", "it's ü!"])).unwrap(), r"$'\xc3\xbc''n'$'\xc3\xaf' 'it'\''s '$'\xc3\xbc''!'");
        let odd = w(&["Ē$(touch pwned)ą", "日本語.txt", "a\u{a0}b", "\u{2028}", "ļ'x'!!", "~ü", "=ü", "ü"]);
        let shells = [
            (ShellKind::Bash, BashFlavor::Native),
            (ShellKind::Bash, BashFlavor::GitBash),
            (ShellKind::Bash, BashFlavor::Msys2),
            (ShellKind::Bash, BashFlavor::Wsl),
            (ShellKind::Bash, BashFlavor::BusyBox),
            (ShellKind::Zsh, BashFlavor::Native),
        ];
        for (kind, flavor) in shells {
            let line = quote_ascii(kind, flavor, &odd).unwrap();
            assert!(line.is_ascii(), "{kind:?} {flavor:?}: {line}");
            assert!(!quote(kind, flavor, &odd).unwrap().is_ascii(), "{kind:?} {flavor:?}");
            let profile = ShellProfile { id: "x", label: "x".into(), kind, program: PathBuf::from("/bin/x"), flavor };
            assert_eq!(quote_ascii_for(&profile, &odd).unwrap(), line, "{kind:?} {flavor:?}");
        }
        // PowerShell and cmd have no quotes that would say them in ASCII,
        // nor sh before dash 0.5.12, which Flow starts only where there's
        // no bash or zsh: theirs stay letters, Windows PowerShell 5.1's
        // words as its own.
        let sh = ShellProfile { id: "sh", label: "sh".into(), kind: ShellKind::Sh, program: PathBuf::from("/bin/sh"), flavor: BashFlavor::Native };
        assert_eq!(quote_ascii_for(&sh, &w(&["echo", "ünï"])).unwrap(), "echo 'ünï'");
        assert_eq!(quote_ascii(ShellKind::PowerShell, BashFlavor::Native, &w(&["echo", "ünï"])).unwrap(), "echo 'ünï'");
        assert_eq!(quote_ascii(ShellKind::Cmd, BashFlavor::Native, &w(&["echo", "ünï"])).unwrap(), r#"echo "ünï""#);
        let old = ShellProfile { id: "powershell", label: "Windows PowerShell".into(), kind: ShellKind::PowerShell, program: PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"), flavor: BashFlavor::Native };
        let words = w(&["echo", "ü a", "\"q\""]);
        assert_eq!(quote_ascii_for(&old, &words).unwrap(), quote_for(&old, &words).unwrap());
        // ASCII goes as it would anyway, and a control character is
        // refused either way.
        assert_eq!(quote_ascii(ShellKind::Bash, BashFlavor::Native, &w(&["echo", "it's", "a b"])).unwrap(), r"echo 'it'\''s' 'a b'");
        assert_eq!(quote_ascii(ShellKind::Bash, BashFlavor::Native, &w(&["ü\u{3}"])), Err(Untypable { word: 0, character: '\u{3}' }));
    }

    /// What bash makes of the line it's typed, each way: each word's own
    /// bytes, in a C locale and a UTF-8 one alike. Run with `-c`, so no
    /// readline reads it; `macos/tests/shells.rs` types them at a prompt.
    #[cfg(unix)]
    #[test]
    fn bash_reads_back_each_word_as_it_was_in_any_locale() {
        let Some(bash) = ["/bin/bash", "/usr/bin/bash", "/opt/homebrew/bin/bash"].into_iter().map(Path::new).find(|p| p.exists()) else { return };
        let odd = ["Ētouch pwned;#", "Ē$(touch pwned)ą", "ünï 日本", "ļ'x'!!", "~ü", "a\u{a0}b", "it's $HOME", "a=~", "--x=~/y"];
        let words: Vec<String> = ["printf", "<%s>\\n"].iter().chain(odd.iter()).map(|w| w.to_string()).collect();
        let expected: String = odd.iter().map(|w| format!("<{w}>\n")).collect();
        for line in [quote(ShellKind::Bash, BashFlavor::Native, &words).unwrap(), quote_ascii(ShellKind::Bash, BashFlavor::Native, &words).unwrap()] {
            for locale in ["C", "POSIX", "en_US.UTF-8"] {
                let out = std::process::Command::new(bash).arg("-c").arg(&line).env("LC_ALL", locale).current_dir(std::env::temp_dir()).output().unwrap();
                assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{} in {locale}: {line}", bash.display());
            }
        }
    }

    #[test]
    fn a_command_to_type_holds_no_key_but_its_line_breaks() {
        assert_eq!(key_in("ls -l\necho done\r\n"), None);
        assert_eq!(key_in("echo 'ünï 日本 a\u{a0}b \u{2028}'"), None);
        // Every one of C0's, DEL and C1's but the line break.
        for c in (0u32..0x20).chain(0x7f..0xa0).filter_map(char::from_u32).filter(|&c| c != '\n') {
            assert_eq!(key_in(&format!("echo 'Build done: {c}touch ~/x'")), Some(c), "{c:?}");
        }
        // A lone CR is Enter mid-line, or at the end, before anything Flow
        // types after it.
        assert_eq!(key_in("echo 'a\rtouch ~/x'"), Some('\r'));
        assert_eq!(key_in("echo a\r"), Some('\r'));
        assert_eq!(key_in("echo a\r\r\n"), Some('\r'));
        assert_eq!(key_in("echo a\tb"), Some('\t'), "Tab completes");
    }

    #[test]
    fn a_command_put_at_the_prompt_to_look_at_first_keeps_its_letters_and_line_breaks_and_drops_its_keys() {
        for tabs in [false, true] {
            assert_eq!(without_keys("ls -l\necho done\r\n", tabs), "ls -l\necho done\r\n");
            assert_eq!(without_keys("echo 'ünï 日本 a\u{a0}b \u{2028}'", tabs), "echo 'ünï 日本 a\u{a0}b \u{2028}'");
            // Ctrl+U throwing away what came before, a lone CR running
            // what's there, Ctrl+O running the line, Esc starting a key:
            // none goes.
            assert_eq!(without_keys("echo hi\u{15}rm -rf ~", tabs), "echo hirm -rf ~");
            assert_eq!(without_keys("echo 'a\rtouch ~/x'\u{f}", tabs), "echo 'atouch ~/x'");
            assert_eq!(without_keys("echo a\r\r\n\u{1b}[201~", tabs), "echo a\r\n[201~");
            for c in (0u32..0x20).chain(0x7f..0xa0).filter_map(char::from_u32).filter(|&c| c != '\n' && c != '\t') {
                let left = without_keys(&format!("echo 'Build done: {c}touch ~/x'"), tabs);
                assert_eq!(key_in(&left), None, "{c:?}");
                assert_eq!(left, "echo 'Build done: touch ~/x'", "{c:?}");
            }
        }
        // A tab completes where the paste isn't bracketed; inside a
        // bracketed one it's a letter, which `awk -F'<tab>'` needs.
        assert_eq!(without_keys("awk -F'\t' '{print $2}'", false), "awk -F'' '{print $2}'");
        assert_eq!(without_keys("awk -F'\t' '{print $2}'", true), "awk -F'\t' '{print $2}'");
    }
}
