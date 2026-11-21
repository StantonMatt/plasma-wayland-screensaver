//! Exact squared Euclidean distance transform, separable lower envelopes of
//! parabolas (Felzenszwalb and Huttenlocher). No shaping or font dependencies.
fn transform(source: &[f32], output: &mut [f32], sites: &mut [usize], edges: &mut [f32]) {
    let mut last = 0;
    sites[0] = 0;
    edges[0] = f32::NEG_INFINITY;
    edges[1] = f32::INFINITY;
    for q in 1..source.len() {
        let mut intersection;
        loop {
            let p = sites[last];
            intersection = ((source[q] - source[p]) / (q - p) as f32 + (q + p) as f32) * 0.5;
            if intersection > edges[last] || last == 0 {
                break;
            }
            last -= 1;
        }
        last += 1;
        sites[last] = q;
        edges[last] = intersection;
        edges[last + 1] = f32::INFINITY;
    }
    let mut k = 0;
    for (q, d) in output.iter_mut().enumerate() {
        while edges[k + 1] < (q as f32) {
            k += 1;
        }
        let p = sites[k];
        let delta = q as f32 - p as f32;
        *d = delta * delta + source[p];
    }
}
fn edt(grid: &mut [f32], size: usize) {
    let mut source = [0.; 96];
    let mut dest = [0.; 96];
    let mut sites = [0; 96];
    let mut edges = [0.; 97];
    for line in grid.chunks_exact_mut(size) {
        source[..size].copy_from_slice(line);
        transform(&source[..size], line, &mut sites, &mut edges);
    }
    for x in 0..size {
        for y in 0..size {
            source[y] = grid[y * size + x];
        }
        transform(&source[..size], &mut dest[..size], &mut sites, &mut edges);
        for y in 0..size {
            grid[y * size + x] = dest[y];
        }
    }
}
/// # Safety
/// The caller supplies two nonoverlapping size-squared contiguous u8 buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_glyph_sdf(
    input: *const u8,
    output: *mut u8,
    size: usize,
    spread: f32,
) -> bool {
    if input.is_null()
        || output.is_null()
        || !(1..=96).contains(&size)
        || !(0.5..=32.).contains(&spread)
    {
        return false;
    }
    let ink = unsafe { std::slice::from_raw_parts(input, size * size) };
    let sdf = unsafe { std::slice::from_raw_parts_mut(output, size * size) };
    let mut outside = Vec::with_capacity(size * size);
    let mut inside = Vec::with_capacity(size * size);
    for &sample in ink {
        let a = sample as f32 / 255.;
        outside.push(if sample == 0 {
            1e7
        } else {
            (0.5 - a).max(0.).powi(2)
        });
        inside.push(if sample == 255 {
            1e7
        } else {
            (a - 0.5).max(0.).powi(2)
        });
    }
    edt(&mut outside, size);
    edt(&mut inside, size);
    for ((v, o), i) in sdf.iter_mut().zip(outside).zip(inside) {
        *v = (127.5 + (i.sqrt() - o.sqrt()) * (127.5 / spread))
            .round()
            .clamp(0., 255.) as u8;
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn euclidean_diagonal_and_sign() {
        let mut ink = [0; 81];
        ink[40] = 255;
        let mut result = [0; 81];
        assert!(unsafe { visuals_glyph_sdf(ink.as_ptr(), result.as_mut_ptr(), 9, 4.) });
        assert!(result[40] > 128);
        assert!(result[41] < 128);
        assert!(result[50] < result[41]);
        assert_eq!(result[40 + 2 * 9 + 2], 37);
    }
}
