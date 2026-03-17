# Design: Factor Selection into Subnetwork

## Overview

This feature allows users to select multiple nodes in a node network and convert them into a reusable subnetwork (custom node type). The selection must be a **"single-output subset"** — meaning at most one wire exits the selection to nodes outside it.

## User Story

As a user designing complex node networks, I want to select a group of nodes and factor them into a reusable subnetwork so that I can:
- Reduce visual complexity in my main network
- Reuse the same logic in multiple places
- Create modular, composable designs

## User Flow

1. User selects 1+ nodes in the node network editor
2. User right-clicks to open context menu
3. If selection is valid (single-output subset, no Parameter nodes), show **"Factor into Subnetwork..."** option
4. User clicks the option
5. Dialog appears with:
   - Text field for subnetwork name (with validation)
   - Text area showing parameter names (one per line, editable)
   - OK / Cancel buttons
6. User edits name and parameter names as desired
7. User clicks OK
8. If validation fails (e.g., name already exists), show error in dialog (don't close)
9. On success: system creates subnetwork and replaces selection with custom node

## Validation Rules

### Selection Must Be Valid

1. **At least 1 node selected**
2. **Single-output constraint**: At most 1 wire goes from inside the selection to outside
3. **No Parameter nodes**: Selection must not contain any `parameter` type nodes

### Dialog Validation

1. **Subnetwork name**:
   - Must not be empty
   - Must not match an existing node type (built-in or custom)
   - Should be a valid identifier (alphanumeric + underscore, not starting with digit)

2. **Parameter names**:
   - Each must not be empty
   - Each must be unique within the list
   - Should be valid identifiers

## Technical Design

### Module Structure

The factoring logic is implemented in a dedicated module to keep the codebase modular:

```
rust/src/structure_designer/
├── selection_factoring.rs    <-- NEW: all factoring logic
├── structure_designer.rs     (thin wrapper method)
├── node_network.rs           (unchanged)
├── mod.rs                    (add module declaration)
└── ...
```

This follows the existing pattern of `node_dependency_analysis.rs` - a focused module with pure functions operating on node networks.

### Data Structures

```rust
// In rust/src/structure_designer/selection_factoring.rs

/// Result of analyzing a selection for factoring
pub struct SelectionAnalysis {
    /// Nodes inside the selection
    pub selected_ids: HashSet<u64>,

    /// Wires coming INTO the selection from OUTSIDE
    /// Sorted by destination node Y-coordinate (for parameter ordering)
    pub external_inputs: Vec<ExternalInput>,

    /// Wire going OUT OF the selection to OUTSIDE (0 or 1 for valid selection)
    pub external_output: Option<ExternalOutput>,

    /// Whether the selection is valid for factoring
    pub is_valid: bool,

    /// If not valid, the reason why
    pub invalid_reason: Option<String>,

    /// Bounding box of selection (min, max)
    pub bounding_box: (DVec2, DVec2),
}

pub struct ExternalInput {
    pub source_node_id: u64,           // Outside the selection
    pub source_output_pin_index: i32,
    pub destination_node_id: u64,      // Inside the selection
    pub destination_param_index: usize,
    pub data_type: DataType,           // For creating parameter type
    pub suggested_name: String,        // e.g., "cuboid1_output"
}

pub struct ExternalOutput {
    pub source_node_id: u64,           // Inside the selection (becomes return node)
    pub source_output_pin_index: i32,
    pub destination_node_id: u64,      // Outside the selection
    pub destination_param_index: usize,
}
```

### API Structures

```rust
// In rust/src/api/structure_designer/structure_designer_api_types.rs

/// Information for the factor-into-subnetwork dialog
pub struct FactorSelectionInfo {
    pub can_factor: bool,
    pub invalid_reason: Option<String>,
    pub suggested_name: String,
    pub suggested_param_names: Vec<String>,
}

/// Request to factor selection into subnetwork
pub struct FactorSelectionRequest {
    pub subnetwork_name: String,
    pub param_names: Vec<String>,
}

/// Result of factoring attempt
pub struct FactorSelectionResult {
    pub success: bool,
    pub error: Option<String>,
    pub new_node_id: Option<u64>,  // ID of the created custom node
}
```

### API Endpoints

```rust
// In rust/src/api/structure_designer/structure_designer_api.rs

/// Get information about whether/how the current selection can be factored
pub fn get_factor_selection_info() -> FactorSelectionInfo;

/// Factor the current selection into a new subnetwork
pub fn factor_selection_into_subnetwork(request: FactorSelectionRequest) -> FactorSelectionResult;
```

### Core Algorithm

All core functions are in `selection_factoring.rs` as standalone functions.

#### Step 1: Analyze Selection

```rust
// In rust/src/structure_designer/selection_factoring.rs

/// Analyzes the current selection in a network for factoring eligibility.
/// Returns information about external inputs/outputs and validity.
pub fn analyze_selection_for_factoring(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry
) -> SelectionAnalysis {
    // 1. Check minimum node count
    if network.selected_node_ids.is_empty() {
        return invalid("Select at least 1 node");
    }

    // 2. Check for Parameter nodes in selection
    for &node_id in &network.selected_node_ids {
        if let Some(node) = network.nodes.get(&node_id) {
            if node.node_type_name == "parameter" {
                return invalid("Selection contains Parameter nodes");
            }
        }
    }

    // 3. Find external inputs and outputs
    let mut external_inputs = Vec::new();
    let mut external_outputs = Vec::new();

    for &node_id in &network.selected_node_ids {
        let node = network.nodes.get(&node_id).unwrap();

        // Check each argument for external inputs
        for (param_idx, arg) in node.arguments.iter().enumerate() {
            for (&source_id, &pin_idx) in &arg.argument_output_pins {
                if !network.selected_node_ids.contains(&source_id) {
                    // This is an external input
                    let data_type = get_output_type(network, source_id, pin_idx, registry);
                    let suggested_name = generate_param_name(source_id, &network.nodes, pin_idx);
                    external_inputs.push(ExternalInput {
                        source_node_id: source_id,
                        source_output_pin_index: pin_idx,
                        destination_node_id: node_id,
                        destination_param_index: param_idx,
                        data_type,
                        suggested_name,
                    });
                }
            }
        }
    }

    // Check all nodes for outputs going outside selection
    for (&other_id, other_node) in &network.nodes {
        if network.selected_node_ids.contains(&other_id) {
            continue; // Skip nodes in selection
        }

        for (param_idx, arg) in other_node.arguments.iter().enumerate() {
            for (&source_id, &pin_idx) in &arg.argument_output_pins {
                if network.selected_node_ids.contains(&source_id) {
                    // This is an external output
                    external_outputs.push(ExternalOutput {
                        source_node_id: source_id,
                        source_output_pin_index: pin_idx,
                        destination_node_id: other_id,
                        destination_param_index: param_idx,
                    });
                }
            }
        }
    }

    // 4. Validate single-output constraint
    if external_outputs.len() > 1 {
        return invalid("Selection has multiple output wires");
    }

    // 5. Sort external inputs by destination node Y-coordinate
    external_inputs.sort_by(|a, b| {
        let y_a = network.nodes.get(&a.destination_node_id).map(|n| n.position.y).unwrap_or(0.0);
        let y_b = network.nodes.get(&b.destination_node_id).map(|n| n.position.y).unwrap_or(0.0);
        y_a.partial_cmp(&y_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    // 6. Deduplicate inputs (same source may connect to multiple destinations)
    // Keep track of unique (source_node_id, source_output_pin_index) pairs
    let external_inputs = deduplicate_external_inputs(external_inputs);

    // 7. Calculate bounding box
    let bounding_box = calculate_bounding_box(&network.selected_node_ids, &network.nodes);

    SelectionAnalysis {
        selected_ids: network.selected_node_ids.clone(),
        external_inputs,
        external_output: external_outputs.into_iter().next(),
        is_valid: true,
        invalid_reason: None,
        bounding_box,
    }
}
```

#### Step 2: Create Subnetwork

```rust
// In rust/src/structure_designer/selection_factoring.rs

/// Creates a new NodeNetwork from the selected nodes.
/// The new network contains Parameter nodes for external inputs and has the
/// appropriate return node set if there's an external output.
pub fn create_subnetwork_from_selection(
    source_network: &NodeNetwork,
    analysis: &SelectionAnalysis,
    subnetwork_name: &str,
    param_names: &[String],
    registry: &NodeTypeRegistry,
) -> NodeNetwork {
        // 1. Determine output type
        let output_type = if let Some(ref output) = analysis.external_output {
            get_output_type(output.source_node_id, output.source_output_pin_index, ...)
        } else {
            DataType::None
        };

        // 2. Create parameters from external inputs
        let parameters: Vec<Parameter> = analysis.external_inputs.iter()
            .enumerate()
            .map(|(i, input)| Parameter {
                id: Some(i as u64 + 1),
                name: param_names[i].clone(),
                data_type: input.data_type.clone(),
            })
            .collect();

        // 3. Create NodeType for the subnetwork
        let node_type = NodeType {
            name: subnetwork_name.to_string(),
            description: format!("Custom node factored from selection"),
            summary: None,
            category: NodeTypeCategory::Custom,
            parameters,
            output_type,
            public: true,
            node_data_creator: || Box::new(CustomNodeData::default()),
            node_data_saver: generic_node_data_saver::<CustomNodeData>,
            node_data_loader: generic_node_data_loader::<CustomNodeData>,
        };

        // 4. Create new NodeNetwork
        let mut new_network = NodeNetwork::new(node_type);

        // 5. Build ID mapping and copy nodes
        let mut id_mapping: HashMap<u64, u64> = HashMap::new();
        let center = calculate_center(&analysis.bounding_box);

        for &old_id in &analysis.selected_ids {
            let old_node = source_network.nodes.get(&old_id).unwrap();
            let new_id = new_network.next_node_id;
            new_network.next_node_id += 1;
            id_mapping.insert(old_id, new_id);

            // Create new node with adjusted position
            let new_position = old_node.position - center;
            let new_node = Node {
                id: new_id,
                node_type_name: old_node.node_type_name.clone(),
                custom_name: old_node.custom_name.clone(),
                position: new_position,
                arguments: old_node.arguments.clone(), // Will rewire below
                data: old_node.data.clone_box(),
                custom_node_type: old_node.custom_node_type.clone(),
            };
            new_network.nodes.insert(new_id, new_node);

            // Inherit display status
            if let Some(display_type) = source_network.get_node_display_type(old_id) {
                new_network.set_node_display_type(new_id, Some(display_type));
            }
        }

        // 6. Create Parameter nodes for external inputs
        let mut param_node_ids: HashMap<(u64, i32), u64> = HashMap::new(); // (source, pin) -> param node id

        for (i, input) in analysis.external_inputs.iter().enumerate() {
            let key = (input.source_node_id, input.source_output_pin_index);
            if param_node_ids.contains_key(&key) {
                continue; // Already created parameter for this source
            }

            let param_id = new_network.next_node_id;
            new_network.next_node_id += 1;

            // Position parameter nodes on the left
            let y_offset = i as f64 * 80.0;
            let param_position = DVec2::new(-300.0, y_offset - (analysis.external_inputs.len() as f64 * 40.0));

            let param_data = ParameterData {
                param_id: Some(new_network.next_param_id),
                param_index: i,
                param_name: param_names[i].clone(),
                data_type: input.data_type.clone(),
                sort_order: i as i32,
                data_type_str: None,
                error: None,
            };
            new_network.next_param_id += 1;

            let param_node = Node {
                id: param_id,
                node_type_name: "parameter".to_string(),
                custom_name: Some(param_names[i].clone()),
                position: param_position,
                arguments: vec![Argument::new()], // Default input
                data: Box::new(param_data),
                custom_node_type: None,
            };

            new_network.nodes.insert(param_id, param_node);
            param_node_ids.insert(key, param_id);
        }

        // 7. Rewire internal connections
        for &new_id in id_mapping.values() {
            let node = new_network.nodes.get_mut(&new_id).unwrap();
            for arg in &mut node.arguments {
                let mut new_pins: HashMap<u64, i32> = HashMap::new();

                for (&source_id, &pin_idx) in &arg.argument_output_pins {
                    if let Some(&mapped_id) = id_mapping.get(&source_id) {
                        // Internal connection - use mapped ID
                        new_pins.insert(mapped_id, pin_idx);
                    } else {
                        // External input - connect to parameter node
                        let key = (source_id, pin_idx);
                        if let Some(&param_id) = param_node_ids.get(&key) {
                            new_pins.insert(param_id, 0); // Parameter output is pin 0
                        }
                    }
                }

                arg.argument_output_pins = new_pins;
            }
        }

        // 8. Set return node if there's an external output
        if let Some(ref output) = analysis.external_output {
            let new_return_id = id_mapping.get(&output.source_node_id).unwrap();
            new_network.return_node_id = Some(*new_return_id);
        }

    new_network
}
```

#### Step 3: Replace Selection with Custom Node

```rust
// In rust/src/structure_designer/selection_factoring.rs

/// Replaces the selected nodes with a single custom node instance.
/// Wires up the custom node's inputs and output, then removes the original nodes.
/// Returns the ID of the newly created custom node.
pub fn replace_selection_with_custom_node(
    network: &mut NodeNetwork,
    analysis: &SelectionAnalysis,
    subnetwork_name: &str,
    num_params: usize,
) -> u64 {

    // 1. Calculate position (center of selection)
    let center = calculate_center(&analysis.bounding_box);

    // 2. Determine display type for new node
    let display_type = if let Some(ref output) = analysis.external_output {
        network.get_node_display_type(output.source_node_id)
    } else {
        Some(NodeDisplayType::Normal) // Default to visible
    };

    // 3. Create custom node
    let new_node_id = network.add_node(
            subnetwork_name,
        center,
        num_params,
        Box::new(CustomNodeData::default()),
    );

    // 4. Set display type
    network.set_node_display_type(new_node_id, display_type);

    // 5. Wire inputs to custom node
    // Build map of (source_id, pin) -> param_index
    let mut input_map: HashMap<(u64, i32), usize> = HashMap::new();
    for (i, input) in analysis.external_inputs.iter().enumerate() {
        input_map.entry((input.source_node_id, input.source_output_pin_index))
            .or_insert(i);
    }

    for input in &analysis.external_inputs {
        let param_idx = input_map[&(input.source_node_id, input.source_output_pin_index)];
        network.connect_nodes(
            input.source_node_id,
            input.source_output_pin_index,
            new_node_id,
            param_idx,
            false, // not multi
        );
    }

    // 6. Wire custom node output (if any)
    if let Some(ref output) = analysis.external_output {
        network.connect_nodes(
            new_node_id,
            0, // Main output pin
            output.destination_node_id,
            output.destination_param_index,
            false,
        );
    }

    // 7. Remove selected nodes
    for &node_id in &analysis.selected_ids {
        // Remove from other nodes' arguments
        let nodes_to_process: Vec<u64> = network.nodes.keys().cloned().collect();
        for other_id in nodes_to_process {
            if let Some(node) = network.nodes.get_mut(&other_id) {
                for arg in &mut node.arguments {
                    arg.argument_output_pins.remove(&node_id);
                }
            }
        }

        // Remove from displayed nodes
        network.displayed_node_ids.remove(&node_id);

        // Remove the node
        network.nodes.remove(&node_id);
    }

    // 8. Clear selection and select new node
    network.clear_selection();
    network.select_node(new_node_id);

    new_node_id
}
```

#### Main Entry Point (in StructureDesigner)

The `StructureDesigner` has a thin wrapper method that coordinates the module functions:

```rust
// In rust/src/structure_designer/structure_designer.rs

use super::selection_factoring;

impl StructureDesigner {
    pub fn factor_selection_into_subnetwork(
        &mut self,
        subnetwork_name: &str,
        param_names: Vec<String>,
    ) -> Result<u64, String> {
        // 1. Validate name doesn't exist
        if self.node_type_registry.get_node_type(subnetwork_name).is_some() {
            return Err(format!("Node type '{}' already exists", subnetwork_name));
        }

        // 2. Get active network
        let network_name = self.active_node_network_name.clone()
            .ok_or("No active network")?;

        // 3. Analyze selection (using module function)
        let network = self.node_type_registry.node_networks.get(&network_name)
            .ok_or("Network not found")?;
        let analysis = selection_factoring::analyze_selection_for_factoring(
            network,
            &self.node_type_registry
        );

        if !analysis.is_valid {
            return Err(analysis.invalid_reason.unwrap_or("Invalid selection".to_string()));
        }

        // 4. Validate param names count matches
        if param_names.len() != analysis.external_inputs.len() {
            return Err("Parameter count mismatch".to_string());
        }

        // 5. Create subnetwork (using module function)
        let source_network = self.node_type_registry.node_networks.get(&network_name).unwrap();
        let new_network = selection_factoring::create_subnetwork_from_selection(
            source_network,
            &analysis,
            subnetwork_name,
            &param_names,
            &self.node_type_registry,
        );

        // 6. Register subnetwork
        let num_params = new_network.node_type.parameters.len();
        self.node_type_registry.add_node_network(new_network);

        // 7. Replace selection with custom node (using module function)
        let network = self.node_type_registry.node_networks.get_mut(&network_name).unwrap();
        let new_node_id = selection_factoring::replace_selection_with_custom_node(
            network,
            &analysis,
            subnetwork_name,
            num_params,
        );

        // 8. Validate networks
        self.validate_all_node_networks();

        Ok(new_node_id)
    }
}
```

#### Helper Functions (in selection_factoring.rs)

```rust
// In rust/src/structure_designer/selection_factoring.rs

/// Creates an invalid SelectionAnalysis with the given reason
fn invalid(reason: &str) -> SelectionAnalysis {
    SelectionAnalysis {
        selected_ids: HashSet::new(),
        external_inputs: Vec::new(),
        external_output: None,
        is_valid: false,
        invalid_reason: Some(reason.to_string()),
        bounding_box: (DVec2::ZERO, DVec2::ZERO),
    }
}

/// Gets the output type of a node's output pin
fn get_output_type(
    network: &NodeNetwork,
    node_id: u64,
    pin_index: i32,
    registry: &NodeTypeRegistry,
) -> DataType {
    if let Some(node) = network.nodes.get(&node_id) {
        if let Some(node_type) = registry.get_node_type_for_node(node) {
            return node_type.get_output_pin_type(pin_index);
        }
    }
    DataType::None
}

/// Generates a suggested parameter name from the source node
fn generate_param_name(source_node_id: u64, nodes: &HashMap<u64, Node>, pin_index: i32) -> String {
    // Implementation shown in Parameter Naming Strategy section
}

/// Deduplicates external inputs, keeping one entry per unique (source_node_id, pin_index)
fn deduplicate_external_inputs(inputs: Vec<ExternalInput>) -> Vec<ExternalInput> {
    let mut seen: HashSet<(u64, i32)> = HashSet::new();
    inputs.into_iter()
        .filter(|input| seen.insert((input.source_node_id, input.source_output_pin_index)))
        .collect()
}

/// Calculates the bounding box of the selected nodes
fn calculate_bounding_box(
    selected_ids: &HashSet<u64>,
    nodes: &HashMap<u64, Node>,
) -> (DVec2, DVec2) {
    // Returns (min, max) corners
}

/// Calculates the center point of a bounding box
fn calculate_center(bounding_box: &(DVec2, DVec2)) -> DVec2 {
    (bounding_box.0 + bounding_box.1) / 2.0
}
```

### Parameter Naming Strategy

When generating suggested parameter names:

1. **Get source node's display name**: e.g., `cuboid1`
2. **Append output indicator if function pin**: e.g., `cuboid1_fn` for pin index -1
3. **Deduplicate**: If same name would be used twice, append `_2`, `_3`, etc.
4. **Fallback**: If name generation fails, use `input1`, `input2`, etc.

```rust
fn generate_param_name(source_node_id: u64, nodes: &HashMap<u64, Node>, pin_index: i32) -> String {
    if let Some(node) = nodes.get(&source_node_id) {
        let base_name = node.custom_name.as_ref()
            .unwrap_or(&node.node_type_name);

        if pin_index == -1 {
            format!("{}_fn", base_name)
        } else {
            base_name.clone()
        }
    } else {
        format!("input")
    }
}
```

### Display Status Rules

| Source | New Subnetwork Node Status | Custom Node Status |
|--------|---------------------------|-------------------|
| Node with Normal display | Normal | (based on output) |
| Node with Ghost display | Ghost | (based on output) |
| Node not displayed | Not copied to displayed | (based on output) |
| Output node exists | N/A | Inherits output node's status |
| No output node | N/A | Normal (visible) |

## Flutter UI Design

### Context Menu

In the node network right-click context menu, add item:

```dart
if (canFactorSelection) {
  PopupMenuItem(
    value: 'factor_into_subnetwork',
    child: Text('Factor into Subnetwork...'),
  ),
}
```

### Dialog Design

```
┌─────────────────────────────────────────────┐
│  Factor into Subnetwork                     │
├─────────────────────────────────────────────┤
│                                             │
│  Subnetwork name:                           │
│  ┌─────────────────────────────────────┐    │
│  │ my_custom_node                      │    │
│  └─────────────────────────────────────┘    │
│                                             │
│  Parameter names (one per line):            │
│  ┌─────────────────────────────────────┐    │
│  │ cuboid1                             │    │
│  │ sphere1                             │    │
│  │ radius                              │    │
│  └─────────────────────────────────────┘    │
│                                             │
│  ⚠️ Error: Name already exists             │  <- Only shown if error
│                                             │
│              [Cancel]  [Create]             │
└─────────────────────────────────────────────┘
```

### Dialog Implementation

```dart
class FactorIntoSubnetworkDialog extends StatefulWidget {
  final FactorSelectionInfo info;

  // ...
}

class _FactorIntoSubnetworkDialogState extends State<FactorIntoSubnetworkDialog> {
  late TextEditingController _nameController;
  late TextEditingController _paramsController;
  String? _error;

  @override
  void initState() {
    super.initState();
    _nameController = TextEditingController(text: widget.info.suggestedName);
    _paramsController = TextEditingController(
      text: widget.info.suggestedParamNames.join('\n'),
    );
  }

  void _submit() async {
    final name = _nameController.text.trim();
    final paramNames = _paramsController.text
        .split('\n')
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList();

    // Validate
    if (name.isEmpty) {
      setState(() => _error = 'Name cannot be empty');
      return;
    }

    if (paramNames.length != widget.info.suggestedParamNames.length) {
      setState(() => _error = 'Wrong number of parameters');
      return;
    }

    // Attempt factoring
    final result = await factorSelectionIntoSubnetwork(
      FactorSelectionRequest(
        subnetworkName: name,
        paramNames: paramNames,
      ),
    );

    if (result.success) {
      Navigator.of(context).pop(true);
    } else {
      setState(() => _error = result.error);
    }
  }

  // ... build method
}
```

## Implementation Phases

This feature is implemented in two phases to allow incremental testing and validation.

### Phase 1: Rust Backend + API

**Goal:** Complete backend logic with testable API, no UI changes.

**Deliverables:**
1. Create `rust/src/structure_designer/selection_factoring.rs`:
   - `SelectionAnalysis`, `ExternalInput`, `ExternalOutput` structs
   - `analyze_selection_for_factoring()` function
   - `create_subnetwork_from_selection()` function
   - `replace_selection_with_custom_node()` function
   - Helper functions (`invalid()`, `get_output_type()`, `generate_param_name()`, etc.)

2. Update `rust/src/structure_designer/mod.rs`:
   - Add `pub mod selection_factoring;`

3. Update `rust/src/structure_designer/structure_designer.rs`:
   - Add `factor_selection_into_subnetwork()` wrapper method

4. Update `rust/src/api/structure_designer/structure_designer_api_types.rs`:
   - Add `FactorSelectionInfo`, `FactorSelectionRequest`, `FactorSelectionResult`

5. Update `rust/src/api/structure_designer/structure_designer_api.rs`:
   - Add `get_factor_selection_info()` function
   - Add `factor_selection_into_subnetwork()` function

6. Create tests in `rust/tests/structure_designer/selection_factoring_test.rs`:
   - Selection analysis tests (valid/invalid cases)
   - Subnetwork creation tests
   - Replacement tests
   - End-to-end factoring tests

**Completion criteria:**
- All tests pass
- `cargo build` succeeds
- `cargo clippy` has no warnings
- API endpoints are callable (can test via CLI if needed)

### Phase 2: Flutter Frontend

**Goal:** UI integration with the backend API.

**Deliverables:**
1. Create `lib/structure_designer/dialogs/factor_into_subnetwork_dialog.dart`:
   - Dialog with name field and parameter names textarea
   - Validation and error display
   - Calls `factor_selection_into_subnetwork()` API

2. Update context menu (likely `lib/structure_designer/node_network/` area):
   - Add "Factor into Subnetwork..." menu item
   - Conditionally show based on `get_factor_selection_info().can_factor`
   - Open dialog on click

3. Regenerate FFI bindings:
   - Run `flutter_rust_bridge_codegen generate`

**Completion criteria:**
- Menu item appears when selection is valid
- Menu item hidden when selection is invalid
- Dialog opens and displays suggested names
- Successful factoring creates subnetwork and replaces selection
- Error cases show error in dialog without closing

---

## File Changes Summary

### Rust Backend

| File | Changes |
|------|---------|
| `rust/src/structure_designer/selection_factoring.rs` | **NEW**: All factoring logic - `SelectionAnalysis`, `ExternalInput`, `ExternalOutput` structs; `analyze_selection_for_factoring()`, `create_subnetwork_from_selection()`, `replace_selection_with_custom_node()` functions |
| `rust/src/structure_designer/mod.rs` | Add `pub mod selection_factoring;` declaration |
| `rust/src/structure_designer/structure_designer.rs` | Add thin wrapper `factor_selection_into_subnetwork()` method that coordinates module functions |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `FactorSelectionInfo`, `FactorSelectionRequest`, `FactorSelectionResult` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `get_factor_selection_info()`, `factor_selection_into_subnetwork()` API functions |

### Flutter Frontend

| File | Changes |
|------|---------|
| `lib/structure_designer/node_network/node_network_context_menu.dart` | Add "Factor into Subnetwork..." menu item |
| `lib/structure_designer/dialogs/factor_into_subnetwork_dialog.dart` | New file: dialog for name/params input |

## Testing Strategy

### Phase 1: Unit Tests (Rust)

Tests go in `rust/tests/structure_designer/selection_factoring_test.rs`.

1. **Selection analysis tests**:
   - Test with valid single-output selection
   - Test with invalid multi-output selection
   - Test with selection containing Parameter nodes
   - Test with no external inputs
   - Test with no external outputs
   - Test parameter ordering by Y-coordinate
   - Test empty selection (should be invalid)

2. **Factoring tests**:
   - Test basic factoring (1 node, 1 input, 1 output)
   - Test factoring with 2+ nodes
   - Test factoring with multiple inputs
   - Test factoring with no output (dead-end)
   - Test node data is properly cloned
   - Test display status inheritance
   - Test wire reconnection correctness

3. **Edge case tests**:
   - Test duplicate name rejection
   - Test param name deduplication
   - Test parameter count mismatch error

4. **Integration tests**:
   - Roundtrip test: Factor selection, save, load, verify structure
   - Evaluation test: Factor selection, evaluate original vs factored, compare results

### Phase 2: Manual/UI Tests (Flutter)

1. **Context menu visibility**:
   - Menu item visible with valid selection
   - Menu item hidden with invalid selection (multi-output, Parameter nodes, empty)

2. **Dialog behavior**:
   - Suggested name and param names populated correctly
   - Edit name and params, verify they're used
   - Error shown when name already exists (dialog stays open)
   - Successful factoring closes dialog and updates network

3. **End-to-end**:
   - Factor a selection, navigate into the new subnetwork, verify structure
   - Factor a selection, use the new custom node elsewhere, verify evaluation

## Future Enhancements (Out of Scope)

- **Undo/Redo support**: Allow reverting the factoring operation
- **Inline subnetwork**: Opposite operation - expand a custom node back to its constituent nodes
- **Auto-detect factoring opportunities**: Suggest selections that would make good subnetworks
- **Refactor across usages**: When a subnetwork is edited, update all instances

## Open Design Decisions (Resolved)

| Question | Decision |
|----------|----------|
| Undo support? | Not for initial implementation |
| Name collision handling? | Show error in dialog, don't close |
| Custom node position? | Center of selection bounding box |
| Parameter ordering? | By Y-coordinate of destination nodes |
| Display status? | Nodes inherit, custom node gets output's status (or Normal) |
| Parameter nodes in selection? | Not allowed - show error |
| Parameter name editing? | Yes, simple textarea in dialog |
