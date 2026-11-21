#version 440
layout(location=0) in vec2 corner;
layout(location=1) in vec4 ab;
layout(location=2) in vec4 cd;
layout(location=3) in vec4 ef;
layout(location=4) in vec4 shape;
layout(location=5) in vec4 material;
layout(location=6) in vec4 pose;
layout(std140,binding=0) uniform Scene {
    vec4 eye; vec4 motion; vec4 glass; vec4 light; vec4 flare; vec4 colors[6]; vec4 rim; vec4 guide; vec4 mote; vec4 target;
} u;
layout(location=0) out vec2 local;
layout(location=1) flat out vec4 abOut;
layout(location=2) flat out vec4 cdOut;
layout(location=3) flat out vec4 efOut;
layout(location=4) flat out vec4 form;
layout(location=5) flat out vec4 pigment;
layout(location=6) flat out vec4 optics;
void main() {
    // A narrow strip of the actual round chamber includes the entire viewed
    // sector and a guard band on every side for the two bloom levels.
    float radius=shape.x+0.055;
    local=(corner*2.-1.)*radius;
    float c=cos(pose.z), s=sin(pose.z);
    vec2 chamber=pose.xy+mat2(c,s,-s,c)*(0.93*local);
    vec2 uv=(chamber-vec2(-1.05,-0.18))/vec2(2.10,1.26);
    gl_Position=vec4(uv.x*2.-1.,(uv.y*2.-1.)*u.target.x,0.,1.);
    abOut=ab; cdOut=cd; efOut=ef; form=shape; pigment=material;
    optics=vec4(mat2(c,-s,s,c)*u.light.xy,pose.w,pose.z);
}
