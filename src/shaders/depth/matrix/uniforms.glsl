layout(std140, binding = 0) uniform buf
{
    mat4 qt_Matrix;
    vec4 viewport; // offset into the desk, local size
    vec4 grid; // pitch, first local column and row
    vec4 section; // fractional stamp in 30 Hz ticks, half life, layer opacity, layer
    vec4 light; // palette, background, interpolation, scene opacity
    vec4 weather; // sweep y, lightning envelope, depth, lull
    vec4 strike; // bolt world x, world height, bolt core gain, strike age
    vec4 returns; // return stroke ages, number of returns
    vec4 sky_top;
    vec4 background;
    vec4 colors[6];
}
ub;
