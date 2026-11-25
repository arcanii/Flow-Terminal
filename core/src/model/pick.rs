//! Clicking a model: Flow's own addition to the Ratty protocol, which
//! leaves room for "future interactive behavior". A program asks for clicks
//! on a placement with `clicks=1` on `p` or `u` (`clicks=0` stops them; a
//! terminal that doesn't know the key ignores it), and when the user presses
//! a button over the model's picture (not with Shift held, as for other
//! mouse reports), Flow casts a ray from the viewer through that point and,
//! if it meets the model, answers
//!
//!   ESC _ ratty;g;click;id=<id>;button=<left|middle|right>;x=<x>;y=<y>;z=<z>[;node=<n>] ESC \
//!
//! with the object's id, the button, where it met the model in the mesh's
//! own unit box (centered, its longest side 1, y up and z toward the
//! viewer, as posed at that moment, before the placement turns, scales and
//! moves it), and for a glTF, the index of the node whose mesh it met. A
//! press that misses the model (through its picture's empty corners) gets
//! no answer, and goes on as a click would without the model.

use super::{add, cross, dot, sub, Vertex};

/// The mouse button pressed on a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

impl Button {
    fn name(self) -> &'static str {
        match self {
            Button::Left => "left",
            Button::Middle => "middle",
            Button::Right => "right",
        }
    }
}

/// Where a click met a model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// In the mesh's unit box.
    pub point: [f32; 3],
    /// The glTF node whose mesh it met.
    pub node: Option<usize>,
}

/// The answer to a click on object `id`.
pub fn answer(id: u32, button: Button, hit: &Hit) -> String {
    let [x, y, z] = hit.point.map(|v| if v.abs() < 5e-5 { 0.0 } else { v });
    let node = hit.node.map(|n| format!(";node={n}")).unwrap_or_default();
    format!("\x1b_ratty;g;click;id={id};button={};x={x:.4};y={y:.4};z={z:.4}{node}\x1b\\", button.name())
}

/// Where the line through `origin` along `dir` (away from the viewer)
/// first meets a triangle of `parts`, from either side, no nearer than
/// `from` times `dir` from `origin`: the part it met and the point,
/// nearest the viewer. Drawn straight on, the line runs both ways from
/// `origin`, which is on the picture's plane with the model about it
/// (`from` is minus infinity); seen through a camera, it runs from the
/// eye, and what's nearer than the camera draws isn't met.
pub fn cast<'a>(parts: impl IntoIterator<Item = (&'a [Vertex], &'a [u32])>, origin: [f32; 3], dir: [f32; 3], from: f32) -> Option<(usize, [f32; 3])> {
    let mut best: Option<(f32, usize)> = None;
    for (k, (vertices, indices)) in parts.into_iter().enumerate() {
        for t in indices.as_chunks::<3>().0 {
            let [Some(a), Some(b), Some(c)] = t.map(|i| vertices.get(i as usize).map(|v| v.pos)) else { continue };
            if let Some(s) = through(origin, dir, [a, b, c]).filter(|&s| s >= from) {
                if best.is_none_or(|(nearest, _)| s < nearest) {
                    best = Some((s, k));
                }
            }
        }
    }
    best.map(|(s, k)| (k, add(origin, dir.map(|v| v * s))))
}

