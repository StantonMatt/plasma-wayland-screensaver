//! Shared foundation. All product slots remain disabled until their release gates pass.
pub mod aurora;
pub mod bounce;
pub mod common;
pub mod constellation;
pub mod ffi;
pub mod fireflies;
#[cfg(feature = "visual-capture")]
mod fixture;
pub mod kaleidoscope;
pub mod matrix;
pub mod orbs;
pub mod ribbons;
pub mod schema;
pub mod starfield;
pub mod world;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirroredPolicy {
    SharedCanonical,
    PerOutputLocal,
}
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum MirroredFit {
    Cover = 0,
    Contain = 1,
    Height = 2,
}
pub struct Descriptor {
    pub id: u32,
    pub key: &'static str,
    pub available: bool,
    pub rate: u32,
    pub mirrored: MirroredPolicy,
    pub fit: MirroredFit,
    pub parameters: &'static [schema::Parameter],
    pub create: fn(world::WorldInput) -> Option<Box<dyn world::LivingWorld>>,
}
pub const DESCRIPTORS: &[Descriptor] = &[
    aurora::DESCRIPTOR,
    constellation::DESCRIPTOR,
    starfield::DESCRIPTOR,
    ribbons::DESCRIPTOR,
    kaleidoscope::DESCRIPTOR,
    matrix::DESCRIPTOR,
    bounce::DESCRIPTOR,
    orbs::DESCRIPTOR,
    fireflies::DESCRIPTOR,
];
pub fn descriptor(id: u32) -> Option<&'static Descriptor> {
    DESCRIPTORS.iter().find(|d| d.id == id)
}
