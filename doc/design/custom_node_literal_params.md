# Design: Literal Parameters for Custom Nodes

## Problem Statement

Custom node parameters are currently wire-only. When a user writes:

```
octahedron { size: 8 }
```

The literal value `8` is ignored with a warning, and the user must instead write:

```
sz = int { value: 8 }
octahedron { size: sz }
```

This is an implementation gap, not a deliberate design choice.

## Root Cause Analysis

### Current Data Flow

1. **Custom node registration** creates a NodeType with `node_data_creator: || Box::new(NoData {})`
2. **Every custom node instance** gets `NoData` as its data
3. **`NoData::get_text_properties()`** returns `vec![]` (empty)
4. **Wire-only check** in `apply_literal_properties()` rejects literals for parameters not in `text_prop_names`

### Key Code Locations

| Location | Role |
|----------|------|
| `structure_designer.rs:440-442` | Custom node registration with `NoData` |
| `network_editor.rs:407-454` | Wire-only enforcement during text parsing |
| `parameter.rs:61-77` | Parameter evaluation (wire or default, no literal check) |
| `node_data.rs:75-107` | `NoData` struct definition |

### The Wire-Only Check (network_editor.rs:443-454)

```rust
// text_prop_names comes from node.data.get_text_properties()
// For NoData, this is empty

if !valid_params.is_empty()
    && valid_params.contains(prop_name)      // "size" IS a valid param
    && !text_prop_names.contains(prop_name)  // "size" is NOT in text_prop_names (empty)
{
    self.result.add_warning("wire-only...");
    continue;  // Literal ignored!
}
```

### The Conflation Problem

`get_text_properties()` serves two distinct purposes:
1. **Capability declaration**: "What properties CAN this node accept?"
2. **Value serialization**: "What are the current stored values?"

For built-in nodes like `FloatData`, these align: it always reports `[("value", current_value)]`.

For custom nodes with `NoData`, both return empty, making all parameters wire-only.

---

## Solution Overview

Create a new `CustomNodeData` struct that can store literal values for custom node parameters, and modify the wire-only check to allow literals on custom node parameters.

### Key Insight

The wire-only check conflates two concerns:
1. **"Can this parameter accept literals?"** → Parsing/editing concern
2. **"What values are stored?"** → Serialization concern

These should be separated. For custom nodes, ALL parameters should accept literals (stored in `CustomNodeData`), regardless of what `get_text_properties()` returns.

---

## Implementation

### 1. CustomNodeData Struct (node_data.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomNodeData {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub literal_values: HashMap<String, TextValue>,
}

