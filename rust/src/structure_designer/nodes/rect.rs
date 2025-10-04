use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use glam::i32::IVec2;
use glam::f64::DVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RectData {
  #[serde(with = "ivec2_serializer")]
  pub min_corner: IVec2,
  #[serde(with = "ivec2_serializer")]
  pub extent: IVec2,
}

impl NodeData for RectData {
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
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      let min_corner = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        self.min_corner, 
        NetworkResult::extract_ivec2
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let extent = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        self.extent, 
        NetworkResult::extract_ivec2
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let unit_cell = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        UnitCellStruct::cubic_diamond(), 
        NetworkResult::extract_unit_cell,
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let real_min_corner = unit_cell.ivec2_lattice_to_real(&min_corner);
      let real_extent = unit_cell.ivec2_lattice_to_real(&extent);
      let center = real_min_corner + real_extent / 2.0;
    
      let geo_tree_root = create_parallelogram_from_lattice(
        &unit_cell,
        min_corner.as_dvec2(),
        extent.as_dvec2(),
      );

      return NetworkResult::Geometry2D(
        GeometrySummary2D {
          unit_cell,
          frame_transform: Transform2D::new(
            center,
            0.0,
          ),
          geo_tree_root,
        });
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}

/// Creates a parallelogram in real space from lattice coordinates and unit cell basis vectors.
/// The parallelogram is represented as an intersection of 4 half-planes (2 pairs of opposing edges).
/// Uses the XZ plane for 2D operations (Y=0).
fn create_parallelogram_from_lattice(
  unit_cell: &UnitCellStruct,
  min_corner_lattice: DVec2,
  extent_lattice: DVec2
) -> GeoNode {
  // Convert lattice coordinates to real space coordinates using the proper UnitCellStruct method
  let min_corner_real = unit_cell.dvec2_lattice_to_real(&min_corner_lattice);
  
  // Calculate the four corners of the parallelogram in real space
  let corner_00 = min_corner_real; // min_corner
  let corner_10 = unit_cell.dvec2_lattice_to_real(&(min_corner_lattice + DVec2::new(extent_lattice.x, 0.0)));
  let corner_01 = unit_cell.dvec2_lattice_to_real(&(min_corner_lattice + DVec2::new(0.0, extent_lattice.y)));
  let corner_11 = unit_cell.dvec2_lattice_to_real(&(min_corner_lattice + extent_lattice)); // max_corner
  
  // Create 4 half-planes defining the parallelogram edges
  let mut half_planes = Vec::new();
  
  half_planes.push(GeoNode::HalfPlane {
    point1: corner_10,
    point2: corner_00,
  });
  half_planes.push(GeoNode::HalfPlane {
    point1: corner_01,
    point2: corner_11,
  });
  
  half_planes.push(GeoNode::HalfPlane {
    point1: corner_00,
    point2: corner_01,
  });
  half_planes.push(GeoNode::HalfPlane {
    point1: corner_11,
    point2: corner_10,
  });
  
  // Return the intersection of all half-planes
  GeoNode::Intersection2D {
    shapes: half_planes
  }
}
