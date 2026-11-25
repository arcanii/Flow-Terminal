//! meshoptimizer's compression, as EXT_meshopt_compression keeps a glTF's
//! buffer views in it (and KHR_meshopt_compression, which grew from it,
//! where its data are of the same kinds): a compressed view names where
//! its packed bytes are, how many elements of how many bytes they unpack
//! to, how they were packed ([`Mode`]), and the filter that made each
//! element smaller before that ([`Filter`]). `super::buffers` unpacks each
//! into the view's own place before anything reads it, so the rest of the
//! loader reads it as it would any view.
//!
//! Written from the extension's spec of the bitstreams (and checked against
//! meshoptimizer's own decoder): vertex data in blocks, each byte of the
//! vertex a column of deltas from the vertex before, in groups of 16 packed
//! to so many bits a byte with escapes; in the vertex codec's version 1,
//! which KHR_meshopt_compression adds, each column also packed as a mode
//! of its block says (two tables of bit widths, all zeros, or bytes as
//! they are), and each four bytes' deltas taken as bytes, 16-bit numbers
//! or 32-bit ones xored and turned, as the stream's tail says; triangles
//! (index codec versions 0 and 1) as an edge or vertices from FIFOs of
//! recent ones, the next new vertex, or an index coded as a delta; and
//! index sequences as deltas from one of two baselines. The filters are
//! the octahedral, quaternion and exponential ones, and the color filter
//! KHR_meshopt_compression adds.

/// How a compressed view's bytes were packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Vertex attributes, or any elements a multiple of 4 bytes long.
    Attributes,
    /// A triangle list's indices, 2 or 4 bytes each.
    Triangles,
    /// Any other indices, 2 or 4 bytes each.
    Indices,
}

/// What was done to each element before it was packed, undone after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    None,
    /// Unit vectors as two octahedral coordinates and the scale of one,
    /// in 8 or 16 bits, a fourth value kept as it is.
    Octahedral,
    /// Unit quaternions as their three smallest parts, in 16 bits.
    Quaternion,
    /// Floats as a 24-bit mantissa and an 8-bit exponent.
    Exponential,
    /// RGBA colors as YCoCg and alpha, in 8 or 16 bits, the alpha's
    /// highest set bit saying how many of them the color keeps.
    Color,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "ATTRIBUTES" => Some(Mode::Attributes),
            "TRIANGLES" => Some(Mode::Triangles),
            "INDICES" => Some(Mode::Indices),
            _ => None,
        }
    }
}

impl Filter {
    pub fn parse(s: &str) -> Option<Filter> {
        match s {
            "NONE" => Some(Filter::None),
            "OCTAHEDRAL" => Some(Filter::Octahedral),
            "QUATERNION" => Some(Filter::Quaternion),
            "EXPONENTIAL" => Some(Filter::Exponential),
            "COLOR" => Some(Filter::Color),
            _ => None,
        }
    }
}

/// `count` elements of `stride` bytes from `data`, packed as `mode` with
/// `filter`.
pub fn unpack(data: &[u8], count: usize, stride: usize, mode: Mode, filter: Filter) -> Result<Vec<u8>, String> {
    let mut out = match mode {
        Mode::Attributes => vertices(data, count, stride)?,
        Mode::Triangles => triangles(data, count, stride)?,
        Mode::Indices => sequence(data, count, stride)?,
    };
    match (mode, filter) {
        (_, Filter::None) => {}
        (Mode::Attributes, _) => unfilter(&mut out, stride, filter)?,
        _ => return Err("a filter on indices".into()),
    }
    Ok(out)
}

/// The vertex codec's groups: 16 values, each a byte of one vertex.
const GROUP: usize = 16;
/// A block's vertices take at most 8 KB, and number at most 256.
const BLOCK_BYTES: usize = 8192;
const BLOCK_MAX: usize = 256;
/// The tail after the blocks is at least this long (version 0's, then
/// version 1's); it ends with the first vertex, the baseline the first
/// block's deltas start from, and in version 1 each four bytes' channel.
const TAIL_MIN: [usize; 2] = [32, 24];
/// The bit widths a group's two header bits pick from: version 0's, and
/// version 1's, where a column's mode 0 picks from the first four and mode
/// 1 from the last four.
const BITS_V0: [usize; 4] = [0, 2, 4, 8];
const BITS_V1: [usize; 5] = [0, 1, 2, 4, 8];

/// How many vertices of `stride` bytes a block holds: as many as fit in 8
/// KB, a whole number of groups, at most 256.
fn block_size(stride: usize) -> usize {
    ((BLOCK_BYTES / stride) & !(GROUP - 1)).min(BLOCK_MAX)
}

/// Vertex data (mode ATTRIBUTES): a header byte (0xa0 or 0xa1, the vertex
/// codec's version), then the vertices in blocks, then the tail. Each
/// block holds each byte of its vertices in turn, as a column: every byte
/// the delta from the same byte of the vertex before (the block before's
/// last, or the baseline), zigzagged, so small changes either way are
/// small numbers, in groups of 16 (`bytes`). Version 1 starts each block
/// with two bits a byte of the vertex saying how its column is kept (0 and
/// 1: in groups, from `BITS_V1`'s first four widths or its last four; 2:
/// all zeros, nothing kept; 3: a byte a vertex as it is), and its tail
/// says, for each four bytes, how their deltas are taken (`channel`).
pub fn vertices(data: &[u8], count: usize, stride: usize) -> Result<Vec<u8>, String> {
    if stride == 0 || stride > 256 || !stride.is_multiple_of(4) {
        return Err(format!("meshopt vertices {stride} bytes long"));
    }
    let version = match data.first() {
        Some(0xa0) => 0,
        Some(0xa1) => 1,
        Some(&h) if h & 0xf0 == 0xa0 => return Err(format!("meshopt's vertex codec version {}, which Flow doesn't read", h & 0x0f)),
        _ => return Err("not meshopt's vertex data".into()),
    };
    let short = || "meshopt's vertex data cut short".to_string();
    let tail = stride + if version == 0 { 0 } else { stride / 4 };
    let padded = tail.max(TAIL_MIN[version]);
    if data.len() < 1 + padded {
        return Err(short());
    }
    let end = data.len() - padded;
    let mut last = data[data.len() - tail..][..stride].to_vec();
    let channels = &data[data.len() - tail + stride..];
    if let Some(c) = channels.iter().find(|&&c| c & 15 > 2) {
        return Err(format!("meshopt's vertex channel {}, which isn't one", c & 15));
    }
    let mut out = vec![0u8; count * stride];
    let mut columns = [[0u8; BLOCK_MAX]; 4];
    let (block, mut at, mut start) = (block_size(stride), 1, 0);
    while start < count {
        let n = block.min(count - start);
        let groups = n.div_ceil(GROUP) * GROUP;
        let control = &data[at..at + if version == 0 { 0 } else { stride / 4 }];
        at += control.len();
        if at > end {
            return Err(short());
        }
        for k in (0..stride).step_by(4) {
            for (j, column) in columns.iter_mut().enumerate() {
                let mode = control.get(k / 4).map_or(0, |c| usize::from((c >> (j * 2)) & 3));
                at = match mode {
                    2 => {
                        column[..n].fill(0);
                        at
                    }
                    3 => {
                        column[..n].copy_from_slice(data.get(at..at + n).filter(|_| at + n <= end).ok_or_else(short)?);
                        at + n
                    }
                    _ => bytes(data, end, at, &mut column[..groups], if version == 0 { &BITS_V0 } else { &BITS_V1[mode..mode + 4] })?,
                };
            }
            let place = |i: usize| (start + i) * stride + k;
            match channels.get(k / 4).map_or(0, |c| c & 15) {
                // Each byte on its own.
                0 => {
                    for (j, column) in columns.iter().enumerate() {
                        let mut p = last[k + j];
                        for (i, &z) in column[..n].iter().enumerate() {
                            p = p.wrapping_add(unzigzag(z));
                            out[place(i) + j] = p;
                        }
                    }
                }
                // Two 16-bit numbers, low byte first, each zigzagged.
                1 => {
                    for h in [0, 2] {
                        let mut p = u16::from_le_bytes([last[k + h], last[k + h + 1]]);
                        for i in 0..n {
                            let z = u16::from_le_bytes([columns[h][i], columns[h + 1][i]]);
                            p = p.wrapping_add((z >> 1) ^ (z & 1).wrapping_neg());
                            out[place(i) + h..place(i) + h + 2].copy_from_slice(&p.to_le_bytes());
                        }
                    }
                }
                // A 32-bit number xored with the one before, then turned
                // left by the channel's high four bits, which turning it on
                // round the rest of the way undoes.
                _ => {
                    let turn = (32 - u32::from(channels[k / 4] >> 4)) & 31;
                    let mut p = u32::from_le_bytes([last[k], last[k + 1], last[k + 2], last[k + 3]]);
                    for i in 0..n {
                        p ^= u32::from_le_bytes([columns[0][i], columns[1][i], columns[2][i], columns[3][i]]).rotate_left(turn);
                        out[place(i)..place(i) + 4].copy_from_slice(&p.to_le_bytes());
                    }
                }
            }
        }
        last.copy_from_slice(&out[(start + n - 1) * stride..(start + n) * stride]);
        start += n;
    }
    if at != end {
        return Err("meshopt's vertex data don't end where their tail starts".into());
    }
    Ok(out)
}

