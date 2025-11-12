/// Minimal test case to reproduce the CSG intersection bug in csgrs library.
/// 
/// Bug description: When intersecting 6 half-spaces to form a cube, the result contains
/// only 4 faces instead of the expected 6 faces.

use csgrs::traits::CSG;
use csgrs::mesh::Mesh;
use nalgebra::{Point3, Vector3};
use csgrs::mesh::polygon::Polygon;
use csgrs::mesh::vertex::Vertex;
use csgrs::float_types::Real;
use glam::{DVec3, DQuat};

/// Helper function to convert DVec3 to CSG Point3 (exactly matching atomCAD's dvec3_to_point3)
fn dvec3_to_point3(dvec3: DVec3) -> Point3<Real> {
    Point3::new(
        dvec3.x as Real, 
        dvec3.y as Real, 
        dvec3.z as Real
    )
}

/// Helper function to convert DVec3 to CSG Vector3 (exactly matching atomCAD's dvec3_to_vector3)  
fn dvec3_to_vector3(dvec3: DVec3) -> Vector3<Real> {
    Vector3::new(
        dvec3.x as Real, 
        dvec3.y as Real, 
        dvec3.z as Real
    )
}

/// Creates a half-space exactly like atomCAD's create_half_space_geo function
fn create_half_space_geo(normal: &DVec3, center_pos: &DVec3) -> Mesh<()> {
    let na_normal = dvec3_to_vector3(*normal);
    let rotation = DQuat::from_rotation_arc(DVec3::Z, *normal);

    let width: f64  = 800.0;
    let height: f64 = 800.0;

    let start_x = -width * 0.5;
    let start_y = -height * 0.5;
    let end_x = width * 0.5;
    let end_y = height * 0.5;

    // Front face vertices (at z=0) - counter-clockwise order
    let v1 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, start_y, 0.0)));
    let v2 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, start_y, 0.0)));
    let v3 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, end_y, 0.0)));
    let v4 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, end_y, 0.0)));

    // Debug: Print input vs output normal comparison with high precision
    println!("  Input normal: ({:.16},{:.16},{:.16})", normal.x, normal.y, normal.z);

    // Create polygons based on the visualization type
    let polygons = 
        vec![
          Polygon::new(
              vec![
                  Vertex::new(v1, na_normal),
                  Vertex::new(v2, na_normal),
                  Vertex::new(v3, na_normal),
                  Vertex::new(v4, na_normal),
              ], None
          ),
        ];

    let mesh = Mesh::from_polygons(&polygons, None)
      .translate(center_pos.x as Real, center_pos.y as Real, center_pos.z as Real);
    
    // Debug: Print the 0-th polygon of the created mesh
    println!("  Mesh has {} polygons total", mesh.polygons.len());
    if !mesh.polygons.is_empty() {
        let poly = &mesh.polygons[0];
        println!("  Mesh polygon[0]: {} vertices", poly.vertices.len());
        
        // Print plane normal with high precision
        let plane_normal = poly.plane.normal();
        println!("  Plane normal: ({:.16},{:.16},{:.16})", 
                 plane_normal.x, plane_normal.y, plane_normal.z);
        
        for (i, vertex) in poly.vertices.iter().enumerate() {
            println!("    vertex[{}]: pos=({:.16},{:.16},{:.16}) normal=({:.16},{:.16},{:.16})", 
                     i, vertex.pos.x, vertex.pos.y, vertex.pos.z, 
                     vertex.normal.x, vertex.normal.y, vertex.normal.z);
        }
    }
    
    return mesh;
}

/// Test case data structure
struct TestCase {
    name: &'static str,
    half_spaces: Vec<(DVec3, DVec3)>, // (normal, center_pos) pairs
}

