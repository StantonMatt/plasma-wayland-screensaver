#version 440
layout(location=0) in vec4 vShape;
layout(location=1) in vec2 vParameters;
layout(location=2) in vec4 vColor;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform buf { mat4 matrix; vec4 info; } ubuf;
void main() {
    float distance;
    if (vParameters.y<0.0) {
        distance=min(vParameters.x-abs(vShape.x),vShape.y);
    } else {
        // The two arcs belong to one Canvas fill. Union coverage once, so
        // intersecting circles never add their alpha twice within this pair.
        distance=max(vParameters.x-length(vShape.xy),vParameters.y-length(vShape.zw));
    }
    float coverage=clamp(distance*ubuf.info.y+0.5,0.0,1.0);
    // The private union pass selects the largest coverage of each stroke in
    // depth, then stencil permits its colour to be added exactly once. Later
    // strokes occupy disjoint, nearer depth bands, so they still add normally.
    // Quantizing to RGBA8 coverage leaves ample precision in a 24-bit depth
    // buffer even with 512 primitives. Both passes calculate identical depth.
    gl_FragDepth=ubuf.info.w>0.0
        ? 1.0-(ubuf.info.z+1.0+floor(coverage*255.0+0.5)/512.0)/1024.0
        : gl_FragCoord.z;
    float alpha=coverage*vColor.a*ubuf.info.x;
    fragColor=vec4(vColor.rgb*alpha,alpha);
}
