//! Stable integer-only seed derivation. Never use a process-seeded hash for outputs.
pub fn hash64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
pub fn identity_hash(identity: &str) -> u64 {
    identity.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
pub fn derive_seed(base: u64, domain: u64, entity: u64, generation: u64) -> u64 {
    hash64(hash64(hash64(base ^ domain) ^ entity) ^ generation)
}
pub fn output_seed(base: u64, visual: u32, connector: &str) -> u64 {
    derive_seed(base, u64::from(visual), identity_hash(connector), 0)
}
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    increment: u64,
}
impl Pcg32 {
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut r = Self {
            state: 0,
            increment: (stream << 1) | 1,
        };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6364136223846793005)
            .wrapping_add(self.increment);
        ((((old >> 18) ^ old) >> 27) as u32).rotate_right((old >> 59) as u32)
    }
    pub fn unit(&mut self) -> f64 {
        f64::from(self.next_u32()) / 4294967296.0
    }
    pub fn bounded(&mut self, bound: u32) -> Option<u32> {
        if bound == 0 {
            return None;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return Some(r % bound);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixtures() {
        assert_eq!(hash64(0), 0xe220a8397b1dcdaf);
        let mut r = Pcg32::new(42, 54);
        assert_eq!((r.next_u32(), r.next_u32()), (0xa15c02b7, 0x7b47f409));
        assert_eq!(output_seed(7, 2, "DP-5"), output_seed(7, 2, "DP-5"));
        assert_ne!(output_seed(7, 2, "DP-5"), output_seed(7, 2, "DP-4"));
        assert_ne!(output_seed(7, 2, "DP-5"), output_seed(7, 3, "DP-5"));
    }
}
