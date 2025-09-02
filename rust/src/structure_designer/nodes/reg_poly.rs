use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use glam::f64::DVec2;
use glam::i32::IVec2;
use std::f64::consts::PI;
use std::cmp::max;
use crate::util::mat_utils::consistent_round;
use crate::common::csg_types::CSG;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationCache;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;

#[derive(Debug, Serialize, Deserialize)]
pub struct RegPolyData {
    pub num_sides: i32,     // Number of sides for the polygon
    pub radius: i32,        // Approximate radius in lattice units
}

impl NodeData for RegPolyData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }
}

pub fn eval_reg_poly<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, _registry: &NodeTypeRegistry) -> NetworkResult {
    let node = NetworkStackElement::get_top_node(network_stack, node_id);
    let polygon_data = &node.data.as_any_ref().downcast_ref::<RegPolyData>().unwrap();

    let num_sides = max(3, polygon_data.num_sides);
    let radius = max(1, polygon_data.radius);

    let mut vertices: Vec<IVec2> = Vec::new();

    for i in 0..num_sides {
        // Calculate the ideal angle for this vertex
        let angle = kth_angle(i, num_sides);        
        // Find the lattice point for this angle
        vertices.push(find_lattice_point(angle, radius));
    }

    // Create a transform at the center of the polygon (origin)
    // No rotation is needed for this type of shape
    return NetworkResult::Geometry2D(
      GeometrySummary2D {
        frame_transform: Transform2D::new(
          DVec2::new(0.0, 0.0),  // Center at origin
          0.0,                   // No rotation
        ),
        geo_tree_root: GeoNode::Polygon { vertices },
      }
    );
}

/// Calculates the closest lattice point to a given floating point position
fn closest_lattice_point(x: f64, y: f64) -> IVec2 {
    IVec2::new(consistent_round(x), consistent_round(y))
}

/// Finds a lattice point close to the ideal position but with reasonably small Miller indices
fn find_lattice_point(angle: f64, radius: i32) -> IVec2 {
    // Start with the ideal position
    let ideal_x = (radius as f64) * angle.cos();
    let ideal_y = (radius as f64) * angle.sin();
    
    // Find closest lattice point
    let base_point = closest_lattice_point(ideal_x, ideal_y);
    
    return base_point;

}

/// Calculates the half plane parameters for a polygon side defined by two lattice points
fn calculate_half_plane_for_side(p1: IVec2, p2: IVec2, center: IVec2) -> (IVec2, IVec2) {
    // We need to ensure the half-plane faces inward toward the center
    // Check if the line from p1 to p2 has the center on its left side
    // Cross product to determine which side the center is on
    let v1 = p2 - p1;
    let v2 = center - p1;
    let cross_z = v1.x * v2.y - v1.y * v2.x;
    
    // If cross_z > 0, center is on the left, so we need p1->p2
    // If cross_z < 0, center is on the right, so we need p2->p1
    if cross_z >= 0 {
        (p1, p2)
    } else {
        (p2, p1)
    }
}

pub fn implicit_eval_reg_poly<'a>(
    _evaluator: &ImplicitEvaluator,
    _registry: &NodeTypeRegistry,
    _invocation_cache: &NodeInvocationCache,
    _network_stack: &Vec<NetworkStackElement<'a>>,
    node: &Node,
    sample_point: &DVec2) -> f64 {
    
    let polygon_data = &node.data.as_any_ref().downcast_ref::<RegPolyData>().unwrap();
    
    // Ensure we have at least 3 sides
    let num_sides = max(3, polygon_data.num_sides);
    let radius = max(1, polygon_data.radius);
    
    // Center point is at the origin
    let center = IVec2::new(0, 0);
    
    // Initialize with a large negative value
    let mut max_distance = f64::NEG_INFINITY;
    
    //println!("sample_pont: {:?}", sample_point);

    // Generate the polygon by finding lattice points near the ideal positions
    for i in 0..num_sides {
        // Calculate the ideal angle for this vertex
        let angle = kth_angle(i, num_sides);
        
        // Find the lattice point for this angle
        let p1 = find_lattice_point(angle, radius);

        // Calculate the next vertex
        let next_angle = kth_angle((i + 1) % num_sides, num_sides);
        let p2 = find_lattice_point(next_angle, radius);
        
        // Ensure the half-plane is oriented correctly (facing inward)
        let (half_plane_p1, half_plane_p2) = calculate_half_plane_for_side(p1, p2, center);
        
        // Calculate the half-plane SDF for this side
        // Convert points to double precision for calculations
        let point1 = half_plane_p1.as_dvec2();
        let point2 = half_plane_p2.as_dvec2();
        
        //println!("point1: {:?}, point2: {:?}", point1, point2);

        // Calculate line direction and normal (similar to half_plane.rs)
        let dir_vector = point1 - point2;
        let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
        
        // Calculate signed distance from sample_point to the line
        let distance = normal.dot(*sample_point - point1);
        
        // Update max_distance (for CSG intersection, we take the maximum of all SDFs)
        max_distance = max_distance.max(distance);
    }
    
    //println!("max_distance: {}", max_distance);

    // Return the SDF value for the polygon
    return max_distance;
}


fn kth_angle(k: i32, num_sides: i32) -> f64 {
    return 2.0 * PI * (k as f64) / (num_sides as f64);
}