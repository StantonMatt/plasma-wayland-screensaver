# Use the distribution toolchain before searching PATH (which may contain rustup).
find_program(CARGO_EXECUTABLE cargo PATHS /usr/bin NO_DEFAULT_PATH)
find_program(CARGO_EXECUTABLE cargo REQUIRED)
find_program(RUSTC_EXECUTABLE rustc PATHS /usr/bin NO_DEFAULT_PATH)
find_program(RUSTC_EXECUTABLE rustc REQUIRED)

set(SNAKES_CORE_MANIFEST "${PROJECT_SOURCE_DIR}/rust/snakes-core/Cargo.toml")
set(SNAKES_CORE_CARGO_HOME "${PROJECT_BINARY_DIR}/cargo-home")
set(SNAKES_CORE_TARGET_DIR "${PROJECT_BINARY_DIR}/cargo-target")
set(SNAKES_CORE_CONFIG "$<IF:$<BOOL:$<CONFIG>>,$<CONFIG>,Release>")
set(SNAKES_CORE_PROFILE "$<IF:$<CONFIG:Debug>,dev,release>")
set(SNAKES_CORE_PROFILE_DIR "$<IF:$<CONFIG:Debug>,debug,release>")
# RUSTFLAGS stays in the command's inherited environment, including Debian flags.
set(SNAKES_CORE_CARGO_ENV
    "RUSTC=${RUSTC_EXECUTABLE}"
    "CARGO_HOME=${SNAKES_CORE_CARGO_HOME}"
    "CARGO_TARGET_DIR=${SNAKES_CORE_TARGET_DIR}/${SNAKES_CORE_CONFIG}"
    "CARGO_NET_OFFLINE=true")

# Probe this compiler's native static-library requirements without building the
# simulation at configure time or putting artifacts in its source directory.
set(native_probe_dir "${PROJECT_BINARY_DIR}/rust-native-probe")
file(MAKE_DIRECTORY "${native_probe_dir}")
file(WRITE "${native_probe_dir}/probe.rs" "pub fn probe() {}\n")
separate_arguments(native_probe_flags UNIX_COMMAND "$ENV{RUSTFLAGS}")
execute_process(
    COMMAND "${RUSTC_EXECUTABLE}" ${native_probe_flags}
        --crate-type=staticlib --edition=2024 --print=native-static-libs
        "${native_probe_dir}/probe.rs" -o "${native_probe_dir}/libprobe.a"
    OUTPUT_VARIABLE native_probe_stdout
    ERROR_VARIABLE native_probe_stderr
    COMMAND_ERROR_IS_FATAL ANY)
if(NOT "${native_probe_stdout}${native_probe_stderr}" MATCHES "native-static-libs: ([^\r\n]+)")
    message(FATAL_ERROR "rustc did not report native static-library dependencies")
endif()
separate_arguments(SNAKES_CORE_NATIVE_LIBS UNIX_COMMAND "${CMAKE_MATCH_1}")
message(STATUS "Rust native static libraries: ${SNAKES_CORE_NATIVE_LIBS}")

# Always invoke Cargo; it tracks manifests, source additions/removals and flags.
add_custom_target(snakes-core-build
    COMMAND "${CMAKE_COMMAND}" -E env ${SNAKES_CORE_CARGO_ENV}
        "${CARGO_EXECUTABLE}" build --frozen
        --manifest-path "${SNAKES_CORE_MANIFEST}" --profile "${SNAKES_CORE_PROFILE}"
    BYPRODUCTS "${SNAKES_CORE_TARGET_DIR}/${SNAKES_CORE_CONFIG}/${SNAKES_CORE_PROFILE_DIR}/libsnakes_core.a"
    VERBATIM)
add_library(SnakesCore::snakes_core STATIC IMPORTED GLOBAL)
add_dependencies(SnakesCore::snakes_core snakes-core-build)
set_target_properties(SnakesCore::snakes_core PROPERTIES
    INTERFACE_INCLUDE_DIRECTORIES "${PROJECT_SOURCE_DIR}/rust/snakes-core/include"
    INTERFACE_LINK_LIBRARIES "${SNAKES_CORE_NATIVE_LIBS}")
if(CMAKE_CONFIGURATION_TYPES)
    foreach(config IN LISTS CMAKE_CONFIGURATION_TYPES)
        if(config STREQUAL "Debug")
            set(profile_dir debug)
        else()
            set(profile_dir release)
        endif()
        string(TOUPPER "${config}" config_upper)
        set_property(TARGET SnakesCore::snakes_core APPEND PROPERTY IMPORTED_CONFIGURATIONS "${config}")
        set_property(TARGET SnakesCore::snakes_core PROPERTY "IMPORTED_LOCATION_${config_upper}"
            "${SNAKES_CORE_TARGET_DIR}/${config}/${profile_dir}/libsnakes_core.a")
    endforeach()
else()
    if(CMAKE_BUILD_TYPE STREQUAL "Debug")
        set(profile_dir debug)
    else()
        set(profile_dir release)
    endif()
    if(CMAKE_BUILD_TYPE)
        set(config "${CMAKE_BUILD_TYPE}")
    else()
        set(config Release)
    endif()
    set_property(TARGET SnakesCore::snakes_core PROPERTY IMPORTED_LOCATION
        "${SNAKES_CORE_TARGET_DIR}/${config}/${profile_dir}/libsnakes_core.a")
endif()
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    # Hide archive symbols even when KDE enables executable symbol exports.
    set_property(TARGET SnakesCore::snakes_core PROPERTY
        INTERFACE_LINK_OPTIONS "LINKER:--exclude-libs,libsnakes_core.a")
endif()
