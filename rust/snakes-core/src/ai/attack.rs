// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded cutoff library, sharing the ordinary safety rollout and slots.
use super::*;
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Attack {
    pub(super) valid: bool,
    pub(super) side: i8,
    pub(super) prey: usize,
    pub(super) prey_generation: u32,
    pub(super) start: u64,
    pub(super) turn_at: u64,
    pub(super) end: u64,
    pub(super) approach: f64,
    pub(super) crossing: f64,
    pub(super) burst: f64,
    pub(super) crossing_rush: f64,
    pub(super) point: Point,
    pub(super) prey_heading: f64,
    pub(super) error: f64,
    pub(super) limits: Option<[f64;6]>,
    pub(super) free_boost: bool,
    pub(super) effect_kind: u8,
    pub(super) effect_ticks: u16,
}
impl Attack {
    pub(super) fn control(self,tick:u64)->(f64,f64) {
        if tick<self.turn_at {(self.approach,self.burst)} else {(self.crossing,self.crossing_rush)}
    }
}
impl AiController {
    pub(super) fn attack_limits(w:&World,s:SnakeView<'_>,a:Attack)->[f64;6] {
        let (fast,fast_turn)=w.motion_limits(s.id as usize,a.burst).unwrap();
        let (slow,slow_turn)=w.motion_limits(s.id as usize,a.crossing_rush).unwrap();
        [fast,fast_turn,slow,slow_turn,s.radius,s.segments.len() as f64]
    }
    /// Arc to the approach bearing, straight burst, then a slower crossing arc
    /// and straight exit. Generate six paths, refine the approach bearing using
    /// the exact discrete integration, and retain only two geometric finalists.
    pub(super) fn cutoffs(&self,w:&World,s:SnakeView<'_>,state:State)->[Attack;2] {
        let mut best=[Attack::default();2];
        if state.prey==0 || state.coil_radius>0.0 || s.traits.aggression<if w.config().rules==crate::RuleSet::V2 {0.2} else {0.32} {return best;}
        let prey=self.opportunity_rival(state.prey-1);
        let v2=w.config().rules==crate::RuleSet::V2;
        if v2 && (!w.boost_ready(s.id as usize) || s.segments.len()<16) {return best;}
        let burst=if v2 {0.6} else {s.traits.aggression};
        let motion=self.boosted_motion[s.id as usize];
        let crossing_rush=Self::boost_request(w,s,0.15);
        let (fast,fast_turn)=w.motion_limits(s.id as usize,burst).unwrap();
        let (slow,slow_turn)=w.motion_limits(s.id as usize,crossing_rush).unwrap();
        let head=s.segments[0].current;
        let mut rivals=[Rival::default();MAX_SNAKES];
        for direct in [false,true] {
        if direct && !v2 {continue;}
        // Paid bursts need short crossings. A free Surge can approach farther
        // through its boosted movement and remaining speed bonus, with the
        // same three-window budget and the ordinary exact safety rollout.
        for ticks in if v2 && !surge::active(w,s) {if aggression::bold(w)>0.0 {[12usize,24,36]} else {[12usize,18,24]}} else {[18usize,30,42]} {for side in [-1i8,1] {
            // Lay the barrier 0.2s before the prey's nominal arrival.
            let arrive=ticks.saturating_sub(6);let point=prey.path[ticks];
            let forward=w.displacement(prey.path[ticks-2],point);
            let theta=forward.y.atan2(forward.x);
            let crossing=normalize_angle(theta+side as f64*std::f64::consts::FRAC_PI_2);
            let width=(s.radius+prey.radius)*2.0+prey.speed*0.12;
            let near=if direct {point} else {w.canonical_point(Point{x:point.x-crossing.cos()*width,y:point.y-crossing.sin()*width})};
            let d=w.displacement(head,near);
            let mut bearing=d.y.atan2(d.x);
            if (d.x*d.x+d.y*d.y).sqrt()>fast*arrive as f64*STEP_SECONDS+width {continue;}
            let mut attack=Attack::default();
            for _ in 0..3 {
                let turn_ticks=(normalize_angle(crossing-bearing).abs()/(slow_turn*STEP_SECONDS)).ceil() as usize;
                let cross_ticks=(width/(slow*STEP_SECONDS)).ceil() as usize;
                let switch=if direct {arrive} else {arrive.saturating_sub(turn_ticks/2+cross_ticks).max(2)};
                attack=Attack {valid:true,side,prey:state.prey,prey_generation:state.prey_generation,start:w.tick(),turn_at:w.tick()+switch as u64,end:w.tick()+(ticks+18) as u64,
                    approach:bearing,crossing,burst,crossing_rush,point,prey_heading:theta,error:0.0,free_boost:v2 && w.boost_segment_cost(s.id as usize)==Some(0),effect_kind:s.effect_kind,effect_ticks:s.effect_ticks,limits:Some([fast,fast_turn,slow,slow_turn,s.radius,s.segments.len() as f64])};
                let mut pos=head;let mut angle=s.angle;
                let mut forecast=self.intent_forecast::<true>(w,s,state,arrive,1<<s.id);
                for rival in &mut rivals {rival.dynamic=false;}
                for j in 0..arrive {
                    let (desired,rush)=attack.control(w.tick()+j as u64);
                    let (speed,turn)=if v2 {forecast.motion(w,s.id as usize,burst,&motion,j+1)} else if rush==attack.burst {(fast,fast_turn)} else {(slow,slow_turn)};
                    angle=normalize_angle(angle+normalize_angle(desired-angle).clamp(-turn*STEP_SECONDS,turn*STEP_SECONDS));
                    pos=w.canonical_point(Point{x:pos.x+angle.cos()*speed*STEP_SECONDS,y:pos.y+angle.sin()*speed*STEP_SECONDS});
                    if forecast.movement_changed() & !(1<<s.id)!=0 {
                        Self::advance_rivals(w,&mut rivals,&self.motion,&forecast,j+1,1<<s.id,Some((&self.effects,&self.rivals)),None);
                    }
                    if forecast.has_items() {
                        let radius=forecast.radius(w,s.id as usize,burst,&motion,j+1);forecast.set_radius(s.id as usize,radius);
                        for (id,_) in rivals.iter().enumerate().filter(|(_,r)|r.dynamic) {
                            let radius=forecast.radius(w,id,0.0,&self.motion[id],j+1);forecast.set_radius(id,radius);
                        }
                        let changed=(1<<s.id) | rivals.iter().enumerate().fold(0,|mask,(id,r)|mask | if r.dynamic {1<<id} else {0});
                        forecast.advance_cached(w,j+1,|id|if id==s.id as usize {pos} else {Self::forecast_rival(&rivals,&self.rivals,id).path[j+1]},changed,&self.item_forecast,0.0);
                    }
                }
                let error=w.displacement(pos,point);
                attack.error=(error.x*error.x+error.y*error.y).sqrt();
                bearing=normalize_angle(bearing+(-bearing.sin()*error.x+bearing.cos()*error.y)/(fast*arrive as f64*STEP_SECONDS).max(20.0));
            }
            if attack.error>width*1.5 {continue;}
            let continuity=if state.attack.valid && state.attack.side==side {8.0} else {0.0};
            let rank=attack.error-continuity+ticks as f64*0.1;
            for k in 0..2 {
                let old=best[k];let oldrank=old.error-if state.attack.valid && old.side==state.attack.side {8.0} else {0.0}+(old.end-old.start).saturating_sub(18) as f64*0.1;
                if !old.valid || rank<oldrank {if k==0 {best[1]=best[0];}best[k]=attack;break;}
            }
        }}}
        best
    }
    /// Six physical responses. Count attacker contacts separately from wall or
    /// self failures; samples are attack utility, never a safety certificate.
    pub(super) fn replies_blocked(&self,w:&World,s:SnakeView<'_>,state:State,c:&Candidate)->usize {
        if !self.attack_usable(w,s,state,c.attack) {return 0;}
        let victim=w.snake(state.prey-1).unwrap();
        let mut blocked=0;
        let mut attacker_distance=[0.0;73];
        for j in 1..=72 {attacker_distance[j]=attacker_distance[j-1]+w.distance_squared(c.path[j-1],c.path[j]).sqrt();}
        let mut rivals=[Rival::default();MAX_SNAKES];
        for (offset,delay,rush) in [(0.0,0,0.0),(1.6,0,0.0),(-1.6,0,0.0),(1.6,6,0.0),(-1.6,6,0.0),(0.0,0,1.0)] {
            blocked+=self.reply_simulation(w,s,state,c,victim,offset,delay,rush,&attacker_distance,&mut rivals) as usize;
        }
        blocked
    }
    #[cfg(test)]
    pub(super) fn reply_blocked(&self,w:&World,s:SnakeView<'_>,c:&Candidate,victim:SnakeView<'_>,offset:f64,delay:usize,rush:f64,attacker_distance:&[f64;73])->bool {
        self.reply_simulation(w,s,self.states[s.id as usize],c,victim,offset,delay,rush,attacker_distance,&mut [Rival::default();MAX_SNAKES])
    }
    fn reply_simulation(&self,w:&World,s:SnakeView<'_>,state:State,c:&Candidate,victim:SnakeView<'_>,offset:f64,delay:usize,rush:f64,attacker_distance:&[f64;73],rivals:&mut [Rival;MAX_SNAKES])->bool {
        let reach=(s.radius+victim.radius)*0.78;
        let motion=if rush>0.0 {self.boosted_motion[victim.id as usize]} else {self.motion[victim.id as usize]};
        let (speed,turn)=w.motion_limits(victim.id as usize,rush).unwrap();
        let mut forecast=self.intent_forecast::<true>(w,s,state,72,(if c.rush>0.0 {1<<s.id} else {0}) | (if rush>0.0 {1<<victim.id} else {0}));
        for rival in rivals.iter_mut() {rival.dynamic=false;}
        let mut path=[victim.segments[0].current;73];let mut angle=victim.angle;
        let mut victim_distance=[0.0;73];
        for j in 1..=72 {
            let (speed,turn)=if w.config().rules==crate::RuleSet::V2 {forecast.motion(w,victim.id as usize,rush,&motion,j)} else {(speed,turn)};
            victim_distance[j]=victim_distance[j-1]+speed*STEP_SECONDS;
            let desired=victim.angle+if j>delay {offset} else {0.0};
            angle=normalize_angle(angle+normalize_angle(desired-angle).clamp(-turn*STEP_SECONDS,turn*STEP_SECONDS));
            let q=w.canonical_point(Point{x:path[j-1].x+angle.cos()*speed*STEP_SECONDS,y:path[j-1].y+angle.sin()*speed*STEP_SECONDS});path[j]=q;
            if forecast.movement_changed() & !((1<<s.id)|(1<<victim.id))!=0 {
                Self::advance_rivals(w,rivals,&self.motion,&forecast,j,(1<<s.id)|(1<<victim.id),Some((&self.effects,&self.rivals)),None);
            }
            if forecast.has_items() {
                let own_motion=if c.rush>0.0 {self.boosted_motion[s.id as usize]} else {self.motion[s.id as usize]};
                let radius=forecast.radius(w,s.id as usize,c.rush,&own_motion,j);forecast.set_radius(s.id as usize,radius);
                let radius=forecast.radius(w,victim.id as usize,rush,&motion,j);forecast.set_radius(victim.id as usize,radius);
                for (id,_) in rivals.iter().enumerate().filter(|(_,r)|r.dynamic) {
                    let radius=forecast.radius(w,id,0.0,&self.motion[id],j);forecast.set_radius(id,radius);
                }
                let changed=(1<<s.id) | (1<<victim.id) | rivals.iter().enumerate().fold(0,|mask,(id,r)|mask | if r.dynamic {1<<id} else {0});
                forecast.advance_cached(w,j,|id|if id==s.id as usize {c.path[j]} else if id==victim.id as usize {q} else {Self::forecast_rival(rivals,&self.rivals,id).path[j]},changed,&self.item_forecast,0.0);
            }
            let own_phased=forecast.effects.phased(s.id as usize,j);
            let victim_phased=forecast.effects.phased(victim.id as usize,j);
            let cfg=w.config();
            if cfg.deadly_walls && (q.x<0.0 || q.x>cfg.width || q.y<0.0 || q.y>cfg.height) {break;}
            if cfg.self_collisions && !victim_phased {
                let travel=victim_distance[j];
                let neck=(10.0-travel/(victim.radius*1.18)).max(1.0) as usize;
                let own_hit=victim.segments.iter().enumerate().skip(neck).any(|(k,b)| {
                    if travel>(victim.segments.len()-k) as f64*victim.radius*1.18 {return false;}
                    let d=w.displacement(q,b.current);
                    let reserve=spatial::query_radius(victim.radius*1.48,1.0,1.0,speed*STEP_SECONDS*2.0);
                    if d.x.abs()>=reserve || d.y.abs()>=reserve {return false;}
                    let body=if cfg.rules==crate::RuleSet::V2 {
                        crate::world::taper::span_radius(victim.radius,
                            k as f64/victim.segments.len().saturating_sub(1).max(1) as f64,
                            (k as f64+travel/(victim.radius*1.18))/victim.segments.len().saturating_sub(1).max(1) as f64)
                    } else {victim.radius};
                    let own_reach=crate::world::taper::contact_radius(cfg.rules,victim.radius,body,true)+1.0;
                    d.x.abs()<own_reach+speed*STEP_SECONDS*2.0 && d.y.abs()<own_reach+speed*STEP_SECONDS*2.0
                        && w.segment_distance_squared(b.current,path[j-1],q)<own_reach*own_reach
                });
                if own_hit {break;}
            }
            if cfg.self_collisions && !victim_phased && Self::deposited_contact(w,path[j-1],q,&path,&victim_distance,j,
                10.0*victim.radius*1.18,victim.segments.len().saturating_sub(1) as f64*victim.radius*1.18,victim.radius*1.48,(victim.radius,victim.radius,true)) {break;}
            if own_phased || victim_phased {continue;}
            let head_contact=Self::head_contact(w,&path,&c.path,j-1,j,(s.radius+victim.radius)*0.82);
            if head_contact && c.body_len>=victim.segments.len()+4 {return true;}
            // Distance traveled governs neck exemption and actual barrier
            // persistence; never use the slow safety tail-release bound.
            if Self::deposited_contact(w,path[j-1],q,&c.path[..73],attacker_distance,j,
                s.radius*1.18,c.body_len.saturating_sub(1) as f64*s.radius*1.18,reach,(victim.radius,s.radius,false)) {return true;}
        }
        false
    }

