cmake_minimum_required(VERSION 3.12)

# include(../CMakeFiles/pico.dir/DependInfo.cmake)
include(${DEPENDINFO_PATH})

project(my_project)

list(SORT CMAKE_C_TARGET_INCLUDE_PATH)
list(JOIN CMAKE_C_TARGET_INCLUDE_PATH ":" INCLUDE_PATH)

list(SORT CMAKE_TARGET_DEFINITIONS_C)
list(JOIN CMAKE_TARGET_DEFINITIONS_C ":" DEFINITIONS)

# add_custom_target(write
#     COMMAND echo ${INCLUDE_PATH} > include_path
#     COMMAND echo ${DEFINITIONS} > definitions)
add_custom_target(write
    COMMAND echo -n ${INCLUDE_PATH} > ${INCLUDE_PATH_FILE}
    COMMAND echo -n ${DEFINITIONS} > ${DEFINITIONS_FILE})
    