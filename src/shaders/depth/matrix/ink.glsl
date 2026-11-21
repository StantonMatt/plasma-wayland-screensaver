layout(binding = 2) uniform sampler2D atlas;
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
vec4 pigmentedInk(vec3 color, float emission, float coverage)
{
    const vec3 luma = vec3(0.2126, 0.7152, 0.0722);
    float alpha = max(min(1., emission), coverage);
    float substrate = dot(ub.background.rgb, luma) / max(0.001, dot(color, luma));
    return vec4(color * (emission + max(0., alpha - emission) * substrate), alpha);
}
