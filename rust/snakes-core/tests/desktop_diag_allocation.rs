// SPDX-License-Identifier: GPL-3.0-or-later
//! Public V2 AI path, including its first decision and emergency scratch.
use snakes_core::{ai::AiController,Config,RuleSet,World};
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
fn real_desktop_first_tick_and_54000_ticks_allocate_nothing() {
    let mut w=World::new(Config {width:3440.0,height:1440.0,density:80.0,trails:100.0,
        scale:200.0,speed:300.0,intelligence:100.0,seed:1,self_collisions:true,
        deadly_walls:true,rules:RuleSet::V2,palette_size:6,..Config::default()}).unwrap();
    w.resize(5360.0,1440.0).unwrap();w.resize(7920.0,1440.0).unwrap();
    let mut ai=AiController::new();
    COUNT.with(|count|count.set(0));ENABLED.with(|enabled|enabled.set(true));
    for _ in 0..54000 {w.step(&mut ai);}
    ENABLED.with(|enabled|enabled.set(false));
    assert_eq!(COUNT.with(Cell::get),0);
}

#[path="../examples/support/long_fixtures.rs"]
mod long_fixtures;
#[test]
fn giant_6000_first_tick_and_54000_ticks_allocate_nothing() {
    let mut w=World::new(Config {width:7920.0,height:1440.0,density:80.0,trails:100.0,
        scale:200.0,speed:300.0,intelligence:100.0,seed:1,self_collisions:true,
        deadly_walls:true,rules:RuleSet::V2,palette_size:6,..Config::default()}).unwrap();
    let (points,angle)=long_fixtures::spiral(&w,0,6000);
    w.diagnostic_body(0,&points,angle).unwrap();let mut ai=AiController::new();
    let mut retained_corpses=0;
    COUNT.with(|count|count.set(0));ENABLED.with(|enabled|enabled.set(true));
    for _ in 0..54000 {
        // Keep the giant workload present after death/sever/paid boost. The
        // fixture mutation uses the existing storage and is counted too.
        let s=w.snake(0).unwrap();
        if !s.alive && s.segments.len()>=6000 {retained_corpses+=1;}
        if !s.alive || s.segments.len()<6000 {w.diagnostic_body(0,&points,angle).unwrap();}
        let s=w.snake(0).unwrap();
        assert!(s.alive && s.segments.len()>=6000);
        w.step(&mut ai);
    }
    ENABLED.with(|enabled|enabled.set(false));
    assert_eq!(COUNT.with(Cell::get),0);
    assert!(retained_corpses>0,"exercise dead giants whose full bodies are retained");
}

#[test]
fn giant_spirals_fit_all_desktop_seeds() {
    for seed in [1,73,991] {
        let w=World::new(Config {width:7920.0,height:1440.0,scale:200.0,
            seed,rules:RuleSet::V2,..Config::default()}).unwrap();
        for n in [1000,2000,4000,6000] {
            let (points,_)=long_fixtures::spiral(&w,0,n);
            assert_eq!(points.len(),n);
            assert!(points.iter().all(|p|p.x>0.0 && p.x<7920.0 && p.y>0.0 && p.y<1440.0));
        }
    }
}
