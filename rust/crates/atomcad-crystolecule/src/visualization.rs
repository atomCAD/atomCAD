//! The one visualization mode this crate itself has to reason about.
//!
//! `crystolecule` is otherwise independent of rendering concerns (see
//! `AGENTS.md`), and this module does not weaken that: it holds no rendering
//! code, only the mode enum that `AtomicStructure::hit_test` needs in order to
//! decide whether bonds are pickable at all. Everything else about how atoms
//! are drawn stays in `display` / `renderer`.
//!
//! `doc/design_rust_crate_split.md` D6 moves this type down out of `api/`,
//! where it used to be one of the two `crystolecule → api` back-edges. Two
//! consequences worth knowing:
//!
//! - A same-named, Dart-facing **twin** stays in the root crate's
//!   `api/structure_designer/structure_designer_preferences.rs`, with `From`
//!   impls both ways (D9a). The twin keeps the name because that is what the
//!   generated Dart already declares; conversion happens at the `api/`
//!   boundary.
//! - `display::preferences` used to declare its *own* duplicate of this enum
//!   and now re-exports this one instead, so the domain side has one definition
//!   rather than two.

/// How atoms and bonds are drawn — and, for `hit_test`, whether bonds are
/// pickable (space-filling atoms touch, so their bonds are hidden and, absent a
/// per-atom render-style override, unpickable).
/// `Serialize`/`Deserialize` are here because this enum is a *persisted user
/// preference* (`structure_designer::preferences`, written to
/// `preferences.json`) as well as a `hit_test` input. The variant names are
/// therefore load-bearing: renaming one silently invalidates the field in every
/// existing preferences file.
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AtomicStructureVisualization {
    #[default]
    BallAndStick,
    SpaceFilling,
}
