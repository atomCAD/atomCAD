use glam::i32::IVec2;
use glam::i32::IVec3;

pub fn subdivide_box(
  start_pos: &IVec3,
  size: &IVec3,
  should_subdivide_x: bool,
  should_subdivide_y: bool,
  should_subdivide_z: bool
) -> Vec<(IVec3, IVec3)> {
  let mut result = Vec::new();
  
  // Calculate first subdivision sizes
  let sub_size_x_first = if should_subdivide_x { size.x / 2 } else { size.x };
  let sub_size_y_first = if should_subdivide_y { size.y / 2 } else { size.y };
  let sub_size_z_first = if should_subdivide_z { size.z / 2 } else { size.z };
  
  // Calculate second subdivision sizes, accounting for remainder
  let sub_size_x_second = if should_subdivide_x { size.x - sub_size_x_first } else { size.x };
  let sub_size_y_second = if should_subdivide_y { size.y - sub_size_y_first } else { size.y };
  let sub_size_z_second = if should_subdivide_z { size.z - sub_size_z_first } else { size.z };
  
  // Calculate the number of subdivisions in each direction
  let subdivisions_x = if should_subdivide_x { 2 } else { 1 };
  let subdivisions_y = if should_subdivide_y { 2 } else { 1 };
  let subdivisions_z = if should_subdivide_z { 2 } else { 1 };
  
  // Generate all subdivision boxes
  for dx in 0..subdivisions_x {
      let sub_size_x = if dx == 0 { sub_size_x_first } else { sub_size_x_second };
      let offset_x = if dx == 0 { 0 } else { sub_size_x_first };
      
      for dy in 0..subdivisions_y {
          let sub_size_y = if dy == 0 { sub_size_y_first } else { sub_size_y_second };
          let offset_y = if dy == 0 { 0 } else { sub_size_y_first };
          
          for dz in 0..subdivisions_z {
              let sub_size_z = if dz == 0 { sub_size_z_first } else { sub_size_z_second };
              let offset_z = if dz == 0 { 0 } else { sub_size_z_first };
              
              let sub_start = IVec3::new(
                  start_pos.x + offset_x,
                  start_pos.y + offset_y,
                  start_pos.z + offset_z
              );
              
              let sub_size = IVec3::new(
                  sub_size_x,
                  sub_size_y,
                  sub_size_z
              );
              
              result.push((sub_start, sub_size));
          }
      }
  }
  
  result
}

pub fn subdivide_rect(
    start_pos: &IVec2,
    size: &IVec2,
    should_subdivide_x: bool,
    should_subdivide_y: bool,
  ) -> Vec<(IVec2, IVec2)> {
    let mut result = Vec::new();
    
    // Calculate first subdivision sizes
    let sub_size_x_first = if should_subdivide_x { size.x / 2 } else { size.x };
    let sub_size_y_first = if should_subdivide_y { size.y / 2 } else { size.y };
    
    // Calculate second subdivision sizes, accounting for remainder
    let sub_size_x_second = if should_subdivide_x { size.x - sub_size_x_first } else { size.x };
    let sub_size_y_second = if should_subdivide_y { size.y - sub_size_y_first } else { size.y };
    
    // Calculate the number of subdivisions in each direction
    let subdivisions_x = if should_subdivide_x { 2 } else { 1 };
    let subdivisions_y = if should_subdivide_y { 2 } else { 1 };
    
    // Generate all subdivision rects
    for dx in 0..subdivisions_x {
        let sub_size_x = if dx == 0 { sub_size_x_first } else { sub_size_x_second };
        let offset_x = if dx == 0 { 0 } else { sub_size_x_first };
        
        for dy in 0..subdivisions_y {
            let sub_size_y = if dy == 0 { sub_size_y_first } else { sub_size_y_second };
            let offset_y = if dy == 0 { 0 } else { sub_size_y_first };
            
                
            let sub_start = IVec2::new(
                start_pos.x + offset_x,
                start_pos.y + offset_y,
            );
                
            let sub_size = IVec2::new(
                sub_size_x,
                sub_size_y,
            );
    
            result.push((sub_start, sub_size));
        }
    }

    result
}
