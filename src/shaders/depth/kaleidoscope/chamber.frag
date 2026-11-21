#version 440
layout(std140,binding=0) uniform Scene {
    vec4 eye; vec4 motion; vec4 glass; vec4 light; vec4 flare; vec4 colors[6]; vec4 rim; vec4 guide; vec4 mote; vec4 target;
} u;
layout(location=0) in vec2 local;
layout(location=1) flat in vec4 abOut;
layout(location=2) flat in vec4 cdOut;
layout(location=3) flat in vec4 efOut;
layout(location=4) flat in vec4 form;
layout(location=5) flat in vec4 pigment;
layout(location=6) flat in vec4 optics;
layout(location=0) out vec4 color;

vec3 chroma(vec3 c) { return c / max(0.001, max(c.r, max(c.g,c.b))); }

void main() {
    int n=int(form.z+0.5);
    vec2 v[6]=vec2[6](abOut.xy,abOut.zw,cdOut.xy,cdOut.zw,efOut.xy,efOut.zw);
    float sd=0., other=0.;
    vec2 normal, otherNormal=vec2(0.);
    if(n<=2) {
        vec2 q=local;
        if(n==2) q.x-=clamp(q.x,v[0].x,v[1].x);
        float r=length(q);
        sd=r-form.y; normal=q/max(r,0.00001);
    } else {
        sd=-10.; other=-10.; normal=vec2(1,0);
        for(int i=0;i<6;++i) {
            if(i>=n) break;
            vec2 edge=v[(i+1)%n]-v[i];
            vec2 axis=normalize(vec2(edge.y,-edge.x));
            float h=dot(local-v[i],axis);
            if(h>sd) {
                other=sd; otherNormal=normal; sd=h; normal=axis;
            } else if(h>other) {
                other=h; otherNormal=axis;
            }
        }
    }
    float glow=u.glass.x, zoom=u.glass.y, opal=u.glass.z;
    float pixel=1.67/(u.eye.z*zoom*0.93);
    float aa=max(fwidth(sd)*0.58,pixel*0.40)*mix(1.,1.5,form.w);
    if(n>2) {
        // Blend across the nearest two faces over the footprint of this pixel.
        float ridge=sd-other;
        normal=mix(0.5*(normal+otherNormal),normal,smoothstep(0.,max(pixel,fwidth(ridge)),ridge));
    }
    float inside=1.-smoothstep(-aa,aa,sd);
    float width=pixel*mix(0.62,1.0,glow);
    float core=1.-smoothstep(width*0.42,width+aa,abs(sd));
    float haloWidth=(0.002+0.019*glow)*u.eye.z*pixel;
    float halo=pow(1.-smoothstep(0.,haloWidth,abs(sd)),2.)*glow;
    float facing=dot(normal,optics.xy);
    float key=0.26+0.74*pow(max(facing,0.),0.78);
    float t=clamp(-sd/max(form.x,0.001),0.,1.);
    float bevel=exp2(-t*11.);
    int index=int(pigment.x+0.5);
    int pal=int(u.rim.w+0.5);
    vec3 base=u.colors[index].rgb;
    vec3 adjacent=u.colors[(index+1)%6].rgb;
    if(pal==2 && index==4) adjacent=u.colors[3].rgb;
    float warm=step(base.r-base.b,adjacent.r-adjacent.b);
    vec3 hue=mix(base,adjacent,warm*(0.019*u.light.w+0.05*u.light.z)
                 +pigment.y*(0.12+0.10*sin(optics.w+facing*2.5)));
    vec3 body=chroma(hue);
    // Palette-neighbour transmission in coloured glass. Absorbed yellow
    // light leaves amber and garnet in thicker portions of the piece.
    if(pal==2) {
        if(n==1) {
            // Gold transmits honey in the thick centre (H 32°, S 88%).
            // Deep red belongs to the garnet, a different piece of glass.
            body=mix(chroma(u.colors[1].rgb),vec3(1.,0.59,0.12),0.70+0.30*t);
            hue=u.colors[1].rgb;
        } else if(n==2) {
            body=chroma(u.colors[2].rgb);
            hue=u.colors[2].rgb;
        } else {
            body=chroma(mix(u.colors[4].rgb,u.colors[3].rgb,index==3 ? 0.5 : 0.05));
        }
    } else if(pal==1 && index==1) {
        body=chroma(mix(u.colors[1].rgb,u.colors[5].rgb,0.53+0.32*t));
    }
    float face=0.27+0.065*max(facing,0.)+0.095*bevel;
    // A lit flat face transmits roughly a fifth of the rim's linear light.
    // The thick centre and faces turned away from the key retain their dark tint.
    if(n>2) face+=0.21*pow(max(facing,0.),4.)*(1.-smoothstep(0.40,0.76,t));
    if(pal==2 && n>1) face*=0.85;
    if(opal>0.5) face=0.51+0.09*bevel+0.055*max(facing,0.);
    vec3 rim=chroma(mix(hue,u.rim.rgb,0.22));
    float facet=0.;
    vec3 hot=vec3(0.);
    if(n>2) {
        float ridge=sd-other;
        facet=0.31*exp2(-abs(ridge)/max(aa+pixel*0.8,fwidth(ridge)))*exp2(-t*3.)*key;
        facet+=0.25*exp2(-abs(t-0.105)/(0.012+aa/form.x))*key;
    } else if(n==1) {
        vec2 center=optics.xy*form.y*0.55;
        float spotRadius=min(0.072*form.y,5.*pixel);
        float d=length(local-center);
        float spot=1.-smoothstep(spotRadius*0.72,spotRadius+aa*0.55,d);
        vec3 white=mix(vec3(1.),pal==2?u.colors[0].rgb:rim,0.28);
        hot=white*(0.97*spot + glow*0.12*(1.-smoothstep(spotRadius,2.*spotRadius,d)));
        facet=0.22*exp2(-abs(t-0.18)/0.045)*max(0.,-facing);
    } else {
        float litSide=optics.y>=0.?1.:-1.;
        float along=1.-smoothstep(0.38,0.62,abs(local.x)/max(v[1].x,0.001));
        facet=0.77*along*exp2(-abs(local.y-litSide*0.62*form.y)/(pixel*0.85));
    }
    vec3 edge=rim*(core*key*1.04 + halo*key*0.24 + facet*inside);
    if(pal==2) {
        vec3 gold=u.colors[0].rgb;
        edge=mix(edge,gold*max(edge.r,max(edge.g,edge.b)),0.30);
    }
    vec3 emission=body*face*inside + edge + hot*inside;
    emission+=chroma(mix(rim,vec3(1.),0.60))*optics.z*pow(max(facing,0.),18.)*core;
    emission*=mix(1.,0.60+0.17*opal,form.w);
    if(pigment.z>0.5) {
        float d=length(local)*zoom;
        float mote=1.-smoothstep(0.0025,0.0065,d);
        emission=u.motion.w>0.5 ? vec3(1.)*optics.z*(mote+0.26*exp2(-d/0.008)) : vec3(0.);
    }
    // The target's alpha accumulates optical depth through garnet cuts.
    // Gold retains its hue along rays that only pass through beads.
    float garnet=pal==2 && n>2 ? inside*mix(0.22,1.,smoothstep(0.025,0.32,t)) : 0.;
    color=vec4(emission,garnet);
}
