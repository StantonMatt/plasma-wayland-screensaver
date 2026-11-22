add_test(NAME depth-ribbons-settings COMMAND test-settingsqml -input "${CMAKE_CURRENT_SOURCE_DIR}/qml")
set_tests_properties(depth-ribbons-settings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

if(PVS_ENABLE_VISUAL_CAPTURE)
    # The capture executable is declared after the per-visual test directories.
    function(add_ribbons_crossing_fixture)
        target_sources(test-visual-depth-capture PRIVATE "${PROJECT_SOURCE_DIR}/tests/depth/ribbons/crossingfixture.cpp")
        target_link_options(test-visual-depth-capture PRIVATE "LINKER:--wrap=visuals_ribbons_write" "LINKER:--wrap=visuals_ribbons_write_packed" "LINKER:--wrap=visuals_ribbons_same_pair")
    endfunction()
    cmake_language(DEFER DIRECTORY "${PROJECT_SOURCE_DIR}/tests" CALL add_ribbons_crossing_fixture)
endif()
