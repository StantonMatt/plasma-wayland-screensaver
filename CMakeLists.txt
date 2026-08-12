cmake_minimum_required(VERSION 3.22)

project(plasma-visual-screensaver VERSION 0.3.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

find_package(ECM 6.0 REQUIRED NO_MODULE)
list(APPEND CMAKE_MODULE_PATH ${ECM_MODULE_PATH})
include(KDEInstallDirs6)
include(KDECMakeSettings)
include(KDECompilerSettings NO_POLICY_SCOPE)
include(FeatureSummary)

find_package(Qt6 6.8 REQUIRED COMPONENTS Core DBus Gui Qml Quick QuickControls2)
find_package(KF6 6.0 REQUIRED COMPONENTS Config IdleTime)
find_package(LayerShellQt REQUIRED)

add_subdirectory(src)

include(CTest)
if(BUILD_TESTING)
    find_package(Qt6 6.8 REQUIRED COMPONENTS Test)
    add_subdirectory(tests)
endif()

install(FILES data/applications/org.kde.plasmavisualscreensaver.desktop
        DESTINATION ${KDE_INSTALL_APPDIR})
install(FILES data/autostart/org.kde.plasmavisualscreensaver.desktop
        DESTINATION ${KDE_INSTALL_AUTOSTARTDIR})
install(FILES data/metainfo/org.kde.plasmavisualscreensaver.metainfo.xml
        DESTINATION ${KDE_INSTALL_METAINFODIR})
install(FILES data/icons/org.kde.plasmavisualscreensaver.svg
        DESTINATION ${KDE_INSTALL_ICONDIR}/hicolor/scalable/apps)

feature_summary(WHAT ALL FATAL_ON_MISSING_REQUIRED_PACKAGES)
