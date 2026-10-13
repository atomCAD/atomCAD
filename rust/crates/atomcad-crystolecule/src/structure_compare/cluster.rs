//! The cluster: the atoms a comparison looks at, cut out as a small structure
//! of their own, with everything about it that does not depend on the other
//! side computed once (§3.2, §3.3).

use super::CompareError;
use crate::atomic_structure::AtomicStructure;
use crate::rigid_fit::symmetric_eigen_3;
use glam::{DMat3, DVec3};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// An atom's place in the cluster's bond graph, as far as a matching that
/// agrees on every bond must preserve it: its element and the sorted elements
/// of its bonded neighbours inside the cluster (so also its bond count, the
/// length of that list).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Label {
    pub element: i16,
    pub neighbours: Vec<i16>,
}

/// One kind of bond: the two elements (smaller first) and the order.
pub type BondKind = (i16, i16, u8);

/// The chosen atoms of a structure as a small structure of their own —
/// positions, elements and the bonds *between* them only. Bonds leaving the
/// cluster are dropped: what they say about the environment is included by
/// choosing more atoms.
///
/// Atoms are indexed `0..len()` in ascending order of their id in the source
/// structure, which makes everything downstream deterministic.
#[derive(Debug, Clone)]
pub struct Cluster {
    source_ids: Vec<u32>,
    positions: Vec<DVec3>,
    elements: Vec<i16>,
    /// `(i, j, order)` with `i < j`, sorted.
    bonds: Vec<(usize, usize, u8)>,
    /// The cut as an `AtomicStructure` (atom `i` has id `i + 1`), for the final
    /// check, which runs on structures.
    structure: AtomicStructure,

    // The invariants (§3.3), computed once.
    pub(super) labels: Vec<Label>,
    pub(super) element_counts: BTreeMap<i16, usize>,
    pub(super) bond_counts: BTreeMap<BondKind, usize>,
    pub(super) label_counts: BTreeMap<Label, usize>,
    pub(super) centroid: DVec3,
    /// The second-moment tensor's eigenvalues, ascending, and its eigenvectors
    /// as columns in the same order: the principal axes.
    pub(super) moment_values: [f64; 3],
    pub(super) moment_axes: DMat3,
    /// `Σ |xᵢ − c|`, the factor of the eigenvalue pre-check's bound.
    pub(super) radius_sum: f64,
    pub(super) sorted_distances: Vec<f64>,
}

impl Cluster {
    /// The atoms `ids` of `structure`. Unknown ids are an error, as is an empty
    /// list: an empty cluster would make every pair equal.
    pub fn from_atoms(structure: &AtomicStructure, ids: &[u32]) -> Result<Cluster, CompareError> {
        let mut source_ids = ids.to_vec();
        source_ids.sort_unstable();
        source_ids.dedup();
        if source_ids.is_empty() {
            return Err(CompareError::EmptyCluster);
        }
        let mut index_of: FxHashMap<u32, usize> = FxHashMap::default();
        let mut positions = Vec::with_capacity(source_ids.len());
        let mut elements = Vec::with_capacity(source_ids.len());
        for (index, &id) in source_ids.iter().enumerate() {
            let atom = structure
                .get_atom(id)
                .ok_or(CompareError::UnknownAtom(id))?;
            index_of.insert(id, index);
            positions.push(atom.position);
            elements.push(atom.atomic_number);
        }
        let mut bonds = Vec::new();
        for (index, &id) in source_ids.iter().enumerate() {
            let atom = structure.get_atom(id).expect("checked above");
            for bond in &atom.bonds {
                if let Some(&other) = index_of.get(&bond.other_atom_id())
                    && index < other
                {
                    bonds.push((index, other, bond.bond_order()));
                }
            }
        }
        Ok(Cluster::new(source_ids, positions, elements, bonds))
    }

    /// The atoms of `structure` carrying `tag`. A tag no atom carries is an
    /// error, as the chemisorption search's reactive tags are.
    pub fn from_tag(structure: &AtomicStructure, tag: &str) -> Result<Cluster, CompareError> {
        let ids = structure.atoms_with_tag(tag);
        if ids.is_empty() {
            return Err(CompareError::UnknownTag(tag.to_string()));
        }
        Cluster::from_atoms(structure, &ids)
    }

    /// Every atom of `structure`.
    pub fn whole(structure: &AtomicStructure) -> Result<Cluster, CompareError> {
        let ids: Vec<u32> = structure.atom_ids().copied().collect();
        Cluster::from_atoms(structure, &ids)
    }

