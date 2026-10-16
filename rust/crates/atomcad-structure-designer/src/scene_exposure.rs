//! Baked ambient occlusion, cached on the scene
//! (`doc/design_depth_perception.md` D9).
//!
//! The exposure of a structure's atoms (D6, D7) costs a sizeable fraction of
//! materialize on a large structure, so it is paid on the same terms as
//! evaluation: once per evaluated output, then carried along.
//!
//! - Each [`NodeSceneData`] holds an [`ExposureCache`], one entry per atomic
//!   output pin: the values, the [`OccluderSnapshot`] they were computed for,
//!   and the occluder changes from that snapshot to the current structure,
//!   not yet applied to them.
//! - [`StructureDesigner::update_exposure`] fills the entries, **between
//!   evaluation and tessellation**. The tessellator only reads them, and treats
//!   an absent value as fully exposed.
//! - When a re-evaluated node replaces its older scene entry,
//!   [`carry_over_exposure`] moves the old cache into the new entry and diffs
//!   the snapshot against the new structure, so an edit that moved a few
//!   atoms recomputes only the receivers near them.
//!
//! Two things differ from the letter of D9, and both follow from one fact:
//! **atoms are matched by place, not by id.** `atom_edit`'s `apply_diff`
//! renumbers its result atoms whenever its diff changes (moving one atom of
//! a 1 810-atom block changed 699 ids), so an id-keyed diff would report
//! most of the structure as changed on every atom_edit edit. The diff is
//! `occlusion_changes_by_place`, and the values are re-keyed to the new ids
//! through the id map it returns.
//!
//! - The snapshot stays at the state the values were computed for until an
//!   update runs, and the diff always starts from it. So each diff is
//!   complete, and it **replaces** the pending set; D9's merge would only
//!   grow that set by the moved atoms' positions on every deferred drag tick,
//!   without changing which values are dropped.
//! - The values are kept keyed by the snapshot's ids, and the re-keyed copy
//!   the tessellator reads is rebuilt from them on each carry-over.

use crate::preferences::AtomicRenderingMethod;
use crate::structure_designer::StructureDesigner;
use crate::structure_designer_scene::{NodeOutput, NodeSceneData};
use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_display::ambient_occlusion::{
    AtomExposure, OccluderSnapshot, OcclusionChanges, atom_exposure, occluder_snapshot,
    occlusion_changes_by_place, plan_exposure_update,
};
use atomcad_display::atomic_tessellator::ao_receivers;
use atomcad_display::preferences::{AtomicStructureVisualizationPreferences, DisplayPreferences};
use std::collections::HashMap;
use std::time::Instant;

/// The exposure of one atomic output pin.
#[derive(Debug, Default)]
struct ExposureEntry {
    /// The packed `(4b, E)` of each receiver that has a value, keyed by the
    /// ids of `snapshot`.
    values: AtomExposure,
    /// The occluder state `values` was computed for. `None` until the first
    /// computation.
    snapshot: Option<OccluderSnapshot>,
    /// The occluder changes from `snapshot` to the current structure, matched
    /// by place; not yet applied to `values`.
    pending: OcclusionChanges,
    /// `values` re-keyed to the current structure's ids, when those differ
    /// from the snapshot's.
    current: Option<AtomExposure>,
    /// The display preferences this entry was last brought up to date for.
    /// While they are unchanged and the entry was not carried over, the
    /// structure and its receivers are the same, so the pass skips the entry
    /// without scanning its atoms.
    up_to_date_for: Option<AtomicStructureVisualizationPreferences>,
}

impl ExposureEntry {
    fn is_filled(&self) -> bool {
        self.snapshot.is_some()
    }

    /// The values, keyed by the current structure's ids.
    fn current_values(&self) -> &AtomExposure {
        self.current.as_ref().unwrap_or(&self.values)
    }
}

/// The baked exposure of a scene entry's atomic outputs, keyed by output pin
/// index. A plain field of [`NodeSceneData`]: written by
/// [`StructureDesigner::update_exposure`] and [`carry_over_exposure`] only.
#[derive(Debug, Default)]
pub struct ExposureCache {
    entries: HashMap<i32, ExposureEntry>,
}

impl ExposureCache {
    /// The values of output pin `pin_index`, if they were ever computed.
    pub fn values(&self, pin_index: i32) -> Option<&AtomExposure> {
        self.entries
            .get(&pin_index)
            .filter(|entry| entry.is_filled())
            .map(ExposureEntry::current_values)
    }

