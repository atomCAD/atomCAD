use crate::renderer::mesh::{Mesh, Vertex, Material};
use crate::renderer::line_mesh::LineMesh;
use glam::Vec3;
use glam::DVec3;
use crate::common::poly_mesh::PolyMesh;
use crate::api::structure_designer::structure_designer_preferences::MeshSmoothing;
use crate::structure_designer::common_constants;

/// Adds a triangle to the mesh, respecting the tessellation direction
/// 
/// If tessellate_outside is true, uses v0, v1, v2 order (counter-clockwise, visible from outside)
/// If tessellate_outside is false, uses v0, v2, v1 order (clockwise, visible from inside)
fn add_triangle(mesh: &mut Mesh, tessellate_outside: bool, v0: u32, v1: u32, v2: u32) {
    if tessellate_outside {
        mesh.add_triangle(v0, v1, v2);
    } else {
        mesh.add_triangle(v0, v2, v1);
    }
}

/// Adds a hatched quad to the mesh, creating a grid pattern with square holes
/// 
/// # Arguments
/// * `mesh` - The mesh to add triangles/quads to
/// * `tessellate_outside` - If true, vertices are ordered for outside view
/// * `v_positions` - Array of 4 vertex positions in counter-clockwise order
/// * `v_normals` - Array of 4 vertex normals corresponding to positions
/// * `material` - Material for the vertices
/// * `grid_size` - Size of each grid cell
fn add_hatched_quad(
    mesh: &mut Mesh, 
    tessellate_outside: bool, 
    v_positions: [&Vec3; 4], 
    v_normals: [&Vec3; 4], 
    material: &Material, 
    grid_size: f32
) {
    // Get the quad dimensions
    let side_length = (v_positions[1] - v_positions[0]).length();
    
    // Calculate how many grid cells fit in each direction
    let grid_count = (side_length / grid_size).round().max(1.0) as usize;
    
    // Calculate stride vectors (how much to move per grid cell)
    let stride_u = (*v_positions[1] - *v_positions[0]) / grid_count as f32;
    let stride_v = (*v_positions[3] - *v_positions[0]) / grid_count as f32;
    
    // For each grid cell, create the frame (outer border without the inner square)
    for u in 0..grid_count {
        for v in 0..grid_count {
            // frame is 1/8 of the cell size
            let frame_thickness = 1.0 / 8.0;
            
            // Calculate the four corners of the current cell
            let base_pos = *v_positions[0] + stride_u * u as f32 + stride_v * v as f32;
            
            // Create the outer frame vertices (all 8 points of the frame)
            let corners = [
                // Outer corners
                base_pos,                               // Bottom-left outer
                base_pos + stride_u,                   // Bottom-right outer
                base_pos + stride_u + stride_v,       // Top-right outer
                base_pos + stride_v,                  // Top-left outer
                
                // Inner corners (the hole)
                base_pos + stride_u * frame_thickness + stride_v * frame_thickness,  // Bottom-left inner
                base_pos + stride_u * (1.0 - frame_thickness) + stride_v * frame_thickness,  // Bottom-right inner
                base_pos + stride_u * (1.0 - frame_thickness) + stride_v * (1.0 - frame_thickness),  // Top-right inner
                base_pos + stride_u * frame_thickness + stride_v * (1.0 - frame_thickness),  // Top-left inner
            ];
            
            // Interpolate normal based on position within the quad
            // For simplicity, we'll use the average normal for all vertices in a grid cell
            let u_ratio = u as f32 / grid_count as f32;
            let v_ratio = v as f32 / grid_count as f32;
            
            // Bilinear interpolation of normals
            let normal = *v_normals[0] * (1.0 - u_ratio) * (1.0 - v_ratio) +
                         *v_normals[1] * u_ratio * (1.0 - v_ratio) +
                         *v_normals[2] * u_ratio * v_ratio +
                         *v_normals[3] * (1.0 - u_ratio) * v_ratio;
            
            let normal = if normal.length_squared() > 0.0 { normal.normalize() } else { *v_normals[0] };
            
            // Add vertices to the mesh
            let vertex_indices: Vec<u32> = corners.iter().map(|pos| {
                mesh.add_vertex(Vertex::new(pos, &normal, material))
            }).collect();
            
            // Create the 8 triangles that form the frame (2 for each side of the frame)
            // Bottom side
            add_triangle(mesh, tessellate_outside, vertex_indices[0], vertex_indices[1], vertex_indices[5]);
            add_triangle(mesh, tessellate_outside, vertex_indices[0], vertex_indices[5], vertex_indices[4]);
            
            // Right side
            add_triangle(mesh, tessellate_outside, vertex_indices[1], vertex_indices[2], vertex_indices[6]);
            add_triangle(mesh, tessellate_outside, vertex_indices[1], vertex_indices[6], vertex_indices[5]);
            
            // Top side
            add_triangle(mesh, tessellate_outside, vertex_indices[2], vertex_indices[3], vertex_indices[7]);
            add_triangle(mesh, tessellate_outside, vertex_indices[2], vertex_indices[7], vertex_indices[6]);
            
            // Left side
            add_triangle(mesh, tessellate_outside, vertex_indices[3], vertex_indices[0], vertex_indices[4]);
            add_triangle(mesh, tessellate_outside, vertex_indices[3], vertex_indices[4], vertex_indices[7]);
        }
    }
}

