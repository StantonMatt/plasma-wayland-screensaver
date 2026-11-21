#version 440
layout(location=0) in vec2 corner;
layout(location=0) out vec2 uv;
layout(std140,binding=0) uniform Blur { vec4 params; } u;
void main(){
    uv=corner;
    gl_Position=vec4(2.*corner.x-1.,(2.*corner.y-1.)*u.params.z,0.,1.);
}
