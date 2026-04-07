# atom_replace Node Design

A node that replaces elements in an atomic structure based on user-defined replacement rules.

## Motivation

When designing atomic structures, it is common to want to substitute one element for another — for example, replacing all carbon atoms with silicon, or oxygen with sulfur. Currently this requires manually editing individual atoms. The `atom_replace` node provides a declarative, non-destructive way to perform bulk element substitution as part of the node network.

## Design

### 1. Node Definition

- **Name:** `atom_replace`
- **Category:** `AtomicStructure`
- **Description:** Replaces elements in an atomic structure according to a list of replacement rules. Each rule maps a source atomic number to a target atomic number. The target may be the delete marker (atomic number 0), which removes matched atoms from the structure. Atoms whose element is not listed in any rule pass through unchanged.

**Pins:**

| Direction | Name     | Type   | Required |
|-----------|----------|--------|----------|
| Input     | molecule | Atomic | Yes      |
| Output    | (pin 0)  | Atomic | —        |

Single input, single output. No additional input pins — the replacement rules are stored as node data properties, not as wired inputs.

### 2. Data Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomReplaceData {
    /// List of (from_atomic_number, to_atomic_number) replacement rules.
    /// Each pair maps atoms of element `from` to element `to`.
    pub replacements: Vec<(i16, i16)>,
}
```

Default: empty `replacements` vec (no substitutions — structure passes through unchanged).

### 3. Evaluation

```rust
fn eval(&self, ...) -> EvalOutput {
    // 1. Evaluate required "molecule" input (pin 0)
    // 2. Clone the AtomicStructure
    // 3. Build a HashMap<i16, i16> from self.replacements for O(1) lookup
    // 4. Iterate atoms (skip delete/unchanged markers on source side):
    //    a. If mapping target is 0 (delete marker): collect atom_id for deletion
    //    b. Otherwise: set atom.atomic_number to the target value
    // 5. Delete collected atoms via AtomicStructure::delete_atom() (handles bond cleanup)
    // 6. Return EvalOutput::single(NetworkResult::Atomic(result))
}
```

Note: deletion must be done in a separate pass after iteration to avoid mutating during traversal. Collect atom IDs to delete first, then call `delete_atom()` for each.

Key behaviors:
- If `replacements` is empty, the structure passes through unmodified (clone only).
- If a source element does not exist in the input structure, the rule is silently ignored — no warning.
- If multiple rules map the same source element, the last one wins (HashMap insertion order).
- Atoms that are already delete markers (`atomic_number == 0`) or unchanged markers (`atomic_number == -1`) are never replaced (source side).
- A target atomic number of `0` (delete marker) is valid — this effectively deletes all atoms of the source element. When an atom is replaced with the delete marker, its bonds are also removed (use `AtomicStructure::delete_atom()`).

### 4. Text Format Properties

The replacement list is exposed as a `TextValue::Array` of `TextValue::IVec2` pairs, where `x` = from and `y` = to:

```rust
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    let items: Vec<TextValue> = self.replacements.iter()
        .map(|(from, to)| TextValue::IVec2(IVec2::new(*from as i32, *to as i32)))
        .collect();
    vec![("replacements".to_string(), TextValue::Array(items))]
}

fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    if let Some(v) = props.get("replacements") {
        if let TextValue::Array(items) = v {
            self.replacements = items.iter()
                .map(|item| {
                    let iv = item.as_ivec2()
                        .ok_or("each replacement must be an IVec2")?;
                    Ok((iv.x as i16, iv.y as i16))
                })
                .collect::<Result<Vec<_>, String>>()?;
        }
    }
    Ok(())
}
```

Text format example:
```
replace1 = atom_replace {
    replacements: [(6, 14), (8, 16)]
}
```

This replaces C→Si and O→S.

### 5. Subtitle

Display a compact summary of the replacement rules on the node:

- Empty: `"(no replacements)"`
- 1–3 rules: `"C→Si, O→S"` (using element symbols from `ATOM_INFO`)
- 4+ rules: `"C→Si, O→S, … (+2 more)"`
- Delete rules: `"H→(del)"` — target atomic number 0 displays as `(del)`

If the element symbol is not found for an atomic number, fall back to displaying the raw number.

### 6. Node Registration

```rust
// In get_node_type():
NodeType {
    name: "atom_replace".to_string(),
    description: "Replaces elements in an atomic structure. Define replacement rules \
                  mapping source elements to target elements. Atoms not matching any \
                  rule pass through unchanged.".to_string(),
    summary: None,
    category: NodeTypeCategory::AtomicStructure,
    parameters: vec![
        Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        },
    ],
    output_pins: OutputPinDefinition::single(DataType::Atomic),
    public: true,
    node_data_creator: || Box::new(AtomReplaceData { replacements: vec![] }),
    node_data_saver: generic_node_data_saver::<AtomReplaceData>,
    node_data_loader: generic_node_data_loader::<AtomReplaceData>,
}
```

### 7. Flutter UI

The node's property panel should display:
- A list of current replacement rules, each showing `[source element dropdown] → [target element dropdown]` with a delete button
- An "Add Replacement" button at the bottom

The **source element dropdown** lists all real elements (H, He, Li, ...).

The **target element dropdown** has an additional first entry: **"Delete"** (atomic number 0), which removes matched atoms entirely. The remaining entries are the normal element list (H, He, Li, ...).

This uses the existing node property panel pattern. The Flutter side calls a Rust API function to update the replacements list, which sets the `AtomReplaceData` and triggers re-evaluation.

API function needed:
```rust
pub fn set_atom_replace_replacements(node_id: u64, replacements: Vec<(i32, i32)>)
```

This is a standard node data mutation — wraps in undo command via the existing pattern.

### 8. Serialization

Uses `generic_node_data_saver` / `generic_node_data_loader` with serde. The `AtomReplaceData` struct derives `Serialize` and `Deserialize`, so `.cnnd` persistence works automatically.

JSON representation in `.cnnd`:
```json
{
    "replacements": [[6, 14], [8, 16]]
}
```

## Implementation Plan

### Phase 1: Rust Node (core)
1. Create `rust/src/structure_designer/nodes/atom_replace.rs`
2. Add `pub mod atom_replace;` to `nodes/mod.rs`
3. Register in `node_type_registry.rs`
4. Write tests in `rust/tests/structure_designer/atom_replace_test.rs`

### Phase 2: Flutter UI
1. Add API function for setting replacements
2. Add property panel widget for the replacement list
3. Wire up undo support (follows existing node data mutation pattern)

## Tests

Tests in `rust/tests/structure_designer/atom_replace_test.rs`:

- `atom_replace_empty_rules` — no replacements, structure passes through unchanged
- `atom_replace_single_rule` — replace one element (e.g., C→Si), verify only matching atoms change
- `atom_replace_multiple_rules` — multiple simultaneous replacements
- `atom_replace_no_matching_atoms` — rules for elements not present in structure, no error
- `atom_replace_preserves_bonds` — bonds remain intact after replacement
- `atom_replace_preserves_positions` — atom positions unchanged
- `atom_replace_delete_target` — target atomic number 0 deletes matched atoms and their bonds
- `atom_replace_delete_mixed` — mix of element replacements and deletions in one rule set
- `atom_replace_skip_markers` — existing delete markers and unchanged markers are not replaced
- `atom_replace_text_properties_roundtrip` — get/set text properties roundtrip
- `atom_replace_subtitle` — subtitle formatting with symbols
- `atom_replace_cnnd_roundtrip` — serialize/deserialize through .cnnd format
