use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
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
use crate::util::hit_test_utils::get_closest_point_on_first_ray;
use crate::util::hit_test_utils::get_point_distance_to_ray;
use crate::structure_designer::common_constants;
use std::collections::HashSet;
use crate::common::gadget::Gadget;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::common::csg_types::CSG;
use csgrs::polygon::Polygon;
use csgrs::vertex::Vertex;
use crate::common::csg_utils::dvec3_to_point3;
use crate::common::csg_utils::dvec3_to_vector3;

pub const MAX_MILLER_INDEX: f64 = 4.0;
pub const GADGET_LENGTH: f64 = 6.0;
pub const AXIS_RADIUS: f64 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_RADIUS: f64 = 0.35;
pub const CENTER_SPHERE_HORIZONTAL_DIVISIONS: u32 = 16;
pub const CENTER_SPHERE_VERTICAL_DIVISIONS: u32 = 32;

pub const DIRECTION_HANDLE_RADIUS: f64 = 0.5;
pub const DIRECTION_HANDLE_DIVISIONS: u32 = 16;
pub const DIRECTION_HANDLE_LENGTH: f64 = 0.6;

pub const SHIFT_HANDLE_RADIUS: f64 = 0.4;
pub const SHIFT_HANDLE_DIVISIONS: u32 = 16;
pub const SHIFT_HANDLE_LENGTH: f64 = 1.2;

#[derive(Debug, Serialize, Deserialize)]
pub struct HalfSpaceData {
  pub max_miller_index: i32,
  #[serde(with = "ivec3_serializer")]
  pub miller_index: IVec3,
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
}

impl NodeData for HalfSpaceData {

    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      return Some(Box::new(HalfSpaceGadget::new(&self.miller_index, self.center)));
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

  let dir = half_space_data.miller_index.as_dvec3().normalize();
  let center_pos = half_space_data.center.as_dvec3();

  let normal = dvec3_to_vector3(dir);
  let rotation = DQuat::from_rotation_arc(DVec3::Y, dir);

  let width = 40.0;
  let height = 40.0;

  let start_x =  - width * 0.5;
  let start_z =  - height * 0.5;
  let end_x =   width * 0.5;
  let end_z =   height * 0.5;

  let v1 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, 0.0, start_z)));
  let v2 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(start_x, 0.0, end_z)));
  let v3 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, 0.0, end_z)));
  let v4 = dvec3_to_point3(rotation.mul_vec3(DVec3::new(end_x, 0.0, start_z)));

  let geometry = if context.explicit_geo_eval_needed {
    CSG::from_polygons(&[
        Polygon::new(
            vec![
                Vertex::new(v1, normal),
                Vertex::new(v2, normal),
                Vertex::new(v3, normal),
                Vertex::new(v4, normal),
            ], None
        ),
    ])
    .translate(center_pos.x, center_pos.y, center_pos.z)
  } else { CSG::new() };
  
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
  let float_miller = half_space_data.miller_index.as_dvec3();
  let miller_magnitude = float_miller.length();
  let center_pos = half_space_data.center.as_dvec3();
  
  // Calculate the signed distance from the point to the plane defined by the normal (miller_index) and center point
  return float_miller.dot(*sample_point - center_pos) / miller_magnitude;
}

#[derive(Clone)]
pub struct HalfSpaceGadget {
    pub miller_index: IVec3,
    pub center: IVec3,
    pub visualized_plane_shift: i32,
    pub dir: DVec3, // normalized
    pub shift_handle_offset: f64,
    pub is_dragging: bool,
}

