#version 440
layout(location=0) in vec2 uv;
layout(binding=1) uniform sampler2D source;
layout(std140,binding=0) uniform Blur {vec4 params;} u;
layout(location=0) out vec4 color;
void main(){
    vec2 d=u.params.xy;
    vec3 c=texture(source,uv).rgb*4.;
    c+=texture(source,uv+d).rgb+texture(source,uv-d).rgb;
    c+=texture(source,uv+vec2(d.x,-d.y)).rgb+texture(source,uv+vec2(-d.x,d.y)).rgb;
    color=vec4(c*0.125,1.);
}
