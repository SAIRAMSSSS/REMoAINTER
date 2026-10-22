use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Persistent application configuration saved in ~/.remoainter/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub recent_projects: Vec<String>,
    /// Compositing engine preference: `true` = prefer the wgpu compute
    /// compositor when a supported adapter is present (automatic CPU fallback
    /// per stack); `false` = always CPU. Default off until the GPU stack
    /// composite pass reaches full masks/folders parity with the CPU engine.
    #[serde(default)]
    pub prefer_gpu_compositor: bool,
    pub autosave_interval_secs: u64,
    pub undo_depth: usize,
    pub ui_scale: f32,
    pub default_export_dir: String,
    pub default_bake_resolution: u32,
    pub color_management_mode: usize, // 0: ACES Filmic (Default), 1: sRGB Display / Linear Transfer, 2: Neutral Reinhard
    pub custom_hotkeys: HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut hotkeys = HashMap::new();
        hotkeys.insert("Paint Brush".into(), "1".into());
        hotkeys.insert("Eraser".into(), "2".into());
        hotkeys.insert("Projection".into(), "3".into());
        hotkeys.insert("Polygon Fill".into(), "4".into());
        hotkeys.insert("Smudge".into(), "5".into());
        hotkeys.insert("Clone Stamp".into(), "6".into());
        hotkeys.insert("Color Eyedropper".into(), "I".into());
        hotkeys.insert("Material Channel (Lit)".into(), "M".into());
        hotkeys.insert("Solo Channels Cycle".into(), "C".into());
        hotkeys.insert("Mesh Maps Cycle".into(), "B".into());
        hotkeys.insert("Frame Camera".into(), "F".into());
        hotkeys.insert("Turntable Animation".into(), "Y".into());
        hotkeys.insert("Wireframe Overlay".into(), "F4".into());
        hotkeys.insert("Split 3D | 2D View".into(), "F1".into());
        hotkeys.insert("3D View Only".into(), "F2".into());
        hotkeys.insert("2D UV View Only".into(), "F3".into());
        hotkeys.insert("Undo".into(), "Ctrl+Z".into());
        hotkeys.insert("Redo".into(), "Ctrl+Y".into());
        hotkeys.insert("Save Project".into(), "Ctrl+S".into());
        hotkeys.insert("Save Project As".into(), "Ctrl+Shift+S".into());
        hotkeys.insert("Open Project".into(), "Ctrl+O".into());
        hotkeys.insert("Reload Current Mesh".into(), "Ctrl+R".into());
        hotkeys.insert("New Project".into(), "Ctrl+N".into());
        hotkeys.insert("Bake Mesh Maps".into(), "Ctrl+Shift+B".into());
        hotkeys.insert("Group Selected Layers".into(), "Ctrl+G".into());
        hotkeys.insert("Export Textures".into(), "Ctrl+Shift+E".into());

        Self {
            recent_projects: Vec::new(),
            prefer_gpu_compositor: false,
            autosave_interval_secs: 180,
            undo_depth: 64,
            ui_scale: 1.0,
            default_export_dir: "/mnt/shared/REMoAINTER/exports".into(),
            default_bake_resolution: 2048,
            color_management_mode: 0,
            custom_hotkeys: hotkeys,
        }
    }
}

impl AppConfig {
    /// Returns ~/.remoainter/config.json
    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".remoainter").join("config.json")
    }

    /// Loads configuration from ~/.remoainter/config.json, returning defaults if not found or corrupted
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<Self>(&content) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    /// Saves configuration to ~/.remoainter/config.json
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// Registers a newly opened or saved project in recent list, keeping at most 10 items
    pub fn add_recent(&mut self, path: String) {
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path);
        if self.recent_projects.len() > 10 {
            self.recent_projects.truncate(10);
        }
        let _ = self.save();
    }

    /// Clears recent projects list
    pub fn clear_recent(&mut self) {
        self.recent_projects.clear();
        let _ = self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_defaults_and_recent() {
        let mut cfg = AppConfig::default();
        assert_eq!(cfg.autosave_interval_secs, 180);
        assert_eq!(cfg.undo_depth, 64);
        assert_eq!(cfg.color_management_mode, 0);
        assert!(cfg.custom_hotkeys.contains_key("Undo"));
        assert!(cfg.custom_hotkeys.contains_key("Turntable Animation"));

        // Test recent projects queue
        cfg.add_recent("/path/to/project_a.rmp".into());
        cfg.add_recent("/path/to/project_b.rmp".into());
        cfg.add_recent("/path/to/project_a.rmp".into());

        assert_eq!(cfg.recent_projects.len(), 2);
        assert_eq!(cfg.recent_projects[0], "/path/to/project_a.rmp");
        assert_eq!(cfg.recent_projects[1], "/path/to/project_b.rmp");

        cfg.clear_recent();
        assert!(cfg.recent_projects.is_empty());
    }

    #[test]
    fn test_app_config_json_roundtrip() {
        let cfg = AppConfig::default();
        let json = serde_json::to_string_pretty(&cfg).expect("Serialize config");
        let parsed: AppConfig = serde_json::from_str(&json).expect("Deserialize config");
        assert_eq!(parsed.autosave_interval_secs, cfg.autosave_interval_secs);
        assert_eq!(parsed.undo_depth, cfg.undo_depth);
        assert_eq!(parsed.default_bake_resolution, cfg.default_bake_resolution);
    }
}
