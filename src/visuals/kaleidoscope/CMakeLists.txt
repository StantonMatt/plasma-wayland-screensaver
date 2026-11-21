target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/fold.cpp" "${CMAKE_CURRENT_LIST_DIR}/fold.h")
target_include_directories(VisualDepth SYSTEM PRIVATE "${SNAKES_QT_RHI_INCLUDE_DIR}")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/kaleidoscope" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_kaleidoscope_settings PREFIX "/qml/settings/visuals/kaleidoscope"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/kaleidoscope"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/kaleidoscope/Settings.qml")
