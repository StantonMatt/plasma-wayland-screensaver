//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub flight: &'static str,
    pub steering: i32,
    pub nebulae: i32,
    pub events: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            flight: "voyage",
            steering: 40,
            nebulae: 45,
            events: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"flight", c"voyage|hyperspace|cruise", 2., 0.),
    Parameter::integer(2, c"steering", 0., 100., 40.),
    Parameter::integer(3, c"nebulae", 0., 100., 45.),
    Parameter::boolean(4, c"events", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 3,
    key: "starfield",
    available: false,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
