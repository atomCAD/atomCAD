# AI Assistant Implementation Plan

This document provides a detailed implementation plan for the AI assistant integration in atomCAD. It covers the `query` and `edit` commands that allow AI coding assistants to interact with node networks programmatically.

**Related Documents:**
- [AI Assistant Integration](./ai_assistant_integration.md) - High-level architecture and requirements
- [Node Network Text Format](./node_network_text_format.md) - Text format specification
- [Phase 1 Design](./ai_assistant_integration_phase1_design.md) - Stub implementation (already completed)

## Overview

The implementation is divided into 5 phases:

| Phase | Description | Dependencies | Status |
|-------|-------------|--------------|--------|
| 1 | Core text format infrastructure | None | New |
| 2 | Node text property implementations | Phase 1 | New |
| 3 | Query command (serialize network → text) | Phase 1, 2 | New |
| 4 | Edit command (parse text → modify network) | Phase 1, 2 | New |
| 5 | Integration with HTTP server and CLI | Phase 3, 4 | **Partial** - HTTP server & CLI exist with stubs |

### Existing Infrastructure (from Phase 1 stub implementation)

The following components already exist and return stub responses:

- **HTTP Server:** `lib/ai_assistant/http_server.dart` - endpoints `/health`, `/query`, `/edit`
- **CLI Tool:** `bin/atomcad_cli.dart` - commands `query`, `edit --code="..." [--replace]`
- **Constants:** `lib/ai_assistant/constants.dart` - port 19847, stub responses

See [Phase 1 Design Doc](./ai_assistant_integration_phase1_design.md) for details on existing implementation.

### Key Source Files (Prerequisites)

Before implementing any phase, agents should understand these core source files:

| File | Description |
|------|-------------|
| `rust/src/structure_designer/node_network.rs` | Core data structures: `Node`, `NodeNetwork`, `Wire`, `Argument`. Contains node/wire operations, selection, connections. |
| `rust/src/structure_designer/node_type.rs` | `NodeType` and `Parameter` definitions. Defines how node types are structured with parameters, output types, and data handlers. |
| `rust/src/structure_designer/node_data.rs` | `NodeData` trait that all node data types implement. Will be extended with `get_text_properties()` and `set_text_properties()`. |
| `rust/src/structure_designer/data_type.rs` | `DataType` enum defining all supported data types (Int, Float, Vec3, Geometry, etc.). |
| `rust/src/structure_designer/nodes/` | Individual node implementations. Each file contains a node's data struct and `NodeData` impl. |

---

## Phase 1: Core Text Format Infrastructure

**Goal:** Create the foundational types and utilities for text format serialization/deserialization.

### 1.1 Create TextValue Enum

**File:** `rust/src/structure_designer/text_format/text_value.rs`

```rust
use glam::{IVec2, IVec3, DVec2, DVec3};
use crate::structure_designer::data_type::DataType;

/// Represents a value in the node network text format.
/// Used for both serialization (query) and deserialization (edit).
#[derive(Debug, Clone, PartialEq)]
pub enum TextValue {
    Bool(bool),
    Int(i32),
    Float(f64),
    String(String),
    IVec2(IVec2),
    IVec3(IVec3),
    Vec2(DVec2),
    Vec3(DVec3),
    DataType(DataType),
    Array(Vec<TextValue>),
    /// For complex nested structures like expr parameters
    Object(Vec<(String, TextValue)>),
}
```

**Implementation tasks:**
- [ ] Create the enum with all variants
- [ ] Implement `Display` trait for serialization to text
- [ ] Implement `FromStr` or parsing functions for deserialization
- [ ] Add helper methods: `as_bool()`, `as_int()`, `as_ivec3()`, etc.
- [ ] Add conversion methods: `to_json()`, `from_json()`

### 1.2 Create Text Serializer

**File:** `rust/src/structure_designer/text_format/serializer.rs`

Converts `TextValue` to text format strings:

