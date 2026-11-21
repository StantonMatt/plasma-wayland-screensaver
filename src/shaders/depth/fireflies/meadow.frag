#version 440
layout(std140, binding = 0) uniform Meadow
{
    mat4 matrix;
    vec4 view;
    vec4 bounds;
    ivec4 counts;
    ivec4 rows;
    vec4 art;
    vec4 colours[8];
    vec4 bottom;
    vec4 limits;
}
u;
layout(location = 0) in vec2 local;
layout(location = 1) in vec4 ink;
layout(location = 2) flat in vec4 shape;
layout(location = 3) flat in vec4 details;
layout(location = 4) in vec2 fromHead;
layout(location = 5) flat in vec3 tailHue;
layout(location = 0) out vec4 fragColor;
void main()
{
    vec3 rgb = ink.rgb;
    float coverage;
    float occlusion = 0.;
    if (shape.x < 0.5) {
        float t = local.y;
        float bend = shape.y * (0.22 * t + 0.78 * t * t);
        float vein = local.x - bend;
        float width = shape.z * pow(max(0., 1. - t), 0.64);
        if (shape.w > 0.5 && t > 0.63) {
            float plume = (t - 0.63) / 0.37;
            float seeds = pow(0.5 + 0.5 * sin(plume * 41. + details.x * 3.), 1.3);
            width += shape.z * 2.1 * sin(3.14159 * plume) * (0.42 + 0.58 * seeds);
        }
        float px = max(fwidth(vein), 0.30);
        coverage = clamp((width - abs(vein)) / px + 0.5, 0., 1.);
        // Rust supplies the nearby lantern field. Its lateral gradient picks
        // the illuminated edge, leaving both sides equal under diffuse light.
        float side = 2. * smoothstep(-0.7, 0.7, vein / max(0.65, width)) - 1.;
        float edge = 0.5 + 0.5 * side * details.w;
        rgb = mix(u.bottom.rgb * 0.32, u.bottom.rgb * 0.32 + ink.rgb * (0.17 + 0.26 * edge), 0.24 + 0.76 * t)
            * details.z;
        occlusion = coverage;
    } else if (shape.x < 1.5) {
        float width = max(0.55 * fwidth(local.y), local.x);
        float crossSection = local.y / width;
        coverage = exp(-crossSection * crossSection) * ink.a;
        coverage *= smoothstep(0.5 * details.x, 2. * details.x, length(fromHead));
    } else if (shape.x < 2.5) {
        float r = length(local);
        float coreDistance = r / shape.w;
        float core = exp(-coreDistance * coreDistance);
        float scaled = r / shape.z;
        float air = 0.30 * exp(-scaled * 0.48);
        float halo = 0.75 * exp(-scaled * scaled * 0.50);
        float intensity = ink.a;
        bool black = max(max(u.bottom.r, u.bottom.g), u.bottom.b) < 0.0001;
        if (details.z == 2.) {
            // A steady Ghost has one soft centre. Blend towards its target
            // luminance without adding the halo on top of that centre.
            vec3 hot = mix(rgb, vec3(1.), 0.20);
            hot *= (black ? 1.16 : 1.12) / dot(hot, vec3(0.2126, 0.7152, 0.0722));
            float softCore = exp(-pow(coreDistance * 0.78, 3.));
            vec3 haze = rgb * (0.45 * exp(-0.5 * scaled * scaled)) + tailHue * air;
            rgb = mix(haze, hot, softCore);
            // Compress the small breathing excursion; the dusk and rain still dim the whole lamp.
            intensity = 1.18 * min(intensity, 0.32) + 0.31 * max(0., intensity - 0.32);
        } else if (int(u.colours[0].a) == 3 && (details.z <= 1. || details.z == 3. || details.z == 7. || details.z == 13.)) {
            vec3 hot = mix(vec3(rgb.r, rgb.g, max(rgb.b, 0.74)), vec3(244, 255, 176) / 255., black ? 0.65 : 0.45);
            hot *= (black ? 0.99 : 0.96) / max(max(hot.r, hot.g), hot.b);
            rgb = mix(rgb * (halo + air), hot, exp(-pow(coreDistance * 0.58, 3.)));
        } else {
            vec3 hot = mix(rgb, vec3(1), black ? 0.65 : 0.45);
            rgb = 1.85 * hot * core + rgb * (halo + air);
        }
        coverage = intensity * (1. - smoothstep(0.86 * shape.y, shape.y, r));
        float peak = max(max(rgb.r, rgb.g), rgb.b) * coverage;
        if (!black)
            coverage *= smoothstep(0., 3. / 255., peak);
        if (black && peak < 1. / 255.)
            discard;
    } else if (shape.x < 3.5) {
        float r = length(local) / shape.y;
        float edge = 1. - smoothstep(0.72, 1.0, r);
        float rim = exp(-pow((r - 0.76) * 8., 2.));
        coverage = ink.a * (0.12 + 0.036 * rim) * edge;
    } else if (shape.x > 5.5) {
        float width = max(0.55 * fwidth(local.y), shape.y);
        coverage = exp(-dot(local, local) / (width * width)) * ink.a;
        coverage *= smoothstep(0.5 * details.x, 2. * details.x, length(fromHead));
    } else if (shape.x > 4.5) {
        coverage = ink.a * pow(sin(3.14159 * local.y), 2.);
    } else {
        float d = abs(local.x + local.y * 0.21);
        coverage = ink.a * (1. - smoothstep(0., 1.1, d)) * (1. - abs(local.y) / 17.);
    }
    fragColor = vec4(rgb * coverage, occlusion) * u.limits.y;
}
