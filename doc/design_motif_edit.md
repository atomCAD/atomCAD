# Design: Visual Motif Editor (`motif_edit` node)

## Summary

A new `motif_edit` node type that provides a visual/interactive editor for crystal motifs, reusing the existing `atom_edit` infrastructure. Backed by the same `AtomEditData` struct via the dual-registration pattern (like `lattice_move` / `atom_lmove`). The editor works in **Cartesian space** internally, converting to fractional motif coordinates at the output boundary.

## Motivation

Motifs are currently edited only via the textual `motif` node (PARAM/SITE/BOND commands). While powerful, this is cumbersome for spatial reasoning — placing atoms at fractional coordinates, visualizing bonds across cell boundaries, and iterating on geometry. A visual editor would let users:

- Place and move atoms interactively in 3D, with positions automatically converted to fractional coordinates
- See neighboring cells and create cross-cell bonds visually
- Use parameter elements as placeholders for element substitution in `atom_fill`
- Edit existing atomic structures into motifs (e.g., convert an imported XYZ structure into a motif)

## Architecture: Dual-Registration Pattern

Extend `AtomEditData` with an `is_motif_mode` flag and register it as two node types. This is the same proven pattern used by `LatticeMoveData` → `lattice_move` / `atom_lmove` and `LatticeRotData` → `lattice_rot` / `atom_lrot`.

**Why this over alternatives:**

| Approach | Pros | Cons |
|----------|------|------|
| **Dual-registration (chosen)** | Reuses all tools, selection, undo, UI, gadgets; minimal new code; proven pattern | AtomEditData grows slightly; some conditional branches in eval |
| Composition (MotifEditData wraps AtomEditData) | Clean separation | Tools/API tightly coupled to AtomEditData internals; delegation awkward |
| Shared trait extraction | Cleanest abstraction | Massive refactoring of ~5000 lines across 13 files |

## Node Definition

```
Node: "motif_edit"
Category: AtomicStructure

Input Pins:
  0. "molecule" (DataType::Atomic)   — base atomic structure to edit as motif
  1. "unit_cell" (DataType::UnitCell) — basis vectors for coordinate conversion
  2. "tolerance" (DataType::Float)    — positional matching tolerance

Output Pins:
  0. "result" (DataType::Motif)  — the constructed motif in fractional coordinates
  1. "diff"   (DataType::Atomic) — raw diff structure (for debugging/inspection)
```

For comparison, atom_edit's pins:
```
atom_edit Input:  0. "molecule" (Atomic), 1. "tolerance" (Float)
atom_edit Output: 0. "result" (Atomic),   1. "diff" (Atomic)
```

## Core Design Decisions

### 1. Base Molecule Input (Diff-Based, Same as atom_edit)

motif_edit retains atom_edit's diff-based architecture with a base molecule input. The full `apply_diff()` pipeline — provenance tracking, anchor matching, delete/unchanged markers — is reused as-is. This provides:

- **Editing existing structures into motifs**: Import an XYZ file, connect it, and edit it into a motif
- **Parametric motif editing**: The base input can come from upstream nodes; edits layer on top non-destructively
- **No-input mode**: When nothing is connected, the diff itself is the full content (already works in atom_edit)
- **Free infrastructure**: All diff logic, provenance, undo, tools already handle this correctly

The only difference from atom_edit is at the **output boundary**: pin 0 produces `NetworkResult::Motif` instead of `NetworkResult::Atomic`, by converting the result AtomicStructure to a Motif using the provided unit cell.

### 2. Internal Representation: Cartesian Space

The diff stores atoms in **Cartesian coordinates**, exactly like atom_edit. This is essential because all existing subsystems (tools, guided placement, selection, dragging, gadgets, UFF minimization, renderer) work in Cartesian.

Conversion to fractional coordinates happens only at eval time via `unit_cell.real_to_dvec3_lattice()`.

### 3. Parameter Elements via Reserved Atomic Numbers

The motif system uses negative atomic numbers for parameter elements: `-1` = first parameter, `-2` = second, etc. However, atom_edit already uses special atomic numbers:
- `0` = delete marker (`DELETED_SITE_ATOMIC_NUMBER`)
- `-1` = unchanged marker (`UNCHANGED_ATOMIC_NUMBER`)

**Solution:** Use a reserved range **-100 to -199** for parameter elements in the internal AtomicStructure:
- `-100` = PARAM_1 (maps to motif parameter index 0, i.e., `-1` in Motif)
- `-101` = PARAM_2 (maps to motif parameter index 1, i.e., `-2` in Motif)
- etc.

The conversion at eval time is: `motif_atomic_number = -(internal_atomic_number + 99)`.

This avoids any conflict with delete markers (0), unchanged markers (-1), or real elements (1+).

### 4. Cross-Cell Bonds

Motif bonds use `SiteSpecifier` with `relative_cell: IVec3` to reference atoms in neighboring unit cells. The visual editor needs to support creating these bonds interactively.

#### Ghost Atoms

During rendering, generate **ghost atoms** — copies of motif atoms translated to neighboring cells using unit cell vectors. Ghosts are:
- Generated on-the-fly in the display layer (not stored in the diff)
- Flagged with a new `ATOM_FLAG_GHOST` bit in `Atom.flags`
- Visually distinguished from primary cell atoms (e.g., desaturated/dimmed color via the tessellator, without requiring alpha blending)
- Selectable as bond targets in the Add Bond tool

#### Neighbor Depth Parameter

Showing all atoms in all 26 neighboring cells would be visually overwhelming — the primary cell atoms would be lost in the noise. Instead, a **neighbor depth** parameter (float, 0.0 to 1.0) controls how far into neighboring cells ghost atoms are shown.

For each atom in each neighboring cell, compute its fractional distance from the nearest face of the primary cell. If that distance exceeds the depth threshold, the ghost is not shown.

**Example — diamond cubic (default zincblende motif):**
- Interior atoms sit at fractional positions like (0.25, 0.25, 0.25) — they are 0.25 fractional units from the nearest cell boundary
- With depth = 0.3, these atoms ARE shown as ghosts (0.25 < 0.3), which is exactly what's needed since they participate in cross-cell bonds
- Atoms deeper in the cell (e.g., at 0.5, 0.75) are NOT shown
- **Default value: 0.3** — covers diamond-family bonding geometry with minimal visual clutter

**UI exposure:**
- `neighbor_depth: f64` property in AtomEditData (motif mode only), shown as a slider (0.0–1.0) in the node editor panel
- Default: 0.3
- Serialized to .cnnd

#### Cross-Cell Bond Creation

When the user creates a bond from a primary cell atom to a ghost atom:
1. Identify which primary cell atom the ghost corresponds to (by matching position modulo unit cell translation)
2. Compute the `relative_cell` IVec3 offset from the ghost's cell position
3. Store the bond in the diff AtomicStructure (between the two primary-cell atom IDs)
4. Store the `relative_cell` metadata in `cross_cell_bonds: HashMap<BondReference, IVec3>`, normalized per the convention below

**Offset normalization convention:** `BondReference` is order-insensitive (`(A,B) == (B,A)`), so the stored IVec3 needs a canonical direction. The convention is: **the IVec3 is the cell offset of `max(atom_id1, atom_id2)` relative to `min(atom_id1, atom_id2)`**. At insertion time, if the user draws from atom `from` to ghost of atom `to` in cell `offset`:

```rust
let normalized = if from < to { offset } else { -offset };
map.insert(BondReference::new(from, to), normalized);
```

At motif conversion time, adjust for actual site order:

```rust
let raw_offset = cross_cell_bonds.get(&bond_ref).copied().unwrap_or(IVec3::ZERO);
let site_2_offset = if atom_a < atom_b { raw_offset } else { -raw_offset };
```

This is deterministic regardless of which direction the user draws the bond, composes with `BondReference`'s existing semantics, and requires no new types.

**Symmetric ghost bonds:** When the user creates a cross-cell bond (e.g., from A in cell (0,0,0) to ghost of B in cell (+1,0,0)), the system also creates the symmetric counterpart visible from the other direction: a bond from B in the primary cell to the ghost of A in cell (-1,0,0). Both are visual representations of the same physical bond. Internally, only one canonical entry is stored in `cross_cell_bonds`; the display layer generates the symmetric rendering by negating the stored offset. This way the user sees the bond from whichever direction they're looking, and the bond can be created from either side.

#### Wireframe Bounding Box

Render the unit cell as a wireframe parallelepiped:
- 12 line segments for the primary cell edges (opaque)
- Optionally, neighboring cell edges (faded)
- Generated in the display/tessellation layer when the interactive node is motif_edit

### 5. AtomEditData Changes

New fields (only used in motif mode, all serialized except `cached_unit_cell`):

```rust
pub struct AtomEditData {
    // ... existing fields ...

    /// True for motif_edit nodes, false for atom_edit nodes.
    pub is_motif_mode: bool,

    /// Parameter element definitions: (name, default_atomic_number).
    /// e.g., [("PRIMARY", 6), ("SECONDARY", 14)]
    /// Only used when is_motif_mode = true.
    pub parameter_elements: Vec<(String, i16)>,

    /// Cross-cell bond metadata: maps a bond (in the diff AtomicStructure)
    /// to the relative_cell offset of its second endpoint.
    /// Bonds not in this map are same-cell (relative_cell = IVec3::ZERO).
    /// Only used when is_motif_mode = true.
    pub cross_cell_bonds: HashMap<BondReference, IVec3>,

    /// How far into neighboring cells to show ghost atoms (0.0–1.0).
    /// 0.0 = no ghosts, 1.0 = full neighboring cells.
    /// Default: 0.3 (covers diamond-family cross-cell bonding).
    /// Only used when is_motif_mode = true.
    pub neighbor_depth: f64,

    /// Cached unit cell for interactive editing (avoids re-evaluating upstream).
    /// Transient, not serialized.
    pub cached_unit_cell: Mutex<Option<UnitCellStruct>>,
}
```

