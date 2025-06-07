use std::collections::HashMap;
use glam::i32::IVec3;
use crate::util::box_subdivision::subdivide_box;
use crate::structure_designer::evaluator::implicit_evaluator::NodeEvaluator;
use crate::structure_designer::common_constants;
const DC_3D_SAMPLES_PER_UNIT: i32 = 4;

pub struct DCCell {
  pub vertex_index: i32, // -1 if no vertex for this cell.
}

pub fn generate_cells(node_evaluator: &NodeEvaluator) -> HashMap<(i32, i32, i32), DCCell> {
  let mut cells = HashMap::new();

  generate_cells_for_box(
    node_evaluator,
    &(common_constants::IMPLICIT_VOLUME_MIN * DC_3D_SAMPLES_PER_UNIT),
    &((common_constants::IMPLICIT_VOLUME_MAX - common_constants::IMPLICIT_VOLUME_MIN) * DC_3D_SAMPLES_PER_UNIT),
    &mut cells);

  return cells;
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

