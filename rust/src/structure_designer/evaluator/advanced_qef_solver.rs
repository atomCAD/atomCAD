// Port of advanced QEF solver from C++ to Rust
// Original implementation uses SVD for calculation of feature points from hermite data

use glam::{DMat3, DVec3, DVec4};

// Constants
const SVD_NUM_SWEEPS: usize = 5;
const TINY_NUMBER: f64 = 1.0e-20;

/// Calculates Givens rotation coefficients for symmetric matrix elements
fn givens_coeffs_sym(a_pp: f64, a_pq: f64, a_qq: f64) -> (f64, f64) {
    if a_pq == 0.0 {
        return (1.0, 0.0);
    }
    let tau = (a_qq - a_pp) / (2.0 * a_pq);
    let stt = (1.0 + tau * tau).sqrt();
    let tan = 1.0 / if tau >= 0.0 { tau + stt } else { tau - stt };
    let c = (1.0 + tan * tan).recip().sqrt();
    let s = tan * c;
    (c, s)
}

/// Rotates a vector in the XY plane
/// Returns the updated values for x and y
fn svd_rotate_xy(x: f64, y: f64, c: f64, s: f64) -> (f64, f64) {
    let new_x = c * x - s * y;
    let new_y = s * x + c * y;
    (new_x, new_y)
}

/// Rotates quadratics in the XY plane
/// Returns the updated values for x, y, and a
fn svd_rotateq_xy(x: f64, y: f64, a: f64, c: f64, s: f64) -> (f64, f64, f64) {
    let cc = c * c;
    let ss = s * s;
    let mx = 2.0 * c * s * a;
    let new_x = cc * x - mx + ss * y;
    let new_y = ss * x + mx + cc * y;
    // a becomes zero after rotation
    (new_x, new_y, 0.0)
}

/// Performs a Givens rotation on the specified elements
fn svd_rotate(vtav: &mut [[f64; 3]; 3], v: &mut [[f64; 3]; 3], a: usize, b: usize) {
    if vtav[a][b] == 0.0 {
        return;
    }
    
    let (c, s) = givens_coeffs_sym(vtav[a][a], vtav[a][b], vtav[b][b]);
    
    // Extract values first
    let a_a_val = vtav[a][a];
    let b_b_val = vtav[b][b];
    let a_b_val = vtav[a][b];
    
    // Apply rotation and update
    let (new_aa, new_bb, new_ab) = svd_rotateq_xy(a_a_val, b_b_val, a_b_val, c, s);
    vtav[a][a] = new_aa;
    vtav[b][b] = new_bb;
    vtav[a][b] = new_ab;
    
    // Calculate indices for the off-diagonal element
    let i0 = match (a, b) {
        (0, 1) => 0,
        (0, 2) => 0,
        (1, 2) => 1,
        _ => panic!("Invalid rotation indices"),
    };
    
    let j0 = match (a, b) {
        (0, 1) => 2,
        (0, 2) => 1,
        (1, 2) => 0,
        _ => panic!("Invalid rotation indices"),
    };
    
    // Extract values
    let off1 = vtav[i0][j0];
    let off2 = vtav[j0][i0];
    
    // Apply rotation and update
    let (new_off1, new_off2) = svd_rotate_xy(off1, off2, c, s);
    vtav[i0][j0] = new_off1;
    vtav[j0][i0] = new_off2;
    
    // Rotate V matrix columns
    for i in 0..3 {
        let v_ia = v[i][a];
        let v_ib = v[i][b];
        let (new_via, new_vib) = svd_rotate_xy(v_ia, v_ib, c, s);
        v[i][a] = new_via;
        v[i][b] = new_vib;
    }
}

/// Solves a symmetric 3x3 matrix using SVD
fn svd_solve_sym(a: [[f64; 3]; 3]) -> (DVec3, [[f64; 3]; 3]) {
    // Assuming that A is symmetric: can optimize all operations for 
    // the upper right triangular
    let mut vtav = a;
    // Initialize V as identity matrix
    let mut v = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0]
    ];
    
    // Perform Jacobi iterations
    for _ in 0..SVD_NUM_SWEEPS {
        svd_rotate(&mut vtav, &mut v, 0, 1);
        svd_rotate(&mut vtav, &mut v, 0, 2);
        svd_rotate(&mut vtav, &mut v, 1, 2);
    }
    
    let sigma = DVec3::new(vtav[0][0], vtav[1][1], vtav[2][2]);
    (sigma, v)
}

