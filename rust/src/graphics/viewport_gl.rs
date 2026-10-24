#![allow(dead_code)]
#![allow(unsafe_code)]

use glow::HasContext;
use std::path::{Path, PathBuf};

use super::mesh::Mesh;
use crate::baker::engine::BakedMaps;
use crate::painting::layer::LayerStack;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct GpuSubMesh {
    pub name: String,
    pub index_start: usize,
    pub index_count: usize,
    pub visible: bool,
}

pub struct GpuMesh {
    pub vao: glow::VertexArray,
    pub vbo: glow::Buffer,
    pub ebo: glow::Buffer,
    pub index_count: i32,
    pub submeshes: Vec<GpuSubMesh>,
}

impl GpuMesh {
    pub fn new(gl: &glow::Context, mesh: &Mesh) -> Self {
        unsafe {
            let vao = gl.create_vertex_array().expect("Failed to create VAO");
            let vbo = gl.create_buffer().expect("Failed to create VBO");
            let ebo = gl.create_buffer().expect("Failed to create EBO");

            let mut gpu_mesh = Self {
                vao,
                vbo,
                ebo,
                index_count: 0,
                submeshes: Vec::new(),
            };
            gpu_mesh.update(gl, mesh);
            gpu_mesh
        }
    }

    pub fn update(&mut self, gl: &glow::Context, mesh: &Mesh) {
        let (center, radius) = mesh.compute_bounds();
        let inv_r = 1.0 / radius;

        let n_verts = mesh.vertices.len();
        let mut gl_verts = Vec::with_capacity(n_verts);

        for i in 0..n_verts {
            let p = mesh.vertices[i];
            let n = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            let t = mesh
                .tangents
                .get(i)
                .copied()
                .unwrap_or([1.0, 0.0, 0.0, 1.0]);

            gl_verts.push(GlVertex {
                pos: [
                    (p[0] - center[0]) * inv_r,
                    (p[1] - center[1]) * inv_r,
                    (p[2] - center[2]) * inv_r,
                ],
                normal: n,
                uv,
                tangent: t,
            });
        }

        unsafe {
            gl.bind_vertex_array(Some(self.vao));

            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&gl_verts),
                glow::STATIC_DRAW,
            );

            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
            gl.buffer_data_u8_slice(
                glow::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&mesh.indices),
                glow::STATIC_DRAW,
            );

            let stride = std::mem::size_of::<GlVertex>() as i32;

            // 0: Position
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);

            // 1: Normal
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 12);

            // 2: UV
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 2, glow::FLOAT, false, stride, 24);

            // 3: Tangent
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 4, glow::FLOAT, false, stride, 32);

            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);

            self.index_count = mesh.indices.len() as i32;
            self.submeshes = mesh
                .submeshes
                .iter()
                .map(|sm| GpuSubMesh {
                    name: sm.name.clone(),
                    index_start: sm.index_start,
                    index_count: sm.index_count,
                    visible: true,
                })
                .collect();
        }
    }

    pub fn draw(&self, gl: &glow::Context) {
        if self.index_count == 0 {
            return;
        }
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            if self.submeshes.is_empty() {
                gl.draw_elements(glow::TRIANGLES, self.index_count, glow::UNSIGNED_INT, 0);
            } else {
                for sm in &self.submeshes {
                    if sm.visible && sm.index_count > 0 {
                        let byte_offset = (sm.index_start * std::mem::size_of::<u32>()) as i32;
                        gl.draw_elements(
                            glow::TRIANGLES,
                            sm.index_count as i32,
                            glow::UNSIGNED_INT,
                            byte_offset,
                        );
                    }
                }
            }
            gl.bind_vertex_array(None);
        }
    }

    pub fn set_submesh_visibility(&mut self, idx: usize, visible: bool) {
        if let Some(sm) = self.submeshes.get_mut(idx) {
            sm.visible = visible;
        }
    }

    pub fn isolate_submesh(&mut self, isolated_idx: Option<usize>) {
        match isolated_idx {
            Some(target) => {
                for (i, sm) in self.submeshes.iter_mut().enumerate() {
                    sm.visible = i == target;
                }
            }
            None => {
                for sm in &mut self.submeshes {
                    sm.visible = true;
                }
            }
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.ebo);
        }
    }
}

pub struct PbrShader {
    pub program: glow::Program,
    pub u_model: Option<glow::UniformLocation>,
    pub u_view: Option<glow::UniformLocation>,
    pub u_proj: Option<glow::UniformLocation>,
    pub u_normal_mat: Option<glow::UniformLocation>,
    pub u_cam_pos: Option<glow::UniformLocation>,
    pub u_light_intensity: Option<glow::UniformLocation>,
    pub u_env_rotation: Option<glow::UniformLocation>,
    pub u_exposure: Option<glow::UniformLocation>,
    pub u_display_channel: Option<glow::UniformLocation>,
    pub u_has_env_map: Option<glow::UniformLocation>,
    pub u_has_brdf_lut: Option<glow::UniformLocation>,
    pub u_uv_scale: Option<glow::UniformLocation>,
    pub u_bloom_enabled: Option<glow::UniformLocation>,
    pub u_bloom_intensity: Option<glow::UniformLocation>,
    pub u_bloom_threshold: Option<glow::UniformLocation>,
    pub u_ssao_enabled: Option<glow::UniformLocation>,
    pub u_ssao_intensity: Option<glow::UniformLocation>,
    pub u_dof_enabled: Option<glow::UniformLocation>,
    pub u_dof_focal_distance: Option<glow::UniformLocation>,
    pub u_dof_aperture: Option<glow::UniformLocation>,
    pub u_color_management_mode: Option<glow::UniformLocation>,
}

impl PbrShader {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let vert_src = r#"#version 330 core
layout (location = 0) in vec3 a_position;
layout (location = 1) in vec3 a_normal;
layout (location = 2) in vec2 a_uv;
layout (location = 3) in vec4 a_tangent;

uniform mat4 u_model;
uniform mat4 u_view;
uniform mat4 u_proj;
uniform mat3 u_normal_mat;

out vec3 v_world_pos;
out vec3 v_normal;
out vec2 v_uv;
out mat3 v_tbn;

void main() {
    vec4 world_pos = u_model * vec4(a_position, 1.0);
    v_world_pos = world_pos.xyz;
    v_normal = normalize(u_normal_mat * a_normal);
    v_uv = a_uv;

    vec3 T = normalize(u_normal_mat * a_tangent.xyz);
    T = normalize(T - dot(T, v_normal) * v_normal);
    vec3 B = cross(v_normal, T) * a_tangent.w;
    v_tbn = mat3(T, B, v_normal);

    gl_Position = u_proj * u_view * world_pos;
}
"#;

