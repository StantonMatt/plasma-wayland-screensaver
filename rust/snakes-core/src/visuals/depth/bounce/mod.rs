//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub count: i32,
    pub gravity: i32,
    pub elasticity: i32,
    pub material: &'static str,
    pub collisions: bool,
    pub worldEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            count: 12,
            gravity: 35,
            elasticity: 90,
            material: "mixed",
            collisions: true,
            worldEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"count", 1., 60., 12.),
    Parameter::integer(2, c"gravity", -100., 100., 35.),
    Parameter::integer(3, c"elasticity", 50., 100., 90.),
    Parameter::enumeration(4, c"material", c"mixed|rubber|glass|metal", 3., 0.),
    Parameter::boolean(5, c"collisions", true),
    Parameter::boolean(6, c"worldEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 7,
    key: "bounce",
    available: false,
    rate: 120,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Contain,
    parameters: PARAMETERS,
    create: |_| None,
};
