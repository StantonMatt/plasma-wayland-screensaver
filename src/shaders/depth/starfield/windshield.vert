#version 440
#extension GL_GOOGLE_include_directive : enable
#include "block.glsl"
layout(binding=1) uniform sampler2D stars;
layout(location=0) out vec2 coord;
layout(location=1) out vec2 screenPoint;
layout(location=2) flat out vec4 shape;
layout(location=3) flat out vec4 emission;
layout(location=4) flat out vec4 stellar;

#include "sky-vertex.glsl"
#include "stars-vertex.glsl"
void main(){
    uint skyCount = u.counts.y + (u.counts.z != 0u ? 43u : 0u);
    uint id = uint(gl_InstanceIndex);
    // The galaxy and far stars lie behind the absorbing cloud lanes. Near
    // stars are drawn last. One pipeline handles all layers in painter order.
    uint band = uint(u.counts.y != 0u && skySprite(0).material.y == 0.);
    uint nearStars = u.counts.w << 7;
    uint farStars = (u.counts.x-u.counts.w) << 7;
    if(id < band || (id >= band+farStars && id < skyCount+farStars)){
        stellar = vec4(0.);
        skyVertex(id < band ? id : id-farStars, coord, screenPoint, shape, emission);
    }else{
        screenPoint = vec2(0.);
        stellar.w = 1.;
        uint index = id < band+farStars ? nearStars+id-band : id-skyCount-farStars;
        starVertex(int(index), coord, shape, emission, stellar.xyz);
    }
}