        let frag_src = r#"#version 330 core
in vec3 v_world_pos;
in vec3 v_normal;
in vec2 v_uv;
in mat3 v_tbn;

out vec4 FragColor;

uniform sampler2D u_base_color_map;
uniform sampler2D u_metallic_map;
uniform sampler2D u_roughness_map;
uniform sampler2D u_normal_map;
uniform sampler2D u_ao_map;
uniform sampler2D u_curvature_map;
uniform sampler2D u_env_map;
uniform sampler2D u_brdf_lut;

uniform vec3 u_cam_pos;
uniform float u_light_intensity;
uniform float u_env_rotation;
uniform float u_exposure;
uniform int u_display_channel;
uniform int u_has_env_map;
uniform int u_has_brdf_lut;
uniform float u_uv_scale;
uniform int u_bloom_enabled;
uniform float u_bloom_intensity;
uniform float u_bloom_threshold;
uniform int u_ssao_enabled;
uniform float u_ssao_intensity;
uniform int u_dof_enabled;
uniform float u_dof_focal_distance;
uniform float u_dof_aperture;
uniform int u_color_management_mode;

const float PI = 3.14159265359;

vec3 aces_film(vec3 x) {
    float a = 2.51;
    float b = 0.03;
    float c = 2.43;
    float d = 0.59;
    float e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

float distribution_ggx(vec3 N, vec3 H, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float NdotH = max(dot(N, H), 0.0);
    float NdotH2 = NdotH * NdotH;
    float denom = (NdotH2 * (a2 - 1.0) + 1.0);
    return a2 / (PI * denom * denom + 0.000001);
}

float geometry_schlick_ggx(float NdotV, float roughness) {
    float r = (roughness + 1.0);
    float k = (r * r) / 8.0;
    return NdotV / (NdotV * (1.0 - k) + k);
}

float geometry_smith(vec3 N, vec3 V, vec3 L, float roughness) {
    float NdotV = max(dot(N, V), 0.0);
    float NdotL = max(dot(N, L), 0.0);
    float ggx2 = geometry_schlick_ggx(NdotV, roughness);
    float ggx1 = geometry_schlick_ggx(NdotL, roughness);
    return ggx1 * ggx2;
}

vec3 fresnel_schlick(float cosTheta, vec3 F0) {
    return F0 + (1.0 - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

vec3 fresnel_schlick_roughness(float cosTheta, vec3 F0, float roughness) {
    return F0 + (max(vec3(1.0 - roughness), F0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

vec3 sample_equirectangular(sampler2D env_map, vec3 dir, float roughness_mip) {
    vec2 uv = vec2(atan(dir.z, dir.x) / (2.0 * PI) + 0.5, asin(clamp(dir.y, -1.0, 1.0)) / PI + 0.5);
    return textureLod(env_map, uv, roughness_mip * 7.0).rgb;
}

void main() {
    vec2 uv = v_uv;

    vec4 base_col_sample = texture(u_base_color_map, uv);
    vec3 albedo = pow(base_col_sample.rgb, vec3(2.2));
    float metallic = texture(u_metallic_map, uv).r;
    float roughness = clamp(texture(u_roughness_map, uv).r, 0.04, 1.0);
    float ao = texture(u_ao_map, v_uv).r;
    float curvature = texture(u_curvature_map, v_uv).r;

    // Normal mapping via TBN with double-sided normal flip
    vec3 n_ts = texture(u_normal_map, uv).rgb * 2.0 - 1.0;
    vec3 N = normalize(v_tbn * n_ts);
    if (!gl_FrontFacing) {
        N = -N;
    }

    // Solo Channel Overrides
    if (u_display_channel == 1) { // Base Color
        FragColor = vec4(base_col_sample.rgb, 1.0);
        return;
    } else if (u_display_channel == 2) { // Roughness
        FragColor = vec4(vec3(roughness), 1.0);
        return;
    } else if (u_display_channel == 3) { // Metallic
        FragColor = vec4(vec3(metallic), 1.0);
        return;
    } else if (u_display_channel == 4) { // Normal
        FragColor = vec4(N * 0.5 + 0.5, 1.0);
        return;
    } else if (u_display_channel == 5) { // AO
        FragColor = vec4(vec3(ao), 1.0);
        return;
    } else if (u_display_channel == 6) { // Curvature
        FragColor = vec4(vec3(curvature), 1.0);
        return;
    }

    vec3 V = normalize(u_cam_pos - v_world_pos);
    float NdotV = max(dot(N, V), 0.0);

    vec3 F0 = vec3(0.04);
    F0 = mix(F0, albedo, metallic);

    float env_rad = radians(u_env_rotation);
    mat3 rot_y = mat3(
        cos(env_rad), 0.0, sin(env_rad),
        0.0, 1.0, 0.0,
        -sin(env_rad), 0.0, cos(env_rad)
    );

    // 3-Point Studio Lighting
    vec3 light_dirs[3];
    vec3 light_colors[3];

    light_dirs[0] = normalize(rot_y * vec3(0.5, 0.8, 0.6));
    light_colors[0] = vec3(1.15, 1.08, 0.98) * 2.6 * u_light_intensity;

    light_dirs[1] = normalize(rot_y * vec3(-0.7, 0.4, -0.4));
    light_colors[1] = vec3(0.85, 0.95, 1.15) * 1.6 * u_light_intensity;

    light_dirs[2] = normalize(rot_y * vec3(0.0, 0.6, -0.8));
    light_colors[2] = vec3(1.0, 1.0, 1.0) * 1.3 * u_light_intensity;

    vec3 Lo = vec3(0.0);
    for (int i = 0; i < 3; ++i) {
        vec3 L = light_dirs[i];
        vec3 H = normalize(V + L);

        float NdotL = max(dot(N, L), 0.0);
        if (NdotL > 0.0) {
            float NDF = distribution_ggx(N, H, roughness);
            float G = geometry_smith(N, V, L, roughness);
            vec3 F = fresnel_schlick(max(dot(H, V), 0.0), F0);

            vec3 numerator = NDF * G * F;
            float denominator = 4.0 * NdotV * NdotL + 0.0001;
            vec3 specular = numerator / denominator;

            vec3 kS = F;
            vec3 kD = vec3(1.0) - kS;
            kD *= 1.0 - metallic;

            Lo += (kD * albedo / PI + specular) * light_colors[i] * NdotL;
        }
    }

    // Split-Sum IBL Ambient Integration
    vec3 ambient = vec3(0.0);
    vec3 R = reflect(-V, N);
    vec3 R_rot = rot_y * R;
    vec3 N_rot = rot_y * N;

    if (u_has_env_map == 1) {
        vec3 prefiltered_color = sample_equirectangular(u_env_map, R_rot, roughness);
        vec3 irradiance = sample_equirectangular(u_env_map, N_rot, 0.85);

        vec3 F_env = fresnel_schlick_roughness(NdotV, F0, roughness);
        vec2 env_brdf = vec2(1.0, 0.0);
        if (u_has_brdf_lut == 1) {
            env_brdf = texture(u_brdf_lut, vec2(NdotV, roughness)).rg;
        } else {
            env_brdf = vec2(1.0 - roughness * 0.5, roughness * 0.1);
        }

        vec3 kS_env = F_env;
        vec3 kD_env = 1.0 - kS_env;
        kD_env *= 1.0 - metallic;

        vec3 diffuse_ibl = irradiance * albedo;
        vec3 specular_ibl = prefiltered_color * (F_env * env_brdf.x + env_brdf.y);

        ambient = (kD_env * diffuse_ibl + specular_ibl) * ao;
    } else {
        vec3 ambient_sky = vec3(0.35, 0.38, 0.42);
        vec3 ambient_ground = vec3(0.18, 0.17, 0.16);
        vec3 irradiance = mix(ambient_ground, ambient_sky, N.y * 0.5 + 0.5);
        vec3 kD_fallback = (1.0 - F0) * (1.0 - metallic);
        ambient = kD_fallback * irradiance * albedo * ao;
    }

    // SSAO enhancement
    if (u_ssao_enabled == 1) {
        float ssao_factor = mix(1.0, ao * (1.0 - max(0.0, 0.5 - curvature) * 0.7), u_ssao_intensity);
        ambient *= ssao_factor;
    }

    vec3 color = Lo + ambient;
    color *= pow(2.0, u_exposure);

    // Depth of Field (DoF) defocus
    if (u_dof_enabled == 1) {
        float dist_to_cam = length(u_cam_pos - v_world_pos);
        float coc = abs(dist_to_cam - u_dof_focal_distance) * u_dof_aperture;
        float dof_factor = clamp(coc * 2.0, 0.0, 0.7);
        vec3 blurred_env = sample_equirectangular(u_env_map, N_rot, 0.95);
        color = mix(color, mix(color, blurred_env * 0.8, 0.5), dof_factor);
    }

    // Bloom highlights extraction
    if (u_bloom_enabled == 1) {
        float lum = dot(color, vec3(0.2126, 0.7152, 0.0722));
        float bloom_factor = clamp((lum - u_bloom_threshold) / max(1.0 - u_bloom_threshold, 0.001), 0.0, 1.0);
        vec3 bloom_color = color * bloom_factor * u_bloom_intensity;
        color += bloom_color;
    }

    // Color Management & Tone Mapping
    if (u_color_management_mode == 0) {
        // ACES Filmic Tone Mapping + sRGB Gamma (Default)
        color = aces_film(color);
        color = pow(color, vec3(1.0 / 2.2));
    } else if (u_color_management_mode == 1) {
        // Raw Linear to sRGB display transfer
        color = pow(clamp(color, 0.0, 1.0), vec3(1.0 / 2.2));
    } else {
        // Neutral / Reinhard Tone Mapping
        color = color / (color + vec3(1.0));
        color = pow(color, vec3(1.0 / 2.2));
    }

    FragColor = vec4(color, 1.0);
}
"#;

        unsafe {
            let program = create_program(gl, vert_src, frag_src)?;
            gl.use_program(Some(program));

            set_sampler_uniform(gl, program, "u_base_color_map", 0);
            set_sampler_uniform(gl, program, "u_metallic_map", 1);
            set_sampler_uniform(gl, program, "u_roughness_map", 2);
            set_sampler_uniform(gl, program, "u_normal_map", 3);
            set_sampler_uniform(gl, program, "u_ao_map", 4);
            set_sampler_uniform(gl, program, "u_curvature_map", 5);
            set_sampler_uniform(gl, program, "u_env_map", 6);
            set_sampler_uniform(gl, program, "u_brdf_lut", 7);

            let u_model = gl.get_uniform_location(program, "u_model");
            let u_view = gl.get_uniform_location(program, "u_view");
            let u_proj = gl.get_uniform_location(program, "u_proj");
            let u_normal_mat = gl.get_uniform_location(program, "u_normal_mat");
            let u_cam_pos = gl.get_uniform_location(program, "u_cam_pos");
            let u_light_intensity = gl.get_uniform_location(program, "u_light_intensity");
            let u_env_rotation = gl.get_uniform_location(program, "u_env_rotation");
            let u_exposure = gl.get_uniform_location(program, "u_exposure");
            let u_display_channel = gl.get_uniform_location(program, "u_display_channel");
            let u_has_env_map = gl.get_uniform_location(program, "u_has_env_map");
            let u_has_brdf_lut = gl.get_uniform_location(program, "u_has_brdf_lut");
            let u_uv_scale = gl.get_uniform_location(program, "u_uv_scale");
            let u_bloom_enabled = gl.get_uniform_location(program, "u_bloom_enabled");
            let u_bloom_intensity = gl.get_uniform_location(program, "u_bloom_intensity");
            let u_bloom_threshold = gl.get_uniform_location(program, "u_bloom_threshold");
            let u_ssao_enabled = gl.get_uniform_location(program, "u_ssao_enabled");
            let u_ssao_intensity = gl.get_uniform_location(program, "u_ssao_intensity");
            let u_dof_enabled = gl.get_uniform_location(program, "u_dof_enabled");
            let u_dof_focal_distance = gl.get_uniform_location(program, "u_dof_focal_distance");
            let u_dof_aperture = gl.get_uniform_location(program, "u_dof_aperture");
            let u_color_management_mode =
                gl.get_uniform_location(program, "u_color_management_mode");

            gl.use_program(None);

            Ok(Self {
                program,
                u_model,
                u_view,
                u_proj,
                u_normal_mat,
                u_cam_pos,
                u_light_intensity,
                u_env_rotation,
                u_exposure,
                u_display_channel,
                u_has_env_map,
                u_has_brdf_lut,
                u_uv_scale,
                u_bloom_enabled,
                u_bloom_intensity,
                u_bloom_threshold,
                u_ssao_enabled,
                u_ssao_intensity,
                u_dof_enabled,
                u_dof_focal_distance,
                u_dof_aperture,
                u_color_management_mode,
            })
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
        }
    }
}

pub struct SkyboxShader {
    pub program: glow::Program,
    pub vao: glow::VertexArray,
    pub vbo: glow::Buffer,
    pub u_inv_view_proj: Option<glow::UniformLocation>,
    pub u_env_rotation: Option<glow::UniformLocation>,
    pub u_blur: Option<glow::UniformLocation>,
    pub u_intensity: Option<glow::UniformLocation>,
    pub u_has_env_map: Option<glow::UniformLocation>,
}

impl SkyboxShader {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let vert_src = r#"#version 330 core
layout (location = 0) in vec2 a_position;
out vec2 v_uv;

void main() {
    v_uv = a_position * 0.5 + 0.5;
    gl_Position = vec4(a_position, 0.9999, 1.0);
}
"#;

        let frag_src = r#"#version 330 core
in vec2 v_uv;
out vec4 FragColor;

uniform mat4 u_inv_view_proj;
uniform sampler2D u_env_map;
uniform float u_env_rotation;
uniform float u_blur;
uniform float u_intensity;
uniform int u_has_env_map;

const float PI = 3.14159265359;

vec3 aces_film(vec3 x) {
    float a = 2.51; float b = 0.03; float c = 2.43; float d = 0.59; float e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0, 1.0);
}

void main() {
    vec4 clip = vec4(v_uv * 2.0 - 1.0, 1.0, 1.0);
    vec4 world = u_inv_view_proj * clip;
    vec3 dir = normalize(world.xyz / world.w);

    float env_rad = radians(u_env_rotation);
    mat3 rot_y = mat3(
        cos(env_rad), 0.0, sin(env_rad),
        0.0, 1.0, 0.0,
        -sin(env_rad), 0.0, cos(env_rad)
    );
    vec3 dir_rot = rot_y * dir;

    vec3 color;
    if (u_has_env_map == 1) {
        vec2 env_uv = vec2(atan(dir_rot.z, dir_rot.x) / (2.0 * PI) + 0.5, asin(clamp(dir_rot.y, -1.0, 1.0)) / PI + 0.5);
        color = textureLod(u_env_map, env_uv, u_blur * 6.0).rgb * u_intensity;
    } else {
        float d = length(v_uv - 0.5);
        color = mix(vec3(0.18, 0.18, 0.19), vec3(0.08, 0.08, 0.09), d * 1.4);
    }

    color = aces_film(color);
    color = pow(color, vec3(1.0 / 2.2));
    FragColor = vec4(color, 1.0);
}
"#;

        unsafe {
            let program = create_program(gl, vert_src, frag_src)?;
            gl.use_program(Some(program));
            set_sampler_uniform(gl, program, "u_env_map", 6);

            let u_inv_view_proj = gl.get_uniform_location(program, "u_inv_view_proj");
            let u_env_rotation = gl.get_uniform_location(program, "u_env_rotation");
            let u_blur = gl.get_uniform_location(program, "u_blur");
            let u_intensity = gl.get_uniform_location(program, "u_intensity");
            let u_has_env_map = gl.get_uniform_location(program, "u_has_env_map");

            let vao = gl.create_vertex_array().expect("VAO for skybox");
            let vbo = gl.create_buffer().expect("VBO for skybox");

            let quad_verts: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];

            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&quad_verts),
                glow::STATIC_DRAW,
            );

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);

            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.use_program(None);

            Ok(Self {
                program,
                vao,
                vbo,
                u_inv_view_proj,
                u_env_rotation,
                u_blur,
                u_intensity,
                u_has_env_map,
            })
        }
    }

    pub fn draw(&self, gl: &glow::Context) {
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
            gl.delete_program(self.program);
        }
    }
}