/// A byte's delta back from its zigzag: 0, -1, 1, -2, 2…
fn unzigzag(z: u8) -> u8 {
    (z >> 1) ^ (z & 1).wrapping_neg()
}

/// One column of a block: a header of two bits a group (the first group in
/// the low bits of the first byte) saying how it's packed, as which of
/// `widths`, then each group. Returns where it ends; reads go no further
/// than `end`.
fn bytes(data: &[u8], end: usize, mut at: usize, out: &mut [u8], widths: &[usize]) -> Result<usize, String> {
    let short = || "meshopt's vertex data cut short".to_string();
    let header_size = (out.len() / GROUP).div_ceil(4);
    let header = data.get(at..at + header_size).filter(|_| at + header_size <= end).ok_or_else(short)?;
    at += header_size;
    for (g, group) in out.as_chunks_mut::<GROUP>().0.iter_mut().enumerate() {
        let bits = widths[usize::from((header[g / 4] >> ((g % 4) * 2)) & 3)];
        at = match bits {
            0 => {
                group.fill(0);
                at
            }
            8 => {
                group.copy_from_slice(data.get(at..at + GROUP).filter(|_| at + GROUP <= end).ok_or_else(short)?);
                at + GROUP
            }
            _ => {
                // The values `bits` each, the first in a byte's high bits
                // (in its low bit, for one bit each); one that's all ones
                // (the sentinel) is the next of the bytes that follow them,
                // in order.
                let packed = GROUP * bits / 8;
                if at + packed > end {
                    return Err(short());
                }
                let sentinel = (1u8 << bits) - 1;
                let mut extra = at + packed;
                for (i, v) in group.iter_mut().enumerate() {
                    let byte = data[at + i * bits / 8];
                    let shift = if bits == 1 { i % 8 } else { 8 - bits - (i * bits) % 8 };
                    let value = (byte >> shift) & sentinel;
                    *v = if value == sentinel {
                        if extra >= end {
                            return Err(short());
                        }
                        extra += 1;
                        data[extra - 1]
                    } else {
                        value
                    };
                }
                extra
            }
        };
    }
    Ok(at)
}

/// A triangle list (mode TRIANGLES): a header byte (0xe0 or 0xe1, the
/// index codec's version), a code a triangle, the extra bytes some codes
/// need, then 16 bytes: the table of the codes 0xf0 to 0xfd. The decoder
/// keeps the 16 edges and the 16 vertices last seen, in FIFOs, the next
/// vertex not yet seen (`next`) and the last index coded in full
/// (`last`). A code below 0xf0 is an edge from the FIFO (its high four
/// bits, 0 the newest) and a third vertex (its low four): the next, one
/// from the FIFO (1 the second newest), in version 1 the last less or
/// more one (13, 14), or an index coded as a delta from the last (15).
/// The others are a triangle of three vertices: 0xf0 to 0xfd the next for
/// the first, and the second and third as the table's byte for its low
/// four bits says (four bits each: 0 the next, else the FIFO's, 1 the
/// newest); 0xfe and 0xff such a byte of their own after them, the first
/// the next or (0xff) an index coded in full, either of the others too
/// where its four bits are 15. A byte of 0 there (all three next) starts
/// the next vertex at 0 again.
pub fn triangles(data: &[u8], count: usize, size: usize) -> Result<Vec<u8>, String> {
    if !count.is_multiple_of(3) || (size != 2 && size != 4) {
        return Err(format!("meshopt triangles of {count} indices {size} bytes each"));
    }
    let short = || "meshopt's triangles cut short".to_string();
    let version = match data.first() {
        Some(0xe0) => 0,
        Some(0xe1) => 1,
        _ => return Err("not meshopt's triangles".into()),
    };
    if data.len() < 1 + count / 3 + 16 {
        return Err(short());
    }
    let fec_max = if version >= 1 { 13 } else { 15 };
    let end = data.len() - 16;
    let (codes, table) = (&data[1..1 + count / 3], &data[end..]);
    let mut at = 1 + count / 3;
    let mut fifos = Fifos::default();
    let (mut next, mut last) = (0u32, 0u32);
    let mut out = Vec::with_capacity(count * size);
    for &code in codes {
        if at > end {
            return Err(short());
        }
        let [a, b, c];
        let fresh: [bool; 3];
        if code < 0xf0 {
            [a, b] = fifos.edge(usize::from(code >> 4));
            let fec = usize::from(code & 15);
            c = match fec {
                0 => take(&mut next),
                _ if fec < fec_max => fifos.back(fec + 1),
                13 => {
                    last = last.wrapping_sub(1);
                    last
                }
                14 => {
                    last = last.wrapping_add(1);
                    last
                }
                _ => {
                    last = last.wrapping_add(delta(data, &mut at)?);
                    last
                }
            };
            fresh = [false, false, fec == 0 || fec >= fec_max];
        } else {
            let (fea, aux, full) = match code {
                0xfe | 0xff => {
                    let aux = *data.get(at).ok_or_else(short)?;
                    at += 1;
                    (if code == 0xfe { 0 } else { 15 }, aux, true)
                }
                _ => (0, table[usize::from(code & 15)], false),
            };
            if full && aux == 0 {
                next = 0;
            }
            let (feb, fec) = (usize::from(aux >> 4), usize::from(aux & 15));
            // Each in turn, as the encoder took them: a new one counts on
            // from those before it, and one in full is a delta from the
            // one in full before it.
            let mut corner = |fe: usize| -> Result<u32, String> {
                Ok(match fe {
                    0 => take(&mut next),
                    15 if full => {
                        last = last.wrapping_add(delta(data, &mut at)?);
                        last
                    }
                    _ => fifos.back(fe),
                })
            };
            a = corner(fea)?;
            b = corner(feb)?;
            c = corner(fec)?;
            let new = |fe: usize| fe == 0 || (full && fe == 15);
            fresh = [true, new(feb), new(fec)];
        }
        for i in [a, b, c] {
            if size == 2 {
                out.extend((i as u16).to_le_bytes());
            } else {
                out.extend(i.to_le_bytes());
            }
        }
        for (v, f) in [a, b, c].into_iter().zip(fresh) {
            if f {
                fifos.push(v);
            }
        }
        if code >= 0xf0 {
            fifos.push_edge([b, a]);
        }
        fifos.push_edge([c, b]);
        fifos.push_edge([a, c]);
    }
    if at != end {
        return Err("meshopt's triangles don't end where their table starts".into());
    }
    Ok(out)
}

