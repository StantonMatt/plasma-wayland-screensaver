// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{ Point, SnakeView, World, normalize_angle };
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Steering {
    pub desired_angle: f64,
    pub rush: f64
}
impl Steering {
    pub fn is_valid(self) -> bool {
        self.desired_angle.is_finite() && self.rush.is_finite() && (0.0..=1.0).contains(&self.rush)
    }
}
pub trait Controller {
    fn steer(&mut self, world: &World, snake: SnakeView<'_>) -> Steering;
}
/// Deterministic function of tick and snake. The closure may keep its own
/// randomness, which never consumes the world RNG.
pub struct ScriptedController<F> {
    script: F
}
impl<F> ScriptedController<F> {
    pub fn new(script: F) -> Self {
        Self {
            script
        }
    }
}
impl<F: FnMut(u64, SnakeView<'_>) -> Steering> Controller for ScriptedController<F> {
    fn steer(&mut self, world: &World, snake: SnakeView<'_>) -> Steering {
        (self.script)(world.tick(), snake)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RecordedInput {
    pub tick: u64,
    pub snake_id: u32,
    pub generation: u32,
    pub steering: Steering
}
/// Borrowed table, sorted by tick. Generation zero matches any generation;
/// unspecified snakes continue straight. Duplicate entries use the last one.
pub struct RecordedController<'a> {
    inputs: &'a [RecordedInput],
}
impl<'a> RecordedController<'a> {
    pub fn new(inputs: &'a [RecordedInput]) -> Option<Self> {
        if inputs.windows(2).any(|w| w[0].tick > w[1].tick)
            || inputs.iter().any(|i| !i.steering.is_valid()) {
            None
        } else {
            Some(Self { inputs })
        }
    }
}
impl Controller for RecordedController<'_> {
    fn steer(&mut self, world: &World, snake: SnakeView<'_>) -> Steering {
        let begin = self.inputs.partition_point(|i| i.tick < world.tick());
        let mut result = Steering {
            desired_angle: snake.angle,
            rush: 0.0
        };
        for input in &self.inputs[begin..] {
            if input.tick!=world.tick() {
                break;
            }
            if input.snake_id==snake.id && (input.generation==0 || input.generation==snake.generation) {
                result = input.steering;
            }
        }
        result
    }
}
/// TEMPORARY policy for standalone mechanics benchmarks. Nearest food plus
/// simple wall avoidance; no planning, body safety, reservations or world RNG.
#[derive(Default)]
pub struct BaselineController;
impl Controller for BaselineController {
    fn steer(&mut self, world: &World, snake: SnakeView<'_>) -> Steering {
        let head = snake.segments[0].current;
        let config = world.config();
        let mut nearest = f64::MAX;
        let mut target = None;
        for f in world.foods() {
            let d = world.distance_squared(head, f.position);
            if d<nearest {
                nearest = d;
                target = Some(f.position);
            }
        }
        let mut angle = snake.angle;
        if let Some(p) = target {
            let delta = |from: f64,
            to: f64,
            extent: f64| {
                let mut d = to-from;
                if !config.deadly_walls {
                    if d>extent/2.0 {
                        d-=extent;
                    } else if d< -extent/2.0 {
                        d+=extent;
                    }
                }
                d
            };
            angle = delta(head.y, p.y, config.height).atan2(delta(head.x, p.x, config.width));
        }
        if config.deadly_walls {
            let margin = (snake.radius*12.0).max(100.0);
            let mut direction = Point::default();
            if head.x<margin {
                direction.x+=1.0;
            }
            if head.x>config.width-margin {
                direction.x-=1.0;
            }
            if head.y<margin {
                direction.y+=1.0;
            }
            if head.y>config.height-margin {
                direction.y-=1.0;
            }
            if direction.x!=0.0 || direction.y!=0.0 {
                angle = direction.y.atan2(direction.x);
            }
        }
        Steering {
            desired_angle: normalize_angle(angle),
            rush: 0.0
        }
    }
}
