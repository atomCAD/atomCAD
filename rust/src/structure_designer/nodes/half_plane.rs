use crate::structure_designer::common_constants;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::transform::Transform2D;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::ivec2_serializer;
use glam::i32::IVec2;
use glam::f64::DVec2;
use glam::f64::DVec3;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::display::gadget::Gadget;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::crystolecule::drawing_plane::DrawingPlane;

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
  
      Some(Box::new(HalfPlaneGadget::new(&self.point1, &self.point2, &half_plane_cache.drawing_plane)))
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

      let drawing_plane = match network_evaluator.evaluate_or_default(
        network_stack, node_id, registry, context, 0,
        DrawingPlane::default(),
        NetworkResult::extract_drawing_plane,
      ) {
        Ok(value) => value,
        Err(error) => return error,
      };

      // Evaluate optional miller_index input pin
      let miller_index_result = network_evaluator.evaluate_arg(
        network_stack, node_id, registry, context, 1
      );

      // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
      // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
      if network_stack.len() == 1 {
        let eval_cache = HalfPlaneEvalCache {
          drawing_plane: drawing_plane.clone(),
        };
        context.selected_node_eval_cache = Some(Box::new(eval_cache));
      }

      // Determine point1 and point2 based on whether miller_index is connected
      let (point1, point2) = match miller_index_result {
        NetworkResult::IVec2(miller_index) => {
          // Miller index is connected - use it to determine the half plane
          
          // Evaluate center input pin (defaults to origin)
          let center = match network_evaluator.evaluate_or_default(
            network_stack, node_id, registry, context, 2,
            IVec2::new(0, 0),
            NetworkResult::extract_ivec2
          ) {
            Ok(value) => value,
            Err(error) => return error,
          };

          // Evaluate shift input pin
          let shift = match network_evaluator.evaluate_or_default(
            network_stack, node_id, registry, context, 3,
            0,
            NetworkResult::extract_int
          ) {
            Ok(value) => value,
            Err(error) => return error,
          };

          // Evaluate subdivision input pin
          let subdivision = match network_evaluator.evaluate_or_default(
            network_stack, node_id, registry, context, 4,
            1,
            NetworkResult::extract_int
          ) {
            Ok(value) => value.max(1), // Ensure minimum value of 1
            Err(error) => return error,
          };

          // Convert miller index to plane properties
          let plane_props = match drawing_plane.unit_cell.ivec2_miller_index_to_plane_props(&miller_index) {
            Ok(props) => props,
            Err(error_msg) => return NetworkResult::Error(error_msg),
          };
          
          // Convert center from lattice to real coordinates using effective unit cell
          let center_pos = drawing_plane.effective_unit_cell.ivec2_lattice_to_real(&center);
          
          // Calculate shift distance as multiples of d-spacing, divided by subdivision
          let shift_distance = (shift as f64 / subdivision as f64) * plane_props.d_spacing;
          
          // Calculate two points on the line perpendicular to the miller index
          // The line passes through the shifted center
          let shifted_center = center_pos + plane_props.normal * shift_distance;
          
          // Create a perpendicular direction (rotate normal by 90 degrees)
          let perpendicular = DVec2::new(-plane_props.normal.y, plane_props.normal.x);
          
          // Generate two points on the line
          let p1 = shifted_center + perpendicular;
          let p2 = shifted_center - perpendicular;
          
          (p1, p2)
        },
        NetworkResult::Error(error) => {
          // Error in miller_index evaluation
          return NetworkResult::Error(error);
        },
        _ => {
          // Miller index not connected - use point1 and point2 from node data
          let p1 = drawing_plane.effective_unit_cell.ivec2_lattice_to_real(&self.point1);
          let p2 = drawing_plane.effective_unit_cell.ivec2_lattice_to_real(&self.point2);
          (p1, p2)
        }
      };
    
      // Calculate direction vector from point1 to point2
      let dir_vector = point2 - point1;
      let normal = DVec2::new(-dir_vector.y, dir_vector.x).normalize();
    
      // Use point1 as the position and calculate the angle for the transform
      return NetworkResult::Geometry2D(
        GeometrySummary2D {
          drawing_plane,
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

    fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        let m_index_connected = connected_input_pins.contains("m_index");
        
        // If miller_index is connected, show unconnected miller params
        // Otherwise show point1/point2
        if m_index_connected {
            // Miller index mode - show unconnected parameters
            let center_connected = connected_input_pins.contains("center");
            let shift_connected = connected_input_pins.contains("shift");
            let subdivision_connected = connected_input_pins.contains("subdivision");
            
            if center_connected && shift_connected && subdivision_connected {
                None // All relevant params connected
            } else {
                let mut parts = Vec::new();
                
                if !center_connected {
                    parts.push(format!("c: (0,0)"));
                }
                
                if !shift_connected {
                    parts.push(format!("s: 0"));
                }
                
                if !subdivision_connected {
                    parts.push(format!("sub: 1"));
                }
                
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join(" "))
                }
            }
        } else {
            // Point mode - show point1 and point2
            Some(format!("({},{}) ({},{})", 
                self.point1.x, self.point1.y, self.point2.x, self.point2.y))
        }
    }
}

