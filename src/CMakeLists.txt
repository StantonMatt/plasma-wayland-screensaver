# Share the material and baked resource pack with renderer integration tests.
# RHI's public headers have limited compatibility and Debian ships them in
# qt6-base-private-dev. Only these headers are needed; link the public QtGui.
get_target_property(snake_qt_gui_includes Qt6::Gui INTERFACE_INCLUDE_DIRECTORIES)
find_path(SNAKES_QT_RHI_INCLUDE_DIR rhi/qrhi.h
    HINTS ${snake_qt_gui_includes}
    PATH_SUFFIXES "${Qt6Gui_VERSION}/QtGui")
if(NOT SNAKES_QT_RHI_INCLUDE_DIR)
    message(FATAL_ERROR "QShader/QRhi headers are required; install qt6-base-private-dev matching QtGui")
endif()
find_package(Python3 REQUIRED COMPONENTS Interpreter)
set(snakes_atlas_header "${CMAKE_CURRENT_BINARY_DIR}/snakes_atlas.h")
add_custom_command(OUTPUT "${snakes_atlas_header}"
    COMMAND "${Python3_EXECUTABLE}" "${PROJECT_SOURCE_DIR}/scripts/generate-snakes-atlas.py" "${snakes_atlas_header}"
    DEPENDS "${PROJECT_SOURCE_DIR}/scripts/generate-snakes-atlas.py"
    VERBATIM)
add_library(SnakeMaterial STATIC snakematerial.cpp snakematerial.h "${snakes_atlas_header}")
target_include_directories(SnakeMaterial PRIVATE "${CMAKE_CURRENT_BINARY_DIR}")
set_target_properties(SnakeMaterial PROPERTIES AUTOMOC OFF)
target_include_directories(SnakeMaterial SYSTEM PUBLIC "${SNAKES_QT_RHI_INCLUDE_DIR}")
target_link_libraries(SnakeMaterial PUBLIC Qt6::Quick SnakesCore::snakes_core)
qt_add_shaders(SnakeMaterial snakes_shaders
    BATCHABLE
    # Cover Qt RHI's ES 3.x and desktop/core-profile version selection, while
    # retaining legacy targets. Unsupported legacy drivers fail the probe.
    GLSL "100es,300es,310es,320es,120,130,140,150,330"
    HLSL "50"
    MSL "12"
    PREFIX "/snakes"
    FILES shaders/snake.vert shaders/snake.frag)

add_library(EglDrain STATIC egldrain.cpp egldrain.h)
set_target_properties(EglDrain PROPERTIES AUTOMOC OFF)
target_link_libraries(EglDrain PUBLIC ${CMAKE_DL_LIBS})
target_link_options(EglDrain INTERFACE
    "LINKER:--export-dynamic-symbol=wl_display_create_queue_with_name"
    "LINKER:--export-dynamic-symbol=wl_display_dispatch_queue_pending"
    "LINKER:--export-dynamic-symbol=wl_event_queue_destroy"
    "LINKER:--export-dynamic-symbol=wl_display_disconnect")

add_library(GraphicsBackend STATIC graphicsbackend.cpp graphicsbackend.h eglworkaround.cpp eglworkaround.h)
set_target_properties(GraphicsBackend PROPERTIES AUTOMOC OFF)
target_link_libraries(GraphicsBackend PUBLIC Qt6::Quick EglDrain)

set(screensaver_sources
    main.cpp
    animationstate.cpp
    animationstate.h
    applicationcontroller.cpp
    applicationcontroller.h
    configuration.cpp
    configuration.h
    fireflyrenderer.cpp
    fireflyrenderer.h
    idlemonitor.cpp
    idlemonitor.h
    inhibitor.cpp
    inhibitor.h
    overlaymanager.cpp
    overlaymanager.h
    presentationclock.cpp
    presentationclock.h
    screensaverstatemachine.cpp
    screensaverstatemachine.h
    snakesimulation.cpp
    snakesimulation.h
    snakerenderer.cpp
    snakerenderer.h
    resources.qrc
)

add_executable(plasma-visual-screensaver ${screensaver_sources})
target_link_libraries(plasma-visual-screensaver PRIVATE
    GraphicsBackend
    SnakeMaterial
    SnakesCore::snakes_core
    Qt6::Core
    Qt6::DBus
    Qt6::Gui
    Qt6::Qml
    Qt6::Quick
    Qt6::QuickControls2
    KF6::ConfigCore
    KF6::IdleTime
    LayerShellQt::Interface
)
target_compile_definitions(plasma-visual-screensaver PRIVATE
    APP_VERSION="${PROJECT_VERSION}"
)

install(TARGETS plasma-visual-screensaver ${KDE_INSTALL_TARGETS_DEFAULT_ARGS})

# Tests declare their renderer consumers after this directory. Attach the same
# baked material once all targets exist, without maintaining a second resource pack.
function(link_snake_material_consumers)
    foreach(consumer test-snakerenderer test-snakesimulation test-snakesqml test-overlaysnakes)
        if(TARGET ${consumer})
            target_link_libraries(${consumer} PRIVATE SnakeMaterial)
        endif()
    endforeach()
endfunction()
cmake_language(DEFER DIRECTORY "${PROJECT_SOURCE_DIR}" CALL link_snake_material_consumers)
