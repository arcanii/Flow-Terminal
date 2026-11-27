# glTF instancing drawn on the GPU

`EXT_mesh_gpu_instancing` gives a node a list of instances, each its mesh
moved, turned and sized in the node's own space before the node places
it. Until 0.1.48, Flow made a copy of the mesh's vertices for each instance
on the CPU (`glb::load`, `Rest::instances`), so every copy counted toward
the 2 million triangles a model may have, and a forest of big trees was
refused. Decided 2026-11-27, for **0.1.48**: solid and cut-out parts are
drawn once per instance on the GPU, counted per mesh; see-through parts
stay copied on the CPU, so their triangles still sort farthest first.

## Which parts

A part goes on the GPU in instances where its node has them, its node has
no skin (a skin poses each vertex by its joints, after the instance, which
one matrix per instance can't say), and its material isn't see-through
(`Material::see_through`: blended, or light through it). Everything else is
copied as before.

## What a part holds

- `Part::instances`: each instance's matrix, which takes the part's
  vertices to where that copy is drawn, in the mesh's unit box; empty for a
  part drawn once. A part with instances keeps one set of vertices, in its
  node's own space (its morph targets applied), and the matrices carry the
  instance, the node and the box: `U × N × I`, where `I` is the instance
  (`Rest::drawn`), `N` the node in the world as posed, and `U` the box's
  (less its middle, over its longest side).
- A node scaled to nothing (an animation hiding it) makes matrices that
  draw nothing, as it should; nothing needs inverting.
- `model::expand` makes the copies on the CPU from them, as Flow made them
  before (each copy's triangles numbering its own, a mirrored copy's wound
  the other way), for the tests and for whatever needs them whole.

## Posed

`Rest::drawn` holds the instances of a part on the GPU (`Rest::instances`
stays for one copied on the CPU). `Rig::pose_part` leaves such a part's
vertices in its node's space (morphs only), and `Rig::instances_at` gives
each its matrices from where the nodes are (`U × N(t) × I`), which
`Mesh::pose` hands on as `Pose::instances`. The box's sweep over the clips
(`Rig::sweep`) and the fit at rest (`Mesh::with_rig`) take every copy's
points (`each_drawn`): every vertex of every instance while that's within
`EXACT` (4 million), else each instance's corners of the part's own box,
which can only make the box a little larger.

## Counted

- `MAX_TRIANGLES` (2 million) counts a part on the GPU once, whatever its
  instances, as decided.
- Drawn, every instance counts: `glb::MAX_DRAWN` (32 million triangles
  drawn, all told) refuses a file past it, so a few bytes of instances
  can't ask a GPU for a billion triangles. A placement draws again only
  when it changes or while it's animated.
- `glb::MAX_INSTANCED` (6 million vertices) still holds the CPU's copies.

## Drawn

- `render/models.rs`: every part has a buffer of instance matrices (one,
  the identity, for a part drawn once, and for the see-through
  triangles), the pipeline's second vertex buffer, stepped by instance
  (`@location(6)` to `(9)`), and `draw_indexed` draws as many instances. A
  placement whose mesh moves has its own buffer for each such part's
  matrices as posed, as it has for posed vertices.
- `model.wgsl`'s `vs_model` puts each vertex through its instance's
  matrix, its normal through that matrix's cofactors (its inverse
  transpose up to a scale), turned by the determinant's sign, and its
  tangent through the matrix; and it hands the fragment whether the
  instance is mirrored (`mirror`, flat), so `fs_model` takes the back of
  a triangle as its front there, as the CPU's copies were wound the other
  way round.
- Picking (`pick::cast`) takes each part's instances: the ray is taken
  into each instance's own space by that matrix's inverse (where it has
  one) and cast against the part's own triangles there, which keeps how
  far along the ray each hit is.

## Left out

- Flat artwork drawn in instances on the GPU isn't made thick for `depth`
  (`Shown::mesh`): it's drawn flat.
- Instances' own attributes (`_ID` and the like) are passed over, as glTF
  gives them no meaning.

## Tried

- The Mac (2026-11-27): `glb`'s tests read `instanced_glb` as before
  through `expand`, and a forest of 100 trees of 30,000 triangles each (3
  million drawn) loads where it was refused; one past `MAX_DRAWN` is
  refused saying so; a see-through part's instances are still copied (a
  skinned one's go the same way, by the same test of the part, untried in
  a file). `pick`'s test finds each instance. The GPU test draws the
  instances and a moving node's, the mirrored one lit as the others are
  (267 against 594 with its flag left out, which the test catches).
- Forests gltfpack (`-mi`) and gltf-transform (`instance`) made instances
  of, and Khronos's SimpleInstancing, read as the plain forest and drawn
  alike by the tests on the Mac (`docs/design/gltf-tool-files.md`).
- To try: the GPU tests on Windows and Linux, and those forests placed in
  a live tab.
