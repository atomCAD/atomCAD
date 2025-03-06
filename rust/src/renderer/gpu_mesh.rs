use wgpu::{Buffer, BufferUsages, Device, util::DeviceExt};
use bytemuck;
use super::mesh::Mesh;

/// Represents a mesh on the GPU with vertex and index buffers
pub struct GPUMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl GPUMesh {
    /// Creates a new empty GPUMesh
    pub fn new_empty(device: &Device) -> Self {
        // Create minimal empty buffers
        let vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Empty Vertex Buffer"),
                contents: bytemuck::cast_slice(&[] as &[super::mesh::Vertex]),
                usage: BufferUsages::VERTEX,
            }
        );

        let index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Empty Index Buffer"),
                contents: bytemuck::cast_slice(&[] as &[u32]),
                usage: BufferUsages::INDEX,
            }
        );

        Self {
            vertex_buffer,
            index_buffer,
            num_indices: 0,
        }
    }

    /// Updates the GPUMesh from a CPU Mesh
    pub fn update_from_mesh(&mut self, device: &Device, mesh: &Mesh, label_prefix: &str) {
        let vertex_label = format!("{} Vertex Buffer", label_prefix);
        let index_label = format!("{} Index Buffer", label_prefix);

        // TODO: In the future, consider updating buffer data in-place if size permits
        // instead of recreating buffers

        self.vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&vertex_label),
                contents: bytemuck::cast_slice(mesh.vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }
        );

        self.index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&index_label),
                contents: bytemuck::cast_slice(mesh.indices.as_slice()),
                usage: BufferUsages::INDEX,
            }
        );

        self.num_indices = mesh.indices.len() as u32;
    }
}
