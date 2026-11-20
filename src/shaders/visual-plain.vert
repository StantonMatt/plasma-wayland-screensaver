#version 440
// Rust-expanded fallback; same fragment coverage and ordered blending.
layout(location=0) in vec2 position;
layout(location=1) in vec2 vertexLocal;
layout(location=2) in vec4 vertexColor;
layout(location=3) in vec4 vertexShape;
layout(location=0) out vec2 local;
layout(location=1) out vec4 color;
layout(location=2) out vec4 shape;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; vec4 viewport; } ub;
void main() {local=vertexLocal;color=vertexColor;shape=vertexShape;gl_Position=ub.matrix*vec4(position,0,1);}
