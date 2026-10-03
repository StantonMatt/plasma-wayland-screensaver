#version 440
// SPDX-License-Identifier: GPL-3.0-or-later
// Single-pass tube, skin and additive light. Payload documented in render/README.md.
layout(location=0) in vec2 coord;
layout(location=1) in vec4 base;
layout(location=2) in vec4 packed;
layout(location=3) in vec2 screenPosition;
layout(location=4) in float ribbonLimit;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform buf { mat4 matrix; float opacity; float time; vec2 light; float animationTime; float motionScale; } ub;
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
float hash(float n) { return fract(sin(n*127.1)*43758.5453); }
vec3 lift(vec3 c) { float l=dot(c,vec3(0.2126,0.7152,0.0722));return mix(c,vec3(1),max(0.0,0.30-l)*1.1); }
float falloff(float u) { u=1.0-clamp(u,0.0,1.0); return u*u; }   // matches the prototype's radial glow sprite
// Premultiplied over; `glow` is additive outside opaque coverage, `over` is additive on top.
vec4 composite(vec3 rgb,float a,vec3 glow,vec3 over,float fade) { return vec4(rgb*a+glow*(1.0-a)+over,a)*fade*ub.opacity; }
const float SEG=1.18;  // segment spacing in body radii (world.rs spacing = radius*1.18)
void main() {
    int kind=int(packed.x+0.5), tier=int(packed.y+0.5)&3, flags=int(packed.z+0.5);
    bool boosting=(flags&1)!=0, hunting=(flags&4)!=0, trapped=(flags&8)!=0, leader=(flags&64)!=0;
    vec3 c=lift(base.rgb); vec3 white=vec3(1);vec3 gold=vec3(1.0,0.847,0.29);
    float chroma=max(base.r,max(base.g,base.b))-min(base.r,min(base.g,base.b));
    bool mono=chroma<0.05;
    // Palette identity is packed by Rust, independent of an individual colour.
    if((kind==0 || kind==1 || kind==7) && (int(packed.y+0.5)&64)!=0) gold=mix(gold,white,0.85);
    float t=ub.time;
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
        float wave=corpse?0.0:packed.w/255.0;
        // waves are additive light (prototype: glow sprite 2.4w, alpha 0.5*strength), not a flat tint.
        vec3 waveColor=leader && (int(packed.y+0.5)&128)==0?gold:mix(c,white,0.75);
        over+=waveColor*wave*0.5*falloff(abs(acrossR)/(2.4*w));
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
        float alpha=max(body,shadow);
        vec3 rgb=mix(vec3(0.008,0.012,0.031),tube,body/max(alpha,0.0001));
        // Curvature-limited ribbons retain the original tube width; taper the
        // light to zero at the actual interpolated extrusion instead of
        // abruptly clipping live waves or corpse shards at a quad edge.
        float edge=ribbonLimit*BOUNDS_BODY;
        // Handle degenerate helper-lane envelopes after every derivative.
        float edgeFade=edge>0.0?1.0-smoothstep(max(0.0,edge-BOUNDS_AA),edge,abs(acrossR)):0.0;
        fragColor=composite(rgb,alpha,glow,over,corpse?packed.w/255.0:1.0)*edgeFade;return;
    }
    if(kind==1) {
        vec2 p=coord;float seed=float((int(packed.y+0.5)>>2)&15);
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
        vec3 irisColor=leader?mix(gold,white,0.25):trapped?vec3(0.925,0.941,0.973):hunting?vec3(1,0.588,0.196):mix(vec3(1,0.769,0.329),c,0.22);
        if(mono) irisColor=hunting?white:vec3(0.78,0.82,0.86);
        if(flare) irisColor=mix(irisColor,white,0.85);
        if(hunting||leader||flare) {
            // eye glow lights the head skin too (drawn before the eye, like the prototype).
            vec3 eg=(leader?gold:irisColor)*falloff(length(eye)/1.15)*(flare?0.75:hunting?0.35:0.15);
            rgb+=eg;glow+=eg;
        }
        float socket=mask(ellipse(eye,vec2(er*1.12,er*1.12*max(0.3,blink))));
        float iris=mask(ellipse(eye,vec2(er,er*blink)));
        rgb=mix(rgb,vec3(0.016,0.02,0.04),socket);rgb=mix(rgb,irisColor,iris);
        float look=(packed.w-128.0)/127.0*0.9;
        vec2 pupil=eye-vec2(cos(look)*0.07,sin(look)*0.09*sign(p.y));
        if(trapped) pupil-=vec2(sin(t*31.0+sign(p.y))*0.05,cos(t*27.0+sign(p.y))*0.04);
        float pd=trapped?length(pupil)-0.06:ellipse(pupil,vec2(hunting?0.035:0.075,(hunting?0.24:0.20)*er/0.30*blink));
        rgb=mix(rgb,vec3(0.02,0.024,0.043),mask(pd)*iris*step(0.4,blink));
        rgb=mix(rgb,white,mask(length(eye-vec2(-0.055,(ly*0.10-0.06)*sign(p.y)))-0.065)*iris*0.9);
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
        float tp=(hunting?1.4:3.0)+hash(seed+4.0)*(hunting?4.6:5.0);
        float phase=mod(ub.animationTime+hash(seed+5.0)*tp,tp);float tongueDuration=0.34*ub.motionScale;
        float extension=phase<tongueDuration?sin(phase/tongueDuration*3.141593):0.0;
        if(extension>0.01) {
            vec2 joint=vec2(1.30+extension,0);float tongue=mask(min(line(p,vec2(1.30,0),joint,0.05),min(line(p,joint,vec2(1.30+1.32*extension,0.22*extension),0.045),line(p,joint,vec2(1.30+1.32*extension,-0.22*extension),0.045))));
            float visibleTongue=tongue*(1.0-body);rgb=mix(rgb,mono?vec3(0.82):vec3(1,0.361,0.478),visibleTongue);alpha=max(alpha,visibleTongue);
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
        fragColor=composite(rgb,alpha,glow,over,1.0);return;
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
