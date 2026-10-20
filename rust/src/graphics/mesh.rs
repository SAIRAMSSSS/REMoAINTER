use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// Creates an authentic Substance Calibration Material Ball with equatorial groove and pedestal base
    pub fn create_material_sphere(rings: u32, sectors: u32) -> Self {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        let r_step = 1.0 / (rings as f32);
        let s_step = 1.0 / (sectors as f32);

        for r in 0..=rings {
            let phi = std::f32::consts::PI * (r as f32 * r_step);
            let y_raw = phi.cos();
            let sin_phi = phi.sin();

            // Calibration Shader Ball Profile:
            // - Equatorial calibration groove near y = 0
            // - Pedestal flare base at the bottom
            let r_mult = if y_raw.abs() < 0.09 {
                0.86 // Indented groove
            } else if y_raw < -0.80 {
                1.18 // Flared pedestal base
            } else {
                1.00 // Smooth sphere
            };

            for s in 0..=sectors {
                let theta = 2.0 * std::f32::consts::PI * (s as f32 * s_step);
                let x = sin_phi * theta.cos() * r_mult;
                let z = sin_phi * theta.sin() * r_mult;
                let y = y_raw;

                let len = (x * x + y * y + z * z).sqrt().max(0.001);
                vertices.push([x, y, z]);
                normals.push([x / len, y / len, z / len]);
                uvs.push([s as f32 * s_step, 1.0 - r as f32 * r_step]);
            }
        }

        for r in 0..rings {
            for s in 0..sectors {
                let cur = r * (sectors + 1) + s;
                let next = cur + sectors + 1;

                indices.push(cur);
                indices.push(next);
                indices.push(cur + 1);

                indices.push(cur + 1);
                indices.push(next);
                indices.push(next + 1);
            }
        }

        Self {
            name: "Material Calibration Ball".to_string(),
            vertices,
            normals,
            uvs,
            indices,
        }
    }

    /// Loads Wavefront OBJ 3D meshes (such as lantern.obj or radio.obj)
    pub fn load_obj(path: impl AsRef<Path>) -> Result<Self, String> {
        let path_ref = path.as_ref();
        let name = path_ref.file_stem().and_then(|s| s.to_str()).unwrap_or("Model").to_string();

        let (models, _) = tobj::load_obj(
            path_ref,
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
        ).map_err(|e| format!("Failed to load OBJ {:?}: {}", path_ref, e))?;

        if models.is_empty() {
            return Err("OBJ file contains no geometry models".to_string());
        }

        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        let mut index_offset = 0u32;

        for model in models {
            let m = model.mesh;
            for i in (0..m.positions.len()).step_by(3) {
                vertices.push([m.positions[i], m.positions[i + 1], m.positions[i + 2]]);
            }

            if !m.normals.is_empty() {
                for i in (0..m.normals.len()).step_by(3) {
                    normals.push([m.normals[i], m.normals[i + 1], m.normals[i + 2]]);
                }
            } else {
                normals.resize(vertices.len(), [0.0, 1.0, 0.0]);
            }

            if !m.texcoords.is_empty() {
                for i in (0..m.texcoords.len()).step_by(2) {
                    uvs.push([m.texcoords[i], 1.0 - m.texcoords[i + 1]]);
                }
            } else {
                uvs.resize(vertices.len(), [0.0, 0.0]);
            }

            for idx in m.indices {
                indices.push(idx + index_offset);
            }
            index_offset = vertices.len() as u32;
        }

        Ok(Self {
            name,
            vertices,
            normals,
            uvs,
            indices,
        })
    }

    pub fn compute_bounds(&self) -> ([f32; 3], f32) {
        if self.vertices.is_empty() {
            return ([0.0, 0.0, 0.0], 1.0);
        }
        let mut min = [f32::MAX, f32::MAX, f32::MAX];
        let mut max = [f32::MIN, f32::MIN, f32::MIN];
        for v in &self.vertices {
            min[0] = min[0].min(v[0]);
            min[1] = min[1].min(v[1]);
            min[2] = min[2].min(v[2]);

            max[0] = max[0].max(v[0]);
            max[1] = max[1].max(v[1]);
            max[2] = max[2].max(v[2]);
        }
        let center = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];
        let dx = max[0] - min[0];
        let dy = max[1] - min[1];
        let dz = max[2] - min[2];
        let radius = (dx * dx + dy * dy + dz * dz).sqrt() * 0.5;
        (center, radius.max(0.001))
    }

    /// Projects 3D mesh onto 2D screen with camera transform, lighting, backface culling & depth sorting
    pub fn project_to_egui_mesh(
        &self,
        screen_center: egui::Pos2,
        view_scale: f32,
        yaw: f32,
        pitch: f32,
        cam_dist: f32,
        texture_id: egui::TextureId,
    ) -> (egui::Mesh, Vec<([egui::Pos2; 3], [[f32; 2]; 3])>) {
        let (center, radius) = self.compute_bounds();
        let inv_r = 1.0 / radius;

        let cy = yaw.cos();
        let sy = yaw.sin();
        let cp = pitch.cos();
        let sp = pitch.sin();

        // Studio Lighting: Key (Warm Top-Right), Fill (Cool Left), Rim (Sharp Backlight)
        let l1 = [0.577f32, 0.707, 0.408];
        let l2 = [-0.707f32, 0.353, 0.612];
        let l3 = [0.0f32, 0.85, -0.52];

        // 1. Transform all vertices
        let n_verts = self.vertices.len();
        let mut proj_pos = Vec::with_capacity(n_verts);
        let mut proj_depth = Vec::with_capacity(n_verts);
        let mut proj_color = Vec::with_capacity(n_verts);

        for i in 0..n_verts {
            let v = self.vertices[i];
            let norm = self.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);

            // Center & normalize
            let px = (v[0] - center[0]) * inv_r;
            let py = (v[1] - center[1]) * inv_r;
            let pz = (v[2] - center[2]) * inv_r;

            // Rotate Yaw (around Y)
            let x1 = px * cy + pz * sy;
            let y1 = py;
            let z1 = -px * sy + pz * cy;

            // Rotate Pitch (around X)
            let x2 = x1;
            let y2 = y1 * cp - z1 * sp;
            let z2 = y1 * sp + z1 * cp;

            // Camera space Z
            let z_cam = (z2 + cam_dist).max(0.1);
            let inv_z = 1.0 / z_cam;

            let sx = screen_center.x + (x2 * inv_z) * view_scale;
            let sy = screen_center.y - (y2 * inv_z) * view_scale;

            proj_pos.push(egui::pos2(sx, sy));
            proj_depth.push(z_cam);

            // Rotate Normal to camera/view space
            let nx1 = norm[0] * cy + norm[2] * sy;
            let ny1 = norm[1];
            let nz1 = -norm[0] * sy + norm[2] * cy;

            let nx2 = nx1;
            let ny2 = ny1 * cp - nz1 * sp;
            let nz2 = ny1 * sp + nz1 * cp;
            let n = [nx2, ny2, nz2];

            // View vector V from vertex to camera
            let vx = -x2;
            let vy = -y2;
            let vz = z_cam;
            let v_len = (vx * vx + vy * vy + vz * vz).sqrt().max(0.0001);
            let v = [vx / v_len, vy / v_len, vz / v_len];

            // 1. Key Light Diffuse & Specular (Blinn-Phong / GGX)
            let n_dot_l1 = (n[0] * l1[0] + n[1] * l1[1] + n[2] * l1[2]).max(0.0);
            let h1 = [l1[0] + v[0], l1[1] + v[1], l1[2] + v[2]];
            let h1_len = (h1[0] * h1[0] + h1[1] * h1[1] + h1[2] * h1[2]).sqrt().max(0.0001);
            let n_dot_h1 = ((n[0] * h1[0] + n[1] * h1[1] + n[2] * h1[2]) / h1_len).max(0.0);
            let spec1 = n_dot_h1.powf(28.0);

            // 2. Fill Light
            let n_dot_l2 = (n[0] * l2[0] + n[1] * l2[1] + n[2] * l2[2]).max(0.0);
            let h2 = [l2[0] + v[0], l2[1] + v[1], l2[2] + v[2]];
            let h2_len = (h2[0] * h2[0] + h2[1] * h2[1] + h2[2] * h2[2]).sqrt().max(0.0001);
            let n_dot_h2 = ((n[0] * h2[0] + n[1] * h2[1] + n[2] * h2[2]) / h2_len).max(0.0);
            let spec2 = n_dot_h2.powf(16.0);

            // 3. Rim / Backlight
            let n_dot_l3 = (n[0] * l3[0] + n[1] * l3[1] + n[2] * l3[2]).max(0.0);
            let h3 = [l3[0] + v[0], l3[1] + v[1], l3[2] + v[2]];
            let h3_len = (h3[0] * h3[0] + h3[1] * h3[1] + h3[2] * h3[2]).sqrt().max(0.0001);
            let n_dot_h3 = ((n[0] * h3[0] + n[1] * h3[1] + n[2] * h3[2]) / h3_len).max(0.0);
            let spec3 = n_dot_h3.powf(40.0);

            // 4. Fresnel reflection at grazing angles
            let n_dot_v = (n[0] * v[0] + n[1] * v[1] + n[2] * v[2]).max(0.0);
            let fresnel = (1.0 - n_dot_v).powf(3.5) * 0.35;

            // 5. Studio ground / sky ambient
            let ambient = 0.28 + (n[1] * 0.5 + 0.5) * 0.15;

            // Combine into rich studio tone
            let diffuse = n_dot_l1 * 0.65 + n_dot_l2 * 0.25 + n_dot_l3 * 0.20 + ambient;
            let specular = spec1 * 0.70 + spec2 * 0.25 + spec3 * 0.45 + fresnel;

            let r = ((diffuse * 0.98 + specular * 0.95).clamp(0.12, 1.0) * 255.0) as u8;
            let g = ((diffuse * 0.96 + specular * 0.95).clamp(0.12, 1.0) * 255.0) as u8;
            let b = ((diffuse * 1.00 + specular * 1.00).clamp(0.12, 1.0) * 255.0) as u8;

            proj_color.push(egui::Color32::from_rgb(r, g, b));
        }

        // 2. Filter & Depth-sort Triangles
        struct VisibleTri {
            indices: [u32; 3],
            avg_depth: f32,
        }

        let num_tris = self.indices.len() / 3;
        let mut visible = Vec::with_capacity(num_tris);

        for t in 0..num_tris {
            let i0 = self.indices[t * 3] as usize;
            let i1 = self.indices[t * 3 + 1] as usize;
            let i2 = self.indices[t * 3 + 2] as usize;

            if i0 >= n_verts || i1 >= n_verts || i2 >= n_verts {
                continue;
            }

            let p0 = proj_pos[i0];
            let p1 = proj_pos[i1];
            let p2 = proj_pos[i2];

            // 2D screen cross product for backface culling
            let cross = (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x);
            if cross <= 0.0 {
                continue; // Back-facing
            }

            let avg_z = proj_depth[i0] + proj_depth[i1] + proj_depth[i2];
            visible.push(VisibleTri {
                indices: [i0 as u32, i1 as u32, i2 as u32],
                avg_depth: avg_z,
            });
        }

        // Sort triangles from back to front (Painter's algorithm)
        visible.sort_unstable_by(|a, b| b.avg_depth.partial_cmp(&a.avg_depth).unwrap_or(std::cmp::Ordering::Equal));

        // 3. Assemble egui::Mesh and screen hit-test triangles
        let mut egui_mesh = egui::Mesh::with_texture(texture_id);
        let mut hit_tris = Vec::with_capacity(visible.len());

        for tri in visible {
            let mut tri_idx = [0u32; 3];
            let mut screen_pts = [egui::Pos2::ZERO; 3];
            let mut uv_pts = [[0.0f32; 2]; 3];

            for k in 0..3 {
                let orig_i = tri.indices[k] as usize;
                let uv = self.uvs.get(orig_i).copied().unwrap_or([0.0, 0.0]);
                let idx = egui_mesh.vertices.len() as u32;
                egui_mesh.vertices.push(egui::epaint::Vertex {
                    pos: proj_pos[orig_i],
                    uv: egui::pos2(uv[0], uv[1]),
                    color: proj_color[orig_i],
                });
                tri_idx[k] = idx;
                screen_pts[k] = proj_pos[orig_i];
                uv_pts[k] = uv;
            }

            egui_mesh.add_triangle(tri_idx[0], tri_idx[1], tri_idx[2]);
            hit_tris.push((screen_pts, uv_pts));
        }

        (egui_mesh, hit_tris)
    }

    /// Point-in-triangle ray hit test returning interpolated UV
    pub fn hit_test_uv(hit_tris: &[([egui::Pos2; 3], [[f32; 2]; 3])], mouse_pos: egui::Pos2) -> Option<[f32; 2]> {
        for (pts, uvs) in hit_tris.iter().rev() {
            let p0 = pts[0];
            let p1 = pts[1];
            let p2 = pts[2];

            let v0 = egui::vec2(p2.x - p0.x, p2.y - p0.y);
            let v1 = egui::vec2(p1.x - p0.x, p1.y - p0.y);
            let v2 = egui::vec2(mouse_pos.x - p0.x, mouse_pos.y - p0.y);

            let dot00 = v0.dot(v0);
            let dot01 = v0.dot(v1);
            let dot02 = v0.dot(v2);
            let dot11 = v1.dot(v1);
            let dot12 = v1.dot(v2);

            let inv_denom = 1.0 / (dot00 * dot11 - dot01 * dot01 + 0.000001);
            let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
            let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;

            if u >= 0.0 && v >= 0.0 && (u + v) <= 1.0 {
                let w0 = 1.0 - u - v;
                let w1 = v;
                let w2 = u;

                let hit_u = w0 * uvs[0][0] + w1 * uvs[1][0] + w2 * uvs[2][0];
                let hit_v = w0 * uvs[0][1] + w1 * uvs[1][1] + w2 * uvs[2][1];

                return Some([hit_u.clamp(0.0, 1.0), hit_v.clamp(0.0, 1.0)]);
            }
        }
        None
    }
}
