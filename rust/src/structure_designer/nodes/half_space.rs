use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
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

pub const CENTER_SPHERE_RADIUS: f64 = 0.25;
const CENTER_SPHERE_HORIZONTAL_DIVISIONS: u32 = 16;
const CENTER_SPHERE_VERTICAL_DIVISIONS: u32 = 16;

// Constants for shift drag handle
const SHIFT_HANDLE_ACCESSIBILITY_OFFSET: f64 = 3.0;
const SHIFT_HANDLE_AXIS_RADIUS: f64 = 0.1;
const SHIFT_HANDLE_CYLINDER_RADIUS: f64 = 0.3;
const SHIFT_HANDLE_CYLINDER_LENGTH: f64 = 1.0;
const SHIFT_HANDLE_DIVISIONS: u32 = 16;

// Constants for miller index disc visualization
pub const MILLER_INDEX_DISC_DISTANCE: f64 = 5.0; // Distance from center to place discs
pub const MILLER_INDEX_DISC_RADIUS: f64 = 0.5;   // Radius of each disc
pub const MILLER_INDEX_DISC_THICKNESS: f64 = 0.06; // Thickness of each disc
pub const MILLER_INDEX_DISC_DIVISIONS: u32 = 16;  // Number of divisions for disc cylinder

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
    pub shift: i32,
    pub is_dragging: bool,
    pub possible_miller_indices: HashSet<IVec3>,
}

impl Tessellatable for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        // Calculate center position in world space
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

        // center sphere
        tessellator::tessellate_sphere(
            output_mesh,
            &center_pos,
            CENTER_SPHERE_RADIUS,
            CENTER_SPHERE_HORIZONTAL_DIVISIONS, // number sections when dividing by horizontal lines
            CENTER_SPHERE_VERTICAL_DIVISIONS, // number of sections when dividing by vertical lines
            &Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0));

        // Tessellate shift drag handle along the normal direction
        let plane_normal = self.miller_index.as_dvec3().normalize();
        
        // Use half_space_utils to calculate the shift vector
        let shift_vector = half_space_utils::calculate_shift_vector(&self.miller_index, self.shift);
        
        // Scale shift vector to world space
        let world_shift_vector = shift_vector * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        
        // Calculate the shifted center position (center of the plane)
        let shifted_center = center_pos + world_shift_vector;
        
        // Use the defined constants for handle dimensions
        
        // Calculate the final handle position with the additional offset
        let handle_position = shifted_center + plane_normal * SHIFT_HANDLE_ACCESSIBILITY_OFFSET;
        
        // Define materials
        let axis_material = Material::new(&Vec3::new(0.7, 0.7, 0.7), 1.0, 0.0); // Neutral gray
        let handle_material = Material::new(&Vec3::new(0.2, 0.6, 0.9), 0.5, 0.0); // Blue for handle
        
        // Tessellate the axis cylinder (thin connection from center to handle)
        tessellator::tessellate_cylinder(
            output_mesh,
            &handle_position,
            &center_pos,
            SHIFT_HANDLE_AXIS_RADIUS,
            SHIFT_HANDLE_DIVISIONS,
            &axis_material,
            false, // No caps needed
            None,
            None
        );
        
        // Tessellate the handle cylinder (thicker, draggable part)
        // Place handle centered at the offset position with length along normal direction
        let handle_start = handle_position - plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
        let handle_end = handle_position + plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
        
        tessellator::tessellate_cylinder(
            output_mesh,
            &handle_end,
            &handle_start,
            SHIFT_HANDLE_CYLINDER_RADIUS,
            SHIFT_HANDLE_DIVISIONS,
            &handle_material,
            true, // Include caps for the handle
            None,
            None
        );

        if self.is_dragging {
            let plane_normal = self.miller_index.as_dvec3().normalize();
            let plane_rotator = DQuat::from_rotation_arc(DVec3::Y, plane_normal);

            let roughness: f32 = 1.0;
            let metallic: f32 = 0.0;
            let outside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
            let inside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
            let side_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);      

            let thickness = 0.05;

            // A grid representing the plane
            tessellator::tessellate_grid(
                output_mesh,
                &(center_pos),
                &plane_rotator,
                thickness,
                40.0,
                40.0,
                0.05,
                1.0,
                &outside_material,
                &inside_material,
                &side_material);
                
            // Tessellate miller index discs if we're dragging the central sphere (handle index 2)
            self.tessellate_miller_indices_discs(output_mesh, &center_pos);
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
        // Calculate center position in world space
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        
        // Test central sphere
        if let Some(_t) = sphere_hit_test(
            &center_pos,
            CENTER_SPHERE_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(0); // Central sphere hit
        }
        
        // For the shift handle, we need to calculate its position
        let plane_normal = self.miller_index.as_dvec3().normalize();
        
        // Calculate shifted center using the utility function
        let shift_vector = half_space_utils::calculate_shift_vector(&self.miller_index, 1);
        let world_shift_vector = shift_vector * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        let shifted_center = center_pos + world_shift_vector;
        
        // Calculate handle position with accessibility offset
        let handle_position = shifted_center + plane_normal * SHIFT_HANDLE_ACCESSIBILITY_OFFSET;
        
        // Calculate handle cylinder start and end points
        let handle_start = handle_position - plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
        let handle_end = handle_position + plane_normal * (SHIFT_HANDLE_CYLINDER_LENGTH / 2.0);
        
        // Test shift handle cylinder
        if let Some(_t) = cylinder_hit_test(
            &handle_end,
            &handle_start,
            SHIFT_HANDLE_CYLINDER_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(1); // Shift handle hit
        }

        None // No handle was hit
    }

    fn start_drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
        self.is_dragging = true;
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        // Calculate center position in world space
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        
        if handle_index == 0 {
            // Set is_dragging to true so miller index discs will be tessellated
            self.is_dragging = true;
            
            // Check if any miller index disc is hit
            if let Some(new_miller_index) = self.hit_test_miller_indices_discs(&center_pos, ray_origin, ray_direction) {
                // Set the miller index to the hit disc's miller index
                self.miller_index = new_miller_index;
            }
        }
    }

    fn end_drag(&mut self) {
        // Set is_dragging to false to stop displaying the grid and miller index discs
        self.is_dragging = false;
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
        }
    }
}

