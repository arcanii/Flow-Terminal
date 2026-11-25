#!/usr/bin/env python3
"""Turns a binary STL into the GLB Flow shows as SGI's cube of tubes.

    python3 assets/logos/stl2glb.py VectorCube.stl assets/logos/sgi-cube.glb

An STL is loose triangles with a flat normal each, so the tubes would show
every facet. Corners at the same place are joined, and each corner's normal
is the average of the faces around it that bend from its own by less than
CREASE, so the tubes are round and the tubes' ends keep their sharp rims.
The material (SGI's blue, glossy and a little metallic) is written in too.
"""

import json
import math
import struct
import sys

CREASE = math.radians(30)
COLOR = [0.56, 0.6, 0.82, 1.0]
METALLIC = 0.4
ROUGHNESS = 0.25


def read_stl(path):
    data = open(path, "rb").read()
    count = struct.unpack("<I", data[80:84])[0]
    if 84 + 50 * count != len(data):
        sys.exit(f"{path}: not a binary STL of {count} triangles")
    index, points, triangles = {}, [], []
    for i in range(count):
        values = struct.unpack("<12f", data[84 + 50 * i : 84 + 50 * i + 48])
        corners = []
        for k in range(3):
            p = values[3 + 3 * k : 6 + 3 * k]
            key = tuple(round(c, 5) for c in p)
            if key not in index:
                index[key] = len(points)
                points.append(p)
            corners.append(index[key])
        triangles.append(corners)
    return points, triangles


def sub(a, b):
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def unit(a):
    length = math.sqrt(sum(x * x for x in a)) or 1.0
    return [x / length for x in a]


def main(source, target):
    points, triangles = read_stl(source)
    # Each face's normal, as long as its area, and its direction alone.
    area = [cross(sub(points[b], points[a]), sub(points[c], points[a])) for a, b, c in triangles]
    facing = [unit(n) for n in area]
    around = [[] for _ in points]
    for f, t in enumerate(triangles):
        for v in t:
            around[v].append(f)
    limit = math.cos(CREASE)
    vertices, seen, indices = [], {}, []
    for f, t in enumerate(triangles):
        for v in t:
            near = [g for g in around[v] if sum(facing[f][i] * facing[g][i] for i in range(3)) >= limit]
            normal = unit([sum(area[g][i] for g in near) for i in range(3)])
            key = (v, tuple(round(c, 4) for c in normal))
            if key not in seen:
                seen[key] = len(vertices)
                vertices.append((points[v], normal))
            indices.append(seen[key])
    if len(vertices) > 65535:
        sys.exit(f"{len(vertices)} vertices: more than 16-bit indices hold")

    positions = b"".join(struct.pack("<3f", *p) for p, _ in vertices)
    normals = b"".join(struct.pack("<3f", *n) for _, n in vertices)
    index_bytes = struct.pack(f"<{len(indices)}H", *indices)
    index_bytes += b"\0" * (-len(index_bytes) % 4)
    binary = positions + normals + index_bytes
    low = [min(p[i] for p, _ in vertices) for i in range(3)]
    high = [max(p[i] for p, _ in vertices) for i in range(3)]
    doc = {
        "asset": {"version": "2.0", "generator": "Flow's stl2glb.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0}]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": COLOR, "metallicFactor": METALLIC, "roughnessFactor": ROUGHNESS}}],
        "buffers": [{"byteLength": len(binary)}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": len(positions), "target": 34962},
            {"buffer": 0, "byteOffset": len(positions), "byteLength": len(normals), "target": 34962},
            {"buffer": 0, "byteOffset": len(positions) + len(normals), "byteLength": 2 * len(indices), "target": 34963},
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": len(vertices), "type": "VEC3", "min": low, "max": high},
            {"bufferView": 1, "componentType": 5126, "count": len(vertices), "type": "VEC3"},
            {"bufferView": 2, "componentType": 5123, "count": len(indices), "type": "SCALAR"},
        ],
    }
    text = json.dumps(doc, separators=(",", ":")).encode()
    text += b" " * (-len(text) % 4)
    chunks = struct.pack("<II", len(text), 0x4E4F534A) + text + struct.pack("<II", len(binary), 0x004E4942) + binary
    with open(target, "wb") as out:
        out.write(struct.pack("<III", 0x46546C67, 2, 12 + len(chunks)) + chunks)
    print(f"{target}: {len(vertices)} vertices, {len(indices) // 3} triangles")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
