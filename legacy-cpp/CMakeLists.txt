cmake_minimum_required(VERSION 3.20)
project(REMoAINTER VERSION 1.0.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Find Vulkan
find_package(Vulkan REQUIRED)
message(STATUS "Found Vulkan: ${Vulkan_INCLUDE_DIRS}")

# Find GLFW3
find_package(glfw3 3.3 QUIET)
if(NOT glfw3_FOUND)
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(GLFW REQUIRED glfw3)
endif()

# Find GLM
find_package(glm QUIET)

# Include directories
include_directories(
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${CMAKE_CURRENT_SOURCE_DIR}/third_party
    ${CMAKE_CURRENT_SOURCE_DIR}/third_party/imgui
    ${CMAKE_CURRENT_SOURCE_DIR}/third_party/imgui/backends
    ${Vulkan_INCLUDE_DIRS}
)

if(GLM_INCLUDE_DIRS)
    include_directories(${GLM_INCLUDE_DIRS})
endif()

# ImGui Sources
set(IMGUI_SOURCES
    third_party/imgui/imgui.cpp
    third_party/imgui/imgui_demo.cpp
    third_party/imgui/imgui_draw.cpp
    third_party/imgui/imgui_tables.cpp
    third_party/imgui/imgui_widgets.cpp
    third_party/imgui/backends/imgui_impl_glfw.cpp
    third_party/imgui/backends/imgui_impl_vulkan.cpp
)

# Application Sources
set(APP_SOURCES
    src/main.cpp
    src/core/Engine.cpp
    src/core/Camera.cpp
    src/graphics/Mesh.cpp
    src/graphics/Texture.cpp
    src/painting/Layer.cpp
    src/painting/LayerStack.cpp
    src/ui/UIManager.cpp
    src/io/TextureExporter.cpp
    ${IMGUI_SOURCES}
)

# Shader Compilation using glslc
find_program(GLSLC glslc)
if(GLSLC)
    message(STATUS "Found glslc: ${GLSLC}")
    set(SHADER_SOURCES
        ${CMAKE_CURRENT_SOURCE_DIR}/shaders/pbr.vert
        ${CMAKE_CURRENT_SOURCE_DIR}/shaders/pbr.frag
    )
    foreach(SHADER ${SHADER_SOURCES})
        get_filename_component(SHADER_NAME ${SHADER} NAME)
        set(SPV_OUTPUT ${CMAKE_CURRENT_SOURCE_DIR}/shaders/${SHADER_NAME}.spv)
        add_custom_command(
            OUTPUT ${SPV_OUTPUT}
            COMMAND ${GLSLC} ${SHADER} -o ${SPV_OUTPUT}
            DEPENDS ${SHADER}
            COMMENT "Compiling SPIR-V shader: ${SHADER_NAME}"
        )
        list(APPEND SHADER_OUTPUTS ${SPV_OUTPUT})
    endforeach()
    add_custom_target(compile_shaders ALL DEPENDS ${SHADER_OUTPUTS})
endif()

add_executable(remoainter ${APP_SOURCES})

if(GLSLC)
    add_dependencies(remoainter compile_shaders)
endif()

# Target Link Libraries
target_link_libraries(remoainter PRIVATE Vulkan::Vulkan)

if(TARGET glfw)
    target_link_libraries(remoainter PRIVATE glfw)
elseif(glfw3_FOUND)
    target_link_libraries(remoainter PRIVATE glfw3)
else()
    target_link_libraries(remoainter PRIVATE ${GLFW_LIBRARIES})
    target_include_directories(remoainter PRIVATE ${GLFW_INCLUDE_DIRS})
endif()

if(UNIX AND NOT APPLE)
    target_link_libraries(remoainter PRIVATE pthread dl)
endif()

message(STATUS "Configured REMoAINTER for ${CMAKE_SYSTEM_NAME}")
