cmake_minimum_required(VERSION 3.20)
project(xvisio-windows-test LANGUAGES CXX C)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# --------------------------------------------------------------------------- #
#  Paths                                                                       #
# --------------------------------------------------------------------------- #

set(XSLAM_HEADERS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../xslam-headers")
set(XSLAM_LIBS_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../project-esky-unity/Assets/Plugins/x64")
set(LIBXVISIO_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../libxvisio-source")

# --------------------------------------------------------------------------- #
#  libusb via vcpkg                                                            #
# --------------------------------------------------------------------------- #

# vcpkg installs as libusb-1.0.lib in installed/x64-windows/lib/
find_path(LIBUSB_INCLUDE_DIRS libusb.h
    PATH_SUFFIXES libusb-1.0
)
find_library(LIBUSB_LIBRARIES
    NAMES libusb-1.0
)

if(LIBUSB_INCLUDE_DIRS AND LIBUSB_LIBRARIES)
    set(LIBUSB_FOUND TRUE)
    message(STATUS "Found libusb:")
    message(STATUS "  Include: ${LIBUSB_INCLUDE_DIRS}")
    message(STATUS "  Library: ${LIBUSB_LIBRARIES}")
else()
    message(WARNING "libusb not found. test_hid_protocol and test_compare will not build.")
endif()

# --------------------------------------------------------------------------- #
#  Check for official DLLs (for runtime loading / DLL copying)                 #
# --------------------------------------------------------------------------- #

set(XSLAM_SDK_AVAILABLE FALSE)
if(EXISTS "${XSLAM_LIBS_DIR}/xslam_sdk.dll")
    set(XSLAM_SDK_AVAILABLE TRUE)
    message(STATUS "Found xslam_sdk.dll at ${XSLAM_LIBS_DIR}")
endif()

# --------------------------------------------------------------------------- #
#  Test 1: test_xslam_sdk - Official SDK test (runtime loading only)           #
# --------------------------------------------------------------------------- #

# xslam_sdk.dll uses C++ mangled exports (NOT extern "C").
# We always use runtime loading via LoadLibrary + GetProcAddress with
# mangled name strings. No link-time dependency on xslam_sdk.lib needed.

add_executable(test_xslam_sdk test_xslam_sdk.cpp)
target_include_directories(test_xslam_sdk PRIVATE ${XSLAM_HEADERS_DIR})
if(MSVC)
    target_compile_options(test_xslam_sdk PRIVATE /W3)
    target_compile_definitions(test_xslam_sdk PRIVATE _CRT_SECURE_NO_WARNINGS)
endif()

# --------------------------------------------------------------------------- #
#  Test 2: test_hid_protocol - Raw HID protocol analyzer using libusb          #
# --------------------------------------------------------------------------- #

if(LIBUSB_FOUND)
    add_executable(test_hid_protocol test_hid_protocol.cpp)
    target_include_directories(test_hid_protocol PRIVATE
        ${XSLAM_HEADERS_DIR}
        ${LIBUSB_INCLUDE_DIRS}
    )
    target_link_libraries(test_hid_protocol PRIVATE ${LIBUSB_LIBRARIES})
    if(MSVC)
        target_compile_options(test_hid_protocol PRIVATE /W3)
        target_compile_definitions(test_hid_protocol PRIVATE _CRT_SECURE_NO_WARNINGS)
    endif()
endif()

# --------------------------------------------------------------------------- #
#  Test 3: test_xslam_hid - Official HID layer test                            #
#                                                                               #
#  xslam-drivers.dll exports hid_* as PLAIN C functions (no mangling).         #
#  Can use link-time binding or runtime loading.                                #
# --------------------------------------------------------------------------- #

add_executable(test_xslam_hid test_xslam_hid.cpp)
target_include_directories(test_xslam_hid PRIVATE ${XSLAM_HEADERS_DIR})
if(MSVC)
    target_compile_options(test_xslam_hid PRIVATE /W3)
    target_compile_definitions(test_xslam_hid PRIVATE _CRT_SECURE_NO_WARNINGS)
endif()

if(EXISTS "${XSLAM_LIBS_DIR}/xslam-drivers.lib")
    # Link-time binding works for the C-style hid_* exports
    add_library(xslam_drivers SHARED IMPORTED)
    set_target_properties(xslam_drivers PROPERTIES
        IMPORTED_IMPLIB "${XSLAM_LIBS_DIR}/xslam-drivers.lib"
        IMPORTED_LOCATION "${XSLAM_LIBS_DIR}/xslam-drivers.dll"
    )
    target_link_libraries(test_xslam_hid PRIVATE xslam_drivers)
    target_compile_definitions(test_xslam_hid PRIVATE XSLAM_LINK_STATIC=1)
else()
    target_compile_definitions(test_xslam_hid PRIVATE XSLAM_RUNTIME_LOAD=1)
endif()

# --------------------------------------------------------------------------- #
#  Test 4: test_compare - Side-by-side comparison (runtime loading for SDK)    #
# --------------------------------------------------------------------------- #

if(LIBUSB_FOUND)
    add_executable(test_compare test_compare.cpp)
    target_include_directories(test_compare PRIVATE
        ${XSLAM_HEADERS_DIR}
        ${LIBUSB_INCLUDE_DIRS}
    )
    target_link_libraries(test_compare PRIVATE ${LIBUSB_LIBRARIES})
    if(MSVC)
        target_compile_options(test_compare PRIVATE /W3)
        target_compile_definitions(test_compare PRIVATE _CRT_SECURE_NO_WARNINGS)
    endif()
endif()

# --------------------------------------------------------------------------- #
#  Copy DLLs to build directory for runtime loading                            #
# --------------------------------------------------------------------------- #

if(XSLAM_SDK_AVAILABLE)
    set(XSLAM_DLLS
        xslam_sdk.dll
        xslam-drivers.dll
        xslam_core.dll
        xslam_log.dll
        xslam_loader.dll
        xslam_algo_sdk.dll
        xslam_lib_ange.dll
        xslam_surface_reconstruction.dll
    )

    foreach(dll ${XSLAM_DLLS})
        if(EXISTS "${XSLAM_LIBS_DIR}/${dll}")
            add_custom_command(TARGET test_xslam_sdk POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E copy_if_different
                    "${XSLAM_LIBS_DIR}/${dll}"
                    "$<TARGET_FILE_DIR:test_xslam_sdk>/${dll}"
                COMMENT "Copying ${dll}"
            )
        endif()
    endforeach()

    # Boost DLLs
    set(BOOST_DLLS
        boost_filesystem-vc142-mt-x64-1_70.dll
        boost_system-vc142-mt-x64-1_70.dll
        boost_thread-vc142-mt-x64-1_70.dll
    )
    foreach(dll ${BOOST_DLLS})
        if(EXISTS "${XSLAM_LIBS_DIR}/${dll}")
            add_custom_command(TARGET test_xslam_sdk POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E copy_if_different
                    "${XSLAM_LIBS_DIR}/${dll}"
                    "$<TARGET_FILE_DIR:test_xslam_sdk>/${dll}"
                COMMENT "Copying ${dll}"
            )
        endif()
    endforeach()

    # libusb DLL from project-esky
    if(EXISTS "${XSLAM_LIBS_DIR}/libusb-1.0.dll")
        add_custom_command(TARGET test_xslam_sdk POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${XSLAM_LIBS_DIR}/libusb-1.0.dll"
                "$<TARGET_FILE_DIR:test_xslam_sdk>/libusb-1.0.dll"
            COMMENT "Copying libusb-1.0.dll"
        )
    endif()
endif()
