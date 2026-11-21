#version 440

layout(location = 0) in vec2 local;
layout(location = 1) in vec4 lobe;
layout(location = 2) in vec4 tone;
layout(location = 3) in vec4 film;
layout(location = 4) in vec4 shape;
layout(location = 5) in vec2 optics;
layout(location = 6) in vec4 filmArc;
layout(location = 0) out vec4 frag;
layout(std140, binding = 0) uniform buf {
    mat4 matrix;
    vec4 info;
    vec4 style;
    vec4 view;
    vec4 mapping;
} u;

float value(vec3 c) { return max(c.r, max(c.g, c.b)); }
vec3 unitColour(vec3 c) { return c / max(value(c), 0.001); }

vec3 pastelFilm(float t)
{
    float hue = filmArc.z + t * filmArc.w;
    float L = mix(filmArc.x, filmArc.y, t);
    vec2 ab = 0.122 * vec2(cos(hue),sin(hue));
    vec3 lms = vec3(L) + vec3(dot(ab,vec2(0.3963378,0.2158038)),
                              dot(ab,vec2(-0.1055613,-0.0638542)),
                              dot(ab,vec2(-0.0894842,-1.2914855)));
    lms *= lms*lms;
    vec3 rgb = vec3(dot(lms,vec3(4.0767417,-3.3077116,0.2309699)),
                    dot(lms,vec3(-1.2684380,2.6097574,-0.3413194)),
                    dot(lms,vec3(-0.0041961,-0.7034186,1.7076147)));
    return clamp(1.055 * pow(max(rgb,vec3(0.0)),vec3(1.0/2.4))-0.055,0.0,1.0);
}

