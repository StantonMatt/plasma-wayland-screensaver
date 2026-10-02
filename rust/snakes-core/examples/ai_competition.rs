// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact first-generation duels. Aggression crosses identical jitters, mirrors,
//! and role swaps. Limited prey keeps uniform wall/self safety but does not
//! anticipate an opponent's next maneuver. --trace prints paths around kills.
use snakes_core::{ai::AiController, controller::{Controller,Steering}, Config, Point, SnakeView, World, STEP_SECONDS, normalize_angle};
use std::f64::consts::{PI,FRAC_PI_2};
#[allow(dead_code)]
#[path="support/diagnostics.rs"] mod diagnostics;
#[allow(dead_code)]
#[path="support/accounting.rs"] mod accounting;
use accounting::{DuelOutcome,Observed};
fn p(x:f64,y:f64)->Point {Point{x,y}}
struct Competition {ai:AiController,limited:bool,attacker:usize,goal:Point,held:[f64;3]}
impl diagnostics::ScoreController for Competition {
    fn ai(&self)->Option<&AiController> {Some(&self.ai)}
}
impl Controller for Competition {
    fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
        if !self.limited || s.id as usize==self.attacker {return self.ai.steer(w,s);}
        let id=s.id as usize;
        let head=s.segments[0].current;
        let d=w.displacement(head,self.goal);
        let bearing=d.y.atan2(d.x);
        // A passed food patch is not a command to reverse into the neck.
        let goal=if normalize_angle(bearing-s.angle).abs()<1.0 {bearing} else {s.angle};
        let (speed,turn)=w.motion_limits(id,0.0).unwrap();
        let mut best=(0, f64::NEG_INFINITY, s.angle);
        for desired in [self.held[id],goal,s.angle,s.angle+0.7,s.angle-0.7,s.angle+1.6,s.angle-1.6] {
            let mut path=[head;73];let mut angle=s.angle;let mut safe=0;
            for j in 1..=72 {
                angle=normalize_angle(angle+normalize_angle(desired-angle).clamp(-turn*STEP_SECONDS,turn*STEP_SECONDS));
                path[j]=w.canonical_point(p(path[j-1].x+angle.cos()*speed*STEP_SECONDS,path[j-1].y+angle.sin()*speed*STEP_SECONDS));
                let q=path[j];let cfg=w.config();let radius=speed/turn*1.1;
                if cfg.deadly_walls && (q.x<radius*(1.0-angle.sin().abs())+5.0 || cfg.width-q.x<radius*(1.0-angle.sin().abs())+5.0
                    || q.y<radius*(1.0-angle.cos().abs())+5.0 || cfg.height-q.y<radius*(1.0-angle.cos().abs())+5.0) {break;}
                let neck=(10.0-j as f64*speed*STEP_SECONDS/(s.radius*1.18)).max(1.0) as usize;
                let blocked=w.snakes().filter(|o|o.alive).any(|o| {
                    let same=o.id==s.id;
                    // Own-body safety never waits for tactical reaction. Rival
                    // bodies get an immediate check, without future head plans.
                    if (same && !cfg.self_collisions) || (!same && j>8) {return false;}
                    o.segments.iter().skip(if same {neck} else {1}).any(|b| {
                        let reach=if same {s.radius*1.48+2.0} else {(s.radius+o.radius)*0.78+3.0};
                        let d=w.displacement(q,b.current);let broad=reach+speed*STEP_SECONDS;
                        d.x.abs()<broad && d.y.abs()<broad && w.segment_distance_squared(b.current,path[j-1],q)<reach*reach
                    })
                });
                let age=(10.0*s.radius*1.18/(speed*STEP_SECONDS)).ceil() as usize;
                if blocked || (cfg.self_collisions && j>age && (0..j-age).any(|k|w.segment_distance_squared(path[k],path[j-1],q)<(s.radius*1.48+2.0).powi(2))) {break;}
                safe=j;
            }
            let score=-normalize_angle(desired-goal).abs()*25.0-normalize_angle(desired-self.held[id]).abs()*12.0;
            if safe>best.0 || (safe==best.0 && score>best.1) {best=(safe,score,desired);}
        }
        self.held[id]=best.2;
        Steering {desired_angle:best.2,rush:0.0}
    }
}
fn main() {
    let args:Vec<_>=std::env::args().collect();
    if args.iter().any(|a|a=="--help") {println!("ai_competition: 8 scenarios x IQ 100/50 x 80 crossed fixtures; 20s each, first generation only; --scripted-prey uses competent limited prey; --trace logs kill paths; --scenario NAME selects a fixture");return;}
    let scenario=args.windows(2).find(|a|a[0]=="--scenario").map(|a|a[1].as_str());
    let limited=args.iter().any(|a|a=="--scripted-prey");let trace=args.iter().any(|a|a=="--trace");
    for iq in [100.0,50.0] {for case in 0..8 {for aggressive in [true,false] {
        let name=["open_crossing","wall_escape","wrap_escape","equal_cluster","food_cutoff","long_encircle","feasible_crossing","infeasible_chase"][case];
        if scenario.is_some_and(|s|s!=name) {continue;}
        let mut kills=0;let mut other_kills=0;let mut attacker_dead=0;let mut escaped=0;let mut time_sum=0.0;
        let mut share=[0.0;3];let mut contested=0.0;let mut tactical_ticks=0;let mut coil_ticks=0;let mut rushing_ticks=0;let mut victim_self=0;let mut victim_wall=0;
        for jitter_id in 0..10 {for mirror in [false,true] {for swapped in [false,true] {
            let jitter=(jitter_id as f64-4.5)*3.0;
            let mut snakes=match case {
                0=>vec![(p(800.0,420.0+jitter),FRAC_PI_2,72,1.0),(p(925.0,520.0),PI,24,0.6)],
                1=>vec![(p(970.0,780.0+jitter),0.0,72,1.0),(p(1125.0,890.0),0.0,24,0.6)],
                2=>vec![(p(1420.0,420.0+jitter),0.0,72,1.0),(p(1560.0,520.0),0.0,24,0.6)],
                3=>vec![(p(670.0,470.0+jitter),0.0,40,0.95),(p(930.0,530.0-jitter),PI,40,0.15),(p(800.0,690.0),-FRAC_PI_2,40,0.65)],
                4=>vec![(p(760.0,470.0+jitter),0.0,72,1.0),(p(910.0,630.0),-FRAC_PI_2,24,0.6),(p(1100.0,500.0),PI,32,0.2)],
                5=>vec![(p(820.0,100.0+jitter),0.0,180,1.0),(p(875.0,195.0),-FRAC_PI_2,24,0.6)],
                // A short, reachable crossing with room for both exits.
                6=>vec![(p(800.0,450.0+jitter),FRAC_PI_2,72,1.0),(p(870.0,525.0),PI,24,0.6)],
                _=>vec![(p(800.0,350.0+jitter),0.0,400,1.0),(p(1250.0,650.0),0.0,24,0.6)],
            };
            snakes[0].3=if aggressive {1.0} else {0.15};
            let mut cluster=if case==1 {p(1350.0,890.0)} else if case==2 {p(180.0,520.0)} else {p(850.0,510.0)};
            let transform=|q:Point|if mirror {p(1600.0-q.x,q.y)} else {q};
            for s in &mut snakes {s.0=transform(s.0);if mirror {s.1=normalize_angle(PI-s.1);}}
            cluster=transform(cluster);
            if swapped {snakes.swap(0,1);}
            let attacker=usize::from(swapped);let victim=1-attacker;
            let mut food=Vec::new();
            for i in 0..28 {food.push(p(cluster.x+(i%7) as f64*8.0-24.0,cluster.y+(i/7) as f64*8.0-12.0));}
            let cfg=Config {width:1600.0,height:1000.0,density:0.0,trails:0.0,scale:70.0,seed:73+jitter_id,
                intelligence:iq,self_collisions:true,deadly_walls:case!=2,..Config::default()};
            let mut w=World::diagnostic_arena(cfg,&snakes,&food).unwrap();
            if case==5 {
                wall_u(&mut w,attacker,victim,jitter,mirror);
            }
            if case==7 {
                // Put the giant's long body outside the prey lane and inside
                // the arena; an invalid initial wall-crossing body is no test.
                let body:Vec<_>=(0..400).map(|j|transform(p(800.0-(j as f64*7.08/500.0).sin()*500.0,550.0+(j as f64*7.08/500.0).cos()*180.0-180.0))).collect();
                w.diagnostic_body(attacker,&body,if mirror {PI} else {0.0}).unwrap();
            }
            let mut c=Observed::new(Competition {ai:AiController::new(),limited,attacker,goal:cluster,held:[snakes[0].1,snakes[1].1,if snakes.len()>2 {snakes[2].1} else {0.0}]});
            let mut outcome=DuelOutcome::default();let mut original_eaten=[0.0;3];let mut history=Vec::new();
            for tick in 0..if case==7 {90} else {600} {
                if trace && tick%6==0 {history.push(format!("t={:.2} A={:?}/{:.2} B={:?}/{:.2} tactic={:?}",tick as f64*STEP_SECONDS,w.snake(attacker).unwrap().segments[0].current,w.snake(attacker).unwrap().angle,w.snake(victim).unwrap().segments[0].current,w.snake(victim).unwrap().angle,c.inner.ai.competition_debug(attacker).unwrap()));}
                c.begin();w.step(&mut c);
                let tactic=c.tactics[attacker];
                tactical_ticks+=usize::from(tactic.hunting);coil_ticks+=usize::from(tactic.coil);
                if w.observed_rush(attacker).unwrap()>0.0 {rushing_ticks+=1;}
                for (id,eater,generation,_,value) in w.consumption_events() {
                    if id<=28 && generation==1 {original_eaten[eater as usize]+=value;}
                }
                for e in w.collision_events() {
                    if outcome.record(e,attacker,victim) {
                        time_sum+=(tick+1) as f64*STEP_SECONDS;
                        if trace {println!("kill scenario={name} iq={iq} aggression={aggressive} jitter={jitter_id} mirror={mirror} swap={swapped} event={e:?}");for h in history.iter().rev().take(12).rev() {println!("  {h}");}}
                    }
                }
                if outcome.finished(attacker,victim) {break;}
            }
            if !outcome.dead[victim] {escaped+=1;}
            kills+=outcome.kills;other_kills+=outcome.other_kills;attacker_dead+=outcome.attacker_deaths;
            victim_self+=outcome.victim_self;victim_wall+=outcome.victim_wall;
            share[0]+=original_eaten[attacker];share[1]+=original_eaten[victim];
            if snakes.len()>2 {share[2]+=original_eaten[2];}
            contested+=original_eaten.iter().sum::<f64>();
        }}}
        println!("competition mode={} scenario={name} iq={iq} aggression={} n=40 kills={kills} other_kills={other_kills} kill_pct={:.1} kill_seconds={:.3} smaller_survival_pct={:.1} attacker_deaths={attacker_dead} food_share={:.3}/{:.3}/{:.3} cluster_eaten={contested:.0} tactical_seconds={:.2} coil_seconds={:.2} rush_seconds={:.2} victim_self={victim_self} victim_wall={victim_wall}",if limited {"limited"} else {"responsive"},if aggressive {1.0} else {0.15},kills as f64*2.5,if kills==0 {0.0} else {time_sum/kills as f64},escaped as f64*2.5,share[0]/contested.max(1.0),share[1]/contested.max(1.0),share[2]/contested.max(1.0),tactical_ticks as f64*STEP_SECONDS,coil_ticks as f64*STEP_SECONDS,rushing_ticks as f64*STEP_SECONDS);
    }}}
}

