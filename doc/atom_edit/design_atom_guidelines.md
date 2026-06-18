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
the `t` coordinate is reproducible. Degenerate input — 3 collinear atoms (no
circumcircle), or a zero-length direction — is rejected with a SnackBar and no
guideline is created.

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

- **Auto-resets to OFF** whenever the selection changes (or on entering the
  mode). A freshly selected atom is never moved just by clicking it — snapping
  is always a deliberate action.
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

- **Numeric (exact, reproducible):** set `t` in the position field (or drag the
  placement marker along the line), then click **Place atom**. The atom is
  created at `origin + t · direction`.
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
  deselect. The cursor dot has **hit priority** over atoms so it stays
  grabbable. Selection changes never clear the guideline.
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
  `AtomEditData`, alongside `selection` / `active_tool`. No undo plumbing.
- **Geometry:** circumcenter / midpoint / normal helpers; closest-point-on-line
  (ray and point projection).
- **Rendering:** one decorator visual (line + cursor dot), tessellated in the
  `eval(decorate=true)` phase next to the existing `GuidePlacementVisuals` —
  same pattern as the wireframe sphere/ring guides.
- **Drag:** a single-atom, line-constrained branch in the Default tool's drag
  (a 1D reduction of the existing `ScreenPlaneDragging` projection). Multi-atom
  drag is untouched.
- **API:** `set_guideline_from_selection`, `set_guideline_position`,
  `place_atom_on_guideline`, `clear_guideline` (+ a viewport snap-place path for
  the Add Atom tool).
- **Flutter:** one "Guideline" card in `atom_edit_editor.dart`; Escape handling
  and snap dispatch in `structure_designer_viewport.dart` (mirrors the existing
  guided-placement dispatcher).
</content>
</invoke>
