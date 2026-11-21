#version 440
#extension GL_GOOGLE_include_directive : require
#include "uniforms.glsl"
#include "ink.glsl"
layout(location = 0) in vec2 local;
layout(location = 1) flat in vec4 ink;
layout(location = 2) flat in vec4 lighting;
layout(location = 0) out vec4 fragColor;
// Integral of a unit Gaussian from the segment start to the sample.
// Adjacent segments share their end light without a bright disc at every join.
float erfApprox(float x)
{
    float a = abs(x);
    float t = 1. / (1. + 0.3275911 * a);
    float poly = (((((1.061405429*t - 1.453152027)*t) + 1.421413741)*t - 0.284496736)*t + 0.254829592)*t;
    return sign(x) * (1. - poly * exp(-a*a));
}
float segmentWeight(float sigma)
{
    vec2 end = vec2(local.x, local.x - ink.y) / (1.41421356 * sigma);
    return 0.5 * (erfApprox(end.x) - erfApprox(end.y));
}
void main()
{
    if (lighting.y == 3.)
    {
        float age = (ub.section.x - lighting.z) / 30.;
        float energy = lighting.x * step(0., age) * exp2(-max(0., age - 0.15)) * (1. - smoothstep(5., 6.4, age));
        float sigma = 0.35 * ink.x;
        float halo = 0.35 * exp(-0.5 * pow(local.y / sigma, 2.)) * segmentWeight(sigma);
        float core = (1. - smoothstep(1.25, 2.85, abs(local.y))) * segmentWeight(1.35);
        vec3 color = mix(inkColor(0, energy, false), highlight(0), smoothstep(0.35, 0.85, energy));
        // Allocate peak light before summing segments. Per-quad clipping
        // would brighten shared endpoints when two half kernels meet.
        fragColor = vec4(color * (0.65 * core + halo) * energy * ub.light.w, 0.);
        return;
    }
    int tint = int(ink.w);
    float layer = lighting.z;
    bool droplet = lighting.y == 1., lantern = lighting.y == 2.;
    vec2 q = local;
    float d = glyphDistance(int(ink.z), q + 0.5);
    float softness = layer == 2. ? 2.7 : (layer == 0. ? 1.6 : max(0.5, 19. / ink.y));
    float core = smoothstep(-softness, softness, d + 0.36);
    vec3 body = inkColor(tint, 0.8, lantern),
         hot = lantern ? mix(body, highlight(tint), 0.32) : highlight(tint);
    float width = (lantern ? 3.1 : 1.0) * (int(ub.light.x) == 5 ? 0.78 : 1.);
    vec2 radius = q / width;
    float halo = exp(-dot(radius, radius) * 3.9) * (lantern ? 0.40 : 0.17);
    float streak = exp(-q.x * q.x * 26.) * smoothstep(-1.8, -0.18, q.y) * (1. - smoothstep(-0.2, 0.28, q.y));
    float gain = lighting.x * (1. + 0.6 * lighting.w);
    if (droplet)
    {
        hot = int(ub.light.x) == 2 ? ub.colors[1].rgb : inkColor(tint, 1., false);
        halo = 0.;
        streak = 0.;
    }
    else if (layer == 0.)
    {
        body = hot = distantInk(tint);
        gain *= (ub.light.y == 0. ? 0.50 : 0.35) * min(1., ub.weather.z / 0.60);
    }
    else if (layer == 2.)
    {
        // Defocus the nearest body's own warm ink, including its luminous core.
        body = hot = inkColor(tint, 0.66, false);
        gain *= 0.45 * min(1., ub.weather.z / 0.60);
    }
    float emission = core * gain;
    vec4 stroke = vec4(hot * emission, min(1., emission));
    if (layer != 1. || droplet || int(ub.light.x) == 2)
        stroke = pigmentedInk(hot, emission, smoothstep(0.03, 0.65, core) * smoothstep(0., 0.3, lighting.x));
    stroke.rgb += body * ((halo + 0.17 * streak) * gain);
    fragColor = stroke * ub.light.w;
}