/// The next vertex not yet seen, counting it seen.
fn take(next: &mut u32) -> u32 {
    *next = next.wrapping_add(1);
    next.wrapping_sub(1)
}

/// The index codec's two FIFOs: the 16 edges and the 16 vertices last
/// pushed, each written over the oldest.
pub struct Fifos {
    edges: [[u32; 2]; 16],
    edge_at: usize,
    seen: [u32; 16],
    seen_at: usize,
}

impl Default for Fifos {
    fn default() -> Self {
        Fifos { edges: [[u32::MAX; 2]; 16], edge_at: 0, seen: [u32::MAX; 16], seen_at: 0 }
    }
}

impl Fifos {
    /// The edge `n` back from the newest (0).
    pub fn edge(&self, n: usize) -> [u32; 2] {
        self.edges[(self.edge_at + 15 - n) & 15]
    }

    /// The vertex `n` pushes back: 1 is the newest.
    pub fn back(&self, n: usize) -> u32 {
        self.seen[(self.seen_at + 16 - n) & 15]
    }

    pub fn push(&mut self, v: u32) {
        self.seen[self.seen_at] = v;
        self.seen_at = (self.seen_at + 1) & 15;
    }

    pub fn push_edge(&mut self, e: [u32; 2]) {
        self.edges[self.edge_at] = e;
        self.edge_at = (self.edge_at + 1) & 15;
    }
}

/// A delta from the last index, as a variable-length number (7 bits a
/// byte, low first, the high bit saying another follows; five bytes at
/// most), zigzagged.
fn delta(data: &[u8], at: &mut usize) -> Result<u32, String> {
    let v = vbyte(data, at)?;
    Ok((v >> 1) ^ (v & 1).wrapping_neg())
}

fn vbyte(data: &[u8], at: &mut usize) -> Result<u32, String> {
    let mut byte = || {
        let b = data.get(*at).copied().ok_or("meshopt's indices cut short");
        *at += 1;
        b
    };
    let lead = byte()?;
    if lead < 128 {
        return Ok(u32::from(lead));
    }
    let mut v = u32::from(lead & 127);
    for k in 1..5 {
        let b = byte()?;
        v |= u32::from(b & 127).wrapping_shl(7 * k);
        if b < 128 {
            break;
        }
    }
    Ok(v)
}

/// Any other indices (mode INDICES): a header byte (0xd0 or 0xd1), then
/// for each index a variable-length number whose low bit picks one of two
/// baselines (each the last index taken from it) and the rest the delta
/// from it, zigzagged, then four bytes of tail.
pub fn sequence(data: &[u8], count: usize, size: usize) -> Result<Vec<u8>, String> {
    if size != 2 && size != 4 {
        return Err(format!("meshopt indices {size} bytes each"));
    }
    if !matches!(data.first(), Some(0xd0 | 0xd1)) {
        return Err("not meshopt's index sequence".into());
    }
    if data.len() < 1 + count + 4 {
        return Err("meshopt's index sequence cut short".into());
    }
    let end = data.len() - 4;
    let (mut at, mut last) = (1, [0u32; 2]);
    let mut out = Vec::with_capacity(count * size);
    for _ in 0..count {
        if at >= end {
            return Err("meshopt's index sequence cut short".into());
        }
        let v = vbyte(data, &mut at)?;
        let base = (v & 1) as usize;
        let d = v >> 1;
        last[base] = last[base].wrapping_add((d >> 1) ^ (d & 1).wrapping_neg());
        if size == 2 {
            out.extend((last[base] as u16).to_le_bytes());
        } else {
            out.extend(last[base].to_le_bytes());
        }
    }
    if at != end {
        return Err("meshopt's index sequence doesn't end where its tail starts".into());
    }
    Ok(out)
}