fn wall_u(w:&mut World,attacker:usize,victim:usize,jitter:f64,mirror:bool) {
    let transform=|q:Point|if mirror {p(1600.0-q.x,q.y)} else {q};
    let center=p(875.0,195.0);let radius=180.0;let theta=-0.4+jitter/200.0;
    // Existing U plus a straight tail reserve: closure must finish before the
    // entry arm releases. The nearby top wall supplies the missing boundary.
    let body:Vec<_>=(0usize..180).map(|j| {
        let phase=theta-j.min(129) as f64*7.08/radius;
        let tail=j.saturating_sub(129) as f64*7.08;
        transform(p(center.x+radius*phase.cos()+tail*(phase-FRAC_PI_2).cos(),
            center.y+radius*phase.sin()+tail*(phase-FRAC_PI_2).sin()))
    }).collect();
    w.diagnostic_body(attacker,&body,if mirror {PI-theta-FRAC_PI_2} else {theta+FRAC_PI_2}).unwrap();
    let small:Vec<_>=(0..24).map(|j|transform(p(center.x,center.y+j as f64*7.08))).collect();
    w.diagnostic_body(victim,&small,-FRAC_PI_2).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn food_cutoff_continues_after_the_third_snakes_first_death() {
        let cfg=Config {width:1600.0,height:1000.0,density:0.0,trails:0.0,scale:70.0,seed:73,
            intelligence:100.0,self_collisions:true,deadly_walls:true,..Default::default()};
        let snakes=[(p(760.0,456.5),0.0,72,1.0),(p(910.0,630.0),-FRAC_PI_2,24,0.6),(p(1100.0,500.0),PI,32,0.2)];
        let food:Vec<_>=(0..28).map(|i|p(850.0+(i%7) as f64*8.0-24.0,510.0+(i/7) as f64*8.0-12.0)).collect();
        let mut w=World::diagnostic_arena(cfg,&snakes,&food).unwrap();
        let mut c=Competition {ai:AiController::new(),limited:true,attacker:0,goal:p(850.0,510.0),held:[0.0,-FRAC_PI_2,PI]};
        let mut outcome=DuelOutcome::default();let mut third_death=None;let mut ticks=0;
        for tick in 0..600 {
            w.step(&mut c);ticks=tick+1;
            for e in w.collision_events() {
                outcome.record(e,0,1);
                if e.victim==2 && e.generation==1 {
                    third_death=Some(ticks);
                    assert_eq!(outcome.kills,0);
                    assert!(!outcome.finished(0,1));
                }
            }
            if outcome.finished(0,1) {break;}
        }
        assert_eq!(third_death,Some(38));assert!(ticks>38);
        assert!(outcome.finished(0,1) || ticks==600);
        assert!(outcome.kills<=1);assert_eq!(outcome.other_kills,1);
    }
    #[test]
    fn wall_encirclement_benchmark_enters_pocket() {
        for iq in [100.0] {
            let mut entered=false;
            for mirror in [false,true] {for swapped in [false,true] {
                let attacker=usize::from(swapped);let victim=1-attacker;
                let cfg=Config {width:1600.0,height:1000.0,density:0.0,scale:70.0,intelligence:iq,
                    self_collisions:true,deadly_walls:true,seed:73,..Config::default()};
                let mut snakes=vec![(p(820.0,100.0),0.0,180,1.0),(p(875.0,195.0),-FRAC_PI_2,24,0.6)];
                if swapped {snakes.swap(0,1);}
                let mut w=World::diagnostic_arena(cfg,&snakes,&[]).unwrap();
                wall_u(&mut w,attacker,victim,0.0,mirror);
                let mut c=Competition {ai:AiController::new(),limited:true,attacker,goal:p(850.0,510.0),held:[0.0;3]};
                for _ in 0..120 {
                    w.step(&mut c);
                    if c.ai.competition_debug(attacker).unwrap().coil_radius>0.0 {entered=true;break;}
                    if !w.snake(attacker).unwrap().alive || !w.snake(victim).unwrap().alive {break;}
                }
            }}
            assert!(entered,"aggressive IQ {iq} benchmark must actually enter a pocket");
        }
    }
}
