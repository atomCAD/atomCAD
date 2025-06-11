use crate::common::he_mesh::{HEMesh, VertexId, FaceId, HalfEdgeId};
use crate::renderer::mesh::{Mesh, Vertex};
use crate::renderer::mesh::Material;
use glam::{DVec3, Vec3};

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

/// Tessellates a half-edge mesh into a renderable triangle mesh
///
/// # Arguments
/// * `output_mesh` - The target mesh to add vertices and faces to
/// * `mesh` - The half-edge mesh to tessellate
/// * `smoothing` - The smoothing mode to use
/// * `material` - The material to use for the tessellated mesh
pub fn tessellate_he_mesh(output_mesh: &mut Mesh, mesh: &HEMesh, smoothing: &MeshSmoothing, material: &Material) {
    // Dispatch to the appropriate tessellation method based on smoothing mode
    match smoothing {
        MeshSmoothing::Smooth => tessellate_he_mesh_smooth(output_mesh, mesh, material),
        MeshSmoothing::Sharp => tessellate_he_mesh_sharp(output_mesh, mesh, material),
        MeshSmoothing::SmoothingGroupBased => tessellate_he_mesh_smoothing_group_based(output_mesh, mesh, material),
    }
}

/// Helper function to convert from DVec3 to Vec3
fn dvec3_to_vec3(vec: &DVec3) -> Vec3 {
    Vec3::new(vec.x as f32, vec.y as f32, vec.z as f32)
}

/// Calculate a vertex normal by averaging the normals of all adjacent faces
fn calculate_vertex_normal(mesh: &HEMesh, vertex_id: VertexId) -> Vec3 {
    // Start from any outgoing half-edge of the vertex
    let mut normal_sum = Vec3::ZERO;
    let mut face_count = 0;
    
    if let Some(start_he) = mesh.get_vertex_half_edge(vertex_id) {
        let mut current_he = start_he;
        
        // Traverse all half-edges around the vertex
        loop {
            // Add the face normal for the current half-edge
            let face_id = mesh.get_half_edge_face(current_he);
            normal_sum += dvec3_to_vec3(mesh.get_face_normal(face_id));
            face_count += 1;
            
            // Move to the next outgoing half-edge around this vertex
            current_he = mesh.get_next_half_edge_around_vertex(current_he);
            
            // If we've circled back to the start, we're done
            if current_he == start_he {
                break;
            }
        }
    }
    
    // If we found any faces, normalize the result
    if face_count > 0 && normal_sum != Vec3::ZERO {
        normal_sum.normalize()
    } else {
        Vec3::Y // Default normal if no faces or zero normal sum
    }
}

/// Tessellates a half-edge mesh with smooth normals (averaging all face normals around each vertex)
fn tessellate_he_mesh_smooth(output_mesh: &mut Mesh, mesh: &HEMesh, material: &Material) {
    // Create vertices in output mesh with individually computed normals
    let mut vertex_indices = Vec::with_capacity(mesh.vertices.len());
    
    for i in 0..mesh.vertices.len() {
        let vertex_id = VertexId(i);
        let position = dvec3_to_vec3(mesh.get_vertex_position(vertex_id));
        
        // Calculate normal for this specific vertex by averaging adjacent face normals
        let normal = calculate_vertex_normal(mesh, vertex_id);
        
        // Add the vertex to the output mesh
        vertex_indices.push(output_mesh.add_vertex(Vertex::new(
            &position, &normal, material
        )));
    }
    
    // Add triangulated faces to output mesh
    tessellate_faces(output_mesh, mesh, &vertex_indices);
}

/// Tessellates a half-edge mesh with sharp normals (no normal averaging, each face has unique vertices)
fn tessellate_he_mesh_sharp(output_mesh: &mut Mesh, mesh: &HEMesh, material: &Material) {
    // For each face, create new vertices with face normal
    for face_id in (0..mesh.faces.len()).map(FaceId) {
        let face_normal = dvec3_to_vec3(mesh.get_face_normal(face_id));
        
        // Collect vertices for this face
        let mut face_vertices = Vec::new();
        
        // Walk around the face manually instead of using an iterator
        let start_he = mesh.get_face_half_edge(face_id);
        let mut current_he = start_he;
        
        loop {
            let vertex_id = mesh.get_half_edge_origin(current_he);
            let position = dvec3_to_vec3(mesh.get_vertex_position(vertex_id));
            
            // Add a new vertex with the face normal
            face_vertices.push(output_mesh.add_vertex(Vertex::new(
                &position, &face_normal, material
            )));
            
            // Move to the next half-edge in this face
            current_he = mesh.get_next_half_edge(current_he);
            
            // If we've circled back to the start, we're done
            if current_he == start_he {
                break;
            }
        }
        
        // Triangulate the face
        triangulate_polygon(output_mesh, &face_vertices);
    }
}

