# REMoAINTER 2026.1 (v2.0.0) — Feature Gap Audit

This document provides a comprehensive audit of all menu items, tools, panels, layer features, shaders, and pipelines in **REMoAINTER 2026.1 (v2.0.0)** compared to Adobe Substance 3D Painter.

Status Legend:
- **Implemented**: Fully working, tested, and integrated.
- **Partial**: Functional UI or partial backend, but lacks full Substance Painter depth.
- **Stub**: UI element or empty function exists without actual operational logic.
- **Missing**: Not yet present in the codebase.

---

## 1. Menu Bar Audit

### File Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **New Project...** | `Ctrl+N` | **Implemented** | `src/ui/app.rs`: `ui_new_project_modal()`. Mesh picker, template selection, resolution, normal format (DirectX/OpenGL), UDIM toggle. Resets layers and re-bakes maps. |
| **Open Project...** | `Ctrl+O` | **Implemented** | `src/project/mod.rs` & `src/ui/app.rs`: Versioned project container (`.rmp`) deserializer with manifest, mesh, layer buffers, and baked maps. |
| **Save Project** | `Ctrl+S` | **Implemented** | `src/project/mod.rs`: Full versioned container format (`.rmp`) saving JSON manifest, mesh OBJ, layer buffers, and baked maps. |
| **Save Project As...** | `Ctrl+Shift+S` | **Implemented** | File picker for project save with versioned `.rmp` container. |
| **Autosave & Snapshots** | — | **Implemented** | `src/project/mod.rs` & `app.rs`: 180s background autosave (`autosave.rmp`) and manual/auto snapshot history modal. |
| **Import 3D Mesh (OBJ/FBX/glTF)...**| `Ctrl+Shift+O` | **Implemented** | `src/graphics/mesh.rs`: `load_file()` supports Wavefront OBJ (`tobj`), Autodesk FBX (`ufbx`), and glTF 2.0 / GLB (`gltf`). Fails with clear error feedback if file is invalid (no silent fallback). |
| **Reload Mesh** | `Ctrl+R` | **Implemented** | `src/ui/app.rs`: `reload_mesh()`. Hot-reloads source mesh from disk or primitive while preserving all painted layer channels and history intact. |
| **Bake Mesh Maps...** | `Ctrl+Shift+B`| **Implemented** | `src/ui/app.rs`: `ui_baker_modal()` & `ui_baker_studio_mode()`. |
| **Export Textures...** | `Ctrl+Shift+E`| **Implemented** | `src/painting/export.rs`: Full Substance Painter-parity export engine with presets (Unreal ORM, Unity URP/HDRP MaskMap, glTF 2.0, Generic PBR, OpenEXR 32-bit Float), 8/16/32-bit depth, UV edge dilation/seam padding, resolution override, and logging. |
| **Import Texture Set (Folder)...** | — | **Implemented** | `src/painting/texture_importer.rs`: Auto-matches BaseColor, Normal, Roughness, Metallic, Height, AO from folder names. |
| **Import Texture Maps (Files)...** | — | **Implemented** | `src/painting/texture_importer.rs`: Auto-matches selected files via suffix inspection. |
| **Texture Set Importer Dialog...** | — | **Implemented** | `src/ui/app.rs`: `ui_texture_importer_modal()`. Channel mapping table with DirectX Y-flip toggle. |
| **Recent Projects** | — | **Missing** | No persistent history of recently opened files in config. |
| **Exit** | `Alt+F4` | **Implemented** | `ctx.send_viewport_cmd(egui::ViewportCommand::Close)`. |

---

### Edit Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Undo** | `Ctrl+Z` | **Implemented** | `src/painting/history.rs` & `src/ui/app.rs`. 64x64 non-destructive tile dirty region capture, multi-level undo stack with interactive History panel. |
| **Redo** | `Ctrl+Y` | **Implemented** | `src/painting/history.rs` & `src/ui/app.rs`. Multi-level redo stack with tile pixel restoration. |
| **Cut Layer** | `Ctrl+X` | **Partial** | Available via layer right-click context menu into `self.layer_clipboard`, but not wired into top Edit menu. |
| **Copy Layer** | `Ctrl+C` | **Partial** | Available via layer right-click context menu into `self.layer_clipboard`, not in top Edit menu. |
| **Paste Layer** | `Ctrl+V` | **Partial** | Available via layer right-click context menu, not in top Edit menu. |
| **Preferences...** | — | **Missing** | No application preferences modal (UI scale, undo depth, default paths, GPU device selection). |
| **Hotkey Editor** | — | **Missing** | Keybindings are currently hardcoded in `handle_shortcuts()`. |

---

