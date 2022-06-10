cmake_minimum_required(VERSION 3.0.0)
project(executionbackup-cpp VERSION 0.1.0)

#include(CTest)
#enable_testing()

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/bin)

add_executable(executionbackup-cpp main.cpp)

set(CPACK_PROJECT_NAME ${PROJECT_NAME})
set(CPACK_PROJECT_VERSION ${PROJECT_VERSION})
include(CPack)


include(FetchContent)
FetchContent_Declare(
  asiocmake
  GIT_REPOSITORY "https://github.com/OlivierLDff/asio.cmake"
  GIT_TAG        "main"
)
FetchContent_MakeAvailable(asiocmake)
target_link_libraries(executionbackup-cpp PRIVATE asio::asio)

FetchContent_Declare(cpr GIT_REPOSITORY https://github.com/libcpr/cpr.git
                         GIT_TAG db351ffbbadc6c4e9239daaa26e9aefa9f0ec82d)
FetchContent_MakeAvailable(cpr)
target_link_libraries(executionbackup-cpp PRIVATE cpr::cpr)

FetchContent_Declare(json URL https://github.com/nlohmann/json/releases/download/v3.10.5/json.tar.xz)
FetchContent_MakeAvailable(json)
target_link_libraries(executionbackup-cpp PRIVATE nlohmann_json::nlohmann_json)

# get spdlog through FetchContent
FetchContent_Declare(spdlog GIT_REPOSITORY https://github.com/gabime/spdlog.git
                         GIT_TAG v1.10.0)
FetchContent_MakeAvailable(spdlog)
target_link_libraries(executionbackup-cpp PRIVATE spdlog::spdlog)