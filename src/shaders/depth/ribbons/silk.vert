#version 440
layout(location=0) in vec2 corner;
layout(location=1) in vec4 lowerA;
layout(location=2) in vec4 lowerSilkA;
layout(location=3) in vec4 upperA;
layout(location=4) in vec4 upperSilkA;
layout(location=5) in vec4 lowerB;
layout(location=6) in vec4 lowerSilkB;
layout(location=7) in vec4 upperB;
layout(location=8) in vec4 upperSilkB;
layout(location=9) in uint tagValue;
layout(location=10) in uint nextTag;
layout(binding=1) uniform highp sampler2D capRecords;
layout(location=0) out vec4 shape;
layout(location=1) out vec4 surface;
layout(location=2) out vec3 moteColor;
layout(location=3) flat out vec4 front;
layout(location=4) flat out vec4 back;
layout(location=5) flat out vec4 haloColour;
layout(location=6) flat out vec4 accent;
layout(location=7) flat out float deviceScale;
layout(location=8) out vec3 headGeometry;
layout(location=9) flat out float shoulderGain;
layout(std140,binding=0) uniform Scene {
    mat4 matrix; vec4 info; vec4 silks[72]; vec4 shoulders[80];
} ub;
// Curvature is a scalar packed in the normal length. Interpolating the
// packed vectors would let the more curved tick pull the direction sideways.
vec2 knotDirection(vec2 lo, vec2 hi, float alpha, out float encoded) {
    float lenLo=length(lo), lenHi=length(hi);
    vec2 blend=mix(lo/max(lenLo,0.00001),hi/max(lenHi,0.00001),alpha);
    // A rotating chord shortens between ticks. Its unit direction also
    // rotates faster after normalization. Reserve width for both effects.
    float squared=clamp(dot(blend,blend),0.00001,1.);
    encoded=1.+max(0.,max(lenLo,lenHi)-1.)/squared;
    vec2 n=blend*inversesqrt(squared);
    return vec2(n.y,-n.x);
}
vec4 readCap(uint tag, float alpha) {
    uint index=tag>>16u;
    if(index==0u) return vec4(0.);
    --index;
    ivec2 xy=ivec2((index&31u)*2u,index>>5u);
    return mix(texelFetch(capRecords,xy,0),texelFetch(capRecords,xy+ivec2(1,0),0),alpha);
}
void main() {
    uint tag=tagValue&65535u;
    bool spark=tag>=4096u;
    uint palette=(tag & 31u)*4u;
    if (tag==65535u) {
        gl_Position=vec4(2.,2.,0.,1.);
        shape=vec4(0.);surface=vec4(0.);moteColor=vec3(0.);
        return;
    }
    front=ub.silks[palette];
    back=ub.silks[palette+1u];
    haloColour=ub.silks[palette+2u];
    accent=ub.silks[palette+3u];
    deviceScale=ub.info.x;
    vec4 pointA=mix(lowerA,upperA,ub.info.z);
    vec4 pointB=mix(lowerB,upperB,ub.info.z);
    vec4 silkA=mix(lowerSilkA,upperSilkA,ub.info.z);
    vec4 silkB=mix(lowerSilkB,upperSilkB,ub.info.z);
    float side=corner.y;
    if (spark) {
        vec2 uv=vec2(min(1.,corner.x)*2.-1., side);
        gl_Position=ub.matrix*vec4(pointA.xy+uv*(silkA.x*2.4),0.,1.);
        shape=vec4(uv.y,1.,1.,0.);
        surface=vec4(silkA.x,silkA.w*ub.info.y,uv.x,1.);
        moteColor=vec3(pointA.zw,silkA.y);
        return;
    }
    vec2 gains=ub.shoulders[tag>>5u].xy;
    shoulderGain=mix(gains.x,gains.y,ub.info.z)*ub.info.y;
    // The shared Catmull-Rom tangent at each simulated node is retained in
    // its normal. Three Hermite spans divide the constrained turn (<=18 deg).
    float t=corner.x/3.;
    vec2 delta=pointB.xy-pointA.xy;
    float chord=length(delta);
    if (chord<0.0001) {
        // At the beginning of growth an unmatched endpoint collapses onto
        // its predecessor. It covers no area at this presentation fraction.
        gl_Position=vec4(2.,2.,0.,1.);
        shape=vec4(0.);surface=vec4(0.);moteColor=vec3(0.);
        return;
    }
    float curveA, curveB;
    vec2 ta=knotDirection(lowerA.zw,upperA.zw,ub.info.z,curveA);
    vec2 tb=knotDirection(lowerB.zw,upperB.zw,ub.info.z,curveB);
    vec2 m0=ta*chord, m1=tb*chord;
    vec2 c2=3.*delta-2.*m0-m1, c3=m0+m1-2.*delta;
    vec2 centre=pointA.xy+t*(m0+t*(c2+t*c3));
    vec2 normal;
    vec4 silk;
    float encoded;
    if (t==0.) {
        centre=pointA.xy; normal=vec2(-ta.y,ta.x); silk=silkA; encoded=curveA;
    } else if (t==1.) {
        centre=pointB.xy; normal=vec2(-tb.y,tb.x); silk=silkB; encoded=curveB;
    } else {
        vec2 knotTangent=mix(ta,tb,t);
        normal=vec2(-knotTangent.y,knotTangent.x);
        silk=mix(silkA,silkB,t);
        encoded=mix(curveA,curveB,t);
    }
    float rad=silk.x*(0.12+0.88*abs(silk.y));
    float soft=back.w<0.5 ? 1.45 : back.w>1.5 ? 0.19*rad : 0.40;
    float aa=max(1.2/ub.info.x,soft*1.4);
    float curvature=max(0.00001,(encoded-1.)/50.);
    float haloWidth=min(silk.x,max(0.,0.85/curvature-rad-aa)/max(haloColour.w*7.,0.00001));
    vec4 capA=readCap(tagValue,ub.info.z);
    vec4 capB=readCap(nextTag,ub.info.z);
    bool ahead=capA.y<0.;
    vec4 cap=t==0. ? capA : t==1. ? capB : mix(capA,capB,t);
    float capMix=ahead ? 1. : clamp(cap.y,0.,1.);
    float envelope=mix(haloWidth*haloColour.w*7.,1.2*cap.x,capMix);
    float outer=min(rad+aa+envelope,0.85/curvature);
    vec2 world=centre+side*outer*normal;
    gl_Position=ub.matrix*vec4(world,0.,1.);
    // The forward span and its first material neighbour know the exact tip.
    // Extrapolate along fitted tangents only farther behind the shoulder.
    vec2 tip = ahead ? pointB.xy : capA.z==0. ? pointA.xy
        : mix(pointA.xy+ta*capA.z,pointB.xy+tb*capB.z,t);
    headGeometry=vec3(world-tip,outer);
    shape=vec4(side*outer,rad,silk.y,silk.z);
    surface=vec4(haloWidth,silk.w*ub.info.y,cap.z,ahead ? -1. : 0.);
    moteColor=vec3(cap.x,capMix,cap.w);
}
