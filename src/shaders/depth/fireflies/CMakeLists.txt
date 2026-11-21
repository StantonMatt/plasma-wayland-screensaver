qt_add_shaders(VisualDepth depth_fireflies_shaders
    GLSL "300es,310es,320es,330" HLSL "50" MSL "12"
    PREFIX "/fireflies" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/meadow.vert" "${CMAKE_CURRENT_SOURCE_DIR}/meadow.frag")
