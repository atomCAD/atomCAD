use bytemuck;
use glam::f32::Vec3;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],  // RGB color
}

impl LineVertex {
    pub fn new(position: &Vec3, color: &[f32; 3]) -> Self {
        Self {
            position: [position.x, position.y, position.z],
            color: *color,
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ]
        }
    }
}

/*
 * A Line mesh in CPU memory.
 */
pub struct LineMesh {
    pub vertices: Vec<LineVertex>,
    pub indices: Vec<u32>,  // Each pair of indices represents a line segment
}

impl LineMesh {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    // Returns the index of the added vertex
    pub fn add_vertex(&mut self, vertex: LineVertex) -> u32 {
        let length = self.vertices.len() as u32;
        self.vertices.push(vertex);
        return length;
    }

    // Add a line segment between two vertices
    pub fn add_line(&mut self, index0: u32, index1: u32) {
        self.indices.push(index0);
        self.indices.push(index1);
    }

    // Add a line directly specifying two positions and colors
    pub fn add_line_with_positions(&mut self, 
                                 start_pos: &Vec3, 
                                 start_color: &[f32; 3],
                                 end_pos: &Vec3, 
                                 end_color: &[f32; 3]) {
        let start_index = self.add_vertex(LineVertex::new(start_pos, start_color));
        let end_index = self.add_vertex(LineVertex::new(end_pos, end_color));
        self.add_line(start_index, end_index);
    }
    
    // Convenience method to add a line with the same color for both vertices
    pub fn add_line_with_uniform_color(&mut self, 
                                     start_pos: &Vec3, 
                                     end_pos: &Vec3, 
                                     color: &[f32; 3]) {
        self.add_line_with_positions(start_pos, color, end_pos, color);
    }
}
















