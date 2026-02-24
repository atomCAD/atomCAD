# Design: Bond Creation & Bond Order Manipulation

Redesign of the AddBond tool interaction and addition of bond order manipulation across tools. Replaces the current two-click bond workflow with drag-to-bond and adds bond order control throughout.

**Date:** 2026-02-24
**Scope:** AddBond tool (major rework), Default tool (bond order features), Flutter UI, keyboard shortcuts
**Prerequisite reading:** `doc/atom_edit/atom_edit_ux_research.md`

---

## 1. Current State

### What exists today

| Aspect | Current behavior |
|---|---|
| Bond creation (AddBond tool) | Two-click workflow: click atom A, click atom B |
| Bond order | Always `BOND_SINGLE` (hardcoded `1` at `add_bond_tool.rs:125`) |
| Bond deletion | Default tool: select bond, click "Delete Selected" in panel |
| Bond selection | Default tool: click bond to select |
| AddBond tool UI panel | `SizedBox.shrink()` — no controls at all |
| Tool switching | Panel buttons only, no keyboard shortcuts |

### Key problems

1. **Two-click bond creation is unintuitive** — no visual feedback between clicks, cancel is non-obvious (click same atom), no rubber-band preview
2. **No bond order control at all** — cannot create double/triple bonds, cannot change existing bond order
3. **Tool switching is slow** — must click panel button, no keyboard shortcut, no quick-return mechanism

### Data model (already supports bond orders)

Bond order is stored as 3 bits in `InlineBond` (`inline_bond.rs:11-18`):

| Constant | Value | Meaning |
|---|---|---|
| `BOND_DELETED` | 0 | Delete marker in diffs |
| `BOND_SINGLE` | 1 | Single bond |
| `BOND_DOUBLE` | 2 | Double bond |
| `BOND_TRIPLE` | 3 | Triple bond |
| `BOND_QUADRUPLE` | 4 | Quadruple bond |
| `BOND_AROMATIC` | 5 | Aromatic bond |
| `BOND_DATIVE` | 6 | Dative bond |
| `BOND_METALLIC` | 7 | Metallic bond |

The diff system already supports bond order overrides — a diff bond between two atoms replaces the base bond's order. The infrastructure is complete; only the interaction layer is missing.

---

## 2. Design Decisions

### Decision 1: Keep AddBond as a separate tool

**Rationale:** The Default tool's left-drag gesture space is fully occupied:

| Gesture | Default tool function |
|---|---|
| Left drag on atom | Move selected atoms |
| Shift + left drag on atom | Add to selection + move |
| Ctrl + left drag on atom | Toggle selection + move |
| Left drag on empty | Marquee selection |

There is no modifier + left-drag combination available for bond creation without conflicting with selection/movement workflows. Merging bond creation into the Default tool would require either ambiguous context-dependent drags (fragile, accidental bonds/moves) or obscure modifier keys (undiscoverable).

A separate tool with a fast keyboard activation is the proven pattern (Blender, Photoshop, Unity, Figma).

### Decision 2: Replace two-click with drag-to-bond

**Rationale:** Dragging is the universal gesture for "connect A to B" in visual editors. It provides continuous visual feedback (rubber-band line), implicit directionality, and natural cancel (release in empty space).

### Decision 3: Hold-to-activate for tool switching (not toggle)

**Rationale:** A hold-to-activate (spring-loaded) key gives a fluid gesture: hold B, drag bond, release B, back to Default — feels like a single action.

A toggle key (press B to switch, press B again to switch back) conflicts with hold-to-activate because both start with the same key-down event. The system cannot disambiguate intent at key-down time. Timing heuristics (hold > Nms = spring-loaded, tap = toggle) are unreliable and frustrating.

Therefore: **B key = hold-to-activate only.** For sticky AddBond mode, use the panel button.

---

## 3. AddBond Tool: Drag-to-Bond Interaction

### State machine

Replace `AddBondToolState { last_atom_id: Option<u32> }` with:

