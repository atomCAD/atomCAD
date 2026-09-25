//! The bond-energy term of the score: mean single-bond enthalpies, Pauling's
//! estimate for the pairs the table lacks, and the bond inventory they are
//! summed over.
//!
//! UFF models a bond as a spring around its rest length, so a bond at rest
//! costs nothing and UFF alone cannot say whether forming three Si–O bonds
//! beats forming two. `ΔE_bond = Σ D(broken) − Σ D(formed)` adds the missing
//! term. The values are crude by nature: a surface dimer bond and a bulk bond
//! are both "Si–Si".

use super::config::ChemisorptionError;
use crate::atomic_constants::element_symbol;
use std::collections::BTreeMap;
use std::fmt;

/// kJ/mol → kcal/mol, applied once when a table value is read.
pub const KJ_PER_KCAL: f64 = 4.184;

/// Pauling's factor in kJ/mol per unit electronegativity difference squared
/// (the classic 23 kcal/mol).
pub const PAULING_FACTOR_KJ: f64 = 96.5;

/// The elements the tables cover, in the order of their rows.
const ELEMENTS: [i16; 12] = [1, 6, 7, 8, 9, 14, 15, 16, 17, 32, 35, 53];

/// Pauling electronegativities, same order as [`ELEMENTS`]. Source: Wikipedia,
/// *Electronegativities of the elements (data page)*, Pauling scale.
const ELECTRONEGATIVITY: [f64; 12] = [
    2.20, 2.55, 3.04, 3.44, 3.98, 1.90, 2.19, 2.58, 3.16, 2.01, 2.96, 2.66,
];

