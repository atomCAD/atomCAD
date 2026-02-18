# atom_edit UX Research Report

Research into extending the `atom_edit` node from a basic diff editor into a full-featured, intuitive atomic editor. Draws on Avogadro 2's UX as reference and analyzes gaps against atomCAD's current capabilities.

**Date:** 2026-02-18

---

## 1. Current State Summary

The `atom_edit` node has a **strong architectural foundation** (diff-based, provenance-tracked, non-destructive) but its **interactive UX is minimal**.

### What exists today

| Capability | Status | UX |
|---|---|---|
| Add atom | 3 tools: Default, AddAtom, AddBond | Click-to-place on drawing plane |
| Delete atom/bond | Via selection then button | Select, then click "Delete" button in panel |
| Replace element | Via selection then button | Select, choose element, click "Replace" |
| Move/rotate | Via selection then gadget | Select, drag transform gadget |
| Add bond | AddBond tool | Two-click workflow (click atom1, click atom2) |
| Delete bond | Via selection then button | Select bond, click "Delete" |
| Energy minimization | 3 freeze modes | Click button in panel |
| Selection | Single atom/bond ray-cast | Click with Shift/Ctrl modifiers |

### Key pain points

- Every operation requires **multiple steps** (select -> navigate to panel -> click button)
- No direct manipulation shortcuts (right-click, keyboard)
- No measurement capability
- No marquee/group selection
- No bond-centric manipulation (length, angle, torsion)
- No undo for individual diff edits

### Architecture strengths (already implemented)

- **Diff-based non-destructive editing** with provenance tracking
- **Provenance-stable selection** (survives re-evaluation)
- **Dual view modes** (Result view and Diff view with anchors)
- **Energy minimization** with FreezeBase / FreeAll / FreeSelected modes
- **Text format** for human-readable diffs and AI integration
- **Three-phase interaction pattern** (Gather -> Compute -> Mutate) avoiding borrow conflicts
- **Comprehensive testing** (60+ mutation tests, 55+ text format tests)

---

## 2. Avogadro 2 Feature Reference