/// Tessellates a half-edge mesh with smoothing group based normals
/// (vertices are shared within smoothing groups but duplicated at boundaries)
fn tessellate_he_mesh_smoothing_group_based(output_mesh: &mut Mesh, mesh: &HEMesh, material: &Material) {
    // Maps (vertex_id, smoothing_group) to output mesh vertex indices
    let mut vertex_map: std::collections::HashMap<(usize, Option<u32>), u32> = std::collections::HashMap::new();
    
    // First pass: Create vertices for each vertex+smoothing_group combination
    for vertex_id in (0..mesh.vertices.len()).map(VertexId) {
        // Collect normals for each smoothing group this vertex belongs to
        // Map of smoothing_group -> accumulated normal
        let mut smoothing_group_normals: std::collections::HashMap<Option<u32>, Vec3> = 
            std::collections::HashMap::new();
        
        // Skip vertices with no half-edge
        let start_he = match mesh.get_vertex_half_edge(vertex_id) {
            Some(he) => he,
            None => continue,
        };
        
        let mut current_he = start_he;
        
        // Traverse all half-edges around the vertex to accumulate normals by smoothing group
        loop {
            let face_id = mesh.get_half_edge_face(current_he);
            let smoothing_group = mesh.get_face_smoothing_group(face_id);
            let face_normal = dvec3_to_vec3(mesh.get_face_normal(face_id));
            
            // Accumulate this normal into the appropriate smoothing group
            let normal_sum = smoothing_group_normals
                .entry(smoothing_group)
                .or_insert(Vec3::ZERO);
                
            *normal_sum += face_normal;
            
            // Move to the next half-edge around the vertex
            current_he = mesh.get_next_half_edge_around_vertex(current_he);
            
            // If we've circled back to the start, we're done
            if current_he == start_he {
                break;
            }
        }
        
        // Now create a vertex for each smoothing group this vertex belongs to
        let position = dvec3_to_vec3(mesh.get_vertex_position(vertex_id));
        
        for (smoothing_group, normal_sum) in smoothing_group_normals {
            // Calculate the final normal for this smoothing group
            let normal = if normal_sum != Vec3::ZERO {
                normal_sum.normalize()
            } else {
                Vec3::Y // Default normal if zero normal sum
            };
            
            // Add this vertex with the calculated normal
            let output_vertex_idx = output_mesh.add_vertex(Vertex::new(
                &position, &normal, material
            ));
            
            // Store the mapping for face creation
            vertex_map.insert((vertex_id.0, smoothing_group), output_vertex_idx);
        }
    }
    
    // Second pass: Create triangulated faces
    for face_id in (0..mesh.faces.len()).map(FaceId) {
        let smoothing_group = mesh.get_face_smoothing_group(face_id);
        let mut face_vertices = Vec::new();
        
        // Walk around the face to collect vertices
        let start_he = mesh.get_face_half_edge(face_id);
        let mut current_he = start_he;
        
        loop {
            let vertex_id = mesh.get_half_edge_origin(current_he);
            
            // Look up the output mesh vertex index for this (vertex, smoothing_group)
            if let Some(&vertex_idx) = vertex_map.get(&(vertex_id.0, smoothing_group)) {
                face_vertices.push(vertex_idx);
            }
            
            // Move to the next half-edge
            current_he = mesh.get_next_half_edge(current_he);
            
            // If we've circled back to the start, we're done
            if current_he == start_he {
                break;
            }
        }
        
        // Triangulate the face
        triangulate_polygon(output_mesh, &face_vertices);
    }
}

/// Helper function to triangulate a polygon (represented as an array of vertex indices)
/// Uses simple fan triangulation, which works for convex polygons
fn triangulate_polygon(output_mesh: &mut Mesh, vertices: &[u32]) {
    // Need at least 3 vertices to form a triangle
    if vertices.len() < 3 {
        return;
    }
    
    // For a triangle, just add it directly
    if vertices.len() == 3 {
        output_mesh.add_triangle(vertices[0], vertices[1], vertices[2]);
        return;
    }
    
    // For quads, use the built-in add_quad which creates two triangles
    if vertices.len() == 4 {
        output_mesh.add_quad(vertices[0], vertices[1], vertices[2], vertices[3]);
        return;
    }
    
    // For polygons with more than 4 vertices, use fan triangulation
    let anchor = vertices[0];
    for i in 1..(vertices.len() - 1) {
        output_mesh.add_triangle(anchor, vertices[i], vertices[i + 1]);
    }
}

/// Helper function to tessellate faces - used for smooth tessellation where vertices are shared
fn tessellate_faces(output_mesh: &mut Mesh, mesh: &HEMesh, vertex_indices: &[u32]) {
    // Process each face
    for face_id in (0..mesh.faces.len()).map(FaceId) {
        // Collect vertex indices for this face
        let mut face_vertices = Vec::new();
        
        // Walk around the face manually using direct half-edge traversal
        let start_he = mesh.get_face_half_edge(face_id);
        let mut current_he = start_he;
        
        loop {
            let vertex_id = mesh.get_half_edge_origin(current_he);
            face_vertices.push(vertex_indices[vertex_id.0]);
            
            // Move to the next half-edge in this face
            current_he = mesh.get_next_half_edge(current_he);
            
            // If we've circled back to the start, we're done
            if current_he == start_he {
                break;
            }
        }
        
        // Triangulate the face
        triangulate_polygon(output_mesh, &face_vertices);
    }
}
