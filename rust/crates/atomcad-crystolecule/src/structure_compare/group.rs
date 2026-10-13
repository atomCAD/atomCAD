//! Grouping a list of clusters into classes of equal ones, greedily in the
//! given order (§3.6, D9).

use super::cluster::{BondKind, Cluster};
use super::{CompareError, CompareSettings, compare};
use rayon::prelude::*;
use std::collections::BTreeMap;

/// Element counts, and bond counts when bonds are compared: clusters in
/// different bins are never equal.
type Bin = (BTreeMap<i16, usize>, BTreeMap<BondKind, usize>);

/// The classes `group_in_order` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grouping {
    /// The listed clusters — each the first of its class — ascending.
    pub listed: Vec<usize>,
    /// Each cluster's representative: the listed cluster it was merged into,
    /// or itself when listed.
    pub class_of: Vec<usize>,
    /// How many clusters were merged into each one (0 for a copy).
    pub copies: Vec<usize>,
    /// Pairs compared.
    pub compared: usize,
    /// Pairs that hit the work cap (D5): counted as not equal.
    pub undecided: usize,
}

/// For each cluster in order: compare it with the clusters listed so far in
/// its bin (same element counts, and same bond counts when bonds are
/// compared); the lowest-index equal one is its representative, else it is
/// listed itself.
///
/// The comparisons of one cluster against the listed ones run in parallel, and
/// every one of them runs — the choice is the lowest index, and the counts
/// cover them all — so the answer does not depend on scheduling. Tolerance
/// equality is not transitive; this fixed order is what makes the classes
/// deterministic.
pub fn group_in_order(
    clusters: &[Cluster],
    settings: &CompareSettings,
) -> Result<Grouping, CompareError> {
    settings.validate()?;
    let mut bins: BTreeMap<Bin, Vec<usize>> = BTreeMap::new();
    let mut grouping = Grouping {
        listed: Vec::new(),
        class_of: Vec::with_capacity(clusters.len()),
        copies: vec![0; clusters.len()],
        compared: 0,
        undecided: 0,
    };
    for (index, cluster) in clusters.iter().enumerate() {
        let bond_key = if settings.compare_bonds {
            cluster.bond_counts.clone()
        } else {
            BTreeMap::new()
        };
        let bin = bins
            .entry((cluster.element_counts.clone(), bond_key))
            .or_default();
        let verdicts: Vec<(bool, bool)> = bin
            .par_iter()
            .map(|&listed| {
                let comparison = compare(&clusters[listed], cluster, settings)
                    .expect("settings validated above");
                (comparison.equal, comparison.undecided)
            })
            .collect();
        grouping.compared += verdicts.len();
        grouping.undecided += verdicts.iter().filter(|v| v.1).count();
        match verdicts.iter().position(|v| v.0) {
            Some(k) => {
                let representative = bin[k];
                grouping.class_of.push(representative);
                grouping.copies[representative] += 1;
            }
            None => {
                bin.push(index);
                grouping.listed.push(index);
                grouping.class_of.push(index);
            }
        }
    }
    Ok(grouping)
}
