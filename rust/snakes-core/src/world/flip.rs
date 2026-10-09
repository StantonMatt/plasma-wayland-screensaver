// SPDX-License-Identifier: GPL-3.0-or-later
//! Flip is an inventory action, independent of the active timed effect.
use super::*;
impl World {
    pub(super) fn flip_snake(&mut self,id:usize) {
        let s=self.snakes[id];if s.len<2 {return;}
        let base=id*MAX_SEGMENTS;let old=self.segments[base].current;
        self.segments[base..base+s.len].reverse();
        // Reversal is instantaneous, never a head sweep through the body.
        for segment in &mut self.segments[base..base+s.len] {segment.previous=segment.current;}
        let head=self.segments[base].current;
        let d=self.displacement(self.segments[base+1].current,head);
        let angle=d.y.atan2(d.x);
        self.snakes[id].angle=angle;self.snakes[id].desired=angle;
        self.snakes[id].intent_flags=0;self.snakes[id].stretch=0.0;
        self.rebuild_trail(id);
        // Indexed state belongs to the old head/tail. Detached orphans already
        // own their geometry and continue independently; the stump is retired.
        let f=&mut self.faces[id];
        f.bulges=[Bulge::default();2];f.stump_ticks=0;f.strike=false;
        f.target_id=0;f.target_ticks=0;f.prey=u32::MAX;f.guarding=false;f.has_target=false;
        f.look=Point{x:angle.cos(),y:angle.sin()};f.pupil=Point::default();f.jaw_ticks=0;
        f.flip_tick=self.tick+1;f.flip_grace_ticks=6;f.dizzy_ticks=30;
        f.mood=Mood::Dizzy;f.age=0;
        self.push_event(FrameEvent {tick:self.tick+1,position:head,snake_id:id as u32,
            generation:s.generation,other_snake_id:6,color_index:s.color,kind:EventKind::Flip,
            duration_ticks:15,..Default::default()});
        // A second ring uses the old head point without a second Flip event.
        self.inventory_event(id,usize::MAX,effects::EffectKind::Flip,EventKind::EffectExpiry,old,15);
    }

}
/// Endpoint reversal uses the body after this movement, exactly as pickup does.
/// Predicted growth is deliberately not invented; admission rejects a growing
/// holder until its indexed tail can be certified.
pub(crate) fn forecast_pose(w:&World,s:SnakeView<'_>,path:&[Point],distances:&[f64],len:usize,radius:f64)->(Point,f64) {
    let at=|back:f64| {
        let target=distances[distances.len()-1]-back;
        if target<=0.0 {return w.forecast_trail_point(s.id as usize,-target);}
        let end=distances.partition_point(|&d|d<target).clamp(1,distances.len()-1);
        let t=((target-distances[end-1])/(distances[end]-distances[end-1]).max(0.0001)).clamp(0.0,1.0);
        let d=w.displacement(path[end-1],path[end]);
        w.canonical_point(Point{x:path[end-1].x+d.x*t,y:path[end-1].y+d.y*t})
    };
    let tail=at(radius*1.18*len.saturating_sub(1) as f64);
    let before=at(radius*1.18*len.saturating_sub(2) as f64);
    let d=w.displacement(before,tail);(tail,d.y.atan2(d.x))
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Use(u32);
    impl Controller for Use {
        fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering {desired_angle:s.angle,rush:0.0}}
        fn use_request(&self,_:u32)->u32 {self.0}
    }
    fn arena(store:bool,wrap:bool)->World {World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
        store_power_ups:store,world_events:false,deadly_walls:!wrap,self_collisions:true,..Default::default()},
        &[(Point{x:1500.0,y:1000.0},0.0,40,0.0)],&[]).unwrap()}
    #[test]
    fn flip_world_step_windup_reversal_trail_and_expiry() {
        for store in [false,true] {for wrap in [false,true] {
            let mut w=arena(store,wrap);
            w.items.push(Item {kind:effects::EffectKind::Flip,position:w.segments[0].current,radius:40.0,life_ticks:750,..Default::default()});
            w.step(&mut Use(0));assert_eq!(w.snakes[0].inventory.kinds[0],6);
            w.snakes[0].effect_kind=1;w.snakes[0].effect_ticks=100;
            w.faces[0].bulges[0]=Bulge {duration_ticks:30,strength:0.3,..Default::default()};w.faces[0].stump_ticks=30;
            w.step(&mut Use(1));
            for _ in 0..3 {w.step(&mut Use(0));assert_eq!(w.faces[0].flip_tick,0);}
            w.step(&mut Use(0));assert_eq!(w.faces[0].flip_tick,w.tick());assert_eq!(w.faces[0].flip_grace_ticks,6);
            assert_eq!(w.faces[0].dizzy_ticks,30);assert_eq!(w.snakes[0].inventory.count,0);
            assert_eq!(w.snakes[0].effect_kind,1);
            assert_eq!(w.faces[0].stump_ticks,0);assert_eq!(w.faces[0].bulges[0].duration_ticks,0);
            assert!(w.snakes[0].angle.abs()>3.0);assert!(w.snakes[0].alive);
            for seg in &w.segments[..w.snakes[0].len] {assert_eq!(seg.current,seg.previous);}
            for _ in 0..6 {w.step(&mut Use(0));assert!(w.snakes[0].alive);}
            assert_eq!(w.faces[0].flip_grace_ticks,0);
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1,0,0],count:1,..Default::default()};
            let tick=w.faces[0].flip_tick;w.step(&mut Use(1));assert_eq!(w.faces[0].flip_tick,tick);
            assert!(w.frame_events().any(|e|e.kind==EventKind::Fizzle && e.other_snake_id==6));
        }}
    }
    #[test]
    fn flip_guarded_full_touch_dizzy_duration_and_death_drop() {
        struct Guard;
        impl Controller for Guard {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering {desired_angle:s.angle,rush:0.0}}
            fn face_intent(&self,_:u32)->crate::controller::FaceIntent {crate::controller::FaceIntent {guarding:true,..Default::default()}}
        }
        for store in [false,true] {
            let mut w=arena(store,false);w.snakes[0].effect_kind=1;w.snakes[0].effect_ticks=100;
            w.faces[0].guarding=true;
            w.items.push(Item {kind:effects::EffectKind::Flip,position:w.segments[0].current,radius:40.0,life_ticks:750,..Default::default()});
            w.step(&mut Guard);assert_eq!(w.snakes[0].inventory.count,1,"guard must allow a held pickup");
            w.snakes[0].inventory=Inventory {kinds:[6;3],life:[1800;3],count:3,..Default::default()};
            w.items.push(Item {kind:effects::EffectKind::Flip,position:w.segments[0].current,radius:40.0,life_ticks:750,..Default::default()});
            w.step(&mut Guard);assert_eq!(w.faces[0].flip_tick,w.tick());
            assert_eq!(w.snakes[0].inventory.count,3);assert_eq!(w.snakes[0].effect_kind,1);
            assert!(w.frame_events().any(|e|e.kind==EventKind::Use && e.other_snake_id==6 && e.duration_ticks==0));
            assert!(w.faces[0].look.x< -0.99 && w.faces[0].look.y.abs()<0.01);
            for _ in 0..29 {w.step(&mut Guard);assert_eq!(w.faces[0].mood,Mood::Dizzy);assert!(w.snakes[0].alive);}
            w.step(&mut Guard);assert_eq!(w.faces[0].dizzy_ticks,0);assert_ne!(w.faces[0].mood,Mood::Dizzy);
            // Drive the real collision/death path while three Flips are held.
            w.snakes[0].inventory=Inventory {kinds:[6;3],life:[1800;3],count:3,..Default::default()};
            w.segments[0].current=Point{x:1.0,y:1000.0};w.segments[0].previous=w.segments[0].current;
            w.snakes[0].angle=std::f64::consts::PI;w.rebuild_trail(0);
            w.step(&mut Guard);assert!(!w.snakes[0].alive);assert_eq!(w.snakes[0].inventory.count,0);
            assert!(w.items.iter().any(|i|i.kind==effects::EffectKind::Flip && i.dropped));
            assert!(w.frame_events().any(|e|e.kind==EventKind::Fizzle && e.other_snake_id==6));
        }
    }

}
