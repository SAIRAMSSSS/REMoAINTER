#![allow(dead_code)]
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AlphaMaskData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

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
    Custom,
}

/// Pen pressure response curve mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureCurve {
    Linear,
    EaseIn,
    EaseOut,
    Smooth, // S-curve
}

impl PressureCurve {
    pub fn evaluate(&self, pressure: f32) -> f32 {
        let p = pressure.clamp(0.0, 1.0);
        match self {
            PressureCurve::Linear => p,
            PressureCurve::EaseIn => p * p,
            PressureCurve::EaseOut => p * (2.0 - p),
            PressureCurve::Smooth => p * p * (3.0 - 2.0 * p),
        }
    }
}

/// LazyMouse Stroke Stabilizer
#[derive(Debug, Clone, Copy)]
pub struct LazyMouseTracker {
    pub enabled: bool,
    pub radius: f32,
    pub current_pos: [f32; 2],
    pub is_dragging: bool,
}

impl Default for LazyMouseTracker {
    fn default() -> Self {
        Self {
            enabled: true,
            radius: 18.0,
            current_pos: [0.0, 0.0],
            is_dragging: false,
        }
    }
}

impl LazyMouseTracker {
    pub fn update(&mut self, pointer_pos: [f32; 2]) -> [f32; 2] {
        if !self.enabled {
            self.current_pos = pointer_pos;
            return pointer_pos;
        }

        if !self.is_dragging {
            self.current_pos = pointer_pos;
            self.is_dragging = true;
            return pointer_pos;
        }

        let dx = pointer_pos[0] - self.current_pos[0];
        let dy = pointer_pos[1] - self.current_pos[1];
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > self.radius {
            let excess = dist - self.radius;
            let dir_x = dx / dist;
            let dir_y = dy / dist;
            self.current_pos[0] += dir_x * excess;
            self.current_pos[1] += dir_y * excess;
        }

        self.current_pos
    }

    pub fn reset(&mut self) {
        self.is_dragging = false;
    }
}

#[derive(Debug, Clone)]
pub struct Brush {
    pub size: f32,
    pub min_size_percent: f32,
    pub hardness: f32,
    pub flow: f32,
    pub opacity: f32,
    pub spacing: f32,
    pub angle: f32,
    pub follow_path: bool,
    pub lazy_mouse: bool,
    pub lazy_radius: f32,

    // Jitter & Pressure Dynamics
    pub size_jitter: f32,
    pub angle_jitter: f32,
    pub flow_jitter: f32,
    pub pressure_curve: PressureCurve,
    pub pressure_affects_size: bool,
    pub pressure_affects_flow: bool,

    // 3-Axis Symmetry
    pub symmetry_x: bool,
    pub symmetry_y: bool,
    pub symmetry_z: bool,

    // Stencil Overlay
    pub stencil: Option<Arc<AlphaMaskData>>,
    pub stencil_scale: f32,
    pub stencil_angle: f32,

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
    pub custom_alpha: Option<Arc<AlphaMaskData>>,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 32.0,
            min_size_percent: 1.0,
            hardness: 0.85,
            flow: 1.0,
            opacity: 1.0,
            spacing: 0.15,
            angle: 0.0,
            follow_path: true,
            lazy_mouse: true,
            lazy_radius: 18.0,

            size_jitter: 0.0,
            angle_jitter: 0.0,
            flow_jitter: 0.0,
            pressure_curve: PressureCurve::Smooth,
            pressure_affects_size: true,
            pressure_affects_flow: true,

            symmetry_x: false,
            symmetry_y: false,
            symmetry_z: false,

            stencil: None,
            stencil_scale: 1.0,
            stencil_angle: 0.0,

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
            custom_alpha: None,
        }
    }
}

impl Brush {
    /// Computes effective brush parameters for an individual stamp factoring in pressure curves and jitters
    pub fn compute_stamp_params(&self, pressure: Option<f32>, stamp_seed: u32) -> (f32, f32, f32, f32) {
        let mut eff_size = self.size;
        let mut eff_flow = self.flow;
        let mut eff_angle = self.angle;

        if let Some(p) = pressure {
            let curve_p = self.pressure_curve.evaluate(p);
            if self.pressure_affects_size {
                let min_ratio = (self.min_size_percent / 100.0).clamp(0.01, 1.0);
                eff_size = self.size * (min_ratio + (1.0 - min_ratio) * curve_p);
            }
            if self.pressure_affects_flow {
                eff_flow = self.flow * curve_p;
            }
        }

        // Pseudo-random deterministic hash jitter based on stamp_seed
        if self.size_jitter > 0.001 {
            let h = ((stamp_seed.wrapping_mul(1664525).wrapping_add(1013904223)) >> 16) as f32 / 65535.0; // 0..1
            let jitter_factor = 1.0 - (h * self.size_jitter);
            eff_size *= jitter_factor.clamp(0.05, 1.0);
        }
        if self.flow_jitter > 0.001 {
            let h = ((stamp_seed.wrapping_mul(22695477).wrapping_add(1)) >> 16) as f32 / 65535.0;
            let jitter_factor = 1.0 - (h * self.flow_jitter);
            eff_flow *= jitter_factor.clamp(0.01, 1.0);
        }
        if self.angle_jitter > 0.001 {
            let h = ((stamp_seed.wrapping_mul(1103515245).wrapping_add(12345)) >> 16) as f32 / 65535.0; // 0..1
            eff_angle = (eff_angle + (h * 2.0 - 1.0) * 180.0 * self.angle_jitter).rem_euclid(360.0);
        }

        (eff_size, eff_flow, self.opacity, eff_angle)
    }
}

