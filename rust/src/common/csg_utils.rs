use crate::common::csg_types::CSG;
use crate::common::poly_mesh::PolyMesh;
use crate::common::unique_3d_points::Unique3DPoints;
use glam::DVec3;
use csgrs::float_types::Real;
use nalgebra::{Point3, Vector3};

/// Convert a CSG object to a PolyMesh, merging vertices that are within epsilon distance of each other.
/// 
/// This helps avoid issues with floating point precision in the CSG operations, where vertices
/// that should be identical might have slight differences in their coordinates.
pub fn convert_csg_to_poly_mesh(csg: &CSG) -> PolyMesh {
    let mut poly_mesh = PolyMesh::new();
    
    // Use our spatial hashing structure with a small epsilon for vertex precision
    // The epsilon value should be adjusted based on the scale of your models
    let epsilon = 1e-5;
    let mut unique_vertices = Unique3DPoints::new(epsilon);

    let polys_from_2d = if csg.geometry.is_empty() {
        Vec::new()
    } else {
        let mut polys = csg.to_polygons();
        
        // Transform from XY plane to XZ plane
        for poly in &mut polys {
            // Transform each vertex: move Y coordinate to Z, and set y to be just above the grid
            for vertex in &mut poly.vertices {
                vertex.pos.z = vertex.pos.y;
                vertex.pos.y = 0.001;
            }
            
            // Reverse vertex order to maintain correct winding after transformation
            poly.vertices.reverse();
        }
        
        polys
    };

    let polygons = if csg.geometry.is_empty() {
        &csg.polygons
    } else {
        &polys_from_2d
    };

    // Process each polygon in the CSG
    for polygon in polygons {
        let mut face_vertices = Vec::new();
        
        // Process each vertex in the polygon
        for vertex in &polygon.vertices {
            // Convert nalgebra Point3 to glam DVec3
            let position = DVec3::new(
                vertex.pos.x as f64,
                vertex.pos.y as f64,
                vertex.pos.z as f64,
            );

            // Get or insert the vertex, retrieving its index
            let vertex_idx = unique_vertices.get_or_insert(position, {
                let idx = poly_mesh.add_vertex(position);
                idx
            });
            
            face_vertices.push(vertex_idx);
        }
        
        // Add the face to the poly_mesh if we have at least 3 vertices
        if face_vertices.len() >= 3 {
            poly_mesh.add_face(face_vertices);
        }
    }
    
    poly_mesh
}

pub fn dvec3_to_point3(dvec3: DVec3) -> Point3<Real> {
    Point3::new(dvec3.x as Real, dvec3.y as Real, dvec3.z as Real)
}

pub fn dvec3_to_vector3(dvec3: DVec3) -> Vector3<Real> {
    Vector3::new(dvec3.x as Real, dvec3.y as Real, dvec3.z as Real)
}

