#version 440
layout(location=0) in vec2 uv;
layout(location=1) in vec4 pigment;
layout(location=2) in vec4 surface;
layout(location=3) in vec4 light;
layout(location=4) in vec4 coating;
layout(location=5) in vec2 cabinetPosition;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform buf {
    mat4 matrix; vec4 viewport; vec4 mapping; vec4 ranges; vec4 top; vec4 bottom; vec4 cabinet;
} ub;
layout(binding=2) uniform sampler2D knots;
vec4 trailKnot(float i,float component) {
    return texture(knots,vec2((2.*mod(coating.x-i,32.)+component+.5)/64.,(coating.z+.5)/61.));
}
vec2 trailTangent(vec2 velocity,vec2 chord,float dt) {
    if(dot(velocity,chord)<=0.) return chord;
    return velocity*min(dt,1.25*length(chord)/max(length(velocity),.001));
}
float curveParameter(vec2 point,vec2 start,vec2 quadratic,vec2 cubic) {
    // Find the closest of four chord subdivisions, then project onto the
    // cubic with its analytic tangent. Endpoints retain round coverage.
    float best=1e30,t=0.;
    vec2 previous=vec2(0.);
    for(int j=1;j<=4;++j){
        float u=float(j)*.25;
        vec2 next=u*(start+u*(quadratic+u*cubic));
        vec2 span=next-previous;
        float along=clamp(dot(point-previous,span)/max(dot(span,span),1e-8),0.,1.);
        vec2 difference=previous+along*span-point;
        float d=dot(difference,difference);
        if(d<best){best=d;t=.25*(float(j-1)+along);}
        previous=next;
    }
    for(int j=0;j<2;++j){
        vec2 difference=t*(start+t*(quadratic+t*cubic))-point;
        vec2 tangent=start+t*(2.*quadratic+3.*t*cubic);
        float slope=dot(tangent,tangent)+dot(difference,2.*quadratic+6.*t*cubic);
        if(slope>1e-6) t=clamp(t-clamp(dot(difference,tangent)/slope,-.25,.25),0.,1.);
    }
    return t;
}
vec3 trailSection(vec3 foot,vec3 hue,vec3 hot,float radius) {
    if(foot.x>=radius*radius) return vec3(0.);
    float r=surface.z;
    float along=max(0.,foot.y-r)/max(.1*r,uv.y-r);
    float fade=max(0.,1.-along);
    float coreEnvelope=smoothstep(.4,.75,fade);
    float longArc=smoothstep(4.,8.,uv.y/r);
    // Long, fast arcs carry their colour farther across the room. Their
    // narrower body still resolves both shoulders of the light at 0.25r.
    float exposure=exp(-mix(2.35,2.05,longArc)*along*along);
    float alpha=exposure*(1.-smoothstep(.84*surface.w,surface.w,foot.z))
        *smoothstep(.35*r,.9*r,foot.y);
    float sigma=r*mix(.270,mix(.288,.276,longArc),sqrt(fade));
    // Let the final few display levels spread into the body before the
    // coloured channels quantize to black at different distances.
    sigma=mix(.39*r,sigma,smoothstep(.012,.075,alpha));
    float body=.70*exp(-.5*foot.x/(sigma*sigma))*(1.-smoothstep(.78*radius,radius,sqrt(foot.x)));
    float coreSigma=max(1.3/ub.mapping.z,.105*r);
    float core=mix(.050,.105,longArc)*exp(-.5*foot.x/(coreSigma*coreSigma))*alpha*coreEnvelope;
    return alpha*(hue*body+hot*core);
}
vec3 trailLight(vec3 dye) {
    float r=surface.z;
    float radius=(ub.ranges.w+.47)*r;
    vec3 hue=dye/max(.05,max(dye.r,max(dye.g,dye.b)));
    vec3 hot=mix(hue,vec3(1.),.35);
    vec4 young=vec4(surface.xy,coating.w,light.x);
    vec4 motion=vec4(light.yz,uv.x,0.);
    vec3 result=vec3(0.);
    for(int i=0;i<32;++i){
        if(float(i)>=coating.y) break;
        vec4 old=trailKnot(float(i),0.);
        vec4 prior=trailKnot(float(i),1.);
        vec2 point=cabinetPosition-old.xy;
        if(old.w<0.){
            float life=clamp(1.-(coating.w-old.z)/surface.w,0.,1.);
            float d=length(point)/(.36*r);
            float dotLight=.27*(-old.w)*life*exp(-2.6*d*d)*(1.-smoothstep(.74,1.,d));
            result=max(result,hot*dotLight);
        }
        vec2 chord=young.xy-old.xy;
        float dt=max(0.,young.z-old.z);
        vec2 start=trailTangent(prior.xy,chord,dt);
        vec2 end=young.w<0. ? chord : trailTangent(motion.xy,chord,dt);
        vec2 lo=min(vec2(0.),min(chord,min(start/3.,chord-end/3.)))-radius;
        vec2 hi=max(vec2(0.),max(chord,max(start/3.,chord-end/3.)))+radius;
        if(dot(chord,chord)>.0001 && all(greaterThanEqual(point,lo)) && all(lessThanEqual(point,hi))){
            vec2 quadratic=3.*chord-2.*start-end;
            vec2 cubic=start+end-2.*chord;
            float t=curveParameter(point,start,quadratic,cubic);
            vec2 delta=point-t*(start+t*(quadratic+t*cubic));
            float distance2=dot(delta,delta);
            if(distance2<radius*radius) {
                // Continue the exposure along the analytic tangent past a
                // sample. The capsule itself keeps its round end. Near a
                // tight reversal the short chords naturally become round dots.
                vec2 direction=start+t*(2.*quadratic+3.*t*cubic);
                float chordLength=length(chord);
                float extension=dot(delta,direction)/max(.001,length(direction))
                    *smoothstep(.06*r,.21*r,chordLength);
                float arc=mix(mod(uv.x-prior.z,65536.),mod(uv.x-motion.z,65536.),t);
                float age=coating.w-mix(old.z,young.z,t)-extension*dt/chordLength;
                result=max(result,trailSection(vec3(distance2,arc-extension,age),hue,hot,radius));
            }
        }
        young=old;motion=prior;
    }
    return result*smoothstep(.72,1.,length(cabinetPosition-surface.xy)/r)*smoothstep(.85*r,1.6*r,uv.y);
}


