use glam::f64::DVec3;
use glam::f64::DQuat;

// ray_direction must be normalized
// If the ray hits the sphere, returns the distance from the ray origin to the intersection point
// If the ray does not hit the sphere, returns None
pub fn sphere_hit_test(
    sphere_center: &DVec3,
    sphere_radius: f64,
    ray_origin: &DVec3,
    ray_direction: &DVec3) -> Option<f64> {
    
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

/*
 * This function calculates the parameter 't' on ray1 where the two rays are closest to each other.
   based on the formula for distance between two skew lines.
 */
pub fn get_closest_point_on_first_ray(
    ray1_origin: &DVec3,
    ray1_direction: &DVec3,
    ray2_origin: &DVec3,
    ray2_direction: &DVec3) -> f64 {
    
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

    t
}

pub fn get_point_distance_to_ray(
    ray_origin: &DVec3,
    ray_direction: &DVec3,
    point: &DVec3) -> f64 {
    // Vector from ray origin to the point
    let v = *point - *ray_origin;
    
    // Project v onto the ray direction
    let proj = v.dot(*ray_direction);
    
    // Calculate the closest point on the ray to the given point
    let closest_point = *ray_origin + *ray_direction * proj;
    
    // Return the distance between the point and the closest point on the ray
    (*point - closest_point).length()
}

// If the ray hits the cylinder, returns the distance from the ray origin to the intersection point
// If the ray does not hit the cylinder, returns None
pub fn cylinder_hit_test(
    cylinder_top_center: &DVec3,
    cylinder_bottom_center: &DVec3,
    cylinder_radius: f64,
    ray_origin: &DVec3,
    ray_direction: &DVec3) -> Option<f64> {
    
    // Step 1: Calculate cylinder properties
    let cylinder_axis = *cylinder_top_center - *cylinder_bottom_center;
    let cylinder_height = cylinder_axis.length();
    let cylinder_center = (*cylinder_bottom_center + *cylinder_top_center) * 0.5;
    let cylinder_axis_normalized = cylinder_axis.normalize();
    
    // Step 2: Create a rotation that maps the cylinder axis to the y-axis (0,1,0)
    let y_axis = DVec3::new(0.0, 1.0, 0.0);
    
    // Much simpler rotation calculation using from_rotation_arc
    let quat = DQuat::from_rotation_arc(cylinder_axis_normalized, y_axis);
    
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
    let mut t_cyl = std::f64::MAX;
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
    local_ray_origin: &DVec3, 
    local_ray_direction: &DVec3, 
    half_height: f64,
    cylinder_radius: f64
) -> Option<f64> {
    let mut t_min = std::f64::MAX;
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

// If the ray hits the cone, returns the distance from the ray origin to the intersection point
// If the ray does not hit the cone, returns None
pub fn cone_hit_test(
    apex: &DVec3,
    base_center: &DVec3,
    radius: f64,
    ray_origin: &DVec3,
    ray_direction: &DVec3) -> Option<f64> {
    
    // Step 1: Calculate cone properties
    let cone_axis = *apex - *base_center;
    let cone_height = cone_axis.length();
    let cone_axis_normalized = cone_axis.normalize();
    
    // Step 2: Create a rotation that maps the cone axis to the y-axis (0,1,0)
    let y_axis = DVec3::new(0.0, 1.0, 0.0);
    let quat = DQuat::from_rotation_arc(cone_axis_normalized, y_axis);
    
    // Step 3: Transform the ray to the cone's local space where cone axis is along y
    // and apex is at (0,0,0)
    let base_to_origin = *ray_origin - *base_center;
    let local_ray_origin = quat.mul_vec3(base_to_origin);
    let local_ray_direction = quat.mul_vec3(*ray_direction);
    
    // In local space, the apex is at (0, cone_height, 0) and base center is at (0, 0, 0)
    
    // Step 4: Test intersection with the conical surface
    // Equation of cone with apex at (0, cone_height, 0) and base at y=0:
    // x²+z² = (cone_height - y)² * (radius/cone_height)²
    
    // Extract components
    let ox = local_ray_origin.x;
    let oy = local_ray_origin.y;
    let oz = local_ray_origin.z;
    let dx = local_ray_direction.x;
    let dy = local_ray_direction.y;
    let dz = local_ray_direction.z;
    
    // Ratio of radius to height squared (tan²)
    let k = (radius / cone_height).powi(2);
    
    // Coefficients for the quadratic equation: at² + bt + c = 0
    let a = dx * dx + dz * dz - k * dy * dy;
    let b = 2.0 * (ox * dx + oz * dz - k * dy * (cone_height - oy));
    let c = ox * ox + oz * oz - k * (cone_height - oy).powi(2);
    
    // Check if we have a valid solution to the quadratic equation
    let discriminant = b * b - 4.0 * a * c;
    
    // If a is very close to 0, ray is parallel to the cone surface
    if a.abs() < 1e-6 {
        if b.abs() < 1e-6 {
            // Ray lies on the cone surface or doesn't intersect
            return None;
        }
        // Linear equation: bt + c = 0
        let t = -c / b;
        if t <= 0.0 {
            return None; // Intersection behind ray origin
        }
        
        // Check if intersection is between apex and base
        let hit_y = oy + t * dy;
        if hit_y < 0.0 || hit_y > cone_height {
            return None; // Outside cone's height bounds
        }
        
        return Some(t); // Valid intersection
    }

    if discriminant < 0.0 {
        // No real solutions, ray doesn't intersect cone
        return None;
    }
    
    // Calculate intersection points
    let discriminant_sqrt = discriminant.sqrt();
    let t1 = (-b - discriminant_sqrt) / (2.0 * a);
    let t2 = (-b + discriminant_sqrt) / (2.0 * a);
    
    // We need to find the t value that represents the closest valid intersection
    let mut t_cone = std::f64::MAX;
    let mut found = false;
    
    // Check if t1 is a valid intersection (within cone height)
    if t1 > 0.0 {
        let hit_y = oy + t1 * dy;
        if hit_y >= 0.0 && hit_y <= cone_height {
            //println!("t1: {} hit_y: {}", t1, hit_y);
            t_cone = t1;
            found = true;
        }
    }
    
    // Check if t2 is a valid intersection and closer (within cone height)
    if t2 > 0.0 && t2 < t_cone {
        let hit_y = oy + t2 * dy;
        if hit_y >= 0.0 && hit_y <= cone_height {
            //println!("t2: {} hit_y: {}", t2, hit_y);
            t_cone = t2;
            found = true;
        }
    }
    
    // Step 5: Test intersection with the base cap (the circle at the bottom)
    let mut t_cap = None;
    
    // Skip base cap intersection test if ray is parallel to the base plane
    if dy.abs() > 1e-6 {
        // Base cap (y = 0)
        let t_base = -oy / dy;
        if t_base > 0.0 {
            let hit_x = ox + t_base * dx;
            let hit_z = oz + t_base * dz;
            // Check if hit point is within the circle of the base
            if hit_x * hit_x + hit_z * hit_z <= radius * radius {
                t_cap = Some(t_base);
            }
        }
    }
    
    // Return the closest intersection point (either with the cone surface or base cap)
    match (found, t_cap) {
        (true, Some(t_cap_val)) => Some(t_cone.min(t_cap_val)),
        (true, None) => Some(t_cone),
        (false, Some(t_cap_val)) => Some(t_cap_val),
        (false, None) => None,
    }
}

pub fn arrow_hit_test(
    start_center: &DVec3,
    axis_dir: &DVec3,
    cylinder_radius: f64,
    cone_radius: f64,
    cylinder_length: f64,
    cone_length: f64,
    cone_offset: f64,
    ray_origin: &DVec3,
    ray_direction: &DVec3) -> Option<f64> {

    let cone_hit = cone_hit_test(
        &(start_center + axis_dir * (cylinder_length - cone_offset + cone_length)),
        &(start_center + axis_dir * (cylinder_length - cone_offset)),
        cone_radius,
        ray_origin,
        ray_direction
    );

    if cone_hit.is_some() {
        return cone_hit;
    }

    let cylinder_hit = cylinder_hit_test(
        &(start_center + axis_dir * cylinder_length),
        &start_center,
        cylinder_radius,
        ray_origin,
        ray_direction);

    if cylinder_hit.is_some() {
        cylinder_hit
    } else {
        None
    }
}
















