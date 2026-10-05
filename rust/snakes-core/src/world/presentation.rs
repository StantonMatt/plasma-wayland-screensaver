// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed-size character/race state. No randomness, allocation or body scans.
use super::*;
use crate::controller::FaceIntent;
pub const MAX_BUBBLES:usize=3;
pub const MAX_CONTENDERS:usize=2;
#[repr(u8)]
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Mood {#[default] Calm, Sleepy, Hunting, Scared, Angry, Happy, Trapped, Dizzy, Frozen}
#[repr(u8)]
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Glyph {#[default] Alert, Question, Anger, Sleep, Heart}
#[repr(C)]
#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub struct Bulge {pub start_tick:u64,pub duration_ticks:u16,pub origin_segment:u16,pub strength:f32}
#[repr(C)]
#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub struct Bubble {pub snake_id:u32,pub generation:u32,pub age_ticks:u16,pub glyph:u8,pub reserved:u8}
#[repr(C)]
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct WorldEventState {
    pub start_tick:u64,pub end_tick:u64,pub x:f32,pub y:f32,pub radius:f32,
    pub night:f32,pub ambient:f32,pub kind:u8,pub phase:u8,pub meteor_count:u8,pub reserved:u8,
}
impl Default for WorldEventState {
    fn default()->Self {Self {start_tick:0,end_tick:0,x:0.0,y:0.0,radius:0.0,night:0.0,ambient:1.0,
        kind:0,phase:0,meteor_count:0,reserved:0}}
}
#[derive(Clone,Copy,Debug)]
pub struct FaceState {
    pub mood:Mood,pub intensity:u8,pub age:u16,pub target_id:u64,pub target_ticks:u16,
    pub strike:bool,
    pub prey:u32,pub guarding:bool,pub has_target:bool,pub look:Point,pub pupil:Point,
    pub happy_ticks:u16,pub angry_ticks:u16,pub scared_ticks:u16,pub idle_ticks:u16,
    pub dizzy_ticks:u16,pub frozen_ticks:u16,pub thaw_immunity_ticks:u16,
    pub bite_immunity_ticks:u16,pub stump_ticks:u16,pub breath_ticks:u16,
    pub flip_grace_ticks:u16,pub flip_tick:u64,pub jaw_ticks:u16,pub bulges:[Bulge;2],
    pub grudge_id:u32,pub grudge_generation:u32,pub grudge_ticks:u16,
    bubble_cooldown:u16,pending:Mood,pending_ticks:u8,yawn_ticks:u16,
}
impl Default for FaceState {
    fn default()->Self {Self {mood:Mood::Calm,intensity:255,age:0,target_id:0,target_ticks:0,
        strike:false,prey:u32::MAX,guarding:false,has_target:false,look:Point::default(),pupil:Point::default(),
        happy_ticks:0,angry_ticks:0,scared_ticks:0,idle_ticks:0,dizzy_ticks:0,frozen_ticks:0,
        thaw_immunity_ticks:0,bite_immunity_ticks:0,stump_ticks:0,breath_ticks:0,flip_grace_ticks:0,
        flip_tick:0,jaw_ticks:0,bulges:[Bulge::default();2],grudge_id:u32::MAX,grudge_generation:0,
        grudge_ticks:0,bubble_cooldown:0,pending:Mood::Calm,pending_ticks:0,yawn_ticks:0}}
}
impl World {
    pub fn bubbles(&self)->&[Bubble] {&self.bubbles[..self.bubble_count]}
    pub(super) fn advance_presentation(&mut self) {
        let mut i=0;
        while i<self.bubble_count {
            let b=&mut self.bubbles[i];b.age_ticks+=1;
            let s=&self.snakes[b.snake_id as usize];
            if b.age_ticks>=45 || !s.alive || s.generation!=b.generation {
                self.bubble_count-=1;self.bubbles.copy_within(i+1..=self.bubble_count,i);
            } else {i+=1;}
        }
        for f in &mut self.faces {
            for counter in [&mut f.happy_ticks,&mut f.angry_ticks,&mut f.scared_ticks,&mut f.dizzy_ticks,
                &mut f.frozen_ticks,&mut f.thaw_immunity_ticks,&mut f.bite_immunity_ticks,&mut f.stump_ticks,
                &mut f.breath_ticks,&mut f.flip_grace_ticks,&mut f.grudge_ticks,&mut f.bubble_cooldown,&mut f.jaw_ticks] {
                *counter=counter.saturating_sub(1);
            }
        }
    }
    pub(super) fn emit_bubble(&mut self,id:usize,glyph:Glyph) {
        let forced=matches!(glyph,Glyph::Anger|Glyph::Heart);
        if !forced && self.faces[id].bubble_cooldown>0 {return;}
        let own=self.bubbles().iter().position(|b|b.snake_id==id as u32);
        // Forced emotions replace the owner's bubble, or the oldest global
        // bubble at capacity (prototype rule). Heart/Denied stay visible while
        // the one-owner and three-bubble limits remain intact.
        let slot=if let Some(i)=own {if !forced {return;} i}
            else if self.bubble_count==MAX_BUBBLES {
                if !forced {return;}
                self.bubbles().iter().enumerate().max_by_key(|(_,b)|b.age_ticks).unwrap().0
            } else {let i=self.bubble_count;self.bubble_count+=1;i};
        self.bubbles[slot]=Bubble {snake_id:id as u32,generation:self.snakes[id].generation,
            glyph:glyph as u8,..Bubble::default()};
        self.faces[id].bubble_cooldown=150;
        self.push_event(FrameEvent {tick:self.tick+1,position:self.segments[id*MAX_SEGMENTS].current,
            snake_id:id as u32,other_snake_id:glyph as u32,color_index:self.snakes[id].color,kind:EventKind::Emote,generation:self.snakes[id].generation,duration_ticks:45, ..FrameEvent::default()});
    }
    pub(super) fn set_face_intent(&mut self,id:usize,intent:FaceIntent) {
        let old=self.faces[id];
        let target=intent.target_id;
        if old.target_id!=target {
            if old.target_id!=0 && old.target_ticks>=30 && (self.items.iter().any(|i|i.id==old.target_id) || self.food.iter().any(|f|f.id | (1<<63)==old.target_id)) {self.emit_bubble(id,Glyph::Question);}
            else if target!=0 && (self.items.iter().any(|i|i.id==target) || self.food.iter().any(|f|f.id | (1<<63)==target)) {self.emit_bubble(id,Glyph::Alert);}
        }
        if self.config.rules==RuleSet::V2 && self.config.aggression>50 && intent.prey!=u32::MAX && old.prey!=intent.prey {
            self.emit_bubble(id,Glyph::Alert);
            self.faces[id].angry_ticks=30;
        }
        let desired=self.snakes[id].desired;
        let f=&mut self.faces[id];
        f.target_ticks=if target==old.target_id && target!=0 {old.target_ticks.saturating_add(1)} else {0};
        f.target_id=target;f.prey=intent.prey;f.guarding=intent.guarding;f.has_target=intent.has_target;f.look=if intent.look.x==0.0 && intent.look.y==0.0 {Point{x:desired.cos(),y:desired.sin()}} else {intent.look};
    }
    pub(super) fn resolve_denial(&mut self,item:Item,winner:usize) {
        let mut nearest=None;let mut distance=f64::INFINITY;
        for (id,s) in self.snakes.iter().enumerate() {
            if id==winner || !s.alive || self.faces[id].target_id!=item.id {continue;}
            let d=self.distance_squared(self.segments[id*MAX_SEGMENTS].current,item.position);
            if d<(13.0*s.radius).powi(2) && d<distance {nearest=Some(id);distance=d;}
        }
        if let Some(id)=nearest {
            let generation=self.snakes[winner].generation;
            let f=&mut self.faces[id];
            f.angry_ticks=78;f.grudge_id=winner as u32;f.grudge_generation=generation;f.grudge_ticks=150;
            self.emit_bubble(id,Glyph::Anger);
        }
        for f in &mut self.faces {if f.target_id==item.id {f.target_id=0;f.target_ticks=0;f.guarding=false;}}
    }
    pub(super) fn update_presentation(&mut self) {
        // Existing exact kill and consumption records are the authoritative
        // signals. One near-kill fear scan over heads, only on kill ticks.
        for i in 0..self.event_count {
            let e=self.frame_events[(self.event_start+i)%MAX_EVENTS];
            if e.kind!=EventKind::Kill {continue;}
            if (e.other_snake_id as usize)<self.snakes.len() {
                let killer=e.other_snake_id as usize;
                if self.snakes[killer].alive {self.faces[killer].happy_ticks=45;}
                for id in 0..self.snakes.len() {
                    if id==killer || !self.snakes[id].alive || self.snakes[killer].len<=self.snakes[id].len {continue;}
                    if self.distance_squared(e.position,self.segments[id*MAX_SEGMENTS].current)<(12.0*self.snakes[id].radius).powi(2) {
                        self.faces[id].scared_ticks=36;
                    }
                }
            }
        }
        for &(_,id,_,_,value) in &self.consumptions {if value>=3.0 {self.faces[id as usize].happy_ticks=45;}}
        for id in 0..self.snakes.len() {
            if !self.snakes[id].alive {continue;}
            let head=self.segments[id*MAX_SEGMENTS].current;
            let r=self.snakes[id].radius;
            let threatened=self.snakes.iter().enumerate().any(|(other,s)| other!=id && s.alive
                && ((self.faces[other].prey==id as u32 && s.intent_flags & flags::HUNTING!=0)
                    || (s.effect_kind==effects::EffectKind::Venom as u8 && self.faces[other].prey==id as u32))
                && self.distance_squared(head,self.segments[other*MAX_SEGMENTS].current)<(14.0*r).powi(2));
            // strike_ready uses the previous pose for its 5r/6r hysteresis.
            self.faces[id].strike=self.strike_ready(id);
            let flags=self.snake_flags(id);
            let angle=self.snakes[id].angle;let generation=self.snakes[id].generation;
            let f=&mut self.faces[id];
            if threatened {f.scared_ticks=6;}
            if !f.has_target && f.prey==u32::MAX && flags & (flags::HUNTING|flags::BOOSTING)==0 {f.idle_ticks=f.idle_ticks.saturating_add(1);} else {f.idle_ticks=0;}
            let next=if f.frozen_ticks>0 || flags & flags::FROZEN!=0 {Mood::Frozen}
                else if f.dizzy_ticks>0 {Mood::Dizzy} else if flags & flags::TRAPPED!=0 {Mood::Trapped}
                else if f.happy_ticks>0 {Mood::Happy} else if f.angry_ticks>0 {Mood::Angry}
                else if f.scared_ticks>0 {Mood::Scared} else if flags & (flags::HUNTING|flags::BOOSTING)!=0 {Mood::Hunting}
                else if f.idle_ticks>=600 || (self.world_event.night>0.0 && !f.has_target) {Mood::Sleepy} else {Mood::Calm};
            if next!=f.pending {f.pending=next;f.pending_ticks=0;} else {f.pending_ticks=f.pending_ticks.saturating_add(1);}
            // Urgent/event moods enter immediately; calm/sleep/hunt need six
            // stable observations, and exits cannot flutter on one tick.
            let urgent=matches!(next,Mood::Frozen|Mood::Dizzy|Mood::Trapped|Mood::Happy|Mood::Angry|Mood::Scared);
            if next!=f.mood && (f.pending_ticks>=6 || (urgent && next as u8>f.mood as u8)) {f.mood=next;f.age=0;} else {f.age=f.age.saturating_add(1);}
            f.intensity=((f.age.min(6) as u32*255)/6) as u8;
            let d=crate::normalize_angle(f.look.y.atan2(f.look.x)-angle);
            f.pupil=Point{x:d.cos()*0.18,y:d.sin()*0.35};
            let yawn=150+(id as u64*73+generation as u64*31)%211;
            // A bounded recurring timer keeps advancing after exported age saturates.
            if f.mood==Mood::Sleepy && f.age>0 {
                f.yawn_ticks+=1;
                if f.yawn_ticks>=yawn as u16 {f.yawn_ticks=0;f.jaw_ticks=39;self.emit_bubble(id,Glyph::Sleep);}
            } else {f.yawn_ticks=0;}
        }
    }
    pub(super) fn update_item_races(&mut self) {
        for index in 0..self.items.len() {
            let mut item=self.items[index];
            item.leader_snake_id=u32::MAX;item.leader_eta=f32::INFINITY;
            item.contender_count=0;item.contender_ids=[u32::MAX;MAX_CONTENDERS];
            item.contender_etas=[f32::INFINITY;MAX_CONTENDERS];item.guard_snake_id=u32::MAX;
            for (id,s) in self.snakes.iter().enumerate() {
                if !s.alive || self.faces[id].target_id!=item.id {continue;}
                if self.faces[id].guarding {if item.guard_snake_id==u32::MAX {item.guard_snake_id=id as u32;}continue;}
                let d=self.displacement(self.segments[id*MAX_SEGMENTS].current,item.position);
                let (speed,turn)=self.motion_limits(id,0.0).unwrap();
                let eta=(((d.x*d.x+d.y*d.y).sqrt()/speed)+crate::normalize_angle(d.y.atan2(d.x)-s.angle).abs()/turn) as f32;
                // Contender arcs are the two spatially nearest committed heads;
                // race leadership uses turn-aware ETA, independently.
                if eta<item.leader_eta {item.leader_eta=eta;item.leader_snake_id=id as u32;}
                let distance=d.x*d.x+d.y*d.y;
                let slot=(0..MAX_CONTENDERS).find(|&i| item.contender_ids[i]==u32::MAX || distance<self.distance_squared(self.segments[item.contender_ids[i] as usize*MAX_SEGMENTS].current,item.position));
                if let Some(i)=slot {
                    for j in (i+1..MAX_CONTENDERS).rev() {item.contender_ids[j]=item.contender_ids[j-1];item.contender_etas[j]=item.contender_etas[j-1];}
                    item.contender_ids[i]=id as u32;item.contender_etas[i]=eta;
                }
                item.contender_count=(item.contender_count+1).min(MAX_CONTENDERS as u8);
            }
            self.items[index]=item;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arena()->World {
        World::diagnostic_arena(Config {rules:RuleSet::V2,density:9.0,self_collisions:false,..Config::default()},
            &[(Point{x:400.0,y:300.0},0.0,48,0.9),(Point{x:460.0,y:300.0},0.0,24,0.9),
              (Point{x:500.0,y:350.0},0.0,24,0.9),(Point{x:540.0,y:350.0},0.0,24,0.9)],&[]).unwrap()
    }
    #[test]
    fn bubble_cap_cooldown_replacement_lifetime_and_respawn() {
        let mut w=arena();
        for id in 0..4 {w.emit_bubble(id,Glyph::Alert);}
        assert_eq!(w.bubbles().len(),3);
        w.bubbles[0].age_ticks=10;w.emit_bubble(0,Glyph::Question);
        assert_eq!(w.bubbles()[0].age_ticks,10,"ordinary emission obeys cooldown");
        w.emit_bubble(0,Glyph::Anger);assert_eq!(w.bubbles()[0].glyph,Glyph::Anger as u8);
        assert_eq!(w.bubbles()[0].age_ticks,0);
        w.bubbles[2].age_ticks=12;
        w.emit_bubble(3,Glyph::Heart);assert_eq!(w.bubbles().len(),3,"forced replacement cannot exceed cap");
        assert!(w.bubbles().iter().any(|b|b.snake_id==3 && b.glyph==Glyph::Heart as u8));
        assert!(!w.bubbles().iter().any(|b|b.snake_id==2),"oldest global bubble is evicted");
        w.snakes[1].generation+=1;w.advance_presentation();
        assert_eq!(w.bubbles().len(),2,"old life bubbles disappear");
        for _ in 0..44 {w.advance_presentation();}assert!(w.bubbles().is_empty());
        w.emit_bubble(0,Glyph::Alert);assert!(w.bubbles().is_empty());
        for _ in 0..105 {w.advance_presentation();}w.emit_bubble(0,Glyph::Alert);
        assert_eq!(w.bubbles().len(),1);
    }
    #[test]
    fn prism_meal_emits_one_heart_and_classic_stays_unchanged() {
        let mut w=arena();w.food.clear();
        let prism=Food {id:77,kind:FoodKind::Prism,value:9.0,..Food::default()};
        w.emit_bubble(0,Glyph::Alert);
        w.food.push(prism);w.consume_food(0,0);
        assert_eq!(w.bubbles().len(),1);
        assert_eq!(w.bubbles()[0].glyph,Glyph::Heart as u8);
        assert_eq!(w.faces[0].happy_ticks,45);
        w.bubble_count=0;w.faces[0]=FaceState::default();w.config.rules=RuleSet::Classic;
        w.food.push(prism);w.consume_food(0,0);
        assert!(w.bubbles().is_empty());assert_eq!(w.faces[0].happy_ticks,0);
    }
    #[test]
    fn mood_priority_and_hysteresis_follow_signals() {
        let mut w=arena();w.snakes[0].intent_flags=flags::HUNTING;
        for _ in 0..7 {w.update_presentation();}assert_eq!(w.faces[0].mood,Mood::Hunting);
        w.faces[0].happy_ticks=45;w.update_presentation();assert_eq!(w.faces[0].mood,Mood::Happy);
        w.snakes[0].intent_flags|=flags::TRAPPED;w.update_presentation();assert_eq!(w.faces[0].mood,Mood::Trapped);
        w.faces[0].dizzy_ticks=30;w.update_presentation();assert_eq!(w.faces[0].mood,Mood::Dizzy);
        w.faces[0].frozen_ticks=75;w.update_presentation();assert_eq!(w.faces[0].mood,Mood::Frozen);
        w.faces[0]=FaceState {mood:Mood::Hunting,..FaceState::default()};
        w.snakes[0].intent_flags=0;w.update_presentation();assert_eq!(w.faces[0].mood,Mood::Hunting);
        w.snakes[0].intent_flags=flags::HUNTING;w.update_presentation();assert_eq!(w.faces[0].mood,Mood::Hunting);
        w.snakes[0].intent_flags=0;for _ in 0..7 {w.update_presentation();}assert_eq!(w.faces[0].mood,Mood::Calm);
        w.faces[0].idle_ticks=600;for _ in 0..7 {w.update_presentation();}assert_eq!(w.faces[0].mood,Mood::Sleepy);
    }
    #[test]
    fn sleepy_yawns_keep_recurring_after_exported_age_saturates() {
        // Cover both failure modes of modulo on u16::MAX: an interval that
        // divides 65535 (255) and one that does not.
        for interval in [255u16,256] {
            let mut w=arena();
            for s in &mut w.snakes[1..] {s.alive=false;}
            w.snakes[0].generation=(1..212).find(|g|150+(*g as u64*31)%211==interval as u64).unwrap();
            w.faces[0]=FaceState {mood:Mood::Sleepy,pending:Mood::Sleepy,idle_ticks:600,
                target_id:77,target_ticks:u16::MAX-1,grudge_ticks:150,bubble_cooldown:150,..Default::default()};
            let mut yawns=0;let mut last_yawn=0;let mut completed_after_saturation=false;
            for tick in 1..=70_000 {
                w.tick=tick;w.advance_presentation();w.update_presentation();
                w.set_face_intent(0,FaceIntent {target_id:77,..Default::default()});
                if w.faces[0].jaw_ticks==39 {
                    if last_yawn>0 {assert_eq!(tick-last_yawn,interval as u64);}
                    last_yawn=tick;yawns+=1;
                }
                if tick>u16::MAX as u64 && w.faces[0].jaw_ticks==0 {completed_after_saturation=true;}
            }
            assert_eq!(w.faces[0].age,u16::MAX);assert_eq!(w.faces[0].target_ticks,u16::MAX);
            assert_eq!(yawns,70_000/interval as u64);assert!(last_yawn>u16::MAX as u64);
            assert!(completed_after_saturation);assert_eq!(w.faces[0].grudge_ticks,0);
            // Bubble lifetime/cooldown keeps progressing alongside the yawns.
            assert!(w.bubbles().iter().all(|b|b.age_ticks<45));
            assert!(w.faces[0].bubble_cooldown<=150);
            w.faces[0].has_target=true;
            for _ in 0..7 {w.advance_presentation();w.update_presentation();}
            assert_eq!(w.faces[0].mood,Mood::Calm);assert_eq!(w.faces[0].yawn_ticks,0);
        }
    }
    #[test]
    fn only_nearest_committed_loser_gets_denial_and_generation_guarded_grudge() {
        let mut w=arena();let item=Item {id:77,position:Point{x:430.0,y:300.0},..Item::default()};
        for f in &mut w.faces {f.target_id=77;}
        w.resolve_denial(item,0);
        assert_eq!(w.faces.iter().filter(|f|f.angry_ticks>0).count(),1);
        assert_eq!(w.faces[1].grudge_id,0);assert_eq!(w.faces[1].grudge_ticks,150);
        assert_eq!(w.faces[1].grudge_generation,w.snakes[0].generation);
        assert!(w.faces.iter().all(|f|f.target_id==0));
        assert_eq!(w.bubbles().len(),1);assert_eq!(w.bubbles()[0].glyph,Glyph::Anger as u8);
    }
    #[test]
    fn race_leadership_uses_eta_and_arcs_use_nearest_heads() {
        let mut w=arena();let pos=Point{x:550.0,y:300.0};
        w.items.push(Item{id:77,position:pos,..Item::default()});
        for f in &mut w.faces {f.target_id=77;}
        w.faces[3].guarding=true;w.update_item_races();let item=w.items[0];
        assert_eq!(item.contender_count,2);assert_eq!(item.contender_ids,[2,1]);
        assert_eq!(item.guard_snake_id,3);assert!(item.leader_eta.is_finite());
        w.snakes[1].angle=std::f64::consts::PI;w.update_item_races();
        assert_ne!(w.items[0].leader_snake_id,1,"turning away costs ETA");
        for f in &mut w.faces {f.target_id=0;}w.update_item_races();
        assert_eq!(w.items[0].leader_snake_id,u32::MAX);assert!(w.items[0].leader_eta.is_infinite());
    }
}
