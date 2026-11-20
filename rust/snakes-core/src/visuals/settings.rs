// SPDX-License-Identifier: GPL-3.0-or-later
//! Normalized VisualUtils/QML settings. All procedural math uses JavaScript doubles.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub abi_version: u32,
    pub struct_size: u32,
    pub visual: u32,
    pub palette: u32,
    pub density: f64,
    pub scale: f64,
    pub speed: f64,
    pub trails: f64,
    pub seed: i32,
    pub reduced_motion: u32,
    pub epoch_ns: i64,
}
impl Config {
    pub fn valid(&self) -> bool {
        self.abi_version == super::ABI_VERSION
            && self.struct_size as usize == size_of::<Self>()
            && self.visual == 1
            && self.palette <= 5
            && self.reduced_motion <= 1
            && self.density.is_finite()
            && (10.0..=100.0).contains(&self.density)
            && self.scale.is_finite()
            && (0.25..=2.0).contains(&self.scale)
            && self.speed.is_finite()
            && (0.1..=3.0).contains(&self.speed)
            && self.trails.is_finite()
            && (0.0..=1.0).contains(&self.trails)
    }
}
