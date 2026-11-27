# Whole glTF files from the tools, and flat normals

Until 0.1.48, Flow's glTF readers were tried against files its own tests
built (meshopt and quantized ones from an encoder written after
meshoptimizer's), and against Draco meshes and KTX2 textures from Google's
and Binomial's encoders wrapped in those, but never against a whole file as
a glTF tool writes it. Decided 2026-11-27, for **0.1.48**: make such files
with gltfpack and gltf-transform from a few of Khronos's samples and a
forest of Flow's own, keep them in `core/src/model/testdata` (its README
says how each was made, and whose each sample is), and have the tests read
and draw them. Blender is left out: it needs an install and a script, and
the Khronos samples were exported from it.

## The files

- **The Fox** (skinned, three clips, no normals) as gltfpack packs it
  (quantized alone; `-cc` with KHR_meshopt_compression, its vertex codec
  version 1 and the quaternion and exponential filters) and as
  gltf-transform does (Draco, joints and weights among its attributes, with
  a WebP texture; meshopt, its vertex codec version 0, with filters). Its
  texture made 128 pixels square first, to keep the files small.
- **A forest** (`forest_gltf.py`): 64 nodes of one tree of two primitives,
  one mirrored, plain, then made GPU instances by gltfpack (`-mi -c`, its
  turns normalized shorts, meshopt) and by gltf-transform (`instance`,
  `quantize`).
- **SimpleInstancing** as Khronos has it (JglTF's).
- **AnisotropyStrengthTest** through gltfpack (`-c -kv`) and
  **IridescenceSuzanne** through gltf-transform (its texture 128 pixels
  WebP, meshopt with the octahedral filter).

About 360 KB in all. Between them they cover meshopt's vertex codecs 0 and
1, its index codec 1, all three filters a tool writes, Draco with skins,
quantization, WebP, KHR_texture_transform as gltfpack adds it, instancing
as both tools write it, and the two newer materials.

## What the tests hold

- `glb.rs`: every Fox has the same 576 triangles, skin and clips (gltfpack
  resamples them at 30 a second), and every triangle within 0.003 of the
  quantized one's at rest and a moment into each clip; each instanced
  forest is two parts of 64 instances, one mirrored, every copy facing the
  way its normals do, every triangle within 0.0001 of the plain forest's;
  Khronos's 125 cubes fill the box; the spheres read their seven strengths
  and keep their tangents; Suzanne reads her three films and her light.
- `render/models.rs`'s GPU test: the Foxes draw alike standing and
  walking, the forests as the plain one, the cubes, the spheres changed by
  their anisotropy the more the stronger it is (none on the row without),
  and Suzanne colored by her films.

## What they found: flat normals

A glTF primitive without normals must be drawn flat (glTF 2.0, 3.7.2.1:
a viewer MUST work out flat normals, and pass over the file's tangents);
Flow worked out smooth ones, each vertex the average of its triangles. The
Fox, whose every corner is a vertex of its own, came out flat anyway, but
gltfpack welds the corners that match and Draco shares them, so the same
fox came out smooth from those, and differently lit.

So from 0.1.48, `glb::load` gives each corner of a primitive without
normals a vertex of its own wherever triangles share one
(`shares_vertices`), spreading every attribute to the corners as it's read
(`spread`: positions, uvs, colors, joints and weights, morph targets), and
works out its normals from those, which makes them flat. A file's tangents
are passed over there and worked out from the uvs where a map needs them.
The triangles are counted against the model's 2 million before the corners
are spread, so a small file naming the same three vertices millions of
times is refused before it takes any room. An OBJ without normals stays
smooth, as OBJ readers make it.

## Tried

- The Mac (2026-11-27): the tests above, and the pictures looked at.
- To try: the GPU test on Windows and Linux, and the files placed in a live
  tab on each system (`cargo run --release --example show_model --
  core/src/model/testdata/<file> --cols 40 --rows 12` in a Flow tab; on
  Windows not yet, until models go down the pipe).
