qt_add_shaders(VisualDepth rain_shaders
    GLSL "300es,330" HLSL "50" MSL "12"
    PREFIX "/depth/rain" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/field.vert" "${CMAKE_CURRENT_SOURCE_DIR}/field.frag"
          "${CMAKE_CURRENT_SOURCE_DIR}/heads.vert" "${CMAKE_CURRENT_SOURCE_DIR}/heads.frag")
