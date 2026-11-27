# glTF's newer materials: anisotropy, iridescence, dispersion

Until 0.1.48, a glTF file that required `KHR_materials_anisotropy`,
`KHR_materials_iridescence` or `KHR_materials_dispersion` was refused by
name, and one that only used them was drawn without them. Decided
2026-11-27, for **0.1.48**: draw anisotropy and iridescence, read
dispersion and draw nothing for it, and leave `COLOR_1` and on passed
over.

## What limits it

A model's material had fourteen textures (`model::Tex`, `TEXTURES`), bound
at 1 and 3 to 15 beside the uniforms (0) and the sampler (2). Flow asks
the GPU for no more than OpenGL ES 3.0's limits (`render::limits`), which
let a shader stage sample sixteen textures. So two were left: one for
anisotropy's texture, and one for iridescence's two, made one.

## Anisotropy

- **Read** (`glb::read_material`, as JSON, the gltf crate having no type
  for it): `anisotropyStrength` (0 to 1; none at 0), `anisotropyRotation`
  (radians, turned from the tangent toward the bitangent) and
  `anisotropyTexture` (its way in red and green, from -1 to 1, its strength
  in blue), with its texCoord and transform, into `model::Anisotropy`.
- **Tangents**: it needs the mesh's. Where the file has none and no normal
  map or clearcoat normal map has had them worked out, they're worked out
  from the first set of uvs (`glb::tangents`); a part with no uvs at all
  has no way along its surface, and its anisotropy is dropped.
- **Drawn** (`model.wgsl`): Flow's highlights aren't GGX's, so anisotropy
  stretches them as such a surface looks. Its way in the surface (`along`)
  is the tangent turned by its rotation and its texture's way, and the
  half vector of each highlight (the broad one from above, `spot`, and the
  lights' glints, `lighting`'s `glint`) has its part along that way taken
  down by 0.9 of its strength (`stretched`) before the highlight is worked
  out, so the highlight lasts that much further along the way. With none,
  `along` is nothing and every highlight is as it was. The clearcoat's
  glaze isn't stretched.
- **Uniform**: `Draw::anisotropy` (its strength, the cosine and sine of
  its rotation, 1 with a texture) and `more.x`; its texture `Tex::Anisotropy`
  at binding 16.

## Iridescence

- **Read** (as JSON too): `iridescenceFactor` (0 to 1; none at 0),
  `iridescenceIor` (1.3 by default), `iridescenceThicknessMinimum` and
  `Maximum` (100 and 400 nm), `iridescenceTexture` (its red the strength)
  and `iridescenceThicknessTexture` (its green the thickness between them),
  into `model::Iridescence`.
- **One texture for two** (`buffers::Images::film`): where both name the
  same texture, it's that texture; else one is made, the strength's red and
  the thickness's green, white where one is missing (all its strength; its
  thickest, as glTF has a film without a thickness map), at the strength's
  size, the thickness read at the same place in its own. It's read through
  the strength map's uvs (the thickness map's where it alone is given): where
  the two read different uvs, the thickness follows the strength's, which
  is the approximation this makes.
- **Drawn**: a thin film's color (`thin_film`): for red, green and blue
  light (650, 532 and 450 nm) the light reflected from the film's two sides
  comes back as much in step as the path between them (twice its
  thickness, times its ior, times the cosine inside it) says, 0 to 1. A
  film of no thickness reflects white, so a film thinning out leaves the
  surface as it was (the real film's black at its thinnest isn't drawn).
  The glaze's reflections and a metal's whole look are multiplied by twice
  that color, white on the whole, as much as the film's strength (`film`);
  the clearcoat's glaze, over it, isn't.
- **Uniform**: `Draw::iridescence` (its strength, ior, thinnest and
  thickest) and `more.y`; its texture `Tex::Iridescence` at binding 17.

## Dispersion

`KHR_materials_dispersion` parts light through glass by its color. Flow
can't bend light through glass at all (a model's picture is laid over the
terminal, so what's behind a part is only seen through it), so there's
nothing for it to part. It's among `glb::REQUIRABLE`, so a file requiring
it shows, as it would in a viewer without it.

## `COLOR_1` and on

glTF gives vertex colors past `COLOR_0`, and instances' own attributes
such as `_ID`, no meaning: viewers read `COLOR_0` alone. Flow passes them
over, and the manual says so; the backlog's item is closed.

## Tried

- The Mac (2026-11-27): `glb`'s test reads all three from a file that
  requires them, the iridescence maps made one or shared, and anisotropy
  dropped without uvs; `buffers`' test makes the one texture; and the GPU
  test (`anisotropy_stretches_highlights_and_iridescence_colors_them_on_the_gpu`)
  draws a glossy ball's highlight stretched round it and, its way turned a
  quarter, from pole to pole, and a white ball under a 400 nm film colored
  where the bare one is grey.
- Files from the glTF tools using them (Khronos's AnisotropyStrengthTest
  through gltfpack, IridescenceSuzanne through gltf-transform) are read and
  drawn by the tests on the Mac (`docs/design/gltf-tool-files.md`).
- To try: the same GPU tests on Windows (Direct3D 12, Vulkan) and Linux
  (Vulkan, OpenGL), where the sixteen textures are every one a stage may
  sample; and those files placed in a live tab.
