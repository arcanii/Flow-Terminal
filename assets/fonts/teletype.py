"""Draws Flow Teletype, Flow's own font after the type a Teletype Model 33
printed, and writes it as a TrueType font with plain Python.

    python3 -I teletype.py flow-teletype.ttf [sheet.png]

The Model 33 struck its letters from a cylindrical typebox through an inked
ribbon onto roll paper, ten to the inch and six lines to the inch: a plain,
heavy gothic of capitals alone, its strokes ending round and the ink
spreading a little. No free font has them, so these are drawn from its
printouts. Each glyph is a few strokes along center lines (lines and arcs of
ellipses) on a 1000-unit em, traced by one round pen, so ends and outside
corners come out round as the ribbon left them, and where strokes meet the
ink fills the inside corners (the pen's shape grown and shrunk back by INK).
The outline is traced from distances on a grid by marching squares and
simplified, so each glyph is a set of contours that don't overlap and any
renderer draws it the same.

What the Model 33 lacked is drawn in the same hand: lowercase as small
capitals, so text keeps its case; `{ | } ~` and the grave accent; Latin-1's
letters and signs, so accented text stays in the one face. `^` and `_` are a
caret and an underscore, so file names read right, and the arrows the
Model 33 printed for those codes are at their own code points (U+2191,
U+2190), with the other two arrows to match.

The optional second argument writes a sample sheet of every glyph, read
back from the outlines, for looking at.
"""
import math, struct, sys, zlib

UPEM = 1000
ADV = 600          # every glyph's advance: ten to the inch in a cell of 0.6 em
ASC, DESC = 800, 200
HW = 49            # half the pen's width: a fairly heavy monoline
INK = 20           # the radius of the ink that fills the corners where strokes meet
O = 8              # how far round letters overshoot the flat ones
STEP = 8           # the outline's grid, in font units
TOL = 0.6          # how far a simplified outline may stray from the traced one
AXIS = 320         # the height of the hyphen, the arrows and the signs

class Box:
    """The center lines' box a letter is drawn in, so a capital and its
    small capital are one drawing."""
    def __init__(s, l, r, b, t):
        s.l, s.r, s.b, s.t = l, r, b, t
    w = property(lambda s: s.r - s.l)
    h = property(lambda s: s.t - s.b)
    cx = property(lambda s: (s.l + s.r) / 2)
    cy = property(lambda s: (s.b + s.t) / 2)
    def x(s, f):
        return s.l + f * (s.r - s.l)
    def y(s, f):
        return s.b + f * (s.t - s.b)
    def wide(s, k, dx=0):
        half = s.w * k / 2
        return Box(s.cx - half + dx, s.cx + half + dx, s.b, s.t)

# Capitals and digits: 650 tall and about 450 wide outside the pen, in a
# cell 600 wide, as the Model 33's letters sat in their tenth of an inch.
CAP = Box(125, 475, 50, 600)
# Small capitals for the lowercase codes, a little under three quarters as tall.
SC = Box(138, 462, 50, 440)
# Superscripts, ordinals and fractions' digits, drawn with a lighter pen.
SUP = Box(205, 395, 395, 650)
SUP_HW = HW - 14

class Stroke:
    def __init__(s, pts, hw=None):
        s.pts, s.hw = pts, hw

def chain(*parts, hw=None):
    """One stroke through points and runs of points, in order."""
    pts = []
    for p in parts:
        for q in ([p] if isinstance(p[0], (int, float)) else p):
            if not pts or abs(pts[-1][0] - q[0]) + abs(pts[-1][1] - q[1]) > 0.01:
                pts.append((float(q[0]), float(q[1])))
    return Stroke(pts, hw)

def dot(x, y, r=HW + 12):
    return Stroke([(float(x), float(y))], r)

def arc(cx, cy, rx, ry, a0, a1, step=6.0):
    """Points on an ellipse from angle a0 to a1 in degrees, counter-clockwise
    when a1 > a0."""
    n = max(2, int(abs(a1 - a0) / step) + 1)
    return [(cx + rx * math.cos(math.radians(a0 + (a1 - a0) * i / (n - 1))),
             cy + ry * math.sin(math.radians(a0 + (a1 - a0) * i / (n - 1)))) for i in range(n)]

def at_y(p, q, y):
    """The x where the line through p and q crosses y."""
    return p[0] + (q[0] - p[0]) * (y - p[1]) / (q[1] - p[1])

def ellipse(cx, cy, rx, ry, hw=None):
    return chain(arc(cx, cy, rx, ry, 90, 450), hw=hw)

def bowl(x0, top, bot, xr, round_=0.62):
    """From a stem at x0 along `top`, round the right side down to `bot` and
    back to the stem: the bowls of B, D, P and R."""
    ry = (top - bot) / 2
    rx = min(ry * 1.2, (xr - x0) * round_)
    return chain((x0, top), arc(xr - rx, (top + bot) / 2, rx, ry, 90, -90), (x0, bot))

def turned(strokes, cx=300, cy=CAP.cy):
    """The strokes turned half round about (cx, cy): 9 from 6, ¿ from ?."""
    return [Stroke([(2 * cx - x, 2 * cy - y) for x, y in s.pts], s.hw) for s in strokes]

def mirrored(strokes, cx=300):
    return [Stroke([(2 * cx - x, y) for x, y in s.pts], s.hw) for s in strokes]

def moved(strokes, dx=0, dy=0):
    return [Stroke([(x + dx, y + dy) for x, y in s.pts], s.hw) for s in strokes]

def pen(strokes, hw):
    """The strokes drawn with another pen, where it isn't set already."""
    return [Stroke(s.pts, s.hw or hw) for s in strokes]

# ---------------------------------------------------------------------------
# The letters, each a function of its box.

def A(b):
    b = b.wide(1.08)
    flat = 0.06 * b.w
    left, right = (b.l, b.b), (b.r, b.b)
    top_l, top_r = (b.cx - flat, b.t), (b.cx + flat, b.t)
    yb = b.y(0.34)
    return [chain(left, top_l, top_r, right), chain((at_y(left, top_l, yb), yb), (at_y(right, top_r, yb), yb))]

def B(b):
    b = b.wide(0.98, -0.01 * b.w)
    ym = b.y(0.53)
    return [chain((b.l, b.b), (b.l, b.t)), bowl(b.l, b.t, ym, b.r - 0.08 * b.w), bowl(b.l, ym, b.b, b.r)]

def C(b):
    b = b.wide(1.02, 0.02 * b.w)
    return [chain(arc(b.cx, b.cy, b.w / 2, b.h / 2 + O, 42, 318))]

def D(b):
    return [chain((b.l, b.b), (b.l, b.t)), bowl(b.l, b.t, b.b, b.r, 0.7)]

def E(b):
    b = b.wide(0.92, 0.02 * b.w)
    return [chain((b.r, b.t), (b.l, b.t), (b.l, b.b), (b.r, b.b)), chain((b.l, b.y(0.52)), (b.r - 0.12 * b.w, b.y(0.52)))]

def F(b):
    b = b.wide(0.92, 0.03 * b.w)
    return [chain((b.r, b.t), (b.l, b.t), (b.l, b.b)), chain((b.l, b.y(0.5)), (b.r - 0.14 * b.w, b.y(0.5)))]

def G(b):
    b = b.wide(1.04)
    rx, ry = b.w / 2, b.h / 2 + O
    ybar = b.y(0.44)
    return [chain(arc(b.cx, b.cy, rx, ry, 40, 345), (b.r, ybar), (b.cx + 0.08 * b.w, ybar))]

