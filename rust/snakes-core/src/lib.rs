// SPDX-License-Identifier: GPL-3.0-or-later
// Allocation-free 30 Hz mechanics, with control policy supplied by the caller.
mod math;
mod rng;
mod render;
pub mod shape;
mod world;
pub mod controller;
pub mod ai;
pub mod ffi;
pub use math::{ Point, normalize_angle, wrap_coordinate };
pub use rng::WorldRng;
pub use world::effects;
pub use world::events::EventStats;
pub use world::{Inventory, INVENTORY_SLOTS, INVENTORY_WINDUP_TICKS,  Mood, Glyph, FaceState, Bubble, Bulge, WorldEventState, MAX_BUBBLES, MAX_CONTENDERS, Item, MAX_ITEMS, MAX_CAPSULES, Config, ConfigError, RuleSet, FoodKind, EventKind, FrameEvent, flags, event_flags, MAX_EVENTS, CollisionEvent, DeathReason, FoodView, Segment, SnakeView, Stats, Traits, World, MAX_SNAKES, MAX_SEGMENTS, MAX_FOOD, STEP_SECONDS };

#[cfg(feature = "parity")]
pub use world::parity;
