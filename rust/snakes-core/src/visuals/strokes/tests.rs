// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn config(visual: u32) -> Config {
    Config {
        abi_version: crate::visuals::ABI_VERSION,
        struct_size: 64,
        visual,
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
fn request() -> Request {
    Request {
        width: 3440.0,
        height: 1440.0,
        advance: 1,
        presentation_ns: 12_500_000_000,
        ..Request::default()
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 * b.abs().max(1.0), "{a} != {b}");
}
fn rgb(c: &Config, index: usize) -> String {
    format!("#{:06x}", palette::COLORS[c.palette as usize][index % 6])
}
#[test]
fn frozen_qml_commands() {
    let lines: Vec<_> = include_str!("../../../../../tests/parity/strokes/fixtures.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect();
    let mut cursor = 0;
    let mut frames = 0;
    while cursor < lines.len() {
        let f: Vec<_> = lines[cursor].split_whitespace().collect();
        assert_eq!(f[0], "F");
        cursor += 1;
        frames += 1;
        let num = |i: usize| f[i].parse::<f64>().unwrap();
        let c = Config {
            visual: num(1) as u32,
            density: num(2),
            scale: num(3),
            speed: num(4),
            trails: num(5),
            seed: num(6) as i32,
            palette: num(10) as u32,
            reduced_motion: num(11) as u32,
            ..config(RIBBONS)
        };
        let r = Request {
            width: num(7),
            height: num(8),
            ..request()
        };
        let phase = if c.reduced_motion != 0 {
            0.0
        } else {
            num(9) * if c.visual == RIBBONS { 0.55 } else { 1.0 }
        };
        close(phase, num(12));
        let mut verify = |tag: &str, values: &[f64], index: usize| {
            let f: Vec<_> = lines[cursor].split_whitespace().collect();
            cursor += 1;
            assert_eq!(f[0], tag);
            assert_eq!(f.len(), values.len() + 2);
            for (a, b) in values.iter().zip(&f[1..]) {
                close(*a, b.parse().unwrap());
            }
            assert_eq!(f.last().unwrap(), &rgb(&c, index));
        };
        if c.visual == RIBBONS {
            ribbons::paths(&c, phase, r.width, r.height, |path| {
                let mut v: Vec<_> = path.points.iter().flat_map(|p| [p.x, p.y]).collect();
                v.extend([path.width, path.alpha]);
                verify("R", &v, path.color);
            });
        } else {
            kaleidoscope::shapes(&c, phase, r.width, r.height, |shape| {
                let origin = Point::new(r.width * 0.5, r.height * 0.5);
                let mut v = Vec::new();
                for i in 0..2 {
                    let p = shape.circles[i].transform(shape.angle, origin);
                    v.extend([p.x, p.y, shape.radii[i]]);
                }
                v.push(shape.alpha);
                verify("C", &v, shape.color);
                let mut v: Vec<_> = shape
                    .curve
                    .iter()
                    .map(|p| p.transform(shape.angle, origin))
                    .flat_map(|p| [p.x, p.y])
                    .collect();
                v.extend([shape.width, shape.alpha]);
                verify("Q", &v, shape.color + 2);
            });
        }
    }
    assert_eq!(frames, 144);
}
#[test]
fn ffi_capacity_freeze_topology_and_mapping() {
    unsafe {
        assert_eq!(visuals_strokes_abi_version(), 1);
        assert!(visuals_strokes_create(std::ptr::null()).is_null());
        for kind in [RIBBONS, KALEIDOSCOPE] {
            let mut c = config(kind);
            let core = visuals_strokes_create(&c);
            assert!(!core.is_null());
            let mut r = request();
            let count = visuals_strokes_vertex_count(core, &r);
            assert!(count > 0);
            let mut vertices = vec![Vertex::default(); count];
            assert_eq!(
                visuals_strokes_frame(core, &r, vertices.as_mut_ptr(), count - 1),
                -1
            );
            assert!(vertices.iter().all(|v| *v == Vertex::default()));
            assert_eq!((*core).phase, 0.0);
            assert_eq!(
                visuals_strokes_frame(core, &r, vertices.as_mut_ptr(), count),
                count as i32
            );
            let first = vertices.clone();
            r.world_bounds = [-3440.0, -1080.0, 7920.0, 2520.0];
            r.viewport_origin = [-3440.0, -1080.0];
            assert_eq!(
                visuals_strokes_frame(core, &r, vertices.as_mut_ptr(), count),
                count as i32
            );
            assert_eq!(vertices, first);
            c.reduced_motion = 1;
            assert_eq!(visuals_strokes_configure(core, &c), 0);
            r.presentation_ns += 90_000_000_000;
            assert_eq!(
                visuals_strokes_frame(core, &r, vertices.as_mut_ptr(), count),
                count as i32
            );
            assert_eq!(vertices, first);
            c.reduced_motion = 0;
            assert_eq!(visuals_strokes_configure(core, &c), 0);
            for time in [0, 1, 999_000_000, 100_000_000_000] {
                r.presentation_ns = time;
                assert_eq!(visuals_strokes_vertex_count(core, &r), count);
                assert_eq!(
                    visuals_strokes_frame(core, &r, vertices.as_mut_ptr(), count),
                    count as i32
                );
                assert!(
                    vertices
                        .iter()
                        .flat_map(|v| v
                            .position
                            .iter()
                            .chain(v.shape.iter())
                            .chain(v.parameters.iter()))
                        .all(|x| x.is_finite())
                );
            }
            r.width = f64::NAN;
            assert_eq!(visuals_strokes_vertex_count(core, &r), 0);
            visuals_strokes_destroy(core);
        }
        visuals_strokes_destroy(std::ptr::null_mut());
    }
}
#[test]
fn tessellation_chord_error_and_continuity() {
    for (w, h) in [(640.0, 360.0), (3440.0, 1440.0), (65536.0, 65536.0)] {
        for phase in [0.0, 0.1, 1.0, 6.875, 100000.0] {
            let c = config(RIBBONS);
            let n = mesh::steps(RIBBONS, w, h, c.scale);
            ribbons::paths(&c, phase, w, h, |p| {
                for control in [&p.points[..4], &p.points[3..]] {
                    for i in 0..n {
                        let a = mesh::cubic(control, i as f64 / n as f64);
                        let b = mesh::cubic(control, (i + 1) as f64 / n as f64);
                        let middle = mesh::cubic(control, (i as f64 + 0.5) / n as f64);
                        assert!((middle - (a + b) * 0.5).length() < 0.25);
                    }
                }
            });
            let c = config(KALEIDOSCOPE);
            let n = mesh::steps(KALEIDOSCOPE, w, h, c.scale);
            kaleidoscope::shapes(&c, phase, w, h, |p| {
                for i in 0..n {
                    let a = mesh::quadratic(&p.curve, i as f64 / n as f64);
                    let b = mesh::quadratic(&p.curve, (i + 1) as f64 / n as f64);
                    let middle = mesh::quadratic(&p.curve, (i as f64 + 0.5) / n as f64);
                    assert!((middle - (a + b) * 0.5).length() < 0.25);
                }
            });
        }
    }
    let mut v = vec![Vertex::default(); 12];
    let mut writer = mesh::Writer {
        vertices: &mut v,
        count: 0,
    };
    writer.stroke(
        &[
            Point::new(0.0, 0.0),
            Point::new(30.0, 4.0),
            Point::new(60.0, 0.0),
        ],
        6.0,
        [1.0; 4],
    );
    assert_eq!(writer.count, 12);
    assert_eq!(v[2], v[6]);
    assert_eq!(v[5], v[7]);
}
#[test]
fn raster_union_and_additive_rgba8_layer() {
    let mut vertices = vec![Vertex::default(); 12];
    let mut w = mesh::Writer {
        vertices: &mut vertices,
        count: 0,
    };
    w.circles(
        [Point::new(10.0, 10.0), Point::new(12.0, 10.0)],
        [8.0, 8.0],
        [1.0, 0.25, 0.0, 0.6],
    );
    w.circles(
        [Point::new(10.0, 10.0); 2],
        [3.0, 3.0],
        [0.0, 0.0, 1.0, 0.6],
    );
    let mut pixels = vec![0; 20 * 20 * 4];
    raster::raster(&vertices, 20.0, 20.0, 20, 20, &mut pixels);
    assert_eq!(
        &pixels[(10 * 20 + 10) * 4..(10 * 20 + 10) * 4 + 4],
        &[153, 38, 153, 255]
    );
    assert_eq!(
        &pixels[(10 * 20 + 16) * 4..(10 * 20 + 16) * 4 + 4],
        &[153, 38, 0, 153]
    );
}

#[test]
fn single_stroke_alpha_at_folded_ribbon_join() {
    let c = config(RIBBONS);
    let r = request();
    let steps = mesh::steps(RIBBONS, r.width, r.height, c.scale);
    let mut points = vec![Point::default(); steps * 2 + 1];
    let mut vertices = vec![Vertex::default(); steps * 12];
    let mut index = 0;
    ribbons::paths(&c, 5.52, r.width, r.height, |path| {
        if index == 5 {
            // Ribbon index 2's isolated glow: a Canvas stroke fills once even
            // where the wide inner offset folds over several nearby segments.
            for (i, p) in points.iter_mut().enumerate() {
                *p = if i <= steps {
                    mesh::cubic(&path.points[..4], i as f64 / steps as f64)
                } else {
                    mesh::cubic(&path.points[3..], (i - steps) as f64 / steps as f64)
                };
            }
            let mut writer = mesh::Writer {
                vertices: &mut vertices,
                count: 0,
            };
            writer.stroke(&points, path.width, color(&c, path.color, path.alpha));
        }
        index += 1;
    });
    let mut pixels = vec![0; 3440 * 1440 * 4];
    raster::raster(&vertices, r.width, r.height, 3440, 1440, &mut pixels);
    assert_eq!(pixels.chunks_exact(4).map(|p| p[3]).max(), Some(20));
}
