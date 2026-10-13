//! The pre-checks (§3.3): cheap invariants that reject a pair before any
//! motion is tried.
//!
//! Every one is **exact as a rejection** — it rejects only pairs no matching
//! within the tolerance can make equal — so a pre-check can cost a pair its
//! motion search but never a merge. Each takes the precomputed invariants of
//! the two clusters and returns `Ok(())` or the reason it rejected.

use super::cluster::Cluster;
use super::{Motion, PreCheck, Rejection};
use crate::atomic_constants::element_symbol;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Slack on the two numeric bounds, so that rounding in the invariants of an
/// exactly-at-the-bound pair cannot reject it. Å or Å².
const ROUNDING_SLACK: f64 = 1e-9;

/// Every pre-check in order of cost, the bond ones only when bonds are
/// compared. The first that rejects decides.
pub fn pre_check(
    a: &Cluster,
    b: &Cluster,
    motion: Motion,
    tolerance: f64,
    compare_bonds: bool,
) -> Result<(), Rejection> {
    element_counts(a, b)?;
    if compare_bonds {
        bond_counts(a, b)?;
        labels(a, b)?;
    }
    eigenvalues(a, b, motion, tolerance)?;
    distances(a, b, tolerance)
}

/// Equal element counts: necessary under every motion.
pub fn element_counts(a: &Cluster, b: &Cluster) -> Result<(), Rejection> {
    if a.element_counts == b.element_counts {
        return Ok(());
    }
    let mut reason = String::from("element counts differ:");
    for (element, (na, nb)) in count_differences(&a.element_counts, &b.element_counts) {
        let _ = write!(reason, " {} {na}/{nb}", element_symbol(element));
    }
    Err(Rejection {
        check: PreCheck::ElementCounts,
        reason,
    })
}

/// Equal bond counts by kind (element pair and order). A matching that agrees
/// on every bond maps the bonds between cluster atoms one to one, kind for
/// kind.
pub fn bond_counts(a: &Cluster, b: &Cluster) -> Result<(), Rejection> {
    if a.bond_counts == b.bond_counts {
        return Ok(());
    }
    let mut reason = String::from("bond counts differ:");
    for ((e1, e2, order), (na, nb)) in count_differences(&a.bond_counts, &b.bond_counts) {
        let _ = write!(
            reason,
            " {}–{} {} {na}/{nb}",
            element_symbol(e1),
            element_symbol(e2),
            order_name(order)
        );
    }
    Err(Rejection {
        check: PreCheck::BondCounts,
        reason,
    })
}

/// Equal multisets of atom labels (element plus bonded neighbours' elements).
/// A matching that agrees on every bond maps each atom to one with the same
/// label.
pub fn labels(a: &Cluster, b: &Cluster) -> Result<(), Rejection> {
    if a.label_counts == b.label_counts {
        return Ok(());
    }
    let differing = count_differences(&a.label_counts, &b.label_counts);
    let mut reason = String::from("bonded neighbourhoods differ:");
    for (label, (na, nb)) in differing.into_iter().take(4) {
        let neighbours: Vec<String> = label
            .neighbours
            .iter()
            .map(|&e| element_symbol(e))
            .collect();
        let _ = write!(
            reason,
            " {} bonded to [{}] {na}/{nb};",
            element_symbol(label.element),
            neighbours.join(" ")
        );
    }
    reason.pop();
    Err(Rejection {
        check: PreCheck::Labels,
        reason,
    })
}

/// The eigenvalues of the second-moment tensor about the centroid.
///
/// The least-squares fit of an equal pair puts the centroids together, so each
/// centred position moves by `|dᵢ| ≤ t`, and the tensors differ by at most
/// `E = Σ (2·t·|aᵢ| + t²)` in the spectral norm; by Weyl's inequality no
/// eigenvalue moves by more. Under `none` there is no fit to bring the
/// centroids together: each position moves by `t`, the centroid by up to `t`,
/// so a centred position by up to `2t`, and the bound is taken at `2t`.
pub fn eigenvalues(
    a: &Cluster,
    b: &Cluster,
    motion: Motion,
    tolerance: f64,
) -> Result<(), Rejection> {
    let t = match motion {
        Motion::None => 2.0 * tolerance,
        _ => tolerance,
    };
    let n = a.len() as f64;
    let bound = 2.0 * t * a.radius_sum + n * t * t;
    let slack = ROUNDING_SLACK * (1.0 + a.moment_values[2].abs());
    for k in 0..3 {
        let difference = (a.moment_values[k] - b.moment_values[k]).abs();
        if difference > bound + slack {
            return Err(Rejection {
                check: PreCheck::Eigenvalues,
                reason: format!(
                    "shapes differ: second moment {:.3}/{:.3} Å² (at most {:.3} apart for equal clusters)",
                    a.moment_values[k], b.moment_values[k], bound
                ),
            });
        }
    }
    Ok(())
}

/// The sorted lists of all pairwise distances. Every motion considered
/// preserves distances, a matching within `t` changes each by at most `2t`,
/// and sorting is 1-Lipschitz in the max norm.
pub fn distances(a: &Cluster, b: &Cluster, tolerance: f64) -> Result<(), Rejection> {
    let bound = 2.0 * tolerance + ROUNDING_SLACK;
    if a.sorted_distances.len() != b.sorted_distances.len() {
        // Unreachable after the element counts; kept so this check is exact on
        // its own when called directly.
        return Err(Rejection {
            check: PreCheck::Distances,
            reason: format!("atom counts differ: {}/{}", a.len(), b.len()),
        });
    }
    let worst = a
        .sorted_distances
        .iter()
        .zip(&b.sorted_distances)
        .map(|(x, y)| (x - y).abs())
        .enumerate()
        .max_by(|x, y| x.1.total_cmp(&y.1));
    match worst {
        Some((k, difference)) if difference > bound => Err(Rejection {
            check: PreCheck::Distances,
            reason: format!(
                "interatomic distances differ: {:.3}/{:.3} Å (rank {} of {})",
                a.sorted_distances[k],
                b.sorted_distances[k],
                k + 1,
                a.sorted_distances.len()
            ),
        }),
        _ => Ok(()),
    }
}

fn count_differences<K: Ord + Clone>(
    a: &BTreeMap<K, usize>,
    b: &BTreeMap<K, usize>,
) -> Vec<(K, (usize, usize))> {
    let mut keys: Vec<&K> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter_map(|key| {
            let na = a.get(key).copied().unwrap_or(0);
            let nb = b.get(key).copied().unwrap_or(0);
            (na != nb).then(|| (key.clone(), (na, nb)))
        })
        .collect()
}

fn order_name(order: u8) -> String {
    match order {
        1 => "single".into(),
        2 => "double".into(),
        3 => "triple".into(),
        4 => "quadruple".into(),
        5 => "aromatic".into(),
        6 => "dative".into(),
        7 => "metallic".into(),
        other => format!("order {other}"),
    }
}
