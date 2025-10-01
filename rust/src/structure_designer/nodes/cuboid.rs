use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::util::transform::Transform;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DQuat;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_result::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuboidData {
  #[serde(with = "ivec3_serializer")]
  pub min_corner: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub extent: IVec3,
}

impl NodeData for CuboidData {
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
      context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
      let min_corner = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        self.min_corner, 
        NetworkResult::extract_ivec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let extent = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        self.extent, 
        NetworkResult::extract_ivec3
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

      let real_min_corner = unit_cell.lattice_to_real_ivec3(&min_corner);
      let real_extent = unit_cell.lattice_to_real_ivec3(&extent);
      let center = real_min_corner + real_extent / 2.0;

      let geo_tree_root = create_parallelepiped_from_lattice(
        &unit_cell,
        min_corner.as_dvec3(),
        extent.as_dvec3()
      );

      return NetworkResult::Geometry(GeometrySummary {
        unit_cell,
        frame_transform: Transform::new(
          center,
          DQuat::IDENTITY,
        ),
        geo_tree_root,
      });
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}

/// Creates a parallelepiped in real space from lattice coordinates and unit cell basis vectors.
/// The parallelepiped is represented as an intersection of 6 half-spaces (3 pairs of opposing faces).
fn create_parallelepiped_from_lattice(
  unit_cell: &UnitCellStruct,
  min_corner_lattice: DVec3,
  extent_lattice: DVec3
) -> GeoNode {
  // Get the unit cell basis vectors
  let basis_a = unit_cell.a;
  let basis_b = unit_cell.b;
  let basis_c = unit_cell.c;
  
  // Convert lattice coordinates to real space coordinates
  let min_corner_real = min_corner_lattice.x * basis_a + 
                       min_corner_lattice.y * basis_b + 
                       min_corner_lattice.z * basis_c;
  
  let max_corner_lattice = min_corner_lattice + extent_lattice;
  let max_corner_real = max_corner_lattice.x * basis_a + 
                       max_corner_lattice.y * basis_b + 
                       max_corner_lattice.z * basis_c;
  
  // Create 6 half-spaces defining the parallelepiped faces
  let mut half_spaces = Vec::new();
  
  // For each basis direction, create two opposing half-spaces
  // Face pair perpendicular to basis_a
  let normal_a = basis_a.normalize();
  let min_point_a = min_corner_real;
  let max_point_a = min_corner_real + extent_lattice.x * basis_a;
  
  half_spaces.push(GeoNode::HalfSpace {
    miller_index: normal_a,
    center: min_point_a,
    shift: 0.0,
  });
  half_spaces.push(GeoNode::HalfSpace {
    miller_index: -normal_a,
    center: max_point_a,
    shift: 0.0,
  });
  
  // Face pair perpendicular to basis_b
  let normal_b = basis_b.normalize();
  let min_point_b = min_corner_real;
  let max_point_b = min_corner_real + extent_lattice.y * basis_b;
  
  half_spaces.push(GeoNode::HalfSpace {
    miller_index: normal_b,
    center: min_point_b,
    shift: 0.0,
  });
  half_spaces.push(GeoNode::HalfSpace {
    miller_index: -normal_b,
    center: max_point_b,
    shift: 0.0,
  });
  
  // Face pair perpendicular to basis_c
  let normal_c = basis_c.normalize();
  let min_point_c = min_corner_real;
  let max_point_c = min_corner_real + extent_lattice.z * basis_c;
  
  half_spaces.push(GeoNode::HalfSpace {
    miller_index: normal_c,
    center: min_point_c,
    shift: 0.0,
  });
  half_spaces.push(GeoNode::HalfSpace {
    miller_index: -normal_c,
    center: max_point_c,
    shift: 0.0,
  });
  
  // Return the intersection of all half-spaces
  GeoNode::Intersection3D {
    shapes: half_spaces
  }
}
