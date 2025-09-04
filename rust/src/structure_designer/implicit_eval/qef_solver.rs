use glam::DVec3;

const BIAS_STRENGTH: f64 = 0.01;

/// A simple QEF (Quadratic Error Function) solver that minimizes the sum of squared
/// distances from a point to a set of planes defined by points and normals.
pub struct QefSolver {
    ata: [[f64; 3]; 3],        // Accumulates upper-triangular AtA
    atb: [f64; 3],             // Accumulates Atb
    btb: f64,                  // Accumulates btb
    mass_point: DVec3,         // Sum of intersection points
    num_points: usize,         // Count of added points
    points: Vec<DVec3>,        // Store intersection points
    normals: Vec<DVec3>,       // Store normals for active-set and rank-1
}

impl QefSolver {
    pub fn new() -> Self {
        Self {
            ata: [[0.0; 3]; 3],
            atb: [0.0; 3],
            btb: 0.0,
            mass_point: DVec3::ZERO,
            num_points: 0,
            points: Vec::new(),
            normals: Vec::new(),
        }
    }

    pub fn print_data(&self) {
        println!("Num points: {}", self.num_points);
        println!("points: {:?}", self.points);
        println!("normals: {:?}", self.normals);
        println!("Mass point: {:?}", self.mass_point);
    }

    /// Adds a point-normal pair to the QEF
    pub fn add(&mut self, point: &DVec3, normal: &DVec3) {
        self.points.push(*point);
        self.normals.push(*normal);
        self.mass_point += *point;
        self.num_points += 1;
        let d = -normal.dot(*point);

        // accumulate upper-triangular ATA and Atb
        for i in 0..3 {
            for j in i..3 {
                self.ata[i][j] += normal[i] * normal[j];
            }
            self.atb[i] += normal[i] * d;
        }
        self.btb += d * d;
    }

    /// Adds extra normals that add extra error the further we go
    /// from the cell, this encourages the final result to be
    /// inside the cell
    /// These normals are shorter than the input normals
    /// as that makes the bias weaker,  we want them to only
    /// really be important when the input is ambiguous
    /// Take a simple average of positions as the point we will
    /// pull towards.
    fn add_bias_normals(&mut self) {
        let mass_point = self.mass_point_avg();
        self.add(&mass_point, &DVec3::new(BIAS_STRENGTH, 0.0, 0.0));
        self.add(&mass_point, &DVec3::new(0.0, BIAS_STRENGTH, 0.0));
        self.add(&mass_point, &DVec3::new(0.0, 0.0, BIAS_STRENGTH));
    }

