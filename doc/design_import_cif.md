# Design: `import_cif` Node

## Motivation

atomCAD currently supports XYZ file import for atomic structures, but XYZ files contain only
atom positions — no crystal lattice information. The CIF (Crystallographic Information File)
format is the standard exchange format for crystallographic data and contains unit cell
parameters, space group symmetry, and atom positions in fractional coordinates. Importing CIF
files would allow users to load any crystal structure from public databases and use it directly
in the atomCAD node network pipeline.

### Target Pipeline

```
+-------------+     +-------------+     +------------+     +-----------+
| import_cif  |---->| motif_edit  |---->|  atom_fill  |---->| result    |
|             |     | (optional)  |     |             |     +-----------+
| pin 0: unit |---->|             |     |             |
| pin 1: atoms|     +-------------+     |             |
| pin 2: motif|------------------------>|             |
+-------------+                         +------+------+
       |                                       |
       +--- unit_cell --------------------------+
```

**Direct path:** Connect `motif` (pin 2) + `unit_cell` (pin 0) to `atom_fill` → fill geometry.

**Editing path:** Connect `atoms` (pin 1) + `unit_cell` (pin 0) to `motif_edit` → edit the
motif interactively → then to `atom_fill`.

---

## CIF Format Reference

### Documentation

- **Atomsk CIF tutorial** (practical, developer-friendly walkthrough):
  https://atomsk.univ-lille.fr/tutorial_cif.php
- **GEMMI CIF parser docs** (best-in-class parser design reference):
  https://gemmi.readthedocs.io/en/latest/cif.html
- **IUCr CIF 1.1 syntax specification** (formal grammar):
  https://www.iucr.org/resources/cif/spec/version1.1/cifsyntax
- **IUCr CIF resources hub**:
  https://www.iucr.org/resources/cif
- **Fractional-to-Cartesian coordinate math**:
  https://daniloroccatano.blog/2023/11/21/crystallographic-coordinates/
- **CCDC Guide to CIFs**:
  https://www.ccdc.cam.ac.uk/community/access-deposit-structures/deposit-a-structure/guide-to-cifs/

### Sample CIF Files

- **Crystallography Open Database (COD)** — largest free database, CC0 licensed:
  https://www.crystallography.net/cod/
  Individual files: `https://www.crystallography.net/cod/1000041.cif`
- **Avogadro element CIFs on GitHub** — small curated set, ideal for testing:
  https://github.com/cryos/avogadro/tree/master/crystals/elements
- **Materials Project** — DFT-optimized structures, free with registration:
  https://next-gen.materialsproject.org/
- **American Mineralogist Crystal Structure Database**:
  https://rruff.geo.arizona.edu/AMS/amcsd.php

### Format Overview

CIF is a plain-text format based on the STAR specification. Key syntax elements:

- **`data_blockname`** — starts a data block (one structure per block)
- **`_tag value`** — tag-value pairs (tags start with `_`, case-insensitive)
- **`loop_`** — introduces tabular data (column tags followed by rows of values)
- **`# comments`** — line comments
- **Quoted strings:** `'single'` or `"double"` for values containing spaces
- **Multi-line strings:** delimited by `;` on its own line (opening and closing)
- **Null values:** `.` (not applicable), `?` (unknown)
- **Numeric uncertainties:** parenthesized, e.g., `5.4307(2)` — strip the `(2)` when parsing

### Key CIF Data Fields

**Unit cell (6 crystallographic parameters):**

| Tag | Meaning |
|-----|---------|
| `_cell_length_a`, `_cell_length_b`, `_cell_length_c` | Lattice vector lengths in Angstroms |
| `_cell_angle_alpha`, `_cell_angle_beta`, `_cell_angle_gamma` | Angles between vectors in degrees |

**Space group (multiple redundant tags — parse whichever is present):**

| Tag | Meaning |
|-----|---------|
| `_symmetry_space_group_name_H-M` | Hermann-Mauguin symbol (e.g., `'F d -3 m'`) |
| `_space_group_name_H-M_alt` | Newer CIF2 equivalent |
| `_space_group_IT_number` | International Tables number (1–230) |
| `_symmetry_Int_Tables_number` | Older equivalent |
| `_symmetry_space_group_name_Hall` | Hall symbol (unambiguous, machine-readable) |

**Symmetry operations (loop):**

| Tag | Meaning |
|-----|---------|
| `_symmetry_equiv_pos_as_xyz` | Symmetry operation string, e.g., `x,y,z` or `-x+1/2,-y,z+1/2` |
| `_space_group_symop_operation_xyz` | Newer CIF2 equivalent |

