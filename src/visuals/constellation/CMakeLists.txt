# Add this visual's adapter sources here when its implementation is ready.
target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/constellation" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_constellation_settings PREFIX "/qml/settings/visuals/constellation"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/constellation"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/constellation/Settings.qml")
target_link_libraries(VisualDepth PRIVATE VisualRenderer)
