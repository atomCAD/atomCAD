# Design: `export_atoms` node — multi-format atom export (issue #353)

**Issue:** https://github.com/atomCAD/atomCAD/issues/353 — "add export_mol node.
There is functionality to export .xyz and .mol files but there is only a node
to export to xyz files ATM."

## Motivation

`save_mol_v3000` (`crystolecule/io/mol_exporter.rs`) is complete and tested,
but the only node-network export path is `export_xyz`. Exporting atoms is one
operation with a format axis, not N operations — the issue itself anticipates
more formats (`.pdb`), and per-format nodes scale linearly in duplicated code
(path resolution, relative-path saver/loader, subtitle, metadata sidecar,
property API, Flutter editor) and add-node-palette clutter.

We rename `export_xyz` → **`export_atoms`** and derive the output format from
the **file extension** of the resolved path.

## Why this shape (alternatives considered)

- **Separate `export_mol` node** — rejected. Nearly all of `export_xyz.rs`
  would be duplicated, plus a cloned property API and Flutter editor; a future
  `.pdb` means a third copy. Switching format on an existing graph means
  deleting the node and rewiring. The metadata-sidecar feature either gets
  duplicated or stays xyz-only, so the nodes drift apart.
- **`format` property (dropdown) on the node** — rejected. Redundant with the
  extension and can silently disagree with it (`format: mol`,
  `file_name: out.xyz`). The extension is already the user's mental model:
  the existing *File → Export visible* flow dispatches on extension
  (`structure_designer.rs::export_visible_atomic_structures`), so one rule
  serves both paths.
- **Extension-driven format (chosen)** — the node's shape is unchanged: same
  three pins (`molecule`, `file_name`, `metadata`), same stored `file_name`
  property. A wired `file_name` works naturally; an unrecognized extension
  surfaces as a localized `NetworkResult::Error` at Execute time (consistent
  with the effect-node error model). The `.cnnd` migration reduces to a
  mechanical rename of the two name-carrying JSON keys (see Migration).
  Adding `.pdb` later = one enum arm + one saver.

## Naming

`export_atoms` — "atoms" matches the domain vocabulary (`HasAtoms` input pin,
"atomic structure"), stays format-neutral, and sorts next to a hypothetical
future `export_*` family. Category stays `AtomicStructure`. The node remains
an **effect node**: output `Unit`, gated by the central skip rule, fired via
right-click → Execute (`doc/design_node_execution.md`).

## Format dispatch: single source of truth in Rust

The supported-format set is consulted in five places: the node's eval
dispatch, the node subtitle warning, the *Export visible* menu action, the
Flutter Browse dialog, and the Flutter format indicator. To prevent drift,
define it **once**, in a new `crystolecule/io/atom_export.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomExportFormat {
    Xyz,
    Mol,
}

#[derive(Debug, Error)]
pub enum AtomExportError {
    #[error(transparent)]
    Xyz(#[from] XyzSaveError),
    #[error(transparent)]
    Mol(#[from] MolSaveError),
}

impl AtomExportFormat {
    pub const ALL: &[AtomExportFormat] = &[AtomExportFormat::Xyz, AtomExportFormat::Mol];

    /// Case-insensitive match on the path's final extension.
    /// None for a missing or unrecognized extension.
    pub fn from_path(path: &str) -> Option<Self>;

    pub fn extension(&self) -> &'static str;       // "xyz" / "mol"
    pub fn label(&self) -> &'static str;           // "XYZ" / "MOL (V3000)"
    pub fn description(&self) -> &'static str;     // "Atomic coordinates only" /
                                                   // "Molecular structure with bond information"
    /// Human-readable list for error messages / UI: ".xyz, .mol"
    pub fn supported_extensions_display() -> String;

    pub fn save(&self, structure: &AtomicStructure, path: &str) -> Result<(), AtomExportError>;
}
```

This lives in `crystolecule` (beside the savers it dispatches to), which keeps
the module-dependency DAG intact — both `structure_designer` consumers sit
above it. `structure_designer.rs::export_visible_atomic_structures` switches
to `AtomExportFormat::from_path(...)` + `save(...)`, deleting its hand-rolled
extension matching (behavior unchanged, including the error text listing
supported extensions).