def H(b):
    return [chain((b.l, b.b), (b.l, b.t)), chain((b.r, b.b), (b.r, b.t)), chain((b.l, b.y(0.52)), (b.r, b.y(0.52)))]

def I(b):
    s = 0.3 * b.w
    return [chain((b.cx, b.b), (b.cx, b.t)), chain((b.cx - s, b.t), (b.cx + s, b.t)), chain((b.cx - s, b.b), (b.cx + s, b.b))]

def J(b):
    b = b.wide(0.96, 0.02 * b.w)
    rx = 0.47 * b.w
    ry = 0.36 * b.h
    cx = b.r - rx
    return [chain((b.r, b.t), arc(cx, b.b + ry - O, rx, ry, 0, -180), (cx - rx, b.b + ry + 0.06 * b.h))]

def K(b):
    b = b.wide(1.0, 0.02 * b.w)
    arm_end = (b.l, b.y(0.36))
    arm = chain((b.r, b.t), arm_end)
    xj = b.x(0.34)
    yj = (b.t - arm_end[1]) * (xj - b.l) / (b.r - b.l) + arm_end[1]
    return [chain((b.l, b.b), (b.l, b.t)), arm, chain((xj, yj), (b.r, b.b))]

def L(b):
    b = b.wide(0.9, 0.04 * b.w)
    return [chain((b.l, b.t), (b.l, b.b), (b.r, b.b))]

def M(b):
    b = b.wide(1.2)
    return [chain((b.l, b.b), (b.l, b.t), (b.cx, b.y(0.28)), (b.r, b.t), (b.r, b.b))]

def N(b):
    return [chain((b.l, b.b), (b.l, b.t), (b.r, b.b), (b.r, b.t))]

def O_(b):
    b = b.wide(1.16)
    return [ellipse(b.cx, b.cy, b.w / 2, b.h / 2 + O)]

def P(b):
    return [chain((b.l, b.b), (b.l, b.t)), bowl(b.l, b.t, b.y(0.44), b.r)]

def Q(b):
    b2 = b.wide(1.16)
    return O_(b) + [chain((b.x(0.62), b.y(0.22)), (b2.r + 0.04 * b.w, b.b - 0.1 * b.h))]

def R(b):
    ym = b.y(0.46)
    return [chain((b.l, b.b), (b.l, b.t)), bowl(b.l, b.t, ym, b.r), chain((b.x(0.42), ym), (b.r, b.b))]

def S(b, k=1.0):
    b = b.wide(k)
    ym = b.y(0.54)
    ryu = (b.t + O - ym) / 2
    ryl = (ym - b.b + O) / 2
    rxu, rxl = 0.47 * b.w, 0.5 * b.w
    upper = arc(b.cx + 0.01 * b.w, ym + ryu, rxu, ryu, 28, 262)
    lower = arc(b.cx, ym - ryl, rxl, ryl, 82, -152)
    return [chain(upper, lower)]

def T(b):
    b = b.wide(1.04)
    return [chain((b.l, b.t), (b.r, b.t)), chain((b.cx, b.t), (b.cx, b.b))]

def U(b):
    ry = 0.38 * b.h
    return [chain((b.l, b.t), arc(b.cx, b.b + ry - O, b.w / 2, ry, 180, 360), (b.r, b.t))]

def V(b):
    b = b.wide(1.08)
    flat = 0.05 * b.w
    return [chain((b.l, b.t), (b.cx - flat, b.b), (b.cx + flat, b.b), (b.r, b.t))]

def W(b):
    b = b.wide(1.18)
    return [chain((b.l, b.t), (b.x(0.24), b.b), (b.cx, b.y(0.66)), (b.x(0.76), b.b), (b.r, b.t))]

def X(b):
    b = b.wide(1.04)
    return [chain((b.l, b.t), (b.r, b.b)), chain((b.r, b.t), (b.l, b.b))]

def Y(b):
    b = b.wide(1.08)
    ym = b.y(0.46)
    return [chain((b.l, b.t), (b.cx, ym), (b.r, b.t)), chain((b.cx, ym), (b.cx, b.b))]

def Z(b):
    return [chain((b.l, b.t), (b.r, b.t), (b.l, b.b), (b.r, b.b))]

LETTERS = dict(A=A, B=B, C=C, D=D, E=E, F=F, G=G, H=H, I=I, J=J, K=K, L=L, M=M, N=N, O=O_, P=P, Q=Q, R=R,
               S=S, T=T, U=U, V=V, W=W, X=X, Y=Y, Z=Z)

# ---------------------------------------------------------------------------
# Digits, as tall as the capitals; 0 narrower than O, with nothing in it.

def d0(b):
    return [ellipse(b.cx, b.cy, 0.42 * b.w, b.h / 2 + O)]

def d1(b):
    x = b.cx + 0.06 * b.w
    return [chain((b.cx - 0.3 * b.w, b.t - 0.2 * b.h), (x, b.t), (x, b.b))]

def d2(b):
    ry = 0.27 * b.h
    cy = b.t - ry + O
    top = arc(b.cx, cy, b.w / 2, ry, 162, -42)
    return [chain(top, (b.l, b.b), (b.r, b.b))]

def d3(b):
    ym = b.y(0.55)
    ryu = (b.t + O - ym) / 2
    ryl = (ym - b.b + O) / 2
    upper = arc(b.cx - 0.03 * b.w, ym + ryu, 0.45 * b.w, ryu, 156, -90)
    lower = arc(b.cx, ym - ryl, 0.5 * b.w, ryl, 90, -154)
    return [chain(upper, lower), chain((b.cx - 0.2 * b.w, ym), (b.cx - 0.03 * b.w, ym))]

def d4(b):
    x = b.r - 0.14 * b.w
    yb = b.y(0.3)
    return [chain((x, b.b), (x, b.t), (b.l, yb), (b.r + 0.04 * b.w, yb))]

def d5(b):
    ry = 0.34 * b.h
    cy = b.b + ry - O
    stem = (b.l + 0.03 * b.w, b.y(0.57))
    return [chain((b.r - 0.02 * b.w, b.t), (b.l + 0.05 * b.w, b.t), stem, arc(b.cx + 0.03 * b.w, cy, 0.48 * b.w, ry, 132, -152))]

def d6(b):
    ry = 0.33 * b.h
    cy = b.b + ry - O
    return [chain(arc(b.cx, cy, b.w / 2, b.t + O - cy, 62, 180), arc(b.cx, cy, b.w / 2, ry, 180, 540))]

def d7(b):
    return [chain((b.l, b.t), (b.r, b.t), (b.cx - 0.06 * b.w, b.b))]

def d8(b):
    ym = b.y(0.56)
    ryu = (b.t + O - ym) / 2
    ryl = (ym - b.b + O) / 2
    return [ellipse(b.cx, ym + ryu, 0.42 * b.w, ryu), ellipse(b.cx, ym - ryl, 0.5 * b.w, ryl)]

def d9(b):
    return turned(d6(b), b.cx, b.cy)

DIGITS = [d0, d1, d2, d3, d4, d5, d6, d7, d8, d9]

# ---------------------------------------------------------------------------
# Signs and punctuation.

c = CAP

