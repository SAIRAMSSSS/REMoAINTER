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
    /// Creates a high-fidelity Substance PBR Material Sphere with seamless polar coordinates
    pub fn create_material_sphere(rings: u32, sectors: u32) -> Self {
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
                indices.push(next);
                indices.push(cur + 1);

                indices.push(cur + 1);
                indices.push(next);
                indices.push(next + 1);
            }
        }

        Self {
            name: "Substance Material Sphere".to_string(),
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
}
