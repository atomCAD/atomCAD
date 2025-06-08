use std::collections::HashMap;
use glam::{i32::IVec3, Vec3, DVec3};
use crate::util::box_subdivision::subdivide_box;
use crate::structure_designer::evaluator::implicit_evaluator::NodeEvaluator;
use crate::structure_designer::common_constants;
use crate::renderer::mesh::{Mesh, Vertex, Material};
use crate::structure_designer::evaluator::quad_mesh::QuadMesh;

/*
 * Terminology for Dual Contouring:
 * - Cell/Cube: A volumetric unit in our grid, identified by its minimum corner vertex
 * - Vertex/Corner: A corner of a cell (8 per cell) where we evaluate the SDF
 * - Edge: Connection between two adjacent vertices
 * - In dual contouring, we place mesh vertices INSIDE cells, not at grid vertices
 */
const DC_3D_SAMPLES_PER_UNIT: i32 = 4;

#[derive(Clone, Copy)]
pub struct EdgeIntersection {
  pub position: DVec3,
  pub normal: DVec3,
}

pub struct DCCell {
  pub vertex_index: i32, // -1 if no vertex for this cell.
  pub edge_intersections: Vec<EdgeIntersection>, // Intersections that influence this cell's vertex
}

// Edge directions between cell corners/vertices (x, y, z)
// These represent the 3 edges originating from the minimum corner of each cell
const EDGE_DIRECTIONS: [(i32, i32, i32); 3] = [
    (1, 0, 0), // Edge along +X direction from vertex at (i,j,k) to (i+1,j,k)
    (0, 1, 0), // Edge along +Y direction from vertex at (i,j,k) to (i,j+1,k)
    (0, 0, 1)  // Edge along +Z direction from vertex at (i,j,k) to (i,j,k+1)
];

