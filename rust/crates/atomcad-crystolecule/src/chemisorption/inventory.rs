//! What a candidate changes, by kind: the multiset of bonds formed and broken,
//! by element pair and order.
//!
//! An inventory is topology, not energy. Two candidates' UFF energies compare
//! cleanly only when their inventories are equal — a different bond graph
//! shifts the UFF energy for reasons that are not strain — which is why the
//! `chemisorb` node can filter its listing by inventory. It is never a ranking
//! key.

use crate::atomic_constants::element_symbol;
use std::collections::BTreeMap;
use std::fmt;

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

/// The multiset of bonds formed and broken, by element pair and order. Its
/// [`Display`](fmt::Display) form is the label a listing filter matches on.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BondInventory {
    pub formed: BTreeMap<BondKind, usize>,
    pub broken: BTreeMap<BondKind, usize>,
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
