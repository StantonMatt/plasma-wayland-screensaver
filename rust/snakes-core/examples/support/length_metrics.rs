// SPDX-License-Identifier: GPL-3.0-or-later
//! Observer-only exposure metrics. The nominal lookahead is 72 / 30 seconds.
//! Body length is estimated from physical spacing, not from a segment cohort.
//! The seeded giant is tracked by slot AND generation: its replacement cannot
//! conceal an early death in an otherwise thirty-minute fixture.
use snakes_core::{CollisionEvent, DeathReason, World, MAX_SNAKES};

pub struct LengthMetrics {
    seeded: Option<(u32, u32)>,
    pre_cohort: [Option<(u32,usize)>; MAX_SNAKES],
    newborn_deaths: [u64; 5],
    live: [u64; 3],
    deaths: [[u64; 5]; 3],
    giant_live: u64,
    giant_deaths: [u64; 5],
    giant_death_tick: u64,
    other_live: u64,
    other_deaths: u64,
}
impl LengthMetrics {
    pub fn new(w: &World, giant: Option<usize>) -> Self {
        Self {
            seeded: giant.map(|id| (id as u32, w.snake(id).unwrap().generation)),
            pre_cohort: [None; MAX_SNAKES], newborn_deaths: [0; 5], live: [0; 3], deaths: [[0; 5]; 3],
            giant_live: 0, giant_deaths: [0; 5], giant_death_tick: 0,
            other_live: 0, other_deaths: 0,
        }
    }
    pub fn before(&mut self, w: &World) {
        self.pre_cohort.fill(None);
        for s in w.snakes().filter(|s| s.alive) {
            let body = s.segments.len().saturating_sub(1) as f64 * s.radius * 1.18;
            let speed = w.motion_limits(s.id as usize, 0.0).unwrap().0.max(1.0);
            let cohort = if body >= w.config().height * 2.0 { 2 }
                else if body / speed > 4.0 * 2.4 { 1 } else { 0 };
            self.pre_cohort[s.id as usize] = Some((s.generation,cohort));
            self.live[cohort] += 1;
            if self.seeded == Some((s.id, s.generation)) { self.giant_live += 1; }
            else { self.other_live += 1; }
        }
    }
    pub fn after(&mut self, w: &World) {
        for e in w.collision_events() {self.record_death(w,e);}
    }
    fn record_death(&mut self,w:&World,e:&CollisionEvent) {
            let cohort=match self.pre_cohort[e.victim as usize].filter(|(generation,_)|*generation==e.generation) {
                Some((_,cohort))=>cohort,
                None=>{
                    self.newborn_deaths[e.reason as usize]+=1;
                    let s=w.snake(e.victim as usize).unwrap();
                    let body=e.victim_length.saturating_sub(1) as f64*s.radius*1.18;
                    let speed=w.motion_limits(e.victim as usize,0.0).unwrap().0.max(1.0);
                    if body>=w.config().height*2.0 {2} else if body/speed>4.0*2.4 {1} else {0}
                }
            };
            self.deaths[cohort][e.reason as usize] += 1;
            if self.seeded == Some((e.victim, e.generation)) {
                self.giant_deaths[e.reason as usize] += 1;
                self.giant_death_tick = e.tick;
            } else { self.other_deaths += 1; }
    }
    pub fn print(&self) {
        println!("length_newborn_deaths={:?}",self.newborn_deaths);
        let rates = self.deaths.map(|row| row.iter().sum::<u64>())
            .into_iter().zip(self.live).map(|(n,t)| n as f64*1800.0/t.max(1) as f64)
            .collect::<Vec<_>>();
        println!("length_cohorts nominal_lookahead_s=2.4 spacing_estimate=true live_ticks={:?} deaths={:?} deaths_per_live_snake_min={:?}", self.live, self.deaths, rates);
        if self.seeded.is_some() {
            println!("seeded_giant live_ticks={} live_s={:.3} deaths={:?} self_deaths={} death_tick={} other_live_ticks={} other_deaths={} other_deaths_per_live_snake_min={:.6}",
                self.giant_live, self.giant_live as f64/30.0, self.giant_deaths,
                self.giant_deaths[DeathReason::SelfHit as usize], self.giant_death_tick,
                self.other_live, self.other_deaths, self.other_deaths as f64*1800.0/self.other_live.max(1) as f64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_death_uses_newborn_length_and_not_giant_exposure() {
        let w=World::new(snakes_core::Config::default()).unwrap();
        let generation=w.snake(0).unwrap().generation;
        let mut lengths=LengthMetrics::new(&w,Some(0));
        lengths.pre_cohort[0]=Some((generation,2));lengths.live[2]=1;
        lengths.record_death(&w,&CollisionEvent {victim:0,generation:generation+1,victim_length:4,reason:DeathReason::Body,tick:2,..Default::default()});
        assert_eq!(lengths.deaths[0][DeathReason::Body as usize],1);
        assert_eq!(lengths.deaths[2],[0;5]);
        assert_eq!(lengths.newborn_deaths[DeathReason::Body as usize],1);
        assert_eq!(lengths.giant_deaths,[0;5]);
        assert_eq!(lengths.other_deaths,1);
        assert_eq!(lengths.live,[0,0,1]);
    }
}
