use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use nalgebra::Point3;
use nalgebra::Vector3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use glam::i32::IVec2;
use glam::f64::DVec2;
use glam::f64::DVec3;
use glam::f32::Vec3;
use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::gadget::Gadget;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::common::csg_types::CSG;
use csgrs::polygon::Polygon;
use csgrs::vertex::Vertex;

#[derive(Debug, Serialize, Deserialize)]
pub struct HalfPlaneData {
  #[serde(with = "ivec2_serializer")]
  pub point1: IVec2,
  #[serde(with = "ivec2_serializer")]
  pub point2: IVec2,
}

impl NodeData for HalfPlaneData {

    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      Some(Box::new(HalfPlaneGadget::new(&self.point1, &self.point2)))
    }
  
}

pub fn eval_half_plane<'a>(
    network_stack: &Vec<NetworkStackElement<'a>>,
    node_id: u64, _registry: &NodeTypeRegistry,
    context: &mut NetworkEvaluationContext,
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let half_plane_data = &node.data.as_any_ref().downcast_ref::<HalfPlaneData>().unwrap();

  // Convert point1 to double precision for calculations
  let point1 = half_plane_data.point1.as_dvec2() * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

  // Calculate direction vector from point1 to point2
  let dir_vector = half_plane_data.point2.as_dvec2() * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM - point1;
  let dir = dir_vector.normalize();
  let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
  
  let center_pos = point1 + dir_vector * 0.5;

  let width = 100.0 * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
  let height = 100.0 * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;  

  let geometry = if context.explicit_geo_eval_needed {
    let tr = center_pos - dir * width * 0.5 - normal * height;
    CSG::square(width, height, None)
    .rotate(0.0, 0.0, dir.y.atan2(dir.x).to_degrees())
    .translate(tr.x, tr.y, 0.0)
  } else { CSG::new() };

  // Use point1 as the position and calculate the angle for the transform
  return NetworkResult::Geometry2D(
    GeometrySummary2D {
      frame_transform: Transform2D::new(
        point1,
        normal.x.atan2(normal.y), // Angle from Y direction to normal in radians
      ),
      csg: geometry,
    });
}

pub fn implicit_eval_half_plane<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {
  let half_plane_data = &node.data.as_any_ref().downcast_ref::<HalfPlaneData>().unwrap();
  
  // Convert points to double precision for calculations
  let point1 = half_plane_data.point1.as_dvec2();
  let point2 = half_plane_data.point2.as_dvec2();
  
  // Calculate line direction and normal
  let dir_vector = point2 - point1;
  let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
  
  // Calculate signed distance from sample_point to the line
  // Formula: distance = normal·(sample_point - point1)
  return normal.dot(*sample_point - point1);
}

// Constants for the gadget
pub const HANDLE_TRIANGLE_SIDE_LENGTH: f64 = 1.0;
// pub const HANDLE_RADIUS: f64 = 0.4; // Replaced by triangle side length logic
pub const HANDLE_DIVISIONS: u32 = 16; // Still used for line tessellation, could be removed if not used elsewhere
pub const HANDLE_HEIGHT: f64 = 0.6;
pub const LINE_RADIUS: f64 = 0.15;
pub const SELECTED_HANDLE_COLOR: Vec3 = Vec3::new(1.0, 0.6, 0.0); // Orange for selected handle
pub const HANDLE_COLOR: Vec3 = Vec3::new(0.1, 1.0, 0.3); // Light green for handles
pub const LINE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.6);

#[derive(Clone)]
pub struct HalfPlaneGadget {
    pub point1: IVec2,
    pub point2: IVec2,
    pub is_dragging: bool,
    pub dragged_handle: Option<i32>, // 0 for point1, 1 for point2
}