```
                pointer_down on atom
Idle ──────────────────────────────────► Pending
  ▲                                      │ hit_atom_id
  │                                      │ mouse_down_screen
  │         pointer_move > threshold     │
  │    ┌─────────────────────────────────┘
  │    │
  │    ▼
  │  Dragging ──── pointer_move ────► Dragging (update preview_target)
  │    │ source_atom_id                   │
  │    │ preview_target: Option<u32>      │
  │    │                                  │
  │    ├── pointer_up on atom ──► [CREATE BOND] ──► Idle
  │    │
  │    └── pointer_up on empty ──► [CANCEL] ──► Idle
  │
  └── pointer_up (< threshold, i.e. click on atom) ──► [CANCEL] ──► Idle
```

Notes:
- Click (no drag) on an atom does nothing in AddBond tool — bonds require an explicit drag gesture
- Click on an existing bond could cycle its order (see Section 5)
- Click on empty space does nothing

### New state type

```rust
// In types.rs, replacing the current AddBondToolState

#[derive(Debug)]
pub struct AddBondToolState {
    pub bond_order: u8,  // BOND_SINGLE, BOND_DOUBLE, BOND_TRIPLE (default: BOND_SINGLE)
    pub interaction_state: AddBondInteractionState,
}

#[derive(Debug)]
pub enum AddBondInteractionState {
    Idle,
    Pending {
        hit_atom_id: u32,       // diff atom ID of source (promoted if needed)
        is_diff_view: bool,
        mouse_down_screen: DVec2,
    },
    Dragging {
        source_atom_id: u32,    // diff atom ID of source
        preview_target: Option<u32>,  // result atom ID currently hovered (for highlight)
    },
}
```

### API changes

Replace the single `atom_edit_draw_bond_by_ray` with three pointer event functions (matching the Default tool pattern):

```rust
// In atom_edit_api.rs

/// Pointer down in AddBond tool. Returns whether an atom was hit.
fn add_bond_pointer_down(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> bool;

/// Pointer move in AddBond tool. Returns preview state for rubber-band rendering.
fn add_bond_pointer_move(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> AddBondMoveResult;

/// Pointer up in AddBond tool. Creates bond if released on valid target.
fn add_bond_pointer_up(
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> bool;  // true if bond was created

/// Set the bond order for the AddBond tool.
fn set_add_bond_order(order: u8);
```

### Preview state (for Flutter rubber-band rendering)

```rust
pub struct AddBondMoveResult {
    pub is_dragging: bool,
    pub source_atom_pos: Option<APIVec3>,   // world position of source atom
    pub preview_end_pos: Option<APIVec3>,   // world position of cursor or snapped target
    pub snapped_to_atom: bool,              // true if hovering over a valid target
}
```

Flutter uses this to draw a rubber-band line from `source_atom_pos` to `preview_end_pos`, with distinct styling when `snapped_to_atom` is true (e.g., thicker line, target atom highlight).

### Provenance handling

Same as current `draw_bond_by_ray`: when an atom is hit in result view, resolve its provenance. If it's a `BasePassthrough`, promote it to a diff identity entry before creating the bond. This logic is already implemented and reusable.

---

## 4. Bond Order Control in AddBond Tool

### Tool panel UI

Replace `SizedBox.shrink()` with:

```
┌──────────────────────────────────────┐
│  Bond Order                          │
│  ┌──────┬──────┬──────┐             │
│  │Single│Double│Triple│             │
│  └──────┴──────┴──────┘             │
│                                      │
│  Drag from atom to atom to bond.     │
└──────────────────────────────────────┘
```

- Segmented control with three options: Single (default), Double, Triple
- The selected order is stored in `AddBondToolState.bond_order`
- Keyboard shortcuts 1/2/3 also change the selection (see Section 6)
- Status text shows the current interaction hint

### Bond creation uses selected order

In the `pointer_up` handler, when creating a bond:

```rust
// Instead of hardcoded 1:
atom_edit_data.add_bond_in_diff(source_id, target_id, state.bond_order);
```

---

## 5. Bond Order Control in Default Tool

### Click selected bond to cycle order

When a bond is already selected and the user clicks it again, cycle its order:

```
single → double → triple → single → ...
```

