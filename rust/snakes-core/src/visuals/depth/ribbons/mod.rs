//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub turbulence: i32,
    pub twist: i32,
    pub length: i32,
    pub dance: bool,
    pub worldEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            turbulence: 40,
            twist: 55,
            length: 55,
            dance: true,
            worldEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"turbulence", 0., 100., 40.),
    Parameter::integer(2, c"twist", 0., 100., 55.),
    Parameter::integer(3, c"length", 0., 100., 55.),
    Parameter::boolean(4, c"dance", true),
    Parameter::boolean(5, c"worldEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 4,
    key: "ribbons",
    available: false,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