def comma(x, y):
    """A comma whose dot sits at (x, y): the dot, and its tail down to the left."""
    return [dot(x, y, HW + 10), Stroke([(x + 14, y - 4), (x - 10, y - 70), (x - 46, y - 132)], HW - 9)]

def chevron(x, y, dx, dy):
    return chain((x + dx, y + dy), (x, y), (x + dx, y - dy))

def arrow_up(b=c):
    head = 0.4 * b.w
    return [chain((b.cx, b.b), (b.cx, b.t)), chain((b.cx - head, b.t - head), (b.cx, b.t), (b.cx + head, b.t - head))]

def arrow_left(b=c):
    l, r = b.l - 0.06 * b.w, b.r + 0.06 * b.w
    head = 0.4 * b.w
    return [chain((l, AXIS), (r, AXIS)), chevron(l, AXIS, head, head)]

def at_sign(b=c):
    hw = HW - 13
    cx, cy = b.cx, b.cy
    rx, ry = 0.66 * b.w, b.h / 2 + 2 * O
    ix, iy, irx, iry = cx - 0.06 * b.w, cy - 0.02 * b.h, 0.21 * b.w, 0.24 * b.h
    stem = ix + irx
    foot = arc(stem + 0.14 * b.w, iy - iry + 0.1 * b.h, 0.14 * b.w, 0.1 * b.h, 180, 300)
    return [chain(arc(cx, cy, rx, ry, -16, 296), hw=hw), ellipse(ix, iy, irx, iry, hw=hw),
            chain((stem, iy + iry), (stem, iy - iry + 0.1 * b.h), foot, (cx + rx * math.cos(math.radians(-16)), cy + ry * math.sin(math.radians(-16))), hw=hw)]

def ampersand(b=c):
    hw = HW - 7
    ul = (b.cx - 0.08 * b.w, b.y(0.78), 0.3 * b.w, 0.22 * b.h + O)
    lo = (b.cx - 0.04 * b.w, b.y(0.27), 0.48 * b.w, 0.27 * b.h + O)
    def on(e, a):
        return (e[0] + e[2] * math.cos(math.radians(a)), e[1] + e[3] * math.sin(math.radians(a)))
    leg = chain((b.r + 0.04 * b.w, b.b), on(ul, 224), arc(*ul, 224, -42), hw=hw)
    body = chain(on(ul, -42), arc(*lo, 142, 376), (b.r + 0.08 * b.w, b.y(0.44)), hw=hw)
    return [leg, body]

def hash_sign(b=c):
    x1, x2 = b.cx - 0.27 * b.w, b.cx + 0.27 * b.w
    y1, y2 = b.y(0.28), b.y(0.72)
    return [chain((x1, b.b), (x1, b.t)), chain((x2, b.b), (x2, b.t)),
            chain((b.l - 0.03 * b.w, y1), (b.r + 0.03 * b.w, y1)), chain((b.l - 0.03 * b.w, y2), (b.r + 0.03 * b.w, y2))]

def dollar(b=c):
    return pen(S(Box(b.l, b.r, b.b + 0.04 * b.h, b.t - 0.04 * b.h), 1.06), HW - 9) + [chain((b.cx, b.t + 0.12 * b.h), (b.cx, b.b - 0.12 * b.h), hw=HW - 9)]

def percent(b=c):
    rx, ry = 0.24 * b.w, 0.21 * b.h
    hw = HW - 9
    return [ellipse(b.l + rx - 0.08 * b.w, b.t - ry + 0.02 * b.h, rx, ry, hw=hw), ellipse(b.r - rx + 0.08 * b.w, b.b + ry - 0.02 * b.h, rx, ry, hw=hw),
            chain((b.r + 0.02 * b.w, b.t + 0.02 * b.h), (b.l - 0.02 * b.w, b.b - 0.02 * b.h), hw=hw)]

def paren(b=c):
    return [chain(arc(b.cx + 0.62 * b.w, b.cy + 0.06 * b.h, 0.62 * b.w, 0.68 * b.h, 118, 242))]

def bracket(b=c):
    top, bot = b.t + 70, b.b - 110
    x, arm = b.cx - 0.1 * b.w, b.cx + 0.24 * b.w
    return [chain((arm, top), (x, top), (x, bot), (arm, bot))]

def brace(b=c):
    top, bot = b.t + 70, b.b - 110
    mid = (top + bot) / 2
    x = b.cx
    r = 46
    pts = [(x + 0.26 * b.w, top)] + arc(x + r, top - r, r, r, 90, 180) + arc(x - r, mid + r, r, r, 0, -90) \
        + arc(x - r, mid - r, r, r, 90, 0) + arc(x + r, bot + r, r, r, 180, 270) + [(x + 0.26 * b.w, bot)]
    return [chain((x - 0.26 * b.w, mid), (x - r, mid)), chain(pts)]

def question(b=c):
    ry = 0.27 * b.h
    cy = b.t - ry + O
    return [chain(arc(b.cx, cy, b.w / 2, ry, 158, -38), (b.cx + 0.02 * b.w, b.y(0.42)), (b.cx, b.y(0.3))), dot(b.cx, b.b + 10)]

def exclam(b=c):
    return [chain((b.cx, b.t), (b.cx, b.y(0.34))), dot(b.cx, b.b + 10)]

def tilde(y=AXIS, b=c, amp=48, hw=None):
    pts = []
    for i in range(41):
        t = i / 40
        pts.append((b.l + 0.02 * b.w + t * 0.96 * b.w, y + amp * math.sin(t * 2 * math.pi + math.pi / 6)))
    return [chain(pts, hw=hw)]

def asterisk(b=c):
    cx, cy, r = b.cx, b.y(0.6), 0.44 * b.w
    out = []
    for a in (90, 30, 150):
        dx, dy = r * math.cos(math.radians(a)), r * math.sin(math.radians(a))
        out.append(chain((cx - dx, cy - dy), (cx + dx, cy + dy), hw=HW - 9))
    return out

PUNCT = {
    ' ': lambda: [],
    '!': exclam,
    '"': lambda: [chain((c.cx - 0.25 * c.w, c.t + 20), (c.cx - 0.25 * c.w, c.t - 150)), chain((c.cx + 0.25 * c.w, c.t + 20), (c.cx + 0.25 * c.w, c.t - 150))],
    '#': hash_sign,
    '$': dollar,
    '%': percent,
    '&': ampersand,
    "'": lambda: [chain((c.cx, c.t + 20), (c.cx, c.t - 150))],
    '(': paren,
    ')': lambda: mirrored(paren()),
    '*': asterisk,
    '+': lambda: [chain((c.l, AXIS), (c.r, AXIS)), chain((c.cx, AXIS - c.w / 2), (c.cx, AXIS + c.w / 2))],
    ',': lambda: comma(c.cx + 10, c.b + 10),
    '-': lambda: [chain((c.l + 0.08 * c.w, AXIS), (c.r - 0.08 * c.w, AXIS))],
    '.': lambda: [dot(c.cx, c.b + 10)],
    '/': lambda: [chain((c.l, c.b - 0.04 * c.h), (c.r, c.t + 0.04 * c.h))],
    ':': lambda: [dot(c.cx, c.b + 10), dot(c.cx, c.y(0.62))],
    ';': lambda: comma(c.cx + 10, c.b + 10) + [dot(c.cx + 10, c.y(0.62))],
    '<': lambda: [chain((c.r - 0.04 * c.w, AXIS + 245), (c.l + 0.02 * c.w, AXIS), (c.r - 0.04 * c.w, AXIS - 245))],
    '=': lambda: [chain((c.l + 0.02 * c.w, AXIS + 96), (c.r - 0.02 * c.w, AXIS + 96)), chain((c.l + 0.02 * c.w, AXIS - 96), (c.r - 0.02 * c.w, AXIS - 96))],
    '>': lambda: mirrored([chain((c.r - 0.04 * c.w, AXIS + 245), (c.l + 0.02 * c.w, AXIS), (c.r - 0.04 * c.w, AXIS - 245))]),
    '?': question,
    '@': at_sign,
    '[': bracket,
    '\\': lambda: [chain((c.l, c.t + 0.04 * c.h), (c.r, c.b - 0.04 * c.h))],
    ']': lambda: mirrored(bracket()),
    '^': lambda: [chain((c.l + 0.04 * c.w, c.y(0.56)), (c.cx, c.t), (c.r - 0.04 * c.w, c.y(0.56)))],
    '_': lambda: [chain((40, -110), (560, -110))],
    '`': lambda: [chain((c.cx - 0.13 * c.w, c.t + 20), (c.cx + 0.1 * c.w, c.t - 120), hw=HW - 5)],
    '{': brace,
    '|': lambda: [chain((c.cx, c.t + 70), (c.cx, c.b - 110))],
    '}': lambda: mirrored(brace()),
    '~': lambda: tilde(),
}