    /// The number of changed places pending on pin `pin_index` (0 when there
    /// is no entry).
    pub fn pending_change_count(&self, pin_index: i32) -> usize {
        self.entries
            .get(&pin_index)
            .map_or(0, |entry| entry.pending.len())
    }

    /// Whether no pin has computed values.
    pub fn is_empty(&self) -> bool {
        !self.entries.values().any(ExposureEntry::is_filled)
    }

    /// Bytes held: about 8 per value and 24 per snapshot atom (D9).
    pub fn memory_usage_bytes(&self) -> usize {
        self.entries
            .values()
            .map(|entry| {
                std::mem::size_of::<ExposureEntry>()
                    + entry.values.memory_usage_bytes()
                    + entry
                        .current
                        .as_ref()
                        .map_or(0, AtomExposure::memory_usage_bytes)
                    + entry
                        .snapshot
                        .as_ref()
                        .map_or(0, OccluderSnapshot::memory_usage_bytes)
                    + entry.pending.len() * 32
            })
            .sum()
    }
}

/// What the exposure machinery did during one refresh, summed over every
/// output (Testing layer 1). Kept per `StructureDesigner`, never in a global:
/// `cargo test` runs a harness's tests on parallel threads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneExposureStats {
    /// Receivers whose exposure was computed.
    pub receivers_computed: usize,
    /// Updates (or first computations) that took the full path.
    pub full_path_updates: usize,
    /// Occluder diffs run by [`carry_over_exposure`].
    pub diffs_run: usize,
    /// Updates not run because an interaction was open and they would have
    /// taken the full path (D9, point 3).
    pub deferred_updates: usize,
}

/// The structure on output pin `pin_index` of `data`, if it is atomic.
fn atomic_output(data: &NodeSceneData, pin_index: i32) -> Option<&AtomicStructure> {
    match data.pin_output(pin_index)? {
        NodeOutput::Atomic(structure, _) => Some(structure),
        _ => None,
    }
}

/// Moves `old`'s exposure into `new`, the scene entry replacing it. The
/// pending changes become the place-matched diff from the entry's snapshot
/// to the new structure, which replaces the previous pending set (it starts
/// from the same snapshot, so it is complete), and the values are re-keyed
/// to the new structure's ids. The snapshot is not advanced: the values still
/// correspond to it, and only an update moves it on.
///
/// Every place that inserts a `NodeSceneData` over an older one goes through
/// here; call it only with ambient occlusion on. Returns the number of
/// occluder diffs run.
pub fn carry_over_exposure(old: ExposureCache, new: &mut NodeSceneData) -> usize {
    let mut diffs_run = 0;
    for (pin_index, mut entry) in old.entries {
        let Some(snapshot) = &entry.snapshot else {
            continue;
        };
        // A pin that is no longer atomic has nothing to carry to.
        let Some(structure) = atomic_output(new, pin_index) else {
            continue;
        };
        let (changes, renumbering) = occlusion_changes_by_place(snapshot, structure);
        diffs_run += 1;
        entry.current = (!renumbering.keeps_ids()).then(|| entry.values.remapped(&renumbering));
        entry.pending = changes;
        entry.up_to_date_for = None;
        new.exposure.entries.insert(pin_index, entry);
    }
    diffs_run
}

impl StructureDesigner {
    /// Whether ambient occlusion is computed at all, read from this
    /// designer's own preferences: the checkbox, and atoms drawn as
    /// impostors (the only drawing that reads exposure). The refresh paths
    /// use it to decide whether to carry exposure over.
    pub fn ambient_occlusion_active(&self) -> bool {
        self.preferences.rendering_preferences.ambient_occlusion
            && self
                .preferences
                .atomic_structure_visualization_preferences
                .rendering_method
                == AtomicRenderingMethod::Impostors
    }

