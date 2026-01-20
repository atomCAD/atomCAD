# atomCAD Skill Enhancement Plan

This document outlines the plan for enhancing the atomCAD skill to enable AI agents to use atomCAD efficiently via the CLI.

## Goals

1. **Dynamic documentation:** Node reference via CLI commands (not static docs)
2. **Concise skill.md:** Respect context window, assume agent knowledge of crystallography
3. **Progressive disclosure:** Core concepts in skill.md, details via CLI or references/

## Decisions

- CLI output format: **Human-readable text** (agents understand it too)
- Categories: **Expose all** (MathAndProgramming, Geometry2D, Geometry3D, AtomicStructure, Annotation, OtherBuiltin, Custom)
- Example generation in `describe`: **Postponed** (not trivial)
- References folder: **Yes**, create for detailed topics

---

## Phase 1: Implement Dynamic Node Documentation CLI Commands

### New CLI Commands

| Command | Purpose |
|---------|---------|
| `atomcad-cli nodes` | List all available node types by category |
| `atomcad-cli nodes --category=<cat>` | List nodes in specific category |
| `atomcad-cli describe <node-name>` | Full details: description, inputs, output type |

### Technical Challenge: Discovering All Text Property Names

In atomCAD's text format, valid property names come from **two sources**:

1. **NodeType.parameters** - Input pins that can be wired
2. **NodeData.get_text_properties()** - Properties with stored default values

These overlap but are not identical:

| Category | Example (atom_fill) | Wirable? | Has Default? |
|----------|---------------------|----------|--------------|
| **Parameter + Property** | `m_offset`, `passivate` | Yes | Yes |
| **Parameter only** | `shape`, `motif` | Yes | No (required) |
| **Property only** | `parameter_element_value_definition` | No | Yes |

**Solution: Runtime Introspection**

Use `node_data_creator()` to create a default instance, then call `get_text_properties()`:

```rust
pub fn get_node_type_info(node_type: &NodeType, registry: &NodeTypeRegistry) -> NodeTypeInfo {
    // 1. Get parameters from NodeType
    let param_names: HashSet<_> = node_type.parameters.iter()
        .map(|p| p.name.clone()).collect();

    // 2. Create default instance and get text properties
    let default_data = (node_type.node_data_creator)();
    let text_props = default_data.get_text_properties();

    // 3. Build property map: name -> (type, default_value)
    let prop_map: HashMap<String, (DataType, String)> = text_props.iter()
        .map(|(name, value)| {
            (name.clone(), (value.inferred_data_type(), format_text_value(value)))
        }).collect();

    // 4. Build parameter info (mark which have defaults)
    let parameters: Vec<ParameterInfo> = node_type.parameters.iter().map(|p| {
        let default_info = prop_map.get(&p.name);
        ParameterInfo {
            name: p.name.clone(),
            data_type: p.data_type.to_string(),
            has_default: default_info.is_some(),
            default_value: default_info.map(|(_, v)| v.clone()),
        }
    }).collect();

    // 5. Find properties that are NOT parameters (stored-only)
    let stored_only_properties: Vec<PropertyInfo> = text_props.iter()
        .filter(|(name, _)| !param_names.contains(name))
        .map(|(name, value)| PropertyInfo {
            name: name.clone(),
            data_type: value.inferred_data_type().to_string(),
            default_value: format_text_value(value),
        }).collect();

    NodeTypeInfo {
        name: node_type.name.clone(),
        description: node_type.description.clone(),
        category: format!("{:?}", node_type.category),
        parameters,
        stored_only_properties,
        output_type: node_type.output_type.to_string(),
    }
}
```

**Key insight:** `TextValue::inferred_data_type()` returns the `DataType` for any text value, enabling type discovery without additional metadata.

**Benefits:**
- No changes to existing node definitions
- Works for custom nodes (node networks)
- Shows default values in documentation
- Distinguishes required vs optional parameters

---

### Implementation Tasks

#### 1.1 Rust API Layer

**File:** `rust/src/api/structure_designer/ai_assistant_api.rs`

Add new FFI functions:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn ai_list_node_types(category: Option<String>) -> String {
    // Returns human-readable list of node types
    // Groups by category, shows name + brief description
}

#[flutter_rust_bridge::frb(sync)]
pub fn ai_describe_node_type(node_type_name: String) -> String {
    // Returns detailed human-readable description:
    // - Name, category, description
    // - Input pins with names and types
    // - Output type
    // Works for both built-in AND custom nodes
}
```

#### 1.2 HTTP Server Endpoints

**File:** `lib/ai_assistant/http_server.dart`

Add endpoints:

| Endpoint | Method | Handler |
|----------|--------|---------|
| `/nodes` | GET | List nodes, optional `?category=X` param |
| `/describe?node=<name>` | GET | Describe specific node |

#### 1.3 CLI Commands

**File:** `bin/atomcad_cli.dart`

Add command parsing:

```dart
// New commands
parser.addCommand('nodes');      // with --category option
parser.addCommand('describe');   // positional arg: node name
```

#### 1.4 Regenerate FFI Bindings

```bash
flutter_rust_bridge_codegen generate
```

### Expected Output Examples

**`atomcad-cli nodes`:**
```
=== MathAndProgramming ===
  int          - Outputs an integer value
  float        - Outputs a float value
  vec3         - Outputs a Vec3 value
  expr         - Evaluates a mathematical expression
  range        - Creates an array of integers
  map          - Applies function to array elements
  ...

=== Geometry3D ===
  cuboid       - Outputs a cuboid with integer corner and extent
  sphere       - Outputs a sphere with integer center and radius
  union        - Boolean union of geometries
  diff         - Boolean difference of geometries
  ...

