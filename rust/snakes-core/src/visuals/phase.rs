// SPDX-License-Identifier: GPL-3.0-or-later
pub fn initial(seed: i32) -> f64 {
    (seed % 1000) as f64 / 159.0
}
pub fn aurora(seed: i32, speed: f64, epoch_ns: i64, presentation_ns: i64) -> f64 {
    // Subtract as i128 to avoid overflowing arbitrary caller timestamps.
    initial(seed) + (presentation_ns as i128 - epoch_ns as i128) as f64 * 0.000_000_000_54 * speed
}
