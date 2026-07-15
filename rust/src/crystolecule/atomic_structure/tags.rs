//! Per-atom **tag** primitives for [`AtomicStructure`](super::AtomicStructure):
//! the error type and the cross-structure bit remap. The interned name table
//! (`tag_names`) and every accessor live on `AtomicStructure` itself (see
//! `super`); this module holds only the two supporting value types.
//!
//! A tag is a user-chosen name attached to a set of atoms — inert, durable
//! metadata that downstream consumers *interpret* (visual rules, region
//! selectors). Storage is an interned name table plus a per-atom bitmask
//! (`Atom.tag_bits`); bit indices are **per-structure**, so every
//! cross-structure operation translates masks through a [`TagRemap`] built by
//! name. See `doc/design_atom_tags.md`.

use thiserror::Error;

/// Maximum number of distinct tag names a single structure can hold. Equals the
/// bit width of [`Atom::tag_bits`](super::atom::Atom::tag_bits).
pub const MAX_TAGS: usize = 32;

/// Error returned by the fallible tag operations (interning and, transitively,
/// the name-level merge in `add_atomic_structure`).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TagError {
    /// All [`MAX_TAGS`] slots are *live* (each carried by at least one atom),
    /// so no new name can be interned even after slot reclamation.
    #[error("tag limit ({MAX_TAGS} names) reached")]
    LimitReached,
    /// The supplied name was empty (or all-whitespace) after trimming.
    #[error("tag name is empty")]
    EmptyName,
}

/// Cross-structure tag-bit translation: source bit index → target bit index.
///
/// Built once per (source → target) pair by
/// [`AtomicStructure::build_tag_remap`](super::AtomicStructure::build_tag_remap)
/// and passed to every per-atom mask copy. This type is the compiler-enforced
/// carrier of the per-structure bit-index invariant: any code path that moves
/// atoms between structures must obtain one, so a raw cross-table bit copy
/// cannot happen by omission.
#[derive(Debug, Clone)]
pub struct TagRemap {
    /// `map[i]` is the target bit for source bit `i`, or `None` when source bit
    /// `i` has no target slot (its name was dropped — those names are the ones
    /// reported alongside the remap by `build_tag_remap`).
    pub(super) map: [Option<u8>; MAX_TAGS],
}

impl TagRemap {
    /// The identity remap — every source bit maps to the same target bit.
    ///
    /// Correct for same-structure copies (the mask passes through unchanged)
    /// and for merges whose two tables are already identical (the cheap common
    /// case: clone-mutate diffs and repeated merges of sibling structures).
    pub fn identity() -> Self {
        let mut map = [None; MAX_TAGS];
        for (i, slot) in map.iter_mut().enumerate() {
            *slot = Some(i as u8);
        }
        Self { map }
    }
}
