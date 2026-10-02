// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixture-only snapshot import and numeric replay. Absent from production builds.
use super::*;
use std::io::{self, Write};
use std::str::{FromStr, SplitWhitespace};

struct Input<'a>(SplitWhitespace<'a>);
impl Input<'_> {
    fn n<T: FromStr>(&mut self) -> T where T::Err: std::fmt::Debug {
        self.0.next().expect("truncated parity input").parse().expect("invalid parity number")
    }
    fn point(&mut self) -> Point { Point { x: self.n(), y: self.n() } }
    fn flag(&mut self) -> bool { self.n::<u8>() != 0 }
}
struct Script(Vec<f64>);
impl Controller for Script {
    fn steer(&mut self, _: &World, snake: SnakeView<'_>) -> Steering {
        Steering { desired_angle: self.0[snake.id as usize], rush: 0.0 }
    }
}

// Event hooks stay out of the production build and are inert unless replaying.
impl World {
    pub(super) fn parity_respawn(&mut self, i: usize) {
        if self.parity_record {
            self.events.push(format!(
                "{{\"type\":\"respawn\",\"tick\":{},\"snake\":{}}}", self.tick+1, i));
        }
    }
    pub(super) fn parity_growth(&mut self, i: usize) {
        if self.parity_record {
            self.events.push(format!(
                "{{\"type\":\"growth\",\"tick\":{},\"snake\":{},\"length\":{}}}",
                self.tick+1, i, self.snakes[i].len));
        }
    }
    pub(super) fn parity_eat(&mut self, owner: usize, f: Food) {
        if self.parity_record {
            self.events.push(format!(
                "{{\"type\":\"eat\",\"tick\":{},\"snake\":{},\"food\":{},\"value\":{}}}",
                self.tick+1, owner, f.id, f.value));
        }
    }
    pub(super) fn parity_collisions(&mut self) {
        if self.parity_record {
            self.parity_killers = (0..self.snakes.len()).map(|i| killers(self, i)).collect();
        }
    }
    pub(super) fn parity_death(&mut self, i: usize, reason: DeathReason, emitted: usize) {
        if !self.parity_record { return; }
        let candidates = &self.parity_killers[i];
        let killer = if candidates.len()==1 { candidates[0].to_string() } else { "null".into() };
        let reason = match reason {
            DeathReason::Wall => "wall",
            DeathReason::Head => "head",
            DeathReason::SelfHit => "self",
            _ => "body",
        };
        self.events.push(format!(
            concat!("{{\"type\":\"death\",\"tick\":{},\"snake\":{},\"reason\":\"{}\",",
                    "\"killer\":{},\"killerCandidates\":{:?},\"emittedFood\":{}}}"),
            self.tick+1, i, reason, killer, candidates, emitted));
    }
}

fn killers(w: &World, i: usize) -> Vec<usize> {
    let s = w.snakes[i];
    match s.dying {
        DeathReason::Wall | DeathReason::None => return vec![],
        DeathReason::SelfHit => return vec![i],
        _ => {}
    }
    let head = w.segments[i*MAX_SEGMENTS];
    let g = w.config.geometry();
    let mut result = vec![];
    for (j, other) in w.snakes.iter().enumerate() {
        if j==i || !other.alive { continue; }
        let (start, end, factor) = if s.dying==DeathReason::Head { (0, 1, 0.82) } else { (1, other.len, 0.78) };
        let reach2 = ((s.radius+other.radius)*factor).powi(2);
        if (start..end).any(|k| {
            let body = w.segments[j*MAX_SEGMENTS+k];
            g.distance2(head.current, body.current)<reach2 || g.segments_distance2(head.previous, head.current, body.previous, body.current)<reach2
        }) { result.push(j); }
    }
    result
}

fn emit(w: &mut World, out: &mut impl Write) -> io::Result<()> {
    write!(out, "{{\"tick\":{},\"simulationTime\":{},\"rngState\":{},\"rngDraws\":{},\"world\":[{},{}],\"nextFoodId\":{},\"nextFeastId\":{},\"snakes\":[", w.tick,w.time,w.rng.state(),w.rng.draws,w.config.width,w.config.height,w.next_food,w.next_feast)?;
    for (i, s) in w.snakes.iter().enumerate() {
        if i>0 { write!(out, ",")?; }
        write!(out, "{{\"index\":{},\"alive\":{},\"angle\":{},\"desiredAngle\":{},\"radius\":{},\"length\":{},\"growth\":{},\"growthStretch\":{},\"respawn\":{},\"segments\":[",i,s.alive,s.angle,s.desired,s.radius,s.len,s.growth,s.stretch,s.respawn)?;
        for j in 0..s.len {
            if j>0 { write!(out, ",")?; }
            let p = w.segments[i*MAX_SEGMENTS+j].current;
            write!(out, "[{},{}]",p.x,p.y)?;
        }
        write!(out, "]}}")?;
    }
    write!(out, "],\"food\":[")?;
    for (i, f) in w.food.iter().enumerate() {
        if i>0 { write!(out, ",")?; }
        write!(out,"[{},{},{},{},{},{}]",f.id,f.p.x,f.p.y,f.size,f.owner,f.attraction)?;
    }
    writeln!(out, "],\"events\":[{}]}}",w.events.join(","))?;
    w.events.clear();
    Ok(())
}

