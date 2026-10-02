// SPDX-License-Identifier: GPL-3.0-or-later
/// The world LCG. Seeding intentionally uses f64 multiplication, as QML does:
/// large signed 32-bit seeds lose low product bits before the modulo operation.
#[derive(Clone, Copy, Debug)]
pub struct WorldRng {
    state: u32
}
impl WorldRng {
    pub fn new(seed: i32) -> Self {
        let state = (((seed as f64).abs()+1.0)*2654435761.0 % 4294967296.0) as u32;
        Self {
            state: state.max(1)
        }
    }
    pub fn state(self) -> u32 {
        self.state
    }
    pub fn random(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        self.state as f64/4294967296.0
    }
}