pub struct GpuTextures {
    pub base_color: glow::Texture,
    pub metallic: glow::Texture,
    pub roughness: glow::Texture,
    pub normal: glow::Texture,
    pub ao: glow::Texture,
    pub curvature: glow::Texture,
    pub width: u32,
    pub height: u32,
}

impl GpuTextures {
    pub fn new(gl: &glow::Context, width: u32, height: u32) -> Self {
        unsafe {
            let base_color = create_texture_2d(
                gl,
                width,
                height,
                glow::RGBA8,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
            );
            let metallic =
                create_texture_2d(gl, width, height, glow::R8, glow::RED, glow::UNSIGNED_BYTE);
            let roughness =
                create_texture_2d(gl, width, height, glow::R8, glow::RED, glow::UNSIGNED_BYTE);
            let normal = create_texture_2d(
                gl,
                width,
                height,
                glow::RGB8,
                glow::RGB,
                glow::UNSIGNED_BYTE,
            );
            let ao = create_texture_2d(gl, width, height, glow::R8, glow::RED, glow::UNSIGNED_BYTE);
            let curvature =
                create_texture_2d(gl, width, height, glow::R8, glow::RED, glow::UNSIGNED_BYTE);

            Self {
                base_color,
                metallic,
                roughness,
                normal,
                ao,
                curvature,
                width,
                height,
            }
        }
    }

