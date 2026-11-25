#version 440
layout(location=0) in vec2 coords;
layout(location=0) out vec4 fragColor;
layout(std140,binding=0) uniform Params { mat4 matrix; vec4 blend; } ub;
layout(binding=1) uniform sampler2D oldStill;
layout(binding=2) uniform sampler2D newStill;
void main() {
    fragColor=mix(texture(oldStill,coords),texture(newStill,coords),ub.blend.x)*ub.blend.y;
    // The stills carry premultiplied colour and opacity from the same blend
    // as the main-pass holds. Interpolate both to preserve their background.
}
