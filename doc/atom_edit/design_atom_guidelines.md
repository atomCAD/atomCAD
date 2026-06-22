# Atom Placement Guidelines — UX Design (v1)

Issue: https://github.com/atomCAD/atomCAD/issues/368

## Purpose & scope

The `atom_edit` node gains a **guideline**: a temporary line in 3D space that
constrains atom placement to positions that are hard to hit by free clicking —
e.g. the ad-atom site of a Si(111) √3×√3 R30° reconstruction, which sits
equidistant from three surface atoms.

v1 is deliberately minimal. The following are **out of scope** by explicit
decision (all additive later if a real need appears):

- **Lines only.** No plane / sphere / ring constraints.
- **No persistence.** The guideline is transient: it is *not* serialized to
  `.cnnd` and is *not* part of the undo/redo history.
- **No dynamic dependence.** Origin and direction are a **frozen snapshot**
  taken once at setup. The guideline does *not* move if the atoms it was
  derived from later move.
- **No auto-bond.** Placing an atom never creates bonds. This feature exists
  only to *position* atoms that are otherwise hard to place; adding a bond
  afterwards with the Add Bond tool is trivial and would only pollute it.
- No composable constraints, no Miller-normal direction gizmo, no expr-based
  generic placement.
- **No draggable placement marker.** In Place sub-mode the marker is a read-only
  visual driven by the numeric field; placement is numeric-field + Add-Atom-click
  only. Dragging the marker handle along the line is additive later.
- **No group-along-line.** Dragging ≥ 2 selected atoms is unconstrained (see
  *Moving multiple selected atoms*).

## The guideline value

A guideline is `{ origin: DVec3, direction: DVec3 }` (direction normalized),
plus a current 1D **position** `t` (signed Å, measured from `origin` along
`direction`). All of it is transient state on the node.

`t` is surfaced as a numeric **position field** in the panel and as a marker on
the line in the viewport. It is the single control the user manipulates, and
what it points at depends on the selection (see *The 1D position field* below):
the position of the **selected atom** along the line (Move sub-mode), or the
position where the **next new atom** will be placed (Place sub-mode).

## Setup (selection-driven)

A **Guideline** card in the atom_edit panel shows one context-sensitive button
whose label and behavior depend on the current selection:

| Selected | Button | Frozen line |
|---|---|---|
| 3 atoms | **Equidistant line** | `origin` = circumcenter of the triangle; `direction` = triangle normal. Every point on the line is equidistant from all three atoms. |
| 2 atoms | **Center line** | `origin` = midpoint; `direction` = atom₁→atom₂ (by selection order). |
| 1 atom | **Directional line** | `origin` = the atom; `direction` = a Vec3 the user enters. A direction field appears with a "Normalize" affordance. |

The direction sign is deterministic (selection order, or the entered vector) so
the `t` coordinate is reproducible. **Degenerate input is rejected (SnackBar, no
guideline created), using tolerances rather than exact tests:**

- 3 atoms whose circumradius is undefined *or numerically unstable* — i.e. the
  triangle is collinear or nearly so (area below an epsilon, equivalently the
  circumradius above a large cap). An exact collinearity test is insufficient.
- 2 atoms that are coincident or near-coincident (the `atom₁→atom₂` direction is
  below a length epsilon).
- 1 atom with a zero-/near-zero-length entered direction.

Once set up, the **selection is no longer relevant**: it may be changed or
cleared freely. The line is a frozen snapshot.

## Modal lifecycle

The guideline is a **modal state** of the node, orthogonal to which tool is
active.

- **Enter:** click the setup button (requires a valid 1/2/3-atom selection).
- **Active:** the line renders in the viewport (thin cylinder in a distinct
  guide color) with a marker at `t`. The panel card shows the 1D position field
  (labeled per sub-mode — "Selected atom position" or "New atom position"), a
  **Snap to guideline** checkbox (Move sub-mode only), the element selector, a
  **Place atom** button (Place sub-mode only), and a **Cancel** button.
- **Persists** across switching between the **Default** and **Add Atom** tools —
  the normal flow is to set it up, then place/move.
- **Exit (clears the guideline):** the **Cancel** button, the **Escape** key, or
  leaving / deselecting the atom_edit node. Nothing else clears it.

## Behavior while in guideline mode

Everything keys off the **selection**, in two sub-modes:

