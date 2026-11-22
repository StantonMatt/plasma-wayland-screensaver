// SPDX-License-Identifier: GPL-3.0-or-later
//! One triangle strip per visible run. Neighbouring Hermite intervals reference
//! the same pair of end vertices. Degenerate triangles separate distinct runs.
use super::super::ffi::Lease;
use super::mix;
use super::render::{CURTAIN, GHOST, Quad, endpoints, sample};
use std::mem::MaybeUninit;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    position_uv: [f32; 4],
    light: [f32; 4],
    surface: [f32; 4],
}

struct Strip<'a> {
    output: &'a mut [MaybeUninit<Vertex>],
    count: usize,
    last: Vertex,
    edge: Option<([f32; 4], [f32; 4])>,
}

impl Strip<'_> {
    #[inline]
    fn push(&mut self, v: Vertex) {
        if let Some(out) = self.output.get_mut(self.count) {
            out.write(v);
        }
        self.count += 1;
    }

    fn quad(&mut self, q: Quad, pixels: f32) {
        // A delayed expansion curtain or a buried hem can have genuinely
        // zero flux at both endpoints. It cannot contribute to screen blend.
        if q.field[2] == 0. && q.field[3] == 0. {
            self.edge = None;
            return;
        }
        let sheet = q.material[0] == CURTAIN || q.material[0] == GHOST;
        let connected = sheet && self.edge == Some((q.left, q.material));
        self.edge = sheet.then_some((q.right, q.material));
        let steps = if sheet { subdivisions(&q, pixels) } else { 1 };
        let inv = 1. / steps as f32;
        for index in usize::from(connected)..=steps {
            let u = index as f32 * inv;
            let (p, tangent) = if index == 0 {
                (q.left, q.tangent_left)
            } else if index == steps {
                (q.right, q.tangent_right)
            } else {
                let v = 1. - u;
                let p = std::array::from_fn(|k| {
                    (1. + 2. * u) * v * v * q.left[k]
                        + (3. - 2. * u) * u * u * q.right[k]
                        + u * v * v * q.tangent_left[k]
                        - u * u * v * q.tangent_right[k]
                });
                let tangent = std::array::from_fn(|k| {
                    6. * u * v * (q.right[k] - q.left[k])
                        + v * (1. - 3. * u) * q.tangent_left[k]
                        + u * (3. * u - 2.) * q.tangent_right[k]
                });
                (p, tangent)
            };
            let mut lo = Vertex {
                position_uv: [p[0], p[1], u, 0.],
                light: [
                    mix(q.field[0], q.field[1], u),
                    mix(q.field[2], q.field[3], u),
                    0.,
                    mix(q.optics[2], q.optics[3], u),
                ],
                surface: q.material,
            };
            let mut hi = lo;
            hi.position_uv = [p[2], p[3], u, 1.];
            if sheet {
                let hem = if q.material[0] == GHOST {
                    0.12 / 0.39
                } else {
                    0.12
                };
                let dx = mix(tangent[0], tangent[2], hem);
                let dy = mix(tangent[1], tangent[3], hem);
                let ray_x = p[2] - p[0];
                let ray_y = p[3] - p[1];
                // Project the ray onto the normal of the actual sloping hem.
                // Distances and extinction are continuous across the shared edge.
                let length = (dx * dx + dy * dy).sqrt().max(0.001);
                let depth = ((dx * ray_y - dy * ray_x) / length).abs().max(1.);
                let knot = mix(q.optics[0], q.optics[1], u);
                let slope = dy.abs() / dx.abs().max(0.001);
                // A sheet's vertical UV is given by the sign of normal
                // distance. Its other coordinate carries the continuous
                // tangent of the actual hem across the ray column.
                lo.position_uv[3] = slope;
                hi.position_uv[3] = slope;
                lo.light[2] = -hem * depth;
                hi.light[2] = (1. - hem) * depth;
                lo.position_uv[2] = knot;
                hi.position_uv[2] = knot;
            }
            if index == 0 && self.count > 0 {
                self.push(self.last);
                self.push(lo);
            }
            self.push(lo);
            self.push(hi);
            if index == steps {
                self.last = hi;
            }
        }
    }
}