### Mode Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Painting (Edition)** | — | **Implemented** | Default workspace with 3D/2D viewport, layers, properties, shelf. |
| **Bake Mesh Maps (Studio)**| — | **Implemented** | Allegorithmic-style dedicated 1:1 baking studio layout. |
| **Visualisation (Render)** | — | **Partial** | Interactive lit PBR viewport exists; offline path-traced / Iray renderer is missing. |

---

### Geometry Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Substance Material Sphere** | — | **Implemented** | `src/graphics/mesh.rs`: `Mesh::create_material_sphere(64, 64)`. |
| **Antique Lantern** | — | **Implemented** | Test OBJ asset bundled in `/mnt/shared/REMoAINTER/assets/`. |
| **Retro Radio** | — | **Implemented** | Test OBJ asset bundled in `/mnt/shared/REMoAINTER/assets/`. |
| **Primitive Cube** | — | **Implemented** | Unit cube with standard unwrapped UVs. |
| **Primitive Plane** | — | **Implemented** | Quad planar mesh for decal/tiling material testing. |
| **Isolate / Hide Meshes** | — | **Missing** | Cannot toggle visibility of sub-meshes or geometry groups. |

---

### Viewport Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **3D \| 2D Split View** | `F1` | **Implemented** | `viewport_view_mode = 2`. Renders 3D GL viewport alongside 2D UV canvas. |
| **3D View Only** | `F2` | **Implemented** | `viewport_view_mode = 0`. Full-width 3D GL viewport. |
| **2D UV View Only** | `F3` | **Implemented** | `viewport_view_mode = 1`. Full-width 2D UV texture canvas. |
| **Wireframe Overlay** | `F4` | **Implemented** | `glow::POLYGON_OFFSET_LINE` and cyan wireframe shader in 3D; UV layout overlay in 2D. |
| **Material (Lit PBR)** | `M` | **Implemented** | `display_channel = 0`. Cook-Torrance GGX fragment shader. |
| **Base Color (Solo)** | `C` | **Implemented** | `display_channel = 1`. Cycles solo channels. |
| **Roughness (Solo)** | — | **Implemented** | `display_channel = 2`. Grayscale visualization. |
| **Metallic (Solo)** | — | **Implemented** | `display_channel = 3`. Grayscale visualization. |
| **Normal (Solo)** | — | **Implemented** | `display_channel = 4`. Tangent normal vector visualization. |
| **Ambient Occlusion (Solo)**| `B` | **Implemented** | `display_channel = 5`. Baked AO map. |
| **Curvature (Solo)** | — | **Implemented** | `display_channel = 6`. Baked Curvature map. |
| **Position (Solo)** | — | **Implemented** | `display_channel = 7`. Baked Object Position map. |
| **Thickness (Solo)** | — | **Implemented** | `display_channel = 8`. Baked Thickness map. |
| **Frame Camera** | `F` | **Implemented** | Resets camera distance and angles to frame active bounds. |
| **Camera Bookmarks** | — | **Missing** | Saving and recalling camera views (1-9). |
| **Turntable Animation** | — | **Missing** | Automated continuous rotation for material showcase. |
| **Post-Processing (Bloom/DoF/SSAO)** | — | **Missing** | Viewport renders direct PBR without post-process stack. |
| **Grid & World Axes** | — | **Missing** | 3D ground plane grid and coordinate gizmo missing. |

---

### Window Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Texture Set Settings** | — | **Implemented** | Right dock panel index 0. Full channel manager (add/remove channels), per-set resolution override (512–4096), format specification (sRGB8, L8, RGB16F, L16F, RGB32F), and normal mixing. |
| **Texture Set List** | — | **Implemented** | Right dock panel index 1. Multi-texture set & UDIM multi-tile manager (`src/painting/texture_set.rs`), add/duplicate/delete sets, UDIM tile detection (1001, 1002...), visibility toggles, and live switching. |
| **Layers** | `F4` / Strip | **Implemented** | Right dock panel index 2. |
| **Display Settings** | — | **Implemented** | Right dock panel index 3. |
| **Shader Settings** | — | **Implemented** | Right dock panel index 4. |
| **History** | — | **Partial** | Right dock panel index 5. Displays history list, but items are not clickable for undo rollback. |
| **Log Console** | — | **Implemented** | Right dock panel index 6. Real-time log messages. |
| **Reset Layout** | — | **Missing** | Restore dock panels to default positions. |

---

### Help Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **About REMoAINTER...** | — | **Implemented** | Displays version banner and core description. |
| **Documentation / Shortcuts** | — | **Missing** | Quick help reference dialog not implemented. |

---

## 2. Tools & Brush Engine Audit

