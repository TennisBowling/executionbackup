cmake_minimum_required(VERSION 3.0.0)
project(executionbackup)

# include(CTest)
# enable_testing()
# set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/bin)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if (MSVC)
add_compile_options(/bigobj)
else ()
  add_compile_options(-Wa,-mbig-obj)
endif ()

add_executable(executionbackup main.cpp)

option(BUILD_SHARED_LIBS "build libraries shared" OFF)
option(CURL_ZLIB "use curl with zlib" OFF)
set(CPACK_PROJECT_NAME ${PROJECT_NAME})
set(CPACK_PROJECT_VERSION 0.1.0)


include(CPack)

# target_include_directories(executionbackup PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
add_subdirectory(boost-cmake)
target_link_libraries(executionbackup PUBLIC Boost::boost)
target_link_libraries(executionbackup PUBLIC Boost::program_options)

option(JWT_BUILD_EXAMPLES "build jwt examples" OFF)
add_subdirectory(jwt-cpp)
target_link_libraries(executionbackup PRIVATE jwt-cpp::jwt-cpp)

include(FetchContent)
FetchContent_Declare(cpr GIT_REPOSITORY https://github.com/libcpr/cpr.git
    GIT_TAG db351ffbbadc6c4e9239daaa26e9aefa9f0ec82d)
FetchContent_MakeAvailable(cpr)
target_link_libraries(executionbackup PRIVATE cpr::cpr)

FetchContent_Declare(json URL https://github.com/nlohmann/json/releases/download/v3.10.5/json.tar.xz)
FetchContent_MakeAvailable(json)
target_link_libraries(executionbackup PRIVATE nlohmann_json::nlohmann_json)

# get spdlog through FetchContent
FetchContent_Declare(spdlog GIT_REPOSITORY https://github.com/gabime/spdlog.git
    GIT_TAG v1.10.0)
FetchContent_MakeAvailable(spdlog)
target_link_libraries(executionbackup PRIVATE spdlog::spdlog_header_only)

install(TARGETS executionbackup RUNTIME DESTINATION bin)
