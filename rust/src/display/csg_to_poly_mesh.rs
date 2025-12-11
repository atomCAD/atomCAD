use crate::geo_tree::csg_types::CSGMesh;
use crate::geo_tree::csg_types::CSGSketch;
use crate::display::poly_mesh::PolyMesh;
use crate::util::unique_3d_points::Unique3DPoints;
use crate::geo_tree::csg_utils::{scale_to_csg, unscale_from_csg};
use glam::DVec3;
use glam::f64::DVec2;
use crate::crystolecule::drawing_plane::DrawingPlane;
use csgrs::{float_types::Real, mesh::polygon::Polygon, mesh::vertex::Vertex};
use nalgebra::Point3;
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

pub fn convert_csg_sketch_to_poly_mesh(csg_sketch: CSGSketch, triangulate_2d: bool, drawing_plane: &DrawingPlane) -> PolyMesh {
 
    let mut polys = if triangulate_2d {
        triangulate_csg_sketch(&csg_sketch, drawing_plane)
    } else {
        let mesh = CSGMesh::from(csg_sketch);
        mesh.polygons
    };
    
    // Check if plane is horizontal (parallel to XY) and at z≈0
    // If so, apply small z-offset to avoid z-fighting with grid
    let u_real = drawing_plane.unit_cell.ivec3_lattice_to_real(&drawing_plane.u_axis);
    let v_real = drawing_plane.unit_cell.ivec3_lattice_to_real(&drawing_plane.v_axis);
    let plane_normal = u_real.cross(v_real).normalize();
    let plane_origin = drawing_plane.unit_cell.ivec3_lattice_to_real(&drawing_plane.center);
    
    let is_horizontal_at_z_zero = plane_normal.z.abs() > 0.99 && plane_origin.z.abs() < 0.01;
    
    if is_horizontal_at_z_zero {
        // Add small Z offset to render 2D sketches slightly above the grid (avoid z-fighting)
        for poly in &mut polys {
            for vertex in &mut poly.vertices {
                vertex.pos.z += scale_to_csg(0.001) as Real;
            }
        }
    }

    convert_polygons_to_poly_mesh(&polys, true, false)
}

fn convert_polygons_to_poly_mesh(polygons: &Vec<Polygon<()>>, open: bool, hatched: bool) -> PolyMesh {

    // Use our spatial hashing structure with a small epsilon for vertex precision
    // The epsilon value should be adjusted based on the scale of your models
    let epsilon = unscale_from_csg(1e-5);
    let mut unique_vertices = Unique3DPoints::new(epsilon);

    let mut poly_mesh = PolyMesh::new(open, hatched);

    // Process each polygon in the CSG
    for polygon in polygons {
        let mut face_vertices = Vec::new();
        
        // Process each vertex in the polygon
        for vertex in &polygon.vertices {
            // Convert nalgebra Point3 to glam DVec3 and unscale from CSG coordinates
            let position = DVec3::new(
                unscale_from_csg(vertex.pos.x as f64),
                unscale_from_csg(vertex.pos.y as f64),
                unscale_from_csg(vertex.pos.z as f64),
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

fn triangulate_geo_polygon(poly2d: &GeoPolygon<Real>, drawing_plane: &DrawingPlane) -> Vec<Polygon<()>> {
    let mut ret = Vec::new();
    
    // Calculate plane normal from u × v cross product
    let u_real = drawing_plane.unit_cell.ivec3_lattice_to_real(&drawing_plane.u_axis);
    let v_real = drawing_plane.unit_cell.ivec3_lattice_to_real(&drawing_plane.v_axis);
    let plane_normal = u_real.cross(v_real).normalize();
    
    for triangle in poly2d.earcut_triangles() {
        // Map 2D plane coordinates to 3D world positions using drawing_plane
        let p0_3d = drawing_plane.real_2d_to_world_3d(&DVec2::new(triangle.0.x as f64, triangle.0.y as f64));
        let p1_3d = drawing_plane.real_2d_to_world_3d(&DVec2::new(triangle.1.x as f64, triangle.1.y as f64));
        let p2_3d = drawing_plane.real_2d_to_world_3d(&DVec2::new(triangle.2.x as f64, triangle.2.y as f64));
        
        // Note: geo crate's earcut_triangles() produces clockwise triangles
        // even from counter-clockwise input polygons. We reverse the vertex 
        // order to get counter-clockwise triangles for proper rendering.
        let vertices = [
            Vertex::new(
                Point3::new(p2_3d.x as Real, p2_3d.y as Real, p2_3d.z as Real),
                nalgebra::Vector3::new(plane_normal.x as Real, plane_normal.y as Real, plane_normal.z as Real)
            ),
            Vertex::new(
                Point3::new(p1_3d.x as Real, p1_3d.y as Real, p1_3d.z as Real),
                nalgebra::Vector3::new(plane_normal.x as Real, plane_normal.y as Real, plane_normal.z as Real)
            ),
            Vertex::new(
                Point3::new(p0_3d.x as Real, p0_3d.y as Real, p0_3d.z as Real),
                nalgebra::Vector3::new(plane_normal.x as Real, plane_normal.y as Real, plane_normal.z as Real)
            ),
        ];
        ret.push(Polygon::new(vertices.to_vec(), None));
    }
    ret
}

// Triangulate a CSG sketch object into 3D CSG polygons
fn triangulate_csg_sketch(csg_sketch: &CSGSketch, drawing_plane: &DrawingPlane) -> Vec<Polygon<()>> {
    let mut ret = Vec::new();
    for geom in csg_sketch.geometry.iter() {
        match geom {
            Geometry::Polygon(poly2d) => {
                ret.extend(triangulate_geo_polygon(&poly2d, drawing_plane));
            },
            Geometry::MultiPolygon(multipoly) => {
                for poly2d in multipoly {
                    ret.extend(triangulate_geo_polygon(&poly2d, drawing_plane));
                }
            },
            // Optional: handle other geometry types like LineString here.
            _ => {},
        }
    }
    ret
}
