#version 440
layout(location=0) in vec2 local;
layout(location=1) in vec4 color;
layout(location=2) in vec4 shape;
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
    if (shape.z<0.5) {
        coverage=clamp((shape.x-abs(local.y))/pixel+0.5,0.0,1.0)
            *clamp((shape.y-abs(local.x))/pixel+0.5,0.0,1.0);
    } else if (shape.z<1.5) {
        coverage=clamp((shape.x-length(local))/pixel+0.5,0.0,1.0);
    } else if (shape.z<2.5) {
        coverage=clamp(1.0-length(local)/shape.x,0.0,1.0);
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
    fragColor=vec4(rgb*alpha,alpha);
}
