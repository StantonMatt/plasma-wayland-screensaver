// SPDX-License-Identifier: GPL-3.0-or-later
//! Real-desktop intent observer. Requires --features desktop-diag.
#[allow(dead_code)]
#[path="support/item_lifecycle.rs"] mod item_lifecycle;
#[cfg(feature="desktop-diag")]
#[path="support/desktop_metrics.rs"]
mod desktop_metrics;
#[cfg(feature="desktop-diag")]
#[path="support/death_leadups.rs"]
mod death_leadups;
#[cfg(feature="desktop-diag")]
#[path="support/long_fixtures.rs"]
mod long_fixtures;
#[cfg(feature="desktop-diag")]
#[path="support/length_metrics.rs"]
mod length_metrics;
#[cfg(feature="desktop-diag")]
#[path="support/inventory_metrics.rs"]
mod inventory_metrics;
#[cfg(not(feature="desktop-diag"))]
fn main() {panic!("enable desktop-diag");}
#[cfg(feature="desktop-diag")]
mod diag {
use snakes_core::{ai::{AiController,DesktopObservation},controller::{Controller,Steering},*};
use super::{desktop_metrics::Metrics,death_leadups::Leadups,item_lifecycle::{lifecycle,EffectEpisode}};
use std::{collections::{HashMap,VecDeque},io::{BufWriter,Write},fs::File,time::Instant};
const ITEM_BIT:u64=1<<63;
const MODES:[&str;12]=["wander","recovery","escape","coil","guard","vulture","standoff","venom","hunt","capsule","prism","food"];
#[derive(Clone,Copy,Default)] struct Sample {generation:u32,turn:f64,position:Point,distance:f64,target:u64,mode:u8,ate:u64}
struct Observer {inner:AiController,obs:[DesktopObservation;MAX_SNAKES],called:[bool;MAX_SNAKES]}
impl Controller for Observer {
    fn delegate(&self,_id:u32)->Option<&dyn Controller> {Some(&self.inner)}
 fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {let out=self.inner.steer(w,s);self.obs[s.id as usize]=self.inner.desktop_observation(s.id as usize);self.called[s.id as usize]=true;out}
}
#[derive(Default)] struct Counts {ticks:u64, modes:[u64;12],circles:[u64;12],recoveries:u64,progress_rejections:u64,
 near_wall:u64,invisible:u64,near_gap:u64,food:u64,intended:u64,food_value:f64,intended_value:f64,
 capsules:u64,caps_intended:u64,prisms:u64,prisms_intended:u64,targets:u64,target_dist:f64,
 selections:[u64;13],strategy:u64,direct_safe:u64,direct_viable:u64,viable_ignored:u64,
 safety_veto:u64,tracked:u64,behind:u64,length:u64,patch_intended:u64,cap_recent:u64,recover_caps:u64,recover_targets:u64,recover_progress:u64,venom_picks:u64,venom_used:u64}
