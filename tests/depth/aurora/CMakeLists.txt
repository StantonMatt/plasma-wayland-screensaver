add_test(NAME depth-aurora-settings COMMAND test-settingsqml -input "${CMAKE_CURRENT_SOURCE_DIR}/qml")
set_tests_properties(depth-aurora-settings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