- **Move sub-mode — exactly one atom selected.** The field and the *Snap to
  guideline* checkbox operate on that atom.
- **Place sub-mode — 0 atoms selected (or ≥2).** The field positions a marker for
  the *next new* atom; the checkbox is hidden.

### Position decomposition

A selected atom's position is decomposed relative to the line into:

- **`t`** — the signed along-line **projection** (Å from `origin`). This is the
  value the field shows.
- **perpendicular offset** — the orthogonal vector from the line to the atom.
  Its length is the atom's distance from the line.

### The "Snap to guideline" checkbox (Move sub-mode)

When exactly one atom is selected, the card shows a **Snap to guideline**
checkbox that represents whether the atom is locked onto the line. It is the
*only* control that ever moves an atom onto the line.

The snapped state is a **transient mode bit** on the guideline (not serialized,
not undoable). It cannot be derived purely from geometry — unchecking is a
geometric no-op (the atom stays at offset 0), so a "snapped ⟺ offset≈0" rule
could not distinguish *released-but-on-line* from *snapped*. Because the bit is
independent of atom positions, it must be **explicitly invalidated** whenever an
atom can move out from under it:

- **Auto-resets to OFF** whenever the selection changes, on entering the mode,
  **on any undo/redo**, and on leaving/deselecting the node. (The undo/redo reset
  is load-bearing: undoing a snap-move restores the atom to its off-line position,
  and a stale ON bit would otherwise silently re-constrain the next drag.) A
  freshly selected atom is never moved just by clicking it — snapping is always a
  deliberate action.
- **Check (OFF→ON):** snaps the atom onto the line — sets its perpendicular
  offset to zero, moving it to `origin + t · direction` at its current
  projection `t`. From then on, dragging it is **constrained to the line**.
  (One undo step.)
- **Uncheck (ON→OFF):** releases the atom — it stays where it is, and dragging it
  becomes **free 3D motion** again. (No move.)

### The 1D position field (two-way sync)

The field is **two-way bound** to `t` and always edits `t` while **preserving the
atom's current perpendicular offset**:

- **Reflects:** displays the atom's current projection `t`; updates live while the
  atom is dragged.
- **Controls — Snapped (checkbox ON):** offset is zero, so editing `t` slides the
  atom *along* the line.
- **Controls — Not snapped (checkbox OFF):** offset is preserved, so editing `t`
  moves the atom *parallel* to the line — its projection changes, its orthogonal
  distance is unchanged. (Slide along a crystal direction while holding the
  lateral offset.)

A read-only **off-line: X Å** readout shows the orthogonal distance — zero when
snapped, the preserved offset otherwise.

This binding is the point of the feature: read where the atom is, then
**iterate** — type `5.0`, see it; type `5.2`, see it; nudge until it's right.
Each applied value is one undo step.

In **Place sub-mode** the field instead drives a **placement marker** on the line
(always at offset zero); the previewed new atom sits at `t` and nothing commits
until placed.

### Adding a new atom (primary use case)

In **Place sub-mode**, both paths create a **free atom (no bonds)** of the
panel-selected element, as a single undo step. The guideline **stays active**
afterward, so several atoms can be placed in sequence.

- **Numeric (exact, reproducible):** set `t` in the position field, then click
  **Place atom**. The atom is created at `origin + t · direction`. (The placement
  marker is a non-draggable visual driven by the field; dragging the marker along
  the line is **out of scope for v1** — see *Out of scope*.)
- **Viewport (quick, with the Add Atom tool):** click anywhere → the atom is
  placed at the point on the line closest to the click ray (snap-to-line), and
  the field updates to that `t`.

### Moving one atom (Move sub-mode)

A typical flow: select the atom → **check Snap to guideline** (it jumps onto the
line) → position it with the field or by dragging:

- **Numeric:** edit the position field. Best for an exact, reproducible value.
- **Drag (Default tool):**
  - **Snapped (ON):** dragging is **constrained to the line** — the atom rides the
    line tracking the cursor's projection. The field updates live. Release
    commits (one undo step).
  - **Not snapped (OFF):** dragging is **free** 3D motion; the field still shows
    the live projection `t` and the off-line distance.

For the 1-atom *Directional* line the reference atom already lies on the line, so
snapping is a no-op and the field becomes a pure "slide along the direction"
control.

