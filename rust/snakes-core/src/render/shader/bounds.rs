// SPDX-License-Identifier: GPL-3.0-or-later
//! Literal GLSL constants are the single source of the CPU geometry contract.
//! This tiny decimal reader runs only at compile time; no build script, heap,
//! dependency, shader parsing or extra work is introduced per frame.
const SHADER: &str = include_str!("../../../../../src/shaders/snake.frag");
const fn value(name: &str) -> f64 {
    let source = SHADER.as_bytes();
    let key = name.as_bytes();
    let mut i = 0;
    while i + key.len() < source.len() {
        let mut n = 0;
        while n < key.len() && source[i + n] == key[n] { n += 1; }
        if n == key.len() && source[i + n] == b' ' {
            i += n;
            while source[i] == b' ' || source[i] == b'=' { i += 1; }
            let mut result = 0.0;
            let mut fraction = 0.0;
            while source[i] != b';' {
                let digit = source[i];
                if digit == b'.' { fraction = 1.0; }
                else {
                    assert!(digit >= b'0' && digit <= b'9', "bounds must be positive decimal literals");
                    if fraction == 0.0 { result = result * 10.0 + (digit - b'0') as f64; }
                    else { fraction *= 0.1; result += (digit - b'0') as f64 * fraction; }
                }
                i += 1;
            }
            return result;
        }
        i += 1;
    }
    panic!("missing shader bounds constant");
}