/// Adds a quad to the mesh, respecting the tessellation direction
/// 
/// If tessellate_outside is true, uses v0, v1, v2, v3 order (counter-clockwise, visible from outside)
/// If tessellate_outside is false, uses v0, v3, v2, v1 order (clockwise, visible from inside)
fn add_quad(mesh: &mut Mesh, tessellate_outside: bool, v0: u32, v1: u32, v2: u32, v3: u32) {
    if tessellate_outside {
        mesh.add_quad(v0, v1, v2, v3);
    } else {
        mesh.add_quad(v0, v3, v2, v1);
    }
}

/// Converts a PolyMesh into an existing Mesh with smooth normals (averaged from adjacent faces)
/// 
/// # Arguments
/// * `poly_mesh` - The PolyMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `material` - The material to apply to the mesh vertices
fn tessellate_poly_mesh_smooth(poly_mesh: &PolyMesh, mesh: &mut Mesh, material: &Material, tessellate_outside: bool) {
    // First calculate normal for each vertex by averaging adjacent face normals
    let mut vertex_normals: Vec<Vec3> = vec![Vec3::ZERO; poly_mesh.vertices.len()];
      
    for (vertex_idx, vertex) in poly_mesh.vertices.iter().enumerate() {
        let mut normal_sum = DVec3::ZERO;
          
        // Sum up the normals of all faces that use this vertex
        for &face_idx in &vertex.face_indices {
            normal_sum += poly_mesh.faces[face_idx as usize].normal;
        }
          
        // Normalize the result if not zero
        if normal_sum.length_squared() > 0.0 {
            normal_sum = normal_sum.normalize();
        }
          
        // Convert from DVec3 to Vec3 for the renderer
        vertex_normals[vertex_idx] = normal_sum.as_vec3();
    }
      
    // Add all vertices to the mesh
    let vertex_indices: Vec<u32> = poly_mesh.vertices.iter().enumerate().map(|(idx, vertex)| {
        let position = vertex.position.as_vec3();
        // Apply normal direction based on tessellation direction
        let mut normal = vertex_normals[idx];
        if !tessellate_outside {
            normal = -normal; // Flip normal for inside-facing mesh
        }
        mesh.add_vertex(Vertex::new(&position, &normal, material))
    }).collect();

    // Add all faces to the mesh
    for face in &poly_mesh.faces {
        // For faces with exactly 4 vertices, use add_quad
        if face.vertices.len() == 4 {
            add_quad(
                mesh,
                tessellate_outside,
                vertex_indices[face.vertices[0] as usize],
                vertex_indices[face.vertices[1] as usize],
                vertex_indices[face.vertices[2] as usize],
                vertex_indices[face.vertices[3] as usize]
            );
        } else {
            // For faces with 3 or more vertices, triangulate using fan triangulation
            // This is a simple approach: create triangles from the first vertex to each pair of consecutive vertices
            let v0 = vertex_indices[face.vertices[0] as usize];
            for i in 1..face.vertices.len() - 1 {
                let v1 = vertex_indices[face.vertices[i] as usize];
                let v2 = vertex_indices[face.vertices[i + 1] as usize];
                add_triangle(mesh, tessellate_outside, v0, v1, v2);
            }
        }
    }
}
  
