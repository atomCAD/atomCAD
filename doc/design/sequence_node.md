# Design: `sequence` Node

## Status: Draft

## Motivation

Several nodes accept `Array<T>` input pins (e.g., `union` takes `Array<Geometry>`). When
multiple wires connect to an array pin, the evaluator concatenates them — but the order is
determined by **source node ID** (creation order), which is invisible to the user and not
controllable.

For order-insensitive operations like CSG union this is fine. But order-sensitive operations
— such as composing atomic diffs sequentially — need explicit user control over element
ordering.

The `sequence` node solves this: it has N individually-wired input pins (numbered 0..N-1)
and produces an `Array<T>` output with elements in pin order. The user controls ordering by
wiring to specific numbered pins.

### Example Use Case

```
+------------+     +------------+     +------------+
| atom_edit  |     | atom_edit  |     | atom_edit  |
| (diff A)   |     | (diff B)   |     | (diff C)   |
+-----+------+     +-----+------+     +-----+------+
      |                   |                   |
      v                   v                   v
  +---+-------------------+-------------------+---+
  | sequence (element_type: Atomic)               |
  |   0: diff_a                                   |
  |   1: diff_b                                   |
  |   2: diff_c                                   |
  +------------------------+----------------------+
                           |
                           v  Array<Atomic>
                  +--------+--------+
                  | compose_diff    |
                  +-----------------+
```

## Prior Art in atomCAD

The `sequence` node reuses patterns already established by `map` and `expr`:

| Pattern | Source | Reuse in `sequence` |
|---------|--------|---------------------|
| User-selectable data type | `map` node (`input_type`, `output_type`) | `element_type` property |
| Dynamic pin list from data | `expr` node (`parameters: Vec<ExprParameter>`) | Pin count derived from `input_count` |
| `calculate_custom_node_type()` | Both `map` and `expr` | Generates N input pins + typed array output |
| `SetNodeDataCommand` for undo | Both `map` and `expr` | Same — JSON snapshot of `SequenceData` |
| `get/set_text_properties()` | Both `map` and `expr` | Exposes `element_type` and `count` |

## Design

### Data Structure

```rust
// rust/src/structure_designer/nodes/sequence.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceData {
    /// The element type for all input pins and the output array.
    pub element_type: DataType,
    /// Number of input pins (minimum 1).
    pub input_count: usize,
}

impl Default for SequenceData {
    fn default() -> Self {
        Self {
            element_type: DataType::Atomic,
            input_count: 2,
        }
    }
}
```

Compared to `expr`'s `Vec<ExprParameter>`, this is simpler: all pins share the same type
and names are just stringified indices (`"0"`, `"1"`, ...). No need for per-pin metadata.

### Base Node Type Registration

```rust
// In node_type_registry.rs

NodeType {
    name: "sequence".to_string(),
    description: "Collects inputs into an ordered array.".to_string(),
    summary: Some("Ordered array from numbered pins".to_string()),
    category: NodeTypeCategory::OtherBuiltin,
    parameters: vec![],  // overridden by calculate_custom_node_type
    output_pins: OutputPinDefinition::single(DataType::Array(Box::new(DataType::None))),
    node_data_creator: || Box::new(SequenceData::default()),
    node_data_saver: generic_node_data_saver::<SequenceData>,
    node_data_loader: sequence_data_loader,
    public: true,
}
```

The base type has empty parameters and a placeholder output — both are overridden at
runtime by the custom node type cache.

### Dynamic Node Type

Following the `map` node pattern (`map.rs:34-47`):

```rust
impl NodeData for SequenceData {
    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
        let mut custom = base_node_type.clone();

        // Generate numbered input pins, all of element_type
        custom.parameters = (0..self.input_count)
            .map(|i| Parameter {
                id: Some(i as u64),
                name: format!("{}", i),
                data_type: self.element_type.clone(),
            })
            .collect();

        // Output is Array<element_type>
        custom.output_pins = OutputPinDefinition::single(
            DataType::Array(Box::new(self.element_type.clone()))
        );

        Some(custom)
    }
}
```

