//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub mirrors: &'static str,
    pub points: i32,
    pub glass: &'static str,
    pub turning: i32,
    pub events: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            mirrors: "mandala",
            points: 8,
            glass: "mixed",
            turning: 40,
            events: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"mirrors", c"mandala|lattice", 1., 0.),
    Parameter::integer(2, c"points", 3., 16., 8.),
    Parameter::enumeration(3, c"glass", c"mixed|beads|shards|rods-wire", 3., 0.),
    Parameter::integer(4, c"turning", 0., 100., 40.),
    Parameter::boolean(5, c"events", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 5,
    key: "kaleidoscope",
    available: false,
    rate: 120,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