```rust
impl TextValue {
    /// Serialize to text format string
    pub fn to_text(&self) -> String {
        match self {
            TextValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            TextValue::Int(i) => i.to_string(),
            TextValue::Float(f) => format_float(*f), // Ensure decimal point
            TextValue::String(s) => format_string(s), // Handle escaping, multi-line
            TextValue::IVec2(v) => format!("({}, {})", v.x, v.y),
            TextValue::IVec3(v) => format!("({}, {}, {})", v.x, v.y, v.z),
            TextValue::Vec2(v) => format!("({}, {})", format_float(v.x), format_float(v.y)),
            TextValue::Vec3(v) => format!("({}, {}, {})",
                format_float(v.x), format_float(v.y), format_float(v.z)),
            TextValue::DataType(dt) => dt.to_string(),
            TextValue::Array(arr) => format_array(arr),
            TextValue::Object(obj) => format_object(obj),
        }
    }
}

/// Format float ensuring it has a decimal point (to distinguish from int)
fn format_float(f: f64) -> String {
    let s = f.to_string();
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

/// Format string with proper escaping and multi-line handling
fn format_string(s: &str) -> String {
    if s.contains('\n') {
        // Use triple-quoted string for multi-line
        format!("\"\"\"{}\"\"\"", s)
    } else {
        // Use regular quoted string with escaping
        format!("\"{}\"", escape_string(s))
    }
}
```

**Implementation tasks:**
- [ ] Implement `format_float()` - ensure decimal point for type inference
- [ ] Implement `format_string()` - escape special chars, detect multi-line
- [ ] Implement `format_array()` - `[val1, val2, ...]`
- [ ] Implement `format_object()` - `{ key1: val1, key2: val2 }`
- [ ] Handle DataType serialization

### 1.3 Create Text Parser

**File:** `rust/src/structure_designer/text_format/parser.rs`

Lexer and parser for the text format.

```rust
/// Token types for the text format lexer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Identifier(String),
    Int(i32),
    Float(f64),
    String(String),
    True,
    False,
    Equals,         // =
    Colon,          // :
    Comma,          // ,
    LeftBrace,      // {
    RightBrace,     // }
    LeftBracket,    // [
    RightBracket,   // ]
    LeftParen,      // (
    RightParen,     // )
    At,             // @
    Hash,           // #
    Output,         // output keyword
    Delete,         // delete keyword
    Newline,
    Eof,
}

/// Parsed statements from the text format
#[derive(Debug, Clone)]
pub enum Statement {
    Assignment {
        name: String,
        node_type: String,
        properties: Vec<(String, PropertyValue)>,
    },
    Output {
        node_name: String,
    },
    Delete {
        node_name: String,
    },
    Comment(String),
}

/// A property value can be a literal or a reference
#[derive(Debug, Clone)]
pub enum PropertyValue {
    Literal(TextValue),
    NodeRef(String),      // Regular node reference
    FunctionRef(String),  // @node_name - function pin reference
}
```

**Implementation tasks:**
- [ ] Implement `Lexer` struct with tokenization
- [ ] Handle string literals (regular and triple-quoted)
- [ ] Handle numeric literals (int vs float detection)
- [ ] Handle vector literals `(x, y)` and `(x, y, z)`
- [ ] Implement `Parser` struct
- [ ] Parse assignments: `name = type { props }`
- [ ] Parse statements: `output name`, `delete name`
- [ ] Parse property values (literals, node refs, function refs)
- [ ] Handle comments (skip or preserve)
- [ ] Comprehensive error messages with line/column info

### 1.4 Extend NodeData Trait

**File:** `rust/src/structure_designer/node_data.rs`

Add new methods to the `NodeData` trait:

```rust
use crate::structure_designer::text_format::TextValue;
use std::collections::HashMap;

pub trait NodeData: Any + AsAny {
    // ... existing methods ...

    /// Returns the properties to serialize for text format output.
    ///
    /// Keys are property names as they appear in text format.
    /// These should match parameter names where applicable.
    /// Only returns properties that have stored values (not input-only params).
    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![]
    }

    /// Updates node data from parsed text properties.
    ///
    /// Only properties present in the map are updated.
    /// Returns error if a property value has wrong type or is invalid.
    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        let _ = props;
        Ok(())
    }
}
```

**Implementation tasks:**
- [ ] Add imports for TextValue
- [ ] Add `get_text_properties()` with default empty implementation
- [ ] Add `set_text_properties()` with default no-op implementation
- [ ] Update `NoData` implementation (keep defaults)

### 1.5 Module Structure

**File:** `rust/src/structure_designer/text_format/mod.rs`

```rust
mod text_value;
mod serializer;
mod parser;

pub use text_value::TextValue;
pub use parser::{Parser, Statement, PropertyValue, ParseError};
pub use serializer::Serializer;
```

**File:** `rust/src/structure_designer/mod.rs` - add module declaration