    pub fn upload_all(&mut self, gl: &glow::Context, stack: &LayerStack, baked: &BakedMaps) {
        let w = stack.width;
        let h = stack.height;
        let total = (w * h) as usize;

        if w != self.width || h != self.height {
            self.destroy(gl);
            *self = Self::new(gl, w, h);
        }

        let mut bc_bytes = Vec::with_capacity(total * 4);
        let mut met_bytes = Vec::with_capacity(total);
        let mut rgh_bytes = Vec::with_capacity(total);
        let mut nrm_bytes = Vec::with_capacity(total * 3);

        for i in 0..total {
            let c = stack
                .composite_color
                .get(i)
                .copied()
                .unwrap_or([0.5, 0.5, 0.5, 1.0]);
            bc_bytes.push((c[0].clamp(0.0, 1.0) * 255.0) as u8);
            bc_bytes.push((c[1].clamp(0.0, 1.0) * 255.0) as u8);
            bc_bytes.push((c[2].clamp(0.0, 1.0) * 255.0) as u8);
            bc_bytes.push((c[3].clamp(0.0, 1.0) * 255.0) as u8);

            let m = stack.composite_metallic.get(i).copied().unwrap_or(0.0);
            met_bytes.push((m.clamp(0.0, 1.0) * 255.0) as u8);

            let r = stack.composite_roughness.get(i).copied().unwrap_or(0.5);
            rgh_bytes.push((r.clamp(0.0, 1.0) * 255.0) as u8);

            let n = stack
                .composite_normal
                .get(i)
                .copied()
                .unwrap_or([0.5, 0.5, 1.0]);
            nrm_bytes.push((n[0].clamp(0.0, 1.0) * 255.0) as u8);
            nrm_bytes.push((n[1].clamp(0.0, 1.0) * 255.0) as u8);
            nrm_bytes.push((n[2].clamp(0.0, 1.0) * 255.0) as u8);
        }

        let ao_bytes: Vec<u8> = if let Some(ref ao) = baked.ambient_occlusion {
            ao.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
                .collect()
        } else {
            vec![255u8; total]
        };

        let curv_bytes: Vec<u8> = if let Some(ref cur) = baked.curvature {
            cur.iter()
                .map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
                .collect()
        } else {
            vec![128u8; total]
        };

        unsafe {
            update_tex_data(
                gl,
                self.base_color,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                &bc_bytes,
            );
            update_tex_data(
                gl,
                self.metallic,
                w,
                h,
                glow::RED,
                glow::UNSIGNED_BYTE,
                &met_bytes,
            );
            update_tex_data(
                gl,
                self.roughness,
                w,
                h,
                glow::RED,
                glow::UNSIGNED_BYTE,
                &rgh_bytes,
            );
            update_tex_data(
                gl,
                self.normal,
                w,
                h,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                &nrm_bytes,
            );
            update_tex_data(gl, self.ao, w, h, glow::RED, glow::UNSIGNED_BYTE, &ao_bytes);
            update_tex_data(
                gl,
                self.curvature,
                w,
                h,
                glow::RED,
                glow::UNSIGNED_BYTE,
                &curv_bytes,
            );
        }
    }

    pub fn upload_dirty(&mut self, gl: &glow::Context, stack: &LayerStack, bbox: [i32; 4]) {
        let x0 = bbox[0].clamp(0, self.width as i32 - 1) as u32;
        let y0 = bbox[1].clamp(0, self.height as i32 - 1) as u32;
        let x1 = (bbox[2] + 1).clamp(0, self.width as i32) as u32;
        let y1 = (bbox[3] + 1).clamp(0, self.height as i32) as u32;

        if x1 <= x0 || y1 <= y0 {
            return;
        }

        let sub_w = x1 - x0;
        let sub_h = y1 - y0;
        let total = (sub_w * sub_h) as usize;

        let mut bc_sub = Vec::with_capacity(total * 4);
        let mut met_sub = Vec::with_capacity(total);
        let mut rgh_sub = Vec::with_capacity(total);
        let mut nrm_sub = Vec::with_capacity(total * 3);

        for y in y0..y1 {
            for x in x0..x1 {
                let idx = (y * self.width + x) as usize;
                let c = stack
                    .composite_color
                    .get(idx)
                    .copied()
                    .unwrap_or([0.5, 0.5, 0.5, 1.0]);
                bc_sub.push((c[0].clamp(0.0, 1.0) * 255.0) as u8);
                bc_sub.push((c[1].clamp(0.0, 1.0) * 255.0) as u8);
                bc_sub.push((c[2].clamp(0.0, 1.0) * 255.0) as u8);
                bc_sub.push((c[3].clamp(0.0, 1.0) * 255.0) as u8);

                let m = stack.composite_metallic.get(idx).copied().unwrap_or(0.0);
                met_sub.push((m.clamp(0.0, 1.0) * 255.0) as u8);

                let r = stack.composite_roughness.get(idx).copied().unwrap_or(0.5);
                rgh_sub.push((r.clamp(0.0, 1.0) * 255.0) as u8);

                let n = stack
                    .composite_normal
                    .get(idx)
                    .copied()
                    .unwrap_or([0.5, 0.5, 1.0]);
                nrm_sub.push((n[0].clamp(0.0, 1.0) * 255.0) as u8);
                nrm_sub.push((n[1].clamp(0.0, 1.0) * 255.0) as u8);
                nrm_sub.push((n[2].clamp(0.0, 1.0) * 255.0) as u8);
            }
        }

        unsafe {
            update_sub_tex(
                gl,
                self.base_color,
                x0,
                y0,
                sub_w,
                sub_h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                &bc_sub,
            );
            update_sub_tex(
                gl,
                self.metallic,
                x0,
                y0,
                sub_w,
                sub_h,
                glow::RED,
                glow::UNSIGNED_BYTE,
                &met_sub,
            );
            update_sub_tex(
                gl,
                self.roughness,
                x0,
                y0,
                sub_w,
                sub_h,
                glow::RED,
                glow::UNSIGNED_BYTE,
                &rgh_sub,
            );
            update_sub_tex(
                gl,
                self.normal,
                x0,
                y0,
                sub_w,
                sub_h,
                glow::RGB,
                glow::UNSIGNED_BYTE,
                &nrm_sub,
            );
        }
    }

