#version 440
layout(location=0) in vec2 position;
layout(std140,binding=0) uniform Window {
    mat4 matrix; vec4 eye; vec4 motion; vec4 glass; vec4 light; vec4 flare;
    vec4 colors[6]; vec4 rim; vec4 guide; vec4 mote; vec4 display;
} u;
layout(location=0) out vec2 local;
void main(){local=position;gl_Position=u.matrix*vec4(position,0.,1.);}
