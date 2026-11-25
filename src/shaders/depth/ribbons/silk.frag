#version 440
layout(location=0) in vec4 shape;
layout(location=1) in vec4 surface;
layout(location=2) in vec3 moteColor;
layout(location=0) out vec4 fragColor;
layout(location=3) flat in vec4 front;
layout(location=4) flat in vec4 back;
layout(location=5) flat in vec4 haloColour;
layout(location=6) flat in vec4 accent;
layout(location=7) flat in float deviceScale;
layout(location=8) in vec3 headGeometry;
layout(location=9) flat in float shoulderGain;
void main() {
    vec3 light;
    if (surface.w>0.5) {
        float d=length(vec2(shape.x,surface.z));
        float weight=(1.-smoothstep(0.0,1.0,d));
        light=moteColor*weight*weight;
    } else {
        float d=abs(shape.x);
        float rad=shape.y;
        float face=abs(shape.z);
        float across=d/max(rad,0.01);
        float px=1./deviceScale;
        float soft=max(0.68*px,back.w<0.5 ? 1.45 : back.w>1.5 ? 0.19*rad : 0.40);
        float core=1.-smoothstep(max(rad-soft,0.),rad+soft,d);
        float atTip=0.;
        if (moteColor.y>0.) {
            // In the first couple of native pixels the face boundary faces
            // forward. Its antialiasing needs the normal distance to the
            // ellipse, including the subpixel support just ahead of the nose.
            float nose=max(moteColor.z,0.025*px);
            float signedDistance=(surface.z+d*d/(2.*nose))*inversesqrt(1.+d*d/(nose*nose));
            // Pixel coverage is linear across an oblique tangent. A cubic ramp
            // here can shimmer along a flat part of the rounded outline.
            // The entire face lies behind the tangent at its tip. Its signed
            // distance cannot be smaller than that of the enclosing half-plane.
            signedDistance=max(signedDistance,surface.z);
            float noseCore=clamp(0.5-signedDistance/(3.*px),0.,1.);
            atTip=1.-smoothstep(1.5*px,4.*px,-surface.z);
            core=mix(core,noseCore,atTip);

        }
        float outside=max(d-rad,0.);
        float halo=haloColour.w*0.35*exp(-4.7*outside/max(0.01,surface.x*haloColour.w*7.));
        if (moteColor.y>0.) {
            float radius=max(moteColor.x,0.001);
            float setback=smoothstep(2.*px,8.*px,-surface.z);
            // Pixel footprints straddling either axis share the cap maximum.
            // This keeps quantized bilinear samples ordered even on a ray
            // whose minor coordinate changes by a tiny fraction of a pixel.
            vec2 disk=max(abs(headGeometry.xy)-vec2(px),vec2(0.));
            float distance=mix(length(disk),max(d-rad,0.),setback);
            // Use the same optical falloff as the transverse halo. The
            // geometric envelope is wider than its photometric radius.
            float falloff=exp(-4.7*distance/radius);
            // A tight bend can have less transverse support than its nominal
            // glow. Reach zero smoothly at the actual triangle boundary.
            float room=1.-d/max(headGeometry.z,0.001);
            float rounded=haloColour.w*0.35*min(falloff,smoothstep(0.,0.48,room));
            halo=mix(halo,rounded,moteColor.y);
        }
        float layerGain=back.w<0.5 ? 0.66 : 1.;
        float gain=1.+0.5*shape.w;
        if (moteColor.y>0. && surface.y>0.) {
            // Carry the shoulder's radiance around the end, including its
            // current pulse. The face-only tip highlight does not light the halo.
            float peak=dot(haloColour.rgb,vec3(0.2126,0.7152,0.0722))
                *haloColour.w*0.35*front.w*layerGain;
            // Bound the native one-pixel slope of narrow, bright shoulders.
            // The cap and both flanks use this same peak and exponential.
            float resolved=2.15*deviceScale*moteColor.x/(255.*4.7*max(peak,0.00001));
            gain=mix(gain,min(shoulderGain,resolved)/surface.y,moteColor.y);
        }
        halo*=gain*(1.-core)*layerGain;
        vec3 col=mix(back.rgb,front.rgb,smoothstep(-0.17,0.17,shape.z));
        float rim=smoothstep(0.48,0.94,across);
        float threads=smoothstep(10.,19.,2.*rad*deviceScale)*smoothstep(0.70,0.93,face)*float(back.w>0.5)*(back.w>1.5 ? 0.5 : 1.);
        // Fine lengthwise warp and weft catch light along the two selvedges.
        float silk=0.72+0.22*rim+0.042*threads*cos(19.5*shape.x/max(rad,0.01));
        silk=mix(silk,0.90,atTip);
        rim=mix(rim,1.,atTip);
        float edge=1.-smoothstep(0.,0.44,face);
        col=mix(col,accent.rgb,0.16*rim+0.24*edge+0.53*shape.w);
        float concentrated=(0.62+0.38*face)*(1.+0.85*edge);
        vec3 glow=haloColour.rgb;
        light=(col*core*silk*concentrated*(1.+1.20*shape.w)+glow*halo)*front.w;
        if (accent.w == -1.) {
            // Keep isolated Pastel cores bright and coloured. Only the high
            // radiance contributed by highlights approaches the 0.8 knee.
            float peak=max(light.r,max(light.g,light.b));
            float knee=0.8+0.2*(1.-exp(-max(peak-0.8,0.)/0.2));
            light*=min(1.,knee/max(peak,0.0001));
        }
    }
    float alpha=surface.y;
    vec3 emitted=light*alpha;
    float peak=max(emitted.r,max(emitted.g,emitted.b));
    if (peak>.92) {
        float excess=peak-.92;
        float shoulder=.92+.08*excess/(.08+excess);
        emitted*=shoulder/peak;
        peak=shoulder;
    }
    // RGB remains the isolated emission. Palette-owned opacity lets stacked
    // Ember and Ocean light build toward their hot cores. Pastel and motes keep
    // full peak attenuation; no target or extra pass is needed.
    float attenuation=surface.w>0.5 || accent.w<0. ? 1. : accent.w;
    fragColor=vec4(emitted,peak*attenuation);
}
