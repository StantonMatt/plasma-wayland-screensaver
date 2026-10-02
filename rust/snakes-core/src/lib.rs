// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocation-free 30 Hz mechanics, with control policy supplied by the caller.
mod math;
mod rng;
mod world;
pub mod controller;
pub mod ffi;
pub use math::{ Point, normalize_angle, wrap_coordinate };
pub use rng::WorldRng;
pub use world::{ Config, ConfigError, DeathReason, FoodView, Segment, SnakeView, Stats, Traits, World, MAX_SNAKES, MAX_SEGMENTS, MAX_FOOD, STEP_SECONDS };
