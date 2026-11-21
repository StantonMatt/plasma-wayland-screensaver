//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub gravity: i32,
    pub twinkle: i32,
    pub milkyWay: i32,
    pub lifeCycle: bool,
    pub skyEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            gravity: 40,
            twinkle: 50,
            milkyWay: 40,
            lifeCycle: true,
            skyEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"gravity", 0., 100., 40.),
    Parameter::integer(2, c"twinkle", 0., 100., 50.),
    Parameter::integer(3, c"milkyWay", 0., 100., 40.),
    Parameter::boolean(4, c"lifeCycle", true),
    Parameter::boolean(5, c"skyEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 2,
    key: "constellation",
    available: false,
    rate: 60,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
