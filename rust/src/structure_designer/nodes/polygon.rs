use glam::i32::IVec2;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::vec_ivec2_serializer;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary2D;
use crate::util::transform::Transform2D;
use glam::DVec2;
use glam::f64::DVec3;
use crate::structure_designer::common_constants;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::common::gadget::Gadget;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::structure_designer::node_network::Node;
use crate::structure_designer::evaluator::network_evaluator::NodeInvocationCache;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::geo_tree::GeoNode;

#[derive(Debug, Serialize, Deserialize)]
pub struct PolygonData {
  #[serde(with = "vec_ivec2_serializer")]
  pub vertices: Vec<IVec2>,
}

impl NodeData for PolygonData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      Some(Box::new(PolygonGadget::new(&self.vertices)))
    }
}

pub fn eval_polygon<'a>(network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, _registry: &NodeTypeRegistry) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let polygon_data = &node.data.as_any_ref().downcast_ref::<PolygonData>().unwrap();

  return NetworkResult::Geometry2D(
    GeometrySummary2D {
      frame_transform: Transform2D::new(
        DVec2::new(0.0, 0.0),
        0.0,
      ),
      geo_tree_root: GeoNode::Polygon { vertices: polygon_data.vertices.clone() },
    }
  );
}

/// Calculates the minimum distance from a point to a line segment
fn point_to_line_segment_distance(point: &DVec2, line_start: &DVec2, line_end: &DVec2) -> f64 {
    let line_vector = *line_end - *line_start;
    let line_length_squared = line_vector.length_squared();
    
    // Handle degenerate case where line segment is actually a point
    if line_length_squared < 1e-10 {
        return (*point - *line_start).length();
    }
    
    // Calculate projection of point onto line
    let t = f64::max(0.0, f64::min(1.0, (*point - *line_start).dot(line_vector) / line_length_squared));
    
    // Calculate closest point on the line segment
    let closest_point = *line_start + line_vector * t;
    
    // Return distance from point to closest point on line segment
    (*point - closest_point).length()
}

/// Checks if a line segment from point1 to point2 intersects with a ray
/// cast from test_point in the positive X direction
fn line_segment_intersects_ray(test_point: &DVec2, point1: &DVec2, point2: &DVec2) -> bool {
    // Early exclusion: both endpoints are above or below the ray
    if (point1.y > test_point.y && point2.y > test_point.y) || 
       (point1.y < test_point.y && point2.y < test_point.y) {
        return false;
    }
    
    // Early exclusion: both endpoints are to the left of the test point
    if point1.x < test_point.x && point2.x < test_point.x {
        return false;
    }
    
    // Calculate intersection point of line segment with horizontal ray
    if (point1.y - test_point.y).abs() < 1e-10 || (point2.y - test_point.y).abs() < 1e-10 {
        // One endpoint is on the ray - special case
        // Count intersection only if the endpoint is the lower one
        if (point1.y - test_point.y).abs() < 1e-10 {
            return point1.y > point2.y && point1.x >= test_point.x;
        } else {
            return point2.y > point1.y && point2.x >= test_point.x;
        }
    } else {
        // Normal case - check if ray intersects line segment
        let t = (test_point.y - point1.y) / (point2.y - point1.y);
        if t >= 0.0 && t <= 1.0 {
            let x_intersect = point1.x + t * (point2.x - point1.x);
            return x_intersect >= test_point.x;
        }
    }
    
    false
}

/// Check if a point is inside a polygon using ray casting algorithm
fn is_point_inside_polygon(point: &DVec2, vertices: &Vec<DVec2>) -> bool {
    let num_vertices = vertices.len();
    if num_vertices < 3 {
        return false; // Not a proper polygon
    }
    
    // Cast a ray from the point in the positive X direction
    // and count intersections with polygon edges
    let mut intersections = 0;
    
    for i in 0..num_vertices {
        let j = (i + 1) % num_vertices;
        if line_segment_intersects_ray(point, &vertices[i], &vertices[j]) {
            intersections += 1;
        }
    }
    
    // If number of intersections is odd, point is inside the polygon
    intersections % 2 == 1
}

pub fn implicit_eval_polygon<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _invocation_cache: &NodeInvocationCache,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec2) -> f64 {
    // Get polygon data from node
    let polygon_data = &node.data.as_any_ref().downcast_ref::<PolygonData>().unwrap();
    
    // Convert vertices to double precision for calculations
    let vertices_dvec2: Vec<DVec2> = polygon_data.vertices.iter()
        .map(|v| v.as_dvec2())
        .collect();
    
    // Handle degenerate case - not enough vertices for a polygon
    if vertices_dvec2.len() < 3 {
        return f64::MAX;
    }
    
    // Calculate minimum distance to any line segment (absolute value of SDF)
    let mut min_distance = f64::MAX;
    for i in 0..vertices_dvec2.len() {
        let j = (i + 1) % vertices_dvec2.len();
        let distance = point_to_line_segment_distance(
            sample_point, 
            &vertices_dvec2[i], 
            &vertices_dvec2[j]
        );
        min_distance = min_distance.min(distance);
    }
    
    // Determine sign using ray casting
    let is_inside = is_point_inside_polygon(sample_point, &vertices_dvec2);
    
    // Apply sign: negative inside, positive outside
    if is_inside {
        -min_distance
    } else {
        min_distance
    }
}