impl Tessellatable for HalfSpaceGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        // Calculate center position in world space
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        let direction_handle_center = center_pos + self.dir * GADGET_LENGTH;

        // axis of the gadget
        tessellator::tessellate_cylinder(
          output_mesh,
          &(center_pos + self.dir * f64::min(self.shift_handle_offset, 0.0)),
          &(center_pos + self.dir * f64::max(self.shift_handle_offset, GADGET_LENGTH)),
          AXIS_RADIUS,
          AXIS_DIVISIONS,
          &Material::new(&Vec3::new(0.95, 0.93, 0.88), 0.4, 0.8), 
          false);

        // center sphere
        tessellator::tessellate_sphere(
            output_mesh,
            &center_pos,
            CENTER_SPHERE_RADIUS,
            CENTER_SPHERE_HORIZONTAL_DIVISIONS, // number sections when dividing by horizontal lines
            CENTER_SPHERE_VERTICAL_DIVISIONS, // number of sections when dividing by vertical lines
            &Material::new(&Vec3::new(0.95, 0.0, 0.0), 0.3, 0.0));

        
        let shift_handle_center = center_pos + self.dir * self.shift_handle_offset;

        // shift handle
        tessellator::tessellate_cylinder(
            output_mesh,
            &(shift_handle_center - self.dir * 0.5 * SHIFT_HANDLE_LENGTH),
            &(shift_handle_center + self.dir * 0.5 * SHIFT_HANDLE_LENGTH),
            SHIFT_HANDLE_RADIUS,
            SHIFT_HANDLE_DIVISIONS,
            &Material::new(&Vec3::new(1.0, 1.0, 0.0), 0.3, 0.0), 
            true);
      
        // direction handle
        tessellator::tessellate_cylinder(
            output_mesh,
            &(direction_handle_center - self.dir * 0.5 * DIRECTION_HANDLE_LENGTH),
            &(direction_handle_center + self.dir * 0.5 * DIRECTION_HANDLE_LENGTH),
            DIRECTION_HANDLE_RADIUS,
            DIRECTION_HANDLE_DIVISIONS,
            &Material::new(&Vec3::new(0.0, 0.0, 0.95), 0.3, 0.0), 
            true);

        if self.is_dragging {
            let plane_normal = self.miller_index.as_dvec3().normalize();
            let plane_rotator = DQuat::from_rotation_arc(DVec3::Y, plane_normal);

            let roughness: f32 = 1.0;
            let metallic: f32 = 0.0;
            let outside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
            let inside_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);
            let side_material = Material::new(&Vec3::new(0.5, 0.5, 0.5), roughness, metallic);      

            let thickness = 0.05;
            let plane_offset = ((self.visualized_plane_shift as f64) / self.miller_index.as_dvec3().length()) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);

            // A grid representing the plane
            tessellator::tessellate_grid(
                output_mesh,
                &(center_pos + plane_offset * plane_normal),
                &plane_rotator,
                thickness,
                40.0,
                40.0,
                0.05,
                1.0,
                &outside_material,
                &inside_material,
                &side_material);
        }

        self.tessellate_lattice_points(output_mesh);     
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for HalfSpaceGadget {
    // Returns the index of the handle that was hit, or None if no handle was hit
    // handle 0: shift handle
    // handle 1: direction handle
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        // Calculate center position in world space
        let center_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        
        // Test shift handle
        let shift_handle_center = center_pos + self.dir * self.shift_handle_offset;
        let shift_handle_start = shift_handle_center - self.dir * 0.5 * SHIFT_HANDLE_LENGTH;
        let shift_handle_end = shift_handle_center + self.dir * 0.5 * SHIFT_HANDLE_LENGTH;

        if let Some(_t) = cylinder_hit_test(
            &shift_handle_end,
            &shift_handle_start,
            SHIFT_HANDLE_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(0); // Shift handle hit
        }

        let direction_handle_center = center_pos + self.dir * GADGET_LENGTH;
   
        // Test direction handle (cylinder centered at gadget_end_point)
        let direction_handle_start = direction_handle_center - self.dir * 0.5 * DIRECTION_HANDLE_LENGTH;
        let direction_handle_end = direction_handle_center + self.dir * 0.5 * DIRECTION_HANDLE_LENGTH;
        
        if let Some(_t) = cylinder_hit_test(
            &direction_handle_end,
            &direction_handle_start,
            DIRECTION_HANDLE_RADIUS,
            &ray_origin,
            &ray_direction
        ) {
            return Some(1); // Direction handle hit
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
            // Shift handle drag
            let t = get_closest_point_on_first_ray(
                &center_pos,
                &self.dir,
                &ray_origin,
                &ray_direction);
            self.shift_handle_offset = t;
            self.visualized_plane_shift = self.offset_to_quantized_shift(self.shift_handle_offset);
        }
        else if handle_index == 1 {
            // Direction handle drag
            if let Some(t) = sphere_hit_test(
                &center_pos,
                GADGET_LENGTH,
                &ray_origin,
                &ray_direction
            ) {
                let new_end_point = ray_origin + ray_direction * t;
                self.dir = new_end_point.normalize();
                self.miller_index = self.quantize_dir(&self.dir);
                // We're changing direction, so reset the visualized plane shift
                self.visualized_plane_shift = 0;
            }
        }
    }

    fn end_drag(&mut self) {
        self.is_dragging = false;
        self.dir = self.miller_index.as_dvec3().normalize();

        if self.visualized_plane_shift != 0 {
            // Get shift in world space
            let shift_amount = (self.visualized_plane_shift as f64) * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64) / self.miller_index.as_dvec3().length();
            
            // Calculate the shifted point in world coordinates
            let center_world_pos = self.center.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
            let shifted_point = center_world_pos + self.dir * shift_amount;
            
            // Find a lattice point that lies exactly on the shifted plane
            self.center = self.find_lattice_point_on_plane(shifted_point);
            
            // Reset the visualized shift since we've moved the center
            self.visualized_plane_shift = 0;
        }
        
        // Always reset the shift handle offset when dragging ends
        self.shift_handle_offset = 0.0;
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

    pub fn new(miller_index: &IVec3, center: IVec3) -> Self {
        let normalized_dir = miller_index.as_dvec3().normalize();
        
        let ret = Self {
            miller_index: *miller_index,
            center,
            visualized_plane_shift: 0, // No initial shift relative to center
            dir: normalized_dir,
            shift_handle_offset: 0.0,  // No initial offset
            is_dragging: false
        };

        return ret;
    }

    /// Find a lattice point (integer coordinates) that lies exactly on the plane
    /// defined by the given miller_index and a target point in world coordinates.
    /// 
    /// Returns the closest lattice point to the target_point that satisfies the plane equation.
    fn find_lattice_point_on_plane(&self, target_point: DVec3) -> IVec3 {
        // Convert target point to lattice coordinates
        let target_lattice = target_point / (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        
        // Calculate an initial guess by rounding to nearest integer coordinates
        let initial_guess = IVec3::new(
            target_lattice.x.round() as i32,
            target_lattice.y.round() as i32,
            target_lattice.z.round() as i32
        );
        
        // Get the normalized miller index as the plane normal
        let normal = self.miller_index.as_dvec3().normalize();
        
        // Determine search radius based on miller index components
        // For mathematical correctness, we need a radius that accounts for:
        // 1. The magnitude of the miller index
        // 2. Possible large spacings between valid lattice points
        //
        // A conservative approach is to use the sum of the absolute values
        // of the Miller indices, which guarantees finding a solution
        let h = self.miller_index.x.abs();
        let k = self.miller_index.y.abs();
        let l = self.miller_index.z.abs();
        let magnitude_sum = h + k + l;
        
        // Use a very conservative radius that's guaranteed to find a solution
        // if one exists within a reasonable distance
        let max_radius = i32::max(magnitude_sum * 2, 10); // At least radius 10
        
        // Search in increasingly larger cubes around the initial guess
        for radius in 0..=max_radius {
            // Search in a cube with side length 2*radius+1 centered at initial_guess
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    for dz in -radius..=radius {
                        // Skip points we've already checked in smaller cubes
                        if dx.abs() < radius && dy.abs() < radius && dz.abs() < radius {
                            continue;
                        }
                        
                        let candidate = initial_guess + IVec3::new(dx, dy, dz);
                        
                        // Check if this point satisfies the plane equation
                        // For a point to lie on the plane: normal · (point - target_point) = 0
                        // Convert candidate to world space for consistent comparison with target_point
                        let candidate_world = candidate.as_dvec3() * (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
                        let distance_from_plane = normal.dot(candidate_world - target_point);
                        
                        // Allow for a reasonable error due to floating point calculations
                        // Using 1e-4 provides sufficient precision while avoiding false negatives
                        if distance_from_plane.abs() < 1e-4 {
                            return candidate;
                        }
                    }
                }
            }
        }
        
        // If no exact solution found, use the initial guess
        // This should be extremely rare with reasonable miller indices
        initial_guess
    }


    fn tessellate_lattice_points(&self, output_mesh: &mut Mesh) {
        let float_miller = self.miller_index.as_dvec3();
        let miller_magnitude = float_miller.length();
        
        // Calculate center position in world space
        let center_pos = self.center.as_dvec3();
        
        // Calculate total plane shift (center + visualized_plane_shift)
        let plane_shift = self.visualized_plane_shift as f64;

        let regular_cube_size = DVec3::new(0.1, 0.1, 0.1);
        let regular_cube_material = Material::new(&Vec3::new(0.6, 0.6, 0.6), 0.5, 0.0);

        let highlighted_cube_size = DVec3::new(0.2, 0.2, 0.2);
        let highlighted_cube_material = Material::new(&Vec3::new(1.0, 1.0, 0.2), 0.3, 0.0);

        // Iterate over voxel grid
        for x in common_constants::IMPLICIT_VOLUME_MIN.x..common_constants::IMPLICIT_VOLUME_MAX.x {
            for y in common_constants::IMPLICIT_VOLUME_MIN.y..common_constants::IMPLICIT_VOLUME_MAX.y {
                for z in common_constants::IMPLICIT_VOLUME_MIN.z..common_constants::IMPLICIT_VOLUME_MAX.z {
                    let sample_point = DVec3::new(x as f64, y as f64, z as f64);
                    
                    // Calculate signed distance from the plane defined by center and miller index with shift
                    // First get the distance from the center-based plane
                    let center_distance = float_miller.dot(sample_point - center_pos) / miller_magnitude;
                    
                    // Then apply the visualized plane shift
                    let adjusted_distance = (center_distance - plane_shift / miller_magnitude).abs();

                    if adjusted_distance < 0.01 {
                        let lattice_point = sample_point * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
                        let highlighted = adjusted_distance < 0.01;

                        let cube_size = if highlighted {
                            &highlighted_cube_size
                        } else {
                            &regular_cube_size
                        };

                        let cube_material = if highlighted {
                            &highlighted_cube_material
                        } else {
                            &regular_cube_material
                        };

                        tessellator::tessellate_cuboid(
                            output_mesh,
                            &lattice_point,
                            cube_size,
                            &DQuat::IDENTITY,
                            cube_material, cube_material, cube_material);
                    } 
                }
            }
        }
    }

    // Returns a miller index
    fn quantize_dir(&self, dir: &DVec3) -> IVec3 {
        let mut candidate_points: HashSet<IVec3> = HashSet::new();
        let mut t = MAX_MILLER_INDEX * 0.5;
        while t <= MAX_MILLER_INDEX {
            let p = dir * t;

            // Calculate floor and ceiling for each component to get unit cell corners
            let x_floor = p.x.floor() as i32;
            let y_floor = p.y.floor() as i32;
            let z_floor = p.z.floor() as i32;
            let x_ceil = p.x.ceil() as i32;
            let y_ceil = p.y.ceil() as i32;
            let z_ceil = p.z.ceil() as i32;
            
            // Add all 8 corners of the unit cell to candidate_points
            candidate_points.insert(IVec3::new(x_floor, y_floor, z_floor));
            candidate_points.insert(IVec3::new(x_floor, y_floor, z_ceil));
            candidate_points.insert(IVec3::new(x_floor, y_ceil, z_floor));
            candidate_points.insert(IVec3::new(x_floor, y_ceil, z_ceil));
            candidate_points.insert(IVec3::new(x_ceil, y_floor, z_floor));
            candidate_points.insert(IVec3::new(x_ceil, y_floor, z_ceil));
            candidate_points.insert(IVec3::new(x_ceil, y_ceil, z_floor));
            candidate_points.insert(IVec3::new(x_ceil, y_ceil, z_ceil));
            
            t += 0.5;
        }
                
        let mut closest_point = None;
        let mut min_distance = f64::MAX;

        for point in &candidate_points {
            let distance = get_point_distance_to_ray(&DVec3::ZERO, dir, &point.as_dvec3());
            if distance < min_distance {
                min_distance = distance;
                closest_point = Some(*point);
            }
        }

        self.simplify_miller_index(closest_point.unwrap_or(IVec3::new(1, 0, 0)))
    }

    fn simplify_miller_index(&self, miller_index: IVec3) -> IVec3 {
        // Get absolute values for checking divisibility
        let abs_x = miller_index.x.abs();
        let abs_y = miller_index.y.abs();
        let abs_z = miller_index.z.abs();

        // Try divisions from MAX_MILLER_INDEX down to 2
        let max_divisor = MAX_MILLER_INDEX.ceil() as i32;
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

    fn offset_to_quantized_shift(&self, offset: f64) -> i32 {
        let shift = offset * (self.miller_index.as_dvec3().length()) / (common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64);
        return shift.round() as i32;
    }
}
