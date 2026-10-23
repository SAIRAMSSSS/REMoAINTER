# REMoAINTER 2026.1 (v2.0.0) — Architecture Specification

This document details the internal architecture, memory models, data structures, and execution pipelines of **REMoAINTER 2026.1 (v2.0.0)**.

---

## 1. System Module Map

```mermaid
graph TD
    Main[src/main.rs: Entry Point & eframe Setup] --> UI[src/ui/app.rs: RemoainterApp & Dock Panels]
    
    UI --> Paint[src/painting/: Core Painting & Compositing]
    UI --> Baker[src/baker/: Raycast Mesh Map Baker]
    UI --> Gfx[src/graphics/: Viewport & OpenGL Pipeline]
    
    Paint --> Layer[layer.rs: LayerStack & Channel Buffers]
    Paint --> Brush[brush.rs: Brush Engine & Dynamics]
    Paint --> Mask[mask.rs: LayerMask & Effects DAG]
    Paint --> Anchor[anchor.rs: Anchor Points]
    Paint --> MatGen[material_gen.rs: Procedural Materials]
    Paint --> TexImp[texture_importer.rs: PBR Set Matching]
    
    Baker --> BakeEng[engine.rs: Rayon Multithreaded Baker]
    
    Gfx --> Viewport[viewport_gl.rs: GL Context & PBR Shaders]
    Gfx --> Mesh[mesh.rs: 3D Mesh & OBJ Parser]
    Gfx --> Lighting[lighting.rs: Camera & IBL Probes]
```

### Module Responsibilities

| Subsystem | Primary Source Files | Responsibility |
| :--- | :--- | :--- |
| **App Core & UI** | `src/main.rs`<br>`src/ui/app.rs`<br>`src/ui/icons.rs`<br>`src/ui/theme.rs` | Window lifecycle, event dispatching, egui dock panels, modal dialogs, status bar, and shortcut translation. |
| **Painting Engine** | `src/painting/layer.rs`<br>`src/painting/brush.rs`<br>`src/painting/mask.rs`<br>`src/painting/anchor.rs`<br>`src/painting/material_gen.rs` | Channel pixel buffers, 3D Euclidean surface projection painting, layer compositing, mask effect evaluation, procedural materials. |
| **Baking Engine** | `src/baker/engine.rs`<br>`src/baker/mod.rs` | High-to-low poly raycasting, cage bounds, MikkTSpace tangent vectors, Ambient Occlusion, Curvature, Position, Thickness, and ID map generation. |
| **Graphics & Viewport** | `src/graphics/viewport_gl.rs`<br>`src/graphics/mesh.rs`<br>`src/graphics/lighting.rs` | Embedded OpenGL 3.3+ rendering via `glow`, Cook-Torrance GGX fragment shader, ACES tonemapping, wireframe overlay, HDRI cubemap sampling, and mesh I/O. |

---

## 2. Storage Models

### A. Layers & Channel Buffers
A project's texturing state is managed by `LayerStack`, which holds a linear list of `Layer` structs evaluated from bottom (index 0) to top:

```rust
pub struct LayerStack {
    pub layers: Vec<Layer>,
    pub active_layer_index: usize,
    pub width: u32,
    pub height: u32,

    // Flattened composite buffers (Width * Height)
    pub composite_color: Vec<[f32; 4]>,
    pub composite_metallic: Vec<f32>,
    pub composite_roughness: Vec<f32>,
    pub composite_normal: Vec<[f32; 3]>,
    pub composite_lit: Vec<[f32; 4]>,
}
```

Each `Layer` contains discrete channel buffers stored in flat CPU heap arrays:
```rust
pub struct Layer {
    pub id: usize,
    pub name: String,
    pub layer_type: LayerType, // Paint, Fill, Folder
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode, // Normal, Multiply, Screen, Overlay, PassThrough
    pub folder_expanded: bool,

    // Mask & Anchor
    pub mask: Option<LayerMask>,
    pub anchor_point: Option<AnchorPoint>,
    pub color_tag: Option<u8>,

    // Fill parameters (for Fill layers)
    pub fill_color: [f32; 4],
    pub fill_metallic: f32,
    pub fill_roughness: f32,
    pub fill_height: f32,

    // Channel Pixel Buffers (sized width * height)
    pub color_buffer: Vec<[f32; 4]>,
    pub metallic_buffer: Vec<f32>,
    pub roughness_buffer: Vec<f32>,
    pub normal_buffer: Vec<[f32; 3]>,
    pub height_buffer: Vec<f32>,

    pub width: u32,
    pub height: u32,
}
```

