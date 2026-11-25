"""Draws Flow Instruments, the letters of the Flow Instruments Home Computer
theme, and writes them as a TrueType font with plain Python.

    python3 -I instruments.py flow-instruments.ttf [sheet.png]

An early-eighties home computer drew its text on a television from a
character set of 8 by 8 dots: capitals and digits five dots wide and seven
tall, a dot clear above them and two columns clear to their right, so
letters stood apart on a screen of 32 columns; no descenders, and its
lowercase codes as small capitals five dots tall, so text kept its case.
These are drawn for Flow in that way, dot by dot below, and each dot is a
square of the outline, as `bitmap2ttf.py` makes a bitmap font's, so the
letters come out crisp at any whole multiple of eight pixels.

The optional second argument writes a sample sheet of every glyph, for
looking at.
"""
import struct, sys, zlib

DOTS = 8             # a cell is 8 by 8 dots
U = 256              # font units to a dot
UPEM = DOTS * U
GAP = 192            # the line gap unscii-8 has, which this font stands in for

# Capitals, digits and signs: seven rows of five dots, set in a cell's
# second to sixth columns and its second to eighth rows.
TALL = {
    'A': ".###. #...# #...# ##### #...# #...# #...#",
    'B': "####. #...# #...# ####. #...# #...# ####.",
    'C': ".###. #...# #.... #.... #.... #...# .###.",
    'D': "###.. #..#. #...# #...# #...# #..#. ###..",
    'E': "##### #.... #.... ####. #.... #.... #####",
    'F': "##### #.... #.... ####. #.... #.... #....",
    'G': ".###. #...# #.... #.### #...# #...# .####",
    'H': "#...# #...# #...# ##### #...# #...# #...#",
    'I': ".###. ..#.. ..#.. ..#.. ..#.. ..#.. .###.",
    'J': "..### ...#. ...#. ...#. ...#. #..#. .##..",
    'K': "#...# #..#. #.#.. ##... #.#.. #..#. #...#",
    'L': "#.... #.... #.... #.... #.... #.... #####",
    'M': "#...# ##.## #.#.# #.#.# #...# #...# #...#",
    'N': "#...# #...# ##..# #.#.# #..## #...# #...#",
    'O': ".###. #...# #...# #...# #...# #...# .###.",
    'P': "####. #...# #...# ####. #.... #.... #....",
    'Q': ".###. #...# #...# #...# #.#.# #..#. .##.#",
    'R': "####. #...# #...# ####. #.#.. #..#. #...#",
    'S': ".###. #...# #.... .###. ....# #...# .###.",
    'T': "##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#..",
    'U': "#...# #...# #...# #...# #...# #...# .###.",
    'V': "#...# #...# #...# .#.#. .#.#. ..#.. ..#..",
    'W': "#...# #...# #...# #.#.# #.#.# #.#.# .#.#.",
    'X': "#...# #...# .#.#. ..#.. .#.#. #...# #...#",
    'Y': "#...# #...# .#.#. ..#.. ..#.. ..#.. ..#..",
    'Z': "##### ....# ...#. ..#.. .#... #.... #####",
    '0': ".###. #...# #..## #.#.# ##..# #...# .###.",
    '1': "..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###.",
    '2': ".###. #...# ....# ..##. .#... #.... #####",
    '3': ".###. #...# ....# ..##. ....# #...# .###.",
    '4': "...#. ..##. .#.#. #..#. ##### ...#. ...#.",
    '5': "##### #.... ####. ....# ....# #...# .###.",
    '6': "..##. .#... #.... ####. #...# #...# .###.",
    '7': "##### ....# ...#. ..#.. .#... .#... .#...",
    '8': ".###. #...# #...# .###. #...# #...# .###.",
    '9': ".###. #...# #...# .#### ....# ...#. .##..",
    ' ': "..... ..... ..... ..... ..... ..... .....",
    '!': "..#.. ..#.. ..#.. ..#.. ..#.. ..... ..#..",
    '"': ".#.#. .#.#. .#.#. ..... ..... ..... .....",
    '#': ".#.#. .#.#. ##### .#.#. ##### .#.#. .#.#.",
    '$': "..#.. .#### #.#.. .###. ..#.# ####. ..#..",
    '%': "##... ##..# ...#. ..#.. .#... #..## ...##",
    '&': ".##.. #..#. #.#.. .#... #.#.# #..#. .##.#",
    "'": "..#.. ..#.. .#... ..... ..... ..... .....",
    '(': "...#. ..#.. .#... .#... .#... ..#.. ...#.",
    ')': ".#... ..#.. ...#. ...#. ...#. ..#.. .#...",
    '*': "..... ..#.. #.#.# .###. #.#.# ..#.. .....",
    '+': "..... ..#.. ..#.. ##### ..#.. ..#.. .....",
    ',': "..... ..... ..... ..... .##.. ..#.. .#...",
    '-': "..... ..... ..... ##### ..... ..... .....",
    '.': "..... ..... ..... ..... ..... .##.. .##..",
    '/': "..... ....# ...#. ..#.. .#... #.... .....",
    ':': "..... .##.. .##.. ..... .##.. .##.. .....",
    ';': "..... .##.. .##.. ..... .##.. ..#.. .#...",
    '<': "...#. ..#.. .#... #.... .#... ..#.. ...#.",
    '=': "..... ..... ##### ..... ##### ..... .....",
    '>': ".#... ..#.. ...#. ....# ...#. ..#.. .#...",
    '?': ".###. #...# ....# ...#. ..#.. ..... ..#..",
    '@': ".###. #...# #.### #.#.# #.### #.... .###.",
    '[': ".###. .#... .#... .#... .#... .#... .###.",
    '\\': "..... #.... .#... ..#.. ...#. ....# .....",
    ']': ".###. ...#. ...#. ...#. ...#. ...#. .###.",
    '^': "..#.. .#.#. #...# ..... ..... ..... .....",
    '_': "..... ..... ..... ..... ..... ..... #####",
    '`': ".#... ..#.. ...#. ..... ..... ..... .....",
    '{': "...## ..#.. ..#.. .#... ..#.. ..#.. ...##",
    '|': "..#.. ..#.. ..#.. ..#.. ..#.. ..#.. ..#..",
    '}': "##... ..#.. ..#.. ...#. ..#.. ..#.. ##...",
    '~': "..... ..... .#... #.#.# ...#. ..... .....",
}

