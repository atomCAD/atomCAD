# Atom Placement Guidelines — UX Design

Issue: https://github.com/atomCAD/atomCAD/issues/368

> This replaces an earlier design in which the guideline was a modal state
> orthogonal to the active tool. That approach proved unintuitive (it collided
> with each tool's click/selection meaning), so the guideline is now a
> **dedicated tool**. The earlier design lives in git history; the only thing this
> doc says about it is the *Migration from the previous implementation* section,
> which lists exactly what to change.

## Purpose & scope

The `atom_edit` node gains a **guideline**: a temporary line in 3D space that
constrains atom placement to positions that are hard to hit by free clicking —
e.g. the ad-atom site of a Si(111) √3×√3 R30° reconstruction, which sits
equidistant from three surface atoms.

The guideline is a **dedicated tool** (a fourth `atom_edit` tool alongside
Default, Add Atom, Add Bond). It is **fully self-contained**: the guideline
exists only while the Guideline tool is active, and is cleared the moment you
switch tools or leave the node. Everything the feature needs — defining the
line, placing atoms on it, nudging atoms along it — happens inside this one tool
with no reference to the shared selection.

The design is deliberately minimal. The following are **out of scope** by
explicit decision (all additive later if a real need appears):

- **Lines only.** No plane / sphere / ring constraints.
- **No persistence.** The guideline is transient: not serialized to `.cnnd`, not
  part of undo/redo, and cleared on tool switch / node leave.
- **No dynamic dependence.** Origin and direction are a **frozen snapshot** taken
  once at creation. The guideline does *not* move if the atoms it was derived
  from later move.
- **No auto-bond.** Placing an atom never creates bonds. This feature exists only
  to *position* atoms that are otherwise hard to place; adding a bond afterward
  with the Add Bond tool is trivial and would only pollute it.
- **On-the-line only.** Inside this tool, a moved atom lives exactly *on* the
  line — there is no off-line offset and no "slide parallel to the line" mode.
  Free 3D motion is the Default tool's job.
- No composable constraints, no Miller-normal direction gizmo, no expr-based
  generic placement, no group-along-line (dragging ≥ 2 atoms).

## Design principles

1. **It's a tool, not a mode.** `AtomEditTool::Guideline`. While active it fully
   owns pointer interaction; no other tool's click behavior is in play.
2. **Tool-local state, never the shared selection.** The defining atoms and the
   "picked" atom are stored on the tool, not in `AtomEditSelection`. The shared
   selection is untouched (and cleared on tool entry so no stale highlight leaks).
3. **Explicit, visible state machine.** The current behavior is determined by a
   visible fact (*is an atom picked?*), set by an explicit gesture (*click an
   atom* vs *click empty*) — never silently derived from a selection count.
4. **One "active point on the line."** A single value `t` is always "the active
   point on the line." In Place mode it's the ghost marker where the next atom
   will land; in Move mode it's the picked atom. Same dot, same field, same drag
   — the only difference is whether that point is a ghost or a real atom.
5. **Viewport clicks never place.** Placement is the **Place** button only. This
   single rule dissolves the entire "click selects an atom → instead an atom is
   placed" class of bug.
6. **On-the-line only.** Picking an atom snaps it onto the line; there is no
   off-line offset to reason about. Free 3D motion lives in the Default tool.
7. **Self-contained lifecycle.** Enter the tool → no guideline yet. Build one,
   use it, and it vanishes when you leave the tool or the node. Nothing else
   clears it; nothing else creates it.

## The guideline value

A guideline is `{ origin: DVec3, direction: DVec3 (unit), t: f64 }`. `origin`
and `direction` are the frozen line; `t` is the signed along-line position (Å
from `origin`) of the **active point** (ghost marker or picked atom).

**Single source of truth for `t`.** The stored `t` is authoritative **only in
Place mode**, where it positions the ghost (which has no backing atom). In Move
mode the picked atom *is* the active point, so `t` is **derived on read from the
atom's projection onto the line** (`decompose(atom_position).t`) rather than
trusted from the stored field — the atom's diff position is the one source of
truth. This makes the `t` readout robust against any path that moves the picked
atom without going through the tool (undo/redo, a future external edit): the
field always reflects where the atom actually is, and the two values can never
silently desync. Writes (field edit, drag) still go `set t → move atom to
point_at(t)`, keeping them consistent in the normal flow.

Because picking always snaps the atom onto the line, there is no "snapped" mode
bit to track and no "released but on the line" ambiguity — the perpendicular
offset of a moved atom is always zero by construction.

The guideline lives **inside the tool variant** (`GuidelineTool` state below),
not as a field on `AtomEditData`. Embedding it in the tool is what makes
"clears on tool switch" fall out of the type system: switching tools replaces
the `AtomEditTool` enum value and the guideline goes with it.

## The tool & its three states

```
AtomEditTool::Guideline(GuidelineTool)

GuidelineTool {
    phase: Define { defining: Vec<AtomRef> }
         | Active { guideline: Guideline, picked: Option<AtomRef>, drag: DragState }

    // Remembered settings — see "Remembered settings" below.
    entered_direction: DVec3,   // direction for the 1-atom directional line
    remembered_t: f64,          // last along-line distance; seeds a new line's t
}
```

`AtomRef` is a provenance-tagged atom reference (base id or diff id) — the same
stable identity the selection model uses, but stored here on the tool.

**Remembered settings.** `entered_direction` and `remembered_t` live on the tool
(not inside a phase) so they **persist across Clear / re-Define**, not just across
placements on one line. Rebuilding a line from a *different* anchor at the same
direction and distance therefore needs no re-entry: the direction field is
pre-filled and the freshly-created line's `t` is seeded from `remembered_t`. This
is the same "remember what you set" continuity the ghost-marker already has (see
*Placing a new atom*), extended across line rebuilds. `remembered_t` mirrors the
active point whenever `t` changes (field edit, drag, pick); the direction is
updated only by the 1-atom direction field. Both reset only when the tool is
freshly entered (`GuidelineTool::new`). This deliberately does **not** introduce a
live "re-anchor" mode (clicking a different atom moving the existing line) — that
is a heavier change in tension with the frozen-snapshot principle and the
Define/Active split, and is parked under *Future*.

The three user-visible states:

| State | When | What a viewport click does | Drag | Panel |
|---|---|---|---|---|
| **Define** | no guideline yet | **on atom:** toggle it in the defining set (1–3). **on empty:** clear the defining set | — | instruction + defining count + **Create** button (label by count) + direction field (1-atom case) |
| **Place** | guideline exists, no atom picked | **on atom:** pick it → it jumps onto the line → **Move**. **on empty:** no-op | **on the marker or from empty:** slide the ghost marker along the line (sets `t`) | `t` field, element selector, **Place atom**, **Clear** |
| **Move** | guideline exists, an atom is picked | **on the picked atom:** keep it picked. **on another atom:** pick that one (it jumps; the previous stays). **on empty (click):** unpick → **Place** | **on the picked atom:** slide it along the line. **from empty:** unpick → **Place** (no slide) | `t` field (drives the picked atom), **Clear** |

The lifecycle is a tight loop:

```
Define  --Create-->  Place  --click atom / Place button-->  Move
                       ^                                       |
                       +----------- click empty --------------+
```

## Define — building the line

The panel shows one instruction ("Pick 1–3 atoms to define a guideline") and a
context-sensitive **Create** button whose label and behavior depend on how many
atoms are in the tool-local defining set:

| Defining set | Button | Frozen line |
|---|---|---|
| 3 atoms | **Equidistant line** | `origin` = circumcenter of the triangle; `direction` = triangle normal. Every point on the line is equidistant from all three atoms. |
| 2 atoms | **Center line** | `origin` = midpoint; `direction` = atom₁→atom₂ (by pick order). |
| 1 atom | **Directional line** | `origin` = the atom; `direction` = a Vec3 the user enters. A direction field appears with a "Normalize" affordance. |

The **3-atom case is an equidistant line, not a centroid** — this is the correct
geometry for the √3 ad-atom site (equidistant from three surface atoms). The
button is labeled "Equidistant line" with a tooltip ("perpendicular through the
circumcenter — equidistant from all three atoms") so no one expects a centroid.

A click on **empty space in Define clears the entire defining set** (a deliberate
"start over" gesture). This is intentional and low-cost: the defining set is not
undoable, but it is cheap to rebuild — the atoms are still present, so re-picking
1–3 of them is a couple of clicks. Toggling individual atoms off (click an
already-picked atom) is the fine-grained alternative when you only want to drop
one.

The direction sign is deterministic (pick order, or the entered vector) so `t` is
reproducible. **Degenerate input is rejected (SnackBar, no guideline created),
using tolerances rather than exact tests:**

- 3 atoms whose circumradius is undefined *or numerically unstable* — the triangle
  is collinear or nearly so (area below an epsilon / circumradius above a cap).
- 2 atoms that are coincident or near-coincident (length epsilon).
- 1 atom with a zero-/near-zero entered direction.

Clicking **Create** with a valid set builds the frozen line and transitions to
**Place**. The defining set is then irrelevant — it may be cleared freely; the
line is a frozen snapshot and never moves again.

The defining atoms are rendered with a distinct highlight while in Define (the
same highlight resolution path the selection uses, but driven by the tool-local
set — the shared selection is not involved).

## Active — placing and moving on the line

Once a guideline exists, the line renders as a thin cylinder in a distinct guide
color with a marker dot at the **active point** `t`.

### The unified "active point" and the `t` field

`t` is two-way bound to the panel **position field** and is always the active
point on the line:

- **Place mode:** `t` is the ghost marker — where the next placed atom will land.
- **Move mode:** `t` is the picked atom's along-line position; editing the field
  slides the atom along the line; the field updates live while it is dragged.

Because picking snaps the atom onto the line, the atom's perpendicular offset is
always zero — there is no off-line readout and no parallel-slide mode. The field
purely slides the active point along the direction.

### Placing a new atom → auto-pick (the common loop)

In **Place mode**, **Place atom** creates a **free atom (no bonds)** of the
panel-selected element at `origin + t · direction`, as one undo step — and then
**transitions to Move mode with the just-placed atom as the picked atom.** The
ghost doesn't vanish; it *becomes* the real atom and stays the active point.

This optimizes the dominant action: you place one atom at (say) the equidistant
site and then immediately tune its height by dragging it or editing the field.
Rapid-fire placement of *many* atoms is not the use case (placing-then-adjusting
is), so the small cost — one deliberate **click on empty** to begin another atom
— is worth the clarity.

Consequences that the implementation must honor:

- **Place is hidden in Move mode.** After place → auto-pick → Move, there is no
  Place button. The *only* way to start a new atom is to **click empty** (→ Place
  mode), which removes any chance of an accidental double-place and makes "click
  empty to begin another atom" the one rule to learn.
- **The ghost reappears at the last `t`** after you click empty — the marker stays
  where you left it (continuity), it does not reset to 0.
- **Different element for the next atom:** since the element selector is hidden in
  Move mode, the flow is click-empty → pick element → Place. Rare and fine.

### Picking an existing atom → auto-snap

In **Place** or **Move** mode, clicking an existing atom makes it the picked atom
and **snaps it onto the line** (sets its perpendicular offset to zero, moving it
to `origin + t_proj · direction` at its current projection `t_proj`; the field
then reads `t_proj`). This is one undo step. From then on it is the active point:
drag it or edit the field to slide it along the line.

- If the atom is a **base atom** not yet in the diff, the snap promotes it to the
  diff first (`add_atom` + `set_anchor_position`, the existing promotion machinery)
  and then moves it — same as a normal drag.
- If the atom is a **pure-addition** diff atom (e.g. one you just placed), it is
  already in the diff; the move is a plain `move_in_diff` with **no anchor** (per
  the anchor invariant — pure additions must never gain an anchor).

Clicking **empty** in Move mode **unpicks** (the atom stays where it is, on the
line) and returns to Place mode. Clicking a *different* atom re-picks that one.

### Dragging

- **Move mode — drag the picked atom:** a line-constrained drag. Project the
  cursor ray onto the guideline (`closest_t_to_ray`) and move the atom to
  `point_at(t)`; the field tracks live. Reuses drag coalescing
  (`begin/end_atom_edit_drag`) so the whole press-drag-release is **one** undo
  step (the snap-on-pick and the slide coalesce together).
- **Place mode — drag the ghost:** the ghost has no backing atom, so a drag
  starting **on the marker dot or anywhere on empty space** slides the ghost
  marker — sets `t` to the cursor ray's projection. The marker dot is a grab
  handle *and* empty-drag works (the marker is small and easy to miss, so empty
  space is a forgiving fallback); both do the same thing. Nothing commits; `t`
  is transient until **Place**.
- **Move mode — drag from empty:** a drag starting on empty space while an atom
  is picked **unpicks** (→ Place) and does not slide anything; it mirrors the
  empty-*click* unpick. Only a drag that starts on the picked atom slides it.
- A drag whose source ray is parallel to the line (`closest_t_to_ray == None`) is
  ignored.

### `t` two-way sync (summary)

| Mode | Field reflects | Field controls | Drag |
|---|---|---|---|
| Place | ghost marker `t` | moves the ghost marker | drag on the marker or from empty slides the ghost |
| Move | picked atom's live `t` (derived from its projection) | slides the picked atom along the line | drag on the picked atom slides it; drag from empty unpicks |

## Lifecycle

- **Enter the tool:** start in **Define** with an empty defining set. Clear the
  shared selection so no stale highlight leaks in. No guideline yet.
- **Create:** Define → Place (builds the frozen line).
- **Clear (panel button / Escape in Active):** drop the guideline and return to
  **Define** (the tool stays active so you can build a new line without leaving).
  The **remembered settings** (`entered_direction`, `remembered_t`) survive — only
  the line and the defining set are dropped (see *Remembered settings*).
- **Escape in Define:** clear the defining set.
- **Exit (clears everything):** switching to any other tool, or
  leaving/deselecting the atom_edit node, drops the `Guideline` tool variant — the
  guideline and all tool-local state (including the remembered settings) vanish.

Switching tools intentionally clears the guideline (chosen for clarity over
persistence). The natural flow doesn't ping-pong: place all the atoms you need in
this tool, *then* switch to Add Bond to bond them (placement makes no bonds by
design). Re-deriving a line is cheap since the defining atoms are still present.

### Undo / redo

The guideline value itself is transient (not undoable). The *atom mutations* it
triggers are undoable, via the existing `with_atom_edit_undo` path:

- **Place** is one step (creates the atom). Undo removes the atom — and because
  the atom is gone, the tool **auto-unpicks** back to Place mode.
- **Pick-and-snap** (incl. any subsequent constrained drag) is one coalesced step.
  Undo restores the atom's off-line position — and the tool **auto-unpicks** back
  to Place mode.

**Auto-unpick on undo/redo** is required: undoing a snap moves the atom out from
under the picked state, and a stale picked atom would otherwise silently
re-constrain the next drag. Implemented as a call into the active tool from the
undo/redo API path after the command is applied.

This makes redo of a Place **intentionally asymmetric**: the original Place ends
in Move with the new atom picked, but *redoing* it lands in Place with the atom
present and unpicked. That is acceptable and by design — the tool state
(picked/phase) is transient and not itself undoable, so undo/redo only guarantees
the *atom mutations* are restored, not the exact picked state. Auto-unpick is the
safe, uniform rule for both directions; re-picking is one click away.

## Implementation footprint (sketch)

- **State:** `AtomEditTool::Guideline(GuidelineTool)` with the `Define` / `Active`
  phases above. `GuidelineTool` carries the defining set, the `Guideline`, the
  `picked: Option<AtomRef>`, a small drag sub-state, and the remembered settings
  (`entered_direction`, `remembered_t`) that outlive a Clear. No serialization; no
  undo plumbing for the tool state itself (atom mutations reuse
  `with_atom_edit_undo`).
- **Geometry (`guideline.rs`):** `Guideline { origin, direction, t }` plus the
  pure helpers — `from_three_atoms` (circumcenter + normal, tolerance degeneracy),
  `from_two_atoms` (midpoint + dir), `from_one_atom`, `decompose` (t +
  perpendicular offset), `point_at`, `closest_t_to_ray` (`None` when parallel).
  These carry the bulk of the test coverage.
- **Tool mutations:** `create_from_defining(entered_direction) -> Result<_,
  GuidelineError>`, `set_position(t)`, `place_atom()` (creates free atom →
  auto-pick), `pick_atom(AtomRef)` (snap + promote-if-base), `unpick()`,
  `clear()` (→ Define). Mutating entries wrapped in `with_atom_edit_undo`.
- **Pointer interaction:** a `pointer_down` / `pointer_move` / `pointer_up` state
  machine on the Guideline tool (mirrors `default_tool.rs`): hit-test atom vs
  empty vs marker; click-vs-drag threshold; pick/auto-snap, ghost-drag,
  picked-atom constrained drag, unpick. The constrained drag is the 1-D ray↔line
  reduction (`closest_t_to_ray`).
- **Rendering:** a `GuidelineVisuals` decorator on `AtomicStructureDecorator`,
  populated from the tool state in `eval(decorate=true)` for both result and diff
  outputs; line cylinder + marker dot at `point_at(t)`. Plus a highlight for the
  defining atoms (Define) and the picked atom (Move), via the selection-highlight
  resolution path but driven by the tool-local refs.
- **Refresh plumbing:** panel-driven edits (`t`, element, create, place, clear)
  have no pointer event, so the model→API path must explicitly request a
  redecorate refresh (`Lightweight`/`Partial`) after mutating transient tool state.
- **API:** `set_active_atom_edit_tool(Guideline)` (existing mechanism, new variant
  in `APIAtomEditTool`); `guideline_create_from_defining(dir) -> String` (empty or
  error message for the SnackBar); `guideline_set_position(t)`,
  `guideline_place_atom() -> bool`, `guideline_clear()`,
  `guideline_set_entered_direction(vec3)`. View struct
  `APIGuidelineToolView { phase, defining_count, can_create, needs_direction, t }`
  for the panel. Pointer events flow through the existing atom_edit pointer API,
  dispatched to the tool.
- **Flutter:** a fourth tool button in the atom_edit toolbar; a phase-driven panel
  card (Define: instruction + Create + direction field; Place: `t` field + element
  + Place + Clear; Move: `t` field + Clear); SnackBar on degenerate Create;
  Escape handling (Active → Clear/Define, Define → clear set). Model methods
  forward `propertyEditorScopeChain`, call `refreshFromKernel()` +
  `notifyListeners()`.

## Phased implementation plan

Three phases, one per architecture layer (core / viewport / UI). Each is
independently committable, ends green (`cargo test`, `cargo clippy`,
`flutter analyze`), and lands its own tests. Tests live in `rust/tests/...`
(never inline `#[cfg(test)]`); Phase 1 carries the bulk of the coverage (API
wrappers, tessellation, and Flutter are exempt per policy).

### Phase 1 — Rust core (geometry + tool state + mutations)

The pure math and the full state machine, with no rendering, pointer plumbing, or
Flutter. The whole feature is exercisable from tests after this phase.

> **✅ STATUS — Phase 1 DONE (2026-06-23), implemented ADDITIVELY. Phase 2 & 3
> authors: read this first.**
>
> Phase 1 added the new tool-based core *alongside* the still-working v1 **modal**
> guideline, so the build stayed green with **zero** API/Flutter/FRB churn (Phase 1
> is backend-only). The v1 modal system was therefore **not** removed yet — that is
> now explicit Phase 2/3 work (see the **Migration** section's status note). What
> exists after Phase 1:
>
> - **New core, wired & tested:** `AtomEditTool::Guideline(GuidelineTool)` (in
>   `types.rs`); tool methods on `AtomEditData` named `guideline_*`
>   (`guideline_create_from_defining(&HashMap<u32,DVec3> base_positions)`,
>   `guideline_set_position`, `guideline_place_atom`, `guideline_pick_atom`,
>   `guideline_unpick`, `guideline_tool_clear`, plus `guideline_toggle_defining` /
>   `guideline_clear_defining` / `guideline_set_entered_direction` and the
>   `guideline_active` / `guideline_picked` / `guideline_defining` readers).
>   Undo auto-unpick: `auto_unpick_active_atom_edit_guideline` (called in
>   `structure_designer.rs` `undo()`+`redo()`). Deselect-clear:
>   `clear_guideline_tool_on_node_deselect` (called in `mark_selection_changed`).
>   Tests in **`atom_edit_guideline_tool_test.rs`** (the geometry tests stayed in
>   `atom_edit_guideline_test.rs`).
> - **Deferred to Phase 2 (viewport):** delete the v1 modal viewport paths — the
>   `default_tool.rs` `GuidelineDragging` state, `drag_selected_along_guideline`,
>   and `place_atom_on_guideline_by_ray` (the doc's "no viewport snap-place"
>   migration item). Build the *new* pointer machine + rendering against the tool
>   state instead.
> - **Deferred to Phase 3 (API + Flutter):** (1) add `APIAtomEditTool::Guideline` +
>   wire `set_active_tool` to construct `GuidelineTool` + run `flutter_rust_bridge_codegen`
>   — **until then `get_active_tool` maps `Guideline(_) → APIAtomEditTool::Default` as
>   a placeholder** (the tool can only be entered from Rust tests, so the arm is
>   unreachable in production; replace it). (2) Delete the v1 API/view shape
>   (`APIGuideline`, `APIGuidelineSubMode`, `set_guideline_*`,
>   `place_atom_on_guideline*`, `build_api_guideline`, `get_atom_edit_guideline`) and
>   the v1 Flutter card, replacing them with `APIGuidelineToolView` + the new tool
>   functions and the phase-driven panel.
> - **Deferred cleanup (do in the phase that removes v1):** `Guideline` **still has
>   its `snapped` field** because v1 still uses it; drop `snapped` (and the legacy
>   `guideline: Option<Guideline>` field on `AtomEditData`, `set_guideline_snapped`,
>   `reset_guideline_snapped` / `reset_active_atom_edit_guideline_snapped`, and the
>   `single_guideline_atom` / `move_guideline_atom` / `GuidelineAtom` helpers) when
>   the v1 path goes. The new tool never reads `snapped`.
>
> The bullets below are the original Phase 1 spec; the only behavioural delta from
> them is the **Remembered settings** feature (see that section) — `entered_direction`
> and `remembered_t` moved onto `GuidelineTool` and persist across Clear.

- **Geometry (`guideline.rs`):** `Guideline { origin, direction, t }` plus the
  pure helpers — `from_three_atoms` (circumcenter + normal, tolerance degeneracy),
  `from_two_atoms` (midpoint + dir), `from_one_atom`, `decompose`, `point_at`,
  `closest_t_to_ray` (`None` when parallel).
- **Tool state:** `AtomEditTool::Guideline(GuidelineTool)` with `Define` / `Active`
  phases.
- **Mutations:** `create_from_defining`, `set_position`, `place_atom` (→ auto-pick),
  `pick_atom` (snap + promote-if-base), `unpick`, `clear`. Wrap mutating entries in
  `with_atom_edit_undo`. Wire **auto-unpick on undo/redo** and **clear-everything
  on node-deselect** (tool-switch clears via variant replacement; deselect needs
  the explicit hook).
- **Tests** (`atom_edit_guideline_test.rs` + `atom_edit_guideline_state_test.rs`):
  - geometry: constructors; degeneracy (collinear, near-collinear, coincident,
    zero-dir); `decompose`/`point_at` round-trip; `closest_t_to_ray` foot +
    parallel `None`.
  - create from 1/2/3-atom defining set populates `Active`; degenerate set returns
    `Err` and stays in `Define`.
  - `place_atom` adds a pure-addition atom (zero bonds, no anchor) at `point_at(t)`
    **and** transitions to Move with that atom picked.
  - `pick_atom` on a base atom promotes it (anchor set once) and snaps to the line
    (offset ≈ 0); on a pure-addition atom it moves with no anchor.
  - `set_position` in Move slides the picked atom; in Place it moves the ghost
    (no atom mutation). `unpick` returns to Place with the ghost at the last `t`.
  - undo of a place removes the atom and auto-unpicks; undo of a pick-snap restores
    the off-line position and auto-unpicks.

### Phase 2 — Viewport (rendering + pointer interaction)

Make the tool work in the 3D view. Both halves are the "viewport" layer; the
pointer math stays unit-testable independent of rendering.

- **Rendering:** populate `GuidelineVisuals` from the tool state in
  `eval(decorate=true)` for both result and diff outputs — line cylinder + marker
  dot at `point_at(t)`; highlight the defining atoms (Define) and the picked atom
  (Move).
- **Pointer state machine:** `pointer_down` / `pointer_move` / `pointer_up` on the
  Guideline tool (dispatched from the existing atom_edit pointer handler): defining
  toggle (Define), pick + auto-snap, ghost-drag (Place), picked-atom constrained
  drag (Move), unpick on empty-click (Move), click-vs-drag threshold. Reuse
  `closest_t_to_ray` + `begin/end_atom_edit_drag`.
- **Tests** (`atom_edit_guideline_render_test.rs` + `atom_edit_guideline_drag_test.rs`):
  decorate populates the visuals with the right origin/direction/marker in `Active`,
  nothing in `Define`, `None` when `decorate=false`; the constrained drag target
  equals `point_at(closest_t_to_ray(ray))` with off-line component zero; parallel
  ray is a no-op; ghost drag sets `t` without mutating atoms.

### Phase 3 — API + Flutter UI

The thin FFI surface and the panel/toolbar that drive it.

- **API + FFI:** add the `Guideline` variant to `APIAtomEditTool`; add
  `guideline_create_from_defining` (empty string or SnackBar error),
  `guideline_set_position`, `guideline_place_atom`, `guideline_clear`,
  `guideline_set_entered_direction`, and the `APIGuidelineToolView` builder. Each
  follows the three-phase borrow pattern and requests the redecorate refresh after
  mutating transient state. Run `flutter_rust_bridge_codegen generate`.
- **Flutter:** fourth tool button in the atom_edit toolbar; phase-driven Guideline
  panel card (Define / Place / Move as above); SnackBar on degenerate Create;
  Escape handling (Active → Clear/Define, Define → clear set). Model methods on
  `StructureDesignerModel` forward the scope chain and call `refreshFromKernel()` +
  `notifyListeners()`.
- **Tests:** API wrappers exempt (add a roundtrip assertion only for non-trivial
  mapping, e.g. defining-count → `can_create` / `needs_direction`); `flutter
  analyze` clean; optional smoke in `integration_test/`; manual walkthrough via
  `flutter run` (enter tool → pick 3 atoms → Create → Place atom → tune by
  drag/field → click empty → place another → Clear → switch tool clears the line).

## Migration from the previous implementation

The guideline already exists in the codebase as a modal state on `AtomEditData`.
This is the complete list of what changes to reach the design above. (Everything
else in this doc is the target spec, independent of how the old version worked.)

> **Migration status (after Phase 1, additive).** None of the *removals* below
> have happened yet — Phase 1 only **added** the new tool core beside the v1 modal
> system (see the Phase 1 STATUS note). So every bullet that says "delete" / "remove"
> / "loses" is still **outstanding work for Phase 2 (viewport bullets) and Phase 3
> (API/Flutter bullets)**. The "New tool variant", "Place auto-picks", "Setup reads
> tool-local defining set", and "Lifecycle tool-bound" bullets are **done** (in the
> new core); their v1 counterparts still coexist until the corresponding remove-bullet
> is executed. Concretely still-present v1 code to delete: `AtomEditData.guideline`
> field, `Guideline.snapped`, `set_guideline_snapped`/`reset_guideline_snapped`/
> `reset_active_atom_edit_guideline_snapped`, `single_guideline_atom`/
> `move_guideline_atom`/`GuidelineAtom`, `place_atom_on_guideline*`,
> `set_guideline_from_selection`/`set_guideline_position`, the `default_tool.rs`
> `GuidelineDragging` path + `drag_selected_along_guideline`, the v1 API
> (`APIGuideline`, `APIGuidelineSubMode`, `build_api_guideline`, the `atom_edit_*guideline*`
> FFI fns), and the v1 Flutter guideline card. Also replace the placeholder
> `get_active_tool` arm (`Guideline(_) → Default`) once `APIAtomEditTool::Guideline`
> lands.

- **New tool variant** `AtomEditTool::Guideline(GuidelineTool)`. The guideline
  value moves **off `AtomEditData`** into `GuidelineTool::Active`; the
  `guideline: Option<Guideline>` field on `AtomEditData` is removed.
- **`Guideline` loses its `snapped` field.** Delete `set_guideline_snapped`, the
  snapped-reset hook (`reset_active_atom_edit_guideline_snapped`), and the
  off-line / slide-parallel branch. Picking always snaps (offset → 0).
- **Setup no longer reads `AtomEditSelection`.** It reads the tool-local defining
  set. Defining picks and the picked atom are tool-local; the shared selection is
  cleared on tool entry and otherwise untouched. The old selection-count-derived
  sub-mode is replaced by the explicit `picked: Option<AtomRef>`.
- **No viewport snap-place path** (delete `place_atom_on_guideline_by_ray`).
  Placement is the Place button only; the Add Atom tool's click handler loses its
  guideline branch.
- **Place auto-picks the placed atom** (transitions to Move) — new behavior.
- **Lifecycle becomes tool-bound** — clears on tool switch and node deselect.
- **Delete the old API/view shape** (`APIGuideline`, `APIGuidelineSubMode`,
  `set_guideline_snapped`) in favor of `APIGuidelineToolView` and the tool
  functions listed under *API*.
- **Flutter:** remove the old guideline panel card, the Snap checkbox, and the
  off-line readout; replace with the tool button + phase-driven card.
- **Reusable as-is:** the pure geometry in `guideline.rs` (minus `snapped`), the
  `GuidelineVisuals` decorator + tessellation (line + marker dot), and the
  line-constrained drag projection (moved from the Default tool into the Guideline
  tool).

## Future (explicitly deferred)

- **Compose with guided placement** — "pick an atom and place a properly-spaced
  neighbour on the ball surface" combined with a directional guideline. A natural
  extension once both are stable; out of scope here.
- **Alternate 3-atom centers** (centroid, min-spanning) if a non-equidistant use
  case appears. This design ships the equidistant line only.
- **Persistence / dynamic guidelines** if users want a line to survive tool
  switches or track moving atoms.
- **Live re-anchor** — clicking a different atom *moves the existing line's origin*
  to it while keeping direction and `t`, for rapid "same offset from a series of
  atoms" placement (raised by a v1 user, 2026-06). Deferred: it overloads the
  click gesture (which already means "pick the atom to move"), and it cuts against
  the frozen-snapshot principle and the deliberate de-prioritisation of rapid-fire
  multi-atom placement. The cheap, in-scope substitute — **remembering**
  `entered_direction` + `t` across Clear (see *Remembered settings*) — already
  removes the "re-enter the distance/direction" tedium, so re-anchoring needs only
  re-pick + Create. Revisit if more users hit it; it overlaps with *Compose with
  guided placement* above.
