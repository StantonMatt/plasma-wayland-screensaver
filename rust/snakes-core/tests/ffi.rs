// SPDX-License-Identifier: GPL-3.0-or-later
use std::ptr;
use snakes_core::{ Config, World };
use snakes_core::ffi::*;
#[test] fn ffi_round_trip_and_validation() {
    unsafe {
        assert_eq!(snakes_core_abi_version(), 1);
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
    assert_eq!(std::mem::size_of::<CoreConfig>(), 72);
    assert_eq!(std::mem::size_of::<SnakeRecord>(), 48);
    assert_eq!(std::mem::size_of::<FoodRecord>(), 40);
    assert_eq!(std::mem::offset_of!(FoodRecord, color_index), 36);
}
