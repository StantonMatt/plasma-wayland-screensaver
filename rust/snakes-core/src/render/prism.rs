// SPDX-License-Identifier: GPL-3.0-or-later
//! Zero-primitive gulp and Feast colour bands. Both geometry paths share timing.
use super::*;
pub(super) const HUES:[Color;6]=[Color {red:255,green:100,blue:120,alpha:255},Color {red:255,green:180,blue:90,alpha:255},
    Color {red:245,green:240,blue:120,alpha:255},Color {red:120,green:245,blue:170,alpha:255},Color {red:100,green:205,blue:255,alpha:255},Color {red:195,green:145,blue:255,alpha:255}];
pub(super) const HALLOWEEN_HUES:[Color;6]=[Color {red:255,green:117,blue:24,alpha:255},Color {red:255,green:179,blue:71,alpha:255},
    Color {red:182,green:240,blue:74,alpha:255},Color {red:155,green:107,blue:255,alpha:255},Color {red:208,green:92,blue:255,alpha:255},Color {red:239,green:62,blue:54,alpha:255}];
pub(super) fn hues(season:u8)-> &'static [Color;6] {if season==1 {&HALLOWEEN_HUES} else {&HUES}}
pub(super) fn centers(s:&SnakeRecord,info:&FrameInfo,p:&Params,calm:bool)->[(f64,f64);2] {
    let mut result=[(-100.0,0.0);2];
    if s.alive==0 || s.flags&flags::CORPSE!=0 {return result;}
    for (out,b) in result.iter_mut().zip(s.bulges) {
        if b.duration_ticks==0 || !b.strength.is_finite() || b.strength<=0.0 || b.start_tick>info.tick {continue;}
        let age=info.tick.saturating_sub(b.start_tick) as f64+(p.presentation_time-info.simulation_time).max(0.0)*30.0;
        let duration=b.duration_ticks as f64*if calm {0.6} else {1.0};
        if age<duration {*out=(b.origin_segment as f64+age/duration*(s.segment_count.saturating_sub(1)) as f64,b.strength.min(0.35) as f64);}
    }
    result
}
pub(super) fn widen(normals:&mut [P],centers:[(f64,f64);2]) {
    // Union, rather than sum, keeps even overlapping gulps <=35% wider.
    for slot in 0..2 {
        let (center,strength)=centers[slot];if strength==0.0 {continue;}
        let first=(center-6.3).max(0.0).ceil() as usize;
        let last=((center+6.3).floor() as usize).min(normals.len()-1);
        for i in first..=last {
            if slot==1 && centers[0].1>0.0 && (i as f64-centers[0].0).abs()<=6.3 {continue;}
            // Most gulps occupy only one slot. An inactive slot contributes
            // exactly zero; avoid its underflowing exponential per vertex.
            let a=centers.iter().filter(|&&(_,s)|s>0.0)
                .map(|&(c,s)|{let z=(i as f64-c)/2.1;s*(-z*z).exp()}).fold(0.0_f64,f64::max);
            normals[i]=normals[i]*(1.0+a);
        }
    }
}
pub(super) fn rainbow(season:u8,s:&SnakeRecord,waves:[shader::Wave;2],info:&FrameInfo,p:&Params,calm:bool,palette:&[Color],base:Color,out:&mut [Color])->bool {
    // Rides the bulge: 2-segment front, full-strength 5 segments behind it, then a
    // 5-segment exponential rainbow wake toward the head;
    // continuous hue, then a 0.5 s fade once the bulge leaves the tail.
    // Colour alpha carries the light weight (0..255) for the brightness byte.
    if s.alive==0 || s.flags&flags::CORPSE!=0 || out.is_empty() {return false;}
    let hues=hues(season);
    const WAKE_FADE:f64=0.5;
    let scale=if calm {0.6} else {1.0};
    let time=event_time(info,p,calm);
    let lifetime=|w:shader::Wave|w.duration_ticks as f64*crate::STEP_SECONDS*scale+WAKE_FADE*scale;
    let active=waves.iter().any(|w|w.active && w.kind==8 && w.duration_ticks>0 && (0.0..lifetime(*w)).contains(&(time-w.time)));
    if !active {return false;}out.fill(base.alpha(0));
    let last=(out.len()-1) as f64;
    for w in waves {
        let age=time-w.time;if !w.active || w.kind!=8 || w.duration_ticks==0 || !(0.0..lifetime(w)).contains(&age) {continue;}
        // Event duration is fixed at ingestion, just like its accompanying bulge.
        // Growth and boost payments only change the current tail position.
        let duration=w.duration_ticks as f64*crate::STEP_SECONDS*scale;
        let life=lifetime(w);
        let center=(age/duration).min(1.0)*last;
        let fade=if age>duration {1.0-(age-duration)/(life-duration)} else {1.0};
        let first=(center-20.0).max(0.0).ceil() as usize;let end=((center+2.0).floor() as usize).min(out.len()-1);
        for i in first..=end {
            let d=center-i as f64;
            let weight=if d<0.0 {(1.0+d/2.0).max(0.0)} else if d<=5.0 {1.0} else {(-(d-5.0)/5.0).exp()}*fade;
            let light=(weight*0.6*255.0).round() as u8;
            if light<=out[i].alpha {continue;}
            let h=(d/11.0-if calm {0.0} else {age*0.3}).rem_euclid(1.0)*6.0;
            let k=h.floor() as usize;let f=h-k as f64;
            let (a,b)=(hues[k%6],hues[(k+1)%6]);
            let lerp=|x:u8,y:u8|x as f64+(y as f64-x as f64)*f;
            let hue=items::tint(Color::new(lerp(a.red,b.red).round() as u8,lerp(a.green,b.green).round() as u8,lerp(a.blue,b.blue).round() as u8,255),palette);
            let m=0.9*weight;
            let mix=|x:u8,y:u8|(x as f64+(y as f64-x as f64)*m).round() as u8;
            out[i]=Color::new(mix(base.red,hue.red),mix(base.green,hue.green),mix(base.blue,hue.blue),light);
        }
    }true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn feast_colors(age:f64, calm:bool, palette:&[Color])->(bool,[Color;80]) {
        let base=Color::new(240,240,240,255);
        let mut out=[base;80];
        let active=rainbow(0,&SnakeRecord {alive:1,segment_count:80,..Default::default()},
            [shader::Wave {active:true,kind:8,time:0.0,duration_ticks:90},shader::Wave::default()],
            &FrameInfo {simulation_time:age,..FrameInfo::default()},
            &Params {presentation_time:age,..Params::default()},calm,palette,base,&mut out);
        (active,out)
    }
    #[test]
    fn rainbow_comet_has_bright_core_continuous_hue_and_headward_wake() {
        let duration=3.0;
        let age=duration*40.0/79.0;
        let (active,out)=feast_colors(age,false,&[]);
        assert!(active);
        assert_eq!(out[40].alpha,153);
        assert_eq!(out[35].alpha,153);
        assert_eq!(out[41].alpha,77);
        assert_eq!(out[42].alpha,0);
        assert!(out[20].alpha>0 && out[19].alpha==0);
        assert!(out[34].alpha<out[35].alpha && out[34].alpha>out[33].alpha);
        // Test immediately across an old whole-table time step, and across
        // segment centres. Neither may jump a hue entry anymore.
        let (_,a)=feast_colors(1.0/3.0-0.00001,false,&[]);
        let (_,b)=feast_colors(1.0/3.0+0.00001,false,&[]);
        for (a,b) in a.iter().zip(b) {
            assert!(a.red.abs_diff(b.red)<=1 && a.green.abs_diff(b.green)<=1
                && a.blue.abs_diff(b.blue)<=1 && a.alpha.abs_diff(b.alpha)<=1);
        }
        for pair in out[35..=40].windows(2) {
            assert!(pair[0].red.abs_diff(pair[1].red)<90
                && pair[0].green.abs_diff(pair[1].green)<90 && pair[0].blue.abs_diff(pair[1].blue)<90);
        }
    }
    #[test]
    fn rainbow_tail_fades_and_calm_shortens_without_hue_drift() {
        let duration=3.0;
        for (calm,scale) in [(false,1.0),(true,0.6)] {
            let (active,full)=feast_colors(duration*scale,calm,&[]);
            let (fading,half)=feast_colors((duration+0.25)*scale,calm,&[]);
            assert!(active && fading);
            assert_eq!(full[79].alpha,153);
            assert!(half[79].alpha.abs_diff(77)<=1);
            assert!(!feast_colors((duration+0.5)*scale+0.00001,calm,&[]).0);
            assert!(!feast_colors(-0.01,calm,&[]).0);
        }
        let (_,moving)=feast_colors(duration,false,&[]);
        let (_,calm)=feast_colors(duration*0.6,true,&[]);
        assert_ne!(moving[79],calm[79]);
        // Calm hue at the centre remains the first spectrum entry (no drift).
        assert_eq!(calm[79],Color::new(254,114,132,153));
    }
    #[test]
    fn rainbow_preserves_palette_modes_and_strongest_overlapping_wave() {
        let age=3.0*40.0/79.0;
        let (_,normal)=feast_colors(age,false,&[]);
        let (_,mono)=feast_colors(age,false,&[Color::new(255,255,255,255)]);
        let (_,pastel)=feast_colors(age,false,&[Color::new(255,200,221,255)]);
        assert_eq!(normal[40].alpha,mono[40].alpha);
        assert_eq!(normal[40].alpha,pastel[40].alpha);
        assert!(mono[40].red.abs_diff(mono[40].green)<20 && mono[40].green.abs_diff(mono[40].blue)<20);
        assert!(pastel[40].red>=normal[40].red && pastel[40].green>=normal[40].green && pastel[40].blue>=normal[40].blue);
        let s=SnakeRecord {alive:1,segment_count:80,..Default::default()};
        let p=Params {presentation_time:age,..Params::default()};
        let waves=[shader::Wave {active:true,kind:8,time:0.0,duration_ticks:90},shader::Wave {active:true,kind:8,time:-0.2,duration_ticks:90}];
        let base=Color::new(240,240,240,255);
        let mut both=[base;80];let mut older=[base;80];
        assert!(rainbow(0,&s,waves,&FrameInfo::default(),&p,false,&[],base,&mut both));
        assert!(rainbow(0,&s,[waves[1],shader::Wave::default()],&FrameInfo::default(),&p,false,&[],base,&mut older));
        for j in 0..80 {assert_eq!(both[j].alpha,normal[j].alpha.max(older[j].alpha));}
        assert!(!rainbow(0,&s,waves,&FrameInfo::default(),&p,false,&[],base,&mut []));
    }
    #[test]
    fn feast_does_not_override_corpse_flash_or_dead_colours() {
        let base=Color::new(240,240,240,128);
        let p=Params {presentation_time:0.5,..Params::default()};
        let waves=[shader::Wave {active:true,kind:8,time:0.0,duration_ticks:90},shader::Wave::default()];
        for (alive,flags) in [(0,0),(1,flags::CORPSE)] {
            let s=SnakeRecord {alive,flags,segment_count:26,..Default::default()};
            let mut colors=[base;26];
            assert!(!rainbow(0,&s,waves,&FrameInfo::default(),&p,false,&[],base,&mut colors));
            assert_eq!(colors,[base;26]);
        }
    }
    #[test]
    fn feast_keeps_event_duration_through_meal_growth_and_boost_payments() {
        let base=Color::new(240,240,240,255);
        let wave=shader::Wave {time:20.0,active:true,kind:8,duration_ticks:90};
        // 78 samples at ingestion, five added by the meal, then boost payments.
        for n in [78,83,81,70] {for calm in [false,true] {
            let scale=if calm {0.6} else {1.0};
            let s=SnakeRecord {alive:1,segment_count:n,bulges:[crate::world::Bulge {
                start_tick:600,duration_ticks:90,strength:0.35,..Default::default()
            };2],..Default::default()};
            let age:f64=1.5*scale;
            let info=FrameInfo {tick:600+(age*30.0).round() as u64,simulation_time:20.0+age,..Default::default()};
            let p=Params {presentation_time:if calm {19.0} else {20.0+age},..Default::default()};
            let center=centers(&s,&info,&p,calm)[0].0;
            assert!((center-(n-1) as f64*0.5).abs()<0.00001);
            let mut out=[base;83];
            assert!(rainbow(0,&s,[wave,shader::Wave::default()],&info,&p,calm,&[],base,&mut out[..n as usize]));
            // Front and wake stay centred on the same travelling gulp.
            assert_eq!(out[center.floor() as usize].alpha,153);
            assert!(out[(center+2.0).ceil() as usize].alpha==0);
            let info=FrameInfo {simulation_time:20.0+3.25*scale,..info};
            let p=Params {presentation_time:if calm {19.0} else {info.simulation_time},..p};
            assert!(rainbow(0,&s,[wave,shader::Wave::default()],&info,&p,calm,&[],base,&mut out[..n as usize]));
            assert!(out[n as usize-1].alpha.abs_diff(77)<=1,"tail fade uses ingestion duration: n={n}, calm={calm}");
            let info=FrameInfo {simulation_time:20.0+3.5*scale+0.0001,..info};
            let p=Params {presentation_time:if calm {19.0} else {info.simulation_time},..p};
            assert!(!rainbow(0,&s,[wave,shader::Wave::default()],&info,&p,calm,&[],base,&mut out[..n as usize]));
        }}
    }
    #[test]
    fn overlapping_feasts_keep_independent_durations_and_ignore_zero_duration() {
        let s=SnakeRecord {alive:1,segment_count:80,..Default::default()};
        let base=Color::new(240,240,240,255);let mut out=[base;80];
        let info=FrameInfo::default();let p=Params {presentation_time:2.0,..Default::default()};
        let short=shader::Wave {time:0.0,active:true,kind:8,duration_ticks:30};
        let long=shader::Wave {duration_ticks:90,..short};
        assert!(!rainbow(0,&s,[short,shader::Wave::default()],&info,&p,false,&[],base,&mut out));
        assert!(rainbow(0,&s,[short,long],&info,&p,false,&[],base,&mut out));
        let both=out;
        assert!(rainbow(0,&s,[long,shader::Wave::default()],&info,&p,false,&[],base,&mut out));
        assert_eq!(both,out);
        assert!(!rainbow(0,&s,[shader::Wave {duration_ticks:0,..long};2],&info,&p,false,&[],base,&mut out));
    }
    #[test]
    fn overlapping_gulps_cap_width_expire_and_calm_shortens_duration() {
        let mut s=SnakeRecord {alive:1,segment_count:80,..Default::default()};
        s.bulges=[crate::world::Bulge {start_tick:0,duration_ticks:90,strength:0.35,..Default::default()};2];
        let info=FrameInfo {tick:45,..Default::default()};let p=Params::default();
        let normal=P::new(0.0,1.0);let mut normals=[normal;80];
        widen(&mut normals,centers(&s,&info,&p,false));
        assert!(normals.iter().all(|n|n.y<=1.350001 && n.y>=1.0));assert!(normals.iter().any(|n|n.y>1.32));
        let c=centers(&s,&info,&p,true);assert!(c[0].0>60.0);
        assert!(centers(&s,&FrameInfo {tick:54,..info},&p,true).iter().all(|c|c.1==0.0));
        assert!(centers(&s,&FrameInfo {tick:90,..info},&p,false).iter().all(|c|c.1==0.0));
    }
}
