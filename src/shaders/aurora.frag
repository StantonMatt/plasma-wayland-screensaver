#version 440
layout(location=0) in vec2 coord;
layout(location=0) out vec4 fragColor;
struct Band { vec4 geometry; vec4 inner; vec4 middle; };
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; Band bands[9]; vec4 shade; } ub;
void main() {
    vec4 result=vec4(0.0);
    for (int i=0;i<9;++i) {
        if (float(i)>=ub.info.y) break;
        float t=clamp(length(coord-ub.bands[i].geometry.xy)/ub.bands[i].geometry.z,0.0,1.0);
        // QPainter interpolates premultiplied gradient stops: opaque 0/.5,
        // transparent black at 1. Preserve band order and source-over alpha.
        float a=ub.bands[i].geometry.w*min(1.0,2.0-2.0*t);
        vec3 c=mix(ub.bands[i].inner.rgb,ub.bands[i].middle.rgb,min(1.0,t*2.0));
        result=vec4(c*a,a)+result*(1.0-a);
    }
    float t=coord.y/ub.info.w;
    float shade=(t<ub.shade.y ? ub.shade.x*(1.0-t/ub.shade.y)
        : ub.shade.z*(t-ub.shade.y)/(1.0-ub.shade.y))*ub.shade.w;
    fragColor=(vec4(0.0,0.0,0.0,shade)+result*(1.0-shade))*ub.info.x;
}
