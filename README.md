# REMoAINTER

<p align="center">
  <img src="https://raw.githubusercontent.com/SAIRAMSSSS/REMoAINTER/master/assets/branding/logo.png" alt="REMoAINTER Logo" width="180"/>
</p>

<h3 align="center">Next-Generation Open-Source 3D PBR Texture Painting Suite</h3>
<p align="center">
  <b>Version 2.0.0 (Rust High-Performance Core)</b> • 1-to-1 Substance Painter Workflow • Raytraced Baking • Black Masks • Anchor Points
</p>

---

## What is REMoAINTER?

**REMoAINTER** is an open-source, industry-aligned 3D physically based rendering (PBR) texture painting application engineered to deliver the feel, workflow, and capabilities of Adobe Substance 3D Painter. 

Version `v2.0.0` introduces a high-performance **Rust Engine** designed from the ground up for safe parallel execution, multi-core raytraced mesh map baking, non-destructive layer sub-stacks, black masks, anchor points, and seamless UV seam boundary painting.

---

## Key Features in v2.0.0

### 1. Substance 3D Painter Parity UI
- **Authentic Allegorithmic Dark Theme**: Built with identical color palettes (`#212121`, `#242424`, `#1A4B75`, `#1A8DFF`, `#EF7E22`).
- **Complete Panel Architecture**:
  - **Left Tool Strip**: Paint (`1`), Eraser (`2`), Projection (`3`), Polygon Fill (`4`), Smudge (`5`), Clone (`6`), Eyedropper (`I`).
  - **Context Control Bar**: Brush radius, flow %, opacity %, spacing %, seam wrapping, symmetry, and render channel switcher (Material, Base Color, Roughness, Metallic, Normal, AO, Curvature).
  - **Central 3D Viewport**: Interactive camera orbit (`Alt + Left Click`), pan (`Alt + Middle Click`), and zoom (`Alt + Right Click`).
  - **Shelf / Asset Browser**: Organized tabs for Smart Materials (Orange `SM`), Materials (Blue `M`), Smart Masks, Alphas, and Textures with authentic sphere preview swatches.
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

### 6. Bundled Test Assets
- **Antique Lantern** (`assets/test_models/lantern.obj`): High-detail multi-part mesh for testing metal, glass, and curvature wear.
- **Retro Radio** (`assets/test_models/radio.obj`): Complex mesh with knobs, grilles, and antennas for ambient occlusion and crevice testing.
- **PBR Material Sphere**: Standard sphere for look-dev.

---

## Directory Structure

```
REMoAINTER/
├── assets/
│   ├── branding/           # REMoAINTER logos and icons
│   └── test_models/        # Test assets (lantern.obj, radio.obj)
├── docs/
│   └── v2.0.0_ARCHITECTURE.md # Full architecture specification
├── rust/                   # Rust v2.0.0 implementation
│   ├── Cargo.toml
│   └── src/
│       ├── baker/          # Rayon multithreaded baking engine
│       │   ├── engine.rs
│       │   └── mod.rs
│       ├── graphics/       # Wavefront OBJ and mesh processing
│       │   ├── mesh.rs
│       │   └── mod.rs
│       ├── painting/       # Layer stack, black masks, brush, anchors
│       │   ├── anchor.rs
│       │   ├── brush.rs
│       │   ├── layer.rs
│       │   ├── mask.rs
│       │   └── mod.rs
│       ├── ui/             # egui / eframe Substance Painter UI
│       │   ├── app.rs
│       │   ├── theme.rs
│       │   └── mod.rs
│       └── main.rs
├── src/                    # C++20 / Vulkan engine (v1.x legacy core)
└── CMakeLists.txt
```

---

## Building & Running

### Prerequisites
- Rust 1.80+ (with Cargo)
- Wayland / X11 development headers (for Linux desktop display)

### Quick Start (Rust v2.0.0)
```bash
cd rust
cargo build --release
./target/release/remoainter
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
| `Ctrl + N` | New Project |
| `Ctrl + Shift + B` | Open Mesh Maps Baker Dialog |
| `Alt + Left Drag` | Orbit Viewport Camera |

---

## License
REMoAINTER is licensed under MIT OR Apache-2.0.
All logos, branding, and original code copyright © 2026 REMoAINTER Contributors.
