target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/shore.cpp" "${CMAKE_CURRENT_LIST_DIR}/shore.h")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/aurora" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_aurora_settings PREFIX "/qml/settings/visuals/aurora"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/aurora"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/aurora/Settings.qml")
target_include_directories(VisualDepth SYSTEM PRIVATE "${SNAKES_QT_RHI_INCLUDE_DIR}")
