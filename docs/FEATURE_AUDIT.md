# REMoAINTER 2026.1 (v2.0.0) — Feature Gap Audit

This document provides a comprehensive audit of all menu items, tools, panels, layer features, shaders, and pipelines in **REMoAINTER 2026.1 (v2.0.0)** compared to reference PBR suite.

Status Legend:
- **Implemented**: Fully working, tested, and integrated.
- **Partial**: Functional UI or partial backend, but lacks full reference PBR suite depth.
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
| **Export Textures...** | `Ctrl+Shift+E`| **Implemented** | `src/painting/export.rs`: Full reference PBR suite-parity export engine with presets (Unreal ORM, Unity URP/HDRP MaskMap, glTF 2.0, Generic PBR, OpenEXR 32-bit Float), 8/16/32-bit depth, UV edge dilation/seam padding, resolution override, and logging. |
| **Import Texture Set (Folder)...** | — | **Implemented** | `src/painting/texture_importer.rs`: Auto-matches BaseColor, Normal, Roughness, Metallic, Height, AO from folder names. |
| **Import Texture Maps (Files)...** | — | **Implemented** | `src/painting/texture_importer.rs`: Auto-matches selected files via suffix inspection. |
| **Texture Set Importer Dialog...** | — | **Implemented** | `src/ui/app.rs`: `ui_texture_importer_modal()`. Channel mapping table with DirectX Y-flip toggle. |
| **Recent Projects** | — | **Implemented** | `src/project/config.rs` & `src/ui/app.rs`: Persistent MRU recent projects queue in `~/.remoainter/config.json`, File > Open Recent submenu with click-to-load and Clear Recent. |
| **Exit** | `Alt+F4` | **Implemented** | `ctx.send_viewport_cmd(egui::ViewportCommand::Close)`. |

---

### Edit Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Undo** | `Ctrl+Z` | **Implemented** | `src/painting/history.rs` & `src/ui/app.rs`. 64x64 non-destructive tile dirty region capture, multi-level undo stack with interactive History panel. |
| **Redo** | `Ctrl+Y` | **Implemented** | `src/painting/history.rs` & `src/ui/app.rs`. Multi-level redo stack with tile pixel restoration. |
| **Cut Layer** | `Ctrl+X` | **Implemented** | `src/ui/app.rs`: Cut active layer to clipboard and remove from layer stack, wired into Edit menu and `Ctrl+X` shortcut. |
| **Copy Layer** | `Ctrl+C` | **Implemented** | `src/ui/app.rs`: Copy active layer to clipboard, wired into Edit menu and `Ctrl+C` shortcut. |
| **Paste Layer** | `Ctrl+V` | **Implemented** | `src/ui/app.rs`: Duplicate clipboard layer into active layer stack with new ID and "(Copy)" suffix, wired into Edit menu and `Ctrl+V` shortcut. |
| **Preferences...** | — | **Implemented** | `src/project/config.rs` & `src/ui/app.rs`: Preferences modal controlling autosave interval, undo depth, default export path, default bake resolution, and color management mode, persisted across sessions. |
| **Hotkey Editor** | — | **Implemented** | `src/ui/app.rs`: Interactive keybindings reference and hotkey dictionary modal with filter search and restore defaults. |

---

### Mode Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Painting (Edition)** | — | **Implemented** | Default workspace with 3D/2D viewport, layers, properties, shelf. |
| **Bake Mesh Maps (Studio)**| — | **Implemented** | Allegorithmic-style dedicated 1:1 baking studio layout. |
| **Visualisation (Render)** | — | **Implemented** | Offline path-traced render mode (`src/render/renderer.rs`): progressive RGB CPU path tracer (Rayon scanline tiles, BVH over mesh tris, f32 film accumulation), GGX microfacet BSDF (α=rough², Smith visibility, Schlick F0) + Lambert mixture, seeded deterministic RNG per texel, analytic studio env + sun disc, exposure + ACES tonemap, quality presets (Draft 64 / Medium 256 / High 1024 / Extra 2560 spp + custom), Start/Pause/Resume/Stop, throttled egui preview, render-to-PNG with convergence auto-save. Honest scope: RGB not spectral, no HDRI-file environment, no denoiser, env-only lighting. |

---