// Cells that surround each edge direction
// Each array contains the relative positions of the 4 cells that meet at an edge
// These cells will contribute vertices to form a quad in the final mesh
const CELLS_AROUND_EDGES: [[(i32, i32, i32); 4]; 3] = [
    // Cells around X-direction edge from vertex at (i,j,k) to vertex at (i+1,j,k)
    // For this X edge, we use the cells at: (i,j,k), (i,j+1,k), (i,j+1,k+1), (i,j,k+1)
    [(0, 0, 0), (0, 1, 0), (0, 1, 1), (0, 0, 1)],
    
    // Cells around Y-direction edge from vertex at (i,j,k) to vertex at (i,j+1,k)
    // For this Y edge, we use the cells at: (i,j,k), (i+1,j,k), (i+1,j,k+1), (i,j,k+1)
    [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
    
    // Cells around Z-direction edge from vertex at (i,j,k) to vertex at (i,j,k+1)
    // For this Z edge, we use the cells at: (i,j,k), (i+1,j,k), (i+1,j+1,k), (i,j+1,k)
    [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
];

pub fn generate_cells(node_evaluator: &NodeEvaluator) -> HashMap<(i32, i32, i32), DCCell> {
  let mut cells = HashMap::new();

  generate_cells_for_box(
    node_evaluator,
    &(common_constants::IMPLICIT_VOLUME_MIN * DC_3D_SAMPLES_PER_UNIT),
    &((common_constants::IMPLICIT_VOLUME_MAX - common_constants::IMPLICIT_VOLUME_MIN) * DC_3D_SAMPLES_PER_UNIT),
    &mut cells);

  return cells;
}

pub fn generate_mesh(cells: &mut HashMap<(i32, i32, i32), DCCell>, node_evaluator: &NodeEvaluator) -> QuadMesh {
  let mut mesh = QuadMesh::new();
  
  // First pass: Generate vertices for cells and process edges
  process_cell_edges(cells, node_evaluator, &mut mesh);
  
  // Second pass: Calculate proper vertex positions for each cell
  optimize_vertex_positions(cells, node_evaluator, &mut mesh);
  
  // Compute normals for quads
  mesh.compute_quad_normals();
  
  mesh
}

fn process_cell_edges(
  cells: &mut HashMap<(i32, i32, i32), DCCell>, 
  node_evaluator: &NodeEvaluator, 
  mesh: &mut QuadMesh
) {
  // Create a list of vertices to process (each cell key is also the key of its minimum vertex)
  let vertex_keys: Vec<(i32, i32, i32)> = cells.keys().cloned().collect();
  
  // Process all edges that connect vertices
  for &vertex_key in &vertex_keys {
    // For each vertex, check the 3 edges in positive directions (+X, +Y, +Z)
    // This ensures we process each edge exactly once
    for (dir_idx, &(dx, dy, dz)) in EDGE_DIRECTIONS.iter().enumerate() {
      // Calculate the adjacent vertex along this edge
      let adjacent_vertex = (vertex_key.0 + dx, vertex_key.1 + dy, vertex_key.2 + dz);
      
      // Get the SDF values at the endpoints (vertices) of the edge
      // These are the actual grid vertices where we evaluate the SDF
      let p1 = get_vertex_world_pos(vertex_key); // World position of first vertex
      let p2 = get_vertex_world_pos(adjacent_vertex); // World position of second vertex
      
      let sdf1 = node_evaluator.eval(&p1);
      let sdf2 = node_evaluator.eval(&p2);
      
      // Skip if there's no sign change across the edge
      if sdf1 * sdf2 > 0.0 || sdf1 == sdf2 {
        continue;
      }
      
      // If we got here, we found a sign change, which means the surface intersects this edge
      // We'll create a quad around this edge using the dual contouring approach
      let cells_around_edge = &CELLS_AROUND_EDGES[dir_idx];
      
      // First, check if all required cells for the quad exist
      let mut surrounding_cell_keys = [(0, 0, 0); 4];
      let mut all_cells_exist = true;
      
      // Check if all required cells exist before creating any vertices
      for (i, &(rx, ry, rz)) in cells_around_edge.iter().enumerate() {
        let surrounding_cell_key = (
          vertex_key.0 + rx, 
          vertex_key.1 + ry, 
          vertex_key.2 + rz
        );
        
        surrounding_cell_keys[i] = surrounding_cell_key;
        
        if !cells.contains_key(&surrounding_cell_key) {
          all_cells_exist = false;
          break;
        }
      }
      
      // Skip if not all required cells exist
      if !all_cells_exist {
        continue;
      }
      
      // Calculate the intersection point and normal once for all cells
      // Find the precise intersection point on the edge where SDF = 0
      let intersection = find_edge_intersection(node_evaluator, &p1, &p2);
      
      // Get the gradient (normal) at the intersection point using built-in method
      let (normal, _) = node_evaluator.get_gradient(&intersection);
      
      // Create the edge intersection data
      let edge_intersection = EdgeIntersection {
        position: intersection,
        normal: normal,
      };
      
      // If we get here, all cells exist, so we can safely create/use vertices
      let mut cell_indices = [0; 4];
      
      // Now create or reuse vertices in each surrounding cell, and store the intersection data
      for (i, &surrounding_cell_key) in surrounding_cell_keys.iter().enumerate() {
        let cell = cells.get_mut(&surrounding_cell_key).unwrap();
        
        // Store the intersection data in this cell
        cell.edge_intersections.push(edge_intersection.clone());
        
        // Create vertex for this cell if it doesn't have one yet
        if cell.vertex_index == -1 {
          // For now, put the vertex at the center of the cell
          // We'll optimize the position later in optimize_vertex_positions
          let cell_center = get_cell_center_pos(surrounding_cell_key);
          
          // With QuadMesh, we only need to add the position (no normals/materials needed)
          let vertex_index = mesh.add_vertex(cell_center);
          cell.vertex_index = vertex_index as i32;
        }
        
        cell_indices[i] = cell.vertex_index as u32;
      }
      
      // Add quad using the vertices from all four surrounding cells
      mesh.add_quad(cell_indices[0], cell_indices[1], cell_indices[2], cell_indices[3]);
    }
  }
}

// Helper function to convert vertex coordinates to world position
// Note: Cell coordinates are the same as the coordinates of the minimum corner vertex of the cell
// For example: Cell (5,3,2) has its minimum corner vertex at world position (5/SPU, 3/SPU, 2/SPU)
fn get_vertex_world_pos(vertex_key: (i32, i32, i32)) -> DVec3 {
  DVec3::new(
    vertex_key.0 as f64 / DC_3D_SAMPLES_PER_UNIT as f64,
    vertex_key.1 as f64 / DC_3D_SAMPLES_PER_UNIT as f64, 
    vertex_key.2 as f64 / DC_3D_SAMPLES_PER_UNIT as f64
  )
}

// Helper function to get the center position of a cell
// The center is 0.5 units (in grid coordinates) from the minimum vertex
fn get_cell_center_pos(cell_key: (i32, i32, i32)) -> DVec3 {
  DVec3::new(
    (cell_key.0 as f64 + 0.5) / DC_3D_SAMPLES_PER_UNIT as f64,
    (cell_key.1 as f64 + 0.5) / DC_3D_SAMPLES_PER_UNIT as f64, 
    (cell_key.2 as f64 + 0.5) / DC_3D_SAMPLES_PER_UNIT as f64
  )
}

// Function to find the zero-crossing point on an edge using binary search
fn find_edge_intersection(node_evaluator: &NodeEvaluator, p1: &DVec3, p2: &DVec3) -> DVec3 {
  let mut a = *p1;
  let mut b = *p2;
  let mut sdf_a = node_evaluator.eval(&a);
  let mut sdf_b = node_evaluator.eval(&b);
  
  // Ensure we have opposite signs
  if sdf_a * sdf_b > 0.0 {
    return (a + b) * 0.5; // Return midpoint if not a zero-crossing
  }
  
  // Binary search for zero-crossing (8 iterations should be enough)
  for _ in 0..8 {
    let mid = (a + b) * 0.5;
    let sdf_mid = node_evaluator.eval(&mid);
    
    if sdf_mid * sdf_a <= 0.0 {
      b = mid;
      sdf_b = sdf_mid;
    } else {
      a = mid;
      sdf_a = sdf_mid;
    }
  }
  
  // Return the best approximation of the zero-crossing point
  (a + b) * 0.5
}

// Optimize vertex positions using intersection data
fn optimize_vertex_positions(cells: &mut HashMap<(i32, i32, i32), DCCell>, _node_evaluator: &NodeEvaluator, mesh: &mut QuadMesh) {
  // Iterate over all cells to optimize vertex positions based on stored edge intersections
  for cell in cells.values() {
    // Skip cells without a vertex or intersections
    if cell.vertex_index < 0 || cell.edge_intersections.is_empty() {
      continue;
    }
    
    // Compute average position from edge intersections
    let mut avg_position = DVec3::ZERO;
    
    for intersection in &cell.edge_intersections {
      avg_position += intersection.position;
    }
    
    avg_position /= cell.edge_intersections.len() as f64;
    
    // Update the vertex position in the mesh (no normals in QuadMesh)
    if cell.vertex_index >= 0 {
      mesh.set_vertex_position(cell.vertex_index as usize, avg_position);
    }
  }
}

fn generate_cells_for_box(
  node_evaluator: &NodeEvaluator,
  start_pos: &IVec3,
  size: &IVec3,
  cells: &mut HashMap<(i32, i32, i32), DCCell>) {

  let spu = DC_3D_SAMPLES_PER_UNIT as f64;
  let epsilon = 0.001;

  // Calculate the center point of the box
  let center_point = (start_pos.as_dvec3() + size.as_dvec3() / 2.0) / spu;

  // Evaluate SDF at the center point
  let sdf_value = node_evaluator.eval(&center_point);

  let half_diagonal = size.as_dvec3().length() / spu / 2.0;

  // If absolute SDF value is greater than half diagonal, there's no surface in this box
  if sdf_value.abs() > half_diagonal + epsilon {
    return;
  }

  // Determine if we should subdivide in each dimension (size >= 4)
  let should_subdivide_x = size.x >= 4;
  let should_subdivide_y = size.y >= 4;
  let should_subdivide_z = size.z >= 4;

  // If we can't subdivide in any direction, process each cell individually
  if !should_subdivide_x && !should_subdivide_y && !should_subdivide_z {
    // Process each cell within the box
    for x in 0..size.x {
        for y in 0..size.y {
            for z in 0..size.z {
                let cell_pos = IVec3::new(
                    start_pos.x + x,
                    start_pos.y + y,
                    start_pos.z + z
                );
                cells.insert(
                    (cell_pos.x, cell_pos.y, cell_pos.z),
                    DCCell {
                        vertex_index: -1,
                        edge_intersections: Vec::new(),
                    }
                );
            }
        }
    }
    return;
  }

  // Otherwise, subdivide the box and recursively process each subdivision
  let subdivisions = subdivide_box(
    start_pos,
    size,
    should_subdivide_x,
    should_subdivide_y,
    should_subdivide_z
  );

  // Process each subdivision recursively
  for (sub_start, sub_size) in subdivisions {
    generate_cells_for_box(
        node_evaluator,
        &sub_start,
        &sub_size,
        cells,
    );
  }
}

