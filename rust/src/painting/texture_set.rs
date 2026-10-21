#![allow(dead_code)]

// ─────────────────────────────────────────────────────────────────────────────
// REMoAINTER — Multi-Texture Set & UDIM Tile Architecture (Substance Painter Parity)
// Features:
// - Multiple Texture Sets per Mesh (Material Groups)
// - UDIM Tile Multi-Tile UV Support (1001, 1002, 1003...)
// - Dynamic Texture Channels (Base Color, Roughness, Metallic, Normal, Height, Emissive, AO...)
// - Per-Texture-Set Resolution & Format Specifiers
// - Solo / Mute Texture Sets
// ─────────────────────────────────────────────────────────────────────────────

use crate::baker::engine::BakedMaps;
use crate::graphics::mesh::Mesh;
use crate::painting::layer::LayerStack;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Standard PBR Channel Specification in a Texture Set
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureChannelDef {
    pub name: String,
    pub format: String, // e.g. "sRGB8", "L8", "RGB16F", "L16F", "RGB32F"
    pub enabled: bool,
}

impl TextureChannelDef {
    pub fn new(name: impl Into<String>, format: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            format: format.into(),
            enabled: true,
        }
    }

    /// List of standard available channels in Substance Painter
    pub fn standard_channels() -> Vec<Self> {
        vec![
            Self::new("Base color", "sRGB8"),
            Self::new("Metallic", "L8"),
            Self::new("Roughness", "L8"),
            Self::new("Normal", "RGB16F"),
            Self::new("Height", "L16F"),
            Self::new("Emissive", "sRGB8"),
            Self::new("Opacity", "L8"),
            Self::new("Ambient occlusion", "L8"),
            Self::new("Displacement", "L32F"),
        ]
    }
}

/// A Texture Set representing a distinct material group or UDIM UV tile
#[derive(Debug, Clone)]
pub struct TextureSet {
    pub id: usize,
    pub name: String,
    pub udim_tile: Option<u32>, // Some(1001), Some(1002), etc.
    pub resolution: u32,
    pub channels: Vec<TextureChannelDef>,
    pub layer_stack: LayerStack,
    pub baked_maps: BakedMaps,
    pub visible: bool,
    pub solo: bool,
}

impl TextureSet {
    /// Creates a new Texture Set with default standard PBR channels
    pub fn new(id: usize, name: impl Into<String>, resolution: u32) -> Self {
        let channels = vec![
            TextureChannelDef::new("Base color", "sRGB8"),
            TextureChannelDef::new("Metallic", "L8"),
            TextureChannelDef::new("Roughness", "L8"),
            TextureChannelDef::new("Normal", "RGB16F"),
            TextureChannelDef::new("Height", "L16F"),
        ];

        Self {
            id,
            name: name.into(),
            udim_tile: None,
            resolution,
            channels,
            layer_stack: LayerStack::new(resolution, resolution),
            baked_maps: BakedMaps::default(),
            visible: true,
            solo: false,
        }
    }

    /// Creates a UDIM Tile Texture Set (e.g. tile 1001, 1002)
    pub fn new_udim(id: usize, udim_tile: u32, resolution: u32) -> Self {
        let name = format!("UDIM_{}", udim_tile);
        let mut set = Self::new(id, name, resolution);
        set.udim_tile = Some(udim_tile);
        set
    }

    /// Adds a channel if it doesn't already exist
    pub fn add_channel(&mut self, channel: TextureChannelDef) -> bool {
        if self.channels.iter().any(|c| c.name.eq_ignore_ascii_case(&channel.name)) {
            return false;
        }
        self.channels.push(channel);
        true
    }

    /// Removes a channel by name
    pub fn remove_channel(&mut self, name: &str) -> bool {
        // Do not allow removing Base color or Normal
        if name.eq_ignore_ascii_case("Base color") || name.eq_ignore_ascii_case("Normal") {
            return false;
        }
        let before = self.channels.len();
        self.channels.retain(|c| !c.name.eq_ignore_ascii_case(name));
        self.channels.len() < before
    }

    /// Resizes the Texture Set resolution, recomputing all layer buffers
    pub fn resize(&mut self, new_res: u32) {
        if self.resolution == new_res || new_res == 0 {
            return;
        }
        self.resolution = new_res;
        self.layer_stack.resize(new_res, new_res);
    }
}

/// Manager handling multiple Texture Sets & UDIM workflows
#[derive(Debug, Clone)]
pub struct TextureSetManager {
    pub sets: Vec<TextureSet>,
    pub active_index: usize,
    pub next_id: usize,
    pub is_udim_workflow: bool,
}

impl Default for TextureSetManager {
    fn default() -> Self {
        Self::new(2048)
    }
}

impl TextureSetManager {
    pub fn new(default_res: u32) -> Self {
        let default_set = TextureSet::new(1, "DefaultMaterial", default_res);
        Self {
            sets: vec![default_set],
            active_index: 0,
            next_id: 2,
            is_udim_workflow: false,
        }
    }

    pub fn active(&self) -> &TextureSet {
        &self.sets[self.active_index.min(self.sets.len().saturating_sub(1))]
    }

    pub fn active_mut(&mut self) -> &mut TextureSet {
        let idx = self.active_index.min(self.sets.len().saturating_sub(1));
        &mut self.sets[idx]
    }

