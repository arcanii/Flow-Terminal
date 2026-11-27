"""Write a glTF of a small forest: one tree mesh, many nodes using it.

The tree is a cone of leaves over a trunk, two primitives of two
materials, flat shaded. The nodes stand on a grid, each turned and sized
a little differently, one of them mirrored, so a tool that makes them
GPU instances (gltfpack -mi, gltf-transform instance) has translations,
rotations and scales to write, and a mirrored one to deal with.
"""

import base64
import json
import math
import struct
import sys

SIDES = 12
GRID = 8


def cone(radius, bottom, top, sides):
    """A cone's sides and base as flat triangles (positions, normals)."""
    tip = (0.0, top, 0.0)
    ring = [(radius * math.cos(2 * math.pi * i / sides), bottom, radius * math.sin(2 * math.pi * i / sides)) for i in range(sides)]
    triangles = []
    for i in range(sides):
        a, b = ring[i], ring[(i + 1) % sides]
        triangles.append((a, tip, b))
        triangles.append((a, b, (0.0, bottom, 0.0)))
    return triangles


def cylinder(radius, bottom, top, sides):
    ring = [(radius * math.cos(2 * math.pi * i / sides), radius * math.sin(2 * math.pi * i / sides)) for i in range(sides)]
    triangles = []
    for i in range(sides):
        (ax, az), (bx, bz) = ring[i], ring[(i + 1) % sides]
        a0, b0, a1, b1 = (ax, bottom, az), (bx, bottom, bz), (ax, top, az), (bx, top, bz)
        triangles.append((a0, a1, b0))
        triangles.append((b0, a1, b1))
    return triangles


def flat(triangles, middle):
    """Positions and normals, three vertices a triangle, each wound
    counter-clockwise seen from outside, away from the solid's middle."""
    positions, normals = [], []
    for a, b, c in triangles:
        u = [b[i] - a[i] for i in range(3)]
        v = [c[i] - a[i] for i in range(3)]
        n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        length = math.sqrt(sum(x * x for x in n)) or 1.0
        n = [x / length for x in n]
        out = [(a[i] + b[i] + c[i]) / 3 - middle[i] for i in range(3)]
        if sum(n[i] * out[i] for i in range(3)) < 0:
            b, c, n = c, b, [-x for x in n]
        for p in (a, b, c):
            positions.append(p)
            normals.append(n)
    return positions, normals


def main(path):
    leaves = flat(cone(0.5, 0.3, 1.5, SIDES), (0.0, 0.6, 0.0))
    trunk = flat(cylinder(0.12, 0.0, 0.3, SIDES), (0.0, 0.15, 0.0))
    data = bytearray()
    views, accessors = [], []

    def add(values, kind):
        offset = len(data)
        for value in values:
            data.extend(struct.pack("<3f", *value))
        views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(data) - offset, "target": 34962})
        low = [min(v[i] for v in values) for i in range(3)]
        high = [max(v[i] for v in values) for i in range(3)]
        accessor = {"bufferView": len(views) - 1, "componentType": 5126, "count": len(values), "type": "VEC3"}
        if kind == "POSITION":
            accessor["min"], accessor["max"] = low, high
        accessors.append(accessor)
        return len(accessors) - 1

    primitives = []
    for material, (positions, normals) in enumerate((leaves, trunk)):
        primitives.append({"attributes": {"POSITION": add(positions, "POSITION"), "NORMAL": add(normals, "NORMAL")}, "material": material})

    nodes = []
    for row in range(GRID):
        for column in range(GRID):
            turn = 0.37 * (row * GRID + column)
            size = 0.8 + 0.4 * (((row * 5 + column * 3) % 7) / 6.0)
            node = {
                "mesh": 0,
                "translation": [(column - (GRID - 1) / 2) * 1.5, 0.0, (row - (GRID - 1) / 2) * 1.5],
                "rotation": [0.0, math.sin(turn / 2), 0.0, math.cos(turn / 2)],
                "scale": [size, size, size],
            }
            if row == 0 and column == 0:
                node["scale"] = [-size, size, size]
            nodes.append(node)

    gltf = {
        "asset": {"version": "2.0", "generator": "Flow's forest_gltf.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "forest", "children": list(range(1, len(nodes) + 1))}] + nodes,
        "meshes": [{"name": "tree", "primitives": primitives}],
        "materials": [
            {"name": "leaves", "pbrMetallicRoughness": {"baseColorFactor": [0.18, 0.55, 0.22, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.8}},
            {"name": "trunk", "pbrMetallicRoughness": {"baseColorFactor": [0.45, 0.3, 0.15, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.9}},
        ],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(data), "uri": "data:application/octet-stream;base64," + base64.b64encode(bytes(data)).decode()}],
    }
    with open(path, "w") as out:
        json.dump(gltf, out, indent=1)


if __name__ == "__main__":
    main(sys.argv[1])
