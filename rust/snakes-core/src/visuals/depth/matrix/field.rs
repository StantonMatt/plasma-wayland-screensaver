//! Column-major ink. Cell timestamps make cooling independent of refresh and
//! leave a permanent zero-energy glyph for the next resonance sweep.
use super::{Params, hash64, unit, weather::*};
use crate::visuals::depth::world::WorldInput;

pub const BASE: [u16; 7] = [1, 17, 49, 145, 209, 225, 321];
pub const COUNT: [u16; 7] = [12, 22, 74, 50, 8, 96, 8];
// A visit's cooling age is an integer number of 30 Hz ticks. Cache the exact
// scalar values for common revisits; old, cold memory keeps the same formula.
fn visit_decay(ticks: u32) -> f32 {
    static DECAY: std::sync::LazyLock<[f32; 8192]> = std::sync::LazyLock::new(|| {
        std::array::from_fn(|ticks| (-(ticks as f32 / 30.) / 100.).exp())
    });
    DECAY
        .get(ticks as usize)
        .copied()
        .unwrap_or_else(|| (-(ticks as f32 / 30.) / 100.).exp())
}
pub fn glyph(d: u8, h: u64) -> u16 {
    BASE[d as usize] + (h % u64::from(COUNT[d as usize])) as u16
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Cell {
    pub stamp: [u8; 4],
    pub glyph: [u8; 2],
    pub style: u8,
    pub power: u8,
}
impl Cell {
    pub fn id(self) -> u16 {
        u16::from_le_bytes(self.glyph) & 511
    }
    fn weight(self) -> u16 {
        (u16::from_le_bytes(self.glyph) >> 9) & 31
    }
    fn age(self, stamp: u32) -> f32 {
        stamp.saturating_sub(u32::from_le_bytes(self.stamp)) as f32 / 30.
    }
}
#[derive(Clone, Copy, Default)]
pub struct Column {
    pub floor: f32,
    output: usize,
    wet: f32,
    pub revision: u32,
    pub lo: u16,
    pub hi: u16,
    next_spawn: u32,
    active_until: u32,
    dry_until: u32,
    sheltered: bool,
    pub intensity: f32,
    surge: f32,
    fronts: [Front; 2],
    blend: f32,
    landing_band: f32,
}
#[derive(Clone, Copy)]
pub struct Stream {
    key: u64,
    pub col: usize,
    pub y: f32,
    mass: f32,
    base_speed: f32,
    age: f32,
    loss: f32,
    fade: f32,
    ease: f32,
    wobble: f32,
    pause: f32,
    phase: f32,
    hop_from: f32,
    hop_age: f32,
    flare: f32,
    dialect: u8,
    tint: u8,
    symbol: u16,
    recent: [u16; 2],
    scramble_hz: f32,
    pub lantern: bool,
    floor_occluded: bool,
    pub previous: [f32; 2],
    pub position: [f32; 2],
}
impl Stream {
    fn pick_glyph(&mut self, far_size: Option<f32>, stamp: u32, row: i32) {
        let bucket = (stamp as f32 * self.scramble_hz / 30.) as u32;
        let h = hash64(self.key ^ ((row as u32 as u64) << 32) ^ u64::from(bucket));
        // Each row omits one residue class. Three successive rows cannot all
        // become the same letter, even when their shader swap clocks differ.
        let excluded = (row + self.col as i32).rem_euclid(3) as u16;
        let repeats = |id: u16| self.recent.contains(&id) || id % 3 == excluded;
        let mut id = glyph(self.dialect, h);
        if self.dialect == 4
            && h % 40 < 2
            && let Some(size) = far_size
        {
            let base = if size <= 16. { 217 } else { 212 };
            let index = (h % 2) as u16;
            id = base + index;
            if repeats(id) {
                id = base + 1 - index;
            }
        }
        while repeats(id) {
            let base = BASE[self.dialect as usize];
            id = base + (id - base + 1) % COUNT[self.dialect as usize];
        }
        self.symbol = id;
    }
}
#[derive(Clone, Copy)]
struct Droplet {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    tint: u8,
    glyph: u16,
    previous: [f32; 2],
    strength: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Head {
    pub path: [f32; 4],
    pub ink: [f32; 4],
    pub light: [f32; 4],
}
// A stream, strike or lantern may revisit the same cell before it can cool.
// Remember this independently of the render payload so pooled frames and GPU
// uploads stay small. Only writes above the OLED threshold touch this state.
#[derive(Clone, Copy, Default)]
struct HotInterval {
    start: u32,
    restart: u32,
}

pub struct Layer {
    pub index: usize,
    pub pitch: [f32; 2],
    pub cols: Vec<Column>,
    pub rows: usize,
    pub cells: Vec<Cell>,
    pub streams: Vec<Stream>,
    drops: Vec<Droplet>,
    pub(super) bolt_segments: Vec<Head>,
    bolt_cells: Vec<usize>,
    pub(super) overlay_limit: usize,
    bolt_start: Option<f32>,
    next_bolt_row: i32,
    heat: Vec<HotInterval>,
    hot_lifetime: [[u16; 256]; 4],
    pub half: f32,
    trail_span: f32,
    masked: Vec<usize>,
    density: f32,
    shelter: f32,
    landing_fade: f32,
    speed: f32,
    seed: u64,
    candidate: Vec<i32>,
    row_heads: Vec<i32>,
    next: Vec<i32>,
    cap: usize,
    near_limit: usize,
    screen_columns: Vec<[f32; 2]>,
    pub merges: u32,
    pub splashes: u32,
    pub hops: u32,
    pub evaporations: u32,
}
impl Layer {
    pub fn new(i: usize, input: &WorldInput, p: &Params) -> Self {
        let mid = (22. * input.scale as f32).round().clamp(11., 64.);
        let size = [(mid * 0.55).max(8.), mid, (mid * 1.9).min(96.)][i];
        let b = input.layout.bounds;
        let mut pitch = [size * [0.98, 1.04, 1.08][i], size * 1.06];
        // Bound pathological sparse layouts and keeps their logical coordinates intact.
        let enlarge = ((b.width * b.height / (pitch[0] * pitch[1]) as f64 / 350_000.)
            .sqrt()
            .max(b.width / (pitch[0] as f64 * 4096.))
            .max(b.height / (pitch[1] as f64 * 2048.))
            .max(1.)) as f32;
        for n in &mut pitch {
            *n *= enlarge;
        }
        let n = (b.width as f32 / pitch[0]).ceil() as usize;
        let rows = (b.height as f32 / pitch[1]).ceil() as usize;
        let seed = hash64(input.seed ^ ((i as u64 + 1) << 52));
        let cols = (0..n)
            .map(|c| {
                let x = b.x + ((c as f64 + 0.5) * pitch[0] as f64).min(b.width - 0.01);
                let floor = input
                    .layout
                    .vertical_intervals(x)
                    .last()
                    .map_or(b.bottom(), |r| r.end);
                Column {
                    output: input
                        .layout
                        .outputs
                        .iter()
                        .position(|o| {
                            x >= o.rect.x && x < o.rect.right() && o.rect.bottom() == floor
                        })
                        .unwrap_or(input.layout.outputs.len()),
                    floor: (floor - b.y) as f32 / pitch[1],
                    landing_band: 1.,
                    next_spawn: 1 + (hash64(seed ^ ((c as u64) << 31)) % 30) as u32,
                    ..Column::default()
                }
            })
            .collect();
        let cap = ((input.layout.visible_area / 3_686_400.).max(0.15)
            * if input.density > 80. { 1500. } else { 600. }) as usize;
        // A large cell has a shorter ceiling-to-floor transit in rows. At the
        // drizzle end, compensate for that so a sparse, flat field still has
        // several drops. Preserve the sheet response above the midpoint.
        let rainfall = input.density as f32 / 50.;
        let arrival = if rainfall < 1. {
            rainfall.powf(0.70) * (1. + (1. - rainfall) * (input.scale as f32 - 1.).max(0.))
        } else {
            rainfall.powf(1.55)
        };
        let half = 0.2 + 3. * (input.trails as f32).powf(1.4);
        let speed = (input.speed as f32).min(1. + 0.3 * (input.speed as f32 - 1.));
        let shortest = input
            .layout
            .outputs
            .iter()
            .map(|o| o.rect.height as f32)
            .fold(b.height as f32, f32::min);
        let half = half.min(0.30 * shortest / pitch[1] / (20. * speed * [0.55, 1., 1.6][i]));
        let hot_lifetime = [half, half * 3., 1., 1.25].map(|half| {
            std::array::from_fn(|power| {
                ((power as f32 / (255. * 0.4)).log2() * half * 30.)
                    .ceil()
                    .max(0.) as u16
            })
        });
        Self {
            index: i,
            pitch,
            cols,
            rows,
            cells: vec![Cell::default(); n * rows],
            heat: vec![HotInterval::default(); n * rows],
            hot_lifetime,
            half,
            trail_span: 0.30 * shortest / pitch[1],
            masked: Vec::new(),
            streams: Vec::new(),
            drops: Vec::new(),
            bolt_segments: Vec::new(),
            bolt_cells: Vec::with_capacity(rows + 13),
            overlay_limit: usize::MAX,
            bolt_start: None,
            next_bolt_row: 0,
            landing_fade: (input.scale as f32 - 1.).clamp(0., 1.)
                * ((input.density as f32 - 50.) / 30.).clamp(0., 1.),
            shelter: ((input.scale as f32 - 1.).clamp(0., 1.)
                * ((input.density as f32 - 50.) / 30.).clamp(0., 1.))
                * if i == 1 { 1. } else { 0. },
            density: arrival * [0.40 * p.depth, 0.40, 0.55 * p.depth][i],
            speed,
            seed,
            candidate: vec![-1; n],
            row_heads: vec![-1; rows * 2 + 2],
            next: Vec::new(),
            screen_columns: input
                .layout
                .outputs
                .iter()
                .map(|o| {
                    [
                        (o.rect.x - b.x) as f32 / pitch[0],
                        (o.rect.right() - b.x) as f32 / pitch[0],
                    ]
                })
                .collect(),
            near_limit: (12. * p.depth / 0.6).round() as usize,
            cap: if i == 2 {
                (12. * input.layout.visible_area / 3_686_400. * f64::from(p.depth / 0.6)).ceil()
                    as usize
            } else {
                (cap * [49, 44, 7][i] / 100).max(2)
            },
            merges: 0,
            splashes: 0,
            hops: 0,
            evaporations: 0,
        }
    }
    fn write(&mut self, col: usize, row: i32, stamp: u32, id: u16, style: u8, power: f32) {
        if row < 0 || row as usize >= self.rows {
            return;
        }
        let c = &mut self.cols[col];
        if (c.sheltered || (c.dry_until > stamp && c.surge <= 0.35)) && style & (8 | 16 | 64) == 0 {
            return;
        }
        let r = row as usize;
        let index = col * self.rows + r;
        let old = self.cells[index];
        if old.style & 16 != 0 && style & 16 == 0 && old.age(stamp) < 2.5 {
            return;
        }
        let power = if style & (8 | 16 | 64) == 0 {
            power * Self::landing_light(self.landing_fade, c, row as f32 + 0.5)
        } else {
            power
        };
        let power = (power.clamp(0., 1.) * 255.) as u8;
        if power > 102 {
            let kind = if style & 64 != 0 {
                3
            } else if style & 16 != 0 {
                2
            } else {
                usize::from(style & 8 != 0)
            };
            // Downpour ink cools faster than normal ink; normal lifetime is
            // a conservative upper bound for that case.
            let cool = stamp
                + u32::from(self.hot_lifetime[kind][power as usize])
                + if kind == 2 { 5 } else { 0 };
            let heat = &mut self.heat[index];
            if stamp >= heat.restart {
                heat.start = stamp;
            }
            // Preserve the last real write and let it cool naturally. Wait
            // a full second below threshold before starting the next interval.
            if cool - heat.start > 58 * 30 {
                return;
            }
            heat.restart = cool + 31;
        }
        let previous = self.cells[index];
        // The glyph lane holds a nine-bit address, five bits of visit history and two bits of cooling rate.
        let weight = if previous.power == 0 {
            0.
        } else {
            previous.weight() as f32
                * visit_decay(stamp.saturating_sub(u32::from_le_bytes(previous.stamp)))
        };
        self.cells[index] = Cell {
            stamp: stamp.to_le_bytes(),
            glyph: (id | ((weight + 10.85).round().min(31.) as u16) << 9).to_le_bytes(),
            style,
            power,
        };
        Self::dirty(c, stamp, r);
    }
    // Only the final cell dims at the physical floor. The landing-strip
    // occupancy masks, rather than a dark lower quarter, separate dense ink.
    fn landing_light(amount: f32, col: &Column, y: f32) -> f32 {
        if amount == 0. || y < col.floor - col.landing_band {
            return 1.;
        }
        let t = ((col.floor - y) / col.landing_band).clamp(0., 1.);
        1. - amount * 0.95 * (1. - t * t * (3. - 2. * t))
    }
    fn dirty(c: &mut Column, stamp: u32, r: usize) {
        if c.revision == stamp {
            c.lo = c.lo.min(r as u16);
            c.hi = c.hi.max(r as u16);
        } else {
            c.revision = stamp;
            c.lo = r as u16;
            c.hi = r as u16;
        }
    }
    pub(super) fn overlay_count(&self) -> usize {
        self.streams.len() + self.drops.len()
    }
    pub(super) fn mask_heads(&mut self, stamp: u32) {
        for index in self.masked.drain(..) {
            self.cells[index].style &= 127;
            Self::dirty(&mut self.cols[index / self.rows], stamp, index % self.rows);
        }
        // Cover the swept footprint at both endpoints of this tick, including eased sideways hops.
        for s in self.streams.iter().take(self.overlay_limit) {
            let [x, y] = s.position;
            let [px, py] = s.previous;
            let left = (x.min(px) - 0.48).floor() as usize;
            let right = ((x.max(px) + 0.48).floor() as usize).min(self.cols.len() - 1);
            let top = (y.min(py) - 1.).floor() as usize;
            let end = ((y.max(py) + 1.).ceil() as usize).min(self.rows);
            for col in left..=right {
                for row in top..end {
                    let i = col * self.rows + row;
                    if self.cells[i].power > 0 && self.cells[i].style & (16 | 128) == 0 {
                        self.cells[i].style |= 128;
                        self.masked.push(i);
                        Self::dirty(&mut self.cols[col], stamp, row);
                    }
                }
            }
        }
    }
    pub fn speed(&self) -> f32 {
        self.speed
    }
    fn near_full(&self, column: usize) -> bool {
        if self.index != 2 {
            return false;
        }
        let Some(&[left, right]) = self.screen_columns.get(self.cols[column].output) else {
            return false;
        };
        // An eased hop straddles both screens. Keep its old place reserved
        // until its entire interpolation interval is off the original screen.
        self.streams
            .iter()
            .filter(|s| {
                let target = s.col as f32 + 0.5;
                target.max(s.previous[0]).max(s.position[0]) >= left
                    && target.min(s.previous[0]).min(s.position[0]) < right
            })
            .count()
            >= self.near_limit
    }
    pub fn spawn(&mut self, col: usize, key: u64, lantern: bool, heavy: bool, p: &Params) {
        if self.streams.len() >= self.cap || self.near_full(col) {
            return;
        }
        let c = &mut self.cols[col];
        let a = hash64(key);
        let b = hash64(a);
        let f = unit(a);
        let g = unit(b);
        let light = !heavy && key % 10 < 3;
        let mass = if heavy {
            2.45
        } else if light {
            0.4 + 0.055 * f
        } else {
            (1.05 * (1.35 * (f + g - 1.)).exp()).clamp(0.4, 2.5)
        };
        let x = col as f32 + 0.5;
        let front = c.fronts[usize::from(unit(hash64(b ^ 27)) < c.blend)];
        let dialect = if p.glyphs == 0 {
            front.dialect
        } else {
            p.glyphs - 1
        };
        self.streams.push(Stream {
            key,
            col,
            y: -0.6,
            mass,
            base_speed: 7. + 11. * unit(hash64(b)),
            age: 0.,
            fade: 1.,
            loss: if light { 0.046 + 0.014 * g } else { 0. },
            ease: 0.4 + 0.6 * f,
            wobble: 1.6 + 1.5 * g,
            pause: if key.is_multiple_of(40) {
                1. + 2.7 * f
            } else {
                -9.
            },
            phase: std::f32::consts::TAU * f,
            hop_from: x,
            hop_age: 1.,
            flare: 0.,
            dialect,
            tint: (front.tint + if key.is_multiple_of(8) { 1 } else { 0 }) % 6,
            symbol: glyph(dialect, a),
            recent: [0; 2],
            scramble_hz: 8. + 4. * unit(key.rotate_left(21)),
            lantern,
            floor_occluded: false,
            previous: [x, -0.6],
            position: [x, -0.6],
        });
        c.wet = (c.wet + 0.25).min(1.);
    }
    pub fn weather(&mut self, weather: &Weather, tick: u32) {
        for (i, c) in self.cols.iter_mut().enumerate() {
            let (rain, fronts, blend, surge) =
                weather.local((i as f32 + 0.5) * self.pitch[0], self.pitch[0]);
            // Large glyphs can fill a desk in under two seconds. Leave variable-width
            // openings between broad channels as their front slowly travels.
            let group = i / 11;
            let phase = hash64(self.seed ^ group as u64);
            let shift = (tick as u64 + phase % 1260) / 1260;
            c.sheltered =
                (i as u64 + (phase % 11) + shift) % 11 < (5. * self.shelter).round() as u64;
            c.intensity = rain;
            c.surge = surge;
            c.fronts = fronts;
            c.blend = blend;
            c.wet *= 0.9608;
            if c.active_until == 0 || tick >= c.active_until {
                let h = hash64(self.seed ^ i as u64 ^ (tick as u64) << 28);
                // Let every channel dry completely. Different refractory lengths
                // and phases prevent a synchronized shutter across the field.
                let dry = if tick <= 30 {
                    (h % 300) as u32
                } else {
                    390 + (h % 140) as u32
                };
                c.dry_until = tick + dry;
                c.active_until = c.dry_until
                    + if tick <= 30 {
                        240 + (h % 660) as u32
                    } else {
                        720 + (h % 240) as u32
                    };
            }
        }
    }
    #[cfg(test)]
    pub fn step(&mut self, stamp: u32, dt: f32, weather: &Weather, p: &Params) {
        self.step_ink(stamp, dt, weather, p);
        self.mask_heads(stamp);
    }
    pub(super) fn step_ink(&mut self, stamp: u32, dt: f32, weather: &Weather, p: &Params) {
        if self.density == 0. {
            return;
        }
        // Independent renewal clocks prevent rows of adjacent streams from
        // arriving on a one-second beat. Changing the width neither advances
        // another column's clock nor consumes one of its random rolls.
        for i in 0..self.cols.len() {
            let c = self.cols[i];
            if c.next_spawn > stamp {
                continue;
            }
            let key = hash64(self.seed ^ ((i as u64) << 31) ^ u64::from(c.next_spawn));
            self.cols[i].next_spawn = c.next_spawn + 12 + (hash64(key) % 37) as u32;
            if c.sheltered || (c.dry_until > stamp && c.surge <= 0.35) {
                continue;
            }
            let side = self.cols[i.saturating_sub(1)].wet
                + self.cols[(i + 1).min(self.cols.len() - 1)].wet;
            let rate = 0.17
                * self.speed.max(0.3)
                * self.density
                * (c.intensity + if self.index == 0 { 4. * c.surge } else { 0. })
                * (1. + 2.5 * c.wet)
                * (1. + 0.4 * side);
            if unit(key) < 1. - (-rate).exp() {
                self.spawn(i, key, false, false, p);
            }
        }
        let far_size = (self.index == 0).then_some(self.pitch[1] / 1.06);
        let mut kept = 0;
        for index in 0..self.streams.len() {
            let mut s = self.streams[index];
            let old = s.y;
            s.previous = s.position;
            s.age += dt;
            s.hop_age += dt;
            s.flare = (s.flare - dt).max(0.);
            let cruise = if s.lantern {
                4.8
            } else {
                s.base_speed
                    * (0.7 + 0.45 * s.mass.sqrt())
                    * [0.55, 1., 1.6][self.index]
                    * (1. + 0.34 * self.cols[s.col].surge)
            };
            let vel = cruise
                * self.speed
                * (s.age / s.ease).min(1.)
                * (1. + 0.06 * (s.phase + s.age * s.wobble).sin());
            if s.age < s.pause || s.age > s.pause + 0.19 {
                s.y += vel * dt;
            }
            s.mass *= 1. - s.loss * dt;
            if s.mass < 0.3 {
                s.fade = (s.fade - dt / 0.4).max(0.);
            }
            let departed_col = s.col;
            if s.y.floor() > old.floor() {
                let h = hash64(s.key ^ s.y.floor().to_bits() as u64);
                let wind = weather.drift(s.col as f32 * self.pitch[0], p);
                let gust = weather.moments[GUST].envelope(
                    weather.time()
                        - (s.position[0] * self.pitch[0] - weather.moments[GUST].x).abs() / 770.,
                    5.,
                );
                let probability = (0.06 * gust * p.wind
                    + wind.abs() * (0.039 + 0.08 * gust) / s.mass.max(0.3))
                .min(0.45);
                if p.wind > 0.
                    && !s.lantern
                    && s.hop_age >= 0.08
                    && unit(h) < probability * if self.index == 2 { 1.5 } else { 1. }
                {
                    let to = (s.col as i32 + wind.signum() as i32)
                        .clamp(0, self.cols.len() as i32 - 1) as usize;
                    if to != s.col
                        && !self.cols[to].sheltered
                        && (self.cols[to].output == self.cols[s.col].output || !self.near_full(to))
                    {
                        s.hop_from = s.position[0];
                        s.col = to;
                        s.hop_age = 0.;
                        self.hops += 1;
                    }
                }
                let end =
                    (s.y.floor() as i32 - 1).min(self.cols[departed_col].floor.ceil() as i32 - 1);
                // A faster stream consumes more rows in the same time. Round its
                // decay time down so the two-half-life trail fits the visible height.
                let travel = self.half * vel;
                let cooler: u16 = if travel <= self.trail_span {
                    0
                } else if 0.75 * travel <= self.trail_span {
                    1
                } else if 0.5 * travel <= self.trail_span {
                    2
                } else {
                    3
                };
                for r in (old.floor() as i32).max(0)..=end {
                    let id = s.symbol;
                    s.recent = [id, s.recent[0]];
                    s.pick_glyph(far_size, stamp, r + 1);
                    let power = (0.51 + 0.34 * s.mass.min(1.6)) * (0.91 + 0.09 * unit(h));
                    self.write(
                        departed_col,
                        r,
                        stamp,
                        id | (cooler << 14),
                        s.tint
                            | if s.lantern {
                                8
                            } else if self.cols[s.col].surge > 0.4 {
                                32
                            } else {
                                0
                            },
                        power,
                    );
                    if s.lantern {
                        for off in [-2, -1, 1, 2] {
                            let k = departed_col as i32 + off;
                            if k >= 0 && (k as usize) < self.cols.len() {
                                let prev = self.cells[k as usize * self.rows + r as usize];
                                if prev.power != 0 {
                                    self.write(
                                        k as usize,
                                        r,
                                        stamp,
                                        prev.id(),
                                        s.tint | 8,
                                        (prev.power as f32 / 255.
                                            * (-prev.age(stamp) / self.half).exp2()
                                            + if off.abs() == 1 { 0.30 } else { 0.18 })
                                        .min(1.),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            s.pick_glyph(far_size, stamp, s.y.floor() as i32);
            let f = (s.hop_age / 0.08).min(1.);
            s.position = [
                s.hop_from + (s.col as f32 + 0.5 - s.hop_from) * f * f * (3. - 2. * f),
                s.y,
            ];
            if s.fade == 0. || s.y >= self.cols[s.col].floor {
                if s.y >= self.cols[s.col].floor {
                    self.splash(s.col, s.key, s.mass, s.tint, s.dialect);
                } else {
                    self.evaporations += 1;
                }
            } else {
                self.streams[kept] = s;
                kept += 1;
            }
        }
        self.streams.truncate(kept);
        self.collisions(stamp);
        self.drops.retain_mut(|d| {
            d.previous = [d.x, d.y];
            d.life -= dt;
            d.x += d.vx * dt;
            d.y += (d.vy + 9. * dt) * dt;
            d.vy += 18. * dt;
            d.life > 0.
                && d.x >= 0.
                && d.x < self.cols.len() as f32
                && d.y < self.cols[d.x as usize].floor
        });
    }
    fn collisions(&mut self, stamp: u32) {
        // Half-cell buckets order all heads from the floor up in linear time.
        // Heads in the same bucket necessarily overlap. Keep the leading head
        // for each column while traversing; distinct layers never meet.
        self.row_heads.fill(-1);
        self.candidate.fill(-1);
        self.next.resize(self.streams.len(), -1);
        for (i, s) in self.streams.iter().enumerate() {
            let bin = ((s.y * 2. + 2.) as usize).min(self.row_heads.len() - 1);
            self.next[i] = self.row_heads[bin];
            self.row_heads[bin] = i as i32;
        }
        for bin in (0..self.row_heads.len()).rev() {
            let mut link = self.row_heads[bin];
            while link >= 0 {
                let j = link as usize;
                link = self.next[j];
                let other = self.streams[j];
                let col = other.col;
                let first = self.candidate[col];
                if first >= 0 {
                    let s = &mut self.streams[first as usize];
                    if s.y - other.y < 0.59
                        || (s.previous[1] < other.previous[1] && s.previous[0] == other.previous[0])
                    {
                        s.mass = (s.mass + other.mass).min(5.);
                        s.flare = 0.3;
                        s.lantern |= other.lantern;
                        s.loss = 0.;
                        s.fade = 1.;
                        let r = s.y as i32;
                        let tint = s.tint;
                        let id = glyph(s.dialect, s.key);
                        self.streams[j].mass = 0.;
                        let side = (col as i32 + if stamp & 1 == 0 { 1 } else { -1 })
                            .clamp(0, self.cols.len() as i32 - 1);
                        self.write(side as usize, r, stamp, id, tint, 0.58);
                        self.merges += 1;
                        continue;
                    }
                }
                self.candidate[col] = j as i32;
            }
        }
        self.streams.retain(|s| s.mass > 0.);
    }
    pub fn splash(&mut self, col: usize, key: u64, mass: f32, tint: u8, d: u8) {
        let floor = self.cols[col].floor;
        // The first bead rises from the landing. Additional spray needs open
        // lanes on either side, including on the other depth planes.
        for j in 0..(2. + mass.min(3.)) as u64 {
            if self.drops.len() >= self.cap {
                break;
            }
            let h = hash64(key ^ (j << 18));
            let sign = if (j ^ key) & 1 == 0 { -1. } else { 1. };
            let x = col as f32 + 0.5 + sign * j.div_ceil(2) as f32 * 2.6;
            if x < 0.
                || x >= self.cols.len() as f32
                || (self.cols[x as usize].floor - floor).abs() > 0.5
            {
                continue;
            }
            let launch = floor - 0.65;
            let reach = ((1. + 3. * unit(h)).min(110. / self.pitch[1]) - 0.65).max(0.05);
            let v = (36. * reach).sqrt();
            self.drops.push(Droplet {
                x,
                y: launch,
                vx: sign * (0.08 + unit(hash64(h)) * 0.42),
                vy: -v,
                life: 2. * v / 18.,
                glyph: glyph(d, h),
                previous: [x, launch],
                tint,
                strength: 0.45,
            });
        }
        self.splashes += 1;
    }
    pub fn heads(&self, out: &mut Vec<Head>) {
        let mut emit =
            |a: [f32; 2], b: [f32; 2], id: u16, tint: u8, energy: f32, kind: f32, flare: f32| {
                let scale = if kind == 1. { 0.64 } else { 1. };
                out.push(Head {
                    path: [
                        a[0] * self.pitch[0],
                        a[1] * self.pitch[1],
                        b[0] * self.pitch[0],
                        b[1] * self.pitch[1],
                    ],
                    ink: [
                        scale * self.pitch[0],
                        scale * self.pitch[1],
                        id as f32,
                        tint as f32,
                    ],
                    light: [energy, kind, self.index as f32, flare],
                });
            };
        for s in self.streams.iter().take(self.overlay_limit) {
            if s.floor_occluded {
                continue;
            }
            let id = s.symbol;
            emit(
                s.previous,
                s.position,
                id,
                s.tint,
                s.fade
                    * (0.70 + 0.20 * s.mass.min(2.))
                    * Self::landing_light(self.landing_fade, &self.cols[s.col], s.y).max(0.70),
                if s.lantern { 2. } else { 0. },
                s.flare / 0.3,
            );
        }
        for d in self
            .drops
            .iter()
            .take(self.overlay_limit.saturating_sub(self.streams.len()))
        {
            emit(
                d.previous,
                [d.x, d.y],
                d.glyph,
                d.tint,
                d.strength * (d.life * 5.).min(1.),
                1.,
                0.,
            );
        }
    }
    fn bolt_segment(&mut self, row: i32, start: f32, end: f32, power: f32, stamp: u32) {
        let last = self.cols.len() as f32 - 0.5;
        let (a, b) = (start.clamp(0.5, last), end.clamp(0.5, last));
        let col = (0.5 * (a + b)).floor() as usize;
        let y = row as f32;
        let bottom = (y + 1.).min(self.cols[col].floor);
        if bottom <= y {
            return;
        }
        let id = if b < a {
            210
        } else if b > a {
            211
        } else {
            209
        };
        self.write(col, row, stamp, id, 16, power);
        let path = [
            a * self.pitch[0],
            y * self.pitch[1],
            (a + (b - a) * (bottom - y)) * self.pitch[0],
            bottom * self.pitch[1],
        ];
        let width = 1.6 * self.pitch[1] / 1.06;
        self.bolt_cells.push(col * self.rows + row as usize);
        self.bolt_segments.push(Head {
            path,
            ink: [width, (path[2] - path[0]).hypot(path[3] - path[1]), 0., 0.],
            light: [power, 3., stamp as f32, 0.],
        });
    }
    pub(super) fn mask_bolts(&mut self, stamp: u32) {
        for (&index, segment) in self
            .bolt_cells
            .iter()
            .zip(&self.bolt_segments)
            .take(super::OVERLAY_CAP)
        {
            let cell = &mut self.cells[index];
            if cell.style & 16 != 0 && u32::from_le_bytes(cell.stamp) == segment.light[2] as u32 {
                cell.style |= 128;
                self.masked.push(index);
                Self::dirty(&mut self.cols[index / self.rows], stamp, index % self.rows);
            }
        }
    }
    pub fn bolt(&mut self, m: Moment, age: f32, dt: f32, stamp: u32) {
        if age >= 6.5 {
            self.bolt_segments.clear();
            self.bolt_cells.clear();
        }
        if !(0. ..0.14 + dt).contains(&age) {
            return;
        }
        let centre = (m.x / self.pitch[0])
            .floor()
            .clamp(0., (self.cols.len() - 1) as f32) as i32;
        let nominal_rows = self.cols[centre as usize].floor.ceil() as i32;
        // A strike near a step in the desk's skyline can lean across it. The
        // first ground intersection determines the splash, even on a raised floor.
        let step = (1..=12).find_map(|distance| {
            [-distance, distance].into_iter().find(|off| {
                let k = centre + off;
                k >= 0
                    && k < self.cols.len() as i32
                    && (self.cols[k as usize].floor - self.cols[centre as usize].floor).abs() > 1.
            })
        });
        let lean = step.map_or_else(
            || if m.key & 1 == 0 { 4.5 } else { -4.5 },
            |off| off as f32 + 2. * (off as f32).signum(),
        );
        let bend = |r: i32| {
            let k = r.div_euclid(6);
            let f = r.rem_euclid(6) as f32 / 6.;
            let a = unit(hash64(m.key ^ k as u64));
            let b = unit(hash64(m.key ^ (k + 1) as u64));
            let along = (r as f32 / nominal_rows.max(1) as f32 * 1.42).clamp(0., 1.);
            centre
                + ((a + (b - a) * f - 0.5) * 3.0 + (r as f32 * 0.11).sin() * 1.7 + lean * along)
                    .round() as i32
        };
        let rows = (0..=nominal_rows)
            .find(|&r| {
                let c = bend(r).clamp(0, self.cols.len() as i32 - 1) as usize;
                r as f32 >= self.cols[c].floor - 0.5
            })
            .unwrap_or(nominal_rows);
        if self.bolt_start != Some(m.start) {
            self.bolt_start = Some(m.start);
            self.bolt_segments.clear();
            self.bolt_cells.clear();
            self.next_bolt_row = 0;
        }
        let joint = |r: i32| 0.5 + 0.5 * (bend(r - 1) + bend(r)) as f32;
        let to = (((age / 0.12).clamp(0., 1.) * rows as f32) as i32).min(rows);
        for r in self.next_bolt_row..=to {
            let c = bend(r).clamp(0, self.cols.len() as i32 - 1);
            self.bolt_segment(r, joint(r), joint(r + 1), 1., stamp);
            for (branch, fork, sign) in [(1, rows / 3, 1.), (2, rows / 2, -1.)] {
                let length = 3 + (hash64(m.key ^ branch) % 4) as i32;
                let n = r - fork;
                if m.key % 3 < branch || !(0..length).contains(&n) {
                    continue;
                }
                // One resting row in four gives a short, angled fork with a shared root.
                let side = |k: i32| joint(fork) + sign * (k - k / 4) as f32;
                self.bolt_segment(
                    r,
                    side(n),
                    side(n + 1),
                    1. - 0.5 * n as f32 / (length - 1) as f32,
                    stamp,
                );
            }
            for side in -6i32..=6 {
                let k = c + side;
                if side == 0 || k < 0 || k >= self.cols.len() as i32 || r >= self.rows as i32 {
                    continue;
                }
                let cell = self.cells[k as usize * self.rows + r as usize];
                if cell.power > 0 && !(cell.style & (16 | 64) != 0 && cell.age(stamp) < 3.) {
                    let energy = cell.power as f32 / 255. * (-cell.age(stamp) / self.half).exp2();
                    self.write(
                        k as usize,
                        r,
                        stamp,
                        cell.id(),
                        (cell.style & 7) | 64,
                        energy + 0.35 * (1. - side.abs() as f32 / 7.),
                    );
                }
            }
        }
        self.next_bolt_row = to + 1;
        if age >= 0.12 && age - dt < 0.12 {
            self.splash(
                bend(rows).clamp(0, self.cols.len() as i32 - 1) as usize,
                m.key,
                3.,
                0,
                4,
            );
        }
    }
}

/// Each bit covers a quarter mid column on the real desk. All depth planes
/// share this landing strip, so a far splash cannot print on a nearer one.
#[derive(Default)]
pub struct LandingStrip {
    occupied: Vec<bool>,
    footprints: Vec<[f32; 4]>,
    ink: Vec<Vec<[f32; 4]>>,
}
impl LandingStrip {
    pub fn settle(&mut self, layers: &mut [Layer; 3], stamp: u32) {
        self.footprints.clear();
        self.space_spray(layers, stamp);
        let band = 300. * layers[1].landing_fade;
        if band > 0. {
            self.separate_floor_ink(layers, stamp, band);
        }
    }
    fn space_spray(&mut self, layers: &mut [Layer; 3], stamp: u32) {
        if layers.iter().all(|l| l.drops.is_empty()) {
            return;
        }
        let mid = layers[1].pitch[0];
        let bin = mid * 0.25;
        self.occupied.resize(layers[1].cols.len() * 4, false);
        self.occupied.fill(false);
        let buckets = self.occupied.len();
        let range = |left: f32, right: f32| {
            ((left / bin).floor() as usize).min(buckets)
                ..((right / bin).ceil() as usize).min(buckets)
        };
        for l in layers.iter() {
            for s in l.streams.iter().take(l.overlay_limit) {
                let floor = l.cols[s.col].floor;
                if (floor - s.y) * l.pitch[1] > 110. + 1.7 * l.pitch[1] {
                    continue;
                }
                let left = (s.previous[0].min(s.position[0]) - 0.5) * l.pitch[0];
                let right = (s.previous[0].max(s.position[0]) + 0.5) * l.pitch[0];
                self.occupied[range(left, right)].fill(true);
            }
        }
        for i in [1, 2, 0] {
            let layer = &mut layers[i];
            let [pw, ph] = layer.pitch;
            let drop_limit = layer.overlay_limit.saturating_sub(layer.streams.len());
            let mut kept = 0;
            layer.drops.retain(|d| {
                let left = d.x.min(d.previous[0]) * pw;
                let right = d.x.max(d.previous[0]) * pw;
                let reservation = range(left - pw.max(mid), right + pw.max(mid));
                if self.occupied[reservation.clone()].contains(&true) {
                    return false;
                }
                self.occupied[reservation].fill(true);
                if kept < drop_limit {
                    self.footprints.push([
                        left - 0.32 * pw,
                        (d.y.min(d.previous[1]) - 0.32) * ph,
                        right + 0.32 * pw,
                        (d.y.max(d.previous[1]) + 0.32) * ph,
                    ]);
                }
                kept += 1;
                true
            });
        }
        for l in layers {
            if l.density == 0. {
                continue;
            }
            for &[left, top, right, bottom] in &self.footprints {
                let first = (left / l.pitch[0]).floor() as usize;
                let last = ((right / l.pitch[0]).ceil() as usize).min(l.cols.len());
                let lo = (top / l.pitch[1]).floor() as usize;
                let hi = ((bottom / l.pitch[1]).ceil() as usize).min(l.rows);
                for c in first..last {
                    for row in lo..hi {
                        let index = c * l.rows + row;
                        let cell = &mut l.cells[index];
                        if cell.power > 0 && cell.style & (16 | 128) == 0 {
                            cell.style |= 128;
                            l.masked.push(index);
                            Layer::dirty(&mut l.cols[c], stamp, row);
                        }
                    }
                }
            }
        }
    }
    // A large, dense sheet can put two planes of writing in the same footprint
    // at its landing. Give the sharp mid plane first choice of the space. The
    // field samples once per cell, so the same decision holds between ticks.
    fn separate_floor_ink(&mut self, layers: &mut [Layer; 3], stamp: u32, band: f32) {
        let pitch = layers[1].pitch[0];
        self.ink.resize_with(layers[1].cols.len(), Vec::new);
        for col in &mut self.ink {
            col.clear();
        }
        for &bounds in &self.footprints {
            Self::reserve(&mut self.ink, pitch, bounds, true);
        }
        for i in [1, 2, 0] {
            let l = &mut layers[i];
            let [pw, ph] = l.pitch;
            for (index, s) in l.streams.iter_mut().enumerate() {
                s.floor_occluded = false;
                if index >= l.overlay_limit {
                    continue;
                }
                if (l.cols[s.col].floor - s.y - 0.5) * ph > band {
                    continue;
                }
                let bounds = [
                    (s.previous[0].min(s.position[0]) - 0.5) * pw,
                    (s.previous[1].min(s.position[1]) - 0.5) * ph,
                    (s.previous[0].max(s.position[0]) + 0.5) * pw,
                    (s.previous[1].max(s.position[1]) + 0.5) * ph,
                ];
                s.floor_occluded = !Self::reserve(&mut self.ink, pitch, bounds, true);
            }
        }
        for i in [1, 2, 0] {
            let l = &mut layers[i];
            if l.density == 0. {
                continue;
            }
            let [pw, ph] = l.pitch;
            for (col, c) in l.cols.iter_mut().enumerate() {
                let top = (c.floor - band / ph).floor() as usize;
                let end = (c.floor.ceil() as usize).min(l.rows);
                for row in top..end {
                    let index = col * l.rows + row;
                    let cell = &mut l.cells[index];
                    if cell.power == 0 || cell.style & (16 | 128) != 0 {
                        continue;
                    }
                    let bounds = [
                        col as f32 * pw,
                        row as f32 * ph,
                        (col + 1) as f32 * pw,
                        (row + 1) as f32 * ph,
                    ];
                    // Cold history cannot displace visible writing.
                    let lit = cell.power > 5 && cell.age(stamp) < 6. * l.half;
                    if !Self::reserve(&mut self.ink, pitch, bounds, lit) {
                        cell.style |= 128;
                        l.masked.push(index);
                        Layer::dirty(c, stamp, row);
                    }
                }
            }
        }
    }
    fn reserve(columns: &mut [Vec<[f32; 4]>], pitch: f32, b: [f32; 4], lit: bool) -> bool {
        let lo = ((b[0] / pitch).floor() as usize).min(columns.len());
        let end = ((b[2] / pitch).ceil() as usize).min(columns.len());
        let bins = &mut columns[lo..end];
        if bins
            .iter()
            .flatten()
            .any(|a| a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1])
        {
            return false;
        }
        if lit {
            for bin in bins {
                bin.push(b);
            }
        }
        true
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn large_rain_reaches_the_last_cell_before_dimming() {
        let mut config = input(7, true);
        config.scale = 2.;
        config.density = 80.;
        let mut layer = Layer::new(1, &config, &Params::default());
        for col in &layer.cols {
            assert!(col.landing_band <= 1.);
            assert_eq!(
                Layer::landing_light(layer.landing_fade, col, col.floor - 1.),
                1.
            );
        }
        layer.spawn(0, 100, false, false, &Params::default());
        let stream = &mut layer.streams[0];
        stream.fade = 1.;
        stream.y = layer.cols[0].floor - 0.01;
        let full_light = 0.70 + 0.20 * stream.mass.min(2.);
        let mut heads = Vec::new();
        layer.heads(&mut heads);
        assert!(heads[0].light[0] >= 0.70 * full_light);
    }
    use super::*;
    use crate::visuals::depth::matrix::tests::input;

    #[test]
    fn cached_visit_decay_is_bit_identical_to_the_scalar_formula() {
        for ticks in (0..=8193).chain([30_000, u32::MAX]) {
            let age = ticks as f32 / 30.;
            assert_eq!(visit_decay(ticks).to_bits(), (-age / 100.).exp().to_bits());
        }
    }

    #[test]
    fn a_repeated_lantern_write_must_leave_a_cooling_interval() {
        let mut config = input(1, false);
        config.trails = 1.;
        let mut layer = Layer::new(1, &config, &Params::default());
        let mut consecutive = 0;
        let mut longest = 0;
        let mut rewound = false;
        for tick in 1..=30 * 150 {
            layer.write(0, 0, tick, 17, 8, 1.);
            let cell = layer.cells[0];
            let age = (tick - u32::from_le_bytes(cell.stamp)) as f32 / 30.;
            let hot = (cell.power as f32 / 255.) * (-age / (3. * layer.half)).exp2() > 0.4;
            if hot {
                rewound |= longest > 30 * 50 && consecutive == 0;
                consecutive += 1;
                longest = longest.max(consecutive);
            } else {
                consecutive = 0;
            }
        }
        assert!(longest <= 30 * 60);
        assert!(rewound);
    }
}

#[cfg(test)]
mod art_tests {
    use super::*;
    use crate::visuals::depth::matrix::{MatrixWorld, tests::input};
    use crate::visuals::depth::{
        common::rng::output_seed,
        world::{Event, LivingWorld},
    };

    #[test]
    fn bolt_segments_share_endpoints_and_ignore_crossing_head_masks() {
        let p = Params::default();
        let mut layer = Layer::new(1, &input(7, true), &p);
        let m = Moment {
            start: 1.,
            end: 4.,
            x: 5800.,
            key: 2,
        };
        for t in 0..6 {
            layer.bolt(m, t as f32 / 30., 1. / 30., 930 + t);
        }
        let mut trunk = None;
        let mut last_start = None;
        for segment in &layer.bolt_segments {
            let [x, y, end_x, end_y] = segment.path;
            // The trunk is always the first segment emitted for a row.
            if last_start != Some(y) {
                if let Some(endpoint) = trunk {
                    assert_eq!([x, y], endpoint);
                }
                trunk = Some([end_x, end_y]);
                last_start = Some(y);
            }
            assert!(end_y > y);
            assert!((segment.ink[0] - 1.6 * 22.).abs() < 0.001);
        }
        assert!(trunk.unwrap()[1] >= 1274.);
        let i = layer
            .cells
            .iter()
            .position(|cell| cell.style & 16 != 0)
            .unwrap();
        let (c, row) = (i / layer.rows, i % layer.rows);
        layer.spawn(c, 100, false, false, &p);
        layer.streams[0].previous = [c as f32 + 0.5, row as f32 + 0.5];
        layer.streams[0].position = layer.streams[0].previous;
        layer.mask_heads(938);
        assert_eq!(layer.cells[i].style & 128, 0);
        layer.bolt(m, 7., 1. / 30., 1140);
        assert!(layer.bolt_segments.is_empty());
    }

    #[test]
    fn bolt_mask_budget_leaves_shed_segments_as_visible_field_ink() {
        let mut config = input(7, false);
        config.scale = 0.25;
        let mut layer = Layer::new(1, &config, &Params::default());
        // More unique bolt cells than the entire bounded overlay texture.
        for col in 0..20 {
            for row in 0..110 {
                layer.bolt_segment(row, col as f32 + 0.5, col as f32 + 0.5, 1., 930);
            }
        }
        assert_eq!(layer.bolt_segments.len(), 2200);
        layer.mask_bolts(930);
        for (ordinal, &index) in layer.bolt_cells.iter().enumerate() {
            let cell = layer.cells[index];
            assert_ne!(cell.style & 16, 0);
            assert_eq!(cell.style & 128 != 0, ordinal < super::super::OVERLAY_CAP);
            assert_ne!(cell.power, 0);
        }
        // Expired overlays must also release their field masks on the next tick.
        layer.bolt(
            Moment {
                start: 1.,
                end: 4.,
                x: 0.,
                key: 1,
            },
            7.,
            1. / 30.,
            1140,
        );
        layer.mask_heads(1140);
        assert!(layer.cells.iter().all(|cell| cell.style & 128 == 0));
    }

    #[test]
    fn lattice_substitutes_crosses_above_sixteen_pixels() {
        let p = Params {
            glyphs: 5,
            ..Params::default()
        };
        let mut layer = Layer::new(0, &input(7, false), &p);
        layer.spawn(0, 42, false, false, &p);
        let mut s = layer.streams[0];
        let mut blocks = 0;
        for row in 0..1024 {
            s.pick_glyph(Some(16.), 30, row);
            let small = s.symbol;
            s.pick_glyph(Some(16.01), 30, row);
            assert!((209..217).contains(&s.symbol));
            if small >= 217 {
                blocks += 1;
                assert!((212..=213).contains(&s.symbol));
            }
            s.pick_glyph(None, 30, row);
            assert!((209..217).contains(&s.symbol));
        }
        assert!(blocks > 20);
    }

    #[test]
    fn profile_spray_has_clear_swept_footprints_on_all_planes() {
        let seed = output_seed(7, 6, "Virtual-0");
        let mut config = input(seed, false);
        config.scale = 2.;
        config.speed = 3.;
        config.density = 80.;
        config.trails = 1.;
        let mut w = MatrixWorld::new(config);
        let mut count = 0;
        let mut peak = 0;
        let mut high = 0f32;
        for t in 1..=610 * 30 {
            w.step(t);
            if t < 590 * 30 {
                continue;
            }
            let mut spans = Vec::new();
            for l in &w.layers {
                for d in &l.drops {
                    let [pw, ph] = l.pitch;
                    let bottom = l.cols[d.x as usize].floor * ph;
                    high = high.max(bottom - d.y * ph);
                    assert!(bottom - d.y * ph <= 110.001);
                    let x = d.x * pw;
                    let px = d.previous[0] * pw;
                    spans.push((x.min(px), x.max(px), pw.max(w.layers[1].pitch[0])));
                    for field in &w.layers {
                        let left = ((x.min(px) - 0.32 * pw) / field.pitch[0]).floor() as usize;
                        let right = (((x.max(px) + 0.32 * pw) / field.pitch[0]).ceil() as usize)
                            .min(field.cols.len());
                        let top = ((d.y.min(d.previous[1]) - 0.32) * ph / field.pitch[1]).floor()
                            as usize;
                        let end = (((d.y.max(d.previous[1]) + 0.32) * ph / field.pitch[1]).ceil()
                            as usize)
                            .min(field.rows);
                        for col in left..right {
                            for row in top..end {
                                let c = field.cells[col * field.rows + row];
                                assert!(c.power == 0 || c.style & (128 | 16) != 0);
                            }
                        }
                    }
                }
            }
            // Check exported glyph extents independently of landing-strip buckets.
            // Union of the two endpoints covers both 60 Hz draws of this tick.
            let mut ink = vec![];
            let mut heads = vec![];
            for l in &w.layers {
                l.heads(&mut heads);
                let [pw, ph] = l.pitch;
                for (col, cells) in l.cells.chunks(l.rows).enumerate() {
                    for (row, cell) in cells.iter().enumerate() {
                        if (row + 1) as f32 * ph <= 1140. || cell.style & (16 | 128) != 0 {
                            continue;
                        }
                        let heat = cell.power as f32 / 255.
                            * (-cell.age(super::super::PREWARM + t as u32) / l.half).exp2();
                        if heat > 0.07 {
                            ink.push([
                                col as f32 * pw,
                                row as f32 * ph,
                                (col + 1) as f32 * pw,
                                (row + 1) as f32 * ph,
                            ]);
                        }
                    }
                }
            }
            for h in heads {
                if h.light[1] == 3. {
                    continue;
                }
                let [x, y, px, py] = h.path;
                let [hx, hy] = [h.ink[0] * 0.5, h.ink[1] * 0.5];
                if y.max(py) + hy > 1140. {
                    ink.push([
                        x.min(px) - hx,
                        y.min(py) - hy,
                        x.max(px) + hx,
                        y.max(py) + hy,
                    ]);
                }
            }
            ink.sort_by(|a, b| a[0].total_cmp(&b[0]));
            for (i, b) in ink.iter().enumerate() {
                for other in &ink[i + 1..] {
                    if other[0] >= b[2] - 0.01 {
                        break;
                    }
                    assert!(
                        other[3] <= b[1] + 0.01 || other[1] >= b[3] - 0.01,
                        "tick {t}: {b:?} overlaps {other:?}"
                    );
                }
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert!(
                spans
                    .windows(2)
                    .all(|pair| pair[1].0 - pair[0].1 >= pair[0].2 + pair[1].2 - 0.01)
            );
            count += spans.len();
            peak = peak.max(spans.len());
        }
        eprintln!(
            "G 590–610 s: {count} droplet tick samples; max simultaneous {peak}; peak height {high:.2} px"
        );
        assert!(count > 600);
    }
    #[test]
    fn departure_hands_off_the_glyph_and_masks_the_interpolated_path() {
        let p = Params {
            wind: 0.,
            ..Params::default()
        };
        let config = input(7, false);
        let mut l = Layer::new(1, &config, &p);
        let mut weather = Weather::new(7, 2560., 1., &p);
        weather.step(0., &p);
        for col in &mut l.cols {
            col.next_spawn = u32::MAX;
        }
        l.spawn(10, 42, false, false, &p);
        let head = &mut l.streams[0];
        head.y = 8.95;
        head.age = 3.;
        head.symbol = 19;
        head.position = [10.5, head.y];
        for row in 7..12 {
            l.write(10, row, 10, 17, 0, 0.6);
        }
        l.step(11, 1. / 30., &weather, &p);
        let s = l.streams[0];
        assert!(s.y.floor() >= 9.);
        let at = |row: usize| l.cells[10 * l.rows + row];
        assert_eq!((at(8).id(), u32::from_le_bytes(at(8).stamp)), (19, 11));
        assert_eq!((at(9).id(), u32::from_le_bytes(at(9).stamp)), (17, 10));
        for phase in [0., 0.25, 0.5, 0.75, 1.] {
            let y = s.previous[1] + phase * (s.position[1] - s.previous[1]);
            for row in (y - 1.).floor() as usize..(y + 1.).ceil() as usize {
                assert_ne!(at(row).style & 128, 0);
            }
        }
    }

    #[test]
    fn contact_strip_follows_one_mid_head() {
        let mut w = MatrixWorld::new(input(output_seed(1, 6, "Virtual-0"), false));
        for t in 1..=95 * 30 {
            w.step(t);
        }
        let layer = &w.layers[1];
        let key = layer
            .streams
            .iter()
            .filter(|s| (12. ..38.).contains(&s.y) && s.fade == 1.)
            .min_by_key(|s| (s.position[0] * layer.pitch[0] - 1100.).abs() as u32)
            .unwrap()
            .key;
        for k in 0..16 {
            if k % 2 == 0 {
                w.step(95 * 30 + k / 2 + 1);
            }
            let l = &w.layers[1];
            let s = l.streams.iter().find(|s| s.key == key).unwrap();
            let alpha = (k % 2) as f32 * 0.5;
            let [x, y] = std::array::from_fn(|i| {
                (s.previous[i] + alpha * (s.position[i] - s.previous[i])) * l.pitch[i]
            });
            eprintln!("contact {k} {x:.2} {y:.2}");
        }
    }

    #[test]
    fn lantern_cruises_at_four_to_five_rows_per_second() {
        let mut w = MatrixWorld::new(input(output_seed(7, 6, "Virtual-0"), false));
        w.inject(Event {
            id: 6,
            at_ns: 4_000_000_000,
            anchor: [1240., 0.],
            strength: 1.,
        })
        .unwrap();
        let mut previous_y = None;
        for t in 1..=11 * 30 {
            w.step(t);
            if t < 8 * 30 {
                continue;
            }
            let s = w.layers[1].streams.iter().find(|s| s.lantern).unwrap();
            if let Some(y) = previous_y {
                let speed: f32 = (s.y - y) * 30.;
                assert!((4.3..5.3).contains(&speed));
            }
            previous_y = Some(s.y);
        }
    }

    #[test]
    fn the_recorded_first_drop_splashes_before_the_end() {
        use crate::visuals::depth::schema::Value;
        let mut config = input(7, true);
        config.parameters.push(Value {
            id: 1,
            kind: 0,
            payload: 2,
        });
        let mut w = MatrixWorld::new(config);
        for (id, second, x) in [(2, 15, 2430.), (3, 45, 5800.), (4, 40, 2750.)] {
            w.inject(Event {
                id,
                at_ns: second * 1_000_000_000,
                anchor: [x, 0.],
                strength: 1.,
            })
            .unwrap();
        }
        let mut near_floor = None;
        let mut landed = false;
        let mut max_near = 0;
        for t in 1..=90 * 30 {
            w.step(t);
            let l = &w.layers[1];
            let key = w.weather.moments[LULL].key;
            if let Some(s) = l.streams.iter().find(|s| s.key == key) {
                if s.y > l.cols[s.col].floor - 1. {
                    near_floor = Some(s.col);
                }
            } else if let Some(col) = near_floor.take() {
                assert!(
                    l.drops
                        .iter()
                        .any(|d| (d.previous[0] - col as f32 - 0.5).abs() < 0.01
                            && (d.previous[1] - l.cols[col].floor + 0.65).abs() < 0.01)
                );
                landed = true;
                eprintln!(
                    "recorded first splash at {:.3}s, world x {:.0}",
                    t as f64 / 30.,
                    (col as f32 + 0.5) * l.pitch[0]
                );
            }
            let near = &w.layers[2];
            max_near = max_near.max(
                near.streams
                    .iter()
                    .filter(|s| (2560. ..6000.).contains(&(s.position[0] * near.pitch[0])))
                    .count(),
            );
        }
        eprintln!("DP-5 peak near heads {max_near}");
        assert!(landed);
        assert!((5..=12).contains(&max_near));
    }
}
