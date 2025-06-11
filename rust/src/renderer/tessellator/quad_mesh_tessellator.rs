use crate::renderer::mesh::{Mesh, Vertex, Material};
use glam::Vec3;
use glam::DVec3;
use crate::common::quad_mesh::QuadMesh;

/// Enum to control mesh smoothing behavior during tessellation
#[derive(Debug, Clone)]
pub enum MeshSmoothing {
    /// Smooth normals: averages normals at each vertex from all connected faces
    Smooth,
    /// Sharp normals: uses face normals directly, duplicates vertices as needed
    Sharp,
    /// Smoothing group based: averages normals within the same smoothing group,
    /// duplicates vertices at smoothing group boundaries
    SmoothingGroupBased,
}

/// Converts a QuadMesh into an existing Mesh with smooth normals (averaged from adjacent faces)
/// 
/// # Arguments
/// * `quad_mesh` - The QuadMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `material` - The material to apply to the mesh vertices
fn tessellate_quad_mesh_smooth(quad_mesh: &QuadMesh, mesh: &mut Mesh, material: &Material) {
    // First calculate normal for each vertex by averaging adjacent quad normals
    let mut vertex_normals: Vec<Vec3> = vec![Vec3::ZERO; quad_mesh.vertices.len()];
      
    for (vertex_idx, vertex) in quad_mesh.vertices.iter().enumerate() {
        let mut normal_sum = DVec3::ZERO;
          
        // Sum up the normals of all quads that use this vertex
        for &quad_idx in &vertex.quad_indices {
            normal_sum += quad_mesh.quads[quad_idx as usize].normal;
        }
          
        // Normalize the result if not zero
        if normal_sum.length_squared() > 0.0 {
            normal_sum = normal_sum.normalize();
        }
          
        // Convert from DVec3 to Vec3 for the renderer
        vertex_normals[vertex_idx] = normal_sum.as_vec3();
    }
      
    // Add all vertices to the mesh
    let vertex_indices: Vec<u32> = quad_mesh.vertices.iter().enumerate().map(|(idx, vertex)| {
        let position = vertex.position.as_vec3();
        let normal = vertex_normals[idx];
        mesh.add_vertex(Vertex::new(&position, &normal, material))
    }).collect();
      
    // Add all quads (as two triangles) to the mesh
    for quad in &quad_mesh.quads {
        mesh.add_quad(
            vertex_indices[quad.vertices[0] as usize],
            vertex_indices[quad.vertices[1] as usize],
            vertex_indices[quad.vertices[2] as usize],
            vertex_indices[quad.vertices[3] as usize]
        );
    }
}
  
/// Converts this QuadMesh into an existing Mesh with sharp edges (no normal averaging)
/// 
/// # Arguments
/// * `quad_mesh` - The QuadMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `material` - The material to apply to the mesh vertices
fn tessellate_quad_mesh_sharp(quad_mesh: &QuadMesh, mesh: &mut Mesh, material: &Material) {
    // Process each quad
      
    // Sharp version: duplicate vertices for each quad
    for quad in &quad_mesh.quads {
        // Create a normal for this quad's vertices
        let normal = quad.normal.as_vec3();
          
        // Create four unique vertices for this quad, all with the same normal
        let v0_idx = mesh.add_vertex(Vertex::new(
              &quad_mesh.vertices[quad.vertices[0] as usize].position.as_vec3(),
              &normal,
              material
          ));
          
        let v1_idx = mesh.add_vertex(Vertex::new(
              &quad_mesh.vertices[quad.vertices[1] as usize].position.as_vec3(),
              &normal,
              material
          ));
          
        let v2_idx = mesh.add_vertex(Vertex::new(
              &quad_mesh.vertices[quad.vertices[2] as usize].position.as_vec3(),
              &normal,
              material
          ));
          
        let v3_idx = mesh.add_vertex(Vertex::new(
              &quad_mesh.vertices[quad.vertices[3] as usize].position.as_vec3(),
              &normal,
              material
          ));
          
        // Add the quad (as two triangles) to the mesh
        mesh.add_quad(v0_idx, v1_idx, v2_idx, v3_idx);

    }
}

/// Converts this QuadMesh into an existing Mesh
/// 
/// # Arguments
/// * `quad_mesh` - The QuadMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `smooth` - If true, vertex normals are averaged from adjacent face normals.
///              If false, each quad gets its own set of vertices with the quad's normal.
/// * `material` - The material to apply to the mesh vertices
pub fn tessellate_quad_mesh(quad_mesh: &QuadMesh, mesh: &mut Mesh, smooth: bool, material: &Material) {
  if smooth {
    tessellate_quad_mesh_smooth(quad_mesh, mesh, material);
  } else {
    tessellate_quad_mesh_sharp(quad_mesh, mesh, material);
  }
}
