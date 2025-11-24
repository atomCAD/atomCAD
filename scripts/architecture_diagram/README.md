# Architecture Diagram Generator

Automatically generates an SVG visualization of atomCAD's module architecture.

## What it does

- **Counts lines of code** in each module (excluding comments, empty lines, and generated files)
- **Generates an SVG diagram** where:
  - Circle sizes represent lines of code
  - Arrows show module dependencies
  - Colors distinguish different modules

## Usage

### Quick Start (Windows)

Simply double-click:
```
update_diagram.bat
```

### Manual Steps

1. Count lines of code:
```bash
python count_loc.py
```

2. Generate SVG diagram:
```bash
python generate_architecture_diagram.py
```

### Output

- **LOC counts**: `scripts/architecture_diagram/loc_counts.json`
- **SVG diagram**: `doc/architecture_diagram.svg`

## Module Definitions

### Rust Modules
- **structure_designer**: Application logic + API (`rust/src/structure_designer/` + `rust/src/api/`)
- **crystolecule**: Atomic structure library (`rust/src/crystolecule/`)
- **renderer**: GPU rendering (`rust/src/renderer/`)
- **display**: Tessellation adapter layer (`rust/src/display/`)
- **expr**: Expression language (`rust/src/expr/`)
- **geo_tree**: Geometry library (`rust/src/geo_tree/`)
- **util**: Foundation utilities (`rust/src/util/`)

### Flutter Module
- **ui**: Flutter UI code (`lib/`, excluding `*.g.dart` and `*.freezed.dart`)

## Customization

Edit `generate_architecture_diagram.py` to modify:
- **Colors**: `COLORS` dictionary
- **Layout**: `LAYERS` and spacing constants
- **Dependencies**: `DEPENDENCIES` list
- **Sizing**: `LOC_SCALE`, `MIN_RADIUS`, `MAX_RADIUS`

## Requirements

- Python 3.7+
- No external dependencies (uses only standard library)