**Parameter IDs** are set to the pin index (`i as u64`). When the user removes a pin from
the middle (e.g., removes pin 2 from a 5-pin sequence), pins 3 and 4 keep their original
IDs. The existing wire preservation logic in `network_validator.rs` matches old-to-new
parameters by ID, so wires to pins 3 and 4 are preserved and remapped to their new
positions.

### Properties

```rust
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    vec![
        ("element_type".into(), TextValue::DataType(self.element_type.clone())),
        ("count".into(), TextValue::Int(self.input_count as i64)),
    ]
}

fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    if let Some(TextValue::DataType(dt)) = props.get("element_type") {
        self.element_type = dt.clone();
    }
    if let Some(TextValue::Int(n)) = props.get("count") {
        let n = *n as usize;
        if n < 1 {
            return Err("sequence requires at least 1 input".into());
        }
        self.input_count = n;
    }
    Ok(())
}
```

### Evaluation

```rust
fn eval<'a>(
    &self,
    network_evaluator: &NetworkEvaluator,
    network_stack: &[NetworkStackElement<'a>],
    node_id: u64,
    registry: &NodeTypeRegistry,
    _decorate: bool,
    context: &mut NetworkEvaluationContext,
) -> EvalOutput {
    let mut items = Vec::new();

    for i in 0..self.input_count {
        let val = network_evaluator.evaluate_arg(
            network_stack, node_id, registry, context, i,
        );
        match val {
            NetworkResult::None => {} // unconnected pin — skip
            other => items.push(other),
        }
    }

    EvalOutput::single(NetworkResult::Array(items))
}
```

Unconnected pins are skipped (not errors). This matches the `union` node's behavior and
lets users leave gaps while building the graph.

### Serialization

```rust
// Saver: use generic_node_data_saver::<SequenceData> (registered in NodeType above)

pub fn sequence_data_loader(
    value: &Value,
    _design_dir: Option<&str>,
) -> io::Result<Box<dyn NodeData>> {
    let data: SequenceData = serde_json::from_value(value.clone())?;
    Ok(Box::new(data))
}
```

`.cnnd` representation:

```json
{
  "node_type_name": "sequence",
  "node_data": {
    "element_type": "Atomic",
    "input_count": 3
  }
}
```

### Undo

Uses `SetNodeDataCommand` (same as `map` and `expr`). Any change to `element_type` or
`input_count` snapshots the full `SequenceData` JSON before and after. No new undo command
types needed.

### Text Format

```
my_seq = sequence(element_type: Atomic, count: 3) {
    0: diff_a,
    1: diff_b,
    2: diff_c,
}
```

The existing text format parser already handles named pin connections (`pin_name: source`).
Since pin names are `"0"`, `"1"`, etc., no parser changes are needed.

## Implementation Phases

### Phase 1: Rust Node

1. Create `rust/src/structure_designer/nodes/sequence.rs`
   - `SequenceData` struct with `Serialize`/`Deserialize`
   - `NodeData` impl: `calculate_custom_node_type`, `eval`, `get/set_text_properties`
   - Saver and loader functions
2. Register in `nodes/mod.rs` and `node_type_registry.rs`
3. Tests in `rust/tests/structure_designer/`:
   - Basic evaluation (2 inputs, 3 inputs, N inputs)
   - Unconnected pins are skipped
   - Type selector changes output array type
   - Changing `input_count` preserves wires via parameter IDs
   - Roundtrip serialization (`.cnnd` save/load)
   - Text format roundtrip

### Phase 2: Flutter UI

1. Property panel: type selector dropdown + count stepper (+ / - buttons)
2. Verify node widget renders numbered input pins correctly
3. Verify wire connections work with the dynamic pin count

### Phase 3: Undo Integration

1. Verify `SetNodeDataCommand` works for `element_type` and `count` changes
2. Test: change count, undo → pin count and wires restored
3. Test: change type, undo → type and output restored

## Resolved Questions

1. **Default element type:** `Atomic` — the primary use case is composing atomic diffs
   sequentially (for the planned `atom_composediff` node).

2. **Maximum pin count:** No cap. Matches `expr` node behavior.

3. **Pin removal UX:** Removing the last pin only (decrement count). No per-pin delete buttons.
