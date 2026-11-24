//! Sequential normal and Coulomb impulses. Pair identities survive the sort along
//! X; the cache only warms contacts seen on consecutive substeps.
use super::{arena::*, body::*, common::rng::Pcg32};
const MAX: usize = 61;
const ITERATIONS: usize = 6;
#[derive(Default, Clone, Copy)]
struct Impulses {
    normal: f64,
    tangent: f64,
    stamp: u64,
    floating_time: f64,
}
#[derive(Clone, Copy)]
struct Contact {
    a: usize,
    b: usize,
    n: V,
    inv_sum: f64,
    friction: f64,
    normal: f64,
    tangent: f64,
}
#[derive(Clone, Copy)]
struct Support {
    i: usize,
    n: V,
}
pub(super) struct Solver {
    order: Vec<usize>,
    contacts: Vec<Contact>,
    walls: Vec<Support>,
    cache: Vec<Impulses>,
    stamp: u64,
}
fn pair(balls: &mut [Ball], a: usize, b: usize) -> (&mut Ball, &mut Ball) {
    debug_assert!(a < b);
    let (l, r) = balls.split_at_mut(b);
    (&mut l[a], &mut r[0])
}
fn movable(b: &Ball) -> f64 {
    if b.sleeping { 0. } else { b.inv_mass }
}
fn apply(a: &mut Ball, b: &mut Ball, n: V, normal: f64, tangent: f64) {
    let impulse = n * normal + n.perp() * tangent;
    if !a.sleeping {
        a.v -= impulse * a.inv_mass;
        a.spin -= tangent * a.r * a.inv_inertia;
    }
    if !b.sleeping {
        b.v += impulse * b.inv_mass;
        b.spin -= tangent * b.r * b.inv_inertia;
    }
}
impl Solver {
    pub fn new() -> Self {
        Self {
            order: Vec::with_capacity(MAX),
            contacts: Vec::with_capacity(MAX * 4),
            walls: Vec::with_capacity(MAX * 2),
            cache: vec![Impulses::default(); MAX * MAX],
            stamp: 0,
        }
    }
    /// Wake the touching bed before effective masses are assembled. Without
    /// this, the first contact sees the neighbours below it as immovable.
    fn wake_drop_contacts(balls: &mut [Ball], height: f64) {
        let Some((drop, bed)) = balls.split_last_mut() else {
            return;
        };
        if !drop.heavy || drop.landed || drop.v.y <= 0. {
            return;
        }
        let touching = bed
            .iter()
            .any(|b| b.room == drop.room && (b.p - drop.p).len2() <= (drop.r + b.r + 0.5).powi(2));
        if !touching {
            return;
        }
        drop.landed = true;
        drop.contact = true;
        // Transfer the impact only through the bed's actual touching graph.
        // Nearby airborne bodies are not part of that reaction chain.
        let mut connected = [false; MAX];
        let mut queue = [0usize; MAX];
        let mut length = 0;
        for (i, b) in bed.iter().enumerate() {
            if b.room == drop.room && (b.p - drop.p).len2() <= (b.r + drop.r + 0.5).powi(2) {
                connected[i] = true;
                queue[length] = i;
                length += 1;
            }
        }
        let mut cursor = 0;
        while cursor < length {
            let from = queue[cursor];
            cursor += 1;
            for (i, b) in bed.iter().enumerate() {
                if !connected[i]
                    && b.room == drop.room
                    && (b.p - bed[from].p).len2() <= (b.r + bed[from].r + 0.5).powi(2)
                {
                    connected[i] = true;
                    queue[length] = i;
                    length += 1;
                }
            }
        }
        // Transfer a bounded share of the incident energy into the two sides
        // of the loaded bed. Floor reactions redirect these impulses upward.
        // Neighbours farther along the bed respond to the usual contact solve.
        let incoming = drop.v.y;
        let mut nearest = [(f64::INFINITY, usize::MAX); 4];
        for (i, b) in bed.iter().enumerate().filter(|(_, b)| b.room == drop.room) {
            let d = b.p - drop.p;
            if !connected[i] {
                continue;
            }
            if d.y < 0. || d.y > drop.r + 2.2 * b.r || d.x.abs() > 1.8 * drop.r + b.r {
                continue;
            }
            let start = if d.x < 0. { 0 } else { 2 };
            let score = (d.x.abs() - 0.6 * drop.r).abs() + d.y;
            if score < nearest[start + 1].0 {
                nearest[start + 1] = (score, i);
                if score < nearest[start].0 {
                    nearest.swap(start, start + 1);
                }
            }
        }
        let mut distributed = 0.;
        for (_, i) in nearest.into_iter().filter(|&(_, i)| i != usize::MAX) {
            let b = &mut bed[i];
            b.contact = true;
            b.wake();
            let direction = v(if b.p.x < drop.p.x { -0.82 } else { 0.82 }, -0.57);
            let speed =
                (incoming * (0.045 * b.inv_mass / drop.inv_mass).sqrt()).min(incoming * 0.58);
            let kick = direction * speed;
            distributed += ((b.v + kick).len2() - b.v.len2()).max(0.) / b.inv_mass;
            b.v += kick;
            b.strike(direction, speed, height, true);
        }
        drop.v.y = (incoming * incoming - distributed * drop.inv_mass)
            .max(0.)
            .sqrt();
        for b in bed
            .iter_mut()
            .filter(|b| b.room == drop.room && (b.p - drop.p).len2() < (2.1 * drop.r + b.r).powi(2))
        {
            if b.sleeping {
                b.wake();
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn solve(
        &mut self,
        balls: &mut [Ball],
        rooms: &[Room],
        down: V,
        g: V,
        wall_dt: f64,
        height: f64,
        elasticity: f64,
        pairs: bool,
        hops: bool,
        rng: &mut Pcg32,
    ) {
        let floating = g.len2() < 4.;
        let bounce_speed = if floating { 2. } else { 42. };
        if pairs {
            Self::wake_drop_contacts(balls, height);
        }
        self.contacts.clear();
        self.walls.clear();
        self.stamp += 1;
        for (i, b) in balls.iter_mut().enumerate() {
            if b.alpha == 0. || rooms[b.room].clear_of_walls(b.p, b.r + 0.5) {
                b.floating_wall_time = 0.;
                continue;
            }
            let mut away = V::default();
            for &wall in &rooms[b.room].walls {
                let d = b.p - wall.nearest(b.p);
                if d.len2() > (b.r + 0.5).powi(2) {
                    continue;
                }
                let n = if d.len2() < 1e-12 {
                    wall.inward
                } else {
                    d.unit()
                };
                b.contact = true;
                b.supported |= n.dot(down) < -0.55;
                if floating {
                    away += n;
                }
                if !b.sleeping {
                    self.walls.push(Support { i, n });
                }
            }
            if away.len2() > 0. {
                b.floating_wall_time += wall_dt;
                if b.floating_wall_time >= 1. {
                    // At a corner use both contacts, so the impulse clears
                    // both walls. The material seed keeps the drift repeatable.
                    let seed = super::common::rng::hash64(u64::from(b.glass_pattern) ^ self.stamp);
                    b.wake();
                    b.v += away.unit() * (25. + 15. * (seed >> 11) as f64 / (1u64 << 53) as f64);
                    b.floating_wall_time = 0.;
                }
            } else {
                b.floating_wall_time = 0.;
            }
        }
        if !pairs {
            return;
        }
        self.order.clear();
        self.order
            .extend((0..balls.len()).filter(|&i| balls[i].alpha > 0.));
        self.order.sort_unstable_by(|&a, &b| {
            (balls[a].p.x - balls[a].r)
                .total_cmp(&(balls[b].p.x - balls[b].r))
                .then(a.cmp(&b))
        });
        for k in 0..self.order.len() {
            let i = self.order[k];
            let right = balls[i].p.x + balls[i].r + 0.5;
            for &j in &self.order[k + 1..] {
                if balls[j].p.x - balls[j].r > right {
                    break;
                }
                let (ia, ib) = (i.min(j), i.max(j));
                let (a, b) = pair(balls, ia, ib);
                if a.room != b.room {
                    continue;
                }
                let d = b.p - a.p;
                let reach = a.r + b.r;
                if d.len2() > (reach + 0.5).powi(2) {
                    continue;
                }
                a.near_ball = true;
                b.near_ball = true;
                a.contact = true;
                b.contact = true;
                let dist = d.len();
                let n = if dist > 1e-8 { d / dist } else { v(1., 0.) };
                let nd = n.dot(down);
                let slope = if elasticity < 0.75 && a.sleeping && b.sleeping {
                    0.12
                } else {
                    0.55
                };
                a.supported |= nd > slope;
                b.supported |= nd < -slope;
                let cache = &mut self.cache[ia * MAX + ib];
                let consecutive = cache.stamp + 1 == self.stamp;
                cache.floating_time = if floating {
                    wall_dt + if consecutive { cache.floating_time } else { 0. }
                } else {
                    0.
                };
                if cache.floating_time >= 0.6 {
                    a.wake();
                    b.wake();
                    a.v -= n * between(rng, 25., 40.);
                    b.v += n * between(rng, 25., 40.);
                    let spin = between(rng, -1., 1.);
                    a.spin += spin;
                    b.spin -= spin;
                    cache.floating_time = 0.;
                }
                let incoming = -(b.v - a.v).dot(n);
                if incoming > 20. {
                    for ball in [&mut *a, &mut *b] {
                        if ball.sleeping {
                            if hops && ball.charge > ball.period * 0.5 {
                                ball.hop(g, height - 2. * ball.r, rng);
                            } else {
                                ball.wake();
                            }
                        }
                    }
                }
                let sum = movable(a) + movable(b);
                if sum == 0. {
                    if floating {
                        cache.stamp = self.stamp;
                        cache.normal = 0.;
                        cache.tangent = 0.;
                    }
                    continue;
                }
                cache.stamp = self.stamp;
                let inv_sum = 1. / sum;
                let correction =
                    (reach - dist - 1.5).max(0.).min(0.22 * a.r.min(b.r)) * 0.75 * inv_sum;
                a.p -= n * (correction * movable(a));
                b.p += n * (correction * movable(b));
                let warm =
                    consecutive && incoming < bounce_speed && (!floating || incoming.abs() >= 5.);
                self.contacts.push(Contact {
                    a: ia,
                    b: ib,
                    n,
                    inv_sum,
                    friction: (a.mat.friction() * b.mat.friction()).sqrt(),
                    normal: if warm { cache.normal * 0.75 } else { 0. },
                    tangent: if warm { cache.tangent * 0.75 } else { 0. },
                });
            }
        }
        // A newly moving neighbour can carry the restitution impulse down a
        // touching row during this sweep, before the resting constraint solve.
        for c in &self.contacts {
            let (a, b) = pair(balls, c.a, c.b);
            let speed = -(b.v - a.v).dot(c.n);
            if speed > bounce_speed {
                let restitution = if a.heavy || b.heavy {
                    0.12
                } else {
                    a.mat
                        .restitution(elasticity)
                        .min(b.mat.restitution(elasticity))
                };
                let j = (1. + restitution) * speed * c.inv_sum;
                let slip = (b.v - a.v).dot(c.n.perp()) - a.spin * a.r - b.spin * b.r;
                let friction = c.friction * j;
                let tangent = (-slip * c.inv_sum / 3.).clamp(-friction, friction);
                apply(a, b, c.n, j, tangent);
                a.strike(-c.n, speed, height, true);
                b.strike(c.n, speed, height, true);
            } else {
                apply(a, b, c.n, c.normal, c.tangent);
            }
        }
        for _ in 0..ITERATIONS {
            for w in &self.walls {
                let b = &mut balls[w.i];
                let vn = b.v.dot(w.n);
                if vn < 0. {
                    b.v -= w.n * vn;
                }
            }
            for c in &mut self.contacts {
                let (a, b) = pair(balls, c.a, c.b);
                let rel = b.v - a.v;
                let nn = (c.normal - rel.dot(c.n) * c.inv_sum).max(0.);
                let slip = rel.dot(c.n.perp()) - a.spin * a.r - b.spin * b.r;
                let limit = c.friction * nn;
                let nt = (c.tangent - slip * c.inv_sum / 3.).clamp(-limit, limit);
                apply(a, b, c.n, nn - c.normal, nt - c.tangent);
                c.normal = nn;
                c.tangent = nt;
            }
        }
        for c in &self.contacts {
            // A ball wedged between two shallow contacts can be supported by
            // their combined normal force even when neither normal is steep.
            let lift = c.normal * c.n.dot(down);
            if elasticity < 0.75 && lift > 0. && lift * balls[c.a].inv_mass > 0.02 {
                balls[c.a].supported = true;
            } else if elasticity < 0.75 && lift < 0. && -lift * balls[c.b].inv_mass > 0.02 {
                balls[c.b].supported = true;
            }
            let cache = &mut self.cache[c.a * MAX + c.b];
            cache.normal = c.normal;
            cache.tangent = c.tangent;
        }
        for b in balls.iter_mut().filter(|b| !b.sleeping) {
            rooms[b.room].project(&mut b.p, b.r);
        }
    }
}
