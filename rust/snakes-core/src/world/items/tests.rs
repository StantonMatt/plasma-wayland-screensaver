// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
#[test]
fn spawn_timer_cap_clearance_kinds_and_determinism() {
    let mut w=World::new(Config{rules:RuleSet::V2,..Config::default()}).unwrap();
    let mut copy=w.diagnostic_snapshot();
    for _ in 0..100 {
        assert!((450..=900).contains(&w.item_timer));
        w.items.clear();copy.items.clear();
        let last=w.last_item_kind;
        w.spawn_item();copy.spawn_item();
        assert_eq!(w.items,copy.items);assert_eq!(w.rng_state(),copy.rng_state());
        let item=w.items[0];let r=w.config.base_radius();
        assert!(effects::ENABLED_KINDS.contains(&item.kind));assert_ne!(item.kind,last);
        assert!(item.position.x>=10.0*r && item.position.x<=w.config.width-10.0*r);
        assert!(item.position.y>=10.0*r && item.position.y<=w.config.height-10.0*r);
        assert_eq!(item.life_ticks,780);
        w.reset_item_timer();copy.reset_item_timer();
    }
    w.items.clear();
    for _ in 0..w.item_cap() {w.spawn_item();}
    for (i,a) in w.items.iter().enumerate() {for b in &w.items[i+1..] {assert!(w.distance_squared(a.position,b.position)>=(20.0*w.config.base_radius()).powi(2));}}
    w.item_timer=1;w.advance_items_and_effects();assert!(w.items.len()<=w.item_cap());
    assert_eq!(World::new(Config{rules:RuleSet::V2,density:0.0,..Config::default()}).unwrap().item_cap(),1);
    assert_eq!(World::new(Config{rules:RuleSet::V2,density:100.0,..Config::default()}).unwrap().item_cap(),3);
}
#[test]
fn pickup_replacement_and_exact_expiry() {
    let mut w=World::new(Config{rules:RuleSet::V2,..Config::default()}).unwrap();
    let p=w.segments[0].current;
    for kind in [EffectKind::Magnet,EffectKind::Surge,EffectKind::Phase] {
        w.items.push(Item{id:1,kind,position:p,life_ticks:750,radius:w.config.base_radius()*2.1,..Item::default()});
        w.pickup_items();
        assert!(w.items.is_empty());assert_eq!(w.snakes[0].effect_kind,kind as u8);
        assert_eq!(w.snakes[0].effect_ticks,kind.duration());
        assert!(w.frame_events().any(|e|e.kind==EventKind::Pickup && e.snake_id==0 && e.other_snake_id==kind as u32));
    }
    for _ in 0..119 {w.advance_items_and_effects();}
    assert_eq!(w.snakes[0].effect_ticks,1);
    w.advance_items_and_effects();assert_eq!(w.snakes[0].effect_ticks,0);assert_eq!(w.snakes[0].effect_kind,0);
    assert!(w.frame_events().any(|e|e.kind==EventKind::EffectExpiry));
}
#[test]
fn item_expiry_off_and_classic_do_not_spawn() {
    let mut w=World::new(Config{rules:RuleSet::V2,..Config::default()}).unwrap();w.spawn_item();
    w.items[0].life_ticks=1;w.advance_items_and_effects();assert!(w.items.is_empty());
    assert!(w.frame_events().any(|e|e.kind==EventKind::ItemExpiry));
    w.spawn_item();w.snakes[0].effect_kind=2;w.snakes[0].effect_ticks=300;
    let rng=w.rng_state();w.reconfigure(Config{power_ups:false,..w.config()}).unwrap();
    assert!(w.items.is_empty());assert_eq!(w.snakes[0].effect_ticks,0);assert_eq!(w.snakes[0].effect_kind,0);
    assert_eq!(rng,w.rng_state());
    let classic=World::new(Config::default()).unwrap();assert_eq!(classic.item_timer,0);
    let off=World::new(Config{power_ups:false,..Config::default()}).unwrap();assert_eq!(classic.rng_state(),off.rng_state());
}
#[test]
fn timer_birth_blink_pickup_boundary_and_resize() {
    let mut w=World::new(Config{rules:RuleSet::V2,density:0.0,..Config::default()}).unwrap();
    for s in &mut w.snakes {s.alive=false;s.len=0;}
    w.item_timer=450;
    for _ in 0..449 {w.advance_items_and_effects();}
    assert!(w.items.is_empty());w.advance_items_and_effects();assert_eq!(w.items.len(),1);
    assert_eq!(w.items[0].age_ticks,0);assert_eq!(w.items[0].life_ticks,780);
    for _ in 0..690 {w.advance_items_and_effects();}
    assert!(w.items[0].blinking());assert_eq!(w.items[0].life_ticks,90);
    w.tick=690; // direct counter helper does not advance the world clock
    let item=w.items[0];w.resize(1600.0,900.0).unwrap();
    assert_eq!(w.items[0].position.x,item.position.x*1.25);
    assert_eq!(w.items[0].position.y,item.position.y*1.25);
    w.reconfigure(Config{scale:150.0,..w.config()}).unwrap();
    assert_eq!(w.items[0].radius,w.config.base_radius()*2.1);
    w.snakes[0].alive=true;w.snakes[0].len=1;w.snakes[0].radius=6.0;
    let pos=w.items[0].position;let reach=1.3*6.0+w.items[0].radius;
    w.segments[0].current=Point{x:pos.x+reach+0.01,y:pos.y};w.pickup_items();assert_eq!(w.items.len(),1);
    w.segments[0].current=Point{x:pos.x+reach-0.01,y:pos.y};w.pickup_items();assert!(w.items.is_empty());
}

#[test]
fn incoming_capsule_is_visible_for_thirty_ticks_before_pickup() {
    let mut w=World::new(Config{rules:RuleSet::V2,..Config::default()}).unwrap();w.spawn_item();
    let item=w.items[0];assert_eq!(item.pickable_from_tick,31);
    let p=item.position;w.segments[0].current=p;
    for tick in 0..30 {w.tick=tick;w.pickup_items();assert_eq!(w.items.len(),1);}
    w.tick=30;w.pickup_items();assert!(w.items.is_empty());
    assert_eq!(w.faces[0].happy_ticks,45);
}