    /// Brings every displayed atomic output's exposure up to date for
    /// `prefs` (D9). Runs between [`refresh`](Self::refresh) and
    /// tessellation; returns its time in ms, together with that of the
    /// carry-over the preceding `refresh` ran, or `None` when it returned at
    /// once — on a lightweight refresh, or with ambient occlusion off.
    ///
    /// Per output: an entry with a value for every receiver and no pending
    /// changes is kept; otherwise the pending changes are applied
    /// (`update_atom_exposure`) or, on an empty entry, everything is computed.
    /// While an interaction is open, an update that would take the full path
    /// is deferred: the old values stay in use and the pending changes keep
    /// accumulating until the first update after the interaction closes.
    pub fn update_exposure(
        &mut self,
        prefs: &DisplayPreferences,
        lightweight: bool,
    ) -> Option<f64> {
        let viz_prefs = &prefs.atomic_structure_visualization;
        if lightweight || !viz_prefs.ambient_occlusion_active() {
            return None;
        }
        let start = Instant::now();
        let interaction_open = self.open_interaction().is_some();
        let mut stats = SceneExposureStats::default();

        for data in self
            .last_generated_structure_designer_scene
            .node_data
            .values_mut()
        {
            // Split borrow: the outputs are read while the cache is written.
            let NodeSceneData {
                output,
                pin_outputs,
                displayed_pins,
                exposure,
                ..
            } = data;
            let use_fallback = pin_outputs.is_empty();
            let displayed: Vec<(i32, &AtomicStructure)> = if use_fallback {
                match output {
                    NodeOutput::Atomic(structure, _) => vec![(0, &*structure)],
                    _ => Vec::new(),
                }
            } else {
                pin_outputs
                    .iter()
                    .filter(|p| displayed_pins.contains(&p.pin_index))
                    .filter_map(|p| {
                        let pin_output = if p.pin_index == 0 {
                            &*output
                        } else {
                            &p.output
                        };
                        match pin_output {
                            NodeOutput::Atomic(structure, _) => Some((p.pin_index, structure)),
                            _ => None,
                        }
                    })
                    .collect()
            };

            for (pin_index, structure) in displayed {
                let entry = exposure.entries.entry(pin_index).or_default();
                if entry.up_to_date_for.as_ref() == Some(viz_prefs) {
                    continue;
                }
                let receivers = ao_receivers(structure, viz_prefs);
                if update_entry(entry, structure, &receivers, interaction_open, &mut stats) {
                    entry.up_to_date_for = Some(viz_prefs.clone());
                }
            }
        }

        self.last_exposure_stats.receivers_computed += stats.receivers_computed;
        self.last_exposure_stats.full_path_updates += stats.full_path_updates;
        self.last_exposure_stats.deferred_updates += stats.deferred_updates;
        // The carry-over that ran inside `refresh` is part of this cost.
        Some(start.elapsed().as_secs_f64() * 1000.0 + self.exposure_carry_over_ms)
    }
}

/// One output's step of [`StructureDesigner::update_exposure`]. Returns
/// whether the entry is now up to date (false when the update was deferred).
fn update_entry(
    entry: &mut ExposureEntry,
    structure: &AtomicStructure,
    receivers: &[u32],
    interaction_open: bool,
    stats: &mut SceneExposureStats,
) -> bool {
    if !entry.is_filled() {
        // Nothing to compute (no receivers: a diff structure, or every atom a
        // ghost); the entry stays empty rather than paying for a snapshot.
        if receivers.is_empty() {
            return true;
        }
        // A first computation is a full one.
        if interaction_open {
            stats.deferred_updates += 1;
            return false;
        }
        let (values, computed) = atom_exposure(structure, receivers);
        stats.receivers_computed += computed.receivers_computed;
        stats.full_path_updates += 1;
        *entry = ExposureEntry {
            values,
            snapshot: Some(occluder_snapshot(structure)),
            pending: OcclusionChanges::default(),
            current: None,
            up_to_date_for: None,
        };
        return true;
    }

    let has_changes = !entry.pending.is_empty();
    let current = entry.current_values();
    if !has_changes && receivers.iter().all(|id| current.contains(*id)) {
        return true;
    }
    let plan = plan_exposure_update(current, &entry.pending, structure, receivers);
    if interaction_open && plan.takes_full_path() {
        stats.deferred_updates += 1;
        return false;
    }
    let (values, computed) = plan.run(structure);
    stats.receivers_computed += computed.receivers_computed;
    if computed.full_path {
        stats.full_path_updates += 1;
    }
    // The new values are keyed by the current ids. Without changes and
    // without renumbering, the snapshot still describes this structure.
    if has_changes || entry.current.is_some() {
        entry.snapshot = Some(occluder_snapshot(structure));
    }
    entry.values = values;
    entry.pending = OcclusionChanges::default();
    entry.current = None;
    true
}
