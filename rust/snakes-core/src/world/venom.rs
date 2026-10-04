// SPDX-License-Identifier: GPL-3.0-or-later
//! One bite rule for mechanics and forecasts. Detached nutrition has separate
//! storage from food so it cannot be claimed, evicted or eaten before release.
use super::*;
pub(crate) const WRIGGLE_TICKS:u16=33;
const TAIL_CAPACITY:usize=MAX_SEGMENTS/2;
#[derive(Clone,Copy,Default)]
pub(crate) struct DetachedTail {pub release_tick:u64,pub len:usize,pub color:u32}
#[inline]
pub(crate) fn bite_eligible(rules:RuleSet,kind:u8,ticks:u16,own:usize,other:usize,
    cut:usize,len:usize,immunity:u16,phased:bool)->bool {
    rules==RuleSet::V2 && own!=other && kind==effects::EffectKind::Venom as u8 && ticks>0
        && !phased && immunity==0 && cut>3 && cut>len/2 && cut<len
}
impl World {
    pub(super) fn sever_tail(&mut self,biter:usize,victim:usize,mut cut:usize,biter_radius:f64) {
        let s=self.snakes[victim];
        // A swept strike can touch adjacent points in several grid buckets.
        // Cut at the deepest eligible contact, not the arbitrary first bucket:
        // otherwise an already-touched neighbour becomes a lethal fresh stump.
        let g=self.config.geometry();let head=self.segments[biter*MAX_SEGMENTS];
        for j in (s.len/2+1).max(4)..cut {
            let body=taper::body_radius(s.radius,j as f64,s.len);
            let reach=taper::contact_radius(self.config.rules,biter_radius,body,false);
            if Self::swept_hit(g,head,self.segments[victim*MAX_SEGMENTS+j],reach) {cut=j;break;}
        }
        let len=s.len-cut;
        debug_assert!(len<=TAIL_CAPACITY && self.detached[victim].len==0);
        let start=victim*TAIL_CAPACITY;
        for j in 0..len {self.detached_points[start+j]=self.segments[victim*MAX_SEGMENTS+cut+j].current;}
        let endpoint=self.tick.wrapping_add(1);
        self.detached[victim]=DetachedTail {release_tick:endpoint+WRIGGLE_TICKS as u64,len,color:s.color};
        self.snakes[victim].len=cut;
        // Keep the existing trail: shortening must never teleport the head.
        Self::update_radius(&mut self.snakes[victim]);
        self.snakes[biter].effect_kind=0;self.snakes[biter].effect_ticks=0;
        self.faces[biter].happy_ticks=45;
        let f=&mut self.faces[victim];
        f.bite_immunity_ticks=60;f.stump_ticks=48;f.angry_ticks=78;
        f.grudge_id=biter as u32;f.grudge_generation=self.snakes[biter].generation;f.grudge_ticks=150;
        self.emit_bubble(victim,Glyph::Anger);
        self.push_event(FrameEvent {tick:endpoint,position:self.segments[victim*MAX_SEGMENTS+cut].current,
            snake_id:victim as u32,other_snake_id:biter as u32,color_index:s.color,kind:EventKind::Sever,
            cut_index:cut as u16,duration_ticks:WRIGGLE_TICKS,generation:s.generation,
            other_generation:self.snakes[biter].generation,value:len as f32,release_tick:endpoint+WRIGGLE_TICKS as u64});
    }
    pub(super) fn release_detached(&mut self) {
        for id in 0..self.snakes.len() {
            let tail=self.detached[id];
            if tail.len==0 || self.tick+1<tail.release_tick {continue;}
            self.detached[id]=DetachedTail::default();
            let intended=tail.len.min(360);
            self.make_room_for_death_food(intended.min(48));
            let emitted=intended.min(self.config.maximum_food().saturating_sub(self.food.len()));
            let feast=self.next_feast;self.next_feast=self.next_feast.wrapping_add(1);
            for j in 0..emitted {
                let p=self.detached_points[id*TAIL_CAPACITY+j*(tail.len-1)/emitted.saturating_sub(1).max(1)];
                let angle=self.rng.random()*TAU;let force=22.0+self.rng.random()*90.0;let life=18.0+self.rng.random()*16.0;
                self.add_food(Food {p,value:tail.len as f64/emitted as f64,color:tail.color,
                    velocity:Point{x:angle.cos()*force,y:angle.sin()*force},life,
                    feast,kind:FoodKind::Shard,trail_index:j as u32,feast_len:emitted as u32,..Food::default()});
            }
        }
    }
    pub(super) fn strike_ready(&self,id:usize)->bool {
        let s=self.snakes[id];
        if s.effect_kind!=4 || s.effect_ticks==0 {return false;}
        // Spatial hysteresis keeps the jaw steady during a 5r standoff.
        // Reuse the same body scan; expired Venom and ineligible victims still
        // clear the pose immediately.
        let reach=if self.faces[id].strike {6.0} else {5.0};
        let distance=(reach*s.radius).powi(2);
        let head=self.segments[id*MAX_SEGMENTS].current;
        for (other,r) in self.snakes.iter().enumerate() {
            if !r.alive || !bite_eligible(self.config.rules,s.effect_kind,s.effect_ticks,id,other,
                r.len.saturating_sub(1),r.len,self.faces[other].bite_immunity_ticks,
                effects::modifiers(r.effect_kind,r.effect_ticks).intangible) {continue;}
            for j in (r.len/2+1).max(4)..r.len {
                if self.distance_squared(head,self.segments[other*MAX_SEGMENTS+j].current)<=distance {return true;}
            }
        }
        false
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn resolve_contacts(w:&mut World) {w.mark_collisions();}
    pub(crate) fn fixture(len:usize,cut:usize)->World {
        let mut w=World::diagnostic_arena(Config {width:12000.0,height:4000.0,rules:RuleSet::V2,
            self_collisions:false,deadly_walls:true,..Config::default()},
            &[(Point{x:4000.0,y:1000.0},0.0,20,0.0),(Point{x:11500.0,y:1000.0},0.0,len,0.0)],&[]).unwrap();
        w.snakes[0].effect_kind=4;w.snakes[0].effect_ticks=240;
        let mut p=w.segments[MAX_SEGMENTS+cut].current;
        p.y+=0.7*taper::contact_radius(RuleSet::V2,6.0,taper::body_radius(6.0,cut as f64,len),false);
        w.segments[0]=Segment {current:p,previous:p};w.rebuild_trail(0);
        w.item_timer=u16::MAX;w
    }
    pub(crate) fn hunt_fixture()->World {
        let mut w=World::diagnostic_arena(Config {width:2000.0,height:1200.0,
            density:0.0,seed:73,intelligence:100.0,rules:crate::RuleSet::V2,
            self_collisions:true,deadly_walls:true,..Config::default()},&[
                (Point{x:800.0,y:750.0},-std::f64::consts::FRAC_PI_2,20,0.3),
                (Point{x:1200.0,y:600.0},0.0,100,0.3)],&[]).unwrap();
        w.snakes[0].effect_kind=4;w.snakes[0].effect_ticks=240;
        w
    }
    #[allow(dead_code)]
    pub(crate) fn exercise(w:&mut World) {w.mark_collisions();for tick in 1..=34 {w.tick=tick;w.release_detached();w.advance_presentation();}}
    #[test]
    fn titan_tail_sever_payload_delayed_nutrition_and_immunity() {
        let mut w=fixture(1600,1248);let head=w.segments[0];let old=w.food.len();
        w.mark_collisions();
        assert_eq!(w.snakes[1].len,1248);assert_eq!(w.snakes[0].dying,DeathReason::None);
        assert_eq!(w.segments[0],head);assert_eq!(w.snakes[0].effect_ticks,0);
        assert_eq!((w.faces[1].bite_immunity_ticks,w.faces[1].stump_ticks),(60,48));
        assert_eq!((w.faces[1].angry_ticks,w.faces[1].grudge_id),(78,0));
        assert!(w.bubbles().iter().any(|b|b.snake_id==1 && b.glyph==Glyph::Anger as u8));
        let e=*w.frame_events().find(|e|e.kind==EventKind::Sever).unwrap();
        assert_eq!((e.cut_index,e.duration_ticks,e.release_tick,e.value),(1248,33,34,352.0));
        assert_eq!((e.generation,e.other_generation),(1,1));
        for tick in 1..33 {w.tick=tick;w.release_detached();assert_eq!(w.food.len(),old);}
        w.tick=33;w.release_detached();
        assert_eq!(w.detached[1].len,0);
        let value:f64=w.food.iter().filter(|f|f.kind==FoodKind::Shard).map(|f|f.value).sum();
        assert!((value-352.0).abs()<1e-8);
        assert!(w.food.iter().filter(|f|f.kind==FoodKind::Shard).all(|f|f.owner==-1));
        let count=w.food.len();w.release_detached();assert_eq!(w.food.len(),count);
    }
    pub(crate) fn overlap_fixture()->World {
        let mut w=fixture(80,62);
        let a=w.segments[MAX_SEGMENTS+61].current;let b=w.segments[MAX_SEGMENTS+62].current;
        let head=Point{x:(a.x+b.x)*0.5,y:a.y+0.2};
        w.segments[0]=Segment {current:head,previous:head};w.snakes[0].angle=std::f64::consts::FRAC_PI_2;
        w.rebuild_trail(0);
        w
    }
    pub(crate) fn multi_contact_fixture(lethal_owner:usize,bite_first:bool)->World {
        let mut w=fixture(80,62);
        w.config.self_collisions=true;
        if lethal_owner==2 {
            w.snakes[2]=w.snakes[1];
            w.segments[2*MAX_SEGMENTS]=Segment {current:Point{x:10000.0,y:2000.0},previous:Point{x:10000.0,y:2000.0}};
        }
        for owner in 0..3 {for j in 1..w.snakes[owner].len {
            let p=Point{x:11000.0,y:3000.0+owner as f64*100.0};
            w.segments[owner*MAX_SEGMENTS+j]=Segment {current:p,previous:p};
        }}
        // Put the two contacts across a cell boundary, so swapping them
        // reverses traversal order without changing either physical overlap.
        let h=Point{x:100.0*w.config.width/w.grid_columns as f64,y:1000.0};
        w.segments[0]=Segment {current:h,previous:h};
        let direction=if bite_first {-1.0} else {1.0};
        let bite=Point{x:h.x+direction,y:h.y};
        let lethal=Point{x:h.x-direction,y:h.y};
        w.segments[MAX_SEGMENTS+62]=Segment {current:bite,previous:bite};
        let index=if lethal_owner==2 {62} else {10};
        w.segments[lethal_owner*MAX_SEGMENTS+index]=Segment {current:lethal,previous:lethal};
        w
    }
    pub(crate) fn boundary_contact_fixture(bite_first:bool)->World {
        let mut w=multi_contact_fixture(1,bite_first);
        let head=w.segments[0].current;let direction=if bite_first {-1.0} else {1.0};
        // Strictly between the old 9.36 and retapered 9.20409 reaches.
        let distance=(taper::contact_radius(RuleSet::V2,6.0,taper::body_radius(6.0,40.0,80),false)
            +taper::contact_radius(RuleSet::V2,6.0,taper::body_radius(6.0,40.0,62),false))*0.5;
        let remote=Point{x:11000.0,y:3100.0};
        w.segments[MAX_SEGMENTS+10]=Segment {current:remote,previous:remote};
        let p=Point{x:head.x-direction*distance,y:head.y};
        w.segments[MAX_SEGMENTS+40]=Segment {current:p,previous:p};
        w
    }
    #[test]
    fn sever_preserves_shallow_retained_contacts_in_both_bucket_orders() {
        for bite_first in [true,false] {
            let mut w=boundary_contact_fixture(bite_first);w.mark_collisions();
            assert_eq!(w.snakes[0].dying,DeathReason::Body,"bite_first={bite_first}");
            assert_eq!(w.snakes[1].len,if bite_first {62} else {80});
        }
    }
    #[test]
    fn sever_does_not_mask_self_front_half_or_second_victim_contacts() {
        for owner in 0..3 {for bite_first in [true,false] {
            let mut w=multi_contact_fixture(owner,bite_first);
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying,if owner==0 {DeathReason::SelfHit} else {DeathReason::Body},
                "owner={owner} bite_first={bite_first}");
            assert_eq!(w.snakes[1].len,if bite_first {62} else {80});
            if owner==2 {
                assert_eq!(w.snakes[2].len,if bite_first {80} else {62});
                assert_eq!(w.frame_events().filter(|e|e.kind==EventKind::Sever).count(),1,
                    "a spent charge cannot sever another victim");
            }
        }}
    }
    #[test]
    fn wall_and_head_contacts_are_not_rescued_by_an_eligible_bite() {
        let mut w=fixture(80,62);
        w.segments[0].current.x=-1.0;w.segments[0].previous=w.segments[0].current;
        w.segments[MAX_SEGMENTS+62]=w.segments[0];
        w.mark_collisions();assert_eq!(w.snakes[0].dying,DeathReason::Wall);assert_eq!(w.snakes[1].len,80);
        let mut w=fixture(80,62);
        w.snakes[2]=w.snakes[0];w.segments[2*MAX_SEGMENTS]=w.segments[0];
        w.mark_collisions();assert_eq!(w.snakes[0].dying,DeathReason::Head);assert_eq!(w.snakes[1].len,80);
        let mut w=multi_contact_fixture(2,true);
        w.snakes[2].effect_kind=3;w.snakes[2].effect_ticks=120;
        w.mark_collisions();assert_eq!(w.snakes[0].dying,DeathReason::None);assert_eq!(w.snakes[1].len,62);
    }
    #[test]
    fn a_multi_point_contact_severs_deepest_and_does_not_leave_a_touched_stump() {
        let mut w=overlap_fixture();
        // Resolution must not depend on spatial bucket enumeration.
        let mut straight=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|crate::controller::Steering {desired_angle:s.angle,rush:0.0});
        w.mark_collisions();assert_eq!(w.snakes[1].len,61);
        assert_eq!(w.frame_events().find(|e|e.kind==EventKind::Sever).unwrap().cut_index,61);
        for _ in 0..12 {w.step(&mut straight);}
        assert!(w.snakes[0].alive);
    }
    #[test]
    fn front_half_self_phase_and_immunity_use_normal_contacts() {
        for cut in [2,3,10,40] {
            let mut w=fixture(80,cut);w.mark_collisions();assert_ne!(w.snakes[0].dying,DeathReason::None);assert_eq!(w.snakes[1].len,80);
        }
        for immune in [1,60] {let mut w=fixture(80,62);w.faces[1].bite_immunity_ticks=immune;
            w.mark_collisions();assert_eq!(w.snakes[0].dying,DeathReason::Body);assert_eq!(w.snakes[1].len,80);}
        for id in [0,1] {let mut w=fixture(80,62);w.snakes[id].effect_kind=3;w.snakes[id].effect_ticks=120;
            w.mark_collisions();assert_eq!(w.snakes[0].dying,DeathReason::None);assert_eq!(w.snakes[1].len,80);}
        assert!(!bite_eligible(RuleSet::V2,4,240,0,0,62,80,0,false));
        assert!(!bite_eligible(RuleSet::Classic,4,240,0,1,62,80,0,false));
        assert!(!bite_eligible(RuleSet::V2,4,0,0,1,62,80,0,false));
    }
    #[test]
    fn second_contact_cannot_chain_bite_and_stale_grid_entries_are_removed() {
        let mut w=fixture(80,62);w.snakes[2]=w.snakes[0];w.snakes[2].color=2;
        w.segments[2*MAX_SEGMENTS]=w.segments[0];w.mark_collisions();
        // The two heads tie before the body pass; neither gets a free sever.
        assert_eq!(w.snakes[1].len,80);
        w.snakes[2].alive=false;w.mark_collisions();assert!(w.snakes[1].len<80);
        w.snakes[0].effect_kind=4;w.snakes[0].effect_ticks=240;
        let cut=w.snakes[1].len*78/100;w.segments[0]=w.segments[MAX_SEGMENTS+cut];w.mark_collisions();
        assert_eq!(w.snakes[0].dying,DeathReason::Body);
    }
    #[test]
    fn strike_standoff_uses_inclusive_five_and_six_radius_hysteresis() {
        let mut w=fixture(80,62);
        let point=Point{x:6000.0,y:2000.0};
        // One eligible bite point, isolated from the rest of the body.
        w.segments[MAX_SEGMENTS+62].current=point;
        let r=w.snakes[0].radius;
        let mut previous=false;let mut toggles=0;let mut last_toggle=None;
        for tick in 0..64 {
            let distance=if tick%2==0 {5.0} else {5.1};
            w.segments[0].current=Point{x:point.x,y:point.y+distance*r};
            w.advance_presentation();w.update_presentation();
            let strike=w.snake_flags(0)&flags::STRIKE!=0;
            if strike!=previous {
                if let Some(last)=last_toggle {assert!(tick-last>=8,"standoff jaw flickered");}
                last_toggle=Some(tick);toggles+=1;
            }
            previous=strike;
            assert!(strike,"5r enters STRIKE and nearby standoff motion holds it");
        }
        assert_eq!(toggles,1);
        w.segments[0].current.y=point.y+6.0*r;w.update_presentation();
        assert_ne!(w.snake_flags(0)&flags::STRIKE,0,"6r holds the pose");
        w.segments[0].current.y=point.y+6.01*r;w.update_presentation();
        assert_eq!(w.snake_flags(0)&flags::STRIKE,0,"beyond 6r releases it");
        w.segments[0].current.y=point.y+5.1*r;w.update_presentation();
        assert_eq!(w.snake_flags(0)&flags::STRIKE,0,"inactive pose waits for 5r");
        w.segments[0].current.y=point.y+5.0*r;w.update_presentation();
        assert_ne!(w.snake_flags(0)&flags::STRIKE,0);
        w.faces[1].bite_immunity_ticks=60;w.update_presentation();
        assert_eq!(w.snake_flags(0)&flags::STRIKE,0,"ineligible victims release the pose");
        w.faces[1].bite_immunity_ticks=0;w.update_presentation();
        assert_ne!(w.snake_flags(0)&flags::STRIKE,0);
        w.snakes[0].effect_ticks=0;w.update_presentation();
        assert!(!w.faces[0].strike,"expired Venom cannot hold a stale pose");
    }
    #[test]
    fn successful_sever_gives_happy_without_a_heart_and_preserves_victim_anger() {
        let mut w=fixture(80,62);
        w.faces[0].happy_ticks=1; // A new success restarts the payoff/blep timer.
        w.emit_bubble(0,Glyph::Alert);
        w.mark_collisions();w.update_presentation();
        assert_eq!(w.faces[0].happy_ticks,45);
        assert_eq!(w.faces[0].mood,Mood::Happy);
        assert_eq!(w.faces[1].mood,Mood::Angry);
        assert_eq!(w.bubbles().len(),2);
        assert!(w.bubbles().iter().any(|b|b.snake_id==0 && b.glyph==Glyph::Alert as u8));
        assert!(w.bubbles().iter().any(|b|b.snake_id==1 && b.glyph==Glyph::Anger as u8));
        assert!(w.bubbles().iter().all(|b|b.glyph!=Glyph::Heart as u8),"Heart is for prism meals");
        for remaining in (0..45).rev() {
            w.advance_presentation();w.update_presentation();
            assert_eq!(w.faces[0].happy_ticks,remaining);
            if remaining>0 {assert_eq!(w.faces[0].mood,Mood::Happy);}
        }
    }
    #[test]
    fn strike_and_powerups_off_preserve_detached_food() {
        let mut w=fixture(80,62);assert!(w.strike_ready(0));
        w.mark_collisions();w.reconfigure(Config {power_ups:false,..w.config()}).unwrap();
        assert!(w.snakes().all(|s|s.effect_ticks==0));assert!(!w.strike_ready(0));
        w.tick=33;w.release_detached();assert!(w.food.iter().any(|f|f.kind==FoodKind::Shard));
    }
}
