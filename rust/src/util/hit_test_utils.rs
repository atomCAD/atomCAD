use glam::f32::Vec3;
use glam::f32::Quat;

// ray_direction must be normalized
pub fn sphere_hit_test(
    sphere_center: &Vec3,
    sphere_radius: f32,
    ray_origin: &Vec3,
    ray_direction: &Vec3) -> Option<f32> {
    
    // Vector from sphere center to ray origin
    let oc = *ray_origin - *sphere_center;
    
    // Coefficients for the quadratic equation: at² + bt + c = 0
    // Since ray_direction is normalized, a = 1.0
    let a = 1.0; // ray_direction.length_squared() which is 1 since it's normalized
    let b = 2.0 * ray_direction.dot(oc);
    let c = oc.length_squared() - sphere_radius * sphere_radius;
    
    // Calculate the discriminant
    let discriminant = b * b - 4.0 * a * c;
    
    if discriminant < 0.0 {
        // No intersection
        return None;
    }
    
    // Calculate the two potential intersection points
    let discriminant_sqrt = discriminant.sqrt();
    
    // Calculate both solutions (-b ± sqrt(discriminant)) / (2a)
    let t1 = (-b - discriminant_sqrt) / (2.0 * a);
    let t2 = (-b + discriminant_sqrt) / (2.0 * a);
    
    // We want the closest intersection point that's in front of the ray origin
    if t1 > 0.0 {
        // First intersection is valid and in front of us
        Some(t1)
    } else if t2 > 0.0 {
        // Second intersection is valid (ray starts inside the sphere)
        Some(t2)
    } else {
        // Both intersections are behind us
        None
    }
}

pub fn get_closest_point_on_first_ray(
    ray1_origin: &Vec3,
    ray1_direction: &Vec3,
    ray2_origin: &Vec3,
    ray2_direction: &Vec3) -> f32 {
    // This function calculates the parameter 't' on ray1 where the two rays are closest to each other.
    // Based on the formula for distance between two skew lines.
    
    // Normalize directions to ensure proper calculations
    let dir1 = ray1_direction.normalize();
    let dir2 = ray2_direction.normalize();
    
    // Compute the difference between origins
    let r = *ray1_origin - *ray2_origin;
    
    // Calculate dot products needed for the formula
    let a = dir1.dot(dir1); // Always 1 since we normalized
    let b = dir1.dot(dir2);
    let c = dir2.dot(dir2); // Always 1 since we normalized
    let d = dir1.dot(r);
    let e = dir2.dot(r);
    
    // Compute denominator
    let denominator = a * c - b * b;
    
    // If rays are parallel (or nearly parallel), use a different approach
    if denominator.abs() < 1e-6 {
        // When parallel, just project r onto the first direction
        return -d / a;
    }
    
    // Calculate the parameter 't' for the first ray
    let t = (b * e - c * d) / denominator;
    
    return t;
}

pub fn get_point_distance_to_ray(
    ray_origin: &Vec3,
    ray_direction: &Vec3,
    point: &Vec3) -> f32 {
    // Vector from ray origin to the point
    let v = *point - *ray_origin;
    
    // Project v onto the ray direction
    let proj = v.dot(*ray_direction);
    
    // Calculate the closest point on the ray to the given point
    let closest_point = *ray_origin + *ray_direction * proj;
    
    // Return the distance between the point and the closest point on the ray
    (*point - closest_point).length()
}