/// Replay the versioned whitespace numeric protocol produced by compare.py.
pub fn run(input: &str, out: &mut impl Write) -> io::Result<()> {
    let mut r = Input(input.split_whitespace());
    assert_eq!(r.n::<u32>(), 1, "unsupported numeric protocol");
    let config = Config { width:r.n(), height:r.n(), seed:r.n(), density:r.n(), trails:r.n(), scale:r.n(), speed:r.n(), intelligence:r.n(), palette_size:r.n(), self_collisions:r.flag(), deadly_walls:r.flag() };
    let ticks: u64 = r.n();
    let dt: f64 = r.n();
    let collision_only = r.flag();
    let mut w = World::new(config).unwrap();
    w.rng = WorldRng::restore(r.n(),r.n());
    w.time = r.n(); w.next_food=r.n(); w.next_feast=r.n(); w.growth_slots=r.n();
    let count: usize = r.n();
    w.snakes.truncate(count);
    for i in 0..count {
        let s = Snake { alive:r.flag(), respawn:r.n(), angle:r.n(), desired:r.n(), base_radius:r.n(), radius:r.n(), birth_len:r.n(), color:r.n(),
            traits: Traits { speed_bias:r.n(), turn_bias:r.n(), wander_phase:r.n(), aggression:r.n(), initial_brain_cooldown:r.n() },
            growth:r.n(), stretch:r.n(), blocked:r.flag(), rush:r.n(), score:r.n(), len:r.n(), ..Snake::default() };
        w.snakes[i]=s;
        for j in 0..s.len { w.segments[i*MAX_SEGMENTS+j]=Segment { current:r.point(), previous:r.point() }; }
        let start: usize = r.n(); let len: usize = r.n();
        for j in 0..len {
            let t = TrailPoint { p:r.point(), distance:r.n() };
            if j>=start { w.push_trail(i,t); }
        }
    }
    w.food.clear();
    let count: usize = r.n();
    for _ in 0..count {
        w.food.push(Food { id:r.n(), p:r.point(), value:r.n(), color:r.n(), size:r.n(), velocity:r.point(), life:r.n(), phase:r.n(), feast:r.n(), trail_index:r.n::<i64>() as u32, feast_len:r.n(), attraction:r.n(), target:r.point(), owner:r.n(), original_life:r.n() });
    }
    w.events.clear();
    w.parity_record = true;
    for i in 0..w.snakes.len() { w.events.push(format!("{{\"type\":\"spawn\",\"tick\":0,\"snake\":{i}}}")); }
    emit(&mut w,out)?;
    for tick in 1..=ticks {
        let sample = r.flag();
        let actions: usize = r.n();
        for _ in 0..actions {
            match r.n::<u8>() {
                1 => {
                    let width = r.n(); let height = r.n();
                    // Focused fixtures deliberately retain fewer slots than config.snake_count.
                    let old = w.config;
                    w.config.width=width; w.config.height=height;
                    w.scale_geometry(old.width,old.height); w.prepare_storage();
                    w.events.push(format!("{{\"type\":\"resize\",\"tick\":{tick},\"world\":[{width},{height}]}}"));
                },
                kind @ (2|3) => {
                    let snake: usize = r.n(); let segment: usize = r.n(); let p = r.point();
                    let s = &mut w.segments[snake*MAX_SEGMENTS+segment]; s.current=p;
                    if kind==3 { s.previous=p; }
                    w.events.push(if kind==2 { format!("{{\"type\":\"setHead\",\"tick\":{tick},\"snake\":{snake},\"position\":[{},{}]}}",p.x,p.y) } else { format!("{{\"type\":\"setSegment\",\"tick\":{tick},\"snake\":{snake},\"segment\":{segment},\"position\":[{},{}]}}",p.x,p.y) });
                },
                _ => panic!("unknown action")
            }
        }
        let mut script = Script((0..w.snakes.len()).map(|_| r.n()).collect());
        if collision_only {
            for i in 0..w.snakes.len() { if w.snakes[i].alive { w.snakes[i].desired=script.0[i]; } }
            w.mark_collisions();
            for i in 0..w.snakes.len() { if w.snakes[i].alive && w.snakes[i].dying!=DeathReason::None { w.explode_snake(i); } }
            w.tick+=1;
        } else {
            let generations = w.generations;
            w.step_seconds(&mut script,dt);
            for i in 0..w.snakes.len() {
                if w.generations[i]!=generations[i] && w.snakes[i].alive { w.snakes[i].desired=script.0[i]; }
            }
        }
        if sample { emit(&mut w,out)?; }
    }
    assert!(r.0.next().is_none(), "extra parity input");
    Ok(())
}
