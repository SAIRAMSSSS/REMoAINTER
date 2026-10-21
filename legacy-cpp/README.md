# REMoAINTER — Legacy C++ / Vulkan Prototype

This directory archives the original C++ prototype of REMoAINTER.

---

## Status Notice

> **IMPORTANT**: The C++ implementation is a legacy prototype and is **no longer the active product codebase**.
> The production implementation of REMoAINTER is located in [`/rust`](../rust/) and is built with Rust, `eframe 0.29`, `egui_glow`, and `glow 0.14` (OpenGL 3.3+ Core).

---

## Contents

- `src/`: Original C++ source code:
  - `core/`: Math, memory, and engine primitives.
  - `graphics/`: Vulkan 1.3 rendering device, instance, and swapchain management.
  - `painting/`: Layer stack and basic brush blitting.
  - `ui/`: Dear ImGui docking integration.
  - `io/`: Asset importers.
- `CMakeLists.txt`: CMake build configuration.
- `shaders/`: GLSL shaders compiled to SPIR-V (`pbr.vert.spv`, `pbr.frag.spv`).
- `third_party/`: Submodules / external dependencies (GLFW, Dear ImGui, Volk, etc.).
- `build/`: Pre-compiled binary (`build/remoainter`) targeting discrete Vulkan hardware.

---

## Legacy Build Instructions (Reference Only)

To build the legacy prototype on Linux with Vulkan SDK installed:

```bash
mkdir -p build && cd build
cmake -GNinja -DCMAKE_BUILD_TYPE=Release ..
ninja
./remoainter
```
