// SPDX-License-Identifier: GPL-3.0-or-later

pub fn pixels() -> &'static [u8] {
    include_bytes!("nebula.rgba")
}

// The tile is immutable across worlds. Keep the Rust bake reproducible and
// verify its checked-in bytes without doing this work on the first frame.
#[cfg(test)]
#[test]
fn baked_noise_matches_the_periodic_lattices() {
    use super::super::common::rng::hash64;
    use super::space::smooth;
    // Independent periodic lattices. Opposite edges interpolate through the
    // same control points, including the slopes used for the domain warp.
    let mut out = vec![0u8; 256 * 256 * 4];
    for channel in 0..3 {
        let mut values = vec![0.; 256 * 256];
        for (n, weight) in [(4, 0.37), (8, 0.27), (16, 0.18), (32, 0.11), (64, 0.07)] {
            let grid: Vec<f64> = (0..n * n)
                .map(|i| {
                    (hash64((i + n * 300 + channel * 70001) as u64) >> 11) as f64
                        / ((1u64 << 53) as f64)
                })
                .collect();
            let pitch = 256 / n;
            let frac: Vec<_> = (0..pitch)
                .map(|x| smooth(x as f64 / pitch as f64))
                .collect();
            for (y, row) in values.chunks_exact_mut(256).enumerate() {
                let iy = y / pitch;
                let fy = frac[y % pitch];
                let top = &grid[iy * n..][..n];
                let bottom = &grid[((iy + 1) % n) * n..][..n];
                for (ix, pixels) in row.chunks_exact_mut(pitch).enumerate() {
                    let next = (ix + 1) % n;
                    let (a, b, c, d) = (top[ix], top[next], bottom[ix], bottom[next]);
                    for (value, &fx) in pixels.iter_mut().zip(&frac) {
                        let top = a * (1. - fx) + b * fx;
                        let bottom = c * (1. - fx) + d * fx;
                        *value += (top + (bottom - top) * fy) * weight;
                    }
                }
            }
        }
        for (i, v) in values.into_iter().enumerate() {
            out[i * 4 + channel] = (255. * v) as u8;
            out[i * 4 + 3] = 255;
        }
    }
    assert_eq!(out.as_slice(), pixels());
}
