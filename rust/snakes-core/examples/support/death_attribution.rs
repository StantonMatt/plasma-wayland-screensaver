// SPDX-License-Identifier: GPL-3.0-or-later
//! Observer-only death attribution. Requests are not activations; histories
//! follow (slot, generation), including the unmeasured warm-up. An episode is
//! a run of unsafe/escape decisions separated by at least one safe second.
use snakes_core::{ai::DesktopObservation, *};
use std::{fs::File, io::{BufWriter, Write}};
use super::item_lifecycle::{lifecycle, ItemLifecycle};

const WINDOW: u64 = 300;
const QUIET: u64 = 30;
const NAMES: [&str; 3] = ["short", "long", "giant"];

#[derive(Clone, Copy, Default)]
struct Request { tick: u64, kind: u8, slot: u16, escape: bool, proved: bool }
#[derive(Clone, Copy, Default)]
struct History {
    generation: u32,
    pending: Option<Request>,
    escape: [Option<u64>; 8],
    held: [Option<u64>; 8],
    proved: [Option<u64>; 8],
    episode_start: Option<u64>,
    last_danger: Option<u64>,
    previous_end: Option<u64>,
}
impl History {
    fn sync(&mut self, generation: u32) {
        if self.generation != generation { *self = Self { generation, ..Default::default() }; }
    }
    fn danger(&mut self, tick: u64) {
        if self.last_danger.is_none_or(|last| tick.saturating_sub(last) > QUIET) {
            self.previous_end = self.last_danger;
            self.episode_start = Some(tick);
        }
        self.last_danger = Some(tick);
    }
    fn event(&mut self, e: &FrameEvent, danger: bool, proved: u8) {
        self.sync(e.generation);
        let kind = e.other_snake_id as usize;
        match lifecycle(e) {
            ItemLifecycle::HeldRequest if kind < 8 => {
                let rescue = matches!(kind, 1 | 3 | 4 | 5 | 6);
                self.pending = Some(Request { tick: e.tick, kind: kind as u8, slot: e.cut_index,
                    escape: rescue && (danger || proved == kind as u8), proved: proved == kind as u8 });
            }
            ItemLifecycle::HeldActivation if kind < 8 => {
                self.held[kind] = Some(e.tick);
                if let Some(r) = self.pending.take().filter(|r| r.kind as usize == kind
                    && e.tick.saturating_sub(r.tick) == INVENTORY_WINDUP_TICKS as u64) {
                    if r.escape { self.escape[kind] = Some(e.tick); }
                    if r.proved { self.proved[kind] = Some(e.tick); }
                }
            }
            ItemLifecycle::Fizzle => {
                if let Some(r) = &mut self.pending {
                    if e.cut_index == r.slot { self.pending = None; }
                    else if e.cut_index < r.slot { r.slot -= 1; }
                }
            }
            _ => {}
        }
    }
}
fn recent(times: &[Option<u64>; 8], tick: u64) -> u8 {
    times.iter().enumerate().fold(0, |mask, (kind, at)|
        mask | if kind > 0 && at.is_some_and(|at| at<=tick && tick-at <= WINDOW) { 1 << kind } else { 0 })
}
// Same physical thresholds as length_metrics; retain the segment thresholds
// from death_leadups as a second column to make both existing schemes explicit.
fn physical(length: usize, radius: f64, speed: f64, height: f64) -> usize {
    let body = length.saturating_sub(1) as f64 * radius * 1.18;
    if body >= height * 2.0 { 2 } else if body / speed.max(1.0) > 4.0 * 2.4 { 1 } else { 0 }
}
fn segments(length: usize) -> usize { if length < 40 { 0 } else if length < 100 { 1 } else { 2 } }
fn age(tick: u64, at: Option<u64>) -> String {
    at.map_or_else(String::new, |at| format!("{:.6}", tick.saturating_sub(at) as f64 / 30.0))
}

