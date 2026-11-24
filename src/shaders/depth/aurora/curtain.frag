#version 440
layout(std140,binding=0) uniform Camera {
    mat4 projection; vec4 bounds; vec4 colors[6]; vec4 top; vec4 bottom; vec4 style; vec4 clock; vec4 view; vec4 optics; vec4 overlap[8]; vec4 phases;
} sky;
layout(binding=1) uniform sampler2D fieldAtlas;
layout(location=0) in vec2 uv;
layout(location=1) in vec4 light;
layout(location=2) flat in vec4 surface;
layout(location=3) in vec2 world;
layout(location=4) in float hemSlope;
layout(location=0) out vec4 color;
float patchHash(vec2 p) {
    vec3 v=fract(p.xyx*vec3(.1031,.1030,.0973));
    v+=dot(v,v.yxz+33.33);
    return fract((v.x+v.y)*v.z);
}
float patchNoise(vec2 p) {
    vec2 i=floor(p), u=fract(p);
    u=u*u*(3.-2.*u);
    return mix(mix(patchHash(i),patchHash(i+vec2(1,0)),u.x),
               mix(patchHash(i+vec2(0,1)),patchHash(i+vec2(1)),u.x),u.y);
}
void main() {
    float kind=surface.x;
    vec3 rgb;
    float resolved=0.;
    if (kind==1.) {
        vec2 p=uv*2.-1.;
        float r=dot(p,p);
        float star=exp2(-5.3*r)*light.y
            *(0.90+0.10*sin(sky.clock.x*0.20+surface.y));
        rgb=sky.optics.rgb*star;
    } else if (kind==2. || kind==3.) {
        float h=(uv.y*(kind==3.?0.39:1.)-0.12)/0.88;
        float row=(surface.y+0.5)/64.;
        float band=floor(surface.y/8.)*8.;
        float longExposure=smoothstep(1.,1.95,surface.y-band)*smoothstep(1.,2.,sky.style.w);
        vec4 broad=texture(fieldAtlas,vec2(light.x*0.5+h*0.038,row));
        // A tall, rapidly advecting curtain retains a persistent oxygen crown.
        // Substantial material structure survives as short-lived rays cross it.
        if (longExposure>0.)
            broad=mix(broad,texture(fieldAtlas,vec2(uv.x*.5+h*.038,row)),longExposure);
        // Mild bending in altitude breaks up rigid columns. The horizontal
        // footprint averages the material when it is compressed at a turn.
        float phase=6.2831853*light.x;
        float y=(world.y-sky.bounds.y)*1440./sky.bounds.w;
        float crowded=smoothstep(.45,.95,sky.style.y);
        float bend=(.0022+.0027*crowded)*sin(y/36.+1.5*phase+2.1*broad.a)
                   +.0013*sin(y/19.-phase);
        float rayU=light.x+.028*broad.a+.028*h*h+bend;
        float spread=clamp(2.1*fwidth(rayU),.0015,.012);
        // Resolve each ray bundle against both its spatial footprint and
        // the exposure of a fast camera. Eight mip levels belong to each arc.
        float exposure=max(surface.y-band,1.4+2.3*crowded);
        float fineRow=(band+min(7.,exposure)+.5)/64.;
        vec4 fine=.5*texture(fieldAtlas,vec2(rayU,fineRow))
                 +.25*(texture(fieldAtlas,vec2(rayU-spread,fineRow))+texture(fieldAtlas,vec2(rayU+spread,fineRow)));
        float up=max(h,0.);
        if (longExposure>0.) {
            float fixedU=uv.x+.028*broad.a+.028*h*h;
            vec4 persistent=texture(fieldAtlas,vec2(fixedU,fineRow));
            fine=mix(fine,persistent,longExposure*mix(.32,.92,smoothstep(.06,.43,up)));
        }
        float ray=smoothstep(.09,.52,mix(fine.g,fine.b,.17));
        // Both oscillators close at the two-repeat material wrap. The knots
        // drift with the rays over approximately 400 to 800 screen pixels.
        float knotPhase=6.2831853*uv.x;
        float knot=smoothstep(.24,.76,.46+.54*cos(1.5*knotPhase+surface.w*5.+.79*sin(.5*knotPhase+surface.w*3.)));
        float envelope=mix(.50,1.40,knot)*(.73+.55*broad.r);
        float fray=.72+.29*broad.a+.16*ray;
        float taper=1.-smoothstep(fray*.63,fray+.03,h);
        float distance=light.z*1440./sky.bounds.w;
        // A resolved sloping foot has a measured normal profile of its own.
        // Preserve it when reconstructing broader emission higher on the ray.
        resolved=float(kind==2.)*smoothstep(.50,.78,hemSlope)*(1.-smoothstep(13.,29.,abs(distance)))
                *smoothstep(.012,.12,light.y);
        float far=smoothstep(.72,.89,(world.y-sky.bounds.y)/sky.bounds.w);
        float soft=mix(3.,9.,far);
        float body=exp(-3.0*up)*smoothstep(-soft,soft,distance)*taper;
        float trail=sky.style.x;
        float haloGain=trail<.35 ? .18*trail/.35 : mix(.18,.26,(trail-.35)/.65);
        float haloWidth=mix(30.,12.,far)*(1.+smoothstep(.35,1.,trail));
        float neighbours=0.;
        float gap=1.;
        bool densePastel=sky.style.z==5. && sky.style.y>.6 && sky.style.w>1.;
        if (densePastel) {
            // Sample the same projected layers on either side of an output join.
            vec2 cell=clamp((world-sky.bounds.xy)/sky.bounds.zw*vec2(8.,4.)-.5,
                            vec2(0.),vec2(7.,3.));
            ivec2 at=ivec2(cell), next=min(at+1,ivec2(7,3));
            vec4 column=mix(sky.overlap[at.x],sky.overlap[next.x],fract(cell.x));
            neighbours=max(0.,mix(column[at.y],column[next.y],fract(cell.y))-1.);
            haloWidth/=1.+.25*neighbours;
            // Neighbouring sheets follow the same magnetic flux bundles. Their
            // diffuse overlap must preserve gaps in precipitation before screen blending.
            float nx=(world.x-sky.clock.w)*1440./sky.bounds.w;
            float fieldLean=mix(.22,.44,clamp(sky.phases.z,0.,1.));
            float foot=(world.x-sky.bounds.x)*1440./sky.bounds.w
                    -fieldLean*nx/sqrt(nx*nx+150.*150.)*(y-740.);
            float fieldU=(foot-12.*sky.clock.x)/1024.;
            float flux=.5*texture(fieldAtlas,vec2(fieldU,3.5/64.)).g
                      +.25*(texture(fieldAtlas,vec2(fieldU-.007,3.5/64.)).g
                           +texture(fieldAtlas,vec2(fieldU+.007,3.5/64.)).g);
            float precipitation=smoothstep(.15,.55,flux);
            gap=mix(1.,mix(.06,1.12,precipitation),mix(.44,.94,smoothstep(.10,1.8,neighbours)));
        }
        float halo=haloGain*exp(-abs(distance)/haloWidth);
        float weight=min(.90,mix(.78,.82,smoothstep(.04,.57,up))*surface.z);
        // Pastel softens the optical footprint while retaining material contrast.
        if (sky.style.z==5.)
            weight=max(.5,weight);
        // At grazing angles several fine rays can cover a pixel. Their mean
        // emission survives filtering and the individual rays disappear gently.
        weight*=1.-smoothstep(.0025,.010,fwidth(rayU));
        // The ray material coordinate is constant along a field line. Its
        // screen derivatives give the line's tilt during a corona.
        float tilt=abs(dFdy(light.x))/max(.00001,abs(dFdx(light.x)));
        weight*=1.-.64*smoothstep(.4663,.7002,tilt);
        // Excitation broadens the ray feet across neighbouring field lines.
        // Fine structure returns above the bright lower fringe of the surge.
        float stormFoot=smoothstep(.045,.30,light.w)*(1.-smoothstep(62.,105.,abs(distance)));
        weight*=1.-max(.55*light.w,.96*stormFoot);
        if (densePastel)
            weight=max(.5,weight);
        // Broad mottling can dominate a compressed corona sheet. Keep faint
        // field-aligned striation and let that diffuse excitation yield to the
        // resolved rays and bright hem as the crown opens.
        float corona=clamp(sky.phases.z,0.,1.);
        if (corona>0.) {
            weight=max(weight,.20*corona);
            body*=1.-.70*corona*broad.r;
            // Use angular sky spacing for these faint bundles: a highly folded
            // material coordinate would compress them back into a blurred blob.
            float nx=(world.x-sky.clock.w)*1440./sky.bounds.w;
            float foot=(world.x-sky.bounds.x)*1440./sky.bounds.w
                -.44*nx/sqrt(nx*nx+150.*150.)*(y-740.);
            float strands=smoothstep(.18,.82,patchNoise(vec2(
                foot/28.+19.1*surface.w,y/450.+surface.w)));
            body*=1.+.40*corona*smoothstep(.10,.25,up)*(strands-.5);
        }
        float rays=1.+weight*(2.*ray-1.);
        vec3 base=mix(sky.colors[1].rgb,sky.colors[2].rgb,.16+.48*broad.a);
        vec3 crown=mix(sky.colors[3].rgb,sky.colors[4].rgb,.30+.33*broad.r);
        if(sky.style.z==1.) {
            // Hue moves on the continuous sky. The advected ray coordinate
            // wraps every two texture repeats and cannot drive a long-period tint.
            float hue=(world.x-sky.bounds.x)/1850.+surface.w+sky.phases.x;
            base=mix(sky.colors[1].rgb,sky.colors[2].rgb,.12+.16*sin(hue));
        } else if(sky.style.z==5.) {
            base=surface.w<.25?sky.colors[1].rgb:surface.w<.75?sky.colors[2].rgb:sky.colors[3].rgb;
            base=clamp(vec3(max(max(base.r,base.g),base.b))+
                       1.85*(base-vec3(max(max(base.r,base.g),base.b))),0.,1.);
            crown=sky.colors[0].rgb;
        }
        float crownMix=smoothstep(.15,.62,up);
        vec3 dye=mix(base,crown,crownMix);
        if(sky.style.z==2.) {
            dye=mix(base,crown,smoothstep(.08,.51,up));
            body*=1.+.65*crownMix;
        }
        float fringe=exp(-abs(distance)/mix(3.8+3.2*light.w,9.,far));
        if (kind==3.) {
            // Pastel's wide afterglow shares the sheet hue instead of carrying
            // a desaturated crown band below the lower edge.
            vec3 ghostDye=sky.style.z==5. ? base : crown;
            float ghostGain=(.18*trail)*(sky.style.z==5.?.70:1.);
            rgb=ghostDye*ghostGain*exp(-abs(distance)/(4.+48.*trail))
                *(1.-smoothstep(.8,1.,uv.y));
        } else {
            // The wide Pastel halo inherits the sheet's chroma rather than
            // carrying a desaturated grey ramp below its lower edge.
            vec3 haloDye=sky.style.z==5. ? mix(base,sky.colors[0].rgb,.18) : dye;
            rgb=(dye*body*rays+haloDye*halo*(sky.style.z==5.?.70:1.))*envelope;
            vec3 hem=mix(base,sky.colors[0].rgb,.72);
            if(sky.style.z==1.) {
                float wide=exp(-abs(distance)/20.);
                rgb=mix(rgb,sky.colors[0].rgb*envelope*(.92+.4*light.w),
                        clamp(fringe+.42*wide,0.,1.));
            } else {
                rgb+=hem*fringe*mix(.62,.21,far)*envelope*rays;
            }
            // Hard-excited feet remain brighter than the neighbouring body.
            // The storm gain in light.y multiplies every height, including this hem.
            rgb+=sky.colors[0].rgb*light.w*envelope*(fringe+.45*exp(-abs(distance)/32.));
        }
        float footprint=smoothstep(0.,.035,uv.y)*(1.-smoothstep(.86,1.,uv.y));
        rgb*=footprint*light.y*gap/(1.+.64*neighbours);
    } else if(kind==4.) {
        vec2 p=2.*uv-1.;
        float angle=atan(p.y,p.x+.00001);
        float phase=surface.y*2.39;
        float radius=.79*(1.+.19*sin(3.*angle+phase)+.08*cos(5.*angle-phase));
        vec2 site=world/(sky.bounds.w/1440.)/137.+vec2(phase);
        float m=.68*patchNoise(site)+.32*patchNoise(site*2.13+17.1);
        float contour=1.-dot(p,p)/(radius*radius);
        rgb=mix(sky.colors[1].rgb,sky.colors[2].rgb,.36)*light.y
            *(.60+.80*m)*pow(max(0.,contour),1.65);
        // Faint magnetic striation keeps diffuse islands related to the rays.
        float toward=sky.clock.w-world.x;
        float lean=.20*toward/sqrt(toward*toward+150.*150.);
        float rayX=(world.x+lean*(world.y-sky.bounds.y))*1440./sky.bounds.w;
        float striation=patchNoise(vec2(rayX/12.+phase,site.y*.16));
        rgb*=.8+.4*striation;
    } else {
        float along=surface.y>=1. ? smoothstep(0.,.12,light.x)*(1.-smoothstep(.85,1.,light.x)) : smoothstep(0.,.83,uv.x)*(1.-smoothstep(.90,1.,uv.x));
        float width=0.45+0.55*uv.x;
        float across=(2.*uv.y-1.)/width;
        rgb=mix(sky.colors[5].rgb,sky.colors[3].rgb,clamp(surface.y-1.,0.,1.))*light.y*along*exp2(-7.*across*across);
    }
    // Give every source the same Pastel exposure before accumulating emission.
    rgb*=sky.style.z==5.?.8:1.;
    if (kind==2. || kind==3.)
        rgb=1.-exp(-1.24*rgb);
    color=vec4(clamp(rgb,0.,1.),resolved);
}
