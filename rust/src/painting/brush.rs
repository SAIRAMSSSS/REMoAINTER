#![allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaShape {
    Circle,
    SoftRound,
    Square,
    Gaussian,
    Dirt,
    Grunge,
    Concrete,
    Scratches,
}

#[derive(Debug, Clone)]
pub struct Brush {
    pub size: f32,
    pub min_size_percent: f32,
    pub flow: f32,
    pub opacity: f32,
    pub spacing: f32,
    pub angle: f32,
    pub follow_path: bool,
    pub lazy_mouse: bool,
    pub lazy_radius: f32,

    // Channels enabled
    pub paint_color: bool,
    pub paint_metallic: bool,
    pub paint_roughness: bool,
    pub paint_normal: bool,
    pub paint_height: bool,

    // Channel values
    pub color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub normal: [f32; 3],
    pub height: f32,

    pub alpha_shape: AlphaShape,
    pub alpha_name: String,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 32.0,
            min_size_percent: 1.0,
            flow: 1.0,
            opacity: 1.0,
            spacing: 0.15,
            angle: 0.0,
            follow_path: true,
            lazy_mouse: true,
            lazy_radius: 18.0,

            paint_color: true,
            paint_metallic: true,
            paint_roughness: true,
            paint_normal: false,
            paint_height: false,

            color: [0.85, 0.82, 0.78, 1.0], // Cream base
            metallic: 0.0,
            roughness: 0.35,
            normal: [0.5, 0.5, 1.0],
            height: 0.0,

            alpha_shape: AlphaShape::Circle,
            alpha_name: "Basic Shape".to_string(),
        }
    }
}
