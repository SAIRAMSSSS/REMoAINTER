# REMoAINTER

<p align="center">
  <img src="https://raw.githubusercontent.com/SAIRAMSSSS/REMoAINTER/master/assets/branding/logo.png" alt="REMoAINTER Logo" width="180"/>
</p>

<h3 align="center">Open-Source 3D PBR Texture Painting Suite</h3>
<p align="center">
  <b>Version 0.2 </b> • Industry-Standard PBR Painting Workflow • Raytraced Baking • Black Masks • Anchor Points
</p>

---

## What is REMoAINTER?

**REMoAINTER** is an open-source, industry-aligned 3D physically based rendering (PBR) texture painting application engineered to deliver the feel, workflow, and capabilities of a from-scratch professional PBR painting studio. 

Version `v0.2` introduces a high-performance **Rust Engine** designed from the ground up for safe parallel execution, multi-core raytraced mesh map baking, non-destructive layer sub-stacks, black masks, anchor points, and seamless UV seam boundary painting.

---

## Key Features in v0.2

### 1. 3D PBR Painting UI
- **Dark Theme**: Built with a custom color palette (`#212121`, `#242424`, `#1A4B75`, `#1A8DFF`, `#EF7E22`).
- **Complete Panel Architecture**:
  - **Left Tool Strip**: Paint (`1`), Eraser (`2`), Projection (`3`), Polygon Fill (`4`), Smudge (`5`), Clone (`6`), Eyedropper (`I`).
  - **Context Control Bar**: Brush radius, flow %, opacity %, spacing %, seam wrapping, symmetry, and render channel switcher (Material, Base Color, Roughness, Metallic, Normal, AO, Curvature).
  - **Central 3D Viewport**: Interactive camera orbit (`Alt + Left Click`), pan (`Alt + Middle Click`), and zoom (`Alt + Right Click`).
  - **Shelf / Asset Browser**: Organized tabs for Smart Materials (Orange `SM`), Materials (Blue `M`), Smart Masks, Alphas, and Textures with sphere preview swatches.
  - **Texture Set List**: Manage multiple meshes and texture set resolutions (1024x1024, 2048x2048, 4096x4096).
  - **Layer Stack**: Multi-layer compositing, blend modes, opacity, mask thumbnails, and anchor indicators.
  - **Properties Panel**: Direct control over active tool dynamics and PBR channels (Base Color, Roughness, Metallic, Normal).

### 2. Multi-Threaded Mesh Maps Baking Engine
Rayon-accelerated CPU raytracing for fast, GPU-agnostic map generation:
- **Ambient Occlusion (AO)**: Cosine-weighted hemispherical ray casting (8–128 rays/pixel) with ray-triangle occlusion testing.
- **Curvature Map**: Laplacian neighbor normal variance detecting surface edges and cavities.
- **World Space Normals**: Surface orientation mapped to vector color space.
- **Tangent Space Normals**: Base surface tangent space mapping.
- **Position Map**: Normalized object bounding box coordinates for vertical dirt gradients.
- **Thickness Map**: Inverted normal raycasting for subsurface scattering.
- **ID Map**: Polygon / material group color ID generation.
- **Selective & Batch Baking**: Bake all maps with one click or re-bake individual maps on demand.

### 3. Black Masks & Layer Sub-Stack
- **Mask Types**: Full White Mask and Black Mask.
- **Procedural Effect Sub-Stack**:
  - **Fill Effect**: Procedural noise and patterns.
  - **Paint Effect**: Freeform brush painting inside the mask.
  - **Generator Effect**: Dynamic edge wear and crevice dirt driven by baked Curvature and AO maps.
  - **Levels Effect**: Black in, white in, midtone gamma, black out, and white out adjustments.
  - **Invert Effect**: Fast one-click mask inversion.

### 4. Anchor Points System
- Define named anchor points (`AnchorPoint`) on any layer or mask.
- Reference anchor points in higher-level layers or generators to create dynamic, interconnected wear patterns.

### 5. Toroidal UV Seam Wrapping
- Eliminates unsightly seams when painting across 0.0–1.0 UV boundary edges by dynamically wrapping brush kernels toroidally.

### 6. Native Rust / Vulkan Hardware PBR Viewport (`wgpu`)
- **24-bit Hardware Depth Buffer**: Full Vulkan `Depth24Plus` with `LessEqual` depth sorting—eliminating triangle flickering and ordering artifacts on complex overlapping geometry (lantern glass, burner, frame bars).
- **Cook-Torrance Microfacet GGX BRDF**:
  - Specular distribution: Trowbridge-Reitz $D_{GGX}$
  - Shadowing-masking: Smith-Schlick $G_{Smith}$
  - Fresnel: Schlick approximation with roughness attenuation
  - Tangent-space TBN normal mapping using per-vertex MikkTSpace orthogonal tangents
