add_executable(test-statemachine
    test_statemachine.cpp
    ../src/screensaverstatemachine.cpp
    ../src/screensaverstatemachine.h
)
target_link_libraries(test-statemachine PRIVATE Qt6::Core Qt6::Test)
add_test(NAME state-machine COMMAND test-statemachine)

add_executable(test-configuration
    test_configuration.cpp
    ../src/configuration.cpp
    ../src/configuration.h
)
target_link_libraries(test-configuration PRIVATE Qt6::Core Qt6::Test KF6::ConfigCore)
add_test(NAME configuration COMMAND test-configuration)

add_executable(test-animationstate
    test_animationstate.cpp
    ../src/animationstate.cpp
    ../src/animationstate.h
)
target_link_libraries(test-animationstate PRIVATE Qt6::Core Qt6::Gui Qt6::Test)
add_test(NAME animation-state COMMAND test-animationstate)

add_executable(test-fireflyrenderer
    test_fireflyrenderer.cpp
    ../src/fireflyrenderer.cpp
    ../src/fireflyrenderer.h
)
target_include_directories(test-fireflyrenderer PRIVATE ../src)
target_link_libraries(test-fireflyrenderer PRIVATE Qt6::Quick Qt6::Test)
add_test(NAME firefly-renderer COMMAND test-fireflyrenderer)
set_tests_properties(firefly-renderer PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

add_executable(test-snakerenderer
    test_snakerenderer.cpp
    ../src/snakerenderer.cpp
    ../src/snakerenderer.h
)
target_include_directories(test-snakerenderer PRIVATE ../src)
target_link_libraries(test-snakerenderer PRIVATE Qt6::Qml Qt6::Quick Qt6::Test)
add_test(NAME snake-renderer COMMAND test-snakerenderer)
set_tests_properties(snake-renderer PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

if(TARGET Qt6::qmltestrunner)
    add_test(NAME snakes-qml
        COMMAND Qt6::qmltestrunner -input "${CMAKE_CURRENT_SOURCE_DIR}/qml")
    set_tests_properties(snakes-qml PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
else()
    message(FATAL_ERROR "Qt6 qmltestrunner is required for visual module tests")
endif()

add_executable(test-snakescore-abi test_snakescore_abi.cpp)
target_link_libraries(test-snakescore-abi PRIVATE Qt6::Core Qt6::Test SnakesCore::snakes_core)
add_test(NAME snakes-core-abi COMMAND test-snakescore-abi)

add_test(NAME snakes-core-rust
    COMMAND "${CMAKE_COMMAND}" -E env ${SNAKES_CORE_CARGO_ENV}
        "${CARGO_EXECUTABLE}" test --frozen
        --manifest-path "${SNAKES_CORE_MANIFEST}" --profile "${SNAKES_CORE_PROFILE}")
