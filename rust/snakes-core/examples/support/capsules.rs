// SPDX-License-Identifier: GPL-3.0-or-later
//! Observer work is outside the timed simulation. Targets are sampled at each
//! actual control call, so deaths cannot erase a contested objective. Spawn
//! ETA is the unobstructed, turn-aware estimate from pre-movement live heads.
//! Magnet extra-reach attribution compares post-movement heads to the original
//! stationary Spark position, before the first pull moves the food inward.
use snakes_core::{EventKind, FoodKind, Point, World, MAX_FOOD, MAX_SNAKES, STEP_SECONDS};
use super::accounting::{Tactics,EFFECT_KIND_COUNT};
use snakes_core::effects::EffectKind;
struct Capsule {
    id:u64, kind:u8, position:Point, spawned:u64, eta:f64,
    targeted:u16, max_contest:usize, contest_ticks:u64, outcome:Option<Option<u32>>, elapsed:f64,
}
#[derive(Clone,Copy,Default)]
struct Episode {generation:u32,kind:u8,used:bool,extended:bool}
#[derive(Clone,Copy,Default)]
struct SpawnSnake {alive:bool,head:Point,angle:f64,radius:f64,speed:f64,turn:f64}
#[derive(Clone,Copy)]
struct FoodProbe {id:u64,position:Point,size:f64,unclaimed:bool,scavenging:bool,stationary:bool}
#[derive(Default)]
pub struct Capsules {
    records:Vec<Capsule>, episodes:[Episode;MAX_SNAKES],
    previous_food_goal:[bool;MAX_SNAKES], magnet_extended:u64,
    pickups:[u64;EFFECT_KIND_COUNT], used:[u64;EFFECT_KIND_COUNT], surge_staged:u64, staged:[bool;MAX_SNAKES], surge_chained:u64, bursts:[u8;MAX_SNAKES], old_boosts:[u8;MAX_SNAKES], clear_phase:u64, phase_expiries:u64,
    foods:Vec<FoodProbe>, snakes:[SpawnSnake;MAX_SNAKES], old_effects:[(u8,u16);MAX_SNAKES],
}
impl Capsules {
    pub fn new()->Self {Self {records:Vec::with_capacity(32),foods:Vec::with_capacity(MAX_FOOD),..Self::default()}}
    pub fn before(&mut self,w:&World) {
        for snake in w.snakes() {
            let id=snake.id as usize;
            self.old_effects[id]=(snake.effect_kind,snake.effect_ticks);
            self.old_boosts[id]=snake.boost_ticks;
            let (speed,turn)=w.motion_limits(id,0.0).unwrap();
            self.snakes[id]=SpawnSnake {alive:snake.alive,head:snake.segments.first().map_or(Point::default(),|s|s.current),angle:snake.angle,radius:snake.radius,speed,turn};
        }
        self.foods.clear();
        for f in w.foods() {
            self.foods.push(FoodProbe {id:f.id,position:f.position,size:f.size,unclaimed:f.vacuum_owner<0,
                scavenging:f.kind==FoodKind::Prism || f.feast_id!=0,stationary:f.kind==FoodKind::Spark});
        }
        // World snapshots need not preserve ID order. One allocation-free
        // sort supports O(log F) lookup for retained claims and consumption.
        self.foods.sort_unstable_by_key(|f|f.id);
    }
    fn food_probe(&self,id:u64)->Option<FoodProbe> {
        self.foods.binary_search_by_key(&id,|f|f.id).ok().map(|i|self.foods[i])
    }
    fn observe_magnet(&mut self,w:&World,food:u64,id:usize,tactics:&[Tactics;MAX_SNAKES],consumed:bool) {
        if self.old_effects[id].0!=2 || self.old_effects[id].1<=1 {return;}
        let Some(old)=self.food_probe(food) else {return;};
        let s=w.snake(id).unwrap();
        let distance=w.distance_squared(s.segments[0].current,old.position);
        let extended=old.unclaimed && old.stationary
            && distance>(s.radius*3.0+old.size).powi(2) && distance<=(s.radius*9.0+old.size).powi(2);
        if extended {
            let episode=&mut self.episodes[id];
            if episode.kind==2 && episode.generation==s.generation && !episode.extended {
                episode.extended=true;self.magnet_extended+=1;
            }
        }
        let food_goal=tactics[id].generation==s.generation && tactics[id].target!=0 && tactics[id].target & (1<<63)==0;
        if (extended && (food_goal || self.previous_food_goal[id])) || (consumed && old.scavenging) {
            self.use_episode(id,2,s.generation);
        }
    }
    fn use_episode(&mut self,id:usize,kind:u8,generation:u32) {
        let e=&mut self.episodes[id];
        if e.kind==kind && e.generation==generation && !e.used {e.used=true;self.used[kind as usize]+=1;}
    }
    pub fn after(&mut self,w:&World,tactics:&[Tactics;MAX_SNAKES]) {
        for event in w.frame_events().filter(|e|e.kind==EventKind::ItemSpawn) {
            let item=w.items().find(|i|i.position==event.position && i.kind as u32==event.other_snake_id);
            // Pickup can happen on the spawn movement. The monotonic ID can be
            // recovered from the preceding spawn even if no item survives.
            let id=item.map_or_else(||self.records.last().map_or(1,|r|r.id+1),|i|i.id);
            let radius=item.map_or(w.config().base_radius()*2.1,|i|i.radius);
            let eta=self.snakes.iter().filter(|s|s.alive).map(|s| {
                let d=w.displacement(s.head,event.position);
                let distance=(d.x*d.x+d.y*d.y).sqrt();
                let bearing=snakes_core::normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                let radius_turn=s.speed/s.turn.max(0.01);
                (distance-1.3*s.radius-radius).max(0.0)/s.speed.max(1.0)
                    +bearing/s.turn.max(0.01)*0.6
                    +if bearing>1.2 && distance<2.0*radius_turn {radius_turn*(bearing-1.2)/s.speed.max(1.0)} else {0.0}
            }).fold(f64::INFINITY,f64::min);
            self.records.push(Capsule {id,kind:event.other_snake_id as u8,position:event.position,
                spawned:w.tick(),eta,targeted:0,max_contest:0,contest_ticks:0,outcome:None,elapsed:0.0});
        }
        for r in self.records.iter_mut().filter(|r|r.outcome.is_none()) {
            let mut count=0;
            for (id,t) in tactics.iter().enumerate() {
                if t.target==(r.id|(1<<63)) {r.targeted|=1<<id;count+=1;}
            }
            r.max_contest=r.max_contest.max(count);
            r.contest_ticks+=u64::from(count>1);
        }
        // Count extended claims immediately; consumption can follow expiry.
        // Intended use requires a food goal on this or the preceding movement.
        // A passive extra-reach benefit has its own counter.
        for f in w.foods().filter(|f|f.vacuum_owner>=0) {
            self.observe_magnet(w,f.id,f.vacuum_owner as usize,tactics,false);
        }
        // A newly claimed Spark can disappear during the very first pull.
        // Attribute it from its original position exactly as a retained claim.
        for (food,id,_,_,_) in w.consumption_events() {
            self.observe_magnet(w,food,id as usize,tactics,true);
        }
        for s in w.snakes().filter(|s|s.alive && s.effect_ticks>0) {
            let id=s.id as usize;
            if self.old_effects[id].0==1 && self.old_effects[id].1>1 && tactics[id].generation==s.generation {
                let approaching=tactics[id].prey.is_some_and(|prey| {
                    let to_prey=w.displacement(self.snakes[id].head,self.snakes[prey].head);
                    let movement=w.displacement(self.snakes[id].head,s.segments[0].current);
                    self.snakes[prey].alive && to_prey.x*movement.x+to_prey.y*movement.y>0.0
                });
                if (tactics[id].staged || approaching) && self.old_boosts[id]<=1 && s.boost_ticks==24 {
                    self.bursts[id]=self.bursts[id].saturating_add(1);
                    if self.bursts[id]==2 {self.surge_chained+=1;}
                }
                if tactics[id].staged {
                    if !self.staged[id] {self.staged[id]=true;self.surge_staged+=1;}
                    self.use_episode(id,1,s.generation);
                } else if approaching && s.boost_ticks>0 {
                    self.use_episode(id,1,s.generation);
                }
            }
        }
        for event in w.frame_events() {self.observe_event(w,event);}
        for s in w.snakes().filter(|s|s.alive && s.effect_kind==3 && s.effect_ticks>0) {
            if super::phase_contact::otherwise_lethal(w,s) {self.use_episode(s.id as usize,3,s.generation);}
        }
        for (id,t) in tactics.iter().enumerate() {
            self.previous_food_goal[id]=t.target!=0 && t.target & (1<<63)==0;
        }
    }
    fn observe_event(&mut self,w:&World,event:&snakes_core::FrameEvent) {
        if matches!(event.kind,EventKind::Pickup|EventKind::ItemExpiry) {
            // Frame events carry positions/kinds, not item IDs. Resolve
            // only against live records; item placement keeps them apart.
            if let Some(r)=self.records.iter_mut().find(|r|r.outcome.is_none() && r.kind==event.other_snake_id as u8 && r.position==event.position) {
                r.outcome=Some(if event.kind==EventKind::Pickup {Some(event.snake_id)} else {None});
                r.elapsed=(w.tick()-r.spawned) as f64*STEP_SECONDS;
            }
        }
        if event.kind==EventKind::Pickup {
            let id=event.snake_id as usize;let kind=event.other_snake_id as u8;
            self.pickups[kind as usize]+=1;
            self.episodes[id]=Episode {generation:w.snake(id).unwrap().generation,kind,..Episode::default()};
            self.staged[id]=false;self.bursts[id]=0;
        }
        if event.kind==EventKind::EffectExpiry && event.other_snake_id==3 {
            self.phase_expiries+=1;
            self.clear_phase+=u64::from(w.snake(event.snake_id as usize).is_some_and(|s|s.alive));
        }
        if event.kind==EventKind::Sever {
            self.use_episode(event.other_snake_id as usize,EffectKind::Venom as u8,event.other_generation);
        }
    }
    pub fn report(&self,label:&str,cfg:snakes_core::Config,trace:bool) {
        let picked=self.records.iter().filter(|r|matches!(r.outcome,Some(Some(_)))).count();
        let expired=self.records.iter().filter(|r|r.outcome==Some(None)).count();
        let pending=self.records.len()-picked-expired;
        assert_eq!(picked as u64,self.pickups.iter().sum::<u64>(),"every pickup must resolve a spawn record");
        let targeted=self.records.iter().filter(|r|r.targeted!=0).count();
        let contested=self.records.iter().filter(|r|r.max_contest>1).count();
        let mut times:Vec<_>=self.records.iter().filter(|r|matches!(r.outcome,Some(Some(_)))).map(|r|r.elapsed).collect();
        times.sort_unstable_by(f64::total_cmp);
        let median=if times.is_empty() {f64::NAN} else {(times[(times.len()-1)/2]+times[times.len()/2])*0.5};
        println!("capsules controller={label} seed={} iq={} walls={} spawned={} picked={picked} expired={expired} pending={pending} targeted={targeted} contested={contested} median_s={median:.3} surge_pickups={} surge_used={} magnet_pickups={} magnet_used={} phase_pickups={} phase_used={} phase_expiries={} phase_clear={} venom_pickups={} venom_used={} surge_staged={} surge_chained={} magnet_extended={}",cfg.seed,cfg.intelligence,if cfg.deadly_walls {"deadly"} else {"wrap"},self.records.len(),self.pickups[1],self.used[1],self.pickups[2],self.used[2],self.pickups[3],self.used[3],self.phase_expiries,self.clear_phase,self.pickups[EffectKind::Venom as usize],self.used[EffectKind::Venom as usize],self.surge_staged,self.surge_chained,self.magnet_extended);
        if trace {
            for r in &self.records {
                println!("capsule id={} kind={} spawn_tick={} nearest_eta_s={:.3} targeted={} targeters={} max_contest={} contest_ticks={} outcome={} pickup_s={:.3}",r.id,r.kind,r.spawned,r.eta,r.targeted!=0,r.targeted.count_ones(),r.max_contest,r.contest_ticks,match r.outcome {Some(Some(id))=>format!("snake:{id}"),Some(None)=>"expired".into(),None=>"pending".into()},if matches!(r.outcome,Some(Some(_))) {r.elapsed} else {f64::NAN});
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    fn observer()->(World,Capsules,[Tactics;MAX_SNAKES]) {
        let w=World::diagnostic_arena(snakes_core::Config {density:0.0,scale:70.0,
            rules:snakes_core::RuleSet::V2,..Default::default()},
            &[(Point{x:400.0,y:300.0},0.0,24,0.9)],&[]).unwrap();
        let generation=w.snake(0).unwrap().generation;
        let mut c=Capsules::new();c.old_effects[0]=(2,100);
        c.episodes[0]=Episode {kind:2,generation,..Default::default()};
        let mut tactics=[Tactics::default();MAX_SNAKES];
        tactics[0]=Tactics {generation,target:77,..Default::default()};
        (w,c,tactics)
    }
    #[test]
    fn every_effect_kind_resolves_and_counts_pickups_including_venom() {
        let (w,mut c,_)=observer();
        for raw in 1..EFFECT_KIND_COUNT {
            let kind=EffectKind::from_byte(raw as u8);
            assert_eq!(kind as usize,raw);
            let position=Point{x:raw as f64,y:100.0};
            c.records.push(Capsule {id:raw as u64,kind:kind as u8,position,spawned:w.tick(),eta:0.0,
                targeted:0,max_contest:0,contest_ticks:0,outcome:None,elapsed:0.0});
            c.observe_event(&w,&snakes_core::FrameEvent {kind:EventKind::Pickup,other_snake_id:kind as u32,
                position,snake_id:0,..Default::default()});
            assert_eq!(c.pickups[raw],1);
            assert_eq!(c.records.last().unwrap().outcome,Some(Some(0)));
            assert_eq!(c.episodes[0].kind,kind as u8);
        }
        for &kind in snakes_core::effects::ENABLED_KINDS {assert!((kind as usize)<EFFECT_KIND_COUNT);}
        c.report("test",w.config(),false); // Includes the resolved-total invariant.
    }
    #[test]
    fn venom_use_is_attributed_once_to_the_biter_episode_and_generation() {
        let (w,mut c,_)=observer();let generation=w.snake(0).unwrap().generation;
        let pickup=snakes_core::FrameEvent {kind:EventKind::Pickup,other_snake_id:EffectKind::Venom as u32,
            snake_id:0,..Default::default()};
        c.observe_event(&w,&pickup);
        let mut sever=snakes_core::FrameEvent {kind:EventKind::Sever,snake_id:1,other_snake_id:0,
            other_generation:generation+1,..Default::default()};
        c.observe_event(&w,&sever);assert_eq!(c.used[EffectKind::Venom as usize],0);
        sever.other_generation=generation;
        c.observe_event(&w,&sever);c.observe_event(&w,&sever);
        assert_eq!(c.used[EffectKind::Venom as usize],1);
        c.observe_event(&w,&pickup);c.observe_event(&w,&sever);
        assert_eq!(c.used[EffectKind::Venom as usize],2);
    }
    #[test]
    fn magnet_counts_same_tick_consumed_spark_as_extended_and_used() {
        for consumed in [false,true] {for goal in [false,true] {for previous in [false,true] {
            let (w,mut c,mut tactics)=observer();let s=w.snake(0).unwrap();
            // radius 6, size 2: normal reach 20, Magnet reach 56.
            assert_eq!(s.radius,6.0);
            c.foods.push(FoodProbe {id:77,position:Point{x:424.0,y:300.0},size:2.0,
                unclaimed:true,stationary:true,scavenging:false});
            if !goal {tactics[0].target=0;}
            c.previous_food_goal[0]=previous;
            c.observe_magnet(&w,77,0,&tactics,consumed);
            c.observe_magnet(&w,77,0,&tactics,true); // An episode is counted once.
            assert_eq!(c.magnet_extended,1);
            assert_eq!(c.used[2],u64::from(goal || previous));
        }}}
        for distance in [20.0,56.0,56.01] {
            let (w,mut c,tactics)=observer();
            c.foods.push(FoodProbe {id:77,position:Point{x:400.0+distance,y:300.0},size:2.0,
                unclaimed:true,stationary:true,scavenging:false});
            c.observe_magnet(&w,77,0,&tactics,true);
            assert_eq!(c.magnet_extended,u64::from(distance>20.0 && distance<=56.0));
        }
    }
    #[test]
    fn magnet_snapshot_lookup_handles_sparse_ids_and_retained_claims() {
        let (w,mut c,tactics)=observer();
        for id in (0..MAX_FOOD as u64).rev() {
            c.foods.push(FoodProbe {id:id*17+1,position:Point{x:424.0,y:300.0},size:2.0,
                unclaimed:false,stationary:true,scavenging:false});
        }
        c.foods.sort_unstable_by_key(|f|f.id);
        for i in 0..MAX_FOOD as u64 {
            assert_eq!(c.food_probe(i*17+1).unwrap().id,i*17+1);
            assert!(c.food_probe(i*17+2).is_none());
            c.observe_magnet(&w,i*17+1,0,&tactics,false);
        }
        assert_eq!(c.magnet_extended,0,"existing vacuum claims are not new extra reach");
        assert_eq!(c.used[2],0);
        c.foods[0].scavenging=true;
        c.observe_magnet(&w,1,0,&tactics,true);
        assert_eq!(c.used[2],1,"consumed scavenging food remains intended use");
    }
}
