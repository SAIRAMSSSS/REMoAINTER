# Substance Painter Reference Extraction Notes

> Living document. Purpose: record exactly where reference material for the 1:1
> parity rebuild lives on this machine, and how to use it. Every agent working
> on REMoAINTER should read this before attempting decomp work.
>
> CHEATING NOTICE: Recipes, formats, and behavioral parameters from these files
> are re-implemented from scratch in Rust/WGSL. Do NOT copy Adobe's shaders,
> QML, or any binary blobs into the repository.

## 1. Artifacts on disk

| Path | What it is |
|---|---|
| `/mnt/shared/Decomp/adobe-substance-3d-painter/` | 148 MiB x86-64 PE exe + intake report + 621,694-symbol address table + `debug_types.h` |
| `/mnt/shared/Decomp/reverse-engineering/*.md` | 10 subsystem specs already distilled from the exe (brush engine, layer DAG, UDIM/TextureSet, IBL viewport, export, bake, mesh intake, substack height blending, anchors/smart-mask eval) |
| `/mnt/shared/Decomp/.decomp/decomp.db` | 1.1 GB SQLite: `functions` (385,578 rows, full x86-64 disassembly text in `asm`) and `symbols` (kind func/data, 231k data rows). Read-only URI: `file:/mnt/shared/Decomp/.decomp/decomp.db?mode=ro` |
| `/mnt/shared/SubstancePainter11/` | FULL INSTALL tree (exe + DLLs + all support dirs) |
| `…/App/qml/AlgWidgets*` | Substance's complete QML widget library (Alg*.qml) — UI behavior spec |
| `…/App/resources/python/modules/substance_painter/` | The entire Python API (`layerstack`, `export`, `textureset`, `project`, `baking`, `properties`, `colormanagement`, `display`, `resource`, …) — the AUTHORITATIVE surface list of components and enum values |
| `…/App/resources/ocioc…` dir (App/resources → see listing) + `.icc`, `mdl`, `usd` dirs | Color management config references, MDL materials, USD plugin surface |
| `…/resources/python/modules/automated_tests/resources/**` | Sample `.sbs` XML graphs (noise, generic_filter, simple_material, substance_color, input_test) |
| `scratch/spsm/Dirt.spsm` and `/mnt/shared/test_spsm/*.spsm` | Smart-material `.spsm` "HDF"-style containers (header "HDF"), which embed textures + at least sometimes a `.sbsar` |
| `/mnt/shared/tmp-test/embedded.sbsar` | Minimal working full example smart material (mg_dust.sbsar) used to crack the container format |

## 2. Format findings (verified)

- `.sbsar` = **7-zip archive (LZMA, solid)** with members `assemblies/content/0000/<name>.sbsasm` and `<name>.xml` (further 0000/index etc. for multi-graph versions). Verified with `7z l/x`. Rust note: the 7z container can be read with an LZMA-based reader; `cargo` has crates (`lzma-rs`, `sevenz-rust`) — decide during implementation.
- `<name>.xml` = **sbsdescription** XML: output formats (`format="N"` int codes = luminance/RGB), per-input parameter list with **type codes** `0=float(1)`, `4=int`, `5=image`, `8=int2`, `256=float-with-widget-ext…`, plus widget metadata (slider min/max/clamp/step, togglebutton, image). This is the parameter-UI schema.
- `<name>.sbsasm` = **compiled Substance bytecode ("jen")** from Substance Designer compilation — the …REME missing semantics will be minable by diffing across many files. Disassembly/virt-translation of this bytecode is the shortest path to high-fidelity node behavior data and import full sets (not just authored limited IVM). This work-site: see `docs/SBSASM_BYTECODE_NOTES.md` (agent-owned RFC).
- `.sbs` = full editor project XML serialization; composer graph tags confirmed: `compNodes / compNode / compImplementation (compInstance|compInputBridge|compOutputBridge) / comptype`, `paraminputs` with widget option blocks (`clamp/candefaults/max/min/step/label0…`), `graphOutputs` with `usages/usage` channel usage tags (`baseColor`/`roughness`). Node *library kernel names* live in retail `.sbs`/`.sbsar` assets, not in the exe strings.
- REMoAINTER `assets/substance_cache/**` is NOT a real smart material: only baked thumbnails + `meta.json` + `params.json`. No graph evaluation today — the highest-value gap in the codebase is the graph engine (being built in `rust/src/painting/graph.rs`).