=== AtomicStructure ===
  atom_fill    - Fills geometry with atoms using a motif
  atom_trans   - Transforms atomic structure in real space
  ...
```

**`atomcad-cli describe sphere`:**
```
Node: sphere
Category: Geometry3D
Description: Outputs a sphere with integer center coordinates and integer radius.

Parameters (input pins):
  center    : IVec3     [default: (0, 0, 0)]
  radius    : Int       [default: 1]
  unit_cell : UnitCell  [no default - wire only]

Output: Geometry
```

**`atomcad-cli describe atom_fill`:**
```
Node: atom_fill
Category: AtomicStructure
Description: Converts a 3D geometry into an atomic structure by carving out
a crystal from an infinite crystal lattice using the geometry on its shape input.

Parameters (input pins):
  shape        : Geometry  [required]
  motif        : Motif     [default: cubic zincblende]
  m_offset     : Vec3      [default: (0, 0, 0)]
  passivate    : Bool      [default: true]
  rm_single    : Bool      [default: false]
  surf_recon   : Bool      [default: false]
  invert_phase : Bool      [default: false]

Properties (not wirable):
  parameter_element_value_definition : String  [default: ""]

Output: Atomic
```

---

## Phase 2: Restructure SKILL.md

### Target Structure (~150-200 lines)

```
skill.md
├── Frontmatter (name, description)
├── § Prerequisites
├── § Core Concepts
│   ├── Node Networks (DAG, typed pins, wires, evaluation)
│   ├── Data Types (table + implicit conversions)
│   └── Text Format Syntax
├── § CLI Commands
│   ├── Global Options
│   ├── Network Operations (query, edit, replace)
│   └── Node Discovery (nodes, describe)
├── § Common Patterns (2-3 practical examples)
└── § See Also (link to references/)
```

### Content Guidelines

**Include (atomCAD-specific, non-obvious):**
- Node network DAG model and evaluation semantics
- Data type system and pin compatibility rules
- Text format syntax specification
- CLI command reference
- Brief workflow patterns

**Omit (Claude already knows):**
- Crystallography basics (unit cells, Miller indices, motifs)
- CSG/SDF concepts
- What atoms and bonds are
- Full node reference (use `describe` command)
- GUI/UI instructions

### Section Drafts

#### Core Concepts: Node Networks

```markdown
## Core Concepts

### Node Networks

atomCAD designs are parametric node networks (DAGs):
- **Nodes** have typed input pins and one output pin
- **Wires** connect output→input of compatible types
- **Evaluation** is lazy: only visible nodes trigger computation
- **Custom nodes** are defined by creating a node network with the same name

Each node network can have an **output node** (set via `output <node_id>`) that
defines what value the network returns when used as a custom node.
```

#### Core Concepts: Data Types

```markdown
### Data Types

| Type | Description |
|------|-------------|
| `Bool`, `String`, `Int`, `Float` | Primitives |
| `Vec2`, `Vec3`, `IVec2`, `IVec3` | 2D/3D vectors (float/int) |
| `Geometry2D` | 2D shapes (for extrusion) |
| `Geometry` | 3D geometry (SDF-based) |
| `Atomic` | Atomic structure (atoms + bonds) |
| `UnitCell` | Crystal lattice parameters |
| `Motif` | Crystal motif definition |
| `[T]` | Array of type T |
| `A -> B` | Function type |

**Implicit conversions:** `Int`↔`Float`, `IVec`↔`Vec`, `T`→`[T]`

Array pins (marked with dot) accept multiple wires; values are concatenated.
```

#### CLI Commands: Node Discovery

```markdown
### Node Discovery

```bash
# List all node types by category
atomcad-cli nodes

# List nodes in specific category
atomcad-cli nodes --category=Geometry3D

# Get detailed info about a node type (works for custom nodes too)
atomcad-cli describe <node-name>
atomcad-cli describe sphere
```

Use `describe` to discover input pins, types, and behavior for any node.
```

---

## Phase 3: Create References Folder

### Structure

```
.claude/skills/atomcad/
├── skill.md
└── references/
    ├── text-format.md    # Complete text format specification
    └── data-types.md     # Detailed type system documentation
```

### references/text-format.md

Full specification of the text format syntax:
- Node creation syntax
- Wire connection syntax
- Output node syntax
- Delete syntax
- Comments
- Multi-line handling
- Error handling

### references/data-types.md

Detailed type system documentation:
- All types with full descriptions
- Implicit conversion rules
- Function types and partial application
- Array handling and concatenation

---

## Implementation Order

1. **Phase 1.1-1.4:** Implement CLI commands for dynamic documentation
2. **Phase 2:** Rewrite skill.md with new structure
3. **Phase 3:** Create references/ folder with detailed docs
4. **Testing:** Verify skill works end-to-end with test prompts

## Files to Modify

| File | Changes |
|------|---------|
| `rust/src/api/structure_designer/ai_assistant_api.rs` | Add `ai_list_node_types`, `ai_describe_node_type` |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add response types if needed |
| `lib/ai_assistant/http_server.dart` | Add `/nodes`, `/describe` endpoints |
| `bin/atomcad_cli.dart` | Add `nodes`, `describe` commands |
| `.claude/skills/atomcad/skill.md` | Rewrite with new structure |
| `.claude/skills/atomcad/references/text-format.md` | Create |
| `.claude/skills/atomcad/references/data-types.md` | Create |

## Success Criteria

1. `atomcad-cli nodes` lists all node types grouped by category
2. `atomcad-cli describe <node>` returns useful info for any node (built-in or custom)
3. skill.md is under 200 lines and contains essential concepts
4. An agent with this skill can efficiently create/modify atomCAD designs
