# Add this visual's adapter sources here when its implementation is ready.
target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/starfield" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_starfield_settings PREFIX "/qml/settings/visuals/starfield"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/starfield"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/starfield/Settings.qml")
