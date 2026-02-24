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
    pub bond_order: u8,  // Any valid bond order 1-7 (default: BOND_SINGLE)
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
/// Triggers one refresh if an atom is hit (to show source atom highlight).
fn add_bond_pointer_down(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> bool;

/// Pointer move in AddBond tool. Returns preview state for rubber-band rendering.
/// NO refresh, NO evaluation — only a ray-cast hit test. See Section 8.
fn add_bond_pointer_move(
    screen_pos: APIVec2,
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> AddBondMoveResult;

/// Pointer up in AddBond tool. Creates bond if released on valid target.
/// Triggers one refresh to show the new bond (or remove source highlight on cancel).
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
    pub bond_order: u8,                     // current bond order for visual styling
}
```

Flutter uses this to draw a rubber-band line from `source_atom_pos` to `preview_end_pos`, with distinct styling based on `snapped_to_atom` and `bond_order` (see Section 7 for visual details).

### Provenance handling

Same as current `draw_bond_by_ray`: when an atom is hit in result view, resolve its provenance. If it's a `BasePassthrough`, promote it to a diff identity entry before creating the bond. This logic is already implemented and reusable.

---

## 4. Bond Order Control in AddBond Tool

### Bond order categories

The 7 user-facing bond orders fall into two tiers based on usage frequency:

| Tier | Orders | Rationale |
|---|---|---|
| **Common** (always visible) | Single, Double, Triple | Used in >95% of bond creation. Standard covalent bonds. |
| **Specialized** (expandable) | Quadruple, Aromatic, Dative, Metallic | Domain-specific. Dative/Metallic relevant to APM; Aromatic to organic chemistry; Quadruple rare but exists. |

### Tool panel UI

Replace `SizedBox.shrink()` with:

```
┌──────────────────────────────────────┐
│  Bond Order                          │
│  ┌──────┬──────┬──────┐             │
│  │Single│Double│Triple│             │
│  └──────┴──────┴──────┘             │
│  ┌────┬────────┬──────┬────────┐    │
│  │Quad│Aromatic│Dative│Metallic│    │
│  └────┴────────┴──────┴────────┘    │
│                                      │
│  Drag from atom to atom to bond.     │
└──────────────────────────────────────┘
```

- **Two rows of segmented buttons** — common orders on top, specialized below
- Both rows behave as a single radio group (exactly one selected at a time)
- Default selection: Single
- The selected order is stored in `AddBondToolState.bond_order`
- Keyboard shortcuts 1-7 change the selection (see Section 6)
- Status text shows the current interaction hint
- The specialized row may use smaller text or abbreviated labels to fit the panel width
- This is the **`BondOrderSelector` widget** — a shared Flutter widget reused by both tools (see Section 5 panel and Section 8 Phase C)

### Bond creation uses selected order

In the `pointer_up` handler, when creating a bond:

```rust
// Instead of hardcoded 1:
atom_edit_data.add_bond_in_diff(source_id, target_id, state.bond_order);
```

---

## 5. Bond Order Control in Default Tool

### Click selected bond to cycle order

When a bond is already selected and the user clicks it again, cycle its order through the **common** orders only:

```
single → double → triple → single → ...
```

Cycling is limited to single/double/triple because cycling through all 7 orders would require too many clicks to reach common targets. Specialized orders (quadruple, aromatic, dative, metallic) are set via keyboard shortcuts or the panel buttons.

Implementation location: `default_tool.rs` `pointer_up`, in the `PendingBond` handling path (`default_tool.rs:567-569`). Currently this unconditionally calls `select_result_bond()`. New logic:

```
if bond is already selected:
    cycle its order (single → double → triple → single)
else:
    select the bond (existing behavior)
```

Note: if the bond's current order is a specialized type (e.g. aromatic), clicking it cycles into the common sequence starting at single. This prevents users from getting "stuck" on a specialized order with no way to click out of it.

### Keyboard 1-7 to set order directly

When one or more bonds are selected in the Default tool, pressing a number key sets their order directly:

| Key | Bond order |
|---|---|
| `1` | Single |
| `2` | Double |
| `3` | Triple |
| `4` | Quadruple |
| `5` | Aromatic |
| `6` | Dative |
| `7` | Metallic |

This provides direct access to all bond orders without cycling, making specialized orders equally accessible to power users.

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

The `new_order` parameter accepts any valid bond order (1-7). Values of 0 (`BOND_DELETED`) or >7 are rejected.

The implementation follows the same pattern as `delete_selected_atoms_and_bonds` for result view:
1. Look up provenance of both bond endpoints
2. If either is `BasePassthrough`, promote to diff identity entry
3. Call `atom_edit_data.add_bond_in_diff(id_a, id_b, new_order)` — this overwrites any existing bond between these atoms in the diff

For diff view: directly modify the bond in the diff.

### Bond info in panel

When a bond is selected in the Default tool, show its current order in the property panel. Use the same two-row layout as the AddBond tool:

```
┌──────────────────────────────────────┐
│  Selected: 1 bond                    │
│  Order: Double                       │
│  ┌──────┬──────┬──────┐             │
│  │Single│Double│Triple│             │
│  └──────┴──────┴──────┘             │
│  ┌────┬────────┬──────┬────────┐    │
│  │Quad│Aromatic│Dative│Metallic│    │
│  └────┴────────┴──────┴────────┘    │
│  [Delete Selected]                   │
└──────────────────────────────────────┘
```

The segmented control here is both a display and an edit control — clicking an order button immediately changes the selected bond's order. When multiple bonds are selected with different orders, no button is highlighted (mixed state); clicking a button sets all selected bonds to that order.

This uses the same **`BondOrderSelector` widget** as the AddBond tool panel (see Section 8 Phase C for widget specification).

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
| `1` | Set bond order to Single | Change selected bond(s) to Single |
| `2` | Set bond order to Double | Change selected bond(s) to Double |
| `3` | Set bond order to Triple | Change selected bond(s) to Triple |
| `4` | Set bond order to Quadruple | Change selected bond(s) to Quadruple |
| `5` | Set bond order to Aromatic | Change selected bond(s) to Aromatic |
| `6` | Set bond order to Dative | Change selected bond(s) to Dative |
| `7` | Set bond order to Metallic | Change selected bond(s) to Metallic |

Keys 1-3 cover common workflows. Keys 4-7 provide power-user access to specialized bond types without navigating the panel.

These keys are handled in Flutter's key event handler and routed to the appropriate Rust API call based on the active tool and selection state. When no bond is selected in the Default tool, number keys are no-ops (they don't conflict with other shortcuts).

---

## 7. Rubber-Band Visual Feedback

### What Flutter needs to draw

During AddBond dragging, Flutter draws a line overlay on the 3D viewport:

**Snap state styling:**

| State | Visual |
|---|---|
| `Dragging`, no snap target | Thin dashed line from source atom to cursor position |
| `Dragging`, snapped to target | Solid line from source atom to target atom, target atom highlighted |

**Bond order styling (applied to the rubber-band line):**

| Bond order | Visual | Color hint |
|---|---|---|
| Single (1) | Single line | Default (white/light gray) |
| Double (2) | Two parallel lines | Default |
| Triple (3) | Three parallel lines | Default |
| Quadruple (4) | Four parallel lines | Default |
| Aromatic (5) | Solid + dashed parallel lines | Distinct color (e.g., amber) |
| Dative (6) | Arrow line (→ from source to target) | Distinct color (e.g., cyan) |
| Metallic (7) | Thick single line | Distinct color (e.g., silver/gray) |

The visual style for aromatic/dative/metallic should match how these bonds are rendered in the 3D view (ball-and-stick mode), providing consistency between the preview and the final result.

The line is drawn in screen space after projecting `source_atom_pos` and `preview_end_pos` from the `AddBondMoveResult`. This is a 2D overlay on the viewport, not a 3D rendered object.

The `AddBondMoveResult` includes the active `bond_order` so Flutter can select the correct visual style:

```rust
pub struct AddBondMoveResult {
    pub is_dragging: bool,
    pub source_atom_pos: Option<APIVec3>,
    pub preview_end_pos: Option<APIVec3>,
    pub snapped_to_atom: bool,
    pub bond_order: u8,  // current bond order setting for visual styling
}
```

### Source atom highlight

When dragging starts (transition from Pending to Dragging), the source atom should be visually marked. This can use the existing `AtomDisplayState::Marked` mechanism that the old two-click workflow used for the anchor atom.

---

## 8. Performance: No Per-Frame Evaluation

### Contrast with atom dragging

Atom dragging (Default tool `ScreenPlaneDragging`) is the existing interactive performance benchmark. It mutates atom positions in the diff **every frame**, which requires per-frame:

1. `drag_selected_by_delta()` → mutates diff atom positions
2. `mark_node_data_changed()` → flags atom_edit node dirty
3. `mark_skip_downstream()` → skips downstream BFS + re-evaluation
4. `refresh_structure_designer_auto()` → triggers partial refresh
5. `eval()` with `cached_input` → skips upstream re-evaluation, but runs `apply_diff()` on the cached input molecule
6. Full tessellation → rebuild atom + bond impostor meshes
7. GPU mesh upload

The optimizations (`skip_downstream` + `cached_input`) make this fast enough, but `apply_diff` + tessellation + GPU upload still run every frame.

### Why rubber-band preview needs none of this

During an AddBond drag, **nothing in the diff changes per frame**. No atoms move, no bonds are created — the bond is only created on `pointer_up`. The only thing changing frame-to-frame is:

- The cursor position (known to Flutter already)
- Whether the cursor is hovering over a target atom (requires a ray-cast hit test)

The rubber-band line is a **2D Flutter overlay** (`CustomPainter`), not a 3D rendered object. It requires zero Rust-side evaluation, zero tessellation, zero GPU mesh updates per frame.

### Per-frame data flow

| Phase | Rust work | Evaluation? | Tessellation? |
|---|---|---|---|
| **Drag start** (threshold crossed) | Mark source atom as `Marked`, store world position | Once | Once |
| **Each `pointer_move`** | Ray-cast for snap target (spatial grid lookup, ~microseconds). Return `AddBondMoveResult` with positions. | **No** | **No** |
| **Snap target changes** | Update `preview_target` in state. Flutter handles highlight as 2D overlay. | **No** | **No** |
| **`pointer_up` on atom** | Create bond in diff. Normal partial refresh. | Once | Once |
| **`pointer_up` on empty** | Cancel. Unmark source atom. | Once | Once |

### API implementation requirement

The `add_bond_pointer_move()` API function must:

1. Ray-cast against the current scene's spatial grid (lightweight hit test)
2. Update the `preview_target` in `AddBondInteractionState::Dragging`
3. Return `AddBondMoveResult` with source position, cursor/target position, snap state
4. **NOT** call `mark_node_data_changed()`
5. **NOT** call `refresh_structure_designer_auto()`

This matches the pattern of `default_tool_pointer_down()` (line 450 of `atom_edit_api.rs`) which also performs a hit test without triggering a refresh.

Flutter then projects the 3D positions to screen space and draws the rubber-band line via `CustomPainter` — entirely on the Flutter side with no Rust re-evaluation.

### Target atom highlighting

Rather than marking the target atom via `AtomDisplayState` (which would require evaluation + tessellation on each snap change), the target highlight is rendered as a **2D overlay** by Flutter — e.g., a circle or ring drawn at the projected screen position of the target atom. This keeps snap-target changes completely evaluation-free.

---

## 9. Implementation Plan

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

6. **`BondOrderSelector` shared widget** in `atom_edit_editor.dart`
   - Reusable `StatelessWidget` used by both the AddBond tool panel and the Default tool bond info panel
   - Two-row layout: common row (Single/Double/Triple) + specialized row (Quad/Aromatic/Dative/Metallic)
   - Acts as a single radio group across both rows
   - Props:
     - `selectedOrder: int?` — currently selected bond order (1-7), or `null` for mixed/no-selection state
     - `onOrderChanged: void Function(int order)` — callback when user clicks a button
   - When `selectedOrder` is `null`, no button is highlighted (mixed state for multi-bond selection)
   - Clicking a button calls `onOrderChanged` with the corresponding bond order constant

7. **AddBond tool panel** in `atom_edit_editor.dart`
   - Replace `SizedBox.shrink()` with `BondOrderSelector` + status text
   - `selectedOrder` bound to `AddBondToolState.bond_order` from Rust
   - `onOrderChanged` calls `set_add_bond_order` API

8. **Default tool panel: bond info** in `atom_edit_editor.dart`
   - When bond(s) selected: show count, current order label, and `BondOrderSelector`
   - `selectedOrder` derived from selected bonds (single value if all same, `null` if mixed)
   - `onOrderChanged` calls `change_selected_bonds_order` API

9. **Pointer event routing for AddBond tool** in Flutter viewport
   - Route left pointer down/move/up to `add_bond_pointer_down/move/up` when AddBond tool is active
   - Draw rubber-band line overlay from `AddBondMoveResult`

10. **Keyboard shortcuts** in Flutter
    - Hold B: spring-loaded tool activation with deferred release during drag
    - 1-7: bond order shortcuts routed by active tool and selection state

### Phase D: FRB codegen & testing

11. **Run `flutter_rust_bridge_codegen generate`** after API changes

12. **Tests**
    - Rust unit tests for `change_bond_order` operation (all 7 orders, selected bonds, provenance promotion, reject order 0/invalid)
    - Rust unit tests for AddBond drag state machine (idle → pending → dragging → create/cancel, with various bond orders)
    - Rust unit tests for bond order cycling in Default tool (single→double→triple→single, and specialized-order-enters-cycle-at-single)
    - Verify existing mutation tests still pass (bond creation with order 1 should be unchanged)

---

## 10. Key Files

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
