use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::util::serialization_utils::vec_ivec2_serializer;
use crate::structure_designer::text_format::TextValue;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::GeometrySummary2D;
use crate::util::transform::Transform2D;
use glam::DVec2;
use glam::f64::DVec3;
use crate::structure_designer::common_constants;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::display::gadget::Gadget;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::geo_tree::GeoNode;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::crystolecule::drawing_plane::DrawingPlane;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolygonData {
  #[serde(with = "vec_ivec2_serializer")]
  pub vertices: Vec<IVec2>,
}

impl NodeData for PolygonData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      let eval_cache = structure_designer.get_selected_node_eval_cache()?;
      let polygon_cache = eval_cache.downcast_ref::<PolygonEvalCache>()?;
      Some(Box::new(PolygonGadget::new(&self.vertices, &polygon_cache.drawing_plane)))
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

      // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
      // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
      if network_stack.len() == 1 {
        let eval_cache = PolygonEvalCache {
          drawing_plane: drawing_plane.clone(),
        };
        context.selected_node_eval_cache = Some(Box::new(eval_cache));
      }

      // Convert vertices using effective unit cell
      let real_vertices = self.vertices.iter().map(|v| {
        drawing_plane.effective_unit_cell.ivec2_lattice_to_real(v)
      }).collect();

      return NetworkResult::Geometry2D(
        GeometrySummary2D {
          drawing_plane,
          frame_transform: Transform2D::new(
            DVec2::new(0.0, 0.0),
            0.0,
          ),
          geo_tree_root: GeoNode::polygon(real_vertices),
        }
      );
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        None
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![
            ("vertices".to_string(), TextValue::Array(
                self.vertices.iter().map(|v| TextValue::IVec2(*v)).collect()
            )),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("vertices") {
            let arr = v.as_array().ok_or_else(|| "vertices must be an array".to_string())?;
            let mut new_vertices = Vec::new();
            for (i, item) in arr.iter().enumerate() {
                let vertex = item.as_ivec2()
                    .ok_or_else(|| format!("vertex {} must be an IVec2", i))?;
                new_vertices.push(vertex);
            }
            if new_vertices.len() < 3 {
                return Err("polygon must have at least 3 vertices".to_string());
            }
            self.vertices = new_vertices;
        }
        Ok(())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert("d_plane".to_string(), (false, Some("XY plane".to_string())));
        m
    }
}

pub struct PolygonEvalCache {
    pub drawing_plane: DrawingPlane,
}

#[derive(Clone)]
pub struct PolygonGadget {
    pub vertices: Vec<IVec2>,
    pub is_dragging: bool,
    pub dragged_handle: Option<usize>, // index of the dragged vertex
    pub drawing_plane: DrawingPlane,
}

impl PolygonGadget {
    pub fn new(vertices: &Vec<IVec2>, drawing_plane: &DrawingPlane) -> Self {
        PolygonGadget {
            vertices: vertices.clone(),
            is_dragging: false,
            dragged_handle: None,
            drawing_plane: drawing_plane.clone(),
        }
    }
    
    /// Removes any vertices that are at the same position as an adjacent vertex.
    /// This allows the user to delete a vertex by dragging it onto one of its neighbors.
    /// The polygon must maintain at least 3 vertices.
    fn eliminate_duplicate_vertices(&mut self) {
        // Don't allow reducing below the minimum number of vertices for a polygon (3)
        if self.vertices.len() <= 3 {
            return;
        }
        
        // First, check for duplicates between consecutive vertices
        let mut i = 0;
        while i < self.vertices.len() - 1 {
            if self.vertices[i] == self.vertices[i + 1] {
                // Remove the duplicate
                self.vertices.remove(i + 1);
                // Don't increment i, check the next pair
            } else {
                i += 1;
            }
        }
        
        // Finally, check if the first and last vertices are duplicates (wrap around case)
        let len = self.vertices.len();
        if len > 3 && self.vertices[0] == self.vertices[len - 1] {
            self.vertices.pop(); // Remove the last vertex
        }
    }
    
