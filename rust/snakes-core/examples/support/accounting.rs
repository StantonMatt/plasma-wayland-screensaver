// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact event accounting shared by the benchmark observers.
/// Byte-indexed effect telemetry includes disabled kinds, so enabling one
/// cannot truncate pickup counts or fold its diagnostics into another effect.
pub const EFFECT_KIND_COUNT:usize=snakes_core::effects::EffectKind::Whirlpool as usize+1;
use snakes_core::{ai::AiController, controller::{Controller, Steering}, CollisionEvent, DeathReason, SnakeView, World, MAX_SNAKES};
use super::diagnostics::ScoreController;

#[derive(Clone, Copy, Default)]
pub struct Tactics { pub generation: u32, pub hunting: bool, pub staged: bool, pub coil: bool, pub target: u64, pub prey: Option<usize> }
impl Tactics {
    pub fn observe(ai: &AiController, s: SnakeView<'_>) -> Self {
        let mut result=tactics_from_debug(s.generation,ai.debug(s.id as usize).unwrap().generation,
            ai.competition_debug(s.id as usize).unwrap());
        if result.generation==s.generation {result.target=ai.selected_target(s.id as usize).unwrap();}
        result
    }
}
fn tactics_from_debug(generation:u32,controller_generation:u32,d:snakes_core::ai::CompetitionDebug)->Tactics {
    if controller_generation!=generation {return Tactics::default();}
    Tactics {generation,hunting:d.prey.is_some(),staged:d.prey.is_some() && d.attack_stage!=0,
        coil:d.prey.is_some() && d.coil_radius>0.0,target:0,prey:d.prey}
}
// Sample after each actual control call, before movement or deaths. Dead slots
// that respawn during mechanics receive no control call and no tactical label.
pub struct Observed<C> { pub inner: C, pub tactics: [Tactics; MAX_SNAKES] }
impl<C> Observed<C> {
    pub fn new(inner: C) -> Self { Self { inner, tactics: [Tactics::default(); MAX_SNAKES] } }
    pub fn begin(&mut self) { self.tactics.fill(Tactics::default()); }
}
impl<C: ScoreController> Controller for Observed<C> {
    fn face_intent(&self,id:u32)->snakes_core::controller::FaceIntent {self.inner.face_intent(id)}
    fn intent_flags(&self, id: u32) -> Option<u32> { self.inner.intent_flags(id) }
    fn steer(&mut self, w: &World, s: SnakeView<'_>) -> Steering {
        let result = self.inner.steer(w, s);
        if let Some(ai) = self.inner.ai() { self.tactics[s.id as usize] = Tactics::observe(ai, s); }
        result
    }
}
impl<C: ScoreController> ScoreController for Observed<C> {
    fn ai(&self) -> Option<&AiController> { self.inner.ai() }
}
#[derive(Default)]
pub struct CombatTotals {
    pub deaths: [u64; 5], pub opponent_kills: u64, pub ambiguous_kills: u64,
    pub bigger_kills: u64, pub smaller_kills: u64, pub hunting_kills: u64,
    pub staged_kills: u64, pub attack_deaths: u64, pub staged_deaths: u64,
}
impl CombatTotals {
    pub fn record(&mut self, e: &CollisionEvent, tactics: &[Tactics; MAX_SNAKES]) {
        let id = e.victim as usize;
        let victim = tactics[id];
        if victim.generation == e.generation {
            self.attack_deaths += u64::from(victim.hunting);
            self.staged_deaths += u64::from(victim.staged);
        }
        let opponents = e.owner_mask & !(1 << id);
        if opponents != 0 {
            self.opponent_kills += 1;
            if opponents.count_ones() > 1 { self.ambiguous_kills += 1; }
            else {
                let owner = opponents.trailing_zeros() as usize;
                let label = tactics[owner];
                if label.generation == e.owner_generations[owner] {
                    self.hunting_kills += u64::from(label.hunting);
                    self.staged_kills += u64::from(label.staged);
                }
                if e.owner_lengths[owner] >= e.victim_length + 4 { self.bigger_kills += 1; }
                else if e.owner_lengths[owner] + 4 <= e.victim_length { self.smaller_kills += 1; }
            }
        }
        match e.reason {
            DeathReason::Wall => self.deaths[0] += 1,
            DeathReason::Body => self.deaths[1] += 1,
            DeathReason::SelfHit => self.deaths[2] += 1,
            DeathReason::Head => {
                let lost = opponents.count_ones() == 1 && e.owner_lengths[opponents.trailing_zeros() as usize] >= e.victim_length + 4;
                self.deaths[if lost { 3 } else { 4 }] += 1;
            }
            DeathReason::None => {}
        }
    }
}

