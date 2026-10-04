// SPDX-License-Identifier: GPL-3.0-or-later
//! C ABI v3. Handles and buffers must be valid, aligned, nonoverlapping memory
//! and calls on one handle must be serialized. Nulls, invalid numeric inputs,
//! flags, IDs and insufficient capacities return status codes, without panic.
use std::{ mem::{ align_of, size_of }, ptr };
use crate::{ Config, World, MAX_SNAKES };
use crate::controller::{ Controller, Steering };
pub const OK: i32 = 0;
pub const INVALID_ARGUMENT: i32 = 1;
pub const BUFFER_TOO_SMALL: i32 = 2;
pub const ABI_VERSION: u32 = 3;
pub const WORLD_EVENTS_OFF:u32 = 0x4000_0000;
pub const POWER_UPS_OFF: u32 = 0x8000_0000;
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
        if self.self_collisions>1 || self.deadly_walls>1 || self.rule_set>2 || self.reserved & !(POWER_UPS_OFF|WORLD_EVENTS_OFF)!=0 {
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
            power_ups: self.reserved & POWER_UPS_OFF == 0,
            world_events:self.reserved & WORLD_EVENTS_OFF == 0,
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
            reserved: (if c.power_ups {0} else {POWER_UPS_OFF}) | (if c.world_events {0} else {WORLD_EVENTS_OFF}),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SteeringInput {
    pub id: u32,
    pub generation: u32,
    pub desired_angle: f64,
    pub rush: f64,
    pub actions:u32,pub reserved:u32,
}
/// Face flags bit 1 distinguishes authoritative Calm from absent legacy data.
pub const FACE_OBSERVED:u8=2;
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
    pub mood:u8,pub mood_intensity:u8,pub mood_age_ticks:u16,
    pub target_item:u8,pub face_flags:u8,pub jaw_ticks:u16,
    pub look_x:f32,pub look_y:f32,pub pupil_x:f32,pub pupil_y:f32,
    pub frozen_ticks:u16,pub dizzy_ticks:u16,pub bite_immunity_ticks:u16,pub stump_ticks:u16,
    pub thaw_immunity_ticks:u16,pub breath_ticks:u16,pub flip_grace_ticks:u16,pub happy_ticks:u16,
    pub grudge_snake_id:u32,pub grudge_ticks:u16,pub reserved:u16,pub grudge_generation:u32,
    pub flip_tick:u64,pub bulges:[crate::world::Bulge;2],
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
    pub ripe_tick:u64,pub motion_origin_x:f32,pub motion_origin_y:f32,
    pub motion_ticks:u16,pub captured_by:u16,pub food_flags:u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemRecord {
    pub id: u64, pub x: f32, pub y: f32,
    pub kind: u8, pub reserved_byte: u8,
    pub age_ticks: u16, pub life_ticks: u16, pub reserved: u16,
    pub pickable_from_tick:u64,pub leader_snake_id:u32,pub leader_eta:f32,
    pub landing_ticks:u16,pub contender_count:u8,pub state:u8,
    pub contender_ids:[u32;crate::world::MAX_CONTENDERS],pub contender_etas:[f32;crate::world::MAX_CONTENDERS],
    pub guard_snake_id:u32,pub radius:f32,pub captured_value:f32,pub charge_ticks:u16,
    pub reserved_v3:u16,pub owner_generation:u32,pub owner_snake_id:u32,pub reserved_owner:u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct EventRecord {
    pub tick: u64, pub x: f32, pub y: f32,
    pub snake_id: u32, pub other_snake_id: u32, pub color_index: u32,
    pub kind: u8, pub reserved: [u8; 3],
    pub cut_index:u16,pub duration_ticks:u16,pub generation:u32,pub other_generation:u32,
    pub value:f32,pub release_tick:u64,
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
    pub geometry_generation: u64,
    pub ambient:f32,pub bubble_count:u32,pub bubbles:[crate::world::Bubble;crate::world::MAX_BUBBLES],
    pub world_event:crate::world::WorldEventState,
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
    fn face_intent(&self,id:u32)->crate::controller::FaceIntent {if self.scripted {crate::controller::FaceIntent::default()} else {self.ai.face_intent(id)}}
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
        }).is_valid() || i.actions & !1!=0 || i.reserved!=0 || inputs[i.id as usize].is_some() {
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
            items: w.items().len() as u32,
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
                effect_kind: s.effect_kind, boost_ticks: s.boost_ticks,
                mood:s.face.mood as u8,mood_intensity:s.face.intensity,mood_age_ticks:s.face.age,
                target_item:w.items().position(|item|item.id==s.face.target_id).map_or(255,|i|i as u8),face_flags:(if s.face.guarding {1} else {0}) | if w.config().rules==crate::RuleSet::V2 {FACE_OBSERVED} else {0},jaw_ticks:s.face.jaw_ticks,
                look_x:s.face.look.x as f32,look_y:s.face.look.y as f32,pupil_x:s.face.pupil.x as f32,pupil_y:s.face.pupil.y as f32,
                frozen_ticks:s.face.frozen_ticks,dizzy_ticks:s.face.dizzy_ticks,bite_immunity_ticks:s.face.bite_immunity_ticks,
                stump_ticks:s.face.stump_ticks,thaw_immunity_ticks:s.face.thaw_immunity_ticks,breath_ticks:s.face.breath_ticks,
                flip_grace_ticks:s.face.flip_grace_ticks,happy_ticks:s.face.happy_ticks,grudge_snake_id:s.face.grudge_id,
                grudge_ticks:s.face.grudge_ticks,grudge_generation:s.face.grudge_generation,flip_tick:s.face.flip_tick,
                bulges:s.face.bulges,reserved:0
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
    for (i, (f, payload)) in w.foods().zip(w.food.iter()).enumerate() {
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
                kind: f.kind as u8, life_fraction: f.life_fraction, reserved: 0,
                ripe_tick:payload.ripe_tick,motion_origin_x:payload.motion_origin.x as f32,motion_origin_y:payload.motion_origin.y as f32,
                motion_ticks:f.motion_ticks,captured_by:payload.captured_by,food_flags:0
            });
        }
    }
    unsafe {
        ptr::write(info, FrameInfo {
            tick: w.tick(),
            simulation_time: w.simulation_time(),
            world_width: w.config().width,
            world_height: w.config().height,
            geometry_generation: w.geometry_generation(),ambient:if w.world_event.ambient>0.0 {w.world_event.ambient} else {1.0},
            bubble_count:w.bubbles().len() as u32,bubbles:std::array::from_fn(|i|w.bubbles().get(i).copied().unwrap_or_default()),
            world_event:w.world_event
        });
    }
    OK
}
/// Exports ABI v3's bounded event ring and live items atomically.
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
    if item_capacity < w.items().len() || event_capacity < count { return BUFFER_TOO_SMALL; }
    if !buffer(items, item_capacity, w.items().len()) || !buffer(events, event_capacity, count) { return INVALID_ARGUMENT; }
    for (i,item) in w.items().enumerate() {
        unsafe { ptr::write(items.add(i), ItemRecord {id:item.id,x:item.position.x as f32,y:item.position.y as f32,
            kind:item.kind as u8,reserved_byte:0,age_ticks:item.age_ticks,life_ticks:item.life_ticks,reserved:0,
            pickable_from_tick:item.pickable_from_tick,leader_snake_id:item.leader_snake_id,leader_eta:item.leader_eta,
            landing_ticks:item.pickable_from_tick.saturating_sub(w.tick()).min(u16::MAX as u64) as u16,
            contender_count:item.contender_count,state:0,contender_ids:item.contender_ids,contender_etas:item.contender_etas,
            guard_snake_id:item.guard_snake_id,radius:item.radius as f32,captured_value:item.captured_value,
            charge_ticks:item.charge_ticks,owner_generation:item.owner_generation,owner_snake_id:item.owner_snake_id,reserved_v3:0,reserved_owner:0}); }
    }
    for (i,e) in w.frame_events().enumerate() {
        unsafe { ptr::write(events.add(i), EventRecord { tick: e.tick,
            x: e.position.x as f32, y: e.position.y as f32, snake_id: e.snake_id,
            other_snake_id: e.other_snake_id, color_index: e.color_index,
            kind: e.kind as u8, reserved: [0; 3],cut_index:e.cut_index,duration_ticks:e.duration_ticks,
            generation:e.generation,other_generation:e.other_generation,value:e.value,release_tick:e.release_tick }); }
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
    assert!(size_of::<SteeringInput>()==32);
    assert!(size_of::<SnakeRecord>()==152);
    assert!(size_of::<SegmentRecord>()==16);
    assert!(size_of::<FoodRecord>()==72);
    assert!(size_of::<FrameSizes>()==24);
    assert!(size_of::<FrameInfo>()==128);
    assert!(size_of::<CoreStats>()==56);
    assert!(size_of::<ItemRecord>()==88);
    assert!(size_of::<EventRecord>()==56);
    assert!(size_of::<crate::world::Bubble>()==12);
    assert!(size_of::<crate::world::Bulge>()==16);
    assert!(size_of::<crate::world::WorldEventState>()==40);
    assert!(std::mem::offset_of!(SteeringInput,actions)==24);
    assert!(std::mem::offset_of!(SnakeRecord,mood)==56);
    assert!(std::mem::offset_of!(SnakeRecord,look_x)==64);
    assert!(std::mem::offset_of!(SnakeRecord,flip_tick)==112);
    assert!(std::mem::offset_of!(SnakeRecord,bulges)==120);
    assert!(std::mem::offset_of!(FoodRecord,ripe_tick)==48);
    assert!(std::mem::offset_of!(ItemRecord,pickable_from_tick)==24);
    assert!(std::mem::offset_of!(ItemRecord,contender_ids)==44);
    assert!(std::mem::offset_of!(EventRecord,cut_index)==32);
    assert!(std::mem::offset_of!(FrameInfo,bubbles)==48);
    assert!(std::mem::offset_of!(FrameInfo,world_event)==88);
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
    fn cold_future_food_payload_round_trips_without_expanding_ai_view() {
        assert!(size_of::<crate::FoodView>() <= 128);
        unsafe {
            let cfg=CoreConfig::from(Config::default());
            let mut handle=ptr::null_mut();
            assert_eq!(snakes_core_create(&cfg,&mut handle),OK);
            let payload=&mut (&mut (*handle).world.food)[0];
            payload.ripe_tick=90;payload.motion_origin=Point{x:12.5,y:30.25};
            payload.motion_ticks=17;payload.captured_by=4;
            let mut snakes=[SnakeRecord::default();MAX_SNAKES];
            let mut segments=vec![SegmentRecord::default();crate::MAX_SNAKES*crate::MAX_SEGMENTS];
            let mut food=[FoodRecord::default();crate::MAX_FOOD];
            let mut info=FrameInfo::default();
            assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),
                segments.as_mut_ptr(),segments.len(),food.as_mut_ptr(),food.len(),&mut info),OK);
            assert_eq!(food[0].ripe_tick,90);
            assert_eq!((food[0].motion_origin_x,food[0].motion_origin_y),(12.5,30.25));
            assert_eq!((food[0].motion_ticks,food[0].captured_by),(17,4));
            snakes_core_destroy(handle);
        }
    }
    #[test]
    fn leader_hysteresis_round_trips_through_ffi() {
        unsafe {
            let cfg=CoreConfig::from(Config{rules:RuleSet::V2,..Config::default()});
            let mut handle=ptr::null_mut();assert_eq!(snakes_core_create(&cfg,&mut handle),OK);
            (*handle).world=World::diagnostic_arena(Config{width:4096.0,height:1440.0,density:0.0,
                rules:RuleSet::V2,deadly_walls:false,..Config::default()},
                &[(Point{x:1500.0,y:400.0},0.0,30,1.0),(Point{x:3000.0,y:1000.0},0.0,29,1.0)],&[]).unwrap();
            let input=SteeringInput{id:0,generation:0,desired_angle:0.0,rush:0.0, actions:0,reserved:0};
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
            let input=SteeringInput{id:0,generation:1,desired_angle:0.0,rush:0.01, actions:0,reserved:0};
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
            let input=SteeringInput{id:0,generation,desired_angle:0.0,rush:0.0, actions:0,reserved:0};
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

// Shader geometry is additive; the classic entry point/layout is unchanged.
pub use crate::render::ShaderVertex as ShaderRenderVertex;
/// Same borrowed-buffer and initialized-capacity contract as render_build.
/// # Safety
/// All pointers must be aligned, initialized, nonoverlapping for their lengths;
/// calls on a renderer must be serialized. Output is the complete required count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_build_shader(
    renderer: *mut RenderHandle, info: *const FrameInfo,
    snakes: *const SnakeRecord, snake_count: usize,
    segments: *const SegmentRecord, segment_count: usize,
    food: *const FoodRecord, food_count: usize,
    events: *const EventRecord, event_count: usize,
    palette: *const RenderColor, palette_count: usize,
    params: *const RenderParams, vertices: *mut ShaderRenderVertex,
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
    let result=unsafe { &mut *renderer }.build_shader(info,snakes,
        unsafe { records(segments,segment_count) },unsafe { records(food,food_count) },
        unsafe { records(events,event_count) },unsafe { records(palette,palette_count) },params,vertices);
    let status=if result.vertex_count>vertex_capacity { BUFFER_TOO_SMALL } else { OK };
    unsafe { output.write(result); }
    status
}

/// Freeze procedural shader motion separately in the host; this shortens
/// renderer-managed discrete waves and dissolves. Classic output is unaffected.
/// # Safety
/// The handle must be live and exclusively borrowed for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_set_reduced_motion(renderer: *mut RenderHandle, enabled:u32) -> i32 {
    if !valid(renderer) || enabled>1 { return INVALID_ARGUMENT; }
    unsafe { &mut *renderer }.reduced_motion=enabled!=0;
    OK
}

