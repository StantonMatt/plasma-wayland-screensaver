#version 440
layout(location=0) in vec2 local;
layout(location=1) in vec4 color;
layout(location=2) in vec4 shape;
layout(location=3) in float detail;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; vec4 viewport; } ub;
void main() {
    float pixel=max(length(vec2(dFdx(local.x),dFdy(local.x))),0.00001);
    // Synchronized Bounce maps world circles nonuniformly. Use the radial
    // screen-space derivative for one-pixel coverage in every direction.
    if (shape.w>0.5) {
        float distance=length(local);
        pixel=max(length(vec2(dFdx(distance),dFdy(distance))),0.00001);
    }
    float coverage;
    if (shape.z<0.5 || shape.z==9.0) {
        coverage=clamp((shape.x-abs(local.y))/pixel+0.5,0.0,1.0)
            *clamp((shape.y-abs(local.x))/pixel+0.5,0.0,1.0);
        if (shape.z==9.0) {
            float d=length(local-vec2(shape.y,0.0));
            float nib=exp(-1.5*d*d)*detail;
            coverage=max(coverage,1.8*nib);
        }
    } else if (shape.z<1.5) {
        coverage=clamp((shape.x-length(local))/pixel+0.5,0.0,1.0);
    } else if (shape.z<2.5) {
        coverage=clamp(1.0-length(local)/shape.x,0.0,1.0);
    } else if (shape.z==6.0) {
        float r2=dot(local,local)/(shape.x*shape.x);
        coverage=max(0.0,(exp(-4.5*r2)-0.011109)/0.988891);
    } else if (shape.z==11.0 || shape.z==12.0) {
        vec2 ellipse=local/vec2(shape.y,shape.x);
        float r2=dot(ellipse,ellipse);
        if (shape.z==11.0) {
            coverage=max(0.0,(exp(-4.5*r2)-0.011109)/0.988891);
        } else {
            float start=floor(detail/128.0), sweep=mod(detail,128.0);
            float angle=degrees(atan(ellipse.y,ellipse.x));
            float fromMiddle=abs(mod(angle-start-0.5*sweep+180.0,360.0)-180.0);
            coverage=exp(-120.0*pow(sqrt(r2)-0.70,2.0))
                *(1.0-smoothstep(0.5*sweep-7.0,0.5*sweep,fromMiddle));
        }
    } else if (shape.z==7.0) {
        float t=clamp(0.5+0.5*local.x/max(shape.y,0.001),0.0,1.0);
        coverage=t*clamp((shape.x*t-abs(local.y))/pixel+0.5,0.0,1.0)
            *clamp((shape.y-abs(local.x))/pixel+0.5,0.0,1.0);
    } else if (shape.z==10.0) {
        float t=clamp(0.5+0.5*local.x/max(shape.y,0.001),0.0,1.0);
        float bend=2.0*shape.y*(85.0/245.0)*t*t;
        float distance=(local.y-bend)/(shape.x*mix(0.079,1.0,t));
        coverage=exp(-2.2*distance*distance)*(1.0-t)
            *clamp((shape.y-abs(local.x))/pixel+0.5,0.0,1.0);
    } else if (shape.z==8.0) {
        float distance=length(local)-shape.x;
        float lobes=1.0+0.25*cos(4.0*atan(local.y,local.x)+0.68*local.x/shape.x);
        coverage=lobes*(1.0-smoothstep(0.3,1.5+0.5*pixel,distance))
            *exp(-0.085*pow(min(0.0,distance+0.45),2.0));
    } else if (shape.z==13.0) {
        float u=smoothstep(0.0,0.25,detail);
        float along=local.x+shape.y;
        float head=u*2.0*shape.y;
        float arrival=0.25*acos(clamp(1.0-along/max(shape.y,0.001),-1.0,1.0))/3.141593;
        float wake=0.27*(1.0-smoothstep(0.0,0.4,detail-arrival))*step(along,head);
        float pulse=(1.0-smoothstep(9.0,19.0,abs(head-along)))*(1.0-smoothstep(0.25,0.35,detail));
        coverage=(pulse+wake)*(0.66*exp(-local.y*local.y)+0.34*exp(-0.13*local.y*local.y))
            *clamp((shape.y-abs(local.x))/pixel+0.5,0.0,1.0);
    } else {
        coverage=clamp((shape.x-length(local))/pixel+0.5,0.0,1.0);
    }
    vec3 rgb=color.rgb;
    float opacity=color.a;
    if (shape.z>2.5 && shape.z<3.5) {
        // Canvas two-circle radial gradient: focal radius 1, outer radius .56*size.
        vec2 focus=vec2(-.30,-.40)*shape.x;
        vec2 p=local-focus;
        float dr=1.12*shape.x-1.0;
        float a=dot(focus,focus)-dr*dr;
        float b=2.0*(dot(p,focus)-dr);
        float c=dot(p,p)-1.0;
        float t=clamp((-b-sqrt(max(0.0,b*b-4.0*a*c)))/(2.0*a),0.0,1.0);
        rgb=t<.22 ? mix(vec3(1),rgb,t/.22) : mix(rgb,vec3(19,27,69)/255.0,(t-.22)/.78);
    } else if (shape.z>3.5 && shape.z<4.5) {
        // QQuickPen.pixelAligned rounds after device-pixel-ratio scaling.
        float border=floor(max(1.0,shape.x*.05)/ub.info.w+0.5)*ub.info.w;
        float ring=clamp((length(local)-(shape.x-border))/pixel+0.5,0.0,1.0);
        // Rectangle's translucent border replaces the fill in the ring;
        // it does not paint white over an opaque palette-coloured body.
        opacity=mix(1.0,128.0/255.0,ring);
        rgb=mix(rgb,vec3(128.0/255.0),ring)/opacity;
    }
    float alpha=opacity*coverage*ub.info.x;
    fragColor=vec4(rgb*alpha,shape.w< -0.5 ? 0.0 : alpha);
}
