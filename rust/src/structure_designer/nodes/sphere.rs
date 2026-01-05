use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::ivec3_serializer;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::util::transform::Transform;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::f64::DQuat;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::geo_tree::GeoNode;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SphereData {
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub radius: i32,
}

impl NodeData for SphereData {
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
      let center = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        self.center, 
        NetworkResult::extract_ivec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };
    
      let radius = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        self.radius, 
        NetworkResult::extract_int
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

      let real_center = unit_cell.ivec3_lattice_to_real(&center);
      let real_radius = unit_cell.int_lattice_to_real(radius);

      return NetworkResult::Geometry(GeometrySummary { 
        unit_cell,
        frame_transform: Transform::new(
        real_center,
        DQuat::IDENTITY,
        ),
        geo_tree_root: GeoNode::sphere(real_center, real_radius),
      });
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let show_center = !connected_input_pins.contains("center");
        let show_radius = !connected_input_pins.contains("radius");
        
        match (show_center, show_radius) {
            (true, true) => Some(format!("c: ({},{},{}) r: {}", 
                self.center.x, self.center.y, self.center.z, self.radius)),
            (true, false) => Some(format!("c: ({},{},{})", 
                self.center.x, self.center.y, self.center.z)),
            (false, true) => Some(format!("r: {}", self.radius)),
            (false, false) => None,
        }
    }
    
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "sphere".to_string(),
      description: "Outputs a sphere with integer center coordinates and integer radius.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
        Parameter {
            name: "center".to_string(),
            data_type: DataType::IVec3,
        },
        Parameter {
          name: "radius".to_string(),
          data_type: DataType::Int,
        },
        Parameter {
          name: "unit_cell".to_string(),
          data_type: DataType::UnitCell,
        },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(SphereData {
        center: IVec3::new(0, 0, 0),
        radius: 1,
      }),
      node_data_saver: generic_node_data_saver::<SphereData>,
      node_data_loader: generic_node_data_loader::<SphereData>,
    }
}