    pub fn bind(&self, gl: &glow::Context) {
        unsafe {
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.base_color));

            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.metallic));

            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.roughness));

            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.normal));

            gl.active_texture(glow::TEXTURE4);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.ao));

            gl.active_texture(glow::TEXTURE5);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.curvature));
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.base_color);
            gl.delete_texture(self.metallic);
            gl.delete_texture(self.roughness);
            gl.delete_texture(self.normal);
            gl.delete_texture(self.ao);
            gl.delete_texture(self.curvature);
        }
    }
}

pub struct WireframeShader {
    pub program: glow::Program,
    pub u_mvp: Option<glow::UniformLocation>,
    pub u_wire_color: Option<glow::UniformLocation>,
}

impl WireframeShader {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let vert_src = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
uniform mat4 u_mvp;
void main() {
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
"#;
        let frag_src = r#"#version 330 core
out vec4 FragColor;
uniform vec4 u_wire_color;
void main() {
    FragColor = u_wire_color;
}
"#;
        unsafe {
            let program = create_program(gl, vert_src, frag_src)?;
            gl.use_program(Some(program));
            let u_mvp = gl.get_uniform_location(program, "u_mvp");
            let u_wire_color = gl.get_uniform_location(program, "u_wire_color");
            gl.use_program(None);
            Ok(Self {
                program,
                u_mvp,
                u_wire_color,
            })
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
        }
    }
}

pub struct GpuGrid {
    pub grid_vao: glow::VertexArray,
    pub grid_vbo: glow::Buffer,
    pub grid_vert_count: i32,
    pub axes_vao: glow::VertexArray,
    pub axes_vbo: glow::Buffer,
    pub axes_vert_count: i32,
}

impl GpuGrid {
    pub fn new(gl: &glow::Context) -> Self {
        unsafe {
            let mut grid_verts: Vec<[f32; 3]> = Vec::new();
            let y_plane = -1.0f32;
            let extent = 2.0f32;
            let step = 0.2f32;
            let steps = (extent / step) as i32;

            for i in -steps..=steps {
                let coord = i as f32 * step;
                // Line parallel to Z
                grid_verts.push([coord, y_plane, -extent]);
                grid_verts.push([coord, y_plane, extent]);
                // Line parallel to X
                grid_verts.push([-extent, y_plane, coord]);
                grid_verts.push([extent, y_plane, coord]);
            }

            let grid_vao = gl.create_vertex_array().expect("Failed to create Grid VAO");
            let grid_vbo = gl.create_buffer().expect("Failed to create Grid VBO");

            gl.bind_vertex_array(Some(grid_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(grid_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&grid_verts),
                glow::STATIC_DRAW,
            );
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);

            let axes_verts: [[f32; 3]; 6] = [
                // X (Red)
                [0.0, y_plane, 0.0],
                [0.6, y_plane, 0.0],
                // Y (Green)
                [0.0, y_plane, 0.0],
                [0.0, y_plane + 0.6, 0.0],
                // Z (Blue)
                [0.0, y_plane, 0.0],
                [0.0, y_plane, 0.6],
            ];

            let axes_vao = gl.create_vertex_array().expect("Failed to create Axes VAO");
            let axes_vbo = gl.create_buffer().expect("Failed to create Axes VBO");

            gl.bind_vertex_array(Some(axes_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(axes_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&axes_verts),
                glow::STATIC_DRAW,
            );
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 12, 0);

            gl.bind_vertex_array(None);

            Self {
                grid_vao,
                grid_vbo,
                grid_vert_count: grid_verts.len() as i32,
                axes_vao,
                axes_vbo,
                axes_vert_count: 6,
            }
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_vertex_array(self.grid_vao);
            gl.delete_buffer(self.grid_vbo);
            gl.delete_vertex_array(self.axes_vao);
            gl.delete_buffer(self.axes_vbo);
        }
    }
}

pub struct ViewportGl {
    pub mesh: GpuMesh,
    pub grid: GpuGrid,
    pub pbr_shader: PbrShader,
    pub skybox_shader: SkyboxShader,
    pub wireframe_shader: WireframeShader,
    pub textures: GpuTextures,
    pub env_texture: Option<glow::Texture>,
    pub brdf_lut_texture: glow::Texture,
    pub active_env_name: String,
    pub exposure: f32,
    pub skybox_visible: bool,
    pub skybox_blur: f32,
    pub skybox_intensity: f32,
    pub uv_scale: f32,
    pub show_wireframe: bool,
    pub show_grid: bool,
    pub bloom_enabled: bool,
    pub bloom_intensity: f32,
    pub bloom_threshold: f32,
    pub ssao_enabled: bool,
    pub ssao_intensity: f32,
    pub dof_enabled: bool,
    pub dof_focal_distance: f32,
    pub dof_aperture: f32,
    pub color_management_mode: usize,
}

impl ViewportGl {
    pub fn new(
        gl: &glow::Context,
        mesh: &Mesh,
        stack: &LayerStack,
        baked: &BakedMaps,
    ) -> Result<Self, String> {
        let gpu_mesh = GpuMesh::new(gl, mesh);
        let grid = GpuGrid::new(gl);
        let pbr_shader = PbrShader::new(gl)?;
        let skybox_shader = SkyboxShader::new(gl)?;
        let wireframe_shader = WireframeShader::new(gl)?;
        let mut textures = GpuTextures::new(gl, stack.width, stack.height);
        textures.upload_all(gl, stack, baked);

        // Generate 2D BRDF LUT (Split-sum 512x512)
        let lut_tex = generate_and_upload_brdf_lut(gl);

        let mut viewport = Self {
            mesh: gpu_mesh,
            grid,
            pbr_shader,
            skybox_shader,
            wireframe_shader,
            textures,
            env_texture: None,
            brdf_lut_texture: lut_tex,
            active_env_name: "Studio Tomoco".to_string(),
            exposure: 0.0,
            skybox_visible: true,
            skybox_blur: 0.25,
            skybox_intensity: 1.0,
            uv_scale: 1.0,
            show_wireframe: false,
            show_grid: true,
            bloom_enabled: false,
            bloom_intensity: 0.8,
            bloom_threshold: 0.85,
            ssao_enabled: true,
            ssao_intensity: 1.0,
            dof_enabled: false,
            dof_focal_distance: 2.5,
            dof_aperture: 0.08,
            color_management_mode: 0,
        };

        // Attempt to load Studio Tomoco by default
        let mut default_env_paths: Vec<PathBuf> = crate::paths::find_assets_dir()
            .map(|a| vec![a.join("environments").join("Studio Tomoco.exr")])
            .unwrap_or_default();
        default_env_paths.push(PathBuf::from("assets/environments/studio_tomoco.png"));
        default_env_paths.retain(|p| p.exists());
        for path in &default_env_paths {
            if Path::new(path).exists() {
                let _ = viewport.load_environment(gl, path);
                break;
            }
        }

        Ok(viewport)
    }

    pub fn set_mesh(&mut self, gl: &glow::Context, mesh: &Mesh) {
        self.mesh.update(gl, mesh);
    }

    pub fn set_submesh_visibility(&mut self, idx: usize, visible: bool) {
        self.mesh.set_submesh_visibility(idx, visible);
    }

    pub fn isolate_submesh(&mut self, isolated_idx: Option<usize>) {
        self.mesh.isolate_submesh(isolated_idx);
    }

