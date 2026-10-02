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

add_executable(test-presentationclock
    test_presentationclock.cpp
    ../src/presentationclock.cpp
    ../src/presentationclock.h
)
target_include_directories(test-presentationclock PRIVATE ../src)
target_link_libraries(test-presentationclock PRIVATE Qt6::Quick Qt6::Test)
add_test(NAME presentation-clock COMMAND test-presentationclock)
set_tests_properties(presentation-clock PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

add_executable(test-snakerenderer
    test_snakerenderer.cpp
    ../src/snakerenderer.cpp
    ../src/snakerenderer.h
    ../src/snakesimulation.cpp
    ../src/snakesimulation.h
    ../src/configuration.cpp
    ../src/configuration.h
)
target_include_directories(test-snakerenderer PRIVATE ../src)
target_link_libraries(test-snakerenderer PRIVATE Qt6::Qml Qt6::Quick Qt6::Test KF6::ConfigCore SnakesCore::snakes_core)
add_test(NAME snake-renderer COMMAND test-snakerenderer)
set_tests_properties(snake-renderer PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

add_executable(test-snakesimulation
    test_snakesimulation.cpp
    ../src/snakesimulation.cpp ../src/snakesimulation.h
    ../src/snakerenderer.cpp ../src/snakerenderer.h
    ../src/configuration.cpp ../src/configuration.h)
target_include_directories(test-snakesimulation PRIVATE ../src)
target_link_libraries(test-snakesimulation PRIVATE Qt6::Quick Qt6::Test KF6::ConfigCore SnakesCore::snakes_core)
add_test(NAME snake-simulation COMMAND test-snakesimulation)
set_tests_properties(snake-simulation PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

find_package(Qt6 6.8 REQUIRED COMPONENTS QuickTest)
add_executable(test-snakesqml
    test_snakesqml.cpp
    ../src/snakesimulation.cpp ../src/snakesimulation.h
    ../src/snakerenderer.cpp ../src/snakerenderer.h
    ../src/configuration.cpp ../src/configuration.h)
target_include_directories(test-snakesqml PRIVATE ../src)
target_link_libraries(test-snakesqml PRIVATE Qt6::Quick Qt6::Qml Qt6::QuickTest KF6::ConfigCore SnakesCore::snakes_core)
add_test(NAME snakes-qml COMMAND test-snakesqml -input "${CMAKE_CURRENT_SOURCE_DIR}/qml")
set_tests_properties(snakes-qml PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

add_executable(test-snakescore-abi test_snakescore_abi.cpp)
target_link_libraries(test-snakescore-abi PRIVATE Qt6::Core Qt6::Test SnakesCore::snakes_core)
add_test(NAME snakes-core-abi COMMAND test-snakescore-abi)

add_test(NAME snakes-core-rust
    COMMAND "${CMAKE_COMMAND}" -E env ${SNAKES_CORE_CARGO_ENV}
        "${CARGO_EXECUTABLE}" test --frozen
        --manifest-path "${SNAKES_CORE_MANIFEST}" --profile "${SNAKES_CORE_PROFILE}")

# Build fixture-only hooks separately; the production C ABI remains feature-free.
find_package(Python3 REQUIRED COMPONENTS Interpreter)
add_custom_target(snakes-parity-build ALL
    COMMAND "${CMAKE_COMMAND}" -E env ${SNAKES_CORE_CARGO_ENV}
        "${CARGO_EXECUTABLE}" build --frozen --offline
        --manifest-path "${SNAKES_CORE_MANIFEST}" --profile "${SNAKES_CORE_PROFILE}"
        --features parity --example parity
    BYPRODUCTS "${SNAKES_CORE_TARGET_DIR}/${SNAKES_CORE_CONFIG}/${SNAKES_CORE_PROFILE_DIR}/examples/parity"
    VERBATIM)
add_test(NAME snakes-parity
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/parity/snakes/compare.py"
        --binary "${SNAKES_CORE_TARGET_DIR}/${SNAKES_CORE_CONFIG}/${SNAKES_CORE_PROFILE_DIR}/examples/parity")
set_tests_properties(snakes-parity PROPERTIES TIMEOUT 120)
add_test(NAME snakes-parity-record
    COMMAND bash "${CMAKE_CURRENT_SOURCE_DIR}/parity/snakes/record.sh" --check)
set_tests_properties(snakes-parity-record PROPERTIES TIMEOUT 240)
add_executable(test-overlaysnakes
    test_overlaysnakes.cpp
    ../src/overlaymanager.cpp ../src/overlaymanager.h
    ../src/animationstate.cpp ../src/animationstate.h
    ../src/presentationclock.cpp ../src/presentationclock.h
    ../src/snakesimulation.cpp ../src/snakesimulation.h
    ../src/snakerenderer.cpp ../src/snakerenderer.h
    ../src/fireflyrenderer.cpp ../src/fireflyrenderer.h
    ../src/configuration.cpp ../src/configuration.h
    ../src/resources.qrc)
target_include_directories(test-overlaysnakes PRIVATE ../src)
target_link_libraries(test-overlaysnakes PRIVATE Qt6::Quick Qt6::Test KF6::ConfigCore LayerShellQt::Interface SnakesCore::snakes_core)
add_test(NAME overlay-snakes COMMAND test-overlaysnakes)
set_tests_properties(overlay-snakes PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