**Implementation tasks:**
- [ ] Create `text_format/` directory
- [ ] Create `mod.rs` with public exports
- [ ] Add `pub mod text_format;` to parent mod.rs

---

## Phase 2: Node Text Property Implementations

**Goal:** Implement `get_text_properties()` and `set_text_properties()` for all node types.

### 2.1 Design Decisions (from user feedback)

- **Primitive value nodes** (ivec3, vec3, etc.): Use parameter names (`x: 1, y: 2, z: 3`) not compound value
- **Connected parameters**: Omit the stored property value; only show the connection (cleaner for LLMs, connection is the runtime value)
- **Input-only parameters**: Show when connected, omit when using default

### 2.2 Node Categories

#### Category A: Simple Direct Mapping
Nodes where NodeData fields directly map to text properties.

| Node | Fields | Text Properties |
|------|--------|-----------------|
| `sphere` | `center: IVec3, radius: i32` | `center: (x,y,z), radius: n` |
| `cuboid` | `min_corner: IVec3, extent: IVec3` | `min_corner: (x,y,z), extent: (x,y,z)` |
| `circle` | `center: IVec2, radius: i32` | `center: (x,y), radius: n` |
| `rect` | `min_corner: IVec2, extent: IVec2` | `min_corner: (x,y), extent: (x,y)` |
| `int` | `value: i32` | `value: n` |
| `float` | `value: f64` | `value: n.n` |
| `bool` | `value: bool` | `value: true/false` |
| `string` | `value: String` | `value: "..."` |
| `range` | `start, step, count: i32` | `start: n, step: n, count: n` |

**Implementation:** Direct implementation - field names match text property names.

```rust
// Example for SphereData
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    vec![
        ("center".to_string(), TextValue::IVec3(self.center)),
        ("radius".to_string(), TextValue::Int(self.radius)),
    ]
}

fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    if let Some(TextValue::IVec3(v)) = props.get("center") {
        self.center = *v;
    }
    if let Some(TextValue::Int(v)) = props.get("radius") {
        self.radius = *v;
    }
    Ok(())
}
```

#### Category B: Decomposed Fields
Nodes where one field maps to multiple text properties.

| Node | Field | Text Properties |
|------|-------|-----------------|
| `ivec2` | `value: IVec2` | `x: n, y: n` |
| `ivec3` | `value: IVec3` | `x: n, y: n, z: n` |
| `vec2` | `value: DVec2` | `x: n.n, y: n.n` |
| `vec3` | `value: DVec3` | `x: n.n, y: n.n, z: n.n` |

**Implementation:** Custom implementation that decomposes the vector.

```rust
// Example for IVec3Data
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    vec![
        ("x".to_string(), TextValue::Int(self.value.x)),
        ("y".to_string(), TextValue::Int(self.value.y)),
        ("z".to_string(), TextValue::Int(self.value.z)),
    ]
}

fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    if let Some(TextValue::Int(x)) = props.get("x") {
        self.value.x = *x;
    }
    if let Some(TextValue::Int(y)) = props.get("y") {
        self.value.y = *y;
    }
    if let Some(TextValue::Int(z)) = props.get("z") {
        self.value.z = *z;
    }
    Ok(())
}
```

#### Category C: Renamed Fields
Nodes where field names differ from text property names.

| Node | Field | Text Property |
|------|-------|---------------|
| `atom_fill` | `hydrogen_passivation` | `passivate` |
| `atom_fill` | `remove_single_bond_atoms_before_passivation` | `rm_single` |
| `atom_fill` | `surface_reconstruction` | `surf_recon` |
| `atom_fill` | `motif_offset` | `m_offset` |

**Implementation:** Custom mapping in the node's implementation.

#### Category D: Multi-line String Fields
Nodes with string fields that typically contain multi-line content.

| Node | Field | Notes |
|------|-------|-------|
| `motif` | `definition` | Motif DSL |
| `atom_fill` | `parameter_element_value_definition` | Element assignments |
| `expr` | `expression` | Math expression (usually single line) |

**Implementation:** Serializer handles triple-quote format automatically for strings containing newlines.

#### Category E: Dynamic/Complex Nodes
Nodes with special serialization needs.

| Node | Complexity |
|------|------------|
| `expr` | Has dynamic `parameters` array with name+type |
| `map` | Has `input_type` and `output_type` as DataType |
| `parameter` | Has `param_name`, `data_type`, `sort_order` |
| `unit_cell` | Stores crystallographic params, exposes basis vectors |

