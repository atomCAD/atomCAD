use wgpu::{BufferUsages, Device, util::DeviceExt};
use bytemuck;
use super::mesh::Mesh;
use crate::renderer::line_mesh::LineMesh;

/// Specifies the type of mesh for rendering purposes
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MeshType {
    /// Triangle mesh rendered with triangles primitive topology
    Triangles,
    /// Line mesh rendered with lines primitive topology
    Lines,
}

/// Represents a mesh on the GPU with vertex and index buffers
pub struct GPUMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub mesh_type: MeshType,
}

impl GPUMesh {
    /// Creates a new empty GPUMesh
    pub fn new_empty(device: &Device, mesh_type: MeshType) -> Self {
        // Create minimal empty buffers
        let (vertex_buffer, vertex_label) = match mesh_type {
            MeshType::Triangles => {
                let vertex_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Empty Triangle Vertex Buffer"),
                        contents: bytemuck::cast_slice(&[] as &[super::mesh::Vertex]),
                        usage: BufferUsages::VERTEX,
                    }
                );
                (vertex_buffer, "Empty Triangle Vertex Buffer")
            },
            MeshType::Lines => {
                let vertex_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Empty Line Vertex Buffer"),
                        contents: bytemuck::cast_slice(&[] as &[crate::renderer::line_mesh::LineVertex]),
                        usage: BufferUsages::VERTEX,
                    }
                );
                (vertex_buffer, "Empty Line Vertex Buffer")
            },
        };

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
            mesh_type,
        }
    }

    /// Creates a new empty triangle mesh
    pub fn new_empty_triangle_mesh(device: &Device) -> Self {
        Self::new_empty(device, MeshType::Triangles)
    }

    /// Creates a new empty line mesh
    pub fn new_empty_line_mesh(device: &Device) -> Self {
        Self::new_empty(device, MeshType::Lines)
    }

    /// Updates the GPUMesh from a CPU Triangle Mesh
    pub fn update_from_mesh(&mut self, device: &Device, mesh: &Mesh, label_prefix: &str) {
        assert!(self.mesh_type == MeshType::Triangles, "Cannot update a non-triangle GPUMesh with a triangle Mesh");
        
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

    /// Updates the GPUMesh from a CPU Line Mesh
    pub fn update_from_line_mesh(&mut self, device: &Device, line_mesh: &LineMesh, label_prefix: &str) {
        assert!(self.mesh_type == MeshType::Lines, "Cannot update a non-line GPUMesh with a LineMesh");
        
        let vertex_label = format!("{} Line Vertex Buffer", label_prefix);
        let index_label = format!("{} Line Index Buffer", label_prefix);

        // TODO: In the future, consider updating buffer data in-place if size permits
        // instead of recreating buffers

        self.vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&vertex_label),
                contents: bytemuck::cast_slice(line_mesh.vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }
        );

        self.index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&index_label),
                contents: bytemuck::cast_slice(line_mesh.indices.as_slice()),
                usage: BufferUsages::INDEX,
            }
        );

        self.num_indices = line_mesh.indices.len() as u32;
    }
}