- **Real-Time Dynamic Lighting**: Per-fragment specular and Fresnel reflections calculated every frame that respond interactively to camera orbit and light rotation.
- **Display Channels**: Instant GPU shader uniform solo switching between Lit Material, Base Color, Roughness, Metallic, Normal, AO, and Curvature.

### 7. Real HDR Image-Based Lighting (IBL) & Studio Probes
- **OpenEXR High Dynamic Range Probes**: Directly loads authentic 32-bit floating-point panoramic `.exr` environments (`Studio Tomoco.exr`, `Soft 1Front.exr`, etc.) from `Studio/`.
- **Numerical Split-Sum Integration LUT**: Precomputes a 512x512 RG16F BRDF integration LUT via Hammersley importance sampling for accurate ambient specular response.
- **Panoramic Skybox**: Interactive background rendering with blur filter and intensity multiplier.
- **ACES Filmic Tone Mapping**: Industry-standard Academy Color Encoding System (ACES) filmic S-curve with gamma 2.2 correction into linear sRGB display space.

### 8. Real Material Pipeline & Texture Set Importer
- **SBSAR & SPSM Batch Exporter (`tools/export_sbsar.py`)**:
  - Utilizes the `sbsrender` CLI to batch-render `.sbsar` procedural graphs into 2048x2048 PBR texture maps (BaseColor, Normal, Roughness, Metallic, Height, AO) with `material.json` manifests.
  - Graceful fallback with clear instructions when `sbsrender` is not installed; extracts bundled textures from `.spsm` smart materials.
- **In-App PBR Texture Set Importer & Drag-and-Drop**:
  - Drag and drop any texture pack folder or multiple files onto the window to automatically match channels.
  - Suffix detection for ambientCG, Poly Haven, and reference conventions (`_albedo`, `_nor_gl`, `_nor_dx`, `_roughness`, `_metallic`, `_ao`, `_height`).
  - Automatic DirectX to OpenGL normal map Y-inversion.
  - Dedicated interactive modal dialog for manual channel assignments and unmatched fallback maps.

### 9. Bundled Test Assets & Starter Materials
- **Antique Lantern** (`assets/test_models/lantern.obj`): High-detail multi-part mesh for testing metal, glass, and curvature wear.
- **Retro Radio** (`assets/test_models/radio.obj`): Complex mesh with knobs, grilles, and antennas for ambient occlusion and crevice testing.
- **PBR Material Sphere**: Standard sphere for look-dev.
- **10 Core Real PBR Materials**: Aluminium Anodized Red, Brass Antique, Carbon Fiber, Concrete Wall, Copper Oxidized, Gold Pure, Iron Rough, Leather Dark, Steel Painted Scraped Green, Wood Walnut.

---

## Architecture & Implementation Status

REMoAINTER is implemented in **100% safe Rust** using the `wgpu` ecosystem for native Vulkan (Linux/Windows), Metal (macOS), and DirectX 12 (Windows) hardware acceleration.

### Implementation Status Matrix

| Subsystem | Implemented Status | Details |
| :--- | :---: | :--- |
| **3D Hardware Viewport** | **WORKING** | Pure WGSL rendering pipelines (`wgpu`): Cook-Torrance GGX microfacet BRDF, Karis split-sum IBL with 512×512 LUT, equirectangular HDR skyboxes, 24-bit depth buffer, wireframe overlay, ground grid with RGB axes. |
| **Multi-Channel Painting** | **WORKING** | Real-time painting into BaseColor, Metallic, Roughness, Normal, and Height buffers with UV seam wrapping, LazyMouse stabilizer, and symmetry. |
| **Layer Stack & Blending** | **WORKING** | Non-destructive layers with blend modes (Normal, Multiply, Screen, Overlay, Add, Subtract, Darken, Lighten, Color Dodge, Linear Dodge), opacity, folder hierarchies, and clipping masks. |
| **Black Masks & Generators** | **WORKING** | Procedural mask generator effects driven by baked Curvature (edge wear) and Ambient Occlusion (crevices/dirt), plus Levels and Invert effects. |
| **Mesh Maps Baking** | **WORKING** | Multi-threaded CPU raytracing baker via Rayon & BVH: AO (cosine-weighted hemispherical rays), Curvature (Laplacian normal variance), World Space Normals, Tangent Space Normals, Position, Thickness, and ID maps. |
| **Asset Browser & Shelf** | **WORKING** | Live discovery of bundled PBR materials (`assets/materials/`), starter smart materials, procedural smart masks, brush alphas, and HDR environment probes. |
| **Texture Import & Export** | **WORKING** | Drag-and-drop PBR texture sets with auto-channel matching (ambientCG / Poly Haven); presets for Unreal Engine (ORM), Unity (HDRP/URP MaskMap), and glTF 2.0 with UV gutter dilation. |
| **Project Persistence** | **WORKING** | Complete save/load of `.rmp` project archives containing JSON manifests, layer channel PNGs, and baked maps. |
| **Procedural Texture Graph** | **PARTIAL** | Core node graph engine (`painting/graph.rs`) with noise, gradients, and blend math; integrated in procedural material dialog. |
| **SBSAR / SPSM Import** | **PARTIAL** | Package archive parsing and preset extraction via `sbsar.rs` and `sevenz-rust`; batch procedural graph rendering delegates to `tools/export_sbsar.py`. |
| **UDIM Multi-Tile Workflow** | **EXPERIMENTAL** | UDIM tile coordinate math and detection implemented; active editing currently operates on primary tile. |
| **GPU Compute Baking** | **PLANNED** | Hardware compute shader baking pipeline planned for future milestone. |

