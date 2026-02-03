use wgpu::{BufferUsages, Device, util::DeviceExt};
use bytemuck;
use super::mesh::Mesh;
use crate::renderer::line_mesh::LineMesh;
use crate::renderer::atom_impostor_mesh::AtomImpostorMesh;
use crate::renderer::bond_impostor_mesh::BondImpostorMesh;
use glam::Mat4;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelUniform {
    model_matrix: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
}

impl Default for ModelUniform {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelUniform {
    pub fn new() -> Self {
        Self {
            model_matrix: Mat4::IDENTITY.to_cols_array_2d(),
            normal_matrix: Mat4::IDENTITY.to_cols_array_2d(),
        }
    }

    pub fn update_from_transform(&mut self, transform: &crate::util::transform::Transform) {
        // Convert the transform to a model matrix
        let translation = transform.translation.as_vec3();
        let rotation = transform.rotation.as_quat();
        
        // Create the model matrix
        let model_matrix = Mat4::from_rotation_translation(rotation, translation);
        self.model_matrix = model_matrix.to_cols_array_2d();
        
        // Calculate the normal matrix (inverse transpose of the model matrix)
        let normal_matrix = model_matrix.inverse().transpose();
        self.normal_matrix = normal_matrix.to_cols_array_2d();
    }

}

/// Specifies the type of mesh for rendering purposes
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MeshType {
    /// Triangle mesh rendered with triangles primitive topology
    Triangles,
    /// Line mesh rendered with lines primitive topology
    Lines,
    /// Atom impostor mesh rendered as quads with sphere ray-casting
    AtomImpostors,
    /// Bond impostor mesh rendered as quads with cylinder ray-casting
    BondImpostors,
}

/// Represents a mesh on the GPU with vertex and index buffers and its own transform
pub struct GPUMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub mesh_type: MeshType,
    pub model_buffer: wgpu::Buffer,
    pub model_bind_group: wgpu::BindGroup,
}

impl GPUMesh {
    /// Creates a new empty GPUMesh
    pub fn new_empty(device: &Device, mesh_type: MeshType, model_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        // Create minimal empty buffers
        let (vertex_buffer, _vertex_label) = match mesh_type {
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
            MeshType::AtomImpostors => {
                let vertex_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Empty Atom Impostor Vertex Buffer"),
                        contents: bytemuck::cast_slice(&[] as &[crate::renderer::atom_impostor_mesh::AtomImpostorVertex]),
                        usage: BufferUsages::VERTEX,
                    }
                );
                (vertex_buffer, "Empty Atom Impostor Vertex Buffer")
            },
            MeshType::BondImpostors => {
                let vertex_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Empty Bond Impostor Vertex Buffer"),
                        contents: bytemuck::cast_slice(&[] as &[crate::renderer::bond_impostor_mesh::BondImpostorVertex]),
                        usage: BufferUsages::VERTEX,
                    }
                );
                (vertex_buffer, "Empty Bond Impostor Vertex Buffer")
            },
        };

        let index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Empty Index Buffer"),
                contents: bytemuck::cast_slice(&[] as &[u32]),
                usage: BufferUsages::INDEX,
            }
        );

        // Create and initialize model buffer with identity transform
        let model_uniform = ModelUniform::new();
        let model_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Mesh Model Buffer"),
                contents: bytemuck::cast_slice(&[model_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
        
        // Create the model bind group for this mesh
        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: model_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: model_buffer.as_entire_binding(),
                }
            ],
            label: Some("Mesh Model Bind Group"),
        });

        Self {
            vertex_buffer,
            index_buffer,
            num_indices: 0,
            mesh_type,
            model_buffer,
            model_bind_group,
        }
    }

    /// Creates a new empty triangle mesh
    pub fn new_empty_triangle_mesh(device: &Device, model_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        Self::new_empty(device, MeshType::Triangles, model_bind_group_layout)
    }

    /// Creates a new empty line mesh
    pub fn new_empty_line_mesh(device: &Device, model_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        Self::new_empty(device, MeshType::Lines, model_bind_group_layout)
    }

    /// Creates a new empty atom impostor mesh
    pub fn new_empty_atom_impostor_mesh(device: &Device, model_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        Self::new_empty(device, MeshType::AtomImpostors, model_bind_group_layout)
    }

    /// Creates a new empty bond impostor mesh
    pub fn new_empty_bond_impostor_mesh(device: &Device, model_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        Self::new_empty(device, MeshType::BondImpostors, model_bind_group_layout)
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
        
        // Note: The model buffer and bind group remain unchanged
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
        
        // Note: The model buffer and bind group remain unchanged
    }

    /// Updates the GPUMesh from a CPU Atom Impostor Mesh
    pub fn update_from_atom_impostor_mesh(&mut self, device: &Device, atom_impostor_mesh: &AtomImpostorMesh, label_prefix: &str) {
        assert!(self.mesh_type == MeshType::AtomImpostors, "Cannot update a non-atom-impostor GPUMesh with an AtomImpostorMesh");
        
        let vertex_label = format!("{} Atom Impostor Vertex Buffer", label_prefix);
        let index_label = format!("{} Atom Impostor Index Buffer", label_prefix);

        // TODO: In the future, consider updating buffer data in-place if size permits
        // instead of recreating buffers

        self.vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&vertex_label),
                contents: bytemuck::cast_slice(atom_impostor_mesh.vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }
        );

        self.index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&index_label),
                contents: bytemuck::cast_slice(atom_impostor_mesh.indices.as_slice()),
                usage: BufferUsages::INDEX,
            }
        );

        self.num_indices = atom_impostor_mesh.indices.len() as u32;
        
        // Note: The model buffer and bind group remain unchanged
    }

    /// Updates the GPUMesh from a CPU Bond Impostor Mesh
    pub fn update_from_bond_impostor_mesh(&mut self, device: &Device, bond_impostor_mesh: &BondImpostorMesh, label_prefix: &str) {
        assert!(self.mesh_type == MeshType::BondImpostors, "Cannot update a non-bond-impostor GPUMesh with a BondImpostorMesh");
        
        let vertex_label = format!("{} Bond Impostor Vertex Buffer", label_prefix);
        let index_label = format!("{} Bond Impostor Index Buffer", label_prefix);

        // TODO: In the future, consider updating buffer data in-place if size permits
        // instead of recreating buffers

        self.vertex_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&vertex_label),
                contents: bytemuck::cast_slice(bond_impostor_mesh.vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            }
        );

        self.index_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some(&index_label),
                contents: bytemuck::cast_slice(bond_impostor_mesh.indices.as_slice()),
                usage: BufferUsages::INDEX,
            }
        );

        self.num_indices = bond_impostor_mesh.indices.len() as u32;
        
        // Note: The model buffer and bind group remain unchanged
    }

    /// Update the model transform for this mesh
    pub fn update_transform(&self, queue: &wgpu::Queue, transform: &crate::util::transform::Transform) {
        // Create and update a model uniform with the transform
        let mut model_uniform = ModelUniform::new();
        model_uniform.update_from_transform(transform);
        
        // Write the updated uniform data to the buffer
        queue.write_buffer(&self.model_buffer, 0, bytemuck::cast_slice(&[model_uniform]));
    }
    
    /// Set an identity transform for this mesh
    pub fn set_identity_transform(&self, queue: &wgpu::Queue) {
        // Create a default model uniform (identity transform)
        let model_uniform = ModelUniform::new();
        
        // Write the identity transform to the buffer
        queue.write_buffer(&self.model_buffer, 0, bytemuck::cast_slice(&[model_uniform]));
    }
}
















