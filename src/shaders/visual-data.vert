#version 440
layout(location=0) in vec2 position;
layout(location=1) in vec2 uv;
layout(location=0) out vec4 color;
struct Band { vec4 geometry; vec4 inner; vec4 middle; };
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; Band bands[9]; vec4 shade; } ub;
layout(binding=1) uniform sampler2D records;
void main() {
    vec4 data=texture(records,vec2(0.25,0.5));
    color=texture(records,vec2(0.75,0.5))*ub.info.x;
    gl_Position=ub.matrix*vec4(position+data.xy+uv*data.zw,0.0,1.0);
}
