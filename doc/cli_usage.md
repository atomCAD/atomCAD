# atomCAD Command Line Interface (CLI)

The atomCAD CLI allows you to evaluate node networks and export atomic structures without launching the GUI. This is useful for batch processing and automation.

## Installation

Build atomCAD as usual. The CLI functionality is integrated into the main executable.

## Usage

### Mode 1: Single Run

Evaluate a single network with specific parameters:

```bash
atomcad --headless \
  --file <path_to_cnnd> \
  --network <network_name> \
  --output <output_mol_file> \
  [--param <name>=<value>] ...
```

**Example:**
```bash
atomcad --headless \
  --file samples/diamond.cnnd \
  --network DiamondSlab \
  --output diamond_slab.mol \
  --param size=10 \
  --param thickness=5
```

**Short flags:**
```bash
atomcad --headless -f design.cnnd -n MyNetwork -o output.mol -p width=20 -p height=15
```

### Mode 2: Batch Processing

Run multiple evaluations with different parameters using a batch configuration file:

```bash
atomcad --headless --batch <batch_config_file>
```

**Example:**
```bash
atomcad --headless --batch samples/batch_example.toml
```

## Batch Configuration File

Batch files use TOML format. Each run specifies its own network name, allowing you to evaluate different networks from the same `.cnnd` file:

```toml
# Common settings for all runs
cnnd_file = "samples/diamond.cnnd"

# Individual runs
[[run]]
network = "DiamondSlab"
output = "diamond_10x10x10.mol"
[run.params]
size = 10
thickness = 5

[[run]]
network = "DiamondSlab"
output = "diamond_20x20x20.mol"
[run.params]
size = 20
thickness = 10

[[run]]
network = "GrapheneSheet"  # Different network!
output = "graphene.mol"
[run.params]
width = 15
```

## Supported Parameter Types

The CLI currently supports the following data types for parameters:

| Type | Format | Example |
|------|--------|---------|
| `Bool` | `true` or `false` | `--param enabled=true` |
| `Int` | Integer | `--param count=42` |
| `Float` | Decimal | `--param radius=3.14` |
| `String` | Text | `--param name="My Structure"` |
| `Vec2` | `x,y` (comma-separated floats) | `--param point="1.5,2.5"` |
| `Vec3` | `x,y,z` (comma-separated floats) | `--param position="1.0,2.0,3.0"` |
| `IVec2` | `x,y` (comma-separated ints) | `--param grid="10,20"` |
| `IVec3` | `x,y,z` (comma-separated ints) | `--param cell="5,5,5"` |

## Output Format

The CLI exports visible atomic structures to `.mol` files (V3000 format) or `.xyz` files based on the file extension.

## Error Handling

- If the `.cnnd` file is not found, an error is reported
- If the specified network doesn't exist, an error is reported
- If a parameter name is not found in the network, an error is reported
- If a parameter value cannot be parsed, an error is reported with details

## Performance & Flexibility

**Batch mode is significantly faster** than running atomCAD multiple times because:
- The `.cnnd` file is loaded only once
- Node type registry is reused across all runs
- Only parameter values and evaluation need to change between runs

**Batch mode is highly flexible** because each run can specify a different network:
- Evaluate multiple network variants from a single `.cnnd` library
- Compare different structural designs side-by-side
- Generate diverse structures in one automated workflow

## Examples

### Simple Parameter Sweep

Create a batch file to generate diamond slabs of various sizes:

```toml
cnnd_file = "samples/diamond.cnnd"

[[run]]
network = "DiamondSlab"
output = "results/size_5.mol"
[run.params]
size = 5

[[run]]
network = "DiamondSlab"
output = "results/size_10.mol"
[run.params]
size = 10

[[run]]
network = "DiamondSlab"
output = "results/size_15.mol"
[run.params]
size = 15
```

Then run:
```bash
atomcad --headless --batch parameter_sweep.toml
```

### Multiple Networks in One Batch

Evaluate different networks from the same library file:

```toml
cnnd_file = "samples/library.cnnd"

[[run]]
network = "DiamondSlab"
output = "diamond.mol"
[run.params]
size = 10

[[run]]
network = "GrapheneSheet"
output = "graphene.mol"
[run.params]
width = 15
height = 10

[[run]]
network = "NanotubeBuilder"
output = "nanotube.mol"
[run.params]
diameter = 8
length = 20
```

### Vector Parameters

```bash
atomcad --headless \
  -f design.cnnd \
  -n PositionedStructure \
  -o output.mol \
  -p offset="1.5,2.0,3.5" \
  -p cell="10,10,10"
```

## Implementation Status

**Current Status:** The CLI infrastructure is complete with parameter parsing and batch file support. The following steps are stubbed with `println!` statements:

1. ✓ Command line argument parsing
2. ✓ Loading `.cnnd` files
3. ✓ Setting active network
4. ✓ Parameter validation and parsing
5. ⏳ **TODO:** Applying parameter values to network (creating constant nodes)
6. ⏳ **TODO:** Network evaluation (mark_full_refresh + generate_scene)
7. ⏳ **TODO:** Exporting to `.mol`/`.xyz` files

The stubbed steps will be implemented in the next phase.

## Future Enhancements

- Support for more complex parameter types (UnitCell, Geometry, etc.)
- Progress reporting for batch runs
- Parallel execution of independent batch runs
- Validation mode (check network without evaluation)