Implementation location: `default_tool.rs` `pointer_up`, in the `PendingBond` handling path (`default_tool.rs:567-569`). Currently this unconditionally calls `select_result_bond()`. New logic:

```
if bond is already selected:
    cycle its order (single → double → triple → single)
else:
    select the bond (existing behavior)
```

### Keyboard 1/2/3 to set order directly

When one or more bonds are selected in the Default tool, pressing 1/2/3 sets their order to single/double/triple respectively.

### New operation: `change_bond_order`

```rust
// In operations.rs

/// Change the order of a bond in the diff.
/// Handles provenance promotion for base-passthrough atoms.
pub fn change_bond_order(
    structure_designer: &mut StructureDesigner,
    bond_reference: &BondReference,
    new_order: u8,
);

/// Change the order of all selected bonds.
pub fn change_selected_bonds_order(
    structure_designer: &mut StructureDesigner,
    new_order: u8,
);
```

The implementation follows the same pattern as `delete_selected_atoms_and_bonds` for result view:
1. Look up provenance of both bond endpoints
2. If either is `BasePassthrough`, promote to diff identity entry
3. Call `atom_edit_data.add_bond_in_diff(id_a, id_b, new_order)` — this overwrites any existing bond between these atoms in the diff

For diff view: directly modify the bond in the diff.

### Bond info in panel

When a bond is selected in the Default tool, show its current order in the property panel:

```
┌──────────────────────────────────────┐
│  Selected: 1 bond                    │
│  Order: Double                       │
│  ┌──────┬──────┬──────┐             │
│  │Single│Double│Triple│             │
│  └──────┴──────┴──────┘             │
│  [Delete Selected]                   │
└──────────────────────────────────────┘
```

The segmented control here is both a display and an edit control — clicking an order button immediately changes the selected bond's order.

---

## 6. Keyboard Shortcuts

### Hold-to-activate (spring-loaded tool switch)

| Key event | Action |
|---|---|
| B key down | Switch to AddBond tool, remember previous tool |
| B key up | Switch back to previous tool |

Implementation: Flutter `RawKeyboardListener` (or `KeyboardListener`) wrapping the viewport. On key-down, call `set_active_atom_edit_tool(AddBond)` and store the previous tool. On key-up, call `set_active_atom_edit_tool(previous_tool)`.

Edge case: if B is released during an active drag (between pointer_down and pointer_up), do NOT switch tools until the drag completes. The tool switch should be deferred until `pointer_up` fires.

### Bond order shortcuts

| Key | In AddBond tool | In Default tool (bond selected) |
|---|---|---|
| `1` | Set bond order to single | Change selected bond(s) to single |
| `2` | Set bond order to double | Change selected bond(s) to double |
| `3` | Set bond order to triple | Change selected bond(s) to triple |

These keys are handled in Flutter's key event handler and routed to the appropriate Rust API call based on the active tool and selection state.

---

## 7. Rubber-Band Visual Feedback

### What Flutter needs to draw

During AddBond dragging, Flutter draws a line overlay on the 3D viewport:

| State | Visual |
|---|---|
| `Dragging`, no snap target | Thin dashed line from source atom to cursor position |
| `Dragging`, snapped to target | Solid line from source atom to target atom, target atom highlighted |
| Bond order 1 | Single line |
| Bond order 2 | Double line (two parallel lines) |
| Bond order 3 | Triple line (three parallel lines) |

The line is drawn in screen space after projecting `source_atom_pos` and `preview_end_pos` from the `AddBondMoveResult`. This is a 2D overlay on the viewport, not a 3D rendered object.

### Source atom highlight

When dragging starts (transition from Pending to Dragging), the source atom should be visually marked. This can use the existing `AtomDisplayState::Marked` mechanism that the old two-click workflow used for the anchor atom.

---

## 8. Implementation Plan

### Phase A: Rust core (bond order infrastructure)

1. **Add `bond_order` field to `AddBondToolState`** in `types.rs`
   - Change from `{ last_atom_id: Option<u32> }` to `{ bond_order: u8, interaction_state: AddBondInteractionState }`
   - Add `AddBondInteractionState` enum
   - Update `set_active_tool` in `atom_edit_data.rs` to initialize with `bond_order: BOND_SINGLE`

