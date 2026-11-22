#version 440
layout(location=0) in vec4 vertex;
layout(location=1) in vec2 uv;
layout(location=0) out vec2 coords;
layout(std140,binding=0) uniform Params { mat4 matrix; vec4 blend; } ub;
void main() { coords=uv; gl_Position=ub.matrix*vertex; }