## The node (`nodes/export_atoms.rs`)

Rename of `nodes/export_xyz.rs`: module `export_xyz` → `export_atoms`, struct
`ExportXYZData` → `ExportAtomsData`, `export_xyz_data_saver/loader` →
`export_atoms_data_saver/loader`. Stored data is unchanged —
`{ file_name: String }` — so the per-node JSON `data` payload needs **no**
migration (the `data_type` *tag* beside it does; see Migration).

`eval` changes only at the save step:

1. Existing checks unchanged: required `molecule`, `file_name`
   wired-overrides-stored, empty-name error, design-dir-relative path
   resolution.
2. `let format = AtomExportFormat::from_path(&resolved_path)` — `None` →
   `NetworkResult::Error("Unsupported export format for '<file_name>'. \
   Supported extensions: .xyz, .mol")` (list rendered from
   `supported_extensions_display()`, never hardcoded).
3. `format.save(&atomic_structure, &resolved_path)` — error wrapped as today
   (`"Failed to save <LABEL> file '<name>': <err>"`).
4. Metadata sidecar (below) written for **every** format, not just xyz.

`get_subtitle` keeps the existing pattern (wired `file_name` → `None`; empty →
`"(no file name)"`) and adds the eager-feedback arm for the Execute-deferred
format check, mirroring the rationale documented in the current file:

- recognized extension → `Some(file_name)` (unchanged),
- unrecognized/missing extension → `Some(format!("{} (unsupported format)", file_name))`.

Node `description` updated: "Exports the atomic structure on its `molecule`
input to a file; the format is chosen by the file extension (.xyz, .mol). …"

### Metadata sidecar generalization

`write_generation_parameters_sidecar` already writes to the format-agnostic
path `{path}.params.json`; only the JSON keys are xyz-specific. Generalize:

- `"xyz_file"` → `"file"`, `"xyz_blake3"` → `"blake3"`,
- `"version": 1` → `2` (the `"format": "atomcad-generation-parameters"` tag
  is unchanged; version 2 simply renames the two keys),
- doc-comment and node/editor descriptions say "the exported file" instead of
  "the XYZ file".

The sidecar feature thereby works for `.mol` (and future formats) for free.

## `.cnnd` migration: v6 → v7

`SERIALIZATION_VERSION` 6 → 7. New `serialization/migrate_v6_to_v7.rs`,
chained in `node_networks_serialization.rs` as
`if version < 7 { migrate_v6_to_v7(&mut root_value)?; }` after the v5→v6 call.

The pass is a mechanical rename, but it must rewrite **two** keys, not one.
A serialized built-in node carries its type name twice: in `node_type_name`
and in the polymorphic data tag `data_type` (`node_to_serializable` writes
`data_type = node_type_name` for built-ins) — and it is **`data_type`** that
`serializable_to_node` uses to dispatch the node-data loader. Rewriting only
`node_type_name` would leave `data_type: "export_xyz"` matching no built-in
while the new `node_type_name` *is* one, so the loader's fallback would
construct `NoData {}` — silently dropping the stored `file_name` and breaking
the property API's `ExportAtomsData` downcast. So the pass recursively walks
the whole JSON value and rewrites every object entry whose key is
`"node_type_name"` **or** `"data_type"` and whose string value is
`"export_xyz"` to `"export_atoms"`. Rewriting `data_type` tree-wide is
unambiguous: the key's other use (serialized `DataType`s on parameters/pins)
holds enum encodings like `"Float"` / `{"Record": …}`, never a node type
name. A generic whole-tree walk (rather than a network→nodes→zone recursion
like v5→v6) is deliberate — it automatically covers nodes at every zone-body
depth. No wire, pin, or data *payload* changes (`{ file_name }` is
untouched).

