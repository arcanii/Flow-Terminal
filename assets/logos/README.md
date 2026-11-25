# Logos

The old machines' makers' logos that Flow shows in their themes' boot
(and, last below, the graphics APIs' that the startup logo's banner
names)
(`core/src/logos.rs`), built into Flow. They are their owners' trademarks,
shown as a historical showcase of the computers of their time
([Trademarks](../../docs/trademarks.md), and the same notice after `assets/fonts/NOTICES.txt` in each
release's THIRD-PARTY-NOTICES.txt). Intel's and AMD's are set from the
system's fonts as Flow runs, and the Amiga's Boing Ball, for the Flow
notanAmiga CLI theme, is a ball Flow makes itself (`logos::boing_ball`: 16
checks round its leaning axis and 8 from pole to pole, in its red and white,
spinning), so they have no file here. The window systems' logos are here
too, for About's version line on Linux (`logos::windowing`), where they
only say which one Flow's window is on.

None of these files is Flow's own, and Flow's [MIT License](../../LICENSE)
doesn't cover them: each belongs to its owner, named below, and anyone
taking Flow's code takes none of them with it. Flow has no license for the
makers' logos and Kickstart's picture beyond showing them as a historical
showcase; `stl2glb.py` is Flow's own, under its license.

- `sun.png`: Sun Microsystems' emblem as it was from 1983 to 1996, for the
  Workstation theme; Sun's trademarks are Oracle's since it bought Sun in
  2010. Made from a copy of the later emblem (the same shape in blue) that
  Flow's maintainer had, recolored to the earlier purple, `#5D4A9F`, made
  square and shrunk to 512 pixels by averaging, its background clear.
- `sgi-cube.glb`: SGI's cube of tubes, for the Graphics workstation theme,
  turning as a 3D model. Made from Ali Aldubaisi's STL model of the logo,
  `VectorCube.stl` (16,466 triangles), uploaded on 6 June 2011 and shared
  for free use with no license terms, and no other permission than that
  (the logo itself is SGI's, now Hewlett Packard Enterprise's). `stl2glb.py` made it a GLB: it joins the model's
  corners, rounds the tubes' normals while keeping their ends' rims sharp,
  and writes in the material.
- `stl2glb.py`: `python3 assets/logos/stl2glb.py VectorCube.stl
  assets/logos/sgi-cube.glb`.
- `vulkan.png` and `opengl.png`: the graphics APIs' logos, which the
  banner beside the startup logo shows in place of the API's name
  (`logos::api`), one row tall. Wikimedia Commons'
  [Vulkan_logo.svg](https://commons.wikimedia.org/wiki/File:Vulkan_logo.svg)
  and [Opengl-logo.svg](https://commons.wikimedia.org/wiki/File:Opengl-logo.svg),
  each in the public domain there as simple text and shapes, and each a
  trademark: Vulkan the Khronos Group's, OpenGL Hewlett Packard
  Enterprise's (used under license by the Khronos Group). Made 96 pixels
  tall with their backgrounds clear by `rsvg-convert -h 96` (librsvg
  2.63.4); Flow averages them down to the row it shows them in.
- `metal.png`: Metal's icon for the same line, Apple's newer one (a
  rounded square from green to blue under a glassy M), a trademark of
  Apple Inc., in place of Commons'
  [Apple_Metal_logo,_version_2.svg](https://commons.wikimedia.org/wiki/File:Apple_Metal_logo,_version_2.svg)
  on the maintainer's ask (2026-11-15). It's Apple's own artwork, not a
  free copy: the maintainer gave it as a 256 × 256 PNG with a clear
  background and the icon 11 pixels in from each side, and where that
  copy was taken from wasn't written down. It was cut to the icon's own
  edges (234 × 234, nothing scaled) so it fills its square, as the one
  before it did, and stands as tall as the row allows.
- `directx.png`: the word "DirectX" of Microsoft's DirectX wordmark, for
  Direct3D 12 in the banner, which Flow draws in the text's color
  (`ApiLogo::in_ink`), as it's black. Wikimedia Commons'
  [Microsoft-DirectX-Logo-wordmark.svg](https://commons.wikimedia.org/wiki/File:Microsoft-DirectX-Logo-wordmark.svg),
  in the public domain there as simple text, a trademark of Microsoft
  Corporation. Its one path kept as it is, in a viewBox round the word
  alone (`-1 76 785 189.5`), without the small "Microsoft®" above it or
  the ® after it, which one row would blur; made 96 pixels tall (398 ×
  96), its background clear, with resvg 0.44 on Windows, where there's no
  `rsvg-convert`.
- `kickstart.png`: Kickstart 1.3's picture asking for the Workbench disk (a
  hand holding it out, its label upside down, and V1.3), for the Flow
  notanAmiga CLI theme's first screen (`logos::kickstart`), Amiga
  Corporation's. It's a copyrighted picture, not only a trademark, shown
  as it was as the machine's own first screen (unlike Flow notaTopaz,
  which is Flow's own drawing after Topaz). Cut from a capture of that screen at 640 × 400, each of
  its 320 × 200 pixels doubled, at its own pixels: the 130 × 131 from 79,
  50 to 208, 180, every pixel of the screen outside them white, in its four
  colors (white, black, `#7777CC` and `#BBBBBB`).
- `x11.png`: the X Window System's X (X11; the system is The Open Group's
  trademark), black, from Wikimedia Commons' `File:X11.svg` (by
  MarianSigler, public domain, being too simple to be copyrighted), drawn
  at 256 pixels square with ImageMagick (`magick -background none -density
  300 X11.svg -resize 256x256 -strip PNG32:x11.png`). Flow draws it in its
  text's color, so it shows on the glass.
- `wayland.png`: Wayland's logo, its white W on a yellow disc, from
  Wikimedia Commons' `File:Wayland_Logo.svg` (by Kristian Høgsberg,
  vectorised by Vulphere from Weston's `data/wayland.svg`; public domain,
  being too simple to be copyrighted, and also MIT-licensed, © Kristian
  Høgsberg, Intel Corporation, Benjamin Franzke, Collabora, Ltd., Red Hat,
  whose notice `assets/fonts/NOTICES.txt` carries into each release), drawn
  the same way.
