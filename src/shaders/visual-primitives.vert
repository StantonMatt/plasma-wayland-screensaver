#version 440
// Static repeated quads: corner [-1,1], record index. Three RGBA32F texels/record.
layout(location=0) in vec2 positionBounds;
layout(location=1) in vec2 recordIndex;
layout(location=0) out vec2 local;
layout(location=1) out vec4 color;
layout(location=2) out vec4 shape;
layout(location=3) out float detail;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; vec4 viewport; } ub;
layout(binding=1) uniform sampler2D records;
void main() {
    detail=0.0;
    if (recordIndex.x >= ub.info.y) {
        local=vec2(0);color=vec4(0);shape=vec4(1);gl_Position=ub.matrix*vec4(0,0,0,1);return;
    }
    float row=(recordIndex.x+0.5)/ub.info.z;
    vec4 g=texture(records,vec2(1.0/6.0,row));
    color=texture(records,vec2(0.5,row));
    vec4 s=texture(records,vec2(5.0/6.0,row));
    vec2 axis=vec2(1,0), center=g.xy;
    float halfLength=s.x;
    if (s.y<0.5 || s.y==7.0 || s.y==9.0 || s.y==10.0 || s.y==13.0) {
        vec2 delta=g.zw-g.xy;float len=length(delta);
        if (len>0.0) axis=delta/len;else color.a=0.0;
        halfLength=len*0.5;center=(g.xy+g.zw)*0.5;
    }
    if (s.y==11.0 || s.y==12.0) {
        vec2 delta=g.zw-g.xy;
        halfLength=max(length(delta),0.001);
        axis=delta/halfLength;
    }
    if (s.y==9.0 || s.y==12.0 || s.y==13.0) detail=s.w;
    vec2 corner=positionBounds/ub.viewport.xy*2.0-1.0;
    // ABI v2 kinds 1/2 ignore geometry.zw. Scaling is extension-only.
    bool scaled=s.y>=3.0 && s.y<=5.0 && g.z>0.0 && g.w>0.0;
    vec2 scale=scaled ? g.zw : vec2(1);
    float nib=s.y==9.0 ? 1.5 : (s.y==8.0 ? 1.35 : 0.0);
    local=corner*(vec2(halfLength,s.x)+vec2(nib)+vec2(ub.info.w)/scale);
    if (s.y==10.0) {
        float halfBend=halfLength*(85.0/245.0);
        local.y+=halfBend*(1.0+corner.y);
    }
    shape=vec4(s.x,halfLength,s.y,scaled ? 1.0 : -s.z);
    vec2 offset=(axis*local.x+vec2(-axis.y,axis.x)*local.y)*scale;
    vec2 position=center+offset;
    gl_Position=ub.matrix*vec4(position,0,1);
}