**Implementation:** Fully custom implementations.

```rust
// Example for ExprData
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    vec![
        ("expression".to_string(), TextValue::String(self.expression.clone())),
        ("parameters".to_string(), TextValue::Array(
            self.parameters.iter().map(|p| {
                TextValue::Object(vec![
                    ("name".to_string(), TextValue::String(p.name.clone())),
                    ("type".to_string(), TextValue::DataType(p.data_type.clone())),
                ])
            }).collect()
        )),
    ]
}
```

#### Category F: No Stored Data
Nodes that only have input connections, no stored properties.

| Node | Notes |
|------|-------|
| `union`, `intersect`, `diff` | Only have input connections (shapes, base, sub) |
| `union_2d`, `intersect_2d`, `diff_2d` | Same pattern |
| `extrude` | Has inputs but no stored defaults worth exposing |

**Implementation:** Return empty from `get_text_properties()`, connections handled separately.

### 2.3 Implementation Checklist

**Math and Programming Nodes:**
- [ ] `int` - simple
- [ ] `float` - simple
- [ ] `bool` - simple
- [ ] `string` - simple
- [ ] `ivec2` - decomposed
- [ ] `ivec3` - decomposed
- [ ] `vec2` - decomposed
- [ ] `vec3` - decomposed
- [ ] `range` - simple
- [ ] `expr` - complex (parameters array)
- [ ] `map` - complex (DataType fields)
- [ ] `parameter` - complex

**2D Geometry Nodes:**
- [ ] `rect` - simple
- [ ] `circle` - simple
- [ ] `polygon` - array of vertices
- [ ] `reg_poly` - simple
- [ ] `half_plane` - simple
- [ ] `union_2d` - no data
- [ ] `intersect_2d` - no data
- [ ] `diff_2d` - no data

**3D Geometry Nodes:**
- [ ] `cuboid` - simple
- [ ] `sphere` - simple
- [ ] `half_space` - simple
- [ ] `extrude` - simple
- [ ] `union` - no data
- [ ] `intersect` - no data
- [ ] `diff` - no data
- [ ] `lattice_move` - simple
- [ ] `lattice_rot` - simple
- [ ] `geo_trans` - simple

**Atomic Structure Nodes:**
- [ ] `unit_cell` - complex (crystallographic params)
- [ ] `motif` - multi-line string
- [ ] `atom_fill` - renamed + multi-line
- [ ] `atom_trans` - simple
- [ ] `atom_cut` - simple
- [ ] `import_xyz` - simple
- [ ] `export_xyz` - simple

**Other Nodes:**
- [ ] `drawing_plane` - simple
- [ ] `comment` - simple
- [ ] `value` - check what this is
- [ ] `facet_shell` - check what this is
- [ ] `relax` - check what this is
- [ ] `lattice_symop` - check what this is

---

## Phase 3: Query Command

**Goal:** Implement the query API that serializes a node network to text format.

### 3.1 Query API Function

**File:** `rust/src/api/structure_designer/ai_assistant_api.rs`

```rust
/// Serializes the active node network to text format.
/// Returns the text representation suitable for AI assistant consumption.
pub fn query_network(structure_designer: &StructureDesigner) -> String {
    let network = structure_designer.get_active_network();
    let serializer = NetworkSerializer::new(network);
    serializer.serialize()
}
```

### 3.2 Network Serializer

**File:** `rust/src/structure_designer/text_format/network_serializer.rs`

```rust
pub struct NetworkSerializer<'a> {
    network: &'a NodeNetwork,
    /// Maps node IDs to generated names
    node_names: HashMap<u64, String>,
    /// Counter per node type for name generation
    type_counters: HashMap<String, u32>,
}

impl<'a> NetworkSerializer<'a> {
    pub fn new(network: &'a NodeNetwork) -> Self { ... }

    pub fn serialize(&mut self) -> String { ... }

    /// Generate names for all nodes in topological order
    fn generate_names(&mut self) { ... }

    /// Get topologically sorted node IDs (dependencies before dependents)
    fn topological_sort(&self) -> Vec<u64> { ... }

    /// Serialize a single node to text
    fn serialize_node(&self, node_id: u64) -> String { ... }

    /// Get the generated name for a node
    fn get_node_name(&self, node_id: u64) -> &str { ... }
}
```

### 3.3 Serialization Algorithm

