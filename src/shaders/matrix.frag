#version 440
layout(location=0) in vec2 uv;
layout(location=1) in vec4 tint;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform buf {mat4 qt_Matrix;vec4 info;} ub;
layout(binding=2) uniform sampler2D atlas;
void main(){fragColor=tint*texture(atlas,uv).r;}