/// Test function that intersects half-spaces and reports results
fn test_intersection(test_case: &TestCase) {
    println!("Testing: {}", test_case.name);
    println!("Half-spaces: {}", test_case.half_spaces.len());
    
    // Create meshes from half-space parameters
    let mut meshes = Vec::new();
    for (i, (normal, center_pos)) in test_case.half_spaces.iter().enumerate() {
        let mesh = create_half_space_geo(normal, center_pos);
        println!("Half-space {}: {} polygons", i, mesh.polygons.len());
        meshes.push(mesh);
    }
    
    // Perform intersection
    println!("Performing intersection...");
    let mut result = meshes[0].clone();
    for i in 1..meshes.len() {
        result = result.intersection(&meshes[i]);
        println!("After intersecting with half-space {}: {} polygons", i, result.polygons.len());
    }
    
    println!("Final result: {} polygons", result.polygons.len());
    println!("Expected: 6 polygons (cube faces)");
    
    if result.polygons.len() != 6 {
        println!("❌ BUG REPRODUCED: Expected 6 faces, got {}", result.polygons.len());
    } else {
        println!("✅ Working correctly: Got expected 6 faces");
    }
    println!();
}

fn main() {
    println!("CSG Intersection Bug Test - Minimal Version");
    println!("===========================================");
    
    // Test case 1: Simple cube (should work correctly)
    let simple_case = TestCase {
        name: "Simple 10x10x10 cube",
        half_spaces: vec![
            // X faces
            (DVec3::new(-1.0, 0.0, 0.0), DVec3::new(0.0, 5.0, 5.0)),   // min X
            (DVec3::new(1.0, 0.0, 0.0), DVec3::new(10.0, 5.0, 5.0)),   // max X
            // Y faces  
            (DVec3::new(0.0, -1.0, 0.0), DVec3::new(5.0, 0.0, 5.0)),   // min Y
            (DVec3::new(0.0, 1.0, 0.0), DVec3::new(5.0, 10.0, 5.0)),   // max Y
            // Z faces
            (DVec3::new(0.0, 0.0, -1.0), DVec3::new(5.0, 5.0, 0.0)),   // min Z
            (DVec3::new(0.0, 0.0, 1.0), DVec3::new(5.0, 5.0, 10.0)),   // max Z
        ],
    };
    
    // Test case 2: Almost problematic cube
    let almost_problematic_case = TestCase {
        name: "Almost problematic atomCAD cube (exact reproduction case)",
        half_spaces: vec![
            // X faces
            (DVec3::new(-1.0, -0.0, -0.0), DVec3::new(0.0, 120.0, 130.0)),
            (DVec3::new(1.0, 0.0, 0.0), DVec3::new(256.0, 120.0, 130.0)),
            // Y faces
            (DVec3::new(-0.0, -1.0, -0.0), DVec3::new(128.0, 0.0, 130.0)),
            (DVec3::new(0.0, 1.0, 0.0), DVec3::new(128.0, 240.0, 130.0)),
            // Z faces  
            (DVec3::new(-0.0, -0.0, -1.0), DVec3::new(128.0, 120.0, 0.0)),
            (DVec3::new(0.0, 0.0, 1.0), DVec3::new(128.412, 120.0, 260.0)),
        ],
    };

    // Test case 3: Problematic cube (exact values from atomCAD that reproduce the bug)
    let problematic_case = TestCase {
        name: "Problematic atomCAD cube (exact reproduction case)",
        half_spaces: vec![
            // X faces
            (DVec3::new(-0.9999999999999999, -0.0, -0.0), DVec3::new(0.0, 120.0, 130.0)),
            (DVec3::new(0.9999999999999999, 0.0, 0.0), DVec3::new(256.0, 120.0, 130.0)),
            // Y faces
            (DVec3::new(-0.0, -0.9999999999999999, -0.0), DVec3::new(128.0, 0.0, 130.0)),
            (DVec3::new(0.0, 0.9999999999999999, 0.0), DVec3::new(128.0, 240.0, 130.0)),
            // Z faces  
            (DVec3::new(-0.0, -0.0, -0.9999999999999999), DVec3::new(128.0, 120.0, 0.0)),
            (DVec3::new(0.0, 0.0, 0.9999999999999999), DVec3::new(128.412, 120.0, 260.0)),
        ],
    };
    
    test_intersection(&simple_case);
    println!("{}", "=".repeat(50));
    test_intersection(&almost_problematic_case);
    println!("{}", "=".repeat(50));
    test_intersection(&problematic_case);
}