### 6. Display Override Mechanism (EvalOutput Extension)

#### The Problem

Pin 0 of motif_edit is declared as `DataType::Motif`. When a pin is visible, the scene generator calls `convert_result_to_node_output()` which checks the pin's `DataType` to decide how to render it. `Motif` is not a displayable type — it falls through to `NodeOutput::None`, producing no 3D visualization.

But we need to display a rich `AtomicStructure` in the viewport (with ghost atoms, wireframe box, selection highlights, etc.) while the wire still carries a `Motif` value for downstream nodes like `atom_fill`.

#### Existing Mechanisms (Insufficient)

The `decorate: bool` parameter in `eval()` already distinguishes display vs downstream evaluation — `generate_scene()` passes `decorate=true`, downstream nodes get `decorate=false`. However, `decorate` only adds visual metadata (selection, guide dots) to the **same type**. It was never designed to change the output type itself.

#### Solution: `display_results` in `EvalOutput`

Add an optional display override map to `EvalOutput`:

```rust
pub struct EvalOutput {
    pub results: Vec<NetworkResult>,
    /// Optional per-pin display overrides. When present for a pin index,
    /// the scene generator uses this value for viewport rendering instead
    /// of results[index]. Downstream wire evaluation always uses results[index].
    pub display_results: HashMap<usize, NetworkResult>,
}
```

**How the two paths diverge:**

| Path | Source | motif_edit pin 0 |
|------|--------|------------------|
| Downstream wire (`evaluate()`) | `results[pin]` | `NetworkResult::Motif(motif)` |
| Viewport display (`generate_scene()`) | `display_results[pin]` if present, else `results[pin]` | `NetworkResult::Atomic(viz_structure)` |

**Changes to existing code:**

1. **`EvalOutput`**: Add `display_results: HashMap<usize, NetworkResult>` field. Default: empty. Existing constructors (`single()`, `multi()`) produce an empty map — zero cost for all existing nodes.

2. **`generate_scene()`** in `network_evaluator.rs`: When converting each pin, check `eval_output.display_results.get(pin_index)` first. If present, pass it to `convert_result_to_node_output()` with the display result's **actual type** (auto-detected from the `NetworkResult` variant, e.g., `Atomic` → `DataType::Atomic`), not the declared pin type. Fall back to the wire result as today.

3. **`evaluate()`**: No change — always returns `results[pin_index]`. Display overrides are invisible to downstream consumers.

This mechanism is general-purpose: any future node that needs a different display type than its wire type can use it.

### 7. Evaluation (Motif Mode)

```
fn eval_motif_mode(&self, input: AtomicStructure, unit_cell: UnitCellStruct,
                   decorate: bool) -> EvalOutput {
    // 1. Apply diff to input (standard apply_diff pipeline)
    let diff_result = apply_diff(&input, &self.diff, self.tolerance);
    let result_structure = diff_result.result;

    // 2. Convert result AtomicStructure → Motif:
    //    a. Build sites: for each atom in result_structure:
    //       - fractional_pos = unit_cell.real_to_dvec3_lattice(atom.position)
    //       - Map atomic_number: -100 → -1, -101 → -2, etc.
    //       - Positive atomic numbers stay as-is
    //    b. Build bonds: for each bond in result_structure:
    //       - site_1 index and site_2 index from atom ordering
    //       - Look up relative_cell from cross_cell_bonds (default: IVec3::ZERO)
    //       - site_1 always gets relative_cell = IVec3::ZERO
    //    c. Build parameter list from self.parameter_elements

    // 3. Construct Motif { parameters, sites, bonds, bonds_by_site*_index }

    // 4. Build display visualization (AtomicStructure with ghost atoms)
    let mut viz = result_structure.clone();
    //    a. Generate ghost atoms: for each of the 26 neighboring cells,
    //       copy atoms whose fractional distance from primary cell < neighbor_depth,
    //       translate by unit cell vectors, flag with ATOM_FLAG_GHOST
    //    b. Generate symmetric ghost bonds for cross-cell bonds
    //    c. If decorate: apply selection highlights, tool state, guide dots
    //       (same decoration logic as atom_edit)

    // 5. Return with display override
    let mut output = EvalOutput::multi(vec![
        NetworkResult::Motif(motif),      // pin 0 wire value
        NetworkResult::Atomic(diff),      // pin 1
    ]);
    output.display_results.insert(0,
        NetworkResult::Atomic(viz),       // pin 0 display value
    );
    output
}
```