    /// A cluster from bare data, with `source_ids` `1..=n`; `bonds` are
    /// `(i, j, order)` over indices, either way round.
    pub fn from_parts(
        positions: Vec<DVec3>,
        elements: Vec<i16>,
        bonds: &[(usize, usize, u8)],
    ) -> Result<Cluster, CompareError> {
        if positions.is_empty() {
            return Err(CompareError::EmptyCluster);
        }
        assert_eq!(positions.len(), elements.len(), "one element per position");
        let source_ids = (1..=positions.len() as u32).collect();
        let mut normalized: Vec<(usize, usize, u8)> = bonds
            .iter()
            .map(|&(i, j, order)| (i.min(j), i.max(j), order))
            .collect();
        normalized.sort_unstable();
        normalized.dedup_by(|x, y| x.0 == y.0 && x.1 == y.1);
        Ok(Cluster::new(source_ids, positions, elements, normalized))
    }

    fn new(
        source_ids: Vec<u32>,
        positions: Vec<DVec3>,
        elements: Vec<i16>,
        mut bonds: Vec<(usize, usize, u8)>,
    ) -> Cluster {
        bonds.sort_unstable();
        let n = positions.len();

        let structure = build_structure(&positions, &elements, &bonds, |p| p);

        let mut neighbours: Vec<Vec<i16>> = vec![Vec::new(); n];
        let mut bond_counts: BTreeMap<BondKind, usize> = BTreeMap::new();
        for &(i, j, order) in &bonds {
            neighbours[i].push(elements[j]);
            neighbours[j].push(elements[i]);
            let (e1, e2) = (elements[i].min(elements[j]), elements[i].max(elements[j]));
            *bond_counts.entry((e1, e2, order)).or_default() += 1;
        }
        let labels: Vec<Label> = neighbours
            .into_iter()
            .zip(&elements)
            .map(|(mut neighbours, &element)| {
                neighbours.sort_unstable();
                Label {
                    element,
                    neighbours,
                }
            })
            .collect();
        let mut element_counts: BTreeMap<i16, usize> = BTreeMap::new();
        for &element in &elements {
            *element_counts.entry(element).or_default() += 1;
        }
        let mut label_counts: BTreeMap<Label, usize> = BTreeMap::new();
        for label in &labels {
            *label_counts.entry(label.clone()).or_default() += 1;
        }

        let centroid = positions.iter().copied().sum::<DVec3>() / n as f64;
        let mut moment = DMat3::ZERO;
        let mut radius_sum = 0.0;
        for p in &positions {
            let a = *p - centroid;
            moment += DMat3::from_cols(a * a.x, a * a.y, a * a.z);
            radius_sum += a.length();
        }
        let (moment_values, moment_axes) = symmetric_eigen_3(moment);

        let mut sorted_distances = Vec::with_capacity(n * (n - 1) / 2);
        for i in 0..n {
            for j in (i + 1)..n {
                sorted_distances.push(positions[i].distance(positions[j]));
            }
        }
        sorted_distances.sort_unstable_by(f64::total_cmp);

        Cluster {
            source_ids,
            positions,
            elements,
            bonds,
            structure,
            labels,
            element_counts,
            bond_counts,
            label_counts,
            centroid,
            moment_values,
            moment_axes,
            radius_sum,
            sorted_distances,
        }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Never true: a cluster has at least one atom.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The atom ids in the structure the cluster was cut from, by index.
    pub fn source_ids(&self) -> &[u32] {
        &self.source_ids
    }

    pub fn positions(&self) -> &[DVec3] {
        &self.positions
    }

    pub fn elements(&self) -> &[i16] {
        &self.elements
    }

    /// `(i, j, order)` with `i < j`, sorted.
    pub fn bonds(&self) -> &[(usize, usize, u8)] {
        &self.bonds
    }

    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    /// The cluster as a structure of its own; atom `i` has id `i + 1`.
    pub fn structure(&self) -> &AtomicStructure {
        &self.structure
    }

    /// The cluster as a structure with every position mapped by `motion`; atom
    /// `i` has id `i + 1`.
    pub(super) fn moved_structure(&self, motion: impl Fn(DVec3) -> DVec3) -> AtomicStructure {
        build_structure(&self.positions, &self.elements, &self.bonds, motion)
    }

    /// The order of the bond between atoms `i` and `j`, if they are bonded.
    pub fn bond_order(&self, i: usize, j: usize) -> Option<u8> {
        let key = (i.min(j), i.max(j));
        self.bonds
            .binary_search_by(|&(x, y, _)| (x, y).cmp(&key))
            .ok()
            .map(|k| self.bonds[k].2)
    }
}

fn build_structure(
    positions: &[DVec3],
    elements: &[i16],
    bonds: &[(usize, usize, u8)],
    motion: impl Fn(DVec3) -> DVec3,
) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    for (p, &element) in positions.iter().zip(elements) {
        structure.add_atom(element, motion(*p));
    }
    for &(i, j, order) in bonds {
        structure.add_bond(i as u32 + 1, j as u32 + 1, order);
    }
    structure
}
