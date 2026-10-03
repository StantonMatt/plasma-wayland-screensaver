// SPDX-License-Identifier: GPL-3.0-or-later
use std::ptr;
use snakes_core::{ Config, World };
use snakes_core::ffi::*;
#[test] fn ffi_round_trip_and_validation() {
    unsafe {
        assert_eq!(snakes_core_abi_version(), 2);
        let config = CoreConfig::from(Config {
            seed: 73,
            ..Config::default()
        });
        let mut handle = ptr::null_mut();
        assert_eq!(snakes_core_create(&config, &mut handle), OK);
        assert!(!handle.is_null());
        let expected = World::new(Config {
            seed: 73,
            ..Config::default()
        }).unwrap();
        let mut sizes = FrameSizes::default();
        assert_eq!(snakes_core_get_frame_sizes(handle, &mut sizes), OK);
        assert_eq!(sizes.snakes, 9);
        assert_eq!(sizes.food, 82);
        let mut snakes = vec![SnakeRecord::default();
        sizes.snakes as usize];
        let mut segments = vec![SegmentRecord::default();
        sizes.segments as usize];
        let mut food = vec![FoodRecord::default();
        sizes.food as usize];
        let mut info = FrameInfo {
            tick: 999,
            ..FrameInfo::default()
        };
        assert_eq!(snakes_core_export_frame(handle, snakes.as_mut_ptr(), snakes.len(), segments.as_mut_ptr(), 0, food.as_mut_ptr(), food.len(), &mut info), BUFFER_TOO_SMALL);
        assert_eq!(info.tick, 999);
        assert_eq!(snakes[0].generation, 0);
        assert_eq!(snakes_core_export_frame(handle, snakes.as_mut_ptr(), snakes.len(), segments.as_mut_ptr(), segments.len(), food.as_mut_ptr(), food.len(), &mut info), OK);
        assert_eq!(info.tick, 0);
        assert_eq!(info.world_width, 1280.0);
        assert_eq!(snakes[0].generation, 1);
        for (i, s) in expected.snakes().enumerate() {
            assert_eq!(snakes[i].segment_count as usize, s.segments.len());
            assert_eq!(snakes[i].radius, s.radius);
            for (j, seg) in s.segments.iter().enumerate() {
                let got = segments[snakes[i].segment_offset as usize+j];
                assert_eq!(got.x, seg.current.x as f32);
                assert_eq!(got.previous_y, seg.previous.y as f32);
            }
        }
        for (got, f) in food.iter().zip(expected.foods()) {
            assert_eq!(got.id, f.id);
            assert_eq!(got.x, f.position.x as f32);
            assert_eq!(got.size, f.size as f32);
        }
        let input = SteeringInput {
            id: 0,
            generation: 1,
            desired_angle: 1.5,
            rush: 0.2
        };
        assert_eq!(snakes_core_set_steering(handle, &input, 1), OK);
        let invalid = SteeringInput {
            desired_angle: f64::NAN,
            ..input
        };
        assert_eq!(snakes_core_set_steering(handle, &invalid, 1), INVALID_ARGUMENT);
        assert_eq!(snakes_core_set_steering(handle, &input, 15), INVALID_ARGUMENT);
        let duplicate = [input, input];
        assert_eq!(snakes_core_set_steering(handle, duplicate.as_ptr(), 2), INVALID_ARGUMENT);
        assert_eq!(snakes_core_step(handle, 1), OK);
        assert_eq!(snakes_core_export_frame(handle, snakes.as_mut_ptr(), snakes.len(), segments.as_mut_ptr(), segments.len(), food.as_mut_ptr(), food.len(), &mut info), OK);
        assert_eq!(snakes[0].desired_angle, 1.5);
        assert_eq!(info.tick, 1);
        assert_eq!(snakes_core_resize(handle, 2560.0, 1080.0), OK);
        assert_eq!(snakes_core_resize(handle, f64::INFINITY, 100.0), INVALID_ARGUMENT);
        assert_eq!(snakes_core_export_frame(handle, snakes.as_mut_ptr(), snakes.len(), segments.as_mut_ptr(), segments.len(), food.as_mut_ptr(), food.len(), &mut info), OK);
        assert_eq!(info.geometry_generation, 2);
        assert_eq!(info.world_height, 1080.0);
        let invalid_config = CoreConfig {
            deadly_walls: 2,
            ..config
        };
        assert_eq!(snakes_core_reconfigure(handle, &invalid_config), INVALID_ARGUMENT);
        let mut stats = CoreStats::default();
        assert_eq!(snakes_core_stats(handle, &mut stats), OK);
        assert_eq!(stats.alive, 9);
        assert_eq!(snakes_core_get_frame_sizes(handle, &mut sizes), OK);
        assert_eq!(stats.food, sizes.food);
        assert_eq!(snakes_core_step(handle, 1_000_001), INVALID_ARGUMENT);
        assert_eq!(snakes_core_set_steering(handle, ptr::null(), 0), OK);
        assert_eq!(snakes_core_stats(ptr::null(), &mut stats), INVALID_ARGUMENT);
        assert_eq!(snakes_core_get_frame_sizes(handle, ptr::null_mut()), INVALID_ARGUMENT);
        assert_eq!(snakes_core_export_frame(handle, ptr::null_mut(), snakes.len(), segments.as_mut_ptr(), segments.len(), food.as_mut_ptr(), food.len(), &mut info), INVALID_ARGUMENT);
        snakes_core_destroy(handle);
        snakes_core_destroy(ptr::null_mut());
        let mut invalid_handle = ptr::dangling_mut();
        assert_eq!(snakes_core_create(&invalid_config, &mut invalid_handle), INVALID_ARGUMENT);
        assert!(invalid_handle.is_null());
    }
}
#[test] fn ffi_layout_sizes_and_offsets() {
    assert_eq!(std::mem::size_of::<CoreConfig>(), 80);
    assert_eq!(std::mem::size_of::<SnakeRecord>(), 56);
    assert_eq!(std::mem::size_of::<FoodRecord>(), 48);
    assert_eq!(std::mem::offset_of!(FoodRecord, color_index), 36);
}