/// Calculates inverse of value with tolerance check
fn svd_invdet(x: f64, tol: f64) -> f64 {
    if x.abs() < tol || (1.0 / x).abs() < tol {
        0.0
    } else {
        1.0 / x
    }
}

/// Computes pseudoinverse using SVD results
fn svd_pseudoinverse(sigma: &DVec3, v: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let d0 = svd_invdet(sigma[0], TINY_NUMBER);
    let d1 = svd_invdet(sigma[1], TINY_NUMBER);
    let d2 = svd_invdet(sigma[2], TINY_NUMBER);
    
    [
        [
            v[0][0] * d0 * v[0][0] + v[0][1] * d1 * v[0][1] + v[0][2] * d2 * v[0][2],
            v[0][0] * d0 * v[1][0] + v[0][1] * d1 * v[1][1] + v[0][2] * d2 * v[1][2],
            v[0][0] * d0 * v[2][0] + v[0][1] * d1 * v[2][1] + v[0][2] * d2 * v[2][2],
        ],
        [
            v[1][0] * d0 * v[0][0] + v[1][1] * d1 * v[0][1] + v[1][2] * d2 * v[0][2],
            v[1][0] * d0 * v[1][0] + v[1][1] * d1 * v[1][1] + v[1][2] * d2 * v[1][2],
            v[1][0] * d0 * v[2][0] + v[1][1] * d1 * v[2][1] + v[1][2] * d2 * v[2][2],
        ],
        [
            v[2][0] * d0 * v[0][0] + v[2][1] * d1 * v[0][1] + v[2][2] * d2 * v[0][2],
            v[2][0] * d0 * v[1][0] + v[2][1] * d1 * v[1][1] + v[2][2] * d2 * v[1][2],
            v[2][0] * d0 * v[2][0] + v[2][1] * d1 * v[2][1] + v[2][2] * d2 * v[2][2],
        ],
    ]
}

/// Solves A^T*A*x = A^T*b system using SVD
fn svd_solve_ata_atb(ata: [[f64; 3]; 3], atb: DVec3) -> DVec3 {
    let (sigma, v) = svd_solve_sym(ata);
    let vinv = svd_pseudoinverse(&sigma, &v);
    
    // Matrix-vector multiplication
    DVec3::new(
        vinv[0][0] * atb.x + vinv[0][1] * atb.y + vinv[0][2] * atb.z,
        vinv[1][0] * atb.x + vinv[1][1] * atb.y + vinv[1][2] * atb.z,
        vinv[2][0] * atb.x + vinv[2][1] * atb.y + vinv[2][2] * atb.z,
    )
}

/// Vector multiplication with symmetric matrix
fn svd_vmul_sym(a: &[[f64; 3]; 3], v: &DVec3) -> DVec3 {
    DVec3::new(
        a[0][0] * v.x + a[0][1] * v.y + a[0][2] * v.z,
        a[0][1] * v.x + a[1][1] * v.y + a[1][2] * v.z,
        a[0][2] * v.x + a[1][2] * v.y + a[2][2] * v.z,
    )
}

/// Computes A^T*A for a matrix A
fn svd_mul_ata_sym(a: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    [
        [
            a[0][0] * a[0][0] + a[1][0] * a[1][0] + a[2][0] * a[2][0],
            a[0][0] * a[0][1] + a[1][0] * a[1][1] + a[2][0] * a[2][1],
            a[0][0] * a[0][2] + a[1][0] * a[1][2] + a[2][0] * a[2][2],
        ],
        [
            a[0][0] * a[0][1] + a[1][0] * a[1][1] + a[2][0] * a[2][1], // same as [0][1]
            a[0][1] * a[0][1] + a[1][1] * a[1][1] + a[2][1] * a[2][1],
            a[0][1] * a[0][2] + a[1][1] * a[1][2] + a[2][1] * a[2][2],
        ],
        [
            a[0][0] * a[0][2] + a[1][0] * a[1][2] + a[2][0] * a[2][2], // same as [0][2]
            a[0][1] * a[0][2] + a[1][1] * a[1][2] + a[2][1] * a[2][2], // same as [1][2]
            a[0][2] * a[0][2] + a[1][2] * a[1][2] + a[2][2] * a[2][2],
        ],
    ]
}

