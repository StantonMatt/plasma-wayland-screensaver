target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp")
target_include_directories(VisualDepth SYSTEM PRIVATE "${SNAKES_QT_RHI_INCLUDE_DIR}")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/fireflies" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_fireflies_settings PREFIX "/qml/settings/visuals/fireflies"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/fireflies"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/fireflies/Settings.qml")
