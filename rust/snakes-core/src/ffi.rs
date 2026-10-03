// SPDX-License-Identifier: GPL-3.0-or-later
//! C ABI v2. Handles and buffers must be valid, aligned, nonoverlapping memory
//! and calls on one handle must be serialized. Nulls, invalid numeric inputs,
//! flags, IDs and insufficient capacities return status codes, without panic.
use std::{ mem::{ align_of, size_of }, ptr };
use crate::{ Config, World, MAX_SNAKES };
use crate::controller::{ Controller, Steering };
pub const OK: i32 = 0;
pub const INVALID_ARGUMENT: i32 = 1;
pub const BUFFER_TOO_SMALL: i32 = 2;
pub const ABI_VERSION: u32 = 2;
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
    pub rule_set: u32,
    pub reserved: u32,
}
impl CoreConfig {
    fn checked(self) -> Option<Config> {
        if self.self_collisions>1 || self.deadly_walls>1 || self.rule_set>2 || self.reserved!=0 {
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
            deadly_walls: self.deadly_walls!=0,
            rules: if self.rule_set == 1 { crate::RuleSet::Classic } else { crate::RuleSet::V2 }
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
            deadly_walls: c.deadly_walls as u32,
            rule_set: if c.rules == crate::RuleSet::Classic { 1 } else { 2 },
            reserved: 0
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
    pub flags: u32,
    pub effect_ticks: u16,
    pub effect_kind: u8,
    pub boost_ticks: u8,
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
    pub color_index: u32,
    pub kind: u8,
    pub life_fraction: u8,
    pub reserved: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemRecord {
    pub id: u64, pub x: f32, pub y: f32,
    pub kind: u8, pub reserved_byte: u8,
    pub age_ticks: u16, pub life_ticks: u16, pub reserved: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct EventRecord {
    pub tick: u64, pub x: f32, pub y: f32,
    pub snake_id: u32, pub other_snake_id: u32, pub color_index: u32,
    pub kind: u8, pub reserved: [u8; 3],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSizes {
    pub snakes: u32,
    pub segments: u32,
    pub food: u32,
    pub items: u32,
    pub events: u32,
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
    fn intent_flags(&self, id: u32) -> Option<u32> {
        if self.scripted { Some(0) } else { self.ai.intent_flags(id) }
    }
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
            segments: w.exported_segment_count() as u32,
            food: stats.food,
            items: 0,
            events: w.frame_events().len() as u32,
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
    if snake_capacity<w.snake_count() || segment_capacity<w.exported_segment_count() || food_capacity<stats.food as usize {
        return BUFFER_TOO_SMALL;
    }
    if !buffer(snakes, snake_capacity, w.snake_count()) || !buffer(segments, segment_capacity, w.exported_segment_count()) || !buffer(food, food_capacity, stats.food as usize) {
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
                segment_count: s.segments.len() as u32,
                flags: s.flags, effect_ticks: s.effect_ticks,
                effect_kind: s.effect_kind, boost_ticks: s.boost_ticks
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
                color_index: f.color_index,
                kind: f.kind as u8, life_fraction: f.life_fraction, reserved: 0
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
/// Exports ABI v2's bounded event ring and reserved item array (currently empty).
/// Failure leaves all outputs untouched. Events belong to the latest tick only;
/// read/export does not consume them. Excess events evict the oldest entry.
/// # Safety
/// Handle is live/readable; output arrays are aligned writable memory of their
/// declared capacities, disjoint from one another and the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_export_extras(handle: *const WorldHandle,
    items: *mut ItemRecord, item_capacity: usize, events: *mut EventRecord, event_capacity: usize) -> i32 {
    if !valid(handle) { return INVALID_ARGUMENT; }
    let w = &unsafe { &*handle }.world;
    let count = w.frame_events().len();
    if event_capacity < count { return BUFFER_TOO_SMALL; }
    if !buffer(items, item_capacity, 0) || !buffer(events, event_capacity, count) { return INVALID_ARGUMENT; }
    for (i,e) in w.frame_events().enumerate() {
        unsafe { ptr::write(events.add(i), EventRecord { tick: e.tick,
            x: e.position.x as f32, y: e.position.y as f32, snake_id: e.snake_id,
            other_snake_id: e.other_snake_id, color_index: e.color_index,
            kind: e.kind as u8, reserved: [0; 3] }); }
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
    assert!(MAX_SNAKES==14); // SNAKES_CORE_MAX_SNAKES, used by fixed presentation history.
    assert!(crate::MAX_FOOD==480); // SNAKES_CORE_MAX_FOOD, used by history-only LOD replay.
    assert!(size_of::<CoreConfig>()==80);
    assert!(align_of::<CoreConfig>()==8);
    assert!(size_of::<SteeringInput>()==24);
    assert!(size_of::<SnakeRecord>()==56);
    assert!(size_of::<SegmentRecord>()==16);
    assert!(size_of::<FoodRecord>()==48);
    assert!(size_of::<FrameSizes>()==24);
    assert!(size_of::<FrameInfo>()==40);
    assert!(size_of::<CoreStats>()==56);
    assert!(size_of::<ItemRecord>()==24);
    assert!(size_of::<EventRecord>()==32);
    assert!(std::mem::offset_of!(CoreConfig, rule_set)==72);
    assert!(std::mem::offset_of!(SnakeRecord, flags)==48);
    assert!(std::mem::offset_of!(SnakeRecord, effect_ticks)==52);
    assert!(std::mem::offset_of!(SnakeRecord, effect_kind)==54);
    assert!(std::mem::offset_of!(SnakeRecord, boost_ticks)==55);
    assert!(std::mem::offset_of!(FoodRecord, kind)==40);
    assert!(std::mem::offset_of!(FoodRecord, life_fraction)==41);
    assert!(std::mem::offset_of!(ItemRecord, age_ticks)==18);
    assert!(std::mem::offset_of!(ItemRecord, life_ticks)==20);
    assert!(std::mem::offset_of!(EventRecord, snake_id)==16);
    assert!(std::mem::offset_of!(EventRecord, kind)==28);
    assert!(std::mem::offset_of!(SnakeRecord, radius)==16);
    assert!(std::mem::offset_of!(SnakeRecord, segment_offset)==40);
    assert!(std::mem::offset_of!(FoodRecord, color_index)==36);
    assert!(std::mem::offset_of!(CoreStats, deaths)==16);
};

/// Optional developer-overlay record, unchanged in ABI v2.
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
/// capped, 2 safety capped, 4 no full-horizon safe route, 8 interception,
/// 16 reused plan, 32 trapped.
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

#[cfg(test)]
mod v2_tests {
    use super::*;
    use crate::{Point, RuleSet, flags};
    #[test]
    fn leader_hysteresis_round_trips_through_ffi() {
        unsafe {
            let cfg=CoreConfig::from(Config{rules:RuleSet::V2,..Config::default()});
            let mut handle=ptr::null_mut();assert_eq!(snakes_core_create(&cfg,&mut handle),OK);
            (*handle).world=World::diagnostic_arena(Config{width:4096.0,height:1440.0,density:0.0,
                rules:RuleSet::V2,deadly_walls:false,..Config::default()},
                &[(Point{x:1500.0,y:400.0},0.0,30,1.0),(Point{x:3000.0,y:1000.0},0.0,29,1.0)],&[]).unwrap();
            let input=SteeringInput{id:0,generation:0,desired_angle:0.0,rush:0.0};
            assert_eq!(snakes_core_set_steering(handle,&input,1),OK);
            let mut snakes=[SnakeRecord::default();MAX_SNAKES];
            let mut segments=vec![SegmentRecord::default();crate::MAX_SNAKES*crate::MAX_SEGMENTS];
            let mut food=[FoodRecord::default();crate::MAX_FOOD];let mut info=FrameInfo::default();
            for (length,expected) in [(29,0),(32,0),(33,1)] {
                let body:Vec<_>=(0..length).map(|j|Point{x:3000.0-7.08*j as f64,y:1000.0}).collect();
                (*handle).world.diagnostic_body(1,&body,0.0).unwrap();
                assert_eq!(snakes_core_step(handle,1),OK);
                assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),segments.as_mut_ptr(),segments.len(),food.as_mut_ptr(),food.len(),&mut info),OK);
                assert_eq!(snakes.iter().filter(|s|s.flags & flags::LEADER != 0).count(),1);
                assert_eq!(snakes[expected].flags & flags::LEADER,flags::LEADER);
            }
            let mut events=[EventRecord::default();crate::MAX_EVENTS];
            assert_eq!(snakes_core_export_extras(handle,ptr::null_mut(),0,events.as_mut_ptr(),events.len()),OK);
            assert_eq!(events[0].kind,crate::EventKind::Succession as u8);
            assert_eq!((events[0].snake_id,events[0].other_snake_id),(1,0));
            snakes_core_destroy(handle);
        }
    }
    #[test]
    fn default_rules_and_export_events_corpses_and_capacity_validation() {
        unsafe {
            let config=CoreConfig{rule_set:0,..CoreConfig::from(Config::default())};
            let mut handle=ptr::null_mut();assert_eq!(snakes_core_create(&config,&mut handle),OK);
            assert_eq!((*handle).world.config().rules,RuleSet::V2);
            (*handle).world=World::diagnostic_arena(Config{width:4096.0,height:1440.0,density:0.0,
                rules:RuleSet::V2,..Config::default()},
                &[(Point{x:2000.0,y:700.0},0.0,30,1.0)],&[]).unwrap();
            let input=SteeringInput{id:0,generation:1,desired_angle:0.0,rush:0.01};
            assert_eq!(snakes_core_set_steering(handle,&input,1),OK);
            assert_eq!(snakes_core_step(handle,1),OK);
            let mut sizes=FrameSizes::default();assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);
            assert_eq!(sizes.items,0);assert_eq!(sizes.events,1);
            let mut snakes=[SnakeRecord::default();MAX_SNAKES];
            let mut segments=vec![SegmentRecord::default();crate::MAX_SNAKES*crate::MAX_SEGMENTS];
            let mut food=[FoodRecord::default();crate::MAX_FOOD];
            let mut info=FrameInfo::default();
            assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),segments.as_mut_ptr(),segments.len(),food.as_mut_ptr(),food.len(),&mut info),OK);
            assert_eq!(snakes[0].flags,flags::BOOSTING|flags::LEADER);assert_eq!(snakes[0].boost_ticks,24);
            let mut events=[EventRecord{tick:999,..EventRecord::default()};crate::MAX_EVENTS];
            assert_eq!(snakes_core_export_extras(handle,ptr::null_mut(),0,events.as_mut_ptr(),0),BUFFER_TOO_SMALL);
            assert_eq!(events[0].tick,999);
            assert_eq!(snakes_core_export_extras(handle,ptr::null_mut(),0,ptr::null_mut(),32),INVALID_ARGUMENT);
            assert_eq!(snakes_core_export_extras(handle,ptr::null_mut(),0,events.as_mut_ptr(),events.len()),OK);
            assert_eq!(events[0].tick,1);assert_eq!(events[0].kind,crate::EventKind::Succession as u8);
            assert_eq!(events[0].snake_id,0);
            assert_eq!(snakes_core_step(handle,10),OK);
            assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),segments.as_mut_ptr(),segments.len(),food.as_mut_ptr(),food.len(),&mut info),OK);
            assert!(food.iter().any(|f|f.kind==crate::FoodKind::Pellet as u8 && f.life_fraction==255));
            // A scripted wall strike retains the full final body and exports an impact.
            let generation=(*handle).world.snake(0).unwrap().generation;
            (*handle).world.diagnostic_body(0,&[Point{x:4095.9,y:700.0};30],0.0).unwrap();
            let input=SteeringInput{id:0,generation,desired_angle:0.0,rush:0.0};
            assert_eq!(snakes_core_set_steering(handle,&input,1),OK);assert_eq!(snakes_core_step(handle,1),OK);
            assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);assert_eq!(sizes.segments,30);
            let mut stats=CoreStats::default();assert_eq!(snakes_core_stats(handle,&mut stats),OK);assert_eq!(stats.total_segments,0);
            assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),segments.as_mut_ptr(),29,food.as_mut_ptr(),food.len(),&mut info),BUFFER_TOO_SMALL);
            assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),segments.as_mut_ptr(),segments.len(),food.as_mut_ptr(),food.len(),&mut info),OK);
            assert_eq!(snakes[0].alive,0);assert_eq!(snakes[0].flags,flags::CORPSE);assert_eq!(snakes[0].segment_count,30);
            assert_eq!(snakes_core_export_extras(handle,ptr::null_mut(),0,events.as_mut_ptr(),events.len()),OK);
            assert_eq!(events[0].kind,crate::EventKind::Kill as u8);assert_eq!(events[0].tick,12);
            assert_eq!(events[0].other_snake_id,u32::MAX);assert!(events[0].x>4096.0);
            assert_eq!(snakes_core_step(handle,16),OK);assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);assert_eq!(sizes.segments,30);
            assert_eq!(snakes_core_step(handle,1),OK);assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);assert_eq!(sizes.segments,0);
            snakes_core_destroy(handle);
        }
    }
}