(If the selected atom is a base atom not yet in the diff, any move promotes it to
the diff first — the existing promotion machinery, same as a normal drag.)

### Moving multiple selected atoms — NOT constrained (deliberate)

When **two or more** atoms are selected, dragging moves them **freely**, exactly
as if guideline mode were off. The guideline remains drawn but has no effect on
the drag.

Rationale for excluding it from v1:

- A line constrains a *single point* unambiguously. A group of atoms has no
  single point to constrain — it would require choosing a representative (e.g.
  the centroid) and snapping the whole rigid fragment onto the line, which is
  surprising and rarely the intent.
- The feature's purpose is positioning *individual* hard-to-place atoms. Group
  relocation is a different operation.
- It keeps the implementation to a single-atom branch in the existing drag path.

Group-along-line (rigid translation with the centroid constrained to the line)
can be added later if it proves necessary.

### Other interactions

- Normal **selection** still works while modal — click an atom to select it
  (needed before a single-atom constrained drag), click empty space to
  deselect. Selection changes never clear the guideline. (The placement marker is
  not interactive in v1, so atom hit-testing is unchanged — no dot hit-priority
  rule is needed.)
- The **Add Bond** tool behaves normally; it ignores the guideline.

## Summary of cases

| Action | Behavior in guideline mode |
|---|---|
| Check *Snap to guideline* (1 atom) | atom snaps onto the line (offset→0); drag becomes constrained |
| Uncheck *Snap to guideline* | atom released in place; drag becomes free (no move) |
| Select a different atom | checkbox auto-resets to OFF (no atom is moved by selecting) |
| Position field — snapped (ON) | edits `t`; atom slides **along** the line |
| Position field — not snapped (OFF) | edits `t`; atom moves **parallel** to the line (orthogonal offset preserved) |
| Position field — Place sub-mode | sets the placement marker `t` for the next new atom |
| Place new atom — Place button | at `origin + t · direction`, **no bonds** |
| Place new atom — Add Atom click | snapped to nearest point on the line, **no bonds** |
| Drag 1 atom — snapped (ON) | constrained to the line; field tracks live |
| Drag 1 atom — not snapped (OFF) | free 3D drag; field still shows live `t` + off-line distance |
| Drag ≥ 2 selected atoms | **free** (unconstrained) |
| Click empty / select atoms | normal (switches the field between Place / Move sub-modes) |
| Escape / Cancel | exit guideline mode |

## Implementation footprint (sketch)

Kept intentionally small; reuses existing machinery:

- **State:** one transient (non-serialized) `Option<Guideline>` field on
  `AtomEditData`, alongside `selection` / `active_tool`. The `Guideline` carries
  `{ origin, direction, t, snapped: bool }`. No undo plumbing for the guideline
  itself; atom *mutations* it triggers reuse the existing `with_atom_edit_undo`
  path. The `snapped` bit resets on selection change / undo-redo / node-deselect
  (see *The "Snap to guideline" checkbox*).
- **Geometry:** circumcenter / midpoint / triangle-normal helpers (tolerance-based
  degeneracy); point→line decomposition (`t` + perpendicular offset); ray↔line
  closest point (with a parallel-ray fallback that ignores the click). These are
  **pure functions** and carry the bulk of the test coverage.
- **Rendering:** one decorator visual `GuidelineVisuals { origin, direction,
  marker_t, ... }` on `AtomicStructureDecorator`, populated in
  `eval(decorate=true)` (applied to **both** the result and diff outputs, mirroring
  `apply_guided_placement_decoration`) and tessellated in `atomic_tessellator.rs`
  next to the existing `GuidePlacementVisuals` (line cylinder + marker dot in a
  distinct guide color).
- **Refresh plumbing:** panel-driven edits (`t`, snap, element) have no pointer
  event, so the model→API path **must explicitly request a redecorate refresh**
  (`Lightweight`/`Partial`) after mutating transient state — `get_*_mut_transient`
  does not mark the node changed, so a naive reuse leaves a stale viewport.
- **Drag:** a single-atom, line-constrained branch in the Default tool's drag
  (a 1D reduction of the existing `ScreenPlaneDragging` projection), taken only
  when exactly one atom is selected and `snapped == true`. Multi-atom and
  not-snapped drag are untouched. (No marker-handle drag in v1.)
