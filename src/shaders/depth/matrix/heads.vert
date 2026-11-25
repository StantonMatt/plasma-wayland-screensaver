#version 440
#extension GL_GOOGLE_include_directive : require
#include "uniforms.glsl"
layout(location = 0) in vec4 vertex;
layout(location = 1) in vec2 slot;
layout(binding = 1) uniform sampler2D records;
layout(location = 0) out vec2 local;
layout(location = 1) flat out vec4 ink;
layout(location = 2) flat out vec4 lighting;
layout(location = 3) out vec2 worldPoint;
void main()
{
    int i = int(slot.x);
    if (i >= int(ub.grid.w))
    {
        local = vec2(0.);
        ink = vec4(0.);
        lighting = vec4(0.);
        worldPoint = vec2(0.);
        gl_Position = vec4(2., 2., 2., 1.);
        return;
    }
    vec4 path = texelFetch(records, ivec2(0, i), 0);
    ink = texelFetch(records, ivec2(1, i), 0);
    lighting = texelFetch(records, ivec2(2, i), 0);
    if (lighting.y == 3.)
    {
        // Bolt-local coordinates in pixels, tangent first. Padding covers the
        // Gaussian beyond both ends of the segment and across cell boundaries.
        vec2 tangent = (path.zw - path.xy) / ink.y;
        float pad = 4. * 0.55 * ink.x;
        local = mix(vec2(-pad), vec2(ink.y + pad, pad), vertex.xy / ub.viewport.zw);
        worldPoint = path.xy + tangent * local.x + vec2(-tangent.y, tangent.x) * local.y;
        gl_Position = ub.qt_Matrix * vec4(worldPoint - ub.viewport.xy, 0., 1.);
        return;
    }
    vec2 pos = mix(path.xy, path.zw, ub.light.z) - ub.viewport.xy;
    local = (vertex.xy / ub.viewport.zw - 0.5) * vec2(4.4, 5.2) - vec2(0.0, 0.43);
    if (lighting.y == 2.)
        local = (vertex.xy / ub.viewport.zw - 0.5) * vec2(8.0);
    vec2 position = pos + local * ink.xy;
    worldPoint = position + ub.viewport.xy;
    gl_Position = ub.qt_Matrix * vec4(position, 0., 1.);
}
