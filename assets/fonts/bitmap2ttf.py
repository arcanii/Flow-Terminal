"""Turns one strike of a bitmap-only sfnt (Apple 'bdat'/'bloc' or EBDT/EBLC,
index format 1, image format 2) into a TrueType font of outlines, each pixel
a square, so renderers that need outlines can draw it.

    python3 -I bitmap2ttf.py in.ttf out.ttf PPEM "Family" "Copyright line"
"""
import struct, sys

src, dst, ppem, family, copyright = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4], sys.argv[5]
d = open(src, 'rb').read()
n = struct.unpack('>H', d[4:6])[0]
T = {}
for i in range(n):
    tag, _, off, ln = struct.unpack('>4sIII', d[12 + 16 * i:28 + 16 * i])
    T[tag.decode('latin-1')] = (off, ln)
bdat = T.get('bdat') or T['EBDT']
bloc = T.get('bloc') or T['EBLC']

# The cmap: (3,1) or (0,*) format 4.
def cmap():
    off, _ = T['cmap']
    _, count = struct.unpack('>HH', d[off:off + 4])
    for i in range(count):
        pid, eid, so = struct.unpack('>HHI', d[off + 4 + 8 * i:off + 12 + 8 * i])
        sub = off + so
        fmt = struct.unpack('>H', d[sub:sub + 2])[0]
        if fmt != 4 or (pid, eid) not in ((3, 1), (0, 3), (0, 1), (0, 0)):
            continue
        segx2 = struct.unpack('>H', d[sub + 6:sub + 8])[0]
        seg = segx2 // 2
        ends = struct.unpack('>%dH' % seg, d[sub + 14:sub + 14 + segx2])
        starts = struct.unpack('>%dH' % seg, d[sub + 16 + segx2:sub + 16 + 2 * segx2])
        deltas = struct.unpack('>%dh' % seg, d[sub + 16 + 2 * segx2:sub + 16 + 3 * segx2])
        ro_at = sub + 16 + 3 * segx2
        ranges = struct.unpack('>%dH' % seg, d[ro_at:ro_at + segx2])
        m = {}
        for k in range(seg):
            for c in range(starts[k], ends[k] + 1):
                if c == 0xFFFF:
                    continue
                if ranges[k] == 0:
                    g = (c + deltas[k]) & 0xFFFF
                else:
                    at = ro_at + 2 * k + ranges[k] + 2 * (c - starts[k])
                    g = struct.unpack('>H', d[at:at + 2])[0]
                    if g:
                        g = (g + deltas[k]) & 0xFFFF
                if g:
                    m[c] = g
        return m
    raise SystemExit('no format 4 cmap')

codes = cmap()

# The strike.
b = bloc[0]
nsizes = struct.unpack('>I', d[b + 4:b + 8])[0]
strike = None
for s in range(nsizes):
    o = b + 8 + 48 * s
    ista, isz, nist, _ = struct.unpack('>IIII', d[o:o + 16])
    hori = struct.unpack('>bbBbbbbbbbbb', d[o + 16:o + 28])
    first, last, px, py, depth, _ = struct.unpack('>HHBBBb', d[o + 40:o + 48])
    if px == ppem:
        strike = (ista, nist, hori)
if not strike:
    raise SystemExit('no strike at %d ppem' % ppem)
