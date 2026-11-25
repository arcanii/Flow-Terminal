# Test files made by the formats' own encoders

`glb.rs`'s tests wrap these in GLBs they build in code, so Flow's Draco and
Basis readers are tried against what the real encoders write, not only
against draco-core's own encoder. Made on macOS with Homebrew's bottles of
Google's Draco 1.5.9 (`draco_encoder`) and Binomial's Basis Universal 2.50
(`basisu`); neither is needed to build Flow or run its tests.

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
