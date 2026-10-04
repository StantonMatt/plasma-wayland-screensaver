#version 440
layout(location=0) in vec2 position;
layout(location=1) in vec2 uv;
layout(location=2) in vec4 color;
layout(location=3) in vec4 params;
layout(location=0) out vec2 coord;
layout(location=1) out vec4 base;
layout(location=2) out vec4 packed;
layout(location=3) out vec2 screenPosition;
layout(location=4) out float ribbonLimit;
layout(location=5) out vec3 waveLight;
layout(std140,binding=0) uniform buf { mat4 matrix; float opacity; float time; vec2 light; float animationTime; float motionScale; float paletteMode; } ub;
// Decode origins at vertices, then interpolate premultiplied light. Decoding
// interpolated bit fields would mix unrelated flags at replacement wave joins.
vec3 waveAccent(int k) {
    vec3 a=k==1?vec3(1.0,0.88235,0.30196):k==2?vec3(1.0,0.37255,0.82353):
        k==3?vec3(0.66275,0.54510,1.0):k==4?vec3(0.61569,1.0,0.22745):vec3(0.74118,0.95294,1.0);
    if(ub.paletteMode==1.0) a=mix(a,vec3(dot(a,vec3(0.2126,0.7152,0.0722))),0.88);
    if(ub.paletteMode==2.0) a=mix(a,vec3(1),0.3);
    return a;
}
void main() {
    ribbonLimit=abs(uv.x); screenPosition=position; coord=uv; base=color; packed=params*255.0;
    waveLight=vec3(0);
    if(int(packed.x+0.5)==0) {
        int flags=int(packed.z+0.5);
        int origin=((flags>>1)&1)|((flags>>3)&6);
        packed.z=float(flags&~50);
        if((flags&128)==0 && packed.w>0.0) {
            vec3 c=mix(color.rgb,vec3(1),max(0.0,0.30-dot(color.rgb,vec3(0.2126,0.7152,0.0722)))*1.1);
            vec3 gold=vec3(1.0,0.847,0.29);
            if((int(packed.y+0.5)&64)!=0) gold=mix(gold,vec3(1),0.85);
            // origin 0 with light: the vertex's own colour (Feast rainbow), barely whitened.
            vec3 accent=origin>=1 && origin<=5?waveAccent(origin):origin==7?gold:mix(c,vec3(1),origin==0?0.2:0.75);
            waveLight=accent*(packed.w/255.0);
        }
    }
    gl_Position=ub.matrix*vec4(position,0.0,1.0);
}
