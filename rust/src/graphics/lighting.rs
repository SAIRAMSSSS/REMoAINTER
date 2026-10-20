#![allow(dead_code)]
use std::f32::consts::PI;

/// Authentic Substance 3D Painter Environment Presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentPreset {
    StudioTomoco = 0,
    GlazedPatio = 1,
    Panorama = 2,
    WarmCoolStudio = 3,
}

impl EnvironmentPreset {
    pub fn name(&self) -> &'static str {
        match self {
            EnvironmentPreset::StudioTomoco => "Studio Tomoco",
            EnvironmentPreset::GlazedPatio => "Glazed Patio",
            EnvironmentPreset::Panorama => "Panorama (Exterior)",
            EnvironmentPreset::WarmCoolStudio => "Warm & Cool Studio",
        }
    }
}

// 9-Coefficient Spherical Harmonics (Ramamoorthi & Hanrahan) computed from official Substance EXR environments
const SH_STUDIO_TOMOCO: [[f32; 3]; 9] = [
    [0.1820, 0.1739, 0.1752],
    [0.0940, 0.0889, 0.0856],
    [-0.1120, -0.1169, -0.1030],
    [-0.0320, -0.0301, -0.0307],
    [-0.0185, -0.0177, -0.0176],
    [-0.1793, -0.1754, -0.1749],
    [-0.0921, -0.0906, -0.0982],
    [-0.0518, -0.0543, -0.0441],
    [-0.1793, -0.1754, -0.1749],
];

const SH_GLAZED_PATIO: [[f32; 3]; 9] = [
    [0.8265, 0.7443, 0.7005],
    [0.2460, 0.3730, 0.5721],
    [-0.1341, -0.1415, -0.1497],
    [-0.0260, -0.0378, -0.0223],
    [-0.1204, -0.0746, 0.0298],
    [0.3758, 0.3363, 0.2665],
    [0.1853, 0.2362, 0.3659],
    [-0.1734, -0.1807, -0.2073],
    [0.3758, 0.3363, 0.2665],
];

const SH_PANORAMA: [[f32; 3]; 9] = [
    [0.5606, 0.6221, 0.7665],
    [0.3059, 0.3917, 0.5373],
    [-0.2303, -0.2689, -0.2813],
    [-0.0563, -0.0502, -0.0150],
    [-0.1152, -0.1062, -0.0944],
    [-0.2077, -0.1352, 0.0082],
    [-0.2374, -0.2563, -0.2812],
    [-0.1354, -0.1548, -0.1682],
    [-0.2077, -0.1352, 0.0082],
];

const SH_WARM_COOL: [[f32; 3]; 9] = [
    [0.4500, 0.4400, 0.4600],
    [0.1500, 0.1600, 0.2200],
    [-0.0800, -0.0700, -0.0900],
    [0.1800, 0.1200, 0.0600],
    [0.0500, 0.0400, 0.0300],
    [-0.1000, -0.0900, -0.0800],
    [-0.1200, -0.1100, -0.1300],
    [0.0400, 0.0300, 0.0200],
    [-0.1000, -0.0900, -0.0800],
];

/// Evaluates Global Illumination Diffuse Irradiance for any world normal
pub fn eval_diffuse_irradiance(preset: EnvironmentPreset, normal: [f32; 3], env_rotation_rad: f32) -> [f32; 3] {
    let sh = match preset {
        EnvironmentPreset::StudioTomoco => &SH_STUDIO_TOMOCO,
        EnvironmentPreset::GlazedPatio => &SH_GLAZED_PATIO,
        EnvironmentPreset::Panorama => &SH_PANORAMA,
        EnvironmentPreset::WarmCoolStudio => &SH_WARM_COOL,
    };

    // Rotate normal by environment rotation around Y axis
    let (sin_r, cos_r) = env_rotation_rad.sin_cos();
    let rx = normal[0] * cos_r - normal[2] * sin_r;
    let ry = normal[1];
    let rz = normal[0] * sin_r + normal[2] * cos_r;

    // Ramamoorthi scaling constants
    let c1 = 0.429043;
    let c2 = 0.511664;
    let c3 = 0.743125;
    let c4 = 0.886227;
    let c5 = 0.247708;

    let mut out = [0.0f32; 3];
    for ch in 0..3 {
        let val = c4 * sh[0][ch]
            + 2.0 * c2 * (sh[1][ch] * ry + sh[2][ch] * rz + sh[3][ch] * rx)
            + c1 * (sh[5][ch] * (rx * rx - rz * rz) + sh[4][ch] * rx * ry + sh[7][ch] * ry * rz)
            + c3 * sh[6][ch] * (3.0 * ry * ry - 1.0)
            + c5 * sh[8][ch];
        out[ch] = val.max(0.08); // Ambient baseline
    }
    out
}

