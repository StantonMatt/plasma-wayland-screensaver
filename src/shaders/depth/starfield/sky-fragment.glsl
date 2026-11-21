// SPDX-License-Identifier: GPL-3.0-or-later
float square(float x){return x*x;}
vec4 skyLight(vec2 uv, vec2 screenPoint, vec4 material, vec4 detail){
    int kind=int(material.y);float gain=material.z*u.opacity.x;float rr=dot(uv,uv);
    vec3 rgb=vec3(0.);float absorb=0.;
    if(kind==0){
        vec2 q=(screenPoint-u.projection.yz)/u.projection.x;
        vec3 ray=u.projection.w>0.5?vec3(sin(q.x),-q.y,cos(q.x)):vec3(q.x,-q.y,1.);
        ray=normalize(ray); float beta=aberration*u.optics.x;float k=sqrt(1.-beta*beta);
        ray=normalize(ray*k+u.axis.xyz*((1.-k)*dot(ray,u.axis.xyz)-beta));
        ray=rotateEye(ray,vec4(-u.rotation.xyz,u.rotation.w));
        // An inclined galactic plane in inertial coordinates. The bulge and
        // filaments stay in the same sky when the observer rolls or changes yaw.
        vec3 normal=normalize(vec3(-0.23,0.88,0.10+0.09*sin(u.field.x*6.28)));
        float latitude=dot(normal,ray);
        vec2 domain=vec2(atan(ray.x,ray.z)*0.63662,latitude*1.45)+vec2(u.field.x,0.13);
        vec3 n=texture(noiseTile,domain*0.73).rgb;
        vec3 filament=texture(noiseTile,domain*2.7+(n.rg-0.5)*0.69).rgb;
        float plane=latitude+0.17*(n.g-0.5)+0.055*sin(domain.x*7.);
        float band=exp(-37.*plane*plane);
        float bulge=0.56+0.44*max(0.,dot(ray,normalize(vec3(0.46,0.02,0.82))));
        float clouds=smoothstep(0.33,0.69,n.b*0.48+filament.r*0.52);
        float lane=exp(-800.*square(plane+0.18*(filament.g-0.5)+0.022));
        vec3 tint=mix(gasEdge(),gasCore(),smoothstep(0.30,0.65,filament.b));
        rgb=tint*gain*band*bulge*(0.024+0.103*clouds)*(1.-0.83*lane)*(1.-0.15*min(1.,u.surface.y));
        absorb=0.035*gain*lane*lane*band;
        float radius=length(screenPoint-u.field.zw)/u.viewport.z;
        vec2 radial=screenPoint-u.field.zw;
        float angle=atan(radial.y,radial.x)-0.028*u.optics.y;
        float lobes=0.7+0.20*cos(11.*angle+u.field.x*6.28)+0.10*sin(14.*angle+0.9);
        float strength=smoothstep(0.0024,0.0815,u.oldAxis.w)+0.20*smoothstep(0.0815,0.399,u.oldAxis.w);
        float ring=0.041*u.field.y*strength*lobes*exp(-0.5*square((radius-0.28)/0.065));
        float grad=smoothstep(0.22,0.34,radius);
        vec3 bowColor=temperature(1.-grad);
        if(u.surface.w==0.) {
            vec3 inner=mix(u.palette[3].rgb,u.palette[2].rgb,min(2.*grad,1.));
            bowColor=mix(inner,u.palette[0].rgb,max(2.*grad-1.,0.));
        }
        if(u.surface.w==1.) bowColor=mix(u.palette[5].rgb,u.palette[0].rgb,grad);
        rgb+=bowColor*ring*u.opacity.x;
    }else if(kind==1){
        vec2 p=uv*vec2(0.55,0.46)+vec2(detail.x,detail.z);
        vec3 n=texture(noiseTile,p).rgb;
        vec3 fine=texture(noiseTile,uv*vec2(0.82,1.23)+(n.rg-0.5)*0.46+detail.x).rgb;
        float edge=exp(-2.9*rr)*(1.-smoothstep(0.68,1.,rr));
        float fold=fine.g+(n.r-0.5)*0.45;
        float cloud=smoothstep(0.15,0.85,fold);
        float ridge=exp(-54.*square(fold-0.55));
        float dust=smoothstep(0.52,0.80,fine.b+n.g*0.19)*edge;
        vec3 reflection=mix(gasEdge(),temperature(detail.z),0.18);
        vec3 core=mix(gasCore(),temperature(detail.y),0.22);
        if(u.surface.w==2.){
            vec3 crimson=mix(u.palette[0].rgb,u.palette[1].rgb,0.58);
            if(u.surface.y==2.){
                // Break the outer silhouette into folds using the same noise
                // that shapes the core. Pure emission stays additive over teal.
                float irregular=rr*(1.+smoothstep(0.16,0.85,rr)*(0.76*(n.g-0.5)+0.48*(fine.r-0.5)));
                edge=exp(-3.35*irregular)*(1.-smoothstep(0.64,0.97,irregular));
                edge*=1.-smoothstep(0.92,1.,rr);
            }else{
                edge*=exp(-0.45*rr);
            }
            float column=1.-exp(-2.8*edge*(0.44+0.56*cloud));
            float deep=smoothstep(0.48,0.85,column)*(0.45+0.55*ridge);
            vec3 body=crimson*0.24*column+vec3(0.32,0.003,0.009)*deep;
            float heart=smoothstep(0.25,0.66,edge*(0.40+0.60*cloud)*(0.42+0.58*ridge));
            vec3 hot=u.palette[4].rgb*vec3(0.50,0.367,0.29);
            rgb=mix(body,hot,heart)*gain*(1.-0.15*min(1.,u.surface.y));
            if(u.surface.y==2.){
                // On teal, add red to the yellow-orange cores in proportion
                // to their emitted green. The cooler crimson body is untouched.
                float orangeRed=rgb.b+2.18*(rgb.g-rgb.b)+0.066;
                rgb.r+=max(0.,orangeRed-rgb.r)*smoothstep(0.10,0.18,rgb.g);
            }
        }else{
            rgb=mix(reflection,core,cloud*ridge)*edge*gain*(0.19+0.32*cloud+0.19*ridge)*(1.-0.52*dust)*(1.-0.15*min(1.,u.surface.y))*(1.-0.30*float(u.surface.w==5.));
        }
        // Only the thin dust vein absorbs the background. All emission has
        // zero optical opacity, including the crimson body and its hot knots.
        absorb=min(0.045,gain*0.042)*edge*dustLane(n,fine);
    }else if(kind==2){
        float r=sqrt(rr);
        float core=exp(-0.5*square(r*detail.y));
        // Use the outward flank of a Gaussian for the skirt. Its greatest
        // slope is at the core, and its tail reaches zero inside the quad.
        float q=r*detail.x;
        float halo=exp(-q-0.5*q*q)*(1.-smoothstep(3.,3.8,q));
        int stop=u.surface.w==2. || u.surface.w==3. ? 4 : (u.surface.w==1. ? 5 : 2);
        rgb=temperature(1.)*min(0.94,0.92*gain)*core
            +u.palette[stop].rgb*min(0.108,0.090*gain)*halo;
    }
    else if(kind==3){
        float age=clamp(0.5+0.5*uv.x,0.,1.);
        float bend=age*age*0.56;
        float smoke=exp(-6.*square((uv.y-bend)/(0.11+0.86*age)));
        float ion=exp(-33.*square((uv.y+age*0.25)/(0.13+age*0.75)));
        rgb=gain*(gasCore()*smoke*0.54+temperature(0.9)*ion*0.59)*pow(1.-age,1.65);
    }else if(kind==4){
        float len=material.w;
        vec2 offset=capsuleOffset(uv,len);
        float r=length(offset);
        vec3 tint=temperature(detail.x);
        float white=u.surface.w==1.?0.55:(u.surface.w==2.?0.07:0.20);
        vec3 headColour=mix(tint,vec3(1.),white);
        vec2 atHead=uv-vec2(0.5*len,0.);
        if(detail.y>0.5){
            float approach=detail.w;
            float coreRadius=1./mix(4.2,5.4,approach);
            float mote=exp(-0.5*square(r/coreRadius))*shutterGain(coreRadius,len);
            float halo=max(0.,exp(-2.65*r*r)-0.071)*shutterGain(0.45,len);
            // Glint stays on the head and aligned with the windshield.
            vec2 axis=vec2(cos(material.x),sin(material.x));
            vec2 p=abs(vec2(dot(atHead,vec2(axis.x,-axis.y)),dot(atHead,axis.yx)));
            float glint=0.15*approach*exp(-32.*min(p.x,p.y))*exp(-0.75*max(p.x,p.y))
                *(1.-smoothstep(1.25,1.62,max(p.x,p.y)))*shutterGain(coreRadius,len);
            float distantGlint=(1.-approach)*min(0.15/max(gain,0.001),0.12)
                *exp(-28.*min(p.x,p.y)-3.*max(p.x,p.y));
            rgb=gain*(headColour*mote+tint*(mix(0.10,0.63,approach)*halo+distantGlint))+tint*glint;
            float peak=max(rgb.r,max(rgb.g,rgb.b));
            if(peak>0.68)rgb*=(0.68+0.13*(1.-exp(-(peak-0.68)/0.13)))/peak;
        }else{
            float mote=exp(-r*37.)*shutterGain(1./37.,len);
            float halo=max(0.,exp(-5.*r)-0.00674)*shutterGain(0.20,len);
            float glint=detail.z*0.12*exp(-28.*min(abs(atHead.x),abs(atHead.y))-5.*max(abs(atHead.x),abs(atHead.y)));
            rgb=gain*(headColour*mote+tint*(0.78*halo+glint));
        }
    }else if(kind==5){
        vec3 n=texture(noiseTile,uv*0.78+detail.x).rgb;
        vec3 fine=texture(noiseTile,uv*2.5+0.32*n.rg+detail.x).rgb;
        float radial=sqrt(rr)-(0.70+0.40*(n.r-0.5)+0.12*(fine.g-0.5));
        float shell=exp(-0.5*square(radial/0.14))*(1.-smoothstep(0.82,1.,rr));
        rgb=gain*mix(temperature(0.95),gasEdge(),smoothstep(-0.16,0.16,radial))*shell*(0.35+0.64*n.g);
    }else if(kind==7){
        float along=0.5+0.5*uv.x;
        float width=mix(0.40,1.,along);
        float filament=exp(-3.4*square(uv.y/width))*exp(-1.85*(1.-along));
        rgb=temperature(0.88)*gain*filament*smoothstep(0.,0.08,along);
    }else if(kind==9){ rgb=temperature(0.92)*gain*exp(-3.*rr); }
    else{float d=exp(-4.*uv.y*uv.y)*pow(max(0.,1.-uv.x*uv.x),1.7);
        rgb=temperature(0.82)*gain*d*(0.33+0.55*(0.5+0.5*uv.x));}
    return vec4(rgb,absorb);
}