pub(in crate::render) const BODY: f64 = value("BOUNDS_BODY");
pub(super) const HEAD_BACK: f64 = value("BOUNDS_HEAD_BACK");
pub(super) const HEAD_BOOST_BACK: f64 = value("BOUNDS_HEAD_BOOST_BACK");
pub(super) const HEAD_FRONT: f64 = value("BOUNDS_HEAD_FRONT");
pub(super) const HEAD_SIDE: f64 = value("BOUNDS_HEAD_SIDE");
pub(super) const HEAD_BOOST_SIDE: f64 = value("BOUNDS_HEAD_BOOST_SIDE");
pub(super) const FOOD: f64 = value("BOUNDS_FOOD");
pub(super) const IMPACT: f64 = value("BOUNDS_IMPACT");
pub(super) const RING: f64 = value("BOUNDS_RING");
pub(in crate::render) const CAPSULE: f64 = value("BOUNDS_CAPSULE");
pub(super) const MAGNET: f64 = value("BOUNDS_MAGNET");
pub(super) const CONTRAIL: f64 = value("BOUNDS_CONTRAIL");
pub(super) const VACUUM: f64 = value("BOUNDS_VACUUM");
pub(super) const DEVELOPER: f64 = value("BOUNDS_DEVELOPER");
pub(super) const CORPSE_DRIFT: f64 = value("BOUNDS_CORPSE_DRIFT");
pub(in crate::render) const FROST_CRACK: f64 = value("BOUNDS_FROST_CRACK");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_fragment_support_fits_geometry_contract() {
        let aa = value("BOUNDS_AA");
        let head_aa = value("BOUNDS_HEAD_AA");
        let food_aa = value("BOUNDS_FOOD_AA");
        let breath = value("BOUNDS_BREATH");
        // Keep this independent oracle coupled to the expressions whose extrema
        // it derives. Editing their math requires updating the derivation too.
        for expression in [
            "falloff(abs(acrossR)/(2.4*w))", "1.2+7.0*progress*(0.6+hash(seed+float(k)*2.0)*0.6)",
            "2.2*(1.0-age)", "line(effectCoord,direction*start,direction*end,0.012)",
            "1.95-0.70*y*y", "0.286+2.352*y-1.238*y*y", "falloff(length(eye)/eyeRadius)",
            "1.30+1.32*extension", "abs(q.x)/0.85+abs(q.y)/1.6",
            "float sr=0.35+0.45*progress", "float grow=moving?1.0+0.16*sin",
            "abs(r-2.7)", "abs(r-1.75)", "mix(2.35,2.85,longRay)",
            "float shockRadius=1.75+2.4*pop", "min(hw+px,BOUNDS_FOOD-shockRadius)",
            "dot(p-a,d)/max(dot(d,d),0.000001)",
            "extent=tier==0?1.75:tier==1?1.85:tier==2?2.0:2.15",
            "aaD=max(fwidth(d),0.008)",
            "aaG=clamp(fwidth(gd),0.008,BOUNDS_FOOD_AA)",
            "1.0+(kind==9?2.2:5.0)*progress",
            "sx=(p.x-0.05)/1.37", "spade=1.10*sqrt", "float k=0.15",
            "shadow=mask(sd-0.13)", "abs(p.y)-0.56", "abs(cd)-0.035",
            "falloff(length(p-vec2(0.02,0.0))/1.0)",
            "vec2(1.30+1.32*extension,0.22*extension),0.045",
            "b1=mask(s1-0.08)", "b2=mask(s2-0.08)",
            "step(y,1.85)", "step(y,2.4)",
            "r-1.05", "r-1.0", "(dd-1.0)*0.75",
            "falloff(r/4.6)", "falloff(r/4.4)", "falloff(r/3.0)", "falloff(r/4.0)",
            "4.5*a",
            "px=clamp(length(vec2(dFdx(p.x),dFdy(p.x))),0.0001,BOUNDS_FOOD_PIXEL)", "return coverage(d,min(aa,limit))",
            "kind==1?BOUNDS_HEAD_AA:(kind==2||kind==3||kind==4||kind==8)?BOUNDS_FOOD_AA:BOUNDS_AA",
        ] { assert!(SHADER.contains(expression), "rederive shader support: {expression}"); }
        // The smallest ordinary taper is .22; rounded alpha may add .5/255.
        let body_wave = 2.4 * breath * (1.0 + 0.5 / 255.0 / 0.22);
        let body_halo = 2.15 * breath * (1.0 + 0.5 / 255.0 / 0.22);
        assert!(BODY >= body_wave.max(body_halo));
        // At tight bends / on corpse pieces, interpolated ribbonLimit caps
        // every term, including rounded alpha and breathing, to the extrusion.
        assert!(SHADER.contains("edge=ribbonLimit*BOUNDS_BODY"));
        assert!(SHADER.contains("edgeFade=edge>0.0?1.0-smoothstep(max(0.0,edge-BOUNDS_AA),edge,abs(acrossR)):0.0"));
        let age = 14.6 / 16.8;
        let spark_end = 1.2 + 8.4 * (2.0 * age - age * age) + 2.2 * (1.0 - age);
        let spark_extent = spark_end + (0.012 + aa) * value("BOUNDS_EFFECT_UNITS");
        assert!(IMPACT >= spark_extent);
        assert!(RING >= 6.0 + (0.012 + aa) * value("BOUNDS_EFFECT_UNITS"));
        // Magnet reach 9r plus the compact 1.1r spark halo; angular dashes
        // have no extra radial support. Rim/scan lights remain inside BODY.
        for expression in ["mask(abs(radius-9.0)-0.08)", "falloff(spark/1.1)",
            "vec2(cos(a),sin(a))*9.0", "1.05+0.35*hash",
            "vec2(b,(edge+1.0)*SEG),0.6*pixelR)",
            "abs(abs(across)-1.2)-0.06", "scan=coverage("] {
            assert!(SHADER.contains(expression), "rederive active support: {expression}");
        }
        // Capsule birth ring includes its stroke + bounded AA; the halo is
        // compact at CAPSULE and the orbit spark ends at 1.3 + .5 radii.
        for expression in ["coord*BOUNDS_CAPSULE", "abs(length(p)-(3.0-2.0*birth))-0.035",
            "falloff(length(p)/BOUNDS_CAPSULE)", "vec2(cos(orbit),sin(orbit))*1.3)/0.5",
            "abs(h)-0.035", "abs(hexagon(q,0.675))-0.015",
            "kind==12?1.0+5.0", "abs(r-radius)-0.08", "abs(r-radius*0.65)-0.055"] {
            assert!(SHADER.contains(expression), "rederive item/effect support: {expression}");
        }
        assert!(CAPSULE >= 3.0+0.035+aa);
        assert!(CAPSULE >= 1.3+0.5);
        assert!(CAPSULE >= 1.0+(0.035+aa)/0.8660254);
        assert!(RING >= 6.0+0.08+aa);
        assert!(MAGNET >= (9.0+0.08+aa).max(9.0+1.1));
        // Arc centres remain <=1.4w. Their 1.2px stroke and derivative AA
        // are compacted by the existing ribbon edgeFade even at tiny radii.
        assert!(BODY >= 1.4*breath*(1.0+0.5/255.0/0.22));
        // Body derivative AA and curvature are compacted by edgeFade above.
        assert!(SHADER.contains("fragColor=composite(rgb,alpha,glow,over,corpse?packed.w/255.0:activeFade)*edgeFade"));

        assert!(SHADER.contains("smoothstep(6.0,BOUNDS_RING,length(coord))"));

        assert!(HEAD_FRONT >= 1.30 + 1.32 + 0.045 + head_aa); // tongue capsule + AA
        // Frozen head-local shiver: its glow radius is shortened by .05.
        assert!(HEAD_SIDE >= 0.56 + 1.1 + 0.05);
        // Sweat's tear uses a tighter AA ceiling and stays in the head quad.
        assert!(HEAD_SIDE >= 1.37 + 2.0*(0.025 + 0.12) + 0.08);
        // Frost breath: the third puff's centre and radius peak together at e=1;
        // its soft profile is exactly zero at u=1, so no AA term.
        for expression in ["vec2(1.58+(0.20+0.34*fi)*(0.55+0.45*e)", "((0.13+0.07*fi)*(0.85+0.45*e))", "for(int i=0;i<3;i++)", "(1.0-smoothstep(0.45,1.0,u))"] {
            assert!(SHADER.contains(expression), "rederive frost breath: {expression}");
        }
        assert!(HEAD_FRONT >= 1.58 + (0.20 + 0.34*2.0) + (0.13 + 0.07*2.0)*(0.85 + 0.45));
        assert!(HEAD_SIDE >= 0.07*2.0 + (0.13 + 0.07*2.0)*(0.85 + 0.45));
        // Thaw shards: start 1.2 + 3.4*burst*(<=1.2), length 1.2*(1-age), burst cubic.
        for expression in ["float start=1.2+3.4*burst*(0.6+0.6*hash(", "float length_=1.2*(1.0-age)",
            "float burst=1.0-pow(1.0-age,3.0)", "abs(r-(1.3+1.2*burst))", "falloff(r/1.9)"] {
            assert!(SHADER.contains(expression), "rederive thaw crack: {expression}");
        }
        let shard_end = (0..=1000).map(|j| { let u = j as f64 / 1000.0; 1.2 + 4.08*(1.0 - u*u*u) + 1.2*u })
            .fold(0.0_f64, f64::max);
        assert!(FROST_CRACK >= shard_end + 0.3); // + 1.6px taper/AA at r >= 6px
        assert!(HEAD_FRONT >= 1.08 + 0.34 + 0.02 + head_aa); // yawn
        assert!(CAPSULE >= 2.8 + 0.45 + 0.04 + aa); // outside crosshair ticks
        assert!(SHADER.contains("clamp(fwidth(dropDistance),0.008,0.08)"));
        // Contest halo: max half-stroke 2.2*1.15px, support 3*half+2.
        // Rust always reserves >=12px beyond the 1.62-radius ring.
        assert!(12.0 >= 3.0*(1.0+1.2)*1.15+2.0);
        assert!(SHADER.contains("dr/((3.0*halfStroke+2.0)*px)"));
        assert!(include_str!("../faces.rs").contains("(radius*3.4).max(radius*1.62+12.0)"));
        assert!(HEAD_SIDE >= 0.56 + 1.15); // both eye glows, incl. flare
        assert!(HEAD_BACK >= 1.0); // explicit neck fade
        assert!(HEAD_BACK >= 0.60 + 0.035 + head_aa); // crown outline
        assert!(HEAD_SIDE >= 0.8 + 0.035 + head_aa);
        assert!(HEAD_FRONT >= 0.02 + 1.0); // crown additive glow
        assert!(HEAD_SIDE >= (2.15_f64.max(1.42 + food_aa)) * 0.84 / 1.14); // head halo, max tier
        assert!(HEAD_FRONT >= 0.05 + 1.37 * (1.0 + (0.13 + head_aa) / 1.10));
        assert!(HEAD_SIDE >= 1.10 + 0.15 / 4.0 + 0.13 + head_aa); // neck smooth union
        // The normalized polynomial distance has an x support widened by
        // sqrt(1+slope^2). Sample both closed intervals, including endpoints.
        for j in 0..=10000 {
            let y = 0.3 + (1.85 - 0.3) * j as f64 / 10000.0;
            let x = 1.95 - 0.70 * y * y;
            let dx = (0.08 + head_aa) * (1.0 + 1.96 * y * y).sqrt();
            assert!(x + dx <= HEAD_FRONT && x - dx >= -HEAD_BOOST_BACK);
            let y = 1.0 + 1.4 * j as f64 / 10000.0;
            let x = 0.286 + 2.352 * y - 1.238 * y * y;
            let dx = (0.08 + head_aa) * (1.0 + (2.352 - 2.476 * y).powi(2)).sqrt();
            assert!(x + dx <= HEAD_FRONT && x - dx >= -HEAD_BOOST_BACK);
        }
        assert!(HEAD_BOOST_SIDE >= 2.4); // hard bow y gate; no AA beyond it
        let px = value("BOUNDS_FOOD_PIXEL");
        assert!(FOOD >= 4.6); // largest halo, pulse changes amplitude only
        assert!(FOOD >= 4.5); // spark twinkle cross has a hard length gate
        assert!(FOOD >= 1.6 * (1.0 + food_aa / 0.75)); // rotated shard rhombus
        assert!(FOOD >= 1.05 + food_aa); // spark/pellet/prism discs
        assert!(FOOD >= 1.16 + food_aa); // prism orb, including overshoot
        assert!(FOOD >= 2.7 + 0.10_f64.max(0.9*px) + px); // seed track
        assert!(FOOD >= 2.7 + 0.17_f64.max(1.1*px) + food_aa); // fuse core
        assert!(FOOD >= 2.7 + 0.9); // fuse halo
        assert!(FOOD >= 1.75 + 0.11_f64.max(0.9*px) + px); // ripe halo
        assert!(FOOD >= 2.85 + 0.07_f64.max(0.6*px) + food_aa); // dispersion rays
        assert!(FOOD >= 1.75 + 2.4); // shock outer stroke/AA compacted above
        // Streaks / contrails have compact transverse support, and vacuum /
        // steering streaks stop at their endpoints, without pixel-sized AA.
        assert!(SHADER.contains("streak=max(0.0,1.0-abs(coord.x))"));
        assert!(SHADER.contains("(1.0-clamp(coord.y,0.0,1.0))*step(0.0,coord.y)"));
        let vertex = include_str!("../../../../../src/shaders/snake.vert");
        assert!(vertex.contains("ribbonLimit=abs(uv.x)"));
    }
}
