#version 440
layout(std140,binding=0) uniform Shore {
    mat4 matrix; vec4 bounds; vec4 transform; vec4 skyTop; vec4 skyBottom; vec4 body; vec4 crown; vec4 display; vec4 limit; vec4 optics;
} scene;
layout(binding=1) uniform sampler2D emission;
layout(binding=2) uniform sampler2D horizon;
layout(binding=3) uniform sampler2D stars;
layout(location=0) in vec2 uv;
layout(location=1) in vec2 world;
layout(location=0) out vec4 color;
// Toroidal void-and-cluster ranks, packed four native dither samples per word.
const uint blueNoise[64]=uint[64](
    0xb9470a96u, 0x87309513u, 0x593acb23u, 0x2588a6f1u, 0x85fa74bbu, 0x5dd7ac67u, 0x8ee77a46u, 0xee6bd41bu,
    0x1aa84c1eu, 0xfd073beau, 0x2f9d0db6u, 0x5a053fb5u, 0xc236d581u, 0x9375ce5fu, 0xd251de65u, 0xc894fc78u,
    0x90039bf3u, 0xbe289f43u, 0x68af1937u, 0x32ab4910u, 0x77e4546du, 0x53d815f2u, 0x26f789e8u, 0x127edbbfu,
    0x2bbd1fb2u, 0x017db44bu, 0xa13c70a9u, 0xe13d608du, 0xdda34586u, 0x62f99869u, 0xd34d1dd0u, 0xc524ed04u,
    0x0e5eff08u, 0x2e4122ccu, 0x7fefb791u, 0x6492ad56u, 0x397c9ccfu, 0x80b1e684u, 0x2a6a0bdfu, 0x35f516c7u,
    0xc1ec2955u, 0xca4e09a0u, 0xe59a445cu, 0xa5734a8bu, 0x3e176ee0u, 0x1872f85bu, 0xae14fea7u, 0x1cbad933u,
    0xdab38cc3u, 0x8fd12c97u, 0x57c47638u, 0x4299006fu, 0x024ff00fu, 0xeb48b06cu, 0x21dc27b8u, 0x61fb83e9u,
    0x82a42d79u, 0x0c7b20f4u, 0xa28a6652u, 0xaa31cd40u, 0x34d663e2u, 0xc0e358c9u, 0xbc06f69eu, 0xc6501171u
);
float decodeHeight(vec2 pair) {
    return dot(pair,vec2(65280.,255.))*(1.10/65535.);
}
void main() {
    vec2 p=(world-scene.bounds.xy)/scene.bounds.zw;
    float x=(p.x*(scene.display.y-1.)+0.5)/scene.display.y;
    vec4 ridge=texture(horizon,vec2(x,0.25));
    float baseline=decodeHeight(texture(horizon,vec2(x,0.75)).rg);
    float skyline=min(baseline,decodeHeight(ridge.rg));
    float air=exp(-abs(skyline-p.y-.005)/.07);
    float black=1.-step(.001,dot(scene.skyBottom.rgb,vec3(1.)));
    vec3 tint=mix(vec3(1.),scene.body.rgb/max(.001,max(max(scene.body.r,scene.body.g),scene.body.b)),.35);
    float wash=0.5+0.5*sin(world.x/880.+p.y*2.6);
    vec3 background=mix(scene.skyTop.rgb,scene.skyBottom.rgb,clamp(p.y,0.,1.))
        +scene.display.w*(tint*air*(.026+.021*black)*(.9+.1*wash)+scene.crown.rgb*0.016*wash*(1.-p.y));
    // Integrate the emission of neighbouring ray bundles in azimuth. The
    // target includes the same optical margin on both sides of every output.
    float meridian=abs(world.x-scene.display.z)/scene.bounds.w;
    float canopy=16.3*scene.bounds.w/1440.;
    float sigma=mix(canopy,scene.optics.w,smoothstep(.07,.27,meridian));
    float du=sigma*abs(dFdx(uv.x)/dFdx(world.x));
    vec4 central=texture(emission,uv);
    float texel=1./float(textureSize(emission,0).x);
    vec2 dx=vec2(texel,0.);
    vec2 nearOff=mix(1.9,1.4,central.a)*dx, wideOff=mix(4.45,3.55,central.a)*dx;
    vec3 nearPair=texture(emission,uv-nearOff).rgb+texture(emission,uv+nearOff).rgb;
    vec3 left=texture(emission,uv-wideOff).rgb, right=texture(emission,uv+wideOff).rgb;
    float extended=min(max(max(left.g,left.r),left.b),max(max(right.g,right.r),right.b));
    vec3 narrow=mix(central.rgb,.20*central.rgb+.27*nearPair+.13*(left+right),
                    smoothstep(.004,.032,extended));
    vec3 light=narrow;
    float sigmaTexels=du/texel;
    if (sigmaTexels>2.2) {
        // Integrate every emission texel inside three standard deviations.
        // Combining adjacent weights into a linear texture sample avoids
        // undersampling stars or narrow features as the aperture widens.
        float norm=1.;
        vec3 sum=central.rgb;
        for (int i=1;float(i)<3.*sigmaTexels;i+=2) {
            vec2 iz=vec2(float(i),float(i+1))/sigmaTexels;
            vec2 k=exp(-.5*iz*iz);
            float weight=k.x+k.y;
            vec2 off=dx*(float(i)+k.y/weight);
            vec4 a=texture(emission,uv-off), b=texture(emission,uv+off);
            sum+=weight*(mix(a.rgb,narrow,a.a)+mix(b.rgb,narrow,b.a));
            norm+=2.*weight;
        }
        light=mix(narrow,sum/norm,(1.-central.a)*smoothstep(.003,.035,extended));
    }
    // Compose the two emissions in screen space after reconstructing the sheets.
    // Each star can only increase sky radiance.
    // Spread a small point source over the optical footprint without letting
    // its position change the reconstruction of the curtain underneath it.
    vec2 starStep=1.35/vec2(textureSize(stars,0));
    vec2 sx=vec2(starStep.x,0.), sy=vec2(0.,starStep.y);
    vec3 star=.40*texture(stars,uv).rgb
             +.15*(texture(stars,uv-sx).rgb+texture(stars,uv+sx).rgb
                  +texture(stars,uv-sy).rgb+texture(stars,uv+sy).rgb);
    light+=star*(1.-light);
    vec3 sky=background+light*(1.-background);
    vec3 limit=scene.limit.rgb;
    vec3 knee=limit*.61;
    vec3 excess=max(vec3(0.),sky-knee);
    sky=min(sky,knee)+(limit-knee)*excess/(limit-knee+excess);
    float far=p.y-decodeHeight(ridge.rg);
    float nearRidge=p.y-decodeHeight(ridge.ba);
    float fw=max(fwidth(far)*0.55,0.00002);
    float nw=max(fwidth(nearRidge)*0.55,0.00002);
    sky=mix(sky,scene.skyBottom.rgb*0.59,smoothstep(-fw,fw,far));
    sky=mix(sky,scene.skyBottom.rgb*0.33,smoothstep(-nw,nw,nearRidge));
    uvec2 pixel=uvec2(gl_FragCoord.xy);
    uint frame=uint(scene.limit.w);
    uvec2 tile=pixel>>4u;
    uint hash=tile.x*0x9e3779b9u+tile.y*0x85ebca6bu+frame*0xc2b2ae35u;
    hash^=hash>>16u; hash*=0x7feb352du; hash^=hash>>15u;
    uvec2 px=(pixel+uvec2(hash,hash>>8u))&15u;
    uint packed=blueNoise[px.y*4u+(px.x>>2u)];
    float grain=(float((packed>>(8u*(px.x&3u)))&255u)/255.-.5)/255.;
    // A one-bit change near black can alternate between black and saturated
    // red. Keep temporal grain out of that range; visible hems retain it.
    grain*=smoothstep(4./255.,16./255.,max(max(sky.r,sky.g),sky.b));
    color=vec4(clamp(sky+grain,0.,1.),1.)*scene.display.x;
}
