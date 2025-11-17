use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec2_serializer;
use glam::i32::IVec2;
use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::gadget::Gadget;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HalfPlaneData {
  #[serde(with = "ivec2_serializer")]
  pub point1: IVec2,
  #[serde(with = "ivec2_serializer")]
  pub point2: IVec2,
}

impl NodeData for HalfPlaneData {

    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      let eval_cache = structure_designer.get_selected_node_eval_cache()?;
      let half_plane_cache = eval_cache.downcast_ref::<HalfPlaneEvalCache>()?;
  
      Some(Box::new(HalfPlaneGadget::new(&self.point1, &self.point2, &half_plane_cache.unit_cell)))
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

      let unit_cell = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0, 
        UnitCellStruct::cubic_diamond(), 
        NetworkResult::extract_unit_cell,
        ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
      // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
      if network_stack.len() == 1 {
        let eval_cache = HalfPlaneEvalCache {
          unit_cell: unit_cell.clone(),
        };
        context.selected_node_eval_cache = Some(Box::new(eval_cache));
      }

      let point1 = unit_cell.ivec2_lattice_to_real(&self.point1);
      let point2 = unit_cell.ivec2_lattice_to_real(&self.point2);
    
      // Calculate direction vector from point1 to point2
      let dir_vector = point2 - point1;
      let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
      // Use point1 as the position and calculate the angle for the transform
      return NetworkResult::Geometry2D(
        GeometrySummary2D {
          unit_cell,
          frame_transform: Transform2D::new(
            point1,
            normal.x.atan2(normal.y), // Angle from Y direction to normal in radians
          ),
          geo_tree_root: GeoNode::half_plane(point1, point2),
        });
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        Some(format!("({},{}) ({},{})", 
            self.point1.x, self.point1.y, self.point2.x, self.point2.y))
    }
}

#[derive(Debug, Clone)]
pub struct HalfPlaneEvalCache {
  pub unit_cell: UnitCellStruct,
}

#[derive(Clone)]
pub struct HalfPlaneGadget {
    pub point1: IVec2,
    pub point2: IVec2,
    pub is_dragging: bool,
    pub dragged_handle: Option<i32>, // 0 for point1, 1 for point2
    pub unit_cell: UnitCellStruct,
}

