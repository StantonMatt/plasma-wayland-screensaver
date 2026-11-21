// SPDX-License-Identifier: GPL-3.0-or-later
vec3 viewRay(vec3 p, vec4 rotation, float warp, vec3 axis){
    vec3 q=rotateEye(p,rotation);
    float beta=aberration*warp,k=sqrt(1.-beta*beta);
    return q*k+axis*((1.-k)*dot(q,axis)+beta*length(q));
}
vec2 projectRay(vec3 q,out float facing){
    facing=q.z;
    if(u.projection.w>0.5){
        facing=length(q.xz);
        return u.projection.yz+u.projection.x*vec2(atan(q.x,q.z),-q.y/max(0.0001,facing));
    }
    return u.projection.yz+u.projection.x*q.xy*vec2(1.,-1.)/max(q.z,0.01);
}
void starVertex(int idInBuffer, out vec2 local, out vec4 shape, out vec4 emission, out vec3 light){
    int group=idInBuffer>>7;int lane=idInBuffer&127;
    uint pair=uint(u.groups[group>>3][(group>>1)&3]);
    uint code=(pair>>uint(16*(group&1)))&65535u;
    int n=4*(int(code>>11)+1);int id=int(code&2047u);
    int row=id>>3;int col=2*((id&7)*128+lane);
    gl_Position=vec4(2.,2.,0.,1.); local=vec2(0.);shape=vec4(1.);emission=vec4(0.);light=vec3(0.);
    if(lane>=n)return;
    vec4 star=texelFetch(stars,ivec2(col,row),0);
    if(star.w<=0.)return;
    vec4 eye=u.eye[row/125];int cell=row%125;
    vec3 grid=vec3(cell%5,(cell/5)%5,cell/25);
    vec3 dp=mod(grid+star.xyz-eye.xyz+2.5,5.)-2.5;
    float d2=dot(dp,dp);
    if(d2>=4. || d2<0.00001)return;
    float d=sqrt(d2);vec3 p=dp*eye.w;
    vec3 ray=viewRay(p,u.rotation,u.optics.x,u.axis.xyz);
    float z;vec2 head=projectRay(ray,z);
    if(z<=0.001)return;
    // Project the physical shutter with the current lens factor at both ends.
    // A changing lens during punch must never reverse the exposure direction.
    float tailDepth;
    vec3 previous=viewRay(p+u.shift.xyz,u.oldRotation,u.optics.x,u.oldAxis.xyz);
    vec2 naturalTail=projectRay(previous,tailDepth);
    vec2 natural=tailDepth>0.001 ? head-naturalTail : vec2(0.);
    natural*=min(1.,0.36*u.viewport.z/max(1.,length(natural)));
    vec2 radial=head-u.field.zw;
    float r=length(radial)/u.viewport.z;
    float exposure=u.axis.w;
    // Forward translation differentiates the aberrated ray along the same
    // axis, with a positive scalar that cancels on normalisation. Differentiate
    // the planar or cylindrical projection directly, in logical pixel axes.
    vec3 forward=u.axis.xyz;
    vec2 flow=vec2(ray.x*forward.z-ray.z*forward.x,
                   ray.z*forward.y-ray.y*forward.z);
    if(u.projection.w>0.5){
        float horizon=max(0.0001,length(ray.xz));
        flow.y=forward.y*horizon-ray.y*dot(ray.xz,forward.xz)/horizon;
    }
    vec2 outward=flow/max(length(flow),0.0001);
    float reachRatio=min(1.,1.19*r)*smoothstep(0.018,0.082,r);
    float opticalLength=u.viewport.z*u.oldAxis.w*exposure*reachRatio;
    vec2 diff=mix(natural,outward*opticalLength,smoothstep(0.,0.08,abs(exposure)));
    vec2 tail=head-diff;
    float len=length(diff);
    vec2 guard=vec2(0.04*u.viewport.z);
    if(any(lessThan(max(head,tail),-guard)) || any(greaterThan(min(head,tail),u.viewport.xy+guard)))return;
    vec2 axis=len>0.0001?diff/len:vec2(1.,0.);
    vec4 info=texelFetch(stars,ivec2(col+1,row),0);
    // Hand the resident star to the landmark sprite over its actual lifetime.
    // On a leg boundary both the old and new resident stars are fully visible.
    float selected=info.y>0.5 && distance(p,u.destination.xyz)<0.00005?u.surface.z:0.;
    if(selected>=1.)return;
    float fade=(1.-smoothstep(1.58,2.,d))*smoothstep(0.003,mix(0.065,0.015,min(abs(exposure),1.)),length(p));
    fade*=1.-selected;
    if(row>=125) fade*=smoothstep(0.66,0.90,d);
    float b=star.w*(0.63+0.67/(0.43+d2));
    float pulse=1.+float(info.z>0.)*0.14*sin(u.optics.y*info.z+info.w);
    float px=u.viewport.w;
    float radius=u.optics.z*(0.40+1.62*sqrt(b))*(1.+0.13*float(u.surface.w==5.));
    float halo=u.optics.z*(3.6+10.*min(b,1.5))*smoothstep(0.68,1.0,star.w);
    float energy=(0.29+0.80*sqrt(min(1.7,b)))*fade*pulse;
    energy*=mix(1.+0.1*min(1.,u.surface.y),1.,smoothstep(0.1,0.6,b));
    energy*=min(1.,radius/max(px*0.72,0.001));
    radius=max(radius,px*0.72);
    float shift=u.field.y*(0.25-0.5*smoothstep(0.20,0.50,r));
    float hue=clamp(info.x+shift,0.,1.);
    vec3 tint=temperature(hue);
    if(u.surface.w==2.){
        // Ember uses its orange-to-gold centre for the common population.
        // Reserve crimson for faint distant suns and pale gold for giants.
        float variation=fract(info.w/6.2831853);
        tint=mix(u.palette[2].rgb,u.palette[4].rgb,clamp(hue,0.,1.));
        if(variation<0.25) tint=mix(u.palette[1].rgb,u.palette[2].rgb,0.22+0.5*hue);
        if(star.w>0.34) tint=mix(u.palette[4].rgb,u.palette[3].rgb,clamp(0.18+star.w*0.72+shift,0.,1.));
        if(row>=125 && star.w<0.04 && variation<0.10) {
            tint=mix(u.palette[0].rgb,u.palette[1].rgb,hue);
            energy=min(0.5,energy);
        }
    }
    if(info.y>1.5 && distance(p,u.destination.xyz)<0.035){
        float approach=u.destination.w;
        halo=mix(halo,u.optics.z*(8.+6.*fract(info.w)),approach);
        energy*=1.+0.78*approach;
    }
    float bowSaturation=0.2*u.field.y*smoothstep(0.18,0.23,r)*(1.-smoothstep(0.33,0.37,r))*smoothstep(1.,8.,len);
    tint=clamp(mix(vec3(dot(tint,vec3(0.2126,0.7152,0.0722))),tint,1.+bowSaturation),0.,1.);
    energy*=1.+0.35*float(row<125);
    float reach=max(radius+px,halo+px);
    local=corner(gl_VertexIndex)*vec2(0.5*len+reach,reach);
    gl_Position=u.matrix*vec4(0.5*(head+tail)+axis*local.x+vec2(-axis.y,axis.x)*local.y,0.,1.);
    shape=vec4(radius,len,halo,px);emission=vec4(tint,energy*u.opacity.x);
    light=vec3(info.y,u.surface.w,u.surface.y);
}