# ---------------------------------------------------------------------------
# Accents for Latin-1's letters, over (or under) a letter's box, drawn with a
# lighter pen so they keep clear of it.

ACC_HW = HW - 12

def acc_zone(b):
    """Where an accent over a letter in box b goes: its bottom and height."""
    return b.t + HW + 62 + ACC_HW, 92

def grave(b):
    y, h = acc_zone(b)
    return [chain((b.cx - 0.13 * CAP.w, y + h), (b.cx + 0.07 * CAP.w, y), hw=ACC_HW)]

def acute(b):
    y, h = acc_zone(b)
    return [chain((b.cx - 0.07 * CAP.w, y), (b.cx + 0.13 * CAP.w, y + h), hw=ACC_HW)]

def circumflex(b):
    y, h = acc_zone(b)
    return [chain((b.cx - 0.22 * CAP.w, y), (b.cx, y + h), (b.cx + 0.22 * CAP.w, y), hw=ACC_HW)]

def tilde_acc(b):
    y, h = acc_zone(b)
    return tilde(y + h / 2, Box(b.cx - 0.27 * CAP.w, b.cx + 0.27 * CAP.w, 0, 0), amp=h / 2.4, hw=ACC_HW)

def diaeresis(b):
    y, h = acc_zone(b)
    return [dot(b.cx - 0.22 * CAP.w, y + h / 2, HW - 4), dot(b.cx + 0.22 * CAP.w, y + h / 2, HW - 4)]

def ring(b):
    y, h = acc_zone(b)
    return [ellipse(b.cx, y + h / 2 + 6, 62, 58, hw=HW - 20)]

def macron(b):
    y, h = acc_zone(b)
    return [chain((b.cx - 0.24 * CAP.w, y + h / 2), (b.cx + 0.24 * CAP.w, y + h / 2), hw=ACC_HW)]

def cedilla(b):
    x = b.cx + 0.04 * CAP.w
    return [chain((x, b.b - HW + 10), (x, -82), arc(x - 6, -125, 50, 43, 90, -100), (x - 66, -168), hw=HW - 13)]

def slashed(b):
    """Ø's stroke, corner to corner through the O."""
    bb = b.wide(1.16)
    return [chain((bb.l - 0.02 * b.w, b.b - 0.08 * b.h), (bb.r + 0.02 * b.w, b.t + 0.08 * b.h))]

def AE(b):
    b = b.wide(1.2)
    mx = b.cx + 0.04 * b.w
    left = (b.l, b.b)
    top = (mx - 0.06 * b.w, b.t)
    yb = b.y(0.36)
    return [chain(left, top, (b.r, b.t)), chain((mx, b.t), (mx, b.b), (b.r, b.b)),
            chain((mx, b.y(0.52)), (b.r - 0.08 * b.w, b.y(0.52))), chain((at_y(left, top, yb), yb), (mx, yb))]

def eth(b):
    return D(b) + [chain((b.l - 0.16 * b.w, b.y(0.5)), (b.l + 0.28 * b.w, b.y(0.5)))]

def thorn(b):
    return [chain((b.l, b.b), (b.l, b.t)), bowl(b.l, b.y(0.82), b.y(0.24), b.r)]

def sharp_s(b=c):
    """ß, as tall as a capital: a stem, its shoulder and two bumps."""
    l = b.l + 0.02 * b.w
    ym = b.y(0.58)
    top = arc(b.cx - 0.04 * b.w, b.t - 0.22 * b.h, 0.4 * b.w, 0.22 * b.h + O, 180, -62)
    lower = arc(b.cx + 0.1 * b.w, b.y(0.29), 0.4 * b.w, 0.29 * b.h + O, 90, -112)
    return [chain((l, b.b), (l, b.t - 0.22 * b.h), top, (b.cx + 0.02 * b.w, ym), lower)]

def micro(b=SC):
    ry = 0.42 * b.h
    return [chain((b.l, b.t), (b.l, b.b + ry), arc(b.cx, b.b + ry - O, b.w / 2, ry, 180, 360), (b.r, b.t)),
            chain((b.r, b.t), (b.r, b.b)), chain((b.l, b.b + ry), (b.l, -150))]

def pilcrow(b=c):
    x1, x2 = b.cx + 0.08 * b.w, b.r - 0.04 * b.w
    return [chain((x2, b.t), (x1, b.t), (x1, b.b - 0.1 * b.h)), chain((x2, b.t), (x2, b.b - 0.1 * b.h)),
            chain((x1, b.t), arc(x1, b.y(0.71), 0.48 * b.w, 0.29 * b.h, 90, 270), (x1, b.y(0.42))),
            chain((x1 - 0.06 * b.w, b.y(0.56)), (x1 - 0.06 * b.w, b.y(0.86)))]

def section(b=c):
    top = Box(b.l + 0.05 * b.w, b.r - 0.05 * b.w, b.y(0.3), b.t + 0.06 * b.h)
    bot = Box(b.l + 0.05 * b.w, b.r - 0.05 * b.w, b.b - 0.12 * b.h, b.y(0.62))
    return pen(S(top) + S(bot), HW - 7)

def circled(inner, b=c):
    rx, ry = 0.64 * b.w, b.h / 2 + 2 * O
    return [ellipse(b.cx, b.cy, rx, ry, hw=HW - 15)] + pen(inner(Box(b.cx - 0.2 * b.w, b.cx + 0.2 * b.w, b.y(0.26), b.y(0.74))), HW - 15)

def guillemets(right):
    out = [chain((x + 96, AXIS + 116), (x, AXIS), (x + 96, AXIS - 116), hw=HW - 7) for x in (110, 290)]
    return mirrored(out) if right else out

def guillemet(right):
    out = [chain((300 + 60, AXIS + 115), (300 - 60, AXIS), (300 + 60, AXIS - 115), hw=HW - 5)]
    return mirrored(out) if right else out

def fraction(n, d):
    up = Box(70, 230, 405, 650)
    down = Box(370, 530, 50, 295)
    return pen(n(up) + d(down), SUP_HW) + [chain((470, 650), (130, 50), hw=SUP_HW)]