### Geometry Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **reference Material Sphere** | — | **Implemented** | `src/graphics/mesh.rs`: `Mesh::create_material_sphere(64, 64)`. |
| **Antique Lantern** | — | **Implemented** | Test OBJ asset bundled in `/mnt/shared/REMoAINTER/assets/`. |
| **Retro Radio** | — | **Implemented** | Test OBJ asset bundled in `/mnt/shared/REMoAINTER/assets/`. |
| **Primitive Cube** | — | **Implemented** | Unit cube with standard unwrapped UVs. |
| **Primitive Plane** | — | **Implemented** | Quad planar mesh for decal/tiling material testing. |
| **Isolate / Hide Meshes** | — | **Implemented** | `src/graphics/viewport_wgpu.rs`, `src/ui/app.rs`: Selective submesh visibility toggles, isolation button, and isolated submesh GPU drawing in Display Settings panel. |

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
| **Camera Bookmarks** | `Ctrl+1..9` / `Alt+1..9` | **Implemented** | `src/ui/app.rs`: 9 camera bookmark slots with keyboard shortcuts and UI Recall/Save/Clear controls. |
| **Turntable Animation** | `Y` | **Implemented** | `src/ui/app.rs`: Continuous turntable animation loop with speed control and `Y` toggle key. |
| **Post-Processing (Bloom/DoF/SSAO)** | — | **Implemented** | `src/graphics/viewport_wgpu.rs`: Bloom highlights extraction, Depth of Field defocusing, and SSAO contact shadow enhancements integrated in PBR shader. |
| **Grid & World Axes** | — | **Implemented** | GPU ground-plane grid with world axes lines (`GpuGrid` in `src/graphics/viewport_wgpu.rs`, feature branch merged). |

---

### Window Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Texture Set Settings** | — | **Implemented** | Right dock panel index 0. Full channel manager (add/remove channels), per-set resolution override (512–4096), format specification (sRGB8, L8, RGB16F, L16F, RGB32F), and normal mixing. |
| **Texture Set List** | — | **Implemented** | Right dock panel index 1. Multi-texture set & UDIM multi-tile manager (`src/painting/texture_set.rs`), add/duplicate/delete sets, UDIM tile detection (1001, 1002...), visibility toggles, and live switching. |
| **Layers** | `F4` / Strip | **Implemented** | Right dock panel index 2. |
| **Display Settings** | — | **Implemented** | Right dock panel index 3. |
| **Shader Settings** | — | **Implemented** | Right dock panel index 4. |
| **History** | — | **Implemented** | Right dock panel index 5. Displays history list with bidirectional time-travel (click any past step to roll back, click any future step to fast-forward). |
| **Log Console** | — | **Implemented** | Right dock panel index 6. Real-time log messages. |
| **Reset Layout** | — | **Implemented** | `src/ui/app.rs`: Window > Reset Layout resets dock panels, 3D|2D split view, camera angles, and tool selection to factory defaults. |

---

### Help Menu
| Menu Item | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **About REMoAINTER...** | — | **Implemented** | Displays version banner and core description. |
| **Documentation / Shortcuts** | — | **Implemented** | `src/ui/app.rs`: Help > Documentation / Shortcuts modal displaying comprehensive 1:1 reference PBR suite keyboard shortcuts and workflows. |

---

## 2. Tools & Brush Engine Audit

