# ABI records must explicitly initialize fields added by future ABI revisions.
add_executable(test-eglworkaround test_eglworkaround.cpp)
target_include_directories(test-eglworkaround PRIVATE ../src)
target_link_libraries(test-eglworkaround PRIVATE GraphicsBackend Qt6::Test)
add_test(NAME egl-workaround COMMAND test-eglworkaround)

# Test actual libwayland queue/closure dispatch over a socketpair. The tiny
# test-only NVIDIA-named caller is isolated from runtime/system library paths.
find_library(WAYLAND_CLIENT_RUNTIME NAMES wayland-client libwayland-client.so.0 REQUIRED)
add_library(egldrain-fixture SHARED egldrain_fixture.cpp)
set_target_properties(egldrain-fixture PROPERTIES AUTOMOC OFF
    OUTPUT_NAME nvidia-egl-wayland2 VERSION 1.0.1 SOVERSION 1
    LIBRARY_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/egldrain-fixture")
target_link_libraries(egldrain-fixture PRIVATE ${CMAKE_DL_LIBS})
add_library(egldrain-unsupported-fixture SHARED egldrain_fixture.cpp)
set_target_properties(egldrain-unsupported-fixture PROPERTIES AUTOMOC OFF
    OUTPUT_NAME nvidia-egl-wayland2 VERSION 9.9.9 SOVERSION 1
    LIBRARY_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/egldrain-unsupported-fixture")
target_link_libraries(egldrain-unsupported-fixture PRIVATE ${CMAKE_DL_LIBS})
foreach(version IN ITEMS 1.0.2 1.0.3)
    add_library(egldrain-fixture-${version} SHARED egldrain_fixture.cpp)
    set_target_properties(egldrain-fixture-${version} PROPERTIES AUTOMOC OFF
        OUTPUT_NAME nvidia-egl-wayland2 VERSION ${version} SOVERSION 1
        LIBRARY_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/egldrain-fixture-${version}")
    target_link_libraries(egldrain-fixture-${version} PRIVATE ${CMAKE_DL_LIBS})
endforeach()
target_compile_definitions(test-eglworkaround PRIVATE EGL_DRAIN_DISCOVERY_FIXTURE="$<TARGET_FILE:egldrain-fixture-1.0.2>")
add_dependencies(test-eglworkaround egldrain-fixture-1.0.2)
add_executable(test-egldrain test_egldrain.cpp)
target_include_directories(test-egldrain PRIVATE ../src)
target_compile_definitions(test-egldrain PRIVATE EGL_DRAIN_FIXTURE="$<TARGET_FILE:egldrain-fixture>"
    EGL_DRAIN_UNSUPPORTED_FIXTURE="$<TARGET_FILE:egldrain-unsupported-fixture>"
    EGL_DRAIN_FIXTURE_102="$<TARGET_FILE:egldrain-fixture-1.0.2>"
    EGL_DRAIN_FIXTURE_103="$<TARGET_FILE:egldrain-fixture-1.0.3>")
target_link_libraries(test-egldrain PRIVATE EglDrain Qt6::Test "${WAYLAND_CLIENT_RUNTIME}")
add_dependencies(test-egldrain egldrain-fixture egldrain-unsupported-fixture egldrain-fixture-1.0.2 egldrain-fixture-1.0.3)
add_test(NAME egl-drain COMMAND test-egldrain)

add_executable(test-graphicsbackend test_graphicsbackend.cpp)
target_include_directories(test-graphicsbackend PRIVATE ../src)
target_link_libraries(test-graphicsbackend PRIVATE GraphicsBackend Qt6::Test)
add_test(NAME graphics-backend COMMAND test-graphicsbackend)

add_executable(test-graphicsbackend-window test_graphicsbackend_window.cpp
    ../src/presentationclock.cpp ../src/presentationclock.h)
target_include_directories(test-graphicsbackend-window PRIVATE ../src)
target_link_libraries(test-graphicsbackend-window PRIVATE GraphicsBackend Qt6::Test)
add_test(NAME graphics-backend-window COMMAND test-graphicsbackend-window)
set_tests_properties(graphics-backend-window PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# Verify the default without a GPU context or rendering. Preserve the desktop's
# basic OpenGL loop and default swap interval; Vulkan startup changes both.
add_test(NAME graphics-backend-default-opengl
    COMMAND ${CMAKE_COMMAND} -E env --unset=QT_QUICK_BACKEND --unset=QSG_RHI_BACKEND
        --unset=PVS_GRAPHICS_API --unset=PVS_TEST_VULKAN
        QT_QPA_PLATFORM=offscreen QSG_RENDER_LOOP=basic PVS_TEST_DEFAULT_OPENGL=1
        $<TARGET_FILE:test-graphicsbackend-window> defaultOpenGL)

if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    set_source_files_properties(test_snakerenderer.cpp test_snakesimulation.cpp test_snakesqml.cpp
        PROPERTIES COMPILE_OPTIONS "-Werror=missing-field-initializers")
endif()

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
add_test(NAME egl-drain-exports
    COMMAND "${Python3_EXECUTABLE}" "${PROJECT_SOURCE_DIR}/scripts/check-egldrain-exports.py"
        "$<TARGET_FILE:plasma-visual-screensaver>")
add_test(NAME frame-timing-gate
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/test_frame_timing.py")
add_test(NAME frame-timing-comparison
    COMMAND "${Python3_EXECUTABLE}" "${CMAKE_CURRENT_SOURCE_DIR}/test_frame_timing_comparison.py")
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

# Controller failures and the settings window run offscreen with an
# isolated config directory and an unavailable session bus (never the desktop).
add_executable(test-applicationcontroller
    test_applicationcontroller.cpp
    ../src/applicationcontroller.cpp ../src/applicationcontroller.h
    ../src/idlemonitor.cpp ../src/idlemonitor.h
    ../src/inhibitor.cpp ../src/inhibitor.h
    ../src/screensaverstatemachine.cpp ../src/screensaverstatemachine.h
    ../src/overlaymanager.cpp ../src/overlaymanager.h
    ../src/animationstate.cpp ../src/animationstate.h
    ../src/presentationclock.cpp ../src/presentationclock.h
    ../src/snakesimulation.cpp ../src/snakesimulation.h
    ../src/snakerenderer.cpp ../src/snakerenderer.h
    ../src/fireflyrenderer.cpp ../src/fireflyrenderer.h
    ../src/configuration.cpp ../src/configuration.h
    ../src/resources.qrc)
target_include_directories(test-applicationcontroller PRIVATE ../src)
target_link_libraries(test-applicationcontroller PRIVATE GraphicsBackend SnakeMaterial
    Qt6::DBus Qt6::Quick Qt6::QuickControls2 Qt6::Test KF6::ConfigCore KF6::IdleTime
    LayerShellQt::Interface SnakesCore::snakes_core)
add_test(NAME application-controller COMMAND test-applicationcontroller
    settingsQmlDependenciesAreAvailable reportsRealInhibitorFailureOnlyForPreview exposesReactiveMonitorCount
    settingsWindowLoadsFromResources mapsTechnicalInhibitorErrors busyPreviewReportsFailure updateCenterReportsDestination
    logsActivationSourceAndDismissalReason dismissalGatesIdleAndPreview freshIdleIntervalResetsOnResume freshIntervalAloneDoesNotActivate mappingResumeDoesNotDismiss
    pendingResumeRearmIsInvalidated teardownReconcilesIdleConfiguration)
add_test(NAME application-controller-no-screens COMMAND test-applicationcontroller
    -platform "offscreen:configfile=${CMAKE_CURRENT_SOURCE_DIR}/fixtures/offscreen-no-screens.json"
    noUsableScreenReportsPreviewFailure)
set_tests_properties(application-controller application-controller-no-screens PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# The real settings window (from the app's resources) with a real Configuration
# and a stand-in controller; offscreen, isolated config and session bus.
add_executable(test-settingsqml test_settingsqml.cpp
    ../src/configuration.cpp ../src/configuration.h ../src/resources.qrc)
target_include_directories(test-settingsqml PRIVATE ../src)
target_link_libraries(test-settingsqml PRIVATE Qt6::Quick Qt6::Qml Qt6::QuickTest KF6::ConfigCore)
add_test(NAME settings-qml COMMAND test-settingsqml -input "${CMAKE_CURRENT_SOURCE_DIR}/settings")
set_tests_properties(settings-qml PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