impl NodeData for CustomNodeData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        _network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        _registry: &NodeTypeRegistry,
        _decorate: bool,
        _context: &mut NetworkEvaluationContext
    ) -> NetworkResult {
        let node = NetworkStackElement::get_top_node(network_stack, node_id);
        NetworkResult::Error(format!("eval not implemented for node {}", node.node_type_name))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &HashSet<String>) -> Option<String> {
        None
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        // Only return stored values (correct for serialization)
        self.literal_values.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        for (k, v) in props {
            self.literal_values.insert(k.clone(), v.clone());
        }
        Ok(())
    }
}
```

### 2. Modified Wire-Only Check (network_editor.rs:443-454)

```rust
fn apply_literal_properties(...) {
    // ... existing code to get valid_params, text_prop_names ...

    // NEW: Check if this is a custom node
    let is_custom_node = self.registry.node_networks.contains_key(&node_type_name);

    for (prop_name, prop_value) in properties {
        if let Some(text_value) = Self::property_value_to_text_value(prop_value) {
            // Warn about unknown properties
            if !valid_params.is_empty()
                && !valid_params.contains(prop_name)
                && !text_prop_names.contains(prop_name)
            {
                self.result.add_warning(format!(
                    "Unknown property '{}' on node type '{}'",
                    prop_name, node_type_name
                ));
            }
            // Wire-only check: MODIFIED to allow custom node parameters
            else if !valid_params.is_empty()
                && valid_params.contains(prop_name)
                && !text_prop_names.contains(prop_name)
                && !is_custom_node  // NEW: Allow literals on custom node params
            {
                self.result.add_warning(format!(
                    "Parameter '{}' on '{}' is wire-only; literal value ignored",
                    prop_name, node_type_name
                ));
                continue;
            }
            literal_props.insert(prop_name.clone(), text_value);
        }
    }

    // ... rest of function ...
}
```

### 3. Update Custom Node Registration (structure_designer.rs:432-446)

```rust
pub fn add_node_network(&mut self, node_network_name: &str) {
    self.node_type_registry.add_node_network(NodeNetwork::new(
        NodeType {
            name: node_network_name.to_string(),
            description: "".to_string(),
            category: NodeTypeCategory::Custom,
            parameters: Vec::new(),
            output_type: DataType::None,
            node_data_creator: || Box::new(CustomNodeData::default()),
            node_data_saver: generic_node_data_saver::<CustomNodeData>,
            node_data_loader: generic_node_data_loader::<CustomNodeData>,
            public: true,
        }
    ));
}
```

### 4. Update Parameter Evaluation (parameter.rs:61-77)

```rust
fn eval<'a>(&self, ...) -> NetworkResult {
    // ... existing isolation check (lines 50-59) ...

    let parent_node_id = network_stack.last().unwrap().node_id;
    let mut parent_network_stack = network_stack.clone();
    parent_network_stack.pop();
    let parent_node = parent_network_stack.last().unwrap()
        .node_network.nodes.get(&parent_node_id).unwrap();

    // Check if parent node has wire connected
    if parent_node.arguments[self.param_index].is_empty() {
        // NEW: Check for stored literal value
        if let Some(custom_data) = parent_node.data
            .as_any()
            .downcast_ref::<CustomNodeData>()
        {
            if let Some(text_value) = custom_data.literal_values.get(&self.param_name) {
                if let Some(result) = text_value_to_network_result(text_value, &self.data_type) {
                    return result;
                }
            }
        }
        // Fall back to default pin
        return eval_default(...);
    }

    // Wire connected - evaluate it
    return network_evaluator.evaluate_arg_required(...);
}
```

### 5. Helper Function: TextValue to NetworkResult

Add to `text_format/text_value.rs` or `parameter.rs`:

```rust
pub fn text_value_to_network_result(value: &TextValue, expected_type: &DataType) -> Option<NetworkResult> {
    match (value, expected_type) {
        (TextValue::Int(i), DataType::Int) => Some(NetworkResult::Int(*i)),
        (TextValue::Float(f), DataType::Float) => Some(NetworkResult::Float(*f)),
        (TextValue::Bool(b), DataType::Bool) => Some(NetworkResult::Bool(*b)),
        (TextValue::String(s), DataType::String) => Some(NetworkResult::String(s.clone())),
        (TextValue::Vec2(v), DataType::Vec2) => Some(NetworkResult::Vec2(*v)),
        (TextValue::Vec3(v), DataType::Vec3) => Some(NetworkResult::Vec3(*v)),
        (TextValue::IVec2(v), DataType::IVec2) => Some(NetworkResult::IVec2(*v)),
        (TextValue::IVec3(v), DataType::IVec3) => Some(NetworkResult::IVec3(*v)),
        // Type coercion: int to float
        (TextValue::Int(i), DataType::Float) => Some(NetworkResult::Float(*i as f64)),
        _ => None,
    }
}
```

### 6. Update Describe Command (ai_assistant_api.rs)

The `describe` command shows `[wire-only]` for parameters. Update to check for custom nodes:

```rust
// When determining if a parameter is wire-only:
let is_custom_node = registry.node_networks.contains_key(&node_type_name);
let has_text_property = text_prop_names.contains(&param.name);
let is_wire_only = !has_text_property && !is_custom_node;

if is_wire_only {
    result.push_str(" [wire-only]");
}
```

---

## Evaluation Priority

When evaluating a custom node parameter, the priority is:

1. **Wire connected** → Evaluate the wire (highest priority)
2. **Literal stored** → Use the literal value
3. **Default pin** → Evaluate the default input pin inside custom node (lowest priority)

This matches user expectations: explicit values override defaults, wires override everything.

---

## Files to Modify

| File | Changes |
|------|---------|
| `node_data.rs` | Add `CustomNodeData` struct with `NodeData` impl |
| `structure_designer.rs:432-446` | Update `add_node_network()` to use `CustomNodeData` |
| `network_editor.rs:443-454` | Add `&& !is_custom_node` to wire-only check |
| `parameter.rs:61-77` | Add literal lookup before default fallback |
| `text_format/text_value.rs` | Add `text_value_to_network_result()` helper |
| `api/structure_designer/ai_assistant_api.rs` | Update describe command's wire-only display |

---

## Testing Strategy

### Unit Tests

1. **CustomNodeData serialization roundtrip**
   - Create CustomNodeData with literals, serialize, deserialize, verify values

2. **text_value_to_network_result conversion**
   - Test all type combinations including int→float coercion

### Integration Tests

1. **Text format parsing**
   - `octahedron { size: 8 }` → verify literal is stored
   - `octahedron { size: sz }` where `sz = int { value: 8 }` → verify wire works

2. **Evaluation priority**
   - Custom node with literal only → uses literal
   - Custom node with wire only → uses wire
   - Custom node with neither → uses default
   - Custom node with both wire and literal → uses wire (wire wins)

3. **Describe command**
   - Verify custom node parameters no longer show `[wire-only]`

4. **Serialization roundtrip**
   - Create custom node with literal, save to .cnnd, reload, verify literal preserved

---

## Migration

No migration needed. Existing `.cnnd` files with custom nodes:
- Currently have no stored literals (`NoData` serializes to `{}`)
- Will load with `CustomNodeData` having empty `literal_values`
- Behavior unchanged until user adds literals
