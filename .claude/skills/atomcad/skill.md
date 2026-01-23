---
name: atomcad
description: Interact with atomCAD node networks programmatically. Query, edit, and replace CAD geometry nodes for atomic/molecular structure design. Use when working with atomCAD projects or when the user wants to manipulate node networks, create CSG shapes, or design atomic structures.
license: MIT
metadata:
  author: atomCAD
  version: "2.0"
allowed-tools: Bash(atomcad-cli:*), Bash(./atomcad-cli:*)
---

# atomCAD Skill

Programmatically interact with atomCAD node networks via CLI. Requires atomCAD to be running.

## Prerequisites

- atomCAD running (the CLI connects to a running instance)

**CLI access** (automatic detection):
- **In atomCAD repo root**: `./atomcad-cli` works directly (no PATH setup needed)
- **Elsewhere**: requires `atomcad-cli` on PATH

## Command Resolution

Before running CLI commands, detect the appropriate command:
- If `./atomcad-cli` exists in current directory → use `./atomcad-cli`
- Otherwise → use `atomcad-cli` (assumes PATH is configured)

## Core Concepts

### Node Networks

atomCAD designs are parametric node networks (DAGs):

- **Nodes** have typed input pins (parameters) and one output pin
- **Wires** connect output→input of compatible types
- **Evaluation** is lazy: only visible nodes trigger computation
- **Custom nodes** are subnetworks with matching names

Each network can have an **output node** (set via `output <node_id>`) that defines what value the network returns when used as a custom node.

### Lattice Coordinates

**Important:** Geometry coordinates (positions, sizes, radii) are in discrete **lattice units** (integers), not angstroms. The actual physical dimensions depend on the unit cell. This ensures all atomic structures are naturally lattice-aligned.

### Unit Cell and Motif

- **Default unit cell:** Cubic diamond (3.567Å lattice constant) if none specified
- **Default motif:** Cubic zincblende with carbon atoms (pure diamond)
- Geometry nodes accept a `unit_cell` input to specify different lattices
- Non-cubic unit cells cause shapes like `cuboid` to become parallelepipeds

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

### Text Format Syntax

```
# Create nodes: id = type { parameter: value, ... }
sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
cuboid1 = cuboid { min_corner: (-5, -5, -5), extent: (10, 10, 10) }

# Wire nodes by referencing node IDs as input values
union1 = union { shapes: [sphere1, cuboid1], visible: true }

# Set network output node
output union1

# Delete a node
delete sphere1
```

**Parameter values:**
- Integers: `42`, `-10`
- Floats: `3.14`, `1.5e-3`
- Booleans: `true`, `false`
- Strings: `"hello"`
- Vectors: `(x, y)` or `(x, y, z)`
- Arrays: `[a, b, c]`
- Node references: use the node's ID

**Note on vectors:** The tuple syntax `(x, y, z)` is for literal values in node parameters (e.g., `sphere { center: (5, 5, 5) }`). The vector constructor nodes (`vec2`, `vec3`, `ivec2`, `ivec3`) use separate component inputs instead:
```
vec3 { x: 1.5, y: 2.5, z: 3.5 }
ivec3 { x: 1, y: 2, z: 3 }
```

## CLI Commands

### Global Options

```bash
atomcad-cli --help       # Show help
atomcad-cli --port=PORT  # Custom server port (default: 19847)
```

### Network Operations

```bash
# Query current network state
atomcad-cli query

# Edit network (add/update nodes, keeps existing)
atomcad-cli edit --code="sphere1 = sphere { radius: 10 }"

# Replace entire network (clears first)
atomcad-cli edit --code="..." --replace

# Multi-line edit (reads stdin until empty line or ".")
atomcad-cli edit
atomcad-cli edit --replace
```

### Multi-line Input

For multi-line code, **stdin piping is recommended** as it avoids shell quoting issues:

```bash
# Recommended: pipe multi-line code via stdin
echo "base = cuboid { extent: (10, 10, 10) }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }" | atomcad-cli edit --replace

# Or use a heredoc
atomcad-cli edit --replace <<'EOF'
base = cuboid { extent: (10, 10, 10) }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }
EOF
```

**Alternative:** The `--code` flag supports `\n` escape sequences for newlines:

```bash
# Escape sequences: \n for newline, \t for tab, \\ for literal backslash
atomcad-cli edit --replace --code="base = cuboid { extent: (10, 10, 10) }\nhole = sphere { radius: 4 }\nresult = diff { base: [base], sub: [hole], visible: true }"
```

**Note:** Avoid using literal newlines inside `--code="..."` as shell quoting behavior varies across platforms (bash, PowerShell, etc.).

