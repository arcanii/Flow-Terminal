# Test files made by the formats' own encoders and the glTF tools

`glb.rs`'s tests wrap the Draco and KTX2 files in GLBs they build in code,
so Flow's Draco and Basis readers are tried against what the real encoders
write, not only against draco-core's own encoder. Made on macOS with
Homebrew's bottles of Google's Draco 1.5.9 (`draco_encoder`) and Binomial's
Basis Universal 2.50 (`basisu`); neither is needed to build Flow or run its
tests.

The GLBs are whole files as the glTF tools write them, which `glb.rs`'s
tests read and `render/models.rs`'s GPU test draws
(`docs/design/gltf-tool-files.md`).

- `ball-cl0.drc`, `ball-cl7.drc`, `ball-cl10.drc`: the ball the tests build
  (`ball(12, 24)` in `glb.rs`, written as an OBJ by `ball_obj.py`, its uvs
  glTF's, v down), compressed at levels 0 (triangles one by one), 7 (the
  default, edgebreaker) and 10 (edgebreaker's valence traversal). Its
  attributes' Draco ids are POSITION 0, TEXCOORD_0 1 and NORMAL 2.
  Edgebreaker leaves out the 48 triangles of no area at the poles.

  ```bash
  python3 ball_obj.py ball.obj
  draco_encoder -i ball.obj -o ball-cl0.drc -cl 0 -qp 14 -qt 12 -qn 10
  draco_encoder -i ball.obj -o ball-cl7.drc -cl 7 -qp 14 -qt 12 -qn 10
  draco_encoder -i ball.obj -o ball-cl10.drc -cl 10 -qp 14 -qt 12 -qn 10
  ```

- `checker-etc1s.ktx2`, `checker-uastc.ktx2`: a 16 × 16 texture checked red
  (220, 30, 30) and white (240, 240, 240) in squares of 4 pixels
  (`checker_png.py`), as ETC1S (BasisLZ, one level) and as UASTC
  (Zstandard, with mipmaps).

  ```bash
  python3 checker_png.py checker.png
  basisu -ktx2 checker.png -output_file checker-etc1s.ktx2
  basisu -ktx2 -uastc checker.png -mipmap -output_file checker-uastc.ktx2
  ```

## Whole files from the glTF tools

Made on macOS on 2026-11-27 with npm's gltfpack 1.3 (meshoptimizer's, its
WebAssembly build, which has no texture compression) and
`@gltf-transform/cli` 4.5.1 (its `webp` and `resize` through sharp),
installed in a folder of their own:

```bash
npm install --ignore-scripts @gltf-transform/cli gltfpack
```

Four come from Khronos's glTF-Sample-Assets
(<https://github.com/KhronosGroup/glTF-Sample-Assets>, `Models/<name>/`),
each under its own license:

- **Fox** (`glTF-Binary/Fox.glb`): the model by PixelMannen (CC0 1.0),
  rigged and animated by @tomkranis (CC BY 4.0), converted by @AsoboStudio
  and @scurest (CC BY 4.0). Skinned, three clips (Survey, Walk, Run), a
  1024-pixel PNG, no normals.
- **SimpleInstancing** (`glTF-Binary/SimpleInstancing.glb`): by Marco Hutter
  (CC0 1.0), written by JglTF; a cube in 125 instances.
- **AnisotropyStrengthTest** (`glTF-Binary/AnisotropyStrengthTest.glb`): by
  Emmett Lalish and Ed Mackey (CC0 1.0); 49 spheres, seven strengths of
  KHR_materials_anisotropy.
- **IridescenceSuzanne** (`glTF-Binary/IridescenceSuzanne.glb`): by Mathias
  Kanzler and Pascal Schoen (CC0 1.0); three Suzannes under
  KHR_materials_iridescence, one of glass whose film's thickness is a
  texture, and a light.

The forest is Flow's own (`forest_gltf.py`): one tree, its leaves and trunk
two primitives, on 64 nodes in a grid, each turned and sized, one mirrored.

```bash
# The Fox with a texture 128 pixels square, as PNG and as WebP.
gltf-transform resize Fox.glb r.glb --width 128 --height 128
gltf-transform png r.glb fox-small.glb --quality 80
gltf-transform webp r.glb fox-small-webp.glb

gltfpack -i fox-small.glb -o fox-gltfpack.glb               # KHR_mesh_quantization
gltfpack -i fox-small.glb -o fox-gltfpack-cc.glb -cc -ce khr  # KHR_meshopt_compression: vertex codec 1, quaternion and exponential filters
gltf-transform draco fox-small-webp.glb fox-gltf-transform-draco.glb   # Draco (joints and weights too), WebP
gltf-transform meshopt fox-small.glb fox-gltf-transform-meshopt.glb    # EXT_meshopt_compression: vertex codec 0, filters

python3 forest_gltf.py forest.gltf
gltf-transform copy forest.gltf forest.glb                   # plain, a node a tree
gltfpack -i forest.gltf -o forest-gltfpack.glb -mi -c        # EXT_mesh_gpu_instancing, meshopt, turns as normalized shorts
gltf-transform instance forest.gltf forest-i.glb
gltf-transform quantize forest-i.glb forest-gltf-transform.glb   # instances, quantized

cp SimpleInstancing.glb simple-instancing.glb

# -kv keeps the spheres' tangents and uvs, which gltfpack otherwise drops
# where no texture maps them, though anisotropy needs the tangents.
gltfpack -i AnisotropyStrengthTest.glb -o anisotropy-gltfpack.glb -c -kv

gltf-transform resize IridescenceSuzanne.glb s1.glb --width 128 --height 128
gltf-transform webp s1.glb s2.glb
gltf-transform meshopt s2.glb iridescence-gltf-transform.glb   # meshopt with the octahedral filter
```