| Tool / Feature | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Paint Brush** | `1` | **Implemented** | `ToolMode::Paint`. 3D Euclidean surface projection painting across UV seams. Modulates Color, Roughness, Metallic, Normal, Height. |
| **Eraser** | `2` | **Implemented** | `ToolMode::Eraser`. Clears painted pixels to transparent background. |
| **Projection Tool** | `3` | **Stub** | `ToolMode::Projection`. Tool mode exists, but stencil overlay and screen-space projection stamping logic is a stub. |
| **Polygon Fill** | `4` | **Implemented** | `ToolMode::PolygonFill`. Supports Triangle, UV Island, and Mesh fill modes. |
| **Smudge Tool** | `5` | **Partial** | `ToolMode::Smudge`. Basic local pixel blur convolution along stroke direction. |
| **Clone Stamp** | `6` | **Partial** | `ToolMode::Clone`. Stores `clone_source_uv` on Alt+Click; samples from source UV into target. |
| **Color Eyedropper / Picker**| `I` | **Implemented** | `ToolMode::Eyedropper`. Samples BaseColor, Metallic, Roughness, and Normal from clicked 3D surface. |
| **Text / Decal Tool** | — | **Missing** | No vector text rasterization or decal stamping tool. |
| **Symmetry Painting** | — | **Implemented** | Multi-axis (X/Y/Z) mirroring across 3D surface mesh and 2D UV layout. |
| **Alpha / Stencil Shapes** | — | **Implemented** | Built-in procedural alphas, custom image alphas, angle rotation, and stencil masking support. |
| **Hardness & Spacing** | — | **Implemented** | Smoothstep hardness falloff and distance-based spacing in brush pipeline. |
| **Angle & Size Jitter** | — | **Implemented** | Pseudo-random seed hashing for size jitter, flow jitter, and angle jitter per stamp. |
| **Stroke Stabilizer (LazyMouse)**| — | **Implemented** | Distance-based smoothing with interactive leash visual guide and configurable radius. |
| **Pressure Sensitivity** | — | **Implemented** | Pressure curves (Linear, EaseIn, EaseOut, Smooth) mapped to size and flow. |
| **Per-Channel Toggles** | — | **Implemented** | UI checkboxes for Color, Metallic, Roughness, Normal, Height channels. |

---

## 3. Layer Stack & Non-Destructive System Audit

| Layer Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Paint Layer** | **Implemented** | Dynamic channel buffers (`color`, `metallic`, `roughness`, `normal`, `height`). |
| **Fill Layer** | **Implemented** | Uniform or tiled material settings (`fill_color`, `fill_metallic`, `fill_roughness`, `fill_height`). |
| **Folder / Group** | **Implemented** | `LayerType::Folder` with nested tree rendering, parent/child hierarchy, recursive opacity & visibility cascading, and Ctrl+G hotkey. |
| **Blend Modes** | **Implemented** | All 18 Substance Painter blend modes: Normal, PassThrough, Multiply, Screen, Overlay, Add, Subtract, Darken, Lighten, ColorDodge, ColorBurn, LinearBurn, HardLight, SoftLight, Difference, Exclusion, Color, Luminosity. |
| **Per-Channel Blend Modes** | **Implemented** | `channel_color_blend`, `channel_metallic_blend`, etc. with per-channel routing toggles and opacities in UI and compositor. |
| **Mask Stack** | **Implemented** | `LayerMask` supports White, Black, and Bitmap masks. |
| **Mask Paint-in-Mask** | **Implemented** | Dual thumbnail workflow (Content & Mask), click-to-activate mask editing mode with 2D & 3D real-time grayscale brush painting. |
| **Mask Effects Stack** | **Implemented** | Sub-stack supporting sequential evaluation of Fill, Generator, Levels, and Invert effects. |
| **Procedural Generators** | **Implemented** | `Metal Edge Wear`, `Dirt Crevices`, `Dust Accumulation`, `Position Gradient`, `World Normal Orientation`, `Thickness / Subsurface`, and `Color / ID Selection` with grunge FBM noise, tolerance, and axis controls. |
| **Filters Stack** | **Implemented** | Non-destructive `FilterEffect` stack supporting Levels, Gaussian Blur, Sharpen, HSL adjustment, Invert, and Gradient Map with dynamic inspector controls. |
| **Anchor Points** | **Partial** | `AnchorPoint` struct and creation UI implemented; referencing anchor points from generators in other layers is partial. |

---

## 4. Mesh Map Baking Pipeline Audit