    pub fn submeshes(&self) -> &[GpuSubMesh] {
        &self.mesh.submeshes
    }

    pub fn load_environment(
        &mut self,
        gl: &glow::Context,
        path: impl AsRef<Path>,
    ) -> Result<(), String> {
        let path_ref = path.as_ref();
        let stem = path_ref
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Environment");

        let (w, h, rgba_data) = if path_ref.extension().and_then(|s| s.to_str()) == Some("exr") {
            load_exr_hdr(path_ref)?
        } else {
            load_ldr_image(path_ref)?
        };

        unsafe {
            if let Some(old) = self.env_texture {
                gl.delete_texture(old);
            }

            let tex = gl.create_texture().map_err(|e| e.to_string())?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA16F as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::FLOAT,
                Some(bytemuck::cast_slice(&rgba_data)),
            );
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR_MIPMAP_LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);

            self.env_texture = Some(tex);
            self.active_env_name = stem.to_string();
        }

        Ok(())
    }

    pub fn paint(
        &mut self,
        gl: &glow::Context,
        viewport_x: i32,
        viewport_y: i32,
        width: i32,
        height: i32,
        yaw: f32,
        pitch: f32,
        cam_dist: f32,
        light_intensity: f32,
        env_rotation: f32,
        display_channel: usize,
    ) {
        if width <= 0 || height <= 0 {
            return;
        }

        unsafe {
            // Setup viewport and scissor
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(viewport_x, viewport_y, width, height);
            gl.viewport(viewport_x, viewport_y, width, height);

            // Ensure completely opaque 3D rendering (disable egui's ambient blend)
            gl.disable(glow::BLEND);

            // Enable depth testing with 24-bit depth buffer & clear viewport
            gl.clear_color(0.16, 0.16, 0.17, 1.0);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);

            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);

            // Camera transformation
            let aspect = width as f32 / height as f32;
            let proj = glam::Mat4::perspective_rh(45.0f32.to_radians(), aspect, 0.05, 50.0);

            let yaw_rad = yaw.to_radians();
            let pitch_rad = pitch.to_radians();
            let cam_pos = glam::Vec3::new(
                cam_dist * pitch_rad.cos() * yaw_rad.sin(),
                cam_dist * pitch_rad.sin(),
                cam_dist * pitch_rad.cos() * yaw_rad.cos(),
            );

            let view = glam::Mat4::look_at_rh(cam_pos, glam::Vec3::ZERO, glam::Vec3::Y);
            let view_proj = proj * view;
            let inv_view_proj = view_proj.inverse();

            // 1. Render Skybox / Environment background (if enabled and lit mode)
            if self.skybox_visible && display_channel == 0 {
                gl.disable(glow::DEPTH_TEST);
                gl.depth_mask(false);
                gl.use_program(Some(self.skybox_shader.program));

                gl.uniform_matrix_4_f32_slice(
                    self.skybox_shader.u_inv_view_proj.as_ref(),
                    false,
                    &inv_view_proj.to_cols_array(),
                );
                gl.uniform_1_f32(self.skybox_shader.u_env_rotation.as_ref(), env_rotation);
                gl.uniform_1_f32(self.skybox_shader.u_blur.as_ref(), self.skybox_blur);
                gl.uniform_1_f32(
                    self.skybox_shader.u_intensity.as_ref(),
                    self.skybox_intensity,
                );
                gl.uniform_1_i32(
                    self.skybox_shader.u_has_env_map.as_ref(),
                    if self.env_texture.is_some() { 1 } else { 0 },
                );

                if let Some(env_tex) = self.env_texture {
                    gl.active_texture(glow::TEXTURE6);
                    gl.bind_texture(glow::TEXTURE_2D, Some(env_tex));
                }

                self.skybox_shader.draw(gl);
                gl.depth_mask(true);
                gl.enable(glow::DEPTH_TEST);
            }

            // 2. Render 3D Mesh with Per-Fragment Cook-Torrance GGX Lighting
            gl.use_program(Some(self.pbr_shader.program));

            let model = glam::Mat4::IDENTITY;
            let normal_mat = glam::Mat3::IDENTITY;

            gl.uniform_matrix_4_f32_slice(
                self.pbr_shader.u_model.as_ref(),
                false,
                &model.to_cols_array(),
            );
            gl.uniform_matrix_4_f32_slice(
                self.pbr_shader.u_view.as_ref(),
                false,
                &view.to_cols_array(),
            );
            gl.uniform_matrix_4_f32_slice(
                self.pbr_shader.u_proj.as_ref(),
                false,
                &proj.to_cols_array(),
            );
            gl.uniform_matrix_3_f32_slice(
                self.pbr_shader.u_normal_mat.as_ref(),
                false,
                &normal_mat.to_cols_array(),
            );

            gl.uniform_3_f32(
                self.pbr_shader.u_cam_pos.as_ref(),
                cam_pos.x,
                cam_pos.y,
                cam_pos.z,
            );
            gl.uniform_1_f32(self.pbr_shader.u_light_intensity.as_ref(), light_intensity);
            gl.uniform_1_f32(self.pbr_shader.u_env_rotation.as_ref(), env_rotation);
            gl.uniform_1_f32(self.pbr_shader.u_exposure.as_ref(), self.exposure);
            gl.uniform_1_i32(
                self.pbr_shader.u_display_channel.as_ref(),
                display_channel as i32,
            );
            gl.uniform_1_i32(
                self.pbr_shader.u_has_env_map.as_ref(),
                if self.env_texture.is_some() { 1 } else { 0 },
            );
            gl.uniform_1_i32(self.pbr_shader.u_has_brdf_lut.as_ref(), 1);
            gl.uniform_1_f32(self.pbr_shader.u_uv_scale.as_ref(), self.uv_scale);

            gl.uniform_1_i32(
                self.pbr_shader.u_bloom_enabled.as_ref(),
                if self.bloom_enabled { 1 } else { 0 },
            );
            gl.uniform_1_f32(
                self.pbr_shader.u_bloom_intensity.as_ref(),
                self.bloom_intensity,
            );
            gl.uniform_1_f32(
                self.pbr_shader.u_bloom_threshold.as_ref(),
                self.bloom_threshold,
            );

            gl.uniform_1_i32(
                self.pbr_shader.u_ssao_enabled.as_ref(),
                if self.ssao_enabled { 1 } else { 0 },
            );
            gl.uniform_1_f32(
                self.pbr_shader.u_ssao_intensity.as_ref(),
                self.ssao_intensity,
            );

            gl.uniform_1_i32(
                self.pbr_shader.u_dof_enabled.as_ref(),
                if self.dof_enabled { 1 } else { 0 },
            );
            gl.uniform_1_f32(
                self.pbr_shader.u_dof_focal_distance.as_ref(),
                self.dof_focal_distance,
            );
            gl.uniform_1_f32(self.pbr_shader.u_dof_aperture.as_ref(), self.dof_aperture);
            gl.uniform_1_i32(
                self.pbr_shader.u_color_management_mode.as_ref(),
                self.color_management_mode as i32,
            );

            // Bind PBR textures (Units 0..5)
            self.textures.bind(gl);

            // Bind HDR environment (Unit 6)
            if let Some(env_tex) = self.env_texture {
                gl.active_texture(glow::TEXTURE6);
                gl.bind_texture(glow::TEXTURE_2D, Some(env_tex));
            }

            // Bind BRDF LUT (Unit 7)
            gl.active_texture(glow::TEXTURE7);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.brdf_lut_texture));

            // Disable backface culling to ensure solid two-sided geometry (no see-through holes in cutouts)
            gl.disable(glow::CULL_FACE);

            self.mesh.draw(gl);

            // 3. Render Ground Plane Grid and World Axes if enabled
            if self.show_grid {
                gl.use_program(Some(self.wireframe_shader.program));
                gl.uniform_matrix_4_f32_slice(
                    self.wireframe_shader.u_mvp.as_ref(),
                    false,
                    &view_proj.to_cols_array(),
                );

                // Subtle dark slate grid
                gl.uniform_4_f32(
                    self.wireframe_shader.u_wire_color.as_ref(),
                    0.25,
                    0.27,
                    0.30,
                    0.65,
                );
                gl.bind_vertex_array(Some(self.grid.grid_vao));
                gl.draw_arrays(glow::LINES, 0, self.grid.grid_vert_count);

                // World coordinate axes
                gl.bind_vertex_array(Some(self.grid.axes_vao));
                // X (Red)
                gl.uniform_4_f32(
                    self.wireframe_shader.u_wire_color.as_ref(),
                    0.95,
                    0.22,
                    0.22,
                    0.95,
                );
                gl.draw_arrays(glow::LINES, 0, 2);
                // Y (Green)
                gl.uniform_4_f32(
                    self.wireframe_shader.u_wire_color.as_ref(),
                    0.22,
                    0.92,
                    0.22,
                    0.95,
                );
                gl.draw_arrays(glow::LINES, 2, 2);
                // Z (Blue)
                gl.uniform_4_f32(
                    self.wireframe_shader.u_wire_color.as_ref(),
                    0.25,
                    0.55,
                    0.98,
                    0.95,
                );
                gl.draw_arrays(glow::LINES, 4, 2);
            }

            // 4. Render 3D Wireframe Overlay if enabled (reference PBR suite F4)
            if self.show_wireframe {
                gl.enable(glow::POLYGON_OFFSET_LINE);
                gl.polygon_offset(-1.0, -1.0);
                gl.polygon_mode(glow::FRONT_AND_BACK, glow::LINE);
                gl.use_program(Some(self.wireframe_shader.program));
                gl.uniform_matrix_4_f32_slice(
                    self.wireframe_shader.u_mvp.as_ref(),
                    false,
                    &view_proj.to_cols_array(),
                );
                gl.uniform_4_f32(
                    self.wireframe_shader.u_wire_color.as_ref(),
                    0.15,
                    0.75,
                    1.0,
                    0.85,
                ); // reference Cyan
                self.mesh.draw(gl);
                gl.polygon_mode(glow::FRONT_AND_BACK, glow::FILL);
                gl.disable(glow::POLYGON_OFFSET_LINE);
            }

            // Restore GL state cleanly for egui
            gl.enable(glow::BLEND);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_vertex_array(None);
            gl.use_program(None);
        }
    }

    pub fn destroy(&self, gl: &glow::Context) {
        self.mesh.destroy(gl);
        self.grid.destroy(gl);
        self.pbr_shader.destroy(gl);
        self.skybox_shader.destroy(gl);
        self.wireframe_shader.destroy(gl);
        self.textures.destroy(gl);
        unsafe {
            if let Some(env) = self.env_texture {
                gl.delete_texture(env);
            }
            gl.delete_texture(self.brdf_lut_texture);
        }
    }
}

