#version 440
#extension GL_GOOGLE_include_directive : require
#include "uniforms.glsl"
layout(location = 0) in vec4 vertex;
layout(location = 0) out vec2 world;
void main()
{
    world = vertex.xy + ub.viewport.xy;
    gl_Position = ub.qt_Matrix * vertex;
}