#[derive(Debug, Clone)]
pub struct HalfPlaneEvalCache {
  pub drawing_plane: DrawingPlane,
}

#[derive(Clone)]
pub struct HalfPlaneGadget {
    pub point1: IVec2,
    pub point2: IVec2,
    pub is_dragging: bool,
    pub dragged_handle: Option<i32>, // 0 for point1, 1 for point2
    pub drawing_plane: DrawingPlane,
}

impl Tessellatable for HalfPlaneGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh: &mut Mesh = &mut output.mesh;

        let plane_to_world = self.drawing_plane.to_world_transform();
        let plane_normal = (plane_to_world.rotation * DVec3::new(0.0, 0.0, 1.0)).normalize();

        // Map points to their 3D positions on the drawing plane
        let p1_3d = self.drawing_plane.lattice_2d_to_world_3d(&self.point1);
        let p2_3d = self.drawing_plane.lattice_2d_to_world_3d(&self.point2);
        
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
        let half_length = self.drawing_plane.unit_cell.float_lattice_to_real(DEFAULT_GRID_SIZE as f64);

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

        // Draw the handles oriented along the drawing plane normal
        let handle_half_height = common_constants::HANDLE_HEIGHT * 0.5;

        let p1_start = p1_3d - plane_normal * handle_half_height;
        let p1_end = p1_3d + plane_normal * handle_half_height;
        tessellator::tessellate_cylinder(
            output_mesh,
            &p1_end,
            &p1_start,
            common_constants::HANDLE_RADIUS,
            common_constants::HANDLE_DIVISIONS,
            &handle1_material,
            true,
            None,
            None,
        );

        let p2_start = p2_3d - plane_normal * handle_half_height;
        let p2_end = p2_3d + plane_normal * handle_half_height;
        tessellator::tessellate_cylinder(
            output_mesh,
            &p2_end,
            &p2_start,
            common_constants::HANDLE_RADIUS,
            common_constants::HANDLE_DIVISIONS,
            &handle2_material,
            true,
            None,
            None,
        );
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for HalfPlaneGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        let plane_to_world = self.drawing_plane.to_world_transform();
        let plane_normal = (plane_to_world.rotation * DVec3::new(0.0, 0.0, 1.0)).normalize();

        // Map points to their 3D positions on the drawing plane
        let p1_3d = self.drawing_plane.lattice_2d_to_world_3d(&self.point1);
        let p2_3d = self.drawing_plane.lattice_2d_to_world_3d(&self.point2);

        let hit_test_radius = common_constants::HANDLE_RADIUS * common_constants::HANDLE_RADIUS_HIT_TEST_FACTOR;
        let handle_half_height = common_constants::HANDLE_HEIGHT * 0.5;

        // Handle for point1
        let p1_start = p1_3d - plane_normal * handle_half_height;
        let p1_end = p1_3d + plane_normal * handle_half_height;
        if cylinder_hit_test(&p1_end, &p1_start, hit_test_radius, &ray_origin, &ray_direction).is_some() {
            return Some(0); // Handle 0 hit
        }

        // Handle for point2
        let p2_start = p2_3d - plane_normal * handle_half_height;
        let p2_end = p2_3d + plane_normal * handle_half_height;
        if cylinder_hit_test(&p2_end, &p2_start, hit_test_radius, &ray_origin, &ray_direction).is_some() {
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
        
        // Find lattice point where ray intersects the drawing plane
        if let Some(lattice_point) = self.drawing_plane.find_lattice_point_by_ray(&ray_origin, &ray_direction) {
            // Update the appropriate point
            if handle_index == 0 {
                self.point1 = lattice_point;
            } else if handle_index == 1 {
                self.point2 = lattice_point;
            }
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
    pub fn new(point1: &IVec2, point2: &IVec2, drawing_plane: &DrawingPlane) -> Self {
        HalfPlaneGadget {
            point1: *point1,
            point2: *point2,
            is_dragging: false,
            dragged_handle: None,
            drawing_plane: drawing_plane.clone(),
        }
    }
}
