impl Tessellatable for HalfPlaneGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        let point1 = self.unit_cell.ivec2_lattice_to_real(&self.point1);
        let point2 = self.unit_cell.ivec2_lattice_to_real(&self.point2);  
        
        // Convert 2D points to 3D space (on XY plane)
        let p1_3d = DVec3::new(point1.x, point1.y, 0.0);
        let p2_3d = DVec3::new(point2.x, point2.y, 0.0);

        // Calculate inward normal direction for triangle orientation
        let dir_vec_2d = (point2 - point1).normalize();
        // The normal in implicit_eval_half_plane DVec2::new(-dir_vec_2d.y, dir_vec_2d.x) points OUTWARD.
        // For the gadget to point INWARD, we need the opposite normal.
        let inward_normal_2d = DVec2::new(dir_vec_2d.y, -dir_vec_2d.x);
        // Angle for prism rotation. The prism's default pointing vertex (local +Y) is (0,1) in its local XY plane.
        // A rotation by angle A around Z transforms this local +Y to (sin A, cos A) in the global XY plane.
        // We want this rotated direction to align with inward_normal_2d = (nx, ny).
        // So, (sin A, cos A) = (inward_normal_2d.x, inward_normal_2d.y).
        // Thus, A = atan2(sin A, cos A) = atan2(inward_normal_2d.x, inward_normal_2d.y).
        let triangle_rotation_angle = -inward_normal_2d.x.atan2(inward_normal_2d.y);
        
        // Create materials
        let roughness: f32 = 0.2;
        let metallic: f32 = 0.0;
        let handle1_material = if self.dragged_handle == Some(0) {
            Material::new(&common_constants::SELECTED_HANDLE_COLOR, roughness, metallic)
        } else {
            Material::new(&common_constants::HANDLE_COLOR, roughness, metallic)
        };
        
        let handle2_material = if self.dragged_handle == Some(1) {
            Material::new(&common_constants::SELECTED_HANDLE_COLOR, roughness, metallic)
        } else {
            Material::new(&common_constants::HANDLE_COLOR, roughness, metallic)
        };
        
        let line_material = Material::new(&common_constants::LINE_COLOR, roughness, metallic);
        
        // Calculate the extended line across the entire coordinate system
        const DEFAULT_GRID_SIZE: i32 = 200; // Default grid size for visualization
        
        // Calculate the line direction in 3D space
        let line_direction = (p2_3d - p1_3d).normalize();
        
        // Calculate the center point of the line segment
        let line_center = (p1_3d + p2_3d) * 0.5;
        
        // Calculate the desired total line length
        let half_length = self.unit_cell.float_lattice_to_real(DEFAULT_GRID_SIZE as f64);

        // Extend the line symmetrically from the center
        let extended_line_start = line_center - line_direction * half_length;
        let extended_line_end = line_center + line_direction * half_length;
        
        // Draw the extended line
        tessellator::tessellate_cylinder(
            output_mesh,
            &extended_line_start,
            &extended_line_end,
            common_constants::LINE_RADIUS,
            common_constants::LINE_DIVISIONS,
            &line_material,
            true,
            None,
            None
        );
        
        // Draw the handles as triangular prisms oriented along Z axis
        // Handle for point1
        tessellator::tessellate_equilateral_triangle_prism(
            output_mesh,
            DVec2::new(p1_3d.x, p1_3d.y), // Centroid of bottom triangle in XY plane
            common_constants::HANDLE_HEIGHT,
            common_constants::HANDLE_TRIANGLE_SIDE_LENGTH,
            triangle_rotation_angle,
            &handle1_material,
        );
        
        // Handle for point2
        tessellator::tessellate_equilateral_triangle_prism(
            output_mesh,
            DVec2::new(p2_3d.x, p2_3d.y), // Centroid of bottom triangle in XY plane
            common_constants::HANDLE_HEIGHT,
            common_constants::HANDLE_TRIANGLE_SIDE_LENGTH,
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
        let point1 = self.unit_cell.ivec2_lattice_to_real(&self.point1);
        let point2 = self.unit_cell.ivec2_lattice_to_real(&self.point2);  
        
        // Convert 2D points to 3D space (on XY plane)
        let p1_3d = DVec3::new(point1.x, point1.y, 0.0);
        let p2_3d = DVec3::new(point2.x, point2.y, 0.0);
        
        // Effective radius for hit testing (distance from centroid to vertex of the triangle)
        let hit_test_radius = common_constants::HANDLE_TRIANGLE_SIDE_LENGTH / 3.0_f64.sqrt();

        // Handle for point1 - test cylinder along Z axis
        let p1_top = DVec3::new(p1_3d.x, p1_3d.y, common_constants::HANDLE_HEIGHT / 2.0);
        let p1_bottom = DVec3::new(p1_3d.x, p1_3d.y, -common_constants::HANDLE_HEIGHT / 2.0);
        if cylinder_hit_test(&p1_top, &p1_bottom, hit_test_radius, &ray_origin, &ray_direction).is_some() {
            return Some(0); // Handle 0 hit
        }
        
        // Handle for point2 - test cylinder along Z axis
        let p2_top = DVec3::new(p2_3d.x, p2_3d.y, common_constants::HANDLE_HEIGHT / 2.0);
        let p2_bottom = DVec3::new(p2_3d.x, p2_3d.y, -common_constants::HANDLE_HEIGHT / 2.0);
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
        
        // Project the ray onto the XY plane (z = 0)
        let plane_normal = DVec3::new(0.0, 0.0, 1.0);
        let plane_point = DVec3::new(0.0, 0.0, 0.0);
        
        // Find intersection of ray with XY plane
        let t = (plane_point - ray_origin).dot(plane_normal) / ray_direction.dot(plane_normal);
        
        if t <= 0.0 { 
            // Ray doesn't hit the plane in the forward direction
            return;
        }
        
        let intersection_point = ray_origin + ray_direction * t;
        
        // Convert the 3D point to lattice coordinates
        let lattice_pos = self.unit_cell.real_to_ivec3_lattice(&intersection_point);
        
        // Update the appropriate point
        if handle_index == 0 {
            self.point1 = IVec2::new(lattice_pos.x, lattice_pos.y);
        } else if handle_index == 1 {
            self.point2 = IVec2::new(lattice_pos.x, lattice_pos.y);
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
    pub fn new(point1: &IVec2, point2: &IVec2, unit_cell: &UnitCellStruct) -> Self {
        HalfPlaneGadget {
            point1: *point1,
            point2: *point2,
            is_dragging: false,
            dragged_handle: None,
            unit_cell: unit_cell.clone(),
        }
    }
}