def ordinal(letter):
    """ª and º: a letter raised over a bar."""
    up = Box(195, 405, 430, 660)
    return pen(letter(up), HW - 15) + [chain((up.l - 0.06 * up.w, 330), (up.r + 0.06 * up.w, 330), hw=HW - 15)]

def currency(b=c):
    r = 0.3 * b.w
    out = [ellipse(b.cx, AXIS, r, r, hw=HW - 5)]
    for a in (45, 135, 225, 315):
        ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
        out.append(chain((b.cx + ca * (r + 20), AXIS + sa * (r + 20)), (b.cx + ca * (r + 90), AXIS + sa * (r + 90)), hw=HW - 5))
    return out

def pound(b=c):
    x = b.l + 0.24 * b.w
    return [chain((x, b.b + 0.02 * b.h), (x, b.t - 0.25 * b.h), arc(b.cx + 0.07 * b.w, b.t - 0.25 * b.h + O, b.cx + 0.07 * b.w - x, 0.25 * b.h, 180, 18)),
            chain((b.l, b.b), (b.r, b.b)), chain((b.l, b.y(0.47)), (b.cx + 0.16 * b.w, b.y(0.47)))]

def yen(b=c):
    bb = b.wide(1.08)
    ym = b.y(0.54)
    bars = b.wide(0.82)
    return [chain((bb.l, bb.t), (bb.cx, ym), (bb.r, bb.t)), chain((bb.cx, ym), (bb.cx, bb.b))] + \
        [chain((bars.l, b.y(y)), (bars.r, b.y(y)), hw=HW - 9) for y in (0.4, 0.14)]

def euro(b=c):
    bb = b.wide(1.0, 0.07 * b.w)
    return [chain(arc(bb.cx, bb.cy, bb.w / 2, bb.h / 2 + O, 45, 315)),
            chain((b.l - 0.1 * b.w, b.y(0.6)), (b.cx + 0.2 * b.w, b.y(0.6)), hw=HW - 7),
            chain((b.l - 0.1 * b.w, b.y(0.4)), (b.cx + 0.16 * b.w, b.y(0.4)), hw=HW - 7)]

# ---------------------------------------------------------------------------
# Which code draws what.

def build():
    G = {}
    for ch, fn in PUNCT.items():
        G[ord(ch)] = fn()
    for ch, fn in LETTERS.items():
        G[ord(ch)] = fn(CAP)
        G[ord(ch.lower())] = fn(SC)
    for i, fn in enumerate(DIGITS):
        G[ord('0') + i] = fn(CAP)
    G[0x2191] = arrow_up()
    G[0x2190] = arrow_left()
    G[0x2192] = mirrored(arrow_left())
    G[0x2193] = turned(arrow_up())

    # Latin-1's letters: a capital or small capital with its accent.
    marks = {'grave': grave, 'acute': acute, 'circ': circumflex, 'tilde': tilde_acc, 'uml': diaeresis, 'ring': ring}
    accented = [
        ('A', 'grave', 0xC0), ('A', 'acute', 0xC1), ('A', 'circ', 0xC2), ('A', 'tilde', 0xC3), ('A', 'uml', 0xC4), ('A', 'ring', 0xC5),
        ('E', 'grave', 0xC8), ('E', 'acute', 0xC9), ('E', 'circ', 0xCA), ('E', 'uml', 0xCB),
        ('I', 'grave', 0xCC), ('I', 'acute', 0xCD), ('I', 'circ', 0xCE), ('I', 'uml', 0xCF),
        ('N', 'tilde', 0xD1),
        ('O', 'grave', 0xD2), ('O', 'acute', 0xD3), ('O', 'circ', 0xD4), ('O', 'tilde', 0xD5), ('O', 'uml', 0xD6),
        ('U', 'grave', 0xD9), ('U', 'acute', 0xDA), ('U', 'circ', 0xDB), ('U', 'uml', 0xDC),
        ('Y', 'acute', 0xDD),
    ]
    for letter, mark, code in accented:
        G[code] = LETTERS[letter](CAP) + marks[mark](CAP)
        G[code + 0x20] = LETTERS[letter](SC) + marks[mark](SC)
    G[0xFF] = Y(SC) + diaeresis(SC)
    G[0xC7] = C(CAP) + cedilla(CAP)
    G[0xE7] = C(SC) + cedilla(SC)
    G[0xC6] = AE(CAP)
    G[0xE6] = AE(SC)
    G[0xD0] = eth(CAP)
    G[0xF0] = eth(SC)
    G[0xD8] = O_(CAP) + slashed(CAP)
    G[0xF8] = O_(SC) + slashed(SC)
    G[0xDE] = thorn(CAP)
    G[0xFE] = thorn(SC)
    G[0xDF] = sharp_s()

    # Latin-1's signs.
    mid = Box(c.l, c.r, AXIS - 175, AXIS + 175)
    G[0xA0] = []
    G[0xA1] = turned(exclam())
    G[0xA2] = C(SC) + [chain((SC.cx + 0.03 * SC.w, SC.t + 80), (SC.cx + 0.03 * SC.w, SC.b - 90))]
    G[0xA3] = pound()
    G[0xA4] = currency()
    G[0xA5] = yen()
    G[0xA6] = [chain((c.cx, c.t + 70), (c.cx, AXIS + 70)), chain((c.cx, AXIS - 40), (c.cx, c.b - 110))]
    G[0xA7] = section()
    G[0xA8] = diaeresis(SC)
    G[0xA9] = circled(C)
    G[0xAA] = ordinal(A)
    G[0xAB] = guillemets(False)
    G[0xAC] = [chain((c.l, AXIS + 70), (c.r, AXIS + 70), (c.r, AXIS - 90))]
    G[0xAD] = PUNCT['-']()
    G[0xAE] = circled(R)
    G[0xAF] = macron(SC)
    G[0xB0] = [ellipse(c.cx, c.t - 70, 78, 82, hw=HW - 9)]
    G[0xB1] = [chain((c.l, AXIS + 70), (c.r, AXIS + 70)), chain((c.cx, AXIS + 70 - c.w / 2 + 20), (c.cx, AXIS + 70 + c.w / 2 - 20)), chain((c.l, c.b), (c.r, c.b))]
    G[0xB2] = pen(d2(SUP), SUP_HW)
    G[0xB3] = pen(d3(SUP), SUP_HW)
    G[0xB4] = acute(SC)
    G[0xB5] = micro()
    G[0xB6] = pilcrow()
    G[0xB7] = [dot(c.cx, AXIS)]
    G[0xB8] = cedilla(Box(c.l, c.r, c.b, c.t))
    G[0xB9] = pen(d1(SUP), SUP_HW)
    G[0xBA] = ordinal(O_)
    G[0xBB] = guillemets(True)
    G[0xBC] = fraction(d1, d4)
    G[0xBD] = fraction(d1, d2)
    G[0xBE] = fraction(d3, d4)
    G[0xBF] = turned(question())
    G[0xD7] = X(Box(mid.l + 0.1 * mid.w, mid.r - 0.1 * mid.w, AXIS - 145, AXIS + 145))
    G[0xF7] = [chain((c.l, AXIS), (c.r, AXIS)), dot(c.cx, AXIS + 150, 52), dot(c.cx, AXIS - 150, 52)]

    # The quotation marks, dashes and the like that programs print: ’ a comma
    # raised, ‘ one turned.
    G[0x2018] = turned(comma(c.cx - 8, c.t - 70), c.cx, c.t - 70 - 50)
    G[0x2019] = comma(c.cx + 8, c.t - 40)
    G[0x201A] = comma(c.cx + 10, c.b + 10)
    G[0x201C] = moved(G[0x2018], -95) + moved(G[0x2018], 95)
    G[0x201D] = moved(G[0x2019], -95) + moved(G[0x2019], 95)
    G[0x201E] = moved(G[0x201A], -95) + moved(G[0x201A], 95)
    G[0x2013] = [chain((c.l - 0.04 * c.w, AXIS), (c.r + 0.04 * c.w, AXIS))]
    G[0x2014] = [chain((30, AXIS), (570, AXIS))]
    G[0x2026] = [dot(110, c.b + 10, 50), dot(300, c.b + 10, 50), dot(490, c.b + 10, 50)]
    G[0x2022] = [dot(c.cx, AXIS, 100)]
    G[0x2039] = guillemet(False)
    G[0x203A] = guillemet(True)
    G[0x20AC] = euro()
    return G