#[derive(Default)]
pub struct DuelOutcome {
    pub dead: [bool; 3], pub kills: usize, pub other_kills: usize,
    pub attacker_deaths: usize, pub victim_self: usize, pub victim_wall: usize,
}
impl DuelOutcome {
    /// First-generation designated duel; unrelated deaths cannot finish it.
    pub fn record(&mut self, e: &CollisionEvent, attacker: usize, victim: usize) -> bool {
        if e.generation != 1 { return false; }
        let id = e.victim as usize;
        self.dead[id] = true;
        if id == attacker { self.attacker_deaths += 1; }
        if id == victim {
            match e.reason { DeathReason::SelfHit => self.victim_self += 1, DeathReason::Wall => self.victim_wall += 1, _ => {} }
        }
        let killed = id != attacker && e.owner_mask == (1 << attacker) && e.owner_generations[attacker] == 1;
        if killed { if id == victim { self.kills += 1; } else { self.other_kills += 1; } }
        killed && id == victim
    }
    pub fn finished(&self, attacker: usize, victim: usize) -> bool { self.dead[attacker] || self.dead[victim] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snakes_core::{Config, ai::CompetitionDebug};
    #[test]
    fn unrelated_death_does_not_finish_or_credit_designated_duel() {
        for attacker in [0,1] {
            let victim=1-attacker;
            let mut outcome=DuelOutcome::default();
            let mut e=CollisionEvent {victim:2,generation:1,reason:DeathReason::Body,owner_mask:1<<attacker,..Default::default()};
            e.owner_generations[attacker]=1;
            assert!(!outcome.record(&e,attacker,victim));
            assert_eq!(outcome.kills,0);assert_eq!(outcome.other_kills,1);
            assert!(!outcome.finished(attacker,victim));
            assert_eq!(outcome.victim_wall,0);
            e.victim=victim as u32;
            assert!(outcome.record(&e,attacker,victim));
            assert_eq!(outcome.kills,1);assert!(outcome.finished(attacker,victim));
            e.generation=2;
            assert!(!outcome.record(&e,attacker,victim));assert_eq!(outcome.kills,1);
        }
    }
    #[test]
    fn scorecard_counts_respawn_tick_deaths_without_old_generation_tactics() {
        // An arena too crowded for guaranteed spawn clearance reproducibly
        // respawns and collides without making a controller call for that life.
        let mut w=World::new(Config {width:80.0,height:80.0,scale:1000.0,density:100.0,self_collisions:true,seed:73,..Default::default()}).unwrap();
        let mut ai=Observed::new(AiController::new());
        let mut totals=CombatTotals::default();let mut respawn_deaths=0;
        for _ in 0..360 {
            let before:Vec<_>=w.snakes().map(|s|(s.alive,s.generation)).collect();
            ai.begin();w.step(&mut ai);
            for e in w.collision_events() {
                if !before[e.victim as usize].0 && before[e.victim as usize].1!=e.generation {
                    respawn_deaths+=1;
                    assert_eq!(ai.tactics[e.victim as usize].generation,0);
                }
                totals.record(e,&ai.tactics);
            }
        }
        assert!(respawn_deaths>0);
        assert_eq!(totals.deaths.iter().sum::<u64>(),w.stats().deaths);
        let mut e=CollisionEvent {victim:0,generation:2,reason:DeathReason::Body,owner_mask:2,..Default::default()};
        e.owner_generations[1]=2;
        let mut labels=[Tactics::default();MAX_SNAKES];
        labels[0]=Tactics {generation:1,hunting:true,staged:true,coil:false,target:0,prey:None};labels[1]=labels[0];
        let mut totals=CombatTotals::default();totals.record(&e,&labels);
        assert_eq!(totals.deaths[1],1);assert_eq!(totals.opponent_kills,1);
        assert_eq!((totals.attack_deaths,totals.hunting_kills,totals.staged_deaths,totals.staged_kills),(0,0,0,0));
    }
    #[test]
    fn prey_death_transitions_to_harvesting_without_hunting_time() {
        struct WallPrey(AiController);
        impl ScoreController for WallPrey {fn ai(&self)->Option<&AiController> {Some(&self.0)}}
        impl Controller for WallPrey {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.0.steer(w,s)} else {Steering {desired_angle:0.0,rush:1.0}}
            }
        }
        let mut w=World::diagnostic_arena(Config {width:1600.0,height:1000.0,density:0.0,intelligence:100.0,deadly_walls:true,..Default::default()},
            &[(snakes_core::Point{x:1400.0,y:400.0},0.0,72,1.0),(snakes_core::Point{x:1580.0,y:400.0},0.0,24,0.6)],&[]).unwrap();
        let mut c=Observed::new(WallPrey(AiController::new()));
        let mut hunted=false;let mut harvested=false;
        for _ in 0..40 {
            c.begin();w.step(&mut c);
            hunted|=c.tactics[0].hunting;
            if !w.snake(1).unwrap().alive && w.snake(0).unwrap().alive {
                c.begin();w.step(&mut c);
                assert!(!c.tactics[0].hunting,"prey death must end hunting on the next control call");
                if c.inner.0.debug(0).unwrap().flags&8!=0 {harvested=true;break;}
            }
        }
        assert!(hunted);assert!(harvested,"fixture must exercise bit 8 during corpse harvesting");
    }
    #[test]
    fn hunting_labels_require_current_prey_after_death_and_recovery() {
        // The observer's bit 8 deliberately remains set in both states. This
        // fixture models the public observations immediately after cancellation.
        let label=|d:CompetitionDebug| tactics_from_debug(1,1,d);
        let hunt=CompetitionDebug {prey:Some(1),attack_stage:1,..Default::default()};
        assert!(label(hunt).hunting);
        let harvesting=CompetitionDebug {prey:None,..hunt};
        assert!(!label(harvesting).hunting);assert!(!label(harvesting).staged);
        let recovery=CompetitionDebug {prey:None,attack_stage:0,..hunt};
        assert!(!label(recovery).hunting);
        assert!(!tactics_from_debug(2,1,hunt).hunting,"new lives cannot inherit hunting labels");
        let mut labels=[Tactics::default();MAX_SNAKES];labels[0]=label(harvesting);labels[1]=label(recovery);
        let mut e=CollisionEvent {victim:0,generation:1,reason:DeathReason::Body,owner_mask:2,..Default::default()};e.owner_generations[1]=1;
        let mut totals=CombatTotals::default();totals.record(&e,&labels);
        assert_eq!((totals.attack_deaths,totals.hunting_kills),(0,0));
    }
}

#[cfg(test)]
mod face_forwarding_tests {
    use super::*;
    #[test]
    fn observation_wrapper_preserves_production_face_intent() {
        struct Policy;
        impl ScoreController for Policy {fn ai(&self)->Option<&AiController> {None}}
        impl Controller for Policy {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering{desired_angle:s.angle,rush:0.0}}
            fn face_intent(&self,_:u32)->snakes_core::controller::FaceIntent {
                snakes_core::controller::FaceIntent{target_id:77,prey:2,has_target:true,..Default::default()}
            }
        }
        let observed=Observed::new(Policy);let face=observed.face_intent(0);
        assert_eq!(face.target_id,77);assert_eq!(face.prey,2);assert!(face.has_target);
    }
}
