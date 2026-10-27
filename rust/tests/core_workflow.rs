use std::fs;

use remoainter::baker::engine::{BakedMaps, BakerOptions};
use remoainter::graphics::lighting::EnvironmentPreset;
use remoainter::graphics::mesh::Mesh;
use remoainter::painting::brush::Brush;
use remoainter::painting::export::{
    execute_export, BitDepth, ExportPreset, OutputFormat, TextureExportContext,
};
use remoainter::painting::filter::{FilterEffect, FilterTargetChannel};
use remoainter::painting::history::HistoryManager;
use remoainter::painting::layer::{BlendMode, Layer, LayerType};
use remoainter::painting::texture_set::TextureSet;
use remoainter::project::ProjectManager;

#[test]
fn test_core_texturing_workflow_end_to_end() {
    let test_dir = std::env::temp_dir().join("remoainter_core_workflow_e2e");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).expect("Failed to create test directory");

    let project_file = test_dir.join("test_model.rmp");
    let export_dir = test_dir.join("exported_textures");
    fs::create_dir_all(&export_dir).expect("Failed to create export directory");

    // -------------------------------------------------------------------------
    // Step 1 & 2: Import / Create a real 3D model
    // -------------------------------------------------------------------------
    let mesh = Mesh::create_material_sphere(32, 32);
    assert!(!mesh.vertices.is_empty(), "Mesh must contain vertices");
    assert!(
        !mesh.indices.is_empty(),
        "Mesh must contain triangle indices"
    );
    assert!(
        !mesh.normals.is_empty(),
        "Mesh must contain surface normals"
    );
    assert!(!mesh.uvs.is_empty(), "Mesh must contain UV coordinates");
    assert!(
        !mesh.tangents.is_empty(),
        "Mesh must contain MikkTSpace tangents"
    );

    // -------------------------------------------------------------------------
    // Step 3: Inspect mesh and material bounds
    // -------------------------------------------------------------------------
    let (center, radius) = mesh.compute_bounds();
    assert!(radius > 0.0, "Mesh bounding radius must be positive");
    assert!(
        center[0].is_finite() && center[1].is_finite() && center[2].is_finite(),
        "Bounding center must be finite"
    );
    for uv in &mesh.uvs {
        assert!(
            uv[0] >= -0.01 && uv[0] <= 1.01 && uv[1] >= -0.01 && uv[1] <= 1.01,
            "UV coordinates must be within standard normalized [0, 1] range: {:?}",
            uv
        );
    }

    // -------------------------------------------------------------------------
    // Step 4: Create a Texture Set (resolution 64x64 for fast deterministic test)
    // -------------------------------------------------------------------------
    let res = 64u32;
    let mut tex_set = TextureSet::new(1, "HeroMaterial", res);
    assert_eq!(tex_set.layer_stack.width, res);
    assert_eq!(tex_set.layer_stack.height, res);
    assert_eq!(
        tex_set.layer_stack.layers.len(),
        2,
        "Should have default base and paint layers"
    );

    // -------------------------------------------------------------------------
    // Step 5: Paint onto the model via 3D Euclidean surface projection
    // -------------------------------------------------------------------------
    let mut history = HistoryManager::new(20);
    let active_layer_idx = tex_set.layer_stack.active_layer_index;

    let brush = Brush {
        size: 24.0,
        color: [0.95, 0.15, 0.10, 1.0], // Bright Red
        metallic: 0.85,
        roughness: 0.15,
        paint_color: true,
        paint_metallic: true,
        paint_roughness: true,
        ..Default::default()
    };

    // Hit coordinates on the material sphere (+Z pole)
    let hit_pos = [0.0, 0.0, 1.0];
    let hit_norm = [0.0, 0.0, 1.0];
    let radius_3d = 0.5;

    // Begin transaction
    history.begin_stroke();

    let dirty_bbox = tex_set.layer_stack.layers[active_layer_idx].paint_3d_surface(
        &mesh,
        hit_pos,
        hit_norm,
        radius_3d,
        &brush,
        false,
        false,
        Some(&mut history),
    );

    // Commit history step
    history.commit_stroke("Paint Red 3D Surface Stroke");

    // -------------------------------------------------------------------------
    // Step 6: Confirm that the correct UV-space texture pixels changed
    // -------------------------------------------------------------------------
    assert!(
        dirty_bbox[2] > dirty_bbox[0],
        "Dirty bbox width must be positive"
    );
    assert!(
        dirty_bbox[3] > dirty_bbox[1],
        "Dirty bbox height must be positive"
    );

    let touched_pixels = tex_set.layer_stack.layers[active_layer_idx]
        .color_buffer
        .iter()
        .filter(|px| px[3] > 0.0)
        .count();
    assert!(
        touched_pixels > 0,
        "Paint stroke must deposit non-zero alpha pixels into layer color buffer"
    );

    // Recompute composite
    tex_set.layer_stack.composite(
        None,
        None,
        None,
        None,
        None,
        None,
        EnvironmentPreset::StudioTomoco,
        0.0,
        1.0,
    );
    let comp_red_count = tex_set
        .layer_stack
        .composite_color
        .iter()
        .filter(|px| px[0] > 0.8 && px[1] < 0.3)
        .count();
    assert!(
        comp_red_count > 0,
        "Composite color buffer must reflect painted red pixels"
    );

    // -------------------------------------------------------------------------
    // Step 7: Modify Layer Stack (Add Fill & Adjustment Layers) & Composite
    // -------------------------------------------------------------------------
    // 7a. Add a Fill Layer at bottom of stack
    let mut fill_layer = Layer::new_fill(
        tex_set.layer_stack.next_id(),
        "Gold Base",
        [1.0, 0.84, 0.0, 1.0],
        1.0,
        0.2,
        res,
        res,
    );
    fill_layer.blend_mode = BlendMode::Normal;
    tex_set.layer_stack.layers.insert(0, fill_layer);

    // 7b. Add an Adjustment Layer with Invert effect at top of stack
    let mut adj_layer = Layer::new_adjustment(
        tex_set.layer_stack.next_id(),
        "Invert Filter",
        FilterEffect::Invert {
            channel: FilterTargetChannel::All,
        },
        res,
        res,
    );
    adj_layer.opacity = 0.5; // 50% opacity
    tex_set.layer_stack.layers.push(adj_layer);

    // 7c. Recompute composite across the stack
    tex_set.layer_stack.composite(
        None,
        None,
        None,
        None,
        None,
        None,
        EnvironmentPreset::StudioTomoco,
        0.0,
        1.0,
    );
    assert_eq!(tex_set.layer_stack.layers.len(), 4);

    // -------------------------------------------------------------------------
    // Step 8: Undo and Redo strokes and layer operations
    // -------------------------------------------------------------------------
    assert!(history.can_undo(), "History must allow undo after stroke");
    let undone = history.undo(&mut tex_set.layer_stack);
    assert_eq!(undone, Some("Paint Red 3D Surface Stroke".to_string()));
    tex_set.layer_stack.composite(
        None,
        None,
        None,
        None,
        None,
        None,
        EnvironmentPreset::StudioTomoco,
        0.0,
        1.0,
    );

    // After undo, the paint stroke in Paint Layer 1 should be reverted
    let paint_layer = tex_set
        .layer_stack
        .layers
        .iter()
        .find(|l| l.name == "Paint Layer 1")
        .expect("Paint Layer 1 must exist");
    let reverted_paint_pixels = paint_layer
        .color_buffer
        .iter()
        .filter(|px| px[3] > 0.0)
        .count();
    assert_eq!(
        reverted_paint_pixels, 0,
        "Undo must restore untouched transparent state"
    );

    assert!(history.can_redo(), "History must allow redo after undo");
    let redone = history.redo(&mut tex_set.layer_stack);
    assert_eq!(redone, Some("Paint Red 3D Surface Stroke".to_string()));
    tex_set.layer_stack.composite(
        None,
        None,
        None,
        None,
        None,
        None,
        EnvironmentPreset::StudioTomoco,
        0.0,
        1.0,
    );

    let paint_layer = tex_set
        .layer_stack
        .layers
        .iter()
        .find(|l| l.name == "Paint Layer 1")
        .expect("Paint Layer 1 must exist");
    let redone_paint_pixels = paint_layer
        .color_buffer
        .iter()
        .filter(|px| px[3] > 0.0)
        .count();
    assert!(
        redone_paint_pixels > 0,
        "Redo must re-apply the painted stroke pixels"
    );

    // -------------------------------------------------------------------------
    // Step 9 & 10: Save Project to .rmp & Reopen it
    // -------------------------------------------------------------------------
    let baked_maps = BakedMaps::default();
    let baker_options = BakerOptions::default();

    ProjectManager::save_project(
        &project_file,
        "CoreWorkflowTestProject",
        &mesh,
        None,
        &tex_set.layer_stack,
        &baked_maps,
        &baker_options,
        (0.0, 0.0, 3.0),
        (0.0, 1.0, 0.0, 0),
    )
    .expect("Failed to save project");

    assert!(
        project_file.exists(),
        "Saved project file must exist on disk"
    );

    let (manifest, loaded_mesh, loaded_stack, _, _) =
        ProjectManager::load_project(&project_file).expect("Failed to load project");

    // -------------------------------------------------------------------------
    // Step 11: Verify project state is accurately preserved
    // -------------------------------------------------------------------------
    assert_eq!(manifest.project_name, "CoreWorkflowTestProject");
    assert_eq!(loaded_mesh.vertices.len(), mesh.vertices.len());
    assert_eq!(loaded_stack.layers.len(), 4);
    assert_eq!(loaded_stack.layers[0].name, "Gold Base");
    assert_eq!(loaded_stack.layers[0].layer_type, LayerType::Fill);
    assert_eq!(loaded_stack.layers[1].name, "Base Metal (Steel)");
    assert_eq!(loaded_stack.layers[2].name, "Paint Layer 1");
    assert_eq!(loaded_stack.layers[2].layer_type, LayerType::Paint);
    assert_eq!(loaded_stack.layers[3].name, "Invert Filter");
    assert_eq!(loaded_stack.layers[3].layer_type, LayerType::Adjustment);

    // -------------------------------------------------------------------------
    // Step 12: Export PBR Textures (Unreal Engine ORM & glTF 2.0)
    // -------------------------------------------------------------------------
    let ue_preset = ExportPreset::unreal_engine();
    let ctx = TextureExportContext {
        layer_stack: &loaded_stack,
        baked_maps: &baked_maps,
        mesh: &loaded_mesh,
        output_dir: &export_dir,
        preset: &ue_preset,
        resolution_override: Some((res, res)),
        padding_radius: 2,
        file_format_override: Some(OutputFormat::Png),
        bit_depth_override: Some(BitDepth::U8),
        udim_tile: None,
    };

    let export_res = execute_export(ctx).expect("Export should execute cleanly");
    assert_eq!(
        export_res.exported_files.len(),
        3,
        "Unreal ORM preset must export BaseColor, Normal, and ORM textures"
    );

    // -------------------------------------------------------------------------
    // Step 13: Inspect exported files and verify pixel channels on disk
    // -------------------------------------------------------------------------
    for file in &export_res.exported_files {
        assert!(
            file.exists(),
            "Exported file must exist on disk: {:?}",
            file
        );
        let img = image::open(file).expect("Exported texture must be readable by image parser");
        let (w, h) = (img.width(), img.height());
        assert_eq!(w, res, "Exported width must match requested resolution");
        assert_eq!(h, res, "Exported height must match requested resolution");

        let file_stem = file.file_stem().unwrap().to_string_lossy();
        if file_stem.ends_with("_BaseColor") {
            let rgba = img.to_rgba8();
            // Verify that non-empty pixels exist
            let non_zero_pixels = rgba.pixels().filter(|p| p[3] > 0).count();
            assert!(
                non_zero_pixels > 0,
                "BaseColor map must contain non-empty pixels"
            );
        } else if file_stem.ends_with("_Normal") {
            let rgb = img.to_rgb8();
            // Normal map should have Z component pointing forward (blue channel > 100)
            let blue_dominant = rgb.pixels().filter(|p| p[2] > 100).count();
            assert!(
                blue_dominant > (res * res / 2) as usize,
                "Normal map must have dominant positive Z component"
            );
        } else if file_stem.ends_with("_ORM") {
            let rgb = img.to_rgb8();
            // Red = AO, Green = Roughness, Blue = Metallic
            assert_eq!(rgb.len(), (res * res * 3) as usize);
        }
    }

    // Clean up temporary test files
    let _ = fs::remove_dir_all(&test_dir);
}