/// Converts this PolyMesh into an existing Mesh with sharp edges (no normal averaging)
/// 
/// # Arguments
/// * `poly_mesh` - The PolyMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `material` - The material to apply to the mesh vertices
fn tessellate_poly_mesh_sharp(poly_mesh: &PolyMesh, mesh: &mut Mesh, material: &Material, tessellate_outside: bool) {
    // Process each face
      
    // Sharp version: duplicate vertices for each face
    for face in &poly_mesh.faces {
        // Create a normal for this face's vertices
        let mut normal = face.normal.as_vec3();
        if !tessellate_outside {
            normal = -normal; // Flip normal for inside-facing mesh
        }
          
        // Create vertices for this face, all with the same normal
        let mut face_vertex_indices = Vec::with_capacity(face.vertices.len());
        
        // Add each vertex
        for &vertex_idx in &face.vertices {
            let vertex_position = poly_mesh.vertices[vertex_idx as usize].position.as_vec3();
            let mesh_vertex_idx = mesh.add_vertex(Vertex::new(
                &vertex_position,
                &normal,
                material
            ));
            face_vertex_indices.push(mesh_vertex_idx);
        }
        
        // Add the face to the mesh
        if face.vertices.len() == 4 {
            // Use quad-specific method for 4-vertex faces
            add_quad(
                mesh,
                tessellate_outside,
                face_vertex_indices[0],
                face_vertex_indices[1],
                face_vertex_indices[2],
                face_vertex_indices[3]
            );
        } else {
            // Triangulate for faces with other vertex counts
            let v0 = face_vertex_indices[0];
            for i in 1..face_vertex_indices.len() - 1 {
                let v1 = face_vertex_indices[i];
                let v2 = face_vertex_indices[i + 1];
                add_triangle(mesh, tessellate_outside, v0, v1, v2);
            }
        }
    }
}