    /// Exact contiguous one-tick edges. Clip both ends to the still-live body
    /// by traveled distance, so neither neck nor released tail is a barrier.
    pub(super) fn deposited_contact(w:&World,a:Point,b:Point,path:&[Point],distance:&[f64;73],j:usize,neck:f64,length:f64,reach:f64,radii:(f64,f64,bool))->bool {
        if length<=neck {return false;}
        let oldest=distance[j]-length;let newest=distance[j]-neck;
        let ab=w.displacement(a,b);
        let bound=spatial::query_radius(reach,0.0,1.0,0.0);
        for k in 1..=j {
            if distance[k]<oldest {continue;}
            if distance[k-1]>newest {break;}
            let edge=w.displacement(path[k-1],path[k]);
            let ap=w.displacement(a,path[k-1]);let bp=Point{x:ap.x+edge.x,y:ap.y+edge.y};
            if ap.x.min(bp.x)>ab.x.max(0.0)+bound || ap.x.max(bp.x)<ab.x.min(0.0)-bound
                || ap.y.min(bp.y)>ab.y.max(0.0)+bound || ap.y.max(bp.y)<ab.y.min(0.0)-bound {continue;}
            let reach=if w.config().rules==crate::RuleSet::V2 {
                let (head,body,same)=radii;
                let radius=crate::world::taper::span_radius(body,
                    ((distance[j]-distance[k])/length).clamp(0.0,1.0),
                    ((distance[j]-distance[k-1])/length).clamp(0.0,1.0));
                crate::world::taper::contact_radius(w.config().rules,head,radius,same)
            } else {reach};
            let span=distance[k]-distance[k-1];
            if span<=1e-12 {continue;}
            let lo=((oldest-distance[k-1])/span).clamp(0.0,1.0);
            let hi=((newest-distance[k-1])/span).clamp(0.0,1.0);
            if lo>hi {continue;}
            let start=Point{x:a.x+ap.x+edge.x*lo,y:a.y+ap.y+edge.y*lo};
            let end=Point{x:a.x+ap.x+edge.x*hi,y:a.y+ap.y+edge.y*hi};
            if w.segments_distance_squared(a,b,start,end)<reach*reach {return true;}
        }
        false
    }
}
