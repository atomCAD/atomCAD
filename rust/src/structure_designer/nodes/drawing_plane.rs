use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::utils::half_space_utils::get_dragged_shift;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::ivec3_serializer;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::Tessellatable;
use std::collections::HashSet;
use crate::display::gadget::Gadget;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::utils::half_space_utils;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::crystolecule::drawing_plane::DrawingPlane;
use glam::f64::DVec3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingPlaneData {
  pub max_miller_index: i32,
  #[serde(with = "ivec3_serializer")]
  pub miller_index: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub shift: i32,
  #[serde(default = "default_subdivision")]
  pub subdivision: i32,
}

fn default_subdivision() -> i32 {
  1
}

impl NodeData for DrawingPlaneData {

    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      let eval_cache = structure_designer.get_selected_node_eval_cache()?;
      let drawing_plane_cache = eval_cache.downcast_ref::<DrawingPlaneEvalCache>()?;

      return Some(Box::new(DrawingPlaneGadget::new(
        self.max_miller_index,
        &self.miller_index,
        self.center,
        self.shift,
        self.subdivision,
        &drawing_plane_cache.unit_cell)));
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
    
      let unit_cell = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        UnitCellStruct::cubic_diamond(), 
        NetworkResult::extract_unit_cell,
        ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let miller_index = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 1, 
        self.miller_index, 
        NetworkResult::extract_ivec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let center = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 2, 
        self.center,
        NetworkResult::extract_ivec3
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let shift = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 3, 
        self.shift,
        NetworkResult::extract_int
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      let subdivision = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 4, 
        self.subdivision,
        NetworkResult::extract_int
      ) {
        Ok(value) => value.max(1), // Ensure minimum value of 1
        Err(error) => return error,
      };

      // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
      // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
      if network_stack.len() == 1 {
        let eval_cache = DrawingPlaneEvalCache {
          unit_cell: unit_cell.clone(),
        };
        context.selected_node_eval_cache = Some(Box::new(eval_cache));
      }

      // Create DrawingPlane using the new constructor
      let drawing_plane = match DrawingPlane::new(
        unit_cell,
        miller_index,
        center,
        shift,
        subdivision,
      ) {
        Ok(plane) => plane,
        Err(error_msg) => return NetworkResult::Error(error_msg),
      };

      return NetworkResult::DrawingPlane(drawing_plane);
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let center_connected = connected_input_pins.contains("center");
        let m_index_connected = connected_input_pins.contains("m_index");
        let shift_connected = connected_input_pins.contains("shift");
        let subdivision_connected = connected_input_pins.contains("subdivision");
        
        if center_connected && m_index_connected && shift_connected && subdivision_connected {
            None
        } else {
            let mut parts = Vec::new();
            
            if !center_connected {
                parts.push(format!("c: ({},{},{})", 
                    self.center.x, self.center.y, self.center.z));
            }
            
            if !m_index_connected {
                parts.push(format!("m: ({},{},{})", 
                    self.miller_index.x, self.miller_index.y, self.miller_index.z));
            }
            
            if !shift_connected {
                parts.push(format!("s: {}", self.shift));
            }
            
            if !subdivision_connected && self.subdivision != 1 {
                parts.push(format!("sub: {}", self.subdivision));
            }
            
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrawingPlaneEvalCache {
  pub unit_cell: UnitCellStruct,
}

#[derive(Clone)]
pub struct DrawingPlaneGadget {
    pub max_miller_index: i32,
    pub miller_index: IVec3,
    pub center: IVec3,
    pub dragged_shift: f64, // this is rounded into 'shift'
    pub shift: i32,
    pub subdivision: i32,
    pub dragged_handle_index: Option<i32>,
    pub possible_miller_indices: HashSet<IVec3>,
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for DrawingPlaneGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let center_pos = self.unit_cell.ivec3_lattice_to_real(&self.center);

        half_space_utils::tessellate_center_sphere(output_mesh, &center_pos);

        half_space_utils::tessellate_shift_drag_handle(
            output_mesh,
            &self.center,
            &self.miller_index,
            self.dragged_shift,
            &self.unit_cell,
            self.subdivision);
        
        // If we are dragging any handle, show the plane grid for visual reference
        if self.dragged_handle_index.is_some() {
            half_space_utils::tessellate_plane_grid(
                output_mesh,
                &self.center,
                &self.miller_index,
                self.shift,
                &self.unit_cell,
                self.subdivision);
        }

        // Tessellate miller index discs only if we're dragging the central sphere (handle index 0)
        if self.dragged_handle_index == Some(0) {
            half_space_utils::tessellate_miller_indices_discs(
                output_mesh,
                &center_pos,
                &self.miller_index,
                &self.possible_miller_indices,
                self.max_miller_index,
                &self.unit_cell);
        } 
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for DrawingPlaneGadget {
    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: miller index handle (central red sphere)
    // handle 1: shift drag handle (blue cylinder)
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        // Test central sphere
        if let Some(_t) = half_space_utils::hit_test_center_sphere(
            &self.unit_cell,
            &self.center,
            &ray_origin,
            &ray_direction
        ) {
            return Some(0); // Central sphere hit
        }
        
        // Test shift handle cylinder
        if let Some(_t) = half_space_utils::hit_test_shift_handle(
            &self.unit_cell,
            &self.center,
            &self.miller_index,
            self.shift as f64,
            &ray_origin,
            &ray_direction,
            self.subdivision,
        ) {
            return Some(1); // Shift handle hit
        }

        None // No handle was hit
    }

    fn start_drag(&mut self, handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
        self.dragged_handle_index = Some(handle_index);
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        // Calculate center position in world space
        let center_pos = self.unit_cell.ivec3_lattice_to_real(&self.center);
        
        if handle_index == 0 {
            // Handle index already stored in dragged_handle_index during start_drag
            
            // Check if any miller index disc is hit
            if let Some(new_miller_index) = half_space_utils::hit_test_miller_indices_discs(
                &self.unit_cell,
                &center_pos,
                &self.possible_miller_indices,
                self.max_miller_index,
                ray_origin,
                ray_direction) {
                // Set the miller index to the hit disc's miller index
                self.miller_index = new_miller_index;
            }
        } else if handle_index == 1 {
            // Handle dragging the shift handle
            // We need to determine the new shift value based on where the mouse ray is closest to the normal ray
            self.dragged_shift = get_dragged_shift(
                &self.unit_cell,
                &self.miller_index,
                &self.center,
                &ray_origin,
                &ray_direction, 
                half_space_utils::SHIFT_HANDLE_ACCESSIBILITY_OFFSET,
                self.subdivision,
            );
            self.shift = self.dragged_shift.round() as i32;
        }
    }

    fn end_drag(&mut self) {
        // Clear the dragged handle index to stop displaying the grid and conditional miller index discs
        self.dragged_handle_index = None;
    }
}

impl NodeNetworkGadget for DrawingPlaneGadget {
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }

    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(drawing_plane_data) = data.as_any_mut().downcast_mut::<DrawingPlaneData>() {
            drawing_plane_data.miller_index = self.miller_index;
            drawing_plane_data.center = self.center;
            drawing_plane_data.shift = self.shift;
        }
    }
}

impl DrawingPlaneGadget {

    pub fn new(max_miller_index: i32, miller_index: &IVec3, center: IVec3, shift: i32, subdivision: i32, unit_cell: &UnitCellStruct) -> Self {        
        return Self {
            max_miller_index,
            miller_index: *miller_index,
            center,
            dragged_shift: shift as f64,
            shift,
            subdivision,
            dragged_handle_index: None,
            possible_miller_indices: half_space_utils::generate_possible_miller_indices(max_miller_index),
            unit_cell: unit_cell.clone(),
        };
    }
}