// Render API is additive to the simulation ABI v2.
pub use crate::render::{Color as RenderColor, Vertex as RenderVertex,
    Params as RenderParams, Output as RenderOutput, Renderer as RenderHandle};

/// Creates per-window scratch and visual history. No caller buffers retained.
#[unsafe(no_mangle)]
pub extern "C" fn snakes_core_render_create() -> *mut RenderHandle {
    Box::into_raw(Box::new(RenderHandle::new()))
}
/// # Safety
/// Handle is null or a live render handle, not currently in use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_destroy(renderer: *mut RenderHandle) {
    if !renderer.is_null() { drop(unsafe { Box::from_raw(renderer) }); }
}
/// # Safety
/// Handle is live and exclusively accessible.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_reset(renderer: *mut RenderHandle) -> i32 {
    if !valid(renderer) { return INVALID_ARGUMENT; }
    unsafe { &mut *renderer }.reset();
    OK
}
/// # Safety
/// Handle is live/exclusive. Records and parameters are readable, vertices and
/// output writable, aligned, valid for their counts and mutually disjoint.
/// Every vertex capacity slot must already hold an initialized RenderVertex,
/// including the unused tail. Zero-filled storage satisfies this requirement;
/// initialize only on allocation, then reuse it across builds. Other output
/// APIs use raw writes and accept uninitialized storage.
/// BUFFER_TOO_SMALL writes a prefix and required count; retry with more space.
/// History is updated once per frame, so a retry cannot duplicate effects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_build(
    renderer: *mut RenderHandle, info: *const FrameInfo,
    snakes: *const SnakeRecord, snake_count: usize,
    segments: *const SegmentRecord, segment_count: usize,
    food: *const FoodRecord, food_count: usize,
    events: *const EventRecord, event_count: usize,
    palette: *const RenderColor, palette_count: usize,
    params: *const RenderParams, vertices: *mut RenderVertex,
    vertex_capacity: usize, output: *mut RenderOutput,
) -> i32 {
    if !valid(renderer) || !valid(info) || !valid(params) || !valid(output)
        || snake_count>crate::MAX_SNAKES || segment_count>crate::MAX_SNAKES*crate::MAX_SEGMENTS
        || food_count>crate::MAX_FOOD || event_count>crate::MAX_EVENTS || palette_count>4096
        || !buffer(snakes.cast_mut(),snake_count,snake_count)
        || !buffer(segments.cast_mut(),segment_count,segment_count)
        || !buffer(food.cast_mut(),food_count,food_count)
        || !buffer(events.cast_mut(),event_count,event_count)
        || !buffer(palette.cast_mut(),palette_count,palette_count)
        || !buffer(vertices,vertex_capacity,vertex_capacity) { return INVALID_ARGUMENT; }
    let info=unsafe { &*info };let params=unsafe { &*params };
    if !crate::render::frame_valid(info,params) { return INVALID_ARGUMENT; }
    // A zero-length slice still requires a nonnull pointer in Rust.
    unsafe fn records<'a,T>(p:*const T,n:usize)->&'a [T] {
        if n==0 { &[] } else { unsafe { std::slice::from_raw_parts(p,n) } }
    }
    let snakes=unsafe { records(snakes,snake_count) };
    let mut seen=0u32;
    for s in snakes {
        if s.id as usize>=crate::MAX_SNAKES || s.segment_count as usize>crate::MAX_SEGMENTS
            || s.segment_offset as usize+s.segment_count as usize>segment_count
            || !crate::render::snake_valid(s)
            || seen&(1<<s.id)!=0 { return INVALID_ARGUMENT; }
        seen|=1<<s.id;
    }
    // Caller initializes the full capacity on allocation, not merely the
    // visible prefix: forming this slice borrows initialized Vertex values.
    let vertices=if vertex_capacity==0 { &mut [] } else { unsafe { std::slice::from_raw_parts_mut(vertices,vertex_capacity) } };
    let result=unsafe { &mut *renderer }.build(info,snakes,
        unsafe { records(segments,segment_count) },unsafe { records(food,food_count) },
        unsafe { records(events,event_count) },unsafe { records(palette,palette_count) },params,vertices);
    let status=if result.vertex_count>vertex_capacity { BUFFER_TOO_SMALL } else { OK };
    unsafe { output.write(result); }
    status
}
