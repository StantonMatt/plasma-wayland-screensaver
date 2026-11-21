target_sources(VisualDepth PRIVATE
    "${CMAKE_CURRENT_LIST_DIR}/descriptor.h" "${CMAKE_CURRENT_LIST_DIR}/adapter.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/material.cpp" "${CMAKE_CURRENT_LIST_DIR}/alphabet.cpp"
    "${CMAKE_CURRENT_LIST_DIR}/../common/glyphatlas.cpp")
target_link_libraries(VisualDepth PRIVATE VisualRenderer)
add_subdirectory("${PROJECT_SOURCE_DIR}/src/shaders/depth/matrix" "${CMAKE_CURRENT_BINARY_DIR}/shaders")
qt_add_resources(VisualDepth depth_matrix_settings PREFIX "/qml/settings/visuals/matrix"
    BASE "${PROJECT_SOURCE_DIR}/qml/settings/visuals/matrix"
    FILES "${PROJECT_SOURCE_DIR}/qml/settings/visuals/matrix/Settings.qml")
