#version 440
layout(location=0) in vec2 positionBounds;
layout(location=1) in vec2 index;
layout(location=0) out vec2 uv;
layout(location=1) out vec4 pigment;
layout(location=2) out vec4 surface; // material, sin(spin), cos(spin), radius
layout(location=4) out vec4 coating;
layout(location=5) out vec2 cabinetPosition;
layout(location=3) out vec4 light;   // glow, flash, palette, record kind
layout(std140,binding=0) uniform buf {
    mat4 matrix;
    vec4 viewport; // item width, height, record capacity, inherited opacity
    vec4 mapping;  // origin, uniform scale, unused
    vec4 ranges; // count of effects, trail headers and spheres
    vec4 top;
    vec4 bottom;
    vec4 cabinet;
} ub;
layout(binding=1) uniform sampler2D records;
layout(binding=2) uniform sampler2D knots;

vec4 knot(float cursor, float slot, float offset, float component) {
    return texture(knots, vec2((2.*mod(cursor-offset,32.)+component+.5)/64.,(slot+.5)/61.));
}
vec2 tangent(vec2 velocity, vec2 chord, float dt) {
    float span=length(chord);
    if(dot(velocity,chord)<=0.) return chord;
    return velocity*min(dt,1.25*span/max(length(velocity),.001));
}
void main(){
    bool trail=index.y>=0.;
    bool sphere=index.y< -1.5;
    float count=sphere ? ub.ranges.z : (trail ? ub.ranges.y : ub.ranges.x);
    cabinetPosition=vec2(0); uv=vec2(0); coating=vec4(0); pigment=vec4(0); surface=vec4(0); light=vec4(0);
    gl_Position=ub.matrix*vec4(0,0,0,1);
    if(index.x>=count) return;
    float base=sphere ? ub.ranges.x+ub.ranges.y : (trail ? ub.ranges.x : 0.);
    float row=(base+index.x+0.5)/ub.viewport.z;
    vec4 g=texture(records,vec2(0.125,row));
    vec4 a=texture(records,vec2(0.375,row));
    pigment=texture(records,vec2(0.625,row));
    light=texture(records,vec2(0.875,row));
    vec2 corner=positionBounds/ub.viewport.xy*2.0-1.0;
    vec2 p;surface=vec4(0);uv=corner;
    if(sphere){
        // A 24-bit sRGB pigment is exactly representable in an RGBA32F texel.
        coating.rgb=mod(floor(light.z/vec3(65536.,256.,1.)),256.)/255.;
        coating.w=floor(a.w/32.);
        light.z=mod(floor(a.w/4.),8.);
        float compress=1.0-a.z;
        vec2 tangent=vec2(-a.y,a.x);
        vec2 stretch=a.xy*dot(a.xy,corner)*compress+tangent*dot(tangent,corner)/compress;
        // Leave a pixel of transparent fringe for derivative AA.
        float fringe=1.0+1.2/max(1.0,g.z*ub.mapping.z);
        p=g.xy+g.z*(stretch*fringe-a.xy*max(a.z,0.0));uv=corner*fringe;
        surface=vec4(mod(a.w,4.),sin(g.w),cos(g.w),g.z*ub.mapping.z);
    }else if(!trail){
        p=g.xy+a.xy*g.z*corner.x+vec2(-a.y,a.x)*g.w*corner.y;
        surface.w=g.w*ub.mapping.z;
    }else{
        // One oriented bounding quad per trail. The fragment shader shades the
        // union of the curve and its round knots exactly once, including at folds.
        vec2 tip=knot(a.x,a.z,0.,0.).xy;
        vec2 tail=knot(a.x,a.z,a.y-1.,0.).xy;
        float headDistance=knot(a.x,a.z,0.,1.).z+length(g.xy-tip);
        float path=mod(headDistance-knot(a.x,a.z,a.y-1.,1.).z,65536.);
        if(path<g.z*.85) return;
        vec2 direction=g.xy-tail;
        direction=dot(direction,direction)>.01 ? normalize(direction) : vec2(1,0);
        vec2 normal=vec2(-direction.y,direction.x);
        mat2 basis=mat2(direction,normal);
        vec2 lo=vec2(0),hi=vec2(0);
        vec4 young=vec4(g.xy,a.w,light.x);
        vec2 velocity=light.yz;
        for(int i=0;i<32;++i){
            if(float(i)>=a.y) break;
            vec4 old=knot(a.x,a.z,float(i),0.);
            vec2 v0=knot(a.x,a.z,float(i),1.).xy;
            vec2 chord=young.xy-old.xy;
            float dt=max(0.,young.z-old.z);
            vec2 start=tangent(v0,chord,dt);
            vec2 end=young.w<0. ? chord : tangent(velocity,chord,dt);
            // Bezier controls bound every point of the Hermite interval.
            vec2 k0=(old.xy-g.xy)*basis;
            vec2 k1=(old.xy+start/3.-g.xy)*basis;
            vec2 k2=(young.xy-end/3.-g.xy)*basis;
            lo=min(lo,min(k0,min(k1,k2)));
            hi=max(hi,max(k0,max(k1,k2)));
            young=old;velocity=v0;
        }
        float padding=g.z*(.47+ub.ranges.w);
        p=g.xy+basis*mix(lo-padding,hi+padding,.5+.5*corner);
        surface=g;coating=a;uv=vec2(headDistance,path);
    }
    cabinetPosition=p;
    vec2 position=(p-ub.mapping.xy)*ub.mapping.z;
    gl_Position=ub.matrix*vec4(position,0.,1.);
}
