cmake_minimum_required(VERSION 3.20)
project(xvisio VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Library
add_library(xvisio SHARED
    src/device/device.cpp
    src/device/hid.cpp
    src/slam.cpp
    src/types/pose.cpp
    src/xvisio.cpp
)

target_compile_options(xvisio PRIVATE -Wall -Wextra -Wno-deprecated-enum-enum-conversion)
target_include_directories(xvisio 
    PUBLIC include/libxvisio
    PRIVATE include/libxvisio/device include/libxvisio/types
)

# libusb dependency
find_package(PkgConfig REQUIRED)
pkg_check_modules(LIBUSB REQUIRED libusb-1.0)

target_link_libraries(xvisio ${LIBUSB_LINK_LIBRARIES})
target_include_directories(xvisio PUBLIC ${LIBUSB_INCLUDE_DIRS})
target_compile_options(xvisio PUBLIC ${LIBUSB_CFLAGS_OTHER})

# Example application
add_executable(xvisio_test example/main.cpp)
target_compile_options(xvisio_test PRIVATE -Wall -Wextra -Wno-deprecated-enum-enum-conversion)
target_link_libraries(xvisio_test xvisio ${LIBUSB_LINK_LIBRARIES})
target_include_directories(xvisio_test 
    PUBLIC ${LIBUSB_INCLUDE_DIRS}
    PRIVATE include/libxvisio include/libxvisio/device include/libxvisio/types
)