### Evaluate Node Results

Evaluate a specific node and return its computed result (useful for verification and debugging).

```bash
# Evaluate a node by ID or custom name
atomcad-cli evaluate <node_id>
atomcad-cli evaluate sphere1

# Verbose output (detailed info for complex types)
atomcad-cli evaluate <node_id> --verbose
atomcad-cli evaluate atoms1 --verbose
```

**Output formats:**
- **Primitives** (Float, Int, Bool, String): Display the value directly (e.g., `42`, `3.140000`, `true`)
- **Vectors** (Vec3, IVec3, etc.): Display coordinates (e.g., `(5.000000, 10.000000, 3.000000)`)
- **Geometry/Geometry2D**: Display `Geometry` or `Geometry2D` (brief), CSG tree details (verbose)
- **Atomic**: Display `Atomic` (brief), or atom/bond counts and sample atoms (verbose)

**Examples:**
```bash
$ atomcad-cli evaluate sphere1
Geometry

$ atomcad-cli evaluate count1
42

$ atomcad-cli evaluate pos1
(5.000000, 10.000000, 3.000000)

$ atomcad-cli evaluate atoms1 --verbose
Atomic:
atoms: 968
bonds: 1372
first 10 atoms:
  [1] Z=6 pos=(-6.242250, -6.242250, -4.458750) bonds=4
  ...
```

### Node Discovery

```bash
# List all node types by category
atomcad-cli nodes

# List nodes in specific category
atomcad-cli nodes --category=Geometry3D

# List with descriptions (verbose mode, can combine with --category)
atomcad-cli nodes --verbose

# Get detailed info about any node type (built-in or custom)
atomcad-cli describe <node-name>
atomcad-cli describe sphere
atomcad-cli describe atom_fill
```

Use `describe` to discover input pins, types, defaults, and behavior for any node.

### Camera Control

Control the viewport camera position and projection mode.

```bash
# Get current camera state (returns JSON)
atomcad-cli camera

# Set camera position and orientation
atomcad-cli camera --eye x,y,z --target x,y,z --up x,y,z

# Switch projection mode
atomcad-cli camera --orthographic    # Orthographic projection (no perspective)
atomcad-cli camera --perspective     # Perspective projection (default)

# Set orthographic zoom level (half-height of viewport)
atomcad-cli camera --ortho-height 50

# Combined example: position camera and switch to orthographic
atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic
```

**Important:** Camera coordinates are in **angstroms**, while geometry node coordinates are in **lattice units**. For cubic diamond (default), 1 lattice unit ≈ 3.567 Å. A 20×20×20 lattice unit object is ~71 Å across, so position the camera 150-250 Å away to see the full object.

**Parameters:**
- `--eye x,y,z` — Camera position in angstroms (world coordinates)
- `--target x,y,z` — Point the camera looks at (angstroms)
- `--up x,y,z` — Up vector for camera orientation (typically `0,0,1`)
- `--orthographic` — Enable orthographic projection (parallel rays, no perspective distortion)
- `--perspective` — Enable perspective projection (default, realistic depth)
- `--ortho-height N` — Half-height of orthographic viewport (controls zoom level)

**Response:** Returns JSON with current camera state:
```json
{
  "success": true,
  "camera": {
    "eye": [30.0, 30.0, 30.0],
    "target": [0.0, 0.0, 0.0],
    "up": [0.0, 0.0, 1.0],
    "orthographic": true,
    "ortho_half_height": 25.0
  }
}
```

### Display Settings

Control viewport display preferences including atomic visualization and geometry rendering.

```bash
# Get current display settings (returns JSON)
atomcad-cli display

# Set atomic visualization mode
atomcad-cli display --atomic-viz ball-and-stick    # Small atoms with bond sticks
atomcad-cli display --atomic-viz space-filling     # Large van der Waals spheres

# Set geometry visualization mode
atomcad-cli display --geometry-viz surface-splatting  # Implicit SDF rendering
atomcad-cli display --geometry-viz solid              # Solid mesh (default)
atomcad-cli display --geometry-viz wireframe          # Wireframe mesh

# Set node display policy
atomcad-cli display --node-policy manual           # User controls visibility
atomcad-cli display --node-policy prefer-selected  # Show selected node (default)
atomcad-cli display --node-policy prefer-frontier  # Show output nodes

# Set background color
atomcad-cli display --background 30,30,30          # Dark gray (default)
atomcad-cli display --background 255,255,255       # White

# Combine multiple settings
atomcad-cli display --atomic-viz space-filling --geometry-viz wireframe
```

