use super::world::WorldInput;
use crate::visuals::palette::color;

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Silk {
    pub front: [f32; 4],
    pub back: [f32; 4],
    pub halo: [f32; 4],
    pub accent: [f32; 4],
}

// Each tuple indexes the shipped palette, preserving its RGB values on disk.
const PAIRS: [[(usize, usize); 6]; 6] = [
    [(1, 2), (2, 3), (4, 1), (5, 3), (1, 2), (4, 1)],
    [(0, 4), (1, 5), (2, 3), (4, 0), (5, 1), (3, 2)],
    [(1, 3), (2, 4), (1, 4), (2, 3), (1, 3), (2, 4)],
    [(1, 0), (2, 4), (1, 4), (2, 0), (1, 0), (2, 4)],
    [(0, 2), (4, 3), (0, 3), (4, 2), (0, 2), (4, 3)],
    [(0, 4), (1, 5), (2, 1), (3, 0), (0, 4), (1, 5)],
];
pub(super) fn adjust_oklab(rgb: [f32; 4], chroma: f32, light_scale: f32) -> [f32; 4] {
    let linear = |s: f32| {
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(rgb[0]), linear(rgb[1]), linear(rgb[2]));
    let l = (0.41222146 * r + 0.53633255 * g + 0.051445995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.10739696 * b).cbrt();
    let s = (0.08830246 * r + 0.28171884 * g + 0.629_978_7 * b).cbrt();
    let light = (0.21045426 * l + 0.7936178 * m - 0.004072047 * s) * light_scale;
    let a = (1.9779985 * l - 2.4285922 * m + 0.4505937 * s) * chroma;
    let b = (0.025904037 * l + 0.78277177 * m - 0.80867577 * s) * chroma;
    let l = (light + 0.39633778 * a + 0.21580376 * b).powi(3);
    let m = (light - 0.105561346 * a - 0.06385417 * b).powi(3);
    let s = (light - 0.08948418 * a - 1.2914855 * b).powi(3);
    let srgb = |v: f32| {
        if v <= 0.0031308 {
            (12.92 * v).max(0.)
        } else {
            (1.055 * v.powf(1. / 2.4) - 0.055).min(1.)
        }
    };
    [
        srgb(4.0767417 * l - 3.3077116 * m + 0.23096994 * s),
        srgb(-1.268438 * l + 2.6097574 * m - 0.3413194 * s),
        srgb(-0.0041960863 * l - 0.7034186 * m + 1.7076147 * s),
        1.,
    ]
}
pub(super) struct Art(pub [[Silk; 6]; 3]);
impl Art {
    pub fn new(input: &WorldInput) -> Self {
        let palette = input.palette as usize;
        Self(std::array::from_fn(|layer| {
            std::array::from_fn(|i| {
                let (a, b) = match (palette, layer) {
                    (0, 0) => (2, 3),
                    (2, 0) => (4, 3),
                    (3, 0) => (3, 2),
                    _ => PAIRS[palette][i],
                };
                let face = |index| {
                    let mut c = color(input.palette, index);
                    if palette == 5 && input.environment.background == 0 {
                        c = adjust_oklab(c, 1.75, 1.);
                    }
                    if layer == 0 {
                        let gray = 0.21 * c[0] + 0.72 * c[1] + 0.07 * c[2];
                        for v in &mut c[..3] {
                            let fade = if palette == 1 { 0.05 } else { 0.20 };
                            *v += fade * (gray - *v);
                        }
                    }
                    if input.environment.background == 2 && palette == 0 {
                        // Lift dark indigos against the blue background.
                        let y = 0.21 * c[0] + 0.72 * c[1] + 0.07 * c[2];
                        let k = (0.59 - y).max(0.) / (1. - y).max(0.01);
                        for v in &mut c[..3] {
                            *v += (1. - *v) * k;
                        }
                    }
                    c
                };
                let (front, back) = (face(a), face(b));
                let glow = match palette {
                    2 => color(2, 3),
                    3 => color(3, 2),
                    4 => color(4, 1),
                    _ => front,
                };
                let mut halo = adjust_oklab(glow, if palette == 5 { 1.4 } else { 1.25 }, 0.85);
                halo[3] = input.trails as f32;
                let mut front = front;
                front[3] = [
                    if palette == 3 {
                        0.63
                    } else if palette == 1 {
                        0.54
                    } else {
                        0.45
                    },
                    0.90,
                    0.85,
                ][layer]
                    * if palette == 5 {
                        // Lift the emitted core, retaining its chromaticity.
                        // Peak highlights take the shader's soft knee instead
                        // of dimming every isolated face by 20 percent.
                        if input.environment.background == 0 {
                            1.16
                        } else {
                            0.8
                        }
                    } else {
                        1.
                    };
                let mut back = back;
                back[3] = layer as f32;
                let mut accent = if palette == 5 && input.environment.background == 0 {
                    // Pastel highlights retain the face's hue instead of
                    // blending every ribbon toward the same pale pink.
                    let mut c = front;
                    c[3] = -1.; // owned shader's soft highlight knee
                    c
                } else if palette == 1 {
                    [1.; 4]
                } else {
                    color(input.palette, 0)
                };
                // Positive shader metadata controls how much light an overlap
                // retains. Pastel keeps its existing negative highlight marker.
                if palette == 2 {
                    accent[3] = 0.10;
                } else if palette == 0 {
                    accent[3] = 0.25;
                }
                if palette == 2 {
                    // Keep the shipped warm Ember atmosphere as a palette choice.
                    halo[1] = halo[0] * 0.4202;
                    halo[2] = halo[0] * 0.3562;
                }
                Silk {
                    front,
                    back,
                    halo,
                    accent,
                }
            })
        }))
    }
}