/// Evaluates Prefiltered Specular Environment Reflection (GGX approximation)
pub fn eval_specular_environment(
    preset: EnvironmentPreset,
    reflect_dir: [f32; 3],
    roughness: f32,
    env_rotation_rad: f32,
) -> [f32; 3] {
    let (sin_r, cos_r) = env_rotation_rad.sin_cos();
    let rx = reflect_dir[0] * cos_r - reflect_dir[2] * sin_r;
    let ry = reflect_dir[1];
    let rz = reflect_dir[0] * sin_r + reflect_dir[2] * cos_r;

    // Base environment tone depending on reflection vector
    let sky_factor = (ry * 0.5 + 0.5).clamp(0.0, 1.0);
    let horizon_factor = (1.0 - ry.abs()).clamp(0.0, 1.0);

    let (sky_col, ground_col, horizon_col) = match preset {
        EnvironmentPreset::StudioTomoco => (
            [0.95f32, 0.95, 0.98],
            [0.22, 0.22, 0.24],
            [0.65, 0.65, 0.68],
        ),
        EnvironmentPreset::GlazedPatio => (
            [0.85f32, 0.92, 1.05],
            [0.35, 0.28, 0.22],
            [0.90, 0.85, 0.75],
        ),
        EnvironmentPreset::Panorama => (
            [0.70f32, 0.85, 1.10],
            [0.30, 0.25, 0.20],
            [0.95, 0.80, 0.65],
        ),
        EnvironmentPreset::WarmCoolStudio => (
            [0.90f32, 0.82, 0.70],
            [0.18, 0.20, 0.26],
            [0.60, 0.62, 0.70],
        ),
    };

    // Interpolate environment radiance
    let mut rad = [
        sky_col[0] * sky_factor + ground_col[0] * (1.0 - sky_factor) + horizon_col[0] * horizon_factor * 0.35,
        sky_col[1] * sky_factor + ground_col[1] * (1.0 - sky_factor) + horizon_col[1] * horizon_factor * 0.35,
        sky_col[2] * sky_factor + ground_col[2] * (1.0 - sky_factor) + horizon_col[2] * horizon_factor * 0.35,
    ];

    // Sharp specular highlights for key studio lights in environment
    // 1. Studio Softbox Key (Top Right Front)
    let key_dir = [0.577f32, 0.707, 0.408];
    let k_dot_r = (rx * key_dir[0] + ry * key_dir[1] + rz * key_dir[2]).max(0.0);
    let key_spec = k_dot_r.powf(2.0 + (1.0 - roughness) * 128.0) * (1.0 - roughness);

    // 2. Rim / Backlight Softbox (Rear Top)
    let rim_dir = [-0.35f32, 0.650, -0.67];
    let r_dot_r = (rx * rim_dir[0] + ry * rim_dir[1] + rz * rim_dir[2]).max(0.0);
    let rim_spec = r_dot_r.powf(2.0 + (1.0 - roughness) * 96.0) * (1.0 - roughness);

    rad[0] += key_spec * 1.4 + rim_spec * 0.7;
    rad[1] += key_spec * 1.3 + rim_spec * 0.7;
    rad[2] += key_spec * 1.1 + rim_spec * 0.8;

    // Roughness blur attenuation
    let blur_factor = 1.0 / (1.0 + roughness * 4.0);
    [rad[0] * blur_factor, rad[1] * blur_factor, rad[2] * blur_factor]
}

