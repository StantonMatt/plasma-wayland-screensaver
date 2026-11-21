add_test(NAME depth-fireflies-settings COMMAND test-settingsqml -input "${CMAKE_CURRENT_SOURCE_DIR}/qml")
set_tests_properties(depth-fireflies-settings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
