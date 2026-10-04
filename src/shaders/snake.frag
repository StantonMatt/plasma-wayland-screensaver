#version 440
// SPDX-License-Identifier: GPL-3.0-or-later
// Single-pass tube, skin and additive light. Payload documented in render/README.md.
layout(location=0) in vec2 coord;
layout(location=1) in vec4 base;
layout(location=2) in vec4 packed;
layout(location=3) in vec2 screenPosition;
layout(location=4) in float ribbonLimit;
layout(location=5) in vec3 waveLight;
layout(location=0) out vec4 fragColor;
layout(binding=1) uniform sampler2D iconAtlas;
layout(std140,binding=0) uniform buf { mat4 matrix; float opacity; float time; vec2 light; float animationTime; float motionScale; float paletteMode; } ub;
// Geometry contract: literal constants are also read at Rust compile time by
// render/shader/bounds.rs. UV mapping, extrusion and copy selection share these values.
const float BOUNDS_AA = 0.04; // effect coordinates: 7 body radii per unit
const float BOUNDS_HEAD_AA = 0.2;
const float BOUNDS_FOOD_AA = 0.8;
const float BOUNDS_BODY = 2.5;
const float BOUNDS_BREATH = 1.028;
const float BOUNDS_HEAD_BACK = 1.0;
const float BOUNDS_HEAD_BOOST_BACK = 2.3;
const float BOUNDS_HEAD_FRONT = 2.9;
const float BOUNDS_HEAD_SIDE = 1.75;
const float BOUNDS_HEAD_BOOST_SIDE = 2.5;
const float BOUNDS_FOOD = 4.6;
const float BOUNDS_FOOD_PIXEL = 0.6;
const float BOUNDS_EFFECT_UNITS = 7.0;
const float BOUNDS_IMPACT = 10.2;
const float BOUNDS_RING = 7.0;
const float BOUNDS_MAGNET = 10.2;
const float BOUNDS_CAPSULE = 3.4;
const float BOUNDS_CONTRAIL = 0.465;
const float BOUNDS_VACUUM = 0.65;
const float BOUNDS_DEVELOPER = 1.2;
const float BOUNDS_CORPSE_DRIFT = 1.6;
vec2 gradient(float value) {
    vec2 dx=dFdx(screenPosition),dy=dFdy(screenPosition);
    float determinant=dx.x*dy.y-dx.y*dy.x;
    vec2 v=vec2(dFdx(value)*dy.y-dFdy(value)*dx.y,dFdy(value)*dx.x-dFdx(value)*dy.x);
    return normalize(v*sign(determinant)+vec2(0.000001));
}
float coverage(float d,float aa) { return 1.0-smoothstep(-aa,aa,d); }
float mask(float d) {
    float aa=max(fwidth(d),0.008);
    int kind=int(packed.x+0.5);
    // Body support is compacted at its actual ribbon edge. Keep its original
    // pixel AA; the sprite ceilings retain pixel AA at normal rendered sizes.
    if(kind==0) return coverage(d,aa);
    float limit=kind==1?BOUNDS_HEAD_AA:(kind==2||kind==3||kind==4||kind==8)?BOUNDS_FOOD_AA:BOUNDS_AA;
    return coverage(d,min(aa,limit));
}
float ellipse(vec2 p,vec2 r) { return (length(p/r)-1.0)*min(r.x,r.y); }
float line(vec2 p,vec2 a,vec2 b,float w) { vec2 d=b-a;return length(p-a-d*clamp(dot(p-a,d)/max(dot(d,d),0.000001),0.0,1.0))-w; }
// iq's signed triangle distance; only the three capped bubble quads use it.
float sdTriangle(vec2 p,vec2 p0,vec2 p1,vec2 p2) {
    vec2 e0=p1-p0,e1=p2-p1,e2=p0-p2;
    vec2 v0=p-p0,v1=p-p1,v2=p-p2;
    vec2 pq0=v0-e0*clamp(dot(v0,e0)/dot(e0,e0),0.0,1.0);
    vec2 pq1=v1-e1*clamp(dot(v1,e1)/dot(e1,e1),0.0,1.0);
    vec2 pq2=v2-e2*clamp(dot(v2,e2)/dot(e2,e2),0.0,1.0);
    float orientation=sign(e0.x*e2.y-e0.y*e2.x);
    vec2 d=min(min(vec2(dot(pq0,pq0),orientation*(v0.x*e0.y-v0.y*e0.x)),
                   vec2(dot(pq1,pq1),orientation*(v1.x*e1.y-v1.y*e1.x))),
                   vec2(dot(pq2,pq2),orientation*(v2.x*e2.y-v2.y*e2.x)));
    return -sqrt(d.x)*sign(d.y);
}
float hash(float n) { return fract(sin(n*127.1)*43758.5453); }
vec3 lift(vec3 c) { float l=dot(c,vec3(0.2126,0.7152,0.0722));return mix(c,vec3(1),max(0.0,0.30-l)*1.1); }
float falloff(float u) { u=1.0-clamp(u,0.0,1.0); return u*u; }   // matches the prototype's radial glow sprite
// Premultiplied over; `glow` is additive outside opaque coverage, `over` is additive on top.
vec4 composite(vec3 rgb,float a,vec3 glow,vec3 over,float fade) { return vec4(rgb*a+glow*(1.0-a)+over,a)*fade*ub.opacity; }
const float SEG=1.18;  // segment spacing in body radii (world.rs spacing = radius*1.18)
vec3 itemAccent(int k) {
    vec3 a;
    if(k==1) a=vec3(1.0,0.88235,0.30196);
    else if(k==2) a= vec3(1.0,0.37255,0.82353);
    else if(k==3) a= vec3(0.66275,0.54510,1.0);
    else if(k==4) a= vec3(0.61569,1.0,0.22745);
    else a=vec3(0.74118,0.95294,1.0);
    if(ub.paletteMode==1.0) a=mix(a,vec3(dot(a,vec3(0.2126,0.7152,0.0722))),0.88);
    if(ub.paletteMode==2.0) a=mix(a,vec3(1),0.3);
    return a;
}
// One padded SDF sample shared by capsules and speech glyphs.
float atlasMask(int icon,vec2 p,float extent) {
    vec2 tile=vec2(float(icon%4),float(icon/4));
    vec2 uv=clamp(p/extent,-1.0,1.0)*0.5+0.5;
    float distance=texture(iconAtlas,(tile*32.0+vec2(0.5)+uv*31.0)/128.0).r-0.5;
    return smoothstep(-max(fwidth(distance),0.025),max(fwidth(distance),0.025),distance)
        *float(max(abs(p.x),abs(p.y))<extent);
}
vec3 moodAccent(vec3 c) {
    if(ub.paletteMode==1.0) return vec3(dot(c,vec3(0.2126,0.7152,0.0722)));
    return ub.paletteMode==2.0?mix(c,vec3(1),0.3):c;
}
float hexagon(vec2 p,float r) {
    const vec3 k=vec3(-0.8660254,0.5,0.5773503);
    p=abs(p);p-=2.0*min(dot(k.xy,p),0.0)*k.xy;
    p-=vec2(clamp(p.x,-k.z*r,k.z*r),r);
    return length(p)*sign(p.y);
}
void main() {
    int kind=int(packed.x+0.5), tier=int(packed.y+0.5)&3, flags=int(packed.z+0.5);
    bool boosting=(flags&1)!=0, hunting=(flags&4)!=0, trapped=(flags&8)!=0, leader=(flags&64)!=0;
    int mood=kind==1?(flags>>1)&15:0;
    if(kind==1) {hunting=mood==2;trapped=mood==6;}
    vec3 c=lift(base.rgb); vec3 white=vec3(1);vec3 gold=vec3(1.0,0.847,0.29);
    float chroma=max(base.r,max(base.g,base.b))-min(base.r,min(base.g,base.b));
    bool mono=chroma<0.05;
    // Palette identity is packed by Rust, independent of an individual colour.
    if((kind==0 || kind==1 || kind==7) && (int(packed.y+0.5)&64)!=0) gold=mix(gold,white,0.85);
    float t=ub.time;
    if(kind==11) {
        vec2 p=coord*BOUNDS_CAPSULE;float angle=t*0.18;
        vec2 q=mat2(cos(angle),-sin(angle),sin(angle),cos(angle))*p;
        float h=hexagon(q,0.8660254);
        float body=mask(h);
        float rim=mask(abs(h)-0.035);
        float inner=mask(abs(hexagon(q,0.675))-0.015)*0.35;
        vec3 accent=base.rgb;
        vec3 rgb=mix(vec3(0.022,0.029,0.055),accent,max(rim,inner));
        float iconMask=atlasMask(int(packed.y+0.5)-1,p,0.66);
        rgb=mix(rgb,mix(accent,white,0.35),iconMask);
        float orbit=t*2.2;
        float spark=falloff(length(p-vec2(cos(orbit),sin(orbit))*1.3)/0.5);
        vec3 glow=accent*(falloff(length(p)/BOUNDS_CAPSULE)*0.32+spark*0.8);
        bool incoming=packed.z>=128.0;
        float birth=incoming?(packed.z-128.0)/127.0:packed.z/127.0;
        if(incoming) {
            // Same capsule quad: closing target, four ticks, dashed ghost hex.
            float radius=2.8-1.8*birth;
            float target=mask(abs(length(p)-radius)-0.035)*(0.45+0.40*birth);
            float ticks=mask(line(abs(q),vec2(radius+0.10,0),vec2(radius+0.45,0),0.04))
                +mask(line(abs(q),vec2(0,radius+0.10),vec2(0,radius+0.45),0.04));
            float dash=step(0.5,fract(atan(q.y,q.x)*6.0/3.141593));
            vec3 telegraph=accent*(target+ticks*0.65+rim*dash*0.4+iconMask*(0.25+0.35*birth));
            fragColor=vec4(min(telegraph,vec3(0.9))*base.a*ub.opacity,0);return;
        }
        if(birth<1.0) glow+=accent*mask(abs(length(p)-(3.0-2.0*birth))-0.035)*(1.0-birth)*0.7;
        float life=packed.w/255.0*25.0;
        float blink=life<3.0?0.65+0.35*sin(t*(18.0+24.0*(1.0-life/3.0))):1.0;
        fragColor=composite(rgb,body,min(glow,vec3(0.9)),vec3(0),blink);return;
    }
    if(kind==16) {
        // Screen-aligned speech quad with a minimum half-extent of 12.5px.
        vec2 p=coord;
        float circle=length(p-vec2(0,-0.10))-0.75;
        float tail=sdTriangle(p,vec2(-0.29,0.52),vec2(-0.525,0.34),vec2(-0.66,0.84));
        float d=min(circle,tail);
        float aa=fwidth(d);
        float fill=coverage(d,aa),rim=coverage(abs(d)-max(0.042,0.75*aa),aa);
        float glyph=atlasMask(7+int(packed.y+0.5),p-vec2(0,-0.10),0.61);
        vec3 rgb=mix(vec3(0.025,0.035,0.055),base.rgb,max(rim*0.85,glyph));
        fragColor=vec4(rgb*fill,fill)*base.a*ub.opacity;return;
    }
    if(kind==17) {
        float c=packed.y/255.0;
        float angle=packed.z/255.0*6.283185;
        // Rotated dot avoids atan per arc pixel; CPU supplies the direction.
        float angular=dot(normalize(coord+vec2(0.000001)),vec2(cos(angle),sin(angle)));
        float r=length(coord);
        float px=max(length(vec2(dFdx(r),dFdy(r))),0.0001);
        int state=int(packed.w+0.5);
        float pulse=(state&2)!=0 && ub.motionScale==1.0?0.84+0.16*sin(t*6.911504):1.0;
        float hw=0.35+0.85*c;
        float arc=smoothstep(cos(hw)-0.03,cos(hw)+0.03,angular);
        bool lead=(state&1)!=0;
        float halfStroke=(1.0+1.2*c)*(lead?1.15:0.9);
        float dr=abs(r-1.62);
        float core=coverage(dr-halfStroke*px,px)*arc;
        float h=max(0.0,1.0-dr/((3.0*halfStroke+2.0)*px)); float halo=h*h*arc;
        float a=(0.5+0.5*c)*(lead?1.0:0.8)*pulse;
        vec3 col=mix(lift(base.rgb),vec3(1),0.25);
        float track=(state&4)!=0?coverage(dr-0.5*px,px)*0.10:0.0;
        fragColor=vec4((col*(core+0.25*halo)*a+vec3(track))*ub.opacity,0);return;
    }
    if(kind==12 || kind==13 || kind==14) {
        float age=packed.w/255.0;float r=length(coord);vec3 accent=base.rgb;
        float radius=kind==12?1.0+5.0*(1.0-pow(1.0-age,2.0)):kind==14?2.0:2.5*(1.0-age);
        float ring=mask(abs(r-radius)-0.08)*(kind==14?1.0:1.0-age)*0.8;
        vec3 glow=accent*ring;
        if(kind==12) glow+=white*mask(abs(r-radius*0.65)-0.055)*(1.0-age)*0.85;
        if(kind==14) glow*=0.5+0.5*sin(t*22.0);
        fragColor=vec4(min(glow,vec3(0.9))*base.a*ub.opacity,0);return;
    }
    if(kind==15) {
        // One head-centred quad, in body radii. The reach stays exactly 9r.
        float radius=length(coord);
        float angle=atan(coord.y,coord.x)-t*1.2;
        float dash=step(0.75,fract(angle*9.0/3.141593));
        float ring=mask(abs(radius-9.0)-0.08)*dash*0.4;
        vec3 glow=base.rgb*ring;
        for(int k=0;k<3;k++) {
            float a=t*1.2+float(k)*2.094395;
            float spark=length(coord-vec2(cos(a),sin(a))*9.0);
            glow+=base.rgb*falloff(spark/1.1)*0.9;
        }
        fragColor=vec4(min(glow,vec3(0.9))*base.a*ub.opacity,0);return;
    }
    if(kind==0) {
        // uv.x = +-taper at each vertex, colour alpha = taper. acrossR is the true signed
        // distance from the centre line in radii (affine, no trapezoid kink); w is the local half-width.
        float breath=1.0+(BOUNDS_BREATH-1.0)*sin(t*2.4+coord.y*0.32);
        float w=max(base.a,1.0/255.0)*breath;
        float acrossR=coord.x*BOUNDS_BODY;
        float tipK=clamp(coord.y,0.0,1.0);
        float sd=abs(acrossR)-w*tipK;           // radii, <0 inside the tube
        float d=1.0+sd/w;                        // 1 at the silhouette, like the old |across|
        float across=acrossR/max(w*tipK,0.001);  // band coordinate, +-1 at the silhouette
        vec2 normal=gradient(coord.x);
        float lit=dot(normal,ub.light);
        // Shared derivatives for slope-one bands and silhouette/shadow/halo.
        float aaA=max(fwidth(across),0.008), aaD=max(fwidth(d),0.008);
        float body=coverage(d-1.0,aaD);
        float shadow=coverage(d-1.14,aaD)*0.62;
        vec3 tube=c*0.45;
        tube=mix(tube,c*0.78,coverage(abs(across-lit*0.10)-0.84,aaA));
        tube=mix(tube,c,coverage(abs(across-lit*0.22)-0.60,aaA));
        vec3 over=vec3(0);
        if(tier>0) {
            // one chevron every 2 segments, prototype geometry, stroke 0.20r, pixel AA.
            float local=(fract(coord.y*0.5)-0.5)*2.0*SEG;
            float arm=abs(local+0.873*abs(acrossR)-0.20*w)*0.753;
            float aa=fwidth(arm);
            float ca=fwidth(acrossR);
            float chev=(1.0-smoothstep(0.10-aa,0.10+aa,arm))*(1.0-smoothstep(0.55*w-ca,0.55*w+ca,abs(acrossR)))*step(4.0,coord.y)*float((int(packed.y+0.5)&32)==0);
            float saddle=0.0;
            if(tier>1) {
                float sdist=abs(mod(coord.y+3.0,6.0)-3.0)*SEG;
                float sa=fwidth(sdist);
                saddle=(1.0-smoothstep(0.25-sa,0.25+sa,sdist))*coverage(abs(across)-0.92,aaA);
            }
            tube=mix(tube,c*0.38,max(chev*0.55,saddle*0.42));
        }
        float sheen=coverage(abs(across-lit*0.50)-0.20,aaA)*0.55;
        tube=mix(tube,mix(c,white,0.62),sheen);
        bool corpse=(flags&128)!=0;
        // waves are additive light (prototype: glow sprite 2.4w, alpha 0.5*strength), not a flat tint.
        int effectKind=(int(packed.y+0.5)>>2)&7;
        over+=waveLight*0.5*falloff(abs(acrossR)/(2.4*w));
        float extent=tier==0?1.75:tier==1?1.85:tier==2?2.0:2.15;
        float strength=(tier==1?0.09:tier==3?0.13:0.11)+(boosting?0.14:0.0)+(leader?0.04:0.0);
        if(trapped) strength*=0.65+0.45*abs(sin(t*12.0));
        float halo=(1.0-smoothstep(extent-0.25,extent,d))*0.55+coverage(d-1.42,aaD);   // inner zone 1.55x like the prototype
        vec3 glow=c*halo*strength;
        if(tier==3) {
            // dot measured in radii (along scaled by SEG), additive dot + 1.2w glow on top.
            vec2 q=vec2(acrossR,(mod(coord.y+1.5,3.0)-1.5)*SEG);
            float dd=length(q)/w;
            float ripple=0.3+0.7*pow(max(0.0,sin(t*2.2+coord.y*0.11)),4.0);
            vec3 lc=mix(c,white,0.7);
            over+=lc*(mask(dd-0.15)*(0.4+0.6*ripple)+falloff(dd/1.2)*0.5*ripple)*body;
        }
        float activeFade=1.0;
        if(!corpse && effectKind==1) {
            // Prototype: a 30% chance every two segments, three edges long,
            // 18 Hz reseeding, and a 1.2 screen-pixel stroke at 85% brightness.
            // Two candidates cover overlapping arcs without extra vertices.
            float frame=floor(t*18.0);
            float pixelR=max(length(vec2(dFdx(acrossR),dFdy(acrossR))),0.0001);
            float arc=0.0;
            for(int k=0;k<2;k++) {
                float start=floor((coord.y-1.0)/2.0)*2.0+1.0-float(k)*2.0;
                float local=coord.y-start;
                float seed=frame*13.0+start+base.r*71.0;
                float side=hash(frame+start*3.0)>0.5?1.0:-1.0;
                float edge=clamp(floor(local),0.0,2.0);
                float a=(1.05+0.35*hash(frame*7.0+start+edge*2.0))*side*w;
                float b=(1.05+0.35*hash(frame*7.0+start+(edge+1.0)*2.0))*side*w;
                float distance=line(vec2(acrossR,local*SEG),vec2(a,edge*SEG),
                                    vec2(b,(edge+1.0)*SEG),0.6*pixelR);
                float stroke=coverage(distance,max(fwidth(distance),pixelR*0.5));
                stroke*=step(hash(seed),0.30)*step(1.0,start)*step(0.0,local)*step(local,3.0);
                arc=max(arc,stroke);
            }
            over+=mix(itemAccent(1),white,0.4)*arc*0.85;
        } else if(!corpse && effectKind==3) {
            activeFade=0.45+0.06*sin(t*9.0);
            float dash=step(0.55,fract(coord.y*SEG/1.1-t*2.0));
            float outline=coverage(abs(abs(across)-1.2)-0.06,aaA)*dash;
            float scan=coverage(abs(mod(coord.y+t*14.0,5.0)-2.5)*SEG-0.125,aaA)*body;
            over+=itemAccent(3)*outline*0.75+mix(itemAccent(3),white,0.5)*scan*0.5;
        }
        float alpha=max(body,shadow);
        vec3 rgb=mix(vec3(0.008,0.012,0.031),tube,body/max(alpha,0.0001));
        // Curvature-limited ribbons retain the original tube width; taper the
        // light to zero at the actual interpolated extrusion instead of
        // abruptly clipping live waves or corpse shards at a quad edge.
        float edge=ribbonLimit*BOUNDS_BODY;
        // Handle degenerate helper-lane envelopes after every derivative.
        float edgeFade=edge>0.0?1.0-smoothstep(max(0.0,edge-BOUNDS_AA),edge,abs(acrossR)):0.0;
        fragColor=composite(rgb,alpha,glow,over,corpse?packed.w/255.0:activeFade)*edgeFade;return;
    }
    if(kind==1) {
        float intensity=float((int(packed.y+0.5)>>2)&15)/15.0;
        float jaw=float(((int(packed.w+0.5)>>6)&3)|((flags&128)>>5))/7.0;
        bool moving=ub.motionScale==1.0;
        vec2 p=coord;
        if(mood==8 && moving) p.y+=0.05*sin(t*69.11504)*intensity;
        float seed=base.r*13.0+base.g*7.0+base.b*5.0;
        bool flare=(int(packed.y+0.5)&128)!=0;
        float hr=tier==0?1.24:1.14;
        // spade (ellipse centre 0.05, radii 1.37 x 1.10) smooth-unioned with the neck, no flat back.
        float neck=base.a/hr;                       // body half-width at segment 1, head units
        float sx=(p.x-0.05)/1.37;
        float spade=1.10*sqrt(max(0.0,1.0-sx*sx));
        float k=0.15;float hh=clamp(0.5+0.5*(spade-neck)/k,0.0,1.0);
        float H=mix(neck,spade,hh)+k*hh*(1.0-hh);
        float Hy=p.x>0.05?1.10:H;
        float yn=p.y/Hy;
        float e=length(vec2(max(p.x-0.05,0.0)/1.37,yn));
        float sd=(e-1.0)*Hy;
        float body=mask(sd)*smoothstep(-1.0,-0.55,p.x);   // fade into the body drawn underneath
        float shadow=mask(sd-0.13)*0.62*smoothstep(-0.9,-0.5,p.x);
        vec2 forward=gradient(p.x);
        vec2 side=vec2(-forward.y,forward.x);float ly=dot(side,ub.light);
        vec3 rgb=c*0.45;
        rgb=mix(rgb,c*0.78,mask((length(vec2(max(p.x-0.15,0.0)/1.37,yn-ly*0.10))-0.84)*Hy));
        rgb=mix(rgb,c,mask((length(vec2(max(p.x-0.25,0.0)/1.37,yn-ly*0.22))-0.60)*Hy));
        rgb=mix(rgb,mix(c,white,0.62),mask((length(vec2(max(p.x-0.70,0.0)/1.37,yn-ly*0.50))-0.20)*Hy)*0.55);
        if(tier>0) { float nostril=mask(length(vec2(p.x-1.18,abs(p.y)-0.20))-0.05);rgb=mix(rgb,vec3(0),nostril*0.45); }
        vec3 over=vec3(0);
        vec3 glow=vec3(0);
        // glow cap around the head point (prototype ribbon cap), same tier extent/strength as the body.
        {
            float extent=tier==0?1.75:tier==1?1.85:tier==2?2.0:2.15;
            float strength=(tier==1?0.09:tier==3?0.13:0.11)+(boosting?0.14:0.0)+(leader?0.04:0.0);
            if(trapped) strength*=0.65+0.45*abs(sin(t*12.0));
            float gd=length(p)*hr/0.84;
            float aaG=clamp(fwidth(gd),0.008,BOUNDS_FOOD_AA);
            glow+=c*((1.0-smoothstep(extent-0.25,extent,gd))*0.55+coverage(gd-1.42,aaG))*strength*step(0.0,p.x);
        }
        float period=4.0+hash(seed+1.0)*7.0;float blinkPhase=mod(ub.animationTime+hash(seed+2.0)*period,period);
        float blinkDuration=0.15*ub.motionScale;
        float blink=blinkPhase<blinkDuration?max(0.10,abs(2.0*blinkPhase/blinkDuration-1.0)):1.0;
        float er=tier==0?0.36:0.30;
        vec2 eye=vec2(p.x-0.50,abs(p.y)-0.56);
        vec3 calmColor=leader?mix(gold,white,0.25):mix(vec3(1,0.769,0.329),c,0.22);
        vec3 irisColor=calmColor;
        float lid=1.0,pupilWidth=0.075,pupilHeight=0.20;
        bool roundPupil=false;
        // One mood dispatch; shared eye SDFs keep each branch compact.
        switch(mood) {
        case 1: break;
        case 2: irisColor=vec3(1,0.588,0.196);pupilWidth=0.035;pupilHeight=0.24;break;
        case 3: irisColor=white;er*=1.0+0.25*intensity;roundPupil=true;break;
        case 4: irisColor=vec3(1,0.39,0.16);pupilWidth=0.035;break;
        case 5: lid=1.0;break;
        case 6: irisColor=vec3(0.925,0.941,0.973);roundPupil=true;break;
        case 7: irisColor=vec3(0.7,0.82,1);break;
        case 8: irisColor=vec3(0.78,0.933,1);lid=0.42;break;
        }
        irisColor=mix(calmColor,moodAccent(irisColor),intensity);
        if(mono || ub.paletteMode==1.0) irisColor=mix(vec3(0.78,0.82,0.86),white,float(hunting)*intensity);
        if(flare) irisColor=mix(irisColor,white,0.85);
        if(hunting||leader||flare||mood==4) {
            float eyeRadius=mood==8?1.1:1.15;
            vec3 eg=(leader?gold:irisColor)*falloff(length(eye)/eyeRadius)*(flare?0.75:hunting?mix(leader?0.15:0.0,0.35,intensity):mood==4?0.18*intensity:0.15);
            rgb+=eg;glow+=eg;
        }
        blink*=mix(1.0,lid,intensity);
        float socket=mask(ellipse(eye,vec2(er*1.12,er*1.12*max(0.3,blink))));
        float iris=mask(ellipse(eye,vec2(er,er*blink)));
        if(mood==5) {socket*=1.0-intensity;iris*=1.0-intensity;}
        vec3 skin=rgb;vec3 dark=vec3(0.016,0.02,0.04);
        rgb=mix(rgb,vec3(0.016,0.02,0.04),socket);rgb=mix(rgb,irisColor,iris);
        int look=int(packed.w+0.5);
        vec2 offset=vec2((float(look&7)-3.0)/3.0*0.18,(float((look>>3)&7)-3.0)/3.0*0.35);
        vec2 pupil=eye-offset*vec2(1,sign(p.y));
        if((trapped||mood==3) && moving) pupil-=vec2(sin(t*31.0+sign(p.y))*0.05,cos(t*27.0+sign(p.y))*0.04)*intensity;
        float pd=ellipse(pupil,vec2(mix(0.075,pupilWidth,intensity),mix(0.20,pupilHeight,intensity)*er/0.30*blink));
        if(roundPupil) pd=mix(pd,length(pupil)-0.06,intensity);
        float pupilMask=mask(pd)*iris*step(0.4,blink);
        if(mood==5) {
            // Forward/lateral coordinates: closed arcs bulge toward the tail.
            vec2 a=eye-vec2(0.12,0.0); vec2 q=vec2(abs(a.y),-a.x);
            const vec2 sc=vec2(0.9128,0.4085); float rr=er*0.95;
            float ad=((sc.y*q.x>sc.x*q.y)?length(q-sc*rr):abs(length(q)-rr))-0.075;
            pupilMask=mix(pupilMask,mask(ad),intensity);
        } else if(mood==7) {
            float a=atan(eye.y,eye.x)+(moving?t*0.7:0.0);
            float spiral=mask(abs(sin(a*2.0-length(eye)*23.0))*0.10-0.035)*iris;
            pupilMask=mix(pupilMask,spiral,intensity);
        }
        rgb=mix(rgb,vec3(0.02,0.024,0.043),pupilMask);
        rgb=mix(rgb,white,mask(length(eye-vec2(-0.055,(ly*0.10-0.06)*sign(p.y)))-0.065)*iris*0.9*(1.0-intensity*float(mood==5 || mood==7)));
        if(mood==4) {
            // Rear lid and V brows pointing at the snout.
            float bd=(-0.05*er-0.55*eye.y-eye.x)*0.8762;
            float cover=mask(-bd)*mask(length(eye)-er*1.12-0.02);
            float brow=mask(abs(bd)-0.06)*mask(length(eye)-er*1.38);
            rgb=mix(rgb,skin,cover*intensity);rgb=mix(rgb,dark,brow*0.95*intensity);
        } else if(mood==1) {
            float ld=-0.02*er-eye.x;
            rgb=mix(rgb,skin,mask(-ld)*mask(length(eye)-er*1.12-0.02)*intensity);
            rgb=mix(rgb,dark,mask(abs(ld)-0.035)*mask(length(eye)-er*1.15)*0.75*intensity);
        }
        float crownCover=0.0;
        if(leader) {
            // bowed band + 3 backward spikes, dark-gold outline, additive crown glow.
            float bx=0.03-0.09*(1.0-min(1.0,(p.y/0.8)*(p.y/0.8)));
            float band=max(abs(p.x-bx)-0.08,abs(p.y)-0.8);
            float cy=mod(p.y+0.25,0.5)-0.25;
            float spikes=max(max(abs(cy)-(p.x+0.60)*0.30,max(-0.60-p.x,p.x-bx)),abs(p.y)-0.66);
            float cd=min(band,spikes);
            float fill=mask(cd);
            float rim=mask(abs(cd)-0.035)*0.85;
            rgb=mix(rgb,gold,fill);rgb=mix(rgb,gold*0.45,rim);
            rgb=mix(rgb,mix(gold,white,0.7),mask(length(p)-0.09));
            over+=gold*falloff(length(p-vec2(0.02,0.0))/1.0)*(0.45+0.2*sin(t*3.0));
            crownCover=max(fill,rim);
        }
        float alpha=max(max(body,shadow),crownCover);
        rgb=mix(vec3(0.008,0.012,0.031),rgb,max(body,crownCover)/max(alpha,0.0001));
        if(mood==1 && jaw>0.0) {
            float m=mask(ellipse(p-vec2(1.08,0),vec2(0.34*jaw+0.02,0.50*jaw+0.02)))*intensity;
            float in_=mask(ellipse(p-vec2(1.0,0),vec2(0.16*jaw,0.24*jaw)+0.001))*intensity;
            rgb=mix(rgb,(mono||ub.paletteMode==1.0)?vec3(0.06):vec3(0.08,0.016,0.04),m*0.95);
            rgb=mix(rgb,(mono||ub.paletteMode==1.0)?vec3(0.55):vec3(0.75,0.27,0.36),in_*0.9);alpha=max(alpha,m);
        }
        float tp=(hunting?1.4:3.0)+hash(seed+4.0)*(hunting?4.6:5.0);
        float phase=mod(ub.animationTime+hash(seed+5.0)*tp,tp);float tongueDuration=0.34*ub.motionScale;
        float extension=phase<tongueDuration?sin(phase/tongueDuration*3.141593):0.0;
        if(mood==5 && jaw>0.0) extension=0.58*intensity;
        if(extension>0.01) {
            vec2 joint=vec2(1.30+extension,0);float tongue=mask(min(line(p,vec2(1.30,0),joint,0.05),min(line(p,joint,vec2(1.30+1.32*extension,0.22*extension),0.045),line(p,joint,vec2(1.30+1.32*extension,-0.22*extension),0.045))));
            float visibleTongue=tongue*(1.0-body);rgb=mix(rgb,(mono||ub.paletteMode==1.0)?vec3(0.82):vec3(1,0.361,0.478),visibleTongue);alpha=max(alpha,visibleTongue);
        }
        if(mood==3) {
            // Doubled temple tear stays within the 1.75 side envelope with AA.
            vec2 drop=(p-vec2(-0.25,1.37))*0.5;
            float dropDistance=2.0*min(length(drop-vec2(0,0.025))-0.12,
                max(abs(drop.x)-(drop.y+0.18)*0.45,max(-0.18-drop.y,drop.y)));
            float sweat=coverage(dropDistance,clamp(fwidth(dropDistance),0.008,0.08))*intensity;
            rgb=mix(rgb,moodAccent(vec3(0.7,0.88,1)),sweat);alpha=max(alpha,sweat);
        } else if(mood==8 && moving) {
            float q=fract(t*0.8+seed*0.31)*0.9;
            float breath=falloff(length(p-vec2(1.65+0.55*q,0))/(0.28+0.40*q))*(1.0-q/0.9)*0.5*intensity;
            glow+=moodAccent(vec3(0.78,0.933,1))*breath;
        }
        if(boosting) {
            // bow wave, two strokes fitted to the prototype beziers; needs the widened boost quad.
            float y=abs(p.y);
            float f1=p.x-(1.95-0.70*y*y);
            float s1=abs(f1)/sqrt(1.0+1.96*y*y);
            float b1=mask(s1-0.08)*step(0.3,y)*step(y,1.85);
            float f2=p.x-(0.286+2.352*y-1.238*y*y);
            float s2=abs(f2)/sqrt(1.0+pow(2.352-2.476*y,2.0));
            float b2=mask(s2-0.08)*step(1.0,y)*step(y,2.4);
            glow+=mix(c,white,0.55)*b1*0.6+c*b2*0.3;
        }
        fragColor=composite(rgb,alpha,glow,over,(flags&32)!=0?0.45+0.06*sin(t*9.0):1.0);return;
    }
    if(kind==5 || kind==10) {
        float streak=max(0.0,1.0-abs(coord.x));
        if(kind==5) streak*=(1.0-clamp(coord.y,0.0,1.0))*step(0.0,coord.y);
        vec3 tint=kind==5?mix(c,white,0.4):base.rgb;
        fragColor=vec4(tint*streak*base.a*ub.opacity,0);return;
    }
    if(kind==6||kind==7||kind==9) {
        vec2 effectCoord=coord/BOUNDS_EFFECT_UNITS;
        float age=packed.w/255.0;float radius=length(effectCoord);float progress=1.0-pow(1.0-age,2.0);
        float ring=mask(abs(radius-(1.0+(kind==9?2.2:5.0)*progress)/BOUNDS_EFFECT_UNITS)-0.012)*(1.0-age)*0.8;
        vec3 glow=(kind==7?gold:mix(c,white,0.5))*ring;
        if(kind==6) {
            glow+=white*exp(-radius*radius*8.0)*(1.0-smoothstep(6.0,BOUNDS_RING,length(coord)))*(1.0-age)*(1.0-age)*0.9;
            float sparks=0.0;
            for(int k=0;k<9;k++) {
                float seed=packed.y+float(k);
                float angle=hash(seed)*6.283185;vec2 direction=vec2(cos(angle),sin(angle));
                float start=(1.2+7.0*progress*(0.6+hash(seed+float(k)*2.0)*0.6))/BOUNDS_EFFECT_UNITS;
                float end=start+2.2*(1.0-age)/BOUNDS_EFFECT_UNITS;
                sparks=max(sparks,mask(line(effectCoord,direction*start,direction*end,0.012)));
            }
            glow+=mix(c,white,0.6)*sparks*(1.0-age);
        }
        fragColor=vec4(min(glow,vec3(0.9))*base.a*ub.opacity,0);return;
    }
    // food. Sizes in units of the food size s (quad half-extent is 4.6 s). No size pulse;
    // twinkle modulates halo alpha only, as in the prototype.
    float phase=packed.y/255.0*6.283185;
    vec2 p=coord*BOUNDS_FOOD;float r=length(p);
    float tw=0.85+0.15*sin(t*2.2+phase);
    float px=clamp(length(vec2(dFdx(p.x),dFdy(p.x))),0.0001,BOUNDS_FOOD_PIXEL);
    vec3 rgb=c;float core=0.0;vec3 glow=vec3(0);vec3 over=vec3(0);
    if(kind==2) {                     // spark
        core=mask(r-1.05);
        rgb=mix(mix(c,white,0.20),white,mask(length(p-vec2(-0.15,-0.20))-0.45)*0.85);
        glow=c*0.42*tw*falloff(r/4.6);
        float q=sin(t*0.9+phase*5.0);
        if(q>0.95) {
            float a=(q-0.95)/0.05;
            float twinkleCross=(1.0-smoothstep(0.4*px,1.0*px,min(abs(p.x),abs(p.y))))*step(max(abs(p.x),abs(p.y)),4.5*a);
            over+=white*twinkleCross*0.75*a*(1.0-core);
        }
    } else if(kind==3) {              // essence shard: slim spinning rhombus with a lit facet
        float a=t*0.8+phase;vec2 q=mat2(cos(a),-sin(a),sin(a),cos(a))*p;
        float dd=abs(q.x)/0.85+abs(q.y)/1.6;
        core=mask((dd-1.0)*0.75);
        rgb=mix(c,white,0.30);
        rgb=mix(rgb,white,0.55*step(0.0,q.x)*step(q.y,0.0));
        glow=c*(0.42+0.18*sin(t*7.0+phase))*falloff(r/4.4);
    } else if(kind==4) {              // spent pellet
        core=mask(r-1.0)*0.55;
        glow=c*0.16*falloff(r/3.0);
    } else {                          // prism fruit (kind 8)
        core=mask(r-1.0);
        float g=clamp(length(p-vec2(-0.30,-0.35))/1.25,0.0,1.0);
        vec3 mid=mix(c,white,0.4);
        rgb=g<0.45?mix(white,mid,g/0.45):mix(mid,c*0.75,(g-0.45)/0.55);
        float a=t*0.5;vec2 q=mat2(cos(a),-sin(a),sin(a),cos(a))*p;
        vec2 R=vec2(1.9,0.6);float f=length(q/R)-1.0;float gl=length(q/(R*R))/max(length(q/R),0.001);
        float ring=1.0-smoothstep(0.55*px,1.1*px,abs(f)/max(gl,0.0001));
        float oa=t*1.6;float dot1=mask(length(q-vec2(cos(oa)*1.9,sin(oa)*0.6))-1.4*px);
        over+=mix(c,white,0.6)*ring*0.55+white*dot1;
        glow=mix(c,white,0.3)*0.55*tw*falloff(r/4.0);
    }
    float expiry=1.0;
    float fadeThreshold=kind==4?96.0:26.0;
    if(kind!=2 && packed.z<fadeThreshold) expiry=packed.z/fadeThreshold*(0.65+0.35*sin(t*18.0+phase));
    fragColor=composite(rgb,core,glow,over,expiry);
}
