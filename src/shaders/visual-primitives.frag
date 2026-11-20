#version 440
layout(location=0) in vec2 local;
layout(location=1) in vec4 color;
layout(location=2) in vec4 shape;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; vec4 viewport; } ub;
void main() {
    float pixel=max(length(vec2(dFdx(local.x),dFdy(local.x))),0.00001);
    float coverage;
    if (shape.z<0.5) {
        coverage=clamp((shape.x-abs(local.y))/pixel+0.5,0.0,1.0)
            *clamp((shape.y-abs(local.x))/pixel+0.5,0.0,1.0);
    } else if (shape.z<1.5) {
        coverage=clamp((shape.x-length(local))/pixel+0.5,0.0,1.0);
    } else {
        coverage=clamp(1.0-length(local)/shape.x,0.0,1.0);
    }
    float alpha=color.a*coverage*ub.info.x;
    fragColor=vec4(color.rgb*alpha,alpha);
}
