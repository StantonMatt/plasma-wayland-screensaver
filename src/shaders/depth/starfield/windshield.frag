#version 440
#extension GL_GOOGLE_include_directive : enable
#include "block.glsl"
layout(binding=2) uniform sampler2D noiseTile;
layout(location=0) in vec2 coord;
layout(location=1) in vec2 screenPoint;
layout(location=2) flat in vec4 shape;
layout(location=3) flat in vec4 emission;
layout(location=4) flat in vec4 stellar;
layout(location=0) out vec4 fragColor;

#include "sky-fragment.glsl"
#include "stars-fragment.glsl"
void main(){
    if(stellar.w > 0.5)
        fragColor = starLight(coord, shape, emission, stellar.xyz);
    else
        fragColor = skyLight(coord, screenPoint, shape, emission);
}
