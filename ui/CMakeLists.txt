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

    if(DEFINED ENV{WHISTLEBLOWER_PROGRAM_BIN})
        set(WHISTLEBLOWER_PROGRAM_BIN "$ENV{WHISTLEBLOWER_PROGRAM_BIN}")
    elseif(NOT DEFINED WHISTLEBLOWER_PROGRAM_BIN)
        set(WHISTLEBLOWER_PROGRAM_BIN "${CMAKE_CURRENT_SOURCE_DIR}/artifacts/whistleblower_registry.bin")
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

    # Bundle the storage_module + delivery_module init configs as qrc:
    #   qrc:/configs/storage_config.json — real Logos storage SPRs
    #   qrc:/configs/delivery_config.json — Logos Delivery `logos.dev` preset
    # WhistleblowerBackend's constructor reads these and calls init+start on
    # both modules so the publish flow has a usable network when the user
    # clicks "Publish".
    qt_add_resources(${PLUGIN_TARGET} "whistleblower_configs"
        PREFIX "/configs"
        BASE "configs"
        FILES configs/storage_config.json configs/delivery_config.json
    )

    # Logos SDK headers are required for the real Basecamp integration: the
    # backend dereferences LogosAPIClient to call storage_module / delivery_module.
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

    # Basecamp's findAvailableUiPlugins() (in main_ui_plugin/MainUIBackend.cpp)
    # discovers C++ plugins via the filename convention `<plugin>/<plugin>.<ext>`
    # — i.e. it expects this dylib to be named exactly `whistleblower.dylib`,
    # not `libwhistleblower_plugin.dylib`. PREFIX/OUTPUT_NAME make cmake emit
    # the right filename without renaming the cmake target.
    set_target_properties(${PLUGIN_TARGET} PROPERTIES
        OUTPUT_NAME "whistleblower"
        PREFIX ""
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
    if(EXISTS "${WHISTLEBLOWER_PROGRAM_BIN}")
        install(FILES "${WHISTLEBLOWER_PROGRAM_BIN}"
            DESTINATION lib
            RENAME whistleblower_registry.bin
        )
    endif()

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

    qt_add_resources(${APP_TARGET} "whistleblower_app_configs"
        PREFIX "/configs"
        BASE "configs"
        FILES configs/storage_config.json configs/delivery_config.json
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
