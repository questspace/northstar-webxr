cmake_minimum_required(VERSION 3.16)
project(xvsdk-bridge CXX)

# The official XVSDK unpacked by tools/xvsdk/fetch-xvsdk.ps1. It ships no import library for
# xvsdk.dll; build.ps1 generates one from the DLL's export table into XVSDK_IMPLIB_DIR.
set(XVSDK_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../tools/xvsdk/sdk" CACHE PATH "Unpacked XVSDK")
set(XVSDK_IMPLIB_DIR "${CMAKE_BINARY_DIR}/implib" CACHE PATH "Generated xvsdk.lib")

set(CMAKE_CXX_STANDARD 17)
add_executable(xvsdk-bridge main.cpp)
target_include_directories(xvsdk-bridge PRIVATE "${XVSDK_DIR}/include/xvsdk" "${XVSDK_DIR}/include2")
target_link_directories(xvsdk-bridge PRIVATE "${XVSDK_IMPLIB_DIR}" "${XVSDK_DIR}/lib")
target_link_libraries(xvsdk-bridge PRIVATE xvsdk)
# The SDK bundles an older msvcp140.dll; std::mutex from VS 2022 17.10+ crashes on it unless
# the constexpr mutex constructor is disabled.
target_compile_definitions(xvsdk-bridge PRIVATE _USE_MATH_DEFINES _DISABLE_CONSTEXPR_MUTEX_CONSTRUCTOR)
