#version 440
layout(location=0) in vec2 position;
layout(location=1) in vec2 uv;
layout(location=0) out vec2 coord;
struct Band { vec4 geometry; vec4 inner; vec4 middle; };
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; Band bands[9]; vec4 shade; } ub;
void main() { coord=uv*ub.info.zw; gl_Position=ub.matrix*vec4(position,0.0,1.0); }
