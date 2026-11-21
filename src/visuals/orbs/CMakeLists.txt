# Add this visual's adapter sources here when its implementation is ready.
target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/orbs" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_orbs_settings PREFIX "/qml/settings/visuals/orbs"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/orbs"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/orbs/Settings.qml")