/// Mean single-bond enthalpies (kJ/mol), lower triangle over [`ELEMENTS`];
/// `0.0` = not tabulated, estimated by Pauling's rule.
///
/// Copied verbatim from Appendix A of the chemisorption search design
/// (the Cengage general-chemistry bond enthalpy table). Two entries are not
/// from that source: C–Si 318 (the Huheey value; Wisconsin gives 301) and
/// Ge–Ge 186, derived from germanium's cohesive energy (3.85 eV/atom ÷ 2 bonds
/// per atom in the diamond lattice; the same derivation gives 223 for silicon
/// against the table's 222). Do not fill gaps from memory.
#[rustfmt::skip]
const ENTHALPY_KJ: [[f64; 12]; 12] = [
    //  H      C      N      O      F      Si     P      S      Cl     Ge     Br     I
    [ 436.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // H
    [ 413.0, 346.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // C
    [ 391.0, 305.0, 163.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // N
    [ 463.0, 358.0, 201.0, 146.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // O
    [ 565.0, 485.0, 283.0, 184.0, 155.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // F
    [ 318.0, 318.0,   0.0, 452.0, 565.0, 222.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0], // Si
    [ 322.0,   0.0,   0.0, 335.0, 490.0,   0.0, 201.0,   0.0,   0.0,   0.0,   0.0,   0.0], // P
    [ 347.0, 272.0,   0.0,   0.0, 284.0, 293.0,   0.0, 226.0,   0.0,   0.0,   0.0,   0.0], // S
    [ 432.0, 339.0, 192.0, 218.0, 253.0, 381.0, 326.0, 255.0, 242.0,   0.0,   0.0,   0.0], // Cl
    [   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0,   0.0, 186.0,   0.0,   0.0], // Ge
    [ 366.0, 285.0, 243.0, 201.0, 249.0, 310.0,   0.0, 213.0, 216.0,   0.0, 193.0,   0.0], // Br
    [ 299.0, 213.0,   0.0, 201.0, 278.0, 234.0, 184.0,   0.0, 208.0,   0.0, 175.0, 151.0], // I
];

fn index_of(z: i16) -> Option<usize> {
    ELEMENTS.iter().position(|&e| e == z)
}

fn unscored(z: i16) -> ChemisorptionError {
    ChemisorptionError::UnscoredElement {
        element: element_symbol(z),
    }
}

/// Whether `z` is one of the twelve elements a bond change can be scored for.
pub fn is_scored_element(z: i16) -> bool {
    index_of(z).is_some()
}

/// The tabulated mean single-bond enthalpy (kJ/mol), `None` for a gap in the
/// table (or an element outside it).
pub fn tabulated_enthalpy_kj(a: i16, b: i16) -> Option<f64> {
    let (i, j) = (index_of(a)?, index_of(b)?);
    let v = ENTHALPY_KJ[i.max(j)][i.min(j)];
    (v > 0.0).then_some(v)
}

/// Pauling's estimate `½[D(A–A) + D(B–B)] + 96.5·(χA − χB)²` (kJ/mol), from
/// the homonuclear enthalpies and electronegativities. Every element of the
/// table has both, so this fails only outside it.
pub fn pauling_estimate_kj(a: i16, b: i16) -> Result<f64, ChemisorptionError> {
    let i = index_of(a).ok_or_else(|| unscored(a))?;
    let j = index_of(b).ok_or_else(|| unscored(b))?;
    let dchi = ELECTRONEGATIVITY[i] - ELECTRONEGATIVITY[j];
    Ok(0.5 * (ENTHALPY_KJ[i][i] + ENTHALPY_KJ[j][j]) + PAULING_FACTOR_KJ * dchi * dchi)
}

/// One bond's enthalpy in kcal/mol, and whether it is a Pauling estimate.
pub fn bond_enthalpy(a: i16, b: i16) -> Result<(f64, bool), ChemisorptionError> {
    match tabulated_enthalpy_kj(a, b) {
        Some(kj) => Ok((kj / KJ_PER_KCAL, false)),
        None => Ok((pauling_estimate_kj(a, b)? / KJ_PER_KCAL, true)),
    }
}

/// An unordered element pair and a bond order: what one line of a bond
/// inventory counts. Elements are stored with the lower atomic number first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BondKind {
    pub element_a: i16,
    pub element_b: i16,
    pub order: u8,
}

impl BondKind {
    pub fn new(a: i16, b: i16, order: u8) -> Self {
        Self {
            element_a: a.min(b),
            element_b: a.max(b),
            order,
        }
    }

    /// The pair's symbols, alphabetical, e.g. `"O–Si"`.
    pub fn pair_label(&self) -> String {
        let (mut a, mut b) = (
            element_symbol(self.element_a),
            element_symbol(self.element_b),
        );
        if b < a {
            std::mem::swap(&mut a, &mut b);
        }
        format!("{a}–{b}")
    }
}

/// The multiset of bonds formed and broken, by element pair and order. It
/// alone determines a candidate's bond-energy term, so candidates with equal
/// inventories rank among themselves in strain order.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BondInventory {
    pub formed: BTreeMap<BondKind, usize>,
    pub broken: BTreeMap<BondKind, usize>,
}

impl BondInventory {
    /// `ΔE_bond = Σ D(broken) − Σ D(formed)` in kcal/mol, and whether any
    /// term is a Pauling estimate.
    pub fn bond_energy(&self) -> Result<(f64, bool), ChemisorptionError> {
        let mut total = 0.0;
        let mut estimated = false;
        for (sign, bonds) in [(1.0, &self.broken), (-1.0, &self.formed)] {
            for (kind, &count) in bonds {
                let (d, est) = bond_enthalpy(kind.element_a, kind.element_b)?;
                total += sign * d * count as f64;
                estimated |= est;
            }
        }
        Ok((total, estimated))
    }

    /// The kinds in this inventory whose enthalpy is a Pauling estimate.
    pub fn estimated_kinds(&self) -> impl Iterator<Item = &BondKind> {
        self.formed
            .keys()
            .chain(self.broken.keys())
            .filter(|k| tabulated_enthalpy_kj(k.element_a, k.element_b).is_none())
    }
}

impl fmt::Display for BondInventory {
    /// `"formed 3× O–Si"`, `"formed 1× H–Si, 1× O–Si; broken 1× H–O"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |bonds: &BTreeMap<BondKind, usize>| {
            let mut items: Vec<(String, usize)> =
                bonds.iter().map(|(k, &n)| (k.pair_label(), n)).collect();
            items.sort();
            items
                .into_iter()
                .map(|(label, n)| format!("{n}× {label}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut parts = Vec::new();
        if !self.formed.is_empty() {
            parts.push(format!("formed {}", list(&self.formed)));
        }
        if !self.broken.is_empty() {
            parts.push(format!("broken {}", list(&self.broken)));
        }
        if parts.is_empty() {
            f.write_str("no bond changes")
        } else {
            f.write_str(&parts.join("; "))
        }
    }
}
