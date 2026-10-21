use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>, // xyz = tangent, w = handedness (+1.0 or -1.0)
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// Creates a smooth, pristine Material Calibration Sphere with continuous UV coordinates and normals
    pub fn create_material_sphere(rings: u32, sectors: u32) -> Self {
        // 1. Try to load authentic MaterialX sphere asset if present
        let sphere_paths = [
            "/mnt/shared/REMoAINTER/assets/test_models/sphere.obj",
            "assets/test_models/sphere.obj",
            "../assets/test_models/sphere.obj",
            "/usr/share/materialx/resources/Geometry/sphere.obj",
        ];
        for p in &sphere_paths {
            if std::path::Path::new(p).exists() {
                if let Ok(mut m) = Self::load_obj(p) {
                    m.name = "Material Sphere".to_string();
                    return m;
                }
            }
        }

        // 2. High-precision procedural sphere fallback with clean continuous UVs
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        let r_step = 1.0 / (rings as f32);
        let s_step = 1.0 / (sectors as f32);

        for r in 0..=rings {
            let phi = std::f32::consts::PI * (r as f32 * r_step);
            let y = phi.cos();
            let sin_phi = phi.sin();

            for s in 0..=sectors {
                let theta = 2.0 * std::f32::consts::PI * (s as f32 * s_step);
                let x = sin_phi * theta.cos();
                let z = sin_phi * theta.sin();

                vertices.push([x, y, z]);
                normals.push([x, y, z]);
                uvs.push([s as f32 * s_step, 1.0 - r as f32 * r_step]);
            }
        }

        for r in 0..rings {
            for s in 0..sectors {
                let cur = r * (sectors + 1) + s;
                let next = cur + sectors + 1;

                indices.push(cur);
                indices.push(cur + 1);
                indices.push(next + 1);

                indices.push(cur);
                indices.push(next + 1);
                indices.push(next);
            }
        }

        let mut mesh = Self {
            name: "Material Sphere".to_string(),
            vertices,
            normals,
            tangents: Vec::new(),
            uvs,
            indices,
        };
        mesh.compute_tangents();
        mesh
    }

    /// Creates a 6-sided Cube primitive with separate vertex normals and [0,1] UVs per face
    pub fn create_cube() -> Self {
        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();

        // 6 faces: (normal, [corners: p00, p10, p11, p01])
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            // Front (+Z)
            ([0.0, 0.0, 1.0], [[-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]]),
            // Back (-Z)
            ([0.0, 0.0, -1.0], [[1.0, -1.0, -1.0], [-1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [1.0, 1.0, -1.0]]),
            // Top (+Y)
            ([0.0, 1.0, 0.0], [[-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]]),
            // Bottom (-Y)
            ([0.0, -1.0, 0.0], [[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, -1.0, 1.0], [-1.0, -1.0, 1.0]]),
            // Right (+X)
            ([1.0, 0.0, 0.0], [[1.0, -1.0, 1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0]]),
            // Left (-X)
            ([-1.0, 0.0, 0.0], [[-1.0, -1.0, -1.0], [-1.0, -1.0, 1.0], [-1.0, 1.0, 1.0], [-1.0, 1.0, -1.0]]),
        ];

        let face_uvs: [[f32; 2]; 4] = [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ];

        for (norm, pts) in faces {
            let base_idx = vertices.len() as u32;
            for i in 0..4 {
                vertices.push(pts[i]);
                normals.push(norm);
                uvs.push(face_uvs[i]);
            }
            indices.push(base_idx);
            indices.push(base_idx + 1);
            indices.push(base_idx + 2);

            indices.push(base_idx);
            indices.push(base_idx + 2);
            indices.push(base_idx + 3);
        }

        let mut mesh = Self {
            name: "Cube".to_string(),
            vertices,
            normals,
            tangents: Vec::new(),
            uvs,
            indices,
        };
        mesh.compute_tangents();
        mesh
    }

    /// Creates a flat 2D Plane primitive facing +Y with clean UVs
    pub fn create_plane() -> Self {
        let vertices = vec![
            [-1.5, 0.0, -1.5],
            [ 1.5, 0.0, -1.5],
            [ 1.5, 0.0,  1.5],
            [-1.5, 0.0,  1.5],
        ];
        let normals = vec![[0.0, 1.0, 0.0]; 4];
        let uvs = vec![
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
        ];
        let indices = vec![0, 2, 1, 0, 3, 2];

        let mut mesh = Self {
            name: "Plane".to_string(),
            vertices,
            normals,
            tangents: Vec::new(),
            uvs,
            indices,
        };
        mesh.compute_tangents();
        mesh
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
                    let u = m.texcoords[i].rem_euclid(1.0);
                    let v = m.texcoords[i + 1].rem_euclid(1.0);
                    uvs.push([u, v]);
                }
            } else {
                uvs.resize(vertices.len(), [0.0, 0.0]);
            }

            for idx in m.indices {
                indices.push(idx + index_offset);
            }
            index_offset = vertices.len() as u32;
        }

        let mut mesh = Self {
            name,
            vertices,
            normals,
            tangents: Vec::new(),
            uvs,
            indices,
        };
        mesh.normalize_to_unit();
        mesh.compute_tangents();
        Ok(mesh)
    }

    /// Serializes the mesh to Wavefront OBJ format string for project archive embedding
    pub fn export_obj_string(&self) -> String {
        let mut s = String::with_capacity(self.vertices.len() * 48);
        s.push_str(&format!("# REMoAINTER Mesh Export: {}\n", self.name));
        for v in &self.vertices {
            s.push_str(&format!("v {:.6} {:.6} {:.6}\n", v[0], v[1], v[2]));
        }
        for n in &self.normals {
            s.push_str(&format!("vn {:.6} {:.6} {:.6}\n", n[0], n[1], n[2]));
        }
        for uv in &self.uvs {
            s.push_str(&format!("vt {:.6} {:.6}\n", uv[0], uv[1]));
        }
        for chunk in self.indices.chunks_exact(3) {
            let i0 = chunk[0] + 1;
            let i1 = chunk[1] + 1;
            let i2 = chunk[2] + 1;
            s.push_str(&format!("f {}/{}/{} {}/{}/{} {}/{}/{}\n", i0, i0, i0, i1, i1, i1, i2, i2, i2));
        }
        s
    }

    /// Deserializes a mesh from Wavefront OBJ string
    pub fn from_obj_string(content: &str, name: &str) -> Result<Self, String> {
        let mut cursor = std::io::Cursor::new(content.as_bytes());
        let (models, _) = tobj::load_obj_buf(
            &mut cursor,
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
            |_| Err(tobj::LoadError::GenericFailure),
        ).map_err(|e| format!("Failed to parse embedded OBJ: {}", e))?;

        if models.is_empty() {
            return Err("Embedded OBJ contains no geometry models".to_string());
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
                    let u = m.texcoords[i].rem_euclid(1.0);
                    let v = m.texcoords[i + 1].rem_euclid(1.0);
                    uvs.push([u, v]);
                }
            } else {
                uvs.resize(vertices.len(), [0.0, 0.0]);
            }
            for idx in m.indices {
                indices.push(idx + index_offset);
            }
            index_offset = vertices.len() as u32;
        }

        let mut mesh = Self {
            name: name.to_string(),
            vertices,
            normals,
            tangents: Vec::new(),
            uvs,
            indices,
        };
        mesh.normalize_to_unit();
        mesh.compute_tangents();
        Ok(mesh)
    }

    /// Automatically centers and scales mesh vertices to fit within unit sphere [-1, 1]
    pub fn normalize_to_unit(&mut self) {
        if self.vertices.is_empty() {
            return;
        }
        let (center, radius) = self.compute_bounds();
        let scale = if radius > 1e-6 { 1.0 / radius } else { 1.0 };
        for v in &mut self.vertices {
            v[0] = (v[0] - center[0]) * scale;
            v[1] = (v[1] - center[1]) * scale;
            v[2] = (v[2] - center[2]) * scale;
        }
    }

    /// Computes MikkTSpace-style orthogonal per-vertex tangents and bitangent signs
    pub fn compute_tangents(&mut self) {
        let n_verts = self.vertices.len();
        if n_verts == 0 || self.uvs.len() != n_verts {
            self.tangents = vec![[1.0, 0.0, 0.0, 1.0]; n_verts];
            return;
        }

        let mut tan1 = vec![glam::Vec3::ZERO; n_verts];
        let mut tan2 = vec![glam::Vec3::ZERO; n_verts];

        let num_tris = self.indices.len() / 3;
        for t in 0..num_tris {
            let i1 = self.indices[t * 3] as usize;
            let i2 = self.indices[t * 3 + 1] as usize;
            let i3 = self.indices[t * 3 + 2] as usize;

            if i1 >= n_verts || i2 >= n_verts || i3 >= n_verts {
                continue;
            }

            let v1 = glam::Vec3::from(self.vertices[i1]);
            let v2 = glam::Vec3::from(self.vertices[i2]);
            let v3 = glam::Vec3::from(self.vertices[i3]);

            let w1 = self.uvs[i1];
            let w2 = self.uvs[i2];
            let w3 = self.uvs[i3];

            let x1 = v2.x - v1.x;
            let x2 = v3.x - v1.x;
            let y1 = v2.y - v1.y;
            let y2 = v3.y - v1.y;
            let z1 = v2.z - v1.z;
            let z2 = v3.z - v1.z;

            let s1 = w2[0] - w1[0];
            let s2 = w3[0] - w1[0];
            let t1 = w2[1] - w1[1];
            let t2 = w3[1] - w1[1];

            let denom = s1 * t2 - s2 * t1;
            let r = if denom.abs() > 1e-7 { 1.0 / denom } else { 1.0 };

            let sdir = glam::Vec3::new(
                (t2 * x1 - t1 * x2) * r,
                (t2 * y1 - t1 * y2) * r,
                (t2 * z1 - t1 * z2) * r,
            );
            let tdir = glam::Vec3::new(
                (s1 * x2 - s2 * x1) * r,
                (s1 * y2 - s2 * y1) * r,
                (s1 * z2 - s2 * z1) * r,
            );

            tan1[i1] += sdir;
            tan1[i2] += sdir;
            tan1[i3] += sdir;

            tan2[i1] += tdir;
            tan2[i2] += tdir;
            tan2[i3] += tdir;
        }

        self.tangents = Vec::with_capacity(n_verts);
        for a in 0..n_verts {
            let n = glam::Vec3::from(self.normals.get(a).copied().unwrap_or([0.0, 1.0, 0.0]));
            let t = tan1[a];

            // Gram-Schmidt orthogonalize
            let tangent = (t - n * n.dot(t)).normalize_or_zero();
            let tangent = if tangent.length_squared() < 0.001 {
                glam::Vec3::X
            } else {
                tangent
            };

            // Calculate handedness (bitangent sign)
            let w = if n.cross(t).dot(tan2[a]) < 0.0 { -1.0 } else { 1.0 };
            self.tangents.push([tangent.x, tangent.y, tangent.z, w]);
        }
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
}

