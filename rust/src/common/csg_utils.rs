use crate::common::csg_types::CSGMesh;
use crate::common::csg_types::CSGSketch;
use crate::common::poly_mesh::PolyMesh;
use crate::common::unique_3d_points::Unique3DPoints;
use glam::DVec3;
use csgrs::{float_types::Real, mesh::polygon::Polygon, mesh::vertex::Vertex};
use nalgebra::{Point3, Vector3};
use geo::{
    Geometry, Polygon as GeoPolygon, TriangulateEarcut,
};

/// Convert a CSG object to a PolyMesh, merging vertices that are within epsilon distance of each other.
/// 
/// This helps avoid issues with floating point precision in the CSG operations, where vertices
/// that should be identical might have slight differences in their coordinates.
pub fn convert_csg_mesh_to_poly_mesh(csg_mesh: &CSGMesh, open_3d: bool, hatched: bool) -> PolyMesh {
    convert_polygons_to_poly_mesh(&csg_mesh.polygons, open_3d, hatched)
}

pub fn convert_csg_sketch_to_poly_mesh(csg_sketch: CSGSketch, triangulate_2d: bool) -> PolyMesh {
 
    let mut polys = if triangulate_2d {
        triangulate_csg_sketch(&csg_sketch)
    } else {
        let mesh = CSGMesh::from(csg_sketch);
        mesh.polygons
    };
 
    // Add small Z offset to render 2D sketches slightly above the grid (avoid z-fighting)
    for poly in &mut polys {
        for vertex in &mut poly.vertices {
            vertex.pos.z += 0.001;
        }
    }

    convert_polygons_to_poly_mesh(&polys, true, false)
}

fn convert_polygons_to_poly_mesh(polygons: &Vec<Polygon<()>>, open: bool, hatched: bool) -> PolyMesh {

    // Use our spatial hashing structure with a small epsilon for vertex precision
    // The epsilon value should be adjusted based on the scale of your models
    let epsilon = 1e-5;
    let mut unique_vertices = Unique3DPoints::new(epsilon);

    let mut poly_mesh = PolyMesh::new(open, hatched);

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

fn triangulate_geo_polygon(poly2d: &GeoPolygon<Real>) -> Vec<Polygon<()>> {
    let mut ret = Vec::new();
    for triangle in poly2d.earcut_triangles() {
        // Note: geo crate's earcut_triangles() produces clockwise triangles
        // even from counter-clockwise input polygons. We reverse the vertex 
        // order to get counter-clockwise triangles for proper Z-up rendering.
        let vertices = [
            Vertex::new(Point3::new(triangle.2.x, triangle.2.y, 0.0), Vector3::new(0.0, 0.0, 1.0)),
            Vertex::new(Point3::new(triangle.1.x, triangle.1.y, 0.0), Vector3::new(0.0, 0.0, 1.0)),
            Vertex::new(Point3::new(triangle.0.x, triangle.0.y, 0.0), Vector3::new(0.0, 0.0, 1.0)),
        ];
        ret.push(Polygon::new(vertices.to_vec(), None));
    }
    ret
}

// Triangulate a CSG sketch object into 3D CSG polygons
fn triangulate_csg_sketch(csg_sketch: &CSGSketch) -> Vec<Polygon<()>> {
    let mut ret = Vec::new();
    for geom in csg_sketch.geometry.iter() {
        match geom {
            Geometry::Polygon(poly2d) => {
                ret.extend(triangulate_geo_polygon(&poly2d));
            },
            Geometry::MultiPolygon(multipoly) => {
                for poly2d in multipoly {
                    ret.extend(triangulate_geo_polygon(&poly2d));
                }
            },
            // Optional: handle other geometry types like LineString here.
            _ => {},
        }
    }
    ret
}