impl HalfSpaceGadget {

    pub fn new(max_miller_index: i32, miller_index: &IVec3, center: IVec3, shift: i32) -> Self {
        let normalized_dir = miller_index.as_dvec3().normalize();
        
        let mut ret = Self {
            max_miller_index,
            miller_index: *miller_index,
            center,
            shift,
            is_dragging: false,
            possible_miller_indices: HashSet::new()
        };
        
        // Generate all possible miller indices
        ret.generate_possible_miller_indices();

        return ret;
    }

    fn simplify_miller_index(&self, miller_index: IVec3) -> IVec3 {
        // Get absolute values for checking divisibility
        let abs_x = miller_index.x.abs();
        let abs_y = miller_index.y.abs();
        let abs_z = miller_index.z.abs();

        // Set max_divisor to the maximum of the absolute values of the components
        // This is an optimization as we don't need to check divisors larger than the largest component
        let max_divisor = abs_x.max(abs_y).max(abs_z);
        for divisor in (2..=max_divisor).rev() {
            // Check if all components are divisible by the divisor
            if abs_x % divisor == 0 && abs_y % divisor == 0 && abs_z % divisor == 0 {
                return IVec3::new(
                    miller_index.x / divisor,
                    miller_index.y / divisor,
                    miller_index.z / divisor,
                );
            }
        }

        // If no common divisor found, return the original miller index
        miller_index
    }
    
    /// Tessellates discs representing each possible miller index
    /// These discs are positioned at a fixed distance from the center in the direction of each miller index
    /// The current miller index disc is highlighted with a yellowish-orange color
    fn tessellate_miller_indices_discs(&self, output_mesh: &mut Mesh, center_pos: &DVec3) {

        //let _timer = Timer::new("tessellate_miller_indices_discs");

        // Material for regular discs - blue color
        let disc_material = Material::new(&Vec3::new(0.0, 0.3, 0.9), 0.3, 0.0);
        
        // Material for the current miller index disc - yellowish orange color
        let current_disc_material = Material::new(&Vec3::new(1.0, 0.6, 0.0), 0.3, 0.0);
        
        // Create a red material for the inside/bottom face of regular discs
        let red_material = Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0);