impl Tessellatable for HalfPlaneGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        // Convert to 3D coordinates and scale by unit cell size
        let cell_size = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64;
        
        // Convert 2D points to 3D space (on XZ plane)
        let p1_3d = DVec3::new(
            self.point1.x as f64 * cell_size, 
            0.0, 
            self.point1.y as f64 * cell_size
        );
        
        let p2_3d = DVec3::new(
            self.point2.x as f64 * cell_size, 
            0.0, 
            self.point2.y as f64 * cell_size
        );

        // Calculate inward normal direction for triangle orientation
        let p1_dvec2 = self.point1.as_dvec2();
        let p2_dvec2 = self.point2.as_dvec2();
        let dir_vec_2d = p2_dvec2 - p1_dvec2;
        // The normal in implicit_eval_half_plane DVec2::new(-dir_vec_2d.y, dir_vec_2d.x) points OUTWARD.
        // For the gadget to point INWARD, we need the opposite normal.
        let inward_normal_2d = DVec2::new(dir_vec_2d.y, -dir_vec_2d.x);
        // Angle for prism rotation. The prism's default pointing vertex (local +Z) is (0,1) in its local XZ plane.
        // A rotation by angle A around Y transforms this local +Z to (sin A, cos A) in the global XZ plane.
        // We want this rotated direction to align with inward_normal_2d = (nx, nz).
        // So, (sin A, cos A) = (inward_normal_2d.x, inward_normal_2d.y).
        // Thus, A = atan2(sin A, cos A) = atan2(inward_normal_2d.x, inward_normal_2d.y).
        let triangle_rotation_angle = inward_normal_2d.x.atan2(inward_normal_2d.y);
        
        // Create materials
        let roughness: f32 = 0.2;
        let metallic: f32 = 0.8;
        let handle1_material = if self.dragged_handle == Some(0) {
            Material::new(&SELECTED_HANDLE_COLOR, roughness, metallic)
        } else {
            Material::new(&HANDLE_COLOR, roughness, metallic)
        };
        
        let handle2_material = if self.dragged_handle == Some(1) {
            Material::new(&SELECTED_HANDLE_COLOR, roughness, metallic)
        } else {
            Material::new(&HANDLE_COLOR, roughness, metallic)
        };
        
        let line_material = Material::new(&LINE_COLOR, roughness, metallic);
        
        // Calculate the extended line across the entire coordinate system
        use crate::renderer::tessellator::coordinate_system_tessellator::CS_SIZE;
        
        // Get the direction vector (normalized)
        let dir_xz = DVec2::new(p2_3d.x - p1_3d.x, p2_3d.z - p1_3d.z).normalize();
        
        // If line is nearly vertical or horizontal, handle separately
        let extended_line_start: DVec3;
        let extended_line_end: DVec3;
        
        let cs_size = CS_SIZE as f64 * common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;

        if dir_xz.x.abs() < 1e-6 {  // Line is parallel to Z axis
            extended_line_start = DVec3::new(p1_3d.x, 0.0, -cs_size);
            extended_line_end = DVec3::new(p1_3d.x, 0.0, cs_size);
        } else if dir_xz.y.abs() < 1e-6 {  // Line is parallel to X axis
            extended_line_start = DVec3::new(-cs_size, 0.0, p1_3d.z);
            extended_line_end = DVec3::new(cs_size, 0.0, p1_3d.z);
        } else {
            // Calculate t values where line crosses grid boundary
            // Parametrize line as p1 + t*dir
            let t_x_min = (-cs_size - p1_3d.x) / dir_xz.x;
            let t_x_max = (cs_size - p1_3d.x) / dir_xz.x;
            let t_z_min = (-cs_size - p1_3d.z) / dir_xz.y;
            let t_z_max = (cs_size - p1_3d.z) / dir_xz.y;
            
            // Find min and max t values within grid
            let t_min = t_x_min.min(t_x_max).max(t_z_min.min(t_z_max));
            let t_max = t_x_min.max(t_x_max).min(t_z_min.max(t_z_max));
            
            // Calculate start and end points
            extended_line_start = DVec3::new(
                p1_3d.x + t_min * dir_xz.x,
                0.0,
                p1_3d.z + t_min * dir_xz.y
            );
            
            extended_line_end = DVec3::new(
                p1_3d.x + t_max * dir_xz.x,
                0.0,
                p1_3d.z + t_max * dir_xz.y
            );
        }
        
        // Draw the extended line
        tessellator::tessellate_cylinder(
            output_mesh,
            &extended_line_start,
            &extended_line_end,
            LINE_RADIUS,
            HANDLE_DIVISIONS,
            &line_material,
            true
        );
        
        // Draw the handles as triangular prisms oriented along Y axis
        // Handle for point1
        tessellator::tessellate_equilateral_triangle_prism(
            output_mesh,
            DVec2::new(p1_3d.x, p1_3d.z), // Centroid of bottom triangle in XZ plane
            HANDLE_HEIGHT,
            HANDLE_TRIANGLE_SIDE_LENGTH,
            triangle_rotation_angle,
            &handle1_material,
        );
        
        // Handle for point2
        tessellator::tessellate_equilateral_triangle_prism(
            output_mesh,
            DVec2::new(p2_3d.x, p2_3d.z), // Centroid of bottom triangle in XZ plane
            HANDLE_HEIGHT,
            HANDLE_TRIANGLE_SIDE_LENGTH,
            triangle_rotation_angle,
            &handle2_material,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for HalfPlaneGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        let cell_size = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64;
        
        // Convert 2D points to 3D space (on XZ plane)
        let p1_3d = DVec3::new(
            self.point1.x as f64 * cell_size, 
            0.0, 
            self.point1.y as f64 * cell_size
        );
        
        let p2_3d = DVec3::new(
            self.point2.x as f64 * cell_size, 
            0.0, 
            self.point2.y as f64 * cell_size
        );
        
        // Effective radius for hit testing (distance from centroid to vertex of the triangle)
        let hit_test_radius = HANDLE_TRIANGLE_SIDE_LENGTH / 3.0_f64.sqrt();

        // Handle for point1 - test cylinder along Y axis
        let p1_top = DVec3::new(p1_3d.x, HANDLE_HEIGHT / 2.0, p1_3d.z);
        let p1_bottom = DVec3::new(p1_3d.x, -HANDLE_HEIGHT / 2.0, p1_3d.z);
        if cylinder_hit_test(&p1_top, &p1_bottom, hit_test_radius, &ray_origin, &ray_direction).is_some() {
            return Some(0); // Handle 0 hit
        }
        
        // Handle for point2 - test cylinder along Y axis
        let p2_top = DVec3::new(p2_3d.x, HANDLE_HEIGHT / 2.0, p2_3d.z);
        let p2_bottom = DVec3::new(p2_3d.x, -HANDLE_HEIGHT / 2.0, p2_3d.z);
        if cylinder_hit_test(&p2_top, &p2_bottom, hit_test_radius, &ray_origin, &ray_direction).is_some() {
            return Some(1); // Handle 1 hit
        }
        
        // No hit
        None
    }

    fn start_drag(&mut self, handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
        self.is_dragging = true;
        self.dragged_handle = Some(handle_index);
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        if !self.is_dragging {
            return;
        }
        
        // Project the ray onto the XZ plane (y = 0)
        let plane_normal = DVec3::new(0.0, 1.0, 0.0);
        let plane_point = DVec3::new(0.0, 0.0, 0.0);
        
        // Find intersection of ray with XZ plane
        let t = (plane_point - ray_origin).dot(plane_normal) / ray_direction.dot(plane_normal);
        
        if t <= 0.0 { 
            // Ray doesn't hit the plane in the forward direction
            return;
        }
        
        let intersection_point = ray_origin + ray_direction * t;
        
        // Convert the 3D point to lattice coordinates by dividing by cell size
        let cell_size = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64;
        let x_lattice = (intersection_point.x / cell_size).round() as i32;
        let z_lattice = (intersection_point.z / cell_size).round() as i32;
        
        // Update the appropriate point
        if handle_index == 0 {
            self.point1 = IVec2::new(x_lattice, z_lattice);
        } else if handle_index == 1 {
            self.point2 = IVec2::new(x_lattice, z_lattice);
        }
    }

    fn end_drag(&mut self) {
        self.is_dragging = false;
        self.dragged_handle = None;
    }
}

impl NodeNetworkGadget for HalfPlaneGadget {
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
    
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(half_plane_data) = data.as_any_mut().downcast_mut::<HalfPlaneData>() {
            half_plane_data.point1 = self.point1;
            half_plane_data.point2 = self.point2;
        }
    }
}

impl HalfPlaneGadget {
    pub fn new(point1: &IVec2, point2: &IVec2) -> Self {
        HalfPlaneGadget {
            point1: *point1,
            point2: *point2,
            is_dragging: false,
            dragged_handle: None,
        }
    }
}
