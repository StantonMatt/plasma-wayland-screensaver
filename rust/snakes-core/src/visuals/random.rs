// SPDX-License-Identifier: GPL-3.0-or-later
//! VisualUtils.js semantics, deliberately separate from the Snakes PRNG.
pub fn fract(value: f64) -> f64 {
    value - value.floor()
}
pub fn random(index: f64, seed: f64) -> f64 {
    fract((index * 127.1 + seed * 311.7).sin() * 43758.5453123)
}
pub fn positive_modulo(value: f64, modulus: f64) -> f64 {
    if modulus > 0.0 {
        ((value % modulus) + modulus) % modulus
    } else {
        0.0
    }
}
