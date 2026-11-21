#version 440

layout(location = 0) in vec2 bounds;
layout(location = 1) in vec2 index;
layout(location = 0) out vec2 local;
layout(location = 1) out vec4 lobe;
layout(location = 2) out vec4 tone;
layout(location = 3) out vec4 film;
layout(location = 4) out vec4 shape;
layout(location = 5) out vec2 optics;
layout(location = 6) out vec4 filmArc;

layout(std140, binding = 0) uniform buf {
    mat4 matrix;
    vec4 info;     // Opacity, rows in use, rows allocated, device scale.
    vec4 style;    // Material, glow, film phase, endpoint interpolation.
    vec4 view;     // Viewport size and origin in world space.
    vec4 mapping;  // Cover scale and offset.
} u;
layout(binding = 1) uniform sampler2D records;

vec3 unpack24(float value)
{
    return mod(floor(value / vec3(65536.0, 256.0, 1.0)), 256.0);
}

// The two film pigments come from neighbouring portions of the Pastel
// OKLab ramp. Carry their hue interval through the draw at constant chroma.
vec2 lightnessHue(vec3 srgb)
{
    vec3 c = mix(srgb / 12.92, pow((srgb + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), srgb));
    vec3 lms = pow(vec3(dot(c,vec3(0.41222147,0.53633254,0.05144599)),
                        dot(c,vec3(0.21190350,0.68069955,0.10739696)),
                        dot(c,vec3(0.08830246,0.28171884,0.62997870))), vec3(1.0/3.0));
    float a = dot(lms,vec3(1.9779985,-2.4285922,0.4505937));
    float b = dot(lms,vec3(0.0259040,0.7827718,-0.8086758));
    return vec2(dot(lms,vec3(0.21045426,0.79361779,-0.00407205)), atan(b,a));
}

void main()
{
    if (index.x >= u.info.y) {
        gl_Position = u.matrix * vec4(0, 0, 0, 1);
        local = vec2(0);
        lobe = tone = film = shape = vec4(0);
        optics = vec2(0);
        filmArc = vec4(0);
        return;
    }

    // Each row contains three RGBA32F texels per endpoint.
    float row = (index.x + 0.5) / u.info.z;
    vec4 body0 = texture(records, vec2(0.5 / 6.0, row));
    vec4 body1 = texture(records, vec2(3.5 / 6.0, row));
    vec4 lobe0 = texture(records, vec2(1.5 / 6.0, row));
    vec4 lobe1 = texture(records, vec2(4.5 / 6.0, row));
    vec4 surface0 = texture(records, vec2(2.5 / 6.0, row));
    vec4 surface1 = texture(records, vec2(5.5 / 6.0, row));

    float t = u.style.w;
    vec4 body = mix(body0, body1, t);
    lobe = mix(lobe0, lobe1, t);
    lobe.w = t < 0.5 ? lobe0.w : lobe1.w;
    tone = vec4(mix(unpack24(surface0.x), unpack24(surface1.x), t) / 255.0, body.w * u.info.x);
    film = vec4(mix(unpack24(surface0.y), unpack24(surface1.y), t) / 255.0,
                mix(mod(surface0.z, 2.0), mod(surface1.z, 2.0), t));
    vec2 encoded = floor(vec2(surface0.z, surface1.z) * 0.5);
    vec2 energies = floor(encoded / 32.0) / 31.0;
    vec2 halos = mod(encoded, 32.0) / 31.0;
    optics = vec2(mix(energies.x, energies.y, t), mix(halos.x, halos.y, t));
    filmArc = vec4(0.0);
    if (u.style.x < 0.5 && lobe.w < 1.5) {
        filmArc.x = smoothstep(0.74, 0.82, lightnessHue(tone.rgb).x);
    } else if (u.style.x > 1.5 && u.mapping.w == 5.0 && lobe.w < 1.5) {
        vec2 a = lightnessHue(tone.rgb), b = lightnessHue(film.rgb);
        filmArc = vec4(a.x,b.x,a.y,mod(b.y - a.y + 3.14159265,6.2831853) - 3.14159265);
    }
    vec3 wobble0 = surface0.w > 0.0 ? (unpack24(surface0.w) - 128.0) / 500.0 : vec3(0);
    vec3 wobble1 = surface1.w > 0.0 ? (unpack24(surface1.w) - 128.0) / 500.0 : vec3(0);
    shape = vec4(mix(wobble0, wobble1, t), body.z);

    float reach = 1.16 + film.w + 1.05 * u.style.y + 0.13 * min(1.0, u.style.y / 0.35);
    if (lobe.w > 1.5 && lobe.w < 2.5)
        reach = 1.0 + min(3.0 / body.z, 0.7);
    if (lobe.w > 2.5)
        reach = lobe.w < 3.5 ? 1.38 : 1.5;

    // The second lobe can fall on either side of the first, including across a seam.
    float margin = max(body.z, lobe.z) * reach + 1.5 / u.info.w;
    vec2 uv = bounds / u.view.xy;
    local = mix(min(vec2(0), lobe.xy) - margin, max(vec2(0), lobe.xy) + margin, uv);
    vec2 position = (body.xy + local - u.view.zw) * u.mapping.x + u.mapping.yz;
    gl_Position = u.matrix * vec4(position, 0, 1);
}
