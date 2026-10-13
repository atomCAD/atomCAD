//! `rigid_fit.rs` on its own: the fit had been tested only through its callers
//! (mechanosynth placement and tool poses, chemisorption seating), and the
//! symmetric eigensolver it shares with the principal axes of
//! `structure_compare` is new.

use crate::test_rng::Rng;
use atomcad_crystolecule::rigid_fit::{rigid_fit, symmetric_eigen_3};
use glam::{DMat3, DVec3};

fn random_symmetric(rng: &mut Rng) -> DMat3 {
    let q = DMat3::from_quat(rng.rotation());
    let d = DMat3::from_diagonal(DVec3::new(
        rng.range(-5.0, 5.0),
        rng.range(-5.0, 5.0),
        rng.range(-5.0, 5.0),
    ));
    q * d * q.transpose()
}

fn max_abs(m: DMat3) -> f64 {
    m.to_cols_array().iter().fold(0.0f64, |a, x| a.max(x.abs()))
}

fn assert_decomposes(m: DMat3) {
    let (values, vectors) = symmetric_eigen_3(m);
    assert!(
        values[0] <= values[1] && values[1] <= values[2],
        "ascending: {values:?}"
    );
    let orthonormality = vectors.transpose() * vectors - DMat3::IDENTITY;
    assert!(max_abs(orthonormality) < 1e-10, "orthonormal: {vectors:?}");
    let rebuilt = vectors * DMat3::from_diagonal(DVec3::from_array(values)) * vectors.transpose();
    assert!(
        max_abs(rebuilt - m) < 1e-9 * (1.0 + max_abs(m)),
        "reconstructs {m:?}"
    );
}

#[test]
fn symmetric_eigen_3_decomposes_random_matrices() {
    let mut rng = Rng::new(11);
    for _ in 0..500 {
        assert_decomposes(random_symmetric(&mut rng));
    }
}

#[test]
fn symmetric_eigen_3_handles_repeated_eigenvalues() {
    let mut rng = Rng::new(12);
    assert_decomposes(DMat3::IDENTITY);
    assert_decomposes(DMat3::ZERO);
    assert_decomposes(DMat3::from_diagonal(DVec3::new(2.0, 2.0, -1.0)));
    for _ in 0..100 {
        let q = DMat3::from_quat(rng.rotation());
        let a = rng.range(-3.0, 3.0);
        let b = rng.range(-3.0, 3.0);
        for d in [
            DVec3::new(a, a, b),
            DVec3::new(b, a, a),
            DVec3::new(a, a, a),
        ] {
            let m = q * DMat3::from_diagonal(d) * q.transpose();
            assert_decomposes(m);
            let (values, _) = symmetric_eigen_3(m);
            let mut expected = d.to_array();
            expected.sort_by(f64::total_cmp);
            for k in 0..3 {
                assert!((values[k] - expected[k]).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn symmetric_eigen_3_is_bit_identical_on_repeated_calls() {
    let mut rng = Rng::new(13);
    for _ in 0..50 {
        let m = random_symmetric(&mut rng);
        let (v1, e1) = symmetric_eigen_3(m);
        let (v2, e2) = symmetric_eigen_3(m);
        assert_eq!(v1.map(f64::to_bits), v2.map(f64::to_bits));
        assert_eq!(
            e1.to_cols_array().map(f64::to_bits),
            e2.to_cols_array().map(f64::to_bits)
        );
    }
}

#[test]
fn rigid_fit_recovers_a_proper_and_an_improper_motion() {
    let mut rng = Rng::new(14);
    for _ in 0..50 {
        let local: Vec<DVec3> = (0..6)
            .map(|_| {
                DVec3::new(
                    rng.range(-2.0, 2.0),
                    rng.range(-2.0, 2.0),
                    rng.range(-2.0, 2.0),
                )
            })
            .collect();
        let r = DMat3::from_quat(rng.rotation());
        let t = DVec3::new(
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
            rng.range(-5.0, 5.0),
        );
        let reflection = DMat3::from_diagonal(DVec3::new(-1.0, 1.0, 1.0));
        for (rotation, mirrored) in [(r, false), (r * reflection, true)] {
            let world: Vec<DVec3> = local.iter().map(|p| rotation * *p + t).collect();
            let fit = rigid_fit(&local, &world, mirrored).unwrap();
            assert!(fit.residual < 1e-9, "residual {}", fit.residual);
            assert!(max_abs(fit.r - rotation) < 1e-9);
            assert!((fit.t - t).length() < 1e-9);
            // The other handedness cannot fit a non-planar set.
            let wrong = rigid_fit(&local, &world, !mirrored).unwrap();
            assert!(wrong.residual > 1e-3);
        }
    }
}
