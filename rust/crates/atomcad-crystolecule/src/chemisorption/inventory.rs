//! What a candidate changes, by kind: the multiset of bonds formed and broken,
//! by element pair and order.
//!
//! An inventory is topology, not energy. Two candidates' UFF energies compare
//! cleanly only when their inventories are equal — a different bond graph
//! shifts the UFF energy for reasons that are not strain — which is why a
//! search can be restricted to one inventory
//! ([`ChemisorptionSearch::bond_inventory`](super::ChemisorptionSearch)). It
//! is never a ranking key.

use crate::atomic_constants::{CHEMICAL_ELEMENTS, element_symbol};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

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
/// [`Display`](fmt::Display) form is the label users see and type; [`FromStr`]
/// reads it back.
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

impl BondInventory {
    /// Bonds formed, transfers' included.
    pub fn formed_count(&self) -> usize {
        self.formed.values().sum()
    }

    /// Bonds broken. Only a transfer breaks a bond, so this is also the number
    /// of transfers.
    pub fn broken_count(&self) -> usize {
        self.broken.values().sum()
    }
}

/// Why a label is not a bond inventory.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{label}' is not a bond inventory: {reason}")]
pub struct InventoryParseError {
    pub label: String,
    pub reason: String,
}

impl FromStr for BondInventory {
    type Err = InventoryParseError;

    /// The [`Display`](fmt::Display) form, back: `"formed 1× H–Si, 1× O–Si;
    /// broken 1× H–O"` or `"no bond changes"`. Every bond is single, as the
    /// search forms and breaks only single bonds. Lenient about spaces, and
    /// accepts `x` for `×` and `-` for `–`, so a typed label works too.
    fn from_str(label: &str) -> Result<Self, Self::Err> {
        let fail = |reason: String| InventoryParseError {
            label: label.to_string(),
            reason,
        };
        let mut inventory = BondInventory::default();
        let text = label.trim();
        if text == "no bond changes" {
            return Ok(inventory);
        }
        for part in text.split(';') {
            let part = part.trim();
            let (side, rest) = part
                .split_once(char::is_whitespace)
                .ok_or_else(|| fail(format!("expected 'formed …' or 'broken …', got '{part}'")))?;
            let target = match side {
                "formed" => &mut inventory.formed,
                "broken" => &mut inventory.broken,
                other => {
                    return Err(fail(format!(
                        "expected 'formed' or 'broken', got '{other}'"
                    )));
                }
            };
            for item in rest.split(',') {
                let item = item.trim();
                let (count, pair) = item
                    .split_once(['×', 'x'])
                    .ok_or_else(|| fail(format!("expected '<n>× A–B', got '{item}'")))?;
                let count: usize = count
                    .trim()
                    .parse()
                    .map_err(|_| fail(format!("'{}' is not a count", count.trim())))?;
                let (a, b) = pair.trim().split_once(['–', '-']).ok_or_else(|| {
                    fail(format!(
                        "expected an element pair like 'O–Si', got '{}'",
                        pair.trim()
                    ))
                })?;
                let element = |symbol: &str| {
                    CHEMICAL_ELEMENTS
                        .get(symbol.trim())
                        .map(|&z| z as i16)
                        .ok_or_else(|| fail(format!("unknown element '{}'", symbol.trim())))
                };
                let kind = BondKind::new(element(a)?, element(b)?, 1);
                if count == 0 {
                    return Err(fail(format!("a count of 0 for {}", kind.pair_label())));
                }
                *target.entry(kind).or_insert(0) += count;
            }
        }
        Ok(inventory)
    }
}
