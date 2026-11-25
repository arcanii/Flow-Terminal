# The ball glb.rs's tests build (`ball(12, 24)`), as an OBJ: its corners,
# normals and uvs (glTF's, v down, written as they are), one index for all.
import math, sys
rings, segments = 12, 24
v, vt, vn, f = [], [], [], []
for r in range(rings + 1):
    across, up = math.sin(r / rings * math.pi), math.cos(r / rings * math.pi)
    for k in range(segments + 1):
        z, x = math.sin(k / segments * math.tau), math.cos(k / segments * math.tau)
        n = (x * across, up, z * across)
        vn.append(n); v.append((10 + n[0], -3 + n[1], 2 + n[2])); vt.append((k / segments, r / rings))
at = lambda r, k: r * (segments + 1) + k + 1
for r in range(rings):
    for k in range(segments):
        f += [(at(r, k), at(r + 1, k + 1), at(r + 1, k)), (at(r, k), at(r, k + 1), at(r + 1, k + 1))]
out = open(sys.argv[1], 'w')
for p in v: out.write('v %.7f %.7f %.7f\n' % p)
for t in vt: out.write('vt %.7f %.7f\n' % t)
for n in vn: out.write('vn %.7f %.7f %.7f\n' % n)
for t in f: out.write('f ' + ' '.join('%d/%d/%d' % (i, i, i) for i in t) + '\n')