**Parameters:**
- `--atomic-viz <mode>` — Atomic visualization: `ball-and-stick` (default) or `space-filling`
- `--geometry-viz <mode>` — Geometry visualization: `solid` (default), `wireframe`, or `surface-splatting`
- `--node-policy <policy>` — Node display: `prefer-selected` (default), `manual`, or `prefer-frontier`
- `--background R,G,B` — Background color as RGB values 0-255 (default: 30,30,30)

**Response:** Returns JSON with current display state:
```json
{
  "success": true,
  "display": {
    "atomic_visualization": "ball-and-stick",
    "geometry_visualization": "solid",
    "node_display_policy": "prefer-selected",
    "background_color": [30, 30, 30]
  }
}
```

### Screenshot Capture

Capture the current viewport to a PNG image file.

```bash
# Capture with default resolution (current viewport size)
atomcad-cli screenshot -o output.png

# Auto-generate timestamped filename (screenshot_YYYY-MM-DD_HHMMSS.png)
atomcad-cli screenshot -o auto

# Capture with specific resolution
atomcad-cli screenshot -o output.png -w 1920 -h 1080

# Capture with custom background color (R,G,B values 0-255)
atomcad-cli screenshot -o output.png --background 255,255,255
```

**Parameters:**
- `-o, --output <path>` — Output PNG file path (required). Use `auto` to generate a timestamped filename
- `-w, --width N` — Image width in pixels (optional, uses viewport size if not specified)
- `-h, --height N` — Image height in pixels (optional, uses viewport size if not specified)
- `--background R,G,B` — Background color as RGB values 0-255 (default: 30,30,30 dark gray)

**Response:** Returns confirmation with file path and actual dimensions:
```
Screenshot saved: /path/to/output.png (1920x1080)
```

**Note:** Relative paths are resolved relative to the CLI's working directory, not atomCAD's.

**Always verify screenshots:** After capturing, read the PNG file to check the result. If the view is too close (only atoms visible, no overall shape) or too far (object too small), adjust the camera and retake. Common issue: using small camera coordinates puts you inside the atomic structure. If the entire screenshot appears green, you are too close to the objects (geometry is rendered in green).

**Recommended visualization styles for attractive screenshots:**
1. **Ball-and-stick with visible geometry:** Use `--atomic-viz ball-and-stick` and make the geometry node feeding into `atom_fill` visible (`visible: true`). This shows atoms running along the surface of the green geometry shape—a visually striking combination.
2. **Space-filling only:** Use `--atomic-viz space-filling` and hide geometry nodes (`visible: false`). The large van der Waals spheres will mostly occlude any geometry anyway, so keeping geometry hidden produces cleaner results.

### Typical AI Agent Workflow

Combine geometry creation, camera control, display settings, and screenshots for visual verification:

```bash
# 1. Create geometry
atomcad-cli edit --code="s = sphere { radius: 10, visible: true }"

# 2. Position camera for good viewing angle
atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic

# 3. Adjust display settings for clear visualization
atomcad-cli display --atomic-viz space-filling --background 255,255,255

# 4. Capture screenshot for visual verification
atomcad-cli screenshot -o sphere_check.png

# 5. Verify result by viewing the image
# (The AI agent can read the PNG file to see the rendered geometry)
```

### REPL Mode

```bash
atomcad-cli              # Enter interactive mode
```

Commands:
- `query`/`q` — Show current node network
- `edit` — Enter edit mode (incremental)
- `replace`/`r` — Enter edit mode (replace entire network)
- `evaluate`/`e <node>` — Evaluate a node
- `nodes` — List available node types
- `describe`/`d <node>` — Describe a node type
- `camera`/`c` — Get/set camera state
- `display` — Get/set display preferences
- `screenshot`/`s <path>` — Capture viewport to PNG
- `help`/`?` — Show help
- `quit`/`exit` — Exit REPL

## Common Patterns

### Create a simple atomic structure

```bash
# Create sphere geometry and fill with atoms
atomcad-cli edit --code="s = sphere { radius: 5, visible: true }"
atomcad-cli edit --code="atoms = atom_fill { shape: s, visible: true }"
```

### Boolean operations on geometry

```bash
# Using heredoc (recommended for multi-line)
atomcad-cli edit --replace <<'EOF'
base = cuboid { extent: (10, 10, 10), visible: false }
hole = sphere { center: (5, 5, 5), radius: 4 }
result = diff { base: [base], sub: [hole], visible: true }
EOF

# Or using escape sequences
atomcad-cli edit --replace --code="base = cuboid { extent: (10, 10, 10), visible: false }\nhole = sphere { center: (5, 5, 5), radius: 4 }\nresult = diff { base: [base], sub: [hole], visible: true }"
```

