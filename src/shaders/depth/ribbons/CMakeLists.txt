qt_add_shaders(VisualDepth depth_ribbons_shaders
    GLSL "300es,310es,320es,150,330" HLSL "50" MSL "12"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}" PREFIX "/ribbons"
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/silk.vert" "${CMAKE_CURRENT_SOURCE_DIR}/silk.frag"
          "${CMAKE_CURRENT_SOURCE_DIR}/dissolve.vert" "${CMAKE_CURRENT_SOURCE_DIR}/dissolve.frag")
