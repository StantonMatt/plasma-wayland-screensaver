// SPDX-License-Identifier: GPL-3.0-or-later
//! Brew targets share the existing tangent-orbit policy and physical rollouts.
use super::*;
impl AiController {
    pub(super) fn whirlpool_minimum(life:u16)->f64 {
        if life<=30 {0.01} else if life<=150 {15.0} else {18.0}
    }
    /// Burst food remains a shared feast after the item disappears. Its
    /// bounded cached index span bypasses the ordinary 64-particle discovery
    /// cap, which dense death fields can exhaust before reaching the shards.
    pub(super) fn whirlpool_feast_shortlist(&self,w:&World,s:SnakeView<'_>,state:&State,shortlist:&mut [(f64,usize);5]) {
        let Some((first,last))=self.essence else {return;};
        let head=s.segments[0].current;
        for index in first..=last {
            let Some(f)=self.food[index].filter(|f|f.feast_id&crate::world::whirlpool::ESSENCE_BIT!=0) else {continue;};
            if f.id==state.rejected && w.tick()<state.reject_until {continue;}
            let contact=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize));
            if contact.collected() {continue;}
            let eta=self.target_arrival(w,s,f);
            if eta+0.5>w.food[index].life {continue;}
            let d=w.displacement(head,f.position);let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
            let score=self.target_score(w,s,state,f,eta,bearing,f.value+500.0);
            if let Some(entry)=shortlist.iter_mut().find(|entry|entry.0>0.0 && entry.1==index) {
                entry.0=entry.0.max(score);shortlist.sort_by(|a,b|b.0.total_cmp(&a.0));
            } else {Self::shortlist(shortlist,score,index);}
        }
    }
    /// Query nutrition through the already-built food grid. The outer rim
    /// takes longer than the pull clock to absorb; count only the inner 21 r.
    pub(super) fn whirlpool_nutrition(&self,w:&World,p:Point)->f64 {
        let reach=21.0*w.config().base_radius();let root=self.spatial.key(p);
        let nx=(reach/self.spatial.dx).ceil() as isize+1;
        let ny=(reach/self.spatial.dy).ceil() as isize+1;
        let (xs,ys)=self.spatial.spans(root,-nx,nx,-ny,ny);
        let mut value=0.0;
        for y in ys {for x in xs.clone() {
            let Some(key)=self.spatial.offset(root,x,y) else {continue;};
            let mut at=self.spatial.food_heads[key];
            while at>=0 {
                let index=at as usize;at=self.spatial.food_next[index];
                let Some(f)=self.food[index] else {continue;};
                if crate::world::whirlpool::capturable(f.kind) && w.distance_squared(p,f.position)<=reach*reach {value+=f.value;}
            }
        }}
        value
    }
    pub(super) fn whirlpool_audience(&self,w:&World,s:SnakeView<'_>,p:Point)->bool {
        let mut alive=0;
        for (id,rival) in self.rivals.iter().enumerate() {
            if !rival.alive || id==s.id as usize {continue;}
            alive+=1;
            let radius=(12.0*w.config().base_radius()).max(rival.speed/rival.turn.max(0.01)*1.35);
            if self.arrival(w,rival,p)-radius/rival.speed.max(1.0)<3.5 {return true;}
        }
        // Single-snake diagnostic/low-population arenas can still brew.
        alive==0
    }
    /// Shortlist dense cells near the reachable forward route, then measure
    /// their exact brew value. Death shards and boost pellets share this grid;
    /// excluded Starfall prizes do not inflate the nutritional estimate.
    pub(super) fn whirlpool_placement(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        if w.vortex().is_some() || self.essence.is_some() || !s.inventory.kinds[..s.inventory.count as usize].contains(&7)
            || s.inventory.windup!=0 || w.tick()<state.dodge_until || state.debug.flags&32!=0 {return;}
        let head=s.segments[0].current;let (speed,turn)=self.motion[s.id as usize].at(0);
        let reach=speed*3.5;let root=self.spatial.key(head);
        let nx=(reach/self.spatial.dx).ceil() as isize;let ny=(reach/self.spatial.dy).ceil() as isize;
        let (xs,ys)=self.spatial.spans(root,-nx,nx,-ny,ny);
        let mut shortlist=[(0.0,0usize);5];
        for y in ys {for x in xs.clone() {
            let Some(key)=self.spatial.offset(root,x,y) else {continue;};
            if self.spatial.weight[key]<1.0 {continue;}
            let p=Point{x:(key%self.spatial.cols) as f64*self.spatial.dx+self.spatial.dx*0.5,
                y:(key/self.spatial.cols) as f64*self.spatial.dy+self.spatial.dy*0.5};
            let d=w.displacement(head,p);let distance=(d.x*d.x+d.y*d.y).sqrt();
            let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
            if distance>reach || bearing>1.2 || distance<speed/turn.max(0.01)*bearing {continue;}
            Self::shortlist(&mut shortlist,self.spatial.cluster_weight(key)/(1.0+distance/speed),key);
        }}
        let life=s.inventory.kinds[..s.inventory.count as usize].iter().zip(s.inventory.life).filter(|(kind,_)|**kind==7).map(|(_,life)|life).min().unwrap();
        let mut best=Self::whirlpool_minimum(life);let mut goal=None;
        for (score,key) in shortlist {
            if score<=0.0 {continue;}
            let p=Point{x:(key%self.spatial.cols) as f64*self.spatial.dx+self.spatial.dx*0.5,
                y:(key/self.spatial.cols) as f64*self.spatial.dy+self.spatial.dy*0.5};
            let value=self.whirlpool_nutrition(w,p)/(1.0+w.distance_squared(head,p).sqrt()/speed*0.08);
            if value>best && self.whirlpool_audience(w,s,p) {let mut tests=0;
                if !self.body_blocked(w,s,head,p,0.0,0.0,&mut tests).0 {best=value;goal=Some(p);}
            }
        }
        if let Some(p)=goal {
            state.set_target(0);state.prey=0;state.clear_attacks(s.angle);state.clear_coil(s.angle);
            state.goal=p;state.track_goal=true;state.waypoint=None;
            state.best_distance=f64::MAX;state.last_progress=w.tick();
        }
    }
    pub(super) fn whirlpool_value(&self,w:&World,s:SnakeView<'_>,v:&crate::Item,eta:f64)->f64 {
        let holder=v.owner_snake_id==s.id && v.owner_generation==s.generation;
        let predicted=v.captured_value as f64+w.food.iter().filter(|f|f.captured_by!=0).map(|f|f.value).sum::<f64>();
        (predicted+12.0)*80.0*if holder {1.2} else {0.55+0.75*aggression::level(w)} /(1.0+eta*0.06)
    }
    pub(super) fn whirlpool_orbit(&self,w:&World,s:SnakeView<'_>,state:&mut State)->bool {
        let Some(f)=self.target_food(*state).filter(|f|f.kind==crate::FoodKind::Meteor && f.item_kind==7) else {return false;};
        let Some(v)=w.vortex().filter(|v|v.id==f.id & !target::ITEM_BIT) else {state.vulturing=false;return false;};
        let holder=v.owner_snake_id==s.id && v.owner_generation==s.generation;
        let (speed,turn)=self.motion[s.id as usize].at(0);
        // Thaw, Surge and Nightfall motion bounds also determine orbit admission.
        let r=w.config().base_radius();
        let tighten=0.7+0.3*(v.life_ticks as f64/45.0).min(1.0);
        let radius=(r*if holder {9.0} else {15.0-3.0*aggression::level(w)}*tighten)
            .max(speed/turn.max(0.01)*1.35)
            .max((s.segments.len() as f64*s.radius*1.18).min(speed*3.0)/std::f64::consts::TAU*1.1);
        let head=s.segments[0].current;let d=w.displacement(v.position,head);
        let distance=(d.x*d.x+d.y*d.y).sqrt();
        let near=distance<radius*1.7+3.0*s.radius;
        let direction=Point{x:s.angle.cos(),y:s.angle.sin()};
        let admitted=wall::reachable_circle(w,head,direction,wall::Circle::new(speed/turn.max(0.01),speed*STEP_SECONDS),s.radius);
        state.guarding=false;state.vulturing=near && admitted;
        if state.vulturing {
            if state.vulture_until==0 {
                let cross=d.x*direction.y-d.y*direction.x;
                state.coil_sign=if cross.abs()>s.radius {cross.signum()} else if s.traits.turn_bias<0.0 {-1.0} else {1.0};
            }
            state.vulture_until=w.tick()+v.life_ticks as u64;
            state.guard_radius=radius;state.vulture_center=v.position;state.goal=v.position;
            state.waypoint=None;state.track_goal=true;state.turn_accum=0.0;
            state.best_distance=f64::MAX;state.last_progress=w.tick();
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn patch(w:&mut World,p:Point,count:usize,value:f64,kind:crate::FoodKind) {
        for _ in 0..count {w.food.push(crate::world::Food{id:100+w.food.len() as u64,p,value,kind,
            life:100.0,original_life:100.0,owner:-1,size:2.0,..Default::default()});}
    }
    #[test]
    fn whirlpool_density_counts_nutrition_and_wraps_without_duplicate_cells() {
        let mut w=arena();let p=Point{x:2.0,y:1000.0};
        patch(&mut w,Point{x:3998.0,y:1000.0},3,8.0,crate::FoodKind::Shard);
        patch(&mut w,p,20,9.0,crate::FoodKind::Star);
        patch(&mut w,p,20,9.0,crate::FoodKind::Meteor);
        patch(&mut w,Point{x:500.0,y:1000.0},5,3.0,crate::FoodKind::Pellet);
        let mut ai=AiController::new();ai.prepare(&w);
        assert_eq!(ai.whirlpool_nutrition(&w,p),24.0);
        // A small wrapped arena's query spans every cell exactly once.
        w.resize(80.0,80.0).unwrap();w.food.clear();patch(&mut w,Point{x:40.0,y:40.0},3,8.0,crate::FoodKind::Shard);
        ai.prepare(&w);assert_eq!(ai.whirlpool_nutrition(&w,Point{x:1.0,y:1.0}),24.0);
    }
    #[test]
    fn whirlpool_holder_saves_low_value_patch_and_uses_checked_rich_endpoint() {
        let mut w=arena();w.snakes[1].alive=false;
        w.snakes[0].inventory=crate::Inventory{kinds:[7,0,0],life:[1800,0,0],count:1,..Default::default()};
        let head=w.segments[0].current;
        patch(&mut w,Point{x:head.x+70.0,y:head.y},4,1.0,crate::FoodKind::Shard);
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State{desired:0.0,turn_until:u64::MAX,..Default::default()};
        let mut baseline=Candidate::default();ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut baseline);
        ai.choose_inventory(&w,w.snake(0).unwrap(),&mut state,&baseline,30);assert_eq!(state.use_slot,0);
        w.food.clear();patch(&mut w,Point{x:head.x+70.0,y:head.y},4,8.0,crate::FoodKind::Shard);
        ai.tick=u64::MAX;ai.prepare(&w);ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut baseline);
        ai.choose_inventory(&w,w.snake(0).unwrap(),&mut state,&baseline,30);assert_eq!(state.use_slot,1);
        state.use_slot=0;state.debug.flags=32;
        ai.choose_inventory(&w,w.snake(0).unwrap(),&mut state,&baseline,30);assert_eq!(state.use_slot,0,"never use into danger");
        state.debug.flags=0;w.food.clear();patch(&mut w,Point{x:head.x+70.0,y:head.y},4,1.0,crate::FoodKind::Pellet);
        w.snakes[0].inventory.life[0]=25;ai.tick=u64::MAX;ai.prepare(&w);
        ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut baseline);
        ai.choose_inventory(&w,w.snake(0).unwrap(),&mut state,&baseline,30);assert_eq!(state.use_slot,1,"safe expiry fallback");
    }
    #[test]
    fn whirlpool_later_rival_is_admitted_to_multi_prize_brew() {
        let mut w=arena();w.open_whirlpool(0,Point{x:1500.0,y:1000.0});
        let mut ai=AiController::new();ai.prepare(&w);ai.prepare_races(&w);
        let f=ai.food[MAX_FOOD].unwrap();let s=w.snake(1).unwrap();
        assert!(ai.capsule_etas[0][1]>ai.capsule_etas[0][0]);
        assert!(ai.target_score(&w,s,&State::default(),f,1.0,0.0,100.0)>0.0);
    }
    #[test]
    fn whirlpool_placement_prefers_reachable_forward_density() {
        let mut w=arena();w.snakes[1].alive=false;
        w.snakes[0].inventory=crate::Inventory{kinds:[7,0,0],life:[1800,0,0],count:1,..Default::default()};
        let head=w.segments[0].current;
        patch(&mut w,Point{x:head.x-250.0,y:head.y},10,8.0,crate::FoodKind::Shard);
        patch(&mut w,Point{x:head.x+250.0,y:head.y},10,5.0,crate::FoodKind::Pellet);
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State{goal:head,..Default::default()};
        ai.whirlpool_placement(&w,w.snake(0).unwrap(),&mut state);
        assert!(state.goal.x>head.x);assert!(state.track_goal);
        assert!(ai.whirlpool_nutrition(&w,state.goal)>=18.0);
    }
    #[test]
    fn whirlpool_feast_discovery_promotes_already_seen_shards_without_duplicates() {
        let mut w=arena();let p=Point{x:1700.0,y:1000.0};patch(&mut w,p,1,0.7,crate::FoodKind::Shard);
        w.food[0].feast=crate::world::whirlpool::ESSENCE_BIT|44;
        let mut ai=AiController::new();ai.prepare(&w);
        let mut shortlist=[(2.0,0),(0.0,0),(0.0,0),(0.0,0),(0.0,0)];
        ai.whirlpool_feast_shortlist(&w,w.snake(0).unwrap(),&State::default(),&mut shortlist);
        assert!(shortlist[0].0>2.0);assert_eq!(shortlist.iter().filter(|entry|entry.0>0.0).count(),1);
        let f=ai.food[0].unwrap();let s=w.snake(0).unwrap();
        let forward=ai.target_score(&w,s,&State::default(),f,1.0,0.0,500.0);
        let behind=ai.target_score(&w,s,&State::default(),f,1.0,std::f64::consts::PI,500.0);
        assert!(behind>=forward*0.5,"an orbiter can return for the shared burst");
        w.snakes[0].inventory=crate::Inventory{kinds:[7,0,0],life:[1800,0,0],count:1,..Default::default()};
        patch(&mut w,p,10,8.0,crate::FoodKind::Shard);ai.tick=u64::MAX;ai.prepare(&w);
        let mut state=State{target:ai.food[0].unwrap().id,goal:p,..Default::default()};
        ai.whirlpool_placement(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.target,ai.food[0].unwrap().id,"finish feeding instead of planning another brew");
        w.food[0].p=Point{x:2800.0,y:1000.0};ai.tick=u64::MAX;ai.prepare(&w);
        assert!(ai.target_arrival(&w,w.snake(0).unwrap(),ai.food[0].unwrap())>8.0);
        shortlist.fill((0.0,0));
        ai.whirlpool_feast_shortlist(&w,w.snake(0).unwrap(),&State::default(),&mut shortlist);
        assert!(shortlist.iter().any(|entry|entry.0>0.0 && entry.1==0),"available distant feast can attract arrivals");
        w.food[0].life=1.0;shortlist.fill((0.0,0));
        ai.whirlpool_feast_shortlist(&w,w.snake(0).unwrap(),&State::default(),&mut shortlist);
        assert!(!shortlist.iter().any(|entry|entry.0>0.0 && entry.1==0),"do not race an expiring distant shard");
    }
    #[test]
    fn whirlpool_hunter_nominates_committed_orbiter_only_near_burst() {
        let mut w=World::diagnostic_arena(crate::Config{rules:crate::RuleSet::V2,width:4000.0,height:2000.0,
            deadly_walls:false,world_events:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,60,0.9),(Point{x:1700.0,y:1100.0},2.0,20,0.0)],&[]).unwrap();
        w.open_whirlpool(0,Point{x:1800.0,y:1000.0});let mut ai=AiController::new();ai.prepare(&w);
        let f=ai.food[MAX_FOOD].unwrap();let state=State{target:f.id,target_index:MAX_FOOD,..Default::default()};
        ai.states[1]=State{generation:1,target:f.id,vulturing:true,..Default::default()};
        assert_eq!(ai.capsule_contender(&w,w.snake(0).unwrap(),state),None);
        w.items[0].life_ticks=60;
        assert_eq!(ai.capsule_contender(&w,w.snake(0).unwrap(),state),Some(1));
    }
    #[test]
    fn whirlpool_deliberate_placement_waits_for_a_reachable_audience() {
        let mut w=World::diagnostic_arena(crate::Config{rules:crate::RuleSet::V2,width:4000.0,height:2000.0,
            deadly_walls:false,world_events:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:3500.0,y:1800.0},0.0,24,0.0),
                (Point{x:3500.0,y:1700.0},0.0,24,0.0)],&[]).unwrap();
        let p=w.segments[0].current;let mut ai=AiController::new();ai.prepare(&w);
        assert!(!ai.whirlpool_audience(&w,w.snake(0).unwrap(),p));
        w.segments[MAX_SEGMENTS].current=Point{x:1700.0,y:1000.0};w.snakes[1].angle=std::f64::consts::PI;
        ai.tick=u64::MAX;ai.prepare(&w);assert!(ai.whirlpool_audience(&w,w.snake(0).unwrap(),p));
        w.segments[MAX_SEGMENTS*2].current=Point{x:1900.0,y:1000.0};w.snakes[2].angle=std::f64::consts::PI;
        ai.tick=u64::MAX;ai.prepare(&w);assert!(ai.whirlpool_audience(&w,w.snake(0).unwrap(),p));
    }
    #[test]
    fn whirlpool_strategy_keeps_brew_orbit_when_holder_body_blocks_center() {
        let mut w=arena();w.open_whirlpool(0,w.segments[10].current);
        let mut ai=AiController::new();ai.prepare(&w);
        let f=ai.food[MAX_FOOD].unwrap();let s=w.snake(1).unwrap();
        let mut state=State{target:f.id,target_index:MAX_FOOD,generation:s.generation,..Default::default()};
        ai.strategy(&w,s,&mut state,1.0);
        assert_eq!(state.target,f.id);assert!(state.vulturing);assert!(state.waypoint.is_none());
    }
    #[test]
    fn whirlpool_checked_feast_capture_receives_prize_continuity_reward() {
        let mut w=arena();w.snakes[1].alive=false;
        let p=Point{x:1600.0,y:1000.0};patch(&mut w,p,1,0.7,crate::FoodKind::Shard);
        let mut ai=AiController::new();ai.prepare(&w);
        let state=State{target:100,target_index:0,goal:p,desired:0.0,track_goal:true,turn_until:u64::MAX,..Default::default()};
        let mut ordinary=Candidate::default();ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut ordinary);
        w.food[0].feast=crate::world::whirlpool::ESSENCE_BIT|44;ai.tick=u64::MAX;ai.prepare(&w);
        let mut feast=Candidate::default();ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut feast);
        assert_eq!(ordinary.steps,30);assert_eq!(feast.steps,ordinary.steps);
        assert!((feast.score-ordinary.score-600.0).abs()<1e-9);
    }
    #[test]
    fn whirlpool_burst_shard_preempts_retained_hunt_and_corpse_harvest() {
        let mut w=arena();let p=Point{x:1600.0,y:1000.0};patch(&mut w,p,1,0.7,crate::FoodKind::Shard);
        w.food[0].feast=crate::world::whirlpool::ESSENCE_BIT|44;
        let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();let rival=w.snake(1).unwrap();
        let shard=ai.food[0].unwrap().id;
        // Between tactical responses a hunt is retained; a corpse-harvest
        // deadline returns early from tactics. Neither may drop the meal.
        for harvest in [false,true] {
            let mut state=State{generation:s.generation,next_response:u64::MAX,
                prey:if harvest {0} else {2},prey_generation:rival.generation,hunt_until:u64::MAX,
                harvest_until:if harvest {u64::MAX} else {0},..Default::default()};
            if harvest {state.next_response=0;}
            ai.strategy(&w,s,&mut state,1.0);
            assert_eq!((state.target,state.prey,state.harvest_until),(shard,0,0),"harvest={harvest}");
            assert!(state.track_goal);
        }
    }
    fn arena()->World {World::diagnostic_arena(crate::Config{rules:crate::RuleSet::V2,width:4000.0,height:2000.0,
        deadly_walls:false,self_collisions:true,world_events:false,..Default::default()},
        &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:1600.0,y:1100.0},0.0,24,0.0)],&[]).unwrap()}
    #[test]
    fn whirlpool_captured_food_is_absent_from_all_queries_and_direct_contact() {
        let mut w=arena();w.food.push(crate::world::Food{id:77,p:w.segments[0].current,kind:crate::FoodKind::Shard,
            owner:0,captured_by:1,size:2.0,value:2.0,life:20.0,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);assert!(ai.food[0].is_none());
        let f=w.foods().next().unwrap();let contact=target::Contact::new(&w,w.snake(0).unwrap(),f);
        assert!(!contact.reached(0.0,1));assert!(!contact.reached(0.0,usize::MAX));
        assert_eq!(ai.spatial.cluster_weight(ai.spatial.key(f.position)),0.0);
        assert!(!ai.flip_nearby(&w,w.snake(0).unwrap()));
    }
    #[test]
    fn whirlpool_holder_and_rival_orbits_have_distinct_radii() {
        let mut w=arena();w.open_whirlpool(0,Point{x:1550.0,y:1000.0});let mut ai=AiController::new();ai.prepare(&w);
        for id in 0..2 {let mut state=State{target:w.items[0].id|target::ITEM_BIT,target_index:MAX_FOOD,..Default::default()};
            assert!(ai.whirlpool_orbit(&w,w.snake(id).unwrap(),&mut state));assert!(state.vulturing);
            assert!(state.guard_radius>=w.config().base_radius()*if id==0 {9.0} else {12.0});
            assert_eq!(state.vulture_center,w.vortex().unwrap().position);
        }
        let s=w.snake(0).unwrap();let (speed,turn)=ai.motion[0].at(0);
        let mut state=State{target:w.items[0].id|target::ITEM_BIT,target_index:MAX_FOOD,..Default::default()};
        ai.whirlpool_orbit(&w,s,&mut state);let radius=state.guard_radius;
        w.items[0].life_ticks=15;ai.whirlpool_orbit(&w,w.snake(0).unwrap(),&mut state);
        assert!(state.guard_radius<radius);assert!(state.guard_radius>=speed/turn.max(0.01)*1.35);
    }
    #[test]
    fn whirlpool_pending_completions_preserve_effects_and_capture_on_next_world_step() {
        use crate::{controller::{ScriptedController,Steering},effects::EffectKind};
        for effect in [EffectKind::Surge,EffectKind::Phase,EffectKind::Magnet,EffectKind::Venom] {
            let mut w=arena();
            for id in 0..2 {
                w.snakes[id].effect_kind=effect as u8;w.snakes[id].effect_ticks=300;
                w.snakes[id].inventory=crate::Inventory{kinds:[7,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:0,..Default::default()};
            }
            let snapshot=w.diagnostic_snapshot();
            let mut forecast=forecast::Forecast::empty(forecast::Timeline::new(&snapshot));
            w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0}));
            let positions=std::array::from_fn::<_,MAX_SNAKES,_>(|id|w.snake(id).filter(|s|s.alive).map_or(Point::default(),|s|s.segments[0].current));
            forecast.advance_windups(&snapshot,1,|id|positions[id]);
            assert_eq!(w.vortex().unwrap().position,positions[0]);assert_eq!(w.vortex().unwrap().owner_snake_id,0);
            assert_eq!(w.snakes[0].effect_kind,effect as u8);assert_eq!(w.snakes[1].effect_kind,effect as u8);
            let p=Point{x:positions[0].x+200.0,y:positions[0].y};
            w.food.push(crate::world::Food{id:88,p,value:1.0,life:20.0,original_life:20.0,kind:crate::FoodKind::Shard,owner:-1,..Default::default()});
            let f:target::TargetFood=w.foods().last().unwrap().into();
            assert!(forecast.effects.food_available(&w,f,1));assert!(!forecast.effects.food_available(&w,f,2));
            w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0}));
            assert!(!w.food.iter().find(|f|f.id==88).unwrap().pickup_eligible(w.tick()));
            assert!(w.frame_events().all(|e|e.kind!=crate::EventKind::Pickup || e.other_snake_id!=7));
        }
    }

    #[test]
    fn whirlpool_moving_food_forecast_matches_step_order_cutoff_and_recreation() {
        use crate::controller::{Controller,ScriptedController,Steering};
        struct Observe {snapshot:Option<World>}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.snapshot=Some(w.diagnostic_snapshot());}
                Steering{desired_angle:s.angle,rush:0.0}
            }
        }
        for (age,offset,velocity,captured) in [(0,3.0,-60.0,true),(136,5.0,-80.0,false)] {
            let mut w=arena();let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
            w.items[0].age_ticks=age;w.items[0].charge_ticks=age;w.items[0].life_ticks=150-age;
            let p=Point{x:center.x+w.items[0].radius+offset,y:center.y};
            w.food.push(crate::world::Food{id:88,p,velocity:Point{x:velocity,y:0.0},value:1.0,
                life:20.0,original_life:20.0,kind:crate::FoodKind::Shard,owner:-1,..Default::default()});
            let mut observe=Observe{snapshot:None};w.step(&mut observe);
            let snapshot=observe.snapshot.unwrap();let f:target::TargetFood=snapshot.foods().find(|f|f.id==88).unwrap().into();
            let timeline=forecast::Timeline::new(&snapshot);
            assert!(timeline.food_available(&snapshot,f,1),"movement one observes food after update/capture");
            w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0}));
            assert_eq!(w.food.iter().find(|f|f.id==88).unwrap().captured_by!=0,captured);
            assert_eq!(!timeline.food_available(&snapshot,f,2),captured);
            if captured {assert!(!timeline.food_available(&snapshot,f,140));assert!(timeline.food_available(&snapshot,f,150));}
            w.reconfigure(crate::Config{power_ups:false,..w.config()}).unwrap();
            assert!(w.food.iter().all(|f|f.captured_by==0));
            w.reconfigure(crate::Config{power_ups:true,..w.config()}).unwrap();
            w.open_whirlpool(0,Point{x:500.0,y:500.0});
            let f:target::TargetFood=w.foods().find(|f|f.id==88).unwrap().into();
            assert!(forecast::Timeline::new(&w).food_available(&w,f,2));
        }
    }
    #[test]
    fn whirlpool_pending_capture_revokes_vacuum_reward_but_not_consumption() {
        use crate::controller::{ScriptedController,Steering};
        for near in [false,true] {
            let mut w=arena();w.snakes[0].effect_kind=crate::effects::EffectKind::Magnet as u8;w.snakes[0].effect_ticks=300;
            w.snakes[1].inventory=crate::Inventory{kinds:[7,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:0,..Default::default()};
            let p=Point{x:1500.0+if near {5.0} else {w.snakes[0].radius*7.0},y:1000.0};
            patch(&mut w,p,1,0.7,crate::FoodKind::Shard);
            let state=State{target:100,target_index:0,goal:p,desired:0.0,track_goal:true,turn_until:u64::MAX,..Default::default()};
            let mut ai=AiController::new();ai.prepare(&w);
            let mut ordinary=Candidate::default();ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut ordinary);
            w.food[0].feast=crate::world::whirlpool::ESSENCE_BIT|44;ai.tick=u64::MAX;ai.prepare(&w);
            let mut feast=Candidate::default();ai.rollout_into(&w,w.snake(0).unwrap(),state,1,30,&mut feast);
            assert_eq!(ordinary.steps,feast.steps);
            assert!((feast.score-ordinary.score-if near {600.0} else {0.0}).abs()<1e-9,"near={near}, ordinary={}, feast={}",ordinary.score,feast.score);
            let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0});
            w.step(&mut straight);
            if near {assert!(w.consumption_events().any(|e|e.0==100));} else {
                assert_eq!(w.food.iter().find(|f|f.id==100).unwrap().owner,0);
                w.step(&mut straight);
                let f=w.food.iter().find(|f|f.id==100).unwrap();assert_eq!(f.owner,-1);assert_ne!(f.captured_by,0);
            }
        }
    }

    #[test]
    fn whirlpool_moving_food_forecast_matches_wrapping_and_wall_bounce() {
        use crate::controller::{Controller,ScriptedController,Steering};
        struct Observe {snapshot:Option<World>}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.snapshot=Some(w.diagnostic_snapshot());}
                Steering{desired_angle:s.angle,rush:0.0}
            }
        }
        for deadly in [false,true] {
            let mut w=arena();w.reconfigure(crate::Config{deadly_walls:deadly,..w.config()}).unwrap();
            let center=Point{x:if deadly {135.0} else {1.0},y:1000.0};w.open_whirlpool(0,center);
            let p=Point{x:if deadly {3.0} else {w.config().width-w.items[0].radius-2.0},y:1000.0};
            w.food.push(crate::world::Food{id:88,p,velocity:Point{x:if deadly {-120.0} else {60.0},y:0.0},value:1.0,
                life:20.0,original_life:20.0,kind:crate::FoodKind::Shard,owner:-1,..Default::default()});
            let mut observe=Observe{snapshot:None};w.step(&mut observe);let snapshot=observe.snapshot.unwrap();
            let f:target::TargetFood=snapshot.foods().find(|f|f.id==88).unwrap().into();
            let timeline=forecast::Timeline::new(&snapshot);
            assert_eq!(timeline.food_available(&snapshot,f,1),snapshot.food.iter().find(|raw|raw.id==88).unwrap().captured_by==0);
            assert!(!timeline.food_available(&snapshot,f,2),"deadly={deadly}");
            w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0}));
            assert_ne!(w.food.iter().find(|f|f.id==88).unwrap().captured_by,0);
        }
    }

    #[test]
    fn whirlpool_forecast_release_and_absorption_match_world_step_order() {
        use crate::controller::{Controller,ScriptedController,Steering};
        struct Observe {snapshot:Option<World>}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.snapshot=Some(w.diagnostic_snapshot());}
                Steering{desired_angle:s.angle,rush:0.0}
            }
        }
        for (absorbed,calm) in [(false,false),(false,true),(true,false),(true,true)] {
            let mut w=arena();w.set_reduced_motion(calm);
            let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
            w.items[0].age_ticks=134;w.items[0].life_ticks=16;
            let radius=w.items[0].radius;
            let velocity=if absorbed {(radius+3.0-w.config().base_radius()*0.5)/(STEP_SECONDS*0.16_f64.powf(STEP_SECONDS))} else {60.0};
            let p=Point{x:center.x-radius-3.0-velocity*STEP_SECONDS,y:center.y};
            patch(&mut w,p,1,1.0,crate::FoodKind::Shard);
            w.food[0].velocity=Point{x:velocity,y:0.0};
            let mut observe=Observe{snapshot:None};w.step(&mut observe);
            let snapshot=observe.snapshot.unwrap();
            let f:target::TargetFood=snapshot.foods().find(|f|f.id==100).unwrap().into();
            let mut ai=AiController::new();ai.prepare(&snapshot);
            let end=ai.opportunities.brew.end as usize;
            let mut motion=forecast::FoodMotion::new(&snapshot,f);
            let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0});
            // Snapshot was taken after food/capture but before movement one.
            for step in 2..=end+2 {
                w.step(&mut straight);
                let raw=w.food.iter().find(|f|f.id==100);
                let available=raw.is_some_and(|f|f.captured_by==0);
                motion.advance_brew(&snapshot,ai.opportunities.brew,f.kind,step);
                assert_eq!(!motion.unavailable(),available,"transport absorbed={absorbed}, calm={calm}, step={step}");
                if let Some(raw)=raw {assert!(snapshot.distance_squared(motion.position,raw.p)<1e-18,"step={step}, motion={:?}, raw={:?}",motion.position,raw.p);}
                assert_eq!(ai.opportunities.food_available(&snapshot,f,step),available,"absorbed={absorbed}, step={step}");
            }
            assert!(w.vortex().is_none());
            assert_eq!(w.food.iter().any(|f|f.id==100),!absorbed);
            let expected=w.whirlpool_stats.captured_value+6.0;
            let burst=w.food.iter().filter(|f|f.feast&crate::world::whirlpool::ESSENCE_BIT!=0).map(|f|f.value).sum::<f64>();
            assert!((burst-expected).abs()<1e-6);
        }
    }

    #[test]
    fn whirlpool_rollout_values_recaptured_survivor_only_after_burst() {
        use crate::controller::{Controller,ScriptedController,Steering};
        struct Observe {snapshot:Option<World>}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.snapshot=Some(w.diagnostic_snapshot());}
                Steering{desired_angle:s.angle,rush:0.0}
            }
        }
        let mut w=arena();w.set_reduced_motion(true);
        let speed=w.motion_limits(0,0.0).unwrap().0;
        let center=Point{x:1750.0,y:1000.0};w.open_whirlpool(0,center);
        w.items[0].age_ticks=134;w.items[0].life_ticks=16;
        let p=Point{x:center.x-w.items[0].radius-5.0,y:center.y};
        patch(&mut w,p,1,1.0,crate::FoodKind::Shard);
        w.food[0].velocity=Point{x:60.0,y:0.0};
        let head=Point{x:p.x-speed*STEP_SECONDS*18.0-w.snakes[0].radius*3.0,y:p.y};
        let body:Vec<_>=(0..24).map(|i|Point{x:head.x-i as f64*w.snakes[0].radius*1.18,y:head.y}).collect();
        w.diagnostic_body(0,&body,0.0).unwrap();
        let mut observe=Observe{snapshot:None};w.step(&mut observe);
        let snapshot=observe.snapshot.unwrap();let p=snapshot.food[0].p;
        let state=State{target:100,target_index:0,goal:p,desired:0.0,track_goal:true,turn_until:u64::MAX,..Default::default()};
        let mut ai=AiController::new();ai.prepare(&snapshot);
        let mut ordinary=Candidate::default();ai.rollout_into(&snapshot,snapshot.snake(0).unwrap(),state,1,30,&mut ordinary);
        let mut marked=snapshot.diagnostic_snapshot();marked.food[0].feast=crate::world::whirlpool::ESSENCE_BIT|44;
        ai.tick=u64::MAX;ai.prepare(&marked);
        let end=ai.opportunities.brew.end as usize;
        let mut feast=Candidate::default();ai.rollout_into(&marked,marked.snake(0).unwrap(),state,1,30,&mut feast);
        assert_eq!(ordinary.steps,30);assert_eq!(feast.steps,30);
        assert!((feast.score-ordinary.score-600.0).abs()<1e-9,"ordinary={}, feast={}",ordinary.score,feast.score);
        let mut early=Candidate::default();ai.rollout_into(&marked,marked.snake(0).unwrap(),state,1,end-1,&mut early);
        let mut unmarked=AiController::new();unmarked.prepare(&snapshot);
        let mut early_ordinary=Candidate::default();unmarked.rollout_into(&snapshot,snapshot.snake(0).unwrap(),state,1,end-1,&mut early_ordinary);
        assert!((early.score-early_ordinary.score).abs()<1e-9);
        // Playback validates the cached rollout's straight path and confirms
        // consumption only after the mechanics' release-before-feeding pass.
        let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0});
        let mut eaten_at=None;
        for step in 2..=30 {
            w.step(&mut straight);
            assert!(w.distance_squared(w.segments[0].current,feast.path[step])<1e-18,"step={step}");
            if w.consumption_events().any(|e|e.0==100) {eaten_at=Some(step);break;}
        }
        assert!(eaten_at.is_some_and(|step|step>=end));
    }

    #[test]
    fn whirlpool_release_does_not_restore_a_revoked_vacuum_claim() {
        use crate::controller::{Controller,ScriptedController,Steering};
        struct Observe {snapshot:Option<World>}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.snapshot=Some(w.diagnostic_snapshot());}
                Steering{desired_angle:s.angle,rush:0.0}
            }
        }
        let mut w=arena();w.set_reduced_motion(true);
        let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
        w.items[0].age_ticks=135;w.items[0].life_ticks=15;
        // Existing ownership is irrevocable only after physical consumption.
        // Make the observed claim before the vortex's next ordered update.
        let p=Point{x:center.x-w.items[0].radius+50.0,y:center.y};
        patch(&mut w,p,1,1.0,crate::FoodKind::Shard);
        w.food[0].owner=0;w.food[0].life=-1.0;
        let snapshot=w.diagnostic_snapshot();let state=State{target:100,target_index:0,goal:p,desired:0.0,track_goal:true,turn_until:u64::MAX,..Default::default()};
        let mut ai=AiController::new();ai.prepare(&snapshot);
        let mut ordinary=Candidate::default();ai.rollout_into(&snapshot,snapshot.snake(0).unwrap(),state,1,30,&mut ordinary);
        let mut marked=snapshot.diagnostic_snapshot();marked.food[0].feast=crate::world::whirlpool::ESSENCE_BIT|44;
        ai.tick=u64::MAX;ai.prepare(&marked);
        let mut feast=Candidate::default();ai.rollout_into(&marked,marked.snake(0).unwrap(),state,1,30,&mut feast);
        assert_eq!(ordinary.steps,30);assert_eq!(feast.steps,30);
        assert!((feast.score-ordinary.score).abs()<1e-9,"release must require fresh physical contact");
        // First real update captures before feeding; the burst releases the
        // survivor without returning it to the distant former owner.
        let mut observe=Observe{snapshot:None};w.step(&mut observe);
        let raw=w.food.iter().find(|f|f.id==100).unwrap();assert_ne!(raw.captured_by,0);assert_eq!(raw.owner,-1);
        w.step_n(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0}),14);
        let raw=w.food.iter().find(|f|f.id==100).unwrap();assert_eq!(raw.captured_by,0);assert_eq!(raw.owner,-1);
        assert!(w.consumption_events().all(|e|e.0!=100));
    }

    #[test]
    fn whirlpool_forecast_preserves_excluded_prism_and_star_food() {
        use crate::controller::{ScriptedController,Steering};
        let mut w=arena();let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
        w.items[0].age_ticks=135;w.items[0].life_ticks=15;
        for kind in [crate::FoodKind::Prism,crate::FoodKind::PrismSeed,crate::FoodKind::Star] {patch(&mut w,center,1,1.0,kind);}
        let snapshot=w.diagnostic_snapshot();let mut ai=AiController::new();ai.prepare(&snapshot);
        let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0});
        for step in 1..=20 {
            w.step(&mut straight);
            for id in 100..103 {
                let f:target::TargetFood=snapshot.foods().find(|f|f.id==id).unwrap().into();
                assert!(ai.opportunities.food_available(&snapshot,f,step));
                assert_eq!(w.food.iter().find(|f|f.id==id).unwrap().captured_by,0);
            }
        }
    }

}
