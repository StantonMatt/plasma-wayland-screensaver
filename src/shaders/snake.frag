#version 440
// SPDX-License-Identifier: GPL-3.0-or-later
// Single-pass tube, skin and additive light. Payload documented in render/README.md.
layout(location=0) in vec2 coord;
layout(location=1) in vec4 base;
layout(location=2) in vec4 packed;
layout(location=3) in vec2 screenPosition;
layout(location=4) in float ribbonLimit;
layout(location=5) in vec3 waveLight;
layout(location=6) in float phaseFade;
layout(location=0) out vec4 fragColor;
layout(binding=1) uniform sampler2D iconAtlas;
layout(std140,binding=0) uniform buf { mat4 matrix; float opacity; float time; vec2 light; float animationTime; float motionScale; float paletteMode; float ambient; float season; } ub;
// Geometry contract: literal constants are also read at Rust compile time by
// render/shader/bounds.rs. UV mapping, extrusion and copy selection share these values.
const float BOUNDS_AA = 0.04; // effect coordinates: 7 body radii per unit
const float BOUNDS_HEAD_AA = 0.2;
const float BOUNDS_FOOD_AA = 0.8;
const float BOUNDS_BODY = 2.5;
const float BOUNDS_BREATH = 1.028;
const float BOUNDS_HEAD_BACK = 1.0;
const float BOUNDS_HEAD_HAT_BACK = 1.3;
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
const float BOUNDS_FROST_CRACK = 6.0; // thaw-crack quad, snake radii
const float BOUNDS_WHIRL_RING = 12.5; // Whirlpool burst ring quad, base radii
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
    if(kind==23 || kind==24 || kind==28 || kind>=29) kind=1;
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
// Held-Flip false head: knob radius in body radii. Must stay below the body quad's
// half-extent at the tail tip, BOUNDS_BODY*taper(1) = 2.5*0.22 = 0.55 radii.
const float FLIP_KNOB=0.38;
vec3 itemAccent(int k) {
    vec3 a;
    if(k==1) a=vec3(1.0,0.88235,0.30196);
    else if(k==2) a= vec3(1.0,0.37255,0.82353);
    else if(k==3) a= vec3(0.66275,0.54510,1.0);
    else if(k==4) a= vec3(0.61569,1.0,0.22745);
    else if(k==6) a=vec3(1.0,0.60392,0.23529);
    else if(k==7) a=vec3(0.2,0.87843,0.78431);
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
// Prism family: palette-independent spectrum (hue 0 red, 1/3 green, 2/3 blue),
// 20% toward white like the prototype. Mono is flat silver so shape carries it.
vec3 spectral(float h) {
    vec3 k=clamp(abs(fract(h+vec3(0.0,2.0/3.0,1.0/3.0))*6.0-3.0)-1.0,0.0,1.0);
    k=mix(k,vec3(1),0.2);
    if(ub.paletteMode==1.0) return vec3(0.86);
    return ub.paletteMode==2.0?mix(k,vec3(1),0.3):k;
}

// ===== Halloween (season 1). Colours are sRGB, like every other literal here.
const vec3 H_PUMPKIN=vec3(1.0,0.459,0.094);      // #ff7518
const vec3 H_PUMPKIN_DEEP=vec3(0.62,0.20,0.03);  // #9e3308 crease/shade
const vec3 H_PUMPKIN_LIT=vec3(1.0,0.70,0.36);    // #ffb35c highlight
const vec3 H_UNRIPE=vec3(0.58,0.78,0.26);        // #94c742
const vec3 H_STEM=vec3(0.36,0.43,0.17);          // #5c6e2b
const vec3 H_CANDLE=vec3(1.0,0.78,0.30);         // #ffc74d
const vec3 H_CANDLE_CORE=vec3(1.0,0.95,0.74);    // #fff2bd
const vec3 H_FLAME=vec3(1.0,0.45,0.08);          // #ff7314
const vec3 H_VIOLET=vec3(0.61,0.42,1.0);         // #9b6bff
bool halloween() { return ub.season==1.0; }
// Palette-mode rule for seasonal colours: Mono greys (shape carries it), Pastel +30%
// white. Near-black tones (luminance < 0.16) stay dark in Pastel so cut-outs stay cut-outs.
vec3 hallow(vec3 c) {
    float l=dot(c,vec3(0.2126,0.7152,0.0722));
    if(ub.paletteMode==1.0) return vec3(l);
    return ub.paletteMode==2.0 && l>=0.16?mix(c,vec3(1),0.3):c;
}
// Candle flicker: three slow sines (0.97, 1.54, 2.44 Hz), +-19% peak, 1.0 in Calm.
float candle(float seed) {
    if(ub.motionScale!=1.0) return 1.0;
    float t=ub.time;
    return 1.0+0.09*sin(t*6.1+seed)+0.06*sin(t*9.7+seed*1.7)+0.04*sin(t*15.3+seed*2.3);
}
// Front-view pumpkin, unit radius (width 2.0, height 1.72), y down. <0 inside.
float pumpkinSd(vec2 q) {
    q.y-=0.04;
    float c=ellipse(q,vec2(0.50,0.84));
    float s=ellipse(vec2(abs(q.x)-0.40,q.y+0.02),vec2(0.60,0.78));
    return min(c,s);
}
float stemSd(vec2 q) {
    vec2 s=q-vec2(0.05,-0.93);
    s=mat2(0.966,0.259,-0.259,0.966)*s;           // 15 degree lean
    return length(max(abs(s)-vec2(0.06,0.13),0.0))-0.05;
}
// Ribbed skin without crease lines: five lobes as a cosine across the width
// (bright at x = 0 and +-0.8, darker at +-0.4), a radial falloff and a top-left highlight.
vec3 pumpkinSkin(vec2 q,vec3 tone) {
    float rr=length(q/vec2(1.0,0.86));
    float ribs=0.88+0.12*cos(q.x*7.853982);
    vec3 skin=tone*ribs*mix(1.0,0.70,smoothstep(0.50,1.0,rr));
    return mix(skin,mix(tone,hallow(H_PUMPKIN_LIT),0.6),falloff(length(q-vec2(-0.34,-0.38))/0.45)*0.7);
}
// Carved face (atlas tile 13): + inside the openings, in tile SDF units (1 = 8 tile px).
float faceSample(vec2 q,float extent) {
    vec2 uv=clamp(q/extent,-1.0,1.0)*0.5+0.5;
    return (texture(iconAtlas,(vec2(1.0,3.0)*32.0+vec2(0.5)+uv*31.0)/128.0).r-0.5)*float(max(abs(q.x),abs(q.y))<extent);
}
// Equilateral triangle centred on its centroid, apex along +x, circumradius r. <0 inside.
float eqTri(vec2 p,float r) {
    const float k=1.7320508;
    float h=r*0.8660254;            // half side
    p=vec2(p.y,p.x);                // apex +y in iq's form
    p.x=abs(p.x)-h;
    p.y=p.y+h/k;
    if(p.x+k*p.y>0.0) p=vec2(p.x-k*p.y,-k*p.x-p.y)/2.0;
    p.x-=clamp(p.x,-2.0*h,0.0);
    return -length(p)*sign(p.y);
}
float hexagon(vec2 p,float r) {
    const vec3 k=vec3(-0.8660254,0.5,0.5773503);
    p=abs(p);p-=2.0*min(dot(k.xy,p),0.0)*k.xy;
    p-=vec2(clamp(p.x,-k.z*r,k.z*r),r);
    return length(p)*sign(p.y);
}
void main() {
    int kind=int(packed.x+0.5), tier=int(packed.y+0.5)&3, flags=int(packed.z+0.5);
    bool venomHead=kind==23 || kind==24, strikeHead=kind==24, surgeHead=kind==28, phaseHead=kind>=29;
    if(venomHead || surgeHead || phaseHead) kind=1;
    bool boosting=(flags&1)!=0, hunting=(flags&4)!=0, trapped=(flags&8)!=0, leader=(flags&64)!=0;
    int mood=kind==1?(flags>>1)&15:0;
    if(kind==1) {hunting=mood==2;trapped=mood==6;}
    vec3 c=lift(base.rgb); vec3 white=vec3(1);vec3 gold=vec3(1.0,0.847,0.29);
    float chroma=max(base.r,max(base.g,base.b))-min(base.r,min(base.g,base.b));
    bool mono=chroma<0.05;
    // Palette identity is packed by Rust, independent of an individual colour.
    if((kind==0 || kind==1 || kind==7) && (int(packed.y+0.5)&64)!=0) gold=mix(gold,white,0.85);
    float t=ub.time;
    float night=(1.0-ub.ambient)/0.72;
    if(kind==20) {
        float r=length(coord);float a=atan(coord.y,coord.x)+t*0.12;
        float dash=step(0.30,fract(a/6.283185*24.0));
        float ring=mask(abs(r-1.0)-0.012)*dash*0.65;
        float sector=floor(a/6.283185*12.0);float h=fract(sin(sector*127.1+31.7)*43758.5453);
        float local=fract(a/6.283185*12.0)-0.5;
        vec2 spark=vec2(local*0.5236*r,r-(0.83+0.31*h));
        float star=mask(abs(spark.x)+abs(spark.y)-0.018)*(0.35+0.25*sin(t*6.9+h*6.283185));
        fragColor=vec4(base.rgb*(ring+star)*base.a*ub.opacity,0);return;
    }
    if(kind==22) {
        if(packed.y<0.5) {
            float across=max(0.0,1.0-abs(coord.x)),along=1.0-coord.y;
            float beam=across*across*along*along;
            fragColor=vec4(base.rgb*beam*base.a*ub.opacity,0);return;
        }
        float r=length(coord*4.6);float core=mask(r-0.62);
        vec3 glow=base.rgb*falloff(r/4.6)*0.55*(1.0+0.6*night);
        fragColor=composite(mix(base.rgb,white,0.6),core,glow,vec3(0),base.a);return;
    }
    if(kind==21) {
        vec2 q=abs(coord*4.6);float r=length(q);float phase=packed.y/255.0*6.283185;
        float tw=0.82+0.18*sin(t*6.9+phase);
        float core=mask(sqrt(q.x*q.y)+max(q.x,q.y)*0.15-0.28);
        float age=packed.w/255.0;float puff=mask(abs(r-(0.8+3.2*age))-0.10)*(1.0-age)*0.45;
        vec3 glow=base.rgb*(falloff(r/4.6)*0.45*tw+puff)*(1.0+0.6*night);
        fragColor=composite(mix(base.rgb,white,0.5),core,glow,vec3(0),clamp(packed.z/8.5,0.0,1.0));return;
    }
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
        int glyphCode=int(packed.y+0.5);
        float glyph=atlasMask(glyphCode>=5?glyphCode-5:7+glyphCode,p-vec2(0,-0.10),glyphCode>=5?0.66:0.61);
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
    if(kind==18) {
        // Nova (coord in nova radii): a crisp white-ice front with a cold wake,
        // 18 tapering needles (inventory) and an inner glow bounded by the front.
        float age=packed.w/255.0,fade=1.0-age;
        float radius=0.08+0.88*(1.0-fade*fade*fade);
        float r=length(coord),d=r-radius;
        float px=max(fwidth(r),0.0005);
        if(d>0.11) {fragColor=vec4(0);return;}
        vec3 rim=mix(base.rgb,white,0.55),deep=base.rgb*base.rgb;
        float front=coverage(abs(d)-px*(0.6+1.0*fade),px);
        float halo=exp(-abs(d)/(px*5.0));
        float wake=d<0.0?exp(d/(0.035+0.10*age)):0.0;
        float inner=falloff(r/max(radius,0.001));
        const float SECTOR=0.3490659;   // 18 needles
        float a=atan(coord.y,coord.x);float k=floor(a/SECTOR+0.5);
        float len=(0.05+0.04*hash(k+packed.y))*fade;
        float u=d/max(len,0.0001);
        float needle=coverage(abs(a-k*SECTOR)*r-px*(0.35+0.75*(1.0-u)),px)*step(0.0,u)*step(u,1.0);
        // Frost flecks settle behind the front: one hashed point per 0.035 R cell.
        vec2 cell=floor(coord/0.035);
        float h=hash(cell.x*13.7+cell.y*71.3+packed.y);
        float fleck=0.0;
        if(h>0.78 && d<0.0) {
            vec2 at=(cell+0.2+0.6*vec2(hash(h*17.0),hash(h*29.0)))*0.035;
            fleck=coverage(length(coord-at)-px*0.8,px)*exp(d/0.18);
        }
        vec3 light=rim*(front*0.95+needle*0.8+fleck*0.7)+base.rgb*halo*0.30+deep*(wake*0.30+inner*0.20);
        fragColor=vec4(min(light*fade,vec3(0.95))*base.a*ub.opacity,0);return;
    }
    if(kind==12 || kind==13 || kind==14) {
        float age=packed.w/255.0;float r=length(coord);vec3 accent=base.rgb;
        float radius=kind==14?2.0:ub.motionScale!=1.0?(kind==12?3.5:1.5):kind==12?1.0+5.0*age:2.5*(1.0-age);
        // The Whirlpool burst ring sweeps past the holder (9r) and the rivals (12r).
        if(packed.z>1.5) radius=ub.motionScale!=1.0?8.0:2.0+10.0*(1.0-(1.0-age)*(1.0-age));
        float ring=mask(abs(r-radius)-0.08)*(kind==14?1.0:1.0-age)*0.8;
        vec3 glow=accent*ring;
        if(kind==12) glow+=white*mask(abs(r-radius*0.65)-0.055)*(1.0-age)*0.85;
        // Whirlpool burst: one ring, then a faint pressure band behind it.
        if(packed.z>1.5) glow=min(glow,vec3(0.6))+accent*falloff(max(radius-r,0.0)/1.8)*step(r,radius)*0.16*(1.0-age);
        if(kind==14) glow*=0.5+0.5*sin(t*22.0);
        fragColor=vec4(min(glow,vec3(0.9))*base.a*ub.opacity,0);return;
    }
    if(kind==15) {
        // One head-centred quad, in body radii. The reach stays exactly 9r.
        float radius=length(coord);
        float angle=atan(coord.y,coord.x)-t*1.2;
        float openW=packed.w/255.0;float opening=(packed.w<0.5||packed.w>254.5)?1.0:openW;
        float reach=9.0*(1.0-pow(1.0-opening,3.0));
        float dash=mix(1.0,step(0.75,fract(angle*9.0/3.141593)),smoothstep(0.6,1.0,opening));
        float ring=mask(abs(radius-reach)-0.08*(1.0+2.0*(1.0-opening)))*dash*mix(0.85,0.4,opening);
        vec3 glow=base.rgb*ring;
        for(int k=0;k<3;k++) {
            float a=t*1.2+float(k)*2.094395;
            float spark=length(coord-vec2(cos(a),sin(a))*reach);
            glow+=base.rgb*falloff(spark/1.1)*0.9*opening;
        }
        fragColor=vec4(min(glow,vec3(0.9))*base.a*ub.opacity,0);return;
    }

    if(kind==27) {
        // Surge slipstream (inventory): a ribbon over the first ~10 r of the body.
        // coord.x = +-1 at 2.0 local half-widths, coord.y = radii behind the head point.
        // byte w = streak length (0..255 -> 0..14 r): 10 r cruising, 14 r while boosting.
        float y=coord.y;
        float x=abs(coord.x)*2.0;
        float pxx=max(fwidth(x),0.0001);
        float L=packed.w/255.0*14.0;
        float env=smoothstep(-0.3,0.9,y)*(1.0-smoothstep(L*0.30,L,y));
        float flowT=ub.motionScale==1.0?t*9.0:0.0;
        float shimmer=1.0+0.22*sin((y+flowT)*1.3);
        float lineA=coverage(abs(x-1.42)-pxx*(0.55+0.5*(1.0-y/max(L,0.01))),pxx)*env*shimmer;
        float lineB=coverage(abs(x-1.82)-pxx*0.45,pxx)*env*smoothstep(1.2,3.0,y)*0.55*shimmer;
        float haze=falloff(abs(x-1.42)/0.5)*env*0.10;
        vec3 col=mix(base.rgb,white,0.30);
        fragColor=vec4(col*(lineA+lineB+haze)*base.a*ub.opacity,0);return;
    }
    if(kind==26) {
        // Inventory pip (inventory). Screen-aligned quad, half-extent 2.0 pip radii.
        // p is in pip units: hexagon circumradius 1 (flat top), keyline to 1.19.
        vec2 p=coord*2.0;
        int item=int(packed.y+0.5)&7;
        float life=packed.z/255.0;                 // 1 = fresh; <1 drains the rim clockwise
        float anim=packed.w/255.0;                 // 0 rest; 1..127 stash pop; 128..255 use flight
        float flight=packed.w>127.5?(packed.w-128.0)/127.0:0.0;
        vec3 accent=base.rgb;
        float px=max(fwidth(p.x),0.0001);
        float key=coverage(hexagon(p,0.866+0.165),px);
        float h=hexagon(p,0.866);
        float gem=coverage(h,px);
        float rimBand=coverage(abs(h+0.09)-0.09,px)*gem;
        float filled=1.0;
        if(life<0.998) {
            float ang=mod(atan(p.x+0.000001,-p.y)+6.283185,6.283185)/6.283185;
            filled=step(1.0-life,ang);
        }

        vec3 fill=mix(vec3(0.02353,0.02745,0.05490),accent,0.12);
        vec3 rgb=mix(vec3(0.00784,0.01176,0.02353),fill,gem);
        vec3 rimColor=mix(mix(accent,white,0.15),vec3(0.16,0.17,0.2),1.0-filled);
        rgb=mix(rgb,rimColor,rimBand);
        float icon=atlasMask(item-1,p,0.80)*gem;
        vec3 iconColor=mix(accent,white,0.40)*(0.6+0.4*smoothstep(0.0,0.6,life));
        rgb=mix(rgb,iconColor,icon);
        rgb=mix(rgb,white,flight*0.6*max(rimBand,icon));
        vec3 glow=accent*falloff(length(p)/2.0)*(0.30+0.30*flight)*(0.55+0.45*life);
        fragColor=composite(rgb,key,min(glow,vec3(0.9)),vec3(0),base.a);return;
    }
    if(kind==0) {
        // uv.x = +-taper at each vertex, colour alpha = taper. acrossR is the true signed
        // distance from the centre line in radii (affine, no trapezoid kink); w is the local half-width.
        float breath=1.0+(BOUNDS_BREATH-1.0)*sin(t*2.4+coord.y*0.32);
        float w=max(base.a,1.0/255.0)*breath;
        float acrossR=coord.x*BOUNDS_BODY;
        float tipK=clamp(coord.y,0.0,1.0);
        float wv=w*tipK;                         // local silhouette half-width
        // Held Flip (body bit 2, never on corpses): the tail tip swells into a small
        // rounded false head; smooth max avoids a kink where it meets the taper.
        bool flipTail=(flags&132)==4 && coord.y<FLIP_KNOB*2.0/SEG;
        if(flipTail) {
            float yr=coord.y*SEG-FLIP_KNOB;
            float knob=sqrt(max(FLIP_KNOB*FLIP_KNOB-yr*yr,0.0));
            float h=clamp(0.5+(knob-wv)/0.12,0.0,1.0);
            wv=mix(wv,knob,h)+0.06*h*(1.0-h);
        }
        float sd=abs(acrossR)-wv;                // radii, <0 inside the tube
        float d=1.0+sd/w;                        // 1 at the silhouette, like the old |across|
        float across=acrossR/max(wv,0.001);      // band coordinate, +-1 at the silhouette
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
        // False eyes on the Flip knob, sized in body radii (not taper) so they read at
        // real size on any body length: dark rim, light accent iris, soft glow that
        // glints at 0.41 Hz (static in Calm). Body bit 2 is presentation-only.
        if(flipTail) {
            float le=length(vec2(abs(acrossR)-0.165,coord.y*SEG-0.425));
            float aaE=max(fwidth(le),0.008);
            vec3 acc=itemAccent(6);
            tube=mix(tube,vec3(0.02,0.024,0.043),coverage(le-0.135,aaE)*body*0.85);
            tube=mix(tube,mix(acc,white,0.42),coverage(le-0.095,aaE)*body);
            float glint=ub.motionScale==1.0?0.8+0.2*sin(t*2.6):1.0;
            over+=acc*falloff(le/0.30)*0.22*glint*body;
        }
        float activeFade=1.0;
        if(!corpse && effectKind==1) {
            // Surge (inventory design): the crackle is gone. The body only gains a faint
            // yellow halo; speed is told by the slipstream ribbon (kind 27), the yellow bow
            // wave on the head (kind 28) and the yellow contrail behind the tail.
            glow+=itemAccent(1)*halo*0.06;
        } else if(!corpse && effectKind==4) {
            // Acid spine: an opaque 0.15w stripe (about 2.5 px at 1080p) with a soft halo;
            // venom pulses flow toward the fangs (~1 Hz, +-20% on the stripe only; still in Calm).
            vec3 acid=itemAccent(4);
            float aR=max(fwidth(acrossR),0.008);
            float spine=coverage(abs(acrossR)-0.15*w,aR)*body;
            float flow=ub.motionScale==1.0?0.8+0.2*sin(coord.y*0.9-t*6.0):0.9;
            tube=mix(tube,acid*flow,spine*0.9);
            over+=acid*falloff(abs(acrossR)/(0.6*w))*0.22*body;
        } else if(!corpse && (effectKind==3 || effectKind==6)) {
            activeFade=effectKind==6?mix(1.0,0.45,phaseFade):0.45;
            float dash=step(0.55,fract(coord.y*SEG/1.1-t*2.0));
            float outline=coverage(abs(abs(across)-1.2)-0.06,aaA)*dash;
            float scan=coverage(abs(mod(coord.y+t*14.0,5.0)-2.5)*SEG-0.125,aaA)*body;
            over+=itemAccent(3)*outline*0.75+mix(itemAccent(3),white,0.5)*scan*0.5;
        }
        if((int(packed.y+0.5)&128)!=0 && !corpse) {
            // Frozen: a crystal every two segments with its long axis on the spine
            // (prototype 0.84 w x 0.40 w), plus four-point glints on about 12% of
            // segments, reseeded at 3 Hz (each under 0.6 w; still in Calm).
            float along=(mod(coord.y+1.0,2.0)-1.0)*SEG;
            float diamond=mask((abs(along)*0.20+abs(acrossR)*0.42-0.084*w)*2.15)*body;
            float frame=floor(t*3.0),cell=floor(coord.y);
            float glint=0.0;
            if(hash(cell*7.1+frame*3.3+base.r*31.0)>0.88) {
                float pixelR=max(length(vec2(dFdx(acrossR),dFdy(acrossR))),0.0001);
                vec2 g=abs(vec2(acrossR-(hash(cell+frame)-0.5)*1.1*w,(fract(coord.y)-0.5)*SEG));
                float L=0.55*w;
                float star=min(max(g.x-L,g.y-pixelR*0.8*(1.0-g.x/L)),max(g.y-L,g.x-pixelR*0.8*(1.0-g.y/L)));
                float life=ub.motionScale==1.0?sin(3.141593*fract(t*3.0)):1.0;
                glint=(coverage(star,pixelR*0.6)+falloff(length(g)/(0.45*w))*0.4)*life;
            }
            over+=(moodAccent(vec3(0.86,0.96,1.0))*diamond*0.6+white*glint*0.85)*body;
        }
        tube*=ub.ambient;
        glow+=c*halo*0.05*night;
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
        rgb*=ub.ambient;
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
            glow+=c*((1.0-smoothstep(extent-0.25,extent,gd))*0.55+coverage(gd-1.42,aaG))*(strength+0.05*night)*step(0.0,p.x);
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
        // Halloween: Calm, Sleepy and Hunting eyes burn like candles; Calm and Hunting
        // are carved triangles (apex toward the tail, the face language's "up").
        bool candleEye=halloween() && mood<=2;
        bool lantern=candleEye && mood!=1;
        float flick=candleEye?candle(seed*3.0):1.0;
        if(candleEye) {
            vec2 gaze=vec2((float(int(packed.w+0.5)&7)-3.0)/3.0*0.18,(float((int(packed.w+0.5)>>3)&7)-3.0)/3.0*0.35*sign(p.y));
            if(lantern) gaze*=vec2(0.40,0.50);
            float hot=1.0-clamp(length(eye-gaze)/(er*0.85),0.0,1.0);
            vec3 flame=mood==2?mix(hallow(vec3(0.95,0.22,0.04)),hallow(vec3(1.0,0.70,0.28)),smoothstep(0.1,0.9,hot))
                               :mix(mix(hallow(H_FLAME),hallow(H_CANDLE),smoothstep(0.0,0.5,hot)),hallow(H_CANDLE_CORE),smoothstep(0.5,1.0,hot));
            irisColor=mono || ub.paletteMode==1.0?mix(vec3(0.62),vec3(1.0),smoothstep(0.2,1.0,hot)):flame;
            irisColor*=mix(1.0,flick,0.6);
            // Jack-o'-lantern glow around each eye, flickering with the candle.
            vec3 eg=(mood==2?hallow(vec3(1.0,0.32,0.06)):hallow(H_FLAME))*falloff(length(eye)/0.95)*0.16*flick;
            rgb+=eg;glow+=eg;
        }
        if(lantern) {
            vec2 apex=vec2(-1.0,0.0);
            float sq=max(blink,0.08);
            vec2 es=vec2(eye.x,eye.y/sq);
            vec2 te=vec2(dot(es,apex),dot(es,vec2(-apex.y,apex.x)));
            float tri=eqTri(te,er*1.30)*sq;
            socket=mask(tri-0.06*sq);
            iris=mask(tri);
        }
        vec3 eyeshine=irisColor*falloff(length(eye)/0.6)*0.5*night;
        if(lantern) eyeshine=irisColor*falloff(length(eye)/0.7)*0.62*night*flick;
        glow+=eyeshine;over+=eyeshine;
        if(mood==5) {socket*=1.0-intensity;iris*=1.0-intensity;}
        vec3 skin=rgb;vec3 dark=vec3(0.016,0.02,0.04);
        rgb=mix(rgb,vec3(0.016,0.02,0.04),socket);rgb=mix(rgb,irisColor,iris);
        int look=int(packed.w+0.5);
        vec2 offset=vec2((float(look&7)-3.0)/3.0*0.18,(float((look>>3)&7)-3.0)/3.0*0.35);
        vec2 pupil=eye-offset*vec2(1,sign(p.y))*(lantern?vec2(0.40,0.50):vec2(1.0));
        if((trapped||mood==3) && moving) pupil-=vec2(sin(t*31.0+sign(p.y))*0.05,cos(t*27.0+sign(p.y))*0.04)*intensity;
        float pd=ellipse(pupil,vec2(mix(0.075,pupilWidth,intensity),mix(0.20,pupilHeight,intensity)*er/0.30*blink));
        if(roundPupil) pd=mix(pd,length(pupil)-0.06,intensity);
        float pupilMask=mask(pd)*iris*step(0.4,blink);
        if(lantern && mood!=2) pupilMask=0.0;
        if(mood==5) {
            // Forward/lateral coordinates: closed arcs bulge toward the tail.
            vec2 a=eye-vec2(0.12,0.0); vec2 q=vec2(abs(a.y),-a.x);
            const vec2 sc=vec2(0.9128,0.4085); float rr=er*0.95;
            float ad=((sc.y*q.x>sc.x*q.y)?length(q-sc*rr):abs(length(q)-rr))-0.075;
            pupilMask=mix(pupilMask,mask(ad),intensity);
        } else if(mood==7) {
            float a=atan(eye.y,eye.x)+(moving?t*5.0:0.0);
            float spiral=mask(abs(sin(a*2.0-length(eye)*23.0))*0.10-0.035)*iris;
            pupilMask=mix(pupilMask,spiral,intensity);
        }
        rgb=mix(rgb,vec3(0.02,0.024,0.043),pupilMask);
        if(!lantern) rgb=mix(rgb,white,mask(length(eye-vec2(-0.055,(ly*0.10-0.06)*sign(p.y)))-0.065)*iris*0.9*(1.0-intensity*float(mood==5 || mood==7)));
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
        if(leader && halloween()) {
            // Witch hat: brim across the head, cone and bent tip toward the tail.
            // Sits where the crown sits: behind the eyes, tip over the back of the head.
            vec2 hp=p;
            float brim=ellipse(hp-vec2(-0.12,0.0),vec2(0.25,0.92));
            float cone=sdTriangle(hp,vec2(-0.12,-0.50),vec2(-0.12,0.50),vec2(-0.80,0.12));
            float tip=sdTriangle(hp,vec2(-0.76,0.00),vec2(-0.72,0.25),vec2(-0.98,0.36));
            float crown=min(cone-0.02,tip-0.02);
            float hat=min(brim,crown);
            float fill=mask(hat);
            float rim=mask(abs(hat)-0.05)*0.95;
            float coneRim=mask(abs(crown)-0.04)*mask(brim)*0.9;
            vec3 cloth=hallow(vec3(0.20,0.11,0.33));
            vec3 brimCloth=hallow(mix(vec3(0.20,0.11,0.33),H_VIOLET,0.45));
            rgb=mix(rgb,brimCloth,mask(brim));
            rgb=mix(rgb,cloth,mask(crown));
            rgb=mix(rgb,mix(cloth,white,0.22),mask(max(crown,-hp.y-0.05))*fill);  // lit side, 22% white
            rgb=mix(rgb,hallow(vec3(1.0,0.86,0.40)),mask(length((hp-vec2(-0.36,0.0))/vec2(0.07,0.09))-1.0)*0.95);
            rgb=mix(rgb,hallow(H_VIOLET)*0.55,coneRim);
            rgb=mix(rgb,hallow(H_VIOLET),rim);
            glow+=hallow(H_VIOLET)*falloff(length(hp-vec2(-0.25,0.0))/0.75)*(0.34+0.10*sin(t*3.0));
            crownCover=max(fill,rim);
        } else if(leader) {
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
        if(venomHead) {
            vec3 acid=itemAccent(4);
            rgb=mix(rgb,acid,body*0.5);
            // Acid aura (0.45 head units) so a holder reads at 1080p hatchling size.
            glow+=acid*falloff(max(sd,0.0)/0.45)*0.45*smoothstep(-0.7,0.0,p.x);
            // Strike: the jaw gapes past the snout (a notch in the silhouette) with venom inside.
            vec2 mc=strikeHead?vec2(1.18,0.0):vec2(1.10,0.0);
            vec2 mr=strikeHead?vec2(0.40,0.40):vec2(0.30,0.06);
            float mouth=mask(ellipse(p-mc,mr));
            vec3 maw=strikeHead?mix(vec3(0.02,0.035,0.01),acid*0.5,falloff(length((p-mc)/mr))):vec3(0.025);
            rgb=mix(rgb,maw,mouth);alpha=max(alpha,mouth);
            // Filled, rounded fangs (prototype triangles) that clear the snout by about
            // 3 px at rest and 6 px when striking at 1080p hatchling size.
            vec2 q=vec2(p.x,abs(p.y));
            vec2 tip=strikeHead?vec2(1.92,0.42):vec2(1.66,0.30);
            float fd=strikeHead?sdTriangle(q,vec2(1.00,0.28),vec2(1.08,0.54),tip)-0.07
                               :sdTriangle(q,vec2(0.98,0.18),vec2(1.02,0.44),tip)-0.06;
            float fang=mask(fd);
            rgb=mix(rgb,mix(acid,white,0.75),fang);alpha=max(alpha,fang);
            // Two venom beads bud from the fang tips and drift forward (inventory), 0.9 Hz.
            for(int k=0;k<2;k++) {
                float qd=moving?fract(t*0.9+seed*0.37+float(k)*0.5):0.5;
                vec2 bead=vec2(tip.x+0.05+0.25*qd,(k==0?1.0:-1.0)*(tip.y+0.04));
                over+=acid*falloff(length(p-bead)/0.30)*(1.0-qd);
            }
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
            // Frosty breath: three puffs leave the snout in turn, growing, then fade
            // (one exhale per 1.25 s). Max reach 2.81 head units < HEAD_FRONT.
            float q=fract(t*0.8+seed*0.31);
            float side=hash(seed+6.0)>0.5?1.0:-1.0;
            float e=smoothstep(0.0,0.6,q);
            float puff=0.0;
            for(int i=0;i<3;i++) {
                float fi=float(i);
                vec2 centre=vec2(1.58+(0.20+0.34*fi)*(0.55+0.45*e),side*0.07*fi*e);
                float u=length(p-centre)/((0.13+0.07*fi)*(0.85+0.45*e));
                puff=max(puff,(1.0-smoothstep(0.45,1.0,u))*smoothstep(fi*0.14,fi*0.14+0.12,q)*(1.0-0.2*fi));
            }
            glow+=moodAccent(vec3(0.9,0.965,1))*puff*(1.0-smoothstep(0.55,1.0,q))*0.85*intensity;
        }
        if(boosting || surgeHead) {
            // bow wave, two strokes fitted to the prototype beziers; needs the widened boost quad.
            float y=abs(p.y);
            float f1=p.x-(1.95-0.70*y*y);
            float s1=abs(f1)/sqrt(1.0+1.96*y*y);
            float b1=mask(s1-0.08)*step(0.3,y)*step(y,1.85);
            float f2=p.x-(0.286+2.352*y-1.238*y*y);
            float s2=abs(f2)/sqrt(1.0+pow(2.352-2.476*y,2.0));
            float b2=mask(s2-0.08)*step(1.0,y)*step(y,2.4);
            if(surgeHead) { vec3 sa=itemAccent(1);float k=boosting?1.0:0.7;glow+=mix(sa,white,0.50)*b1*1.0*k+mix(sa,white,0.25)*b2*0.85*k; }
            else glow+=mix(c,white,0.55)*b1*0.6+c*b2*0.3;
        }
        fragColor=composite(rgb,alpha,glow,over,phaseHead?mix(1.0,0.45,phaseFade):(flags&32)!=0?0.45:1.0);return;
    }
    if(kind==19) {
        // Brew quad: radius 22 base radii, so one base radius is 1/22 quad units.
        // Pixel size is read before the corner rejection (uniform control flow).
        float px=fwidth(coord.x);
        float r2=dot(coord,coord);
        if(r2>1.0) {fragColor=vec4(0);return;}
        float r=sqrt(r2);float a=atan(coord.y,coord.x);
        float brew=packed.w/255.0;                    // 150 ticks
        float open=smoothstep(0.0,0.1,brew);          // 0.5 s fade-in
        float charge=smoothstep(0.72,0.92,brew);      // 1 s swell until the pull stops (tick 138)
        float collapse=smoothstep(0.92,1.0,brew);     // then inhale: arms and rim fade, core contracts
        // Three log-spiral arms on the captured food's path; +t drifts them inward at
        // 0.9 rad/s (prototype). Arm distance = phase offset * r / |grad phase| (sqrt 45).
        float phase=a*3.0+log(max(r,0.022))*6.0+t*2.7;
        float d=abs(fract(phase*0.1591549-0.25)-0.5)*6.283185*r*0.1490712;
        float armFade=smoothstep(0.035,0.11,r)*(1.0-smoothstep(0.86,0.97,r))*open*(1.0-collapse);
        float arms=min(0.42,coverage(d-0.0055,px)*0.42+falloff(d/0.035)*0.12)*armFade;
        float rim=coverage(abs(r-0.97)-0.0035,px)*smoothstep(0.0,0.4,sin(a*36.0))*0.30*open*(1.0-collapse);
        // Core radius in base radii: 0.6 + 0.55 sqrt(V), with packed.z = sqrt(V)/5.
        float core=(0.6+2.75*packed.z/255.0)*(1.0+0.3*charge)*(1.0-0.65*collapse)*0.04545;
        float glowOut=falloff(r/(core*3.2))*(0.35+0.40*charge)*open;
        float glowHot=falloff(r/(core*1.4))*(0.50+0.40*charge)*open;
        vec3 light=c*(arms+rim+glowOut)+mix(c,white,0.6)*glowHot;
        fragColor=vec4(min(light,vec3(0.9))*ub.opacity,0);return;
    }
    if(kind==25 && packed.y>0.5 && halloween()) {
        // coord in bat units: atlas tile 12 spans +-1.0; byte z = flap phase; colour alpha carries fade.
        bool still=ub.motionScale!=1.0;
        float flap=still?1.0:0.70+0.30*sin(t*15.7+packed.z/255.0*6.283185);   // 2.5 Hz
        vec2 q=vec2(coord.x,coord.y/flap)*1.15;
        float tile=texture(iconAtlas,((vec2(0.0,3.0)*32.0+vec2(0.5)+(clamp(q,-1.0,1.0)*0.5+0.5)*31.0)/128.0)).r-0.5;
        float aa=max(fwidth(tile),0.02);
        float inside=smoothstep(-aa,aa,tile)*float(max(abs(q.x),abs(q.y))<1.0);
        float rim=smoothstep(-aa,aa,tile)*(1.0-smoothstep(0.10,0.22,tile))*float(max(abs(q.x),abs(q.y))<1.0);
        vec2 e=vec2(abs(q.x)-0.07,q.y+0.17);
        float eyes=mask(length(e)-0.04);
        vec3 rgb=hallow(vec3(0.06,0.035,0.10));
        rgb=mix(rgb,hallow(H_VIOLET),rim*0.9);
        rgb=mix(rgb,hallow(H_CANDLE),eyes);
        vec3 g=hallow(H_VIOLET)*falloff(length(coord)/1.0)*0.16+hallow(H_FLAME)*falloff(length(e)/0.18)*0.6;
        fragColor=composite(rgb,max(inside,eyes),g,vec3(0),base.a);return;
    }
    if(kind==25) {
        // Venom stump / severed-end glow: prototype radial glow, additive.
        fragColor=vec4(itemAccent(4)*falloff(length(coord))*base.a*ub.opacity,0);return;
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
        if(kind==6 && packed.z>1.5) {
            // Whirlpool's one-shot flash (prototype): half-white, grows 3r -> 5r, alpha <= 0.6.
            glow=mix(c,white,0.5)*falloff(length(coord)/(ub.motionScale==1.0?3.0+2.0*age:4.0))*0.6*(1.0-age);
        } else if(kind==6 && packed.z>0.5) {
            // Thaw crack (coord in snake radii, quad 6 r): seven ice shards fly out
            // from just outside the head (prototype shatter), a thin pop ring and a
            // short frost puff. Shards taper from about 1.6 px to a point.
            float r=length(coord);float px=max(fwidth(r),0.0001);
            float burst=1.0-pow(1.0-age,3.0);
            vec3 shard=mix(c,white,0.6);float shards=0.0;
            for(int k=0;k<7;k++) {
                float angle=float(k)*0.897598+hash(packed.y+float(k))*0.5;
                vec2 direction=vec2(cos(angle),sin(angle));
                float start=1.2+3.4*burst*(0.6+0.6*hash(packed.y+float(k)*3.0+1.0));
                float length_=1.2*(1.0-age);
                float along=clamp(dot(coord,direction)-start,0.0,length_);
                float w=px*(0.25+0.6*(1.0-along/max(length_,0.0001)));
                shards=max(shards,coverage(length(coord-direction*(start+along))-w,px)*step(0.001,length_));
            }
            float pop=coverage(abs(r-(1.3+1.2*burst))-px*0.6,px)*pow(1.0-age,3.0);
            glow=shard*(shards*(1.0-age)+pop*0.6)+c*falloff(r/1.9)*0.30*(1.0-age)*(1.0-age);
        } else if(kind==6) {
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
    bool sweet=halloween() && kind==2 && fract(phase*0.4774648+0.13)<(1.0/3.0);
    if(sweet) {
        float a=phase*3.0;vec2 q=mat2(cos(a),-sin(a),sin(a),cos(a))*p;
        float candy=length(q)-0.95;
        vec2 w=vec2(abs(q.x),q.y);
        float flare=0.16+0.60*clamp((w.x-0.80)/1.15,0.0,1.0);
        float wrapper=max(max(0.80-w.x,w.x-1.95),abs(w.y)-flare);
        float candyMask=mask(candy),wrapMask=mask(wrapper)*(1.0-candyMask);
        core=max(candyMask,wrapMask*0.9);
        float stripe=coverage(abs(fract((q.x*0.7+q.y*0.7)*0.62+0.25)-0.5)*1.6-0.22,max(px*0.8,0.06));
        vec3 sweetColor=mix(mix(c,white,0.15),white,stripe*0.70);
        rgb=mix(mix(c,white,0.50),sweetColor,candyMask/max(core,0.0001));
        glow=c*0.38*tw*falloff(r/4.6);
    } else if(kind==2) {                     // spark
        core=mask(r-1.05);
        rgb=mix(mix(c,white,0.20),white,mask(length(p-vec2(-0.15,-0.20))-0.45)*0.85);
        glow=c*0.42*tw*falloff(r/4.6);
        float q=sin(t*0.9+phase*5.0);
        if(q>0.95) {
            float a=(q-0.95)/0.05;
            float twinkleCross=(1.0-smoothstep(0.4*px,1.0*px,min(abs(p.x),abs(p.y))))*step(max(abs(p.x),abs(p.y)),4.5*a);
            over+=white*twinkleCross*0.75*a*(1.0-core);
        }
    } else if(kind==3 && halloween()) {   // essence bat
        bool still=ub.motionScale!=1.0;
        float flap=still?1.0:0.78+0.22*sin(t*8.2+phase*7.0);
        vec2 q=vec2(p.x,(p.y-(still?0.0:0.16*sin(t*4.1+phase*3.0)))/flap);
        core=atlasMask(12,q,2.4);
        rgb=mix(c,white,0.30);
        glow=c*(0.40+0.10*sin(t*2.0+phase))*falloff(r/4.4);
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
    } else if(packed.w>254.5 && halloween()) {   // pumpkin seed: a green bud ripening to orange
        float progress=packed.z/255.0;
        float angle=mod(atan(p.x+0.000001,-p.y)+6.283185,6.283185);
        float hw=max(0.10,0.9*px);
        float ring=1.0-smoothstep(hw,hw+px,abs(r-2.7));
        float fill=step(angle,progress*6.283185)*step(0.001,progress);
        vec3 ember=mix(hallow(vec3(0.80,0.20,0.05)),hallow(H_CANDLE),angle/6.283185);
        vec3 track=hallow(vec3(0.20,0.13,0.27));
        over+=(ember*fill*0.95+track*(1.0-fill))*ring;
        float ea=progress*6.283185;
        vec2 tip=vec2(sin(ea),-cos(ea))*2.7;
        float td=length(p-tip);float fl=candle(phase*5.0);
        over+=(mix(hallow(H_CANDLE_CORE),white,0.3)*mask(td-max(0.17,1.1*px))*0.9+hallow(H_FLAME)*falloff(td/0.95)*0.60*fl)
            *step(0.001,progress)*step(progress,0.999);
        float sr=0.35+0.45*progress;vec2 q=p/sr;
        float body=mask(pumpkinSd(q)*sr),stem=mask(stemSd(q)*sr);
        core=max(body,stem);
        vec3 tone=mix(hallow(H_UNRIPE),hallow(H_PUMPKIN),smoothstep(0.20,0.95,progress));
        rgb=mix(pumpkinSkin(q,tone),hallow(H_STEM),stem*(1.0-body*0.5));
        glow=mix(hallow(vec3(0.55,0.75,0.30)),hallow(vec3(1.0,0.58,0.20)),progress)*(0.14+0.26*progress)*falloff(r/4.0);
    } else if(packed.w>254.5) {       // prism seed: p is in prism visual units (Rust scales the quad 1.6x)
        float progress=packed.z/255.0;
        float angle=mod(atan(p.x+0.000001,-p.y)+6.283185,6.283185);   // 0 at top, clockwise
        float hw=max(0.10,0.9*px);
        float ring=1.0-smoothstep(hw,hw+px,abs(r-2.7));
        float fill=step(angle,progress*6.283185)*step(0.001,progress);
        over+=(spectral(angle/6.283185)*fill*0.95+vec3(0.20)*(1.0-fill))*ring;
        // fuse spark at the arc's leading end
        float ea=progress*6.283185;
        vec2 tip=vec2(sin(ea),-cos(ea))*2.7;
        float td=length(p-tip);
        over+=white*(mask(td-max(0.17,1.1*px))*0.9+falloff(td/0.9)*0.5)*step(0.001,progress)*step(progress,0.999);
        // pearl grows 0.35 -> 0.8
        float sr=0.35+0.45*progress;
        core=mask(r-sr);
        float g=clamp(length(p-vec2(-0.30,-0.35)*sr)/(1.25*sr),0.0,1.0);
        rgb=mix(white,vec3(0.80,0.84,0.92),smoothstep(0.2,1.0,g));
        glow=vec3(0.80,0.86,1.0)*(0.18+0.32*progress)*falloff(r/4.0);
    } else if(halloween()) {          // ripe jack-o'-lantern (kind 8); packed.w = ripe age
        bool moving=ub.motionScale==1.0;
        float pop=packed.w/254.0;
        float spin=moving?t*0.12:0.0;
        float grow=moving?1.0+0.16*sin(min(pop/0.45,1.0)*3.14159)*(1.0-pop)-0.15*(1.0-smoothstep(0.0,0.12,pop)):1.0;
        float R=1.0*grow;vec2 q=p/R;
        float body=mask(pumpkinSd(q)*R),stem=mask(stemSd(q)*R);
        core=max(body,stem);
        float fl=candle(phase*6.283185);
        // Ignition: the carved face lights over the first 0.35 of the pop, overshooting 25%.
        float lit=moving?smoothstep(0.0,0.35,pop)*(1.0+0.25*sin(clamp((pop-0.2)/0.5,0.0,1.0)*3.14159)):smoothstep(0.0,0.5,pop);
        vec3 skin=pumpkinSkin(q,hallow(H_PUMPKIN));
        vec2 fq=q-vec2(0.0,0.10);
        float fd=faceSample(fq,1.06);
        float faw=max(fwidth(fd),0.02);
        float face=smoothstep(-faw,faw,fd)*body;
        float carve=smoothstep(-faw,faw,fd+0.11)*body;   // cut edge, about 1 px outside each opening
        float hot=1.0-clamp(length(fq*vec2(0.8,1.15))/0.80,0.0,1.0);
        vec3 flame=mix(mix(hallow(H_FLAME),hallow(H_CANDLE),smoothstep(0.0,0.55,hot)),hallow(H_CANDLE_CORE),smoothstep(0.55,1.0,hot));
        vec3 dark=hallow(vec3(0.14,0.04,0.0));
        rgb=mix(skin,dark,carve);
        rgb=mix(rgb,mix(dark,flame*fl,clamp(lit,0.0,1.25)),face);
        rgb=mix(rgb,hallow(H_STEM),stem*(1.0-body*0.5));
        rgb=mix(rgb,hallow(vec3(0.14,0.04,0.0)),mask(abs(pumpkinSd(q)*R+0.035)-0.035)*body*0.75);
        // Candle light spills onto the skin around the carving.
        over+=hallow(H_CANDLE)*falloff(length(fq)/0.95)*0.12*lit*fl*(1.0-carve)*body;
        float life=packed.z/255.0*30.0;
        float drain=clamp(life/5.0,0.0,1.0);
        float ra=mod(atan(p.x+0.000001,-p.y)+6.283185,6.283185);
        float hw=max(0.11,0.9*px);
        float halo=(1.0-smoothstep(hw,hw+px,abs(r-1.75)))*step(ra,drain*6.283185+0.0001);
        // Orange-to-violet halo, turning slowly.
        float k=0.5+0.5*cos(ra-spin*6.283185);
        over+=mix(hallow(H_VIOLET),hallow(vec3(1.0,0.55,0.12)),k)*halo*0.95*(1.0-core);
        float rot=moving?t*0.25:0.2;
        float sector=6.283185/8.0;
        float a8=atan(p.y,p.x)+rot;
        float idx=floor(a8/sector+0.5);
        float local=a8-idx*sector;
        vec2 rq=vec2(cos(local),sin(local))*r;
        float longRay=mod(idx,2.0)==0.0?1.0:0.0;
        float len=mix(2.35,2.85,longRay);
        float ray=mask(line(rq,vec2(2.05,0.0),vec2(len,0.0),max(0.07,0.6*px)));
        over+=mix(hallow(H_VIOLET),hallow(H_CANDLE),longRay)*ray*(0.55+0.3*longRay)*(1.0-core);
        float shockRadius=1.75+2.4*pop;
        float shockOuter=min(hw+px,BOUNDS_FOOD-shockRadius);
        float shockInner=shockOuter<hw+px?min(hw,shockOuter*0.5):hw;
        float shock=(1.0-smoothstep(shockInner,shockOuter,abs(r-shockRadius)))*(1.0-pop)*step(pop,0.999);
        over+=mix(hallow(H_CANDLE_CORE),hallow(H_FLAME),0.4)*shock*0.8;
        glow=hallow(vec3(1.0,0.56,0.20))*(0.80*tw*fl+0.5*(1.0-pop))*falloff(r/4.6)*0.75;
    } else {                          // ripe prism fruit (kind 8); packed.w = ripe age, 0..254 over ~1 s
        bool moving=ub.motionScale==1.0;
        float pop=packed.w/254.0;
        float spin=moving?t*0.12:0.0;
        float grow=moving?1.0+0.16*sin(min(pop/0.45,1.0)*3.14159)*(1.0-pop)-0.15*(1.0-smoothstep(0.0,0.12,pop)):1.0;
        float R=1.0*grow;
        float rr=r/R;
        core=mask(r-R);
        float angle=atan(p.y,p.x)/6.283185;
        // pearl with an iridescent fresnel rim
        float g=clamp(length(p-vec2(-0.32,-0.38)*R)/(1.3*R),0.0,1.0);
        vec3 pearl=mix(white,mix(spectral(angle+spin),white,0.45),smoothstep(0.15,0.75,g));
        rgb=mix(pearl,spectral(angle+spin+0.5)*0.95,smoothstep(0.62,1.0,rr)*0.85);
        // halo ring: full spectrum around the circle, turning slowly; drains in the last 5 s
        float life=packed.z/255.0*30.0;
        float drain=clamp(life/5.0,0.0,1.0);
        float ra=mod(atan(p.x+0.000001,-p.y)+6.283185,6.283185);
        float hw=max(0.11,0.9*px);
        float halo=(1.0-smoothstep(hw,hw+px,abs(r-1.75)))*step(ra,drain*6.283185+0.0001);
        over+=spectral(ra/6.283185+spin)*halo*0.95*(1.0-core);
        // eight dispersion rays outside the halo, long/short alternating
        float rot=moving?t*0.25:0.2;
        float sector=6.283185/8.0;
        float a8=atan(p.y,p.x)+rot;
        float idx=floor(a8/sector+0.5);
        float local=a8-idx*sector;
        vec2 q=vec2(cos(local),sin(local))*r;
        float longRay=mod(idx,2.0)==0.0?1.0:0.0;
        float len=mix(2.35,2.85,longRay);
        float ray=mask(line(q,vec2(2.05,0.0),vec2(len,0.0),max(0.07,0.6*px)));
        over+=spectral(idx/8.0+spin)*ray*(0.55+0.3*longRay)*(1.0-core);
        // ripening pop: one expanding ring and a brief brighter glow
        float shockRadius=1.75+2.4*pop;
        // At tiny display scales, keep the pixel-wide shock inside the unchanged quad.
        float shockOuter=min(hw+px,BOUNDS_FOOD-shockRadius);
        float shockInner=shockOuter<hw+px?min(hw,shockOuter*0.5):hw;
        float shock=(1.0-smoothstep(shockInner,shockOuter,abs(r-shockRadius)))*(1.0-pop)*step(pop,0.999);
        over+=mix(white,spectral(ra/6.283185),0.5)*shock*0.8;
        glow=mix(spectral(spin*2.0),white,0.65)*(0.85*tw+0.5*(1.0-pop))*falloff(r/4.6);
    }
    float expiry=1.0;
    float fadeThreshold=kind==4?96.0:26.0;
    if(kind==8) expiry=packed.w>254.5?1.0:clamp(packed.z/8.5,0.0,1.0);   // prize: the halo drains, then a 1 s fade, no blink
    else if(kind!=2 && packed.z<fadeThreshold) expiry=packed.z/fadeThreshold*(0.65+0.35*sin(t*18.0+phase));
    glow*=1.0+0.6*night;
    fragColor=composite(rgb,core,glow,over,expiry);
}