    /// Finds the nearest lattice point by intersecting a ray with the drawing plane.
    /// Returns None if the ray doesn't intersect the plane in a forward direction.
    fn find_lattice_point_by_ray(&self, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<IVec2> {
        self.drawing_plane.find_lattice_point_by_ray(ray_origin, ray_direction)
    }
}

impl Tessellatable for PolygonGadget {
  fn tessellate(&self, output: &mut TessellationOutput) {
    let output_mesh: &mut Mesh = &mut output.mesh;

    let plane_to_world = self.drawing_plane.to_world_transform();
    let plane_normal = (plane_to_world.rotation * DVec3::new(0.0, 0.0, 1.0)).normalize();

    // Map vertices to their 3D positions on the drawing plane
    let real_3d_vertices: Vec<DVec3> = self.vertices.iter()
        .map(|v| self.drawing_plane.lattice_2d_to_world_3d(v))
        .collect();

    let roughness: f32 = 0.2;
    let metallic: f32 = 0.0;
    let handle_material = Material::new(&common_constants::HANDLE_COLOR, roughness, metallic);
    let selected_handle_material = Material::new(&common_constants::SELECTED_HANDLE_COLOR, roughness, metallic);  
    let line_material = Material::new(&common_constants::LINE_COLOR, roughness, metallic);
    
    for i in 0..real_3d_vertices.len() {
        let selected = self.dragged_handle.is_some() && self.dragged_handle.unwrap() == i;
        let p1_3d = real_3d_vertices[i];
        let p2_3d = real_3d_vertices[(i + 1) % real_3d_vertices.len()];

        // handle for the point
        let handle_half_height = common_constants::HANDLE_HEIGHT * 0.5;
        let handle_start = p1_3d - plane_normal * handle_half_height;
        let handle_end = p1_3d + plane_normal * handle_half_height;
        tessellator::tessellate_cylinder(
            output_mesh,
            &handle_end,
            &handle_start,
            common_constants::HANDLE_RADIUS,
            common_constants::HANDLE_DIVISIONS,
            if selected { &selected_handle_material } else { &handle_material },
            true,
            None,
            None
        );

        // line connecting the points
        tessellator::tessellate_cylinder(
            output_mesh,
            &p2_3d,
            &p1_3d,
            common_constants::LINE_RADIUS,
            common_constants::LINE_DIVISIONS,
            &line_material,
            false,
            None,
            None
        );
    }
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for PolygonGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        let plane_to_world = self.drawing_plane.to_world_transform();
        let plane_normal = (plane_to_world.rotation * DVec3::new(0.0, 0.0, 1.0)).normalize();

        // Map vertices to their 3D positions on the drawing plane
        let real_3d_vertices: Vec<DVec3> = self.vertices.iter()
            .map(|v| self.drawing_plane.lattice_2d_to_world_3d(v))
            .collect();

        // First, check hits with vertex handles
        for i in 0..real_3d_vertices.len() {
            let p1_3d = real_3d_vertices[i];
            
            // Handle for the vertex - test cylinder along Z axis
            let handle_half_height = common_constants::HANDLE_HEIGHT * 0.5;
            let handle_start = p1_3d - plane_normal * handle_half_height;
            let handle_end = p1_3d + plane_normal * handle_half_height;
            
            if cylinder_hit_test(&handle_end, &handle_start, common_constants::HANDLE_RADIUS * common_constants::HANDLE_RADIUS_HIT_TEST_FACTOR, &ray_origin, &ray_direction).is_some() {
                return Some(i as i32); // Return the vertex index if hit
            }
        }
        
        // Next, check hits with line segments
        for i in 0..real_3d_vertices.len() {
            let p1_3d = real_3d_vertices[i];
            let p2_3d = real_3d_vertices[(i + 1) % real_3d_vertices.len()];
            
            if cylinder_hit_test(&p2_3d, &p1_3d, common_constants::LINE_RADIUS * common_constants::LINE_RADIUS_HIT_TEST_FACTOR, &ray_origin, &ray_direction).is_some() {
                // Return the number of vertices plus the line segment index if hit
                return Some(real_3d_vertices.len() as i32 + i as i32);
            }
        }
        
        // No hit
        None
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        let num_vertices = self.vertices.len() as i32;
        
        // Case 1: Vertex handle
        if handle_index >= 0 && handle_index < num_vertices {
            self.is_dragging = true;
            self.dragged_handle = Some(handle_index as usize);
            return;
        }
        
        // Case 2: Line segment handle - add a new vertex
        if handle_index >= num_vertices {
            // Calculate the line segment index (0-based)
            let segment_index = (handle_index - num_vertices) as usize;
            
            // Get the indices of the two vertices that form the line segment
            let start_vertex_index = segment_index;
            
            // Find the lattice point where the ray intersects the XY plane
            if let Some(new_vertex) = self.find_lattice_point_by_ray(&ray_origin, &ray_direction) {
                // Insert the new vertex after the start vertex
                // This works for all cases including the last-to-first segment
                // (where it will insert at the end, which is logically correct)
                let insert_index = start_vertex_index + 1;
                
                self.vertices.insert(insert_index, new_vertex);
                
                // Start dragging the new vertex
                self.is_dragging = true;
                self.dragged_handle = Some(insert_index);
            }
        }
    }

    fn drag(&mut self, _handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        // Skip dragging if not in drag mode
        if !self.is_dragging {
            return;
        }

        // Update the vertex position if we have a valid lattice point
        if let Some(lattice_point) = self.find_lattice_point_by_ray(&ray_origin, &ray_direction) {
            if let Some(vertex_index) = self.dragged_handle {
                self.vertices[vertex_index] = lattice_point;
            }
        }
    }

    fn end_drag(&mut self) {
        self.eliminate_duplicate_vertices();
        self.is_dragging = false;
        self.dragged_handle = None;
    }
}

impl NodeNetworkGadget for PolygonGadget {
    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
    
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(polygon_data) = data.as_any_mut().downcast_mut::<PolygonData>() {
            polygon_data.vertices = self.vertices.clone();
        }
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "polygon".to_string(),
      description: "Outputs a general polygon with integer coordinate vertices. Both convex and concave polygons can be created with this node.
The vertices can be freely dragged.
You can create a new vertex by dragging an edge.
Delete a vertex by dragging it onto one of its neighbour.".to_string(),
      summary: None,
      category: NodeTypeCategory::Geometry2D,
      parameters: vec![
        Parameter {
          name: "d_plane".to_string(),
          data_type: DataType::DrawingPlane,
        },
      ],
      output_type: DataType::Geometry2D,
      public: true,
      node_data_creator: || Box::new(PolygonData {
        vertices: vec![
          IVec2::new(-1, -1),
          IVec2::new(1, -1),
          IVec2::new(0, 1),
        ],
      }),
      node_data_saver: generic_node_data_saver::<PolygonData>,
      node_data_loader: generic_node_data_loader::<PolygonData>,
    }
}












