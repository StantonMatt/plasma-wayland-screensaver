// SPDX-License-Identifier: GPL-3.0-or-later
//! C ABI v1. Handles and buffers must be valid, aligned, nonoverlapping memory
//! and calls on one handle must be serialized. Nulls, invalid numeric inputs,
//! flags, IDs and insufficient capacities return status codes, without panic.
use std::{ mem::{ align_of, size_of }, ptr };
use crate::{ Config, World, MAX_SNAKES };
use crate::controller::{ Controller, Steering };
pub const OK: i32 = 0;
pub const INVALID_ARGUMENT: i32 = 1;
pub const BUFFER_TOO_SMALL: i32 = 2;
pub const ABI_VERSION: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CoreConfig {
    pub width: f64,
    pub height: f64,
    pub density: f64,
    pub trails: f64,
    pub scale: f64,
    pub speed: f64,
    pub intelligence: f64,
    pub seed: i32,
    pub palette_size: u32,
    pub self_collisions: u32,
    pub deadly_walls: u32,
}
impl CoreConfig {
    fn checked(self) -> Option<Config> {
        if self.self_collisions>1 || self.deadly_walls>1 {
            return None;
        }
        let c = Config {
            width: self.width,
            height: self.height,
            seed: self.seed,
            density: self.density,
            trails: self.trails,
            scale: self.scale,
            speed: self.speed,
            intelligence: self.intelligence,
            palette_size: self.palette_size,
            self_collisions: self.self_collisions!=0,
            deadly_walls: self.deadly_walls!=0
        };
        c.validate().ok().map(|()|c)
    }
}
impl From<Config> for CoreConfig {
    fn from(c: Config) -> Self {
        Self {
            width: c.width,
            height: c.height,
            density: c.density,
            trails: c.trails,
            scale: c.scale,
            speed: c.speed,
            intelligence: c.intelligence,
            seed: c.seed,
            palette_size: c.palette_size,
            self_collisions: c.self_collisions as u32,
            deadly_walls: c.deadly_walls as u32
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SteeringInput {
    pub id: u32,
    pub generation: u32,
    pub desired_angle: f64,
    pub rush: f64
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SnakeRecord {
    pub id: u32,
    pub generation: u32,
    pub alive: u32,
    pub color_index: u32,
    pub radius: f64,
    pub angle: f64,
    pub desired_angle: f64,
    pub segment_offset: u32,
    pub segment_count: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SegmentRecord {
    pub x: f32,
    pub y: f32,
    pub previous_x: f32,
    pub previous_y: f32
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FoodRecord {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub phase: f32,
    pub attraction: f32,
    pub attraction_x: f32,
    pub attraction_y: f32,
    pub color_index: u32
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSizes {
    pub snakes: u32,
    pub segments: u32,
    pub food: u32,
    pub reserved: u32
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInfo {
    pub tick: u64,
    pub simulation_time: f64,
    pub world_width: f64,
    pub world_height: f64,
    pub geometry_generation: u64
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CoreStats {
    pub alive: u32,
    pub total_segments: u32,
    pub food: u32,
    pub reserved: u32,
    pub deaths: u64,
    pub wall_deaths: u64,
    pub head_deaths: u64,
    pub body_deaths: u64,
    pub self_deaths: u64
}
/// Opaque to C. No interior pointers or Rust layouts are exposed.
pub struct WorldHandle {
    world: World,
    inputs: [Option<SteeringInput>;
    MAX_SNAKES],
    scripted: bool,
    ai: crate::ai::AiController
}
struct InputController<'a> {
    inputs: &'a [Option<SteeringInput>;
    MAX_SNAKES],
    scripted: bool,
    ai: &'a mut crate::ai::AiController
}
impl Controller for InputController<'_> {
    fn steer(&mut self, w: &World, s: crate::SnakeView<'_>) -> Steering {
        if !self.scripted {
            return self.ai.steer(w, s);
        }
        if let Some(i) = self.inputs[s.id as usize] {
            if i.generation==0 || i.generation==s.generation {
                return Steering {
                    desired_angle: i.desired_angle,
                    rush: i.rush
                };
            }
        }
        Steering {
            desired_angle: s.angle,
            rush: 0.0
        }
    }
}
fn valid<T>(p: *const T) -> bool {
    !p.is_null() && (p as usize).is_multiple_of(align_of::<T>())
}
fn buffer<T>(p: *mut T, capacity: usize, needed: usize) -> bool {
    capacity>=needed && (needed==0 || valid(p)) && capacity<=isize::MAX as usize/size_of::<T>()
}
#[unsafe(no_mangle)]
pub extern "C" fn snakes_core_abi_version() -> u32 {
    ABI_VERSION
}
/// # Safety
/// `config` is readable and `output` writable, aligned, nonoverlapping memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_create(config: *const CoreConfig, output: *mut *mut WorldHandle) -> i32 {
    if !valid(config) || !valid(output) {
        return INVALID_ARGUMENT;
    }
    unsafe {
        ptr::write(output, ptr::null_mut());
    }
    let Some(config) = (unsafe {
        *config
    }).checked() else {
        return INVALID_ARGUMENT;
    };
    let Ok(world) = World::new(config) else {
        return INVALID_ARGUMENT;
    };
    let h = Box::new(WorldHandle {
        world,
        inputs: [None;
        MAX_SNAKES],
        scripted: false,
        ai: crate::ai::AiController::new()
    });
    unsafe {
        ptr::write(output, Box::into_raw(h));
    }
    OK
}
/// # Safety
/// A nonnull handle must be returned by create, not yet destroyed, and unused
/// by any concurrent call. Null is accepted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_destroy(handle: *mut WorldHandle) {
    if valid(handle) {
        unsafe {
            drop(Box::from_raw(handle));
        }
    }
}
/// # Safety
/// Handle is live and exclusively owned for this call; config is readable,
/// aligned memory disjoint from the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_reconfigure(handle: *mut WorldHandle, config: *const CoreConfig) -> i32 {
    if !valid(handle) || !valid(config) {
        return INVALID_ARGUMENT;
    }
    let Some(config) = (unsafe {
        *config
    }).checked() else {
        return INVALID_ARGUMENT;
    };
    let h = unsafe {
        &mut *handle
    };
    if h.world.reconfigure(config).is_err() {
        return INVALID_ARGUMENT;
    }
    OK
}
/// # Safety
/// Handle is live and exclusively owned for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_resize(handle: *mut WorldHandle, width: f64, height: f64) -> i32 {
    if !valid(handle) {
        return INVALID_ARGUMENT;
    }
    if unsafe {
        &mut *handle
    }.world.resize(width, height).is_err() {
        INVALID_ARGUMENT
    } else {
        OK
    }
}
/// Sets persistent per-slot controls. Generation zero matches any respawn;
/// omitted slots continue straight. An empty table restores the AI.
/// # Safety
/// Handle is live/exclusive; input is a readable aligned array of `length`
/// records, disjoint from the handle. Null input is allowed for length zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_set_steering(handle: *mut WorldHandle, input: *const SteeringInput, length: usize) -> i32 {
    if !valid(handle) || length>MAX_SNAKES || (length>0 && !valid(input)) {
        return INVALID_ARGUMENT;
    }
    let h = unsafe {
        &mut *handle
    };
    let mut inputs = [None;
    MAX_SNAKES];
    for j in 0..length {
        let i = unsafe {
            ptr::read(input.add(j))
        };
        if i.id as usize>=h.world.snake_count() || !(Steering {
            desired_angle: i.desired_angle,
            rush: i.rush
        }).is_valid() || inputs[i.id as usize].is_some() {
            return INVALID_ARGUMENT;
        }
        inputs[i.id as usize] = Some(i);
    }
    h.inputs = inputs;
    h.scripted = length>0;
    OK
}
/// # Safety
/// Handle is live and exclusively owned for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_step(handle: *mut WorldHandle, ticks: u32) -> i32 {
    if !valid(handle) || ticks>1_000_000 {
        return INVALID_ARGUMENT;
    }
    let h = unsafe {
        &mut *handle
    };
    let mut controller = InputController {
        inputs: &h.inputs,
        scripted: h.scripted,
        ai: &mut h.ai
    };
    h.world.step_n(&mut controller, ticks);
    OK
}
/// # Safety
/// Handle is live and readable; output is writable aligned memory disjoint
/// from the handle. Calls on the handle are serialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_get_frame_sizes(handle: *const WorldHandle, output: *mut FrameSizes) -> i32 {
    if !valid(handle) || !valid(output) {
        return INVALID_ARGUMENT;
    }
    let w = &unsafe {
        &*handle
    }.world;
    let stats = w.stats();
    unsafe {
        ptr::write(output, FrameSizes {
            snakes: w.snake_count() as u32,
            segments: stats.total_segments,
            food: stats.food,
            reserved: 0
        });
    }
    OK
}
/// Writes a compact snapshot into caller storage, with previous/current pairs
/// already corresponding after growth. On failure, no output is changed.
/// # Safety
/// Handle is live/readable. Arrays are writable aligned memory of the declared
/// capacities; info is writable. All outputs and the handle are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_export_frame(handle: *const WorldHandle, snakes: *mut SnakeRecord, snake_capacity: usize, segments: *mut SegmentRecord, segment_capacity: usize, food: *mut FoodRecord, food_capacity: usize, info: *mut FrameInfo) -> i32 {
    if !valid(handle) || !valid(info) {
        return INVALID_ARGUMENT;
    }
    let w = &unsafe {
        &*handle
    }.world;
    let stats = w.stats();
    if snake_capacity<w.snake_count() || segment_capacity<stats.total_segments as usize || food_capacity<stats.food as usize {
        return BUFFER_TOO_SMALL;
    }
    if !buffer(snakes, snake_capacity, w.snake_count()) || !buffer(segments, segment_capacity, stats.total_segments as usize) || !buffer(food, food_capacity, stats.food as usize) {
        return INVALID_ARGUMENT;
    }
    let mut offset = 0;
    for s in w.snakes() {
        unsafe {
            ptr::write(snakes.add(s.id as usize), SnakeRecord {
                id: s.id,
                generation: s.generation,
                alive: s.alive as u32,
                color_index: s.color_index,
                radius: s.radius,
                angle: s.angle,
                desired_angle: s.desired_angle,
                segment_offset: offset as u32,
                segment_count: s.segments.len() as u32
            });
        }
        for seg in s.segments {
            unsafe {
                ptr::write(segments.add(offset), SegmentRecord {
                    x: seg.current.x as f32,
                    y: seg.current.y as f32,
                    previous_x: seg.previous.x as f32,
                    previous_y: seg.previous.y as f32
                });
            }
            offset+=1;
        }
    }
    for (i, f) in w.foods().enumerate() {
        unsafe {
            ptr::write(food.add(i), FoodRecord {
                id: f.id,
                x: f.position.x as f32,
                y: f.position.y as f32,
                size: f.size as f32,
                phase: f.phase as f32,
                attraction: f.attraction as f32,
                attraction_x: f.attraction_target.x as f32,
                attraction_y: f.attraction_target.y as f32,
                color_index: f.color_index
            });
        }
    }
    unsafe {
        ptr::write(info, FrameInfo {
            tick: w.tick(),
            simulation_time: w.simulation_time(),
            world_width: w.config().width,
            world_height: w.config().height,
            geometry_generation: w.geometry_generation()
        });
    }
    OK
}
/// # Safety
/// Handle is live/readable; output is aligned writable memory disjoint from
/// the handle. Calls on the handle are serialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_stats(handle: *const WorldHandle, output: *mut CoreStats) -> i32 {
    if !valid(handle) || !valid(output) {
        return INVALID_ARGUMENT;
    }
    let s = unsafe {
        &*handle
    }.world.stats();
    unsafe {
        ptr::write(output, CoreStats {
            alive: s.alive,
            total_segments: s.total_segments,
            food: s.food,
            reserved: 0,
            deaths: s.deaths,
            wall_deaths: s.wall_deaths,
            head_deaths: s.head_deaths,
            body_deaths: s.body_deaths,
            self_deaths: s.self_deaths
        });
    }
    OK
}
const _: () =  {
    assert!(size_of::<CoreConfig>()==72);
    assert!(align_of::<CoreConfig>()==8);
    assert!(size_of::<SteeringInput>()==24);
    assert!(size_of::<SnakeRecord>()==48);
    assert!(size_of::<SegmentRecord>()==16);
    assert!(size_of::<FoodRecord>()==40);
    assert!(size_of::<FrameSizes>()==16);
    assert!(size_of::<FrameInfo>()==40);
    assert!(size_of::<CoreStats>()==56);
    assert!(std::mem::offset_of!(SnakeRecord, radius)==16);
    assert!(std::mem::offset_of!(SnakeRecord, segment_offset)==40);
    assert!(std::mem::offset_of!(FoodRecord, color_index)==36);
    assert!(std::mem::offset_of!(CoreStats, deaths)==16);
};