### Known layer-stack model (from the Python API, authoritative)
Foundation
- Layer types: `PaintLayer`, `FillLayer`, `GroupLayer`, `InstanceLayer`.
- Effects: `GeneratorEffect`, `PaintEffect`, `FillEffect`, `LevelsEffect`, `CompareMask`, `Filter`, `ColorSelection`, `AnchorPoint`.
- Projection modes: 3D, UV, UVSet-to-UVSet, Triplanar, Planar, Warp, Spherical, Cylindrical, plus `UVTransformation` and `ProjectionCulling` parameter groups.

Painter's channel architecture per texture set — pack/unpack direction can be probed by running the extracted (not-demo) `export` preset enumeration or by reading `PredefinedExportPreset` docs inside `export.py`.

## 3. Recommended mining recipes

### Strings mining of the exe
```bash
strings -n 6 "/mnt/shared/Decomp/adobe-substance-3d-painter/Adobe Substance 3D Painter.exe" > /tmp/opencode/sp_str_ascii.txt
strings -el -n 6 "…exe" > /tmp/opencode/sp_str_utf16.txt
```
Note: REA/Ghidra headless reaches its 30-min analyzer deadline on this 148 MiB PE — with the current MCP server env it can't be wormage to live pseudocode (currently needs `REA_GHIDRA_STARTUP_TIMEOUT_MS` raised at server start; not yet done). SQLite DB function asm fields work fine for static digging.

### SQLite host-DB probes
```bash
sqlite3 "file:/mnt/shared/Decomp/.decomp/decomp.db?mode=ro" \
  "SELECT name,n_insns FROM functions WHERE n_insns BETWEEN 1500 AND 4000 ORDER BY size DESC LIMIT 20"
```
Limited values; names mostly auto (`sub_…`). Prefer the data table + borrow pack extraction:
```bash
sqlite3 "file:/mnt/shared/Decomp/.decomp/decomp.db?mode=ro" \
  "SELECT gen_random_uuid() - 1 FROM symbols WHERE kind='data' LIMIT 3"
```

### Carving embedded archives out of `.spsm`
Scan the .spsm for 7z header bytes (`37 7A BC AF 27 1C`) and/or `Rar`/`PK\x03\x04`, and extract the embedded `.sbsar` members with `7z x`. Multiple embedded 7z or HDF images may exist inside a single spsm.

### What NOT to bother with
- UCLA hard static function-by-function decomp of 385k functions — the formats + Python/UI/QML references carry more signal metric-for-metric.
- Don't try to run Adobe exe — do not need Wine; all riches are in static artifacts + already extracted docs.

## 4. Outstanding mining work (where to continue)

- [ ] `tools/sbsasm_disasm.py` — .sbsasm bytecode disassembler (7z-extract → parse asm) to spec retail graph semantics; priority HIGH for the graph engine.
- [ ] Full pack-codes table for `format="N"` in sbsdescription (`0` = 8-bit luminance, … — table build iterates over real sbsar archives; enumerate via examples — collect "Ihar" data from the field).
- [ ] OCIO config → `rust/src/project/config.rs` color management presets port.
- [ ] QML `AlgPalette.qml` + `AlgWidgetsStyle.qml` exact UI token values cross-check vs `rust/src/ui/theme.rs` (already close).
- [ ] MDL module inspection — only needed if we ship a molecular path-trace–compatible (never needed — our own path tracer takes plain graphs).
