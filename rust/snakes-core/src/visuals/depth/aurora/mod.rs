//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub activity: i32,
    pub horizon: &'static str,
    pub stars: bool,
    pub skyEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            activity: 40,
            horizon: "forest",
            stars: true,
            skyEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"activity", 0., 100., 40.),
    Parameter::enumeration(2, c"horizon", c"none|forest|mountains", 2., 1.),
    Parameter::boolean(3, c"stars", true),
    Parameter::boolean(4, c"skyEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 1,
    key: "aurora",
    available: false,
    rate: 20,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
