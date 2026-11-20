#version 440
layout(location=0) in vec2 bound;
layout(location=1) in vec4 local;
layout(location=2) in vec4 color;
layout(location=3) in float column;
layout(location=0) out vec2 uv;
layout(location=1) out vec4 tint;
layout(std140,binding=0) uniform buf {mat4 qt_Matrix;vec4 info;} ub;
layout(binding=1) uniform sampler2D offsets;
void main(){
    float y=texture(offsets,vec2((column+0.5)/ub.info.y,0.5)).x;
    gl_Position=ub.qt_Matrix*vec4(local.xy+vec2(0,y),0,1);
    uv=local.zw;tint=color*ub.info.x;
}