pub struct Attribution {
    history: [History; MAX_SNAKES],
    pre: [Option<(u32, f64, f64)>; MAX_SNAKES],
    live: [[u64; 2]; 3],
    segment_live: [[u64; 2]; 3],
    start: u64,
    deaths: BufWriter<File>,
    activations: BufWriter<File>,
}
impl Attribution {
    pub fn new(prefix: &str, start: u64) -> Self {
        let mut deaths = BufWriter::new(File::create(format!("{prefix}.attribution.csv")).unwrap());
        writeln!(deaths, "tick,snake,generation,reason,length,cohort,segment_cohort,escape_mask_10s,held_mask_10s,proved_mask_10s,escape_age_s,episode_start_age_s,last_danger_age_s,previous_episode_end_age_s,episode_active").unwrap();
        let mut activations = BufWriter::new(File::create(format!("{prefix}.escape_activations.csv")).unwrap());
        writeln!(activations, "tick,snake,generation,kind,request_tick,escape,proved,measured").unwrap();
        Self { history: [History::default(); MAX_SNAKES], pre: [None; MAX_SNAKES],
            live: [[0; 2]; 3], segment_live: [[0; 2]; 3], start, deaths, activations }
    }
    pub fn before(&mut self, w: &World) {
        self.pre.fill(None);
        for s in w.snakes().filter(|s| s.alive) {
            let id = s.id as usize;
            self.history[id].sync(s.generation);
            let speed = w.motion_limits(id, 0.0).unwrap().0;
            self.pre[id] = Some((s.generation, s.radius, speed));

        }
    }
    pub fn after(&mut self, w: &World, obs: &[DesktopObservation; MAX_SNAKES], called: &[bool; MAX_SNAKES], proved: &[u8; MAX_SNAKES]) {
        let mut danger = [false; MAX_SNAKES];
        for id in 0..w.snake_count() {
            if called[id] && w.snake(id).is_some_and(|s| s.generation == obs[id].generation) {
                let d = obs[id];
                self.history[id].sync(d.generation);
                danger[id] = d.ordinary_danger || proved[id] != 0;
                if danger[id] { self.history[id].danger(w.tick()); }
            }
        }
        for e in w.frame_events().filter(|e| e.tick == w.tick() && (e.snake_id as usize) < MAX_SNAKES) {
            let id = e.snake_id as usize;
            // Do not let a dead earlier generation erase a replacement history.
            if w.snake(id).is_none_or(|s| s.generation != e.generation) { continue; }
            let h = &mut self.history[id];
            h.sync(e.generation);
            let request = h.pending;
            h.event(e, danger[id], proved[id]);
            if lifecycle(e) == ItemLifecycle::HeldActivation {
                let r = request.filter(|r| r.kind as u32 == e.other_snake_id
                    && e.tick.saturating_sub(r.tick) == INVENTORY_WINDUP_TICKS as u64);
                writeln!(self.activations, "{},{},{},{},{},{},{},{}", e.tick, id, e.generation,
                    e.other_snake_id, r.map_or(0, |r| r.tick), r.is_some_and(|r| r.escape),
                    r.is_some_and(|r| r.proved), e.tick > self.start).unwrap();
            }
        }
        // Assign each moved snake's live tick after completion events, at the
        // same tick used below for death masks. Include snakes dying this frame;
        // exclude replacements born after movement (no pre-movement record).
        if w.tick()>self.start {
            for id in 0..w.snake_count() {
                let Some((generation,radius,speed))=self.pre[id] else {continue;};
                let death=w.collision_events().find(|e|e.victim as usize==id && e.generation==generation);
                let length=death.map(|e|e.victim_length).or_else(||w.snake(id).filter(|s|s.generation==generation).map(|s|s.segments.len()));
                let Some(length)=length else {continue;};
                let h=&self.history[id];
                let post=usize::from(h.generation==generation && recent(&h.escape,w.tick())!=0);
                let c=physical(length,radius,speed,w.config().height);
                self.live[c][post]=self.live[c][post].saturating_add(1);
                let c=segments(length);
                self.segment_live[c][post]=self.segment_live[c][post].saturating_add(1);
            }
        }
        for e in w.collision_events().filter(|e| e.tick > self.start && matches!(e.reason, DeathReason::SelfHit | DeathReason::Head)) {
            let id = e.victim as usize;
            let h = if self.history[id].generation == e.generation { self.history[id] } else { History::default() };
            let (_, radius, speed) = self.pre[id].filter(|p| p.0 == e.generation).unwrap_or_else(|| {
                let s = w.snake(id).unwrap(); (e.generation, s.radius, w.motion_limits(id, 0.0).unwrap().0)
            });
            let c = physical(e.victim_length, radius, speed, w.config().height);
            let active = h.last_danger.is_some_and(|at| e.tick.saturating_sub(at) <= QUIET);
            let previous = if active { h.previous_end } else { h.last_danger };
            let last_escape = h.escape.iter().flatten().copied().max();
            writeln!(self.deaths, "{},{},{},{:?},{},{},{},{},{},{},{},{},{},{},{}", e.tick, id, e.generation,
                e.reason, e.victim_length, NAMES[c], NAMES[segments(e.victim_length)],
                recent(&h.escape, e.tick), recent(&h.held, e.tick), recent(&h.proved, e.tick),
                age(e.tick, last_escape), age(e.tick, h.episode_start), age(e.tick, h.last_danger),
                age(e.tick, previous), active).unwrap();
        }
    }
    pub fn print(&mut self) {
        self.deaths.flush().unwrap(); self.activations.flush().unwrap();
        println!("attribution_exposure live_ticks={:?} segment_live_ticks={:?} columns=no_escape_10s,escape_10s", self.live, self.segment_live);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(tick: u64, kind: EventKind, generation: u32, item: u32) -> FrameEvent {
        FrameEvent { tick, kind, generation, other_snake_id: item, duration_ticks: 4,
            flags: if kind == EventKind::Pickup { event_flags::HELD_ACTIVATION } else { 0 }, ..Default::default() }
    }
    #[test]
    fn completion_not_request_starts_window_and_replacement_cannot_inherit_it() {
        let mut h = History::default();
        h.event(&event(10, EventKind::Use, 1, 6), true, 6);
        assert_eq!(recent(&h.escape, 10), 0);
        h.event(&event(14, EventKind::Pickup, 1, 6), false, 0);
        assert_eq!(recent(&h.escape, 314), 1 << 6);
        assert_eq!(recent(&h.proved, 314), 1 << 6);
        assert_eq!(recent(&h.escape, 315), 0);
        h.sync(2); assert_eq!(recent(&h.escape, 14), 0);
    }
    #[test]
    fn fizzled_and_offensive_requests_are_not_escape_completions() {
        for (danger, fizzle) in [(false, false), (true, true)] {
            let mut h = History::default();
            h.event(&event(10, EventKind::Use, 1, 4), danger, 0);
            if fizzle { h.event(&event(12, EventKind::Fizzle, 1, 4), false, 0); }
            h.event(&event(14, EventKind::Pickup, 1, 4), false, 0);
            assert_eq!(recent(&h.escape, 14), 0);
            assert_eq!(recent(&h.held, 14), 1 << 4);
        }
    }
    #[test]
    fn short_safe_gaps_do_not_replace_the_previous_episode() {
        let mut h = History::default();
        h.danger(10); h.danger(20); h.danger(50);
        assert_eq!(h.episode_start, Some(10)); assert_eq!(h.previous_end, None);
        h.danger(81); assert_eq!(h.episode_start, Some(81)); assert_eq!(h.previous_end, Some(50));
        h.sync(3); assert_eq!(h.previous_end, None);
    }
    #[test]
    fn unrelated_shelf_fizzle_compacts_request_without_erasing_it() {
        let mut h = History::default();
        h.event(&FrameEvent { cut_index: 1, ..event(10, EventKind::Use, 1, 4) }, true, 4);
        h.event(&event(11, EventKind::Fizzle, 1, 3), false, 0);
        assert_eq!(h.pending.unwrap().slot, 0);
        h.event(&event(14, EventKind::Pickup, 1, 4), false, 0);
        assert_eq!(recent(&h.escape, 14), 1 << 4);
    }
    #[test]
    fn completion_and_window_expiry_share_the_movement_exposure_tick() {
        let mut h=History::default();
        h.event(&event(10,EventKind::Use,1,5),true,5);
        let prefix=std::env::temp_dir().join(format!("escape-attribution-{}",std::process::id()));
        let mut a=Attribution::new(prefix.to_str().unwrap(),0);
        let mut w=World::diagnostic_arena(Config {density:0.0,world_events:false,..Default::default()},
            &[(Point{x:500.0,y:500.0},0.0,20,0.0)],&[]).unwrap();
        struct Straight;
        impl snakes_core::controller::Controller for Straight {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->snakes_core::controller::Steering {
                snakes_core::controller::Steering {desired_angle:s.angle,rush:0.0}
            }
        }
        // Completion is injected into the observed history at the completed
        // movement tick, just as after processes World activation events.
        for _ in 0..13 {w.step(&mut Straight);}
        a.before(&w);w.step(&mut Straight);
        h.event(&event(14,EventKind::Pickup,1,5),false,0);a.history[0]=h;
        a.after(&w,&[DesktopObservation::default();MAX_SNAKES],&[false;MAX_SNAKES],&[0;MAX_SNAKES]);
        assert_eq!(a.live[0],[0,1],"completion movement is exposed, including a same-frame death");
        assert_eq!(recent(&h.escape,w.tick()),1<<5,"death mask uses this same completed tick");
        for _ in 14..314 {w.step(&mut Straight);}
        a.before(&w);w.step(&mut Straight);
        a.after(&w,&[DesktopObservation::default();MAX_SNAKES],&[false;MAX_SNAKES],&[0;MAX_SNAKES]);
        assert_eq!(recent(&h.escape,w.tick()),0);
        assert_eq!(a.live[0],[1,1],"expiry movement and death mask are both unexposed");
        assert_eq!(recent(&h.escape,13),0,"future completions cannot expose prior movements");
        drop(a);
        for suffix in ["attribution.csv","escape_activations.csv"] {std::fs::remove_file(format!("{}.{}",prefix.display(),suffix)).unwrap();}
    }

    #[test]
    fn same_movement_completion_and_head_death_have_matching_exposure() {
        let prefix=std::env::temp_dir().join(format!("escape-attribution-death-{}",std::process::id()));
        let mut a=Attribution::new(prefix.to_str().unwrap(),0);
        let mut w=World::diagnostic_arena(Config {density:0.0,world_events:false,deadly_walls:false,..Default::default()},
            &[(Point{x:500.0,y:500.0},0.0,20,0.0),
              (Point{x:510.0,y:500.0},std::f64::consts::PI,20,0.0)],&[]).unwrap();
        a.before(&w);
        struct Straight;
        impl snakes_core::controller::Controller for Straight {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->snakes_core::controller::Steering {
                snakes_core::controller::Steering {desired_angle:s.angle,rush:0.0}
            }
        }
        w.step(&mut Straight);
        assert!(w.collision_events().any(|e|e.victim==0 && e.reason==DeathReason::Head));
        // after records a completion before both exposure and death emission.
        a.history[0].escape[5]=Some(w.tick());a.history[0].held[5]=Some(w.tick());
        a.after(&w,&[DesktopObservation::default();MAX_SNAKES],&[false;MAX_SNAKES],&[0;MAX_SNAKES]);
        assert_eq!(a.live[0][1],1,"dead mover contributes its exposed tick");
        a.print();drop(a);
        let rows=std::fs::read_to_string(format!("{}.attribution.csv",prefix.display())).unwrap();
        let row=rows.lines().skip(1).find(|l|l.split(',').nth(1)==Some("0")).unwrap();
        assert_eq!(row.split(',').nth(7),Some("32"),"death numerator agrees with exposed denominator");
        for suffix in ["attribution.csv","escape_activations.csv"] {std::fs::remove_file(format!("{}.{}",prefix.display(),suffix)).unwrap();}
    }

}
