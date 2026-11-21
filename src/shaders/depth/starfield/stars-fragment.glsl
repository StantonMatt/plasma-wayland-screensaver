// SPDX-License-Identifier: GPL-3.0-or-later
float coreProfile(float distance,float radius,float px){
    // At least one physical sample sees the full head as its centre crosses
    // the pixel grid. Smaller apparent stars still fade by area in the vertex stage.
    float inner=max(radius*0.40,0.708*px);
    float edge=clamp((distance-inner)/max(radius+0.65*px-inner,0.45*px),0.,1.);
    return 0.5+0.5*cos(3.14159265*edge);
}
vec4 starLight(vec2 local, vec4 shape, vec4 emission, vec3 light){
    float radius=shape.x,len=shape.y,px=shape.w;
    vec2 atHead=local-vec2(0.5*len,0.);
    float along=clamp(1.+atHead.x/max(len,0.001),0.,1.);
    float dist=length(capsuleOffset(local,len));
    float core=coreProfile(dist,radius*mix(0.4,1.,along),px);
    float head=coreProfile(length(atHead),radius,px);
    float conserve=shutterGain(radius,len);
    float trail=core*exp(-1.65*(1.-along))*conserve;
    float halo=0.;
    if(shape.z>0.05){
        float r=dist/shape.z;
        float exposure=shutterGain(0.35*shape.z,len);
        float taper=mix(1.,0.35+0.65*along,smoothstep(0.5,2.,len/shape.z));
        halo=0.28*max(0.,exp(-5.*r*r)-0.00674)*exposure*taper;
        if(light.x>0.5){
            vec2 p=abs(atHead)/max(radius,0.01);
            halo+=0.12*exp(-0.7*min(p.x,p.y)*min(p.x,p.y)-0.44*max(p.x,p.y));
        }
    }
    vec3 coreColor=mix(emission.rgb,vec3(1.),light.y==1.?0.55:(light.y==2.?0.07:0.20));
    float point=max(head*(0.95+0.05*conserve),trail);
    vec3 rgb=coreColor*point+emission.rgb*(halo+0.62*trail*(1.-head));
    rgb*=emission.a*(1.-0.08*float(light.y==5.));
    if(light.y==2. && light.z>0.){
        // Midnight and Deep Ocean add blue behind a translucent point.
        // Keep resolved ember cores orange across both background gradients.
        float warm=smoothstep(0.025,0.16,rgb.r)*head;
        rgb.b=max(0.,rgb.b-0.075*warm);
        rgb.g=max(rgb.g,rgb.b+0.125*warm);
        rgb.r+=0.055*warm;
    }
    return vec4(rgb,0.);
}