        // Get the simplified version of the current miller index for comparison
        let simplified_current_miller = self.simplify_miller_index(self.miller_index);
        
        // Iterate through all possible miller indices
        for miller_index in &self.possible_miller_indices {
            // Get the normalized direction for this miller index
            let direction = miller_index.as_dvec3().normalize();
            
            // Calculate the position for the disc
            let disc_center = *center_pos + direction * MILLER_INDEX_DISC_DISTANCE;
            
            // Calculate start and end points for the disc (thin cylinder)
            let disc_start = disc_center - direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            let disc_end = disc_center + direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            
            // Get the dynamic disc radius based on the max miller index
            let disc_radius = self.get_miller_index_disc_radius();
            
            // Check if this is the current miller index (compare simplified forms)
            let is_current = *miller_index == simplified_current_miller;
            
            // Choose material based on whether this is the current miller index
            let material = if is_current {
                &current_disc_material
            } else {
                &disc_material
            };
            
            // Tessellate the disc
            tessellator::tessellate_cylinder(
                output_mesh,
                &disc_start,
                &disc_end,
                disc_radius,
                MILLER_INDEX_DISC_DIVISIONS,
                material,
                true, // Cap the ends
                // If current disc, use the same orange material for top face
                // Otherwise use red material for inside/bottom face
                if is_current { Some(material) } else { Some(&red_material) },
                None,
            );
        }
    }
    
    // Calculate the appropriate disc radius based on the max miller index
    fn get_miller_index_disc_radius(&self) -> f64 {
        let divisor = f64::max(self.max_miller_index as f64 - 1.0, 1.0);
        MILLER_INDEX_DISC_RADIUS / divisor
    }
    
    /// Tests if any miller index disc is hit by the given ray
    /// Returns the miller index of the hit disc (closest to ray origin), or None if no disc was hit
    fn hit_test_miller_indices_discs(&self, center_pos: &DVec3, ray_origin: DVec3, ray_direction: DVec3) -> Option<IVec3> {
        //let _timer = Timer::new("hit_test_miller_indices_discs");

        let mut closest_hit: Option<(f64, IVec3)> = None;
        
        // Get the disc radius based on max miller index
        let disc_radius = self.get_miller_index_disc_radius();
        
        // Iterate through all possible miller indices
        for miller_index in &self.possible_miller_indices {
            // Get the normalized direction for this miller index
            let direction = miller_index.as_dvec3().normalize();
            
            // Calculate the position for the disc
            let disc_center = *center_pos + direction * MILLER_INDEX_DISC_DISTANCE;
            
            // Calculate start and end points for the disc (thin cylinder)
            let disc_start = disc_center - direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            let disc_end = disc_center + direction * (MILLER_INDEX_DISC_THICKNESS * 0.5);
            
            // Test if the ray hits this disc
            if let Some(t) = cylinder_hit_test(
                &disc_end,
                &disc_start,
                disc_radius,
                &ray_origin,
                &ray_direction
            ) {
                // If this is the closest hit so far, record it
                match closest_hit {
                    None => closest_hit = Some((t, *miller_index)),
                    Some((closest_t, _)) if t < closest_t => closest_hit = Some((t, *miller_index)),
                    _ => {}
                }
            }
        }
        
        // Return just the miller index of the closest hit disc, if any
        closest_hit.map(|(_, miller_index)| miller_index)
    }

    /// Generates all possible miller indices within the max_miller_index range
    /// and stores them in the possible_miller_indices HashSet after reducing to simplest form
    fn generate_possible_miller_indices(&mut self) {
        // Clear any existing indices
        self.possible_miller_indices.clear();
        
        // Iterate through all combinations within the max_miller_index range
        for h in -self.max_miller_index..=self.max_miller_index {
            for k in -self.max_miller_index..=self.max_miller_index {
                for l in -self.max_miller_index..=self.max_miller_index {
                    // Skip the origin (0,0,0) as it's not a valid direction
                    if h == 0 && k == 0 && l == 0 {
                        continue;
                    }
                    
                    // Create the miller index and reduce it to simplest form
                    let miller = IVec3::new(h, k, l);
                    let simplified = self.simplify_miller_index(miller);
                    
                    // Add the simplified miller index to the set
                    self.possible_miller_indices.insert(simplified);
                }
            }
        }
    }
}

