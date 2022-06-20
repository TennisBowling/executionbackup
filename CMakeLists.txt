cmake_minimum_required(VERSION 3.0.0)
project(executionbackup VERSION 0.1.0)

# include(CTest)
# enable_testing()
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/bin)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

option(BUILD_SHARED_LIBS "Build shared libraries" ON)

add_executable(executionbackup main.cpp)

set(CPACK_PROJECT_NAME ${PROJECT_NAME})
set(CPACK_PROJECT_VERSION ${PROJECT_VERSION})
include(CPack)

target_include_directories(executionbackup PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

add_subdirectory(boost-cmake)
target_link_libraries(executionbackup PUBLIC Boost::boost)
target_link_libraries(executionbackup PUBLIC Boost::program_options)

include(FetchContent)
FetchContent_Declare(cpr GIT_REPOSITORY https://github.com/libcpr/cpr.git
    GIT_TAG db351ffbbadc6c4e9239daaa26e9aefa9f0ec82d)
FetchContent_MakeAvailable(cpr)
target_include_directories(executionbackup PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/cpr)
target_link_libraries(executionbackup PRIVATE cpr::cpr)

FetchContent_Declare(json URL https://github.com/nlohmann/json/releases/download/v3.10.5/json.tar.xz)
FetchContent_MakeAvailable(json)
target_include_directories(executionbackup PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/json)
target_link_libraries(executionbackup PRIVATE nlohmann_json::nlohmann_json)

# get spdlog through FetchContent
FetchContent_Declare(spdlog GIT_REPOSITORY https://github.com/gabime/spdlog.git
    GIT_TAG v1.10.0)
FetchContent_MakeAvailable(spdlog)
target_include_directories(executionbackup PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/spdlog)
target_link_libraries(executionbackup PRIVATE spdlog::spdlog)

install(TARGETS executionbackup DESTINATION bin)