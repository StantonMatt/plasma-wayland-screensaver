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

// Exact short-body profiles are shared across lengths, snakes and viewports.
// Initialised by Renderer/Spatial constructors, so growth never allocates or repeats powf.
// Long bodies retain the existing per-snake/binned cache. About 1.13 MiB once.
pub(crate) const SHORT_TAPER_MAX:usize=512;
const SHORT_TAPER_ENTRIES:usize=SHORT_TAPER_MAX*(SHORT_TAPER_MAX+1)/2;
struct ShortTapers {widths:Box<[f64]>,bytes:Box<[u8]>}
static SHORT_TAPERS:std::sync::OnceLock<ShortTapers>=std::sync::OnceLock::new();
pub(crate) fn prepare_short_tapers() {
    SHORT_TAPERS.get_or_init(|| {
        let mut widths=vec![0.0;SHORT_TAPER_ENTRIES];let mut bytes=vec![0;SHORT_TAPER_ENTRIES];
        for n in 1..=SHORT_TAPER_MAX {
            let start=n*(n-1)/2;
            for i in 0..n {
                let width=crate::shape::taper(i as f64/n.saturating_sub(1).max(1) as f64);
                widths[start+i]=width;bytes[start+i]=(width*255.0).round() as u8;
            }
        }
        ShortTapers {widths:widths.into_boxed_slice(),bytes:bytes.into_boxed_slice()}
    });
}
pub(crate) fn short_tapers(n:usize)->(&'static [f64],&'static [u8]) {
    debug_assert!((1..=SHORT_TAPER_MAX).contains(&n));
    let table=SHORT_TAPERS.get().expect("Renderer/Spatial constructors initialize shared taper profiles");
    let start=n*(n-1)/2;
    (&table.widths[start..start+n],&table.bytes[start..start+n])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_short_tapers_match_every_original_width_and_byte() {
        prepare_short_tapers();
        for n in 1..=SHORT_TAPER_MAX {
            let (widths,bytes)=short_tapers(n);
            for i in 0..n {
                let expected=crate::shape::taper(i as f64/n.saturating_sub(1).max(1) as f64);
                assert_eq!(widths[i].to_bits(),expected.to_bits());
                assert_eq!(bytes[i],(expected*255.0).round() as u8);
            }
        }
    }
}
