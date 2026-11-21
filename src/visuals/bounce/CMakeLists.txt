# Add this visual's adapter sources here when its implementation is ready.
target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/bounce" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_bounce_settings PREFIX "/qml/settings/visuals/bounce"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/bounce"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/bounce/Settings.qml")
