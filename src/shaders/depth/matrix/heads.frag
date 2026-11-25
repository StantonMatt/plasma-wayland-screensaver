#version 440
#extension GL_GOOGLE_include_directive : require
#include "uniforms.glsl"
#include "ink.glsl"
layout(location = 0) in vec2 local;
layout(location = 1) flat in vec4 ink;
layout(location = 2) flat in vec4 lighting;
layout(location = 3) in vec2 worldPoint;
layout(location = 0) out vec4 fragColor;
void main()
{
    if (lighting.y == 3.)
    {
        float age = (ub.section.x - lighting.z) / 30.;
        bool branch = lighting.w > 0.;
        float energy = lighting.x * step(ink.z, ub.strike.w);
        if (branch) {
            // Branches conduct the main stroke, never a return stroke or
            // the stylised trunk afterimage. Geometry is already cached.
            float mainAge = ub.strike.w - 0.120;
            energy *= step(0., mainAge) * (1. - step(ub.returns.x, ub.strike.w))
                * 4.6 * exp(-max(0., mainAge - 0.020) / 0.055);
        } else {
            energy *= exp2(-max(0., age - 0.15)) * (1. - smoothstep(5., 6.4, age)) * ub.strike.z;
        }
        float taper = branch ? mix(1., lighting.w - 1., clamp(local.x / ink.y, 0., 1.)) : 1.;
        energy *= taper;
        float width = max(0.001, ink.w * taper);
        float sigma = max(0.15, 0.44 * ink.x * taper);
        float along = clamp(local.x, 0., ink.y);
        float distance = length(vec2(local.x - along, local.y));
        float halo = 0.85 * (1. - exp(-0.20 * exp(-0.5 * pow(distance / sigma, 2.)) * energy / 0.85));
        float core = 1. - smoothstep(1.25 * width, 2.85 * width, distance);
        vec3 haloColor = mix(vec3(0.32, 0.65, 1.), vec3(0.10, 0.80, 1.), smoothstep(20., 40., distance));
        vec3 haloLight = haloColor * halo;
        // Weak light over Midnight's violet substrate otherwise reads purple.
        // Add only the missing green/blue radiance at that fringe; already
        // blue-white light retains the Gaussian profile and stroke weight.
        vec3 sky = mix(ub.sky_top.rgb, ub.background.rgb,
            clamp(worldPoint.y / max(1., ub.strike.y), 0., 1.))
            + lightningTint() * localLightning(worldPoint);
        float fringe = smoothstep(0., 0.002, halo) * min(1., energy / 0.5);
        vec3 total = sky + haloLight;
        float green = max(0., total.r + 0.3667 * (total.b - total.r) - total.g);
        haloLight.b = max(haloLight.b, 0.025 * fringe * smoothstep(0., 0.004, green));
        total = sky + haloLight;
        haloLight.b += fringe * max(0., total.r + (total.g - total.r) / 0.52 - total.b);
        total = sky + haloLight;
        haloLight.g += fringe * max(0., total.r + 0.3667 * (total.b - total.r) - total.g);
        // MAX combines only this private emission layer. Its zero-alpha
        // display adds the soft blue-white channel onto the existing scene.
        fragColor = vec4((vec3(0.75, 0.86, 1.) * 0.65 * core * energy + haloLight) * ub.light.w, 0.);
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
    float flash = localLightning(worldPoint);
    hot = litInk(hot, flash);
    body = litInk(body, flash);
    vec4 stroke = vec4(hot * emission, min(1., emission));
    if (layer != 1. || droplet || int(ub.light.x) == 2)
        stroke = pigmentedInk(hot, emission, smoothstep(0.03, 0.65, core) * smoothstep(0., 0.3, lighting.x), flash);
    stroke.rgb += body * ((halo + 0.17 * streak) * gain);
    fragColor = stroke * ub.light.w;
}