```
1. TOPOLOGICAL SORT
   - Build dependency graph from wire connections
   - Sort nodes so dependencies come before dependents
   - Handle cycles (error) and disconnected subgraphs

2. NAME GENERATION
   - For each node in topological order:
     - Get node type name (e.g., "sphere")
     - Increment counter for that type
     - Assign name: "{type}{counter}" (e.g., "sphere1", "sphere2")
   - Store mapping: node_id → name

3. SERIALIZE EACH NODE
   For each node in topological order:

   a. Get stored properties from NodeData.get_text_properties()

   b. Get input connections from wires:
      - For each wire ending at this node:
        - Get source node name
        - Get input pin name
        - If function pin (pin_index == -1): prefix with @

   c. Combine properties and connections:
      - Properties: "prop_name: literal_value"
      - Connections: "param_name: source_node_name" or "param_name: @source_node_name"
      - If a parameter has both stored value AND connection, only output the connection (omit the stored value)

   d. Format: "node_name = node_type { prop1: val1, prop2: val2, ... }"

4. OUTPUT STATEMENT
   - If network has a return node set:
     - Append: "output {return_node_name}"

5. ASSEMBLE OUTPUT
   - Join all node serializations with newlines
   - Add blank lines between logical groups (optional)
```

### 3.4 Implementation Tasks

- [ ] Create `NetworkSerializer` struct
- [ ] Implement topological sort (handle cycles)
- [ ] Implement name generation (type + counter)
- [ ] Implement node serialization
- [ ] Handle wire connections (regular and function pins)
- [ ] Omit stored property values when parameter has a connection
- [ ] Handle output statement
- [ ] Add blank lines / formatting for readability
- [ ] Create public API function
- [ ] Unit tests with snapshot testing

### 3.5 Edge Cases

- **Cycles:** Error with clear message
- **Disconnected nodes:** Include all nodes, not just reachable from output
- **Multiple outputs:** Only one `output` statement (current network design)
- **Empty network:** Return empty string or comment
- **Special characters in strings:** Proper escaping
- **Very long lines:** Consider line wrapping (optional)

---

## Phase 4: Edit Command

**Goal:** Implement the edit API that parses text format and modifies the node network.

### 4.1 Edit API Function

**File:** `rust/src/api/structure_designer/ai_assistant_api.rs`

```rust
/// Result of an edit operation
#[derive(Debug, Serialize)]
pub struct EditResult {
    pub success: bool,
    pub nodes_created: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub connections_made: Vec<String>,
    pub errors: Vec<String>,
}

/// Applies edit commands to the active node network.
///
/// # Arguments
/// * `structure_designer` - The structure designer instance
/// * `code` - The edit commands in text format
/// * `replace` - If true, replace entire network; if false, incremental merge
pub fn edit_network(
    structure_designer: &mut StructureDesigner,
    code: &str,
    replace: bool,
) -> EditResult {
    let mut editor = NetworkEditor::new(structure_designer);
    editor.apply(code, replace)
}
```

### 4.2 Network Editor

**File:** `rust/src/structure_designer/text_format/network_editor.rs`

```rust
pub struct NetworkEditor<'a> {
    structure_designer: &'a mut StructureDesigner,
    /// Maps text names to node IDs (existing + newly created)
    name_to_id: HashMap<String, u64>,
    /// Result tracking
    result: EditResult,
}

impl<'a> NetworkEditor<'a> {
    pub fn new(structure_designer: &'a mut StructureDesigner) -> Self { ... }

    pub fn apply(&mut self, code: &str, replace: bool) -> EditResult { ... }

    /// Build name→id mapping from existing network
    fn build_existing_name_map(&mut self) { ... }

    /// Process a single statement
    fn process_statement(&mut self, stmt: Statement) -> Result<(), String> { ... }

    /// Process assignment (create or update node)
    fn process_assignment(&mut self, name: &str, node_type: &str,
                          props: &[(String, PropertyValue)]) -> Result<(), String> { ... }

    /// Process delete statement
    fn process_delete(&mut self, name: &str) -> Result<(), String> { ... }

    /// Process output statement
    fn process_output(&mut self, name: &str) -> Result<(), String> { ... }
}
```

### 4.3 Edit Algorithm

