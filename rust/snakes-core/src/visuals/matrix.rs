// SPDX-License-Identifier: GPL-3.0-or-later
//! Frozen Matrix.qml stream math. Qt supplies only resolved font metrics/coverage.
use super::{Request, palette, random, settings::Config};
pub const VISUAL: u32 = 6;
pub const GLYPHS: usize = 37;
pub const MAX_ROWS: usize = 30;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Layout {
    pub column_width: f64,
    pub line_height: f64,
    pub glyph_size: u32,
    pub column_count: u32,
    pub stream_opacity: f32,
    pub head_opacity: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Column {
    pub x: f64,
    pub fall_rate: f64,
    pub start_offset: f64,
    pub content_height: f64,
    pub head_y: f64,
    pub color: [f32; 4],
    pub length: u32,
    pub glyphs: [u8; MAX_ROWS],
    pub reserved: [u8; 6],
}
pub struct Matrix {
    config: Config,
    phase: f64,
    columns: Vec<Column>,
    width: f64,
}
fn valid(config: &Config) -> bool {
    // Reuse the shared settings validation without widening visuals_core's IDs.
    let mut common = *config;
    common.visual = 1;
    config.visual == VISUAL && common.valid()
}
pub fn layout(config: &Config, width: f64) -> Layout {
    let size = ((30.0 - config.density * 0.12) * config.scale)
        .round()
        .max(11.0);
    let column_width = size * 1.18;
    Layout {
        column_width,
        line_height: size * 0.95,
        glyph_size: size as u32,
        column_count: (width / column_width.max(1.0)).ceil() as u32 + 1,
        stream_opacity: (0.28 + config.trails * 0.52) as f32,
        head_opacity: (0.72 + config.trails * 0.28) as f32,
    }
}
pub fn column(config: &Config, layout: &Layout, index: usize) -> Column {
    let random = |n| random::random(n, config.seed as f64);
    let length =
        6 + (random((index * 7 + 1) as f64) * (8.0 + config.density * 0.16)).floor() as u32;
    let mut result = Column {
        x: index as f64 * layout.column_width,
        fall_rate: 0.32 + random((index * 7 + 2) as f64) * 0.82,
        start_offset: random((index * 7 + 3) as f64),
        color: palette::color(config.palette, index),
        length,
        ..Column::default()
    };
    for row in 0..length as usize {
        result.glyphs[row] = (random((index * 97 + row * 29) as f64) * GLYPHS as f64).floor() as u8;
    }
    result
}
pub fn content_height(layout: &Layout, length: u32, font_height: f64) -> f64 {
    length as f64 * layout.line_height + (font_height - layout.line_height).max(0.0).trunc()
}
#[cfg(test)]
pub fn offset(
    config: &Config,
    layout: &Layout,
    index: usize,
    phase: f64,
    height: f64,
    font_height: f64,
) -> f64 {
    // Only three seed constants per column/frame, never glyph generation.
    let length = 6
        + (random::random((index * 7 + 1) as f64, config.seed as f64)
            * (8.0 + config.density * 0.16))
            .floor() as u32;
    let fall_rate = 0.32 + random::random((index * 7 + 2) as f64, config.seed as f64) * 0.82;
    let start_offset = random::random((index * 7 + 3) as f64, config.seed as f64);
    let stream_height = content_height(layout, length, font_height);
    let modulus = height + stream_height;
    ((phase * fall_rate * height + start_offset * modulus) % modulus + modulus) % modulus
        - stream_height
}
#[unsafe(no_mangle)]
pub extern "C" fn visuals_matrix_abi_version() -> u32 {
    1
}
/// # Safety
/// Readable config. Returned handle is exclusively owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_matrix_create(config: *const Config) -> *mut Matrix {
    let Some(config) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    if !valid(config) {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(Matrix {
        config: *config,
        phase: 0.0,
        columns: Vec::new(),
        width: 0.0,
    }))
}
/// # Safety
/// Exclusive live handle and readable config; no caller pointers retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_matrix_configure(core: *mut Matrix, config: *const Config) -> i32 {
    let (Some(core), Some(config)) = (unsafe { core.as_mut() }, unsafe { config.as_ref() }) else {
        return -1;
    };
    if !valid(config) {
        return -1;
    }
    if core.config.seed != config.seed
        || core.config.density != config.density
        || core.config.scale != config.scale
    {
        core.columns.clear();
    }
    core.config = *config;
    0
}
/// # Safety
/// Destroy exactly once; null accepted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_matrix_destroy(core: *mut Matrix) {
    if !core.is_null() {
        drop(unsafe { Box::from_raw(core) })
    }
}
/// # Safety
/// Readable live handle; writable layout output, nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_matrix_get_layout(
    core: *const Matrix,
    width: f64,
    out: *mut Layout,
) -> i32 {
    let Some(core) = (unsafe { core.as_ref() }) else {
        return -1;
    };
    if out.is_null() || !width.is_finite() || width <= 0.0 || width > 65536.0 {
        return -1;
    }
    unsafe { out.write(layout(&core.config, width)) };
    0
}
/// # Safety
/// Readable live handle and writable capacity initialized Column records, nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_matrix_columns(
    core: *mut Matrix,
    width: f64,
    font_height: f64,
    out: *mut Column,
    capacity: usize,
) -> i32 {
    let Some(core) = (unsafe { core.as_mut() }) else {
        return -1;
    };
    if out.is_null() || !width.is_finite() || width <= 0.0 || width > 65536.0 {
        return -1;
    }
    let layout = layout(&core.config, width);
    if capacity < (layout.column_count as usize)
        || !font_height.is_finite()
        || !(1.0..=1024.0).contains(&font_height)
    {
        return -1;
    }
    core.columns
        .resize(layout.column_count as usize, Column::default());
    for i in 0..layout.column_count as usize {
        let mut col = column(&core.config, &layout, i);
        col.content_height = content_height(&layout, col.length, font_height);
        col.head_y = (col.content_height - layout.glyph_size as f64 * 1.05).max(0.0);
        core.columns[i] = col;
        unsafe { out.add(i).write(col) }
    }
    core.width = width;
    0
}
/// # Safety
/// Exclusive live handle, readable request, writable 4*capacity initialized floats, no overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_matrix_frame(
    core: *mut Matrix,
    request: *const Request,
    out: *mut f32,
    capacity: usize,
) -> i32 {
    let (Some(core), Some(request)) = (unsafe { core.as_mut() }, unsafe { request.as_ref() })
    else {
        return -1;
    };
    if !request.valid() || out.is_null() {
        return -1;
    }
    let layout = layout(&core.config, request.width);
    if capacity < (layout.column_count as usize)
        || core.width != request.width
        || core.columns.len() != layout.column_count as usize
    {
        return -1;
    }
    if request.advance != 0 && core.config.reduced_motion == 0 {
        core.phase = (request.presentation_ns as i128 - core.config.epoch_ns as i128) as f64
            * 1e-9
            * 0.18
            * core.config.speed;
    }
    for i in 0..layout.column_count as usize {
        let col = &core.columns[i];
        let modulus = request.height + col.content_height;
        let y = (((core.phase * col.fall_rate * request.height + col.start_offset * modulus)
            % modulus
            + modulus)
            % modulus
            - col.content_height) as f32;
        unsafe { std::ptr::copy_nonoverlapping([y, 0.0, 0.0, 0.0].as_ptr(), out.add(i * 4), 4) }
    }
    0
}
const _: () = {
    assert!(size_of::<Layout>() == 32);
    assert!(size_of::<Column>() == 96);
    assert!(std::mem::offset_of!(Column, glyphs) == 60);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::ABI_VERSION;
    fn config() -> Config {
        Config {
            abi_version: ABI_VERSION,
            struct_size: size_of::<Config>() as u32,
            visual: VISUAL,
            palette: 0,
            density: 55.0,
            scale: 2.0,
            speed: 3.0,
            trails: 1.0,
            seed: 1,
            reduced_motion: 0,
            epoch_ns: 0,
        }
    }
    fn prepare(core: &mut Matrix, width: f64, font_height: f64) {
        let mut cols = vec![Column::default(); layout(&core.config, width).column_count as usize];
        assert_eq!(
            unsafe {
                visuals_matrix_columns(core, width, font_height, cols.as_mut_ptr(), cols.len())
            },
            0
        );
    }
    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 2e-7 * expected.abs().max(1.0),
            "{actual} != {expected}"
        );
    }
    #[test]
    fn frozen_qml_columns_and_motion() {
        let mut c = config();
        let mut l = Layout::default();
        let (mut phase, mut height, mut font_height) = (0.0, 0.0, 0.0);
        let mut columns = 0;
        let mut frame_offsets = Vec::new();
        for row in include_str!("../../../../tests/parity/matrix/fixtures.tsv")
            .lines()
            .filter(|x| !x.starts_with('#'))
        {
            let cells: Vec<_> = row.split('\t').collect();
            let n = |i: usize| cells[i].parse::<f64>().unwrap();
            if cells[0] == "F" {
                c.density = n(1);
                c.scale = n(2);
                c.speed = n(3);
                c.trails = n(4);
                c.seed = n(5) as i32;
                c.palette = n(9) as u32;
                c.reduced_motion = n(10) as u32;
                height = n(7);
                phase = n(11);
                font_height = n(15);
                l = layout(&c, n(6));
                assert_eq!(l.glyph_size, n(12) as u32);
                close(l.column_width, n(13));
                assert_eq!(l.column_count, n(14) as u32);
                close(l.stream_opacity as f64, n(16));
                close(l.head_opacity as f64, n(17));
                let mut core = Matrix {
                    config: c,
                    phase: 0.0,
                    columns: Vec::new(),
                    width: 0.0,
                };
                let request = Request {
                    width: n(6),
                    height,
                    presentation_ns: (n(8) * 1e9).round() as i64,
                    advance: 1,
                    ..Request::default()
                };
                frame_offsets.resize(l.column_count as usize * 4, 0.0);
                prepare(&mut core, request.width, font_height);
                assert_eq!(
                    unsafe {
                        visuals_matrix_frame(
                            &mut core,
                            &request,
                            frame_offsets.as_mut_ptr(),
                            l.column_count as usize,
                        )
                    },
                    0
                );
                close(core.phase, phase);
            } else {
                let index = n(1) as usize;
                let col = column(&c, &l, index);
                close(col.x, n(2));
                close(frame_offsets[index * 4] as f64, n(3));
                assert_eq!(
                    &frame_offsets[index * 4 + 1..index * 4 + 4],
                    &[0.0, 0.0, 0.0]
                );
                close(offset(&c, &l, index, phase, height, font_height), n(3));
                assert_eq!(col.length, n(4) as u32);
                close(col.fall_rate, n(5));
                close(col.start_offset, n(6));
                let h = content_height(&l, col.length, font_height);
                close(h, n(7));
                close((h - l.glyph_size as f64 * 1.05).max(0.0), n(8));
                let color = u32::from_str_radix(cells[9].trim_start_matches('#'), 16).unwrap();
                for channel in 0..3 {
                    close(
                        col.color[channel] as f64,
                        ((color >> (16 - channel * 8)) & 255) as f64 / 255.0,
                    );
                }
                close(col.color[3] as f64, 1.0);
                for r in 0..col.length as usize {
                    assert_eq!(col.glyphs[r], n(10 + r) as u8);
                }
                columns += 1;
            }
        }
        assert!(columns > 1000);
    }
    #[test]
    fn abi_rejects_invalid_records_and_keeps_outputs_untouched() {
        let mut c = config();
        assert_eq!(visuals_matrix_abi_version(), 1);
        unsafe {
            assert!(visuals_matrix_create(std::ptr::null()).is_null());
            c.visual = 1;
            assert!(visuals_matrix_create(&c).is_null());
            c = config();
            let core = visuals_matrix_create(&c);
            assert!(!core.is_null());
            let mut out = [9.0f32; 4];
            let request = Request {
                width: 3440.0,
                height: 1440.0,
                ..Request::default()
            };
            assert_eq!(
                visuals_matrix_frame(core, &request, out.as_mut_ptr(), 1),
                -1
            );
            assert_eq!(out, [9.0; 4]);
            let mut cols = [Column::default()];
            cols[0].length = 999;
            assert_eq!(
                visuals_matrix_columns(core, 3440.0, 55.0, cols.as_mut_ptr(), 1),
                -1
            );
            assert_eq!(cols[0].length, 999);
            let mut bad = request;
            bad.width = f64::NAN;
            assert_eq!(visuals_matrix_frame(core, &bad, out.as_mut_ptr(), 1), -1);
            c.abi_version = 999;
            assert_eq!(visuals_matrix_configure(core, &c), -1);
            visuals_matrix_destroy(core);
            visuals_matrix_destroy(std::ptr::null_mut());
        }
    }
    #[test]
    fn reduced_motion_keeps_previous_phase_and_layout_changes_do_not_reset_it() {
        let mut c = config();
        let mut core = Matrix {
            config: c,
            phase: 0.0,
            columns: Vec::new(),
            width: 0.0,
        };
        let mut out = vec![0.0; 1000];
        let mut request = Request {
            width: 320.0,
            height: 180.0,
            presentation_ns: 12_500_000_000,
            advance: 1,
            ..Request::default()
        };
        prepare(&mut core, request.width, 55.0);
        unsafe {
            assert_eq!(
                visuals_matrix_frame(&mut core, &request, out.as_mut_ptr(), 250),
                0
            )
        };
        close(core.phase, 6.75);
        c.reduced_motion = 1;
        c.seed = -19;
        c.density = 100.0;
        unsafe { assert_eq!(visuals_matrix_configure(&mut core, &c), 0) };
        prepare(&mut core, request.width, 55.0);
        request.presentation_ns = 999_000_000_000;
        unsafe {
            assert_eq!(
                visuals_matrix_frame(&mut core, &request, out.as_mut_ptr(), 250),
                0
            )
        };
        close(core.phase, 6.75);
    }
}
