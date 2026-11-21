use crate::visuals::palette::color;
// Ordered neighbours from the released palette. One 256-entry OKLab lookup
// table per activation also keeps the fragment shader free of texture reads.
pub struct Ramp {
    table: [u32; 256],
}
const ORDER: [[usize; 6]; 6] = [
    [0, 5, 1, 4, 2, 3],
    [0, 5, 1, 2, 3, 4],
    [4, 3, 2, 1, 0, 5],
    [3, 2, 5, 1, 4, 0],
    [3, 2, 5, 1, 4, 0],
    [0, 3, 2, 5, 1, 4],
];
fn lab(c: [f32; 4]) -> [f64; 3] {
    let [r, g, b, _] = c.map(|v| {
        let v = f64::from(v);
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.41222147 * r + 0.53633254 * g + 0.05144599 * b).cbrt();
    let m = (0.2119035 * r + 0.6806995 * g + 0.107397 * b).cbrt();
    let s = (0.08830246 * r + 0.28171884 * g + 0.6299787 * b).cbrt();
    [
        0.21045426 * l + 0.79361779 * m - 0.00407205 * s,
        1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
        0.025904 * l + 0.7827718 * m - 0.8086758 * s,
    ]
}
fn rgb([l, a, b]: [f64; 3]) -> u32 {
    let l1 = (l + 0.3963378 * a + 0.21580376 * b).powi(3);
    let m = (l - 0.10556135 * a - 0.06385417 * b).powi(3);
    let s = (l - 0.0894842 * a - 1.2914855 * b).powi(3);
    let enc = |v: f64| {
        let q = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (q.clamp(0., 1.) * 255.).round() as u32
    };
    (enc(4.0767417 * l1 - 3.3077116 * m + 0.2309699 * s) << 16)
        | (enc(-1.268438 * l1 + 2.6097574 * m - 0.3413194 * s) << 8)
        | enc(-0.0041961 * l1 - 0.7034186 * m + 1.7076147 * s)
}
impl Ramp {
    pub fn new(palette: u32) -> Self {
        let stops = ORDER[palette as usize].map(|i| lab(color(palette, i)));
        Self {
            table: std::array::from_fn(|i| {
                let t = i as f64 / 256.;
                let x = if (2..=4).contains(&palette) {
                    (1. - (2. * t - 1.).abs()) * 5.
                } else {
                    t * 6.
                };
                let a = stops[x as usize];
                let b = stops[(x as usize + 1) % 6];
                let mut c: [f64; 3] = std::array::from_fn(|j| a[j] + (b[j] - a[j]) * x.fract());
                if palette == 5 {
                    // Keep the soap film in the pastel range through interpolation.
                    let chroma = c[1].hypot(c[2]).max(0.001);
                    let gain = 0.112 / chroma;
                    c[1] *= gain;
                    c[2] *= gain;
                    c[0] = c[0].clamp(0.82, 0.875);
                }
                rgb(c)
            }),
        }
    }
    // A dim shell glows in the neighbouring cool or red pigment. Gold and
    // ice remain accents at the luminous centre instead of darkening to ochre.
    pub fn pigment(palette: u32, h: f64, z: f64, energy: f64) -> f64 {
        if energy >= 0.4 && (0.30..=0.85).contains(&z) {
            return h;
        }
        match palette {
            0 => 0.53 + 0.35 * h,
            1 if (0.16..0.49).contains(&h) => {
                if h < 0.34 {
                    0.035
                } else {
                    0.56
                }
            }
            2 => 0.045 + 0.14 * (1. - (2. * h - 1.).abs()),
            3 => 0.12 + 0.16 * h,
            5 => 0.50 + 0.36 * h,
            _ => h,
        }
    }
    pub fn at(&self, h: f64) -> f32 {
        self.table[((h.rem_euclid(1.) * 256.) as usize).min(255)] as f32
    }
}