    /// Adds a new named Texture Set
    pub fn add_set(&mut self, name: impl Into<String>, resolution: u32) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let new_set = TextureSet::new(id, name, resolution);
        self.sets.push(new_set);
        let new_idx = self.sets.len() - 1;
        self.active_index = new_idx;
        new_idx
    }

    /// Duplicates an existing Texture Set
    pub fn duplicate_set(&mut self, index: usize) -> Option<usize> {
        if index >= self.sets.len() {
            return None;
        }
        let src = &self.sets[index];
        let id = self.next_id;
        self.next_id += 1;
        let mut dup = src.clone();
        dup.id = id;
        dup.name = format!("{}_copy", src.name);
        self.sets.push(dup);
        let new_idx = self.sets.len() - 1;
        self.active_index = new_idx;
        Some(new_idx)
    }

    /// Removes a Texture Set by index (guaranteeing at least 1 set remains)
    pub fn remove_set(&mut self, index: usize) -> bool {
        if self.sets.len() <= 1 || index >= self.sets.len() {
            return false;
        }
        self.sets.remove(index);
        if self.active_index >= self.sets.len() {
            self.active_index = self.sets.len() - 1;
        }
        true
    }

    /// Scans a 3D Mesh and configures Texture Sets:
    /// - If UDIM UV coordinates (u >= 1.0 or v >= 1.0) are detected, generates UDIM tiles (1001, 1002...)
    /// - Otherwise, preserves or creates a single material Texture Set
    pub fn configure_from_mesh(&mut self, mesh: &Mesh, default_res: u32) {
        let udim_tiles = detect_mesh_udim_tiles(mesh);

        if udim_tiles.len() > 1 {
            // Enable UDIM workflow
            self.is_udim_workflow = true;
            self.sets.clear();
            self.next_id = 1;

            for tile in udim_tiles {
                let id = self.next_id;
                self.next_id += 1;
                self.sets.push(TextureSet::new_udim(id, tile, default_res));
            }
            self.active_index = 0;
        } else if self.sets.is_empty() {
            self.is_udim_workflow = false;
            let id = self.next_id;
            self.next_id += 1;
            let mesh_mat_name = if mesh.name.is_empty() {
                "DefaultMaterial".to_string()
            } else {
                format!("{}_Mat", mesh.name.replace(' ', "_"))
            };
            self.sets.push(TextureSet::new(id, mesh_mat_name, default_res));
            self.active_index = 0;
        }
    }
}

/// Detects all UDIM tiles occupied by the mesh UVs
pub fn detect_mesh_udim_tiles(mesh: &Mesh) -> Vec<u32> {
    let mut tiles = BTreeSet::new();

    for uv in &mesh.uvs {
        let u = uv[0];
        let v = uv[1];

        // Discard NaN or infinite UVs
        if !u.is_finite() || !v.is_finite() {
            continue;
        }

        let u_tile = u.floor() as i32;
        let v_tile = v.floor() as i32;

        if u_tile >= 0 && u_tile < 10 && v_tile >= 0 {
            let udim = 1001 + (u_tile as u32) + 10 * (v_tile as u32);
            tiles.insert(udim);
        }
    }

    if tiles.is_empty() {
        vec![1001]
    } else {
        tiles.into_iter().collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_set_channels() {
        let mut set = TextureSet::new(1, "Body", 1024);
        assert_eq!(set.channels.len(), 5);

        // Add Emissive
        let added = set.add_channel(TextureChannelDef::new("Emissive", "sRGB8"));
        assert!(added);
        assert_eq!(set.channels.len(), 6);

        // Adding duplicate channel fails
        let added_dup = set.add_channel(TextureChannelDef::new("emissive", "sRGB8"));
        assert!(!added_dup);

        // Remove Emissive
        let removed = set.remove_channel("Emissive");
        assert!(removed);
        assert_eq!(set.channels.len(), 5);

        // Cannot remove Base color
        let removed_bc = set.remove_channel("Base color");
        assert!(!removed_bc);
    }

    #[test]
    fn test_texture_set_resize() {
        let mut set = TextureSet::new(1, "Test", 64);
        assert_eq!(set.resolution, 64);
        assert_eq!(set.layer_stack.width, 64);

        set.resize(128);
        assert_eq!(set.resolution, 128);
        assert_eq!(set.layer_stack.width, 128);
        assert_eq!(set.layer_stack.height, 128);
    }

    #[test]
    fn test_udim_detection() {
        let mut mesh = Mesh::default();
        // UV in tile 1001: u in [0, 1), v in [0, 1)
        mesh.uvs.push([0.5, 0.5]);
        // UV in tile 1002: u in [1, 2), v in [0, 1) -> 1001 + 1 = 1002
        mesh.uvs.push([1.2, 0.4]);
        // UV in tile 1011: u in [0, 1), v in [1, 2) -> 1001 + 10 = 1011
        mesh.uvs.push([0.8, 1.3]);

        let tiles = detect_mesh_udim_tiles(&mesh);
        assert_eq!(tiles, vec![1001, 1002, 1011]);
    }

    #[test]
    fn test_manager_add_remove() {
        let mut mgr = TextureSetManager::new(64);
        assert_eq!(mgr.sets.len(), 1);

        let idx2 = mgr.add_set("Head", 128);
        assert_eq!(mgr.sets.len(), 2);
        assert_eq!(mgr.active_index, idx2);
        assert_eq!(mgr.active().name, "Head");

        let dup_idx = mgr.duplicate_set(idx2).unwrap();
        assert_eq!(mgr.sets.len(), 3);
        assert_eq!(mgr.sets[dup_idx].name, "Head_copy");

        let removed = mgr.remove_set(dup_idx);
        assert!(removed);
        assert_eq!(mgr.sets.len(), 2);
    }
}
