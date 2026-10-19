# REMoAINTER

### Open-Source Cross-Platform 3D PBR Texture Painting Software

**REMoAINTER** is an open-source, high-performance 3D physically based rendering (PBR) texture painting application written in modern C++20 and powered by **Vulkan 1.3**.

---

## Key Features

- **High-Performance Vulkan 1.3 Architecture:**
  - Real-time **Cook-Torrance GGX microfacet specular** and Lambertian diffuse shading.
  - Tangent-space normal mapping with automatic triangle tangent/bitangent derivation.
  - Directional key/fill lighting with HDR Reinhard tone mapping and gamma correction.

- **Multi-Channel PBR Layer Stack:**
  - Supports **Paint Layers** (freeform 3D brush painting) and **Fill Layers** (uniform material values).
  - Channels per layer:
    - **Base Color** (sRGB / Linear albedo)
    - **Metallic** (Dielectric to metal scale)
    - **Roughness** (Glossy to rough matte scale)
    - **Normal** (Tangent-space surface detailing)
  - Layer blend modes: `Normal`, `Multiply`, `Screen`, `Overlay`, `LinearAdd`, and **Reoriented Normal Mapping (RNM)** for tangent normals.
  - Per-channel masking and layer opacity control.

- **Interactive 3D Paint Engine:**
  - Screen-to-world raycasting with Möller–Trumbore triangle intersection and barycentric UV interpolation.
  - Smooth brush stamping with adjustable **Radius**, **Hardness** (cosine falloff), **Opacity**, and **Flow**.
  - Dual modes: **Paint** and **Eraser**.

- **Built-in Geometry & Custom Model Ingestion:**
  - Procedural PBR test primitives: **UV Material Sphere**, **Cube**, and **Torus**.
  - Wavefront `OBJ` model loader (via `tinyobjloader`) with automated UV verification.

- **1-Click PBR Texture Exporter:**
  - Exports composited maps to standard PNG formats:
    - `<Prefix>_BaseColor.png`
    - `<Prefix>_Normal.png`
    - `<Prefix>_Roughness.png`
    - `<Prefix>_Metallic.png`
    - `<Prefix>_MetallicRoughness.png` (glTF 2.0 standard packed format: G = Roughness, B = Metallic)

- **Substance-Inspired Dark UI:**
  - Powered by Dear ImGui Docking with full multi-panel docking support, layer stack reordering, property inspectors, and status bar.

---

## Architecture Overview

```
REMoAINTER/
├── CMakeLists.txt              # Cross-platform build definition (Linux & Windows)
├── run.sh                      # Shell launcher
├── shaders/
│   ├── pbr.vert                # 3D Mesh vertex shader (TBN matrix, world position)
│   └── pbr.frag                # Cook-Torrance GGX PBR fragment shader
├── src/
│   ├── main.cpp                # Application entry point
│   ├── core/
│   │   ├── Engine.hpp/cpp      # Vulkan swapchain, pipeline, and render loop
│   │   └── Camera.hpp/cpp      # Orbit/Pan/Zoom camera with screen raycasting
│   ├── graphics/
│   │   ├── Mesh.hpp/cpp        # Vertex layouts, primitives, OBJ loader
│   │   └── Texture.hpp/cpp     # Vulkan image/view/sampler abstractions
│   ├── painting/
│   │   ├── Brush.hpp           # PBR brush parameters (size, hardness, channels)
│   │   ├── Layer.hpp/cpp       # Multi-channel texture layer buffers & stamping
│   │   └── LayerStack.hpp/cpp  # Layer compositor with PBR blend modes
│   ├── ui/
│   │   ├── UIManager.hpp/cpp   # Dockspace, layers panel, properties inspector
│   │   └── Theme.hpp           # Dark theme styling
│   └── io/
│       └── TextureExporter.hpp/cpp # PBR texture map PNG exporter
└── third_party/
    ├── imgui/                  # Dear ImGui (Docking branch)
    ├── stb/                    # stb_image.h & stb_image_write.h
    └── tinyobjloader/          # tiny_obj_loader.h
```

---

## Building and Running

### Linux (CachyOS / Arch / Ubuntu / Fedora)

Prerequisites:
- Vulkan SDK / Vulkan Loader headers
- GLFW3 (`glfw` or `glfw-wayland`)
- GLM (`glm`)
- CMake 3.20+ & Ninja / GCC or Clang

```bash
# Build
cmake -B build -S . -G Ninja -DCMAKE_BUILD_TYPE=Release
ninja -C build

# Run
./build/remoainter
# Or via global command
remoainter
```

### Windows (MSVC 2022 / Clang-cl)

Prerequisites:
- Visual Studio 2022 with C++ Desktop Development
- LunarG Vulkan SDK
- CMake 3.20+

```cmd
:: Configure and build
cmake -B build -S . -G "Visual Studio 17 2022" -A x64
cmake --build build --config Release

:: Run
.\build\Release\remoainter.exe
```

---

## Controls and Keybindings

| Action | Control |
| :--- | :--- |
| **Orbit Camera** | `Alt` + `Left Mouse Button` (drag) |
| **Pan Camera** | `Alt` + `Middle Mouse Button` (drag) |
| **Zoom Camera** | `Alt` + `Right Mouse Button` (drag) or `Scroll Wheel` |
| **Paint on Mesh** | `Left Mouse Button` (click & drag on 3D model) |
| **Switch to Eraser** | Toolbar radio button or shortcut |
| **Export PBR Textures** | `Ctrl + E` or `File > Export PBR Textures...` |
