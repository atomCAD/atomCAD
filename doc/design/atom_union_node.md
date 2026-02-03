# atom_union Node Design Specification

## Overview

The `atom_union` node merges multiple atomic structures into a single combined structure. It is the atomic structure equivalent of the geometry `union` node.

## Node Specification

| Property | Value |
|----------|-------|
| **Name** | `atom_union` |
| **Category** | `NodeTypeCategory::AtomicStructure` |
| **Input** | `structures`: `DataType::Array(Box::new(DataType::Atomic))` (required) |
| **Output** | `DataType::Atomic` |
| **Description** | "Merges multiple atomic structures into one. The `structures` input accepts an array of `Atomic` values (array-typed input; you can connect multiple wires and they will be concatenated)." |

## Implementation Files

### 1. Create `rust/src/structure_designer/nodes/atom_union.rs`

```rust
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver, generic_node_data_loader};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;
use crate::crystolecule::atomic_structure::AtomicStructure;
use crate::util::transform::Transform;
use glam::f64::{DVec3, DQuat};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomUnionData {}

impl NodeData for AtomUnionData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
        // Evaluate the structures array input (required)
        let structures_val = network_evaluator.evaluate_arg_required(
            network_stack,
            node_id,
            registry,
            context,
            0,
        );

        if let NetworkResult::Error(_) = structures_val {
            return structures_val;
        }

        // Extract the array elements
        let structure_results = if let NetworkResult::Array(array_elements) = structures_val {
            array_elements
        } else {
            return NetworkResult::Error("Expected array of atomic structures".to_string());
        };

        let structure_count = structure_results.len();

        if structure_count == 0 {
            return NetworkResult::Error("atom_union requires at least one input structure".to_string());
        }

        // Extract atomic structures and collect frame translations for averaging
        let mut atomic_structures: Vec<AtomicStructure> = Vec::new();
        let mut frame_translation_sum = DVec3::ZERO;

        for structure_val in structure_results {
            if let NetworkResult::Atomic(structure) = structure_val {
                frame_translation_sum += structure.frame_transform().translation;
                atomic_structures.push(structure);
            } else {
                return NetworkResult::Error("All inputs must be atomic structures".to_string());
            }
        }

        // Start with the first structure as base
        let mut result = atomic_structures.remove(0);

        // Merge all subsequent structures into the result
        for other in &atomic_structures {
            result.add_atomic_structure(other);
        }

        // Set the frame transform to the average translation with identity rotation
        let avg_translation = frame_translation_sum / structure_count as f64;
        result.set_frame_transform(Transform::new(avg_translation, DQuat::IDENTITY));

        NetworkResult::Atomic(result)
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        None
    }

    fn get_parameter_metadata(&self) -> std::collections::HashMap<String, (bool, Option<String>)> {
        let mut m = std::collections::HashMap::new();
        m.insert("structures".to_string(), (true, None)); // required
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "atom_union".to_string(),
        description: "Merges multiple atomic structures into one. The `structures` input accepts an array of `Atomic` values (array-typed input; you can connect multiple wires and they will be concatenated).".to_string(),
        summary: None,
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter {
                id: None,
                name: "structures".to_string(),
                data_type: DataType::Array(Box::new(DataType::Atomic)),
            },
        ],
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(AtomUnionData {}),
        node_data_saver: generic_node_data_saver::<AtomUnionData>,
        node_data_loader: generic_node_data_loader::<AtomUnionData>,
    }
}
```

### 2. Update `rust/src/structure_designer/nodes/mod.rs`

Add the following line (in alphabetical order with other atom_* modules):

```rust
pub mod atom_union;
```

### 3. Update `rust/src/structure_designer/node_type_registry.rs`

Add import at the top with other node imports:

```rust
use super::nodes::atom_union::get_node_type as atom_union_get_node_type;
```

Add registration in the `new()` function, near other atom_* nodes:

```rust
ret.add_node_type(atom_union_get_node_type());
```

## Key Implementation Details

### AtomicStructure.add_atomic_structure()

The merging logic already exists in `rust/src/crystolecule/atomic_structure/mod.rs` lines 615-658. This method:
- Remaps atom IDs to avoid conflicts
- Copies all atom properties (atomic_number, position, in_crystal_depth, flags)
- Remaps and copies bonds with correct ID mapping
- Merges bond selections from the decorator
- Returns `FxHashMap<u32, u32>` mapping old IDs to new IDs (not needed for this node)

### Frame Transform Handling

Following the geometry `union` node pattern:
- Average the translation vectors from all input structures
- Use identity rotation (`DQuat::IDENTITY`)
- Set via `result.set_frame_transform()`

### Error Handling

Following the geometry `union` node pattern:
- Empty array: Return `NetworkResult::Error("atom_union requires at least one input structure")`
- Non-atomic element: Return `NetworkResult::Error("All inputs must be atomic structures")`

## Reference Files

- Geometry union node: `rust/src/structure_designer/nodes/union.rs`
- AtomicStructure merge method: `rust/src/crystolecule/atomic_structure/mod.rs` (lines 615-658)
- Similar atomic node: `rust/src/structure_designer/nodes/atom_cut.rs`
- Node registration: `rust/src/structure_designer/node_type_registry.rs`

## Testing

After implementation, verify:
1. `cargo build` succeeds
2. `cargo test` passes
3. `cargo clippy` has no warnings
4. Node appears in the UI under AtomicStructure category
5. Connecting multiple atom_fill outputs to atom_union merges them correctly
