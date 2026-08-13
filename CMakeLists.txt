cmake_minimum_required(VERSION 3.22)

project(plasma-visual-screensaver
        VERSION 0.3.0
        DESCRIPTION "Non-locking visual screensaver for KDE Plasma Wayland"
        HOMEPAGE_URL "https://github.com/StantonMatt/plasma-wayland-screensaver"
        LANGUAGES CXX)

file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/packaging/debian/changelog"
     DEBIAN_CHANGELOG_FIRST_LINE LIMIT_COUNT 1)
string(REPLACE "." "\\." PROJECT_VERSION_REGEX "${PROJECT_VERSION}")
if(NOT DEBIAN_CHANGELOG_FIRST_LINE MATCHES
   "^plasma-visual-screensaver \\(${PROJECT_VERSION_REGEX}-([0-9A-Za-z.+~]+)\\)")
    message(FATAL_ERROR
        "Newest Debian changelog entry must match project version ${PROJECT_VERSION}")
endif()
set(DEBIAN_PACKAGE_RELEASE "${CMAKE_MATCH_1}")

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
find_program(GZIP_EXECUTABLE gzip REQUIRED)
set(DEBIAN_CHANGELOG_GZ "${CMAKE_CURRENT_BINARY_DIR}/changelog.Debian.gz")
execute_process(
    COMMAND "${GZIP_EXECUTABLE}" -9n -c
            "${CMAKE_CURRENT_SOURCE_DIR}/packaging/debian/changelog"
    OUTPUT_FILE "${DEBIAN_CHANGELOG_GZ}"
    COMMAND_ERROR_IS_FATAL ANY)
set(MANPAGE_GZ "${CMAKE_CURRENT_BINARY_DIR}/plasma-visual-screensaver.1.gz")
configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/packaging/man/plasma-visual-screensaver.1.in"
    "${CMAKE_CURRENT_BINARY_DIR}/plasma-visual-screensaver.1"
    @ONLY)
execute_process(
    COMMAND "${GZIP_EXECUTABLE}" -9n -c
            "${CMAKE_CURRENT_BINARY_DIR}/plasma-visual-screensaver.1"
    OUTPUT_FILE "${MANPAGE_GZ}"
    COMMAND_ERROR_IS_FATAL ANY)

install(FILES README.md
        DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/doc/${PROJECT_NAME})
install(FILES packaging/debian/copyright
        DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/doc/${PROJECT_NAME})
install(FILES "${DEBIAN_CHANGELOG_GZ}"
        DESTINATION ${CMAKE_INSTALL_DATAROOTDIR}/doc/${PROJECT_NAME})
install(FILES "${MANPAGE_GZ}"
        DESTINATION ${CMAKE_INSTALL_MANDIR}/man1)

set(CPACK_GENERATOR "DEB")
set(CPACK_PACKAGE_NAME "plasma-visual-screensaver")
set(CPACK_PACKAGE_VENDOR "Matthew Stanton")
set(CPACK_PACKAGE_CONTACT "Matthew Stanton <stantonmatthewj@gmail.com>")
set(CPACK_PACKAGE_DESCRIPTION_SUMMARY "${PROJECT_DESCRIPTION}")
set(CPACK_PACKAGE_DESCRIPTION
    "Decorative screensaver for Plasma on Wayland.
It provides configurable low-resource animations, OLED-friendly backgrounds,
a moving clock, multi-monitor support, and automatic idle activation. This is
a visual overlay, not a screen locker or security boundary.")
set(CPACK_PACKAGE_HOMEPAGE_URL "${PROJECT_HOMEPAGE_URL}")
set(CPACK_RESOURCE_FILE_LICENSE "${CMAKE_CURRENT_SOURCE_DIR}/LICENSE")
set(CPACK_PACKAGING_INSTALL_PREFIX "/usr")
set(CPACK_DEBIAN_FILE_NAME DEB-DEFAULT)
set(CPACK_DEBIAN_PACKAGE_NAME "plasma-visual-screensaver")
set(CPACK_DEBIAN_PACKAGE_MAINTAINER "${CPACK_PACKAGE_CONTACT}")
set(CPACK_DEBIAN_PACKAGE_SECTION "kde")
set(CPACK_DEBIAN_PACKAGE_PRIORITY "optional")
set(CPACK_DEBIAN_PACKAGE_RELEASE "${DEBIAN_PACKAGE_RELEASE}")
set(CPACK_DEBIAN_PACKAGE_HOMEPAGE "${PROJECT_HOMEPAGE_URL}")
set(CPACK_DEBIAN_PACKAGE_SHLIBDEPS ON)
set(CPACK_DEBIAN_PACKAGE_DEPENDS
    "qml6-module-qtquick, qml6-module-qtquick-controls, qml6-module-qtquick-layouts")
set(CPACK_DEBIAN_PACKAGE_RECOMMENDS "xdg-desktop-portal-kde, powerdevil")
set(CPACK_DEBIAN_COMPRESSION_TYPE "zstd")
set(CPACK_STRIP_FILES ON)
set(CPACK_DEBIAN_PACKAGE_CONTROL_STRICT_PERMISSION ON)
set(CPACK_DEBIAN_PACKAGE_CONTROL_EXTRA
    "${CMAKE_CURRENT_SOURCE_DIR}/packaging/debian/conffiles;${CMAKE_CURRENT_SOURCE_DIR}/packaging/debian/postinst;${CMAKE_CURRENT_SOURCE_DIR}/packaging/debian/postrm")
include(CPack)

feature_summary(WHAT ALL FATAL_ON_MISSING_REQUIRED_PACKAGES)
