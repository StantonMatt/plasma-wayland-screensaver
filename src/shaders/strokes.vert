#version 440
layout(location=0) in vec2 position;
layout(location=1) in vec4 shape;
layout(location=2) in vec2 parameters;
layout(location=3) in vec4 color;
layout(location=0) out vec4 vShape;
layout(location=1) out vec2 vParameters;
layout(location=2) out vec4 vColor;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; } ubuf;
void main() {
    gl_Position=ubuf.matrix*vec4(position,0.0,1.0);
    vShape=shape;vParameters=parameters;vColor=color;
}