pub fn cylinder_hit_test(
    cylinder_top_center: &Vec3,
    cylinder_bottom_center: &Vec3,
    cylinder_radius: f32,
    ray_origin: &Vec3,
    ray_direction: &Vec3) -> Option<f32> {
    
    // Step 1: Calculate cylinder properties
    let cylinder_axis = *cylinder_top_center - *cylinder_bottom_center;
    let cylinder_height = cylinder_axis.length();
    let cylinder_center = (*cylinder_bottom_center + *cylinder_top_center) * 0.5;
    let cylinder_axis_normalized = cylinder_axis.normalize();
    
    // Step 2: Create a rotation that maps the cylinder axis to the y-axis (0,1,0)
    let y_axis = Vec3::new(0.0, 1.0, 0.0);
    
    // Much simpler rotation calculation using from_rotation_arc
    let quat = Quat::from_rotation_arc(cylinder_axis_normalized, y_axis);
    
    // Step 3: Transform the ray to the cylinder's local space where cylinder axis is along y
    let center_to_origin = *ray_origin - cylinder_center;
    let local_ray_origin = quat.mul_vec3(center_to_origin);
    let local_ray_direction = quat.mul_vec3(*ray_direction);
    
    // Half height of the cylinder in local space
    let half_height = cylinder_height / 2.0;
    
    // Step 4: Test intersection with infinite cylinder in local space (x^2 + z^2 = r^2)
    
    // We're solving the quadratic: (ox + t*dx)^2 + (oz + t*dz)^2 = r^2
    // Where o is local_ray_origin and d is local_ray_direction
    
    // Extract the x and z components
    let ox = local_ray_origin.x;
    let oz = local_ray_origin.z;
    let dx = local_ray_direction.x;
    let dz = local_ray_direction.z;
    
    // Coefficients for the quadratic equation: at² + bt + c = 0
    let a = dx * dx + dz * dz;
    
    // If a is very small, the ray is almost parallel to the cylinder axis
    if a < 1e-6 {
        // Check if ray is inside the cylinder
        if ox * ox + oz * oz > cylinder_radius * cylinder_radius {
            return None; // Ray is outside and parallel to the cylinder
        }
        
        // Ray is inside the infinite cylinder and parallel to axis
        // Check caps intersection (planes at y = -half_height and y = half_height)
        return cylinder_caps_intersection(&local_ray_origin, &local_ray_direction, half_height, cylinder_radius);
    }
    
    let b = 2.0 * (ox * dx + oz * dz);
    let c = ox * ox + oz * oz - cylinder_radius * cylinder_radius;
    
    // Calculate the discriminant
    let discriminant = b * b - 4.0 * a * c;
    
    if discriminant < 0.0 {
        // No intersection with infinite cylinder
        return None;
    }
    
    // Calculate intersection points with infinite cylinder
    let discriminant_sqrt = discriminant.sqrt();
    let t1 = (-b - discriminant_sqrt) / (2.0 * a);
    let t2 = (-b + discriminant_sqrt) / (2.0 * a);
    
    // We need to find the t value that represents the closest valid intersection
    let mut t_cyl = std::f32::MAX;
    let mut found = false;
    
    // Check if t1 is a valid intersection (within cylinder height)
    if t1 > 0.0 {
        let hit_point_y = local_ray_origin.y + t1 * local_ray_direction.y;
        if hit_point_y >= -half_height && hit_point_y <= half_height {
            t_cyl = t1;
            found = true;
        }
    }
    
    // Check if t2 is a valid intersection and closer (within cylinder height)
    if t2 > 0.0 && t2 < t_cyl {
        let hit_point_y = local_ray_origin.y + t2 * local_ray_direction.y;
        if hit_point_y >= -half_height && hit_point_y <= half_height {
            t_cyl = t2;
            found = true;
        }
    }
    
    // Step 5: Test intersection with the caps (top and bottom)
    let t_caps = cylinder_caps_intersection(&local_ray_origin, &local_ray_direction, half_height, cylinder_radius);
    
    // Return the closest intersection point (either with the cylinder wall or caps)
    match (found, t_caps) {
        (true, Some(t_cap)) => Some(t_cyl.min(t_cap)),
        (true, None) => Some(t_cyl),
        (false, Some(t_cap)) => Some(t_cap),
        (false, None) => None,
    }
}

// Helper function to calculate intersection with the cylinder caps
fn cylinder_caps_intersection(
    local_ray_origin: &Vec3, 
    local_ray_direction: &Vec3, 
    half_height: f32,
    cylinder_radius: f32
) -> Option<f32> {
    let mut t_min = std::f32::MAX;
    let mut found = false;
    
    // Skip cap intersection tests if ray is parallel to the cap planes
    if local_ray_direction.y.abs() > 1e-6 {
        // Bottom cap (y = -half_height)
        let t_bottom = (-half_height - local_ray_origin.y) / local_ray_direction.y;
        if t_bottom > 0.0 {
            let hit_x = local_ray_origin.x + t_bottom * local_ray_direction.x;
            let hit_z = local_ray_origin.z + t_bottom * local_ray_direction.z;
            // Check if hit point is within the circle of the cap
            if hit_x * hit_x + hit_z * hit_z <= cylinder_radius * cylinder_radius {
                t_min = t_bottom;
                found = true;
            }
        }
        
        // Top cap (y = half_height)
        let t_top = (half_height - local_ray_origin.y) / local_ray_direction.y;
        if t_top > 0.0 && t_top < t_min {
            let hit_x = local_ray_origin.x + t_top * local_ray_direction.x;
            let hit_z = local_ray_origin.z + t_top * local_ray_direction.z;
            // Check if hit point is within the circle of the cap
            if hit_x * hit_x + hit_z * hit_z <= cylinder_radius * cylinder_radius {
                t_min = t_top;
                found = true;
            }
        }
    }
    
    if found {
        Some(t_min)
    } else {
        None
    }
}
