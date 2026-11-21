//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub synchrony: i32,
    pub nightCycle: bool,
    pub nightEvents: bool,
    pub hunters: bool,
    pub grass: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            synchrony: 55,
            nightCycle: true,
            nightEvents: true,
            hunters: true,
            grass: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"synchrony", 0., 100., 55.),
    Parameter::boolean(2, c"nightCycle", true),
    Parameter::boolean(3, c"nightEvents", true),
    Parameter::boolean(4, c"hunters", true),
    Parameter::boolean(5, c"grass", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 9,
    key: "fireflies",
    available: false,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Height,
    parameters: PARAMETERS,
    create: |_| None,
};