**Atom sites (loop — the asymmetric unit only):**

| Tag | Meaning |
|-----|---------|
| `_atom_site_label` | Unique label (e.g., `Na1`, `O2`) |
| `_atom_site_type_symbol` | Element symbol (e.g., `Na`, `Cl`, sometimes `Fe3+`) |
| `_atom_site_fract_x`, `_atom_site_fract_y`, `_atom_site_fract_z` | Fractional coordinates |
| `_atom_site_occupancy` | Site occupancy (1.0 = fully occupied) |

### Symmetry Expansion

CIF files store only the **asymmetric unit** — the minimal unique set of atoms. To obtain all
atoms in the conventional unit cell, each asymmetric atom must be transformed by every symmetry
operation, and duplicate positions (within a tolerance) must be removed.

**Example: Diamond (Fd-3m, #227)**

Asymmetric unit: 2 atoms — C at (0,0,0) and C at (0.25,0.25,0.25).

After applying the 192 symmetry operations and deduplicating within [0,1):

| Asymmetric atom | Expanded positions (fractional) |
|---|---|
| C at (0,0,0) | (0,0,0), (0.5,0.5,0), (0.5,0,0.5), (0,0.5,0.5) |
| C at (0.25,0.25,0.25) | (0.25,0.25,0.25), (0.25,0.75,0.75), (0.75,0.25,0.75), (0.75,0.75,0.25) |

Result: 8 atoms in the conventional cubic cell — identical to the existing
`DEFAULT_ZINCBLENDE_MOTIF` with both parameters set to Carbon.

---

## Node Design: `import_cif`

### Node Type

```
Name:        import_cif
Description: Imports a crystal structure from a CIF file. Outputs the unit cell,
             an atomic structure of the full conventional unit cell, and a motif
             with fractional coordinates.
Category:    AtomicStructure
Public:      true
```

### Parameters (Input Pins)

| Index | Name | Type | Description |
|-------|------|------|-------------|
| 0 | `file_name` | String | Path to the .cif file |
| 1 | `block_name` | String | Data block name to use (default: empty = first block) |
| 2 | `use_cif_bonds` | Bool | Use explicit bond data from the CIF file if present (default: true) |
| 3 | `infer_bonds` | Bool | Infer bonds from covalent radii distances (default: true) |
| 4 | `bond_tolerance` | Float | Multiplier on covalent radii sum for bond detection (default: 1.15) |

### Output Pins

| Index | Name | Type | Description |
|-------|------|------|-------------|
| 0 | `unit_cell` | UnitCell | Unit cell from the 6 crystallographic parameters |
| 1 | `atoms` | Atomic | All atoms in the conventional unit cell (Cartesian coordinates, symmetry-expanded) |
| 2 | `motif` | Motif | All sites in fractional coordinates with bonds (including cross-cell bonds) |

### Node Data

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCifData {
    pub file_name: Option<String>,
    pub block_name: Option<String>,  // default: None (use first block)
    pub use_cif_bonds: bool,         // default: true
    pub infer_bonds: bool,           // default: true
    pub bond_tolerance: f64,         // default: 1.15

    #[serde(skip)]
    pub cached_result: Option<CifImportResult>,
}
```

The `CifImportResult` is a transient cache (not serialized) holding the parsed and expanded
data so re-evaluation doesn't re-parse the file:

```rust
#[derive(Debug, Clone)]
pub struct CifImportResult {
    pub unit_cell: UnitCellStruct,
    pub atomic_structure: AtomicStructure,
    pub motif: Motif,
}
```

### Evaluation Logic

1. Resolve the file path (same logic as `import_xyz` — relative path support).
2. Parse the CIF file → `CifData` (raw parsed data).
3. Extract unit cell parameters → `UnitCellStruct`.
4. Extract asymmetric unit atoms and symmetry operations.
5. Apply symmetry expansion → list of `(element, fract_x, fract_y, fract_z)` for the full
   conventional cell.
6. Check for explicit bond data (`_geom_bond_*` loop).
7. Determine bond source:

   | `use_cif_bonds` | CIF has bonds? | `infer_bonds` | Result |
   |---|---|---|---|
   | true | yes | (ignored) | Use CIF bonds |
   | true | no | true | Infer bonds |
   | true | no | false | No bonds |
   | false | — | true | Infer bonds |
   | false | — | false | No bonds |
8. Build the `Motif` from fractional coordinates with bonds from step 7.
9. Build the `AtomicStructure` by converting fractional → Cartesian using the unit cell.
   Apply the same bond source as step 7.
10. Cache the result; return the requested output pin.

### Persistence

Follow the same pattern as `import_xyz`:

- **Saver:** Convert absolute file paths to relative paths before serialization.
- **Loader:** Resolve file path back to absolute, pre-load and parse the CIF file.
- Only `file_name`, `block_name`, `use_cif_bonds`, `infer_bonds`, and `bond_tolerance`
  are serialized — the parsed data is reconstructed from the file on load.

---

## Refactoring: Parameterized Bond Tolerance

### Current State

`auto_create_bonds()` in `atomic_structure_utils.rs` uses a hardcoded constant:

```rust
const BOND_DISTANCE_MULTIPLIER: f64 = 1.15;
```

### Change

Add a new function alongside the existing one:

```rust
/// Auto-create bonds with a custom covalent radius multiplier.
pub fn auto_create_bonds_with_tolerance(
    structure: &mut AtomicStructure,
    tolerance_multiplier: f64,
) {
    // Same algorithm as auto_create_bonds, using tolerance_multiplier
    // instead of BOND_DISTANCE_MULTIPLIER.
}

/// Auto-create bonds using the default 1.15x multiplier.
pub fn auto_create_bonds(structure: &mut AtomicStructure) {
    auto_create_bonds_with_tolerance(structure, BOND_DISTANCE_MULTIPLIER);
}
```

The existing `auto_create_bonds()` becomes a thin wrapper, so all current call sites
(including `import_xyz` and `load_xyz`) continue to work unchanged. The new
`auto_create_bonds_with_tolerance()` is used by `import_cif` and can later be exposed
as a parameter on `import_xyz` too.

---

## Bond Inference for the Motif Output

The Atomic output uses `auto_create_bonds_with_tolerance()` on Cartesian coordinates, same as
`import_xyz`. The Motif output requires a different approach because motif bonds use
`SiteSpecifier` with relative cell offsets.

### Algorithm: Motif Bond Inference

To detect bonds including cross-cell bonds:

1. For each pair of sites (i, j) in the motif, and for each neighboring cell offset
   `(dx, dy, dz)` where dx, dy, dz are in {-1, 0, +1}:
   - Compute the Cartesian distance between site i at (fx, fy, fz) and site j at
     (fx' + dx, fy' + dy, fz' + dz), using the unit cell basis vectors.
   - If distance <= (covalent_radius_i + covalent_radius_j) * bond_tolerance, create a
     `MotifBond` with appropriate `SiteSpecifier` cell offsets.
2. Avoid duplicate bonds: a bond from site i in cell (0,0,0) to site j in cell (1,0,0) is
   the same as site j in cell (0,0,0) to site i in cell (-1,0,0). Use canonical ordering.
3. Avoid self-bonds: skip when i == j and offset == (0,0,0).

This produces the same bond topology as the Cartesian auto-bonding but expressed in the
motif's fractional coordinate system with proper cross-cell references.

This algorithm should live in the `crystolecule` module (e.g., in `motif.rs` or a new
`motif_bond_inference.rs`) since it operates on `Motif` + `UnitCellStruct` and is reusable
beyond CIF import.

---

## CIF Parsing: Module Structure in `crystolecule`

CIF parsing belongs in the `crystolecule` module since it produces crystallographic data types
(`UnitCellStruct`, `Motif`, `AtomicStructure`). The parsing code should be modular:

```
crystolecule/
└── io/
    ├── cif/
    │   ├── mod.rs              # Public API: load_cif() function
    │   ├── parser.rs           # CIF text format parser (data blocks, tags, loops)
    │   ├── structure.rs        # Extract crystallographic data from parsed CIF
    │   └── symmetry.rs         # Symmetry operation parsing and expansion
    ├── xyz_loader.rs           # (existing)
    ├── xyz_saver.rs            # (existing)
    └── mol_exporter.rs         # (existing)
```

### `parser.rs` — CIF Text Format Parser

Parses the raw CIF text into a structured representation. This is purely syntactic — it
knows about data blocks, tag-value pairs, loops, quoted strings, and semicolon-delimited
text fields, but has no knowledge of crystallography.

```rust
/// A parsed CIF file containing one or more data blocks.
pub struct CifDocument {
    pub data_blocks: Vec<CifDataBlock>,
}

/// A single data block (e.g., `data_diamond`).
pub struct CifDataBlock {
    pub name: String,
    pub tags: HashMap<String, String>,       // Single tag-value pairs
    pub loops: Vec<CifLoop>,                 // Tabular data sections
}

/// A loop_ section with column headers and rows.
pub struct CifLoop {
    pub columns: Vec<String>,                // Tag names (e.g., "_atom_site_fract_x")
    pub rows: Vec<Vec<String>>,              // Row values (same length as columns)
}

/// Parse a CIF file from a string.
pub fn parse_cif(input: &str) -> Result<CifDocument, CifParseError>;
```

Parsing rules:
- Tag names are case-insensitive (normalize to lowercase).
- Strip numeric uncertainties from values: `5.4307(2)` → `5.4307`.
- Handle single-quoted, double-quoted, and semicolon-delimited strings.
- Handle `.` and `?` as special null values.

### `structure.rs` — Crystallographic Data Extraction

Extracts crystallographic information from a parsed `CifDataBlock`. This layer understands
which CIF tags correspond to unit cell parameters, atom sites, etc.

```rust
/// Extracted crystallographic data from a CIF data block.
pub struct CifCrystalData {
    pub unit_cell: UnitCellStruct,
    pub asymmetric_atoms: Vec<CifAtomSite>,
    pub symmetry_operations: Vec<SymmetryOperation>,
    pub bonds: Vec<CifBond>,         // From _geom_bond_* if present, otherwise empty
}

pub struct CifAtomSite {
    pub label: String,
    pub element: String,             // Parsed from _atom_site_type_symbol
    pub fract: DVec3,                // Fractional coordinates
    pub occupancy: f64,              // Default 1.0 if absent
}

pub struct CifBond {
    pub atom_label_1: String,
    pub atom_label_2: String,
    pub distance: f64,               // Bond length in Angstroms
    pub symmetry_code_1: Option<String>,  // e.g., "." or "2_655"
    pub symmetry_code_2: Option<String>,
    pub bond_order: i32,             // 1=single (default), 2=double, 3=triple
}

/// Extract crystal data from a CIF data block.
/// Handles both old (_symmetry_*) and new (_space_group_*) tag names.
/// Extracts _geom_bond_* data if present, with bond order from
/// _ccdc_geom_bond_type or _chemical_conn_bond_type when available.
pub fn extract_crystal_data(block: &CifDataBlock) -> Result<CifCrystalData, CifError>;
```

Tag lookup strategy:
- Try the newer tag name first, fall back to the older equivalent.
- For unit cell parameters: all 6 are required; error if any are missing.
- For symmetry operations: if explicit operations are listed, use them; otherwise, look up
  by space group number (see symmetry.rs).
- For atom sites: require at minimum `_atom_site_type_symbol` (or `_atom_site_label` to
  infer element) and fractional coordinates.

### `symmetry.rs` — Symmetry Operations

Handles parsing symmetry operation strings and applying them to expand the asymmetric unit
into the full conventional cell.

```rust
/// A symmetry operation parsed from a string like "x,y,z" or "-x+1/2,-y,z+1/2".
pub struct SymmetryOperation {
    // Each component (x, y, z output) is a linear combination of (x, y, z input) + translation.
    // Represented as a 3x4 matrix: [rotation 3x3 | translation 3x1]
    pub transform: DMat4,  // or a simpler [f64; 12] representation
}

/// Parse a symmetry operation string (e.g., "-x+1/2, y, -z+1/2").
pub fn parse_symmetry_operation(s: &str) -> Result<SymmetryOperation, CifError>;

/// Apply all symmetry operations to the asymmetric unit, wrap into [0,1),
/// and deduplicate positions within a tolerance.
pub fn expand_asymmetric_unit(
    atoms: &[CifAtomSite],
    operations: &[SymmetryOperation],
    tolerance: f64,              // Typically ~0.01 in fractional coordinates
) -> Vec<CifAtomSite>;
```

#### Symmetry Operation String Syntax

Symmetry operations use Jones' faithful notation (coordinate triplets). Each string contains
3 comma-separated expressions, one per output coordinate. Each expression is a linear
combination of input coordinates with an optional translation constant.

**Grammar (informal):**

```
triplet    := expr ',' expr ',' expr
expr       := term (('+' | '-') term)*
term       := [sign] variable
            | [sign] fraction
            | [sign] fraction '*' variable
            | [sign] variable '/' integer
variable   := 'x' | 'y' | 'z'     (case-insensitive; also accept 'a','b','c')
fraction   := integer '/' integer   (e.g., 1/2, 3/4)
            | decimal               (e.g., 0.5, 0.333)
            | integer               (e.g., 1)
```

**Accepted syntax variants** (all of these appear in real CIF files):

| Variant | Example | Notes |
|---------|---------|-------|
| Vulgar fractions | `-x+1/2` | Most common |
| Decimal fractions | `-x+0.5` | Equivalent to above |
| Translation before variable | `1/2+x` | Same as `x+1/2` |
| Uppercase variables | `X, Y, Z` | Normalize to lowercase |
| Alternate variable names | `a, b, c` | Treat as x, y, z respectively |
| Underscore as whitespace | `x,_y,_z` | Strip underscores |
| Explicit coefficient | `2*x` or `x/3` | Rare; standard ops use only -1, 0, +1 |
| Spaces around operators | `-x + 1/2, y, -z + 1/2` | Strip all whitespace |

**Implementation notes:**
- Strip whitespace and underscores before parsing each component.
- Normalize variable names to lowercase; map `a`→`x`, `b`→`y`, `c`→`z`.
- In standard crystallographic operations, coefficients on variables are always -1, 0, or
  +1. Supporting general coefficients (`2*x`, `x/3`) adds minimal complexity and handles
  edge cases robustly.
- Translations are typically multiples of 1/12 (common values: 0, 1/6, 1/4, 1/3, 1/2,
  2/3, 3/4, 5/6). Using `f64` arithmetic avoids any need for fixed-point constraints.

#### Parsing to a 3×4 Matrix

Crystallographic symmetry operations are by definition affine transformations on fractional
coordinates. The Jones notation is a direct human-readable encoding of a 3×4 matrix (3×3
rotation + 3×1 translation). There are no products of variables, no nonlinear terms — every
term is either `coefficient × variable` (rotation part) or a `constant` (translation part).

The matrix falls out directly from the parse with no intermediate AST or additional
transformation step. For each of the 3 comma-separated expressions, accumulate one row:

```rust
// Per component (one row of the 3×4 matrix):
let mut row = [0.0_f64; 4]; // [c_x, c_y, c_z, translation]

for term in parsed_terms {
    match term {
        Variable(v, sign) => row[v.index()] += sign,  // x→0, y→1, z→2
        Constant(val)     => row[3] += val,
        CoeffVar(c, v)    => row[v.index()] += c,      // handles 2*x, x/3 etc.
    }
}
```

Three rows → complete `SymmetryOperation`. Applying the operation to a fractional position
`(fx, fy, fz)` is a matrix-vector multiply followed by wrapping into [0, 1) via modulo.

**Examples:**

| String | Row 0 (x') | Row 1 (y') | Row 2 (z') |
|--------|-----------|-----------|-----------|
| `x,y,z` | `[1, 0, 0, 0]` | `[0, 1, 0, 0]` | `[0, 0, 1, 0]` |
| `-x+1/2,-y,z+1/2` | `[-1, 0, 0, 0.5]` | `[0, -1, 0, 0]` | `[0, 0, 1, 0.5]` |
| `1/4+y,1/4-x,3/4+z` | `[0, 1, 0, 0.25]` | `[-1, 0, 0, 0.25]` | `[0, 0, 1, 0.75]` |

- After applying an operation, wrap fractional coordinates into [0, 1) via modulo.

Deduplication:
- After all operations are applied to all atoms, remove duplicates where two atoms have the
  same element and fractional coordinates within a tolerance (e.g., 0.01 in fractional units).
- This accounts for atoms on special positions (e.g., the origin in diamond is invariant
  under many operations).

### `mod.rs` — Public API

The top-level function composes the above:

```rust
/// Result of loading a CIF file.
pub struct CifLoadResult {
    pub unit_cell: UnitCellStruct,
    pub atoms: Vec<ExpandedAtomSite>,   // Full conventional cell, fractional coords
}

pub struct ExpandedAtomSite {
    pub label: String,
    pub atomic_number: i16,
    pub fract: DVec3,
}

/// Load and process a CIF file. Returns unit cell and expanded atom sites.
/// The caller is responsible for converting to AtomicStructure/Motif and
/// for bond inference.
///
/// `block_name`: if Some, selects the data block by name; if None, uses the
/// first block. Returns an error listing available block names if the
/// requested name is not found.
pub fn load_cif(file_path: &str, block_name: Option<&str>) -> Result<CifLoadResult, CifError>;
```

The conversion from `CifLoadResult` to `AtomicStructure` and `Motif` happens in the node
evaluation code, since it depends on node parameters (use_cif_bonds, infer_bonds,
bond_tolerance). This
keeps the CIF parser focused on crystallographic data extraction.

---

## Space Group Lookup Table

When a CIF file does not list explicit symmetry operations but provides only a space group
number or Hermann-Mauguin symbol, we need a lookup table mapping the 230 space groups to
their symmetry operations.

### Approach

- Store symmetry operations as a static lookup table keyed by International Tables number
  (1–230).
- Source: International Tables for Crystallography, Vol. A (the data itself is well-known
  and reproduced in many open-source projects — e.g., GEMMI, spglib, pymatgen).
- The table maps each space group number to a list of symmetry operation strings
  (the same `x,y,z` format found in CIF files).
- This table should live in `symmetry.rs` or a separate `space_groups.rs` within the
  `cif/` module.

### Scope

The space group lookup is not required for an initial implementation — most CIF files from
COD and Materials Project include explicit symmetry operations. The lookup can be added in
a later phase for robustness. If a CIF file has neither explicit operations nor a recognized
space group, the parser should return an error.

---

## Node Implementation: `import_cif.rs`

Location: `rust/src/structure_designer/nodes/import_cif.rs`

Follows the same patterns as `import_xyz.rs`:

### `get_node_type()`

Registers the node with 5 parameters and 3 output pins as specified above.

### `eval()`

```
fn eval(pin_index: i32, ...) -> NetworkResult:
    1. Get file_name from parameter 0 (or use cached file_name).
    2. Get block_name from parameter 1 (default empty = None).
    3. Get use_cif_bonds from parameter 2 (default true).
    4. Get infer_bonds from parameter 3 (default true).
    5. Get bond_tolerance from parameter 4 (default 1.15).
    6. If cached_result is valid (same file, same block, same parameters), use it.
    7. Otherwise:
       a. Resolve file path (relative → absolute, same as import_xyz).
       b. Call load_cif(&resolved_path, block_name) → CifLoadResult.
       c. Build UnitCellStruct from CifLoadResult.unit_cell.
       d. Determine bond source (see Evaluation Logic step 7).
       e. Build Motif:
          - Create Site entries from expanded atom positions (fractional coords).
          - Add bonds from the determined bond source.
       f. Build AtomicStructure:
          - Convert each expanded atom from fractional → Cartesian using unit cell.
          - Add atoms to AtomicStructure.
          - Add bonds from the determined bond source.
       g. Cache the result.
    8. Return the appropriate output based on pin_index:
       - 0 → NetworkResult::UnitCell(unit_cell)
       - 1 → NetworkResult::Atomic(atomic_structure)
       - 2 → NetworkResult::Motif(motif)
```

### Persistence (loader/saver)

Same pattern as `import_xyz`:
- Saver: convert absolute path → relative path.
- Loader: resolve relative → absolute, pre-load and parse the CIF.

### Registration

Add to `node_type_registry.rs` alongside `import_xyz`.

---

## Implementation Phases

### Phase 0: Test Fixtures

Download CIF files from COD into `rust/tests/fixtures/cif/` and verify they contain the
expected data (unit cell parameters, symmetry operations, atom sites). These fixtures are
used by tests in all subsequent phases.

### Phase 1: CIF Parser (`crystolecule/io/cif/parser.rs`)

Implement the text format parser: data blocks, tag-value pairs, loops, quoted strings,
semicolon text fields, comment stripping, uncertainty stripping.

**Tests:** Unit tests on CIF syntax using small inline CIF snippets (not full files). Verify
correct extraction of tags and loop data. Test edge cases: multi-line strings, quoted values
with spaces, numeric uncertainties (e.g., `5.4307(2)` → `5.4307`), multiple data blocks,
comment handling, `.` and `?` null values.

### Phase 2: Symmetry Operations (`crystolecule/io/cif/symmetry.rs`)

Implement symmetry operation string parsing and asymmetric unit expansion.

**Tests:** Unit tests parsing individual symmetry operation strings (e.g., `x,y,z`,
`-x+1/2,-y,z+1/2`, `1/4+y,1/4-x,3/4+z`). Integration tests expanding diamond asymmetric
unit (2 atoms → 8 atoms) and NaCl (2 atoms → 8 atoms). Verify deduplication of atoms on
special positions. Test fractional coordinate wrapping into [0,1).

### Phase 3: Crystal Data Extraction (`crystolecule/io/cif/structure.rs`)

Implement extraction of unit cell, atom sites, symmetry operations, and bond data from
parsed CIF data. Handle old/new tag name variants. Parse `_geom_bond_*` loops and symmetry
codes (`S_XYZ` format) when present.

**Tests:** Extract from fixture CIF files downloaded from COD. Verify unit cell parameters
match known values. Verify atom site counts and elements. Test old/new tag name fallback
(e.g., `_symmetry_equiv_pos_as_xyz` vs `_space_group_symop_operation_xyz`). Test bond
extraction from `with_bonds.cif` fixture including symmetry code parsing.

### Phase 4: `load_cif()` Integration (`crystolecule/io/cif/mod.rs`)

Wire together parser → structure extraction → symmetry expansion into the public
`load_cif()` function.

**Tests:** End-to-end: load a diamond CIF, verify 8 expanded atom sites at expected
fractional positions with correct elements and correct unit cell parameters. Load NaCl CIF,
verify 8 atoms. Load a hexagonal structure to test non-orthogonal unit cells. These tests
verify `load_cif()` output only (unit cell + expanded atoms) — motif construction and bond
inference are tested in Phase 6 and Phase 7 respectively.

### Phase 5: Bond Tolerance Refactor (`crystolecule/atomic_structure_utils.rs`)

Extract `auto_create_bonds_with_tolerance()`. Make `auto_create_bonds()` a wrapper.

**Tests:** All existing auto-bonding tests must still pass (no behavioral change for default
multiplier). New tests with custom tolerance multipliers: verify that a higher multiplier
creates more bonds and a lower multiplier creates fewer.

### Phase 6: Motif Bond Inference (`crystolecule/`)

Implement bond inference on motif fractional coordinates with cross-cell bond detection.

**Tests:** Infer bonds for diamond motif → serialize to motif text → verify 16 bonds
matching `DEFAULT_ZINCBLENDE_MOTIF` (same sites, same cross-cell bond offsets). This is the
critical correctness test — the AI assistant reviews the serialized motif text against the
known-good diamond motif, and once verified, the text becomes a regression snapshot. Also
test NaCl bonds and cross-cell bond canonicalization (no duplicate bonds).

### Phase 7: `import_cif` Node (`structure_designer/nodes/import_cif.rs`)

Implement the node: data struct, eval, persistence, registration. Multi-output pins.

**Tests:** Node evaluation returns correct types on each pin. File path resolution (relative
and absolute). Snapshot tests: load fixture CIF → serialize each output (motif text, XYZ,
unit cell parameters) → compare against verified snapshots. Round-trip serialization of node
data (save + load preserves file_name, block_name, use_cif_bonds, infer_bonds,
bond_tolerance).

### Phase 8 (Later): Space Group Lookup Table

Add a lookup table for the 230 space groups. Use it as fallback when explicit symmetry
operations are not listed in the CIF.

---

## Test Strategy

### Approach: Motif Text Serialization as Ground Truth

The primary verification strategy for end-to-end CIF import is:

1. Load a CIF fixture file → parse → symmetry expand → infer bonds → build Motif.
2. Serialize the resulting Motif to the text format (the same `SITE`/`BOND`/`PARAM` syntax
   used in `DEFAULT_ZINCBLENDE_MOTIF_TEXT`).
3. The AI assistant reviews the text output against known crystallographic data to confirm
   correctness (e.g., diamond should produce 8 sites at expected fractional positions with
   16 tetrahedral bonds matching our existing default motif).
4. Once verified correct, the motif text becomes the expected output in a regression test
   (string comparison or `insta` snapshot).

This works well because:
- The motif text format is human-readable and directly comparable to known-good motifs.
- Errors are easy to spot visually (wrong coords, missing bonds, wrong cell offsets).
- It naturally documents what each CIF file should produce.
- For diamond specifically, the output can be compared against `DEFAULT_ZINCBLENDE_MOTIF_TEXT`.

For the Atomic output, serialize to XYZ format (using the existing `xyz_saver`) and snapshot.
For UnitCell, compare the 6 crystallographic parameters directly.

### Motif Serialization Helper

A `motif_to_text()` function is needed (inverse of `parse_motif()`). This may already be
partially available — check if the motif serializer in the text format system can be reused.
If not, a simple serializer that outputs `SITE` and `BOND` lines in a canonical order
(sites sorted by fractional coordinates, bonds sorted by site indices and cell offsets)
ensures deterministic output for snapshot comparison.

### Test Fixtures

Include a small set of CIF files in `rust/tests/fixtures/cif/` for testing:

- `diamond.cif` — cubic, Fd-3m, 2 asymmetric atoms → 8 in unit cell. Validates against
  our existing `DEFAULT_ZINCBLENDE_MOTIF`. The gold standard test case.
- `nacl.cif` — cubic, Fm-3m, 2 asymmetric atoms → 8 in unit cell. Simple ionic structure,
  tests a different space group.
- `hexagonal.cif` — a hexagonal structure (e.g., lonsdaleite or wurtzite) to test
  non-orthogonal unit cell conversion and non-90° angles.
- `multi_block.cif` — a CIF file with multiple data blocks to test block selection.
- `with_bonds.cif` — an organic CIF from COD that includes `_geom_bond_*` data, to test
  explicit bond parsing.

These can be downloaded from the Crystallography Open Database (COD).

See individual phase descriptions above for specific test details.

---

## Design Decisions

### Why 3 outputs instead of just Motif + UnitCell?

The Atomic output (pin 1) serves two purposes:
1. It can be fed into `motif_edit` for interactive editing (motif_edit takes Atomic + UnitCell
   as inputs, not Motif).
2. It can be used directly for visualization or further manipulation without going through
   the lattice fill pipeline.

### Why bond inference in the node rather than a separate node?

Bond handling in the import node is a convenience for the common case — most users will want
bonds immediately after import. Two separate parameters give fine-grained control:
`use_cif_bonds` prefers explicit bond data from the CIF file (more reliable, experimentally
determined), while `infer_bonds` falls back to distance-based inference. A separate,
standalone bond inference node is also planned for use cases where bonds need to be added or
recalculated independently (e.g., after transforming an atomic structure, or on structures
from sources other than CIF). Setting both flags to false disables all bond creation when the
user prefers to use the standalone node instead.

### Bond information from CIF files

CIF files can contain bond information via `_geom_bond_*` loop tags. Research shows this is
not insignificant — roughly half of organic/organometallic CIF files on COD include this data,
since it is routinely generated by refinement software (e.g., SHELXL). Inorganic/mineral CIFs
almost never include it.

The standard `_geom_bond_*` tags provide:
- `_geom_bond_atom_site_label_1` / `_geom_bond_atom_site_label_2` — atom pair (references
  `_atom_site_label`)
- `_geom_bond_distance` — bond length in Angstroms (with uncertainty)
- `_geom_bond_site_symmetry_1` / `_geom_bond_site_symmetry_2` — symmetry operation codes

**Limitation:** Standard CIF does not include bond order. Only the CCDC's proprietary
extension `_ccdc_geom_bond_type` provides order (S/D/T/A). The `_chemical_conn_bond_type`
tag (part of the IUCr core dictionary, supports `sing`/`doub`/`trip`/`arom`/etc.) exists
but is almost never populated in deposited files.

**Implementation:** When `use_cif_bonds` is true and `_geom_bond_*` data is present, parse
it and use the explicit connectivity — skipping distance-based inference entirely. Bonds from
CIF data are more reliable than distance inference since they represent experimentally
determined connectivity. Bond order defaults to single (1) unless `_ccdc_geom_bond_type` or
`_chemical_conn_bond_type` is available. When `use_cif_bonds` is false or `_geom_bond_*` data
is absent, fall back to distance-based inference if `infer_bonds` is true (using the
`bond_tolerance` multiplier).

This is easy to implement since `_geom_bond_*` is a standard CIF loop — the existing
parser handles it. The atom labels are cross-referenced against `_atom_site_label` to
resolve connectivity.

**Cross-cell bonds:** The symmetry operation codes (`_geom_bond_site_symmetry_2`) use the
format `S_XYZ` where `S` is a symmetry operation index and `X`, `Y`, `Z` encode cell
translations as `digit + 5` (so `5` = same cell, `4` = -1, `6` = +1). For example,
`2_655` means "apply symmetry op #2, then translate by (+1, 0, 0)". A `.` or `1_555`
means the same atom in the same cell. This maps directly to our
`SiteSpecifier.relative_cell: IVec3` in the motif bond system, so CIF bond data can
fully represent cross-cell bonds.

### Why modular parsing?

The CIF text format parser (`parser.rs`) is a generic STAR/CIF parser that could be reused
for other CIF-based formats (e.g., mmCIF for proteins). Separating symmetry operations
(`symmetry.rs`) from crystallographic data extraction (`structure.rs`) keeps each file focused
and independently testable. The symmetry operation parser in particular has well-defined
inputs and outputs that benefit from isolated testing.

### Occupancy < 1.0

For the initial implementation, atoms with occupancy < 1.0 (disordered sites) will be
included with a warning. Partial occupancy is not meaningful for atomically precise
manufacturing, so the user will need to decide how to handle these manually (e.g., by editing
the motif to select one configuration). A future enhancement could add a parameter to filter
by minimum occupancy.
