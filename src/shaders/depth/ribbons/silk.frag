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
layout(location=10) in vec3 glowFromTip;
// The integer half-plane excludes every saturated-red RGB8 colour and is
// closed under clamped additive updates. Quantise before the 8-bit blend.
vec3 guardedEmberHalo(vec3 radiance, bool balanced) {
    uvec3 rgb=uvec3(floor(clamp(radiance,vec3(0.),vec3(1.))*255.+0.5));
    uint allowed=(balanced ? (1000u*rgb.g+1000u*rgb.b)/777u : (1000u*rgb.g+817u*rgb.b)/709u);
    // Usually only one red byte needs removing. On the few coarser low
    // levels, lift G/B together instead of losing two red bytes.
    if (allowed+1u<rgb.r) {
        rgb.gb=min(rgb.gb+uvec2(1u),uvec2(255u));
        allowed=(balanced ? (1000u*rgb.g+1000u*rgb.b)/777u : (1000u*rgb.g+817u*rgb.b)/709u);
    }
    rgb.r=min(rgb.r,allowed);
    return vec3(rgb)/255.;
}
void main() {
    vec3 light;
    vec3 emittedHalo=vec3(0.);
    float weight;
    float glowFade=1.;
    if (surface.w>0.5) {
        float d=length(vec2(shape.x,surface.z));
        weight=(1.-smoothstep(0.0,1.0,d));
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
        if (accent.w < -1.5) {
            // Let the dim tail finish at exact zero inside its mesh support.
            float room=max(0.,1.-d/max(headGeometry.z,0.001));
            halo=min(halo,haloColour.w*0.35*smoothstep(0.,0.16,room));
        }
        float layerGain=back.w<0.5 ? 0.66 : 1.;
        float headMix=moteColor.y;
        float haloScale=1.;
        if (accent.w < -1.5 && moteColor.x>0.) {
            // Keep the accepted head/shoulder luminance while the distant
            // atmosphere retains its red amplitude. Cover the complete head
            // neighbourhood, then return smoothly to the body glow.
            float neck=1.-smoothstep(1.35*moteColor.x,1.8*moteColor.x,
                                    length(headGeometry.xy));
            headMix=max(headMix,neck);
            float actual=dot(haloColour.rgb,vec3(0.2126,0.7152,0.0722));
            float previous=0.2126*haloColour.r;
            haloScale=mix(1.,previous/max(actual,0.00001),headMix);
        }
        float gain=1.+0.5*shape.w;
        if (headMix>0. && surface.y>0.) {
            // Carry the shoulder's radiance around the end, including its
            // current pulse. The face-only tip highlight does not light the halo.
            float peak=dot(haloColour.rgb,vec3(0.2126,0.7152,0.0722))
                *haloColour.w*0.35*front.w*layerGain*haloScale;
            // Bound the native one-pixel slope of narrow, bright shoulders.
            // The cap and both flanks use this same peak and exponential.
            float resolved=2.15*deviceScale*moteColor.x/(255.*4.7*max(peak,0.00001));
            gain=mix(gain,min(shoulderGain,resolved)/surface.y,headMix);
        }
        // Keep the original head computation as an upper bound. Extend its
        // dim radiance over 1.65 glow radii, including untagged body spans.
        if (accent.w < -2.5 && glowFromTip.z>0.) {
            float fade=1.-smoothstep(1.35*glowFromTip.z,3.*glowFromTip.z,
                                    length(glowFromTip.xy));
            float extendedMix=max(headMix,fade);
            float actual=dot(haloColour.rgb,vec3(0.2126,0.7152,0.0722));
            float extendedScale=mix(1.,0.2126*haloColour.r/max(actual,0.00001),extendedMix);
            float extendedGain=1.+0.5*shape.w;
            if (extendedMix>0. && surface.y>0.) {
                float peak=actual*haloColour.w*0.35*front.w*layerGain*extendedScale;
                float resolved=2.15*deviceScale*glowFromTip.z/(255.*4.7*max(peak,0.00001));
                extendedGain=mix(extendedGain,min(shoulderGain,resolved)/surface.y,extendedMix);
            }
            glowFade=min(1.,extendedGain*extendedScale/max(gain*haloScale,0.00001));
        }
        halo*=gain*(1.-core)*layerGain*haloScale;
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
        if (accent.w < -1.5) {
            float emitted=glow.r*halo*front.w*surface.y;
            // Keep Ember's red amplitude with a six-degree warm bias.
            // Fixed ratios stay safe after arbitrary additive halo overlap
            // and native RGB8 rounding. Only G protects the final red byte;
            // its floor falls to exact zero with the red contribution.
            float tail=min(1.,(1./255.)/max(emitted,1e-12));
            // Material RGB is already the emitted halo hue, so the head's
            // peak normalizer and the actual glow stay coupled.
            glow.g=max(glow.g,glow.r*tail);
            if (accent.w < -3.5)
                glow.b=max(glow.b,glow.r*tail);
        }
        emittedHalo=glow*halo*front.w*surface.y;
        light=(col*core*silk*concentrated*(1.+1.20*shape.w)+glow*halo)*front.w;
        if (accent.w == -1.) {
            // Keep isolated Pastel cores bright and coloured. Only the high
            // radiance contributed by highlights approaches the 0.8 knee.
            float peak=max(light.r,max(light.g,light.b));
            float knee=0.8+0.2*(1.-exp(-max(peak-0.8,0.)/0.2));
            light*=min(1.,knee/max(peak,0.0001));
        }
        weight=core/(1.+across*0.14)+halo*0.12;
    }
    float alpha=surface.y;
    fragColor=vec4(light*alpha,clamp(weight*alpha*0.35,0.,1.));
    if (accent.w < -2.5) {
        vec3 original=guardedEmberHalo(emittedHalo,accent.w < -3.5);
        vec3 dimmed=min(original,guardedEmberHalo(emittedHalo*glowFade,accent.w < -3.5));
        // Cap every channel after rounding. Restore the integer safety
        // half-plane by reducing R only, never by adding G/B.
        uvec3 rgb=uvec3(floor(dimmed*255.+0.5));
        uint allowed=accent.w < -3.5 ? (1000u*rgb.g+1000u*rgb.b)/777u
                                             : (1000u*rgb.g+817u*rgb.b)/709u;
        rgb.r=min(rgb.r,allowed);
        fragColor.rgb+=vec3(rgb)/255.-emittedHalo;
    }
}