# The lowercase codes as small capitals: five rows of five dots, standing
# on the same line as the capitals.
SMALL = {
    'a': ".###. #...# ##### #...# #...#",
    'b': "####. #...# ####. #...# ####.",
    'c': ".#### #.... #.... #.... .####",
    'd': "####. #...# #...# #...# ####.",
    'e': "##### #.... ####. #.... #####",
    'f': "##### #.... ####. #.... #....",
    'g': ".#### #.... #..## #...# .###.",
    'h': "#...# #...# ##### #...# #...#",
    'i': ".###. ..#.. ..#.. ..#.. .###.",
    'j': "..### ...#. ...#. #..#. .##..",
    'k': "#..#. #.#.. ##... #.#.. #..#.",
    'l': "#.... #.... #.... #.... #####",
    'm': "#...# ##.## #.#.# #...# #...#",
    'n': "#...# ##..# #.#.# #..## #...#",
    'o': ".###. #...# #...# #...# .###.",
    'p': "####. #...# ####. #.... #....",
    'q': ".###. #...# #.#.# #..#. .##.#",
    'r': "####. #...# ####. #..#. #...#",
    's': ".#### #.... .###. ....# ####.",
    't': "##### ..#.. ..#.. ..#.. ..#..",
    'u': "#...# #...# #...# #...# .###.",
    'v': "#...# #...# .#.#. .#.#. ..#..",
    'w': "#...# #...# #.#.# #.#.# .#.#.",
    'x': "#...# .#.#. ..#.. .#.#. #...#",
    'y': "#...# .#.#. ..#.. ..#.. ..#..",
    'z': "##### ...#. ..#.. .#... #####",
}

# Signs wider than five dots, as a cell of eight: the copyright sign the
# title screen carries.
WHOLE = {
    '©': "........ .#####.. #.....#. #..##.#. #.#...#. #..##.#. #.....#. .#####..",
}


