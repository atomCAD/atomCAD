//! Viewport **overlays**: line work a node emits about its own output, drawn
//! into the wireframe pass when the preference for its kind is switched on.
//!
//! An overlay is not a gadget and not part of a node's value. A gadget is the
//! *selected* node's interactive handle set; an overlay is a fact about every
//! displayed node's output, wants no hit test and no drag, and shows for a node
//! that is merely displayed. The unit-cell wireframe was the first thing shaped
//! like this and had a field of its own on `NodeSceneData`; this is that route
//! made general, so the next overlay is a [`OverlayKind`] rather than a third
//! field.
//!
//! **Preferences affect tessellation, never evaluation.** A node emits its
//! segments whether or not the box is ticked, so toggling the preference
//! re-tessellates the scene and re-evaluates nothing, and the memoised
//! evaluator's outputs stay independent of the preferences — the invariant every
//! other preference keeps.
//!
//! Design doc: `doc/design_mechanosynth_trajectory.md` §The envelope cage.

use glam::DVec3;

/// What an overlay depicts. Each kind has its own preference switch and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    /// A `mechanosynth` tool's collision envelope: the cone from its apex to
    /// the rim where the cylinder begins, and a length of the cylinder beyond.
    ToolEnvelope,
}

/// One overlay: a kind and the line segments that draw it, in **design space**.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub kind: OverlayKind,
    /// Endpoint pairs. Not a polyline — the cage's meridians and rings share no
    /// traversal order.
    pub segments: Vec<(DVec3, DVec3)>,
}

impl Overlay {
    pub fn new(kind: OverlayKind, segments: Vec<(DVec3, DVec3)>) -> Self {
        Self { kind, segments }
    }

    /// Bytes this overlay's segments occupy on the heap, for the evaluation
    /// memo's size estimate.
    pub fn heap_bytes(&self) -> usize {
        self.segments.capacity() * std::mem::size_of::<(DVec3, DVec3)>()
    }
}