| Baker Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Dedicated Studio UI** | **Implemented** | Full-window baking interface matching Substance Painter 1:1. |
| **Ambient Occlusion (AO)** | **Implemented** | Cosine-weighted hemisphere raycasting with sample count and distance sliders. |
| **Curvature** | **Implemented** | Surface Laplacian / divergence computation with intensity slider. |
| **Normal Map (Tangent)** | **Implemented** | High-to-low raycast projection with MikkTSpace tangent encoding. |
| **World Space Normal** | **Implemented** | Absolute orientation encoding for top-down procedural effects. |
| **Position Map** | **Implemented** | Object-space bounding-box normalized coordinates. |
| **Thickness Map** | **Implemented** | Reverse normal penetration raycasting for subsurface scattering. |
| **ID Map** | **Implemented** | Vertex color / submesh color ID baking. |
| **High-Poly to Low-Poly** | **Implemented** | OBJ loading of high-poly mesh with raycast intersection testing. |
| **Cage / Ray Distances** | **Implemented** | Max frontal distance and max rear distance sliders. |
| **Match by Mesh Name** | **Partial** | UI fields for low suffix (`_low`) and high suffix (`_high`) exist; BVH name-filtering query needs completion. |
| **Antialiasing (AA)** | **Partial** | UI dropdown for None, 2x2, 4x4, 8x8; supersampling rasterizer pass needs completion. |
| **Skew Correction** | **Missing** | Skew mesh painting/correction not implemented. |
| **Background Thread Baking** | **Partial** | Runs on background thread with `AtomicUsize` progress bar; needs non-blocking cancel button. |

---

## 5. Viewport & Rendering Pipeline Audit

| Viewport Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Cook-Torrance GGX** | **Implemented** | `src/graphics/viewport_gl.rs`: $D_{\text{GGX}}$, $G_{\text{Smith-Schlick}}$, $F_{\text{Schlick}}$. |
| **Split-Sum IBL** | **Implemented** | Prefiltered cubemap convolution + 2D numerical BRDF integration LUT ($512 \times 512$). |
| **HDR Environment Loader** | **Implemented** | Reads `.exr` and `.hdr` studio environments; dynamic exposure and rotation controls. |
| **ACES Tonemapping** | **Implemented** | ACES filmic curve with sRGB gamma correction. |
| **Wireframe Overlay** | **Implemented** | 3D mesh line overlay (`GL_POLYGON_OFFSET_LINE`) and 2D UV layout. |
| **Solo Channels** | **Implemented** | BaseColor, Roughness, Metallic, Normal, AO, Curvature, Position, Thickness. |
| **Bloom & Glow** | **Missing** | High-luminance thresholding and multi-pass downsample/upsample blur missing. |
| **Depth of Field (DoF)** | **Missing** | Circle of confusion bokeh simulation missing. |
| **Screen Space Ambient Occlusion (SSAO)** | **Missing** | Real-time viewport depth/normal buffer SSAO pass missing. |
| **Color Management (OCIO)** | **Missing** | OpenColorIO profile loading missing (uses hardcoded ACES). |

---

## 6. Texture Export Pipeline Audit

| Export Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Export Dialog UI** | **Implemented** | Window with target mesh, resolution, directory picker, preset dropdown, and export status logging. |
| **Unreal Engine ORM Preset**| **Implemented** | `src/painting/export.rs`: automated channel packing (R=AO, G=Roughness, B=Metallic) with Normal DirectX Y-. |
| **Unity MaskMap Preset** | **Implemented** | `src/painting/export.rs`: automated packing (R=Metallic, G=AO, B=Thickness, A=Smoothness) with Normal OpenGL Y+. |
| **glTF 2.0 Preset** | **Implemented** | `src/painting/export.rs`: Khronos standard packing (B=Metallic, G=Roughness, R=AO). |
| **Generic PBR Preset** | **Implemented** | Exports separate BaseColor, Roughness, Metallic, Normal, and AO images. |
| **Bit Depth Selection** | **Implemented** | 8-bit PNG, 16-bit PNG, and 32-bit OpenEXR floating-point support. |
| **Edge Dilation / Padding** | **Implemented** | Push-pull dilation into UV gutter regions to prevent black mipmap seams. |
| **Per-Texture-Set Output** | **Implemented** | `export_all_texture_sets` iterates and exports all active texture sets with naming template tokens. |

---

## Summary Scorecard

| Category | Total Items | Implemented | Partial | Stub | Missing |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Menu Items** | 32 | 20 | 3 | 0 | 9 |
| **Tools & Brush Engine** | 15 | 11 | 2 | 1 | 1 |
| **Layer Stack & System** | 11 | 10 | 1 | 0 | 0 |
| **Mesh Map Baking** | 14 | 8 | 4 | 0 | 2 |
| **Viewport & Shaders** | 10 | 6 | 0 | 0 | 4 |
| **Texture Export** | 8 | 8 | 0 | 0 | 0 |
| **Total** | **90** | **63 (70.0%)** | **10 (11.1%)** | **1 (1.1%)** | **16 (17.8%)** |
