[← Flow's manual](../../README.md#documentation)

# Games

Flow's games run in its tabs, made to show what the terminal can draw.
Flow itself carries none from 0.1.34: in a Flow tab, `flowapp install
flow-rogue` (or `flow-mines`, `flow-invaders`) installs one, and `flowapp
upgrade` keeps it up to date (see [Apps: flowapp](flowapp.md)). flowapp
puts each in `apps/bin` beside `config.toml`, which is on Flow's tabs'
PATH, so other terminals don't see it. An update from an older Flow,
which carried Flow Rogue and Flow Mines itself until 0.1.33, installs
those two for you.

- **Flow Rogue** (`flow-rogue`) is a roguelike in the spirit of Rogue,
  drawn in animated sprites: a town to outfit an adventurer in, the
  wilderness around it, lairs, and a dungeon to go down. Its sprites are
  kitty images in placeholder cells, its treasures turn in 3D (Ratty's
  models), its music is made as it plays, and its spells and blows set
  Flow's liquid off (see [Images](images.md) and [The glass
  reacts](liquid-styles.md#the-glass-reacts)). On Windows it sends its
  pictures down Flow's pipe for images, since ConPTY drops the escapes
  that carry them.
- **Flow Mines** (`flow-mines`) is the mine-finding puzzle, drawn in text
  over Flow's liquid: each tile opened sets off a ring or a splash, and a
  mine a blast with lightning. It plays with the keys or the mouse.
- **Flow Invaders** (`flow-invaders`) has ships coming down in waves in
  3D, fought from a truck, with a shop between the waves.

They use none of Flow's code and talk to the terminal only through escape
sequences and [Flow's SDK](sdk.md), so they also run in other terminals
with kitty images, and as letters in any terminal. Their source isn't
public: they're made in a private repository of their own, which also
makes flowapp's downloads of them, and Flow's own build takes nothing
from it.
