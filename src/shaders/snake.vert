#version 440
layout(location=0) in vec2 position;
layout(location=1) in vec2 uv;
layout(location=2) in vec4 color;
layout(location=3) in vec4 params;
layout(location=0) out vec2 coord;
layout(location=1) out vec4 base;
layout(location=2) out vec4 packed;
layout(location=3) out vec2 screenPosition;
layout(location=4) out float ribbonLimit;
layout(std140,binding=0) uniform buf { mat4 matrix; float opacity; float time; vec2 light; float animationTime; float motionScale; } ub;
void main() { ribbonLimit=abs(uv.x); screenPosition=position; coord=uv; base=color; packed=params*255.0; gl_Position=ub.matrix*vec4(position,0.0,1.0); }