- **API:** `set_guideline_from_selection`, `set_guideline_position`,
  `set_guideline_snapped`, `place_atom_on_guideline`, `clear_guideline` (+ a
  viewport snap-place path for the Add Atom tool). Thin wrappers over `AtomEditData`
  methods; tests target the core methods, not the wrappers.
- **Flutter:** one "Guideline" card in `atom_edit_editor.dart`; Escape handling
  and snap dispatch in `structure_designer_viewport.dart` (mirrors the existing
  guided-placement dispatcher). **Escape precedence:** if Add-Atom guided
  placement is active, Escape cancels that first; a second Escape clears the
  guideline.

## Phased implementation plan

Each phase is independently committable, ends green (`cargo test`, `cargo clippy`,
`flutter analyze`), and lands its own tests. Tests live in `rust/tests/...` (never
inline `#[cfg(test)]`); the geometry and state-transition phases carry the real
coverage, since API wrappers, tessellation, and Flutter are exempt per the
testing policy.

### Phase 1 — Pure geometry + `Guideline` type (foundation)

The math, isolated from all interaction. No `AtomEditData` wiring yet.

- New module `rust/src/structure_designer/nodes/atom_edit/guideline.rs` (or a
  `crystolecule` geometry helper if it stays domain-free):
  - `Guideline { origin: DVec3, direction: DVec3 (unit), t: f64, snapped: bool }`.
  - `from_three_atoms(a,b,c) -> Result<(origin,dir), GuidelineError>` — circumcenter
    + triangle normal, tolerance-based degeneracy.
  - `from_two_atoms(a,b) -> Result<…>` — midpoint + normalized `a→b`.
  - `from_one_atom(p, dir) -> Result<…>` — origin = atom, normalized `dir`.
  - `decompose(point) -> (t, offset_vec)` — `t = (point-origin)·dir`,
    `offset = (point-origin) - t·dir`.
  - `point_at(t) -> DVec3` = `origin + t·dir`.
  - `closest_t_to_ray(ray_origin, ray_dir) -> Option<f64>` — ray↔line closest
    point; `None` when parallel.
- `GuidelineError` (`thiserror`): `Collinear`, `Coincident`, `ZeroDirection`.

**Tests** (`rust/tests/structure_designer/atom_edit/guideline_test.rs`, registered
in `tests/structure_designer.rs`):
- circumcenter of a known triangle (equilateral, right triangle) — origin
  equidistant from all three; normal ⟂ both edges.
- midpoint + direction sign follows selection order (a→b vs b→a flips sign of `t`).
- one-atom: origin == atom, direction normalized.
- **degeneracy:** exact-collinear, *near*-collinear (tiny area → `Collinear`),
  coincident pair (→ `Coincident`), zero/near-zero entered direction
  (→ `ZeroDirection`).
- `decompose`/`point_at` round-trip: `decompose(point_at(t))` recovers `t`,
  offset ≈ 0; off-line point recovers correct `t` and offset length == distance.
- `closest_t_to_ray`: a ray crossing the line returns the foot; parallel ray → `None`.

### Phase 2 — Transient state on `AtomEditData` + core mutations

Wire the type into the node; no rendering, no drag, no Flutter.

- Add `guideline: Option<Guideline>` to `AtomEditData` transient state (not
  serialized; mirror `selection`/`active_tool`; handle in `clone_box`).
- Methods:
  - `set_guideline_from_selection()` — reads current selection (1/2/3 atoms),
    builds via Phase-1 helpers, returns `Result` for SnackBar surfacing.
  - `set_guideline_position(t)` — updates `t`; if a single atom is selected, moves
    it: snapped → onto line (`point_at(t)`); not-snapped → preserve current offset
    (`point_at(t) + offset`). Uses recorded mutations + promotion for base atoms.
  - `set_guideline_snapped(bool)` — ON: zero the selected atom's offset (one move,
    promote if base); OFF: no geometric change.
  - `place_atom_on_guideline()` — create a **free** atom (no bonds) at `point_at(t)`.
  - `clear_guideline()`.
  - Reset hooks: clear `snapped` (and optionally the guideline) at the existing
    selection-change, deselect, and **undo/redo** sites.
- Wrap mutating entry points in `with_atom_edit_undo`.

**Tests** (`rust/tests/structure_designer/atom_edit/guideline_state_test.rs`):
- setup from 1/2/3-atom selection populates `guideline`; degenerate selection
  leaves it `None` and returns `Err`.
