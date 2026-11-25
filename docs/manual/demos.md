[← Flow's manual](../../README.md#documentation)

# Demos

`flow-demo` holds demos of what Flow draws. Like the games, they use none of
Flow's code; they're built on [the SDK](sdk.md), and live
beside it in [Flow-SDK](https://github.com/arcanii/Flow-SDK)'s `demos/`, under the same licenses.
`flowapp install flow-sdk` puts `flow-demo` on Flow's tabs' PATH, with the
SDK (Flow carried it itself until 0.1.33).
Each shows off something else Flow draws: scenes in 3D sent with the
[Ratty graphics protocol](images.md#3d-models), a picture sent with
[kitty's](images.md), teletext in the terminal's own cells, Flow's
[sounds](sound.md) as a program asks for them, the glass washed in a
program's color, and a model Flow reads from a file. Each sets the liquid
moving where things happen (OSC 7731, under
[The glass reacts](liquid-styles.md#the-glass-reacts)). A scene goes anew each frame it
changes, to Flow packed into whole numbers (`KHR_mesh_quantization`), half
the size. `flow-demo` alone lists them.

| Demo | What it is |
| --- | --- |
| `flow-demo boing` | A ball checkered red and white, turning about its tilted axis, bounces from side to side before a purple grid that runs back into a floor, its shadow on the wall: after the Amiga's Boing demo of 1984. Each landing ripples the liquid and splashes it red, with a boing, and meeting a side stirs it with a thud. A click, or ← and →, sends it the other way; space stops it. |
| `flow-demo cube` | A small cube drops onto the top of a big cube of rounded blocks, three by three by three with a slim gap between them, that's missing its top right block, and rolls about it, each square it lands on lighting up until they spell an F; then it tips over the edge into the gap and fits, and the big cube is whole: after the GameCube's startup. The light runs from the top's F down the strokes of an F on every face, and the view pulls back and goes slowly once round the cube until it comes to rest. It's tinted violet glass, the far blocks showing through the near ones, the missing block a hole in shadow and the F's lit cyan from inside, its corners sparkling as it turns. m changes what it's made of as it plays, without losing its place, and 1 to 4 pick one: glass, polished metal anodised violet with the F's faces bright, matte stone hewn and chipped a little differently block to block, whose F glows like embers through the cracks in its crust of ash, each square cracked its own way, or bright jelly that squashes, sways and wobbles where the small cube lands, rolls and fits, and is still before it rests. The solid ones close their gaps on a dark core, as a real cube does. Close up, the blocks are cut more finely, so they stay round. The words below light the one shown. The landings and the fit ripple the liquid with a thud, the reveal bursts it in the F's color with a chime, and FLOW comes up below. Space plays it again. |
| `flow-demo plasma` | A plasma, the demoscene's sum of waves with a palette of 256 colors cycling through it, made in code each frame and sent as a picture with kitty's graphics protocol. Its dark bands are see-through, and it bends with the liquid as pictures do, the liquid stirred along with the plasma's eye as it wanders. A click drops a stone, which rings out through the plasma and ripples the liquid; ← and → go through its colors (rainbow, fire, sea and glass); space stops it. |
| `flow-demo teletext` | Pages of teletext, forty columns of DEC's double lines (twice as wide, the titles twice as tall too), with its mosaics, which Flow draws itself, and its eight colors over the glass: an index with FLOW in big mosaics over a wave, news turning through its subpages to a quiz whose answer `?` reveals, a clock in big mosaics keeping your machine's time, and a boat riding the waves. Type a page's number (100 to 103), or ← and →; the Fastext row's colors (r, g, y and c, or a click) go to theirs, and h holds a page's turn. A page arriving splashes the liquid in its color, the clock ripples it each minute and the boat's bow splashes it. It needs 80 × 24 cells, and looks its best in the School computer theme, whose Bedstead font is teletext's. |
| `flow-demo vector` | Glowing lines, as the arcades' vector monitors drew them around 1980: a flight low over a grid toward hills under a crescent moon, a volcano erupting among them and FLOW turning ahead in wireframe, each line a hot core inside two halos on a dark tube. It's at its best under the [CRT](liquid-styles.md#crt), whose afterglow trails the lines, which only Settings turns on, so it says so below. Each eruption strikes the liquid with lightning, a click bursts the word apart (and the liquid with it) till it gathers again, and the liquid streams past as you fly. Space stops it. |
| `flow-demo sounds` | Flow's sounds as a program asks for them, on a keyboard of DECPS's two octaves (C5 to C7, as the VT520 had them) drawn in cells, under a roll whose notes fall onto their keys. A tune plays from the roll (Ode to Joy, the start of Für Elise, or every note DECPS has, up and down; ← and → pick one, space stops it), each note sent as DECPS as it reaches its key, which lights and splashes the liquid in the note's color. The letters on the keys play notes too (`a` is C, `w` C♯, and on along the home row; z and x move them an octave), the digits play Flow's eleven sounds by name from a row of pads (`fx=sound`), each setting the liquid off its own way, and ↑ and ↓ change the volume. Flow plays a program's notes one after another, so a key's note makes the tune give way rather than fall behind. Below, it shows what it last sent, and says that Flow plays them only with [Sound](sound.md) on, and that an old machine's hum and clatter under a theme are Flow's own, which no program can ask for. It needs 72 × 22 cells. |
| `flow-demo tint` | A day passing over hills, dawn to night and round again in a little over a minute: the hills, a village, clouds, the sun, the moon and the stars are a 3D scene, and the sky is left empty, so it's Flow's glass, which the demo washes in the sky's color with `fx=tint` (deep blue at night, rose and gold at dawn and dusk, blue by day). The wash is worked out for each quarter of an hour and sent only when it changes, and Flow eases the glass toward it; leaving, the demo washes it off. Sunrise bursts the liquid gold with a chime, sunset red with two falling notes, a breeze stirs it by day, a shooting star pushes it at night, and a click ripples it in the sky's color. ← and → go an hour back or on; space stops the day. |
| `flow-demo model` | A hot-air balloon that Flow reads from a file: the demo writes it as a glTF into a folder of its own under the system's temporary one and names the file with the Ratty protocol's `path=` rather than sending it. Flow asks you first, naming the tab and the file; Allow, and it reads the file and the balloon turns where it was placed; Don't Allow, and Flow reads nothing and tells the program nothing, so the space stays empty (the words below say so). The demo turns the balloon itself, sending how it looks each frame (`u`), and its burner fires now and then and on a click: the balloon brightens, and the liquid bursts orange under it and is pushed up with a whoosh. ← and → turn it faster one way or the other; space stops it. Quitting takes the model away and deletes the file and its folder, and so does closing its tab under it; a folder that a run killed outright left behind (a crash, `kill -9`) is deleted as the next run starts, only if it holds nothing but the demo's file. |

```bash
cargo run --release -p flow-demo -- boing     # in Flow's tree, from Flow-SDK's tag
```

`q` quits any of them, and `--no-effects` leaves the liquid be (the sounds
demo's notes and sounds, and the tint demo's wash, still go, as they aren't
the liquid's). The scenes and
the plasma fill the window above their words, as far as one placement may (200
cells and 4096 pixels a side, about 8 million pixels), and follow it when it's
resized. Placements can't overlap, since a cell shows one picture, so each of a
scene's frames is the whole scene as one glTF (a few hundred to a thousand
triangles), registered again under the same id; four invisible corners keep
the model's box the picture's, so Flow fits it exactly. The plasma's picture,
about 40,000 pixels that Flow scales smoothly to its cells, goes again under
the same id each frame, about 30 a second, while the placeholder cells that
show it are written once. A frame that hasn't changed isn't sent. Teletext
rewrites only the rows that changed, ten times a second, and the sounds demo
sixty. The tint demo's scene goes twenty times a second while the day moves;
the balloon is named once, and then only how it's turned goes each frame.

The scenes and the balloon need a terminal that shows models through the
Ratty protocol, and the plasma one that shows kitty's pictures, as Flow
does; teletext and the sounds demo play in any terminal (one without DEC's
double lines shows each tall line twice), their effects and sounds in Flow
only (outside it, the sounds demo's keys light silently, and it says why).
Outside Flow the tint demo's sky isn't washed, and it says that too. On
Windows, where ConPTY drops the protocols, they ask and send their scenes,
pictures and effects down Flow's pipe for images (since Flow 0.1.25.1),
after the words below the picture; there the plasma's cells go bare, each
row of it a picture of its own. Flow.app, the Windows installer and zip, and
the Linux tarball carry `flow-demo` beside the games, at the end of its
tabs' PATH.