### B. Masks & Effect Stack
Masks are scalar maps ($[0.0, 1.0]$) that multiply or blend layer contribution:
```rust
pub struct LayerMask {
    pub mask_type: MaskType, // White (1.0), Black (0.0)
    pub buffer: Vec<f32>,    // Flattened grayscale bitmap
    pub effects: Vec<MaskEffect>,
    pub width: u32,
    pub height: u32,
}

pub enum MaskEffect {
    Fill { value: f32, noise_frequency: f32, blend_mode: MaskBlendMode, opacity: f32 },
    Paint { blend_mode: MaskBlendMode, opacity: f32 },
    Generator {
        name: String,
        wear_level: f32,
        contrast: f32,
        dirt_amount: f32,
        invert: bool,
        use_ao: bool,
        use_curvature: bool,
        anchor_name: Option<String>,
        blend_mode: MaskBlendMode,
        opacity: f32,
    },
    Levels { black_in: f32, white_in: f32, gamma: f32 },
    Invert,
}
```
During `recompute()`, each effect is evaluated sequentially and blended into `mask.buffer`.

### C. Texture Sets & UDIM Tiles
- Currently, the active mesh is stored in `RemoainterApp::mesh` with channels defined in `texture_channels: Vec<TextureChannel>`.
- **Target reference PBR suite Architecture**:
  - `TextureSet` represents an individual material index or UV tile.
  - Multi-set meshes maintain an array of `TextureSet` instances, each owning an isolated `LayerStack`, `BakedMaps`, and GPU texture handles.
  - UDIM tiles correspond to UV coordinates $[u, v]$ mapped to tile IDs $1000 + \lfloor u \rfloor + 1 + 10 \times \lfloor v \rfloor$.

---

## 3. Paint Stroke Pipeline (Pointer to GPU)

```mermaid
sequenceDiagram
    participant User as User (Mouse / Stylus)
    participant App as RemoainterApp (egui CentralPanel)
    participant Layer as LayerStack (CPU Buffers)
    participant TexMgr as TextureManager (GL Context)
    participant Viewport as PBR Fragment Shader (GPU)
    
    User->>App: Pointer Drag (Screen X, Y)
    App->>App: Raycast Screen Pixel -> 3D Mesh Surface
    Note over App: Hits point P_hit in R^3, Normal N_hit, UV
    App->>App: Distance-based spacing interpolation
    App->>Layer: paint_stroke_euclidean(P_hit, N_hit, Brush)
    Note over Layer: Find triangles within Radius R.<br/>Barycentric interpolate 3D pos P(u, v).<br/>Falloff = smoothstep(R, R*hardness, ||P - P_hit||).<br/>Blit into Color, Metal, Rough, Normal buffers.
    Layer->>Layer: recompute_composite() (Blend all layers bottom-to-top)
    App->>TexMgr: update_layer_textures(gl, composite_buffers)
    TexMgr->>Viewport: glTexSubImage2D(GL_TEXTURE0..3)
    Viewport->>User: Render Frame with Cook-Torrance GGX Shading
```

### Detailed Execution Steps:
1. **Raycast Hit Detection**:
   When painting in the 3D viewport, camera ray $\vec{R}(t) = \vec{O} + t \vec{D}$ is intersected against all triangles of `Mesh`. The nearest valid intersection yields hit position $P_{\text{hit}}$, surface normal $N_{\text{hit}}$, and barycentric UV.
2. **Spacing & Stroke Smoothing**:
   If distance from previous stamp $\ge \text{brush.size} \times \text{brush.spacing}$, intermediate stamps are generated via linear interpolation.