struct FoodRecord {claim:Option<(u32,u32,bool,bool,bool)>,kind:FoodKind}
fn near_gap(p:Point)->bool {p.x>5940.0 && ((p.y-195.0).abs()<60.0 || (p.y-1275.0).abs()<60.0 || (p.x-6000.0).abs()<60.0 && (p.y<195.0 || p.y>1275.0))}
fn visible(p:Point)->bool {p.x<6000.0 || p.y>=195.0 && p.y<1275.0}
fn replay(w:&World,obs:&[DesktopObservation;MAX_SNAKES],out:&mut BufWriter<File>) {
 write!(out,"{{\"tick\":{},\"snakes\":[",w.tick()).unwrap();let mut comma="";
 for s in w.snakes().filter(|s|s.alive) {let raw=obs[s.id as usize];let d=if raw.generation==s.generation {raw} else {DesktopObservation::default()};write!(out,"{comma}{{\"id\":{},\"mode\":{},\"target\":{},\"prey\":{},\"prey_generation\":{},\"cutoff\":{},\"boosted\":{},\"power\":{},\"venom\":{},\"goal\":[{:.2},{:.2}],\"radius\":{:.2},\"body\":[",s.id,d.mode,d.target,d.prey,d.prey_generation,d.cutoff,d.boosted,d.power_up,d.venom_target,d.goal.x,d.goal.y,s.radius).unwrap();
 let mut sep="";for seg in s.segments.iter().step_by(2) {write!(out,"{sep}[{:.2},{:.2}]",seg.current.x,seg.current.y).unwrap();sep=",";}write!(out,"]}}").unwrap();comma=",";}
 write!(out,"],\"food\":[").unwrap();comma="";for f in w.foods() {write!(out,"{comma}[{:.2},{:.2},{}]",f.position.x,f.position.y,f.kind as u8).unwrap();comma=",";}
 write!(out,"],\"items\":[").unwrap();comma="";for item in w.items() {write!(out,"{comma}[{:.2},{:.2},{}]",item.position.x,item.position.y,item.kind as u8).unwrap();comma=",";}writeln!(out,"]}}").unwrap();
}
pub fn run() {
 let args:Vec<String>=std::env::args().collect();let case=&args[1];let seed:i32=args[2].parse().unwrap();let minutes:usize=args[3].parse().unwrap();let experiment:u8=args[4].parse().unwrap();let prefix=&args[5];
 let mut cfg=Config {width:7920.0,height:1440.0,density:80.0,trails:100.0,scale:200.0,speed:300.0,intelligence:100.0,self_collisions:true,deadly_walls:true,palette_size:6,rules:RuleSet::V2,seed,..Config::default()};
 match case.as_str() {"standard"=>{cfg.width=3440.0;cfg.density=30.0;cfg.scale=185.0;cfg.speed=230.0;},"width"=>cfg.width=3440.0,"density"=>cfg.density=30.0,"speed"=>cfg.speed=230.0,"scale"=>cfg.scale=100.0,"food"=>cfg.trails=1000.0,"wrap"=>cfg.deadly_walls=false,"noevents"=>cfg.world_events=false,"real"|"startup-hdmi"|"startup-dp5"|"startup-dp4"|"startup-priority"=>{},_=>panic!("unknown case")}
 cfg.snake_length_limit=args.iter().any(|s|s=="--limit");
 let giant=args.iter().position(|s|s=="--giant").map(|i|args[i+1].parse::<usize>().unwrap());
 if let Some(i)=args.iter().position(|a|a=="--aggression") {cfg.aggression=args[i+1].parse().unwrap();}
 let mut initial=cfg;
 match case.as_str() {"startup-hdmi"=>initial.width=2560.0,"startup-dp5"|"startup-priority"=>initial.width=3440.0,"startup-dp4"=>{initial.width=1920.0;initial.height=1080.0;},_=>{}}
 let mut w=World::new(initial).unwrap();
 match case.as_str() {"startup-hdmi"|"startup-dp5"=>{w.resize(6000.0,1440.0).unwrap();w.resize(7920.0,1440.0).unwrap();},"startup-priority"=>{w.resize(5360.0,1440.0).unwrap();w.resize(7920.0,1440.0).unwrap();},"startup-dp4"=>{w.resize(5360.0,1440.0).unwrap();w.resize(7920.0,1440.0).unwrap();},_=>{}}
let inner=AiController::new();assert_eq!(experiment,0,"policy experiments are not shipped");let mut ai=Observer {inner,obs:[DesktopObservation::default();MAX_SNAKES],called:[false;MAX_SNAKES]};
 if let Some(n)=giant {let (points,angle)=super::long_fixtures::spiral(&w,0,n);w.diagnostic_body(0,&points,angle).unwrap();}
 if giant.is_none() {for _ in 0..9000 {w.step(&mut ai);}}
 let initial_stats=w.stats();let mut metrics=Metrics::default();let mut leadups=Leadups::new(prefix);let mut lengths=super::length_metrics::LengthMetrics::new(&w,giant.map(|_|0));
 let mut counts:Vec<Counts>=(0..w.snake_count()).map(|_|Counts::default()).collect();let mut foods:HashMap<u64,FoodRecord>=HashMap::new();let mut caps:HashMap<u64,u64>=HashMap::new();let mut cap_times=Vec::new();let mut prism_times=Vec::new();let mut prism_spawn:HashMap<u64,u64>=HashMap::new();
 let mut histories:Vec<VecDeque<Sample>>=(0..w.snake_count()).map(|_|VecDeque::with_capacity(301)).collect();let mut last_angle=[0.0;MAX_SNAKES];let mut last_circle=[0u64;MAX_SNAKES];let mut ate=[0u64;MAX_SNAKES];
 let mut times=Vec::with_capacity(minutes*1800);let mut recent:HashMap<(u64,u32,u32),u64>=HashMap::new();let mut events=BufWriter::new(File::create(format!("{prefix}.events.csv")).unwrap());writeln!(events,"tick,event,snake,item,kind,immediate,recent,last_target_age").unwrap();let mut spawn_count=0;let mut expiry=0;let mut sever=0;let mut cap_kind=[0u64;8];let mut venom_episode=std::array::from_fn::<_,MAX_SNAKES,_>(|id|w.snake(id).filter(|s|s.alive && s.effect_ticks>0)
  .map_or(EffectEpisode::default(),|s|EffectEpisode {generation:s.generation,kind:s.effect_kind,used:false}));
 let mut replay_out=if args.iter().any(|a|a=="--replay") {Some(BufWriter::new(File::create(format!("{prefix}.replay.jsonl")).unwrap()))} else {None};
 let mut trace=if replay_out.is_some() {let mut f=BufWriter::new(File::create(format!("{prefix}.ticks.csv")).unwrap());writeln!(f,"tick,snake,generation,mode,target,distance,selected,direct_safe,direct_viable,tracking,turn_accum,recovery,stall,prey,prey_generation,venom_target").unwrap();Some(f)} else {None};
 let mut inventory=super::inventory_metrics::InventoryMetrics::default();
 let (mut frost_spawns,mut frost_picks,mut frost_activations,mut frost_freezes,mut frost_kills,mut frost_bites)=(0u64,0u64,0u64,0u64,0u64,0u64);
 for tick in 0..minutes*1800 {
  let frozen_before:[(u32,u16);MAX_SNAKES]=std::array::from_fn(|id|w.snake(id).map_or((0,0),|s|(s.generation,s.face.frozen_ticks)));
  let pre_lengths=std::array::from_fn(|id|w.snake(id).map_or(0,|s|s.segments.len()));
  let pre_items:Vec<Item>=w.items().copied().collect();let food_before:Vec<FoodView>=w.foods().collect();let pre_targets=ai.obs.map(|d|(d.generation,d.target));
  for item in &pre_items {caps.entry(item.id).or_insert(w.tick().saturating_sub(item.age_ticks as u64));}
  for f in w.foods() {foods.entry(f.id).or_insert(FoodRecord {claim:None,kind:f.kind});if matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed) {prism_spawn.entry(f.id).or_insert(w.tick());}}
  inventory.before(&w);leadups.before(&w);lengths.before(&w);ai.called.fill(false);let start=Instant::now();w.step(&mut ai);times.push(start.elapsed().as_secs_f64()*1000.0);
  for (id,d) in ai.obs.iter().enumerate().filter(|(id,_)|ai.called[*id]) {if d.target!=0 {recent.insert((d.target,id as u32,d.generation),tick as u64);}}
  let intended=|id:usize,generation:u32,food_id:u64| (ai.called[id] && ai.obs[id].generation==generation && ai.obs[id].target==food_id) || pre_targets[id]==(generation,food_id);
  let patch=|id:usize,generation:u32,f:FoodView| {let d=ai.obs[id];ai.called[id] && d.generation==generation && matches!(d.mode,11|10) && w.distance_squared(d.goal,f.position)<(w.snake(id).unwrap().radius*6.0).powi(2)};
  
  for f in w.foods().filter(|f|f.vacuum_owner>=0) {if let Some(rec)=foods.get_mut(&f.id) {if rec.claim.is_none() {let id=f.vacuum_owner as usize;let generation=w.snake(id).unwrap().generation;rec.claim=Some((id as u32,generation,intended(id,generation,f.id),patch(id,generation,f),metrics.deliberate(id,generation,f.id)));}}}
  let mut captured=Vec::new();
  for (food_id,id,generation,_,value) in w.consumption_events() {let id=id as usize;captured.push((food_id,id));let rec=foods.remove(&food_id);let deliberate=rec.as_ref().and_then(|r|r.claim).filter(|r|r.0==id as u32 && r.1==generation).map_or_else(||metrics.deliberate(id,generation,food_id),|r|r.4);metrics.deliberate+=u64::from(deliberate);let patch_targeted=rec.as_ref().and_then(|r|r.claim).filter(|r|r.0==id as u32 && r.1==generation).map_or_else(||food_before.iter().find(|f|f.id==food_id).is_some_and(|f|patch(id,generation,*f)),|r|r.3);let targeted=rec.as_ref().and_then(|r|r.claim).filter(|r|r.0==id as u32 && r.1==generation).map_or_else(||intended(id,generation,food_id),|r|r.2);let c=&mut counts[id];c.food+=1;c.intended+=u64::from(targeted);c.patch_intended+=u64::from(targeted || patch_targeted);c.food_value+=value;c.intended_value+=if targeted {value} else {0.0};ate[id]+=1;
   if rec.is_some_and(|r|matches!(r.kind,FoodKind::Prism|FoodKind::PrismSeed)) {c.prisms+=1;c.prisms_intended+=u64::from(targeted);if let Some(t)=prism_spawn.remove(&food_id) {prism_times.push((w.tick()-t) as f64/30.0);}}
  }
  for e in w.frame_events().filter(|e|e.tick==w.tick()) {if (e.snake_id as usize)<MAX_SNAKES {venom_episode[e.snake_id as usize].observe(e);}match e.kind {EventKind::Pickup|EventKind::Stash=>{let id=e.snake_id as usize;
    // Pickup also marks inventory activation; only a unique field acquisition
    // belongs in targeting and capsule totals.
    if let Some(item)=super::inventory_metrics::field_acquisition(&w,e,&pre_items,&captured) {counts[id].capsules+=1;ate[id]+=1;if e.other_snake_id==4 {counts[id].venom_picks+=1;}cap_kind[e.other_snake_id as usize]+=1;captured.push((item.id|ITEM_BIT,id));let generation=e.generation;let now=intended(id,generation,item.id|ITEM_BIT);counts[id].caps_intended+=u64::from(now);let age=recent.get(&(item.id|ITEM_BIT,id as u32,generation)).map_or(u64::MAX,|t|tick as u64-*t);counts[id].cap_recent+=u64::from(now || age<=90);writeln!(events,"{},pickup,{},{},{},{},{},{}",w.tick(),id,item.id,item.kind as u8,now,now || age<=90,age).unwrap();if let Some(t)=caps.remove(&item.id) {cap_times.push((w.tick()-t) as f64/30.0);}}
   },EventKind::ItemSpawn=>{spawn_count+=1;},EventKind::ItemExpiry=>{expiry+=1;},EventKind::Sever=>{sever+=1;let id=e.other_snake_id as usize;if venom_episode[id].use_once(4,e.other_generation) {counts[id].venom_used+=1;}},_=>{}}}
  for e in w.frame_events().filter(|e|e.tick==w.tick()) {
   if e.kind==EventKind::ItemSpawn && e.other_snake_id==5 {frost_spawns+=1;}
   if e.other_snake_id==5 {frost_picks+=u64::from(lifecycle(e).acquisition());frost_activations+=u64::from(lifecycle(e).activation());}
   if e.kind==EventKind::Sever && w.snake(e.snake_id as usize).is_some_and(|s|s.generation==e.generation && s.face.frozen_ticks>0) {frost_bites+=1;}
  }
  for s in w.snakes() {let (generation,ticks)=frozen_before[s.id as usize];if s.generation==generation && s.face.frozen_ticks>ticks {frost_freezes+=1;}}
  for e in w.collision_events() {
   if e.owner_mask & !(1<<e.victim)!=0 && w.snake(e.victim as usize).is_some_and(|s|s.generation==e.generation && s.face.frozen_ticks>0) {frost_kills+=1;}
  }
  inventory.after(&w,&ai.obs);
  metrics.observe(&w,&ai.obs,&ai.called,&captured,&pre_lengths);
  leadups.after(&w,&ai.obs,&ai.called);lengths.after(&w);
  for s in w.snakes().filter(|s|s.alive && ai.called[s.id as usize] && ai.obs[s.id as usize].generation==s.generation) {let id=s.id as usize;let d=ai.obs[id];let c=&mut counts[id];let p=s.segments[0].current;
   c.ticks+=1;c.modes[d.mode as usize]+=1;c.recoveries+=u64::from(d.recovery_started);c.recover_caps+=u64::from(d.recovery_started && d.recovery_target&ITEM_BIT!=0);c.recover_targets+=u64::from(d.recovery_started && d.recovery_target!=0);c.recover_progress+=u64::from(d.recovery_started && d.recovery_target!=0 && d.recovery_progress_age<15);c.progress_rejections+=u64::from(d.progress_rejected);c.length+=s.segments.len() as u64;
   c.near_wall+=u64::from(p.x.min(cfg.width-p.x).min(p.y).min(cfg.height-p.y)<s.radius*5.0);if cfg.width==7920.0 {c.invisible+=u64::from(!visible(p));c.near_gap+=u64::from(near_gap(p));}
   c.selections[d.selected as usize]+=1;c.strategy+=u64::from(d.strategic);if d.strategic {c.direct_safe+=u64::from(d.direct_safe);c.direct_viable+=u64::from(d.direct_viable);c.viable_ignored+=u64::from(d.direct_viable && d.selected!=0);c.safety_veto+=u64::from(!d.direct_viable && d.target!=0);}
   c.tracked+=u64::from(d.tracking);if d.target!=0 {c.targets+=1;c.target_dist+=d.distance;c.behind+=u64::from(d.target_bearing.abs()>std::f64::consts::FRAC_PI_2);}
   let h=&mut histories[id];if h.back().is_some_and(|last|last.generation!=s.generation) {h.clear();}
   let turn=if let Some(last)=h.back() {last.turn+normalize_angle(s.angle-last_angle[id])} else {0.0};last_angle[id]=s.angle;
   h.push_back(Sample {generation:s.generation,turn,position:p,distance:d.distance,target:d.target,mode:d.mode,ate:ate[id]});if h.len()>301 {h.pop_front();}
   if h.len()==301 && tick as u64>=last_circle[id]+300 {let first=h.front().unwrap();let last=h.back().unwrap();let turn=(last.turn-first.turn).abs();let turning_radius=w.motion_limits(id,0.0).map_or(100.0,|(v,r)|v/r);let stationary=w.distance_squared(first.position,last.position)<(turning_radius*2.0).powi(2);let no_progress=first.target!=last.target || first.distance-last.distance<8.0;
    if turn>=std::f64::consts::TAU && stationary && no_progress && first.ate==last.ate {let mut modes=[0usize;12];for e in h.iter(){modes[e.mode as usize]+=1;}let mode=(0..12).max_by_key(|i|modes[*i]).unwrap();c.circles[mode]+=1;last_circle[id]=tick as u64;}
   }
   if (0..1800).contains(&tick) || (36000..37800).contains(&tick) {if let Some(f)=&mut trace {writeln!(f,"{},{},{},{},{},{:.2},{},{},{},{},{:.3},{},{},{},{},{}",w.tick(),id,s.generation,MODES[d.mode as usize],d.target,d.distance,d.selected,d.direct_safe,d.direct_viable,d.tracking,d.turn_accum,d.recovery_started,d.progress_rejected,d.prey,d.prey_generation,d.venom_target).unwrap();}}
  }
  // Drop expired/evicted food metadata to bound observer memory.
  if tick%300==0 {let ids:Vec<u64>=w.foods().map(|f|f.id).collect();foods.retain(|id,_|ids.contains(id));recent.retain(|_,t|tick as u64-*t<=300);}
  if ((0..1800).contains(&tick) || (36000..37800).contains(&tick)) && tick%3==0 {if let Some(f)=&mut replay_out {replay(&w,&ai.obs,f);}}
 }
 inventory.write(prefix);
 times.sort_unstable_by(f64::total_cmp);cap_times.sort_unstable_by(f64::total_cmp);prism_times.sort_unstable_by(f64::total_cmp);
 let median=|xs:&[f64]|if xs.is_empty(){0.0}else{xs[xs.len()/2]};let total_ticks:u64=counts.iter().map(|c|c.ticks).sum();let food:u64=counts.iter().map(|c|c.food).sum();let intended:u64=counts.iter().map(|c|c.intended).sum();let pickups:u64=counts.iter().map(|c|c.capsules).sum();let caps_intended:u64=counts.iter().map(|c|c.caps_intended).sum();let circles:u64=counts.iter().map(|c|c.circles.iter().sum::<u64>()).sum();let strategy:u64=counts.iter().map(|c|c.strategy).sum();
 let pct=|a:u64,b:u64|a as f64*100.0/b.max(1) as f64;let sum=|f:fn(&Counts)->u64|counts.iter().map(f).sum::<u64>();
 println!("case={case} seed={seed} minutes={minutes} experiment={experiment} snakes={} food={} radius={} world_segments={} ticks={total_ticks} food_eaten={food} food_intended={intended} food_intended_pct={:.2} capsules={pickups} caps_intended={caps_intended} cap_intended_pct={:.2} cap_spawns={spawn_count} cap_expiry={expiry} cap_median_s={:.2} prisms={} prism_intended={} prism_median_s={:.2} circles={circles} circles_snake_min={:.4} recoveries={} recovery_snake_min={:.3} stalls={} wall_pct={:.2} invisible_pct={:.2} gap_pct={:.2} direct_safe_pct={:.2} direct_viable_pct={:.2} direct_viable_ignored_pct={:.2} safety_veto_pct={:.2} tracked_pct={:.2} target_mean_distance={:.2} target_behind_pct={:.2} mean_length={:.2} ms_mean={:.5} ms_p99={:.5} deaths={} wall_deaths={} self_deaths={} head_deaths={} body_deaths={} sever={sever} cap_kinds={cap_kind:?}",w.snake_count(),cfg.food_count(),cfg.base_radius(),cfg.maximum_world_segments(),pct(intended,food),pct(caps_intended,pickups),median(&cap_times),sum(|c|c.prisms),sum(|c|c.prisms_intended),median(&prism_times),circles as f64/(total_ticks as f64/1800.0),sum(|c|c.recoveries),sum(|c|c.recoveries) as f64/(total_ticks as f64/1800.0),sum(|c|c.progress_rejections),pct(sum(|c|c.near_wall),total_ticks),pct(sum(|c|c.invisible),total_ticks),pct(sum(|c|c.near_gap),total_ticks),pct(sum(|c|c.direct_safe),strategy),pct(sum(|c|c.direct_viable),strategy),pct(sum(|c|c.viable_ignored),sum(|c|c.direct_viable)),pct(sum(|c|c.safety_veto),strategy),pct(sum(|c|c.tracked),total_ticks),sum(|c|c.target_dist as u64) as f64/sum(|c|c.targets).max(1) as f64,pct(sum(|c|c.behind),sum(|c|c.targets)),sum(|c|c.length) as f64/total_ticks as f64,times.iter().sum::<f64>()/times.len() as f64,times[times.len()*99/100],w.stats().deaths-initial_stats.deaths,w.stats().wall_deaths-initial_stats.wall_deaths,w.stats().self_deaths-initial_stats.self_deaths,w.stats().head_deaths-initial_stats.head_deaths,w.stats().body_deaths-initial_stats.body_deaths);
 println!("prism_spawned={} prism_pickup_pct={:.3}",prism_times.len()+prism_spawn.len(),100.0*prism_times.len() as f64/(prism_times.len()+prism_spawn.len()).max(1) as f64);
 println!("extra patch_intended={} patch_intended_pct={:.3} cap_recent={} cap_recent_pct={:.3} recovery_targets={} recovery_caps={} recovery_with_progress={} venom_picks={} venom_used={}",sum(|c|c.patch_intended),pct(sum(|c|c.patch_intended),food),sum(|c|c.cap_recent),pct(sum(|c|c.cap_recent),pickups),sum(|c|c.recover_targets),sum(|c|c.recover_caps),sum(|c|c.recover_progress),sum(|c|c.venom_picks),sum(|c|c.venom_used));
 println!("frost spawns={frost_spawns} pickups={frost_picks} pickup_pct={:.3} freezes={frost_freezes} activations={frost_activations} freezes_per_activation={:.3} frozen_prey_kills={frost_kills} frozen_prey_bites={frost_bites}",pct(frost_picks,frost_spawns),frost_freezes as f64/frost_activations.max(1) as f64);
 metrics.print(total_ticks);metrics.save(prefix);leadups.print();lengths.print();
 let mut csv=BufWriter::new(File::create(format!("{prefix}.summary.csv")).unwrap());writeln!(csv,"snake,mode,ticks,percent,circles,food,intended,capsules,caps_intended,recoveries,stalls").unwrap();
 for (id,c) in counts.iter().enumerate() {for (m,name) in MODES.iter().enumerate() {writeln!(csv,"{id},{name},{},{:.3},{},{},{},{},{},{},{}",c.modes[m],pct(c.modes[m],c.ticks),c.circles[m],c.food,c.intended,c.capsules,c.caps_intended,c.recoveries,c.progress_rejections).unwrap();}}
 for (m,name) in MODES.iter().enumerate(){println!("mode={name} percent={:.3} circles={}",pct(counts.iter().map(|c|c.modes[m]).sum(),total_ticks),counts.iter().map(|c|c.circles[m]).sum::<u64>());}
 println!("selections={:?}",(0..13).map(|k|counts.iter().map(|c|c.selections[k]).sum::<u64>()).collect::<Vec<_>>());
}

}
#[cfg(feature="desktop-diag")]
fn main(){diag::run();}
