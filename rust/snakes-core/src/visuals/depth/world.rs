//! Mutable worlds have one exclusive owner. Published snapshots contain no
//! borrowed mutable memory and may outlive the world and cross render threads.
use super::{
    common::{
        clock::Rate,
        layout::{Layout, Rect},
    },
    schema::Value,
};
use std::sync::Arc;
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorMode {
    Independent = 0,
    Mirrored = 1,
    Spanning = 2,
}
#[derive(Clone)]
pub struct WorldInput {
    pub visual_id: u32,
    pub seed: u64,
    pub mode: MonitorMode,
    pub layout: Layout,
    pub speed: f64,
    pub density: f64,
    pub scale: f64,
    pub trails: f64,
    pub palette: u32,
    pub reduced_motion: bool,
    pub parameters: Vec<Value>,
    pub environment: Environment,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Obstacle {
    pub output_id: u64,
    pub rect: Rect,
    pub inflation: f64,
    pub kind: u32,
    pub enabled: u32,
}
#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub background: u32,
    pub top: [f32; 4],
    pub bottom: [f32; 4],
    pub frame_cap: u32,
    pub obstacles: Vec<Obstacle>,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Generations {
    pub layout: u64,
    pub topology: u64,
    pub data: u64,
    pub sample: u64,
}
/// Frozen 72-byte example record. Identity is (id,generation); birth/death are
/// integer timeline times. Typed visual writers may additionally publish payloads.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Circle {
    pub id: u64,
    pub generation: u64,
    pub x: f64,
    pub y: f64,
    pub radius: f32,
    pub rgba: [f32; 4],
    pub flags: u32,
    pub birth_ns: i64,
    pub death_ns: i64,
}
#[derive(Clone, Debug)]
pub struct DataTexture {
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub tick: u64,
    pub time_ns: i64,
    pub generations: Generations,
    pub circles: Arc<[Circle]>,
    pub texture: Option<Arc<DataTexture>>,
    pub payload: Arc<[u8]>,
    /// Visual-owned immutable frame; typed sample writers borrow it through their lease.
    pub typed: Option<Arc<dyn std::any::Any + Send + Sync>>,
    pub emission_estimate: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub id: u32,
    pub at_ns: i64,
    pub anchor: [f64; 2],
    pub strength: f64,
}
pub trait LivingWorld: Send + 'static {
    fn rate(&self) -> Rate;
    fn step(&mut self, tick: u64);
    fn snapshot(&self) -> Snapshot;
    fn next_wake_ns(&self) -> Option<i64> {
        None
    }
    fn emission_estimate(&self) -> f64 {
        0.
    }
    fn inject(&mut self, _event: Event) -> Result<(), &'static str> {
        Err("unsupported event")
    }
    fn state_hash(&self) -> u64;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_contracts() {
        fn send<T: Send + 'static>() {}
        fn sync<T: Send + Sync>() {}
        send::<Box<dyn LivingWorld>>();
        sync::<Snapshot>();
        assert_eq!(size_of::<Circle>(), 72);
    }
}
