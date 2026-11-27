# Security

## Reporting a vulnerability

Please don't report a security hole in a public issue. Report it privately
through this repository's **Security** tab: **Report a vulnerability**. Only
the maintainer sees it there, and you can talk it through before anything is
made public.

Say what Flow does that it shouldn't, how to make it happen, and on which
system and version (**About**, or `flow --version` in a tab). A short program
or a file to `cat` that shows it helps most. Leave out API keys, tokens and
anything else secret.

There's no bounty, and no promise of a time to answer, but every report is
read, and a fix goes into the next release on each system it touches.

The same private report takes a report under Flow's [code of
conduct](CODE_OF_CONDUCT.md), with a title that starts with **Conduct:**.

## Supported versions

Only the newest release on each system (macOS, Windows and Linux, listed in
[Flow-releases](https://github.com/arcanii/Flow-releases)) gets fixes. Flow
updates itself, so that's the one most people run.

## What Flow guards, and what it doesn't

Anything printed in a tab can reach Flow: a file being `cat`ed, a remote host
over ssh, a theme's start screen, a package. [Untrusted
output](docs/design/untrusted-output.md) says what such output can't make Flow
do, how each guard works, and what each leaves. Read it first: a report of
something it already lists as left is still welcome, but it isn't new.

Two things are by design and not holes on their own:

- **The assistant's command rules are a check by pattern, not a sandbox.** In
  auto mode they ask before what they see as risky and wherever they can't
  tell what a command runs, but a command they rate low still runs as you. A
  way to get a harmful command rated low is worth reporting; that the rules
  can be fooled at all isn't news.
- **A program already running as you can do what you can.** Flow keeps output
  and other people from acting as you, not your own programs.

Updates, and the apps `flowapp` installs, are signed with Flow's own key and
checked before anything is put in place ([Updates](docs/manual/updates.md),
[flowapp](docs/manual/flowapp.md)). A way past those checks is the most
serious kind of report.
