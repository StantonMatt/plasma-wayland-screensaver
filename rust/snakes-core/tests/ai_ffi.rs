// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::{Config, ffi::*};
#[test]
fn default_ai_debug_and_scripted_switch_preserve_the_abi() {
    unsafe {
        let cfg=CoreConfig::from(Config {self_collisions:true,..Config::default()});
        let mut h=std::ptr::null_mut();
        assert_eq!(snakes_core_create(&cfg,&mut h),OK);
        let mut d=AiDebugRecord::default();
        assert_eq!(snakes_core_ai_debug(h,0,&mut d),OK);
        assert_eq!(d.path_count,0);
        assert_eq!(snakes_core_step(h,1),OK);
        assert_eq!(snakes_core_ai_debug(h,0,&mut d),OK);
        assert!(d.path_count>0 && d.path_count<=16 && d.target_count<=5);
        assert!(d.path[..d.path_count as usize].iter().all(|p|p.x.is_finite() && p.y.is_finite()));
        let saved=d;
        assert_eq!(snakes_core_ai_debug(h,99,&mut d),INVALID_ARGUMENT);
        assert_eq!(d.path_count,saved.path_count);
        assert_eq!(snakes_core_ai_debug(h,0,std::ptr::null_mut()),INVALID_ARGUMENT);
        let input=SteeringInput {id:0,generation:0,desired_angle:0.4,rush:0.0, actions:0,reserved:0};
        assert_eq!(snakes_core_set_steering(h,&input,1),OK);
        assert_eq!(snakes_core_step(h,2),OK);
        assert_eq!(snakes_core_ai_debug(h,0,&mut d),OK);
        assert_eq!(d.path_count,0);assert_eq!(d.target_count,0);
        assert_eq!(snakes_core_set_steering(h,std::ptr::null(),0),OK);
        assert_eq!(snakes_core_step(h,1),OK);
        assert_eq!(snakes_core_ai_debug(h,0,&mut d),OK);
        assert!(d.path_count>0);
        snakes_core_destroy(h);
    }
    assert_eq!(std::mem::size_of::<AiDebugRecord>(),200);
    assert_eq!(std::mem::offset_of!(AiDebugRecord,path),72);
    assert_eq!(snakes_core_abi_version(),3);
}
