// SPDX-License-Identifier: GPL-3.0-or-later
uint mixBits(uint v){v^=v>>16;v*=0x7feb352du;v^=v>>15;v*=0x846ca68bu;return v^(v>>16);}
float randomUnit(uint i){return float(mixBits(i)>>8)*(1./16777216.);}
void skyVertex(uint id, out vec2 uv, out vec2 screenPoint, out vec4 material, out vec4 detail){
    uint index=id<u.counts.y?id:u.counts.z-1u;
    Sprite s=skySprite(int(index));uv=corner(gl_VertexIndex);material=s.material;detail=s.detail;
    vec2 p=s.shape.xy,r=s.shape.zw;float theta=material.x;
    if(int(material.y)==6){
        uint j=id<u.counts.y?0u:id-u.counts.y+1u;uint seed=uint(detail.x*68783.)+j*73u;
        float age=fract(randomUnit(seed+1u)+detail.y*(0.17+0.12*randomUnit(seed+2u)));
        vec2 edge=vec2((j%2u==0u?1.:-1.)*(0.73+0.22*randomUnit(seed)),1.85*(randomUnit(seed+3u)-0.5));
        vec2 heading=edge*detail.zw;
        theta=atan(heading.y,heading.x);
        float distance=max(0.65*length(detail.zw),length(heading)*(0.82+0.29*age));
        p+=normalize(heading)*distance;
        r=vec2((0.045+0.057*age)*u.viewport.z,(1.+1.25*age)*u.viewport.w);
        if(j%11u==0u){
            r=vec2(2.2+1.2*age)*u.viewport.w;
            material.y=9.;
            material.z*=0.65+0.30*cos(6.2831853*(1.1+0.48*randomUnit(seed))*detail.y);
        }else{
            material.z*=1.35;
        }
        material.z*=smoothstep(0.,0.13,age)*(1.-smoothstep(0.86,1.,age));
    }
    vec2 axis=vec2(cos(theta),sin(theta));
    float starRadius=r.y;
    if(int(material.y)==4){
        p-=axis*(0.5*material.w);
        float glintReach=detail.y>0.5 ? 0.65*detail.w : 0.;
        r=vec2(starRadius*(1.+glintReach))+vec2(0.5*material.w,0.);
    }
    screenPoint=p+axis*uv.x*r.x+vec2(-axis.y,axis.x)*uv.y*r.y;
    if(int(material.y)==4){
        uv*=r/starRadius;
        material.w/=starRadius;
    }
    gl_Position=u.matrix*vec4(screenPoint,0.,1.);
}