3. **3D Euclidean Surface Projection**:
   Instead of naively stamping in 2D UV space (which tears across seams and pinches at poles), `LayerStack::paint_stroke_euclidean` marches over all triangles whose 3D bounding sphere overlaps $(P_{\text{hit}}, R_{\text{brush}})$. For every texel in those triangles, its real 3D coordinate $P(u, v)$ is calculated. Texels within $R_{\text{brush}}$ receive paint proportional to distance and surface normal angle falloff.
4. **Channel Blitting**:
   The active `Layer`'s pixel buffers (`color_buffer`, `roughness_buffer`, etc.) are updated based on active brush channel switches (`paint_color`, `paint_roughness`, etc.).
5. **Compositing**:
   `recompute_composite()` iterates over visible layers, applying blend modes and masks into `composite_color`, `composite_roughness`, `composite_metallic`, `composite_normal`.
6. **GPU Texture Upload**:
   `texture_needs_update` is flagged. `TextureManager::update_layer_textures` invokes `glTexSubImage2D` on OpenGL texture IDs.
7. **Viewport Draw**:
   `PbrShader` binds the updated textures to texture units 0..3 and executes Cook-Torrance GGX fragment shading.

---

## 4. Mesh Map Baking Pipeline

```mermaid
flowchart TD
    Start[User selects Bake Mesh Maps] --> Opts[Configure BakerOptions: Res, AO samples, Ray Distances]
    Opts --> HPCheck{High-Poly Mesh Uploaded?}
    HPCheck -->|Yes| Raycast[Raycast Projection: Low-Poly UV -> High-Poly BVH]
    HPCheck -->|No| LowSelf[Self-Mesh Curvature & Geometric AO]
    
    Rayon[Rayon Parallel Scanline Rasterizer] --> PassAO[Pass 1: Cosine Hemisphere AO Raycast]
    Rayon --> PassCurv[Pass 2: Normal Laplacian Curvature]
    Rayon --> PassNorm[Pass 3: MikkTSpace Tangent Normal]
    Rayon --> PassWSN[Pass 4: World Space Normal]
    Rayon --> PassPos[Pass 5: Normalized Bounding Box Position]
    Rayon --> PassThick[Pass 6: Inverted Thickness Raycast]
    Rayon --> PassID[Pass 7: Material / Color ID]
    
    Raycast --> Rayon
    LowSelf --> Rayon
    
    PassAO & PassCurv & PassNorm & PassWSN & PassPos & PassThick & PassID --> BakedMaps[BakedMaps Struct]
    BakedMaps --> PushViewport[Upload to GL Viewport Textures]
    BakedMaps --> PushSmartMasks[Expose to Mask Generators & Edge Wear]
```

### Baking Engine Internals:
- **Parallel Rasterization**: Uses `rayon` to divide the UV height into parallel chunks.
- **MikkTSpace Parity**: Tangents and bitangents computed per vertex. High-poly normals are rotated into the low-poly tangent frame and packed into $16$-bit normal buffers.
- **AO Solver**: Generates stratified cosine-weighted hemisphere rays, tests ray intersections against geometry, and integrates unoccluded radiance.
- **Curvature Solver**: Evaluates divergence of the normal field across adjacent texels to isolate sharp convex ridges ($> 0.5$) from concave crevices ($< 0.5$).
- **DirectX / OpenGL Normal Orientation**: Inverts the Green channel if `invert_normal_y` is checked.

---

## 5. History & Undo/Redo System

### Current Implementation (Stub)
Currently, `RemoainterApp::history` is a `Vec<String>` that logs human-readable descriptions of operations (e.g. `"Stroke applied"`, `"Added Fill Layer"`). `Ctrl+Z` and `Ctrl+Y` only update the status bar text.

### Target Architecture (reference PBR suite Parity)
To deliver non-destructive undo/redo without excessive RAM consumption:

```mermaid
classDiagram
    class HistoryStack {
        +Vec~Box~dyn UndoCommand~~ commands
        +usize cursor
        +usize max_memory_bytes
        +undo()
        +redo()
        +push(command)
    }
    
    class UndoCommand {
        <<interface>>
        +apply(app: &mut RemoainterApp)
        +revert(app: &mut RemoainterApp)
        +memory_size() usize
    }
    
    class StrokeCommand {
        +usize layer_id
        +Vec~TileDelta~ dirty_tiles
        +apply()
        +revert()
    }
    
    class LayerParamCommand {
        +usize layer_id
        +LayerProperty old_val
        +LayerProperty new_val
        +apply()
        +revert()
    }
    
    class LayerTopologyCommand {
        +usize index
        +LayerAction action
        +apply()
        +revert()
    }
    
    HistoryStack o-- UndoCommand
    UndoCommand <|.. StrokeCommand
    UndoCommand <|.. LayerParamCommand
    UndoCommand <|.. LayerTopologyCommand
```

#### Key Architecture Principles:
1. **Tile-Based Sparse Channel Storage**:
   - 2K/4K channels are partitioned into $64 \times 64$ texel tiles.
   - Brush strokes only copy the pre-stroke state of modified tiles into `TileDelta`.
   - Reverting a stroke copies back only the touched tiles, reducing stroke history memory from $\sim 64\text{MB}$ to $< 500\text{KB}$.
2. **Atomic Transaction Batching**:
   - Pointer down begins a transaction.
   - Pointer move accumulates dirty tiles.
   - Pointer up finalizes and commits the `StrokeCommand` to `HistoryStack`.
3. **Bounded Memory Eviction**:
   - The stack tracks cumulative memory size.
   - When total memory exceeds the configured cache limit (e.g., 2 GB), oldest commands are evicted from the tail.

---

## 6. Shaders & Viewport Rendering Core

### A. OpenGL Shading Architecture (`viewport_gl.rs`)
The viewport uses three custom GLSL 330 core shader programs managed via `glow`:

1. **`PbrShader`**:
   - **Vertex Shader**: Transforms vertex coordinates to world and clip space; constructs normalized tangent-bitangent-normal matrix $v_{\text{tbn}} = [T, B, N]$ using vertex tangents and bitangent signs ($A_{\text{tangent}}.w$).
   - **Fragment Shader**:
     - Evaluates Cook-Torrance microfacet specular BRDF:
       - NDF: $D_{\text{GGX}}(N \cdot H, \alpha)$
       - Geometry: $G_{\text{Smith-Schlick}}(N \cdot V, N \cdot L, \alpha)$
       - Fresnel: $F_{\text{Schlick}}(V \cdot H, F_0)$ modified by roughness for grazing angles.
     - Split-Sum IBL:
       - Radiance: Sampled from prefiltered HDR cubemap (`GL_TEXTURE6`) with explicit roughness mip-level sampling.
       - Environment BRDF: Looked up from precomputed $512 \times 512$ 2D BRDF LUT texture (`GL_TEXTURE7`).
     - Tone Mapping: Standard ACES Filmic curve mapping high-dynamic-range radiance into linear range, followed by sRGB gamma encoding ($x^{1/2.2}$).
2. **`SkyboxShader`**:
   - Equirectangular or cubemap skybox environment sphere with adjustable rotational yaw, exposure, and blur factor.
3. **`WireframeShader`**:
   - Renders polygon edges over the lit geometry using `GL_POLYGON_OFFSET_LINE` and cyan color token (`#26bfe6`).

### B. Texture Unit Allocations

| Texture Unit | Binding Target | Description |
| :--- | :--- | :--- |
| `GL_TEXTURE0` | BaseColor Map | Composite linear RGB albedo |
| `GL_TEXTURE1` | Metallic Map | Composite scalar metallic |
| `GL_TEXTURE2` | Roughness Map | Composite scalar roughness |
| `GL_TEXTURE3` | Normal Map | Composite tangent space normal |
| `GL_TEXTURE4` | Baked AO Map | Ambient occlusion shadow contribution |
| `GL_TEXTURE5` | Baked Curvature | Curvature visualization / solo mode |
| `GL_TEXTURE6` | HDR Environment | Pre-filtered panoramic/cubemap radiance map |
| `GL_TEXTURE7` | BRDF LUT | Pre-integrated 2D split-sum scale and bias LUT |
