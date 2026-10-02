// SPDX-License-Identifier: GPL-3.0-or-later
include!("../src/lib.rs");
#[test]
fn exact_consumption_preserves_locked_owner_during_simultaneous_feeding() {
    let mut w=World::diagnostic_arena(Config {width:1600.0,height:1000.0,density:0.0,..Config::default()},
        &[(Point{x:400.0,y:400.0},0.0,24,0.6),(Point{x:411.0,y:400.0},0.0,24,0.6)],
        &[Point{x:404.0,y:400.0},Point{x:395.0,y:400.0}]).unwrap();
    // Particle 1 is closer to head 0, but belongs to head 1. Both eat this
    // step; score deltas and nearest pre-step heads cannot identify owners.
    w.food[0].owner=1;
    let mut straight=controller::ScriptedController::new(|_,s:SnakeView<'_>|controller::Steering {desired_angle:s.angle,rush:0.0});
    w.step(&mut straight);
    let records:Vec<_>=w.consumption_events().collect();
    assert_eq!(records.len(),2);
    assert!(records.iter().any(|e|e.0==1 && e.1==1 && e.2==1));
    assert!(records.iter().any(|e|e.0==2 && e.1==0 && e.2==1));
    // The other particle is on the opposite side and eaten by slot 0.
    // The original geometry deliberately keeps both heads in both capture areas.
    assert_eq!(records.iter().map(|e|e.4).sum::<f64>(),2.0);
    assert_eq!(w.diagnostic_snapshot().consumption_events().count(),2);
    w.step(&mut straight);
    assert_eq!(w.consumption_events().count(),0,"records must reset each step");
}

#[test]
fn disappearance_without_consumption_is_not_recorded() {
    let mut w=World::diagnostic_arena(Config {density:0.0,..Config::default()},
        &[(Point{x:400.0,y:400.0},0.0,24,0.6)],&[Point{x:800.0,y:700.0}]).unwrap();
    w.food[0].life=0.001;
    let mut c=controller::BaselineController;
    w.step(&mut c);
    assert!(w.foods().all(|f|f.id!=1));
    assert_eq!(w.consumption_events().count(),0);
}
