use bytemuck;
use glam::f32::Vec3;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BondImpostorVertex {
    pub start_position: [f32; 3],     // Bond start position
    pub end_position: [f32; 3],       // Bond end position  
    pub quad_offset: [f32; 2],        // Quad corner offset
    pub radius: f32,                  // Bond radius
    pub color: [f32; 3],             // Bond color
}

impl BondImpostorVertex {
    pub fn new(start_position: &Vec3, end_position: &Vec3, quad_offset: [f32; 2], radius: f32, color: &[f32; 3]) -> Self {
        Self {
            start_position: [start_position.x, start_position.y, start_position.z],
            end_position: [end_position.x, end_position.y, end_position.z],
            quad_offset,
            radius,
            color: *color,
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BondImpostorVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // start_position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // end_position
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // quad_offset
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // radius
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                // color
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 9]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ]
        }
    }
}

/*
 * A Bond Impostor mesh in CPU memory.
 * Each bond is represented as a single quad (4 vertices, 6 indices).
 */
pub struct BondImpostorMesh {
    pub vertices: Vec<BondImpostorVertex>,
    pub indices: Vec<u32>,  // 6 indices per bond (2 triangles per quad)
}

impl BondImpostorMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    // Returns the starting index of the added quad vertices
    pub fn add_bond_quad(&mut self, start_position: &Vec3, end_position: &Vec3, radius: f32, color: &[f32; 3]) -> u32 {
        let base_index = self.vertices.len() as u32;
        
        // Add 4 vertices for quad corners: bottom-left, bottom-right, top-right, top-left
        let quad_offsets = [
            [-1.0, -1.0], // bottom-left
            [1.0, -1.0],  // bottom-right
            [1.0, 1.0],   // top-right
            [-1.0, 1.0],  // top-left
        ];
        
        for &offset in &quad_offsets {
            self.vertices.push(BondImpostorVertex::new(
                start_position,
                end_position,
                offset,
                radius,
                color,
            ));
        }
        
        // Add 6 indices for 2 triangles (quad)
        self.add_quad(base_index, base_index + 1, base_index + 2, base_index + 3);
        
        base_index
    }

    // Add a quad using 4 vertex indices (creates 2 triangles)
    pub fn add_quad(&mut self, index0: u32, index1: u32, index2: u32, index3: u32) {
        // First triangle: 0, 1, 2
        self.indices.push(index0);
        self.indices.push(index1);
        self.indices.push(index2);
        
        // Second triangle: 2, 3, 0
        self.indices.push(index2);
        self.indices.push(index3);
        self.indices.push(index0);
    }

    // Add a bond directly specifying two positions and color
    pub fn add_bond_with_positions(&mut self, 
                                 start_pos: &Vec3, 
                                 end_pos: &Vec3,
                                 radius: f32,
                                 color: &[f32; 3]) {
        self.add_bond_quad(start_pos, end_pos, radius, color);
    }

    /// Returns the total memory usage in bytes for vertices and indices vectors
    pub fn memory_usage_bytes(&self) -> usize {
        let vertices_bytes = self.vertices.len() * std::mem::size_of::<BondImpostorVertex>();
        let indices_bytes = self.indices.len() * std::mem::size_of::<u32>();
        vertices_bytes + indices_bytes
    }
}




