use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use glam::i32::IVec2;
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
use crate::structure_designer::evaluator::network_result::UnitCellStruct;

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
    
      let real_min_corner = min_corner.as_dvec2();
      let real_extent = extent.as_dvec2();
      let center = real_min_corner + real_extent / 2.0;
    
      return NetworkResult::Geometry2D(
        GeometrySummary2D {
          unit_cell: UnitCellStruct::cubic_diamond(),
          frame_transform: Transform2D::new(
            center,
            0.0,
          ),
          geo_tree_root: GeoNode::Rect {
            min_corner: min_corner,
            extent: extent 
          },
        });
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}



