// SPDX-License-Identifier: GPL-3.0-or-later
//! Public V2 AI path, including its first decision and emergency scratch.
use snakes_core::{ai::AiController,Config,RuleSet,World,flags};
use std::alloc::{GlobalAlloc,Layout,System};
use std::cell::Cell;
thread_local! {
    static ENABLED:Cell<bool>=const {Cell::new(false)};
    static COUNT:Cell<usize>=const {Cell::new(0)};
}
struct Counting;
#[global_allocator]
static ALLOCATOR:Counting=Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self,layout:Layout)->*mut u8 {
        ENABLED.with(|enabled|if enabled.get() {COUNT.with(|count|count.set(count.get()+1));});
        unsafe {System.alloc(layout)}
    }
    unsafe fn realloc(&self,pointer:*mut u8,layout:Layout,size:usize)->*mut u8 {
        ENABLED.with(|enabled|if enabled.get() {COUNT.with(|count|count.set(count.get()+1));});
        unsafe {System.realloc(pointer,layout,size)}
    }
    unsafe fn dealloc(&self,pointer:*mut u8,layout:Layout) {unsafe {System.dealloc(pointer,layout)}}
}
#[test]
fn v2_ai_boost_planning_and_first_tick_allocate_nothing() {
    let mut w=World::new(Config {width:3440.0,height:1440.0,density:100.0,trails:100.0,
        intelligence:100.0,seed:73,self_collisions:true,deadly_walls:false,rules:RuleSet::V2,..Config::default()}).unwrap();
    let mut ai=AiController::new();
    let mut boosting=false;let mut hunting=false;let mut bubbles=false;let mut racing=false;
    COUNT.with(|count|count.set(0));ENABLED.with(|enabled|enabled.set(true));
    for _ in 0..3000 {
        w.step(&mut ai);
        bubbles|=!w.bubbles().is_empty();
        assert!(w.bubbles().len()<=3);
        racing|=w.items().any(|item|item.contender_count>0);
        for s in w.snakes().filter(|s|s.alive) {
            boosting|=s.flags & flags::BOOSTING!=0;
            hunting|=s.flags & flags::HUNTING!=0;
        }
    }
    ENABLED.with(|enabled|enabled.set(false));
    assert_eq!(COUNT.with(Cell::get),0);
    assert!(bubbles && racing,"must exercise bubbles and race exports");
    assert!(boosting && hunting,"must exercise paid boosts and tactical planning");
}

#[test]
fn user_settings_faces_and_races_allocate_nothing() {
    let mut w=World::new(Config {width:3440.0,height:1440.0,density:30.0,trails:100.0,
        scale:185.0,speed:230.0,intelligence:100.0,seed:73,self_collisions:true,
        deadly_walls:true,rules:RuleSet::V2,..Config::default()}).unwrap();
    let mut ai=AiController::new();let mut bubbles=false;let mut racing=false;
    COUNT.with(|count|count.set(0));ENABLED.with(|enabled|enabled.set(true));
    for _ in 0..3000 {w.step(&mut ai);bubbles|=!w.bubbles().is_empty();racing|=w.items().any(|i|i.contender_count>0);}
    ENABLED.with(|enabled|enabled.set(false));
    assert_eq!(COUNT.with(Cell::get),0);assert!(bubbles && racing);
}