# ---------------------------------------------------------------------------
# Tracing: the pen's distance field, the ink, marching squares.

def prepare(strokes):
    """Each stroke's segments, pen and bounds."""
    out = []
    for s in strokes:
        hw, pts = s.hw or HW, s.pts
        segs = []
        if len(pts) == 1:
            segs.append((pts[0][0], pts[0][1], 0.0, 0.0, 0.0))
        for (ax, ay), (bx, by) in zip(pts, pts[1:]):
            dx, dy = bx - ax, by - ay
            l2 = dx * dx + dy * dy
            segs.append((ax, ay, dx, dy, 1.0 / l2 if l2 else 0.0))
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        out.append((min(xs), min(ys), max(xs), max(ys), hw, segs))
    return out

def seg_d2(segs, x, y, best):
    """The squared distance from (x, y) to the nearest of segs, or best."""
    for ax, ay, dx, dy, inv in segs:
        px, py = x - ax, y - ay
        t = (px * dx + py * dy) * inv
        if t < 0.0:
            t = 0.0
        elif t > 1.0:
            t = 1.0
        ex, ey = px - t * dx, py - t * dy
        d2 = ex * ex + ey * ey
        if d2 < best:
            best = d2
    return best

def pen_field(prep, x, y):
    """How far (x, y) is outside what the pen traced (below zero inside)."""
    best = 1e9
    for x0, y0, x1, y1, hw, segs in prep:
        ddx = x0 - x if x < x0 else (x - x1 if x > x1 else 0.0)
        ddy = y0 - y if y < y0 else (y - y1 if y > y1 else 0.0)
        if (ddx or ddy) and math.sqrt(ddx * ddx + ddy * ddy) - hw >= best:
            continue
        d = math.sqrt(seg_d2(segs, x, y, 1e18)) - hw
        if d < best:
            best = d
    return best