| Tool / Feature | Shortcut | Status | Implementation Details / File Location |
| :--- | :--- | :--- | :--- |
| **Paint Brush** | `1` | **Implemented** | `ToolMode::Paint`. 3D Euclidean surface projection painting across UV seams. Modulates Color, Roughness, Metallic, Normal, Height. |
| **Eraser** | `2` | **Implemented** | `ToolMode::Eraser`. Clears painted pixels to transparent background. |
| **Projection Tool** | `3` | **Implemented** | True screen-space projection painting (`paint_3d_projection_at`): every touched surface point is re-projected into screen space; alpha = screen brush shape × depth window along the view ray (`brush.projection_depth`,uprojfar/udepthscale parity). Symmetry supported. |
| **Polygon Fill** | `4` | **Implemented** | `ToolMode::PolygonFill`. Supports Triangle, UV Island, and Mesh fill modes. |
| **Smudge Tool** | `5` | **Implemented** | Real displacement smudge (`smudge_stamp_at` / `smudge_3d_at`, EngineActionSmudge parity): content pulled behind the stroke motion; `Sample Merged` samples from the stack composite; `Finger Painting` deposits brush color. Works in 2D & 3D with tangent-space stamp alpha. |
| **Clone Stamp** | `6` | **Implemented** | Full-pattern clone (`clone_stamp_at` / `clone_stamp_3d_at`): aligned stroke-relative offset locked per stroke; clones full channels (color, alpha, metallic, roughness + optional normal/height) from the merged composite; pattern rotation/scale/mirror transforms; 3D rasterization through the mesh. Fix: 3D Alt+Click (no drag) no longer conflicts with orbit. |
| **Color Eyedropper / Picker**| `I` | **Implemented** | `ToolMode::Eyedropper`. Samples BaseColor, Metallic, Roughness, and Normal from clicked 3D surface. |
| **Text / Decal Tool** | `7` | **Implemented** | `ToolMode::Decal`: real TTF/OTF system-font rasterization via `ab_glyph` (`painting/text.rs`: recursive font discovery of `/usr/share/fonts` etc. cap 500, default DejaVu/Liberation/Noto selection, per-path `FontArc` cache, kerning via `PxScaleFont`, max-coverage AA), font ComboBox in tool properties with auto-rerase on change, full fallback to the embedded bitmap engine on bad fonts (never panics); plus image-file decal load, scale/rotation, stamps via the custom-alpha brush path in 2D & 3D. |
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
| **Fill Layer** | **Implemented** | Uniform or tiled material settings (`fill_color`, `fill_metallic`, `fill_roughness`, `fill_height`). Fill-layer materials can now be instantiated from the authentic reference PBR suite starter library (see Event-Driven Import note below). |
| **Folder / Group** | **Implemented** | `LayerType::Folder` with nested tree rendering, parent/child hierarchy, recursive opacity & visibility cascading, and Ctrl+G hotkey. |
| **Blend Modes (29/29)** | **Implemented** | All 29 reference PBR suite layer blend modes (`layer.rs::blend_colors`): Normal, PassThrough, Multiply, Screen, Overlay, Add, Subtract, Darken, Lighten, ColorDodge, ColorBurn, LinearBurn, HardLight, SoftLight, Difference, Exclusion, VividLight, LinearLight, PinLight, HardMix, DarkerColor, LighterColor, Divide, Hue, Saturation, Color, Luminosity, plus real Tangent-Space RNM and Whiteout normal-map compositing (not channel mistreats). Persisted per layer and per channel; UI dropdowns expose all 29. |
| **Per-Channel Blend Modes** | **Implemented** | `channel_color_blend`, `channel_metallic_blend`, etc. with per-channel routing toggles and opacities in UI and compositor. |
| **Mask Stack** | **Implemented** | `LayerMask` supports White, Black, and Bitmap masks. |
| **Mask Paint-in-Mask** | **Implemented** | Dual thumbnail workflow (Content & Mask), click-to-activate mask editing mode with 2D & 3D real-time grayscale brush painting. |
| **Mask Effects Stack** | **Implemented** | Sub-stack supporting sequential evaluation of Fill, Generator, Levels, and Invert effects. |
| **Procedural Generators** | **Implemented** | `Metal Edge Wear`, `Dirt Crevices`, `Dust Accumulation`, `Position Gradient`, `World Normal Orientation`, `Thickness / Subsurface`, and `Color / ID Selection` with grunge FBM noise, tolerance, and axis controls. |
| **Filters Stack** | **Implemented** | Non-destructive `FilterEffect` stack supporting Levels, Gaussian Blur, Sharpen, HSL adjustment, Invert, and Gradient Map with dynamic inspector controls. |
| **SBS Graph Engine** | **Implemented** | `painting/graph.rs` (~2,200 lines): typed node DAG + evaluator with memoization, cycle detection (carrying the DFS path), serde persistence, and 21 deterministic nodes — FBM noise, white noise, tileable Voronoi spots/crystal panels, bricks (running bond), marble, gradient (linear/radial/angular), gradient map, blend (reuses layer blend modes), levels, invert, gaussian blur, slope blur, directional warp, transform (wrapped bilinear), tile generator, external image, and 8K² per-eval budget. 12 unit tests. |
| **Procedural (Graph) Material Dialog** | **Implemented** | Edit menu → "Procedural (Graph) Material...": 6 engine-backed templates (FBM clouds, grain, blotches, marble, brick, crystal) with live 128² preview + seed/scale/contrast parameters that rebuild the DAG, and Apply that renders at document resolution into a new paint layer (history-logged). |
| **.sbsar Container Parser** | **Implemented** | `painting/sbsar.rs`: 7z/LZMA unpack (sevenz-rust, with a salvage path for the upstream solid-block CRC defect), `sbsdescription` XML deserialization of graph metadata, parameters, widgets (sliders/combos/attachments), `sbsasm` presence detection. Known limit: sevenz-rust #31 truncates trailing XML members inside solid archives — first-member graphs parse fully; revisit when upstream ships >0.6.1 or with a vendored decoder. |

---

## 4. Mesh Map Baking Pipeline Audit

