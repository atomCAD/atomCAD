use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use serde::{Serialize, Deserialize};
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DVec2;
use glam::i32::IVec2;
use std::f64::consts::PI;
use std::cmp::max;
use crate::util::mat_utils::consistent_round;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegPolyData {
    pub num_sides: i32,     // Number of sides for the polygon
    pub radius: i32,        // Approximate radius in lattice units
}

impl NodeData for RegPolyData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext) -> NetworkResult {    
        let num_sides = max(3, self.num_sides);
        let radius = max(1, self.radius);
    
        let unit_cell = match network_evaluator.evaluate_or_default(
            network_stack, node_id, registry, context, 0, 
            UnitCellStruct::cubic_diamond(), 
            NetworkResult::extract_unit_cell,
        ) {
            Ok(value) => value,
            Err(error) => return error,
        };


        let mut vertices: Vec<IVec2> = Vec::new();
    
        for i in 0..num_sides {
            // Calculate the ideal angle for this vertex
            let angle = kth_angle(i, num_sides);        
            // Find the lattice point for this angle
            vertices.push(find_lattice_point(angle, radius));
        }
    
        let real_vertices = vertices.iter().map(|v| {
            unit_cell.ivec2_lattice_to_real(v)
        }).collect();

        // Create a transform at the center of the polygon (origin)
        // No rotation is needed for this type of shape
        return NetworkResult::Geometry2D(
          GeometrySummary2D {
            unit_cell: unit_cell,
            frame_transform: Transform2D::new(
              DVec2::new(0.0, 0.0),  // Center at origin
              0.0,                   // No rotation
            ),
            geo_tree_root: GeoNode::Polygon { vertices: real_vertices },
          }
        );
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
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

fn kth_angle(k: i32, num_sides: i32) -> f64 {
    return 2.0 * PI * (k as f64) / (num_sides as f64);
}