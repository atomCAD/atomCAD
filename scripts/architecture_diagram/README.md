# Architecture Diagram Generator

Automatically generates an SVG visualization of atomCAD's crate architecture.

## What it does

- **Counts lines of code** in each crate (excluding comments, empty lines, and generated files)
- **Generates an SVG diagram** where:
  - Circle sizes represent lines of code
  - Arrows show crate dependencies
  - Colors distinguish different crates

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

## Crate Definitions

Since `doc/design_rust_crate_split.md` the backend is a cargo workspace, so
each circle below is a real crate rather than a module convention. Only `src/`
is counted — the test trees are larger than the sources here and would
dominate the areas.

### Rust crates
- **api**: FFI boundary, the root package `rust_lib_flutter_cad` (`rust/src/api/`; the generated `frb_generated.rs` is excluded)
- **structure_designer**: `atomcad-structure-designer` — node network, evaluator, nodes, and `expr` as a submodule (`rust/crates/atomcad-structure-designer/src/`)
- **display**: `atomcad-display` — tessellation adapter layer (`rust/crates/atomcad-display/src/`)
- **crystolecule**: `atomcad-crystolecule` — atomic structure library (`rust/crates/atomcad-crystolecule/src/`)
- **renderer**: `atomcad-renderer` — GPU rendering (`rust/crates/atomcad-renderer/src/`)
- **geo_tree**: `atomcad-geo-tree` — geometry library (`rust/crates/atomcad-geo-tree/src/`)
- **util**: `atomcad-util` — foundation utilities (`rust/crates/atomcad-util/src/`)

`atomcad-test-support` is deliberately absent: it is a dev-only helper crate,
not a layer.

### Flutter Module
- **ui**: Flutter UI code (`lib/`, excluding `*.g.dart` and `*.freezed.dart`)

## Customization

Edit `generate_architecture_diagram.py` to modify:
- **Colors**: `COLORS` dictionary
- **Layout**: `LAYERS` and spacing constants
- **Dependencies**: `DEPENDENCIES` list — mirror the `[dependencies]` sections
  of the workspace manifests; put an edge you do not want drawn in
  `ELIDED_ARROWS` rather than deleting it, so the list stays a faithful copy
- **Sizing**: `LOC_SCALE`, `MIN_RADIUS`

`LOC_SCALE` is the one to re-check after a large growth spurt: circle *area* is
proportional to LOC, so the biggest circle's diameter must stay below
`LAYER_SPACING` or adjacent layers overlap.

## Requirements

- Python 3.7+
- No external dependencies (uses only standard library)