/// Converts a PolyMesh into an existing Mesh with smoothing group based normals
/// Vertices are shared within smoothing groups but duplicated at smoothing group boundaries
/// 
/// # Arguments
/// * `poly_mesh` - The PolyMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `material` - The material to apply to the mesh vertices
fn tessellate_poly_mesh_smoothing_group_based(poly_mesh: &PolyMesh, mesh: &mut Mesh, material: &Material, tessellate_outside: bool) {
    // Maps (vertex_id, smoothing_group) to output mesh vertex indices
    let mut vertex_map: std::collections::HashMap<(u32, Option<u32>), u32> = std::collections::HashMap::new();
    
    // First pass: Create vertices for each vertex+smoothing_group combination
    for (vertex_idx, vertex) in poly_mesh.vertices.iter().enumerate() {
        let vertex_id = vertex_idx as u32;
        
        // Collect normals for each smoothing group this vertex belongs to
        // Map of smoothing_group -> accumulated normal
        let mut smoothing_group_normals: std::collections::HashMap<Option<u32>, DVec3> = 
            std::collections::HashMap::new();
        
        // For each face that uses this vertex
        for &face_idx in &vertex.face_indices {
            let face = &poly_mesh.faces[face_idx as usize];
            let smoothing_group = face.smoothing_group_id;
            let face_normal = face.normal;
            
            // Accumulate this normal into the appropriate smoothing group
            let normal_sum = smoothing_group_normals
                .entry(smoothing_group)
                .or_insert(DVec3::ZERO);
                
            *normal_sum += face_normal;
        }
        
        // Now create a vertex for each smoothing group this vertex belongs to
        let position = vertex.position;
        
        for (smoothing_group, normal_sum) in smoothing_group_normals {
            // Calculate the final normal for this smoothing group
            let mut normal = if normal_sum.length_squared() > 0.0 {
                normal_sum.normalize()
            } else {
                DVec3::Y // Default normal if zero normal sum
            };
            
            // Flip normal if we're tessellating for inside view
            if !tessellate_outside {
                normal = -normal;
            }
            
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
    
    // Second pass: Create faces
    for face in &poly_mesh.faces {
        let smoothing_group = face.smoothing_group_id;
        
        // Get the vertices for this face with the correct smoothing group
        let mut face_vertex_indices = Vec::with_capacity(face.vertices.len());
        
        for &vertex_id in &face.vertices {
            let mapped_vertex_idx = vertex_map.get(&(vertex_id, smoothing_group)).unwrap_or(&0);
            face_vertex_indices.push(*mapped_vertex_idx);
        }
        
        // Add the face to the mesh
        if face.vertices.len() == 4 {
            // Special handling for quads
            add_quad(
                mesh,
                tessellate_outside,
                face_vertex_indices[0],
                face_vertex_indices[1],
                face_vertex_indices[2],
                face_vertex_indices[3]
            );
        } else {
            // Triangulate for other polygon types
            let v0 = face_vertex_indices[0];
            for i in 1..face_vertex_indices.len() - 1 {
                let v1 = face_vertex_indices[i];
                let v2 = face_vertex_indices[i + 1];
                add_triangle(mesh, tessellate_outside, v0, v1, v2);
            }
        }
    }
}

/// Converts this PolyMesh into an existing Mesh
/// 
/// # Arguments
/// * `poly_mesh` - The PolyMesh to convert
/// * `mesh` - The target mesh to add vertices and faces to
/// * `smoothing` - Controls how normals are calculated and vertices are shared
/// * `material` - The material to apply to the mesh vertices
pub fn tessellate_poly_mesh(poly_mesh: &PolyMesh, mesh: &mut Mesh, smoothing: MeshSmoothing, outside_material: &Material, inside_material: Option<&Material>) {
    tessellate_poly_mesh_one_sided(poly_mesh, mesh, smoothing.clone(), outside_material, true);
    if poly_mesh.open {
        if let Some(material) = inside_material {
            tessellate_poly_mesh_one_sided(poly_mesh, mesh, smoothing, material, false);
        } 
    }
}

fn tessellate_poly_mesh_one_sided(poly_mesh: &PolyMesh, mesh: &mut Mesh, smoothing: MeshSmoothing, material: &Material, tessellate_outside: bool) {
    // Special case for hatched quads: if the poly_mesh contains exactly one quad face and is marked as hatched
    if poly_mesh.hatched && poly_mesh.faces.len() == 1 && poly_mesh.faces[0].vertices.len() == 4 {
        // Get the vertex positions and normals directly from the poly_mesh
        let face = &poly_mesh.faces[0];
        let v0_idx = face.vertices[0] as usize;
        let v1_idx = face.vertices[1] as usize;
        let v2_idx = face.vertices[2] as usize;
        let v3_idx = face.vertices[3] as usize;
        
        let v0_pos = poly_mesh.vertices[v0_idx].position.as_vec3();
        let v1_pos = poly_mesh.vertices[v1_idx].position.as_vec3();
        let v2_pos = poly_mesh.vertices[v2_idx].position.as_vec3();
        let v3_pos = poly_mesh.vertices[v3_idx].position.as_vec3();
        
        // Get the face normal
        let mut normal = face.normal.as_vec3();
        if !tessellate_outside {
            normal = -normal;
        }
        
        // Use the same normal for all vertices in this simple case
        let positions = [&v0_pos, &v1_pos, &v2_pos, &v3_pos];
        let normals = [&normal, &normal, &normal, &normal];
        
        // Call add_hatched_quad directly
        add_hatched_quad(mesh, tessellate_outside, positions, normals, material, common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f32 * 0.5);
        return;
    }

    // Normal flow for other cases
    match smoothing {
        MeshSmoothing::Smooth => tessellate_poly_mesh_smooth(poly_mesh, mesh, material, tessellate_outside),
        MeshSmoothing::Sharp => tessellate_poly_mesh_sharp(poly_mesh, mesh, material, tessellate_outside),
        MeshSmoothing::SmoothingGroupBased => tessellate_poly_mesh_smoothing_group_based(poly_mesh, mesh, material, tessellate_outside),
    }
}

/// Legacy function for backward compatibility
pub fn tessellate_quad_mesh(quad_mesh: &PolyMesh, mesh: &mut Mesh, smoothing: MeshSmoothing, outside_material: &Material, inside_material: Option<&Material>) {
    tessellate_poly_mesh(quad_mesh, mesh, smoothing, outside_material, inside_material)
}

/// Converts a PolyMesh into a LineMesh with lines representing the edges
/// Sharp edges will be rendered with a different color to highlight them
/// 
/// # Arguments
/// * `poly_mesh` - The PolyMesh to convert
/// * `line_mesh` - The target line mesh to add lines to
/// * `smoothing` - Controls how edges are interpreted (affects what's considered a sharp edge)
/// * `sharp_edge_color` - The color for sharp edges [r, g, b]
/// * `normal_edge_color` - The color for non-sharp edges [r, g, b]
pub fn tessellate_poly_mesh_to_line_mesh(
    poly_mesh: &PolyMesh, 
    line_mesh: &mut LineMesh, 
    smoothing: MeshSmoothing, 
    sharp_edge_color: [f32; 3], 
    normal_edge_color: [f32; 3]
) {
    // Set of edges already processed to avoid duplicates
    let mut processed_edges = std::collections::HashSet::new();
    
    // Process each edge in the poly mesh
    for ((v1_idx, v2_idx), edge) in &poly_mesh.edges {
        // Skip if we've already processed this edge
        // We need to check both directions since the edge map uses ordered pairs
        if processed_edges.contains(&(*v1_idx, *v2_idx)) || processed_edges.contains(&(*v2_idx, *v1_idx)) {
            continue;
        }
        
        // Mark as processed
        processed_edges.insert((*v1_idx, *v2_idx));
        
        // Get vertex positions
        let v1_pos = poly_mesh.vertices[*v1_idx as usize].position.as_vec3();
        let v2_pos = poly_mesh.vertices[*v2_idx as usize].position.as_vec3();
        
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

/// Legacy function for backward compatibility
pub fn tessellate_quad_mesh_to_line_mesh(
    poly_mesh: &PolyMesh, 
    line_mesh: &mut LineMesh, 
    smoothing: MeshSmoothing, 
    sharp_edge_color: [f32; 3], 
    normal_edge_color: [f32; 3]
) {
    tessellate_poly_mesh_to_line_mesh(poly_mesh, line_mesh, smoothing, sharp_edge_color, normal_edge_color)
}