void main()
{
    float radius = max(shape.w, 0.01);
    vec2 p = local / radius;
    float d = length(p);
    vec2 n = p / max(d, 0.001);

    // Evaluate the angular modes without a polar coordinate conversion.
    float ripple = shape.x * (n.x * n.x - n.y * n.y)
                 + shape.y * (2.0 * n.x * n.y)
                 + shape.z * (4.0 * n.x * n.x * n.x - 3.0 * n.x);
    d /= 1.0 + ripple;
    if (lobe.w > 0.5 && lobe.w < 1.5) {
        float second = length(local - lobe.xy) / radius + 1.0 - lobe.z / radius;
        float neck = max(0.28 - abs(d - second), 0.0);
        d = min(d, second) - neck * neck / 1.12;
    }

    bool pastelBubble = u.mapping.w == 5.0 && u.style.x > 1.5 && lobe.w < 1.5;
    float glow = u.style.y;
    float aa = max(0.65 / (radius * u.info.w), 0.003);
    float blur = film.w + aa;
    float inside = 1.0 - smoothstep(1.0 - blur, 1.0 + blur, d);
    float rim = exp(-abs(d - 0.94) / (0.045 + 0.36 * blur)) * inside;
    float reach = 1.15 + 1.05 * glow + 0.08 * min(glow / 0.35, 1.0);
    float rimGain = min(glow / 0.35, 1.0) * (0.095 + 0.07 * glow);
    reach += 0.045 * (1.0 - smoothstep(0.35, 1.0, glow)) * min(glow / 0.35, 1.0);
    float uHalo = clamp((d - 1.0) / (reach - 1.0), 0.0, 1.0);
    float halo = rimGain * exp(-1.35 * uHalo) * (1.0 - smoothstep(0.65, 1.0, uHalo));

    vec3 rgb;
    float occlusion = 0.0;
    if (lobe.w > 1.5) {
        float light;
        if (lobe.w < 2.5) { // Mote
            light = (0.55 * exp(-(3.0 + 6.0 * (0.30 - film.w)) * d * d) + 0.25 * exp(-1.9 * d * d)) * (1.0 - smoothstep(0.65, 1.4, d));
        } else if (lobe.w < 3.5) { // Burst ring
            float sigma = mix(0.05, 0.15, smoothstep(0.0, 1.0, film.w));
            float delta = d - 1.0;
            light = 0.24 * exp(-0.5 * delta * delta / (sigma * sigma))
                + 0.06 * exp(-0.5 * delta * delta / 0.032) * smoothstep(0.0, 0.1, delta)
                + 0.10 * inside * (1.0 - smoothstep(0.0, 0.125, film.w));
        } else { // Wake
            light = 0.7 * exp(-3.0 * d * d) * (1.0 - smoothstep(1.0, 1.5, d));
        }
        rgb = tone.rgb * light;
    } else {
        vec2 lamp = p + vec2(0.20, 0.22);
        vec3 light = u.mapping.w == 2.0 ? tone.rgb : mix(tone.rgb, film.rgb, 0.24);
        float edge = 1.0 - smoothstep(0.0, 2.3 * aa + 0.66 * blur, abs(d - 0.955));
        float crescent = exp(-8.0 * abs(d - 0.80)) * smoothstep(0.49, 0.97, dot(n, vec2(-0.67, -0.742)));
        vec2 focus = p - vec2(0.37, 0.42);
        float spot = exp(-16.0 * dot(focus, focus));

        if (u.style.x < 0.5) { // Glow
            float caustic = (cos(8.0 * p.x + 2.3 * p.y + u.style.z) + sin(5.0 * p.y - 1.5 * p.x - u.style.z))
                          * 0.012 * (1.0 - smoothstep(0.05, 0.28, blur));
            vec3 pigment = unitColour(tone.rgb);
            vec3 shell = unitColour(film.rgb);
            // Amber cools toward pink, but mint cools toward blue-green. A
            // pink shell under a mint core crosses grey at half intensity.
            if (u.mapping.w == 1.0 && pigment.g > pigment.r && pigment.g > pigment.b)
                shell = vec3(0.17, 0.71, 0.85);
            float energy = optics.x;
            float pale = filmArc.x;
            float peak = mix(u.mapping.w == 1.0 ? 0.60 : 0.70, 0.855, pale);
            if (u.mapping.w == 0.0)
                pigment = mix(pigment, vec3(1.0), 0.25 * energy * smoothstep(0.48, 0.58, energy));
            bool amber = (u.mapping.w == 1.0 || u.mapping.w == 2.0) && tone.r > 0.8 && tone.g > 0.55*tone.r && tone.b < 0.7*tone.g;
            float fromLamp = length(lamp);
            float hot = (1.0 - smoothstep(amber ? 0.31 : 0.20, amber ? 0.98 : 0.74, fromLamp))
                * (1.0 - 0.065 * smoothstep(0.0, 0.48, fromLamp));
            float strength = mix(0.145 + caustic, peak, hot) + 0.050 * rim;
            // Along the falloff, first heat the red/cool shell, then illuminate
            // the pale pigment. Never attenuate an amber lamp into a brown disc.
            float emitting = strength * min(1.0, tone.a * mix(1.22, 1.55, pale));
            float litPigment = mix(smoothstep(0.18, 0.48, emitting),
                                   smoothstep(0.49, 0.75, emitting), pale);
            rgb = mix(shell, pigment, litPigment) * emitting * inside
                + shell * halo * optics.y * tone.a * (1.0 - 0.78 * inside);
            rgb += light * (0.018 * crescent + 0.012 * spot) * inside * tone.a;
        } else if (u.style.x < 1.5) { // Glass
            occlusion = 0.16 * inside;
            if (u.mapping.w == 2.0) {
                float defocus = optics.x * smoothstep(0.08, 0.33, film.w);
                float spread = 1.0 + 2.0 * defocus;
                crescent = exp(-8.0 * abs(d - 0.80) / spread)
                    * smoothstep(0.49 - 0.12 * defocus, 0.97, dot(n, vec2(-0.67, -0.742))) / spread;
                spot = exp(-16.0 * dot(focus, focus) / spread) / spread;
                float rimTransmission = mix(1.0, 0.37, defocus);
                rim *= rimTransmission;
                edge *= rimTransmission;
            }
            float refraction = 0.26 * rim + 0.50 * edge + 0.48 * crescent + 0.36 * spot;
            float bowl = exp(-5.2 * abs(d - 0.69)) * smoothstep(-0.28, 0.82, dot(n, vec2(0.46, 0.89)));
            rgb = film.rgb * (0.13 * inside + 0.25 * halo) + light * (refraction + 0.21 * bowl) * inside;
            if (u.mapping.w == 2.0 && tone.g > 0.53 * tone.r && tone.g > tone.b) {
                // A bright gold reflection sits on a red glass shell. Keep the
                // weak tail of its focus spot red until the filament is lit.
                float intensity = value(rgb) + 0.30 * smoothstep(0.22, 0.58, refraction) * inside;
                vec3 pigment = mix(unitColour(film.rgb), unitColour(tone.rgb),
                                   smoothstep(0.61, 0.67, intensity * tone.a));
                rgb = pigment * intensity;
            }
        } else { // Bubbles
            float c2 = n.x*n.x - n.y*n.y, s2 = 2.0*n.x*n.y;
            vec2 n4 = vec2(c2*c2 - s2*s2, 2.0*c2*s2);
            vec2 n5 = vec2(n4.x*n.x - n4.y*n.y, n4.x*n.y + n4.y*n.x);
            float bands = clamp(0.5 + 0.40 * dot(vec2(s2,c2), vec2(cos(u.style.z),sin(u.style.z)))
                                   + 0.16 * dot(n5.yx, vec2(cos(2.6*u.style.z),-sin(2.6*u.style.z))), 0.0, 1.0);
            vec3 tint = mix(tone.rgb, film.rgb, bands);
            float ribbon = exp(-abs(d - (0.865 + 0.025 * bands)) / (0.018 + 0.3 * blur));
            vec2 highlight = p + vec2(0.23, 0.56);
            float dotLight = exp(-210.0 * dot(highlight, highlight));
            if (pastelBubble) {
                // Film thickness varies continuously around the membrane.
                float phase = 0.5 + 0.34 * dot(vec2(s2, c2), vec2(cos(u.style.z), sin(u.style.z)))
                                  + 0.13 * dot(n5.yx, vec2(cos(2.6*u.style.z), -sin(2.6*u.style.z)));
                tint = pastelFilm(phase);
                float dist = abs(d - 0.955) * radius * u.info.w;
                float peakRim = exp(-0.5 * dist * dist / 6.2);
                vec3 cool = mix(vec3(0.35, 0.52, 0.94), vec3(0.75, 0.42, 0.86), phase);
                float thickness = (d - (0.865 + 0.025 * phase)) / (0.115 + 0.34 * blur);
                float ref = 0.24 * exp(-0.5 * thickness * thickness) + 0.17 * crescent;
                float warm = smoothstep(-0.28, 0.08, min(tint.r, tint.g) - tint.b);
                vec3 band = mix(mix(tint, vec3(1.0), 0.30), cool, warm);
                float pixelDist = length(highlight) * radius * u.info.w;
                float dotPeak = exp(-0.5 * pixelDist * pixelDist / 5.4);
                rgb = cool * 0.028 * inside + band * ref;
                rgb = mix(rgb, tint, peakRim);
                rgb = mix(rgb, mix(tint, vec3(1.0), 0.7), dotPeak);
            } else {
                rgb = tint * (0.022 * inside + 0.23 * rim + 0.64 * edge + 0.22 * ribbon + 0.039 * halo)
                    + mix(tone.rgb, film.rgb, 0.24) * (0.40 * crescent + 0.52 * dotLight) * inside;
            }
            occlusion = 0.035 * inside;
        }
    }
    // Soften single-surface highlights before the fixed-function blend.
    if (u.style.x >= 0.5 || lobe.w > 1.5)
        rgb *= pastelBubble
            ? min(1.0, 1.24 * tone.a) : tone.a;
    float peak = max(rgb.r, max(rgb.g, rgb.b));
    // A red halo tapers through ruby. On a black backdrop, a residual
    // two-code red, one-code green gradation would quantise to ochre.
    if (!pastelBubble && rgb.r > rgb.g && rgb.g > rgb.b)
        rgb.b = mix(rgb.g, rgb.b, smoothstep(0.022, 0.085, peak));
    if (peak > 0.84) {
        float shoulder = peak - 0.84;
        rgb *= (0.84 + shoulder / (1.0 + 8.0 * shoulder)) / peak;
    }
    float alpha = u.style.x < 0.5 ? 0.0 : min(1.0, max(occlusion * tone.a, 1.06 * value(rgb)));
    frag = vec4(rgb, alpha);

}