#[derive(Clone)]
pub struct PolygonGadget {
    pub vertices: Vec<IVec2>,
    pub is_dragging: bool,
    pub dragged_handle: Option<usize>, // index of the dragged vertex
}

impl PolygonGadget {
    pub fn new(vertices: &Vec<IVec2>) -> Self {
        PolygonGadget {
            vertices: vertices.clone(),
            is_dragging: false,
            dragged_handle: None,
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
    
    /// Finds the nearest lattice point by intersecting a ray with the XZ plane
    /// Returns None if the ray doesn't intersect the plane in a forward direction
    fn find_lattice_point_by_ray(&self, ray_origin: &DVec3, ray_direction: &DVec3) -> Option<IVec2> {
        // Project the ray onto the XZ plane (y = 0)
        let plane_normal = DVec3::new(0.0, 1.0, 0.0);
        let plane_point = DVec3::new(0.0, 0.0, 0.0);
        
        // Find intersection of ray with XZ plane
        let denominator = ray_direction.dot(plane_normal);
        
        // Avoid division by zero (ray parallel to plane)
        if denominator.abs() < 1e-6 { 
            return None;
        }
        
        let t = (plane_point - ray_origin).dot(plane_normal) / denominator;
        
        if t <= 0.0 { 
            // Ray doesn't hit the plane in the forward direction
            return None;
        }
        
        let intersection_point = *ray_origin + *ray_direction * t;
        
        // Convert the 3D point to lattice coordinates by dividing by cell size
        let cell_size = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64;
        let x_lattice = (intersection_point.x / cell_size).round() as i32;
        let z_lattice = (intersection_point.z / cell_size).round() as i32;
        
        Some(IVec2::new(x_lattice, z_lattice))
    }
}

impl Tessellatable for PolygonGadget {
  fn tessellate(&self, output_mesh: &mut Mesh) {
    // Convert to 3D coordinates and scale by unit cell size
    let cell_size = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64;

    // Convert 2D points to 3D space (on XZ plane)
    let vertices_3d: Vec<DVec3> = self.vertices.iter().map(|v| {
      DVec3::new(
        v.x as f64 * cell_size, 
        0.0, 
        v.y as f64 * cell_size
      )
    }).collect();

    // Create materials
    let roughness: f32 = 0.2;
    let metallic: f32 = 0.8;
    let handle_material = Material::new(&common_constants::HANDLE_COLOR, roughness, metallic);
    let selected_handle_material = Material::new(&common_constants::SELECTED_HANDLE_COLOR, roughness, metallic);  
    let line_material = Material::new(&common_constants::LINE_COLOR, roughness, metallic);
    
    for i in 0..vertices_3d.len() {
        let selected = self.dragged_handle.is_some() && self.dragged_handle.unwrap() == i;
        let p1_3d = vertices_3d[i];
        let p2_3d = vertices_3d[(i + 1) % vertices_3d.len()];

        // handle for the point
        let handle_half_height = common_constants::HANDLE_HEIGHT * 0.5;
        let handle_start = DVec3::new(p1_3d.x, -handle_half_height, p1_3d.z);
        let handle_end = DVec3::new(p1_3d.x, handle_half_height, p1_3d.z);
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
        // Convert to 3D coordinates and scale by unit cell size
        let cell_size = common_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM as f64;

        // Convert 2D points to 3D space (on XZ plane)
        let vertices_3d: Vec<DVec3> = self.vertices.iter().map(|v| {
            DVec3::new(
                v.x as f64 * cell_size, 
                0.0, 
                v.y as f64 * cell_size
            )
        }).collect();
        
        // First, check hits with vertex handles
        for i in 0..vertices_3d.len() {
            let p1_3d = vertices_3d[i];
            
            // Handle for the vertex - test cylinder along Y axis
            let handle_half_height = common_constants::HANDLE_HEIGHT * 0.5;
            let handle_start = DVec3::new(p1_3d.x, -handle_half_height, p1_3d.z);
            let handle_end = DVec3::new(p1_3d.x, handle_half_height, p1_3d.z);
            
            if cylinder_hit_test(&handle_end, &handle_start, common_constants::HANDLE_RADIUS, &ray_origin, &ray_direction).is_some() {
                return Some(i as i32); // Return the vertex index if hit
            }
        }
        
        // Next, check hits with line segments
        for i in 0..vertices_3d.len() {
            let p1_3d = vertices_3d[i];
            let p2_3d = vertices_3d[(i + 1) % vertices_3d.len()];
            
            if cylinder_hit_test(&p2_3d, &p1_3d, common_constants::LINE_RADIUS, &ray_origin, &ray_direction).is_some() {
                // Return the number of vertices plus the line segment index if hit
                return Some(vertices_3d.len() as i32 + i as i32);
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
            let end_vertex_index = (segment_index + 1) % self.vertices.len();
            
            // Find the lattice point where the ray intersects the XZ plane
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

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
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