#[derive(Clone, Copy, Debug)]
pub struct RaycastHit {
    pub uv: [f32; 2],
    pub dist: f32,
    pub tri_idx: usize,
    pub normal: [f32; 3],
    pub pos: [f32; 3], // World position
}

impl Mesh {
    /// Fast Möller-Trumbore ray-triangle intersection against normalized mesh
    pub fn raycast_normalized(
        &self,
        ray_origin: glam::Vec3,
        ray_dir: glam::Vec3,
        center: [f32; 3],
        radius: f32,
    ) -> Option<RaycastHit> {
        let inv_r = 1.0 / radius;
        let num_tris = self.indices.len() / 3;
        let mut closest_t = f32::MAX;
        let mut hit = None;

        for t in 0..num_tris {
            let i0 = self.indices[t * 3] as usize;
            let i1 = self.indices[t * 3 + 1] as usize;
            let i2 = self.indices[t * 3 + 2] as usize;

            if i0 >= self.vertices.len() || i1 >= self.vertices.len() || i2 >= self.vertices.len() {
                continue;
            }

            let p0 = self.vertices[i0];
            let p1 = self.vertices[i1];
            let p2 = self.vertices[i2];

            let v0 = glam::Vec3::new(
                (p0[0] - center[0]) * inv_r,
                (p0[1] - center[1]) * inv_r,
                (p0[2] - center[2]) * inv_r,
            );
            let v1 = glam::Vec3::new(
                (p1[0] - center[0]) * inv_r,
                (p1[1] - center[1]) * inv_r,
                (p1[2] - center[2]) * inv_r,
            );
            let v2 = glam::Vec3::new(
                (p2[0] - center[0]) * inv_r,
                (p2[1] - center[1]) * inv_r,
                (p2[2] - center[2]) * inv_r,
            );

            let e1 = v1 - v0;
            let e2 = v2 - v0;
            let h = ray_dir.cross(e2);
            let a = e1.dot(h);

            // Backface culling: ignore back-facing triangles to prevent painting through to the opposite side
            if a < 1e-7 {
                continue;
            }

            let f = 1.0 / a;
            let s = ray_origin - v0;
            let u = f * s.dot(h);

            if u < 0.0 || u > 1.0 {
                continue;
            }

            let q = s.cross(e1);
            let v = f * ray_dir.dot(q);

            if v < 0.0 || (u + v) > 1.0 {
                continue;
            }

            let dist = f * e2.dot(q);
            if dist > 0.0001 && dist < closest_t {
                closest_t = dist;
                let w = 1.0 - u - v;

                let uv0 = self.uvs.get(i0).copied().unwrap_or([0.0, 0.0]);
                let uv1 = self.uvs.get(i1).copied().unwrap_or([0.0, 0.0]);
                let uv2 = self.uvs.get(i2).copied().unwrap_or([0.0, 0.0]);

                let hit_u = (w * uv0[0] + u * uv1[0] + v * uv2[0]).clamp(0.0, 1.0);
                let hit_v = (w * uv0[1] + u * uv1[1] + v * uv2[1]).clamp(0.0, 1.0);

                let n0 = self.normals.get(i0).copied().unwrap_or([0.0, 1.0, 0.0]);
                let n1 = self.normals.get(i1).copied().unwrap_or([0.0, 1.0, 0.0]);
                let n2 = self.normals.get(i2).copied().unwrap_or([0.0, 1.0, 0.0]);
                let hit_norm = [
                    w * n0[0] + u * n1[0] + v * n2[0],
                    w * n0[1] + u * n1[1] + v * n2[1],
                    w * n0[2] + u * n1[2] + v * n2[2],
                ];

                let world_pos = [
                    (w * v0.x + u * v1.x + v * v2.x) * radius + center[0],
                    (w * v0.y + u * v1.y + v * v2.y) * radius + center[1],
                    (w * v0.z + u * v1.z + v * v2.z) * radius + center[2],
                ];

                hit = Some(RaycastHit {
                    uv: [hit_u, hit_v],
                    dist,
                    tri_idx: t,
                    normal: hit_norm,
                    pos: world_pos,
                });
            }
        }

        hit
    }

