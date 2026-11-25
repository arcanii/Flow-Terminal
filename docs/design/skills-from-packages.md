# Skills from flowapp packages (0.1.42)

Built on the Mac. This is the design as it stands, with the asks that
shaped it.

## Context

0.1.41 gave the assistant the user's own skills: playbooks in
`assistant-skills/` beside config.toml (`core/src/ai/skills.rs`), listed in
the system prompt and read with `read_skill`. BACKLOG's last open line for
the assistant was "skills from `flowapp` packages, as themes come", and it
became 0.1.42's first work (decided 2026-11-16).

What was decided:
- **Both carriers.** A package of nothing but skills, installed as packages
  of themes are, and any app's own download with a `skills/` folder (an
  app of commands with a skill for them, flow-sdk with one for the SDK).
- **A first package, `flow-skills`**, of Flow's own skills, made and signed
  here as `flow-core-themes` is, published only once that's agreed.
- **Every skill listed, each turned on by itself.** It began with a switch
  per package. Seen in use, that became a section of its own with a
  switch for each skill, a hint shown whole (it had been cut off), and a
  package's whole inventory listed from the start, each skill off until
  turned on.

## What a package holds (`core/src/apps.rs`, `core/src/ai/skills.rs`)

- Any installed package's version folder (`apps/<name>/<version>/`) may hold
  `skills/`, laid out as the user's folder: `<name>.md`, or
  `<name>/SKILL.md` with files of its own, each beginning with front matter
  naming it (lowercase letters, digits and hyphens, at most 64) and saying
  what it's for (at most 300 characters). `apps::skill_dirs` finds them, as
  `apps::man_dirs` finds man pages.
- A package of nothing but skills has `skills.json` at its root
  (`{"format": 1, "package": …, "version": …}`, `skills::Manifest`), its
  `skills/`, and no `bin/`. It's kind `skills` (`apps::SKILLS`) in
  `installed.json` and lives in the catalog's `skills` map, with one
  download for `any` system; a flowapp from before reads past that map.
- `install_bytes` refuses a package of skills in which no skill reads.
  `install_file` takes one of the user's own `.tar.gz`, unsigned and
  refused with a program in it, as it takes a package of themes: its skills
  stay off until the user turns them on.

## Reading them

- `skills::gather(dir, packages, off, on)`: the user's own but those in
  `ai.skills_off`, then each installed package's in `ai.skills_on`, each
  named `<package>:<name>` so none takes the place of one of the user's;
  at most 50 in all, the user's own first. `read` finds a skill by the name
  the list gives it, and a package's folder skill lists its other files
  for `read_file`.
- The system prompt lists them, saying that those named `package:skill`
  came with apps the user installed and turned on, and that Flow's
  approvals stand over them as over the user's own. The assistant's helpers
  get the same list. `save_skill` writes only bare names, into the user's
  folder.

## Settings' Skills section (`Section::Skills`, after the Assistant's)

- **Your skills**: how many, and Open for their folder; then a switch for
  each of the user's own, on until switched off (`ai.skills_off`, by name),
  its description the hint.
- For each installed package with skills, **Skills from** and its name:
  how many of its skills are on, with **All on** and **All off** (Enter
  turns them all on, or all off where they all are); then every one of its
  skills as `<package>:<name>`, each with a switch of its own, off until
  turned on (`ai.skills_on`), its hint saying which package it came from.
  So installing an app never changes what the assistant reads.
- Both lists are under `ai`, so neither `flow-settings` nor the assistant's
  `flow_settings` can change them.
- Settings' footer shows the chosen row's whole hint, up to eight lines
  (`ui::HINT_LINES`), the list giving up the room; it showed two and cut the
  rest.

## Saying so

- An install that brings skills, none of them on, says where each is
  turned on: flowapp's line after the install (`say_skills`, which reads
  `ai.skills_on` from config.toml through `skills::turned_on`), or how many
  are on; the toast from Settings' Apps.
- `/skills` lists the skills on, then the user's switched off and each
  package's still off.
- `flowapp list` shows packages of skills under a heading of their own,
  `--json` gives them `kind` `skills`, and Settings' Apps lists them after
  the packages of themes.

## flow-skills

Flow's own package: four skills in `assets/skills/`, each a folder with its
`SKILL.md`: `theme-package` (making a package of themes), `sdk-programs`
(writing a program with Flow's SDK or its escapes), `flowapp` (using
flowapp, and making a package of skills), and `flow-settings` (Flow's
settings from a shell or a script). `examples/skill_package.rs` packs them
with `skills.json`, reading each back as Flow will, owned by no one, ready
to sign; `examples/app_catalog.rs --skills --platform any` writes its entry.