class Near:
    """Segments of closed loops, found by the square they pass through."""
    CELL = 48
    def __init__(s, loops):
        s.cells = {}
        for loop in loops:
            for i in range(len(loop)):
                (ax, ay), (bx, by) = loop[i - 1], loop[i]
                dx, dy = bx - ax, by - ay
                l2 = dx * dx + dy * dy
                seg = (ax, ay, dx, dy, 1.0 / l2 if l2 else 0.0)
                for cx in range(int(math.floor(min(ax, bx) / s.CELL)), int(math.floor(max(ax, bx) / s.CELL)) + 1):
                    for cy in range(int(math.floor(min(ay, by) / s.CELL)), int(math.floor(max(ay, by) / s.CELL)) + 1):
                        s.cells.setdefault((cx, cy), []).append(seg)
    def distance(s, x, y, limit):
        """The distance to the nearest segment, or limit if none is nearer."""
        r = int(limit // s.CELL) + 1
        cx, cy = int(math.floor(x / s.CELL)), int(math.floor(y / s.CELL))
        best = limit * limit
        for i in range(cx - r, cx + r + 1):
            for j in range(cy - r, cy + r + 1):
                segs = s.cells.get((i, j))
                if segs:
                    best = seg_d2(segs, x, y, best)
        return math.sqrt(best)

def level(value, nx, ny):
    """The loops where value(i, j), sampled on a grid, crosses zero
    (marching squares), as unordered points in grid units. Only coarse cells
    the edge could pass through are traced finely: the values change no
    faster than the distance, so a cell whose corners are all farther from
    the edge than half its diagonal holds none of it."""
    K = 4
    near = K * 0.75
    segs = []
    for J in range(0, ny, K):
        for I in range(0, nx, K):
            if min(abs(value(I, J)), abs(value(I + K, J)), abs(value(I, J + K)), abs(value(I + K, J + K))) > near:
                continue
            for j in range(J, J + K):
                for i in range(I, I + K):
                    v00, v10, v11, v01 = value(i, j), value(i + 1, j), value(i + 1, j + 1), value(i, j + 1)
                    case = (v00 < 0) | (v10 < 0) << 1 | (v11 < 0) << 2 | (v01 < 0) << 3
                    if case == 0 or case == 15:
                        continue
                    B, R, T, L_ = ('h', i, j), ('v', i + 1, j), ('h', i, j + 1), ('v', i, j)
                    centre = (v00 + v10 + v11 + v01) < 0
                    table = {1: [(L_, B)], 2: [(B, R)], 3: [(L_, R)], 4: [(R, T)],
                             5: [(L_, T), (B, R)] if centre else [(L_, B), (R, T)],
                             6: [(B, T)], 7: [(L_, T)], 8: [(L_, T)], 9: [(B, T)],
                             10: [(L_, B), (R, T)] if centre else [(B, R), (L_, T)],
                             11: [(R, T)], 12: [(L_, R)], 13: [(B, R)], 14: [(L_, B)]}
                    segs.extend(table[case])
    def point(key):
        kind, i, j = key
        if kind == 'h':
            a, b = value(i, j), value(i + 1, j)
            return (i + a / (a - b), j)
        a, b = value(i, j), value(i, j + 1)
        return (i, j + a / (a - b))
    links = {}
    for a, b in segs:
        links.setdefault(a, []).append(b)
        links.setdefault(b, []).append(a)
    loops, seen = [], set()
    for start in links:
        if start in seen:
            continue
        loop, prev, cur = [start], None, start
        seen.add(start)
        while True:
            nxt = [k for k in links[cur] if k != prev]
            nxt = nxt[0] if nxt else links[cur][0]
            if nxt == start or nxt in seen:
                break
            seen.add(nxt)
            loop.append(nxt)
            prev, cur = cur, nxt
        if len(loop) >= 3:
            loops.append([point(k) for k in loop])
    return loops

def trace(strokes):
    """The glyph's outline as closed polygons, outer ones clockwise.

    The pen's strokes, closed by the ink: grown by INK and shrunk back, which
    fills each inside corner with a round of that radius, as ink spreads
    where strokes meet, and leaves the round ends and outside corners as the
    pen drew them."""
    if not strokes:
        return []
    prep = prepare(strokes)
    pad = max(p[4] for p in prep) + INK + 3 * STEP
    x0 = math.floor((min(p[0] for p in prep) - pad) / STEP) * STEP
    y0 = math.floor((min(p[1] for p in prep) - pad) / STEP) * STEP
    nx = (int(math.ceil((max(p[2] for p in prep) + pad - x0) / STEP)) + 3) // 4 * 4
    ny = (int(math.ceil((max(p[3] for p in prep) + pad - y0) / STEP)) + 3) // 4 * 4
    pens = {}
    def body(i, j):
        v = pens.get((i, j))
        if v is None:
            v = pens[(i, j)] = pen_field(prep, x0 + i * STEP, y0 + j * STEP)
        return v
    grown = [[(x0 + x * STEP, y0 + y * STEP) for x, y in loop]
             for loop in level(lambda i, j: (body(i, j) - INK) / STEP, nx, ny)]
    near = Near([dp_loop(loop, 0.25) for loop in grown])
    reach = INK + 8 * STEP
    inks = {}
    def ink(i, j):
        v = inks.get((i, j))
        if v is None:
            d = near.distance(x0 + i * STEP, y0 + j * STEP, reach)
            v = inks[(i, j)] = (INK - d if body(i, j) < INK else INK + d) / STEP
        return v
    loops = [[(x0 + x * STEP, y0 + y * STEP) for x, y in loop] for loop in level(ink, nx, ny)]
    # Outer contours clockwise and holes the other way, as TrueType fills them.
    out = []
    for n, loop in enumerate(loops):
        depth = sum(1 for m, other in enumerate(loops) if m != n and inside(loop[0], other))
        if (depth % 2 == 0) == (signed_area(loop) > 0):
            loop = loop[::-1]
        loop = simplify(loop)
        if len(loop) >= 3:
            out.append(loop)
    return out

def signed_area(poly):
    return sum(poly[i - 1][0] * poly[i][1] - poly[i][0] * poly[i - 1][1] for i in range(len(poly))) / 2

def inside(p, poly):
    x, y = p
    c = False
    for i in range(len(poly)):
        (ax, ay), (bx, by) = poly[i - 1], poly[i]
        if (ay > y) != (by > y) and x < ax + (bx - ax) * (y - ay) / (by - ay):
            c = not c
    return c

def dp_loop(loop, tol):
    """Douglas-Peucker on a closed loop: the fewest of its points that keep
    it within tol."""
    n = len(loop)
    far = max(range(n), key=lambda i: (loop[i][0] - loop[0][0]) ** 2 + (loop[i][1] - loop[0][1]) ** 2)
    def dp(pts):
        keep = [True] + [False] * (len(pts) - 2) + [True]
        stack = [(0, len(pts) - 1)]
        while stack:
            a, b = stack.pop()
            (ax, ay), (bx, by) = pts[a], pts[b]
            dx, dy = bx - ax, by - ay
            ln = math.hypot(dx, dy) or 1e-9
            worst, at = 0.0, None
            for i in range(a + 1, b):
                d = abs((pts[i][0] - ax) * dy - (pts[i][1] - ay) * dx) / ln
                if d > worst:
                    worst, at = d, i
            if at is not None and worst > tol:
                keep[at] = True
                stack += [(a, at), (at, b)]
        return [p for p, k in zip(pts, keep) if k]
    first = dp(loop[:far + 1])
    second = dp(loop[far:] + [loop[0]])
    return first[:-1] + second[:-1]

def simplify(loop):
    """The loop simplified, in whole units."""
    out = []
    for x, y in dp_loop(loop, TOL):
        q = (int(round(x)), int(round(y)))
        if not out or q != out[-1]:
            out.append(q)
    while len(out) > 1 and out[0] == out[-1]:
        out.pop()
    return out

# ---------------------------------------------------------------------------
# The TrueType font, after bitmap2ttf.py.

def glyph_data(contours):
    pts = [p for cnt in contours for p in cnt]
    xmin, ymin = min(p[0] for p in pts), min(p[1] for p in pts)
    xmax, ymax = max(p[0] for p in pts), max(p[1] for p in pts)
    data = struct.pack('>hhhhh', len(contours), xmin, ymin, xmax, ymax)
    ends, n = [], 0
    for cnt in contours:
        n += len(cnt)
        ends.append(n - 1)
    data += struct.pack('>%dH' % len(ends), *ends) + struct.pack('>H', 0)
    flags, xs, ys = [], b'', b''
    px = py = 0
    for x, y in pts:
        dx, dy = x - px, y - py
        fl = 1  # on the curve
        if dx == 0:
            fl |= 0x10
        elif -256 < dx < 256:
            fl |= 0x02 | (0x10 if dx > 0 else 0)
            xs += bytes([abs(dx)])
        else:
            xs += struct.pack('>h', dx)
        if dy == 0:
            fl |= 0x20
        elif -256 < dy < 256:
            fl |= 0x04 | (0x20 if dy > 0 else 0)
            ys += bytes([abs(dy)])
        else:
            ys += struct.pack('>h', dy)
        flags.append(fl)
        px, py = x, y
    data += bytes(flags) + xs + ys
    if len(data) % 4:
        data += b'\0' * (4 - len(data) % 4)
    return data, (xmin, ymin, xmax, ymax), len(pts)

def notdef():
    """A box, for codes the font hasn't."""
    return [[(100, 0), (100, 650), (500, 650), (500, 0)], [(170, 70), (430, 70), (430, 580), (170, 580)]]

def write_font(path, outlines, codes):
    """outlines: one list of contours a glyph, in glyph order; codes: {code: glyph}."""
    glyf, loca, hmtx = b'', [0], b''
    max_points = max_contours = 0
    bbox = None
    for contours in outlines:
        if contours:
            data, gb, npts = glyph_data(contours)
            glyf += data
            max_points, max_contours = max(max_points, npts), max(max_contours, len(contours))
            bbox = gb if bbox is None else (min(bbox[0], gb[0]), min(bbox[1], gb[1]), max(bbox[2], gb[2]), max(bbox[3], gb[3]))
            lsb = gb[0]
        else:
            lsb = 0
        loca.append(len(glyf))
        hmtx += struct.pack('>Hh', ADV, lsb)
    n = len(outlines)
    head = struct.pack('>IIIIHHqqhhhhHHhhh', 0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0x000B, UPEM, 0, 0,
                       bbox[0], bbox[1], bbox[2], bbox[3], 0, 8, 2, 1, 0)
    hhea = struct.pack('>Ihhh' + 'H' + 'hhh' + 'hhh' + 'hhhh' + 'hH', 0x00010000, ASC, -DESC, 0, ADV, bbox[0], ADV - bbox[2],
                       bbox[2], 1, 0, 0, 0, 0, 0, 0, 0, n)
    maxp = struct.pack('>IHHHHHHHHHHHHHH', 0x00010000, n, max_points, max_contours, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0)
    first_c, last_c = min(codes), max(codes)
    # Version 4: its sizes and positions for sub- and superscripts, strikeout and family class.
    os2 = struct.pack('>HhHHHhhhhhhhhhhh', 4, ADV, 400, 5, 0, 650, 700, 0, 140, 650, 700, 0, 480, 50, 260, 0)
    # Panose: Latin text, sans, medium, monospaced.
    os2 += bytes([2, 11, 6, 9, 0, 0, 0, 0, 0, 0])
    # Basic Latin, Latin-1, general punctuation; currency symbols, arrows.
    os2 += struct.pack('>IIII', 1 | 1 << 1 | 1 << 31, 1 << 1 | 1 << 5, 0, 0) + b'NONE'
    os2 += struct.pack('>HHHhhhHHII', 0xC0, first_c, min(last_c, 0xFFFF), ASC, -DESC, 0, ASC + 130, DESC + 190, 1, 0)
    os2 += struct.pack('>hhHHH', 480, 650, 0, 32, 1)
    post = struct.pack('>IIhhIIIII', 0x00030000, 0, -110, HW * 2, 1, 0, 0, 0, 0)

    items = sorted(codes.items())
    segs = [(cd, cd, (g - cd) & 0xFFFF) for cd, g in items] + [(0xFFFF, 0xFFFF, 1)]
    segx2 = 2 * len(segs)
    sr = 1
    while sr * 2 <= len(segs):
        sr *= 2
    sub = struct.pack('>HHHHHHH', 4, 0, 0, segx2, sr * 2, sr.bit_length() - 1, segx2 - sr * 2)
    sub += struct.pack('>%dH' % len(segs), *[e for _, e, _ in segs]) + b'\0\0'
    sub += struct.pack('>%dH' % len(segs), *[s for s, _, _ in segs])
    sub += struct.pack('>%dH' % len(segs), *[dl for _, _, dl in segs])
    sub += struct.pack('>%dH' % len(segs), *([0] * len(segs)))
    sub = sub[:2] + struct.pack('>H', len(sub)) + sub[4:]
    cmap = struct.pack('>HHHHIHHI', 0, 2, 0, 3, 20, 3, 1, 20) + sub

    family = 'Flow Teletype'
    names = [(0, 'Copyright 2026 the Flow authors. Drawn for Flow after the type of the Teletype Model 33 ASR.'),
             (1, family), (2, 'Regular'), (3, family + ' Regular 1.0'), (4, family + ' Regular'), (5, 'Version 1.000'),
             (6, 'FlowTeletype-Regular'),
             (10, 'A heavy monoline gothic of capitals after the Model 33 teletype\'s printouts, lowercase as small capitals.')]
    store, recs = b'', b''
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
    body, dirs = b'', b''
    head_at = None
    for t in tags:
        data = tables[t]
        if t == 'head':
            head_at = offset + len(body)
        dirs += struct.pack('>4sIII', t.encode('latin-1'), checksum(data), offset + len(body), len(data))
        body += data + b'\0' * ((4 - len(data) % 4) % 4)
    font = bytearray(out + dirs + body)
    adj = (0xB1B0AFBA - checksum(bytes(font))) & 0xFFFFFFFF
    font[head_at + 8:head_at + 12] = struct.pack('>I', adj)
    open(path, 'wb').write(bytes(font))
    return len(font), max_points, max_contours

# ---------------------------------------------------------------------------
# A sample sheet, drawn from the outlines with a nonzero fill.

def raster(contours, img, W, ox, oy, scale, ink):
    """Adds the glyph's coverage to img (W wide), its origin at pixel
    (ox, oy), scale pixels per unit, y down."""
    edges = []
    for cnt in contours:
        for i in range(len(cnt)):
            (ax, ay), (bx, by) = cnt[i - 1], cnt[i]
            if ay != by:
                edges.append((ox + ax * scale, oy - ay * scale, ox + bx * scale, oy - by * scale))
    if not edges:
        return
    top = int(min(min(e[1], e[3]) for e in edges))
    bot = int(max(max(e[1], e[3]) for e in edges)) + 1
    H = len(img) // W
    SUB = 5
    for py in range(max(top, 0), min(bot, H)):
        row = [0.0] * W
        for s in range(SUB):
            y = py + (s + 0.5) / SUB
            xs = []
            for ax, ay, bx, by in edges:
                if (ay <= y < by) or (by <= y < ay):
                    xs.append((ax + (bx - ax) * (y - ay) / (by - ay), 1 if by > ay else -1))
            xs.sort()
            wind = 0
            for k in range(len(xs) - 1):
                wind += xs[k][1]
                if wind:
                    a, b = max(xs[k][0], 0), min(xs[k + 1][0], W - 1e-6)
                    while a < b:
                        px = int(a)
                        nxt = min(b, px + 1)
                        row[px] += (nxt - a) / SUB
                        a = nxt
        base = py * W
        for x in range(W):
            if row[x] > 0:
                img[base + x] = min(1.0, img[base + x] + row[x] * ink)

def write_png(path, img, W, H, paper=(0xF4, 0xEE, 0xDC), ink=(0x24, 0x22, 0x2A)):
    raw = bytearray()
    for y in range(H):
        raw.append(0)
        for x in range(W):
            a = min(1.0, img[y * W + x])
            raw += bytes(int(round(p + (q - p) * a)) for p, q in zip(paper, ink))
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xFFFFFFFF)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', W, H, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(bytes(raw), 9)) + chunk(b'IEND', b'')
    open(path, 'wb').write(png)