- `place_atom_on_guideline` adds a pure-addition atom at the expected position with
  **zero bonds** and **no anchor** (PureAddition per the anchor invariant).
- `set_guideline_position` snapped: atom lands exactly on the line (offset ≈ 0).
- `set_guideline_position` not-snapped: atom's perpendicular offset is preserved,
  `t` changes (moves parallel to the line).
- `set_guideline_snapped(true)` zeroes offset and promotes a base atom to the diff
  (anchor set once); `set_guideline_snapped(false)` is a geometric no-op.
- **undo/redo:** placing then undo removes the atom; a snap-move then undo restores
  the off-line position **and** the `snapped` bit is reset OFF (the issue-#1 guard).
- guideline survives a Default↔AddAtom tool switch (not stored in `active_tool`).

### Phase 3 — Rendering (decorator visual + tessellation)

- Add `GuidelineVisuals` to `AtomicStructureDecorator` + `guideline_visuals:
  Option<…>` field (default `None`).
- Populate it in `eval(decorate=true)` for both result and diff outputs from
  `self.guideline` (new `apply_guideline_decoration` helper alongside
  `apply_guided_placement_decoration`).
- Tessellate in `atomic_tessellator.rs`: a thin cylinder for the line + a marker
  dot at `point_at(marker_t)` in a distinct guide color.

**Tests** (logic only — tessellation/GPU is exempt):
- `eval(decorate=true)` with a guideline set populates `decorator.guideline_visuals`
  with the right origin/direction/marker; `decorate=false` leaves it `None`;
  no guideline → `None`.

### Phase 4 — Default-tool constrained drag

- In `default_tool.rs`, when starting a drag with exactly one atom selected and
  `guideline.snapped`, enter a line-constrained drag: project the cursor ray onto
  the guideline (`closest_t_to_ray`) and move the atom to `point_at(t)`; update the
  live `t`. Reuse drag coalescing (`begin/end_atom_edit_drag`) and continuous
  minimization (atom frozen at its constrained position).
- Not-snapped and multi-atom drags fall through to the existing `ScreenPlaneDragging`.

**Tests** (`guideline_drag_test.rs` — exercise the projection + apply, not pointer
plumbing):
- given a guideline and a cursor ray, the constrained drag target equals
  `point_at(closest_t_to_ray(ray))`; off-line component is zero after the move.
- a not-snapped single-atom drag is unaffected (still free 3D).
- a ≥2-atom drag is unaffected.

### Phase 5 — API layer + FFI

- `rust/src/api/structure_designer/atom_edit_api.rs`: `set_guideline_from_selection`,
  `set_guideline_position`, `set_guideline_snapped`, `place_atom_on_guideline`,
  `clear_guideline`, plus a viewport snap-place path for the Add-Atom tool (click →
  `closest_t_to_ray` → place). Each follows the three-phase borrow pattern and
  requests the redecorate refresh after mutating transient state. Return a small
  view struct (`Option<APIGuideline { origin, direction, t, off_line_distance,
  snapped, sub_mode }>`) for the panel.
- `flutter_rust_bridge_codegen generate`.

**Tests:** API wrappers are exempt (thin); coverage is the Phase 1/2/4 core. Add a
roundtrip-style assertion only if a wrapper carries non-trivial mapping logic
(e.g. selection-count → sub-mode).

### Phase 6 — Flutter UI

- "Guideline" card in `atom_edit_editor.dart`: context-sensitive setup button
  (label from selection count), 1D position field (two-way bound, labeled per
  sub-mode), `off-line: X Å` readout, Snap checkbox (Move sub-mode only), element
  selector, Place button (Place sub-mode only), Cancel. SnackBar on degenerate
  setup.
- `structure_designer_viewport.dart`: Escape handling (precedence: guided
  placement → guideline) and Add-Atom snap-place dispatch, mirroring the existing
  guided-placement dispatcher.
- Model methods on `StructureDesignerModel` forward `propertyEditorScopeChain` and
  call `refreshFromKernel()` + `notifyListeners()`.

**Tests:** `flutter analyze` clean; optional smoke in `integration_test/`; manual
walkthrough via `flutter run` (setup from 3 atoms → place several atoms → select
one → snap → field-iterate → drag constrained → Cancel/Escape).
</content>
</invoke>
