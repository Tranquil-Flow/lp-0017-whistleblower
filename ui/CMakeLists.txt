cmake_minimum_required(VERSION 3.20)
project(whistleblower-ui LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ── Framework build (Basecamp plugin builder) ─────────────────────────────────
if(DEFINED ENV{LOGOS_MODULE_BUILDER_ROOT})
    include($ENV{LOGOS_MODULE_BUILDER_ROOT}/cmake/LogosModule.cmake)

    logos_module(
        NAME whistleblower
        SOURCES
            src/WhistleblowerPlugin.h
            src/WhistleblowerPlugin.cpp
            src/WhistleblowerBackend.h
            src/WhistleblowerBackend.cpp
        EXTERNAL_LIBS whistleblower_ffi
    )

# ── Manual build (development / standalone) ───────────────────────────────────
else()
    set(CMAKE_AUTOMOC ON)

    find_package(Qt6 REQUIRED COMPONENTS Core Gui Widgets Quick QuickWidgets Qml Concurrent RemoteObjects)

    # Location of the compiled Rust cdylib (build with: cargo build --release in ui/ffi/).
    # Override via -DWHISTLEBLOWER_FFI_LIB_DIR=... or set env var of same name.
    if(DEFINED ENV{WHISTLEBLOWER_FFI_LIB_DIR})
        set(WHISTLEBLOWER_FFI_LIB_DIR "$ENV{WHISTLEBLOWER_FFI_LIB_DIR}")
    elseif(NOT DEFINED WHISTLEBLOWER_FFI_LIB_DIR)
        set(WHISTLEBLOWER_FFI_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/ffi/target/release")
    endif()

    set(PLUGIN_TARGET whistleblower_plugin)

    qt_add_plugin(${PLUGIN_TARGET} CLASS_NAME WhistleblowerPlugin)

    target_sources(${PLUGIN_TARGET} PRIVATE
        src/WhistleblowerPlugin.cpp
        src/WhistleblowerBackend.cpp
    )

    qt_add_resources(${PLUGIN_TARGET} "whistleblower_qml"
        PREFIX "/qml"
        BASE "qml"
        FILES qml/Main.qml
    )

    # The Logos SDK headers are optional — our code only forward-declares
    # LogosAPI as an opaque pointer and never dereferences it.
    if(DEFINED ENV{LOGOS_CPP_SDK_ROOT})
        set(LOGOS_CPP_SDK_ROOT "$ENV{LOGOS_CPP_SDK_ROOT}")
    endif()

    target_include_directories(${PLUGIN_TARGET} PRIVATE
        ${CMAKE_CURRENT_SOURCE_DIR}/src
    )
    if(LOGOS_CPP_SDK_ROOT)
        target_include_directories(${PLUGIN_TARGET} PRIVATE
            ${LOGOS_CPP_SDK_ROOT}/include
            ${LOGOS_CPP_SDK_ROOT}/include/cpp
            ${LOGOS_CPP_SDK_ROOT}/include/core
            ${LOGOS_CPP_SDK_ROOT}/cpp
        )
    endif()

    target_link_directories(${PLUGIN_TARGET} PRIVATE "${WHISTLEBLOWER_FFI_LIB_DIR}")

    target_link_libraries(${PLUGIN_TARGET} PRIVATE
        Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Quick Qt6::QuickWidgets Qt6::RemoteObjects
        Qt6::Qml Qt6::Concurrent
        whistleblower_ffi
    )

    if(APPLE)
        set_target_properties(${PLUGIN_TARGET} PROPERTIES
            BUILD_RPATH   "@loader_path"
            INSTALL_RPATH "@loader_path"
        )
    elseif(UNIX)
        set_target_properties(${PLUGIN_TARGET} PROPERTIES
            BUILD_RPATH   "$ORIGIN"
            INSTALL_RPATH "$ORIGIN"
        )
    endif()

    install(TARGETS ${PLUGIN_TARGET}
        LIBRARY DESTINATION lib
    )
    # Pick the right shared-lib extension per host OS.
    if(APPLE)
        set(WHISTLEBLOWER_FFI_SUFFIX "dylib")
    elseif(WIN32)
        set(WHISTLEBLOWER_FFI_SUFFIX "dll")
    else()
        set(WHISTLEBLOWER_FFI_SUFFIX "so")
    endif()
    install(FILES "${WHISTLEBLOWER_FFI_LIB_DIR}/libwhistleblower_ffi.${WHISTLEBLOWER_FFI_SUFFIX}"
        DESTINATION lib
    )

    # ── Standalone preview app ────────────────────────────────────────────────
    set(APP_TARGET whistleblower_app)

    qt_add_executable(${APP_TARGET} src/main.cpp
        src/WhistleblowerPlugin.cpp
        src/WhistleblowerBackend.cpp
    )

    qt_add_resources(${APP_TARGET} "whistleblower_app_qml"
        PREFIX "/qml"
        BASE "qml"
        FILES qml/Main.qml
    )

    target_include_directories(${APP_TARGET} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)
    if(LOGOS_CPP_SDK_ROOT)
        target_include_directories(${APP_TARGET} PRIVATE
            ${LOGOS_CPP_SDK_ROOT}/include
            ${LOGOS_CPP_SDK_ROOT}/include/cpp
            ${LOGOS_CPP_SDK_ROOT}/include/core
            ${LOGOS_CPP_SDK_ROOT}/cpp
        )
    endif()

    target_link_directories(${APP_TARGET} PRIVATE "${WHISTLEBLOWER_FFI_LIB_DIR}")

    target_link_libraries(${APP_TARGET} PRIVATE
        Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Quick Qt6::QuickWidgets Qt6::RemoteObjects
        Qt6::Qml Qt6::Concurrent
        whistleblower_ffi
    )

    if(APPLE)
        set_target_properties(${APP_TARGET} PROPERTIES
            BUILD_RPATH   "@loader_path"
            INSTALL_RPATH "@loader_path"
        )
    elseif(UNIX)
        set_target_properties(${APP_TARGET} PROPERTIES
            BUILD_RPATH   "$ORIGIN"
            INSTALL_RPATH "$ORIGIN"
        )
    endif()

    install(TARGETS ${APP_TARGET} RUNTIME DESTINATION bin)
endif()
