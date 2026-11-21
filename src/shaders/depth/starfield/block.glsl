// SPDX-License-Identifier: GPL-3.0-or-later
const float aberration = 0.029;
struct Sprite { vec4 shape; vec4 material; vec4 detail; };
layout(std140,binding=0) uniform Block {
    mat4 matrix;
    vec4 projection; // focal length, local optical centre, cylindrical switch
    vec4 viewport;   // width, height, reference height, one physical pixel
    vec4 rotation; vec4 oldRotation;
    vec4 eye[2];     // bounded camera coordinates and cell size
    vec4 shift;      // translation during the exposure, previous warp factor
    vec4 optics;     // warp, absolute seconds, star size, punch envelope
    vec4 surface;    // gas, background style, selected sprite lifetime, palette index
    vec4 field; // seed, starbow strength, local vanishing point
    vec4 axis; vec4 oldAxis; // xyz forward, w optical exposure and shutter extent
    vec4 destination; // selected world landmark relative to the current eye
    vec4 palette[6];
    uvec4 counts;    // groups, sprites, debris row + 1, near groups
    vec4 opacity;
    // Qt GLES uploads signed ivec4 arrays in full. The bit pattern packs two groups per word.
    ivec4 groups[128];
    // One vec4 array keeps the same std140 layout as the native sprite
    // triples. QRhi's GL backend can upload it in a single uniform call.
    vec4 spriteData[48];
} u;
Sprite skySprite(int index){
    int offset=3*index;
    return Sprite(u.spriteData[offset],u.spriteData[offset+1],u.spriteData[offset+2]);
}
vec3 rotateEye(vec3 p,vec4 q){return p+2.*cross(q.xyz,cross(q.xyz,p)+q.w*p);}
vec3 temperature(float t){
    float x=clamp(t,0.,0.99999)*(u.surface.w==1.?5.:3.);int i=int(x);
    return mix(u.palette[i].rgb,u.palette[i+1].rgb,fract(x));
}
vec3 gasCore(){return u.surface.w==1.?u.palette[1].rgb:u.palette[4].rgb;}
vec3 gasEdge(){return u.surface.w==1.?u.palette[4].rgb:u.palette[5].rgb;}
vec2 corner(int id){return vec2(float(id&1),float(id>>1))*2.-1.;}

float dustLane(vec3 coarse,vec3 fine){
    return (1.-smoothstep(0.006,0.033,abs(fine.b+0.16*(coarse.g-0.5)-0.57)))*smoothstep(0.43,0.64,coarse.b);
}

// Local x runs from the middle of the exposure towards its head.
vec2 capsuleOffset(vec2 local, float lengthPx) {
    return vec2(local.x-clamp(local.x,-0.5*lengthPx,0.5*lengthPx), local.y);
}
float shutterGain(float radius, float lengthPx) {
    return max(0.35,pow(radius/(radius+0.35*lengthPx),0.60));
}
