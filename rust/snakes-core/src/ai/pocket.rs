// SPDX-License-Identifier: GPL-3.0-or-later
//! Classic wall-U closure and V2 bounded open-space encirclement.
use super::*;
impl AiController {
    pub(super) fn spiral_curvature(radius:f64,pitch:f64)->f64 {
        let k=pitch/std::f64::consts::TAU;
        (radius*radius+2.0*k*k)/(radius*radius+k*k).powf(1.5)
    }
    pub(super) fn advance_spiral(w:&World,state:&mut State,head:Point) {
        if state.coil_radius<=0.0 {return;}
        let d=w.displacement(state.coil_center,head);let theta=d.y.atan2(d.x);
        let change=normalize_angle(theta-state.coil_last_angle)*state.coil_sign;
        state.coil_progress+=change;
        state.coil_last_angle=theta;
        state.coil_distance+=w.distance_squared(state.coil_last_head,head).sqrt();state.coil_last_head=head;
        state.coil_radius=state.coil_initial_radius-state.coil_pitch/std::f64::consts::TAU*(state.coil_progress-0.9).max(0.0);
    }
    /// Retained pockets must remain feasible on every decision, independently
    /// of the quota and tactical response deadline. This never admits a pocket.
    pub(super) fn pocket_usable(&self,w:&World,s:SnakeView<'_>,state:State)->bool {
        if state.prey==0 || (w.config().rules==crate::RuleSet::Classic && !w.config().deadly_walls) || w.tick()>state.hunt_until
            || w.tick()<state.escape_until || w.tick()<state.orbit_until || w.tick()<state.dodge_until {return false;}
        // The entry arm's tail-release reserve was measured in segments at
        // this body size. Do not reuse that reserve after shrinking/resizing.
        if state.coil_body.is_some_and(|body|body!=(s.segments.len(),s.radius)) {return false;}
        let Some(victim)=w.snake(state.prey-1).filter(|v|v.alive && v.generation==state.prey_generation) else {return false;};
        let (speed,turn)=w.motion_limits(s.id as usize,0.0).unwrap();
        let min_radius=(speed/turn*1.15+s.radius+victim.radius).max(if w.config().rules==crate::RuleSet::V2 {4.2*s.radius} else {0.0});
        let lo=s.radius*1.48+2.0;let hi=2.0*(s.radius+victim.radius)*0.78-2.0;
        let release=state.coil_release_distance.min(s.segments.len() as f64*s.radius*1.18);
        s.segments.len()>=victim.segments.len()*3 && state.coil_pitch>lo && state.coil_pitch<hi
            && w.distance_squared(state.coil_center,victim.segments[0].current).sqrt()<state.coil_radius-victim.radius
            && state.coil_distance+speed*0.6<release && state.coil_radius>=min_radius
            && state.coil_progress>= -0.2 && speed*Self::spiral_curvature(state.coil_radius,state.coil_pitch)<=turn
            && w.tick()<=state.coil_until
    }
    pub(super) fn pocket(&self,w:&World,s:SnakeView<'_>,state:&mut State)->bool {
        let head=s.segments[0].current;let victim=&self.rivals[state.prey-1];
        let (speed,turn)=w.motion_limits(s.id as usize,0.0).unwrap();
        let min_radius=(speed/turn*1.15+s.radius+victim.radius).max(if w.config().rules==crate::RuleSet::V2 {4.2*s.radius} else {0.0});
        let lo=s.radius*1.48+2.0;let hi=2.0*(s.radius+victim.radius)*0.78-2.0;
        if state.coil_radius>0.0 {
            if !self.pocket_usable(w,s,*state) {state.clear_coil(s.angle);return false;}
            state.goal=Self::trajectory_goal(w,*state,head);return true;
        }
        if w.config().rules==crate::RuleSet::V2 {
            if s.segments.len()<150 || s.segments.len()<victim.len*3 || hi<=lo {return false;}
            let center=victim.path[0];let radius=w.distance_squared(center,head).sqrt();
            let pitch=(lo+hi)*0.5;
            let body=s.segments.len() as f64*s.radius*1.18;
            if radius>20.0*s.radius || radius<min_radius+pitch || body<std::f64::consts::TAU*radius*1.2
                || speed*Self::spiral_curvature(radius-pitch,pitch)>turn {return false;}
            if w.config().deadly_walls && center.x.min(w.config().width-center.x).min(center.y).min(w.config().height-center.y)<radius+s.radius {return false;}
            let d=w.displacement(center,head);let theta=d.y.atan2(d.x);
            let sign=if normalize_angle(theta+std::f64::consts::FRAC_PI_2-s.angle).abs()
                <normalize_angle(theta-std::f64::consts::FRAC_PI_2-s.angle).abs() {1.0} else {-1.0};
            // Entry and the existing body still pass the normal safety rollout.
            state.coil_center=center;state.coil_sign=sign;state.coil_pitch=pitch;
            state.coil_radius=radius;state.coil_initial_radius=radius;
            state.coil_progress=0.0;state.coil_distance=0.0;
            state.coil_last_head=head;state.coil_last_angle=theta;state.coil_release_distance=body;
            state.coil_body=Some((s.segments.len(),s.radius));
            state.coil_until=w.tick()+(body/speed/STEP_SECONDS).ceil() as u64;
            state.hunt_until=state.hunt_until.max(state.coil_until);
            state.clear_attacks(s.angle);state.track_goal=true;
            state.goal=Self::trajectory_goal(w,*state,head);return true;
        }
        if !w.config().deadly_walls || s.segments.len()<victim.len*3 || hi-lo<3.0 {return false;}
        // The prey must already lie in a wall-assisted U supplied by our body.
        // Reject torus/seam topology and expensive general enclosure search.
        let center=victim.path[0];let radius=w.distance_squared(center,head).sqrt();
        let cfg=w.config();let wall=center.x.min(cfg.width-center.x).min(center.y).min(cfg.height-center.y);
        let pitch=(lo+hi)*0.5;let inner_radius=radius-pitch;
        if inner_radius<min_radius || speed*Self::spiral_curvature(inner_radius,pitch)>turn {return false;}
        if radius<min_radius+20.0 || radius>190.0 || wall<radius+5.0 || wall>radius+40.0 {return false;}
        let mut bins=0u16;let mut max_body=0;
        for (j,b) in s.segments.iter().enumerate().skip(10).take(s.segments.len().saturating_sub(28)) {
            let d=w.displacement(center,b.current);let r=(d.x*d.x+d.y*d.y).sqrt();
            if (r-radius).abs()>s.radius*3.0 {continue;}
            let theta=d.y.atan2(d.x).rem_euclid(std::f64::consts::TAU);
            bins|=1<<((theta/std::f64::consts::TAU*12.0) as usize).min(11);max_body=max_body.max(j);
        }
        if bins.count_ones()<8 {return false;}
        let release=(s.segments.len()-max_body) as f64*s.radius*1.18;
        let closure=radius*0.9;
        // Enough distance before tail release for closing AND a safe exit.
        if release<closure+speed*0.6+s.radius*10.0 {return false;}
        let d=w.displacement(center,head);let theta=d.y.atan2(d.x);
        let sign=if normalize_angle(theta+std::f64::consts::FRAC_PI_2-s.angle).abs()
            <normalize_angle(theta-std::f64::consts::FRAC_PI_2-s.angle).abs() {1.0} else {-1.0};
        if normalize_angle(theta+sign*std::f64::consts::FRAC_PI_2-s.angle).abs()>0.65 {return false;}
        state.coil_center=center;state.coil_sign=sign;state.coil_pitch=pitch;
        // Enter one legal pitch inside the old U before crossing its far arm.
        // Following the old radius would hit our still-present closing body.
        state.coil_radius=inner_radius;state.coil_initial_radius=state.coil_radius;
        state.coil_progress=0.0;state.coil_distance=0.0;
        state.coil_last_head=head;state.coil_last_angle=theta;state.coil_release_distance=release;
        state.coil_body=Some((s.segments.len(),s.radius));
        state.coil_until=w.tick()+120;state.clear_attacks(s.angle);state.track_goal=true;
        state.goal=Self::trajectory_goal(w,*state,head);true
    }
}
