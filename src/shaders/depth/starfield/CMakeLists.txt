qt_add_shaders(VisualDepth depth_starfield_shaders
    GLSL "300es,310es,320es,130,140,150,330" HLSL "50" MSL "12"
    PREFIX "/depth/starfield" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/windshield.vert" "${CMAKE_CURRENT_SOURCE_DIR}/windshield.frag")