#[test]
fn output_records_accept_uninitialized_storage() {
    use std::mem::MaybeUninit;
    unsafe {
        let config = CoreConfig::from(Config::default());
        let mut handle = MaybeUninit::uninit();
        assert_eq!(snakes_core_create(&config, handle.as_mut_ptr()), OK);
        let handle = handle.assume_init();
        let mut sizes = MaybeUninit::<FrameSizes>::uninit();
        assert_eq!(snakes_core_get_frame_sizes(handle, sizes.as_mut_ptr()), OK);
        let sizes = sizes.assume_init();
        let mut snakes = vec![MaybeUninit::<SnakeRecord>::uninit(); sizes.snakes as usize];
        let mut segments = vec![MaybeUninit::<SegmentRecord>::uninit(); sizes.segments as usize];
        let mut food = vec![MaybeUninit::<FoodRecord>::uninit(); sizes.food as usize];
        let mut info = MaybeUninit::<FrameInfo>::uninit();
        assert_eq!(snakes_core_export_frame(handle, snakes.as_mut_ptr().cast(), snakes.len(),
            segments.as_mut_ptr().cast(), segments.len(), food.as_mut_ptr().cast(), food.len(), info.as_mut_ptr()), OK);
        assert_eq!(info.assume_init().tick, 0);
        assert!(snakes.iter().all(|s| s.assume_init().generation == 1));
        assert!(segments.iter().all(|s| s.assume_init().x.is_finite()));
        assert!(food.iter().all(|f| f.assume_init().id != 0));
        // Script straight movement until a wall strike produces an event.
        let input = SteeringInput { id: 0, generation: 0, desired_angle: 0.0, rush: 0.0 };
        assert_eq!(snakes_core_set_steering(handle, &input, 1), OK);
        let mut sizes = FrameSizes::default();
        for _ in 0..1000 {
            assert_eq!(snakes_core_step(handle, 1), OK);
            assert_eq!(snakes_core_get_frame_sizes(handle, &mut sizes), OK);
            if sizes.events > 0 { break; }
        }
        assert!(sizes.events > 0);
        let mut events = vec![MaybeUninit::<EventRecord>::uninit(); sizes.events as usize];
        assert_eq!(snakes_core_export_extras(handle, ptr::null_mut(), 0,
            events.as_mut_ptr().cast(), events.len()), OK);
        assert!(events.iter().all(|e| e.assume_init().tick > 0));
        let mut stats = MaybeUninit::<CoreStats>::uninit();
        assert_eq!(snakes_core_stats(handle, stats.as_mut_ptr()), OK);
        assert!(stats.assume_init().alive > 0);
        let mut debug = MaybeUninit::<AiDebugRecord>::uninit();
        assert_eq!(snakes_core_ai_debug(handle, 0, debug.as_mut_ptr()), OK);
        assert_eq!(debug.assume_init().id, 0);
        snakes_core_destroy(handle);
    }
}

#[test]
fn items_export_round_trip_failure_atomicity_and_disable() {
    use snakes_core::{RuleSet,MAX_ITEMS,MAX_EVENTS};
    let cfg=Config {rules:RuleSet::V2,deadly_walls:false,..Config::default()};
    let mut expected=World::new(cfg).unwrap();let mut ai=snakes_core::ai::AiController::new();
    unsafe {
        let mut handle=ptr::null_mut();assert_eq!(snakes_core_create(&CoreConfig::from(cfg),&mut handle),OK);
        let mut sizes=FrameSizes::default();
        for _ in 0..900 {
            expected.step(&mut ai);assert_eq!(snakes_core_step(handle,1),OK);
            assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);
            if sizes.items>0 {break;}
        }
        assert_eq!(sizes.items as usize,expected.items().len());assert!(sizes.items>0);
        let mut items=[ItemRecord::default();MAX_ITEMS];let mut events=[EventRecord::default();MAX_EVENTS];
        items[0].id=u64::MAX;events[0].tick=u64::MAX;
        assert_eq!(snakes_core_export_extras(handle,items.as_mut_ptr(),0,events.as_mut_ptr(),events.len()),BUFFER_TOO_SMALL);
        assert_eq!(items[0].id,u64::MAX);assert_eq!(events[0].tick,u64::MAX);
        assert_eq!(snakes_core_export_extras(handle,items.as_mut_ptr(),items.len(),events.as_mut_ptr(),events.len()),OK);
        for (record,item) in items.iter().zip(expected.items()) {
            assert_eq!(record.id,item.id);assert_eq!(record.kind,item.kind as u8);
            assert_eq!(record.x,item.position.x as f32);assert_eq!(snakes_core_item_radius(handle),item.radius);
            assert_eq!(record.age_ticks,item.age_ticks);assert_eq!(record.life_ticks,item.life_ticks);
        }
        assert_eq!(snakes_core_reconfigure(handle,&CoreConfig::from(Config{power_ups:false,..cfg})),OK);
        assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);assert_eq!(sizes.items,0);
        snakes_core_destroy(handle);
    }
}
