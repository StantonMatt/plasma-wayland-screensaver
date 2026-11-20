#version 440
// Static repeated quads: corner [-1,1], record index. Three RGBA32F texels/record.
layout(location=0) in vec2 positionBounds;
layout(location=1) in vec2 recordIndex;
layout(location=0) out vec2 local;
layout(location=1) out vec4 color;
layout(location=2) out vec4 shape;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; vec4 viewport; } ub;
layout(binding=1) uniform sampler2D records;
void main() {
    if (recordIndex.x >= ub.info.y) {
        local=vec2(0);color=vec4(0);shape=vec4(1);gl_Position=ub.matrix*vec4(0,0,0,1);return;
    }
    float row=(recordIndex.x+0.5)/ub.info.z;
    vec4 g=texture(records,vec2(1.0/6.0,row));
    color=texture(records,vec2(0.5,row));
    vec4 s=texture(records,vec2(5.0/6.0,row));
    vec2 axis=vec2(1,0), center=g.xy;
    float halfLength=s.x;
    if (s.y<0.5) {
        vec2 delta=g.zw-g.xy;float len=length(delta);
        if (len>0.0) axis=delta/len;else color.a=0.0;
        halfLength=len*0.5;center=(g.xy+g.zw)*0.5;
    }
    vec2 corner=positionBounds/ub.viewport.xy*2.0-1.0;
    local=corner*(vec2(halfLength,s.x)+ub.info.w);
    shape=vec4(s.x,halfLength,s.y,0);
    vec2 position=center+axis*local.x+vec2(-axis.y,axis.x)*local.y;
    gl_Position=ub.matrix*vec4(position,0,1);
}
