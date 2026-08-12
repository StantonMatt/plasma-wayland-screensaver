set(screensaver_sources
    main.cpp
    animationstate.cpp
    animationstate.h
    applicationcontroller.cpp
    applicationcontroller.h
    configuration.cpp
    configuration.h
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
    resources.qrc
)

add_executable(plasma-visual-screensaver ${screensaver_sources})
target_link_libraries(plasma-visual-screensaver PRIVATE
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
