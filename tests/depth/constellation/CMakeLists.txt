add_test(NAME depth-constellation-settings COMMAND test-settingsqml -input "${CMAKE_CURRENT_SOURCE_DIR}/qml")
set_tests_properties(depth-constellation-settings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

add_executable(test-constellation-upload test_upload.cpp)
target_link_libraries(test-constellation-upload PRIVATE VisualDepth VisualRenderer Qt6::Test)
add_test(NAME depth-constellation-upload COMMAND test-constellation-upload)
set_tests_properties(depth-constellation-upload PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
