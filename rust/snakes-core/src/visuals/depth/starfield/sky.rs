// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::common::rng::Pcg32;
use super::{
    Frame,
    optics::{Glass, Packet},
    space::*,
    voyage::Pose,
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Sprite {
    pub shape: [f32; 4],
    pub material: [f32; 4],
    pub detail: [f32; 4],
}
impl Sprite {
    fn new(p: [f64; 2], r: [f64; 2], angle: f64, kind: u8, gain: f64, detail: [f32; 4]) -> Self {
        Self {
            shape: [p[0] as f32, p[1] as f32, r[0] as f32, r[1] as f32],
            material: [angle as f32, kind as f32, gain as f32, 0.],
            detail,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Cloud {
    mark: Mark,
    tint: f64,
    offset: V,
    angle: f64,
    texture: f32,
}
pub fn neighbourhood(seed: u64, centre: Cell) -> Vec<Cloud> {
    let mut clouds = Vec::new();
    let mut marks = Vec::with_capacity(343);
    for z in -3..=3 {
        for y in -3..=3 {
            for x in -3..=3 {
                marks.push(landmark(seed, centre.offset(x, y, z)))
            }
        }
    }
    let at = |x: i64, y: i64, z: i64| marks[((z + 3) * 49 + (y + 3) * 7 + x + 3) as usize];
    for z in -2..=2 {
        for y in -2..=2 {
            for x in -2..=2 {
                let mark = at(x, y, z);
                if mark.kind != Kind::Cloud && mark.kind != Kind::Open {
                    continue;
                }
                let mut hot = [(f64::INFINITY, mark.temp); 2];
                for k in -1..=1 {
                    for j in -1..=1 {
                        for i in -1..=1 {
                            let other = at(x + i, y + j, z + k);
                            if other.id == mark.id || other.temp < 0.50 {
                                continue;
                            }
                            let delta = other.p - mark.p;
                            let distance = delta.dot(delta);
                            if distance < hot[0].0 {
                                hot[1] = hot[0];
                                hot[0] = (distance, other.temp)
                            } else if distance < hot[1].0 {
                                hot[1] = (distance, other.temp)
                            }
                        }
                    }
                }
                let mut r = Pcg32::new(mark.id, 57);
                clouds.push(Cloud {
                    offset: V(r.unit() - 0.5, r.unit() - 0.5, r.unit() - 0.5) * 0.85,
                    angle: 0.8 * (r.unit() - 0.5),
                    texture: r.unit() as f32,
                    mark,
                    tint: 0.62 * hot[0].1 + 0.38 * hot[1].1,
                });
            }
        }
    }
    clouds
}
// Two steady gas volumes leave space for both outgoing volumes during a
// handoff. A slot changes owner only at zero exposure. Ranking uses the
// common windshield, so a bezel cannot give a cloud a second lifetime.
const CLOUD_SECONDS: f64 = 1.5;
type Selection = [Option<(Cloud, f64)>; 2];
#[derive(Clone, Copy, Default)]
struct SkyKey {
    volumes: Selection,
    nursery: Option<(u64, V, f64)>,
}
type CloudDistrict = Option<(Cell, std::sync::Arc<[Cloud]>)>;
#[derive(Clone, Default)]
pub struct CloudStream {
    epoch: i64,
    keys: [SkyKey; 3],
    district: CloudDistrict,
}
impl CloudStream {
    fn key(frame: &Frame, k: i64, district: &mut CloudDistrict) -> SkyKey {
        let t = (k.max(0) as f64) * CLOUD_SECONDS;
        let pose = frame.pose(t);
        let front = pose.attitude.inverse().apply(V(0., 0., 1.));
        let mut selection: Selection = [None; 2];
        let cell = Cell::at(pose.p, DISTRICT);
        if district.as_ref().is_none_or(|(old, _)| *old != cell) {
            *district = Some((cell, neighbourhood(frame.art.input.seed, cell).into()));
        }
        for &cloud in district.as_ref().unwrap().1.iter() {
            let delta = cloud.mark.p + cloud.offset - pose.p;
            let d = delta.length().max(0.01);
            let visible = smooth((front.dot(delta) / d + 0.28) / 0.57);
            let brightness = visible * smooth((6. - d) / 3.) / (1. + 0.16 * d);
            if brightness <= 0. {
                continue;
            }
            let mut candidate = Some((cloud, brightness));
            for slot in &mut selection {
                if candidate.map_or(0., |c| c.1) > slot.map_or(0., |c| c.1) {
                    std::mem::swap(slot, &mut candidate);
                }
            }
        }
        let still = frame.art.input.reduced_motion;
        let mark = if still {
            frame.art.vista
        } else {
            frame.leg_at(t).destination
        };
        let mut sum = V::default();
        let mut weights = 0.;
        if mark.kind == Kind::Open {
            for (i, centre) in nursery(mark).into_iter().enumerate() {
                let delta = centre - pose.p;
                let weight = if still {
                    f64::from(i == 0)
                } else {
                    smooth((front.dot(delta) - 0.12) / 0.42)
                        * (-((delta.length() - 1.18) / 1.25).powi(2)).exp()
                };
                sum = sum + (centre - mark.p) * weight;
                weights += weight;
            }
        }
        let nursery =
            (weights > 1e-8).then(|| (mark.id, sum * (1. / weights), smooth(weights / 0.58)));
        SkyKey {
            volumes: selection,
            nursery,
        }
    }
    pub fn update(frame: &mut Frame, tick: u64, reset: bool) {
        let epoch = (tick / 45) as i64;
        if frame.art.params.nebulae == 0 || (!reset && epoch == frame.clouds.epoch) {
            return;
        }
        let mut district = frame.clouds.district.clone();
        let mut key = |epoch| Self::key(frame, epoch, &mut district);
        let keys = if !reset && epoch == frame.clouds.epoch + 1 {
            [frame.clouds.keys[1], frame.clouds.keys[2], key(epoch + 1)]
        } else {
            let now = key(epoch);
            [
                if epoch == 0 { now } else { key(epoch - 1) },
                now,
                key(epoch + 1),
            ]
        };
        frame.clouds = std::sync::Arc::new(Self {
            epoch,
            keys,
            district,
        });
    }
    fn interval(&self, t: f64) -> (SkyKey, SkyKey, f64) {
        let offset = (t / CLOUD_SECONDS - self.epoch as f64).clamp(-1., 1.);
        if offset < 0. {
            (self.keys[0], self.keys[1], smooth(offset + 1.))
        } else {
            (self.keys[1], self.keys[2], smooth(offset))
        }
    }
    // A disappearing nursery retains its last world-space centre until its
    // exposure reaches zero. A newly selected knot enters at the same cadence
    // as the general gas stream, even at the fastest warp velocity.
    fn nursery(&self, mark: Mark, t: f64) -> Option<(V, f64)> {
        let (left, right, phase) = self.interval(t);
        let mut total = 0.;
        let mut centre = V::default();
        for (key, fraction) in [(left, 1. - phase), (right, phase)] {
            if let Some((_, offset, light)) = key.nursery.filter(|n| n.0 == mark.id) {
                let weight = fraction * light;
                total += weight;
                centre = centre + offset * weight;
            }
        }
        (total > 0.).then(|| (mark.p + centre * (1. / total), total))
    }
    fn volumes(&self, t: f64) -> impl Iterator<Item = (Cloud, f64, f64)> {
        let (left, right, phase) = self.interval(t);
        let (left, right) = (left.volumes, right.volumes);
        let old = left.into_iter().flatten().map(move |(cloud, gain)| {
            let next = right
                .iter()
                .flatten()
                .find(|c| c.0.mark.id == cloud.mark.id);
            let share = if next.is_some() { 1. } else { 1. - phase };
            (
                cloud,
                gain * (1. - phase) + next.map_or(0., |c| c.1) * phase,
                share,
            )
        });
        let new = right
            .into_iter()
            .flatten()
            .filter(move |(cloud, _)| {
                !left
                    .iter()
                    .flatten()
                    .any(|old| old.0.mark.id == cloud.mark.id)
            })
            .map(move |(cloud, gain)| (cloud, gain * phase, phase));
        // Dust is partially absorbing. Preserve its paint order when the two
        // keyframes choose the same volumes at different brightness ranks.
        let mut volumes = [None; 4];
        for (slot, volume) in volumes.iter_mut().zip(old.chain(new)) {
            *slot = Some(volume);
        }
        volumes.sort_unstable_by_key(|volume| volume.map(|c| c.0.mark.id));
        volumes
            .into_iter()
            .flatten()
            .filter(|(_, gain, _)| *gain > 0.)
    }
}
pub fn compose(f: &Frame, pose: Pose, before: Pose, g: &Glass, t: f64, out: &mut Packet) {
    let a = &f.art;
    let vanishing = g.project(pose.direction, pose).unwrap_or(g.anchor);
    let amount = a.params.nebulae as f64 * 0.01;
    let leg = f.leg_at(t);
    let lifetime = if a.input.reduced_motion {
        1.
    } else {
        leg.visibility(t)
    };
    let mark = if a.input.reduced_motion {
        a.vista
    } else {
        leg.destination
    };
    let local_area = if g.cylinder {
        a.min_area
    } else {
        g.width * g.height
    };
    if amount > 0. || pose.bow > 0. {
        out.sprite(Sprite::new(
            [g.width / 2., g.height / 2.],
            [g.width / 2., g.height / 2.],
            0.,
            0,
            amount,
            [0.; 4],
        ));
    }
    if amount > 0. {
        let mut nursery_area = 0.;
        if let Some((centre, light)) = f.clouds.nursery(mark, t) {
            let cloud = centre - pose.p;
            if let Some(p) = g.project(cloud, pose) {
                nursery_area = 1.10 * local_area * light * lifetime;
                let radius = (nursery_area / (4. * 0.67)).sqrt();
                let gain =
                    amount * light * lifetime * if a.input.reduced_motion { 1.40 } else { 1. };
                out.sprite(Sprite::new(
                    p,
                    [radius, 0.67 * radius],
                    -0.18,
                    1,
                    gain,
                    [
                        (mark.id % 8191) as f32 / 8191.,
                        mark.temp as f32,
                        mark.temp as f32,
                        cloud.length() as f32,
                    ],
                ));
            }
        }
        // Fade geometric area as well as light. The sum of the two
        // keyframes' shares is bounded during a four-cloud crossfade.
        for (c, weight, share) in f.clouds.volumes(t) {
            let delta = c.mark.p + c.offset - pose.p;
            let Some(p) = g.project(delta, pose) else {
                continue;
            };
            let angle = c.angle;
            let allowance = (1.49 * local_area - nursery_area) * share / 2.;
            let radius =
                (g.f * 1.30 / delta.length().max(0.55)).min((allowance / (4. * 0.64)).sqrt());
            let detail = [
                c.texture,
                c.tint as f32,
                c.mark.temp as f32,
                delta.length() as f32,
            ];
            out.sprite(Sprite::new(
                p,
                [radius, radius * 0.64],
                angle,
                1,
                amount * weight,
                detail,
            ));
        }
    }
    // Each output projects one world-space shutter. Store the head, extent
    // and local angle so the fragment pass can expose both the core and halo.
    let previous = Pose {
        warp: pose.warp,
        ..before
    };
    let capsule = |head: [f64; 2], tail: [f64; 2], radius: f64, gain: f64, detail: [f32; 4]| {
        let mut x = head[0] - tail[0];
        if g.cylinder {
            let half_turn = std::f64::consts::PI * g.f;
            x = (x + half_turn).rem_euclid(2. * half_turn) - half_turn;
        }
        let y = head[1] - tail[1];
        let mut sprite = Sprite::new(head, [radius; 2], y.atan2(x), 4, gain, detail);
        sprite.material[3] = x.hypot(y).min(0.36 * g.h) as f32;
        sprite
    };
    let star = |head, old_world, radius, gain, detail| {
        let tail = g.project(old_world - before.p, previous).unwrap_or(head);
        capsule(head, tail, radius, gain, detail)
    };
    let delta = mark.p - pose.p;
    let distance = delta.length();
    if let Some(p) = g.project(delta, pose) {
        let age = t - leg.arrival_start();
        let approach = if a.input.reduced_motion {
            0.86
        } else {
            smooth(age / 11.)
        };
        let scale = a.input.scale;
        let radius = scale * (5.5 + 12.5 * approach);
        let gain = lifetime * (0.42 + 1.75 * approach) * smooth((distance - 0.004) / 0.014);
        out.sprite(star(
            p,
            mark.p,
            radius,
            gain,
            [
                mark.temp as f32,
                if a.input.reduced_motion { 0. } else { 1. },
                1.,
                approach as f32,
            ],
        ));
    }
    if pose.warp > 0.001 || pose.exposure > 0.01 {
        let sigma = 0.0115 * g.h * a.input.scale;
        let core_sigma = 3.8 * (1. + pose.surge / 6.);
        let radius = 4. * sigma.max(core_sigma);
        let trails = 0.3 + 0.7 * smooth(a.input.trails / 0.35);
        let gain =
            trails * (0.22 * pose.warp + 0.78 * pose.exposure.clamp(0., 1.) + 0.17 * pose.surge);
        out.sprite(Sprite::new(
            vanishing,
            [radius; 2],
            0.,
            2,
            gain,
            [
                (radius / sigma) as f32,
                (radius / core_sigma) as f32,
                0.,
                0.,
            ],
        ));
    }
    if a.input.reduced_motion || !a.params.events {
        return;
    }
    for event in f.moments.iter() {
        let age = t - event.start;
        let duration = event.duration;
        if !(0. ..duration).contains(&age) || t >= event.retire {
            continue;
        }
        let id = event.id;
        let world = event.position(age);
        let p = g.project(world - pose.p, pose);
        let retention = smooth((event.retire - t) / 1.5);
        let fade = smooth(age / 1.5) * smooth((duration - age) / 1.5) * retention;
        let seed = event.seed;
        match id {
            4 => {
                let Some(p) = p else { continue };
                let sun = event.sun;
                let anti = (world - sun.p).unit();
                let q = g
                    .project(world + anti * 0.35 - pose.p, pose)
                    .unwrap_or([p[0] - 60., p[1]]);
                let angle = (q[1] - p[1]).atan2(q[0] - p[0]);
                let len = (0.165 + 0.055 * a.input.trails) * g.h;
                let mid = [
                    p[0] + angle.cos() * len * 0.5,
                    p[1] + angle.sin() * len * 0.5,
                ];
                out.sprite(Sprite::new(
                    mid,
                    [len * 0.5, len * 0.087],
                    angle,
                    3,
                    fade,
                    [seed, sun.temp as f32, 0., 0.],
                ));
                out.sprite(star(
                    p,
                    event.position((age - a.shutter).max(0.)),
                    g.h * 0.011,
                    fade,
                    [0.9, 0., 0., 0.],
                ));
            }
            5 => {
                out.debris = out.sprite_count + 1;
                out.sprite(Sprite::new(
                    vanishing,
                    [g.f, g.f],
                    0.,
                    6,
                    fade,
                    [
                        seed,
                        age as f32,
                        (if g.cylinder {
                            a.input.layout.bounds.width
                        } else {
                            g.h * 16. / 9.
                        } * 0.5) as f32,
                        (g.h * 0.5) as f32,
                    ],
                ));
            }
            6 => {
                // The encounter includes an approach before the one-second
                // visible crossing. The capture samples it two seconds after birth.
                let progress = ((age - 1.45) / 1.1).clamp(0., 1.);
                let theta = seed as f64 * std::f64::consts::TAU;
                let width = if g.cylinder {
                    a.input.layout.bounds.width
                } else {
                    g.h * 16. / 9.
                };
                let edge = (width * 0.5 / theta.cos().abs().max(0.001))
                    .min(g.h * 0.5 / theta.sin().abs().max(0.001));
                let point = |centre: [f64; 2], phase| {
                    let d = 0.015 * g.h + 1.12 * edge * smooth(phase);
                    [centre[0] + theta.cos() * d, centre[1] + theta.sin() * d]
                };
                let width = g.h * (0.18 + 0.11 * progress);
                let gain =
                    1.13 * retention * smooth(progress / 0.12) * smooth((1. - progress) / 0.23);
                let head = point(vanishing, progress);
                let old_vanishing = g.project(before.direction, previous).unwrap_or(g.anchor);
                let old_progress = ((age - a.shutter - 1.45) / 1.1).clamp(0., 1.);
                let tail = point(old_vanishing, old_progress);
                out.sprite(Sprite::new(
                    [
                        head[0] - theta.cos() * width * 0.5,
                        head[1] - theta.sin() * width * 0.5,
                    ],
                    [width * 0.5, 2.8],
                    theta,
                    7,
                    gain,
                    [seed, 0., 0., 0.],
                ));
                out.sprite(capsule(head, tail, 6., gain, [0.88, 0., 0., 0.]));
            }
            7 => {
                let Some(p) = p else { continue };
                let peak = if age < 3. {
                    smooth(age / 3.)
                } else {
                    (-(age - 3.) / 6.).exp()
                };
                if peak > 0.001 {
                    out.sprite(star(
                        p,
                        world,
                        g.h * (0.022 + 0.088 * peak),
                        peak * retention,
                        [0.92, 0., 1., 0.],
                    ));
                }
                let r = g.h * (0.020 + 0.042 * smooth(age / 153.));
                out.sprite(Sprite::new(
                    p,
                    [r, r],
                    0.,
                    5,
                    0.15 * retention * smooth(age / 8.) * smooth((duration - age) / 55.),
                    [seed, age as f32, 0., 0.],
                ));
            }
            _ => {}
        }
    }
}
