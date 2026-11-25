#version 440
layout(std140,binding=0) uniform Window {
    mat4 matrix; vec4 eye; vec4 motion; vec4 glass; vec4 light; vec4 flare;
    vec4 colors[6]; vec4 rim; vec4 guide; vec4 mote; vec4 display;
} u;
layout(binding=1) uniform sampler2D chamber;
layout(binding=2) uniform sampler2D bloom;
layout(location=0) in vec2 local;
layout(location=0) out vec4 color;
const float PI=3.14159265359;
vec2 rotate(vec2 p,float a){float c=cos(a),s=sin(a);return mat2(c,s,-s,c)*p;}
// Equilateral mirror group. Colour the vertices of the triangular grid 0,1,2
// and fold each triangle onto the reference triangle through its barycentrics.
vec2 triangular(vec2 p){
    float y=p.y*1.15470053838;
    vec2 grid=vec2(p.x-y*0.5,y);
    vec2 cell=floor(grid), f=fract(grid);
    vec3 bary;
    if(f.x+f.y<=1.) bary=vec3(1.-f.x-f.y,f.x,f.y);
    else bary=vec3(f.x+f.y-1.,1.-f.y,1.-f.x);
    float id=mod(cell.x-cell.y,3.);
    vec3 weights=id<0.5?bary:(id<1.5?bary.zxy:bary.yzx);
    return vec2(weights.y+weights.z*0.5,weights.z*0.8660254);
}
vec2 reflectBox(vec2 q,vec2 lo,vec2 hi){
    return lo+(hi-lo)*(1.-abs(mod((q-lo)/(hi-lo),2.)-1.));
}
// Physical pixel distances to a point reflected into this petal. The inverse
// radial and angular scales keep a six-pixel star round in every echo.
vec2 spotPixels(vec2 polar,vec2 source,float ring,float radius,float eyeRadius,float points) {
    float dr=(polar.x-source.x)/(ring>0.?0.65:1.);
    float da=(polar.y-source.y)*PI/(points*(ring+1.));
    return eyeRadius*vec2(dr,da*radius);
}
void main(){
    vec2 p=(local-u.eye.xy)/u.eye.z;
    float radius=length(p), k=floor(radius);
    float relief=1., loss=1., seam=0., soft=0.;
    vec2 samplePoint;
    vec2 polar=vec2(0.);
    float catchLight=0.;
    if(u.motion.w<0.5){
        float points=u.eye.w*(k+1.);
        float a=atan(p.y,p.x)+u.motion.x;
        if(k>0.) a+=mix(-1.,1.,mod(k,2.))*u.motion.y;
        float fold=abs(fract(a*(points/(2.*PI))+0.5)*2.-1.);
        float w=fold*PI/u.eye.w;
        float f=fract(radius), path=f;
        if(k>0.) path=0.35+0.65*mix(f,1.-f,mod(k,2.));
        polar=vec2(path,fold);
        samplePoint=vec2(-0.82,0.46)+1.67*path*vec2(cos(w+0.02),sin(w+0.02)-0.10);
        samplePoint+=u.glass.y*u.guide.xy*(1.-smoothstep(0.25,0.53,path));
        // Fixed reflection fade 35. Each bounce loses and scatters light.
        loss=k<0.5?1.:(k<1.5?0.55:(k<2.5?0.40:max(0.22,0.32-0.036*(k-3.))));
        soft=k<0.5?0.:(k<1.5?0.35:0.60);
        float band=k>0.?min(f,1.-f):1.-f;
        float taper=mix(1.,0.625,min(1.,(k+step(0.5,f))*0.25));
        relief=smoothstep(0.008*taper,0.020*taper,band);
        seam=0.015*exp2(-min(fold,1.-fold)*max(radius,0.018)*u.eye.z*0.33/(k+1.))
             +0.022*exp2(-abs(band-0.019)*u.eye.z*0.75);
        float seamPx=min(fold,1.-fold)*PI/points*radius*u.eye.z;
        catchLight=0.020*(1.-smoothstep(0.9,3.8,seamPx));
    } else{
        vec2 tile=triangular((rotate(p,u.motion.x)+vec2(u.motion.z/u.eye.z,0.))/0.68);
        samplePoint=vec2(-1.07,-0.14)+vec2(2.14*tile.x,1.15*tile.y);
        loss=0.22+0.78*exp2(-radius*0.59);soft=min(0.45,0.08*radius);
        seam=0.009*exp2(-min(tile.y,min(tile.x*1.732-tile.y,(1.-tile.x)*1.732-tile.y))*u.eye.z*0.65);
    }
    samplePoint=vec2(-0.18,0.50)+(samplePoint-vec2(-0.18,0.50))/u.glass.y;
    samplePoint=reflectBox(samplePoint,vec2(-0.98,-0.11),vec2(0.98,1.02));
    vec2 uv=(samplePoint-vec2(-1.05,-0.18))/vec2(2.10,1.26);
    vec4 transmitted=texture(chamber,uv);
    vec3 sharp=transmitted.rgb, blur=texture(bloom,uv).rgb;
    vec3 c=mix(sharp,blur,soft);
    float peak=max(blur.r,max(blur.g,blur.b));
    c+=blur*u.glass.x*0.27*smoothstep(0.33,0.65,peak);
    if(u.rim.w==2.) {
        // Thick garnet absorbs the green in gold transmitted by an overlapping
        // bead. Thin bevels retain the amber transition and pale-gold rim.
        float value=max(c.r,max(c.g,c.b));
        vec3 garnet=u.colors[4].rgb/u.colors[4].r;
        float thickness=smoothstep(0.50,0.94,transmitted.a);
        c=mix(c,value*garnet,thickness*(1.-smoothstep(0.43,0.79,value)));
    }
    // Deep transmission is most apparent away from a specular edge.
    // Leave the opal body luminous and preserve the eye's bright rim cores.
    if(u.glass.z<0.5) {
        float value=max(c.r,max(c.g,c.b));
        c*=mix(0.76,1.,smoothstep(0.40,0.77,value));
    }
    c*=relief*(1.-0.35*u.light.z);
    // Incoming pieces travel together through the shadow at the mouth of
    // the tube. Restore the key as they spread out and settle into the cell.
    bool magnifiedMandala=u.motion.w<0.5 && u.glass.y>=1.5 && u.eye.w>=12.;
    if(!magnifiedMandala && u.flare.w>=3.25 && u.flare.w<7.)
        c*=mix(0.30,1.,u.glass.w*u.glass.w);
    float darkBeat=1.-u.glass.w;
    c+=u.rim.rgb*seam;

    // Preserve hue through the shoulder. Independent channel compression
    // would make pale gold, red and mint converge on the same colour.
    peak=max(c.r,max(c.g,c.b));
    if(peak>0.88) c*=(0.88+0.12*(1.-exp(-(peak-0.88)/0.12)))/peak;
    float gain=u.motion.w<0.5 && k>=2. ? min(0.59,sqrt(loss)) : sqrt(loss);
    c*=gain;
    peak=max(c.r,max(c.g,c.b));
    if(u.rim.w==2. && c.r>=c.g && c.g>c.b) {
        // Garnet absorbs green from a bright overlapping amber path. Thin
        // reflections retain more honey. Keep the incident gold end of the
        // palette fixed and interpolate continuously through copper.
        float chroma=c.r-c.b;
        float hue=60.*(c.g-c.b)/max(chroma,0.0001);
        if(hue<40.) {
            float light=smoothstep(0.19,0.64,peak);
            float red=mix(3.,18.,light), honey=mix(28.,30.,light);
            if(u.motion.w<0.5 && k>=1.) {
                red=mix(5.,7.,light); honey=mix(21.,23.,light);
            }
            float transmittedHue=hue<red ? 10.*hue/red :
                (hue<honey ? 10.+20.*(hue-red)/(honey-red) : 30.+10.*(hue-honey)/(40.-honey));
            c.g=c.b+chroma*transmittedHue/60.;
        }
    }
    if(u.rim.w==1. && peak<0.68 && c.b<min(c.r,c.g)) {
        // In absorption a mixed amber/mint path retains the deeper amber
        // neighbour. This also keeps reflections in the outer field warm.
        float chroma=max(0.001,peak-c.b);
        float hue=60.*(1.+(c.g-c.r)/chroma);
        float olive=smoothstep(40.,49.,hue)*(1.-smoothstep(80.,96.,hue))*
                    smoothstep(0.17,0.32,chroma/max(peak,0.001));
        vec3 amber=u.colors[5].rgb;
        c=mix(c,amber*(peak/max(amber.r,0.001)),olive*(1.-smoothstep(0.57,0.68,peak)));
    }

    float swapPhase=magnifiedMandala && u.motion.z<0. ? -1.-u.motion.z : u.flare.w;
    c+=u.rim.rgb*catchLight*darkBeat*(1.-smoothstep(4.,4.55,swapPhase));

    // During a magnified pour, stars belong to the transmitted glass beneath
    // them. A departing body cannot leave a brighter white point in an empty eye.
    bool keyedGlints=magnifiedMandala && u.motion.z<0.;
    float glassPeak=max(c.r,max(c.g,c.b));
    float ring=u.motion.w<0.5 ? k : 0.;
    float age=u.flare.z-0.25*ring;
    if(age>0. && age<1.9 && ring<=2. && darkBeat<0.01) {
        vec2 spot=(samplePoint-u.flare.xy)*u.glass.y/1.67;
        vec2 pixel=u.motion.w<0.5?spotPixels(polar,u.guide.zw,ring,radius,u.eye.z,u.eye.w):spot*u.eye.z;
        vec3 star=mix(u.rim.rgb,vec3(1.),0.68);
        if(ring<0.5) {
            float dist=length(pixel)/u.eye.z;
            float core=1.-smoothstep(0.0103,0.0122,dist);
            float halo=0.35*exp2(-dist/0.019)*(1.-smoothstep(0.032,0.04,dist));
            float rays=0.22*exp2(-min(abs(pixel.x),abs(pixel.y))/(0.0015*u.eye.z))*exp2(-dist/0.019);
            float flash=smoothstep(0.,0.4,age)*(1.-smoothstep(0.4,1.9,age));
            vec3 highlight=star*(core+halo+rays)*flash*relief;
            c=keyedGlints ? max(c,min(highlight,vec3(glassPeak))) : c+highlight;
        } else {
            // The echo is light scattered at each reflected point. Project it
            // into the inner half of the band so the copies fit a wide window.
            float echoRadius=ring+0.82*mix(u.guide.z,1.-u.guide.z,mod(ring,2.));
            pixel.x=(radius-echoRadius)*u.eye.z;
            float dist=length(pixel)/u.eye.z;
            float core=exp2(-pow(dist/0.009,2.));
            float halo=0.25*(1.-smoothstep(0.,0.030,dist));
            float rays=0.33*exp2(-min(abs(pixel.x),abs(pixel.y))/(0.0015*u.eye.z))
                           *(1.-smoothstep(0.,0.030,dist));
            float flash=smoothstep(0.,0.25,age)*exp2(-max(0.,age-0.25)/0.4)
                        *(1.-smoothstep(1.5,1.9,age));
            // Encoded silver peaks correspond to about 60% and 35% linear light.
            vec3 silver=mix(star,vec3(1.),0.8)*(ring<1.5?0.86:0.64);
            float reflected=min(1.,core+halo+rays)*flash*relief;
            vec3 highlight=silver*reflected;
            c=max(c,keyedGlints ? min(highlight,vec3(glassPeak)) : highlight);
        }
    }
    if(u.motion.w<0.5 && u.mote.z>0.) {
        vec2 pixel=spotPixels(polar,u.mote.xy,k,radius,u.eye.z,u.eye.w);
        float d=length(pixel)*676.8/u.eye.z;
        float core=1.-smoothstep(3.6,4.3,d);
        float halo=0.25*(1.-smoothstep(3.5,12.,d));
        vec3 highlight=vec3(0.98,0.99,1.)*(core+halo)*u.mote.z*gain*relief;
        c=keyedGlints ? max(c,min(highlight,vec3(glassPeak))) : c+highlight;
    }
    float grain=(fract(52.9829189*fract(dot(floor(p*u.eye.z),vec2(0.06711056,0.00583715))))-0.5)/255.;
    color=vec4(clamp(c+grain,0.,1.)*u.display.x,0.);
}