/// Developer-overlay record. No existing ABI record or version is changed.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AiDebugPoint { pub x: f32, pub y: f32 }
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AiDebugRecord {
    pub id: u32,
    pub generation: u32,
    pub target_count: u32,
    pub path_count: u32,
    pub flags: u32,
    pub reachable_cells: u32,
    pub safe_seconds: f64,
    pub target_food_ids: [u64; 5],
    pub path: [AiDebugPoint; 16],
}
/// Exports one slot's last AI decision. Scripted mode, dead slots, or a new
/// generation without a decision return a zero-count record. Flags: 1 area
/// capped, 2 safety capped, 4 no full-horizon safe route, 8 interception.
/// # Safety
/// Handle is live/readable and output is aligned writable memory disjoint
/// from the handle. Calls on a handle are serialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_ai_debug(handle: *const WorldHandle, id: u32, output: *mut AiDebugRecord) -> i32 {
    if !valid(handle) || !valid(output) {return INVALID_ARGUMENT;}
    let h=unsafe {&*handle};
    let Some(s)=h.world.snake(id as usize) else {return INVALID_ARGUMENT;};
    let mut record=AiDebugRecord {id,generation:s.generation,..AiDebugRecord::default()};
    let d=h.ai.debug(id as usize).unwrap();
    if !h.scripted && s.alive && d.generation==s.generation {
        record.target_count=d.target_count;record.path_count=d.path_count;
        record.flags=d.flags;record.reachable_cells=d.reachable_cells;
        record.safe_seconds=d.safe_seconds;record.target_food_ids=d.target_food_ids;
        for (p,q) in record.path.iter_mut().zip(d.path) {*p=AiDebugPoint {x:q.x as f32,y:q.y as f32};}
    }
    unsafe {ptr::write(output,record);}
    OK
}
const _: () = {
    assert!(size_of::<AiDebugPoint>()==8);
    assert!(size_of::<AiDebugRecord>()==200);
    assert!(align_of::<AiDebugRecord>()==8);
    assert!(std::mem::offset_of!(AiDebugRecord,target_food_ids)==32);
    assert!(std::mem::offset_of!(AiDebugRecord,path)==72);
};