    /// Returns the completed symmetric ATA matrix
    fn ata_full(&self) -> [[f64; 3]; 3] {
        let mut full = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                full[i][j] = if i <= j { self.ata[i][j] } else { self.ata[j][i] };
            }
        }
        full
    }

    /// Returns the average mass point (centroid)
    fn mass_point_avg(&self) -> DVec3 {
        if self.num_points > 0 {
            self.mass_point / (self.num_points as f64)
        } else {
            DVec3::ZERO
        }
    }

    /// Solves a 3×3 system [A]{x} = -{b} via Gaussian elimination
    fn solve_system(&self, a: &[[f64; 3]; 3], b: &[f64; 3]) -> Option<DVec3> {
        let mut aug = [[0.0; 4]; 3];
        for i in 0..3 {
            aug[i][..3].copy_from_slice(&a[i]);
            aug[i][3] = -b[i];
        }
        for i in 0..3 {
            // pivot
            let mut piv = i;
            let mut maxv = aug[i][i].abs();
            for r in i + 1..3 {
                let v = aug[r][i].abs();
                if v > maxv {
                    piv = r;
                    maxv = v;
                }
            }
            if maxv < 1e-10 {
                return None;
            }
            aug.swap(i, piv);
            // eliminate
            for r in i + 1..3 {
                let f = aug[r][i] / aug[i][i];
                for c in i..4 {
                    aug[r][c] -= f * aug[i][c];
                }
            }
        }
        let mut x = [0.0; 3];
        for i in (0..3).rev() {
            let mut sum = aug[i][3];
            for j in i + 1..3 {
                sum -= aug[i][j] * x[j];
            }
            if aug[i][i].abs() < 1e-10 {
                return None;
            }
            x[i] = sum / aug[i][i];
        }
        Some(DVec3::new(x[0], x[1], x[2]))
    }

    /*
    /// Projects a point to within the given bounds
    fn project_to_bounds(p: DVec3, minb: DVec3, maxb: DVec3) -> DVec3 {
        DVec3::new(
            p.x.clamp(minb.x, maxb.x),
            p.y.clamp(minb.y, maxb.y),
            p.z.clamp(minb.z, maxb.z),
        )
    }
    */

    /// Finds the QEF minimizer, using active-set to respect bounds
    pub fn optimal_position(&self, minb: DVec3, maxb: DVec3) -> DVec3 {
        if self.num_points == 0 {
            return (minb + maxb) * 0.5;
        }
        let ata = self.ata_full();
        let mut solution = if let Some(sol) = self.solve_system(&ata, &self.atb) {
            sol
        } else {
            self.mass_point_avg()
        };

        // Active-set: clamp and resolve on free variables
        let mut active = [false; 3];
        loop {
            let mut violated = false;
            for i in 0..3 {
                if solution[i] < minb[i] {
                    solution[i] = minb[i]; active[i] = true; violated = true;
                } else if solution[i] > maxb[i] {
                    solution[i] = maxb[i]; active[i] = true; violated = true;
                } else if !active[i] {
                    // remains free
                }
            }
            if !violated {
                return solution;
            }
            // build reduced system for free axes
            //println!("Free axes: {:?}", active);
            //if active[0] && active[1] && active[2] {
            //    self.print_data();
            //}
            let free: Vec<usize> = (0..3).filter(|&i| !active[i]).collect();
            if free.is_empty() {
                return solution;
            }
            //return solution;
            let n = free.len();
            // build small A and b
            let mut a = vec![vec![0.0; n]; n];
            let mut b = vec![0.0; n];
            for (ii, &i) in free.iter().enumerate() {
                b[ii] = self.atb[i];
                
                // Set up the system matrix correctly
                for (jj, &j) in free.iter().enumerate() {
                    a[ii][jj] = ata[i][j];
                }
                
                // Subtract contribution from fixed vars (ONCE per row)
                for k in 0..3 {
                    if active[k] {
                        b[ii] -= ata[i][k] * solution[k];
                    }
                }
                
                b[ii] = -b[ii];
            }
            // solve reduced via simple Gaussian elimination
            if let Some(x_free) = Self::solve_reduced(&a, &b) {
                for (ii, &i) in free.iter().enumerate() {
                    solution[i] = x_free[ii];
                }
                // loop to re-check bounds
                continue;
            }
            // if reduced singular, bail to mass point
            return self.mass_point_avg();
        }
    }

    /// Solve n×n system via Gaussian elimination (in-place vectors)
    fn solve_reduced(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
        let n = b.len();
        let mut aug = vec![vec![0.0; n + 1]; n];
        for i in 0..n {
            aug[i][..n].copy_from_slice(&a[i]);
            aug[i][n] = b[i];
        }
        for i in 0..n {
            // pivot
            let mut piv = i;
            let mut maxv = aug[i][i].abs();
            for r in i + 1..n {
                let v = aug[r][i].abs();
                if v > maxv {
                    piv = r; maxv = v;
                }
            }
            if maxv < 1e-10 {
                return None;
            }
            aug.swap(i, piv);
            for r in i + 1..n {
                let f = aug[r][i] / aug[i][i];
                for c in i..=n {
                    aug[r][c] -= f * aug[i][c];
                }
            }
        }
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut sum = aug[i][n];
            for j in i + 1..n {
                sum -= aug[i][j] * x[j];
            }
            if aug[i][i].abs() < 1e-10 {
                return None;
            }
            x[i] = sum / aug[i][i];
        }
        Some(x)
    }

    /// Evaluates quadratic error at a point
    pub fn evaluate_error(&self, p: &DVec3) -> f64 {
        let a = self.ata_full();
        let mut err = self.btb;
        for i in 0..3 {
            for j in 0..3 {
                err += p[i] * a[i][j] * p[j];
            }
            err -= 2.0 * p[i] * self.atb[i];
        }
        err
    }
}

/// Computes the optimal position within a cell using active-set QEF
pub fn compute_optimal_position(
    intersections: &[DVec3],
    normals: &[DVec3],
    min_bound: DVec3,
    max_bound: DVec3,
) -> DVec3 {
    if intersections.is_empty() || intersections.len() != normals.len() {
        return (min_bound + max_bound) * 0.5;
    }
    let mut solver = QefSolver::new();
    for (p, n) in intersections.iter().zip(normals.iter()) {
        solver.add(p, n);
    }
    solver.add_bias_normals();
    let bound_margin = DVec3::new(0.01, 0.01, 0.01);
    solver.optimal_position(min_bound - bound_margin, max_bound + bound_margin)
}