    /// Finds all connected triangles in the same UV island using BFS
    pub fn find_uv_island(&self, seed_tri: usize) -> Vec<usize> {
        let num_tris = self.indices.len() / 3;
        if seed_tri >= num_tris {
            return Vec::new();
        }

        let mut uv_to_tris: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
        let quant = 10000.0f32;
        for t in 0..num_tris {
            for k in 0..3 {
                let idx = self.indices[t * 3 + k] as usize;
                if idx < self.uvs.len() {
                    let uv = self.uvs[idx];
                    let key = ((uv[0] * quant).round() as i32, (uv[1] * quant).round() as i32);
                    uv_to_tris.entry(key).or_default().push(t);
                }
            }
        }

        let mut visited = vec![false; num_tris];
        let mut queue = std::collections::VecDeque::new();
        let mut island = Vec::new();

        visited[seed_tri] = true;
        queue.push_back(seed_tri);

        while let Some(t) = queue.pop_front() {
            island.push(t);
            for k in 0..3 {
                let idx = self.indices[t * 3 + k] as usize;
                if idx < self.uvs.len() {
                    let uv = self.uvs[idx];
                    let key = ((uv[0] * quant).round() as i32, (uv[1] * quant).round() as i32);
                    if let Some(neighbors) = uv_to_tris.get(&key) {
                        for &n in neighbors {
                            if !visited[n] {
                                visited[n] = true;
                                queue.push_back(n);
                            }
                        }
                    }
                }
            }
        }

        island
    }
}

