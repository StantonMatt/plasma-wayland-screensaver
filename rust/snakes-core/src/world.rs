// SPDX-License-Identifier: GPL-3.0-or-later
use std::f64::consts::TAU;
use crate::{ Point, WorldRng, normalize_angle };
use crate::math::Geometry;
use crate::controller::{ Controller, Steering };
pub const MAX_SNAKES: usize  =  14;
pub const MAX_SEGMENTS: usize  =  1600;
pub const MAX_FOOD: usize  =  480;
pub const STEP_SECONDS: f64  =  1.0/30.0;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub width: f64,
    pub height: f64,
    pub seed: i32,
    pub density: f64,
    pub trails: f64,
    pub scale: f64,
    pub speed: f64,
    pub intelligence: f64,
    pub palette_size: u32,
    pub self_collisions: bool,
    pub deadly_walls: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            seed: 1,
            density: 50.0,
            trails: 35.0,
            scale: 100.0,
            speed: 100.0,
            intelligence: 75.0,
            palette_size: 6,
            self_collisions: false,
            deadly_walls: true
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    InvalidGeometry,
    InvalidControl,
    InvalidPalette
}
impl Config {
    pub fn validate(self) -> Result<(), ConfigError> {
        if !self.width.is_finite() || !self.height.is_finite() || !(80.0..=16384.0).contains(&self.width) || !(80.0..=16384.0).contains(&self.height) {
            return Err(ConfigError::InvalidGeometry);
        }
        if [self.density, self.trails, self.scale, self.speed, self.intelligence].iter().any(|v|!v.is_finite() || !(0.0..=1000.0).contains(v)) {
            return Err(ConfigError::InvalidControl);
        }
        if self.palette_size==0 || self.palette_size>4096 {
            return Err(ConfigError::InvalidPalette);
        }
        Ok(())
    }
    pub fn snake_count(self) -> usize {
        (3.0+self.density/9.0).round().clamp(3.0, 14.0) as usize
    }
    pub fn food_count(self) -> usize {
        (28.0+self.trails*1.55).round().clamp(28.0, 190.0) as usize
    }
    pub fn maximum_food(self) -> usize {
        (self.food_count()+260).min(MAX_FOOD)
    }
    pub fn base_radius(self) -> f64 {
        (self.width.min(self.height)*0.0075*self.scale/100.0).clamp(4.5, 18.0)
    }
    pub fn maximum_world_segments(self) -> usize {
        let radius = self.base_radius()*1.14*1.25;
        (self.width*self.height*0.22/(radius*radius*2.35).max(1.0)).floor().clamp((self.snake_count()*80) as f64, 6000.0).round() as usize
    }
    fn geometry(self) -> Geometry {
        Geometry {
            width: self.width,
            height: self.height,
            deadly: self.deadly_walls
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Segment {
    pub current: Point,
    pub previous: Point
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Traits {
    pub speed_bias: f64,
    pub turn_bias: f64,
    pub wander_phase: f64,
    pub aggression: f64,
    pub initial_brain_cooldown: f64
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeathReason {
    #[default] None,
    Wall,
    Head,
    Body,
    SelfHit
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub alive: u32,
    pub total_segments: u32,
    pub food: u32,
    pub deaths: u64,
    pub wall_deaths: u64,
    pub head_deaths: u64,
    pub body_deaths: u64,
    pub self_deaths: u64
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Snake {
    pub generation: u32,
    pub alive: bool,
    pub len: usize,
    pub angle: f64,
    pub desired: f64,
    pub base_radius: f64,
    pub radius: f64,
    pub birth_len: usize,
    pub color: u32,
    pub traits: Traits,
    pub respawn: f64,
    pub growth: f64,
    pub stretch: f64,
    pub blocked: bool,
    pub rush: f64,
    pub score: f64,
    pub dying: DeathReason,
    trail_start: usize,
    trail_len: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct SnakeView<'a> {
    pub id: u32,
    pub generation: u32,
    pub alive: bool,
    pub radius: f64,
    pub angle: f64,
    pub desired_angle: f64,
    pub color_index: u32,
    pub traits: Traits,
    pub growth: f64,
    pub segments: &'a [Segment],
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Food {
    pub id: u64,
    pub p: Point,
    pub value: f64,
    pub color: u32,
    pub size: f64,
    pub velocity: Point,
    pub life: f64,
    pub phase: f64,
    pub feast: u64,
    pub trail_index: u32,
    pub feast_len: u32,
    pub attraction: f64,
    pub target: Point,
    pub owner: i32,
    pub original_life: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct FoodView {
    pub id: u64,
    pub position: Point,
    pub value: f64,
    pub color_index: u32,
    pub size: f64,
    pub life: f64,
    pub phase: f64,
    pub attraction: f64,
    pub attraction_target: Point,
    pub vacuum_owner: i32,
    pub feast_id: u64,
    pub trail_index: u32,
    pub feast_length: u32,
}
impl From<&Food> for FoodView {
    fn from(f: &Food) -> Self {
        Self {
            id: f.id,
            position: f.p,
            value: f.value,
            color_index: f.color,
            size: f.size,
            life: f.life,
            phase: f.phase,
            attraction: f.attraction,
            attraction_target: f.target,
            vacuum_owner: f.owner,
            feast_id: f.feast,
            trail_index: f.trail_index,
            feast_length: f.feast_len
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
struct TrailPoint {
    p: Point,
    distance: f64
}
/// Internal storage is deliberately private. IDs are slot indices; respawn
/// increments generation and reuses all storage. Mutations require exclusive access.
pub struct World {
    pub(crate) config: Config,
    pub(crate) snakes: Vec<Snake>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) food: Vec<Food>,
    trails: Vec<TrailPoint>,
    trail_capacity: usize,
    grid_heads: Vec<i32>,
    grid_next: Vec<i32>,
    grid_columns: usize,
    grid_rows: usize,
    rng: WorldRng,
    next_food: u64,
    next_feast: u64,
    pub(crate) tick: u64,
    pub(crate) time: f64,
    pub(crate) geometry_generation: u64,
    growth_slots: usize,
    deaths: Stats,
    #[cfg(feature = "parity")]
    events: Vec<String>,
    #[cfg(feature = "parity")]
    parity_record: bool,
    #[cfg(feature = "parity")]
    parity_killers: Vec<Vec<usize>>,
    generations: [u32;
    MAX_SNAKES],
}
impl World {
    pub fn new(config: Config) -> Result<Self, ConfigError> {
        config.validate()?;
        let mut world = Self {
            config,
            snakes: vec![Snake::default();
            config.snake_count()],
            segments: vec![Segment::default();
            MAX_SNAKES*MAX_SEGMENTS],
            food: Vec::with_capacity(MAX_FOOD),
            trails: Vec::new(),
            trail_capacity: 0,
            grid_heads: Vec::new(),
            grid_next: vec![-1;
            MAX_SNAKES*MAX_SEGMENTS],
            grid_columns: 0,
            grid_rows: 0,
            rng: WorldRng::new(config.seed),
            next_food: 1,
            next_feast: 1,
            tick: 0,
            time: 0.0,
            geometry_generation: 1,
            growth_slots: 0,
            deaths: Stats::default(),
            #[cfg(feature = "parity")]
            events: Vec::new(),
            #[cfg(feature = "parity")]
            parity_record: false,
            #[cfg(feature = "parity")]
            parity_killers: Vec::new(),
            generations: [0;
            MAX_SNAKES]
        };
        world.prepare_storage();
        world.initialize();
        Ok(world)
    }
    pub fn config(&self) -> Config {
        self.config
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn simulation_time(&self) -> f64 {
        self.time
    }
    pub fn geometry_generation(&self) -> u64 {
        self.geometry_generation
    }
    pub fn rng_state(&self) -> u32 {
        self.rng.state()
    }
    pub fn snake_count(&self) -> usize {
        self.snakes.len()
    }
    pub fn snake(&self, id: usize) -> Option<SnakeView<'_>> {
        self.snakes.get(id).map(|s|SnakeView {
            id: id as u32,
            generation: s.generation,
            alive: s.alive,
            radius: s.radius,
            angle: s.angle,
            desired_angle: s.desired,
            color_index: s.color,
            traits: s.traits,
            growth: s.growth,
            segments: &self.segments[id*MAX_SEGMENTS..id*MAX_SEGMENTS+s.len]
        })
    }
    pub fn snakes(&self) -> impl ExactSizeIterator<Item = SnakeView<'_>> {
        (0..self.snakes.len()).map(|i|self.snake(i).unwrap())
    }
    pub fn foods(&self) -> impl ExactSizeIterator<Item = FoodView>+'_ {
        self.food.iter().map(FoodView::from)
    }
    pub fn distance_squared(&self, a: Point, b: Point) -> f64 {
        self.config.geometry().distance2(a, b)
    }
    pub fn segment_distance_squared(&self, p: Point, a: Point, b: Point) -> f64 {
        self.config.geometry().segment_distance2(p, a, b)
    }
    pub fn segments_distance_squared(&self, a: Point, b: Point, c: Point, d: Point) -> f64 {
        self.config.geometry().segments_distance2(a, b, c, d)
    }
    pub fn stats(&self) -> Stats {
        let mut stats = self.deaths;
        stats.alive = 0;
        stats.total_segments = 0;
        stats.food = self.food.len() as u32;
        for s in &self.snakes {
            if s.alive {
                stats.alive+=1;
                stats.total_segments+=s.len as u32;
            }
        }
        stats
    }
    fn prepare_storage(&mut self) {
        // One trail sample per 30 Hz tick. Minimum possible speed is the
        // lowest bias at maximum length, without rush/growth boost. Include
        // birth samples, the four-point tail reserve, and pruning slack.
        let radius = self.snakes.iter().fold(self.config.base_radius()*1.14*1.25, |r, s|r.max(s.base_radius*1.25));
        let min_step = (52.0+self.config.speed*0.66)*0.86/(1.0+(MAX_SEGMENTS-24) as f64*0.004)*STEP_SECONDS;
        let retained = self.snakes.iter().map(|s|s.trail_len+2).max().unwrap_or(2);
        let capacity = ((radius*1.18*(MAX_SEGMENTS+6) as f64/min_step).ceil() as usize+MAX_SEGMENTS+16).max(retained).next_power_of_two();
        if capacity!=self.trail_capacity {
            let mut trails = vec![TrailPoint::default();
            capacity*self.snakes.len()];
            if self.trail_capacity>0 {
                for i in 0..self.snakes.len() {
                    for j in 0..self.snakes[i].trail_len {
                        trails[i*capacity+j] = self.trail_point(i, j);
                    }
                }
            }
            self.trails = trails;
            self.trail_capacity = capacity;
            for s in &mut self.snakes {
                s.trail_start = 0;
            }
        }
        let target = (self.config.base_radius()*6.0).max(24.0);
        self.grid_columns = (self.config.width/target).ceil() as usize;
        self.grid_rows = (self.config.height/target).ceil() as usize;
        self.grid_heads.resize(self.grid_columns*self.grid_rows, -1);
    }
    fn initialize(&mut self) {
        self.rng = WorldRng::new(self.config.seed);
        self.tick = 0;
        self.time = 0.0;
        self.next_food = 1;
        self.next_feast = 1;
        self.deaths = Stats::default();
        self.food.clear();
        for s in &mut self.snakes {
            s.alive = false;
            s.len = 0;
        }
        for i in 0..self.snakes.len() {
            self.make_snake(i);
        }
        self.growth_slots = self.config.maximum_world_segments().saturating_sub(self.stats().total_segments as usize);
        for _ in 0..self.config.food_count() {
            self.add_ambient_food();
        }
    }
    /// Density-count or seed changes restart the world (as QML initialization
    /// does). Other controls preserve live state; geometry rescales positions.
    pub fn reconfigure(&mut self, config: Config) -> Result<(), ConfigError> {
        config.validate()?;
        if config.seed!=self.config.seed || config.snake_count()!=self.snakes.len() {
            self.config = config;
            self.snakes.resize(config.snake_count(), Snake::default());
            // Same-size trail storage may need to accommodate more slots.
            self.trail_capacity = 0;
            for s in &mut self.snakes {
                s.trail_start = 0;
                s.trail_len = 0;
            }
            self.prepare_storage();
            self.geometry_generation+=1;
            self.initialize();
            return Ok(());
        }
        let old = self.config;
        self.config = config;
        self.scale_geometry(old.width, old.height);
        self.prepare_storage();
        if old.deadly_walls!=config.deadly_walls {
            for i in 0..self.snakes.len() {
                self.rebuild_trail(i);
            }
        }
        if old.width!=config.width || old.height!=config.height || old.deadly_walls!=config.deadly_walls {
            self.geometry_generation+=1;
        }
        self.growth_slots = config.maximum_world_segments().saturating_sub(self.stats().total_segments as usize);
        Ok(())
    }
    pub fn resize(&mut self, width: f64, height: f64) -> Result<(), ConfigError> {
        self.reconfigure(Config {
            width,
            height,
            ..self.config
        })
    }
    fn scale_geometry(&mut self, old_width: f64, old_height: f64) {
        let sx = self.config.width/old_width;
        let sy = self.config.height/old_height;
        if sx==1.0 && sy==1.0 {
            return;
        }
        for i in 0..self.snakes.len() {
            for j in 0..self.snakes[i].len {
                let seg = &mut self.segments[i*MAX_SEGMENTS+j];
                seg.current.x*=sx;
                seg.current.y*=sy;
                seg.previous.x*=sx;
                seg.previous.y*=sy;
            }
            let mut d = 0.0;
            let mut prev = Point::default();
            for j in 0..self.snakes[i].trail_len {
                let idx = self.trail_index(i, j);
                let t = &mut self.trails[idx];
                t.p.x*=sx;
                t.p.y*=sy;
                if j>0 {
                    d+=t.p.distance2(prev).sqrt();
                }
                t.distance = d;
                prev = t.p;
            }
        }
        for f in &mut self.food {
            f.p.x*=sx;
            f.p.y*=sy;
            f.velocity.x*=sx;
            f.velocity.y*=sy;
            f.target.x*=sx;
            f.target.y*=sy;
        }
    }
    fn spawn_position(&mut self, radius: f64, len: usize) -> (Point, f64) {
        let g = self.config.geometry();
        let margin = (radius*5.0).max(18.0);
        let clearance = (g.width.min(g.height)*0.22).min(170.0_f64.max(radius*20.0));
        let mut best = (Point {
            x: g.width/2.0,
            y: g.height/2.0
        }, self.rng.random()*TAU, -1.0);
        for _ in 0..48 {
            let p = Point {
                x: margin+self.rng.random()*(g.width-margin*2.0).max(1.0),
                y: margin+self.rng.random()*(g.height-margin*2.0).max(1.0)
            };
            let angle = self.rng.random()*TAU;
            let mut closest = f64::MAX;
            let samples = len.div_ceil(4).clamp(2, 7);
            for sample in 0..samples {
                let d = radius*1.18*sample as f64*(len-1) as f64/(samples-1) as f64;
                let q = Point {
                    x: p.x-angle.cos()*d,
                    y: p.y-angle.sin()*d
                };
                if g.deadly && (q.x<margin || q.x>g.width-margin || q.y<margin || q.y>g.height-margin) {
                    closest = 0.0;
                    break;
                }
                for (i, other) in self.snakes.iter().enumerate() {
                    if other.alive {
                        for j in (0..other.len).step_by(2) {
                            closest = closest.min(g.distance2(q, self.segments[i*MAX_SEGMENTS+j].current));
                        }
                    }
                }
            }
            if closest>best.2 {
                best = (p, angle, closest);
            }
            if closest>=clearance*clearance {
                break;
            }
        }
        (best.0, best.1)
    }
    fn make_snake(&mut self, i: usize) {
        let radius = self.config.base_radius()*(0.86+self.rng.random()*0.28);
        let len = 14+(self.rng.random()*12.0).floor() as usize+if i==0 {
            7
        } else {
            0
        };
        let (p, angle) = self.spawn_position(radius, len);
        for j in 0..len {
            // QML deliberately does not wrap the birth body.
            let q = Point {
                x: p.x-angle.cos()*radius*1.18*j as f64,
                y: p.y-angle.sin()*radius*1.18*j as f64
            };
            self.segments[i*MAX_SEGMENTS+j] = Segment {
                current: q,
                previous: q
            };
        }
        let traits = Traits {
            speed_bias: 0.86+self.rng.random()*0.28,
            turn_bias: (self.rng.random()-0.5)*0.35,
            wander_phase: self.rng.random()*TAU,
            aggression: self.rng.random(),
            initial_brain_cooldown: self.rng.random()*0.06
        };
        let generation = self.generations[i].wrapping_add(1).max(1);
        self.generations[i] = generation;
        self.snakes[i] = Snake {
            generation,
            alive: true,
            len,
            angle,
            desired: angle,
            base_radius: radius,
            radius,
            birth_len: len,
            color: i as u32%self.config.palette_size,
            traits,
            score: len as f64,
            ..Snake::default()
        };
        self.rebuild_trail(i);
        #[cfg(feature = "parity")]
        self.parity_respawn(i);
    }
    fn trail_index(&self, i: usize, j: usize) -> usize {
        i*self.trail_capacity+((self.snakes[i].trail_start+j)&(self.trail_capacity-1))
    }
    fn trail_point(&self, i: usize, j: usize) -> TrailPoint {
        self.trails[self.trail_index(i, j)]
    }
    fn push_trail(&mut self, i: usize, t: TrailPoint) {
        // Capacity is proven from maximum length/radius and minimum speed.
        debug_assert!(self.snakes[i].trail_len<self.trail_capacity);
        let idx = self.trail_index(i, self.snakes[i].trail_len);
        self.trails[idx] = t;
        self.snakes[i].trail_len+=1;
    }
    fn rebuild_trail(&mut self, i: usize) {
        let s = self.snakes[i];
        self.snakes[i].trail_start = 0;
        self.snakes[i].trail_len = 0;
        if s.len==0 {
            return;
        }
        let g = self.config.geometry();
        let base = i*MAX_SEGMENTS;
        // Reuse the ring's beginning for unwrapped head-to-tail scratch.
        let start = i*self.trail_capacity;
        self.trails[start].p = self.segments[base].current;
        for j in 1..s.len {
            let d = g.delta(self.segments[base+j-1].current, self.segments[base+j].current);
            let prev = self.trails[start+j-1].p;
            self.trails[start+j].p = Point {
                x: prev.x+d.x,
                y: prev.y+d.y
            };
        }
        let tail = self.trails[start+s.len-1].p;
        let mut d = if s.len>1 {
            let before = self.trails[start+s.len-2].p;
            Point {
                x: tail.x-before.x,
                y: tail.y-before.y
            }
        } else {
            Point {
                x: -s.angle.cos(),
                y: -s.angle.sin()
            }
        };
        let length = (d.x*d.x+d.y*d.y).sqrt();
        if length>0.001 {
            d.x/=length;
            d.y/=length;
        } else {
            d = Point {
                x: -s.angle.cos(),
                y: -s.angle.sin()
            };
        }
        for reserve in 1..=4 {
            self.trails[start+s.len+reserve-1].p = Point {
                x: tail.x+d.x*s.radius*1.18*reserve as f64,
                y: tail.y+d.y*s.radius*1.18*reserve as f64
            };
        }
        self.trails[start..start+s.len+4].reverse();
        let mut distance = 0.0;
        for j in 0..s.len+4 {
            if j>0 {
                distance+=self.trails[start+j].p.distance2(self.trails[start+j-1].p).sqrt();
            }
            self.trails[start+j].distance = distance;
        }
        self.snakes[i].trail_len = s.len+4;
    }
    fn ensure_trail(&mut self, i: usize) {
        let s = self.snakes[i];
        if s.trail_len<2 || self.config.geometry().distance2(self.config.geometry().wrap(self.trail_point(i, s.trail_len-1).p), self.segments[i*MAX_SEGMENTS].current)>0.01 {
            self.rebuild_trail(i);
        }
    }
    fn append_head_trail(&mut self, i: usize) {
        let s = self.snakes[i];
        let latest = self.trail_point(i, s.trail_len-1);
        let g = self.config.geometry();
        let head = self.segments[i*MAX_SEGMENTS].current;
        let d = g.delta(g.wrap(latest.p), head);
        let p = if g.deadly {
            head
        } else {
            Point {
                x: latest.p.x+d.x,
                y: latest.p.y+d.y
            }
        };
        let travel = p.distance2(latest.p).sqrt();
        if travel>0.0001 {
            self.push_trail(i, TrailPoint {
                p,
                distance: latest.distance+travel
            });
        }
    }
    fn place_segments(&mut self, i: usize) {
        let s = self.snakes[i];
        if s.trail_len<2 || s.len<2 {
            return;
        }
        let newest = self.trail_point(i, s.trail_len-1);
        let spacing = s.radius*1.18;
        let mut cursor = s.trail_len-2;
        for j in 1..s.len {
            let target = newest.distance-spacing*(j as f64+s.stretch);
            while cursor>0 && self.trail_point(i, cursor).distance>target {
                cursor-=1;
            }
            let older = self.trail_point(i, cursor);
            let newer = self.trail_point(i, (cursor+1).min(s.trail_len-1));
            let t = ((target-older.distance)/(newer.distance-older.distance).max(0.0001)).clamp(0.0, 1.0);
            self.segments[i*MAX_SEGMENTS+j].current = self.config.geometry().wrap(Point {
                x: older.p.x+(newer.p.x-older.p.x)*t,
                y: older.p.y+(newer.p.y-older.p.y)*t
            });
        }
        let keep_after = newest.distance-spacing*(s.len+4) as f64;
        while self.snakes[i].trail_len>1 && self.trail_point(i, 1).distance<keep_after {
            self.snakes[i].trail_start = (self.snakes[i].trail_start+1)&(self.trail_capacity-1);
            self.snakes[i].trail_len-=1;
        }
    }
    fn growth_cost(s: &Snake) -> f64 {
        1.0+((s.len.saturating_sub(120)) as f64/260.0).powf(0.85)
    }
    fn maximum_snake_segments(&self, s: &Snake) -> usize {
        (self.config.width*self.config.height*0.075/(s.radius*s.radius*2.35).max(1.0)).floor().clamp(400.0, 1600.0).round() as usize
    }
    fn speed(&self, s: &Snake) -> f64 {
        let boost = if !s.blocked && s.growth>=Self::growth_cost(s) {
            0.2
        } else {
            0.0
        };
        (52.0+self.config.speed*0.66)*s.traits.speed_bias*(1.0+s.rush+boost)/(1.0+s.len.saturating_sub(24) as f64*0.004)
    }
    fn minimum_turn_radius(s: &Snake) -> f64 {
        s.radius*(3.15+((s.len as f64/20.0).max(1.0).ln()/25.0_f64.ln()).clamp(0.0, 1.0)*1.35)
    }
    fn turn_rate(&self, s: &Snake) -> f64 {
        (2.05+(self.config.intelligence/100.0).clamp(0.0, 1.0)*1.8+20.0/(s.len.max(8) as f64)).min(self.speed(s)/Self::minimum_turn_radius(s).max(1.0))
    }
    fn update_radius(s: &mut Snake) {
        s.radius = s.base_radius*(1.0+(s.len.saturating_sub(s.birth_len) as f64*0.0025).min(0.25));
    }
    fn move_snake(&mut self, i: usize, seconds: f64) {
        let cost = Self::growth_cost(&self.snakes[i]);
        let allowed = self.snakes[i].len<self.maximum_snake_segments(&self.snakes[i]) && self.growth_slots>0;
        self.snakes[i].blocked = !allowed;
        let base = i*MAX_SEGMENTS;
        for j in 0..self.snakes[i].len {
            self.segments[base+j].previous = self.segments[base+j].current;
        }
        self.ensure_trail(i);
        let max_turn = self.turn_rate(&self.snakes[i])*seconds;
        let diff = normalize_angle(self.snakes[i].desired-self.snakes[i].angle);
        self.snakes[i].angle = normalize_angle(self.snakes[i].angle+diff.clamp(-max_turn, max_turn));
        let speed = self.speed(&self.snakes[i]);
        let s = self.snakes[i];
        let head = &mut self.segments[base];
        head.current.x+=s.angle.cos()*speed*seconds;
        head.current.y+=s.angle.sin()*speed*seconds;
        head.current = self.config.geometry().wrap(head.current);
        self.append_head_trail(i);
        self.snakes[i].stretch = if allowed && s.growth>=cost {
            (s.stretch+speed*seconds/(s.radius*1.18).max(1.0)*0.62).min(1.0)
        } else {
            0.0
        };
        self.place_segments(i);
        if allowed && s.growth>=cost && self.snakes[i].stretch>=1.0 {
            let p = self.segments[base].current;
            self.segments.copy_within(base+1..base+s.len, base+2);
            self.segments[base+1] = Segment {
                current: p,
                previous: p
            };
            let snake = &mut self.snakes[i];
            snake.len+=1;
            snake.growth-=cost;
            snake.stretch = 0.0;
            self.growth_slots = self.growth_slots.saturating_sub(1);
            Self::update_radius(snake);
            self.place_segments(i);
            #[cfg(feature = "parity")]
            self.parity_growth(i);
        }
    }
    /// Requests every live snake's input against the same pre-movement world.
    /// Invalid controls are ignored; controllers cannot mutate world RNG.
    pub fn step<C: Controller+?Sized>(&mut self, controller: &mut C) {
        self.step_seconds(controller, STEP_SECONDS);
    }
    fn step_seconds<C: Controller+?Sized>(&mut self, controller: &mut C, seconds: f64) {
        self.time+=seconds;
        self.update_food(seconds);
        self.growth_slots = self.config.maximum_world_segments().saturating_sub(self.stats().total_segments as usize);
        let mut inputs = [Steering::default();
        MAX_SNAKES];
        for (i, input) in inputs.iter_mut().enumerate().take(self.snakes.len()) {
            if self.snakes[i].alive {
                *input = controller.steer(self, self.snake(i).unwrap());
            }
        }
        for (i, input) in inputs.iter().enumerate().take(self.snakes.len()) {
            if self.snakes[i].alive && input.is_valid() {
                self.snakes[i].desired = input.desired_angle;
                self.snakes[i].rush = input.rush;
            }
        }
        for i in 0..self.snakes.len() {
            if !self.snakes[i].alive {
                self.snakes[i].respawn-=seconds;
                if self.snakes[i].respawn<=0.0 {
                    self.make_snake(i);
                }
                continue;
            }
            self.move_snake(i, seconds);
        }
        self.feed_snakes(seconds);
        self.mark_collisions();
        for i in 0..self.snakes.len() {
            if self.snakes[i].alive && self.snakes[i].dying!=DeathReason::None {
                self.explode_snake(i);
            }
        }
        self.tick = self.tick.wrapping_add(1);
    }
    pub fn step_n<C: Controller+?Sized>(&mut self, controller: &mut C, ticks: u32) {
        for _ in 0..ticks {
            self.step(controller);
        }
    }
    fn add_food(&mut self, mut f: Food) {
        debug_assert!(self.food.len()<MAX_FOOD);
        f.id = self.next_food;
        self.next_food = self.next_food.wrapping_add(1);
        f.target = f.p;
        f.p = if self.config.deadly_walls {
            Point {
                x: f.p.x.clamp(3.0, (self.config.width-3.0).max(3.0)),
                y: f.p.y.clamp(3.0, (self.config.height-3.0).max(3.0))
            }
        } else {
            self.config.geometry().wrap(f.p)
        };
        f.size = self.config.base_radius()*(0.23+f.value.min(1.4)*0.13);
        f.phase = self.rng.random()*TAU;
        f.owner = -1;
        f.original_life = f.life;
        self.food.push(f);
    }
    fn add_ambient_food(&mut self) {
        let margin = (self.config.base_radius()*2.0).max(12.0);
        let f = Food {
            p: Point {
                x: margin+self.rng.random()*(self.config.width-margin*2.0).max(1.0),
                y: margin+self.rng.random()*(self.config.height-margin*2.0).max(1.0)
            },
            value: 0.48+self.rng.random()*0.3,
            color: (self.rng.random()*self.config.palette_size as f64).floor() as u32,
            life: 34.0+self.rng.random()*12.0,
            ..Food::default()
        };
        self.add_food(f);
    }
    fn update_food(&mut self, seconds: f64) {
        let g = self.config.geometry();
        let damping = 0.16_f64.powf(seconds);
        for i in (0..self.food.len()).rev() {
            let f = &mut self.food[i];
            f.attraction = 0.0;
            if f.life>0.0 {
                f.life-=seconds;
                if f.life<=0.0 {
                    self.food.remove(i);
                    continue;
                }
            }
            if f.velocity.x.abs()+f.velocity.y.abs()<0.1 {
                continue;
            }
            f.p.x+=f.velocity.x*seconds;
            f.p.y+=f.velocity.y*seconds;
            f.velocity.x*=damping;
            f.velocity.y*=damping;
            if !g.deadly {
                f.p = g.wrap(f.p);
            } else {
                if f.p.x<2.0 || f.p.x>g.width-2.0 {
                    f.p.x = f.p.x.clamp(2.0, g.width-2.0);
                    f.velocity.x*= -0.45;
                }
                if f.p.y<2.0 || f.p.y>g.height-2.0 {
                    f.p.y = f.p.y.clamp(2.0, g.height-2.0);
                    f.velocity.y*= -0.45;
                }
            }
        }
        for _ in 0..3 {
            if self.food.len()>=self.config.food_count() {
                break;
            }
            self.add_ambient_food();
        }
    }
    fn consume_food(&mut self, i: usize, owner: usize) {
        let f = self.food[i];
        #[cfg(feature = "parity")]
        self.parity_eat(owner, f);
        let s = &mut self.snakes[owner];
        s.growth = (s.growth+f.value).min(Self::growth_cost(s)*12.0);
        s.score+=f.value;
        self.food.remove(i);
    }
    fn feed_snakes(&mut self, seconds: f64) {
        let g = self.config.geometry();
        for i in (0..self.food.len()).rev() {
            let mut f = self.food[i];
            let mut eater = None;
            let mut closest = f64::MAX;
            if f.owner>=0 && (f.owner as usize)<self.snakes.len() {
                let owner = f.owner as usize;
                if self.snakes[owner].alive {
                    eater = Some(owner);
                    closest = g.distance2(f.p, self.segments[owner*MAX_SEGMENTS].current);
                }
                else {
                    f.owner = -1;
                    f.life = f.original_life;
                }
            }
            if eater.is_none() {
                for (j, s) in self.snakes.iter().enumerate() {
                    if !s.alive {
                        continue;
                    }
                    let p = self.segments[j*MAX_SEGMENTS].current;
                    let reach = s.radius*3.0+f.size;
                    let d = g.delta(f.p, p);
                    if d.x.abs()>reach || d.y.abs()>reach {
                        continue;
                    }
                    let ds = d.x*d.x+d.y*d.y;
                    if ds<=reach*reach && ds<closest {
                        closest = ds;
                        eater = Some(j);
                    }
                }
            }
            let Some(owner) = eater else {
                self.food[i] = f;
                continue;
            };
            let s = self.snakes[owner];
            let head = self.segments[owner*MAX_SEGMENTS].current;
            let distance = closest.sqrt();
            if f.owner!=owner as i32 {
                f.owner = owner as i32;
                f.original_life = f.life;
                f.velocity = Point::default();
                f.life =  -1.0;
            }
            let eating = s.radius*1.16+f.size;
            if distance<=eating {
                self.consume_food(i, owner);
                continue;
            }
            let pull = (1.0-(distance-eating)/(s.radius*1.84).max(1.0)).clamp(0.08, 1.0);
            let speed = (240.0+pull*420.0).max(self.speed(&s)*1.8);
            let travel = (distance-eating).min(speed*seconds);
            if travel>=distance-eating-0.001 {
                self.consume_food(i, owner);
                continue;
            }
            let d = g.delta(f.p, head);
            f.p.x+=d.x/distance*travel;
            f.p.y+=d.y/distance*travel;
            f.p = g.wrap(f.p);
            f.attraction = pull;
            f.target = head;
            self.food[i] = f;
        }
    }
    fn cell(&self, p: Point) -> (usize, usize) {
        let g = self.config.geometry();
        let p = if g.deadly {
            Point {
                x: p.x.clamp(0.0, g.width-0.001),
                y: p.y.clamp(0.0, g.height-0.001)
            }
        } else {
            g.wrap(p)
        };
        (((p.x/(g.width/self.grid_columns as f64)).floor() as usize).min(self.grid_columns-1), ((p.y/(g.height/self.grid_rows as f64)).floor() as usize).min(self.grid_rows-1))
    }
    fn swept_hit(g: Geometry, a: Segment, b: Segment, reach: f64) -> bool {
        let distance = g.distance2(a.current, b.current);
        let squared = reach*reach;
        if distance<squared {
            return true;
        }
        let travel = g.distance2(a.previous, a.current).sqrt()+g.distance2(b.previous, b.current).sqrt();
        distance<(reach+travel).powi(2) && g.segments_distance2(a.previous, a.current, b.previous, b.current)<squared
    }
    fn mark_collisions(&mut self) {
        let g = self.config.geometry();
        for (i, s) in self.snakes.iter_mut().enumerate() {
            s.dying = DeathReason::None;
            if !s.alive {
                continue;
            }
            let p = self.segments[i*MAX_SEGMENTS].current;
            if g.deadly && (p.x<0.0 || p.x>g.width || p.y<0.0 || p.y>g.height) {
                s.dying = DeathReason::Wall;
            }
        }
        for left in 0..self.snakes.len() {
            let a = self.snakes[left];
            if !a.alive {
                continue;
            }
            for right in left+1..self.snakes.len() {
                let b = self.snakes[right];
                if !b.alive {
                    continue;
                }
                if Self::swept_hit(g, self.segments[left*MAX_SEGMENTS], self.segments[right*MAX_SEGMENTS], (a.radius+b.radius)*0.82) {
                    let diff = a.len as isize-b.len as isize;
                    if diff.abs()<4 {
                        self.snakes[left].dying = DeathReason::Head;
                        self.snakes[right].dying = DeathReason::Head;
                    }
                    else if diff<0 {
                        self.snakes[left].dying = DeathReason::Head;
                    } else {
                        self.snakes[right].dying = DeathReason::Head;
                    }
                }
            }
        }
        // Linked buckets, in JS occupant order: insert reversed, then walk
        // forwards. No bucket Vec can grow during a tick.
        self.grid_heads.fill(-1);
        for i in (0..self.snakes.len()).rev() {
            if !self.snakes[i].alive {
                continue;
            }
            for j in (1..self.snakes[i].len).rev() {
                let index = i*MAX_SEGMENTS+j;
                let (x, y) = self.cell(self.segments[index].current);
                let key = y*self.grid_columns+x;
                self.grid_next[index] = self.grid_heads[key];
                self.grid_heads[key] = index as i32;
            }
        }
        for i in 0..self.snakes.len() {
            let s = self.snakes[i];
            if !s.alive || s.dying!=DeathReason::None {
                continue;
            }
            let head = self.segments[i*MAX_SEGMENTS];
            let (hx, hy) = self.cell(head.current);
            let mut visited = [usize::MAX;
            9];
            let mut count = 0;
            'search: for ox in -1..=1 {
                for oy in -1..=1 {
                    let mut x = hx as isize+ox;
                    let mut y = hy as isize+oy;
                    if !g.deadly {
                        x = x.rem_euclid(self.grid_columns as isize);
                        y = y.rem_euclid(self.grid_rows as isize);
                    }
                    else if x<0 || y<0 || x>=self.grid_columns as isize || y>=self.grid_rows as isize {
                        continue;
                    }
                    let key = y as usize*self.grid_columns+x as usize;
                    if visited[..count].contains(&key) {
                        continue;
                    }
                    visited[count] = key;
                    count+=1;
                    let mut occupant = self.grid_heads[key];
                    while occupant>=0 {
                        let encoded = occupant as usize;
                        let other = encoded/MAX_SEGMENTS;
                        let j = encoded%MAX_SEGMENTS;
                        occupant = self.grid_next[encoded];
                        let same = i==other;
                        if same && (!self.config.self_collisions || j<10) {
                            continue;
                        }
                        let reach = if same {
                            s.radius*1.48
                        } else {
                            (s.radius+self.snakes[other].radius)*0.78
                        };
                        if Self::swept_hit(g, head, self.segments[encoded], reach) {
                            self.snakes[i].dying = if same {
                                DeathReason::SelfHit
                            } else {
                                DeathReason::Body
                            };
                            break 'search;
                        }
                    }
                }
            }
        }
        #[cfg(feature = "parity")]
        self.parity_collisions();
    }
    fn death_particle_count(s: &Snake) -> usize {
        if s.len==0 {
            return 0;
        }
        let thickness = if s.base_radius>0.0 {
            s.radius/s.base_radius
        } else {
            1.0
        };
        (s.len as f64*(0.78+thickness*0.22)).clamp(8.0, 360.0).round() as usize
    }
    fn make_room_for_death_food(&mut self, slots: usize) {
        let mut needed = (self.food.len()+slots).saturating_sub(self.config.maximum_food());
        for pass in 0..2 {
            if needed==0 {
                break;
            }
            for i in (0..self.food.len()).rev() {
                if needed==0 {
                    break;
                }
                let f = self.food[i];
                if f.owner>=0 || (pass==0 && f.feast>0) {
                    continue;
                }
                self.food.remove(i);
                needed-=1;
            }
        }
    }
    fn explode_snake(&mut self, i: usize) -> usize {
        let s = self.snakes[i];
        let intended = Self::death_particle_count(&s);
        self.make_room_for_death_food(intended.min(48));
        let emitted = intended.min(self.config.maximum_food().saturating_sub(self.food.len()));
        let value = if emitted>0 {
            (s.len as f64/emitted as f64).max(0.65)
        } else {
            0.0
        };
        let feast = self.next_feast;
        self.next_feast = self.next_feast.wrapping_add(1);
        let g = self.config.geometry();
        for j in 0..emitted {
            let position = if emitted>1 {
                j as f64*(s.len-1) as f64/(emitted-1) as f64
            } else {
                0.0
            };
            let lower = position.floor() as usize;
            let upper = (lower+1).min(s.len-1);
            let fraction = position-lower as f64;
            let a = self.segments[i*MAX_SEGMENTS+lower].current;
            let d = g.delta(a, self.segments[i*MAX_SEGMENTS+upper].current);
            let p = g.wrap(Point {
                x: a.x+d.x*fraction,
                y: a.y+d.y*fraction
            });
            let angle = self.rng.random()*TAU;
            let force = 22.0+self.rng.random()*90.0;
            let f = Food {
                p: Point {
                    x: p.x+(self.rng.random()-0.5)*s.radius,
                    y: p.y+(self.rng.random()-0.5)*s.radius
                },
                value: value*(0.8+self.rng.random()*0.4),
                color: s.color,
                velocity: Point {
                    x: angle.cos()*force,
                    y: angle.sin()*force
                },
                life: 18.0+self.rng.random()*16.0,
                feast,
                trail_index: j as u32,
                feast_len: emitted as u32,
                ..Food::default()
            };
            self.add_food(f);
        }
        let snake = &mut self.snakes[i];
        snake.alive = false;
        snake.respawn = 2.2+self.rng.random()*3.2;
        snake.len = 0;
        snake.trail_len = 0;
        self.deaths.deaths+=1;
        match s.dying {
            DeathReason::Wall => self.deaths.wall_deaths+=1,
            DeathReason::Head => self.deaths.head_deaths+=1,
            DeathReason::SelfHit => self.deaths.self_deaths+=1,
            _ => self.deaths.body_deaths+=1
        };
        #[cfg(feature = "parity")]
        self.parity_death(i, s.dying, emitted);
        emitted
    }
}
#[cfg(feature = "parity")]
#[path = "world/parity.rs"]
pub mod parity;
#[cfg(test)]
#[path  =  "world/tests.rs"]
pub(crate) mod tests;