Safety of the blanket rename: every `node_type_name: "export_xyz"` occurrence
in a saved file refers to the built-in node. Today `name_is_taken` rejects
user networks named after built-ins, but that check only dates from record
types Phase 2 — before it, `add_node_network_with_name` checked only the
user-network map, so a very old file could in principle contain a user
network *named* `export_xyz`. That doesn't threaten the rename, for two
reasons. First, such a network was always **shadowed** by the built-in: both
node-type resolution and node-data-loader dispatch consult
`built_in_node_types` first, so an instance node created against it was the
built-in node in every respect (including carrying `ExportXYZData` — the
built-in saver's downcast would reject anything else at save time). Every
saved `"export_xyz"` reference is therefore genuinely the built-in node, and
renaming it to the renamed built-in is behavior-preserving. Second, the walk
rewrites only the two *reference* keys — a network definition's own name
lives under `"name"`, so a hypothetical user network named `export_xyz` is
left untouched (it merely becomes un-shadowed once the built-in vacates the
name). Follow the established migration-module conventions (test-only
invocation counter, `MigrationError` reuse, module doc header), modeled on
`migrate_v5_to_v6.rs`.

Existing old-version test fixtures containing `export_xyz` (e.g.
`tests/fixtures/rename_wire_loss/before.cnnd`) are **not** edited — the loader
chain migrates them, and they double as regression coverage for this pass.

## Text format

The text format resolves node type names through the registry in both
directions, so `export_atoms` works with **no parser/serializer change**.
Decision: **no legacy alias** for `export_xyz` in the text format — a stale
snippet fails loudly with "unknown node type", which is preferable to two
names for one node drifting through docs and AI-generated text. (`.cnnd`
files are covered by the migration; text is ephemeral editing input.)

## API + Flutter

### Rust API (mechanical rename + one new getter)

- `APIExportXYZData` → `APIExportAtomsData`, `get/set_export_xyz_data` →
  `get/set_export_atoms_data` (both keep `scope_path`, `#[frb(sync)]`, same
  undo/refresh pattern).
- New `#[frb(sync)] get_atom_export_formats() -> Vec<APIAtomExportFormat>`
  where `APIAtomExportFormat { extension: String, label: String, description: String }`,
  a thin projection of `AtomExportFormat::ALL`. This is what makes the UI
  self-describing: when `.pdb` lands, the file dialog filter, the format
  chooser, the format indicator, and the info card all update with zero
  Flutter edits.
- `flutter_rust_bridge_codegen generate` after the API edits.

### UX: signaling the available formats

Discovery cannot ride on the OS save dialog's file-type dropdown. Git
archaeology (this constraint is load-bearing, so it's recorded here):

- Commit `5bfd5f6a` ("choose extension for export visible") replaced exactly
  the naive design — one `saveFile` with `allowedExtensions: ['mol', 'xyz']` —
  with a format pre-dialog + single-extension `saveFile`. The `file_picker`
  package has no named filter groups: multiple extensions collapse into one
  combined filter, so the dialog offers no real format choice, and a bare
  typed name (`structure`) has no unambiguous extension to append.
- Commit `b63c8a32` ("save as cnnd on macOS") established the companion
  convention: **extension-less default `fileName`** (macOS auto-appends the
  allowed extension; a default name already carrying it misbehaves) plus
  **manual post-append** when the returned path lacks the extension (Windows
  historically doesn't append). An older comment also recorded
  "allowedExtensions doesn't work properly on Windows".

So the node editor signals formats through three channels, none of which is
the OS dialog's filter list:

1. **Reactive format indicator (primary).** A row under the path field,
   derived from the current `file_name`'s extension against
   `get_atom_export_formats()`:
   - `structure.xyz` → "Format: XYZ"
   - `part.mol` → "Format: MOL (V3000)"
   - unrecognized/missing extension → error-colored "Unrecognized extension —
     supported: .xyz, .mol"
   - `file_name` **pin wired** (detected via the standard
     `nodeNetworkView.wires` walk, pin index 1) → neutral "Format is chosen by
     the wired file name's extension at Execute".
   This converts the Execute-deferred eval error into while-typing feedback,
   and covers users who never touch Browse.
2. **Format chooser before Browse (secondary).** The Browse button first shows
   the same two-`ListTile` format dialog the *Export visible* menu uses (per
   `5bfd5f6a`), then `saveFile` with a **single** extension and extension-less
   default name, then appends the extension when the returned path doesn't
   already end with it (the `endsWith` check from `_exportVisible`, **not**
   `_saveDesignAs`'s `contains('.')` — a directory with a dot in its name
   defeats the latter). The chosen format is *not stored on the node* — it
   only parametrizes the picker; the extension in the resulting path stays the
   single source of truth. The per-format `description` strings give this
   dialog its explanatory value.
3. **Node subtitle in the graph (tertiary).** The `get_subtitle` unsupported-
   format arm above — visible without opening the property panel.

The static info card stays, reworded ("The file is written when the node is
Executed; the format is chosen by the file extension: …" — extension list
rendered from the API), plus the generalized sidecar sentence.

### Flutter changes

- `export_xyz_editor.dart` → `export_atoms_editor.dart` (header
  "Export Atoms", elements per above); router case in `node_data_widget.dart`
  `'export_xyz'` → `'export_atoms'`; model method `setExportXyzData` →
  `setExportAtomsData` (still forwards `propertyEditorScopeChain`).
- **Shared format-chooser helper**: extract the dialog from
  `_exportVisible` into `lib/common/export_format_dialog.dart` —
  `Future<String?> showAtomExportFormatDialog(BuildContext)` returning the
  extension, list built from `get_atom_export_formats()` (it already uses
  `showDraggableAlertDialog`, satisfying the draggable-dialog rule).
  `_exportVisible` and the editor's Browse both call it.
- Drive-by fix: the current editor's Browse uses `fileName: 'structure.xyz'`
  *with* extension, predating the `b63c8a32` convention — the rework adopts
  extension-less default + post-append like the other two call sites.

## Ripple inventory (mechanical rename sweep)

- **Rust src:** `nodes/mod.rs`, `node_type_registry.rs` registration, the
  `export_xyz` mentions in `data_type.rs` / `structure_designer.rs` /
  evaluator comments, API file imports.
- **Rust tests:** `abstract_output_type_test.rs`, `text_properties_test.rs`,
  `execute_node_test.rs`, `promote_to_parameter_test.rs`,
  `expr_template_literal_test.rs` (fixtures stay untouched, see Migration).
- **Snapshots:** the node-snapshot suite loads fixture `.cnnd` files; runs
  after the rename will show `export_atoms` — `cargo insta review` for
  intentional changes.
- **Living docs:** `doc/reference_guide/nodes/atomic.md` (+ the other
  reference-guide mentions), `doc/node_network_text_format.md`, and the
  AGENTS.md files that name `export_xyz` (`rust/src/structure_designer/`,
  `.../nodes/`, `.../evaluator/`, `lib/structure_designer/`). Historical
  `design_*.md` docs are records of past decisions and are **not** edited.

## Phased plan

Each phase compiles and tests green on its own. Windows build convention:
`cargo test -j 4`, never two cargo commands concurrently.

### Phase 1 — `AtomExportFormat` dispatch module (pure refactor)

- New `crystolecule/io/atom_export.rs` per the shape above; `pub mod` in
  `io/mod.rs`.
- `export_visible_atomic_structures` switches to it; hand-rolled extension
  matching deleted. No behavior change.
- **Tests** (`rust/tests/crystolecule/io/atom_export_test.rs`, registered in
  `rust/tests/crystolecule.rs`): `from_path` case-insensitivity, missing
  extension, unknown extension, dotted directory names
  (`C:\my.dir\file` → `None`, `C:\my.dir\file.xyz` → `Xyz`); `save` roundtrip
  smoke per format (xyz reloadable via `xyz_loader`; mol prefix/`V3000`
  content check, mirroring `mol_exporter_test.rs`).
- **Deliverable:** one format registry; menu export runs through it.

### Phase 2 — rename + multi-format node + migration

The rename, the migration, and the version bump land **together** (fixtures
with `export_xyz` only load green once the migration exists).

- `nodes/export_xyz.rs` → `nodes/export_atoms.rs` with the eval dispatch,
  subtitle arm, sidecar generalization, and description updates above;
  registration + comment sweep per the ripple inventory.
- `migrate_v6_to_v7.rs` + `SERIALIZATION_VERSION = 7` + loader chain slot.
- Mechanical API + Flutter rename (`APIExportAtomsData`,
  `get/set_export_atoms_data`, FRB regen, editor file/router/model-method
  rename — old UX for now) so `cargo test` **and** `flutter analyze` are green
  at the phase boundary.
- **Tests:**
  - Update the five renamed-node test files; `cargo insta review`.
  - New `rust/tests/integration/export_atoms_migration_test.rs` (registered
    in `integration.rs`, modeled on the existing migration tests): a v6
    fixture containing an `export_xyz` — one top-level with wired
    `file_name`, one inside a `foreach` zone body — loads as `export_atoms`
    with data and wires intact. The data assertion must check the loaded
    node's stored `file_name` **value** (downcast to `ExportAtomsData`), not
    just the type name — this is the regression guard for the `data_type`
    rewrite: a `node_type_name`-only rename reloads as `NoData` and only a
    value check catches it. Re-save emits `version: 7`; invocation-counter
    check that v7 files skip the pass.
  - `execute_node_test.rs` gains the format arms: `.mol` name → file written
    in V3000; `.pdb`/extension-less name → localized `NetworkResult::Error`
    naming the supported extensions; sidecar written next to a `.mol` export
    with `file`/`blake3`/`version: 2` keys.
  - Text-format roundtrip: `e = export_atoms { file_name: "out.mol" }`.
- **Deliverable:** issue #353 functionally closed (mol export via node);
  old projects load unchanged.

### Phase 3 — UX (format signaling) + docs + full gate

- `get_atom_export_formats()` API + FRB regen.
- `lib/common/export_format_dialog.dart` shared helper; `_exportVisible`
  switches to it.
- `export_atoms_editor.dart` rework: reactive format indicator (incl.
  wired-pin neutral state), Browse = chooser → single-extension `saveFile`
  with extension-less default → `endsWith` post-append, reworded info card.
- Living-docs sweep per the ripple inventory.
- **Tests:** none new on the Rust side (thin wrapper; core covered in
  Phase 1). Full gate: `cargo fmt && cargo clippy && cargo test -j 4`,
  `dart format`, `flutter analyze`, `flutter test integration_test/`.
- **Manual walkthrough:** add `export_atoms`; type `out.xyz` / `out.mol` /
  `out.pdb` and watch the indicator flip (incl. error state) and the subtitle
  mirror it; Browse both formats end-to-end on Windows (bare typed name gets
  the extension appended); wire a `string` into `file_name` → indicator goes
  neutral, Execute honors the wired extension; Execute a `.mol` export and
  open the file + `.params.json` sidecar; load a pre-rename `.cnnd` and
  confirm the node, its wires, and undo behave; *File → Export visible* still
  works through the shared dialog.
- **Deliverable:** formats are discoverable from the panel, the Browse flow,
  and the node subtitle — all fed from `AtomExportFormat::ALL`; docs current;
  feature complete for issue #353.

## Deferred work (explicitly out of scope)

- **`.pdb` (and further formats).** The extension points are enumerated:
  one `AtomExportFormat` arm + one saver in `crystolecule/io/`; UI updates
  flow from the registry.
- **`file_selector` package migration.** Would restore real named filter
  groups in the OS save dialog (per-format dropdown) and could retire the
  pre-dialog pattern app-wide; a separate project touching every picker call
  site.
- **Import-side unification** (`import_xyz` / `import_cif` remain separate
  nodes — imports differ in *output type* (Molecule vs Blueprint), so the
  one-operation-with-a-format-axis argument does not apply).
- **Sidecar v1 reader compatibility.** No known consumers of the v1 key names
  yet; the `version` field exists precisely so a future reader can accept
  both.