Full reference from the [Avogadro 2 User Guide](https://two.avogadro.cc/docs/index.html).

### 2.1 Draw Tool

- **Left-click empty space** creates an atom of the currently selected element
- **Left-click existing atom** changes its element to the currently selected one
- **Left-click and drag** from atom to empty space creates atom + bond
- **Left-click and drag** from atom to atom creates a bond between them
- **Right-click atom** deletes it and all connected bonds
- **Right-click bond** deletes the bond
- **Type atomic symbol** (e.g. "O", "Si") to change the selected element
- **Click existing bond** cycles through single -> double -> triple
- **Press 1/2/3** to set bond order directly
- **Adjust Hydrogens** checkbox auto-adds/removes H to satisfy valency
- **Auto bond length adjustment** when changing element or bond order on free ends

### 2.2 Selection Tool

- **Left-click** selects single atom
- **Left-click and drag** (marquee) selects all atoms in rectangle
- **Double-click** selects all connected atoms (flood-fill through bonds)
- **Shift + click/drag** adds to selection
- **Ctrl + click** toggles selection state
- **Ctrl+A** selects all, **Ctrl+Shift+A** clears selection
- **Right-click empty** clears selection

### 2.3 Select Menu

- Select All / Select None / Invert Selection
- **Enlarge Selection** (grow to bonded neighbors)
- **Select by Element** (periodic table filter)
- **Select by Residue**
- **Select SMARTS** (chemical query language)

### 2.4 Manipulation Tool

- **Left-drag** on atom/selection translates in screen plane
- **Right-drag** rotates around selection center
- **Numeric entry** for precise translation/rotation values

### 2.5 Bond-Centric Manipulation Tool

- **Click bond** to select it; blue Bond Manipulation Plane appears
- **Drag bonding atoms** to adjust bond length (preserves internal geometry)
- **Drag bonding atoms** to adjust bond angles
- **Drag substituents** to adjust torsion angles (rotation around bond axis)
- **Drag from bond** to rotate the manipulation plane
- Precise values via Properties pane

### 2.6 Measure Tool

- Click 2 atoms: displays **distance**
- Click 3 atoms: displays **angle** at middle atom
- Click 4 atoms: displays **dihedral (torsion) angle**
- All measurements displayed as viewport overlays and in Measure pane

### 2.7 AutoOptimization Tool

- **Real-time optimization** while drawing/editing
- Continuously minimizes geometry as modifications are made
- Method selection based on molecule composition

### 2.8 Build Menu

- **Insert Fragment** (300+ common molecules)
- **Insert SMILES** (text -> 3D geometry via Open Babel)
- **Insert Peptide** / **DNA/RNA** builders
- **Cartesian Editor** (numeric coordinate editing in Angstroms, Bohrs, or Fractional)
- **Nanotube Builder**

### 2.9 Crystallography

- Add Unit Cell, set lattice parameters
- Space Group perception (via spglib)
- Reduce to Primitive Cell
- Supercell expansion

### 2.10 Undo/Redo

- **Ctrl+Z / Ctrl+Y** for individual edit undo/redo
- Named undo commands ("Undo Manipulate Atom")
- **Interactive edit merging**: dragging creates a single undo command (initial + final position)
- **Macro support**: multiple operations merged into one undo step
- Delete atom automatically includes connected bond removal in same undo command

---

## 3. Gap Analysis

### Category 1: Direct Manipulation Shortcuts (missing entirely)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Type element symbol** to change element | Very high | Works across all tools; eliminates panel navigation |
| **Right-click atom** to delete | Very high | Instant delete without select-then-button flow |
| **Right-click bond** to delete bond | High | Same as above for bonds |
| **Click atom** to change its element | High | Currently requires select -> replace -> pick element |
| **Click bond** to cycle bond order | High | No bond order cycling exists at all |
| **Drag atom-to-atom** to create bond | Medium | atom_edit has 2-click workflow; drag more natural |
| **Press 1/2/3** to set bond order | Medium | Keyboard shortcut for AddBond tool |

### Category 2: Selection (very primitive)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Marquee selection** (drag rectangle) | Very high | Essential for selecting groups of atoms |
| **Double-click** to select connected fragment | High | Flood-fill select all reachable through bonds |
| **Select by element** | High | "Select all carbon" is a common workflow |
| **Enlarge selection** (grow to neighbors) | Medium | Expand selection to bonded neighbors |
| **Invert selection** | Medium | Useful for "everything except these" operations |
| **Select All / Select None** shortcuts | Medium | Ctrl+A / Ctrl+Shift+A |

### Category 3: Measurement (completely absent)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Distance** between 2 atoms | Very high | Click two atoms -> show bond length in Angstroms |
| **Angle** between 3 atoms | Very high | Click three atoms -> show angle at middle atom |
| **Dihedral angle** between 4 atoms | High | Click four atoms -> show torsion angle |
| **Viewport overlay** with measurements | High | Display directly on 3D view, not just in panel |

### Category 4: Bond-Centric Manipulation (completely absent)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Adjust bond length** by dragging | High | Drag atom along bond axis; preserve geometry |
| **Adjust bond angle** | High | Rotate fragment while preserving substituents |
| **Adjust torsion angle** | High | Rotate around bond axis |
| **Visual bond manipulation plane** | Medium | Blue plane showing manipulation context |
| **Precise numeric entry** for bond params | Medium | Type exact bond length / angle values |

### Category 5: Auto-Optimization (partially exists)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Real-time auto-optimize while editing** | Very high | Minimize buttons exist but no continuous optimization |
| **Per-atom freeze constraints** | Medium | FreezeSelected exists but no per-atom UI |
| **Geometry constraints** (fix distance/angle/torsion) | Medium | Not in scope currently |

### Category 6: Building & Templates (mostly N/A for APM)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Fragment insertion** | Low | atomCAD's lattice-fill approach differs fundamentally |
| **SMILES input** | Low | Not directly applicable to crystal lattice editing |
| **Coordinate editor** (numeric) | Medium | Direct position editing useful for precision work |
| **Peptide/DNA builders** | N/A | Out of scope for APM |

### Category 7: Undo/Redo (absent at individual-edit level)

| Avogadro 2 Feature | Impact | atomCAD Adaptation Notes |
|---|---|---|
| **Ctrl+Z undo** individual edits | Very high | Currently only node-network-level undo exists |
| **Edit merging** (drag -> single undo) | High | Interactive transform should be one undo step |
| **Named undo commands** | Medium | "Undo Delete Atom", "Undo Replace Element" |

---

## 4. Prioritized Feature Roadmap

### Phase 1: Highest Impact / Lowest Effort (do these first)

These features have the **biggest UX payoff per line of code** because they eliminate multi-step workflows.

#### 1. Keyboard element switching

- **Impact:** Eliminates navigating to element picker for the most frequent operation
- **Effort:** Small -- intercept keyboard events in Flutter, map to `atom_edit_replace_selected()` or set active element
- **UX:** While any atom_edit tool is active, typing "C", "N", "O", "Si", etc. changes the active element (for AddAtom) or replaces selected atoms (for Default tool)

#### 2. Right-click context actions

- **Impact:** Instant delete without the select-then-button dance
- **Effort:** Small -- add right-click handler to existing ray-cast hit test
- **UX:** Right-click atom -> delete it. Right-click bond -> delete it. Right-click empty -> clear selection.

#### 3. Click-to-change-element (Default tool)

- **Impact:** Single click replaces an atom's element instead of: select -> scroll panel -> pick element -> click replace
- **Effort:** Small -- when Default tool active and an element is "armed", clicking an atom replaces it
- **UX:** Combined with keyboard element switching: type "N", click carbon -> it becomes nitrogen

#### 4. Bond order cycling + keyboard shortcuts

- **Impact:** Currently no way to change bond order interactively at all
- **Effort:** Small -- click bond in Default tool cycles single -> double -> triple (or use 1/2/3 keys)
- **UX:** Click a bond to cycle its order. Press 1/2/3 while bond selected to set order.

### Phase 2: Selection & Measurement (moderate effort)

#### 5. Marquee (rectangle) selection

- **Impact:** Selecting groups of atoms is the most requested missing feature for any 3D editor
- **Effort:** Medium -- need rectangle-to-frustum projection, test all atoms against frustum
- **UX:** Drag in empty space -> draw selection rectangle -> all atoms inside get selected
- **Modifier keys:** Shift+drag = add to selection, Ctrl+drag = toggle

#### 6. Double-click select connected

- **Impact:** Common operation: "select this whole fragment so I can move/delete it"
- **Effort:** Small -- BFS/DFS from clicked atom following bonds
- **UX:** Double-click an atom -> flood-fill select all atoms reachable through bonds

#### 7. Measurement tool

- **Impact:** Users constantly need to verify bond lengths and angles during editing
- **Effort:** Medium -- new tool mode, click up to 4 atoms, compute distance/angle/dihedral, overlay text
- **UX:** New "Measure" tool in tool selector. Click 2 atoms -> distance. Click 3 -> angle. Click 4 -> dihedral. Measurements persist as overlays until cleared.
- **Foundation:** The math (distance, angle, dihedral) is trivial; the work is in the viewport text overlay system

#### 8. Select by element

- **Impact:** "Select all hydrogens" or "select all carbons" for bulk operations
- **Effort:** Small -- iterate atoms, filter by atomic number, select matching
- **UX:** Panel button or keyboard shortcut that opens element filter

### Phase 3: Advanced Editing (higher effort)

#### 9. Undo/redo for individual diff edits

- **Impact:** Currently no way to undo a single atom placement without losing subsequent work
- **Effort:** High -- need an undo stack within the diff model, tracking individual mutations
- **UX:** Ctrl+Z reverses the last atom_edit operation (add, delete, move, replace)
- **Architecture:** Store a stack of `DiffMutation` commands; each undo pops and inverts

#### 10. Real-time auto-optimization

- **Impact:** Atoms snap to physically reasonable positions as you place them
- **Effort:** High -- need async/streaming minimize that runs after each edit, debounced
- **UX:** Toggle button: when enabled, after each atom add/move, run quick minimize (few iterations)
- **Foundation:** Minimize already works; need to make it incremental and non-blocking

#### 11. Numeric coordinate editor

- **Impact:** Precision editing when you know the exact position you want
- **Effort:** Medium -- panel with x/y/z fields for selected atom, write-back to diff
- **UX:** Select atom -> panel shows position -> edit numbers -> atom moves

#### 12. Bond-centric manipulation tool

- **Impact:** Precise control over bond geometry without manually computing positions
- **Effort:** High -- new tool with constrained manipulation (axis-locked drag, torsion)
- **UX:** Click a bond -> manipulation plane appears -> drag endpoints to adjust length/angle/torsion
- **Note:** Less critical for atomCAD's lattice-constrained world than for free-form molecular editors

---

## 5. Implementation Notes

### Phase 1 scope per feature

| Feature | Rust changes | Flutter changes | New API endpoints |
|---|---|---|---|
| Keyboard element switching | None (use existing replace API) | Key event handler, element symbol parser | None |
| Right-click delete | New `atom_edit_delete_at_ray()` | Right-click handler | 1 new endpoint |
| Click-to-change-element | New `atom_edit_replace_at_ray()` | Click handler when element armed | 1 new endpoint |
| Bond order cycling | New `atom_edit_cycle_bond_order()` | Click handler for bonds | 1 new endpoint |

### Architectural considerations

- **Keyboard element switching** needs a symbol accumulator (e.g. typing "S" then "i" within 500ms -> "Si" for silicon, vs "S" alone -> sulfur after timeout). Avogadro 2 handles this with sequential character matching.
- **Right-click actions** need a new interaction path: ray-cast -> identify target -> immediate action (no selection step). This is a new pattern for atom_edit; currently all ray-casts go through selection first.
- **Bond order cycling** requires a new diff operation: modify an existing bond's order. The current diff model supports bond add and bond delete, but not bond order change as a first-class operation. This may need to be modeled as delete + add with new order.
- **Undo/redo** (Phase 3) is architecturally the hardest. The diff model is currently stateless between edits. An undo stack would need to store inverse operations or snapshots of the diff before each mutation.

### atomCAD-specific adaptations

Several Avogadro 2 features need rethinking for atomCAD's lattice-constrained world:

- **Atom placement** is already lattice-snapped in atomCAD (via drawing plane), unlike Avogadro's free-space placement
- **Bond-centric manipulation** may be less useful since bond lengths/angles are largely determined by the crystal lattice
- **Auto-hydrogen adjustment** corresponds to atomCAD's passivation system, which is already a separate node
- **Template/fragment insertion** maps to atomCAD's motif + lattice fill paradigm rather than Avogadro's molecular fragment library

---

## 6. Key Files Reference

### Rust core

| File | Lines | Contents |
|---|---|---|
| `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs` | ~1863 | Node definition, interaction functions, minimize |
| `rust/src/structure_designer/nodes/atom_edit/text_format.rs` | ~352 | Serialization/parsing of diff text format |
| `rust/src/crystolecule/atomic_structure_diff.rs` | ~470 | `apply_diff()`, `enrich_diff_with_base_bonds()` |
| `rust/src/crystolecule/atomic_structure/mod.rs` | ~799 | Core `AtomicStructure` container |
| `rust/src/api/structure_designer/atom_edit_api.rs` | ~271 | Flutter API surface |

### Flutter UI

| File | Lines | Contents |
|---|---|---|
| `lib/structure_designer/node_data/atom_edit_editor.dart` | ~486 | Property panel UI (tools, buttons, minimize) |
| `lib/structure_designer/structure_designer_model.dart` | -- | Model methods calling Rust API |

### Tests

| File | Lines | Contents |
|---|---|---|
| `rust/tests/structure_designer/atom_edit_text_format_test.rs` | ~767 | 55 text format tests |
| `rust/tests/structure_designer/atom_edit_mutations_test.rs` | ~466 | 60+ mutation tests |

---

## 7. Sources

- [Avogadro 2 User Guide](https://two.avogadro.cc/docs/index.html)
- [Draw Tool](https://two.avogadro.cc/docs/tools/draw-tool.html)
- [Selection Tool](https://two.avogadro.cc/docs/tools/selection-tool.html)
- [Manipulation Tool](https://two.avogadro.cc/docs/tools/manipulation-tool.html)
- [Bond-Centric Manipulation Tool](https://two.avogadro.cc/docs/tools/bond-centric-manipulation-tool.html)
- [Measure Tool](https://two.avogadro.cc/docs/tools/measure-tool.html)
- [AutoOptimization Tool](https://two.avogadro.cc/docs/tools/autoopt-tool.html)
- [Template Tool](https://two.avogadro.cc/docs/tools/template-tool.html)
- [Select Menu](https://two.avogadro.cc/docs/menus/select-menu.html)
- [Build Menu](https://two.avogadro.cc/docs/menus/build-menu.html)
- [Extensions Menu](https://two.avogadro.cc/docs/menus/extensions-menu.html)
- [Drawing Molecules](https://two.avogadro.cc/docs/getting-started/drawing-molecules.html)
- [Making Selections](https://two.avogadro.cc/docs/getting-started/making-selections.html)
- [Optimizing Geometry](https://two.avogadro.cc/docs/optimizing-geometry/index.html)