// =========================================================================
// Image & Texture Utilities
// =========================================================================

pub fn list_available_environments() -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(env_dir) = crate::paths::find_assets_dir() {
        dirs.push(env_dir.join("environments"));
    }
    dirs.push(PathBuf::from("assets/environments"));
    // User-local environment pool, if populated.
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            dirs.push(
                PathBuf::from(home)
                    .join(".remoainter")
                    .join("assets")
                    .join("environments"),
            );
        }
    }

    let mut list = Vec::new();
    for d in &dirs {
        let p = d;
        if let Ok(entries) = std::fs::read_dir(p) {
            for entry in entries.flatten() {
                let ep = entry.path();
                if let Some(ext) = ep.extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "exr"
                        || ext_lower == "hdr"
                        || ext_lower == "png"
                        || ext_lower == "jpg"
                    {
                        let stem = ep
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        if !list.iter().any(|(s, _)| s == &stem) {
                            list.push((stem, ep));
                        }
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| a.0.cmp(&b.0));
    list
}

fn load_exr_hdr(path: impl AsRef<Path>) -> Result<(u32, u32, Vec<f32>), String> {
    let path_ref = path.as_ref();
    let image = exr::prelude::read_first_rgba_layer_from_file(
        path_ref,
        |resolution, _| {
            let default_pixel = [0.0f32; 4];
            vec![vec![default_pixel; resolution.width()]; resolution.height()]
        },
        |pixel_vector, position, (r, g, b, a): (f32, f32, f32, f32)| {
            pixel_vector[position.y()][position.x()] = [r, g, b, a];
        },
    )
    .map_err(|e| format!("Failed to read EXR {:?}: {}", path_ref, e))?;

    let h = image.layer_data.channel_data.pixels.len();
    let w = if h > 0 {
        image.layer_data.channel_data.pixels[0].len()
    } else {
        0
    };
    let mut flat = Vec::with_capacity(w * h * 4);
    for row in image.layer_data.channel_data.pixels {
        for px in row {
            flat.extend_from_slice(&px);
        }
    }
    Ok((w as u32, h as u32, flat))
}

fn load_ldr_image(path: impl AsRef<Path>) -> Result<(u32, u32, Vec<f32>), String> {
    let img = image::open(path.as_ref())
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let raw = img.into_raw();
    let mut flat = Vec::with_capacity(raw.len());
    for b in raw {
        flat.push(b as f32 / 255.0);
    }
    Ok((w, h, flat))
}

// =========================================================================
// GL Helper Functions
// =========================================================================

unsafe fn create_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<glow::Program, String> {
    let program = gl.create_program().map_err(|e| e.to_string())?;

    let vs = gl
        .create_shader(glow::VERTEX_SHADER)
        .map_err(|e| e.to_string())?;
    gl.shader_source(vs, vert_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        let log = gl.get_shader_info_log(vs);
        gl.delete_shader(vs);
        gl.delete_program(program);
        return Err(format!("Vertex shader compilation failed: {}", log));
    }

    let fs = gl
        .create_shader(glow::FRAGMENT_SHADER)
        .map_err(|e| e.to_string())?;
    gl.shader_source(fs, frag_src);
    gl.compile_shader(fs);
    if !gl.get_shader_compile_status(fs) {
        let log = gl.get_shader_info_log(fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        gl.delete_program(program);
        return Err(format!("Fragment shader compilation failed: {}", log));
    }

    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);

    if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        gl.delete_program(program);
        return Err(format!("Shader program linking failed: {}", log));
    }

    gl.detach_shader(program, vs);
    gl.detach_shader(program, fs);
    gl.delete_shader(vs);
    gl.delete_shader(fs);

    Ok(program)
}

unsafe fn set_sampler_uniform(gl: &glow::Context, program: glow::Program, name: &str, unit: i32) {
    if let Some(loc) = gl.get_uniform_location(program, name) {
        gl.uniform_1_i32(Some(&loc), unit);
    }
}

unsafe fn create_texture_2d(
    gl: &glow::Context,
    width: u32,
    height: u32,
    internal_format: u32,
    format: u32,
    pixel_type: u32,
) -> glow::Texture {
    let tex = gl.create_texture().expect("Texture creation failed");
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        internal_format as i32,
        width as i32,
        height as i32,
        0,
        format,
        pixel_type,
        None,
    );
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MIN_FILTER,
        glow::LINEAR_MIPMAP_LINEAR as i32,
    );
    gl.tex_parameter_i32(
        glow::TEXTURE_2D,
        glow::TEXTURE_MAG_FILTER,
        glow::LINEAR as i32,
    );
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.bind_texture(glow::TEXTURE_2D, None);
    tex
}