2. **New `change_bond_order` operations** in `operations.rs`
   - `change_bond_order()` for a single bond reference
   - `change_selected_bonds_order()` for all selected bonds
   - Follow the provenance promotion pattern from `delete_selected_atoms_and_bonds`

3. **Bond order cycling in Default tool** in `default_tool.rs`
   - Modify `pointer_up` `PendingBond` path: if bond already selected, cycle order instead of re-selecting

### Phase B: Rust core (drag interaction)

4. **Rewrite AddBond tool as drag state machine** in `add_bond_tool.rs`
   - Replace `draw_bond_by_ray()` with `pointer_down()`, `pointer_move()`, `pointer_up()`
   - `pointer_down`: ray-cast, hit atom → Pending state, promote to diff ID if needed
   - `pointer_move`: if drag threshold exceeded → Dragging state, ray-cast for snap target
   - `pointer_up`: if Dragging + snapped to atom → create bond with `state.bond_order`, else cancel

5. **New API endpoints** in `atom_edit_api.rs`
   - `add_bond_pointer_down`, `add_bond_pointer_move`, `add_bond_pointer_up`
   - `set_add_bond_order`
   - `change_selected_bonds_order`
   - Remove old `atom_edit_draw_bond_by_ray`

### Phase C: Flutter UI

6. **AddBond tool panel** in `atom_edit_editor.dart`
   - Bond order segmented control (Single / Double / Triple)
   - Status text ("Drag from atom to atom")
   - Wire up to `set_add_bond_order` API

7. **Default tool panel: bond info** in `atom_edit_editor.dart`
   - When bond(s) selected: show count, current order, order segmented control
   - Wire up segmented control to `change_selected_bonds_order` API

8. **Pointer event routing for AddBond tool** in Flutter viewport
   - Route left pointer down/move/up to `add_bond_pointer_down/move/up` when AddBond tool is active
   - Draw rubber-band line overlay from `AddBondMoveResult`

9. **Keyboard shortcuts** in Flutter
   - Hold B: spring-loaded tool activation with deferred release during drag
   - 1/2/3: bond order shortcuts routed by active tool and selection state

### Phase D: FRB codegen & testing

10. **Run `flutter_rust_bridge_codegen generate`** after API changes

11. **Tests**
    - Rust unit tests for `change_bond_order` operation (single bond, selected bonds, provenance promotion)
    - Rust unit tests for AddBond drag state machine (idle → pending → dragging → create/cancel)
    - Rust unit tests for bond order cycling in Default tool
    - Verify existing mutation tests still pass (bond creation with order 1 should be unchanged)

---

## 9. Key Files

### Files to modify

| File | Changes |
|---|---|
| `rust/src/structure_designer/nodes/atom_edit/types.rs` | New `AddBondToolState`, `AddBondInteractionState` |
| `rust/src/structure_designer/nodes/atom_edit/add_bond_tool.rs` | Full rewrite: drag state machine |
| `rust/src/structure_designer/nodes/atom_edit/default_tool.rs` | Bond order cycling in `pointer_up` |
| `rust/src/structure_designer/nodes/atom_edit/operations.rs` | New `change_bond_order`, `change_selected_bonds_order` |
| `rust/src/structure_designer/nodes/atom_edit/atom_edit_data.rs` | Update `set_active_tool` for new state shape |
| `rust/src/api/structure_designer/atom_edit_api.rs` | New API endpoints, remove old `draw_bond_by_ray` |
| `lib/structure_designer/node_data/atom_edit_editor.dart` | AddBond panel UI, Default tool bond info, keyboard shortcuts |

### Files unchanged

| File | Why |
|---|---|
| `rust/src/crystolecule/atomic_structure_diff.rs` | Diff apply already handles bond order overrides |
| `rust/src/crystolecule/atomic_structure/inline_bond.rs` | Bond order constants already defined |
| `rust/src/structure_designer/nodes/atom_edit/add_atom_tool.rs` | Guided placement unchanged |
| `rust/src/structure_designer/nodes/atom_edit/text_format.rs` | Text format already supports bond orders |