def cells():
    """Each character's 8 by 8 dots, rows from the top."""
    out = {}
    for c, art in TALL.items():
        rows = art.split()
        assert len(rows) == 7 and all(len(r) == 5 for r in rows), c
        out[c] = ["." * 8] + ["." + r + ".." for r in rows]
    for c, art in SMALL.items():
        rows = art.split()
        assert len(rows) == 5 and all(len(r) == 5 for r in rows), c
        out[c] = ["." * 8] * 3 + ["." + r + ".." for r in rows]
    for c, art in WHOLE.items():
        rows = art.split()
        assert len(rows) == 8 and all(len(r) == 8 for r in rows), c
        out[c] = rows
    return out


def rects(rows):
    """The dots as rectangles in font units: each row's runs, merged down
    while the next row has the same run, so a glyph is a few contours."""
    runs = []
    for r, row in enumerate(rows):
        c = 0
        while c < DOTS:
            if row[c] == '#':
                s = c
                while c < DOTS and row[c] == '#':
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
    # The baseline is the cell's foot: row 8 is y 0.
    return [(c0 * U, (DOTS - r1) * U, c1 * U, (DOTS - r0) * U) for r0, r1, c0, c1 in merged]


def glyph(rs):
    """A simple glyph of the rectangles, clockwise as TrueType has them."""
    if not rs:
        return b'', (0, 0, 0, 0), 0
    xmin = min(r[0] for r in rs); ymin = min(r[1] for r in rs)
    xmax = max(r[2] for r in rs); ymax = max(r[3] for r in rs)
    data = struct.pack('>hhhhh', len(rs), xmin, ymin, xmax, ymax)
    data += struct.pack('>%dH' % len(rs), *[4 * i + 3 for i in range(len(rs))])
    data += struct.pack('>H', 0)
    pts = []
    for x0, y0, x1, y1 in rs:
        pts += [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
    data += bytes([1] * len(pts))
    px = py = 0
    xs = ys = b''
    for x, y in pts:
        xs += struct.pack('>h', x - px); ys += struct.pack('>h', y - py)
        px, py = x, y
    data += xs + ys
    data += b'\0' * ((4 - len(data) % 4) % 4)
    return data, (xmin, ymin, xmax, ymax), len(pts)


def font(art):
    chars = sorted(art, key=ord)
    # Glyph 0 is .notdef: a box.
    notdef = ["........", ".#####..", ".#...#..", ".#...#..", ".#...#..", ".#...#..", ".#...#..", ".#####.."]
    order = [notdef] + [art[c] for c in chars]
    glyf, loca, hmtx = b'', [0], b''
    max_points = max_contours = 0
    bbox = None
    for rows in order:
        rs = rects(rows)
        data, box, points = glyph(rs)
        glyf += data
        loca.append(len(glyf))
        hmtx += struct.pack('>Hh', UPEM, box[0])
        if rs:
            max_points = max(max_points, points); max_contours = max(max_contours, len(rs))
            bbox = box if bbox is None else (min(bbox[0], box[0]), min(bbox[1], box[1]), max(bbox[2], box[2]), max(bbox[3], box[3]))
    n = len(order)
    asc, desc = UPEM, 0
    head = struct.pack('>IIIIHHqqhhhhHHhhh', 0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0x000B, UPEM, 0, 0, *bbox, 0, 8, 2, 1, 0)
    hhea = struct.pack('>Ihhh' + 'H' + 'hhh' + 'hhh' + 'hhhh' + 'hH', 0x00010000, asc, desc, GAP, UPEM, bbox[0], 0, bbox[2], 1, 0, 0, 0, 0, 0, 0, 0, n)
    maxp = struct.pack('>IHHHHHHHHHHHHHH', 0x00010000, n, max_points, max_contours, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0)
    codes = [ord(c) for c in chars]
    os2 = struct.pack('>HhHHHhhhhhhhhhhhh', 4, UPEM, 400, 5, 0, UPEM // 2, UPEM // 2, 0, UPEM // 5, UPEM // 2, UPEM // 2, 0, UPEM // 3, U, UPEM // 4, 0, 0)
    # Panose: a monospaced text face.
    os2 += bytes([2, 0, 6, 9, 0, 0, 0, 0, 0, 0])
    os2 += struct.pack('>IIII', 3, 0, 0, 0) + b'NONE'
    os2 += struct.pack('>HHHhhhHHII', 0x40, min(codes), max(codes), asc, desc, GAP, asc, desc, 1, 0)
    # x-height and capital height: the small capitals' five dots, the capitals' seven.
    os2 += struct.pack('>hhHHH', 5 * U, 7 * U, 0, 32, 1)
    post = struct.pack('>IIhhIIIII', 0x00030000, 0, -U, U, 1, 0, 0, 0, 0)

    segs = [(c, c, (g - c) & 0xFFFF) for g, c in enumerate(codes, start=1)] + [(0xFFFF, 0xFFFF, 1)]
    segx2 = 2 * len(segs)
    sr = 2
    while sr * 2 <= len(segs):
        sr *= 2
    sub = struct.pack('>HHHHHHH', 4, 0, 0, segx2, sr * 2, sr.bit_length() - 1, segx2 - sr * 2)
    sub += struct.pack('>%dH' % len(segs), *[e for _, e, _ in segs]) + b'\0\0'
    sub += struct.pack('>%dH' % len(segs), *[s for s, _, _ in segs])
    sub += struct.pack('>%dH' % len(segs), *[d for _, _, d in segs])
    sub += struct.pack('>%dH' % len(segs), *([0] * len(segs)))
    sub = sub[:2] + struct.pack('>H', len(sub)) + sub[4:]
    cmap = struct.pack('>HHHHI', 0, 1, 3, 1, 12) + sub

    family = 'Flow Instruments'
    names = [(0, 'Copyright 2026 the Flow authors. Drawn for Flow.'), (1, family), (2, 'Regular'), (3, family + ' Regular'), (4, family + ' Regular'), (5, 'Version 1.000'), (6, 'FlowInstruments-Regular')]
    store = recs = b''
    for nid, s in names:
        e = s.encode('utf-16-be')
        recs += struct.pack('>HHHHHH', 3, 1, 0x409, nid, len(e), len(store))
        store += e
    name = struct.pack('>HHH', 0, len(names), 6 + 12 * len(names)) + recs + store

    tables = {'head': head, 'hhea': hhea, 'maxp': maxp, 'OS/2': os2, 'hmtx': hmtx, 'cmap': cmap,
              'loca': struct.pack('>%dI' % len(loca), *loca), 'glyf': glyf, 'name': name, 'post': post}

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
    body = dirs = b''
    for t in tags:
        data = tables[t]
        dirs += struct.pack('>4sIII', t.encode('latin-1'), checksum(data), offset + len(body), len(data))
        body += data + b'\0' * ((4 - len(data) % 4) % 4)
    whole = bytearray(out + dirs + body)
    adj = (0xB1B0AFBA - checksum(bytes(whole))) & 0xFFFFFFFF
    head_at = offset + sum(len(tables[t]) + (4 - len(tables[t]) % 4) % 4 for t in tags[:tags.index('head')])
    whole[head_at + 8:head_at + 12] = struct.pack('>I', adj)
    return bytes(whole), n


def sheet(art, path, scale=4):
    """Every glyph in rows of sixteen, black on the theme's cyan, as a PNG."""
    chars = sorted(art, key=ord)
    across = 16
    down = (len(chars) + across - 1) // across
    w, h = across * DOTS * scale, down * DOTS * scale
    cyan, black = (0x42, 0xEC, 0xF6), (0, 0, 0)
    pixels = [[cyan] * w for _ in range(h)]
    for i, c in enumerate(chars):
        ox, oy = (i % across) * DOTS * scale, (i // across) * DOTS * scale
        for r, row in enumerate(art[c]):
            for col, dot in enumerate(row):
                if dot == '#':
                    for y in range(scale):
                        for x in range(scale):
                            pixels[oy + r * scale + y][ox + col * scale + x] = black
    raw = b''.join(b'\0' + bytes(v for p in row for v in p) for row in pixels)
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xFFFFFFFF)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
    open(path, 'wb').write(png)


if __name__ == '__main__':
    art = cells()
    data, n = font(art)
    open(sys.argv[1], 'wb').write(data)
    print('%d glyphs, %d characters, %d bytes' % (n, n - 1, len(data)))
    if len(sys.argv) > 2:
        sheet(art, sys.argv[2])
