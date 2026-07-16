use bytemuck;
use glam::f32::Vec3;

/// Vertex format for atom-label glyph quads (Phase 3 of
/// `doc/design_atom_labels.md`).
///
/// One quad per **character**. The billboard is expanded in the vertex shader
/// from the atom's anchor plus a plane offset, exactly the way
/// `atom_impostor.wgsl` expands a sphere quad — but the glyph layout (pen
/// advance, centering, the padded SDF cell) is already baked into
/// `plane_offset` CPU-side, so the shader stays trivial.
///
/// **No color attribute.** Fill and outline are shader constants
/// (`label.wgsl`), so a per-vertex color would put the same white on every
/// vertex in the scene. A per-rule `label_color` is deliberate future work and
/// re-enters as an additive attribute.
///
/// 8 f32 = 32 bytes, no padding. The hand-written `desc()` offsets are guarded
/// by `rust/tests/renderer/label_mesh_test.rs`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LabelVertex {
    /// Atom center, world space.
    pub anchor_position: [f32; 3],
    /// Offset within the billboard plane, world units (glyph layout + centering
    /// already applied).
    pub plane_offset: [f32; 2],
    /// Push toward the eye along `camera_backward()`: the atom's displayed
    /// radius plus an epsilon, so the label clears its own sphere.
    pub depth_offset: f32,
    /// Atlas UV for this corner.
    pub glyph_uv: [f32; 2],
}

impl LabelVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LabelVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // anchor_position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // plane_offset
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // depth_offset
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                // glyph_uv
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

/// An atom-label mesh in CPU memory: one quad (4 vertices, 6 indices) per
/// character of every labeled atom.
///
/// Unlike `TransparentImpostorMesh` this carries no sort centers — labels write
/// depth and are not sorted (§Pipeline changes of `doc/design_atom_labels.md`),
/// which is the single biggest reason this is a smaller job than `xray` was.
#[derive(Debug, Clone)]
pub struct LabelMesh {
    pub vertices: Vec<LabelVertex>,
    pub indices: Vec<u32>, // 6 indices per quad (2 triangles)
}

impl Default for LabelMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl LabelMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Add one glyph quad.
    ///
    /// `min` / `max` are the padded SDF cell's bottom-left / top-right corners in
    /// the billboard plane, world units, relative to the anchor. `uv_min` /
    /// `uv_max` are the cell's top-left / bottom-right atlas UVs — UVs are y
    /// **down** while the plane offsets are y **up**, so the two are crossed on
    /// the vertical axis here.
    pub fn add_glyph_quad(
        &mut self,
        anchor_position: &Vec3,
        min: [f32; 2],
        max: [f32; 2],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        depth_offset: f32,
    ) -> u32 {
        let base_index = self.vertices.len() as u32;
        let anchor = [anchor_position.x, anchor_position.y, anchor_position.z];

        // Bottom-left, bottom-right, top-right, top-left — matching the impostor
        // meshes' corner order and the 0-1-2 / 2-3-0 winding below.
        let corners = [
            ([min[0], min[1]], [uv_min[0], uv_max[1]]),
            ([max[0], min[1]], [uv_max[0], uv_max[1]]),
            ([max[0], max[1]], [uv_max[0], uv_min[1]]),
            ([min[0], max[1]], [uv_min[0], uv_min[1]]),
        ];

        for (plane_offset, glyph_uv) in corners {
            self.vertices.push(LabelVertex {
                anchor_position: anchor,
                plane_offset,
                depth_offset,
                glyph_uv,
            });
        }

        self.indices.push(base_index);
        self.indices.push(base_index + 1);
        self.indices.push(base_index + 2);
        self.indices.push(base_index + 2);
        self.indices.push(base_index + 3);
        self.indices.push(base_index);

        base_index
    }

    /// Returns the total memory usage in bytes for the vertex and index vectors.
    pub fn memory_usage_bytes(&self) -> usize {
        let vertices_bytes = self.vertices.len() * std::mem::size_of::<LabelVertex>();
        let indices_bytes = self.indices.len() * std::mem::size_of::<u32>();
        vertices_bytes + indices_bytes
    }
}
