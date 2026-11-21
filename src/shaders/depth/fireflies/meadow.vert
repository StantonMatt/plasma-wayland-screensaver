#version 440
// All draws share one immutable unit quad. Only recently changed rows of this
// world-space atlas cross the bus; each output selects its own view and time.
layout(std140, binding = 0) uniform Meadow
{
    mat4 matrix;
    vec4 view; // world offset xy, uniform scale, interpolation alpha
    vec4 bounds; // visible world x0,y0,x1,y1
    ivec4 counts; // blades, agents with trails, segments, atlas width
    ivec4 rows; // lower, upper, newest trail slot, ring length
    vec4 art; // size, grass pitch and origin, seeded palette variant
    vec4 colours[8];
    vec4 bottom; // ground RGB, flight-speed multiplier
    vec4 limits; // column count, opacity, still exposure, fractional trail sample
}
u;
layout(binding = 1) uniform sampler2D atlas;
layout(location = 0) out vec2 local;
layout(location = 1) out vec4 ink;
layout(location = 2) flat out vec4 shape;
layout(location = 3) flat out vec4 details;
layout(location = 4) out vec2 fromHead;
layout(location = 5) flat out vec3 tailHue;

float hash(float x) { return fract(sin(x * 77.43 + mod(u.art.w, 1024.0)) * 3713.35); }
vec4 head(int id)
{
    vec4 a = texelFetch(atlas, ivec2(id, u.rows.x), 0);
    vec4 b = texelFetch(atlas, ivec2(id, u.rows.y), 0);
    if (distance(a.xy, b.xy) > 24.0 || int(a.w) != int(b.w))
        return b;
    return vec4(mix(a.xyz, b.xyz, u.view.w), b.w);
}
vec4 history(int id, int age)
{
    int slot = (u.rows.z - age + u.rows.w) % u.rows.w;
    return texelFetch(atlas, ivec2(id, 4 + slot), 0);
}
// Centripetal Catmull-Rom tangents in the parameter space of this span.
vec2 knotTangent(vec2 incoming, vec2 outgoing, float before, float after)
{
    return (incoming * (after / before) + outgoing * (before / after)) / (before + after);
}
bool outside(vec2 lo, vec2 hi)
{
    return hi.x < u.bounds.x || hi.y < u.bounds.y || lo.x > u.bounds.z || lo.y > u.bounds.w;
}
vec4 grassLight(int x)
{
    return mix(
        texelFetch(atlas, ivec2(x, 2 + u.rows.x), 0), texelFetch(atlas, ivec2(x, 2 + u.rows.y), 0), u.view.w);
}
vec2 terrain(float x)
{
    int column = clamp(int((x - u.art.z) / u.art.y), 0, int(u.limits.x) - 1);
    int row = 4 + u.rows.w + (u.counts.x + u.counts.w - 1) / u.counts.w;
    return texelFetch(atlas, ivec2(column, row), 0).xy;
}
vec3 saturateHue(vec3 rgb, float saturation)
{
    float hi = max(max(rgb.r, rgb.g), rgb.b);
    float lo = min(min(rgb.r, rgb.g), rgb.b);
    return vec3(1.) - (vec3(hi) - rgb) * (saturation / max(hi - lo, 0.001));
}
vec3 lanternHue(int kind, vec3 rgb)
{
    int palette = int(u.colours[0].a);
    if (kind == 2 && palette != 4) {
        if (palette == 0)
            return vec3(0.44, 0.81, 1.);
        float saturation = 1. - min(min(rgb.r, rgb.g), rgb.b) / max(max(rgb.r, rgb.g), rgb.b);
        return saturateHue(rgb, max(0.50, saturation));
    }
    if (palette == 3 && (kind == 0 || kind == 1 || kind == 3 || kind == 7 || kind == 13)) {
        // The lantern emits yellow-green light, distinct from reflected leaf green.
        vec3 pigment = vec3(rgb.g * 0.92, rgb.g, rgb.b * 0.42);
        pigment /= max(max(pigment.r, pigment.g), pigment.b);
        return mix(pigment, vec3(201, 242, 122) / 255., 0.25);
    }
    return rgb;
}
void main()
{
    int idx = gl_InstanceIndex - 1;
    vec2 corner = vec2(float(gl_VertexIndex % 2), float(gl_VertexIndex / 2));
    vec2 unit = corner * 2.0 - 1.0;
    vec2 p;
    bool invisible = false;
    ink = vec4(0);
    shape = vec4(0);
    details = vec4(0);
    local = unit;
    fromHead = vec2(0);
    tailHue = vec3(0);
    const int subdivisions = 4; // Matches the four quads per history span in the adapter.
    int trails = u.counts.y * u.counts.z * subdivisions;
    if (idx < 0) {
        p = mix(u.bounds.xy, u.bounds.zw, corner);
        vec2 band = terrain(0.5 * (u.bounds.x + u.bounds.z));
        p.y = band.x - band.y * mix(0.35, 0.60, corner.y);
        local.y = corner.y;
        shape.x = 5.;
        ink = vec4(u.colours[4].rgb, 0.016);
        invisible = max(max(u.bottom.r, u.bottom.g), u.bottom.b) < 0.0001;
    } else if (idx < u.counts.x) {
        vec4 blade = texelFetch(atlas, ivec2(idx % u.counts.w, 4 + u.rows.w + idx / u.counts.w), 0);

        float fx = clamp((blade.x - u.art.z) / u.art.y - 0.5, 0., u.limits.x - 1.);
        vec4 left = grassLight(int(fx));
        vec4 right = grassLight(min(int(fx) + 1, int(u.limits.x) - 1));
        vec4 lighting = mix(left, right, fract(fx));
        const vec3 luma = vec3(0.2126, 0.7152, 0.0722);
        float lightDirection = clamp(4. * dot(right.rgb - left.rgb, luma) / (0.10 + dot(lighting.rgb, luma)), -1., 1.);
        float seed = hash(float(idx));
        float bend = (blade.w + lighting.w) * blade.z;
        float width = (0.009 + 0.009 * seed) * blade.z;
        bool awn = seed > 0.89;
        float margin = width * (awn ? 5. : 2.) + 1.5;
        float x = mix(min(0., bend) - margin, max(0., bend) + margin, corner.x);
        p = blade.xy + vec2(x, -corner.y * blade.z);
        local = vec2(x, corner.y);
        shape = vec4(0, bend, width, awn ? 1. : 0.);
        details = vec4(seed, blade.z, idx < u.counts.x / 2 ? 0.64 : 1., lightDirection);
        ink = vec4(lighting.rgb, 1.);
        invisible = outside(
            blade.xy + vec2(min(0., bend) - margin, -blade.z), blade.xy + vec2(max(0., bend) + margin, 0.));
    } else if (idx < u.counts.x + trails) {
        int quad = idx - u.counts.x;
        int strip = quad / subdivisions;
        int part = quad % subdivisions;
        int age = strip / u.counts.y;
        int id = strip % u.counts.y;
        vec4 tip = head(id);
        vec4 a = history(id, age);
        vec4 b = age == 0 ? tip : history(id, age - 1);
        vec2 delta = b.xy - a.xy;
        float len = length(delta);
        int kind = int(b.w);
        float size = terrain(a.x).y / 1440.;
        float vmax = (kind == 2 ? 30. : (kind == 1 ? 60. : 110.)) * u.bottom.w;
        if (kind == 0)
            vmax = max(125., 90. * u.bottom.w);
        float stepLimit = max(24., 0.3 * vmax) * size;
        if (max(a.z, b.z) < 0.025 || len > stepLimit || int(a.w) != kind || kind == 13) {
            gl_Position = vec4(-2, -2, 0, 1);
            return;
        }

        vec4 older = history(id, min(age + 1, u.rows.w - 1));
        vec4 newer = age <= 1 ? tip : history(id, age - 2);
        vec2 incoming = a.xy - older.xy;
        vec2 outgoing = newer.xy - b.xy;
        float before = length(incoming), after = length(outgoing);
        if (int(older.w) != kind || before < 0.03 || before > stepLimit) {
            incoming = delta;
            before = len;
        }
        if (int(newer.w) != kind || after < 0.03 || after > stepLimit) {
            outgoing = delta;
            after = len;
        }
        float baseRadius = (1.35 + 1.35 * fract(b.w)) * pow(u.art.x, 0.6);
        if (kind == 3 || kind == 7)
            baseRadius *= 1.3;
        // At a hovering flash, spend the ribbon's exposure on a circular spot.
        // The brightest sample anchors it; the integrated length weights its
        // neighbours so coincident samples cannot accumulate a white stack.
        float run = len + (older.z > 0.10 ? before : 0.) + (newer.z > 0.10 ? after : 0.);
        bool dotRun = run < 3. * baseRadius;
        bool peak = a.z > 0.18 && a.z > older.z && a.z >= b.z;
        if ((dotRun && part != 0) || (len < 0.03 && !(dotRun && peak))) {
            gl_Position = vec4(-2, -2, 0, 1);
            return;
        }
        float t = dotRun ? 0.5 : (float(part) + corner.x) / float(subdivisions);
        float fraction = u.limits.w;
        float sampleAge = age == 0 ? fraction * (1. - t) : float(age) - t + fraction;
        float fade = clamp(1. - sampleAge / max(1., float(u.counts.z) - 1.), 0., 1.);
        float radius = baseRadius * mix(fade, 1., u.limits.z);
        float extent = 2.6 * radius + 0.8;
        float intensity = mix(a.z, b.z, t);
        if ((kind == 0 || kind == 3 || kind == 7) && min(a.z, b.z) > 0.020)
            intensity = max(intensity, 0.08 / 0.75);
        if (dotRun) {
            p = mix(a.xy, b.xy, t) + unit * extent;
            shape = vec4(6., radius, 0., 0.);
            local = unit * extent;
            intensity = peak ? a.z : intensity * min(1., len / max(0.1, 1.77245 * radius));
        } else {
            float h = sqrt(len);
            vec2 m0 = h * knotTangent(incoming, delta, sqrt(before), h);
            vec2 m1 = h * knotTangent(delta, outgoing, h, sqrt(after));
            vec2 bend = mix(m0 - delta, delta - m1, t);
            vec2 centre = mix(a.xy, b.xy, t) + t * (1. - t) * bend;
            vec2 tangent = delta + (1. - 2. * t) * bend + t * (1. - t) * (2. * delta - m0 - m1);
            vec2 along = length(tangent) > 0.001 ? normalize(tangent) : delta / len;
            p = centre + vec2(-along.y, along.x) * unit.y * extent;
            shape.x = 1.;
            local = vec2(radius, unit.y * extent);
        }
        fromHead = p - tip.xy;
        details.x = (1.35 + 1.35 * fract(tip.w)) * pow(u.art.x, 0.6);
        if (int(u.colours[0].a) == 3 && (kind <= 1 || kind == 3 || kind == 7))
            details.x *= 4.;
        int species = clamp(kind, 0, 7);
        ink = vec4(lanternHue(species, u.colours[species].rgb), intensity * mix(fade * 0.75, 0.8, u.limits.z));
    } else {
        int id = idx - u.counts.x - trails;
        vec4 h = head(id);
        int kind = int(h.w);
        bool bokeh = kind >= 9 && kind <= 12;
        float z = fract(h.w);
        float seed = hash(float(id));
        float sigma = (6.0 + z) * u.art.x;
        float core = (1.35 + 1.35 * z) * pow(u.art.x, 0.6);
        if (kind == 3 || kind == 7) {
            sigma *= 1.30;
            core *= 1.30;
        }
        if (kind == 2) {
            core *= 0.80;
            sigma *= 1.4;
        }
        if (kind == 13)
            sigma *= 1.25;
        if (kind == 4) {
            sigma *= 0.52;
            core *= 0.57;
        }
        if (u.art.w >= 1024.)
            sigma *= 1.25;
        float radius = 8. * sigma;
        if (bokeh)
            radius = (22. + 20. * seed) * u.art.x;
        radius = min(radius, 0.12 * terrain(h.x).y);
        vec2 extent = kind == 8 ? vec2(5., 17.) : vec2(radius);
        p = h.xy + unit * extent;
        local = unit * extent;
        vec3 rgb = u.colours[bokeh ? kind - 9 : (kind == 13 ? 1 : min(kind, 7))].rgb;
        if (kind == 4)
            rgb = mix(rgb, vec3(dot(rgb, vec3(0.21, 0.72, 0.07))), 0.25);
        if (kind == 8)
            rgb = mix(u.colours[4].rgb, vec3(0.55), 0.6);
        rgb = lanternHue(kind, rgb);
        tailHue = rgb;
        if (kind == 2 && int(u.colours[0].a) != 4) {
            float saturation = 1. - min(min(rgb.r, rgb.g), rgb.b) / max(max(rgb.r, rgb.g), rgb.b);
            tailHue = 0.95 * saturateHue(rgb, min(1., 1.5 * saturation));
        }
        shape = vec4(kind == 8 ? 4. : (bokeh ? 3. : 2.), radius, sigma, core);
        ink = vec4(rgb, h.z);
        details = vec4(seed, z, float(kind), 0.);
        invisible = h.z < 0.008 || outside(h.xy - extent, h.xy + extent);
    }
    // RHI clips the expanded world-space extent against each output, including seam halos.
    vec2 item = p * u.view.z + u.view.xy;
    gl_Position = invisible ? vec4(-2, -2, 0, 1) : u.matrix * vec4(item, 0., 1.);
}
