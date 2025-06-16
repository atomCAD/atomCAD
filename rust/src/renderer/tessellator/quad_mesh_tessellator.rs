use crate::renderer::mesh::{Mesh, Vertex, Material};
use crate::renderer::line_mesh::LineMesh;
use glam::Vec3;
use glam::DVec3;
use crate::common::quad_mesh::QuadMesh;
use crate::api::structure_designer::structure_designer_preferences::MeshSmoothing;

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

/// Converts a QuadMesh into an existing Mesh with smoothing group based normals
/// Vertices are shared within smoothing groups but duplicated at smoothing group boundaries
/// 
/// # Arguments
/// * `quad_mesh` - The QuadMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `material` - The material to apply to the mesh vertices
fn tessellate_quad_mesh_smoothing_group_based(quad_mesh: &QuadMesh, mesh: &mut Mesh, material: &Material) {
    // Maps (vertex_id, smoothing_group) to output mesh vertex indices
    let mut vertex_map: std::collections::HashMap<(u32, Option<u32>), u32> = std::collections::HashMap::new();
    
    // First pass: Create vertices for each vertex+smoothing_group combination
    for (vertex_idx, vertex) in quad_mesh.vertices.iter().enumerate() {
        let vertex_id = vertex_idx as u32;
        
        // Collect normals for each smoothing group this vertex belongs to
        // Map of smoothing_group -> accumulated normal
        let mut smoothing_group_normals: std::collections::HashMap<Option<u32>, DVec3> = 
            std::collections::HashMap::new();
        
        // For each quad that uses this vertex
        for &quad_idx in &vertex.quad_indices {
            let quad = &quad_mesh.quads[quad_idx as usize];
            let smoothing_group = quad.smoothing_group_id;
            let quad_normal = quad.normal;
            
            // Accumulate this normal into the appropriate smoothing group
            let normal_sum = smoothing_group_normals
                .entry(smoothing_group)
                .or_insert(DVec3::ZERO);
                
            *normal_sum += quad_normal;
        }
        
        // Now create a vertex for each smoothing group this vertex belongs to
        let position = vertex.position;
        
        for (smoothing_group, normal_sum) in smoothing_group_normals {
            // Calculate the final normal for this smoothing group
            let normal = if normal_sum.length_squared() > 0.0 {
                normal_sum.normalize()
            } else {
                DVec3::Y // Default normal if zero normal sum
            };
            
            // Add this vertex with the calculated normal
            let output_vertex_idx = mesh.add_vertex(Vertex::new(
                &position.as_vec3(), 
                &normal.as_vec3(), 
                material
            ));
            
            // Store the mapping for face creation
            vertex_map.insert((vertex_id, smoothing_group), output_vertex_idx);
        }
    }
    
    // Second pass: Create quads
    for (_quad_idx, quad) in quad_mesh.quads.iter().enumerate() {
        let smoothing_group = quad.smoothing_group_id;
        
        // Get the vertices for this quad with the correct smoothing group
        let v0_idx = *vertex_map.get(&(quad.vertices[0], smoothing_group)).unwrap_or(&0);
        let v1_idx = *vertex_map.get(&(quad.vertices[1], smoothing_group)).unwrap_or(&0);
        let v2_idx = *vertex_map.get(&(quad.vertices[2], smoothing_group)).unwrap_or(&0);
        let v3_idx = *vertex_map.get(&(quad.vertices[3], smoothing_group)).unwrap_or(&0);
        
        // Add the quad (as two triangles) to the mesh
        mesh.add_quad(v0_idx, v1_idx, v2_idx, v3_idx);
    }
}

/// Converts this QuadMesh into an existing Mesh
/// 
/// # Arguments
/// * `quad_mesh` - The QuadMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `smoothing` - Controls how normals are calculated and vertices are shared
/// * `material` - The material to apply to the mesh vertices
pub fn tessellate_quad_mesh(quad_mesh: &QuadMesh, mesh: &mut Mesh, smoothing: MeshSmoothing, material: &Material) {
    match smoothing {
        MeshSmoothing::Smooth => tessellate_quad_mesh_smooth(quad_mesh, mesh, material),
        MeshSmoothing::Sharp => tessellate_quad_mesh_sharp(quad_mesh, mesh, material),
        MeshSmoothing::SmoothingGroupBased => tessellate_quad_mesh_smoothing_group_based(quad_mesh, mesh, material),
    }
}

/// Converts a QuadMesh into a LineMesh with lines representing the edges
/// Sharp edges will be rendered with a different color to highlight them
/// 
/// # Arguments
/// * `quad_mesh` - The QuadMesh to convert
/// * `line_mesh` - The target line mesh to add lines to
/// * `smoothing` - Controls how edges are interpreted (affects what's considered a sharp edge)
/// * `sharp_edge_color` - The color for sharp edges [r, g, b]
/// * `normal_edge_color` - The color for non-sharp edges [r, g, b]
pub fn tessellate_quad_mesh_to_line_mesh(
    quad_mesh: &QuadMesh, 
    line_mesh: &mut LineMesh, 
    smoothing: MeshSmoothing, 
    sharp_edge_color: [f32; 3], 
    normal_edge_color: [f32; 3]
) {
    // Set of edges already processed to avoid duplicates
    let mut processed_edges = std::collections::HashSet::new();
    
    // Process each edge in the quad mesh
    for ((v1_idx, v2_idx), edge) in &quad_mesh.edges {
        // Skip if we've already processed this edge
        // We need to check both directions since the edge map uses ordered pairs
        if processed_edges.contains(&(*v1_idx, *v2_idx)) || processed_edges.contains(&(*v2_idx, *v1_idx)) {
            continue;
        }
        
        // Mark as processed
        processed_edges.insert((*v1_idx, *v2_idx));
        
        // Get vertex positions
        let v1_pos = quad_mesh.vertices[*v1_idx as usize].position.as_vec3();
        let v2_pos = quad_mesh.vertices[*v2_idx as usize].position.as_vec3();
        
        // Determine if this edge should be rendered as sharp based on the smoothing mode
        let is_sharp = match smoothing {
            MeshSmoothing::Smooth => false, // All edges smooth
            MeshSmoothing::Sharp => true,  // All edges sharp
            MeshSmoothing::SmoothingGroupBased => edge.is_sharp // Use the edge's sharp flag
        };
        
        // Choose color based on whether the edge is sharp
        let color = if is_sharp { sharp_edge_color } else { normal_edge_color };
        
        // Add the line with the appropriate color
        line_mesh.add_line_with_uniform_color(&v1_pos, &v2_pos, &color);
    }
}