/// Screen-space clock exclusion, independent of the world projection.
/// Empty width/height disables it. All values must be finite and bounded.
/// # Safety
/// The handle must be live and exclusively borrowed during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_set_clock_rect(renderer:*mut RenderHandle,x:f64,y:f64,w:f64,h:f64)->i32 {
    if !valid(renderer) || ![x,y,w,h].iter().all(|v|crate::render::coordinate(*v)) || w<0.0 || h<0.0 {return INVALID_ARGUMENT;}
    unsafe {&mut *renderer}.set_clock_rect([x,y,w,h]);
    OK
}

/// Copies at most four immutable item records into fixed renderer storage.
/// # Safety
/// Live exclusive renderer; items is a readable, aligned, disjoint array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_render_set_items(renderer: *mut RenderHandle, items: *const ItemRecord, count: usize, radius: f64) -> i32 {
    if !valid(renderer) || count>crate::MAX_ITEMS || (count>0 && (!valid(items) || !(0.0..=crate::render::NUMERIC_LIMIT).contains(&radius))) { return INVALID_ARGUMENT; }
    let records=if count==0 {&[]} else {unsafe {std::slice::from_raw_parts(items,count)}};
    if !records.iter().all(crate::render::items::valid_item) { return INVALID_ARGUMENT; }
    unsafe { &mut *renderer }.set_items(records,radius);
    OK
}

/// Shared physical radius of this world's item capsules.
/// # Safety
/// Handle is live/readable and calls on it are serialized. Invalid pointer returns zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn snakes_core_item_radius(handle: *const WorldHandle) -> f64 {
    if !valid(handle) {return 0.0;}
    unsafe { &*handle }.world.config().base_radius()*2.1
}
