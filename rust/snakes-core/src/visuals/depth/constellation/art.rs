use super::super::{
    common::rng::Pcg32,
    world::{DataTexture, WorldInput},
};
use super::space::*;
use crate::visuals::palette::color;
use std::sync::Arc;
pub type Rgb = [f32; 3];
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
pub struct Art {
    pub stars: [Rgb; 4],
    pub lines: Rgb,
    pub hues: [Rgb; 6],
    pub haze: [Rgb; 2],
    pub accent: Rgb,
    pub forest_on_plum: bool,
    pub line_alpha: f32,
    pub hue_alpha: [f32; 6],
    pub dust: Vec<(Point, f32, f32)>,
    pub band: Option<Arc<DataTexture>>,
    pub band_shift: Point,
    pub tone: f32,
    pub scale: f32,
    pub twinkle: f32,
    pub spectrum: bool,
    pub horizon: f32,
    pub floor: f32,
    pub sun: Point,
}
fn luminance(rgb: Rgb) -> f32 {
    let linear = rgb.map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}
impl Art {
    pub fn new(input: &WorldInput, params: &super::Params, area: f32) -> Self {
        let b = input.layout.bounds;
        let mut rng = Pcg32::new(input.seed, 0xbad5);
        let hues = std::array::from_fn(|i| {
            let c = color(input.palette, i);
            [c[0], c[1], c[2]]
        });
        let (roles, line, haze, accent) = match input.palette {
            1 => ([1, 2, 4, 0], 1, [4, 3], 1),
            2 => ([0, 5, 1, 2], 1, [4, 2], 0),
            3 => ([0, 4, 1, 2], 5, [3, 2], 0),
            4 => ([0, 4, 1, 2], 2, [3, 5], 0),
            5 => ([1, 5, 4, 0], 4, [0, 4], 2),
            _ => (
                [0, 5, 1, 3],
                if input.environment.background == 2 {
                    0
                } else {
                    5
                },
                [2, 4],
                0,
            ),
        };
        let line = hues[line];
        let contrast_alpha = |colour| {
            let mut alpha = 0.34 + 0.11 * input.trails as f32;
            for bg in [input.environment.top, input.environment.bottom] {
                let bg = [bg[0], bg[1], bg[2]];
                while alpha < 1.
                    && (luminance(mix(bg, colour, alpha)) + 0.05) / (luminance(bg) + 0.05) < 3.
                {
                    alpha = (alpha + 0.01).min(1.);
                }
            }
            alpha
        };
        let line_alpha = contrast_alpha(line);
        let mut hue_alpha = if input.palette == 1 {
            hues.map(contrast_alpha)
        } else {
            [line_alpha; 6]
        };
        if input.palette == 1 {
            hue_alpha[3] = hue_alpha[3].max(0.45);
        }
        let tone = if input.environment.background == 0 {
            1.
        } else {
            0.7
        };
        let theta = rand(&mut rng, 20., 31.).to_radians() * if rng.unit() < 0.5 { -1. } else { 1. };
        let band_shift = pt(theta.cos(), theta.sin());
        let strength = params.milkyWay as f32 / 100.;
        let aspect = (b.width / b.height) as f32;
        let panorama = input.mode == super::super::world::MonitorMode::Spanning
            && input.layout.outputs.len() > 1;
        let intercept = if panorama {
            0.51
        } else if rng.unit() < 0.5 {
            0.34
        } else {
            0.66
        };
        let bend = rand(&mut rng, 0., std::f32::consts::TAU);
        let center = |u: f32| {
            let diagonal = (u - 0.5) * theta.tan() * aspect;
            let arc = if panorama {
                0.10 * (diagonal / 0.27).tanh()
            } else {
                diagonal
            };
            intercept + arc + 0.023 * (u * aspect * 3.1 + bend).sin()
        };
        let haze = match input.palette {
            2 => [mix(hues[2], hues[4], 0.20), hues[2]],
            3 => [mix(hues[3], hues[2], 0.30), mix(hues[3], hues[2], 0.42)],
            _ => haze.map(|i| hues[i]),
        };
        let base = ((150. + (input.density as f32 - 10.) * 5.) * area).clamp(20., 4500.) as usize;
        let specks = (area * (1500. + 1000. * strength)).clamp(0., 24000.) as usize;
        let mut dust = Vec::with_capacity(base + specks);
        for i in 0..base + if strength > 0. { specks } else { 0 } {
            let in_band = strength > 0. && (i >= base || rng.unit() < 0.65);
            let mut a = input.layout.choose_visible_anchor(&mut rng, 0.).unwrap();
            if in_band {
                for _ in 0..10 {
                    let x = (a.x - b.x) as f32 / b.width as f32;
                    let scatter = rng.unit() + rng.unit() + rng.unit() - 1.5;
                    let y = b.y
                        + b.height * (center(x) as f64 + scatter * (0.12 + 0.16 * strength) as f64);
                    let q = super::super::common::layout::Vec2 { x: a.x, y };
                    if input.layout.contains(q) {
                        a = q;
                        break;
                    }
                    a = input.layout.choose_visible_anchor(&mut rng, 0.).unwrap();
                }
            }
            let (r, alpha) = if i < base {
                (rand(&mut rng, 0.42, 1.03), rand(&mut rng, 0.15, 0.48))
            } else {
                (
                    rand(&mut rng, 0.31, 0.79),
                    rand(&mut rng, 0.12, 0.49) * strength.sqrt(),
                )
            };
            dust.push((pt(a.x as f32, a.y as f32), r, alpha));
        }
        let band = if strength > 0. {
            let (w, h) = (1024, 192);
            let mut rgba = vec![0; w * h * 4];
            // Tileable noise lattices are tiny. All fractal sampling happens at bake time.
            let noise: Vec<_> = (0..128 * 64).map(|_| rng.unit() as f32).collect();
            let field = |x: f32, y: f32| {
                let (i, j) = (x.floor() as i32, y.floor() as i32);
                let (u, v) = (smooth(x - x.floor()), smooth(y - y.floor()));
                let get = |i: i32, j: i32| noise[((i & 127) + (j & 63) * 128) as usize];
                let a = get(i, j);
                let b = get(i, j + 1);
                let lo = a + (get(i + 1, j) - a) * u;
                lo + (b + (get(i + 1, j + 1) - b) * u - lo) * v
            };
            let centers: Vec<_> = (0..w).map(|x| center(x as f32 / w as f32)).collect();
            for y in 0..h {
                for (x, &center) in centers.iter().enumerate() {
                    let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                    let sx = u * aspect;
                    let n = field(sx * 7., v * 7.);
                    let d = (v - center) / (0.11 + strength * 0.12);
                    if d.abs() > 2. {
                        continue;
                    }
                    let clumps = 0.23 * n
                        + 0.22 * field(sx * 17., v * 17.)
                        + 0.35 * field(sx * 45., v * 45.)
                        + 0.20 * field(sx * 83., v * 83.);
                    let lane = (d + (n - 0.5) * 0.8) * 4.4;
                    let opacity = (-1.9 * d * d).exp()
                        * (1. - 0.8 * (-lane * lane).exp())
                        * clumps.powi(2)
                        * strength
                        * tone;
                    let c = mix(haze[0], haze[1], smooth((clumps - 0.35) * 2.4));
                    let k = (y * w + x) * 4;
                    for i in 0..3 {
                        rgba[k + i] = (c[i] * 255.) as u8
                    }
                    let fleck = if rng.unit() < 0.025 { 0.14 } else { 0. };
                    // High-pass the seeded noise tile before quantising the band.
                    let dither = 0.5
                        * (noise[(x % 128) + (y % 64) * 128]
                            - noise[((x + 1) % 128) + (y % 64) * 128])
                            .signum();
                    rgba[k + 3] = (opacity * (0.56 + fleck) * 255. + dither).round() as u8;
                }
            }
            Some(Arc::new(DataTexture {
                revision: 1,
                width: w as u32,
                height: h as u32,
                rgba: rgba.into(),
            }))
        } else {
            None
        };
        Self {
            stars: roles.map(|i| hues[i]),
            lines: line,
            hues,
            haze,
            accent: hues[accent],
            forest_on_plum: input.palette == 3 && input.environment.background == 3,
            line_alpha,
            hue_alpha,
            dust,
            band,
            band_shift,
            tone,
            scale: input.scale as f32,
            twinkle: params.twinkle as f32 / 100.,
            spectrum: input.palette == 1,
            horizon: b.height as f32,
            floor: b.y as f32,
            sun: pt(
                (b.right() + 0.6 * b.width) as f32,
                (b.y - 1.7 * b.height) as f32,
            ),
        }
    }
}