vec3 linearRGB(vec3 colour) {
    return mix(colour/12.92,pow((colour+.055)/1.055,vec3(2.4)),greaterThan(colour,vec3(.04045)));
}
vec3 displayRGB(vec3 colour) {
    return mix(12.92*colour,1.055*pow(colour,vec3(1./2.4))-.055,greaterThan(colour,vec3(.0031308)));
}
void main(){
    if(ub.mapping.w>.5 && light.w!=4. && (any(lessThan(cabinetPosition,ub.cabinet.xy))
            || any(greaterThan(cabinetPosition,ub.cabinet.zw)))) discard;
    float alpha=pigment.a*ub.viewport.w;
    float rr=dot(uv,uv);
    vec3 dye=pigment.rgb;
    if(light.w>.5){
        vec3 colour=dye;
        float intensity;
        if(light.w<1.5){
            float radius=sqrt(rr)*1.8;
            float envelope=clamp((1.8-radius)/.8,0.,1.);
            // The halo remains outside the silhouette; clear glass retains
            // just a little transmitted light through its centre.
            colour=dye/max(.05,max(dye.r,max(dye.g,dye.b)));
            intensity=envelope*envelope*mix(.16,1.,smoothstep(.72,1.,radius));
            float pixels=abs(radius-(1.+.25*light.y))*surface.w/1.8;
            float ring=(1.-smoothstep(.55,1.55,pixels))*(1.-light.y)*light.x;
            fragColor=vec4(colour*alpha*intensity+mix(colour,vec3(1),.30)*.4*ring*ub.viewport.w,0.);
            return;
        }else if(light.w<2.5){
            intensity=exp(-2.5*uv.x*uv.x-3.125*uv.y*uv.y)*(1.-smoothstep(.8,1.,abs(uv.x)));
        }else if(light.w<3.5){
            fragColor=vec4(0,0,0,alpha*max(0.,exp(-2.6*rr)-.075));return;
        }else if(light.w<4.5){
            float d=abs(uv.y)*surface.w;
            intensity=.15*exp(-d*d/32.)+.35*(1.-smoothstep(.2,.85,d));
        }else if(light.w<5.5){
            fragColor=vec4(trailLight(dye)*alpha,0.);return;
        }else if(light.w<6.5){
            intensity=exp(-2.8*rr)*(1.-smoothstep(.7,1.,rr));
            colour=mix(dye,vec3(1),.35);
        }else if(light.w<7.5){
            intensity=exp(-2.*rr)*(1.-smoothstep(.6,1.,rr));
        }else{
            intensity=exp(-3.1*uv.y*uv.y);
        }
        fragColor=vec4(colour*alpha*intensity,0.);return;
    }
    float edge=sqrt(rr);
    float coverage=clamp((1.-edge)/max(fwidth(edge),.0001)+.5,0.,1.);
    vec3 n=vec3(uv,sqrt(max(0.,1.-rr)));
    const vec3 key=vec3(-.451,-.601,.66);
    const vec3 halfVector=vec3(-.248,-.330,.911);
    float lambert=dot(n,key);
    float shine=max(0.,dot(n,halfVector));
    float fresnel=pow(1.-n.z,2.2);
    vec3 pearl=light.z==4. ? vec3(1.) : vec3(1.,.975,.933);
    float wrap=clamp((lambert+.54)/1.54,0.,1.);
    vec3 emission=light.x*dye*(.35+.65*max(0.,1.-rr));
    vec2 turn=vec2(surface.z*uv.x+surface.y*uv.y,-surface.y*uv.x+surface.z*uv.y);
    vec3 rgb;
    vec3 impact=vec3(0.);
    float boost=max(0.,-light.w);
    float coreShape=.35+.65*max(0.,1.-rr);
    vec3 bodyHue=dye;
    if(surface.x<.5){
        float band=1.-smoothstep(.12,.18,abs(.93*turn.y+.36*n.z));
        vec3 enamel=mix(coating.rgb*.85,mix(coating.rgb,pearl,.50),band*.94);
        vec3 satin=enamel*(.13+.65*wrap)+ub.bottom.rgb*.25+dye*.17*fresnel;
        // Emission passes through the satin and inlay. Use the remaining
        // channel headroom so a white impact retains the stripe and curvature.
        rgb=satin+emission*(1.-clamp(satin,0.,1.))*(.65+.25*band)
            +pearl*(.20*pow(shine,light.z==5. ? 10. : 18.)+.25*pow(shine,54.));
        impact=boost*dye*coreShape*(1.-clamp(satin,0.,1.))*(.65+.25*band);
    }else if(surface.x<1.5){
        float twist=mix(.6,1.4,fract(coating.w*.618034));
        float phase=6.283185*fract(coating.w*.414214);
        // In projected sphere coordinates curvature is bounded by A*w^2.
        // Parallel vanes cannot cross into a sharp W near the silhouette.
        float amplitude=mix(.12,.20,fract(coating.w*.732051));
        float bend=amplitude*sin(3.141593*twist*turn.x+phase);
        float clearKey=smoothstep(.25,.42,length(uv-halfVector.xy));
        float span=1.-smoothstep(.38,.94,abs(turn.x));
        float vein=exp(-48.*pow(turn.y-bend,2.))*span*clearKey;
        float second=.60*exp(-70.*pow(turn.y-bend-.22,2.))*span*clearKey;
        float caustic=exp(-100.*pow(length(uv-vec2(-.12,-.14))-.90,2.))
            *smoothstep(-.12,.85,dot(uv,vec2(.60,.80)));
        bool forest=light.z==3.;
        bool ember=light.z==2.;
        if(forest) caustic*=.5;
        vec3 tint=forest || ember ? dye : coating.rgb;
        bodyHue=tint;
        float rim=pow(1.-n.z,forest ? 1.25 : 1.7);
        if(forest) rim=max(rim,.38*(1.-smoothstep(0.,3.,(1.-edge)*surface.w)));
        alpha*=clamp((forest ? .5 : (ember ? .4 : .31))+.58*rim+.28*(vein+second)
                      +.41*caustic+.52*pow(shine,65.),0.,1.);
        rgb=tint*(.22+.30*n.z+.38*(vein+second)+1.5*coreShape*light.x)
            +pearl*(.50*pow(shine,95.)+.11*pow(shine,18.)+.34*caustic)
            +mix(tint,pearl,.47)*rim*.78;
        rgb+=forest ? .25*tint*wrap : (ember ? vec3(0.) : ub.bottom.rgb*.25);
        impact=tint*boost*(.82+.34*n.z)+mix(tint,vec3(1.),.22)*rim*.27*light.y;
        // Preserve the hue of the glass when a channel reaches display white.
        vec3 lit=rgb+impact;
        rgb/=max(1.,max(rgb.r,max(rgb.g,rgb.b)));
        impact=lit/max(1.,max(lit.r,max(lit.g,lit.b)))-rgb;
    }else{
        vec3 f0=light.z==1. ? .8*dye : coating.rgb;
        bodyHue=f0;
        // A camera pitched twelve degrees down sees the floor wrap around
        // the sides of the sphere. The lighting environment is view-fixed.
        vec3 reflected=reflect(vec3(0.,.207912,-.978148),n);
        float h=reflected.y;
        float room=dot(ub.top.rgb+.35*ub.bottom.rgb,vec3(.2126,.7152,.0722));
        float sky=.44+.52*pow(clamp(1.+h,0.,1.),1.35)+.20*room;
        float ground=.12+.25*exp(-3.4*max(0.,h))+.08*room;
        float floorReturn=pow(max(0.,dot(n,vec3(.47,.60,.647))),23.);
        float horizon=smoothstep(-max(.008,fwidth(h)),max(.008,fwidth(h)),h);
        float env=mix(sky,ground,horizon)+.10*floorReturn;
        float window=exp(-pow(abs((reflected.x+.54)/.17),4.)-pow(abs((h+.47)/.25),4.));
        vec3 fresnelMetal=f0+(1.-f0)*pow(1.-n.z,5.);
        float rim=1.-smoothstep(.3,1.4,(1.-edge)*surface.w);
        rgb=mix(fresnelMetal*(env+.23*window),.90*f0,rim)
            +pearl*(.94*pow(shine,135.)+.08*pow(shine,20.));
        impact=(.22*f0*exp(-35.*abs(h))*boost)
            +vec3(1.)*light.y*(.48*pow(shine,28.)+.45*pow(1.-n.z,1.8)+.25*rim);
    }
    vec3 base=clamp(rgb,0.,1.);
    vec3 lit=clamp(rgb+impact,0.,1.);
    float chroma=bodyHue.r-min(bodyHue.g,bodyHue.b);
    if(bodyHue.r>max(bodyHue.g,bodyHue.b) && abs(bodyHue.g-bodyHue.b)<=chroma*(25./60.)
       && (boost>0. || light.y>0.)){
        vec3 quietLinear=linearRGB(floor(base*255.+.5)/255.);
        vec3 delta=linearRGB(floor(lit*255.+.5)/255.)-quietLinear;
        float stepY=dot(delta,vec3(.2126,.7152,.0722));
        if(stepY>.05) lit=floor(255.*displayRGB(quietLinear+delta*(.05/stepY)))/255.;
    }
    alpha*=coverage;
    fragColor=vec4(lit*alpha,alpha);
}
