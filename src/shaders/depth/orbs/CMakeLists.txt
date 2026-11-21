qt_add_shaders(VisualDepth depth_orbs_material
    GLSL "300es,310es,320es,130,140,150,330" HLSL "50" MSL "12"
    PREFIX "/depth/orbs" BASE "${CMAKE_CURRENT_SOURCE_DIR}" FILES "${CMAKE_CURRENT_SOURCE_DIR}/orbs.vert" "${CMAKE_CURRENT_SOURCE_DIR}/orbs.frag")
