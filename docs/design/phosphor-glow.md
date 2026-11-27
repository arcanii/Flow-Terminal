# A phosphor's floor, and its glow without the tube

The green and amber screens, and any theme with a **Phosphor**, show
everything Flow draws in one color, as a monochrome monitor did
(`palette::on_phosphor`, `ui.wgsl`'s `on_phosphor`). Two things were left
open when that came in for 0.1.38, and both were settled on 2026-11-27 for
**0.1.48**.

## The floor for saturated colors stays

`PHOSPHOR_FLOOR` shows a saturated color at least 35% as bright as its
brightest part, where its luma alone would be less. A real monitor had no
floor: by luma, pure blue is about 11% as bright as white, and blue text
on black all but vanished. Flow keeps the floor (decided 2026-11-27, over
taking it away or making it a switch), since a program's blue or red text
has to stay readable; the manual's Settings says so under **Phosphor**.

## The glow without the tube

A glow quad (the cursor's, a tab's, a program's effects over the cells) is
added over what's under it. Over bright text on a phosphor the sum passes
1 in the phosphor's strongest channel first, and the 8-bit picture clips
each channel on its own: green's green stops at full while its red and
blue go on rising, so the pixel drifts toward white, paler than one
phosphor can be. With the CRT on, the tube's last pass (`crt.wgsl`'s
`tube`) brings every pixel back onto the phosphor, as bright as its luma
and no brighter than the phosphor at full, which mends it. Without the
tube nothing did.

Decided 2026-11-27: a last pass of the phosphor's own, only while a
phosphor is worn with the CRT off (over drawing glows by `max` rather than
adding them, which needs no pass but leaves glows over text weaker).

- `Renderer::render` draws the frame into the CRT layer's frame texture
  whenever the tube is on or a phosphor is worn (`onto_phosphor`), then
  `CrtLayer::onto_phosphor` draws it onto the window through
  `fs_onto_phosphor`: the same mapping as the tube's,
  `phosphor × min(luma / luma(phosphor), 1)`, its alpha at least its
  brightest channel, as the tube's picture has.
- A shade `on_phosphor` drew comes back as it was (within a step of 8-bit
  rounding), so the pass changes only what clipped.
- It needs the frame texture alone: `CrtLayer::prepare` makes the glow's
  quarter-size pair only for the tube (`Targets::glow`), and the afterglow's
  only for a tube that lingers.
- It costs one full-screen texture and one pass a frame, and only while a
  phosphor is worn without the tube: the green and amber screens with
  Theme details' CRT off, efficient mode on them (which turns the CRT
  off), or a theme of your own with a phosphor and no CRT.
- The test twin of the mapping is `palette`'s tests' `onto_phosphor`
  (`a_glow_clipped_over_bright_text_goes_back_onto_the_phosphor_and_shades_stay`).

## Tried

- The Mac (2026-11-27): snapshots of the green screen with its CRT off,
  without and with the pass. Neither had a pixel paler than the phosphor
  (its glows there don't clip), and the pass changed the picture by at
  most a step of 255 within the window, two or three on its anti-aliased
  outline, as 8-bit rounding does. The clipped case is held by the test
  twin.
- To try: a live window on each system, the green screen with the CRT
  off and the cursor's glow over bright text; Windows' Direct3D 12 and
  Vulkan, Linux's Vulkan and OpenGL.