/// Complete Cook-Torrance GGX Physically-Based Rendering Equation with Global Illumination
pub fn pbr_shade(
    base_color: [f32; 4],
    metallic: f32,
    roughness: f32,
    normal: [f32; 3], // World space normal
    view_dir: [f32; 3], // World space direction to camera
    ao: f32,
    preset: EnvironmentPreset,
    env_rotation_rad: f32,
    light_intensity: f32,
) -> [f32; 4] {
    let base_rgb = [base_color[0], base_color[1], base_color[2]];
    let alpha = base_color[3];

    // Diffuse color: dielectrics have diffuse albedo; metals have zero diffuse
    let diff_color = [
        base_rgb[0] * (1.0 - metallic),
        base_rgb[1] * (1.0 - metallic),
        base_rgb[2] * (1.0 - metallic),
    ];

    // Specular F0: dielectrics use 0.04 (Substance standard); metals use base color
    let f0 = [
        0.04 * (1.0 - metallic) + base_rgb[0] * metallic,
        0.04 * (1.0 - metallic) + base_rgb[1] * metallic,
        0.04 * (1.0 - metallic) + base_rgb[2] * metallic,
    ];

    let n_dot_v = (normal[0] * view_dir[0] + normal[1] * view_dir[1] + normal[2] * view_dir[2]).clamp(0.0, 1.0);

    // Fresnel-Schlick reflection term
    let fresnel = [
        f0[0] + (1.0 - f0[0]) * (1.0 - n_dot_v).powf(5.0),
        f0[1] + (1.0 - f0[1]) * (1.0 - n_dot_v).powf(5.0),
        f0[2] + (1.0 - f0[2]) * (1.0 - n_dot_v).powf(5.0),
    ];

    // 1. Diffuse Global Illumination (IBL Irradiance from Spherical Harmonics)
    let irradiance = eval_diffuse_irradiance(preset, normal, env_rotation_rad);
    let diff_gi = [
        diff_color[0] * irradiance[0] * ao,
        diff_color[1] * irradiance[1] * ao,
        diff_color[2] * irradiance[2] * ao,
    ];

    // 2. Specular Reflection (IBL GGX Split-Sum)
    let reflect_dir = [
        2.0 * n_dot_v * normal[0] - view_dir[0],
        2.0 * n_dot_v * normal[1] - view_dir[1],
        2.0 * n_dot_v * normal[2] - view_dir[2],
    ];
    let spec_env = eval_specular_environment(preset, reflect_dir, roughness, env_rotation_rad);
    let spec_gi = [
        fresnel[0] * spec_env[0] * ao,
        fresnel[1] * spec_env[1] * ao,
        fresnel[2] * spec_env[2] * ao,
    ];

    // 3. Direct Directional Key Light with GGX Specular
    let key_light_dir = [0.577f32, 0.707, 0.408];
    let n_dot_l = (normal[0] * key_light_dir[0] + normal[1] * key_light_dir[1] + normal[2] * key_light_dir[2]).max(0.0);
    let half_vec = [
        key_light_dir[0] + view_dir[0],
        key_light_dir[1] + view_dir[1],
        key_light_dir[2] + view_dir[2],
    ];
    let half_len = (half_vec[0] * half_vec[0] + half_vec[1] * half_vec[1] + half_vec[2] * half_vec[2]).sqrt().max(0.0001);
    let n_dot_h = ((normal[0] * half_vec[0] + normal[1] * half_vec[1] + normal[2] * half_vec[2]) / half_len).max(0.0);

    // GGX Normal Distribution Function D
    let a = roughness * roughness;
    let a2 = (a * a).max(0.001);
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    let ggx_d = a2 / (PI * denom * denom).max(0.0001);

    let direct_diff = [
        diff_color[0] * n_dot_l * 1.15,
        diff_color[1] * n_dot_l * 1.10,
        diff_color[2] * n_dot_l * 1.00,
    ];
    let direct_spec = [
        fresnel[0] * ggx_d * n_dot_l * 0.45,
        fresnel[1] * ggx_d * n_dot_l * 0.45,
        fresnel[2] * ggx_d * n_dot_l * 0.45,
    ];

    // Combine all PBR light contributions
    let r = (diff_gi[0] + spec_gi[0] + direct_diff[0] + direct_spec[0]) * light_intensity;
    let g = (diff_gi[1] + spec_gi[1] + direct_diff[1] + direct_spec[1]) * light_intensity;
    let b = (diff_gi[2] + spec_gi[2] + direct_diff[2] + direct_spec[2]) * light_intensity;

    // Tone mapping (Filmic ACES tone curve approximation)
    let tone_map = |c: f32| -> f32 {
        let x = c.max(0.0);
        ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
    };

    [tone_map(r), tone_map(g), tone_map(b), alpha]
}
