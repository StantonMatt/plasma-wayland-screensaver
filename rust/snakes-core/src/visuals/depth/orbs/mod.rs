//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub look: &'static str,
    pub flow: &'static str,
    pub merging: i32,
    pub depth: i32,
    pub worldEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            look: "glow",
            flow: "drift",
            merging: 50,
            depth: 60,
            worldEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"look", c"glow|glass|bubbles", 2., 0.),
    Parameter::enumeration(2, c"flow", c"drift|rise|swirl", 2., 0.),
    Parameter::integer(3, c"merging", 0., 100., 50.),
    Parameter::integer(4, c"depth", 0., 100., 60.),
    Parameter::boolean(5, c"worldEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 8,
    key: "orbs",
    available: false,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