ista, nist, hori = strike
asc_px, desc_px = hori[0], hori[1]
glyphs = {}  # gid -> (width, height, bx, by, advance, rows of bits)
for k in range(nist):
    f, l, ao = struct.unpack('>HHI', d[b + ista + 8 * k:b + ista + 8 * k + 8])
    hdr = b + ista + ao
    ifmt, imgfmt, imgoff = struct.unpack('>HHI', d[hdr:hdr + 8])
    if ifmt != 1 or imgfmt != 2:
        raise SystemExit('index %d image %d not handled' % (ifmt, imgfmt))
    offs = struct.unpack('>%dI' % (l - f + 2), d[hdr + 8:hdr + 8 + 4 * (l - f + 2)])
    for g in range(f, l + 1):
        a, z = offs[g - f], offs[g - f + 1]
        if z <= a:
            continue
        at = bdat[0] + imgoff + a
        h, w, bx, by, adv = struct.unpack('>BBbbB', d[at:at + 5])
        bits = d[at + 5:at + z - a]
        rows = []
        for r in range(h):
            row = []
            for c in range(w):
                i = r * w + c
                row.append((bits[i // 8] >> (7 - i % 8)) & 1 if i // 8 < len(bits) else 0)
            rows.append(row)
        glyphs[g] = (w, h, bx, by, adv, rows)

numglyphs = struct.unpack('>H', d[T['maxp'][0] + 4:T['maxp'][0] + 6])[0]
advance_px = max(set(v[4] for v in glyphs.values()), key=lambda a: sum(1 for v in glyphs.values() if v[4] == a))
U = 64
upem = ppem * U

# Outlines: runs of pixels in a row as rectangles, merged down while the
# next row has the same run.
def rects(g):
    w, h, bx, by, adv, rows = g
    runs = []
    for r, row in enumerate(rows):
        c = 0
        while c < w:
            if row[c]:
                s = c
                while c < w and row[c]:
                    c += 1
                runs.append([r, r + 1, s, c])
            else:
                c += 1
    merged = []
    for run in runs:
        for m in merged:
            if m[1] == run[0] and m[2] == run[2] and m[3] == run[3]:
                m[1] = run[1]
                break
        else:
            merged.append(run)
    out = []
    for r0, r1, c0, c1 in merged:
        x0, x1 = (bx + c0) * U, (bx + c1) * U
        y1, y0 = (by - r0) * U, (by - r1) * U
        out.append((x0, y0, x1, y1))
    return out

glyf = b''
loca = [0]
hmtx = b''
max_points = max_contours = 0
bbox = [0, 0, 0, 0]
first_bbox = True
for gid in range(numglyphs):
    g = glyphs.get(gid)
    adv = (g[4] if g else advance_px) * U
    rs = rects(g) if g else []
    if rs:
        xmin = min(r[0] for r in rs); ymin = min(r[1] for r in rs); xmax = max(r[2] for r in rs); ymax = max(r[3] for r in rs)
        data = struct.pack('>hhhhh', len(rs), xmin, ymin, xmax, ymax)
        data += struct.pack('>%dH' % len(rs), *[4 * i + 3 for i in range(len(rs))])
        data += struct.pack('>H', 0)
        pts = []
        for x0, y0, x1, y1 in rs:
            # Clockwise: up the left side, along the top, down the right.
            pts += [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
        data += bytes([1] * len(pts))
        px = py = 0
        xs = b''; ys = b''
        for x, y in pts:
            xs += struct.pack('>h', x - px); ys += struct.pack('>h', y - py)
            px, py = x, y
        data += xs + ys
        if len(data) % 4:
            data += b'\0' * (4 - len(data) % 4)
        glyf += data
        max_points = max(max_points, len(pts)); max_contours = max(max_contours, len(rs))
        if first_bbox:
            bbox = [xmin, ymin, xmax, ymax]; first_bbox = False
        else:
            bbox = [min(bbox[0], xmin), min(bbox[1], ymin), max(bbox[2], xmax), max(bbox[3], ymax)]
        lsb = xmin
    else:
        lsb = 0
    loca.append(len(glyf))
    hmtx += struct.pack('>Hh', adv, lsb)

asc, desc = asc_px * U, desc_px * U
head = struct.pack('>IIIIHHqqhhhhHHhhh', 0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0x000B, upem, 0, 0, bbox[0], bbox[1], bbox[2], bbox[3], 0, 8, 2, 1, 0)
hhea = struct.pack('>Ihhh' + 'H' + 'hhh' + 'hhh' + 'hhhh' + 'hH', 0x00010000, asc, desc, 0, advance_px * U, bbox[0], 0, bbox[2], 1, 0, 0, 0, 0, 0, 0, 0, numglyphs)
maxp = struct.pack('>IHHHHHHHHHHHHHH', 0x00010000, numglyphs, max_points, max_contours, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0)
first_c, last_c = min(codes), max(codes)
os2 = struct.pack('>HhHHHhhhhhhhhhhhh', 4, advance_px * U, 400, 5, 0, ppem * U // 2, ppem * U // 2, 0, ppem * U // 5, ppem * U // 2, ppem * U // 2, 0, ppem * U // 3, U, ppem * U // 4, 0, 0)
os2 += bytes([2, 0, 6, 9, 0, 0, 0, 0, 0, 0])
os2 += struct.pack('>IIII', 3, 0, 0, 0) + b'NONE'
os2 += struct.pack('>HHHhhhHHII', 0x40, min(first_c, 0xFFFF), min(last_c, 0xFFFF), asc, desc, 0, asc, -desc, 1, 0)
os2 += struct.pack('>hhHHH', ppem * U // 2, ppem * U * 2 // 3, 0, 32, 1)
post = struct.pack('>IIhhIIIII', 0x00030000, 0, -2 * U, U, 1, 0, 0, 0, 0)

# cmap format 4, a segment a code.
items = sorted((c, g) for c, g in codes.items() if c < 0xFFFF)
segs = [(c, c, (g - c) & 0xFFFF) for c, g in items] + [(0xFFFF, 0xFFFF, 1)]
segx2 = 2 * len(segs)
sr = 2
while sr * 2 <= len(segs):
    sr *= 2
sub = struct.pack('>HHHHHHH', 4, 0, 0, segx2, sr * 2, (sr.bit_length() - 1), segx2 - sr * 2)
sub += struct.pack('>%dH' % len(segs), *[e for _, e, _ in segs]) + b'\0\0'
sub += struct.pack('>%dH' % len(segs), *[s for s, _, _ in segs])
sub += struct.pack('>%dH' % len(segs), *[dl for _, _, dl in segs])
sub += struct.pack('>%dH' % len(segs), *([0] * len(segs)))
sub = sub[:2] + struct.pack('>H', len(sub)) + sub[4:]
cmap_t = struct.pack('>HHHHI', 0, 1, 3, 1, 12) + sub

ps = family.replace(' ', '') + '-Regular'
names = [(0, copyright), (1, family), (2, 'Regular'), (3, family + ' Regular; outlines from its %d-pixel bitmaps' % ppem), (4, family + ' Regular'), (5, 'Version 1.110'), (6, ps)]
store = b''
recs = b''
for nid, s in names:
    e = s.encode('utf-16-be')
    recs += struct.pack('>HHHHHH', 3, 1, 0x409, nid, len(e), len(store))
    store += e
name = struct.pack('>HHH', 0, len(names), 6 + 12 * len(names)) + recs + store

tables = {'head': head, 'hhea': hhea, 'maxp': maxp, 'OS/2': os2, 'hmtx': hmtx, 'cmap': cmap_t, 'loca': struct.pack('>%dI' % len(loca), *loca), 'glyf': glyf, 'name': name, 'post': post}

def checksum(b):
    b = b + b'\0' * ((4 - len(b) % 4) % 4)
    return sum(struct.unpack('>%dI' % (len(b) // 4), b)) & 0xFFFFFFFF

tags = sorted(tables)
nt = len(tags)
sr = 1
while sr * 2 <= nt:
    sr *= 2
out = struct.pack('>IHHHH', 0x00010000, nt, sr * 16, sr.bit_length() - 1, nt * 16 - sr * 16)
offset = 12 + 16 * nt
body = b''
dirs = b''
for t in tags:
    data = tables[t]
    dirs += struct.pack('>4sIII', t.encode('latin-1'), checksum(data), offset + len(body), len(data))
    body += data + b'\0' * ((4 - len(data) % 4) % 4)
font = bytearray(out + dirs + body)
adj = (0xB1B0AFBA - checksum(bytes(font))) & 0xFFFFFFFF
head_at = offset + sum(len(tables[t]) + (4 - len(tables[t]) % 4) % 4 for t in tags[:tags.index('head')])
font[head_at + 8:head_at + 12] = struct.pack('>I', adj)
open(dst, 'wb').write(bytes(font))
print('%d glyphs (%d drawn), %d codes, upem %d, advance %d px, ascent %d descent %d px, %d bytes' % (numglyphs, len(glyphs), len(codes), upem, advance_px, asc_px, desc_px, len(font)))