/// Evaluates the normalized alpha stamp at local offset (dx, dy) within radius
pub fn sample_brush_alpha(
    dx: f32,
    dy: f32,
    radius: f32,
    hardness: f32,
    angle_rad: f32,
    brush: &Brush,
) -> f32 {
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > radius {
        return 0.0;
    }

    let norm_dist = dist / radius.max(1.0);
    let edge_falloff = if norm_dist <= hardness {
        1.0
    } else {
        (1.0 - (norm_dist - hardness) / (1.0 - hardness).max(0.001)).clamp(0.0, 1.0)
    };

    // Rotate sample coordinates by brush angle
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let rx = dx * cos_a - dy * sin_a;
    let ry = dx * sin_a + dy * cos_a;

    let shape_alpha = if let Some(ref ca) = brush.custom_alpha {
        let u = ((rx / radius + 1.0) * 0.5 * (ca.width as f32)) as u32;
        let v = ((ry / radius + 1.0) * 0.5 * (ca.height as f32)) as u32;
        if u < ca.width && v < ca.height {
            ca.data[(v * ca.width + u) as usize] as f32 / 255.0
        } else {
            0.0
        }
    } else {
        match brush.alpha_shape {
            AlphaShape::Circle => 1.0,
            AlphaShape::Square => {
                let max_coord = (rx.abs()).max(ry.abs()) / radius;
                if max_coord <= 1.0 { 1.0 } else { 0.0 }
            }
            AlphaShape::SoftRound => (1.0 - norm_dist).powf(2.0),
            AlphaShape::Gaussian => (-4.0 * norm_dist * norm_dist).exp(),
            _ => 1.0,
        }
    };

    let stencil_factor = if let Some(ref st) = brush.stencil {
        let sc = brush.stencil_scale.max(0.01);
        let su = ((rx / (radius * sc) + 1.0) * 0.5 * (st.width as f32)) as u32;
        let sv = ((ry / (radius * sc) + 1.0) * 0.5 * (st.height as f32)) as u32;
        if su < st.width && sv < st.height {
            st.data[(sv * st.width + su) as usize] as f32 / 255.0
        } else {
            0.0
        }
    } else {
        1.0
    };

    (edge_falloff * shape_alpha * stencil_factor).clamp(0.0, 1.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_mouse_stabilizer() {
        let mut tracker = LazyMouseTracker {
            enabled: true,
            radius: 10.0,
            current_pos: [0.0, 0.0],
            is_dragging: false,
        };

        // First click initializes position
        let p0 = tracker.update([50.0, 50.0]);
        assert_eq!(p0, [50.0, 50.0]);

        // Drag within radius (dist < 10) should NOT move the brush tip
        let p1 = tracker.update([55.0, 50.0]); // dist = 5.0 <= 10.0
        assert_eq!(p1, [50.0, 50.0]);

        // Drag beyond radius (dist = 20) moves brush tip towards pointer
        let p2 = tracker.update([70.0, 50.0]); // dist = 20.0 > 10.0
        assert_eq!(p2, [60.0, 50.0]);
    }

    #[test]
    fn test_pressure_curves() {
        let p = 0.5;
        let linear = PressureCurve::Linear.evaluate(p);
        assert!((linear - 0.5).abs() < 1e-5);

        let ease_in = PressureCurve::EaseIn.evaluate(p);
        assert!((ease_in - 0.25).abs() < 1e-5);

        let ease_out = PressureCurve::EaseOut.evaluate(p);
        assert!((ease_out - 0.75).abs() < 1e-5);
    }

    #[test]
    fn test_sample_brush_alpha() {
        let brush = Brush::default();
        let center_alpha = sample_brush_alpha(0.0, 0.0, 16.0, 0.5, 0.0, &brush);
        assert_eq!(center_alpha, 1.0);

        let outside_alpha = sample_brush_alpha(20.0, 0.0, 16.0, 0.5, 0.0, &brush);
        assert_eq!(outside_alpha, 0.0);
    }

    #[test]
    fn test_brush_dynamics_and_stamp_params() {
        let mut brush = Brush::default();
        brush.size = 32.0;
        brush.flow = 1.0;
        brush.pressure_affects_size = true;
        brush.pressure_affects_flow = true;
        brush.pressure_curve = PressureCurve::EaseIn;
        brush.min_size_percent = 20.0;

        // With pressure 0.5, EaseIn is 0.25
        let (size, flow, _, _) = brush.compute_stamp_params(Some(0.5), 42);
        // size: 32.0 * (0.2 + 0.8 * 0.25) = 32.0 * 0.4 = 12.8
        assert!((size - 12.8).abs() < 1e-3);
        assert!((flow - 0.25).abs() < 1e-3);
    }

    #[test]
    fn test_brush_symmetry_axes() {
        let mut brush = Brush::default();
        assert!(!brush.symmetry_x);
        assert!(!brush.symmetry_y);
        assert!(!brush.symmetry_z);

        brush.symmetry_x = true;
        brush.symmetry_y = true;
        assert!(brush.symmetry_x && brush.symmetry_y);
    }
}