**Note:** `diff` and `diff_2d` accept arrays on both inputs. They implicitly union each array before computing the difference: `diff(base, sub) = base₁ ∪ base₂ ∪ ... − (sub₁ ∪ sub₂ ∪ ...)`

### Creating realistic atomic structures

```bash
# Passivate dangling bonds with hydrogen
atomcad-cli edit --code="atoms = atom_fill { shape: geom, passivate: true, visible: true }"

# Remove single-bond atoms recursively (cleaner structures)
atomcad-cli edit --code="atoms = atom_fill { shape: geom, rm_single: true, visible: true }"

# Override motif elements (e.g., silicon carbide instead of diamond)
atomcad-cli edit --code="atoms = atom_fill { shape: geom, element_values: \"PRIMARY Si\\nSECONDARY C\", visible: true }"
```

**atom_fill options:**
- `passivate: true` — Add hydrogen atoms to dangling bonds
- `rm_single: true` — Recursively remove atoms with only one bond
- `surf_recon: true` — Enable surface reconstruction (cubic diamond (100) 2×1 only)
- `element_values: "..."` — Override motif parameter elements (newline-separated `PARAM_NAME Element`)
- `m_offset: (x, y, z)` — Fractional offset (0-1) to adjust cut position

### Parametric design with custom nodes

Custom nodes are created by defining subnetworks:

1. Create a node network named `my_shape`
2. Add `parameter` nodes to define inputs (each parameter becomes an input pin)
3. Set the network's output node via `output <node_id>`
4. Use `my_shape` as a node type in other networks

```bash
# In network "scaled_sphere" (using heredoc):
atomcad-cli edit <<'EOF'
size = parameter { name: "size", type: "Int", default: 5 }
s = sphere { radius: size }
output s
EOF

# Then use it elsewhere:
atomcad-cli edit --code="part1 = scaled_sphere { size: 10, visible: true }"
```

### Functional programming with map

The `map` node applies a function to each array element. Combined with partial application:

```bash
# range creates [0, 1, 2, ...] array
# pattern is a custom node with inputs: index (Int), gap (Int)
# When wired to map's f input, gap is bound; only index varies
atomcad-cli edit <<'EOF'
r = range { start: 0, count: 5, step: 1 }
result = map { xs: r, f: pattern, gap: 3, visible: true }
EOF
```

Extra parameters beyond the expected function signature are bound at wire-time (partial application), enabling parametric patterns.

### Mathematical expressions with expr

The `expr` node evaluates mathematical expressions with dynamic input pins.

**Important:** The `parameters` property defines which input pins the expression uses. Each parameter needs a `name` and `data_type`. The default expr node only has one parameter named `x` of type `Int`.

```bash
# Simple case: using the default 'x' parameter
atomcad-cli edit <<'EOF'
val = int { value: 5 }
doubled = expr { expression: "x * 2", x: val }
EOF

# Multiple inputs: must define parameters explicitly
atomcad-cli edit <<'EOF'
a = int { value: 5 }
b = int { value: 3 }
sum = expr {
  expression: "x + y",
  parameters: [{ name: "x", data_type: Int }, { name: "y", data_type: Int }],
  x: a,
  y: b
}
EOF
```

**Available data types for parameters:** `Int`, `Float`, `Bool`, `Vec2`, `Vec3`, `IVec2`, `IVec3`

**Supported in expr:**
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `^` (exponent)
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logic: `&&`, `||`, !
- Conditionals: `if condition then value1 else value2`
- Vectors: `vec2(x,y)`, `vec3(x,y,z)`, `ivec2`, `ivec3`, member access `.x`, `.y`, `.z`
- Functions: `sin`, `cos`, `tan`, `sqrt`, `abs`, `floor`, `ceil`, `round`, `length2`, `length3`, `normalize2`, `normalize3`, `dot2`, `dot3`, `cross`, `distance2`, `distance3`

## Important Notes

- **facet_shell:** Currently only works correctly with cubic unit cells
- **lattice_move/lattice_rot:** Discrete lattice transformations (integers only). For continuous transforms on atomic structures, use `atom_trans` instead:
  ```bash
  atoms = atom_fill { shape: geom, visible: false }
  moved = atom_trans { molecule: atoms, translation: (10.0, 0.0, 0.0), visible: true }
  ```
- **half_space:** Creates infinite half-spaces; useful for clipping geometry via intersection
- **Lone atoms:** `atom_fill` automatically removes atoms with zero bonds after the geometry cut

## See Also

- `atomcad-cli describe <node>` for detailed node documentation
- `atomcad-cli nodes` to browse available node types
- `references/text-format.md` for complete text format specification
- `references/data-types.md` for detailed type system documentation
