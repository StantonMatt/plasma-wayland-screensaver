set(kaleidoscope_shaders chamber.vert chamber.frag blur.vert blur.frag fold.vert fold.frag)
list(TRANSFORM kaleidoscope_shaders PREPEND "${CMAKE_CURRENT_SOURCE_DIR}/")
qt_add_shaders(VisualDepth depth_kaleidoscope_shaders
    PREFIX "/depth/kaleidoscope" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    GLSL "300es,310es,320es,150,330,440" HLSL "50" MSL "12"
    FILES ${kaleidoscope_shaders})
