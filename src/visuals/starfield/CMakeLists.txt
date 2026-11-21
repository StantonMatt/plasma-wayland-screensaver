target_sources(VisualDepth PRIVATE "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/windshield.cpp" "${CMAKE_CURRENT_LIST_DIR}/windshield.h")
target_include_directories(VisualDepth SYSTEM PRIVATE "${SNAKES_QT_RHI_INCLUDE_DIR}")
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/starfield" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_starfield_settings PREFIX "/qml/settings/visuals/starfield"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/starfield"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/starfield/Settings.qml")
