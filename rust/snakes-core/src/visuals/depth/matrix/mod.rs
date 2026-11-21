//! Disabled living visual slot. Legacy renderer remains authoritative.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub weather: &'static str,
    pub wind: i32,
    pub lightning: bool,
    pub glyphs: &'static str,
    pub depth: i32,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            weather: "changing",
            wind: 40,
            lightning: true,
            glyphs: "weather",
            depth: 60,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"weather", c"steady|changing|stormy", 2., 1.),
    Parameter::integer(2, c"wind", 0., 100., 40.),
    Parameter::boolean(3, c"lightning", true),
    Parameter::enumeration(
        4,
        c"glyphs",
        c"weather|numbers|hex|katakana|letters|lines-blocks|braille",
        6.,
        0.,
    ),
    Parameter::integer(5, c"depth", 0., 100., 60.),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 6,
    key: "matrix",
    available: false,
    rate: 30,
    mirrored: MirroredPolicy::PerOutputLocal,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |_| None,
};