/// How far along `dir` from `origin` the line meets the triangle, ahead or
/// behind (Möller and Trumbore's way), or None if it passes by.
fn through(origin: [f32; 3], dir: [f32; 3], [a, b, c]: [[f32; 3]; 3]) -> Option<f32> {
    const EDGE: f32 = 1e-6;
    let (e1, e2) = (sub(b, a), sub(c, a));
    let p = cross(dir, e2);
    let det = dot(e1, p);
    // Seen edge on.
    if det.abs() < 1e-20 {
        return None;
    }
    let s = sub(origin, a);
    let u = dot(s, p) / det;
    if !(-EDGE..=1.0 + EDGE).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(dir, q) / det;
    if v < -EDGE || u + v > 1.0 + EDGE {
        return None;
    }
    Some(dot(e2, q) / det)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(z: f32) -> (Vec<Vertex>, Vec<u32>) {
        let v = |x: f32, y: f32| Vertex::new([x, y, z], [0.0, 0.0, 1.0], [0.0; 2]);
        (vec![v(-0.5, -0.5), v(0.5, -0.5), v(0.5, 0.5), v(-0.5, 0.5)], vec![0, 1, 2, 0, 2, 3])
    }

    #[test]
    fn a_ray_meets_the_nearest_triangle_in_front_or_behind_its_origin() {
        let (front, fi) = square(0.25);
        let (back, bi) = square(-0.25);
        let down = [0.0, 0.0, -1.0];
        // The back square listed first: the front one is still the nearer.
        let hit = cast([(&back[..], &bi[..]), (&front[..], &fi[..])], [0.1, 0.2, 0.0], down, f32::NEG_INFINITY);
        assert_eq!(hit, Some((1, [0.1, 0.2, 0.25])));
        // Only the back one, behind the origin: met all the same.
        assert_eq!(cast([(&back[..], &bi[..])], [0.1, 0.2, 0.0], down, f32::NEG_INFINITY), Some((0, [0.1, 0.2, -0.25])));
        // Beside them, and along their plane: nothing.
        assert_eq!(cast([(&front[..], &fi[..])], [0.7, 0.0, 0.0], down, f32::NEG_INFINITY), None);
        assert_eq!(cast([(&front[..], &fi[..])], [0.0, 0.0, 0.25], [1.0, 0.0, 0.0], f32::NEG_INFINITY), None);
        // On the diagonal two triangles share, and slanted.
        assert!(cast([(&front[..], &fi[..])], [0.3, 0.3, 0.0], down, f32::NEG_INFINITY).is_some());
        // Slanted, the front square is met behind the origin, lower down.
        let slant = super::super::normalize([0.0, 1.0, -1.0]);
        let hit = cast([(&front[..], &fi[..])], [0.0, -0.1, 0.0], slant, f32::NEG_INFINITY).unwrap().1;
        assert!((hit[1] + 0.35).abs() < 1e-5 && (hit[2] - 0.25).abs() < 1e-5, "{hit:?}");
    }

    #[test]
    fn a_ray_from_a_cameras_eye_meets_nothing_nearer_than_it_draws() {
        let (front, fi) = square(0.25);
        let (back, bi) = square(-0.25);
        let both = || [(&back[..], &bi[..]), (&front[..], &fi[..])];
        // From an eye at z 1, two units along `dir` reach z -1.
        let (eye, dir) = ([0.1, 0.2, 1.0], [0.0, 0.0, -1.0]);
        assert_eq!(cast(both(), eye, dir, 0.5), Some((1, [0.1, 0.2, 0.25])), "the front square, 0.75 along");
        assert_eq!(cast(both(), eye, dir, 1.0), Some((0, [0.1, 0.2, -0.25])), "the front one nearer than drawn: the back");
        assert_eq!(cast(both(), eye, dir, 1.5), None);
        // Behind the eye: never.
        assert_eq!(cast(both(), [0.1, 0.2, -1.0], [0.0, 0.0, 1.0], 0.0).map(|h| h.0), Some(0), "the back square, nearer that way");
        assert_eq!(cast(both(), [0.1, 0.2, 2.0], [0.0, 0.0, 1.0], 0.0), None);
    }

    #[test]
    fn a_click_is_answered_with_the_object_the_button_and_the_point() {
        let hit = Hit { point: [0.12345, -0.5, -0.00001], node: Some(3) };
        assert_eq!(answer(7, Button::Left, &hit), "\x1b_ratty;g;click;id=7;button=left;x=0.1235;y=-0.5000;z=0.0000;node=3\x1b\\");
        let obj = Hit { point: [0.0, 0.25, 0.5], node: None };
        assert_eq!(answer(1, Button::Right, &obj), "\x1b_ratty;g;click;id=1;button=right;x=0.0000;y=0.2500;z=0.5000\x1b\\");
    }
}
