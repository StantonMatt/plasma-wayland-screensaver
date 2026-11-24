#version 440
layout(location=0) in vec4 positionUV;
layout(location=1) in vec4 field;
layout(location=2) in vec4 material;
layout(std140,binding=0) uniform Camera {
    mat4 projection; vec4 bounds; vec4 colors[6]; vec4 top; vec4 bottom; vec4 style; vec4 clock; vec4 view; vec4 optics; vec4 overlap[8]; vec4 phases;
} sky;
layout(location=0) out vec2 uv;
layout(location=1) out vec4 light;
layout(location=2) flat out vec4 surface;
layout(location=3) out vec2 world;
layout(location=4) out float hemSlope;
void main() {
    vec2 p=positionUV.xy;
    if (material.x==1.) {
        vec2 pole=vec2(sky.clock.w,sky.bounds.y+0.24*sky.bounds.w);
        vec2 d=p-pole;
        p=pole+vec2(sky.clock.z*d.x-sky.clock.y*d.y,sky.clock.y*d.x+sky.clock.z*d.y);
    }
    world=p;
    uv=positionUV.zw;
    hemSlope=0.;
    if (material.x==2. || material.x==3.) {
        hemSlope=positionUV.w;
        uv.y=step(0.,field.z);
    }
    light=field;
    surface=material;
    gl_Position=sky.projection*vec4(p*sky.view.zw+sky.view.xy,0.,1.);
}
