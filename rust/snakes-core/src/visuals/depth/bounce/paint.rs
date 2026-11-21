//! Static sRGB pigments. Lift dark enamel along its OkLCh hue on black,
//! maximizing chroma in gamut for saturated pigments and preserving neutrals.
fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn encoded(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1. / 2.4) - 0.055
    }
}
fn luminance(rgb: [f64; 3]) -> f64 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}
fn from_oklab(l: f64, a: f64, b: f64) -> [f64; 3] {
    let x = (l + 0.3963378 * a + 0.2158038 * b).powi(3);
    let y = (l - 0.1055613 * a - 0.0638542 * b).powi(3);
    let z = (l - 0.0894842 * a - 1.2914855 * b).powi(3);
    [
        4.0767417 * x - 3.3077116 * y + 0.2309699 * z,
        -1.2684380 * x + 2.6097574 * y - 0.3413194 * z,
        -0.0041961 * x - 0.7034186 * y + 1.7076147 * z,
    ]
}
fn gamut(l: f64, a: f64, b: f64) -> [f64; 3] {
    let mut lo = 0.;
    let mut hi = 1.;
    for _ in 0..14 {
        let mid = (lo + hi) * 0.5;
        if from_oklab(l, mid * a, mid * b)
            .iter()
            .all(|v| (0.0..=1.).contains(v))
        {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    from_oklab(l, lo * a, lo * b)
}
pub(super) fn packed(colour: [f32; 4], black: bool) -> f32 {
    let srgb = [colour[0], colour[1], colour[2]].map(f64::from);
    let [r, g, b] = srgb.map(linear);
    let dye = if !black || luminance([r, g, b]) >= 0.30 {
        srgb
    } else {
        let x = (0.41222147 * r + 0.53633254 * g + 0.05144599 * b).cbrt();
        let y = (0.21190350 * r + 0.68069955 * g + 0.10739696 * b).cbrt();
        let z = (0.08830246 * r + 0.28171884 * g + 0.62997870 * b).cbrt();
        let a = 1.9779985 * x - 2.4285922 * y + 0.4505937 * z;
        let b = 0.0259040 * x + 0.7827718 * y - 0.8086758 * z;
        let c = a.hypot(b);
        let (a, b) = if c > 0.10 {
            (0.4 * a / c, 0.4 * b / c)
        } else {
            (a, b)
        };
        let mut lo = 0.2104543 * x + 0.7936178 * y - 0.0040721 * z;
        let mut hi = 1.;
        for _ in 0..18 {
            let mid = (lo + hi) * 0.5;
            if luminance(gamut(mid, a, b)) < 0.302 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        gamut(hi, a, b).map(encoded)
    };
    let [r, g, b] = dye.map(|c| (255. * c.clamp(0., 1.)).round() as u32);
    ((r << 16) | (g << 8) | b) as f32
}
