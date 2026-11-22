qt_add_shaders(VisualDepth depth_aurora_shaders
    PREFIX "/depth/aurora"
    GLSL "300es,310es,320es,150,330,440"
    HLSL "50" MSL "12"
    FILES curtain.vert curtain.frag shore.vert shore.frag)
