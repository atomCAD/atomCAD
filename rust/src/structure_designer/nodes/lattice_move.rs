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
use crate::util::serialization_utils::ivec3_serializer;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use glam::DQuat;
use crate::util::transform::Transform;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::util::mat_utils::unit_ivec3;

#[derive(Debug, Clone)]
pub struct LatticeMoveEvalCache {
  pub unit_cell: UnitCellStruct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeMoveData {
  #[serde(with = "ivec3_serializer")]
  pub translation: IVec3,
}

impl NodeData for LatticeMoveData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        let eval_cache = structure_designer.get_selected_node_eval_cache()?;
        let lattice_move_cache = eval_cache.downcast_ref::<LatticeMoveEvalCache>()?;   
        let gadget = LatticeMoveGadget::new(
            self.translation,
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

        let real_translation = shape.unit_cell.ivec3_lattice_to_real(&translation);

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

        if show_translation {
            Some(format!("t: ({},{},{})", 
                self.translation.x, self.translation.y, self.translation.z))
        } else {
          None
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }
}


#[derive(Clone)]
pub struct LatticeMoveGadget {
    pub translation: IVec3,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_translation: IVec3,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for LatticeMoveGadget {
  fn tessellate(&self, output_mesh: &mut Mesh) {
    xyz_gadget_utils::tessellate_xyz_gadget(
      output_mesh,
      &self.unit_cell,
      DQuat::IDENTITY,
      &self.unit_cell.ivec3_lattice_to_real(&self.translation),
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
          &self.unit_cell.ivec3_lattice_to_real(&self.translation),
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
        &self.unit_cell.ivec3_lattice_to_real(&self.translation),
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
        &self.unit_cell.ivec3_lattice_to_real(&self.translation),
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
  pub fn new(translation: IVec3, unit_cell: &UnitCellStruct) -> Self {
      let ret = Self {
          translation,
          dragged_handle_index: None,
          start_drag_offset: 0.0,
          start_drag_translation: translation,
          unit_cell: unit_cell.clone(),
      };
      return ret;
  }

  // Returns whether the application of the drag offset was successful and the drag start should be reset
  fn apply_drag_offset(&mut self, axis_index: i32, offset_delta: f64) -> bool {
    let axis_basis_vector = self.unit_cell.get_basis_vector(axis_index);
    let rounded_delta = (offset_delta / axis_basis_vector.length()).round();

    // Early return if no movement
    if rounded_delta == 0.0 {
      return false;
    }

    // Apply the movement to the translation
    self.translation = self.start_drag_translation + unit_ivec3(axis_index) * (rounded_delta as i32);

    return true;
  }
}
