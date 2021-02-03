cmake_minimum_required(VERSION 3.12)

set(PICO_SDK_FETCH_FROM_GIT on)
include(../pico-sdk/pico_sdk_init.cmake)
project(my_project)
pico_sdk_init()

add_library(pico ${ENTRY_POINT})
target_link_libraries(pico pico_stdlib)
