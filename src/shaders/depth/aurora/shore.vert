#version 440
layout(location=0) in vec4 position;
layout(location=1) in vec2 texcoord;
layout(std140,binding=0) uniform Shore {
    mat4 matrix; vec4 bounds; vec4 transform; vec4 skyTop; vec4 skyBottom; vec4 body; vec4 crown; vec4 display; vec4 limit; vec4 optics;
} scene;
layout(location=0) out vec2 uv;
layout(location=1) out vec2 world;
void main() {
    uv=texcoord;
    world=(position.xy-scene.transform.xy)/scene.transform.zw;
    gl_Position=scene.matrix*position;
}
