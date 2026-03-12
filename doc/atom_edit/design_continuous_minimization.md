# Continuous Minimization Design

Real-time energy minimization that runs frame-by-frame while the user drags selected atoms, giving immediate visual feedback of neighboring atoms relaxing around the drag.

## Motivation

Today, energy minimization is a discrete action: the user clicks "Minimize" and waits for the result. When building complex structures, a tighter feedback loop is desirable — the user drags an atom and sees the surrounding structure relax in real time. This is a well-established paradigm in molecular editors (Avogadro's Auto-Optimize, PyMOL's Sculpting, VMD's Interactive Molecular Dynamics).

## When Minimization Runs

Continuous minimization has two phases:

1. **During drag**: A small number of steepest descent steps (default 4) run each pointer-move frame, giving real-time visual feedback of neighbors relaxing.

2. **Settle burst on drag end**: When the user releases the mouse, an additional burst of steepest descent steps (default 50) runs before the drag operation finalizes. This lets the structure "settle" into a cleaner geometry, since the per-frame steps during fast dragging may not be enough for full relaxation.

The settle burst uses the same algorithm (steepest descent) but **removes all drag-specific constraints**: selected atoms are neither frozen nor spring-restrained. Since the user has released the mouse, there is no cursor position to constrain to — the entire structure (selected + neighbors) relaxes freely toward a local minimum. Only atoms with the persistent frozen flag remain frozen. Because it's steepest descent with a limited step count rather than a full L-BFGS batch minimize, the settling is gradual and produces no jarring "snap" to a distant minimum.

Both phases run inside the `begin_atom_edit_drag` / `end_atom_edit_drag` recording session, so the entire drag + relaxation + settling reverts as a single undo operation.

### Why not a full batch minimize on drag end?

A full L-BFGS minimize could move atoms far from where the user placed them, producing a disorienting visual jump. Steepest descent with a limited step count makes incremental progress — atoms move a bit further toward a minimum but don't teleport. The user can always run a manual "Minimize" afterward if they want full convergence.

### Why not continuous optimization while idle (Avogadro style)?

Avogadro's Auto-Optimize runs steepest descent every frame even when the user isn't interacting. This gives the most polished feel but requires a persistent animation loop / timer in Flutter that fires continuously, draining CPU/battery. It also raises questions about when to stop. Our approach (drag + settle) gets the primary benefit without the architectural complexity — the structure relaxes when the user is actively editing, which is when it matters most.

## Two Methods

Two continuous minimization methods are supported, selectable via a preference:

### Method 1: Constrained Minimization (Simple)

The dragged/selected atoms are **hard-frozen** at the cursor position. A few minimization steps run on all other non-frozen atoms each frame.

- Dragged atoms follow the mouse perfectly.
- Neighbors relax around the constraint.
- Simplest to implement; maps directly onto existing `frozen_indices` infrastructure.

### Method 2: Spring Restraint (Smooth)

Instead of hard-freezing dragged atoms, a **harmonic spring** pulls each selected atom toward its cursor-imposed target position:

```
E_restraint(i) = 0.5 * k * |r_i - r_target_i|^2
```

where `r_target_i` is the position the user is dragging atom `i` to, and `k` is a configurable spring constant (kcal/(mol·Å²)).

- Dragged atoms are pulled toward the cursor but can still respond to force field forces.
- Produces smoother, more physically realistic behavior.
- The atom finds a compromise between user intent and chemical geometry.
- With very large `k`, behavior approaches Method 1 (hard constraint).

### Frozen Atoms

In both methods, atoms that have the persistent **frozen flag** set (via the freeze UI) are always frozen during continuous minimization, in addition to the method-specific constraints. This is consistent with how the existing discrete minimization respects the frozen flag.

### Why Steepest Descent, Not L-BFGS

The existing L-BFGS optimizer builds up curvature history (the `(s, y, rho)` correction pairs) that assumes a stationary objective function. During interactive dragging, the target positions change every frame, invalidating the accumulated Hessian approximation. This causes:

- Stale curvature information producing poor search directions.
- The need to reset L-BFGS memory every frame, reducing it to steepest descent anyway.
- Potential oscillation as the optimizer "remembers" the wrong landscape.

**Steepest descent** is the standard choice for interactive minimization (Avogadro uses it for Auto-Optimize). Each step is independent — no history to invalidate. With a small fixed number of steps per frame (default: 4), the structure makes incremental progress toward a minimum without overshooting. The result is fluid, predictable motion.

An alternative is **FIRE** (Fast Inertial Relaxation Engine), which converges faster than steepest descent while remaining robust to changing landscapes. FIRE could be added as a future enhancement.

## Preferences

### Enable/Disable Toggle: Per-Node on `AtomEditData`

The `continuous_minimization` boolean is stored on `AtomEditData` (per-node), not in `SimulationPreferences`. This is because:

- Enabling/disabling continuous minimization is an everyday editing action, not a one-time preference.
- Different atom_edit nodes may benefit from different settings (small molecules vs. large structures).
- It follows the same pattern as `output_diff`, `show_anchor_arrows`, etc. — per-node flags toggled from the atom_edit UI.

The toggle appears in the atom_edit panel's Energy Minimization section (next to the Minimize buttons). It uses `AtomEditToggleFlagCommand` with `AtomEditFlag::ContinuousMinimization` for undo support. It is serialized with the node data in `.cnnd` files.

### Algorithm Parameters in `SimulationPreferences`

The algorithm tuning knobs remain in `SimulationPreferences` (global preferences):

```rust
#[frb]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationPreferences {
    // ... existing fields ...

    /// Use spring restraints instead of hard constraints for dragged atoms.
    /// When false (default): dragged atoms are frozen, rest minimized (Method 1).
    /// When true: dragged atoms are pulled by harmonic springs (Method 2).
    #[frb(non_final)]
    #[serde(default)]
    pub continuous_minimization_use_springs: bool,

    /// Spring constant for restraints in kcal/(mol*A^2).
    /// Only used when continuous_minimization_use_springs is true.
    /// Higher values make dragged atoms follow the cursor more tightly.
    /// Default: 200.0 (stiff but not rigid).
    #[frb(non_final)]
    #[serde(default = "default_spring_constant")]
    pub continuous_minimization_spring_constant: f64,

    /// Number of steepest descent steps per drag frame.
    /// Higher values give more relaxation per frame but cost more CPU time.
    /// Default: 4 (matches Avogadro's default).
    #[frb(non_final)]
    #[serde(default = "default_steps_per_frame")]
    pub continuous_minimization_steps_per_frame: u32,

    /// Number of steepest descent steps to run as a "settle burst" when
    /// the user releases the mouse after dragging. Lets the structure
    /// relax further without a jarring full-minimize snap.
    /// Default: 50.
    #[frb(non_final)]
    #[serde(default = "default_settle_steps")]
    pub continuous_minimization_settle_steps: u32,
}

fn default_spring_constant() -> f64 { 200.0 }
fn default_steps_per_frame() -> u32 { 4 }
fn default_settle_steps() -> u32 { 50 }
```

All new fields use `#[serde(default)]` for backward-compatible deserialization of existing preferences files.

### Default Values

| Field | Default | Rationale |
|-------|---------|-----------|
| `continuous_minimization` (on AtomEditData) | `false` | Opt-in feature, does not change default behavior |
| `continuous_minimization_use_springs` | `false` | Simple method is the default; springs are an advanced option |
| `continuous_minimization_spring_constant` | `200.0` | Stiff enough to follow the cursor closely, soft enough for force field feedback. Comparable to Avogadro's approach |
| `continuous_minimization_steps_per_frame` | `4` | Matches Avogadro's default for Auto-Optimize. Balances responsiveness vs. CPU cost |
| `continuous_minimization_settle_steps` | `50` | Enough steps to noticeably improve geometry on release without being slow (~50 × gradient eval) |

### Spring Constant Guidelines

The spring constant `k` in kcal/(mol·Å²) controls how tightly dragged atoms follow the cursor:

- **50–100**: Loose. Atom visibly lags behind cursor. Good for seeing force field effects.
- **200**: Default. Atom follows cursor closely but can adjust for local geometry.
- **500–1000**: Very stiff. Atom nearly locked to cursor position. Approaches Method 1 behavior.

For reference, a typical C-C bond stretch force constant in UFF is ~700 kcal/(mol·Å²), so the default of 200 is softer than a covalent bond — the atom will follow the cursor but not at the expense of badly distorting local geometry.

## Implementation

### New Steepest Descent Minimizer

A new function in `simulation/minimize.rs`:

```rust
/// Performs N steps of steepest descent with a fixed step size.
///
/// Unlike L-BFGS, this has no history and is suitable for interactive use
/// where the energy landscape changes every frame.
///
/// Returns the final energy.
pub fn steepest_descent_steps(
    ff: &dyn ForceField,
    positions: &mut [f64],
    frozen: &[usize],
    num_steps: u32,
    max_displacement: f64,
) -> f64 {
    let n = positions.len();
    if n == 0 { return 0.0; }

    let mut energy = 0.0;
    let mut grad = vec![0.0; n];

    for _ in 0..num_steps {
        ff.energy_and_gradients(positions, &mut energy, &mut grad);
        zero_frozen(&mut grad, frozen);

        // Scale step so max atom displacement <= max_displacement
        let max_atom_disp = max_per_atom_displacement(&grad);
        if max_atom_disp < 1e-16 { break; } // already at minimum
        let step = max_displacement / max_atom_disp;

        // Apply: x -= step * grad
        for i in 0..n {
            positions[i] -= step * grad[i];
        }
    }

    // Final energy evaluation
    ff.energy_and_gradients(positions, &mut energy, &mut grad);
    energy
}
```

This reuses the existing `zero_frozen` and `max_per_atom_displacement` helpers.

### Spring Restraint Force Field Wrapper

A wrapper that adds harmonic restraints on top of any base force field:

```rust
/// Wraps a base ForceField and adds harmonic spring restraints
/// pulling specified atoms toward target positions.
pub struct RestrainedForceField<'a> {
    base: &'a dyn ForceField,
    /// (topology_index, target_x, target_y, target_z) for each restrained atom
    restraints: Vec<(usize, f64, f64, f64)>,
    /// Spring constant in kcal/(mol*A^2)
    spring_constant: f64,
}

impl<'a> ForceField for RestrainedForceField<'a> {
    fn energy_and_gradients(
        &self,
        positions: &[f64],
        energy: &mut f64,
        gradients: &mut [f64],
    ) {
        // Compute base energy and gradients
        self.base.energy_and_gradients(positions, energy, gradients);

        // Add restraint terms: E = 0.5 * k * |r - r_target|^2
        // dE/dx_i = k * (x_i - x_target)
        let k = self.spring_constant;
        for &(topo_idx, tx, ty, tz) in &self.restraints {
            let base = topo_idx * 3;
            let dx = positions[base]     - tx;
            let dy = positions[base + 1] - ty;
            let dz = positions[base + 2] - tz;

            *energy += 0.5 * k * (dx * dx + dy * dy + dz * dz);
            gradients[base]     += k * dx;
            gradients[base + 1] += k * dy;
            gradients[base + 2] += k * dz;
        }
    }
}
```

### Modified Drag Flow

The continuous minimization hooks into the existing drag flow in `default_tool.rs` at two points:

```
BeginDrag (on first move past threshold):
  1. begin_atom_edit_drag(sd)                    // existing: start undo recording
  2. promoted_base_atoms = HashMap::new()        // NEW: track base→diff promotions

ContinueDrag:
  1. drag_selected_by_delta(sd, delta)           // existing: move selected atoms
  2. if continuous_minimization enabled:
       continuous_minimize_during_drag(sd,       // NEW: relax neighbors per frame
           &mut promoted_base_atoms)

EndDrag (pointer_up):
  1. if continuous_minimization enabled:
       continuous_minimize_settle(sd,            // NEW: settle burst
           &mut promoted_base_atoms)
  2. end_atom_edit_drag(sd)                      // existing: finalize undo recording
  // promoted_base_atoms dropped here
```

The settle burst runs **before** `end_atom_edit_drag` so that the settling position changes are captured by the same `DiffRecorder` and included in the single undo command.

#### `continuous_minimize_during_drag`

Thin wrapper that reads preferences and delegates to the shared impl:

```rust
pub fn continuous_minimize_during_drag(
    structure_designer: &mut StructureDesigner,
    promoted_base_atoms: &mut HashMap<u32, u32>,
) -> Result<(), String> {
    let prefs = &structure_designer.preferences.simulation_preferences;
    let steps = prefs.continuous_minimization_steps_per_frame;
    let use_springs = prefs.continuous_minimization_use_springs;
    continuous_minimize_impl(
        structure_designer,
        steps,
        promoted_base_atoms,
        !use_springs,  // freeze_selected: true for Method 1, false for Method 2
        use_springs,   // use_springs
    )
}
```

#### Shared Implementation: `continuous_minimize_impl`

Located in `minimization.rs`. Both `continuous_minimize_during_drag` and `continuous_minimize_settle` delegate to this function.

```rust
/// Shared implementation for continuous minimization.
///
/// `freeze_selected`: if true, selected atoms are hard-frozen (Method 1 during drag).
/// `use_springs`: if true, selected atoms are spring-restrained (Method 2 during drag).
/// Both false during settle burst — selected atoms relax freely.
fn continuous_minimize_impl(
    structure_designer: &mut StructureDesigner,
    steps: u32,
    promoted_base_atoms: &mut HashMap<u32, u32>,
    freeze_selected: bool,
    use_springs: bool,
) -> Result<(), String> {
    let prefs = &structure_designer.preferences.simulation_preferences;
    let spring_k = prefs.continuous_minimization_spring_constant;

    // Phase 1: Gather (immutable borrows)
    let (topology, force_field, frozen_indices, selected_topo_indices, result_to_source) = {
        let atom_edit_data = get_active_atom_edit_data(structure_designer)
            .ok_or("No active atom_edit node")?;

        if atom_edit_data.output_diff {
            return Ok(()); // No-op in diff view
        }

        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        let result_structure = structure_designer
            .get_atomic_structure_from_selected_node()
            .ok_or("No result structure")?;

        let vdw_mode = if structure_designer.preferences
            .simulation_preferences.use_vdw_cutoff
        {
            VdwMode::Cutoff(6.0)
        } else {
            VdwMode::AllPairs
        };

        let topology = match &vdw_mode {
            VdwMode::AllPairs => MolecularTopology::from_structure(result_structure),
            VdwMode::Cutoff(_) => MolecularTopology::from_structure_bonded_only(result_structure),
        };

        if topology.num_atoms == 0 {
            return Ok(());
        }

        // Build selected result IDs
        let mut selected_result_ids: HashSet<u32> = HashSet::new();
        for &base_id in &atom_edit_data.selection.selected_base_atoms {
            if let Some(&rid) = eval_cache.provenance.base_to_result.get(&base_id) {
                selected_result_ids.insert(rid);
            }
        }
        for &diff_id in &atom_edit_data.selection.selected_diff_atoms {
            if let Some(&rid) = eval_cache.provenance.diff_to_result.get(&diff_id) {
                selected_result_ids.insert(rid);
            }
        }

        // Frozen indices: always include persistent-frozen atoms.
        // If freeze_selected is true (Method 1 during drag): also freeze selected atoms.
        // If freeze_selected is false (Method 2 during drag, or settle burst):
        //   selected atoms are NOT frozen.
        let frozen_indices: Vec<usize> = topology.atom_ids.iter().enumerate()
            .filter(|(_, result_id)| {
                let is_frozen_flag = result_structure
                    .get_atom(**result_id)
                    .is_some_and(|atom| atom.is_frozen());
                let is_selected = selected_result_ids.contains(result_id);

                if freeze_selected {
                    is_selected || is_frozen_flag
                } else {
                    is_frozen_flag
                }
            })
            .map(|(i, _)| i)
            .collect();

        // For Method 2: build restraint list (selected atom targets)
        // NOTE: targets are built from topology.positions here (stale).
        // They are re-computed from the patched positions array in Phase 1b,
        // after position patching has applied current diff positions.
        let selected_topo_indices: Vec<usize> = if use_springs {
            topology.atom_ids.iter().enumerate()
                .filter(|(_, rid)| selected_result_ids.contains(rid))
                .map(|(topo_idx, _)| topo_idx)
                .collect()
        } else {
            Vec::new()
        };

        let force_field = UffForceField::from_topology_with_frozen(
            &topology, vdw_mode, &frozen_indices
        )?;

        let result_to_source: Vec<Option<AtomSource>> = topology.atom_ids.iter()
            .map(|&rid| eval_cache.provenance.sources.get(&rid).cloned())
            .collect();

        (topology, force_field, frozen_indices, selected_topo_indices, result_to_source)
    };

    // Phase 1b: Patch stale positions (immutable borrow of atom_edit_data)
    // The topology was built from the stale result_structure. Patch all atoms
    // that have current diff positions (selected atoms + neighbors moved by
    // previous frames). See "Evaluation Concern: Stale eval_cache" section.
    let mut positions = topology.positions.clone();
    {
        let atom_edit_data = get_active_atom_edit_data(structure_designer)
            .ok_or("No active atom_edit node")?;
        let eval_cache = structure_designer
            .get_selected_node_eval_cache()
            .ok_or("No eval cache")?
            .downcast_ref::<AtomEditEvalCache>()
            .ok_or("Wrong eval cache type")?;

        for (topo_idx, result_id) in topology.atom_ids.iter().enumerate() {
            // First: check if this atom has a diff entry via provenance
            if let Some(source) = eval_cache.provenance.sources.get(result_id) {
                let current_pos = match source {
                    AtomSource::DiffAdded(diff_id)
                    | AtomSource::DiffMatchedBase { diff_id, .. } => {
                        atom_edit_data.diff.get_atom(*diff_id)
                            .map(|a| a.position)
                    }
                    AtomSource::BasePassthrough(base_id) => {
                        // Check if promoted in a previous frame
                        promoted_base_atoms.get(base_id)
                            .and_then(|&diff_id| {
                                atom_edit_data.diff.get_atom(diff_id)
                                    .map(|a| a.position)
                            })
                    }
                };
                if let Some(pos) = current_pos {
                    let base = topo_idx * 3;
                    positions[base]     = pos.x;
                    positions[base + 1] = pos.y;
                    positions[base + 2] = pos.z;
                }
            }
        }
    }

    // Save pre-minimization positions for the movement threshold check
    // and for anchor positions when promoting BasePassthrough atoms.
    // These are the patched (current) positions, not the stale topology positions.
    let pre_minimize_positions = positions.clone();

    // Phase 2: Minimize (no borrows on structure_designer)
    // Build spring restraints from patched (current) positions
    let selected_restraints: Vec<(usize, f64, f64, f64)> = selected_topo_indices
        .iter()
        .map(|&topo_idx| {
            let base = topo_idx * 3;
            (topo_idx, positions[base], positions[base + 1], positions[base + 2])
        })
        .collect();

    if use_springs && !selected_restraints.is_empty() {
        // Method 2: steepest descent with spring-restrained force field
        let restrained_ff = RestrainedForceField {
            base: &force_field,
            restraints: selected_restraints,
            spring_constant: spring_k,
        };
        steepest_descent_steps(
            &restrained_ff, &mut positions, &frozen_indices, steps, 0.1,
        );
    } else {
        // Method 1: steepest descent with selected atoms frozen
        steepest_descent_steps(
            &force_field, &mut positions, &frozen_indices, steps, 0.1,
        );
    }

    // Phase 3: Write back (mutable borrow)
    // We use the *_recorded methods because the DiffRecorder is active
    // (started by begin_atom_edit_drag). This ensures all minimization-
    // induced position changes are captured and will coalesce with the
    // drag deltas into a single undo command on end_atom_edit_drag.
    let atom_edit_data = get_selected_atom_edit_data_mut(structure_designer)
        .ok_or("No active atom_edit node")?;

    for (topo_idx, source) in result_to_source.iter().enumerate() {
        let new_pos = DVec3::new(
            positions[topo_idx * 3],
            positions[topo_idx * 3 + 1],
            positions[topo_idx * 3 + 2],
        );
        let old_pos = DVec3::new(
            pre_minimize_positions[topo_idx * 3],
            pre_minimize_positions[topo_idx * 3 + 1],
            pre_minimize_positions[topo_idx * 3 + 2],
        );

        if (new_pos - old_pos).length() < 1e-6 {
            continue;
        }

        match source {
            Some(AtomSource::DiffAdded(diff_id))
            | Some(AtomSource::DiffMatchedBase { diff_id, .. }) => {
                atom_edit_data.set_position_recorded(*diff_id, new_pos);
            }
            Some(AtomSource::BasePassthrough(base_id)) => {
                // Base atom moved by minimizer — promote to diff.
                // Guard: check if this base atom was already promoted in a
                // previous frame (provenance is stale, so the same atom may
                // appear as BasePassthrough on every frame). Consult the
                // drag session's promoted_base_atoms map.
                if let Some(&existing_diff_id) = promoted_base_atoms.get(base_id) {
                    // Already promoted — just update its position
                    atom_edit_data.set_position_recorded(existing_diff_id, new_pos);
                } else {
                    // First promotion — create diff entry with anchor at the
                    // original base position (from the stale result_structure,
                    // NOT the patched position). The anchor must match the
                    // base atom's position so apply_diff can match it back.
                    let atomic_number = topology.atomic_numbers[topo_idx];
                    let original_pos = DVec3::new(
                        topology.positions[topo_idx * 3],
                        topology.positions[topo_idx * 3 + 1],
                        topology.positions[topo_idx * 3 + 2],
                    );
                    let new_diff_id = atom_edit_data.add_atom_recorded(atomic_number, new_pos);
                    atom_edit_data.set_anchor_recorded(new_diff_id, original_pos);
                    promoted_base_atoms.insert(*base_id, new_diff_id);
                }
            }
            None => {}
        }
    }

    Ok(())
}
```

### Settle Burst: `continuous_minimize_settle`

Also located in `minimization.rs`. Called once on drag end (pointer up), before `end_atom_edit_drag`:

```rust
/// Runs a burst of steepest descent steps after the user releases the mouse.
///
/// This lets the structure settle into a cleaner geometry after the drag.
/// Unlike per-frame minimization, the settle burst does NOT freeze or
/// spring-restrain the selected atoms — the user has released the mouse,
/// so there is no cursor position to constrain to. The entire structure
/// relaxes freely (only persistent-frozen atoms remain frozen).
pub fn continuous_minimize_settle(
    structure_designer: &mut StructureDesigner,
    promoted_base_atoms: &mut HashMap<u32, u32>,  // base_id → diff_id
) -> Result<(), String> {
    let settle_steps = structure_designer.preferences
        .simulation_preferences.continuous_minimization_settle_steps;

    if settle_steps == 0 {
        return Ok(());
    }

    // Reuses the shared helper with freeze_selected=false and
    // use_springs=false, so selected atoms participate freely.
    continuous_minimize_impl(
        structure_designer,
        settle_steps,
        promoted_base_atoms,
        false,  // freeze_selected
        false,  // use_springs
    )
}
```

In practice, `continuous_minimize_during_drag` and `continuous_minimize_settle` share a common implementation (`continuous_minimize_impl`) parameterized by step count, the `promoted_base_atoms` map, and constraint flags. During drag, `freeze_selected` and `use_springs` are set according to the user's preference. During settle, both are `false` — selected atoms are free to relax.

The `promoted_base_atoms` map (`HashMap<u32, u32>`, base atom ID → diff atom ID) is stored on the drag session (e.g., as a field on `PendingAtomEditDrag` or alongside it in `StructureDesigner`). It is created empty when `begin_atom_edit_drag` starts a drag, populated during write-back as `BasePassthrough` atoms are promoted, and dropped when `end_atom_edit_drag` completes. This gives it the correct lifetime — it spans all per-frame minimizations and the settle burst within a single drag gesture.

### Write-Back and Undo Integration

The continuous minimization runs **inside** the drag recording session (between `begin_atom_edit_drag` and `end_atom_edit_drag`). Both per-frame steps and the settle burst use the same `*_recorded` methods as the drag itself. This means:

- All minimization-induced position changes (per-frame + settle) are captured by the `DiffRecorder`.
- On undo, the entire drag (user movement + per-frame relaxation + settle burst) reverts as a single operation.
- No separate undo command is needed for any continuous minimization portion.

This is the correct behavior: from the user's perspective, they performed one drag gesture, and undo should revert the entire result of that gesture.

### Evaluation Concern: Stale eval_cache

A subtle issue: `continuous_minimize_during_drag` reads from `eval_cache` and `result_structure`, but these reflect the state **before** the current drag started (since drag uses `skip_downstream` mode for performance). The topology, provenance maps, and atom positions used for minimization come from this pre-drag snapshot.

However, `drag_selected_by_delta` modifies atom positions in the **diff** (not the result structure), so the topology positions read from the result structure are stale. This staleness affects **two** categories of atoms:

1. **Selected/dragged atoms**: Their positions have been updated by `drag_selected_by_delta` in the diff but not in the result structure.
2. **Non-selected atoms moved by previous minimization frames**: On frame N, the minimizer moves neighbor atoms and writes new positions to the diff. On frame N+1, the topology is rebuilt from the stale result structure, so these neighbors appear at their **original** pre-drag positions — the minimizer's accumulated progress is lost.

Both must be patched. This is implemented in **Phase 1b** of `continuous_minimize_during_drag` (see the function code above). For each atom in the topology:

- **DiffAdded / DiffMatchedBase**: Read current position directly from the diff.
- **BasePassthrough**: Consult the `promoted_base_atoms` map to find the diff entry created by a previous frame's write-back. If found, use the diff position; otherwise the atom hasn't been moved yet and the stale position is correct.

The patching modifies the cloned `positions` array (not the topology), and happens before the force field and restraint targets are built, so both see the correct geometry.

**Tracking promoted base atoms across frames:** Because provenance is stale, a `BasePassthrough` atom promoted to diff on frame N still appears as `BasePassthrough` on frame N+1. The drag session maintains a `promoted_base_atoms: HashMap<u32, u32>` map (base atom ID → diff atom ID) that is populated during write-back whenever a `BasePassthrough` is first promoted, and consulted during position patching on subsequent frames. This map is created at drag start and dropped at drag end (see Modified Drag Flow above).

### Max Displacement Per Step

The steepest descent `max_displacement` parameter is set to **0.1 Å** for continuous minimization (vs. 0.3 Å for batch minimization). This smaller step size ensures:

- Smooth, gradual relaxation (no jarring jumps).
- Stability even with rapidly changing constraints.
- Predictable visual feedback.

With 4 steps × 0.1 Å max = up to 0.4 Å total movement per frame for any atom, which is visible but not disorienting at typical frame rates.

### Performance Considerations

Continuous minimization adds computation to every drag frame. Key costs:

1. **Topology construction** (`MolecularTopology::from_structure`): O(N) where N = atoms. Already fast for typical atom_edit structures (tens to hundreds of atoms).

2. **Force field construction** (`UffForceField::from_topology_with_frozen`): O(interactions). Done once per frame.

3. **Steepest descent steps**: 4 × (energy + gradient evaluation). Each evaluation is O(bonds + angles + torsions + vdW). With vdW cutoff, this is O(N).

4. **Write-back**: O(moved atoms).

For structures up to ~500 atoms with vdW cutoff, this should complete in under 10ms per frame — well within a 16ms frame budget (60fps). For larger structures, future radius-cutoff optimization would help.

### No Re-evaluation of the Node Network

Critically, `continuous_minimize_during_drag` does **not** trigger a full node network re-evaluation. It reads the existing result structure and modifies the diff directly. The 3D viewport update happens through the existing drag refresh path (which updates atom positions without full re-evaluation). This keeps the per-frame cost low.

However, after the drag ends (`end_atom_edit_drag`), the normal refresh path runs and re-evaluates the node network with the final positions.

## API

No new API functions are needed for the core feature — continuous minimization is triggered automatically during drag when the preference is enabled.

The preferences are already exposed to Flutter via FRB (all `SimulationPreferences` fields are accessible).

### Preferences UI

The existing preferences window gains new controls in the Simulation section:

The **on/off toggle** is in the **atom_edit panel** (Energy Minimization section):

```
┌─ Energy Minimization ─────────────────────────────────────┐
│  [x] Continuous minimization during drag                    │
│                                                             │
│  [Minimize diff]  [Minimize unfrozen]  [Minimize selected]  │
│  ...                                                        │
└─────────────────────────────────────────────────────────────┘
```

The **algorithm parameters** are in the **Preferences** window (Simulation section):

```
┌─ Simulation ──────────────────────────────────────────────┐
│                                                             │
│  [x] Use vdW cutoff                                        │
│                                                             │
│  Continuous Minimization                                    │
│  [ ] Use spring restraints (smoother)                       │
│  Spring constant: [  200.0  ] kcal/(mol·Å²)                │
│  Steps per frame: [    4    ]                               │
│  Settle steps:    [   50    ]                               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Steps

### Phase 1: Steepest Descent and Spring Restraint Infrastructure

1. **`simulation/minimize.rs`** — Add `steepest_descent_steps()` function.
2. **`simulation/force_field.rs`** (or new file `simulation/restrained.rs`) — Add `RestrainedForceField` wrapper.
3. **Tests** — Unit tests in `rust/tests/crystolecule/simulation/`:
   - **Steepest descent converges on simple molecule** — distorted methane or ethane, verify energy decreases and final geometry is reasonable.
   - **Steepest descent with frozen atoms** — freeze one atom, verify it doesn't move while others do. (New code path separate from L-BFGS, needs its own frozen test.)
   - **Steepest descent early exit** — molecule already at equilibrium, verify it exits early when gradient is near zero (the `< 1e-16` check) and returns without modifying positions.
   - **Steepest descent with zero atoms** — empty position array, returns 0.0 energy.
   - **RestrainedForceField gradient numerical check** — finite-difference verification that analytical spring gradients match numerical derivatives. Same pattern as existing UFF energy tests (perturb each coordinate by epsilon, compare `(E(x+h) - E(x-h)) / 2h` to analytical gradient).
   - **RestrainedForceField with k=0** — verify output is identical to base force field (no restraint contribution to energy or gradients).
   - **RestrainedForceField with very large k** — atom stays near target position, effectively frozen.
   - **Large k spring produces same result as frozen constraint** — run the same distorted molecule through both methods: steepest descent with selected atoms frozen (Method 1) and steepest descent with selected atoms spring-restrained at k=10000 (Method 2). Verify that final positions of non-selected atoms are approximately equal (within tolerance), and that selected atoms in the spring method remain within a tight tolerance of their target positions. This is a convergence test: as k → ∞, Method 2 should converge to Method 1.

### Phase 2: Preferences

4. **`structure_designer_preferences.rs`** — Add five new fields to `SimulationPreferences` with defaults and serde helpers (`continuous_minimization`, `continuous_minimization_use_springs`, `continuous_minimization_spring_constant`, `continuous_minimization_steps_per_frame`, `continuous_minimization_settle_steps`).
5. **FRB codegen** — Regenerate bindings.
6. **Tests** — Preferences tests in `rust/tests/structure_designer/`:
   - **Serde backward compatibility** — deserialize a `SimulationPreferences` JSON that lacks the new fields, verify all defaults are applied correctly (tolerant reader pattern).
   - **Serde roundtrip** — serialize with all new fields set to non-default values, deserialize, verify they survive.

### Phase 3: Drag Integration

7. **`minimization.rs`** — Add `continuous_minimize_impl()` shared helper, `continuous_minimize_during_drag()`, and `continuous_minimize_settle()`.
8. **`default_tool.rs`** — Call `continuous_minimize_during_drag()` in `ContinueDrag` action after `drag_selected_by_delta()`, gated on the preference flag. Call `continuous_minimize_settle()` in `pointer_up` before `end_atom_edit_drag()`.
9. **Tests** — Integration tests in `rust/tests/structure_designer/`:
   - **Continuous minimization during drag — neighbors move** — enable continuous minimization, simulate a drag sequence (select atom, drag by delta), verify non-selected neighbor atoms moved toward lower energy.
   - **Settle burst improves geometry** — drag with continuous minimization, capture positions after per-frame steps, then run settle burst, verify energy decreased further and both selected and neighbor positions improved.
   - **Settle burst relaxes selected atoms** — drag an atom to a strained position (Method 1, selected frozen during drag), run settle burst, verify the selected atom moves away from the cursor position toward better geometry. Confirms that settle does not freeze selected atoms.
   - **Frozen-flagged atoms fixed during per-frame and settle** — set frozen flag on specific atoms, drag with continuous minimization, verify persistent-frozen atoms remain at their original positions throughout (during drag AND settle).
   - **Method 1 vs Method 2 both produce valid geometry** — same drag scenario with both methods, verify both result in lower energy than no minimization. Verify Method 1 keeps selected atoms exactly at cursor position during drag while Method 2 allows slight deviation. After settle, both methods allow selected atoms to move.
   - **Spring method selected atoms move toward target** — drag with Method 2, verify selected atoms end up close to (but not necessarily exactly at) the cursor-imposed target position.
   - **Continuous minimization disabled — no side effects** — drag with preference off, verify neighbor positions are unchanged (no regression on existing behavior).
   - **Diff view no-op** — set `output_diff = true`, call `continuous_minimize_during_drag`, verify it returns Ok(()) without modifying anything.
   - **Empty selection** — no atoms selected during drag, continuous minimization should be a no-op or handle gracefully without error.
   - **Stale position patching** — verify that the force field sees dragged atoms at their current (post-delta) positions, not the stale pre-drag result_structure positions. (Can be tested by checking that forces on neighbors reflect the dragged geometry.)
   - **Undo reverts entire drag + relaxation + settle** — enable continuous minimization, drag, verify positions changed, undo, verify ALL positions (dragged + neighbor-relaxed + settle-relaxed) revert to pre-drag state.
   - **Base atom promotion during continuous minimize** — a neighbor that was a `BasePassthrough` atom gets moved by the minimizer, verify it's correctly promoted to diff with anchor set to original position.

### Phase 4: Flutter UI

10. **`preferences_window.dart`** — Add checkbox and number fields for the new simulation preferences (including settle steps).
11. **Verify** drag behavior with continuous minimization enabled.

### Phase 5: Gadget Drag Integration

12. **`atom_edit_gadget.rs`** — The XYZ translation gadget also drags atoms. Hook `continuous_minimize_during_drag()` into the gadget's `sync_data` path, and `continuous_minimize_settle()` into the gadget's drag-end path, so continuous minimization works regardless of whether the user drags with click-drag or the gadget handles.

## Future Enhancements

- **Radius cutoff**: Only minimize atoms within N bonds of the selection. Dramatically reduces cost for large structures.
- **FIRE optimizer**: Replace steepest descent with FIRE for faster convergence per step while remaining robust to changing landscapes.
- **Adaptive step count**: Automatically adjust steps per frame based on frame time budget.
- **Visual energy display**: Show current energy in the status bar during drag.
- **Per-atom spring constants**: Different stiffness for different selected atoms.

## Non-Goals

- **Full molecular dynamics during drag**: We perform energy minimization (seeking a local minimum), not dynamics (integrating equations of motion with temperature). MD would add thermal noise, which is not useful for a CAD tool.
- **Automatic topology rebuild**: The topology connectivity (bonds, angles, etc.) is derived from the pre-drag result structure and does not change during a drag. Bond breaking/formation during a drag is not supported.
- **Undo granularity**: Individual minimization frames are not separately undoable. The entire drag + relaxation is one undo step.

## References

- Avogadro Auto-Optimize Tool: steepest descent, 4 steps/update, fixed atoms excluded
- PyMOL Molecular Sculpting: zone-based (green/cyan/grey), MMFF94s
- VMD/NAMD Interactive Molecular Dynamics: harmonic spring to cursor, full MD
- SAMSON: Adaptive articulated body dynamics + FIRE minimizer
- Surles 1994 "Sculpting Proteins Interactively": rigid constraints + elastic restraints + user tugs
