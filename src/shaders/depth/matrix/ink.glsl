layout(binding = 2) uniform sampler2D atlas;
// The same packed void-and-cluster ranks used by Aurora's shore. Address
// them in desk pixels, so the grain has no output boundary or temporal flicker.
const uint blueNoise[64] = uint[64](
    0xb9470a96u, 0x87309513u, 0x593acb23u, 0x2588a6f1u, 0x85fa74bbu, 0x5dd7ac67u, 0x8ee77a46u, 0xee6bd41bu,
    0x1aa84c1eu, 0xfd073beau, 0x2f9d0db6u, 0x5a053fb5u, 0xc236d581u, 0x9375ce5fu, 0xd251de65u, 0xc894fc78u,
    0x90039bf3u, 0xbe289f43u, 0x68af1937u, 0x32ab4910u, 0x77e4546du, 0x53d815f2u, 0x26f789e8u, 0x127edbbfu,
    0x2bbd1fb2u, 0x017db44bu, 0xa13c70a9u, 0xe13d608du, 0xdda34586u, 0x62f99869u, 0xd34d1dd0u, 0xc524ed04u,
    0x0e5eff08u, 0x2e4122ccu, 0x7fefb791u, 0x6492ad56u, 0x397c9ccfu, 0x80b1e684u, 0x2a6a0bdfu, 0x35f516c7u,
    0xc1ec2955u, 0xca4e09a0u, 0xe59a445cu, 0xa5734a8bu, 0x3e176ee0u, 0x1872f85bu, 0xae14fea7u, 0x1cbad933u,
    0xdab38cc3u, 0x8fd12c97u, 0x57c47638u, 0x4299006fu, 0x024ff00fu, 0xeb48b06cu, 0x21dc27b8u, 0x61fb83e9u,
    0x82a42d79u, 0x0c7b20f4u, 0xa28a6652u, 0xaa31cd40u, 0x34d663e2u, 0xc0e358c9u, 0xbc06f69eu, 0xc6501171u
);
float lightningGrain(vec2 world)
{
    uvec2 pixel = uvec2(floor(world));
    uvec2 tile = pixel >> 4u;
    uint hash = tile.x * 0x9e3779b9u + tile.y * 0x85ebca6bu;
    hash ^= hash >> 16u;
    hash *= 0x7feb352du;
    hash ^= hash >> 15u;
    uvec2 px = (pixel + uvec2(hash, hash >> 8u)) & 15u;
    uint packed = blueNoise[px.y * 4u + (px.x >> 2u)];
    return (float((packed >> (8u * (px.x & 3u))) & 255u) / 255. - 0.5) / 255.;
}
float localLightning(vec2 world)
{
    if (ub.weather.y <= 0.)
        return 0.;
    float dx = (world.x - ub.strike.x) / 2000.;
    return ub.weather.y * (0.33 + 0.67 * exp(-dx * dx))
        * (1. - 0.35 * clamp(world.y / max(1., ub.strike.y), 0., 1.));
}
vec3 lightningTint()
{
    int p = int(ub.light.x);
    int accent = p == 0 || p == 5 ? 4 : (p == 2 ? 5 : (p == 1 ? 3 : 1));
    vec3 tint = mix(vec3(0.60, 0.72, 1.0), ub.colors[accent].rgb, p == 2 ? 0.10 : 0.25);
    if (p == 2) {
        // Ember's warm accent lowers the blue lift's chroma. Restore it
        // around its peak channel while preserving the mixed tint's hue.
        float peak = max(tint.r, max(tint.g, tint.b));
        tint = peak - 1.30 * (peak - tint);
    }
    return tint;
}
vec3 lightningSky(float lift)
{
    // Bound both ends of the underlying gradient so translucent pigment
    // cannot erase the lit sky, including on the shorter output.
    return max(ub.background.rgb, ub.sky_top.rgb) + lightningTint() * lift;
}
vec3 litInk(vec3 color, float lift)
{
    if (lift <= 0.)
        return color;
    vec3 lit = color + lightningTint() * lift;
    // Fade the material correction away with the light; its zero-light
    // limit must be the original ink, not a persistent sky-coloured ghost.
    lit = max(lit, mix(lit, lightningSky(lift), smoothstep(0., 0.02, lift)));
    return lit * (1. + 1.5 * lift);
}
float glyphDistance(int id, vec2 uv)
{
    if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0))))
        return -8.0;
    vec2 origin = vec2(id % 16, id / 16) * 48.0;
    return (texture(atlas, (origin + clamp(uv * 48.0, vec2(0.5), vec2(47.5))) / vec2(textureSize(atlas, 0))).r
               - 0.5)
        * 16.0;
}
// Overlapping light from adjacent palette stops gives a brighter phosphor
// core without inserting a foreign hue into Spectrum or Pastel.
vec3 adjacentPeak(int tint)
{
    vec3 a = ub.colors[tint % 6].rgb;
    vec3 b = ub.colors[(tint + 1) % 6].rgb;
    return a + b - a * b;
}
vec3 inkColor(int tint, float heat, bool lantern)
{
    int p = int(ub.light.x);
    vec3 cool, body, hot;
    if (p == 1 || p == 5)
    {
        body = ub.colors[tint % 6].rgb;
        cool = p == 1 ? mix(body, ub.colors[(tint + 1) % 6].rgb, 0.47) : body * 0.55;
        hot = mix(body, adjacentPeak(tint), p == 1 ? 0.25 : 0.13);
    }
    else if (p == 0)
    {
        cool = ub.colors[3].rgb;
        body = mix(ub.colors[2].rgb, ub.colors[1].rgb, 0.52 + 0.08 * float(tint));
        hot = ub.colors[5].rgb;
        if (!lantern) {
            vec3 tail = mix(cool, ub.colors[2].rgb, smoothstep(0.08, 0.30, heat));
            vec3 current = mix(tail, body, smoothstep(0.30, 0.60, heat));
            return mix(current, hot, smoothstep(0.60, 1.0, heat));
        }
    }
    else if (p == 2)
    {
        cool = mix(ub.colors[4].rgb, ub.colors[3].rgb, 0.13 * float(tint));
        body = ub.colors[2].rgb;
        hot = ub.colors[1].rgb;
    }
    else if (p == 3)
    {
        cool = ub.colors[3].rgb;
        body = mix(ub.colors[2].rgb, ub.colors[5].rgb, float(tint) * 0.15);
        hot = ub.colors[4].rgb;
    }
    else
    {
        cool = ub.colors[3].rgb;
        body = ub.colors[2].rgb;
        hot = ub.colors[4].rgb;
    }
    if (p == 5 && !lantern)
        return mix(cool, body, smoothstep(0.08, 0.50, heat));
    if (lantern)
    {
        int accent = p == 0 ? 4 : (p == 5 ? 4 : (p == 1 ? (tint + 3) % 6 : (p == 2 ? 5 : 1)));
        body = ub.colors[accent].rgb;
        hot = mix(body, hot, 0.23);
    }
    return mix(mix(cool, body, smoothstep(0.02, 0.60, heat)), hot, smoothstep(0.51, 1.1, heat));
}
vec3 highlight(int tint)
{
    int p = int(ub.light.x);
    if (p == 1 || p == 5)
        return mix(adjacentPeak(tint), ub.colors[tint % 6].rgb, 0.33);
    return ub.colors[0].rgb;
}

vec3 distantInk(int tint)
{
    int p = int(ub.light.x);
    vec3 cool = p == 2 ? ub.colors[4].rgb : inkColor(tint, 0., false);
    if (p == 1)
        cool = mix(cool, vec3(dot(cool, vec3(0.2126, 0.7152, 0.0722))), 0.40);
    return mix(cool, ub.background.rgb, 0.45);
}

// Retain the unlit surface luminance under a coloured phosphor stroke.
// Its cast follows the deposited pigment as coverage increases. This keeps
// a translucent ember red, even against a dark blue background.
vec4 pigmentedInk(vec3 color, float emission, float coverage, float flash)
{
    const vec3 luma = vec3(0.2126, 0.7152, 0.0722);
    float alpha = max(min(1., emission), coverage);
    float substrate = dot(ub.background.rgb, luma) / max(0.001, dot(color, luma));
    vec3 pigment = color * (emission + max(0., alpha - emission) * substrate);
    if (flash > 0.)
        pigment = mix(pigment, color * emission + lightningSky(flash) * max(0., alpha - emission), smoothstep(0., 0.02, flash));
    return vec4(pigment, alpha);
}