```
1. PARSE INPUT
   - Tokenize the input code
   - Parse into list of Statement (Assignment, Output, Delete, Comment)
   - Collect parse errors

2. BUILD NAME MAP (for incremental mode)
   - Query existing network
   - Parse to extract name→node_id mapping
   - This allows edits to reference existing nodes by name

3. IF REPLACE MODE:
   - Delete all existing nodes
   - Clear name map

4. FIRST PASS: CREATE/UPDATE NODES
   For each Assignment statement:

   a. Check if name exists in name map:
      - EXISTS: Update existing node
      - NOT EXISTS: Create new node

   b. For new nodes:
      - Look up node type in registry
      - Create node with default data
      - Generate position (simple auto-layout)
      - Add to name map

   c. For all nodes (new and existing):
      - Extract literal properties from statement
      - Call node_data.set_text_properties(literals)
      - Track connections for second pass

5. SECOND PASS: WIRE CONNECTIONS
   For each Assignment statement:

   a. For each property that is a NodeRef or FunctionRef:
      - Resolve source node name → source node ID
      - Resolve parameter name → input pin index
      - Determine output pin index (0 for regular, -1 for function)
      - Remove any existing wire to this input
      - Create new wire

   b. Handle "shapes" array for union/intersect nodes:
      - Parse array of node refs
      - Create multiple wires

6. PROCESS DELETE STATEMENTS
   For each Delete statement:
   - Resolve name → node ID
   - Remove all wires connected to node
   - Delete node
   - Remove from name map

7. PROCESS OUTPUT STATEMENT
   If Output statement present:
   - Resolve name → node ID
   - Set as network's return node

8. TRIGGER UI REFRESH
   - Notify listeners of network change
   - Recalculate evaluation
```

### 4.4 Auto-Layout for New Nodes

**File:** `rust/src/structure_designer/text_format/auto_layout.rs`

Simple algorithm for placing new nodes:

```rust
/// Calculate position for a new node
pub fn calculate_new_node_position(
    network: &NodeNetwork,
    node_type: &str,
    input_connections: &[(u64, usize)], // (source_node_id, input_pin_index)
) -> (f64, f64) {
    // Strategy 1: Place to the right of input nodes
    if !input_connections.is_empty() {
        let max_x = input_connections.iter()
            .filter_map(|(id, _)| network.get_node(*id))
            .map(|n| n.position.0 + NODE_WIDTH + HORIZONTAL_GAP)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let avg_y = /* calculate average Y of inputs */;
        return (max_x, avg_y);
    }

    // Strategy 2: Find empty space
    find_empty_position(network)
}
```

### 4.5 Implementation Tasks

- [ ] Create `NetworkEditor` struct
- [ ] Implement name map building from existing network
- [ ] Implement replace mode (clear network)
- [ ] Implement node creation
- [ ] Implement node update (set_text_properties)
- [ ] Implement wire creation
- [ ] Implement wire removal (for rewiring)
- [ ] Implement node deletion
- [ ] Implement output statement handling
- [ ] Implement auto-layout algorithm
- [ ] Create public API function
- [ ] Return detailed EditResult
- [ ] Unit tests for all edit operations

### 4.6 Edge Cases

- **Forward references:** Node A references Node B, but B defined later in code
  - Solution: Two-pass algorithm (create nodes first, wire second)
- **Self-reference:** Node references itself
  - Solution: Error
- **Missing node type:** Unknown node type name
  - Solution: Error with suggestions
- **Type mismatch:** Wrong property type
  - Solution: Error with expected type
- **Invalid wire:** Incompatible types on wire
  - Solution: Error (or warning if soft type system)
- **Duplicate names:** Same name defined twice in edit
  - Solution: Second definition updates the first

---

## Phase 5: Integration

**Goal:** Connect the Rust implementation to the existing HTTP server and CLI.

### 5.1 Existing Infrastructure (Already Implemented)

The following components already exist from Phase 1 implementation:

**HTTP Server:** `lib/ai_assistant/http_server.dart`
- `AiAssistantServer` class with start/stop lifecycle
- Endpoints: `/health`, `/query`, `/edit`
- Currently returns stub responses

**CLI Tool:** `bin/atomcad_cli.dart`
- Commands: `query`, `edit --code="..." [--replace]`
- Health check before commands
- Error handling for connection failures

**Constants:** `lib/ai_assistant/constants.dart`
- Port configuration (19847)
- Stub response text

### 5.2 Rust API Functions

**File:** `rust/src/api/structure_designer/ai_assistant_api.rs` (new file)