When no molecule input is connected, the diff itself is the full content (apply_diff with an empty base produces the diff's additions as the result).

### 7. Node Registration

```rust
// In atom_edit_data.rs:
pub fn get_node_type_motif_edit() -> NodeType {
    NodeType {
        name: "motif_edit".to_string(),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter::new("molecule", DataType::Atomic),
            Parameter::new("unit_cell", DataType::UnitCell),
            Parameter::new("tolerance", DataType::Float),
        ],
        output_pins: vec![
            OutputPinDefinition { name: "result".into(), data_type: DataType::Motif },
            OutputPinDefinition { name: "diff".into(), data_type: DataType::Atomic },
        ],
        node_data_creator: || Box::new(AtomEditData::new_motif_mode()),
        node_data_saver: generic_node_data_saver::<SerializableAtomEditData>,
        node_data_loader: motif_edit_data_loader, // handles is_motif_mode = true
    }
}

// In node_type_registry.rs:
ret.add_node_type(atom_edit_get_node_type());       // existing
ret.add_node_type(motif_edit_get_node_type());      // new
```

## Rendering & Display

### Ghost Atoms
- Generated in the display/tessellation layer when the interactive node is motif_edit
- All 26 neighboring cells considered; atoms filtered by `neighbor_depth` parameter
- For each neighbor cell atom, compute fractional distance from nearest primary cell face; show only if distance < `neighbor_depth`
- New `ATOM_FLAG_GHOST` bit in `Atom.flags`
- Tessellator renders ghost atoms with desaturated/dimmed colors (no alpha blending required)
- Cross-cell bonds rendered symmetrically: both the primary→ghost and ghost→primary directions shown

### Unit Cell Wireframe
- New tessellation path: 12 line segments forming the parallelepiped from unit cell basis vectors `a`, `b`, `c`
- Primary cell box: opaque lines
- Neighboring cell boxes: faded lines (optional)

### Parameter Element Atoms
- Distinct rendering for atoms with parameter-element atomic numbers (-100, -101, ...)
- Per-parameter color scheme (different from periodic table colors)
- Possibly a letter label overlay ("P1", "P2", etc.)

### Cross-Cell Bond Styling
- Bonds whose `relative_cell != IVec3::ZERO` could be rendered with dashed lines or a different color to indicate they span cell boundaries

## Flutter UI Changes

### Element Selector
- When motif_edit is the active interactive node, prepend the defined parameter elements (PARAM_1, PARAM_2, ...) to the top of the existing element dropdown, before the real elements
- Each parameter element item uses the reserved atomic number (-100, -101, ...) as its value
- A visual separator (divider) between parameter elements and real elements distinguishes the two groups
- Selecting a parameter element sets `selected_atomic_number` to the reserved value, same as selecting any real element
- The `ElementSymbolAccumulator` (keyboard shortcut input) does not need changes — parameter elements are selected via the dropdown only

### Parameter Definition Panel
- Text fields in the node editor panel to define parameter element names and default elements
- e.g., "PRIMARY → C", "SECONDARY → Si"
- Edits update `AtomEditData.parameter_elements`

### Coordinate Display
- Optionally show fractional coordinates in the measurement/info display when motif_edit is active

## Undo Integration

- `cross_cell_bonds` and `parameter_elements` must be captured by the undo system
- The DiffRecorder already captures bond additions/deletions; cross_cell_bonds metadata should be recorded alongside bond deltas
- Parameter element definition changes can use the existing `AtomEditToggleFlagCommand` pattern or a new simple command

## Serialization

- New fields (`is_motif_mode`, `parameter_elements`, `cross_cell_bonds`) added to `SerializableAtomEditData`
- `is_motif_mode` defaults to `false` for backward compatibility
- `parameter_elements` defaults to empty vec
- `cross_cell_bonds` defaults to empty map
- All use `#[serde(default)]` for backward-compat loading of old .cnnd files

## Implementation Phases

| Phase | Description | Effort |
|-------|-------------|--------|
| 1 | EvalOutput display override: add `display_results` field, update `generate_scene()` to use it | Small |
| 2 | Core: `is_motif_mode` flag, `motif_edit` node registration, Cartesian→Motif conversion in eval, display override wiring | Small-Medium |
| 3 | Parameter elements: reserved atomic numbers, rendering, element selector UI | Medium |
| 4 | Unit cell wireframe: display layer tessellation, wireframe rendering | Small-Medium |
| 5 | Ghost atoms: generation, ghost flag, desaturated color rendering, neighbor depth parameter | Medium |
| 6 | Cross-cell bonds: bond-to-ghost detection, metadata storage, motif bond generation, symmetric rendering | Medium |
| 7 | Undo integration: cross_cell_bonds + parameter_elements in undo commands | Small |
| 8 | Serialization: save/load with new fields, backward compat | Small |
| 9 | Polish: coordinate display, bond styling, comprehensive testing | Medium |

---

### Phase 1: EvalOutput Display Override

**Goal:** Allow a node's eval to return a different `NetworkResult` for viewport display than for downstream wire consumption. After this phase, the infrastructure is in place but no node uses it yet.

#### 1.1 EvalOutput changes (`node_data.rs`)

Add an optional display override map to `EvalOutput`:

```rust
pub struct EvalOutput {
    pub results: Vec<NetworkResult>,
    /// Optional per-pin display overrides. When present for a pin index,
    /// the scene generator uses this value for viewport rendering instead
    /// of results[index]. Downstream wire evaluation always uses results[index].
    pub display_results: HashMap<usize, NetworkResult>,
}
```

Update constructors and methods:

- `single()` and `multi()`: initialize `display_results: HashMap::new()`.
- Add a convenience method:

```rust
impl EvalOutput {
    /// Set a display override for a specific output pin.
    pub fn set_display_override(&mut self, pin_index: usize, result: NetworkResult) {
        self.display_results.insert(pin_index, result);
    }

    /// Get the display result for a pin, falling back to the wire result.
    pub fn get_display(&self, pin_index: usize) -> NetworkResult {
        self.display_results
            .get(&pin_index)
            .cloned()
            .unwrap_or_else(|| self.get(pin_index as i32))
    }
}
```

#### 1.2 Auto-detect DataType from NetworkResult

`convert_result_to_node_output()` dispatches on `DataType` to decide how to render a `NetworkResult`. When using a display override, the overridden result's type may differ from the declared pin type (e.g., pin declares `Motif`, but display override is `Atomic`). We need a way to infer the `DataType` from a `NetworkResult` variant.

Add a method to `NetworkResult` (in `network_result.rs`):

```rust
impl NetworkResult {
    /// Returns the DataType corresponding to this result's variant,
    /// or None for variants without a clear single type (None, Error, Function).
    pub fn infer_data_type(&self) -> Option<DataType> {
        match self {
            NetworkResult::Bool(_) => Some(DataType::Bool),
            NetworkResult::String(_) => Some(DataType::String),
            NetworkResult::Int(_) => Some(DataType::Int),
            NetworkResult::Float(_) => Some(DataType::Float),
            NetworkResult::Vec2(_) => Some(DataType::Vec2),
            NetworkResult::Vec3(_) => Some(DataType::Vec3),
            NetworkResult::IVec2(_) => Some(DataType::IVec2),
            NetworkResult::IVec3(_) => Some(DataType::IVec3),
            NetworkResult::UnitCell(_) => Some(DataType::UnitCell),
            NetworkResult::DrawingPlane(_) => Some(DataType::DrawingPlane),
            NetworkResult::Geometry2D(_) => Some(DataType::Geometry2D),
            NetworkResult::Geometry(_) => Some(DataType::Geometry),
            NetworkResult::Atomic(_) => Some(DataType::Atomic),
            NetworkResult::Motif(_) => Some(DataType::Motif),
            // Array, Function, None, Error: no single type or not displayable
            _ => None,
        }
    }
}
```

#### 1.3 generate_scene() changes (`network_evaluator.rs`)

The scene generator currently converts each pin's result using the declared pin `DataType`. Modify it to check `display_results` first.

**Pin 0 (backward-compat primary output):**

Current code (simplified):
```rust
let result = eval_output.get(0);
let node_type = registry.get_node_type_for_node(node).unwrap();
let (output, geo_tree) = self.convert_result_to_node_output(
    result,
    node_type.output_type(),
    ...
);
```

Changed to:
```rust
let (display_result_0, display_type_0) = if let Some(dr) = eval_output.display_results.get(&0) {
    let dt = dr.infer_data_type().unwrap_or_else(|| node_type.output_type().clone());
    (dr.clone(), dt)
} else {
    (eval_output.get(0), node_type.output_type().clone())
};
let (output, geo_tree) = self.convert_result_to_node_output(
    display_result_0,
    &display_type_0,
    ...
);
```

**Pin 1+ loop:**

Similarly, inside the `for pin_index_usize in 0..pin_count` loop, for `pin_index > 0`:

Current:
```rust
let pin_result = eval_output.get(pin_index);
let pin_data_type = node_type.get_output_pin_type(pin_index);
```

Changed to:
```rust
let (pin_result, pin_data_type) = if let Some(dr) = eval_output.display_results.get(&pin_index_usize) {
    let dt = dr.infer_data_type()
        .unwrap_or_else(|| node_type.get_output_pin_type(pin_index));
    (dr.clone(), dt)
} else {
    (eval_output.get(pin_index), node_type.get_output_pin_type(pin_index))
};
```

**No changes to `evaluate()` or `evaluate_all_outputs()`** — display overrides only affect the scene generation path. Wire evaluation always reads from `results[pin_index]`.

#### 1.4 evaluate_all_outputs() — propagate display_results

`evaluate_all_outputs()` currently returns `EvalOutput` from `node.data.eval()`. The `display_results` field flows through naturally since `EvalOutput` is returned as-is. However, for custom networks (user-defined subnetworks), the function reconstructs an `EvalOutput::multi(results)` from the child network's return node — this drops `display_results`. Add propagation:

```rust
// After evaluating child network return node:
let child_eval = self.evaluate_all_outputs(&child_network_stack, ...);
let results: Vec<NetworkResult> = child_eval.results.into_iter().map(|r| ...).collect();
let mut output = EvalOutput::multi(results);
output.display_results = child_eval.display_results; // propagate
output
```

Whether this is desirable for custom networks is debatable — a custom network's internal display overrides probably shouldn't leak out. But for Phase 1 no node uses display overrides yet, so this is a safe default. We can revisit if custom networks wrapping motif_edit become an issue.

#### 1.5 Testing

**Unit tests** (`rust/tests/structure_designer/`):

1. **`test_eval_output_display_override_basic`**: Create an `EvalOutput` with `results = [Motif(...)]` and `display_results = {0: Atomic(...)}`. Verify `get(0)` returns Motif, `get_display(0)` returns Atomic.

2. **`test_eval_output_display_override_fallback`**: Create an `EvalOutput` with no display overrides. Verify `get_display(0)` returns the wire result.

3. **`test_infer_data_type`**: Test `NetworkResult::infer_data_type()` for each variant.

4. **Integration test** (deferred to Phase 2): Full generate_scene() test with a motif_edit node verifying that the viewport gets Atomic while the wire carries Motif.

#### 1.6 Manual verification

After Phase 1, the only observable change is that `EvalOutput` has a new field that is always empty. All existing node evaluations should produce identical results. Run `cargo test` and `flutter run` to verify no regressions.

---

### Phase 2: Core motif_edit Node

**Goal:** A working `motif_edit` node that places atoms in Cartesian space (using all existing atom_edit tools) and outputs a Motif on pin 0. The viewport shows the Atomic visualization. No parameter elements, no ghost atoms, no cross-cell bonds — just basic same-cell motif editing.

**After this phase, you can:**
- Create a motif_edit node, connect a unit_cell
- Place atoms using the Add Atom tool
- Create bonds using the Add Bond tool
- See atoms in the viewport (Atomic display via display override)
- Wire pin 0 into atom_fill and get a valid Motif with sites at correct fractional positions
- Wire pin 1 (diff) for debugging

#### 2.1 AtomEditData changes (`atom_edit_data.rs`)

Add new fields:

```rust
pub struct AtomEditData {
    // ... existing fields ...

    /// True for motif_edit nodes, false for atom_edit nodes.
    /// Controls eval output type and display override behavior.
    pub is_motif_mode: bool,

    /// Cached unit cell for interactive editing (avoids re-evaluating upstream).
    /// Populated during eval(), read by tools during interaction.
    /// Transient — not serialized.
    pub cached_unit_cell: Mutex<Option<UnitCellStruct>>,
}
```

Note: `parameter_elements`, `cross_cell_bonds`, and `neighbor_depth` are deferred to later phases.

Add a new constructor:

```rust
impl AtomEditData {
    pub fn new_motif_mode() -> Self {
        let mut data = Self::new();
        data.is_motif_mode = true;
        data
    }
}
```

Initialize the new fields in `new()`:

```rust
pub fn new() -> Self {
    Self {
        // ... existing fields ...
        is_motif_mode: false,
        cached_unit_cell: Mutex::new(None),
    }
}
```

#### 2.2 Node registration (`atom_edit_data.rs`, `node_type_registry.rs`)

Add a second registration function in `atom_edit_data.rs`:

```rust
pub fn get_node_type_motif_edit() -> NodeType {
    NodeType {
        name: "motif_edit".to_string(),
        description: "Interactive motif editor. Places atoms in Cartesian space; \
            outputs a Motif with fractional coordinates computed from the unit cell. \
            Backed by the same diff-based architecture as atom_edit.\n\
            \n\
            Connect a unit_cell to define the basis vectors for coordinate conversion. \
            Pin 0 (result) outputs a Motif for use with atom_fill. \
            Pin 1 (diff) outputs the raw Atomic diff for inspection."
            .to_string(),
        summary: Some("Visual motif editor".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "molecule".to_string(),
                data_type: DataType::Atomic,
            },
            Parameter {
                id: None,
                name: "unit_cell".to_string(),
                data_type: DataType::UnitCell,
            },
            Parameter {
                id: None,
                name: "tolerance".to_string(),
                data_type: DataType::Float,
            },
        ],
        output_pins: vec![
            OutputPinDefinition {
                name: "result".to_string(),
                data_type: DataType::Motif,
            },
            OutputPinDefinition {
                name: "diff".to_string(),
                data_type: DataType::Atomic,
            },
        ],
        public: true,
        node_data_creator: || Box::new(AtomEditData::new_motif_mode()),
        node_data_saver: |node_data, _design_dir| {
            // Same saver as atom_edit — the serializable struct handles is_motif_mode
            if let Some(data) = node_data.as_any_mut().downcast_ref::<AtomEditData>() {
                let serializable = atom_edit_data_to_serializable(data)?;
                serde_json::to_value(serializable)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Data type mismatch for motif_edit",
                ))
            }
        },
        node_data_loader: |value, _design_dir| {
            // Same loader as atom_edit — is_motif_mode is restored from serialized data
            let serializable: SerializableAtomEditData = serde_json::from_value(value.clone())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Box::new(serializable_to_atom_edit_data(&serializable)?))
        },
    }
}
```

Register in `node_type_registry.rs`:

```rust
use super::nodes::atom_edit::atom_edit::get_node_type_motif_edit as motif_edit_get_node_type;
// ...
ret.add_node_type(motif_edit_get_node_type());
```

#### 2.3 Pin index mapping

atom_edit has two input pins: `molecule` (0), `tolerance` (1).
motif_edit has three: `molecule` (0), `unit_cell` (1), `tolerance` (2).

The `tolerance` pin shifts from index 1 to index 2. All code that evaluates tolerance via `evaluate_arg(..., 1)` or `evaluate_or_default(..., 1, ...)` must be aware of this.

**Solution:** Add a helper method that returns the tolerance pin index:

```rust
impl AtomEditData {
    /// Returns the pin index for the tolerance input.
    /// atom_edit: pin 1; motif_edit: pin 2 (unit_cell is pin 1).
    fn tolerance_pin_index(&self) -> usize {
        if self.is_motif_mode { 2 } else { 1 }
    }
}
```

Update the existing `eval()` tolerance evaluation:

```rust
// Before:
let tolerance = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context, 1, self.tolerance, ...
);

// After:
let tolerance = match network_evaluator.evaluate_or_default(
    network_stack, node_id, registry, context,
    self.tolerance_pin_index(), self.tolerance, ...
);
```

Check all other places in atom_edit code that reference pin indices (e.g., `evaluate_arg(..., 0)` for molecule input is fine since it's pin 0 in both modes).

#### 2.4 Evaluation: motif mode path (`atom_edit_data.rs`)

The existing `eval()` method is ~200 lines. Rather than littering it with conditionals, add a separate method for motif-mode evaluation that the main `eval()` dispatches to:

```rust
fn eval<'a>(&self, ...) -> EvalOutput {
    // ... existing input/tolerance evaluation ...

    if self.is_motif_mode {
        return self.eval_motif_mode(
            network_evaluator, network_stack, node_id, registry,
            decorate, context, input_structure, tolerance,
        );
    }

    // ... existing atom_edit eval code ...
}
```

The `eval_motif_mode` method:

```rust
fn eval_motif_mode<'a>(
    &self,
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    decorate: bool,
    context: &mut NetworkEvaluationContext,
    input_structure: AtomicStructure,
    tolerance: f64,
) -> EvalOutput {
    // 1. Get unit cell from pin 1
    let unit_cell_pin = 1; // motif_edit pin layout
    let unit_cell_val = network_evaluator.evaluate_arg(
        network_stack, node_id, registry, context, unit_cell_pin,
    );
    let unit_cell = match unit_cell_val {
        NetworkResult::UnitCell(uc) => uc,
        NetworkResult::None => {
            return EvalOutput::single(NetworkResult::Error(
                "unit_cell input required".to_string(),
            ));
        }
        NetworkResult::Error(e) => return EvalOutput::single(NetworkResult::Error(e)),
        _ => {
            return EvalOutput::single(NetworkResult::Error(
                "unit_cell: wrong type".to_string(),
            ));
        }
    };

    // Cache unit cell for interactive tools
    if let Ok(mut guard) = self.cached_unit_cell.lock() {
        *guard = Some(unit_cell.clone());
    }

    // 2. Apply diff (identical to atom_edit)
    let diff_result = apply_diff(&input_structure, &self.diff, tolerance);

    // (error_on_stale_entries check — same as atom_edit, factored into helper)

    let result_structure = diff_result.result;

    // 3. Convert AtomicStructure → Motif
    let motif = atomic_structure_to_motif(&result_structure, &unit_cell);

    // 4. Build diff output (pin 1) — same as atom_edit
    let mut diff_clone = self.diff.clone();
    if self.include_base_bonds_in_diff {
        enrich_diff_with_base_bonds(&mut diff_clone, &input_structure, tolerance);
    }
    // ... decoration logic (same as atom_edit) ...

    // 5. Build display visualization (pin 0 display override)
    let mut viz = result_structure.clone();
    if decorate {
        // Apply selection, tool state, guide dots — same as atom_edit result decoration
        // (reuse existing decoration code)
    }

    // 6. Build EvalOutput with display override
    let mut output = EvalOutput::multi(vec![
        NetworkResult::Motif(motif),       // pin 0 wire value
        NetworkResult::Atomic(diff_clone), // pin 1
    ]);
    output.set_display_override(0, NetworkResult::Atomic(viz)); // pin 0 display

    // Store eval cache
    if network_stack.len() == 1 {
        let eval_cache = AtomEditEvalCache {
            provenance: diff_result.provenance,
            stats: diff_result.stats,
        };
        context.selected_node_eval_cache = Some(Box::new(eval_cache));
    }

    output
}
```

#### 2.5 Cartesian→Motif conversion function

Add a conversion function (in `atom_edit_data.rs` or a new `motif_conversion.rs` helper within the atom_edit module):

```rust
use crate::crystolecule::motif::{Motif, Site, MotifBond, SiteSpecifier};

/// Converts an AtomicStructure (Cartesian) to a Motif (fractional coordinates).
/// Phase 2: no parameter elements, no cross-cell bonds.
fn atomic_structure_to_motif(
    structure: &AtomicStructure,
    unit_cell: &UnitCellStruct,
) -> Motif {
    let mut sites = Vec::new();

    // Build atom_id → site_index mapping for bond conversion
    let mut atom_id_to_site_index: HashMap<u32, usize> = HashMap::new();

    for (idx, atom) in structure.atoms().enumerate() {
        let frac_pos = unit_cell.real_to_dvec3_lattice(&atom.position);
        sites.push(Site {
            atomic_number: atom.atomic_number,
            position: frac_pos,
        });
        atom_id_to_site_index.insert(atom.id, idx);
    }

    // Convert bonds — Phase 2: all same-cell (relative_cell = IVec3::ZERO)
    let mut bonds = Vec::new();
    for bond in structure.bonds() {
        if let (Some(&idx1), Some(&idx2)) = (
            atom_id_to_site_index.get(&bond.atom_id_1),
            atom_id_to_site_index.get(&bond.atom_id_2),
        ) {
            bonds.push(MotifBond {
                site_1: SiteSpecifier {
                    site_index: idx1,
                    relative_cell: IVec3::ZERO,
                },
                site_2: SiteSpecifier {
                    site_index: idx2,
                    relative_cell: IVec3::ZERO,
                },
                multiplicity: bond.bond_order as i32,
            });
        }
    }

    // Build precomputed bond index maps
    Motif::new(vec![], sites, bonds)
}
```

This requires a `Motif::new()` constructor or using the existing builder pattern. Check what exists — if `Motif` uses direct field construction, build the `bonds_by_site1_index` / `bonds_by_site2_index` maps inline:

```rust
let site_count = sites.len();
let mut bonds_by_site1_index = vec![Vec::new(); site_count];
let mut bonds_by_site2_index = vec![Vec::new(); site_count];
for (bond_idx, bond) in bonds.iter().enumerate() {
    bonds_by_site1_index[bond.site_1.site_index].push(bond_idx);
    bonds_by_site2_index[bond.site_2.site_index].push(bond_idx);
}
Motif {
    parameters: vec![],
    sites,
    bonds,
    bonds_by_site1_index,
    bonds_by_site2_index,
}
```

#### 2.6 Refactoring eval() to share decoration logic

The atom_edit `eval()` method has ~80 lines of decoration code (applying selection highlights, tool state, guide dots). Rather than duplicating this in `eval_motif_mode()`, extract shared helpers:

```rust
impl AtomEditData {
    /// Apply selection + tool decoration to a result-space AtomicStructure.
    /// Used by both atom_edit and motif_edit eval paths.
    fn decorate_result(
        &self,
        result: &mut AtomicStructure,
        provenance: &DiffProvenance,
    ) {
        result.decorator_mut().from_selected_node = true;
        // Apply atom selection via provenance maps
        for &base_id in &self.selection.selected_base_atoms { ... }
        for &diff_id in &self.selection.selected_diff_atoms { ... }
        // Apply bond selection, selection transform, tool marks, measurement marks
        // ... (move existing code here)
    }

    /// Apply decoration to a diff-space AtomicStructure.
    fn decorate_diff(&self, diff: &mut AtomicStructure) {
        diff.decorator_mut().from_selected_node = true;
        // ... (move existing diff decoration code here)
    }
}
```

This keeps `eval()` and `eval_motif_mode()` clean while avoiding duplication.

#### 2.7 Serialization (`atom_edit_data_serialization.rs`)

Add `is_motif_mode` to `SerializableAtomEditData`:

```rust
#[derive(Serialize, Deserialize)]
pub struct SerializableAtomEditData {
    // ... existing fields ...

    #[serde(default)]
    pub is_motif_mode: bool,
}
```

Update `atom_edit_data_to_serializable()` and `serializable_to_atom_edit_data()` to include this field. The `#[serde(default)]` ensures backward compatibility — old .cnnd files load with `is_motif_mode = false`.

#### 2.8 Interactive node detection

The system needs to know that `motif_edit` is an interactive node (like `atom_edit`). Check how `atom_edit` is detected as interactive — likely via `provide_gadget()` returning `Some(...)` or checking the node type name. Since `motif_edit` shares `AtomEditData`, the gadget system already works: `provide_gadget()` on `AtomEditData` returns the atom edit gadget regardless of mode.

However, there may be places that check `node.node_type_name == "atom_edit"` by string. Search for these and add `|| node.node_type_name == "motif_edit"` where needed. A cleaner approach: add a helper `AtomEditData::is_atom_edit_family(name: &str) -> bool` or check via `node.data.as_any().downcast_ref::<AtomEditData>().is_some()`.

#### 2.9 Testing

**Unit tests:**

1. **`test_atomic_structure_to_motif_basic`**: Create an AtomicStructure with 2 atoms at known Cartesian positions, a cubic unit cell. Convert. Verify fractional positions match expected values.

2. **`test_atomic_structure_to_motif_bonds`**: Structure with atoms and bonds. Verify motif bonds have correct site indices and `relative_cell = IVec3::ZERO`.

3. **`test_atomic_structure_to_motif_empty`**: Empty input. Verify empty motif.

4. **`test_motif_edit_eval_no_unit_cell`**: Evaluate a motif_edit node with no unit_cell connected. Verify error result.

5. **`test_motif_edit_eval_basic`**: Full eval with unit_cell connected, a few atoms placed. Verify pin 0 is `NetworkResult::Motif(...)`, pin 1 is `NetworkResult::Atomic(...)`.

6. **`test_motif_edit_display_override`**: Verify `eval_output.display_results[0]` is `NetworkResult::Atomic(...)` while `eval_output.results[0]` is `NetworkResult::Motif(...)`.

7. **`test_motif_edit_roundtrip_coordinates`**: Place an atom at Cartesian position, convert to motif, convert back using `unit_cell.dvec3_lattice_to_real()`. Verify roundtrip within tolerance.

8. **`test_motif_edit_serialization_roundtrip`**: Serialize and deserialize AtomEditData with `is_motif_mode = true`. Verify flag preserved.

9. **`test_motif_edit_backward_compat_load`**: Load a SerializableAtomEditData JSON without `is_motif_mode` field. Verify defaults to `false`.

**Node snapshot test:** Add `motif_edit` to the existing node snapshot tests (insta) if the project uses them for registration validation.

#### 2.10 Manual verification

1. `flutter run`, create a motif_edit node
2. Connect a unit_cell node to pin 1
3. Use Add Atom tool to place atoms — they should appear in the viewport
4. Connect pin 0 to atom_fill — verify atoms appear at correct lattice positions
5. Add bonds — verify they show up and the motif output includes them
6. Verify undo/redo works for basic operations (the existing atom_edit undo infrastructure should apply since the underlying data is AtomEditData)

---

### Phase 3: Parameter Elements

**Goal:** Support parameter elements (PARAM_1, PARAM_2, ...) — atoms whose element is a variable to be resolved by atom_fill. After this phase, users can define parameter elements with names and defaults, place parameter atoms in the motif, and see them rendered distinctly.

**After this phase, you can:**
- Define parameter elements (e.g., "PRIMARY → C", "SECONDARY → Si") in the node panel
- Place atoms with parameter elements using the element selector
- See parameter atoms rendered with distinct per-parameter colors
- Output a Motif with correct negative atomic numbers and parameter definitions
- Wire into atom_fill and override parameters

#### 3.1 Reserved atomic number constants (`types.rs` or new `motif_constants.rs`)

```rust
/// First reserved atomic number for parameter elements.
/// PARAM_1 = -100, PARAM_2 = -101, etc.
pub const PARAM_ELEMENT_BASE: i16 = -100;

/// Maximum number of parameter elements supported.
pub const MAX_PARAM_ELEMENTS: usize = 100; // -100 to -199

/// Convert an internal parameter atomic number (-100, -101, ...)
/// to a motif parameter index (0, 1, ...).
pub fn param_atomic_number_to_index(atomic_number: i16) -> Option<usize> {
    if atomic_number <= PARAM_ELEMENT_BASE && atomic_number > PARAM_ELEMENT_BASE - MAX_PARAM_ELEMENTS as i16 {
        Some((PARAM_ELEMENT_BASE - atomic_number) as usize)
    } else {
        None
    }
}

/// Convert a motif parameter index (0, 1, ...) to an internal
/// reserved atomic number (-100, -101, ...).
pub fn param_index_to_atomic_number(index: usize) -> i16 {
    PARAM_ELEMENT_BASE - index as i16
}

/// Convert an internal reserved atomic number to the motif's
/// negative atomic number convention (-1, -2, ...).
pub fn param_atomic_number_to_motif(atomic_number: i16) -> i16 {
    -(param_atomic_number_to_index(atomic_number).unwrap() as i16 + 1)
}

/// Returns true if the atomic number is a parameter element.
pub fn is_param_element(atomic_number: i16) -> bool {
    param_atomic_number_to_index(atomic_number).is_some()
}
```

These are deliberately in a separate constants/helper file rather than scattered across modules, since both the Rust eval code and the display/tessellation layer need them.

#### 3.2 AtomEditData changes

Add the parameter elements field:

```rust
pub struct AtomEditData {
    // ... existing + Phase 2 fields ...

    /// Parameter element definitions: (name, default_atomic_number).
    /// e.g., [("PRIMARY", 6), ("SECONDARY", 14)]
    /// Only meaningful when is_motif_mode = true.
    pub parameter_elements: Vec<(String, i16)>,
}
```

Initialize in constructors:

```rust
// new():
parameter_elements: Vec::new(),

// new_motif_mode():
// Same — empty by default. User adds via UI.
```

#### 3.3 Update atomic_structure_to_motif()

Extend the conversion function from Phase 2 to handle parameter elements:

```rust
fn atomic_structure_to_motif(
    structure: &AtomicStructure,
    unit_cell: &UnitCellStruct,
    parameter_elements: &[(String, i16)],
) -> Motif {
    // Build parameters
    let parameters: Vec<ParameterElement> = parameter_elements
        .iter()
        .map(|(name, default_z)| ParameterElement {
            name: name.clone(),
            default_atomic_number: *default_z,
        })
        .collect();

    let mut sites = Vec::new();
    let mut atom_id_to_site_index: HashMap<u32, usize> = HashMap::new();

    for (idx, atom) in structure.atoms().enumerate() {
        let frac_pos = unit_cell.real_to_dvec3_lattice(&atom.position);

        // Map atomic number: parameter reserved range → motif convention
        let motif_z = if is_param_element(atom.atomic_number) {
            param_atomic_number_to_motif(atom.atomic_number)
        } else {
            atom.atomic_number
        };

        sites.push(Site {
            atomic_number: motif_z,
            position: frac_pos,
        });
        atom_id_to_site_index.insert(atom.id, idx);
    }

    // ... bonds same as Phase 2 ...

    Motif { parameters, sites, bonds, bonds_by_site1_index, bonds_by_site2_index }
}
```

#### 3.4 Rendering: parameter element colors

The tessellator converts `AtomicStructure` atoms to colored spheres. It looks up atom color from the periodic table using `atomic_number`. For parameter element atoms (atomic_number in -100..-199 range), the periodic table lookup will fail or return a default.

**Approach:** In the tessellation layer (likely `display/` module), add a check before periodic table lookup:

```rust
fn atom_color(atomic_number: i16, param_index: Option<usize>) -> Color {
    if let Some(idx) = param_index {
        PARAM_ELEMENT_COLORS[idx % PARAM_ELEMENT_COLORS.len()]
    } else if atomic_number > 0 {
        periodic_table_color(atomic_number)
    } else {
        DEFAULT_ATOM_COLOR
    }
}
```

Define a set of distinct parameter colors (6–8 should be plenty):

```rust
const PARAM_ELEMENT_COLORS: &[Color] = &[
    Color::from_rgb(0.9, 0.4, 0.9),  // Purple-pink (PARAM_1)
    Color::from_rgb(0.2, 0.8, 0.6),  // Teal-green (PARAM_2)
    Color::from_rgb(0.9, 0.7, 0.2),  // Gold (PARAM_3)
    Color::from_rgb(0.3, 0.5, 0.9),  // Blue (PARAM_4)
    // ... etc
];
```

The exact location of this change depends on how the tessellator currently resolves colors — search for where `atomic_number` is used to look up atom color/radius and add the parameter element path there.

**Atom radius:** Parameter elements don't have a meaningful covalent radius. Use a default (e.g., carbon's radius, 0.77 Å) or the default element's radius from `parameter_elements[idx].default_atomic_number`.

#### 3.5 Atom name/label for parameter elements

The element name display (in tooltips, info panel, etc.) needs to handle parameter elements. Wherever element names are looked up from atomic number, add:

```rust
fn element_name(atomic_number: i16) -> String {
    if let Some(idx) = param_atomic_number_to_index(atomic_number) {
        format!("P{}", idx + 1)  // "P1", "P2", ...
    } else {
        periodic_table_element_name(atomic_number)
    }
}
```

#### 3.6 Element selector UI (Flutter)

The element selector (used by Add Atom tool, element replacement) currently shows periodic table elements. When the active interactive node is `motif_edit`, add a "Parameter Elements" section.

**Detecting motif mode:** The Flutter model needs to know the active node is motif_edit. Options:
- Check `node_type_name == "motif_edit"` on the active interactive node
- Expose `is_motif_mode` via the API (add to `NodeView` or a dedicated query)

The simplest approach: check `node_type_name`. The element selector already knows which node is active.

**UI layout:**
- Above or below the periodic table, add a section "Parameters" with buttons for each defined parameter element
- Each button shows the parameter name (e.g., "PRIMARY") and the parameter color
- Clicking sets `selected_atomic_number` to the reserved value (-100, -101, ...)
- If no parameter elements are defined yet, show a hint: "Define parameters in the node panel"

**API additions:**
- `get_parameter_elements(node_id: u64) -> Vec<ParameterElementView>` — returns the current parameter definitions
- `ParameterElementView { name: String, default_element: String, color: u32, reserved_atomic_number: i16 }`

#### 3.7 Parameter definition panel (Flutter)

In the node property panel (shown when motif_edit is selected), add a section for parameter element management:

- List of defined parameter elements, each with:
  - Name text field (e.g., "PRIMARY")
  - Default element selector (dropdown or mini periodic table picker, e.g., "C", "Si")
  - Color indicator (read-only, assigned from the fixed palette)
  - Delete button
- "Add Parameter" button at the bottom

**API additions:**
- `add_parameter_element(node_id: u64, name: String, default_atomic_number: i16)`
- `remove_parameter_element(node_id: u64, index: usize)`
- `update_parameter_element(node_id: u64, index: usize, name: String, default_atomic_number: i16)`

These mutate `AtomEditData.parameter_elements` and trigger a re-evaluation.

**Validation:**
- Parameter names must be non-empty and unique within the node
- Default atomic number must be a valid element (1–118) — parameter elements cannot default to other parameter elements

#### 3.8 Serialization (`atom_edit_data_serialization.rs`)

Add to `SerializableAtomEditData`:

```rust
#[derive(Serialize, Deserialize)]
pub struct SerializableAtomEditData {
    // ... existing fields ...

    #[serde(default)]
    pub is_motif_mode: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameter_elements: Vec<SerializableParameterElement>,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableParameterElement {
    pub name: String,
    pub default_atomic_number: i16,
}
```

#### 3.9 Undo integration (lightweight)

Parameter element mutations (add/remove/update) need to be undoable. Two options:

**Option A (simple, recommended for Phase 3):** Use the existing `with_atom_edit_undo` pattern — snapshot the entire `parameter_elements` vec before and after the mutation. Since parameter element changes are infrequent and the vec is small, this is efficient enough.

**Option B (deferred):** A dedicated `AtomEditParameterElementCommand`. This is more precise but unnecessary until Phase 7.

For Phase 3, we can use simple snapshotting. The diff recorder doesn't need to know about parameter elements — they don't affect the diff AtomicStructure, only the Motif conversion.

#### 3.10 Testing

**Unit tests:**

1. **`test_param_atomic_number_roundtrip`**: Verify `param_index_to_atomic_number(0)` = -100, `param_atomic_number_to_index(-100)` = Some(0), etc.

2. **`test_param_atomic_number_to_motif`**: Verify -100 → -1, -101 → -2.

3. **`test_is_param_element`**: Verify true for -100..-199, false for 0, -1, 1, 6, -200.

4. **`test_motif_with_parameter_elements`**: Create AtomicStructure with atoms at atomic_number -100 and 6. Convert with `parameter_elements = [("PRIMARY", 6)]`. Verify motif has one ParameterElement, and the parameter atom site has `atomic_number = -1`.

5. **`test_motif_parameter_defaults`**: Verify parameter element default_atomic_number flows through to Motif correctly.

6. **`test_parameter_element_serialization`**: Roundtrip SerializableAtomEditData with parameter_elements.

7. **`test_parameter_element_backward_compat`**: Load without parameter_elements field — defaults to empty vec.

**Manual verification:**

1. Create motif_edit node, add two parameter elements ("PRIMARY → C", "SECONDARY → Si")
2. Select PARAM_1 in element selector, place an atom — should render in purple-pink
3. Select PARAM_2, place another — should render in teal-green
4. Connect to atom_fill — verify parameter substitution works
5. Save/load project — verify parameter elements persist
6. Undo adding a parameter element — verify it disappears

---

### Phase 4: Unit Cell Wireframe

**Goal:** Render the unit cell as a wireframe parallelepiped in the viewport when a `motif_edit` node is the active interactive node. This gives users spatial context for where the cell boundaries are.

**After this phase, you can:**
- See the primary unit cell as 12 opaque wireframe edges
- Visually understand where atoms sit relative to cell boundaries

#### 4.1 Wireframe geometry generation

Add a wireframe generation function (in the `display/` or tessellation layer) that takes a `UnitCellStruct` and produces 12 line segments — the edges of the parallelepiped formed by the three basis vectors `a`, `b`, `c` from origin `(0,0,0)`. The 8 vertices are: `O`, `O+a`, `O+b`, `O+c`, `O+a+b`, `O+a+c`, `O+b+c`, `O+a+b+c`.

#### 4.2 Integration with scene generation

In `generate_scene()` (or the display adapter), when the active interactive node is `motif_edit` and a unit cell is available (from `cached_unit_cell`), append the wireframe line segments to the scene data. Use a distinct line color (e.g., white or light gray) that contrasts with atom/bond colors.

**Critical:** The wireframe must be generated from `cached_unit_cell`, which is populated during `eval()`. If no unit cell is connected, no wireframe is shown (graceful degradation).

#### 4.3 Line rendering support

Check whether the existing renderer supports line primitives. If only triangles are supported, render wireframe edges as thin elongated cuboids or cylinder segments (same approach as bond sticks but thinner and with a wireframe color).

#### 4.4 Testing

**Automated tests:**
1. **`test_wireframe_vertices`**: Given a cubic unit cell with `a = (5,0,0)`, `b = (0,5,0)`, `c = (0,0,5)`, verify the 12 generated line segments connect the correct 8 vertices.
2. **`test_wireframe_non_orthogonal`**: Verify correct edges for a triclinic cell with non-orthogonal basis vectors.
3. **`test_wireframe_not_generated_without_unit_cell`**: Verify no wireframe segments when `cached_unit_cell` is `None`.

**Manual verification:**
1. Create `motif_edit` node, connect a cubic `unit_cell` — verify wireframe box appears
2. Change unit cell parameters — verify wireframe updates
3. Disconnect unit cell — verify wireframe disappears
4. Connect a non-orthogonal unit cell — verify parallelepiped (not a cube)
5. Verify wireframe does NOT appear when `atom_edit` is the active node

---

### Phase 5: Ghost Atoms

**Goal:** Show atoms from neighboring unit cells as dimmed "ghost" copies, controlled by the `neighbor_depth` parameter. This provides visual context for atoms near cell boundaries and is a prerequisite for cross-cell bond creation in Phase 6.

**After this phase, you can:**
- See ghost atoms in neighboring cells (dimmed/desaturated)
- Control ghost visibility with the neighbor_depth slider (0.0–1.0)
- Ghost atoms appear in the viewport but do NOT affect the Motif output

#### 5.1 AtomEditData changes

Add `neighbor_depth: f64` field to `AtomEditData` (default `0.3`, only used when `is_motif_mode`). Add the `ATOM_FLAG_GHOST` constant to the atom flags.

#### 5.2 Ghost atom generation

In `eval_motif_mode()`, after building the visualization structure (`viz`), generate ghosts:

For each of the 26 neighboring cells `(dx, dy, dz)` where `dx, dy, dz ∈ {-1, 0, 1}` and not `(0,0,0)`:
- For each atom in the result structure, compute its fractional position
- Compute the minimum distance from the atom's fractional position (offset by `(dx, dy, dz)`) to the nearest face of the primary cell `[0,1]^3`
- If distance < `neighbor_depth`, clone the atom, translate by `dx*a + dy*b + dz*c`, set `ATOM_FLAG_GHOST` flag, and add to `viz`

**Critical:** Ghost atoms are added to the **display** AtomicStructure only (the one stored in `display_results`), never to the wire result. They must have unique atom IDs that don't collide with real atom IDs (e.g., use a reserved ID range starting at `u32::MAX / 2`).

#### 5.3 Ghost atom rendering

In the tessellator, detect `ATOM_FLAG_GHOST` and render with desaturated/dimmed colors. A simple approach: multiply the atom's RGB color by a desaturation factor (e.g., blend 50% toward gray). No alpha blending required.

#### 5.4 Neighbor depth UI

Add a slider to the `motif_edit` node's property panel in Flutter:
- Label: "Neighbor Depth"
- Range: 0.0–1.0, step 0.01
- Default: 0.3
- API: `set_neighbor_depth(node_id: u64, depth: f64)` + `get_neighbor_depth(node_id: u64) -> f64`

#### 5.5 Serialization

Add `neighbor_depth` to `SerializableAtomEditData` with `#[serde(default = "default_neighbor_depth")]` where the default function returns `0.3`.

#### 5.6 Testing

**Automated tests:**
1. **`test_ghost_generation_cubic`**: Cubic unit cell, one atom at fractional `(0.1, 0.1, 0.1)` (close to corner). With `neighbor_depth = 0.3`, verify ghosts are generated in the 7 neighboring cells that share that corner.
2. **`test_ghost_generation_depth_zero`**: With `neighbor_depth = 0.0`, verify no ghost atoms are generated.
3. **`test_ghost_generation_depth_one`**: With `neighbor_depth = 1.0`, verify all 26 neighboring cells produce ghosts for all atoms.
4. **`test_ghost_atoms_not_in_wire_result`**: Evaluate `motif_edit`, verify `results[0]` (Motif) contains no ghost atoms, while `display_results[0]` (Atomic) does.
5. **`test_ghost_flag_set`**: Verify all ghost atoms have `ATOM_FLAG_GHOST` set and primary atoms do not.
6. **`test_neighbor_depth_serialization`**: Roundtrip with non-default neighbor_depth value.

**Manual verification:**
1. Create `motif_edit` with a diamond cubic motif and unit cell
2. Verify ghost atoms appear near cell boundaries (dimmed)
3. Slide neighbor_depth to 0 — ghosts disappear
4. Slide to 1.0 — full neighboring cells shown
5. Default 0.3 — interior diamond atoms at 0.25 from boundary should be visible as ghosts
6. Verify ghosts are NOT selectable with the selection tool (they are display-only for now; selectability comes in Phase 6 for bond creation)

---

### Phase 6: Cross-Cell Bonds

**Goal:** Enable creating bonds between atoms in the primary cell and ghost atoms in neighboring cells. These become motif bonds with non-zero `relative_cell` offsets, which is essential for defining crystal bonding topology.

**After this phase, you can:**
- Click on a primary cell atom, then a ghost atom, to create a cross-cell bond
- See the bond rendered symmetrically (primary→ghost and ghost→primary directions)
- Output a Motif with correct `relative_cell` values on cross-cell bonds
- Wire into atom_fill and get correct inter-cell bonding

#### 6.1 AtomEditData changes

Add `cross_cell_bonds: HashMap<BondReference, IVec3>` to `AtomEditData`. A `BondReference` identifies a bond in the diff (by the two atom IDs). Bonds not in this map are same-cell (`IVec3::ZERO`). The stored IVec3 follows the normalization convention from section 4: offset of `max(id1, id2)` relative to `min(id1, id2)`.

#### 6.2 Ghost atom selectability for bond tool

Make ghost atoms selectable **only** when the Add Bond tool is active. This requires:
- Storing ghost atom metadata (which primary atom ID it corresponds to, and which cell offset `(dx, dy, dz)`) alongside the ghost atoms in the display structure — use a side-channel map or encode in the atom ID
- When the bond tool click lands on a ghost atom, resolve it back to the primary atom + cell offset

**Critical:** Ghost atoms must NOT be selectable for move, delete, or other operations — only for the second endpoint of Add Bond. Accidental selection of ghosts for editing would be confusing.

#### 6.3 Bond creation logic

When creating a bond from atom A (primary) to ghost of atom B in cell `(dx, dy, dz)`:
1. Create a normal bond between A and B in the diff AtomicStructure
2. Normalize the offset: `normalized = if A < B { (dx,dy,dz) } else { (-dx,-dy,-dz) }`
3. Store `BondReference(A, B) → normalized` in `cross_cell_bonds`
4. The symmetric rendering is generated automatically in the display layer by negating the stored offset

#### 6.4 Update atomic_structure_to_motif()

When converting bonds, look up `cross_cell_bonds` and adjust for site order:

```rust
let raw_offset = cross_cell_bonds.get(&BondReference::new(atom_a, atom_b))
    .copied().unwrap_or(IVec3::ZERO);
// raw_offset is "max_id relative to min_id" — adjust for actual site_1/site_2 order
let site_2_offset = if atom_a < atom_b { raw_offset } else { -raw_offset };
```

Bonds not in the map get `IVec3::ZERO` (same-cell).

#### 6.5 Symmetric ghost bond rendering

In the display visualization, for each cross-cell bond with stored offset `raw` (max relative to min):
- Compute A's perspective: `offset_of_B = if A < B { raw } else { -raw }`
- Render bond from A's position to ghost_of_B's position (translated by `offset_of_B * unit_cell`)
- Render bond from B's position to ghost_of_A's position (translated by `-offset_of_B * unit_cell`)
- Optionally use dashed lines or a different color for cross-cell bonds

#### 6.6 Serialization

Add `cross_cell_bonds` to `SerializableAtomEditData` with `#[serde(default)]`. The `BondReference` and `IVec3` need serializable representations (e.g., `(u32, u32)` for atom IDs and `[i32; 3]` for the offset).

#### 6.7 Testing

**Automated tests:**
1. **`test_cross_cell_bond_to_motif`**: Create structure with atoms A (id=1) and B (id=2) and a bond marked as cross-cell. Store normalized offset `(1,0,0)` (offset of max=2 relative to min=1). Convert to motif with site_1=A, site_2=B. Verify `site_2.relative_cell = IVec3(1,0,0)`.
2. **`test_cross_cell_bond_to_motif_reversed_order`**: Same setup but with site_1=B, site_2=A. Verify `site_2.relative_cell = IVec3(-1,0,0)` (negated because site_2 is the lower ID).
3. **`test_cross_cell_bond_normalization`**: Create bond from A (id=5) to ghost of B (id=3) with raw offset `(1,0,0)`. Verify stored normalized offset is `(-1,0,0)` (negated because `from > to`). Create same bond from B to ghost of A with raw offset `(-1,0,0)`. Verify same normalized result.
4. **`test_cross_cell_bond_same_cell_default`**: Bond NOT in `cross_cell_bonds` → `relative_cell = IVec3::ZERO`.
5. **`test_cross_cell_bond_serialization`**: Roundtrip `cross_cell_bonds` through serialization.
6. **`test_symmetric_ghost_bond_positions`**: Given a cross-cell bond, verify that the display structure contains bond segments in both directions (A→ghost_B and B→ghost_A) with correct positions.
7. **`test_cross_cell_bond_undo`**: Create cross-cell bond, undo, verify `cross_cell_bonds` entry is removed.

**Manual verification:**
1. Create motif_edit with a unit cell, place two atoms near opposite cell faces
2. Use Add Bond tool: click atom A, then click ghost of atom B in the neighboring cell
3. Verify bond appears rendered in both directions (primary→ghost and ghost→primary)
4. Connect to atom_fill — verify the filled structure has correct inter-cell bonding
5. Save/load — verify cross-cell bonds persist
6. Undo the bond creation — verify cross-cell bond disappears
7. Try creating a bond between two primary-cell atoms — verify it works normally (same-cell, no entry in `cross_cell_bonds`)

---

### Phase 7: Undo Integration

**Goal:** Ensure all motif_edit-specific mutations are fully undoable: `cross_cell_bonds` changes, `parameter_elements` changes, and `neighbor_depth` changes.

**After this phase, you can:**
- Undo/redo parameter element add/remove/update
- Undo/redo cross-cell bond creation/deletion (metadata tracked alongside bond diff)
- Undo/redo neighbor_depth changes

#### 7.1 Cross-cell bond undo

The DiffRecorder already captures bond additions/deletions. Extend it to also snapshot `cross_cell_bonds` entries alongside bond deltas. When a bond that has a `cross_cell_bonds` entry is deleted, the undo command must restore that entry on undo.

**Approach:** In `add_bond_recorded` / `delete_bond_recorded`, also record the `cross_cell_bonds` entry (if any) in the delta. The `AtomEditMutationCommand` already stores the full diff delta — extend the delta struct to include `cross_cell_bond_changes: Vec<(BondReference, Option<IVec3>)>` (before/after pairs).

#### 7.2 Parameter element undo

Use the `with_atom_edit_undo` pattern: snapshot the `parameter_elements` vec before and after mutation. Since parameter changes are infrequent, a full vec snapshot is efficient enough (no need for a granular command type).

**Critical:** If a parameter element is removed while atoms with that parameter's reserved atomic number exist in the diff, those atoms become orphaned (their atomic number references a non-existent parameter). Decide on behavior: warn the user, or auto-convert orphaned parameter atoms to the parameter's default element.

#### 7.3 Neighbor depth undo

`neighbor_depth` is a simple float property. Use `with_atom_edit_undo` to capture before/after values, or add a lightweight command similar to `AtomEditToggleFlagCommand`.

#### 7.4 Testing

**Automated tests:**
1. **`test_undo_add_parameter_element`**: Add a parameter element, undo, verify it's removed. Redo, verify it's back.
2. **`test_undo_remove_parameter_element`**: Remove a parameter element, undo, verify it's restored with correct name and default.
3. **`test_undo_cross_cell_bond`**: Create cross-cell bond, undo, verify bond AND `cross_cell_bonds` entry are both removed. Redo, verify both restored.
4. **`test_undo_neighbor_depth`**: Change neighbor_depth, undo, verify old value restored.
5. **`test_undo_interleaved_motif_operations`**: Sequence of atom placement + parameter add + cross-cell bond, then undo all three in order.

**Manual verification:**
1. Add parameter elements, Ctrl+Z multiple times — verify they undo in order
2. Create cross-cell bonds, undo — verify the bond and its metadata both disappear
3. Change neighbor_depth slider, undo — verify slider returns to previous value

---

### Phase 8: Serialization

**Goal:** Save and load all motif_edit state to .cnnd files with full backward compatibility.

**After this phase, you can:**
- Save a project with motif_edit nodes and reload it with all state intact
- Load old .cnnd files (pre-motif_edit) without errors

#### 8.1 SerializableAtomEditData additions

Add all remaining fields to `SerializableAtomEditData` (some may already be added in earlier phases):
- `is_motif_mode: bool` (Phase 2)
- `parameter_elements: Vec<SerializableParameterElement>` (Phase 3)
- `neighbor_depth: f64` (Phase 5)
- `cross_cell_bonds: Vec<SerializableCrossCellBond>` (Phase 6)

All fields use `#[serde(default)]` or `#[serde(default = "...")]` for backward compatibility.

#### 8.2 SerializableCrossCellBond

```rust
#[derive(Serialize, Deserialize)]
pub struct SerializableCrossCellBond {
    pub atom_id_1: u32,
    pub atom_id_2: u32,
    pub relative_cell: [i32; 3],
}
```

#### 8.3 Migration / backward compatibility

- Old files without `is_motif_mode` → defaults to `false` (atom_edit behavior)
- Old files without `parameter_elements` → empty vec
- Old files without `cross_cell_bonds` → empty vec
- Old files without `neighbor_depth` → 0.3

**Critical:** Ensure that saving a file with a `motif_edit` node and loading it back produces identical state. This is the core roundtrip invariant.

#### 8.4 Testing

**Automated tests:**
1. **`test_motif_edit_full_serialization_roundtrip`**: Create AtomEditData in motif mode with parameter elements, cross-cell bonds, custom neighbor_depth. Serialize → deserialize → verify all fields match.
2. **`test_motif_edit_backward_compat_no_motif_fields`**: Deserialize a JSON blob with no motif-specific fields. Verify all defaults.
3. **`test_motif_edit_cnnd_roundtrip`**: Save a full network containing a motif_edit node to .cnnd, reload, verify the motif_edit node has correct state. (Extend existing `cnnd_roundtrip` tests.)
4. **`test_motif_edit_partial_fields`**: Deserialize with only some motif fields present (e.g., `is_motif_mode` but no `cross_cell_bonds`). Verify graceful defaults.

**Manual verification:**
1. Create a motif_edit node with atoms, parameter elements, cross-cell bonds, custom neighbor_depth
2. Save the project
3. Close and reopen — verify all state is preserved
4. Open an old .cnnd file (pre-motif_edit) — verify it loads without errors
5. Verify that an atom_edit node in the same file is not affected by the new fields

---

### Phase 9: Polish & Comprehensive Testing

**Goal:** Final polish — coordinate display, bond styling, edge case handling, and comprehensive end-to-end testing.

#### 9.1 Cross-cell bond styling

Render bonds with non-zero `relative_cell` using dashed lines or a distinct color (e.g., cyan or orange) to visually distinguish them from same-cell bonds. This helps users see the periodic bonding topology at a glance.

#### 9.2 Fractional coordinate display

When motif_edit is the active node, optionally show fractional coordinates in the measurement/info panel alongside Cartesian coordinates. This helps users verify atom placement relative to the unit cell.

**Implementation:** In the info panel widget (Flutter), detect motif_edit mode and use the cached unit cell to convert displayed positions to fractional form: `frac = unit_cell.real_to_dvec3_lattice(cartesian)`. Display as e.g., "Frac: (0.250, 0.250, 0.250)".

#### 9.3 Edge case handling

- **No unit cell connected:** Show an informative error/warning in the node (not just a generic error). The viewport should still show the Atomic diff (fallback: skip motif conversion, show atoms without wireframe/ghosts).
- **Atoms outside unit cell:** Decide on fractional coordinate wrapping. Recommended: do NOT wrap — let the user see fractional values > 1.0 or < 0.0, but add a warning indicator on out-of-range atoms.
- **Empty motif:** Gracefully handle zero atoms — produce an empty Motif with no sites/bonds.
- **Parameter atoms with no matching definition:** If `parameter_elements` is shorter than the highest parameter index used, warn but don't crash.

#### 9.4 Integration with atom_fill

Verify the full pipeline: `motif_edit` → wire Motif → `atom_fill` → filled structure. Test with:
- Simple cubic motif with 1 atom
- Diamond cubic motif with 2 atoms and cross-cell bonds
- Motif with parameter elements, overridden in atom_fill
- Motif converted from an imported XYZ file (base molecule input)

#### 9.5 Testing

**Automated tests (end-to-end):**
1. **`test_motif_edit_to_atom_fill_pipeline`**: Build a network: `unit_cell` → `motif_edit` → `atom_fill`. Place atoms, evaluate, verify filled structure positions.
2. **`test_motif_edit_diamond_cubic`**: Full diamond cubic motif with cross-cell bonds. Verify atom_fill produces correct diamond structure.
3. **`test_motif_edit_parameter_override`**: Motif with PARAM_1 defaulting to C. Connect to atom_fill, override to Si. Verify Si atoms in output.
4. **`test_motif_edit_base_molecule_input`**: Connect an XYZ-imported structure as base, add diff atoms, verify combined motif output.
5. **`test_motif_edit_all_tools`**: Exercise all interactive tools (add atom, add bond, delete, move, select) on a motif_edit node. Verify each operation works and is undoable.

**Manual verification (full workflow):**
1. Build a complete diamond cubic motif from scratch: create unit cell, create motif_edit, place 2 atoms at correct positions, create bonds (including cross-cell), connect to atom_fill, verify correct 3D crystal
2. Define parameter elements, place parameter atoms, override in atom_fill — verify substitution
3. Import an XYZ file as base molecule, edit into a motif, verify output
4. Undo/redo the entire editing session — verify each step reverses correctly
5. Save, close, reopen — verify identical state
6. Stress test: rapid atom placement + undo + redo sequences, verify no crashes or state corruption

## Open Questions

1. **Parameter element count**: Support a fixed maximum (e.g., 4 parameter elements) or arbitrary? The motif text format supports arbitrary, but a fixed set simplifies the UI. A practical middle ground might be: arbitrary in the data model, but the UI shows up to N with an "add more" button.

2. **Neighbor depth default value**: The design uses 0.3 based on diamond cubic analysis (interior atoms at 0.25 from boundary). Should we verify this works well for other common crystal structures (e.g., hexagonal, BCC, FCC)? Might need per-structure tuning or a smarter adaptive default based on the motif content.

3. **Importing existing text motifs**: Should motif_edit be able to load a text-format motif definition (converting fractional→Cartesian using the unit cell)? This would enable round-tripping between the textual `motif` node and the visual `motif_edit` node. Could be a "paste motif text" action.

4. **Guided placement in motif mode**: The guided placement system supports `BondLengthMode::Crystal` (lattice-derived bond lengths) alongside `BondLengthMode::Uff`. Should motif_edit default to crystal bond lengths? Or is free placement sufficient for typical motif editing?

5. **Hydrogen passivation**: Typically applied by atom_fill after lattice filling, not on the motif. Probably disable in motif mode initially?

6. **UFF minimization**: May not be physically meaningful for a periodic unit cell fragment with dangling bonds at cell boundaries. Disable in motif mode, or leave available for users who want it?

7. **Cross-cell bond creation UX**: When the user draws a bond to a ghost atom, should we auto-detect the matching primary atom and relative_cell? Or require explicit mode/modifier? Auto-detection seems most natural — the ghost atom's identity is unambiguous since we generated it.

8. **Atom-to-site mapping stability**: When converting AtomicStructure → Motif, site indices are assigned by atom ordering. If atoms are added/removed, site indices change, which could break downstream bond references in atom_fill. Should we use stable site IDs? Or is re-generating the full Motif on each eval acceptable (bonds are internal to the motif anyway)?

9. **Motif validation**: Should eval validate the generated motif (e.g., all sites within 0-1 fractional range, no duplicate positions)? Or allow arbitrary fractional coordinates and let atom_fill handle edge cases?

10. **Fractional coordinate wrapping**: If an atom is placed slightly outside the unit cell bounding box, should its fractional coordinate be wrapped to [0, 1)? Or preserved as-is? Wrapping maintains the convention but could surprise users who place atoms near cell boundaries.

## Future Work

- **Semi-transparent ghost atoms**: The renderer currently does not support alpha blending. Once transparency support is added, ghost atoms should be rendered semi-transparently rather than with desaturated colors, providing a clearer visual distinction from primary cell atoms.
