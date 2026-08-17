use bytemuck;
use glam::f32::Vec3;

/// Unified vertex format for the single merged transparent impostor mesh.
///
/// Both ghost atoms (spheres) and ghost bonds (cylinders) live in one mesh so
/// that a single back-to-front quad sort can interleave them correctly (see
/// `doc/design_xray_node.md`, §Renderer). The `kind` field selects the branch
/// in `transparent_impostor.wgsl`:
///   - `kind == 0` → atom (sphere): `position_a` is the center, `position_b`
///     unused; `roughness`/`metallic`/`rim_color` carry the atom appearance.
///   - `kind == 1` → bond (cylinder): `position_a`/`position_b` are the
///     endpoints; `roughness`/`metallic` are `0.0` and `rim_color` is `[0.0; 4]`.
///
/// 20 4-byte fields = 80 bytes, no padding (all fields are 4-byte aligned).
/// The hand-written `desc()` offsets are guarded by the Phase 4 layout test.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransparentImpostorVertex {
    pub kind: u32,            // 0 = atom (sphere), 1 = bond (cylinder)
    pub position_a: [f32; 3], // atom: center; bond: start
    pub position_b: [f32; 3], // atom: unused;  bond: end
    pub quad_offset: [f32; 2],
    pub radius: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub roughness: f32,      // atom branch only; bonds write 0.0
    pub metallic: f32,       // atom branch only; bonds write 0.0
    pub rim_color: [f32; 4], // atom branch only; bonds write [0.0; 4]
}

impl TransparentImpostorVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TransparentImpostorVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // kind
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Uint32,
                },
                // position_a
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 1]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // position_b
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // quad_offset
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 7]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // radius
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 9]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
                // color
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 10]>() as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // alpha
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 13]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32,
                },
                // roughness
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 14]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32,
                },
                // metallic
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 15]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32,
                },
                // rim_color
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 16]>() as wgpu::BufferAddress,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// A merged transparent impostor mesh in CPU memory. Each atom or bond is one
/// quad (4 vertices, 6 indices). `quad_centers` records one world-space sort
/// center per quad — the atom center or the bond midpoint — parallel to the
/// quads (`quad_centers.len() * 6 == indices.len()`), feeding the back-to-front
/// depth sort in Phase 5.
pub struct TransparentImpostorMesh {
    pub vertices: Vec<TransparentImpostorVertex>,
    pub indices: Vec<u32>,       // 6 indices per quad (2 triangles)
    pub quad_centers: Vec<Vec3>, // one sort center per quad, parallel to the quads
}

impl Default for TransparentImpostorMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl TransparentImpostorMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            quad_centers: Vec::new(),
        }
    }

    /// Add a transparent atom (sphere) quad. Mirrors
    /// `AtomImpostorMesh::add_atom_quad` plus `alpha`. Records the atom center
    /// as the sort center.
    #[allow(clippy::too_many_arguments)]
    pub fn add_atom_quad(
        &mut self,
        center_position: &Vec3,
        radius: f32,
        color: &[f32; 3],
        roughness: f32,
        metallic: f32,
        rim_color: &[f32; 4],
        alpha: f32,
    ) -> u32 {
        let base_index = self.vertices.len() as u32;

        for &offset in &QUAD_OFFSETS {
            self.vertices.push(TransparentImpostorVertex {
                kind: 0,
                position_a: [center_position.x, center_position.y, center_position.z],
                position_b: [0.0, 0.0, 0.0],
                quad_offset: offset,
                radius,
                color: *color,
                alpha,
                roughness,
                metallic,
                rim_color: *rim_color,
            });
        }

        self.add_quad_indices(base_index);
        self.quad_centers.push(*center_position);
        base_index
    }

    /// Add a transparent bond (cylinder) quad. Mirrors
    /// `BondImpostorMesh::add_bond_quad` plus `alpha`. Records the segment
    /// midpoint as the sort center.
    pub fn add_bond_quad(
        &mut self,
        start_position: &Vec3,
        end_position: &Vec3,
        radius: f32,
        color: &[f32; 3],
        alpha: f32,
    ) -> u32 {
        let base_index = self.vertices.len() as u32;

        for &offset in &QUAD_OFFSETS {
            self.vertices.push(TransparentImpostorVertex {
                kind: 1,
                position_a: [start_position.x, start_position.y, start_position.z],
                position_b: [end_position.x, end_position.y, end_position.z],
                quad_offset: offset,
                radius,
                color: *color,
                alpha,
                roughness: 0.0,
                metallic: 0.0,
                rim_color: [0.0, 0.0, 0.0, 0.0],
            });
        }

        self.add_quad_indices(base_index);
        self.quad_centers
            .push((*start_position + *end_position) * 0.5);
        base_index
    }

    // Emit 6 indices for the quad starting at `base_index` (2 triangles,
    // 0-1-2 / 0-2-3 winding — matching the opaque impostor meshes).
    fn add_quad_indices(&mut self, base_index: u32) {
        self.indices.push(base_index);
        self.indices.push(base_index + 1);
        self.indices.push(base_index + 2);
        self.indices.push(base_index + 2);
        self.indices.push(base_index + 3);
        self.indices.push(base_index);
    }

    /// Returns the total memory usage in bytes for the vertex and index vectors.
    pub fn memory_usage_bytes(&self) -> usize {
        let vertices_bytes = self.vertices.len() * std::mem::size_of::<TransparentImpostorVertex>();
        let indices_bytes = self.indices.len() * std::mem::size_of::<u32>();
        vertices_bytes + indices_bytes
    }
}

/// Quad corner offsets: bottom-left, bottom-right, top-right, top-left.
const QUAD_OFFSETS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
