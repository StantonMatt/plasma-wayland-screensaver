#version 440
#extension GL_GOOGLE_include_directive : require
#include "uniforms.glsl"
#include "ink.glsl"
layout(binding = 1) uniform sampler2D cells;
layout(location = 0) in vec2 world;
layout(location = 0) out vec4 fragColor;

uint hashCell(uint x)
{
    x ^= x >> 16u;
    x *= 0x7feb352du;
    x ^= x >> 15u;
    x *= 0x846ca68bu;
    return x ^ (x >> 16u);
}
ivec2 alphabet(int id)
{
    if (id >= 321)
        return ivec2(321, 8);
    if (id >= 225)
        return ivec2(225, 96);
    if (id >= 209)
        return ivec2(209, 8);
    if (id >= 145)
        return ivec2(145, 50);
    if (id >= 49)
        return ivec2(49, 74);
    if (id >= 17)
        return ivec2(17, 22);
    return ivec2(1, 12);
}
int swapped(ivec2 dialect, uint seed, int id, int bucket, int excluded)
{
    if (bucket <= 0)
        return id;
    int n = int(hashCell(seed ^ uint(bucket)) % uint(dialect.y));
    for (int i = 0; i < 2 && (dialect.x + n) % 3 == excluded; ++i)
        n = (n + 1) % dialect.y;
    return dialect.x + n;
}
void main()
{
    vec2 grid = world / ub.grid.xy;
    ivec2 p = ivec2(floor(grid)) - ivec2(ub.grid.zw);
    ivec2 texel = ivec2(2 * p.y, p.x);
    ivec4 ink = ivec4(texelFetch(cells, texel + ivec2(1, 0), 0) * 255.0 + 0.5);
    float flash = ub.section.w == 1.0 ? ub.weather.y : 0.0;
    vec4 fieldLight = vec4(ub.colors[0].rgb * flash, flash);
    if (ink.a == 0 || (ink.b & 128) != 0)
    {
        fragColor = fieldLight * ub.light.w;
        return;
    }
    vec4 bytes = round(texelFetch(cells, texel, 0) * 255.0);
    float age = max(0.0, (ub.section.x - dot(bytes, vec4(1., 256., 65536., 16777216.))) / 30.);
    int id = ink.r + 256 * (ink.g & 1), tint = ink.b & 7;
    bool lantern = (ink.b & 8) != 0, bolt = (ink.b & 16) != 0;
    float halfLife = bolt ? 1.0 : ((ink.b & 64) != 0 ? 1.25 : ub.section.y * (lantern ? 3.0 : ((ink.b & 32) != 0 ? 0.83 : 1.0)));
    const float cooling[4] = float[4](1., 0.75, 0.5, 0.375);
    halfLife *= cooling[ink.g >> 6];
    float energy = float(ink.a) / 255. * exp2(-(bolt ? max(0., age - 0.15) : age) / halfLife);
    float band = min(1., abs(world.y - ub.weather.x) / (5.6 * ub.grid.y));
    float sweep = band < 1. ? (0.10/31.) * float((ink.g >> 1) & 31) * exp(-age / 100.) * (1. + cos(3.14159265 * band)) : 0.;
    float glow = max(energy, sweep);
    if (ub.weather.w > 0.001 && age < 40.0)
    {
        uint ember = hashCell(uint(floor(grid.x)) * 3517u + uint(floor(grid.y)) * 107u);
        if (ember % 61u == 0u)
            glow = max(glow,
                0.23 * ub.weather.w * (1. - smoothstep(16., 40., age))
                    * (1. - smoothstep(800., 1300., abs(world.x - ub.background.w))));
    }
    if (glow < 0.009)
    {
        fragColor = fieldLight * ub.light.w;
        return;
    }
    vec2 uv = fract(grid);
    ivec2 dialect = alphabet(id);
    uint seed = hashCell(uint(floor(grid.x)) * 3517u + uint(floor(grid.y)) * 107u + uint(id) * 5557u);
    // Integrate a decaying swap rate. Dark ink slows down smoothly; its
    // sequence never runs backwards as its energy falls.
    float initial = float(ink.a) / 255.0;
    float activeAge = min(age, halfLife * max(0.0, log2(initial / 0.009)));
    float clock = activeAge * 0.30 + 5.7 * initial * halfLife * 1.442695 * (1. - exp2(-activeAge / halfLife));
    float bucket = floor(clock);
    float phase = fract(clock);
    float frequency = 0.3 + 5.7 * initial * exp2(-activeAge / halfLife);
    float blend = clamp(phase / max(0.001, frequency * 0.07), 0., 1.);
    int excluded = (int(floor(grid.x)) + int(floor(grid.y))) % 3;
    int current = swapped(dialect, seed, id, int(bucket), excluded);
    float d = glyphDistance(current, uv);
    float aa = max(0.45, 23. / ub.grid.y);
    float blur = ub.section.w == 0. ? 1.0 : (ub.section.w == 2. ? 2.5 : 0.);
    float width = aa + blur;
    float coverage = smoothstep(-width, width, d + 0.20);
    if (blend < 1.0 && bucket > 0.0)
    {
        float old = glyphDistance(swapped(dialect, seed, id, int(bucket) - 1, excluded), uv);
        coverage = mix(smoothstep(-width, width, old + 0.2), coverage, blend);
    }
    vec3 color = inkColor(tint, glow, lantern);
    if (ub.section.w == 0.)
        color = distantInk(tint);
    float strength = 0.91 * pow(glow, 0.81);
    if (int(ub.light.x) == 5)
        strength = max(strength, 0.77 * smoothstep(0.15, 0.5, glow));
    float attack = (ink.b & 64) != 0 ? 1. : smoothstep(0., 0.07, age);
    float alpha = coverage * strength * attack * ub.section.z;
    vec4 stroke = vec4(color * alpha, alpha);
    if (ub.section.w != 1. || int(ub.light.x) == 2)
        stroke = pigmentedInk(color, alpha, smoothstep(0.03, 0.65, coverage) * smoothstep(0.02, 0.35, glow) * attack);
    fragColor = (stroke + fieldLight * (1. - stroke.a)) * ub.light.w;
}