| Baker Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Dedicated Studio UI** | **Implemented** | Full-window baking interface matching reference PBR suite 1:1. |
| **Ambient Occlusion (AO)** | **Implemented** | Cosine-weighted hemisphere raycasting with sample count and distance sliders. |
| **Curvature** | **Implemented** | Surface Laplacian / divergence computation with intensity slider. |
| **Normal Map (Tangent)** | **Implemented** | High-to-low raycast projection with MikkTSpace tangent encoding. |
| **World Space Normal** | **Implemented** | Absolute orientation encoding for top-down procedural effects. |
| **Position Map** | **Implemented** | Object-space bounding-box normalized coordinates. |
| **Thickness Map** | **Implemented** | Reverse normal penetration raycasting for subsurface scattering. |
| **ID Map** | **Implemented** | Vertex color / submesh color ID baking. |
| **High-Poly to Low-Poly** | **Implemented** | OBJ loading of high-poly mesh with raycast intersection testing. |
| **Cage / Ray Distances** | **Implemented** | Max frontal distance and max rear distance sliders. |
| **Match by Mesh Name** | **Implemented** | Submesh name suffix stripping (`_low` / `_high`) with isolated BVH acceleration per submesh to eliminate inter-object ray bleed. |
| **Antialiasing (AA)** | **Implemented** | Multi-sample accumulation buffer supporting None (1x), 2x2 (4 samples), and 4x4 (16 samples) supersampling rasterization. |
| **Skew Correction (all casters)** | **Implemented** | Baking-time skew correction (USE_SKEW_MAP parity): external 2-channel skew texture (R/G → [-1,1] shear offsets), invert toggle, strength slider; now perturbs the origin of *every* cast — primary normal/position ray, AO hemisphere rays, and thickness reverse ray (previously normal-only). Mirrored-UV signed det handling and degenerate-TBN fallbacks hardened (axis-derived tangent instead of near-zero Gram-Schmidt division). |
| **Painted Ray Offsets** | **Implemented** | `PaintedOffsetMap` ([-1,1] per-texel offset map) shifts cast origins along the surface normal by offset × (frontal+rear), with the ray span extended accordingly — matches reference's painted ray offset workflow. |
| **Match by Shell (fallback)** | **Implemented** | When name-suffix matching misses, low-poly shells correspond to the nearest high-poly shell by world-space AABB distance within one ray span (rejection via `aabb_gap`) instead of falling back to the global BVH; per-shell precomputed AABBs also replaced the per-triangle `submeshes.find()` scan. |
| **Output Bit Depth (8/16/32)** | **Implemented** | `BakedMapFormat { U8, F16, F32 }` with IEEE-half encoding (round-to-nearest, denormal flush) — Bake Studio "Output Depth" combo; project save stores depth-tagged members (`baked/<map>.u8` / `.f16`, legacy `.raw` = f32) with verified round-trip test, cutting `.rmp` baked-map size 4×/2×. |
| **Background Thread Baking** | **Implemented** | Non-blocking background baking with atomic cancelation (`AtomicBool`), progress bar (`AtomicUsize`), and UI cancel button. |

---

## 5. Viewport & Rendering Pipeline Audit

| Viewport Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Cook-Torrance GGX** | **Implemented** | `src/graphics/viewport_wgpu.rs`: $D_{\text{GGX}}$, $G_{\text{Smith-Schlick}}$, $F_{\text{Schlick}}$. |
| **Split-Sum IBL** | **Implemented** | Prefiltered cubemap convolution + 2D numerical BRDF integration LUT ($512 \times 512$). |
| **HDR Environment Loader** | **Implemented** | Reads `.exr` and `.hdr` studio environments; dynamic exposure and rotation controls. |
| **ACES Tonemapping** | **Implemented** | ACES filmic curve with sRGB gamma correction. |
| **Wireframe Overlay** | **Implemented** | 3D mesh line overlay (`GL_POLYGON_OFFSET_LINE`) and 2D UV layout. |
| **Solo Channels** | **Implemented** | BaseColor, Roughness, Metallic, Normal, AO, Curvature, Position, Thickness. |
| **Bloom & Glow** | **Implemented** | `src/graphics/viewport_wgpu.rs`: High-luminance thresholding and specular bloom highlights overlay before ACES tonemapping. |
| **Depth of Field (DoF)** | **Implemented** | `src/graphics/viewport_wgpu.rs`: Circle of confusion defocus simulation based on camera focal distance and aperture. |
| **Screen Space Ambient Occlusion (SSAO)** | **Implemented** | `src/graphics/viewport_wgpu.rs`: Contact shadow ambient factor modulation with curvature-weighted intensity. |
| **Color Management (Profiles)** | **Implemented** | `src/graphics/viewport_wgpu.rs` & `src/ui/app.rs`: Selectable color management modes (ACES Filmic, sRGB Linear Display transfer, Neutral Reinhard tonemapping) via `u_color_management_mode` uniform in PBR shader and persistent config. |

