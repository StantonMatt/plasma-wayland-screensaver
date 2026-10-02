// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::{ Config, SnakeView, World };
use snakes_core::controller::{ RecordedController, RecordedInput, ScriptedController, Steering };
#[test] fn recorded_table_matches_function_and_generation_filter() {
    let config = Config {
        seed: 73,
        ..Config::default()
    };
    let mut recorded_world = World::new(config).unwrap();
    let mut function_world = World::new(config).unwrap();
    let inputs = [RecordedInput {
        tick: 0,
        snake_id: 0,
        generation: 1,
        steering: Steering {
            desired_angle: 1.5,
            rush: 0.2
        }
    }, RecordedInput {
        tick: 4,
        snake_id: 1,
        generation: 999,
        steering: Steering {
            desired_angle: 2.0,
            rush: 0.3
        }
    }, RecordedInput {
        tick: 7,
        snake_id: 0,
        generation: 0,
        steering: Steering {
            desired_angle: -1.0,
            rush: 0.0
        }
    }];
    let mut table = RecordedController::new(&inputs).unwrap();
    let mut function = ScriptedController::new(|tick, s: SnakeView<'_>| {
        inputs.iter().filter(|i|i.tick==tick && i.snake_id==s.id && (i.generation==0 || i.generation==s.generation)).last().map(|i|i.steering).unwrap_or(Steering {
            desired_angle: s.angle,
            rush: 0.0
        })
    });
    recorded_world.step_n(&mut table, 10);
    function_world.step_n(&mut function, 10);
    assert_eq!(recorded_world.rng_state(), function_world.rng_state());
    for (a, b) in recorded_world.snakes().zip(function_world.snakes()) {
        assert_eq!(a.segments, b.segments);
        assert_eq!(a.angle, b.angle);
        assert_eq!(a.desired_angle, b.desired_angle);
    }
    let invalid = [inputs[2], inputs[0]];
    assert!(RecordedController::new(&invalid).is_none());
}
