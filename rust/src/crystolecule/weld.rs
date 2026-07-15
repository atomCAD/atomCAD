//! `weld_coincident_atoms` — fuse atoms that occupy the same position into one.
//!
//! This is the single new core primitive the surface-reconstruction patch
//! feature rests on (see `doc/design_surface_patches.md` §3). Laying out tile
//! copies on a lattice makes every shared/ghost atom land on its neighbour's
//! corresponding atom; welding those coincident atoms turns each
//! boundary-crossing bond into an ordinary intra-structure bond. The *same*
//! weld fuses a tile's collar atoms onto the surviving substrate, so the merged
//! atom inherits the bulk's outward bonds. No diff machinery is involved — the
//! tile is a real structure, not a delta.

use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::crystolecule::atomic_structure::atom::ATOM_FLAG_PATCH_GHOST;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// A minimal union-find over atom ids (1-indexed, possibly sparse).
struct UnionFind {
    parent: FxHashMap<u32, u32>,
}

impl UnionFind {
    fn new() -> Self {
        Self {
            parent: FxHashMap::default(),
        }
    }

    fn find(&mut self, x: u32) -> u32 {
        let p = *self.parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let root = self.find(p);
        self.parent.insert(x, root);
        root
    }

    fn union(&mut self, a: u32, b: u32) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            // Attach the larger id under the smaller so the survivor (the
            // cluster's minimum id) is also the root — keeps things deterministic.
            let (root, child) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.parent.insert(child, root);
        }
    }
}

/// Fuse atoms that occupy the same position (within `tolerance`) into one.
///
/// The surviving atom (the lowest id in each coincident cluster) unions the
/// bond lists of every fused atom (dedup by partner; on a duplicate the bond
/// orders must agree — a mismatch is a bug by construction and panics) and
/// unions their flags and tag bits. The survivor keeps the patch-ghost flag (bit 6) only if
/// *every* fused atom was a patch-ghost; any real atom in the cluster makes the
/// survivor real (the flag is cleared).
///
/// `tolerance` must be well below the smallest interatomic spacing so distinct
/// sites never over-merge (0.1 Å is safely below bond lengths).
pub fn weld_coincident_atoms(structure: &mut AtomicStructure, tolerance: f64) {
    // --- 1. Cluster coincident atoms via union-find over the spatial grid. ---
    let positions: Vec<(u32, glam::f64::DVec3)> = structure
        .iter_atoms()
        .map(|(id, atom)| (*id, atom.position))
        .collect();

    let mut uf = UnionFind::new();
    for (id, pos) in &positions {
        // Seed every atom as its own singleton so it appears in `parent`.
        uf.find(*id);
        for neighbor in structure.get_atoms_in_radius(pos, tolerance) {
            uf.union(*id, neighbor);
        }
    }

    // root -> members
    let mut clusters: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for (id, _) in &positions {
        let root = uf.find(*id);
        clusters.entry(root).or_default().push(*id);
    }

    // Nothing coincident → nothing to do (avoid rebuilding bonds needlessly).
    if clusters.values().all(|members| members.len() == 1) {
        return;
    }

    // survivor_of: every atom id (clustered or singleton) → its cluster survivor
    // (the minimum id in the cluster, which is also the union-find root).
    let mut survivor_of: FxHashMap<u32, u32> = FxHashMap::default();
    for (root, members) in &clusters {
        for member in members {
            survivor_of.insert(*member, *root);
        }
    }

    // --- 2. Build the desired bond set for each survivor (remap partners). ---
    // desired[survivor][partner_survivor] = bond_order
    let mut desired: FxHashMap<u32, BTreeMap<u32, u8>> = FxHashMap::default();
    for (id, atom) in structure.iter_atoms() {
        let s = survivor_of[id];
        for bond in &atom.bonds {
            let partner = survivor_of[&bond.other_atom_id()];
            if partner == s {
                // Bond between two atoms of the same cluster (e.g. the
                // coincidence pair itself) collapses to a self-bond — drop it.
                continue;
            }
            let order = bond.bond_order();
            let entry = desired.entry(s).or_default();
            match entry.get(&partner) {
                Some(&existing) if existing != order => {
                    panic!(
                        "weld_coincident_atoms: conflicting bond order between atoms \
                         {s} and {partner}: {existing} vs {order}"
                    );
                }
                _ => {
                    entry.insert(partner, order);
                }
            }
        }
    }

    // --- 3. Compute the unioned flags and tags for each multi-atom cluster's
    //        survivor. Singleton clusters keep theirs unchanged, so we only
    //        record the ones we actually merge. ---
    // survivor -> flags
    let mut survivor_flags: FxHashMap<u32, u16> = FxHashMap::default();
    // survivor -> tag_bits. Weld runs within one structure, so every member's
    // mask shares one table and the OR is exact (name-level remap not needed).
    let mut survivor_tag_bits: FxHashMap<u32, u32> = FxHashMap::default();
    for (root, members) in &clusters {
        if members.len() == 1 {
            continue;
        }
        let mut union_flags: u16 = 0;
        let mut union_tag_bits: u32 = 0;
        let mut all_patch_ghost = true;
        for member in members {
            let atom = structure.get_atom(*member).expect("clustered atom exists");
            union_flags |= atom.flags;
            union_tag_bits |= atom.tag_bits;
            all_patch_ghost &= atom.is_patch_ghost();
        }
        // The union already set bit 6 if *any* member was a patch-ghost; clear
        // it unless *every* member was, so a real twin makes the survivor real.
        if !all_patch_ghost {
            union_flags &= !ATOM_FLAG_PATCH_GHOST;
        }
        survivor_flags.insert(*root, union_flags);
        survivor_tag_bits.insert(*root, union_tag_bits);
    }

    // --- 4. Mutate: delete non-survivors, then rebuild bonds and flags. ---
    let to_delete: Vec<u32> = survivor_of
        .iter()
        .filter_map(|(member, survivor)| (member != survivor).then_some(*member))
        .collect();
    for id in to_delete {
        // delete_atom strips this atom's bonds from its (surviving) partners;
        // those bonds are re-added below from `desired`.
        structure.delete_atom(id);
    }

    for (survivor, partners) in &desired {
        for (partner, order) in partners {
            // Both endpoints are survivors and therefore still present.
            // add_bond_checked is idempotent: a surviving original bond is
            // simply re-confirmed with the (matching) order.
            structure.add_bond_checked(*survivor, *partner, *order);
        }
    }

    for (survivor, flags) in &survivor_flags {
        structure.set_atom_flags(*survivor, *flags);
    }

    for (survivor, tag_bits) in &survivor_tag_bits {
        structure.set_atom_tag_bits(*survivor, *tag_bits);
    }
}