def sheet(path, outlines, codes):
    lines = [
        (96, ''.join(chr(cd) for cd in range(0x20, 0x40))),
        (96, ''.join(chr(cd) for cd in range(0x40, 0x60))),
        (96, ''.join(chr(cd) for cd in range(0x60, 0x7F)) + '←↑→↓'),
        (96, ''.join(chr(cd) for cd in range(0xA0, 0xC0))),
        (96, ''.join(chr(cd) for cd in range(0xC0, 0xE0))),
        (96, ''.join(chr(cd) for cd in range(0xE0, 0x100))),
        (96, '‘’‚“”„–—…•‹›€ O0 Il1 ^_ ↑←'),
        (40, '10 PRINT "HELLO, WORLD!"  20 GOTO 10   READY.  $ ls -la ~/src/flow_terminal'),
        (40, 'The quick brown fox jumps over the lazy dog. THE QUICK BROWN FOX: 0123456789'),
        (40, 'Façade, naïve, ÉCOLE, Straße, Señor, ¿Qué? © 1963 ½ °C ± × ÷'),
        (20, 'cat /etc/passwd | grep -v nologin > users.txt && echo "done: $?"  [OK] {a: 1} @home #tag 50% & *glob* x<y>z a=b c+d'),
        (20, 'LOAD "TAPE",8,1   RUN   MEMORY SIZE? 4096   TERMINAL WIDTH? 72   WANT SIN-COS-TAN-ATN? Y   3784 BYTES FREE'),
        (15, 'The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. ~/src $ make && ./run --help'),
        (15, 'THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG 0123456789 !"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~'),
    ]
    W = 1900
    rows = []
    y = 20
    for px, text in lines:
        scale = px / UPEM
        y += int(ASC * scale) + 6
        rows.append((y, scale, text))
        y += int((DESC + 250) * scale) + 6
    H = y + 20
    img = [0.0] * (W * H)
    for base, scale, text in rows:
        x = 20.0
        for ch in text:
            g = codes.get(ord(ch))
            if g is not None:
                raster(outlines[g], img, W, x, base, scale, 1.0)
            x += ADV * scale
    write_png(path, img, W, H)

def main():
    G = build()
    order = sorted(G)
    outlines = [notdef()]
    codes = {}
    space = None
    for cd in order:
        strokes = G[cd]
        if not strokes and space is not None:
            codes[cd] = space
            continue
        codes[cd] = len(outlines)
        if not strokes:
            space = len(outlines)
        outlines.append(trace(strokes))
    size, mp, mc = write_font(sys.argv[1], outlines, codes)
    print('%d glyphs, %d codes, %d bytes, at most %d points and %d contours a glyph' % (len(outlines), len(codes), size, mp, mc))
    if len(sys.argv) > 2:
        sheet(sys.argv[2], outlines, codes)

if __name__ == '__main__':
    main()
