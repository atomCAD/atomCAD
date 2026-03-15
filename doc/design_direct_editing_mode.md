# Direct Editing Mode — Design Document

## Motivation

atomCAD's node network paradigm is powerful but intimidating for beginners. New users
who simply want to build molecular structures must first understand nodes, wires, pins,
and data flow — a steep learning curve before they can place their first atom.

**Direct Editing Mode** provides a streamlined entry point: the application opens in a
simplified view focused entirely on the atom_edit editor. Advanced users can switch to
the full **Node Network Mode** when they need parametric, non-destructive editing.

## Goals

1. First-time users can start building atomic structures immediately.
2. The simplified UI hides node-network concepts entirely.
3. Same `.cnnd` file format — no separate "simple" format.
4. Clean upgrade path to Node Network Mode when the user is ready.

## Non-Goals (future work)

- Template gallery / starter structures.
- Simplified viewport toolbar with large tool icons.
- Quick element palette / periodic table popup.
- "Upgrade to Node Network" prompts when the user attempts an operation that requires nodes.

---

## Mode Definitions

### Direct Editing Mode

The default mode when the application starts. The `.cnnd` file contains a single network
named `Main` with a single `atom_edit` node that is active (displayed + set as return node).

### Node Network Mode

The current full-featured editor with all panels, menus, and capabilities.

---

## Switching Between Modes

### Direct Editing → Node Network

Available via:
- **View menu**: "Switch to Node Network Mode"
- **Display section widget**: radio button in the simplified Display panel

Always allowed — the user can switch at any time.

### Node Network → Direct Editing

Available via:
- **View menu**: "Switch to Direct Editing Mode" (grayed out if criteria not met)
- **Display section widget**: radio button (grayed out if criteria not met)

**Criteria for switching back:**
The active network must contain exactly one node, that node must be of type `atom_edit`,
and it must be both displayed and set as the return node. If the criteria are not met,
the menu item / radio button is disabled with a tooltip explaining why
(e.g., "Direct Editing Mode requires a single active atom_edit node").

> **Rationale:** We deliberately keep the criteria simple and strict. Attempting to
> map arbitrary node networks back to "direct editing" would be fragile and confusing.

---

## UI Layout