```rust
use crate::structure_designer::text_format::{NetworkSerializer, NetworkEditor, EditResult};
use crate::structure_designer::structure_designer::StructureDesigner;

/// Query the active node network, returning text format representation.
pub fn query_network(structure_designer: &StructureDesigner) -> String {
    let network = structure_designer.get_active_network();
    let registry = structure_designer.get_node_type_registry();
    let mut serializer = NetworkSerializer::new(network, registry);
    serializer.serialize()
}

/// Edit the node network from text format commands.
pub fn edit_network(
    structure_designer: &mut StructureDesigner,
    code: &str,
    replace: bool,
) -> EditResult {
    let mut editor = NetworkEditor::new(structure_designer);
    editor.apply(code, replace)
}
```

**File:** `rust/src/api/structure_designer/structure_designer_api.rs` (add functions)

```rust
/// FFI-friendly wrapper for query_network
pub fn ai_query_network(structure_designer_ptr: u64) -> String {
    let sd = get_structure_designer(structure_designer_ptr);
    ai_assistant_api::query_network(&sd)
}

/// FFI-friendly wrapper for edit_network
pub fn ai_edit_network(
    structure_designer_ptr: u64,
    code: String,
    replace: bool,
) -> String {
    let mut sd = get_structure_designer_mut(structure_designer_ptr);
    let result = ai_assistant_api::edit_network(&mut sd, &code, replace);
    serde_json::to_string(&result).unwrap()
}
```

### 5.3 FFI Bindings

After adding Rust API functions, regenerate bindings:

```powershell
flutter_rust_bridge_codegen generate
```

### 5.4 Update HTTP Server

**File:** `lib/ai_assistant/http_server.dart`

The server needs access to the StructureDesigner to call Rust. Update to accept it:

```dart
class AiAssistantServer {
  HttpServer? _server;
  final int port;
  final StructureDesignerState? structureDesigner;  // Add this

  AiAssistantServer({
    this.port = aiAssistantPort,
    this.structureDesigner,  // Add this
  });

  // ... existing code ...

  Future<void> _handleQuery(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    if (structureDesigner == null) {
      request.response.statusCode = HttpStatus.serviceUnavailable;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({'error': 'No active design'}));
      return;
    }

    // Call Rust API
    final result = await api.aiQueryNetwork(structureDesigner!.rustPtr);

    request.response.headers.contentType = ContentType.text;
    request.response.write(result);
  }

  Future<void> _handleEdit(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    if (structureDesigner == null) {
      request.response.statusCode = HttpStatus.serviceUnavailable;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({'error': 'No active design'}));
      return;
    }

    final body = await utf8.decoder.bind(request).join();
    final replace = request.uri.queryParameters['replace'] == 'true';

    // Call Rust API
    final resultJson = await api.aiEditNetwork(
      structureDesigner!.rustPtr,
      body,
      replace,
    );

    // Trigger UI refresh
    structureDesigner!.notifyListeners();

    request.response.headers.contentType = ContentType.json;
    request.response.write(resultJson);
  }
}
```

### 5.5 Update Server Initialization

**File:** `lib/main.dart` (or wherever server is started)

Pass the StructureDesigner to the server:

```dart
// When StructureDesigner is available, update server reference
aiServer.structureDesigner = structureDesignerState;
```

### 5.6 CLI Tool

**File:** `bin/atomcad_cli.dart`

The CLI already works correctly - it calls the HTTP endpoints. **No changes needed.**

The CLI will automatically get real results once the HTTP server returns real data.

### 5.7 Implementation Tasks

- [ ] Create `rust/src/api/structure_designer/ai_assistant_api.rs`
- [ ] Add `ai_query_network` and `ai_edit_network` to structure_designer_api.rs
- [ ] Export new functions in `rust/src/api/structure_designer/mod.rs`
- [ ] Run `flutter_rust_bridge_codegen generate`
- [ ] Update `AiAssistantServer` to accept StructureDesigner reference
- [ ] Update `_handleQuery` to call Rust API
- [ ] Update `_handleEdit` to call Rust API and trigger UI refresh
- [ ] Update server initialization to pass StructureDesigner
- [ ] Remove stub constants (or keep as fallback)

### 5.8 Testing Strategy

**Unit Tests (Rust):**
- [ ] TextValue serialization/deserialization
- [ ] Parser correctness (valid inputs)
- [ ] Parser error handling (invalid inputs)
- [ ] Each node's get/set_text_properties
- [ ] NetworkSerializer with snapshot tests
- [ ] NetworkEditor operations

