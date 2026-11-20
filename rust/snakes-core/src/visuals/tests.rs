// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn config() -> Config {
    Config {
        abi_version: ABI_VERSION,
        struct_size: size_of::<Config>() as u32,
        visual: 1,
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
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-10 * b.abs().max(1.0), "{a} != {b}");
}
#[test]
fn frozen_qml_fixtures() {
    let mut bands = 0;
    for line in include_str!("../../../../tests/parity/aurora/fixtures.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split_whitespace().collect();
        let num = |i: usize| f[i].parse::<f64>().unwrap();
        match f[0] {
            "B" => {
                let c = Config {
                    density: num(1),
                    scale: num(2),
                    speed: num(3),
                    trails: num(4),
                    seed: num(5) as i32,
                    palette: num(6) as u32,
                    reduced_motion: num(7) as u32,
                    ..config()
                };
                let p = if c.reduced_motion != 0 {
                    phase::initial(c.seed)
                } else {
                    phase::aurora(c.seed, c.speed, 0, (num(10) * 1e9).round() as i64)
                };
                close(p, num(11));
                let i = num(13) as usize;
                for (actual, k) in aurora::band_geometry(&c, p, num(8), num(9), i)
                    .iter()
                    .zip(14..18)
                {
                    close(*actual, num(k));
                }
                let request = Request {
                    width: num(8),
                    height: num(9),
                    presentation_ns: 0,
                    advance: 0,
                    reserved: 0,
                    ..Request::default()
                };
                let frame = aurora::frame(&c, p, &request);
                assert_eq!(frame.band_count, num(12) as u32);
                assert_eq!(
                    frame.shade,
                    [48.0 / 255.0, 0.55, 120.0 / 255.0, num(17) as f32]
                );
                for (offset, color_index) in [i, i + 2].iter().enumerate() {
                    let rgb = u32::from_str_radix(&f[18 + offset][1..], 16).unwrap();
                    assert_eq!(palette::COLORS[c.palette as usize][color_index % 6], rgb);
                }
                assert_eq!(
                    frame.bands[i].geometry,
                    aurora::band_geometry(&c, p, num(8), num(9), i).map(|x| x as f32)
                );
                bands += 1;
            }
            "S" => {
                assert_eq!(
                    &f[1..7],
                    &["0", "#30000000", "0.55", "#00000000", "1", "#78000000"]
                );
            }
            "R" => close(random::random(num(1), num(2)), num(3)),
            _ => panic!("unknown fixture row"),
        }
    }
    assert_eq!(bands, 372);
}
#[test]
fn javascript_number_semantics() {
    assert_eq!(phase::initial(-1197), -197.0 / 159.0);
    assert_eq!(random::fract(-0.25), 0.75);
    assert_eq!(random::positive_modulo(-7.0, 5.0), 3.0);
    assert_eq!(random::positive_modulo(7.0, 0.0), 0.0);
    assert_eq!(random::positive_modulo(1e-20, 5.0), 0.0);
}
#[test]
fn abi_rejects_invalid_and_short_records_without_writes() {
    unsafe {
        assert!(visuals_core_create(std::ptr::null()).is_null());
        let mut c = config();
        c.abi_version += 1;
        assert!(visuals_core_create(&c).is_null());
        c = config();
        c.struct_size -= 1;
        assert!(visuals_core_create(&c).is_null());
        c = config();
        c.speed = f64::NAN;
        assert!(visuals_core_create(&c).is_null());
        c = config();
        let core = visuals_core_create(&c);
        assert!(!core.is_null());
        let mut output = Frame {
            band_count: 123,
            ..Frame::default()
        };
        let r = Request {
            width: 640.0,
            height: 360.0,
            presentation_ns: 1_000_000_000,
            advance: 1,
            reserved: 0,
            world_bounds: [-3440.0, -1080.0, 7920.0, 2520.0],
            viewport_origin: [-3440.0, -1080.0],
        };
        assert_eq!(
            visuals_core_frame(core, &r, &mut output, size_of::<Frame>() - 1),
            -1
        );
        assert_eq!(output.band_count, 123);
        assert_eq!(
            visuals_core_frame(core, &r, &mut output, size_of::<Frame>()),
            0
        );
        let moving = output.bands[0].geometry;
        c.reduced_motion = 1;
        assert_eq!(visuals_core_configure(core, &c), 0);
        let r = Request {
            presentation_ns: 9_000_000_000,
            ..r
        };
        assert_eq!(
            visuals_core_frame(core, &r, &mut output, size_of::<Frame>()),
            0
        );
        close(output.bands[0].geometry[1] as f64, moving[1] as f64);
        let mut pixels = [0u8; 4];
        assert_eq!(
            visuals_core_raster(&output, 1, 1, pixels.as_mut_ptr(), 3),
            -1
        );
        assert_eq!(
            visuals_core_raster(&output, 1, 1, pixels.as_mut_ptr(), 4),
            0
        );
        assert!(pixels[..3].iter().all(|v| *v <= pixels[3]));
        visuals_core_destroy(core);
        visuals_core_destroy(std::ptr::null_mut());
    }
}
