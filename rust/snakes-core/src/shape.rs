// SPDX-License-Identifier: GPL-3.0-or-later
//! Body half-width profile shared by rendering and V2 collisions (design spec §2.1).

/// Half-width multiplier for normalised position `u` along the body
/// (0 = head, 1 = tail tip).
#[inline]
pub fn taper(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    if u < 0.07 {
        0.84 + (1.0 - 0.84) * (u / 0.07)
    } else if u <= 0.6 {
        1.0
    } else {
        1.0 - 0.78 * ((u - 0.6) / 0.4).powf(1.6)
    }
}
