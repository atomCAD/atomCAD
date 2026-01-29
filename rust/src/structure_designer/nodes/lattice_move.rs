use crate::display::gadget::Gadget;
use crate::structure_designer::evaluator::network_evaluator::{
  NetworkEvaluationContext, NetworkEvaluator
};
use crate::structure_designer::evaluator::network_result::{
  runtime_type_error_in_input, GeometrySummary, NetworkResult
};
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::utils::xyz_gadget_utils;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::util::serialization_utils::ivec3_serializer;
use crate::structure_designer::text_format::TextValue;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::util::mat_utils::unit_ivec3;

#[derive(Debug, Clone)]
pub struct LatticeMoveEvalCache {
  pub unit_cell: UnitCellStruct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeMoveData {
  #[serde(with = "ivec3_serializer")]
  pub translation: IVec3,
  #[serde(default = "default_lattice_subdivision")]
  pub lattice_subdivision: i32,
}

fn default_lattice_subdivision() -> i32 {
  1
}

impl NodeData for LatticeMoveData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let lattice_move_cache = eval_cache.downcast_ref::<LatticeMoveEvalCache>()?;   
        let gadget = LatticeMoveGadget::new(
            self.translation,
            self.lattice_subdivision,
             &lattice_move_cache.unit_cell,
        );
        Some(Box::new(gadget))
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
      let shape_val = network_evaluator.evaluate_arg_required(network_stack, node_id, registry, context, 0);
    
      if let NetworkResult::Error(_) = shape_val {
        return shape_val;
      }
      else if let NetworkResult::Geometry(shape) = shape_val {

        let translation = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 1, 
          self.translation, 
          NetworkResult::extract_ivec3
        ) {
          Ok(value) => value,
          Err(error) => return error,
        };

        let lattice_subdivision = match network_evaluator.evaluate_or_default(
          network_stack, node_id, registry, context, 2, 
          self.lattice_subdivision, 
          NetworkResult::extract_int
        ) {
          Ok(value) => value.max(1), // Ensure minimum value of 1
          Err(error) => return error,
        };

        let subdivided_translation = translation.as_dvec3() / lattice_subdivision as f64;
        let real_translation = shape.unit_cell.dvec3_lattice_to_real(&subdivided_translation);

        // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
        // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
        if network_stack.len() == 1 {
          let eval_cache = LatticeMoveEvalCache {
            unit_cell: shape.unit_cell.clone(),
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        return NetworkResult::Geometry(GeometrySummary {
          unit_cell: shape.unit_cell.clone(),
          frame_transform: Transform::default(),
          geo_tree_root: GeoNode::transform(Transform::new(real_translation, DQuat::IDENTITY), Box::new(shape.geo_tree_root)),
        });

      } else {
        return runtime_type_error_in_input(0);
      }
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let show_translation = !connected_input_pins.contains("translation");
        let show_subdivision = !connected_input_pins.contains("subdivision") && self.lattice_subdivision != 1;

        match (show_translation, show_subdivision) {
            (true, true) => Some(format!("t: ({},{},{}), sub: {}", 
                self.translation.x, self.translation.y, self.translation.z, self.lattice_subdivision)),
            (true, false) => Some(format!("t: ({},{},{})", 
                self.translation.x, self.translation.y, self.translation.z)),
            (false, true) => Some(format!("sub: {}", self.lattice_subdivision)),
            (false, false) => None,
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("translation".to_string(), TextValue::IVec3(self.translation)),
            ("subdivision".to_string(), TextValue::Int(self.lattice_subdivision)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("translation") {
            self.translation = v.as_ivec3().ok_or_else(|| "translation must be an IVec3".to_string())?;
        }
        if let Some(v) = props.get("subdivision") {
            self.lattice_subdivision = v.as_int().ok_or_else(|| "subdivision must be an integer".to_string())?;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("shape".to_string(), (true, None)); // required
        m
    }
}


#[derive(Clone)]
pub struct LatticeMoveGadget {
    pub translation: IVec3,
    pub lattice_subdivision: i32,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_translation: IVec3,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for LatticeMoveGadget {
  fn tessellate(&self, output: &mut TessellationOutput) {
    let output_mesh: &mut Mesh = &mut output.mesh;
    xyz_gadget_utils::tessellate_xyz_gadget(
      output_mesh,
      &self.unit_cell,
      DQuat::IDENTITY,
      &self.get_real_position(),
      false
    );
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for LatticeMoveGadget {
  fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
      xyz_gadget_utils::xyz_gadget_hit_test(
          &self.unit_cell,
          DQuat::IDENTITY,
          &self.get_real_position(),
          &ray_origin,
          &ray_direction,
          false
      )
  }

  fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    self.dragged_handle_index = Some(handle_index);
    self.start_drag_offset = xyz_gadget_utils::get_dragged_axis_offset(
        &self.unit_cell,
        DQuat::IDENTITY,
        &self.get_real_position(),
        handle_index,
        &ray_origin,
        &ray_direction
    );
    self.start_drag_translation = self.translation;
  }

  fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
    let current_offset = xyz_gadget_utils::get_dragged_axis_offset(
        &self.unit_cell,
        DQuat::IDENTITY,
        &self.get_real_position(),
        handle_index,
        &ray_origin,
        &ray_direction
    );
    let offset_delta = current_offset - self.start_drag_offset;
    if self.apply_drag_offset(handle_index, offset_delta) {
      self.start_drag(handle_index, ray_origin, ray_direction);
    }
  }

  fn end_drag(&mut self) {
    self.dragged_handle_index = None;
  }
}

impl NodeNetworkGadget for LatticeMoveGadget {
  fn sync_data(&self, data: &mut dyn NodeData) {
      if let Some(lattice_move_data) = data.as_any_mut().downcast_mut::<LatticeMoveData>() {
        lattice_move_data.translation = self.translation;
      }
  }

  fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
      Box::new(self.clone())
  }
}

impl LatticeMoveGadget {
  pub fn new(translation: IVec3, lattice_subdivision: i32, unit_cell: &UnitCellStruct) -> Self {
      let ret = Self {
          translation,
          lattice_subdivision,
          dragged_handle_index: None,
          start_drag_offset: 0.0,
          start_drag_translation: translation,
          unit_cell: unit_cell.clone(),
      };
      return ret;
  }

  // Helper method to get the real space position accounting for subdivision
  fn get_real_position(&self) -> DVec3 {
    let subdivided_pos = self.translation.as_dvec3() / self.lattice_subdivision as f64;
    self.unit_cell.dvec3_lattice_to_real(&subdivided_pos)
  }

  // Returns whether the application of the drag offset was successful and the drag start should be reset
  fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
    let axis_basis_vector = self.unit_cell.get_basis_vector(axis_index);
    // Multiply by subdivision to allow fractional lattice steps
    let rounded_delta = (offset_delta / axis_basis_vector.length() * self.lattice_subdivision as f64).round();

    // Early return if no movement
    if rounded_delta == 0.0 {
      return false;
    }

    // Apply the movement to the translation
    self.translation = self.start_drag_translation + unit_ivec3(axis_index) * (rounded_delta as i32);

    return true;
  }
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "lattice_move".to_string(),
      description: "Moves the geometry in the discrete lattice space with a relative vector.
Continuous transformation in the lattice space is not allowed (for continuous transformations use the `atom_trans` node which is only available for atomic structures).
You can directly enter the translation vector or drag the axes of the gadget.".to_string(),
      category: NodeTypeCategory::Geometry3D,
      parameters: vec![
          Parameter {
              name: "shape".to_string(),
              data_type: DataType::Geometry,
          },
          Parameter {
            name: "translation".to_string(),
            data_type: DataType::IVec3,
          },
          Parameter {
            name: "subdivision".to_string(),
            data_type: DataType::Int,
          },
      ],
      output_type: DataType::Geometry,
      public: true,
      node_data_creator: || Box::new(LatticeMoveData {
        translation: IVec3::new(0, 0, 0),
        lattice_subdivision: 1,
      }),
      node_data_saver: generic_node_data_saver::<LatticeMoveData>,
      node_data_loader: generic_node_data_loader::<LatticeMoveData>,
    }
}
