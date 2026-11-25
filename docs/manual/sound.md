[← Flow's manual](../../README.md#documentation)

# Sound

Flow makes sounds of its own once **Sound** in Settings turns them on (they're
off until then), at the **Volume** below it. Each is made in code as it plays,
not recorded:

- **Cues:** a chime when a command that ran 30 seconds or more finishes while
  you're in another tab or another app, or the assistant finishes a task of
  several steps while you're elsewhere; a low tone when a command fails (in
  the tab in front, or a long one anywhere; not after Ctrl+C); and a ding when
  the assistant waits for your approval. A command the system's notification
  tells of ([The glass reacts](liquid-styles.md#the-glass-reacts)) makes neither: the
  notification is enough.
- **Effects' sounds:** the effects programs set off ([The glass reacts](liquid-styles.md#the-glass-reacts)) sound as they look,
  from where they happen, left or right: a drop for a ring, a splash for a
  burst, thunder for lightning, a rush of air for a push. Flow Mines, Flow
  Rogue and the demos sound with no change to them. They sound even when the
  liquid is off or the style is None.
- **Programs' sounds:** the notes programs play with DECPS, as the VT520 did
  (Windows Terminal plays them too), and Flow's own sounds asked for by name.
- **Liquid's sounds:** the liquid's own, quietly, from where they happen, while
  Flow is in front: rain tapping on the glass in the rain style (a hiss once
  it rains too hard to hear each drop, each tap and each stretch of the hiss
  a little its own), its storm's thunder, thunder with
  the clouds' own lightning, a bloop as a bubble reaches the surface below
  the water.
- **Machines' sounds:** while an old machine's theme is on (or your own, as
  its **Sounds** says), its own: a tube's low hum while Flow is in front
  (never quite the same from one moment to the next: its level, its
  harmonics and, a little, its pitch drift slowly, as a real set's did) and
  a click for each key (the green and amber terminals' sharper, from their
  own speaker), with a disk drive's head seeking as the home computer or the
  personal computer starts (a new tab with its start screen), a chord as the
  graphics workstation starts, the personal computer's fan whirring (its
  motor speeding up and slowing a little), and
  the workstation's printer printing each line of a command's output; or for the teletype, a
  clack for each key and each letter of its start screen, its carriage
  running back at Return and at the end of each line it types, its margin
  bell as a letter you type lands eight columns from the line's end, and its
  paper tape punch chattering as a command's output comes. What a program
  draws on the alternate screen (an editor, a game) isn't printed.

**Bell** is apart from them: what a program's bell (BEL: Ctrl+G, or a Tab with
nothing to complete) does. **Alert sound**, the default, plays the system's, as
Terminal does (on Linux, which has none Flow plays yet, Flow's own); **Flow's
bell** plays Flow's at its Volume, or under an old machine's theme with
**Machines' sounds** on, the machine's own: the teletype's bell, the green and
amber terminals' beep, the personal computer's speaker; **Off** says nothing.
Bells closer together than 0.15 seconds ring once, and a bell in a tab behind
makes that tab glow, as a long command's end does. **Bell flash** flashes the
glass over the text when the bell rings, heard or not, for a bell that's seen.
Trying the Volume or the Bell in Settings plays it.

A program picks the sound of an effect with `sound=<name>` (`sound=none` for
none), or plays one alone:

```
ESC ] 7731 ; fx=sound ; sound=<name> [; at=<col>,<row>] [; strength=<0..1>] BEL
```

The names are `drop`, `splash`, `thunder`, `whoosh`, `chime`, `low`, `ding`,
`thud`, `click`, `boing`, `bell`, `patter`, `bubble` and `clack`; `at` places it
left or right. Flow Rogue and Flow Mines name some for their effects: a
chime for a level gained or a win, a thud for a blow or a blast, a click for a
flag, thunder for a boss's fall. Notes go as
DECPS has them:

```
ESC [ <volume> ; <length> ; <note> [; <note> …] , ~
```

with the volume from 0 (silent) to 7, the length in 32nds of a second, and
each note from 1 (C5) up a semitone at a time to 25 (C7), 0 being a rest. Notes
follow one another, up to ten seconds ahead. Only the tab in front plays
programs' sounds, and named ones count toward its effects' allowance. On Linux
Flow plays through ALSA's default device, which carries the sound on to
PipeWire or PulseAudio where they run. To hear them all away from Flow,
`cargo run --release --example sounds -- <folder>` writes each to a WAV file.