### Direct Editing Mode Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  MENU BAR (30px)                                                │
├──────────────────────┬──────────────────────────────────────────┤
│  LEFT SIDEBAR        │                                          │
│  (~280px, resizable) │          3D VIEWPORT                     │
│                      │          (fills remaining space)         │
│  ┌────────────────┐  │                                          │
│  │ Display        │  │                                          │
│  │ (simplified)   │  │                                          │
│  ├────────────────┤  │                                          │
│  │ Camera Control │  │                                          │
│  ├────────────────┤  │                                          │
│  │ Atom Edit      │  │                                          │
│  │ Editor         │  │                                          │
│  │ (scrollable)   │  │                                          │
│  │                │  │                                          │
│  └────────────────┘  │                                          │
└──────────────────────┴──────────────────────────────────────────┘
```

**Hidden panels:**
- Node Networks panel (left sidebar section)
- Node Network editor (Graph / Text tabs)
- Node Data / Properties panel (the atom_edit editor replaces it in the sidebar)

**Key difference from Node Network Mode:**
The atom_edit editor panel moves from the bottom-right properties panel into the left
sidebar, below Camera Control. The entire bottom section (node network editor + properties)
is removed — the viewport occupies all of the main content area.

### Node Network Mode Layout

Unchanged from current layout (left sidebar + resizable viewport/node-editor split).

---

## Simplified Display Section

In Direct Editing Mode the Display section contains only:

1. **Atomic visualization toggle**: Ball and Stick / Space Filling (same as current)
2. **Mode switch**: two radio buttons — "Direct Editing" (selected) / "Node Network"

**Hidden in Direct Editing Mode:**
- Geometry visualization buttons (Surface Splatting / Wireframe / Solid) — not relevant
  for pure atomic editing
- Node Display policy buttons (Manual / Prefer Selected / Prefer Frontier) — meaningless
  with a single node

---

## Menu Bar

### File Menu

| Item | Direct Editing | Node Network | Notes |
|------|:-:|:-:|-------|
| New | Yes | Yes | In direct mode, creates a fresh single-atom_edit .cnnd |
| Load Design | Yes | Yes | After load, auto-detect which mode to enter (see below) |
| Save Design | Yes | Yes | |
| Save Design As | Yes | Yes | |
| Export visible | Yes | Yes | |
| Import from .cnnd library | **No** | Yes | Advanced feature, node-network only |

### View Menu

| Item | Direct Editing | Node Network | Notes |
|------|:-:|:-:|-------|
| Switch to Node Network Mode | Yes | — | Switches mode |
| Switch to Direct Editing Mode | — | Yes (conditional) | Grayed out if criteria not met |
| Reset node network view | **No** | Yes | No visible node graph |
| Switch layout (H/V) | **No** | Yes | Direct mode uses fixed horizontal layout |

### Edit Menu

| Item | Direct Editing | Node Network | Notes |
|------|:-:|:-:|-------|
| Undo | Yes | Yes | |
| Redo | Yes | Yes | |
| Validate active network | **No** | Yes | Internal node concept |
| Auto-Layout Network | **No** | Yes | No visible node graph |
| Preferences | Yes | Yes | |

---

## Atom Edit Editor — Direct Mode Adaptations

Rather than creating a separate widget (too much code duplication), the existing
`AtomEditEditor` widget receives a `bool directEditingMode` parameter. When `true`,
the following elements are hidden:

| Element | Why hidden |
|---------|-----------|
| Header row: "Atom Edit" title + NodeDescriptionButton | The mode itself implies atom editing; the node description button is a node-network concept |
| Output mode toggle (Result / Diff segmented button) | Defaults to Result; Diff is a node-network concept (delta from base input) |
| Output mode options row (show anchors, include base bonds) | Only relevant in Diff mode |
| Diff stats row (+X atoms, -Y atoms, etc.) | Only relevant when there's a base input |
| "Error on stale entries" checkbox | Advanced node-network option |

**Everything else remains visible:** tool buttons, element selector, measurement display,
energy minimization, hydrogen passivation, transform section, bond order selector.

> **Rationale:** The hidden elements are all related to the node having an input wire
> carrying a base atomic structure (the Diff workflow). In direct editing mode there is
> no base input — the atom_edit node creates atoms from scratch — so these controls would
> be non-functional or confusing. The remaining UI (tools, measurements, minimization,
> passivation, transforms) is exactly what a direct-editing user needs.

---

## Initial State

When the application starts (or "New" is selected in Direct Editing Mode):

- One network named `Main`
- One `atom_edit` node, set as return node and displayed
- **Active tool**: Add Atom (so the user can immediately start placing atoms)
- **Default element**: Carbon
- **Default hybridization**: Auto
- **Output mode**: Result (hardcoded in direct mode — the toggle is hidden)

---

## Loading a .cnnd File — Mode Auto-Detection

When a `.cnnd` file is loaded, the application checks whether the Direct Editing Mode
criteria are met (single `atom_edit` node, active, displayed, return node in the active
network). If so, it enters Direct Editing Mode. Otherwise, it enters Node Network Mode.

This means:
- Files saved from Direct Editing Mode re-open in Direct Editing Mode.
- Files with complex node networks open in Node Network Mode.
- The mode is not persisted in the file — it is derived from structure.

---

## Keyboard Shortcuts

All existing atom_edit keyboard shortcuts work identically in both modes:

| Shortcut | Action |
|----------|--------|
| F2 | Default tool |
| F3 | Add Atom tool |
| F4 / hold J | Add Bond tool |
| D | Default tool (from viewport) |
| Q | Add Atom tool (from viewport) |
| 1-7 | Set bond order |
| Delete / Backspace | Delete selected |
| Ctrl+H | Add hydrogen to selected |
| Ctrl+Shift+H | Remove hydrogen from selected |
| Ctrl+M | Minimize selected |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z / Ctrl+Y | Redo |

**Disabled in Direct Editing Mode:**
- Ctrl+C / Ctrl+X / Ctrl+V / Ctrl+D (node copy/cut/paste/duplicate — no visible graph)

> Note: Atom-level copy/paste could be a future convenience feature but is out of scope.

---

## Implementation Plan

### Phase 1: Mode State and Switching

1. Add `bool directEditingMode` property to `StructureDesignerModel`.
2. Add `bool canSwitchToDirectEditingMode` getter that checks the criteria.
3. Add `switchToDirectEditingMode()` / `switchToNodeNetworkMode()` methods.
4. On `newProject()`, set `directEditingMode = true` and create the initial
   single-atom_edit-node network.
5. On `loadNodeNetworks()`, auto-detect mode based on criteria.

### Phase 2: Menu Bar Conditional Rendering

1. In `structure_designer.dart`, wrap menu items with `if (!model.directEditingMode)`
   guards for items hidden in direct mode.
2. Add "Switch to Node Network Mode" / "Switch to Direct Editing Mode" items to
   the View menu, with conditional enable/disable.

### Phase 3: Layout Switching

1. In `structure_designer.dart`, conditionally render the left sidebar:
   - **Direct mode**: Display (simplified) + Camera Control + Atom Edit Editor
     (scrollable, from `NodeDataWidget` or directly `AtomEditEditor`)
   - **Node Network mode**: current layout (Display + Camera + Node Networks panel)
2. In `main_content_area.dart`, conditionally hide the node network editor panel:
   - **Direct mode**: viewport fills entire main content area (no resizable split)
   - **Node Network mode**: current resizable split
3. Left sidebar width: ~280px in direct mode (wider than current 200px to accommodate
   the atom_edit editor controls comfortably).

### Phase 4: Simplified Display Section

1. Create a `DirectModeDisplayWidget` (or parameterize the existing Display section)
   that shows only atomic visualization toggle + mode radio buttons.
2. Mode radio buttons call `switchToNodeNetworkMode()` /
   `switchToDirectEditingMode()` on the model.

### Phase 5: Atom Edit Editor Parameterization

1. Add `bool directEditingMode` parameter to `AtomEditEditor` (default `false`).
2. Wrap the hidden elements (header row, output mode toggle, diff options, error on
   stale entries) with `if (!widget.directEditingMode)` guards.
3. No other changes to the editor logic.

### Phase 6: Initial State Setup

1. Ensure `newProject()` creates the correct initial state:
   single network "Main", single `atom_edit` node, active + displayed + return node.
2. Set default tool to Add Atom when entering direct editing mode.
3. Ensure hybridization defaults to Auto.

---

## Open Questions

1. **Left sidebar width**: 280px is a guess. The atom_edit editor has measurement
   displays and segmented buttons that may need more room. Should be tested and
   tuned during implementation.
2. **Preferences in Direct Mode**: Should the Preferences window also be simplified
   (hiding node-network-specific settings)? For now, keep it as-is — minimal scope.
3. **"New" in Node Network Mode**: Currently creates a blank network. Should it also
   offer "New (Direct Editing)" vs "New (Node Network)"? For now, "New" always creates
   a direct-editing-mode project (single atom_edit node). Users in Node Network Mode
   can add more nodes/networks immediately after.
