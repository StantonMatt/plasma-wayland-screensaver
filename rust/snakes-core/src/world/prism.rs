// SPDX-License-Identifier: GPL-3.0-or-later
//! V2-only feature scheduler: one timer draw, then twelve XY candidate pairs
//! on an eligible spawn (reachable race placement), then add_food's phase draw.
//! Classic never enters this module's scheduler and consumes no new RNG draws.
use super::*;
impl Food {
    #[inline]
    pub(crate) fn pickup_eligible(&self,tick:u64)->bool {
        Item::endpoint_eligible(tick,self.ripe_tick,0,usize::MAX,true)
    }
}
impl World {
    pub(super) fn reset_prism_timer(&mut self) {self.prism_timer=1200+(self.rng.random()*601.0) as u16;}
    pub(super) fn advance_prism(&mut self) {
        for i in (0..self.food.len()).rev() {
            let f=&mut self.food[i];
            if f.kind==FoodKind::Prism && f.owner<0 && f.ripe_tick!=0 {
                let remaining=(f.ripe_tick+900).saturating_sub(self.tick+1);
                if remaining==0 {self.food.remove(i);continue;}
                f.life=remaining as f64/30.0;
            }
            if f.kind!=FoodKind::PrismSeed {continue;}
            f.motion_ticks=f.ripe_tick.saturating_sub(self.tick+1).min(90) as u16;
            if f.motion_ticks==0 {f.kind=FoodKind::Prism;f.life=30.0;f.original_life=30.0;}
        }
        self.prism_timer=self.prism_timer.saturating_sub(1);
        if self.prism_timer!=0 {return;}
        if self.food.len()<MAX_FOOD && !self.food.iter().any(|f|matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed)) {
            if let Some(p)=self.prize_spawn_location(true) {
                self.add_food(Food {p,kind:FoodKind::PrismSeed,value:5.0,life:33.0,
                    ripe_tick:self.tick+1+90,motion_ticks:90,..Food::default()});
                // Large planned prize, unlike the tiny nutritional particles.
                self.food.last_mut().unwrap().size=self.config.base_radius()*0.62;
            }
        }
        self.reset_prism_timer();
    }
    pub(super) fn gulp(&mut self,id:usize)->u16 {
        let duration=((self.snakes[id].len as f64/26.0).clamp(1.0,3.6)*30.0).round() as u16;
        let b=&mut self.faces[id].bulges;
        let slot=b.iter().position(|b|b.duration_ticks==0 || self.tick+1>=b.start_tick+b.duration_ticks as u64)
            .unwrap_or(if b[0].start_tick<=b[1].start_tick {0} else {1});
        b[slot]=Bulge {start_tick:self.tick+1,duration_ticks:duration,origin_segment:0,strength:0.35};
        duration
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_placement_announces_a_reachable_race_without_extra_rng_draws() {
        for (scale,speed) in [(100.0,100.0),(185.0,230.0)] {
            let mut w=World::diagnostic_arena(Config {width:3440.0,height:1440.0,
                density:0.0,rules:RuleSet::V2,scale,speed,intelligence:100.0,
                self_collisions:true,deadly_walls:true,..Config::default()},&[
                (Point{x:1500.0,y:650.0},0.0,24,0.9),
                (Point{x:1800.0,y:750.0},std::f64::consts::PI,24,0.9)],&[]).unwrap();
            let mut ordinary=w.diagnostic_snapshot();
            let _=ordinary.item_spawn_location();
            let p=w.prize_spawn_location(true).unwrap();
            assert_eq!(w.rng.random(),ordinary.rng.random(),"same twelve XY draws");
            let eta=w.snakes().filter(|s|s.alive).map(|s| {
                let (speed,turn)=w.motion_limits(s.id as usize,0.0).unwrap();
                let d=w.displacement(s.segments[0].current,p);
                let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                (d.x*d.x+d.y*d.y).sqrt()/speed+bearing/turn
            }).fold(f64::INFINITY,f64::min);
            assert!(eta<2.0,"at least one early arrival: scale={scale} eta={eta}");
            assert!(p.x>=10.0*w.config.base_radius() && p.y>=10.0*w.config.base_radius());
        }
    }
    #[test]
    fn seed_ripens_at_exact_endpoint_and_prism_feasts_with_one_denial() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,self_collisions:false,..Config::default()},
            &[(Point{x:400.0,y:300.0},0.0,48,0.9),(Point{x:450.0,y:300.0},0.0,24,0.9)],&[]).unwrap();
        w.food.clear();w.prism_timer=0;w.advance_prism();
        let f=*w.food.last().unwrap();assert_eq!(f.kind,FoodKind::PrismSeed);assert_eq!(f.value,5.0);
        assert_eq!(f.ripe_tick,91);assert!(!f.pickup_eligible(90));assert!(f.pickup_eligible(91));
        w.tick=89;w.advance_prism();assert_eq!(w.food[0].motion_ticks,1);
        w.tick=90;w.advance_prism();assert_eq!(w.food[0].kind,FoodKind::Prism);assert_eq!(w.food[0].life,30.0);
        w.food[0].p=Point{x:420.0,y:300.0};w.faces[1].target_id=f.id | (1<<63);
        w.consume_food(0,0);assert_eq!(w.faces[0].happy_ticks,45);assert_eq!(w.faces[1].angry_ticks,78);
        assert!(w.frame_events().any(|e|e.kind==EventKind::Feast));assert_eq!(w.faces[0].bulges[0].duration_ticks,55);
        w.gulp(0);w.gulp(0);assert_eq!(w.faces[0].bulges.iter().filter(|b|b.strength>0.0).count(),2);
    }
    #[test]
    fn prism_expires_at_900_ticks_and_only_head_on_wins_gulp() {
        for rules in [RuleSet::V2,RuleSet::Classic] {
            for len in [24,48] {
                let mut w=World::diagnostic_arena(Config {rules,density:0.0,self_collisions:false,..Config::default()},
                    &[(Point{x:400.0,y:300.0},0.0,len,0.9),(Point{x:400.0,y:300.0},3.14,24,0.9)],&[]).unwrap();
                w.mark_collisions();
                assert_eq!(w.faces[0].bulges[0].strength>0.0,rules==RuleSet::V2 && len==48);
                assert_eq!(w.faces[1].bulges[0].strength,0.0);
            }
        }
        let mut w=World::new(Config {rules:RuleSet::V2,..Config::default()}).unwrap();w.food.clear();
        w.food.push(Food {id:88,kind:FoodKind::Prism,ripe_tick:90,life:30.0,original_life:30.0,owner:-1,..Default::default()});
        w.tick=988;w.advance_prism();assert_eq!(w.food[0].life,1.0/30.0);
        w.tick=989;w.advance_prism();assert!(w.food.is_empty());
    }
    #[test]
    fn prism_scheduler_exclusivity_settings_and_gulp_limits() {
        let mut w=World::new(Config {rules:RuleSet::V2,..Config::default()}).unwrap();
        assert!((1200..=1800).contains(&w.prism_timer));
        w.prism_timer=0;w.advance_prism();let count=w.food.len();w.prism_timer=0;w.advance_prism();assert_eq!(w.food.len(),count);
        for (len,ticks) in [(12,30),(26,30),(100,108),(1600,108)] {w.snakes[0].len=len;assert_eq!(w.gulp(0),ticks);}
        w.reconfigure(Config {power_ups:false,..w.config}).unwrap();
        assert!(!w.food.iter().any(|f|matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed)));
    }
}
