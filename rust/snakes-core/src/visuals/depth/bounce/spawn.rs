use super::{arena::*, body::*, common::rng::Pcg32};
use std::f64::consts::TAU;
pub(super) fn population(rooms: &[Room], count: usize) -> Vec<usize> {
    let mut result = vec![0; rooms.len()];
    for _ in 0..count {
        let i = (0..rooms.len())
            .max_by(|&a, &b| {
                (result[a] == 0).cmp(&(result[b] == 0)).then_with(|| {
                    (rooms[a].area / (1 + result[a]) as f64)
                        .total_cmp(&(rooms[b].area / (1 + result[b]) as f64))
                })
            })
            .unwrap();
        result[i] += 1;
    }
    result
}
pub(super) fn material(setting: &str, rng: &mut Pcg32) -> Material {
    match setting {
        "rubber" => Material::Rubber,
        "glass" => Material::Glass,
        "metal" => Material::Metal,
        _ => match rng.unit() {
            x if x < 0.5 => Material::Rubber,
            x if x < 0.8 => Material::Glass,
            _ => Material::Metal,
        },
    }
}
/// Correct the seeded draw to the nearest 50/30/20 allocation, without
/// disturbing its composition or the RNG stream for locations and pigments.
pub(super) fn balance_materials(balls: &mut [Ball], seed: u64) {
    let n = balls.len();
    if n < 3 {
        return;
    }
    let mut rng = Pcg32::new(seed, 0x6d6174657269616c);
    let weight = [5, 3, 2];
    let mut quota = weight.map(|w| (n * w / 10).max(1));
    let mut ties = [0usize, 1, 2];
    for i in (1..3).rev() {
        ties.swap(i, rng.bounded((i + 1) as u32).unwrap() as usize);
    }
    while quota.iter().sum::<usize>() < n {
        let k = *ties
            .iter()
            .max_by_key(|&&k| n as i32 * weight[k] as i32 - 10 * quota[k] as i32)
            .unwrap();
        quota[k] += 1;
    }
    let mut seen = [0; 3];
    for b in balls.iter() {
        seen[b.mat as usize] += 1;
    }
    let mut order: Vec<_> = (0..n).collect();
    for i in (1..n).rev() {
        order.swap(i, rng.bounded((i + 1) as u32).unwrap() as usize);
    }
    let materials = [Material::Rubber, Material::Glass, Material::Metal];
    for i in order {
        let old = balls[i].mat as usize;
        if seen[old] > quota[old]
            && let Some(&new) = ties.iter().find(|&&k| seen[k] < quota[k])
        {
            seen[old] -= 1;
            seen[new] += 1;
            balls[i].set_material(materials[new]);
        }
    }
}
pub(super) fn radius(median: f64, rng: &mut Pcg32) -> f64 {
    let normal = (-2. * (1. - rng.unit()).ln()).sqrt() * (TAU * rng.unit()).cos();
    median * (0.35 * normal).exp().clamp(0.45, 2.2)
}
fn fits(room: &Room, p: V, r: f64) -> bool {
    room.outputs.iter().any(|o| o.contains(p.into()))
        && room
            .walls
            .iter()
            .all(|wall| (p - wall.nearest(p)).len2() >= (r - 1e-6).max(0.).powi(2))
}
pub(super) fn top_point(room: &Room, r: f64, rng: &mut Pcg32) -> V {
    let mut pick = rng.unit()
        * room
            .walls
            .iter()
            .filter(|w| w.inward.y > 0.)
            .map(|w| (w.b.x - w.a.x - 2. * r).max(0.))
            .sum::<f64>();
    for wall in &room.walls {
        let length = wall.b.x - wall.a.x - 2. * r;
        if wall.inward.y <= 0. || length <= 0. {
            continue;
        }
        if pick < length {
            let p = v(wall.a.x + r + pick, wall.a.y + r + 0.01);
            if fits(room, p, r) {
                return p;
            }
            break;
        }
        pick -= length;
    }
    let o = room
        .outputs
        .iter()
        .max_by(|a, b| a.width.min(a.height).total_cmp(&b.width.min(b.height)))
        .unwrap();
    let mut p = v(o.centre().x, o.y + r + 0.01);
    room.project(&mut p, r);
    p
}
fn clearance(p: V, r: f64, room: usize, others: &[Ball]) -> f64 {
    others
        .iter()
        .filter(|b| b.room == room)
        .map(|b| (b.p - p).len() - b.r - r)
        .fold(f64::INFINITY, f64::min)
}
pub(super) fn initial(ball: &Ball, room: &Room, others: &[Ball], rng: &mut Pcg32) -> V {
    let mut best = top_point(room, ball.r, rng);
    let mut score = f64::NEG_INFINITY;
    for attempt in 0..48 {
        let entry = top_point(room, ball.r, rng);
        let local = room
            .outputs
            .iter()
            .find(|o| o.contains(entry.into()))
            .unwrap_or(&room.outputs[0]);
        let drop = (local.height - 2. * ball.r).max(0.)
            * between(rng, 0., if attempt < 20 { 0.48 } else { 0.94 });
        let p = entry + v(0., drop);
        if !fits(room, p, ball.r) {
            continue;
        }
        let c = clearance(p, ball.r, ball.room, others);
        if c >= 0. {
            return p;
        }
        if c > score {
            best = p;
            score = c;
        }
    }
    best
}
// Settle directly in gravity potential order. Floor tangencies and the intersections
// of two support circles are the stable centres a long dissipative pre-roll seeks.
pub(super) fn still_position(
    ball: &Ball,
    room: &Room,
    others: &[Ball],
    gravity: i32,
    rng: &mut Pcg32,
) -> V {
    let mut best = ball.p;
    let mut score = f64::NEG_INFINITY;
    let dir = gravity.signum() as f64;
    let output = room
        .outputs
        .iter()
        .find(|o| o.contains(ball.p.into()))
        .unwrap_or(&room.outputs[0]);
    let u = (ball.p.x - output.x) / output.width;
    // Two loose groups retain the fall-in's lateral ordering. An object can
    // settle on nearby supports before it would have to roll across the room.
    let reference = output.x
        + output.width
            * (if u < 0.55 {
                0.25 + 0.40 * u
            } else {
                0.60 + 0.30 * u
            });
    let mut consider = |p: V| {
        if !fits(room, p, ball.r) {
            return;
        }
        let c = clearance(p, ball.r, ball.room, others);
        let value = if gravity.abs() < 2 {
            c.min(1e5).min(room.nearest(p).2 * 2.)
        } else {
            if c < -0.02 {
                return;
            }
            dir * p.y - 0.88 * (p.x - reference).abs()
        };
        if value > score {
            best = p;
            score = value;
        }
    };
    if gravity.abs() < 2 {
        for _ in 0..64 {
            let o = room.outputs[rng.bounded(room.outputs.len() as u32).unwrap() as usize];
            consider(v(o.x + rng.unit() * o.width, o.y + rng.unit() * o.height));
        }
        return best;
    }
    let r = ball.r;
    for w in room.walls.iter().filter(|w| w.inward.y * dir < 0.) {
        if w.b.x - w.a.x < 2. * r {
            continue;
        }
        let y = w.a.y + w.inward.y * r;
        for x in [reference.clamp(w.a.x + r, w.b.x - r), w.a.x + r, w.b.x - r] {
            consider(v(x, y));
        }
        for b in others.iter().filter(|b| b.room == ball.room) {
            let dx2 = (r + b.r).powi(2) - (y - b.p.y).powi(2);
            if dx2 >= 0. {
                let dx = dx2.sqrt();
                consider(v(b.p.x - dx, y));
                consider(v(b.p.x + dx, y));
            }
        }
    }
    for (i, a) in others
        .iter()
        .enumerate()
        .filter(|(_, a)| a.room == ball.room)
    {
        for b in others[i + 1..].iter().filter(|b| b.room == ball.room) {
            let d = b.p - a.p;
            let len = d.len();
            let r1 = r + a.r;
            let r2 = r + b.r;
            if len < 1e-8 || len > r1 + r2 || len < (r1 - r2).abs() {
                continue;
            }
            let along = (r1 * r1 - r2 * r2 + len * len) / (2. * len);
            let normal = d / len;
            let mid = a.p + normal * along;
            let off = normal.perp() * (r1 * r1 - along * along).max(0.).sqrt();
            for p in [mid + off, mid - off] {
                if (p.x - a.p.x) * (p.x - b.p.x) <= 0.
                    && dir * (p.y - a.p.y) <= 0.
                    && dir * (p.y - b.p.y) <= 0.
                {
                    consider(p);
                }
            }
        }
        for w in room.walls.iter().filter(|w| w.inward.x != 0.) {
            let x = w.a.x + w.inward.x * r;
            let dy2 = (r + a.r).powi(2) - (x - a.p.x).powi(2);
            if dy2 >= 0. {
                let y = a.p.y - dir * dy2.sqrt();
                if y >= w.a.y && y <= w.b.y && (a.p.x - x) * w.inward.x > 0. {
                    consider(v(x, y));
                }
            }
        }
    }
    best
}