**Integration Tests:**
- [ ] Query → Edit round-trip (query, edit back, should be equivalent)
- [ ] Incremental edit preserves unmentioned nodes
- [ ] Replace mode clears network
- [ ] Wire connections work correctly
- [ ] Delete removes node and wires

**End-to-End Tests:**
- [ ] CLI query command
- [ ] CLI edit command
- [ ] HTTP server query endpoint
- [ ] HTTP server edit endpoint
- [ ] UI refreshes after edit

---

## Appendix A: Text Format Quick Reference

### Literals

```
# Booleans
true, false

# Integers
42, -10, 0

# Floats (must have decimal or exponent)
3.14, -1.5, 2.5e-3, 1.0

# Strings
"hello", "path/to/file.xyz"

# Multi-line strings
"""
line 1
line 2
"""

# Vectors
(1, 2)           # IVec2
(1, 2, 3)        # IVec3
(1.0, 2.0)       # Vec2
(1.0, 2.0, 3.0)  # Vec3

# Arrays
[1, 2, 3]
[sphere1, box1]
```

### Statements

```
# Assignment (create or update)
name = type { prop: value, prop: value }

# Output
output node_name

# Delete
delete node_name

# Comments
# This is a comment
```

### References

```
# Regular output reference
union1 = union { shapes: [sphere1, box1] }

# Function pin reference (with @)
map1 = map { f: @pattern }
```

---

## Appendix B: Node Property Reference

Quick reference for what properties each node exposes:

| Node | Properties |
|------|------------|
| `int` | `value: Int` |
| `float` | `value: Float` |
| `bool` | `value: Bool` |
| `string` | `value: String` |
| `ivec2` | `x: Int, y: Int` |
| `ivec3` | `x: Int, y: Int, z: Int` |
| `vec2` | `x: Float, y: Float` |
| `vec3` | `x: Float, y: Float, z: Float` |
| `sphere` | `center: IVec3, radius: Int` |
| `cuboid` | `min_corner: IVec3, extent: IVec3` |
| `circle` | `center: IVec2, radius: Int` |
| `rect` | `min_corner: IVec2, extent: IVec2` |
| `range` | `start: Int, step: Int, count: Int` |
| `expr` | `expression: String, parameters: [...]` |
| `map` | `input_type: DataType, output_type: DataType` |
| `motif` | `definition: String` |
| `unit_cell` | `a: Float, b: Float, c: Float, alpha: Float, beta: Float, gamma: Float` |
| `atom_fill` | `parameter_element_value_definition: String, m_offset: Vec3, passivate: Bool, rm_single: Bool, surf_recon: Bool` |
| `union`, `intersect` | (no stored properties, only connections) |
| `diff` | (no stored properties, only connections) |

---

## Appendix C: File Structure

```
rust/src/structure_designer/
├── text_format/                      # NEW DIRECTORY
│   ├── mod.rs
│   ├── text_value.rs                 # TextValue enum
│   ├── serializer.rs                 # TextValue → String
│   ├── parser.rs                     # String → Statements
│   ├── network_serializer.rs         # Network → Text (query)
│   ├── network_editor.rs             # Text → Network changes (edit)
│   └── auto_layout.rs                # Position calculation for new nodes
├── node_data.rs                      # Updated with new trait methods
└── nodes/
    ├── *.rs                          # Each updated with get/set_text_properties

rust/src/api/structure_designer/
├── ai_assistant_api.rs               # NEW: query_network, edit_network
└── structure_designer_api.rs         # Updated: ai_query_network, ai_edit_network

lib/ai_assistant/                     # EXISTING (Phase 1)
├── http_server.dart                  # Update: call Rust instead of stubs
└── constants.dart                    # Existing: port, stub responses (can remove stubs later)

bin/
└── atomcad_cli.dart                  # EXISTING (Phase 1) - no changes needed
```

---

## Open Questions / Future Work

1. **Port configuration:** How should the HTTP server port be configured/discovered?

2. **Authentication:** Is localhost-only access sufficient, or do we need tokens?

3. **Error format:** Standardize error response JSON structure

4. **Validation errors:** How to surface node validation errors (e.g., invalid expr)?

5. **Undo/redo:** Should AI edits integrate with undo stack?

6. **Batch operations:** Support for multiple edit operations in one request?

7. **Streaming:** For large networks, consider streaming response?

8. **Versioning:** API version in endpoint path for future compatibility?
