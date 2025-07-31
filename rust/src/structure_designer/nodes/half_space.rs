use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::utils::half_space_utils::get_dragged_shift;
use crate::util::timer::Timer;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use glam::f32::Vec3;
use glam::f64::DQuat;
use glam::f64::DVec3;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::util::hit_test_utils::sphere_hit_test;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::structure_designer::common_constants;
use std::collections::HashSet;
use crate::common::gadget::Gadget;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::utils::half_space_utils;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::utils::half_space_utils::{create_half_space_geo, HalfSpaceVisualization};
use crate::structure_designer::utils::half_space_utils::implicit_eval_half_space_calc;
use crate::common::csg_types::CSG;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationCache;

#[derive(Debug, Serialize, Deserialize)]
pub struct HalfSpaceData {
  pub max_miller_index: i32,
  #[serde(with = "ivec3_serializer")]
  pub miller_index: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub shift: i32,
}

impl NodeData for HalfSpaceData {

    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      return Some(Box::new(HalfSpaceGadget::new(
        self.max_miller_index,
        &self.miller_index,
        self.center,
        self.shift)));
    }
  
}

pub fn eval_half_space<'a>(
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64,
    _registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext
  ) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let half_space_data = &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();

  let geometry = if context.explicit_geo_eval_needed {
    create_half_space_geo(
        &half_space_data.miller_index,
        &half_space_data.center,
        half_space_data.shift,
        if network_stack.len() == 1 { HalfSpaceVisualization::Plane } else { HalfSpaceVisualization::Cuboid })
  } else {
    CSG::new()
  };
  
  let dir = half_space_data.miller_index.as_dvec3().normalize();
  let center_pos = half_space_data.center.as_dvec3();

  return NetworkResult::Geometry(GeometrySummary {
    frame_transform: Transform::new(
      center_pos,
      DQuat::from_rotation_arc(DVec3::Y, dir),
    ),
    csg: geometry});
}

pub fn implicit_eval_half_space<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _invocation_cache: &NodeInvocationCache,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
  let half_space_data = &node.data.as_any_ref().downcast_ref::<HalfSpaceData>().unwrap();
  return implicit_eval_half_space_calc(
    &half_space_data.miller_index, &half_space_data.center, half_space_data.shift,
    sample_point);
}

#[derive(Clone)]
pub struct HalfSpaceGadget {
    pub max_miller_index: i32,
    pub miller_index: IVec3,
    pub center: IVec3,
    pub dragged_shift: f64, // this is rounded into 'shift'
    pub shift: i32,
    pub dragged_handle_index: Option<i32>,
    pub possible_miller_indices: HashSet<IVec3>,
}

impl Tessellatable for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

        half_space_utils::tessellate_center_sphere(output_mesh, &self.center);

        half_space_utils::tessellate_shift_drag_handle(
            output_mesh,
            &self.center,
            &self.miller_index,
            self.dragged_shift);
        
        // If we are dragging any handle, show the plane grid for visual reference
        if self.dragged_handle_index.is_some() {
            half_space_utils::tessellate_plane_grid(
                output_mesh,
                &self.center,
                &self.miller_index,
                self.shift);
        }

        // Tessellate miller index discs only if we're dragging the central sphere (handle index 0)
        if self.dragged_handle_index == Some(0) {
            half_space_utils::tessellate_miller_indices_discs(
                output_mesh,
                &center_pos,
                &self.miller_index,
                &self.possible_miller_indices,
                self.max_miller_index);
        } 
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for HalfSpaceGadget {
    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: miller index handle (central red sphere)
    // handle 1: shift drag handle (blue cylinder)
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        // Test central sphere
        if let Some(_t) = half_space_utils::hit_test_center_sphere(
            &self.center,
            &ray_origin,
            &ray_direction
        ) {
            return Some(0); // Central sphere hit
        }
        
        // Test shift handle cylinder
        if let Some(_t) = half_space_utils::hit_test_shift_handle(
            &self.center,
            &self.miller_index,
            self.shift as f64,
            &ray_origin,
            &ray_direction
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
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        
        if handle_index == 0 {
            // Handle index already stored in dragged_handle_index during start_drag
            
            // Check if any miller index disc is hit
            if let Some(new_miller_index) = half_space_utils::hit_test_miller_indices_discs(
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
                &self.miller_index,
                &self.center,
                &ray_origin,
                &ray_direction, 
                half_space_utils::SHIFT_HANDLE_ACCESSIBILITY_OFFSET
            );
            self.shift = self.dragged_shift.round() as i32;
        }
    }

    fn end_drag(&mut self) {
        // Clear the dragged handle index to stop displaying the grid and conditional miller index discs
        self.dragged_handle_index = None;
    }
}

impl NodeNetworkGadget for HalfSpaceGadget {
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }

    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(half_space_data) = data.as_any_mut().downcast_mut::<HalfSpaceData>() {
            half_space_data.miller_index = self.miller_index;
            half_space_data.center = self.center;
            half_space_data.shift = self.shift;
        }
    }
}

impl HalfSpaceGadget {

    pub fn new(max_miller_index: i32, miller_index: &IVec3, center: IVec3, shift: i32) -> Self {        
        return Self {
            max_miller_index,
            miller_index: *miller_index,
            center,
            dragged_shift: shift as f64,
            shift,
            dragged_handle_index: None,
            possible_miller_indices: half_space_utils::generate_possible_miller_indices(max_miller_index),
        };
    }
}