unsafe fn update_tex_data(
    gl: &glow::Context,
    tex: glow::Texture,
    width: u32,
    height: u32,
    format: u32,
    pixel_type: u32,
    data: &[u8],
) {
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        if format == glow::RGBA {
            glow::RGBA8 as i32
        } else if format == glow::RGB {
            glow::RGB8 as i32
        } else {
            glow::R8 as i32
        },
        width as i32,
        height as i32,
        0,
        format,
        pixel_type,
        Some(data),
    );
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.bind_texture(glow::TEXTURE_2D, None);
}

unsafe fn update_sub_tex(
    gl: &glow::Context,
    tex: glow::Texture,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    format: u32,
    pixel_type: u32,
    data: &[u8],
) {
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_sub_image_2d(
        glow::TEXTURE_2D,
        0,
        x as i32,
        y as i32,
        w as i32,
        h as i32,
        format,
        pixel_type,
        glow::PixelUnpackData::Slice(data),
    );
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.bind_texture(glow::TEXTURE_2D, None);
}

/// Generates Brian Karis / Epic Games 512x512 Split-Sum BRDF Integration LUT
pub fn generate_and_upload_brdf_lut(gl: &glow::Context) -> glow::Texture {
    let size = 512;
    let mut lut_bytes = vec![0u8; size * size * 2]; // RG8

    let lut_f32 = compute_brdf_lut(size);

    for i in 0..(size * size) {
        let (a, b) = lut_f32[i];
        lut_bytes[i * 2] = (a.clamp(0.0, 1.0) * 255.0) as u8;
        lut_bytes[i * 2 + 1] = (b.clamp(0.0, 1.0) * 255.0) as u8;
    }

    unsafe {
        let tex = gl.create_texture().expect("BRDF LUT Texture creation");
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RG8 as i32,
            size as i32,
            size as i32,
            0,
            glow::RG,
            glow::UNSIGNED_BYTE,
            Some(&lut_bytes),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        tex
    }
}

/// High-precision numerical computation of BRDF Split-Sum integral for (NdotV, Roughness)
pub fn compute_brdf_lut(size: usize) -> Vec<(f32, f32)> {
    use rayon::prelude::*;

    (0..size)
        .into_par_iter()
        .flat_map(|y| {
            let roughness = ((y as f32 + 0.5) / size as f32).clamp(0.001, 1.0);
            (0..size)
                .map(move |x| {
                    let n_dot_v = ((x as f32 + 0.5) / size as f32).clamp(0.001, 1.0);
                    integrate_brdf(n_dot_v, roughness)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn integrate_brdf(n_dot_v: f32, roughness: f32) -> (f32, f32) {
    let v = glam::Vec3::new((1.0 - n_dot_v * n_dot_v).max(0.0).sqrt(), 0.0, n_dot_v);

    let mut a = 0.0f32;
    let mut b = 0.0f32;
    let n = glam::Vec3::Z;

    let num_samples = 512u32;
    for i in 0..num_samples {
        let xi = hammersley(i, num_samples);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = (2.0 * v.dot(h) * h - v).normalize();

        let n_dot_l = l.z.max(0.0);
        let n_dot_h = h.z.max(0.0);
        let v_dot_h = v.dot(h).max(0.0);

        if n_dot_l > 0.0 {
            let g = geometry_smith_lut(n_dot_v, n_dot_l, roughness);
            let g_vis = (g * v_dot_h) / (n_dot_h * n_dot_v).max(0.0001);
            let fc = (1.0 - v_dot_h).powi(5);

            a += (1.0 - fc) * g_vis;
            b += fc * g_vis;
        }
    }

    (a / num_samples as f32, b / num_samples as f32)
}

fn radical_inverse_vdc(mut bits: u32) -> f32 {
    bits = (bits << 16) | (bits >> 16);
    bits = ((bits & 0x55555555) << 1) | ((bits & 0xAAAAAAAA) >> 1);
    bits = ((bits & 0x33333333) << 2) | ((bits & 0xCCCCCCCC) >> 2);
    bits = ((bits & 0x0F0F0F0F) << 4) | ((bits & 0xF0F0F0F0) >> 4);
    bits = ((bits & 0x00FF00FF) << 8) | ((bits & 0xFF00FF00) >> 8);
    bits as f32 * 2.3283064365386963e-10
}

fn hammersley(i: u32, n: u32) -> glam::Vec2 {
    glam::Vec2::new(i as f32 / n as f32, radical_inverse_vdc(i))
}

fn importance_sample_ggx(xi: glam::Vec2, n: glam::Vec3, roughness: f32) -> glam::Vec3 {
    let a = roughness * roughness;
    let phi = 2.0 * std::f32::consts::PI * xi.x;
    let cos_theta = ((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y)).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    let h = glam::Vec3::new(phi.cos() * sin_theta, phi.sin() * sin_theta, cos_theta);

    let up = if n.z.abs() < 0.999 {
        glam::Vec3::Z
    } else {
        glam::Vec3::X
    };
    let tangent = up.cross(n).normalize();
    let bitangent = n.cross(tangent);

    (tangent * h.x + bitangent * h.y + n * h.z).normalize()
}

fn geometry_smith_lut(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    let k = (roughness * roughness) / 2.0;
    let g1_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let g1_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    g1_v * g1_l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brdf_lut_calculation() {
        let lut = compute_brdf_lut(64);
        assert_eq!(lut.len(), 64 * 64);
        // Test values at grazing angle and mid roughness
        let (a, b) = lut[32 * 64 + 32];
        assert!(a >= 0.0 && a <= 1.0);
        assert!(b >= 0.0 && b <= 1.0);
    }

    #[test]
    fn test_gpu_mesh_submesh_isolation() {
        let dummy_id = std::num::NonZeroU32::new(1).unwrap();
        let mut mesh = GpuMesh {
            vao: glow::NativeVertexArray(dummy_id),
            vbo: glow::NativeBuffer(dummy_id),
            ebo: glow::NativeBuffer(dummy_id),
            index_count: 60,
            submeshes: vec![
                GpuSubMesh {
                    name: "Head".into(),
                    index_start: 0,
                    index_count: 20,
                    visible: true,
                },
                GpuSubMesh {
                    name: "Body".into(),
                    index_start: 20,
                    index_count: 20,
                    visible: true,
                },
                GpuSubMesh {
                    name: "Limbs".into(),
                    index_start: 40,
                    index_count: 20,
                    visible: true,
                },
            ],
        };

        // Hide Body
        mesh.set_submesh_visibility(1, false);
        assert!(mesh.submeshes[0].visible);
        assert!(!mesh.submeshes[1].visible);
        assert!(mesh.submeshes[2].visible);

        // Isolate Limbs (idx 2)
        mesh.isolate_submesh(Some(2));
        assert!(!mesh.submeshes[0].visible);
        assert!(!mesh.submeshes[1].visible);
        assert!(mesh.submeshes[2].visible);

        // Clear isolation (show all)
        mesh.isolate_submesh(None);
        assert!(mesh.submeshes[0].visible);
        assert!(mesh.submeshes[1].visible);
        assert!(mesh.submeshes[2].visible);
    }

    #[test]
    fn test_gpu_grid_dimensions() {
        let extent = 2.0f32;
        let step = 0.2f32;
        let steps = (extent / step) as i32;
        let line_count = (steps * 2 + 1) * 2;
        assert_eq!(line_count, 42); // 21 lines parallel to Z + 21 lines parallel to X
        let vert_count = line_count * 2;
        assert_eq!(vert_count, 84);
    }
}
