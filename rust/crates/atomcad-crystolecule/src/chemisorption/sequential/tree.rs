//! The search tree `plan` records while it enumerates: one row per path of
//! (foot, site) choices, with its counts and near misses and no structures.
//! It is what the debug view shows (§6.5 of the design).

use super::setup::Leg;

/// The parent of the root.
pub const NO_ROW: u32 = u32::MAX;

/// What a row stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// The posed molecule.
    Root,
    /// A leg-1 foot (index into `Setup::feet`): its children are the anchors.
    Foot(u32),
    /// One more leg bonded: `(foot, site)` indices into `Setup::feet` /
    /// `Setup::sites`, and the acceptor its transferred atom went to.
    Leg {
        foot: u32,
        site: u32,
        acceptor: Option<u32>,
    },
}

/// A site that failed this row's children's test by at most
/// [`NEAR_MISS_BAND`](super::setup::NEAR_MISS_BAND), and by how much.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearMiss {
    pub foot: u32,
    pub site: u32,
    /// Å beyond the test's bound.
    pub miss: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub parent: u32,
    pub kind: RowKind,
    /// Legs bonded along the path to this row.
    pub legs: u8,
    /// The hypothesis this row found, when it is the first path to its change
    /// set (index into `SequentialPlan::hypotheses`).
    pub hypothesis: Option<u32>,
    /// The canonical row of the same change set, when this path is a
    /// duplicate. A duplicate row has no children.
    pub duplicate_of: Option<u32>,
    /// Children rejected because their site had no valence left.
    pub rejected_valence: u32,
    /// Children rejected because their transferred atom found no acceptor.
    pub rejected_no_acceptor: u32,
    /// Children rejected by the `bond_inventory` filter.
    pub rejected_filter: u32,
    /// Children that are duplicates (hidden by default in the panel).
    pub duplicates: u32,
    near_start: u32,
    near_len: u32,
}

/// The whole tree, rows in creation (depth-first) order; row 0 is the root.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchTree {
    rows: Vec<Row>,
    near_misses: Vec<NearMiss>,
    child_start: Vec<u32>,
    children: Vec<u32>,
}

impl SearchTree {
    pub(crate) fn push(&mut self, parent: u32, kind: RowKind, legs: u8) -> u32 {
        self.rows.push(Row {
            parent,
            kind,
            legs,
            hypothesis: None,
            duplicate_of: None,
            rejected_valence: 0,
            rejected_no_acceptor: 0,
            rejected_filter: 0,
            duplicates: 0,
            near_start: 0,
            near_len: 0,
        });
        (self.rows.len() - 1) as u32
    }

    pub(crate) fn row_mut(&mut self, row: u32) -> &mut Row {
        &mut self.rows[row as usize]
    }

    /// Records a row's near misses, all at once (its children's rows are
    /// created in between, so they are collected first).
    pub(crate) fn set_near_misses(&mut self, row: u32, misses: Vec<NearMiss>) {
        let start = self.near_misses.len() as u32;
        self.near_misses.extend(misses);
        let r = &mut self.rows[row as usize];
        r.near_start = start;
        r.near_len = self.near_misses.len() as u32 - start;
    }

    /// Builds the child index; called once, when the tree is complete.
    pub(crate) fn finish(&mut self) {
        let n = self.rows.len();
        let mut count = vec![0u32; n + 1];
        for r in &self.rows {
            if r.parent != NO_ROW {
                count[r.parent as usize + 1] += 1;
            }
        }
        for i in 0..n {
            count[i + 1] += count[i];
        }
        let mut fill = count.clone();
        self.children = vec![0; count[n] as usize];
        for (i, r) in self.rows.iter().enumerate() {
            if r.parent != NO_ROW {
                let p = r.parent as usize;
                self.children[fill[p] as usize] = i as u32;
                fill[p] += 1;
            }
        }
        self.child_start = count;
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row(&self, row: u32) -> &Row {
        &self.rows[row as usize]
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Every child of `row`, duplicates included, in creation order.
    pub fn children(&self, row: u32) -> &[u32] {
        let r = row as usize;
        &self.children[self.child_start[r] as usize..self.child_start[r + 1] as usize]
    }

    /// The children the panel lists: without duplicates unless asked for.
    pub fn visible_children(&self, row: u32, show_duplicates: bool) -> Vec<u32> {
        self.children(row)
            .iter()
            .copied()
            .filter(|&c| show_duplicates || self.rows[c as usize].duplicate_of.is_none())
            .collect()
    }

    pub fn near_misses(&self, row: u32) -> &[NearMiss] {
        let r = &self.rows[row as usize];
        &self.near_misses[r.near_start as usize..(r.near_start + r.near_len) as usize]
    }

    /// The legs bonded along the path to `row`, in binding order.
    pub fn path(&self, row: u32) -> Vec<Leg> {
        let mut out = Vec::new();
        let mut r = row;
        while r != NO_ROW {
            let row = &self.rows[r as usize];
            if let RowKind::Leg { foot, site, .. } = row.kind {
                out.push(Leg {
                    foot: foot as usize,
                    site: site as usize,
                });
            }
            r = row.parent;
        }
        out.reverse();
        out
    }
}