pub(super) fn subdivisions(q: &Quad, pixels: f32) -> usize {
    // The quadratic derivative of a cubic lies in the convex hull of these
    // three Bezier derivative controls. Check both ends of the drawn columns.
    let mut span = 0.0f32;
    let mut steep = false;
    let (mut min_dx, mut max_dx) = (f32::INFINITY, f32::NEG_INFINITY);
    for k in [0, 2] {
        for (dx, dy) in [
            (q.tangent_left[k], q.tangent_left[k + 1]),
            (q.tangent_right[k], q.tangent_right[k + 1]),
            (
                3. * (q.right[k] - q.left[k]) - q.tangent_left[k] - q.tangent_right[k],
                3. * (q.right[k + 1] - q.left[k + 1])
                    - q.tangent_left[k + 1]
                    - q.tangent_right[k + 1],
            ),
        ] {
            span = span.max(dx.abs());
            min_dx = min_dx.min(dx);
            max_dx = max_dx.max(dx);
            steep |= dy.abs() > 0.57735 * dx.abs();
        }
    }
    steep |= min_dx < 0. && max_dx > 0.;
    ((span * pixels / if steep { 6. } else { 28. }).ceil() as usize).max(1)
}

/// # Safety
/// The lease must be retained. The output holds capacity vertices. Returning
/// the required length allows the uploader to grow once when a new arc appears.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_aurora_strip(
    lease: *const Lease,
    layer: u32,
    side: i32,
    out: *mut Vertex,
    capacity: usize,
) -> usize {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return 0;
    };
    let Some((a, b)) = endpoints(l, side) else {
        return 0;
    };
    if layer > 1 || (out.is_null() && capacity != 0) {
        return 0;
    }
    let output = if capacity == 0 {
        &mut []
    } else {
        unsafe { std::slice::from_raw_parts_mut(out.cast::<MaybeUninit<Vertex>>(), capacity) }
    };
    let mut strip = Strip {
        output,
        count: 0,
        last: Vertex::default(),
        edge: None,
    };
    let t = if side < 0 { l.sample.alpha as f32 } else { 0. };
    sample(a, b, &l.view, t, layer, |q| {
        strip.quad(q, (l.view.uniform_scale * l.view.dpr) as f32)
    });
    strip.count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjoining_steep_intervals_share_the_edge_and_its_material_coordinate() {
        let ab = Quad {
            left: [0., 20., 2., -100.],
            right: [30., 50., 32., -70.],
            tangent_left: [30.; 4],
            tangent_right: [30.; 4],
            field: [0., 0.2, 1., 1.],
            material: [CURTAIN, 0., 0.7, 0.4],
            ..Quad::default()
        };
        let bc = Quad {
            left: ab.right,
            right: [60., 80., 62., -40.],
            field: [0.2, 0.4, 1., 1.],
            ..ab
        };
        let mut storage = [MaybeUninit::new(Vertex::default()); 64];
        let count = {
            let mut strip = Strip {
                output: &mut storage,
                count: 0,
                last: Vertex::default(),
                edge: None,
            };
            strip.quad(ab, 1.);
            strip.quad(bc, 1.);
            strip.count
        };
        // All elements were initialized above, including any unfilled tail.
        let vertices = storage.map(|v| unsafe { v.assume_init() });
        assert_eq!(count, 22);
        let lower: Vec<_> = vertices[..count].iter().step_by(2).collect();
        assert_eq!(lower.iter().filter(|v| v.position_uv[0] == 30.).count(), 1);
        for pair in lower.windows(2) {
            let dx = pair[1].position_uv[0] - pair[0].position_uv[0];
            assert!(dx > 0. && dx <= 6.001);
            assert!(pair[1].light[0] > pair[0].light[0]);
        }
        for pair in vertices[..count].chunks_exact(2) {
            assert_eq!(pair[0].position_uv[3], pair[1].position_uv[3]);
            assert!((pair[0].position_uv[3] - 1.).abs() < 0.001);
            assert!(pair[0].light[2] < 0. && pair[1].light[2] > 0.);
        }
        let normal_span = (122_f32) / 2_f32.sqrt();
        assert!((lower[5].light[2] + 0.12 * normal_span).abs() < 0.01);
    }
}
