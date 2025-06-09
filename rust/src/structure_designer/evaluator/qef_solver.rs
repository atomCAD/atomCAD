use glam::DVec3;

/// A simple QEF (Quadratic Error Function) solver that minimizes the sum of squared
/// distances from a point to a set of planes defined by points and normals.
pub struct QefSolver {
    // Accumulates AtA matrix (3x3)
    ata: [[f64; 3]; 3],
    // Accumulates Atb vector (3x1)
    atb: [f64; 3],
    // Accumulates btb scalar (for error calculation)
    btb: f64,
    // Accumulated mass point of intersection points
    mass_point: DVec3,
    // Count of intersections added
    num_points: usize,
}

impl QefSolver {
    /// Creates a new empty QEF solver
    pub fn new() -> Self {
        QefSolver {
            ata: [[0.0; 3]; 3],
            atb: [0.0; 3],
            btb: 0.0,
            mass_point: DVec3::ZERO,
            num_points: 0,
        }
    }
    
    /// Adds a point-normal pair to the QEF
    pub fn add(&mut self, point: &DVec3, normal: &DVec3) {
        // Ensure normal is normalized
        let n = if normal.length_squared() > 0.0 {
            normal.normalize()
        } else {
            return; // Skip invalid normals
        };
        
        // Accumulate mass point for later use
        self.mass_point += *point;
        self.num_points += 1;
        
        // Calculate d in the plane equation ax + by + cz + d = 0
        let d = -n.dot(*point);
        
        // Update AtA matrix (symmetric matrix, only need to update upper half)
        for i in 0..3 {
            for j in i..3 {
                self.ata[i][j] += n[i] * n[j];
            }
            // Update Atb vector
            self.atb[i] += n[i] * d;
        }
        
        // Update btb scalar
        self.btb += d * d;
    }
    
    /// Solves the QEF and returns the point that minimizes the error function
    pub fn solve(&self) -> DVec3 {
        // If no points added, return zero
        if self.num_points == 0 {
            return DVec3::ZERO;
        }
        
        // Complete the lower half of AtA matrix
        let mut ata_full = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                if i >= j {
                    ata_full[i][j] = self.ata[j][i];
                } else {
                    ata_full[i][j] = self.ata[i][j];
                }
            }
        }
        
        // Calculate the mass point (average of intersection points)
        let mass_point = if self.num_points > 0 {
            self.mass_point / self.num_points as f64
        } else {
            DVec3::ZERO
        };
        
        // Try to solve the system using pseudo-inverse
        if let Some(solution) = self.solve_system(&ata_full, &self.atb) {
            return solution;
        }
        
        // Fallback to mass point if singular
        mass_point
    }
    
    /// Solves a 3x3 linear system using Gaussian elimination
    fn solve_system(&self, a: &[[f64; 3]; 3], b: &[f64; 3]) -> Option<DVec3> {
        // Create augmented matrix [A|b]
        let mut aug = [[0.0; 4]; 3];
        for i in 0..3 {
            for j in 0..3 {
                aug[i][j] = a[i][j];
            }
            aug[i][3] = -b[i]; // Negative because we computed -Atb
        }
        
        // Gaussian elimination with partial pivoting
        for i in 0..3 {
            // Find pivot
            let mut max_idx = i;
            let mut max_val = aug[i][i].abs();
            
            for j in i+1..3 {
                let abs_val = aug[j][i].abs();
                if abs_val > max_val {
                    max_idx = j;
                    max_val = abs_val;
                }
            }
            
            // Check for singularity
            if max_val < 1e-10 {
                return None;
            }
            
            // Swap rows if needed
            if max_idx != i {
                for j in 0..4 {
                    let temp = aug[i][j];
                    aug[i][j] = aug[max_idx][j];
                    aug[max_idx][j] = temp;
                }
            }
            
            // Eliminate
            for j in i+1..3 {
                let factor = aug[j][i] / aug[i][i];
                for k in i..4 {
                    aug[j][k] -= factor * aug[i][k];
                }
            }
        }
        
        // Back substitution
        let mut x = [0.0; 3];
        for i in (0..3).rev() {
            let mut sum = aug[i][3];
            for j in i+1..3 {
                sum -= aug[i][j] * x[j];
            }
            
            if aug[i][i].abs() < 1e-10 {
                return None; // Singular matrix
            }
            
            x[i] = sum / aug[i][i];
        }
        
        Some(DVec3::new(x[0], x[1], x[2]))
    }
    
    /// Calculate error at a given point
    pub fn evaluate_error(&self, point: &DVec3) -> f64 {
        // Error = p^T A p - 2 p^T b + c
        let mut error = self.btb;
        
        // Compute p^T A p
        for i in 0..3 {
            for j in 0..3 {
                let a_ij = if i <= j { self.ata[i][j] } else { self.ata[j][i] };
                error += point[i] * a_ij * point[j];
            }
            // Subtract 2 p^T b
            error -= 2.0 * point[i] * self.atb[i];
        }
        
        error
    }
}

/// Determines if a point is inside a cell defined by min/max bounds
pub fn is_point_in_cell(point: DVec3, min_bound: DVec3, max_bound: DVec3) -> bool {
    point.x >= min_bound.x && point.x <= max_bound.x &&
    point.y >= min_bound.y && point.y <= max_bound.y &&
    point.z >= min_bound.z && point.z <= max_bound.z
}

/// Projects a point to the nearest point inside cell bounds
pub fn project_to_cell_bounds(point: DVec3, min_bound: DVec3, max_bound: DVec3) -> DVec3 {
    DVec3::new(
        point.x.clamp(min_bound.x, max_bound.x),
        point.y.clamp(min_bound.y, max_bound.y),
        point.z.clamp(min_bound.z, max_bound.z)
    )
}

/// Computes the optimal position using QEF minimization with cell bounds constraints
pub fn compute_optimal_position(
    intersections: &[DVec3],
    normals: &[DVec3],
    min_bound: DVec3,
    max_bound: DVec3,
) -> DVec3 {
    if intersections.is_empty() || normals.is_empty() || intersections.len() != normals.len() {
        // Return center of cell if no valid data
        return (min_bound + max_bound) * 0.5;
    }
    
    // Create and populate QEF solver
    let mut solver = QefSolver::new();
    for i in 0..intersections.len() {
        solver.add(&intersections[i], &normals[i]);
    }
    
    // Get QEF solution
    let qef_solution = solver.solve();
    
    // Check if solution is inside cell
    if is_point_in_cell(qef_solution, min_bound, max_bound) {
        return qef_solution;
    }
    
    // If outside, compute average of intersection points
    let mut avg_position = DVec3::ZERO;
    for pos in intersections.iter() {
        avg_position += *pos;
    }
    avg_position /= intersections.len() as f64;
    
    // Project QEF solution to cell bounds
    let projected_qef = project_to_cell_bounds(qef_solution, min_bound, max_bound);
    
    // Return a weighted blend favoring the projected QEF solution (70% QEF, 30% average)
    projected_qef * 0.7 + avg_position * 0.3
}