/// Undoes `filter` on vertex data, elements of `stride` bytes, in place.
pub fn unfilter(data: &mut [u8], stride: usize, filter: Filter) -> Result<(), String> {
    match (filter, stride) {
        (Filter::None, _) => {}
        (Filter::Octahedral, 4) => {
            for e in data.as_chunks_mut::<4>().0 {
                let v = octahedral([e[0], e[1], e[2]].map(|b| f32::from(b as i8)), 127.0);
                for (out, v) in e.iter_mut().zip(v) {
                    *out = v as i8 as u8;
                }
            }
        }
        (Filter::Octahedral, 8) => {
            for e in data.as_chunks_mut::<8>().0 {
                let v = octahedral([0, 1, 2].map(|k| f32::from(i16::from_le_bytes([e[2 * k], e[2 * k + 1]]))), 32767.0);
                for (k, v) in v.into_iter().enumerate() {
                    e[2 * k..2 * k + 2].copy_from_slice(&(v as i16).to_le_bytes());
                }
            }
        }
        (Filter::Quaternion, 8) => {
            for e in data.as_chunks_mut::<8>().0 {
                let q: [i16; 4] = [0, 1, 2, 3].map(|k| i16::from_le_bytes([e[2 * k], e[2 * k + 1]]));
                for (k, v) in quaternion(q).into_iter().enumerate() {
                    e[2 * k..2 * k + 2].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
        (Filter::Exponential, s) if s.is_multiple_of(4) => {
            for e in data.as_chunks_mut::<4>().0 {
                let v = i32::from_le_bytes([e[0], e[1], e[2], e[3]]);
                e.copy_from_slice(&exponential(v).to_le_bytes());
            }
        }
        (Filter::Color, 4) => {
            for e in data.as_chunks_mut::<4>().0 {
                let c = [e[0], e[1], e[2], e[3]];
                *e = color([i32::from(c[0]), i32::from(c[1] as i8), i32::from(c[2] as i8), i32::from(c[3])], 255.0).map(|v| v as u8);
            }
        }
        (Filter::Color, 8) => {
            for e in data.as_chunks_mut::<8>().0 {
                let c: [u16; 4] = [0, 1, 2, 3].map(|k| u16::from_le_bytes([e[2 * k], e[2 * k + 1]]));
                let rgba = color([i32::from(c[0]), i32::from(c[1] as i16), i32::from(c[2] as i16), i32::from(c[3])], 65535.0);
                for (k, v) in rgba.into_iter().enumerate() {
                    e[2 * k..2 * k + 2].copy_from_slice(&(v as u16).to_le_bytes());
                }
            }
        }
        _ => return Err(format!("meshopt's {filter:?} filter on elements {stride} bytes long")),
    }
    Ok(())
}

/// A unit vector from its octahedral coordinates `x` and `y`, `v[2]`
/// being one at the precision they were kept in: z is what's left of it,
/// and where that's below 0 the coordinates fold back over the
/// octahedron's lower half. Scaled to `max` and rounded, as integers.
fn octahedral(v: [f32; 3], max: f32) -> [i32; 3] {
    let (mut x, mut y) = (v[0], v[1]);
    let z = v[2] - x.abs() - y.abs();
    let t = z.min(0.0);
    x += if x >= 0.0 { t } else { -t };
    y += if y >= 0.0 { t } else { -t };
    let s = max / (x * x + y * y + z * z).sqrt();
    [x, y, z].map(|c| (c * s + if c >= 0.0 { 0.5 } else { -0.5 }) as i32)
}

/// A unit quaternion from its three smallest parts, scaled by √2 to the
/// precision the fourth part's high bits keep (its low two bits saying
/// which part was left out, the largest, made positive), that part their
/// remainder. As 16-bit integers, in order.
fn quaternion(q: [i16; 4]) -> [i16; 4] {
    let scale = std::f32::consts::FRAC_1_SQRT_2 / f32::from(q[3] | 3);
    let [x, y, z] = [q[0], q[1], q[2]].map(|c| f32::from(c) * scale);
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    let round = |c: f32| (c * 32767.0 + if c >= 0.0 { 0.5 } else { -0.5 }) as i32 as i16;
    let largest = (q[3] & 3) as usize;
    let mut out = [0i16; 4];
    out[(largest + 1) & 3] = round(x);
    out[(largest + 2) & 3] = round(y);
    out[(largest + 3) & 3] = round(z);
    out[largest] = round(w);
    out
}

/// An RGBA color from its luma `y`, its chroma `co` and `cg` (signed), and
/// its alpha, all in K bits, which the alpha's highest set bit (bit K - 1)
/// says: the color made from them, the alpha (less that bit) widened to K
/// bits by repeating its lowest, and all four scaled from K bits to `max`
/// and rounded.
fn color([y, co, cg, alpha]: [i32; 4], max: f32) -> [i32; 4] {
    let mut ones = alpha;
    for shift in [1, 2, 4, 8] {
        ones |= ones >> shift;
    }
    let a = ((alpha << 1) & ones) | (alpha & 1);
    let scale = max / ones.max(1) as f32;
    [y + co - cg, y + cg, y - co - cg, a].map(|v| (v as f32 * scale + 0.5) as i32)
}

/// A float from a signed 24-bit mantissa and the signed exponent in the
/// high byte: m × 2^e.
fn exponential(v: i32) -> f32 {
    let (m, e) = ((v << 8) >> 8, v >> 24);
    f32::from_bits(((e + 127) as u32).wrapping_shl(23)) * m as f32
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// meshoptimizer's vertex encoder (version 0), as the spec describes
    /// what it writes: for tests, and for the glTFs the tests build.
    pub fn pack_vertices(data: &[u8], stride: usize) -> Vec<u8> {
        let count = data.len() / stride;
        let mut out = vec![0xa0];
        let mut last = data[..stride.min(data.len())].to_vec();
        last.resize(stride, 0);
        let block = block_size(stride);
        let mut start = 0;
        while start < count {
            let n = block.min(count - start);
            let groups = n.div_ceil(GROUP) * GROUP;
            for k in 0..stride {
                let mut column = vec![0u8; groups];
                deltas(&mut column[..n], &data[start * stride..], stride, &last, k, 0);
                out.extend(pack_column(&column, &BITS_V0));
            }
            last.copy_from_slice(&data[(start + n - 1) * stride..(start + n) * stride]);
            start += n;
        }
        finish(out, data, stride, &[])
    }

    /// meshoptimizer's vertex encoder, version 1, as it encodes at level 2
    /// and over: `channels` says how each four bytes' deltas are taken (0
    /// bytes, 1 shorts, 2 | r << 4 xored and turned by r), and each column
    /// of a block is kept as its estimate finds smallest.
    pub fn pack_vertices_v1(data: &[u8], stride: usize, channels: &[u8]) -> Vec<u8> {
        let count = data.len() / stride;
        let mut out = vec![0xa1];
        let mut last = data[..stride.min(data.len())].to_vec();
        last.resize(stride, 0);
        let block = block_size(stride);
        let mut start = 0;
        while start < count {
            let n = block.min(count - start);
            let groups = n.div_ceil(GROUP) * GROUP;
            let control = out.len();
            out.resize(control + stride / 4, 0);
            for k in 0..stride {
                let mut column = vec![0u8; groups];
                deltas(&mut column[..n], &data[start * stride..], stride, &last, k, channels[k / 4]);
                let mode = estimate_control(&column, n);
                out[control + k / 4] |= (mode << ((k % 4) * 2)) as u8;
                match mode {
                    2 => {}
                    3 => out.extend(&column[..n]),
                    _ => out.extend(pack_column(&column, &BITS_V1[mode..mode + 4])),
                }
            }
            last.copy_from_slice(&data[(start + n - 1) * stride..(start + n) * stride]);
            start += n;
        }
        finish(out, data, stride, channels)
    }

    /// The tail: zeros to its least size, the first vertex, the channels.
    fn finish(mut out: Vec<u8>, data: &[u8], stride: usize, channels: &[u8]) -> Vec<u8> {
        let tail = stride + channels.len();
        out.extend(std::iter::repeat_n(0, tail.max(TAIL_MIN[usize::from(out[0] == 0xa1)]) - tail));
        out.extend(&data[..stride.min(data.len())]);
        out.resize(out.len() + stride.saturating_sub(data.len()), 0);
        out.extend(channels);
        out
    }

    /// Byte `k`'s deltas over `column.len()` vertices from `data`, the
    /// one before them `last`, as `channel` takes them (meshoptimizer's
    /// `encodeDeltas`): the byte's share of its number's delta.
    fn deltas(column: &mut [u8], data: &[u8], stride: usize, last: &[u8], k: usize, channel: u8) {
        let size = [1, 2, 4][usize::from(channel & 3)];
        let (k0, shift) = (k & !(size - 1), (k & (size - 1)) * 8);
        let bits = 8 * size as u32;
        let mask = u32::MAX >> (32 - bits);
        let read = |b: &[u8]| (0..size).fold(0u32, |v, j| v | u32::from(b[k0 + j]) << (8 * j));
        let mut p = read(last);
        for (i, out) in column.iter_mut().enumerate() {
            let v = read(&data[i * stride..]);
            let d = if size == 4 {
                (v ^ p).rotate_left(u32::from(channel >> 4))
            } else {
                let x = v.wrapping_sub(p) & mask;
                ((x << 1) ^ (0u32.wrapping_sub(x >> (bits - 1)))) & mask
            };
            *out = (d >> shift) as u8;
            p = v;
        }
    }

    /// How a column of `n` deltas (in whole groups) is kept, as
    /// meshoptimizer's `estimateControl` picks: all zeros (2), else the
    /// table of widths that takes fewer bytes (0, 1), unless that's no
    /// fewer than the bytes as they are (3).
    fn estimate_control(column: &[u8], n: usize) -> usize {
        if column.iter().all(|&v| v == 0) {
            return 2;
        }
        let header = (column.len() / GROUP).div_ceil(4);
        let (mut zero, mut eight) = (header, header);
        for group in column.as_chunks::<GROUP>().0 {
            let small = [1, 2, 4].map(|b| measure(group, b)).into_iter().min().unwrap();
            zero += small.min(measure(group, 0));
            eight += small.min(measure(group, 8));
        }
        if zero < n || eight < n {
            usize::from(zero >= eight)
        } else {
            3
        }
    }

    /// The bytes a group takes packed `bits` a value; none fits zeros.
    fn measure(group: &[u8; GROUP], bits: usize) -> usize {
        match bits {
            0 if group.iter().all(|&v| v == 0) => 0,
            0 => usize::MAX,
            8 => GROUP,
            _ => GROUP * bits / 8 + group.iter().filter(|&&v| usize::from(v) >= (1 << bits) - 1).count(),
        }
    }

    /// One column: each group as the fewest bytes of `widths`, as
    /// meshoptimizer's `encodeBytes` picks (eight bits unless another is
    /// smaller, a tie going to the width the group before took).
    fn pack_column(column: &[u8], widths: &[usize]) -> Vec<u8> {
        let groups = column.len() / GROUP;
        let mut header = vec![0u8; groups.div_ceil(4)];
        let mut body = Vec::new();
        let mut before = usize::MAX;
        for (g, group) in column.as_chunks::<GROUP>().0.iter().enumerate() {
            let mut best = 3;
            for code in 0..3 {
                let (size, best_size) = (measure(group, widths[code]), measure(group, widths[best]));
                if size < best_size || (size == best_size && widths[code] == before && widths[best] != 8) {
                    best = code;
                }
            }
            let bits = widths[best];
            if bits == 8 {
                body.extend(group);
            } else if bits > 0 {
                let sentinel = (1u8 << bits) - 1;
                body.extend(group.chunks(8 / bits).map(|c| {
                    let byte = c.iter().fold(0u8, |byte, &v| (byte << bits) | v.min(sentinel));
                    // One bit each goes low bit first.
                    if bits == 1 {
                        byte.reverse_bits()
                    } else {
                        byte
                    }
                }));
                body.extend(group.iter().filter(|&&v| v >= sentinel));
            }
            header[g / 4] |= (best as u8) << ((g % 4) * 2);
            before = bits;
        }
        header.extend(body);
        header
    }

    /// The table meshoptimizer codes the most common triangles' second and
    /// third vertices with.
    const TABLE: [u8; 16] = [0x00, 0x76, 0x87, 0x56, 0x67, 0x78, 0xa9, 0x86, 0x65, 0x89, 0x68, 0x98, 0x01, 0x69, 0, 0];

    /// meshoptimizer's index encoder, `version` 0 or 1.
    pub fn pack_triangles(indices: &[u32], version: u8) -> Vec<u8> {
        let fec_max = if version >= 1 { 13 } else { 15 };
        let mut codes = vec![0xe0 | version];
        let mut extra = Vec::new();
        let mut fifos = Fifos::default();
        let (mut next, mut last) = (0u32, 0u32);
        // Where an edge of the triangle is in the FIFO, and which of its
        // three edges it is; where a vertex is (0 the newest).
        let find_edge = |f: &Fifos, t: [u32; 3]| {
            (0..16).find_map(|i| {
                let [e0, e1] = f.edge(i);
                [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])].iter().position(|&(a, b)| e0 == a && e1 == b).map(|r| (i, r))
            })
        };
        let find_seen = |f: &Fifos, v: u32| (0..16).find(|&i| f.back(i + 1) == v);
        let zigzag = |d: u32| (d << 1) ^ ((d as i32 >> 31) as u32);
        let vbyte = |out: &mut Vec<u8>, mut v: u32| {
            while v >= 128 {
                out.push((v & 127) as u8 | 128);
                v >>= 7;
            }
            out.push(v as u8);
        };
        for &t in indices.as_chunks::<3>().0 {
            if let Some((fe, r)) = find_edge(&fifos, t).filter(|(fe, _)| *fe < 15) {
                let (a, b, c) = (t[r], t[(r + 1) % 3], t[(r + 2) % 3]);
                let mut fec = match find_seen(&fifos, c) {
                    Some(f) if f >= 1 && f < fec_max => f,
                    _ if c == next => {
                        next += 1;
                        0
                    }
                    _ => 15,
                };
                if fec == 15 && version >= 1 {
                    if c.wrapping_add(1) == last {
                        (fec, last) = (13, c);
                    } else if c == last.wrapping_add(1) {
                        (fec, last) = (14, c);
                    }
                }
                codes.push(((fe << 4) | fec) as u8);
                if fec == 15 {
                    vbyte(&mut extra, zigzag(c.wrapping_sub(last)));
                    last = c;
                }
                if fec == 0 || fec >= fec_max {
                    fifos.push(c);
                }
                fifos.push_edge([c, b]);
                fifos.push_edge([a, c]);
            } else {
                let r = if t[1] == next { 1 } else if t[2] == next { 2 } else { 0 };
                let (a, b, c) = (t[r], t[(r + 1) % 3], t[(r + 2) % 3]);
                let reset = a == 0 && b == 1 && c == 2 && next > 0 && version >= 1;
                if reset {
                    next = 0;
                    fifos.seen = [u32::MAX; 16];
                }
                let (fb, fc) = (find_seen(&fifos, b), find_seen(&fifos, c));
                let fea = if a == next {
                    next += 1;
                    0
                } else {
                    15
                };
                let mut corner = |f: Option<usize>, v: u32| match f {
                    Some(f) if f < 14 => f + 1,
                    _ if v == next => {
                        next += 1;
                        0
                    }
                    _ => 15,
                };
                let feb = corner(fb, b);
                let fec = corner(fc, c);
                let aux = ((feb << 4) | fec) as u8;
                match TABLE[..14].iter().position(|&x| x == aux) {
                    Some(k) if fea == 0 && !reset => codes.push(0xf0 | k as u8),
                    _ => {
                        codes.push(0xfe | u8::from(fea == 15));
                        extra.push(aux);
                    }
                }
                for (fe, v) in [(fea, a), (feb, b), (fec, c)] {
                    if fe == 15 {
                        vbyte(&mut extra, zigzag(v.wrapping_sub(last)));
                        last = v;
                    }
                }
                for (fe, v) in [(fea, a), (feb, b), (fec, c)] {
                    if fe == 0 || fe == 15 {
                        fifos.push(v);
                    }
                }
                fifos.push_edge([b, a]);
                fifos.push_edge([c, b]);
                fifos.push_edge([a, c]);
            }
        }
        codes.extend(extra);
        codes.extend(TABLE);
        codes
    }

    /// meshoptimizer's index sequence encoder: it takes the other baseline
    /// as soon as the delta from this one is 30 or more.
    pub fn pack_sequence(indices: &[u32]) -> Vec<u8> {
        let mut out = vec![0xd1];
        let (mut last, mut base) = ([0u32; 2], 0usize);
        for &i in indices {
            base ^= usize::from((i.wrapping_sub(last[base]) as i32).unsigned_abs() >= 30);
            let d = i.wrapping_sub(last[base]);
            let mut v = (((d << 1) ^ ((d as i32 >> 31) as u32)) << 1) | base as u32;
            while v >= 128 {
                out.push((v & 127) as u8 | 128);
                v >>= 7;
            }
            out.push(v as u8);
            last[base] = i;
        }
        out.extend([0; 4]);
        out
    }

    /// A unit normal as meshoptimizer's octahedral filter keeps it, in
    /// `bits` (at most 8 for bytes, 16 for shorts) and stored as `size`
    /// bytes a value; the fourth value is `w`.
    pub fn octahedral_normal(n: [f32; 3], bits: u32, w: i32) -> [i32; 4] {
        let l = n[0].abs() + n[1].abs() + n[2].abs();
        let (mut x, mut y) = (n[0] / l, n[1] / l);
        if n[2] < 0.0 {
            let (ox, oy) = (x, y);
            x = (1.0 - oy.abs()) * if ox >= 0.0 { 1.0 } else { -1.0 };
            y = (1.0 - ox.abs()) * if oy >= 0.0 { 1.0 } else { -1.0 };
        }
        let one = (1 << (bits - 1)) - 1;
        let q = |v: f32| (v * one as f32 + if v >= 0.0 { 0.5 } else { -0.5 }) as i32;
        [q(x), q(y), one, w]
    }

    #[test]
    fn a_vertex_stream_worked_out_by_hand_unpacks_to_its_vertices() {
        // Four vertices of two 16-bit numbers, (0, 0), (300, 0), (0, 300)
        // and (300, 300): one block, one group a column.
        let packed = [
            0xa0, // vertex codec, version 0
            // x's low bytes 0, 0x2c, 0, 0x2c: deltas 0, 44, -44, 44,
            // zigzagged 0, 0x58, 0x57, 0x58; in two bits (header 1), the
            // first four all ones (escaped) but the first, 00 11 11 11,
            // the other twelve 0, then the escaped bytes.
            0x01, 0x3f, 0x00, 0x00, 0x00, 0x58, 0x57, 0x58,
            // x's high bytes 0, 1, 0, 1: deltas 0, 1, -1, 1, zigzagged
            // 0, 2, 1, 2: 00 10 01 10.
            0x01, 0x26, 0x00, 0x00, 0x00,
            // y's low bytes 0, 0, 0x2c, 0x2c: 0, 0, 0x58 (escaped), 0.
            0x01, 0x0c, 0x00, 0x00, 0x00, 0x58,
            // y's high bytes 0, 0, 1, 1: 0, 0, 2, 0.
            0x01, 0x08, 0x00, 0x00, 0x00,
            // The tail: 32 bytes, the last four the first vertex.
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let out = vertices(&packed, 4, 4).unwrap();
        let shorts: Vec<u16> = out.as_chunks::<2>().0.iter().map(|&b| u16::from_le_bytes(b)).collect();
        assert_eq!(shorts, [0, 0, 300, 0, 0, 300, 300, 300]);
        // The test encoder writes just that.
        let raw: Vec<u8> = [0u16, 0, 300, 0, 0, 300, 300, 300].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(pack_vertices(&raw, 4), packed);
        // Cut short, or with bytes left over, or a version Flow doesn't read.
        assert!(vertices(&packed[..packed.len() - 1], 4, 4).is_err());
        let mut longer = packed.to_vec();
        longer.insert(30, 0);
        assert!(vertices(&longer, 4, 4).is_err());
        let mut v2 = packed.to_vec();
        v2[0] = 0xa2;
        assert_eq!(vertices(&v2, 4, 4).unwrap_err(), "meshopt's vertex codec version 2, which Flow doesn't read");
        assert!(vertices(&packed, 4, 6).is_err(), "a stride not a multiple of 4");
    }

    #[test]
    fn vertex_streams_unpack_as_they_were_packed_over_many_blocks() {
        // Each group packed every way: zeros, small deltas, middling ones
        // with escapes, and noise; strides from 4 to 256, so blocks of 256
        // vertices down to 32, and some vertices past a whole block.
        let mut seed = 12345u32;
        let mut random = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as u8
        };
        for (stride, count) in [(4, 1), (4, 600), (12, 257), (16, 1000), (64, 300), (256, 70), (8, 0)] {
            let mut data = vec![0u8; count * stride];
            for (i, b) in data.iter_mut().enumerate() {
                let (v, k) = (i / stride, i % stride);
                *b = match k % 4 {
                    0 => (v / 7) as u8,
                    1 => random() & 3,
                    2 => random(),
                    _ => 0,
                } ^ (v as u8 & if k == 5 { 0xf } else { 0 });
            }
            let packed = pack_vertices(&data, stride);
            assert_eq!(vertices(&packed, count, stride).unwrap(), data, "{stride} × {count}");
        }
    }

    #[test]
    fn a_version_1_vertex_stream_worked_out_by_hand_unpacks_to_its_vertices() {
        // Four vertices of four bytes, each byte's column kept another way.
        let packed = [
            0xa1, // vertex codec, version 1
            // The block's control: byte 0 all zeros (2), byte 1 as it is
            // (3), byte 2 from the first table (0), byte 3 from the second
            // (1): 10 00 11 10 from the high bits down.
            0x4e,
            // Byte 1: 10, 20, 30, 40 from the baseline's 10, deltas 0, 10,
            // 10, 10, zigzagged.
            0, 20, 20, 20,
            // Byte 2: 0, 1, 1, 0, deltas 0, 1, 0, -1, zigzagged 0, 2, 0, 1:
            // one bit each (header 1), low bit first, the second and fourth
            // escaped, then the escaped bytes.
            0x01, 0x0a, 0x00, 2, 1,
            // Byte 3: 0, 0x80, 0x7f, 1: deltas 0, -128, -1, -126, zigzagged
            // 0, 255, 1, 251, as bytes (the second table's last, header 3).
            0x03, 0, 255, 1, 251, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            // The tail: 24 bytes, the first vertex, then its channel (0,
            // bytes).
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0,
        ];
        assert_eq!(vertices(&packed, 4, 4).unwrap(), [0, 10, 0, 0, 0, 20, 1, 0x80, 0, 30, 1, 0x7f, 0, 40, 0, 1]);
        // Two shorts (channel 1): (1000, 5) then (1300, 3), deltas 300 and
        // -2 zigzagged to 600 and 3, each column as it is.
        let tail = |vertex: [u8; 4], channel: u8| [vec![0; 19], vertex.to_vec(), vec![channel]].concat();
        let shorts = [vec![0xa1, 0xff, 0, 0x58, 0, 0x02, 0, 3, 0, 0], tail([0xe8, 0x03, 5, 0], 1)].concat();
        assert_eq!(vertices(&shorts, 2, 4).unwrap(), [0xe8, 0x03, 5, 0, 0x14, 0x05, 3, 0]);
        // A 32-bit number xored and turned 3 left (channel 2 | 3 << 4):
        // 0x12345678 then 0x12345679, their xor 1 kept as 8.
        let turned = [vec![0xa1, 0xff, 0, 8, 0, 0, 0, 0, 0, 0], tail([0x78, 0x56, 0x34, 0x12], 0x32)].concat();
        assert_eq!(vertices(&turned, 2, 4).unwrap(), [0x78, 0x56, 0x34, 0x12, 0x79, 0x56, 0x34, 0x12]);
        // The test encoder, as meshoptimizer's, keeps such short columns
        // as they are, which take fewer bytes than groups do.
        let raw = [0, 10, 0, 0, 0, 20, 1, 0x80, 0, 30, 1, 0x7f, 0, 40, 0, 1];
        let ours = pack_vertices_v1(&raw, 4, &[0]);
        assert_eq!((ours[1], ours.len()), (0xfe, packed.len() - 14));
        assert_eq!(vertices(&ours, 4, 4).unwrap(), raw);
        // A channel past the three, a tail cut short, bytes left over.
        let mut bad = turned.clone();
        *bad.last_mut().unwrap() = 3;
        assert_eq!(vertices(&bad, 2, 4).unwrap_err(), "meshopt's vertex channel 3, which isn't one");
        assert!(vertices(&turned[..turned.len() - 1], 2, 4).is_err());
        let mut longer = turned.clone();
        longer.insert(5, 0);
        assert!(vertices(&longer, 2, 4).is_err());
    }

    #[test]
    fn version_1_vertex_streams_unpack_as_they_were_packed_however_their_channels_take_them() {
        let mut seed = 977u32;
        let mut random = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as u8
        };
        for (stride, count) in [(4, 1), (4, 600), (12, 257), (16, 1000), (64, 300), (256, 70), (8, 0), (8, 33)] {
            // Floats that change slowly (so xor and shorts pay), small
            // counters, noise, and zeros, a kind to each four bytes.
            let mut data = vec![0u8; count * stride];
            for v in 0..count {
                for g in 0..stride / 4 {
                    let at = v * stride + g * 4;
                    let word = match g % 4 {
                        0 => (1.0 + v as f32 * 0.01).to_bits(),
                        1 => (v as u32 * 37) & 0xffff | (v as u32 / 3) << 16,
                        2 => u32::from_le_bytes([random(), random() & 7, random(), 0]),
                        _ => 0,
                    };
                    data[at..at + 4].copy_from_slice(&word.to_le_bytes());
                }
            }
            for channels in [vec![0u8; stride / 4], vec![1; stride / 4], vec![2; stride / 4], (0..stride / 4).map(|g| [0x72, 1, 0, 0x32][g % 4]).collect()] {
                let packed = pack_vertices_v1(&data, stride, &channels);
                assert_eq!(vertices(&packed, count, stride).unwrap(), data, "{stride} × {count} by {channels:?}");
            }
        }
    }

    #[test]
    fn the_color_filter_makes_rgba_of_ycocg_at_the_alphas_precision() {
        // Eight bits a value: (200, 100, 50, 255) as meshoptimizer keeps
        // it, y 113, co 75, cg -12 (its integer Co and Cg lose g's low bit),
        // alpha 127 with bit 7 set.
        let mut bytes = [113, 75, (-12i8) as u8, 255];
        unfilter(&mut bytes, 4, Filter::Color).unwrap();
        assert_eq!(bytes, [200, 101, 50, 255]);
        // In six bits (alpha 31 with bit 5 set): y 40, co 10, cg -5 make
        // (55, 35, 35, 63), scaled by 255/63.
        let mut six = [40, 10, (-5i8) as u8, 63, 40, 10, (-5i8) as u8, 32 | 8];
        unfilter(&mut six, 4, Filter::Color).unwrap();
        assert_eq!(six, [223, 142, 142, 255, 223, 142, 142, 65], "alpha 8 widened to 17, of 63");
        // Sixteen bits, in twelve: y 1000, co -200, cg 300 make (500,
        // 1300, 900), scaled by 65535/4095.
        let mut wide: Vec<u8> = [1000i16, -200, 300, 4095].iter().flat_map(|v| v.to_le_bytes()).collect();
        unfilter(&mut wide, 8, Filter::Color).unwrap();
        assert_eq!(as_u32(&wide, 2), [8002, 20805, 14403, 65535]);
        assert!(unfilter(&mut [0; 12], 12, Filter::Color).is_err(), "only 4 or 8 bytes");
    }

    /// meshoptimizer's own test of its index codec (version 0): four
    /// triangles, the third and fourth needing full indices.
    const TRIANGLES_V0: [u8; 27] = [
        0xe0, 0xf0, 0x10, 0xfe, 0xff, 0xf0, 0x0c, 0xff, 0x02, 0x02, 0x02, 0x00, 0x76, 0x87, 0x56, 0x67, 0x78, 0xa9, 0x86, 0x65, 0x89, 0x68, 0x98, 0x01, 0x69,
        0x00, 0x00,
    ];

    /// And of version 1: a restart (0 1 2) and the last index less one.
    const TRIANGLES_V1: [u8; 24] =
        [0xe1, 0xf0, 0x10, 0xfe, 0x1f, 0x3d, 0x00, 0x0a, 0x00, 0x76, 0x87, 0x56, 0x67, 0x78, 0xa9, 0x86, 0x65, 0x89, 0x68, 0x98, 0x01, 0x69, 0x00, 0x00];

    fn as_u32(bytes: &[u8], size: usize) -> Vec<u32> {
        bytes.chunks_exact(size).map(|b| if size == 2 { u32::from(u16::from_le_bytes([b[0], b[1]])) } else { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }).collect()
    }

    #[test]
    fn triangles_worked_out_by_hand_unpack_to_their_indices() {
        // Version 0: (0 1 2) from the table's first code, (2 1 3) on the
        // edge 2-1 with the next vertex, (4 6 5) with 6 in full (a delta of
        // 6, zigzagged 0x0c), (7 8 9) all three in full, a delta of 1 each.
        assert_eq!(as_u32(&triangles(&TRIANGLES_V0, 12, 4).unwrap(), 4), [0, 1, 2, 2, 1, 3, 4, 6, 5, 7, 8, 9]);
        assert_eq!(as_u32(&triangles(&TRIANGLES_V0, 12, 2).unwrap(), 2), [0, 1, 2, 2, 1, 3, 4, 6, 5, 7, 8, 9]);
        // Version 1: then a restart from 0 (0xfe with a byte of 0), (2 1 5)
        // on edge 2-1 with 5 in full (0x1f, 0x0a), and (2 1 4) with the
        // last index less one (0x3d).
        assert_eq!(as_u32(&triangles(&TRIANGLES_V1, 15, 4).unwrap(), 4), [0, 1, 2, 2, 1, 3, 0, 1, 2, 2, 1, 5, 2, 1, 4]);
        // The test encoder writes the same bytes.
        assert_eq!(pack_triangles(&[0, 1, 2, 2, 1, 3, 4, 6, 5, 7, 8, 9], 0), TRIANGLES_V0);
        assert_eq!(pack_triangles(&[0, 1, 2, 2, 1, 3, 0, 1, 2, 2, 1, 5, 2, 1, 4], 1), TRIANGLES_V1);
        // Short, long, or not triangles.
        assert!(triangles(&TRIANGLES_V0[..26], 12, 4).is_err());
        assert!(triangles(&TRIANGLES_V0, 33, 4).is_err(), "more triangles than it has room for");
        assert!(triangles(&TRIANGLES_V0, 11, 4).is_err());
        assert!(triangles(&TRIANGLES_V0[1..], 12, 4).is_err());
    }

    #[test]
    fn triangles_unpack_as_they_were_packed_whatever_their_order() {
        // A grid's triangles, in order and shuffled (so the FIFOs miss and
        // indices go in full), and turned each way round.
        let (w, h) = (24u32, 17u32);
        let mut grid = Vec::new();
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let i = y * w + x;
                grid.extend([i, i + 1, i + w, i + 1, i + w + 1, i + w]);
            }
        }
        let mut shuffled: Vec<[u32; 3]> = grid.as_chunks::<3>().0.to_vec();
        let mut seed = 7u32;
        for i in (1..shuffled.len()).rev() {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            shuffled.swap(i, (seed >> 8) as usize % (i + 1));
        }
        let shuffled: Vec<u32> = shuffled.iter().enumerate().flat_map(|(k, t)| [t[k % 3], t[(k + 1) % 3], t[(k + 2) % 3]]).collect();
        let restarts: Vec<u32> = [0, 1, 2, 2, 1, 3].repeat(3).into_iter().chain([3, 1, 4, 9, 8, 7, 100_000, 5, 99_999]).collect();
        for indices in [&grid, &shuffled, &restarts] {
            for version in [0, 1] {
                let packed = pack_triangles(indices, version);
                let out = as_u32(&triangles(&packed, indices.len(), 4).unwrap(), 4);
                // The same triangles, each maybe turned to start at another corner.
                for (got, want) in out.as_chunks::<3>().0.iter().zip(indices.as_chunks::<3>().0) {
                    let turns = [[want[0], want[1], want[2]], [want[1], want[2], want[0]], [want[2], want[0], want[1]]];
                    assert!(turns.contains(&[got[0], got[1], got[2]]), "version {version}: {got:?} for {want:?}");
                }
            }
        }
    }

    #[test]
    fn an_index_sequence_unpacks_from_its_two_baselines() {
        // meshoptimizer's own test: 0 and 1 from the first baseline, 51
        // (0xcd 0x01, two bytes) from the second, back to the first for 2,
        // 49 as the second's 51 less 2, and 1000 from the first.
        let packed = [0xd1, 0x00, 0x04, 0xcd, 0x01, 0x04, 0x07, 0x98, 0x1f, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(as_u32(&sequence(&packed, 6, 4).unwrap(), 4), [0, 1, 51, 2, 49, 1000]);
        assert_eq!(pack_sequence(&[0, 1, 51, 2, 49, 1000]), packed);
        assert_eq!(as_u32(&sequence(&packed, 6, 2).unwrap(), 2), [0, 1, 51, 2, 49, 1000]);
        let far: Vec<u32> = vec![70_000, 3, 70_001, 0, u32::MAX, 5];
        assert_eq!(as_u32(&sequence(&pack_sequence(&far), 6, 4).unwrap(), 4), far);
        assert!(sequence(&packed, 7, 4).is_err(), "more indices than it holds");
        assert!(sequence(&packed[..12], 6, 4).is_err());
    }

    #[test]
    fn the_octahedral_filter_unfolds_unit_vectors() {
        // Worked out by hand: straight up stays; (64, 0) leans over, its z
        // 127 - 64 = 63, made 127 long: (90.5, 0, 89.1), rounded.
        let mut bytes = [0, 0, 127, 5, 64, 0, 127, 9];
        unfilter(&mut bytes, 4, Filter::Octahedral).unwrap();
        assert_eq!(bytes.map(|b| b as i8), [0, 0, 127, 5, 91, 0, 89, 9], "the fourth value kept");
        // Past the octahedron's edge, z below 0 folds x and y back: (100,
        // 50) leaves z = -23, so (77, 27, -23), made 127 long.
        let mut folded = [100, 50, 127, 0].map(|v: i8| v as u8);
        unfilter(&mut folded, 4, Filter::Octahedral).unwrap();
        assert_eq!(folded.map(|b| b as i8), [115, 40, -34, 0]);
        // 16 bits: a unit vector kept at 12 bits' precision comes back unit
        // length at 16, pointing the same way.
        for n in [[0.6f32, -0.48, 0.64], [-0.36, 0.48, -0.8], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]] {
            let v = octahedral_normal(n, 12, -7);
            let mut bytes: Vec<u8> = v.iter().flat_map(|&c| (c as i16).to_le_bytes()).collect();
            unfilter(&mut bytes, 8, Filter::Octahedral).unwrap();
            let out: Vec<f32> = bytes.as_chunks::<2>().0.iter().map(|&b| f32::from(i16::from_le_bytes(b))).collect();
            assert!((0..3).all(|k| (out[k] / 32767.0 - n[k]).abs() < 0.002), "{n:?}: {out:?}");
            assert_eq!(out[3], -7.0);
        }
    }

    #[test]
    fn the_quaternion_filter_puts_back_the_largest_part() {
        // The identity: w left out (3), its scale 32767's high bits.
        let mut bytes: Vec<u8> = [0i16, 0, 0, 32767].iter().flat_map(|v| v.to_le_bytes()).collect();
        unfilter(&mut bytes, 8, Filter::Quaternion).unwrap();
        assert_eq!(as_u32(&bytes, 2), [0, 0, 0, 32767]);
        // A quarter turn about z, (0, 0, 0.7071, 0.7071): z left out (2),
        // the parts after it in turn, w then x then y, w's 0.7071 times √2
        // being 32767.
        let mut bytes: Vec<u8> = [32767i16, 0, 0, 32764 | 2].iter().flat_map(|v| v.to_le_bytes()).collect();
        unfilter(&mut bytes, 8, Filter::Quaternion).unwrap();
        let q: Vec<i16> = bytes.as_chunks::<2>().0.iter().map(|&b| i16::from_le_bytes(b)).collect();
        assert_eq!(q, [0, 0, 23170, 23170]);
        assert!(unfilter(&mut [0; 4], 4, Filter::Quaternion).is_err(), "only 16-bit quaternions");
    }

    #[test]
    fn the_exponential_filter_makes_floats_of_mantissa_and_exponent() {
        // 3 × 2^-1 and -5 × 2^2.
        let mut bytes: Vec<u8> = [0xff00_0003u32, 0x02ff_fffb, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
        unfilter(&mut bytes, 12, Filter::Exponential).unwrap();
        let f: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|&b| f32::from_le_bytes(b)).collect();
        assert_eq!(f, [1.5, -20.0, 0.0]);
    }

    #[test]
    fn a_filtered_vertex_stream_unpacks_through_its_filter() {
        let normals: Vec<u8> = [[0.0, 0.0, 1.0], [0.6, 0.8, 0.0], [0.0, -0.6, -0.8]].iter().flat_map(|&n| octahedral_normal(n, 8, 0).map(|c| c as i8 as u8)).collect();
        let out = unpack(&pack_vertices(&normals, 4), 3, 4, Mode::Attributes, Filter::Octahedral).unwrap();
        let n: Vec<i8> = out.iter().map(|&b| b as i8).collect();
        assert_eq!(&n[..4], [0, 0, 127, 0]);
        assert!((f32::from(n[4]) / 127.0 - 0.6).abs() < 0.02 && (f32::from(n[5]) / 127.0 - 0.8).abs() < 0.02 && n[6] == 0, "{n:?}");
        assert!(n[8] == 0 && (f32::from(n[9]) / 127.0 + 0.6).abs() < 0.02 && (f32::from(n[10]) / 127.0 + 0.8).abs() < 0.02, "{n:?}");
        assert!(unpack(&pack_triangles(&[0, 1, 2], 1), 3, 4, Mode::Triangles, Filter::Octahedral).is_err(), "indices have no filter");
    }
}