/// Solves Ax = b system using SVD
fn svd_solve_ax_b(a: &[[f64; 3]; 3], b: &DVec3) -> DVec3 {
    let ata = svd_mul_ata_sym(a);
    
    // Compute A^T * b
    let atb = DVec3::new(
        b.x * a[0][0] + b.y * a[1][0] + b.z * a[2][0],
        b.x * a[0][1] + b.y * a[1][1] + b.z * a[2][1],
        b.x * a[0][2] + b.y * a[1][2] + b.z * a[2][2],
    );
    
    svd_solve_ata_atb(ata, atb)
}

/// QEF specific functions
/// ////////////////////////////////////////////////////////////////////////////////

/// Adds a point-normal constraint to the QEF system
pub fn qef_add(
    n: &DVec3,
    p: &DVec3,
    ata: &mut [[f64; 3]; 3],
    atb: &mut DVec3,
    point_accum: &mut DVec4,
) {
    ata[0][0] += n.x * n.x;
    ata[0][1] += n.x * n.y;
    ata[0][2] += n.x * n.z;
    ata[1][1] += n.y * n.y;
    ata[1][2] += n.y * n.z;
    ata[2][2] += n.z * n.z;

    let b = p.dot(*n);
    *atb += *n * b;
    *point_accum += DVec4::new(p.x, p.y, p.z, 1.0);
}

/// Calculates error for a given solution
pub fn qef_calc_error(a: &[[f64; 3]; 3], x: &DVec3, b: &DVec3) -> f64 {
    let vtmp = *b - svd_vmul_sym(a, x);
    vtmp.dot(vtmp)
}

/// Solves the QEF system and returns the error
pub fn qef_solve(
    ata: &[[f64; 3]; 3],
    atb: &DVec3,
    point_accum: &DVec4,
) -> (DVec3, f64) {
    if point_accum.w == 0.0 {
        return (DVec3::ZERO, 0.0);
    }

    let mass_point = DVec3::new(
        point_accum.x / point_accum.w,
        point_accum.y / point_accum.w,
        point_accum.z / point_accum.w,
    );
    
    // Adjust ATb to account for mass point
    let adjusted_atb = *atb - svd_vmul_sym(ata, &mass_point);
    
    // Solve the system
    let mut x = svd_solve_ata_atb(*ata, adjusted_atb);
    
    // Calculate error
    let error = qef_calc_error(ata, &x, &adjusted_atb);
    
    // Add back the mass point
    x += mass_point;
    
    (x, error)
}

/// Computes the optimal position using the advanced QEF solver
pub fn compute_optimal_position_advanced(
    intersections: &[DVec3],
    normals: &[DVec3],
    min_bound: DVec3,
    max_bound: DVec3,
) -> DVec3 {
    if intersections.is_empty() || intersections.len() != normals.len() {
        return (min_bound + max_bound) * 0.5;
    }

    // Initialize QEF matrices and vectors
    let mut ata = [[0.0; 3]; 3];
    let mut atb = DVec3::ZERO;
    let mut point_accum = DVec4::ZERO;

    // Add all constraints
    for (p, n) in intersections.iter().zip(normals.iter()) {
        qef_add(n, p, &mut ata, &mut atb, &mut point_accum);
    }

    // Solve the QEF
    let (solution, _error) = qef_solve(&ata, &atb, &point_accum);
    
    // Clamp to bounds
    DVec3::new(
        solution.x.clamp(min_bound.x, max_bound.x),
        solution.y.clamp(min_bound.y, max_bound.y),
        solution.z.clamp(min_bound.z, max_bound.z),
    )
}