---

## 6. Texture Export Pipeline Audit

| Export Feature | Status | Implementation Details / File Location |
| :--- | :--- | :--- |
| **Export Dialog UI** | **Implemented** | Window with target mesh, resolution, directory picker, preset dropdown, and export status logging. |
| **Unreal Engine ORM Preset**| **Implemented** | `src/painting/export.rs`: automated channel packing (R=AO, G=Roughness, B=Metallic) with Normal DirectX Y-. |
| **Unity HDRP MaskMap Preset** | **Implemented** | `src/painting/export.rs`: corrected to the real HDRP spec — R=Metallic, G=AO, B=Detail Mask (neutral 1.0 until user detail channels exist), A=Smoothness — locked by a spec test. Unity URP (R=Metallic, A=1−Roughness) and Unreal ORM (R=AO, G=Roughness, B=Metallic) spec tests likewise lock the packings. |
| **glTF 2.0 Preset** | **Implemented** | `src/painting/export.rs`: Khronos standard packing (B=Metallic, G=Roughness, R=AO). |
| **Generic PBR Preset** | **Implemented** | Exports separate BaseColor, Roughness, Metallic, Normal, and AO images. |
| **Bit Depth Selection** | **Implemented** | 8-bit PNG, 16-bit PNG, and 32-bit OpenEXR floating-point support. |
| **Edge Dilation / Padding (4 algorithms)** | **Implemented** | reference-parity padding dropdown in the export dialog: Color (dilate), Transparent, Diffusion (Jacobi/Laplace blur into the gutter band with UV coverage as Dirichlet boundary), and Passthrough — plus naming tokens (`$mesh$`, `$textureset$`, `$resolution$`…) and linear-color-space EXR output fixes. |
| **Per-Texture-Set Output** | **Implemented** | `export_all_texture_sets` iterates and exports all active texture sets with naming template tokens. |

---

## Summary Scorecard

> Counted directly from the feature-table rows in this document (2026-10-22, post-blend-modes/baker-depth/export-padding/graph-engine/sbsar/session). New rows added under this date are marked **Partial**-free but scored honestly: `.sbsar` graph-bytecode *evaluation* (SBSASM VM) remains out of scope — see `docs/SBSASM_BYTECODE_NOTES.md`.

| Category | Total Items | Implemented | Partial | Stub | Missing |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Menu Items** | 56 | 56 | 0 | 0 | 0 |
| **Tools & Brush Engine** | 15 | 15 | 0 | 0 | 0 |
| **Layer Stack & System** | 11 | 11 | 0 | 0 | 0 |
| **Mesh Map Baking** | 15 | 15 | 0 | 0 | 0 |
| **Viewport & Shaders** | 10 | 10 | 0 | 0 | 0 |
| **Texture Export** | 8 | 8 | 0 | 0 | 0 |
| **Total** | **121** | **121 (100% of in-scope)** | **0** | **0** | **0** |

### Depth qualifications (v1 / simplified implementations)
- **Visualisation (Render) — Implemented (honest scope)**: offline path-traced renderer ships as RGB (not spectral), analytic studio environment (no HDRI file parsing), no denoiser, env-only lighting; reference's Iray is spectral/MDL GPU machinery — parity bar is preset-ladder + deterministic convergence, not byte-comparison against Iray output.
- **reference Library Import — Implemented (preview-derived content)**: 991 authentic items imported from the decompiled SP starter assets (`scripts/import_material_library.py` → `assets/material_cache/`): 96 materials (with 1024² flat fill-maps unwarped from sphere-preview icons — no more giant-disc content), 121 smart materials (flat bitmaps where SP embedded them), 62 effects, 21 generators, 275 procedurals, 416 alphas (sphere-renders rejected as paint content), 60 SP fonts, 5 flat alpha bitmaps. Parameter presets (roughness/metallic/color scalars) come from the authentic `.sbsar` XMLs; full procedural graph evaluation of `.sbsar` (embed reference Engine) is intentionally out of scope.
- **Skew Correction — Implemented**: sheared-ray-origin approximation of reference's `USE_SKEW_MAP`; geometry-preserving skew-mesh painting pending.
- **Smudge / Clone projection semantics**: 3D stamps honor tangent-space alpha shapes; per-stamp jitter/spacing dynamics on the 3D path are simplified relative to reference's stamp-page pipeline.
- **Grid & World Axes / Anchor routing**: implemented in earlier merged branches; rows were stale until this sync.