---

## Directory Structure

```
REMoAINTER/
├── assets/                 # Shipped application assets
│   ├── branding/           # Logos and application icons
│   ├── environments/       # Bundled HDR/PNG equirectangular environment maps
│   ├── fonts/              # Bundled TrueType and OpenType fonts
│   ├── materials/          # 10 core PBR materials with 1024x1024 texture maps
│   └── test_models/        # Sample 3D meshes (lantern.obj, radio.obj, sphere.obj)
├── docs/                   # Technical documentation
│   ├── ARCHITECTURE.md     # Engineering architecture specification
│   ├── ENGINEERING_AUDIT.md # Comprehensive engineering audit & verification report
│   └── FEATURE_AUDIT.md    # Detailed capability audit
├── run.sh                  # One-click desktop launcher script (sets LC_ALL UTF-8)
├── rust/                   # Rust implementation (authoritative codebase)
│   ├── Cargo.toml          # Rust dependencies and configuration
│   └── src/
│       ├── baker/          # Multithreaded CPU raytracing baking engine
│       ├── graphics/       # Native Vulkan/wgpu viewport, WGSL shaders, mesh geometry
│       ├── painting/       # Layer stack, black masks, brush engine, PBR texture sets
│       ├── paths/          # Machine-independent asset discovery
│       ├── project/        # .rmp project archive serialization & user config
│       ├── render/         # Software path tracing look-dev verification engine
│       ├── ui/             # egui-wgpu UI panels, tools, shelf browser, and theme
│       ├── lib.rs          # Core library interface
│       └── main.rs         # Desktop application entry point
└── tools/
    ├── export_sbsar.py     # SBSAR & SPSM batch exporter CLI
    └── generate_core_pbr_materials.py # Starter PBR material generator
```

---

## Building & Running

### Prerequisites
- Rust 1.80+ (with Cargo)
- Wayland / X11 development headers (for Linux desktop display)

### Quick Start
```bash
# 1. Build the release binary
cd rust
cargo build --release
cd ..

# 2. Launch the application (launcher handles UTF-8 locale configuration)
./run.sh

# Or launch the compiled binary directly:
LC_ALL=C.UTF-8 ./rust/target/release/remoainter
```

### Shortcuts
| Shortcut | Action |
| :--- | :--- |
| `1` | Paint Brush Tool |
| `2` | Eraser Tool |
| `3` | Projection Tool |
| `4` | Polygon Fill Tool |
| `5` | Smudge Tool |
| `6` | Clone Stamp Tool |
| `I` | Eyedropper |
| `F1` | Split 3D & 2D Viewport |
| `F2` | 3D PBR Viewport |
| `F3` | 2D UV Texture Canvas |
| `F4` | Toggle Wireframe Overlay |
| `M` | Solo Lit Material Channel |
| `C` | Solo Base Color Channel |
| `B` | Solo Ambient Occlusion Channel |
| `Ctrl + N` | New Project |
| `Ctrl + Shift + B` | Open Mesh Maps Baker Dialog |
| `Alt + Left Drag` | Orbit Viewport Camera (moves highlights) |
| `Alt + Middle Drag` | Pan Camera |
| `Alt + Right Drag / Wheel` | Zoom Camera |
| `Shift + Right Drag` | Rotate Studio Environment Light |

### Batch SBSAR & Smart Material Exporter
To render `.sbsar` procedural graphs into 2048x2048 PNG PBR texture sets with `material.json` manifests:
```bash
# Using the sbsrender CLI
python3 tools/export_sbsar.py --resolution 2048

# Or with custom directories
python3 tools/export_sbsar.py --sbsar-dir /path/to/materials --out-dir assets/materials
```
If `sbsrender` is not installed on your system, the exporter will print clear setup instructions and proceed to index all `.spsm` smart materials and bundled texture sets.

---
