# Design: Splitting the Rust Backend into Cargo Crates

`doc/architecture_overview.md` states the backend's organising
principle: "We aim to create as independent modules as possible.
Dependencies should be as few as possible and should form a DAG: no
circular dependencies." Today that principle is enforced by convention
and code review only — every module lives in one crate
(`rust_lib_flutter_cad`), so nothing stops a back-edge from being
added, and four of them already exist.

This design converts the top-level modules into a **cargo workspace of
crates inside the existing `rust/` directory**, which makes the DAG
compiler-enforced. It is a relocation-and-boundary exercise, not a
redesign: no module is decomposed, no algorithm changes, and the
Flutter build wiring is untouched.

## For the implementor

Work phase by phase, in order; each phase is one commit that builds and
tests green on its own. Most of the work is a mechanical prefix rewrite
(`crate::geo_tree::` → `atomcad_geo_tree::`) that the compiler
verifies. The non-obvious parts — where an otherwise sensible decision
is wrong — are these:

1. **`cargo test` in a root-package workspace runs only the root
   package.** Without an explicit `default-members`, every phase from 1
   onward silently stops running the extracted crates' tests — which is
   precisely the failure the regression gate exists to catch. **D1**
2. **Do not "tidy" the repeated directory name** inside a crate's
   `tests/` (`atomcad-crystolecule/tests/crystolecule/…`). It keeps
   ~250 `#[path]` declarations and 6 test-data paths valid. **D5.1**
3. **The `structure_designer` test tree does not move as a unit.** Ten
   of its 172 files are genuinely api-level and must stay at the root;
   one file has to be split in two. **D5.1a**
4. **Fixtures are shared by three packages.** Do not copy them per
   crate; use the resolver. **D5.3**
5. **A `pub use` re-export does not make a type visible to
   flutter_rust_bridge.** The obvious fix for a moved Dart-facing type
   does not work, and it fails *silently*. **D11, D9a**
6. **A down-moved type's twin in `api/` keeps that type's *existing*
   name.** Do not rename it to `API…`: these types are already the
   Dart-facing names, and renaming them renames the generated Dart
   symbol and breaks Flutter. **D9a**
7. **Bare `#[frb]` on a type is a no-op** — do not copy it around
   thinking it matters, and do not add it to fix a codegen problem.
8. **Regenerating FRB bindings without error proves nothing.** Diff
   `lib/src/rust/` after every phase that moves a type — and expect one
   *known, benign* diff in Phase 4 (`DrawingPlane`, `UnitCellStruct`).
   **D6a, Regression strategy**
9. **`AtomicStructureVisualization` leaves the preferences module in
   Phase 4, the other 12 types in Phase 6.** D6 and D9.2 touch the
   same file; read both before editing it. **D6**
10. **Crate boundaries can cost runtime performance.** Benchmark before
    Phase 1 and after Phase 5. **D13**

Where this document fixes a choice (D5.3 fixtures, D9a twins, D10.1),
that choice is deliberate and alternatives were considered — prefer
raising a concern over silently substituting a different approach.

## Motivation

- **The DAG is aspirational, not enforced.** Four back-edges exist
  (see Current state). Each was presumably added because the compiler
  did not object. Crate boundaries make the next one a build failure.
- **The `api ⊥ domain` boundary is the one that actually leaks.**
  `structure_designer` reaches up into `api` from 125 files. Most of
  that is one misplaced enum, but ~12 references are genuine
  presentation logic living in the domain layer.
- **Build and test granularity.** The backend is a single compilation
  unit of ~146k hand-written lines plus a 27.9k-line generated file.
  `cargo test -p <crate>` does not exist as an option, and every crate
  in the tree recompiles together. This matters on the Windows dev
  machine, where `cargo test` already needs `-j 4` to avoid exhausting
  the pagefile.
- **An existing workaround disappears.** `rust/src/lib.rs:8` carries
  `#[cfg(not(frb_expand))]` on `pub mod structure_designer;` — the
  module is hidden from flutter_rust_bridge's macro-expansion pass. In
  a separate crate, `frb_expand` never sees it and the cfg is deleted
  rather than explained.

## Current state (analysis)

### Module sizes (lines of Rust)

| Module | `src/` | `tests/` | Notable external deps |
|---|---|---|---|
| `util` | 2,176 | 182 | — |
| `geo_tree` | 2,806 | 1,759 | csgrs, geo, nalgebra |
| `renderer` | 5,473 | 1,024 | **wgpu, bytemuck, image** |
| `crystolecule` | 24,996 | 29,166 | — |
| `display` | 4,063 | 1,918 | — |
| `expr` | 3,822 | 7,803 | — |
| `structure_designer` | 85,565 | 108,987 | — |
| `api` | 17,500 | 6,090 (integration) | flutter_rust_bridge |
| `frb_generated.rs` | 27,873 | — | generated, do not edit |

`structure_designer` internals: `nodes/` 39,964 (129 files),
`evaluator/` 5,261, `serialization/` 3,861, `text_format/` 3,833,
`undo/` 3,619, `layout/` 1,436; plus `structure_designer.rs` 9,618 and
`node_type_registry.rs` 4,129.

### Dependency edges (measured `crate::<module>` references)

```
util  ←  geo_tree, renderer, crystolecule, display, structure_designer, api
geo_tree  ←  crystolecule, display, structure_designer
renderer  ←  display, structure_designer, api
crystolecule  ←  display, structure_designer, api
display  ←  structure_designer, api
expr  ←  structure_designer, api
structure_designer  ←  api
```

That is a clean DAG **except** for four back-edges:

| Back-edge | Sites | Content |
|---|---|---|
| `crystolecule → api` | 2 | `SelectModifier`, `AtomicStructureVisualization` (`crystolecule/atomic_structure/mod.rs:26-27`) |
| `display → structure_designer` | 2 | `param_atomic_number_to_index` (`display/atomic_tessellator.rs:6`); `StructureDesignerScene`/`NodeOutput` (`display/scene_tessellator.rs:20`) |
| `expr → structure_designer` | 10 | `DataType`, `RecordType` (`expr/expr.rs:2`, `expr/parser.rs:6`, `expr/validation.rs:1`), `NetworkResult` (`expr/expr.rs:3`, `expr/validation.rs:2`) |
| `structure_designer → api` | 131 | see decomposition below |

### Decomposing the `structure_designer → api` edge

The raw count (131 references across 125 files) overstates the
coupling by an order of magnitude. By type:

| Referenced type | Refs | Nature |
|---|---|---|
| `NodeTypeCategory` | **113** | One enum (`structure_designer_api_types.rs:1259`), one reference per node file — the palette category in each node's descriptor. |
| `AtomicStructureVisualization`, `GeometryVisualizationPreferences`, `StructureDesignerPreferences`, `BackgroundPreferences`, `GeometryVisualization` | 11 | The `structure_designer_preferences.rs` module (495 lines). |
| `SelectModifier` | 6 | Same type `crystolecule` reaches for. |
| `APINodeTypeView`, `APINodeCategoryView`, `APINetworkWithValidationErrors`, `APIValidationError`, `APIErrorSource`, `APIErrorRootCause`, `APIExecuteResult`, `APINodeEvaluationResult`, `APIPrintLogEntry`, `APIAtomEditTool`, `DragFrozenStatus`, `CliConfig`, `BatchCliConfig` | ~12 | **12 import lines in 5 files** — the only genuine cycle. |

The ~12 genuine references live in exactly five files. Two of them hold
view-builder methods that construct Dart-facing view models from domain
state:

- `NodeTypeRegistry::get_compatible_node_types()`
  (`node_type_registry.rs:950`), `get_node_type_views()` (`:1050`),
  `get_node_networks_with_validation()` (`:1134`) — imports at
  `:106-110`, roughly 220 lines total.
- `StructureDesigner::get_node_networks_with_errors()`
  (`structure_designer.rs:1340`), `resolve_api_root_cause()` (`:1423`,
  used again `:1453`); `APIExecuteResult`/`APINodeEvaluationResult`
  imports at `:22-23`; `APIPrintLogEntry` at `:8147`.

The other three files are not view-builders and need individually
chosen treatments (D10.1):

- `cli_runner.rs:1` takes `CliConfig`/`BatchCliConfig`.
- `nodes/atom_edit/atom_edit_data.rs:6` (`APIAtomEditTool`),
  `nodes/atom_edit/operations.rs:5` (`DragFrozenStatus`).

The registry view-builders touch only `self.node_networks`,
`self.built_in_node_types`, and `self.resolve_drag_candidate_type` —
so relocating them needs a handful of accessors, not a redesign of a
4,129-line file.

The reverse direction is well-behaved: `structure_designer_api_types.rs`
imports `CollapseMode`, `FunctionPinRole`, `FunctionPinDisposition`
(`node_network.rs`) and `PrintLogEntry` (`network_evaluator.rs`), which
is the correct direction for an adapter.

### Two facts that make this much cheaper than expected

- **FRB *annotations* are confined to `api/`.** There are **zero**
  `#[frb(...)]` attributes anywhere in `util`, `geo_tree`, `renderer`,
  `crystolecule`, `display`, `expr`, or `structure_designer`. No
  extracted crate has to carry codegen annotations.
  Note the precise claim: annotations are confined, but *codegen
  visibility* is not — FRB emits Dart for two `crystolecule` types
  today, purely as a side effect of their appearing in the signatures
  of three `#[frb(ignore)]` functions in `api/`. Both land as opaque
  handles, not data classes; see the next section, and D6a for how
  Phase 4 handles them.
- **Visibility is already crate-external-ready.** The whole tree
  contains **15** `pub(crate)` items (crystolecule 10, display 2,
  structure_designer 2, expr 1). The usual visibility avalanche when
  splitting a crate does not occur here.

### How flutter_rust_bridge decides what to generate

Measured by reading the `flutter_rust_bridge_codegen` 2.10.0 source in
the local cargo registry. This governs which types may move between
crates, so it is recorded here in detail.

**Bare `#[frb]` on a type is a no-op.** `FrbAttributes::parse`
(`parser/mir/parser/attribute.rs:29-32`) maps an argument-less
`Meta::Path` attribute to `FrbAttribute::Noop`, and `Noop` is never
read anywhere in the codegen — it is only constructed and asserted on
in unit tests. The 16 bare `#[frb]` annotations in `api/` (on
`NodeView`, `APIErrorSource`, `NodeTypeCategory`, and 13 preferences
types) therefore have **no effect**; they are decorative. Attributes
that *do* carry meaning include `#[frb(mirror(T))]`,
`#[frb(non_final)]` (a **field**-level Dart-mutability flag — all 38
uses here annotate fields, not types), `ignore` / `unignore`,
`opaque` / `non_opaque`, and `sync`.

**Types are included by reachability, not by annotation or
namespace.** A type is pulled into codegen when it is reachable from
an exported function signature, transitively through struct fields and
enum variants. Resolution happens in
`parser/mir/parser/ty/enum_or_struct.rs:46` via
`self.src_objects().get(name)` — a lookup **by bare type name** in a
map built from every expanded crate. The `rust_input` namespace
prefixes (`is_interest`) do *not* gate this; they gate only three
things: which `pub fn` become Dart APIs, `extra_type` inclusion (which
additionally requires an explicit `#[frb(unignore)]`,
`parser/mir/parser/extra_type.rs:35-40`), and the unused-type check.

**Do not read the one apparent in-repo demonstration as confirmation.**
An earlier draft cited `DrawingPlane` and `UnitCellStruct` — defined in
`crate::crystolecule::*`, outside every `rust_input` prefix, carrying
no `#[frb]`, yet generated into `lib/src/rust/crystolecule/` — as proof
that "a type used only as a data member needs no annotation". Checking
the actual output refutes that reading. What codegen emits is:

```dart
// lib/src/rust/crystolecule/drawing_plane.dart
abstract class DrawingPlane implements RustOpaqueInterface {}
```

— an opaque handle, i.e. exactly the degradation shape described under
"An unresolvable type degrades silently" below, not a Dart data class.
The three functions that name these types (`common_api.rs:495`, `:507`,
`:523`) are all `#[frb(ignore)]`, so no Dart API takes or returns
either type; FRB
collected them into the auto-opaque set from the ignored signatures and
emitted dead plumbing. They are also taken by reference, which is
sufficient on its own to force auto-opaque, so this observation cannot
separate "unresolved" from "borrowed" either way.

The conclusion to draw is the conservative one: **do not assume a type
keeps its Dart shape after leaving the expanded crate.** Nothing in
this design depends on cross-namespace reachability — D9a keeps every
Dart-facing shape inside `api/` as a twin — and these two types are
handled explicitly in D6a.

**But the Dart output path follows the type's *defining* namespace.**
`DrawingPlane` lands in `lib/src/rust/crystolecule/drawing_plane.dart`,
not under `api/`. Moving a type to another module or crate therefore
**moves its generated Dart file** and churns Flutter-side imports.

**An unresolvable type degrades silently.** If the name is not found,
`parse_type_path_core` falls through every branch to
`parse_type_rust_auto_opaque_implicit` (`parser/ty/path.rs:71`). The
type becomes an opaque handle — `abstract class X implements
RustOpaqueInterface {}` — with **no error and no warning**. A Dart
`enum` silently becoming an opaque handle breaks every `switch` and
constructor on the Flutter side, and nothing in the build reports it.

**Multi-crate `rust_input` is explicitly supported.** Any entry whose
first path segment is not the literal `crate` is collected as a
third-party crate name (`config/internal_config_parser/rust_path_parser.rs:101-112`),
and the HIR parser then runs `cargo expand -p <crate>` for each, in
addition to the self crate (`parser/hir/raw/mod.rs:12-30`,
`commands/cargo_expand/real.rs:67-70`). Types from a non-self crate are
**automatically treated as mirrored** —
`mirror: mirror_by_ident || !meta.namespace.crate_name().is_self_crate()`
(`parser/hir/flat/parser/syn_item/item_struct_or_enum.rs:48`) — so no
`#[frb(mirror(…))]` is needed. Two constraints apply to such crates:
the type must be `pub` (non-public items are dropped,
`parser/ty/enum_or_struct.rs:213-215`) and must be non-generic.

**`pub use` re-export does NOT relocate a type in the self crate.**
`pub_use_transformer::transform_module` returns early when
`module.meta.namespace.crate_name().is_self_crate()`
(`parser/hir/tree/transformer/pub_use_transformer.rs:20-23`) — the
comment says "Only apply to third party crate currently". A
`pub use atomcad_x::Foo;` inside `crate::api` therefore does **not**
move `Foo` into the `crate::api` namespace.

**Cross-crate name collisions are silent.** The struct/enum maps are
keyed by bare type name, and duplicates are resolved last-one-wins with
only a `debug!` line (`parser/hir/flat/exporter.rs:33-38`) — invisible
at normal log level. Two same-named `pub` types in two scanned crates
will silently shadow each other.

### Build-system constraints

Cargokit hard-codes both the package name and the directory:

- `rust_builder/windows/CMakeLists.txt:12` —
  `apply_cargokit(${PROJECT_NAME} ../../../../../../rust rust_lib_flutter_cad "")`
- `rust_builder/linux/CMakeLists.txt:11` — same, `../../rust`
- `rust_builder/macos/rust_lib_flutter_cad.podspec:31` and the iOS
  counterpart — `build_pod.sh ../../rust rust_lib_flutter_cad`
- `flutter_rust_bridge.yaml` — `rust_root: rust/`, and every
  `rust_input` entry is a `crate::api::…` path

Therefore the cdylib package must remain named `rust_lib_flutter_cad`
and must remain buildable by `-p rust_lib_flutter_cad` against
`rust/Cargo.toml`.

## Non-goals

- **Decomposing `structure_designer` internally.** Splitting the 129
  node implementations (39,964 lines) from `node_type_registry.rs`
  requires replacing static registration with a trait-object or
  inventory mechanism. That is a separate design.
- **Making `expr` an independent crate.** `expr` needs `NetworkResult`,
  which needs `NodeTypeRegistry`, which needs `node_network`, which
  needs the nodes. Independence means making `expr` generic over a
  value trait — a real redesign, not a move. See D8.
- **Publishing any crate to crates.io.** All crates stay
  `publish = false`; the workspace is an internal structuring device.
- **Changing the Flutter build, the FRB configuration, or the
  generated bindings.** `flutter_rust_bridge.yaml` is unchanged by
  this design.
- **Moving `csgrs`.** It stays a vendored path dependency at
  `../csgrs` (relative to `rust/`) with its local EPSILON patch. Note
  that keeping the path literally `../csgrs` requires declaring it in
  `[workspace.dependencies]` — see D4.
- **Performance work.** No runtime *behaviour* changes: no algorithm
  is altered. This is **not** a claim that runtime performance is
  unaffected — crate boundaries change what the optimiser may inline,
  which is a real risk addressed in D13. Compile-time effects are a
  side benefit, quantified honestly in D12, not the justification.

## Design decisions

### D1. A workspace rooted at `rust/`, with `rust/` itself a package

`rust/Cargo.toml` becomes both `[package]` (still
`rust_lib_flutter_cad`, still `crate-type = ["cdylib", "staticlib",
"rlib"]`) and `[workspace]` with `members = ["crates/*"]`. Cargokit
continues to build against `rust/Cargo.toml` and gets the same package
under the same name. Nothing in `rust_builder/` or
`flutter_rust_bridge.yaml` changes.

The alternative — a virtual workspace root with the cdylib demoted to
a member — would require editing four platform build files and is
rejected for no benefit.

**`default-members` is mandatory here, not optional polish.** In a
workspace *with* a root package, cargo's default package selection when
no `-p` / `--workspace` flag is given is **the root package alone**.
Left implicit, `cd rust && cargo test -j 4` would from Phase 1 onward
compile and run only `rust_lib_flutter_cad`'s tests and quietly skip
every extracted crate — the exact "test file stops being compiled and
disappears from the run" failure the regression gate is built to catch,
except that here it would be caused by the gate's own command. The same
applies to `cargo clippy`. Phase 0 therefore sets:

```toml
[workspace]
members = ["crates/*"]
default-members = [".", "crates/*"]
```

Cargokit is unaffected either way: it always passes an explicit package
selection — `cargo build --manifest-path <rust>/Cargo.toml -p
rust_lib_flutter_cad …`
(`rust_builder/cargokit/build_tool/lib/src/builder.dart:139-165`) — so
`default-members` cannot change what it produces. The key is therefore
only that `default-members` is set *and* that the root package stays in
it, so that the human/agent-facing `cargo test` and `cargo clippy`
commands keep covering the whole tree.

### D2. Layout

```
rust/
  Cargo.toml            # [package] rust_lib_flutter_cad + [workspace] + [workspace.dependencies]
  src/
    lib.rs              # shrinks to: pub mod api; mod frb_generated;
    api/                # unchanged in place
    frb_generated.rs
  tests/
    integration/        # cross-layer tests stay here (D5)
    fixtures/           # shared by three packages — stays put (D5.3)
    structure_designer_api.rs + structure_designer_api/
                        # the 10 api-level tests left behind by D5.1a;
                        # the other 162 move to the crate
  crates/
    atomcad-util/{src,tests}/
    atomcad-geo-tree/{src,tests}/
    atomcad-renderer/{src,tests}/
    atomcad-crystolecule/{src,tests}/
    atomcad-display/{src,tests}/
    atomcad-structure-designer/{src,tests}/   # includes expr/ (D8)
    atomcad-test-support/                     # shared test helpers (D5.2)
```

Inside each crate, `tests/` keeps the module's original directory name
(`atomcad-crystolecule/tests/crystolecule/…`) — see D5.1 for why the
apparent redundancy is load-bearing.

Files keep their names and their internal directory structure.
`rust/src/geo_tree/foo.rs` becomes
`rust/crates/atomcad-geo-tree/src/foo.rs`; the module tree inside each
crate is unchanged apart from the removed top-level `mod` wrapper.

### D3. Naming

Crates are `atomcad-<module>` (hyphens), imported as
`atomcad_<module>` (underscores). The per-crate `lib.rs` re-exports
the module's existing public surface, so `crate::geo_tree::GeoNode`
becomes `atomcad_geo_tree::GeoNode` — a one-token change at every call
site, applied by search-and-replace and verified by the compiler.

This façade is a **Rust-level convenience for call sites only**. It
carries no weight with flutter_rust_bridge: a `pub use` does not
relocate a type into the re-exporting namespace for codegen purposes
(D11). Never rely on it to keep a Dart-facing type visible.

### D4. Shared dependency versions live in `[workspace.dependencies]`

Every third-party version (`glam`, `serde`, `thiserror`, `rustc-hash`,
…) moves to `[workspace.dependencies]` in the root manifest; member
crates use `glam = { workspace = true }`. This prevents the classic
workspace failure mode where two crates end up on different `glam`
versions and `DVec3` stops being the same type.

Heavy dependencies are declared only where used. `wgpu`, `bytemuck`,
and `image` appear in `atomcad-renderer` alone (they are referenced in
9 files, all under `renderer/`). `csgrs`, `geo`, and `nalgebra` go to
`atomcad-geo-tree`. `flutter_rust_bridge` stays in the root package
only (D11).

**`csgrs` must be declared in `[workspace.dependencies]`, not inlined
in the member manifest.** It is a path dependency —
`csgrs = { path = "../csgrs" }` (`rust/Cargo.toml:36`) — and a relative
path is resolved against the manifest that declares it. Copied verbatim
into `crates/atomcad-geo-tree/Cargo.toml` it would point at
`rust/crates/csgrs` and fail. Declaring it once in
`[workspace.dependencies]` keeps the path anchored to the workspace
root, so `../csgrs` stays correct and `atomcad-geo-tree` writes only
`csgrs = { workspace = true }`. (The alternative — spelling
`../../../csgrs` in the member — works but re-encodes the crate's depth
in the tree.) Note `../csgrs` sits *outside* the workspace directory, so
it does not become a workspace member; that is unchanged from today.

### D5. Tests travel with their crate; cross-layer tests stay at the root

`rust/tests/<module>/` moves to that crate's `tests/`, and the
`rust_lib_flutter_cad::<module>::` prefix in those files becomes
`atomcad_<module>::`. This preserves the existing convention that
tests live in `tests/`, never inline with `#[cfg(test)]`
(`rust/AGENTS.md`).

Two groups of files break that rule, in opposite directions. In both
cases the blocker is the same: a test cannot live in a crate that does
not depend on everything the test names.

- **Cannot move down with their crate:** 6 files under
  `tests/crystolecule|display|geo_tree|renderer`
  reach *up* into `rust_lib_flutter_cad::structure_designer` or
  `::api` — `crystolecule/atomic_structure_test.rs`,
  `display/atomic_impostor_alpha_test.rs`,
  `display/atomic_render_style_test.rs`, `display/atom_label_test.rs`,
  `geo_tree/batched_implicit_evaluator_test.rs`,
  `renderer/camera_test.rs`. They cannot travel downward with their
  crate. Each is handled individually: either the dependency is
  incidental and gets dropped, or the test moves to the root `tests/`
  tree alongside `tests/integration/`. (`tests/expr/` reaches up in 14
  files, but that is harmless: `expr` merges *into*
  `atomcad-structure-designer` per D8, so those tests move with it.)
- **Cannot move up with their crate:** ten files under
  `tests/structure_designer/` depend on `api` in ways that survive the
  D9 down-moves, so they cannot follow `structure_designer` into
  `atomcad-structure-designer` — that crate is *below* `api` and cannot
  depend on the root. This is the larger and more disruptive of the two,
  and it is enumerated in D5.1a.

The test tree is **not** a simple mirror of `src/`, and five of its
properties dictate how the move must be done. Getting these wrong is
the most likely way to stall Phase 4 or 6.

#### D5.1 Preserve the intra-`tests/` subdirectory name

Each test binary is one root file that declares every module with an
explicit `#[path]` — `tests/structure_designer.rs` is 520 lines of
`#[path = "structure_designer/…"] mod …;` covering 172 files, and
`tests/crystolecule.rs` does the same for 42.

Those `#[path]` strings are relative to the root file, so they stay
valid **only if the harness file and its sibling directory move
together and keep their directory name**. The layout is therefore:

```
crates/atomcad-crystolecule/tests/crystolecule.rs
crates/atomcad-crystolecule/tests/crystolecule/…
```

The repeated `crystolecule` looks redundant, but flattening it would
require rewriting every `#[path]` in all 248 relocated test files, and
would additionally break the 6 UFF/simulation tests that load
`concat!(env!("CARGO_MANIFEST_DIR"), "/tests/crystolecule/simulation/test_data/uff_reference.json")`
(`minimize_test.rs:158`, `topology_test.rs:53`, and four `uff_*_test.rs`).
Preserving the name makes both groups need **zero** edits. Do not
"tidy" this.

The "zero edits" claim holds fully for `atomcad-crystolecule` (Phase 4),
where the whole harness travels intact. It does **not** hold for
`structure_designer`, where the harness must first be partitioned —
see D5.1a. Preserving the directory name still pays there, because it
keeps the `#[path]` strings valid for the 162 files that do move; only
the ten `#[path]` lines of the files left behind are edited (deleted
from one harness, added to the other).

#### D5.1a The `structure_designer` harness must be split in two

`tests/structure_designer/` holds 172 files, of which **31 import
`rust_lib_flutter_cad::api`** (41 import sites). Most of that
disappears on its own once Phase 6.1 lands, and the residue does not:

- **21 files resolve themselves.** 19 use only `NodeTypeCategory`, one
  (`raytrace_per_node_test.rs`) only `AtomicStructureVisualization`,
  and one (`atom_edit_undo_test.rs`) only `DragFrozenStatus`. All three
  types acquire a domain definition in a lower crate — `NodeTypeCategory`
  and `DragFrozenStatus` in `atomcad-structure-designer` (D9.1, D10.1),
  `AtomicStructureVisualization` in `atomcad-crystolecule` (D6) — so
  these tests just switch to the domain path and move with the crate.
- **Ten files do not.** They consume transport types or `api`-level
  functions that D10 deliberately keeps *up*, and a member crate cannot
  depend on the root crate:

  | File | api surface used |
  |---|---|
  | `error_origins_test.rs` | `APIErrorSource`, `APINetworkWithValidationErrors`, `APIValidationError` |
  | `eval_error_snapshot_test.rs` | same three |
  | `chain_hygiene_test.rs` | `APIErrorSource` |
  | `data_type_test.rs` | `api_data_type_to_data_type`, `data_type_to_api_data_type`, `APIDataType`, `APIDataTypeBase` |
  | `function_pin_test.rs` | `build_function_pin_role_views`, `APIFunctionPinRole`, `APIFunctionPinDisposition` |
  | `atom_edit_bond_order_test.rs` | `APIAtomEditTool` |
  | `atom_edit_add_atom_marker_test.rs` | `APIAtomEditTool` |
  | `preferences_test.rs` | `APIIVec3` |
  | `drag_adapter_test.rs` | `APINodeCategoryView` |
  | `node_type_registry_test.rs` | `get_compatible_node_types`, `get_node_type_views` — the D10 view-builders |

**Ten is a floor, not a final count.** The list was built by grepping for
`rust_lib_flutter_cad::api`, which misses tests that *call* a method
whose return type is api-side without ever naming that type —
`node_type_registry_test.rs` is exactly that case (it imports only
`NodeTypeCategory` but calls `get_compatible_node_types` and
`get_node_type_views` throughout). After commit 6.2 moves the
view-builders, re-run the check by compiling, not by grepping: cut the
crate, let the ~162 files fail, and move whatever does not build.

**Decision: two harnesses.**

```
crates/atomcad-structure-designer/tests/structure_designer.rs   # 162 files
crates/atomcad-structure-designer/tests/structure_designer/…    # D5.1 name rule
rust/tests/structure_designer_api.rs                            # the 10 above
rust/tests/structure_designer_api/…
```

The 173 `#[path]` lines of the current harness are partitioned between
the two root files (the 173rd is the `test_support` include, which D5.2
turns into a `use` on both sides); nothing else changes.

Two consequences the implementor should plan for rather than discover:

- **`function_pin_test.rs` must itself be split.** It imports api
  twice, and the two are not equivalent: the preferences types at
  `:13-15` resolve themselves under D9.2, but the view-builder block
  from `:1290` (`build_function_pin_role_views`, `APIFunctionPinRole`,
  `APIFunctionPinDisposition`) does not. Cut that trailing block into
  `tests/structure_designer_api/function_pin_api_test.rs` and leave the
  domain tests with the crate.
- **`preferences_test.rs` is a judgement call.** Its only genuine api
  dependency is `APIIVec3`; the 12 preferences types it exercises move
  down in Phase 6.1. If the `APIIVec3` uses are incidental (constructing
  expected values), convert them to the domain `IVec3` and let the file
  move with the crate; otherwise it stays at the root. Decide this when
  writing commit 6.1, not while cutting the crate.

The api-side harness needs `atomcad-structure-designer` as a
`[dev-dependencies]` entry of the root package — it already will, since
the root depends on it normally.

#### D5.2 `test_support/` must become a real crate

`tests/test_support/structure_equivalence.rs` is `#[path]`-included as
a module by **both** `tests/crystolecule.rs:1` and
`tests/structure_designer.rs:135`. Its own doc comment explains why:
the two test binaries "are separate binaries and cannot import from
each other". After the split they are in separate *packages*, so
`#[path]` inclusion would need `../../..` traversal across package
boundaries.

Introduce `crates/atomcad-test-support/`, a `publish = false` crate
used as a `[dev-dependencies]` entry by the crates that need it. It
depends on `atomcad-crystolecule` (it operates on `AtomicStructure`).
The two `#[path]` includes become ordinary `use` statements.

This creates a **dev-dependency cycle** —
`atomcad-crystolecule` dev-depends on `atomcad-test-support`, which
depends on `atomcad-crystolecule`. That is legal and supported: cargo
permits a cycle that passes through a `[dev-dependencies]` edge (it
breaks the cycle by building the library target without dev-deps
first). `cargo metadata` will show the loop and it is not an error. Do
not try to "fix" it by inlining the helper back into each test tree.

#### D5.3 Shared fixtures need a resolver, not a copy

`tests/fixtures/` (9 directories) is read from **31 sites across 15
files**, which after the split land in three different packages:
`atomcad-crystolecule` (5 files, the CIF tests),
`atomcad-structure-designer` (5 files — `apply_function_pin_iter_test`,
`import_cif_test`, `invariants_test`,
`rename_wire_loss_regression_test`, `zones_migration_test`; none of
them is among the ten D5.1a leaves at the root), and the root crate's
`tests/integration/` (5 files, the `.cnnd` migration corpus).

Both addressing styles in use today break on the move, because both
are anchored to the package root:

- `format!("{}/tests/fixtures/cif/{}", env!("CARGO_MANIFEST_DIR"), …)`
  — 12 sites; `CARGO_MANIFEST_DIR` becomes the *crate* dir.
- `std::fs::read_to_string("tests/fixtures/cif/diamond.cif")` — the
  remainder; the `cargo test` working directory is the package root.

**Decision: fixtures stay put at `rust/tests/fixtures/`** (they are
shared by three packages; duplicating them would fork the migration
corpus, which is exactly the kind of drift the corpus exists to
catch). `atomcad-test-support` gains a resolver:

```rust
pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).join(rel)
}
```

`CARGO_MANIFEST_DIR` here expands at compile time of *test-support*,
so the result is the same no matter which crate calls it — the `../..`
knowledge lives in exactly one line. The 31 call sites become
`fixture_path("cif/diamond.cif")`.

These failures are loud (file-not-found panics), not silent, so the
risk is a stalled phase rather than a wrong result. But the strategy
is a design decision with several plausible-but-worse alternatives
(per-crate fixture copies, scattered `../../` traversal, symlinks),
which is why it is settled here rather than left to the implementor.

#### D5.4 Snapshots

The 24 `.snap` files live in 3 directories
(`tests/integration/snapshots`, `tests/structure_designer/snapshots`,
`tests/structure_designer/nodes/snapshots`). `insta` resolves the
snapshot directory relative to the test file, so preserving D5.1's
layout keeps them adjacent to their tests and requires no edits.

The D5.1a split cuts across this, so be explicit about it:
`nodes/snapshots` moves with the crate (`node_snapshots_test.rs` is a
`NodeTypeCategory`-only file). `structure_designer/snapshots` is shared
between files on both sides of the split — `eval_error_snapshot_test.rs`
stays at the root while its neighbours move — so the snapshots it owns
must be **moved** into `tests/structure_designer_api/snapshots/`,
carrying their `.snap` files, and the rest stay with the crate. Split
by which test file writes each snapshot, and confirm afterwards that
`cargo test -j 4` needs no `cargo insta review`: an orphaned snapshot
shows up as a *new* snapshot on the other side, not as an error. The
integration one stays at the root untouched.

### D6. `crystolecule → api`: move the two types down

`SelectModifier` (defined in `api/common_api_types.rs`) and
`AtomicStructureVisualization` (defined in
`api/structure_designer/structure_designer_preferences.rs:143`) move
into `atomcad-crystolecule`. Both happen in **Phase 4**.

**The preferences module is therefore split, not moved wholesale.**
This is the one place where D6 and D9.2 touch the same file, so the
division is fixed here to avoid a conflict:

- `AtomicStructureVisualization` — the only preferences type
  `crystolecule` needs — goes to `atomcad-crystolecule` in **Phase 4**.
- The remaining 12 `pub` types in
  `structure_designer_preferences.rs` go to
  `atomcad-structure-designer` in **Phase 6** (D9.2).

An implementor must not read D9.2 as "move the whole file in Phase 6";
by then one enum has already left it.

Both types are **Dart-facing** (`lib/src/rust/api/common_api_types.dart:285`
and `.../structure_designer_preferences.dart:22` are generated Dart
`enum`s), so a Dart declaration for each must survive the move. Per the
FRB analysis above, a `pub use` re-export from `api` will **not**
provide one — the transformer skips the self crate. D9a settles the
mechanism: the authoritative definition moves down, and a same-named
twin stays behind in the *same `api/` file it occupies today*, so the
generated Dart path and symbol are both unchanged.

### D6a. `DrawingPlane` and `UnitCellStruct`: their generated Dart will change

These two are not part of any back-edge — they are ordinary
`crystolecule` types that Phase 4 relocates like everything else. They
get their own entry because they are the **only** types in the tree
whose Dart output changes without anyone intending it, and because the
Regression strategy's gate would otherwise flag that change as a build
break.

Today they are reachable from `api/` only through three
`#[frb(ignore)]` functions — `resolve_miller_plane_up`,
`resolve_lattice_direction_up`, `drawing_plane_up`
(`common_api.rs:495`, `:507`, `:523`) — and FRB nonetheless emits
`lib/src/rust/crystolecule/drawing_plane.dart` and
`unit_cell_struct.dart`, each holding a single
`abstract class … implements RustOpaqueInterface {}` with no members.
No Dart API takes or returns either type, and **no file under `lib/`
outside `lib/src/rust/` imports either generated file** (verified). The
Dart output is dead.

After Phase 4 the defining namespace is `atomcad_crystolecule`, which is
not in `rust_input` and is never `cargo expand`ed, so one of two things
happens on regeneration: the two Dart files disappear, or they
reappear under a differently-named directory. **Both outcomes are
acceptable and expected**; neither is the silent-degradation failure
D11 warns about, because there is no data class or `enum` being lost —
these were already opaque handles, and nothing consumes them.

**Action in Phase 4:** regenerate, confirm the *only* change under
`lib/src/rust/` beyond the intended `SelectModifier` /
`AtomicStructureVisualization` edits is the disappearance or relocation
of these two files, confirm `flutter analyze` stays at its baseline,
and record which of the two outcomes occurred here. If the files simply
vanish, prefer that: deleting dead generated code is an improvement, and
`lib/src/rust/crystolecule/` going away is the expected end state.

### D7. `display → structure_designer`: move one file up

`display/scene_tessellator.rs` consumes `StructureDesignerScene` and
`NodeOutput`. It is not an adapter from domain to renderer — it is an
adapter from the *scene graph* to the renderer, and the scene graph is
a `structure_designer` concept. The file moves up into
`atomcad-structure-designer`, where it continues to call into
`atomcad-display` for the per-object tessellation it needs.

`param_atomic_number_to_index` (`display/atomic_tessellator.rs:6`) is a
small pure helper; it moves down into `atomcad-crystolecule` beside the
atomic constants it operates on, with `structure_designer` updated to
use it from there.

### D8. `structure_designer` and `expr` are one crate

`expr` is 3,822 lines and depends on `DataType`, `RecordType`, and
`NetworkResult`. `NetworkResult` (`evaluator/network_result.rs:11-14`)
depends on `NodeTypeRegistry`, `ZoneClosure`, and `Walker`;
`DataType` (`data_type.rs:5`) also depends on `NodeTypeRegistry`.
There is no thin seam: extracting a shared value/type crate below
`expr` would drag `node_type_registry.rs` (4,129 lines), which
transitively pulls in `node_network` and the entire `nodes/` tree.

Placing `expr` inside `atomcad-structure-designer` dissolves the
back-edge at zero cost. The dependency is real and correct — `expr`
*is* the expression language of the node network — it was simply
described in `architecture_overview.md` as a peer when it is a
component.

This is recorded explicitly because the naive reading ("one module,
one crate") would produce a cycle and stall the whole effort.

### D9. `structure_designer → api`: three separate fixes, not one

The 131 references decompose into three unrelated problems. Read "moves
down" throughout as D9a defines it — the **authoritative definition**
moves to the lower crate, while a same-named twin remains in the `api/`
file it lives in today. The api-side files are not deleted, and no
generated Dart path changes.

1. **`NodeTypeCategory` (113 refs) moves down** into
   `atomcad-structure-designer`, next to `NodeType`. It describes
   which palette group a node belongs to — domain metadata that `api`
   happens to have declared. It is Dart-facing
   (`structure_designer_api_types.dart:4397`), so the twin stays in
   `structure_designer_api_types.rs`; the one consumer in `api` itself
   (`ai_assistant_api.rs:392-402`, string→category parsing) keeps
   using that twin and is unaffected.

2. **`structure_designer_preferences.rs` (11 refs) moves down.** These
   are persisted domain settings, not transport DTOs. All 13 of its
   `pub` types are Dart-facing, so each leaves a twin behind and the
   api-side file keeps its current path
   (`api/structure_designer/structure_designer_preferences.rs`); note it
   contains only 2 `pub fn`, which bounds the auto-export risk discussed
   in D9a's escape hatch.
   **Scope:** 12 of the 13 types move here, in Phase 6 —
   `AtomicStructureVisualization` has already gone to
   `atomcad-crystolecule` in Phase 4 (D6).

3. **The ~12 genuine DTO references move *up*** — see D10.

Treating these as one 131-reference problem is what made this split
look prohibitive in the first assessment. What they actually amount to
is ~130 lines of mechanical reference rewriting, 14 twin declarations
with their `From` impls (D9a), and one small refactor (D10).

### D9a. Down-moved Dart-facing types use the existing twin pattern

Every type D6 and D9 move down is Dart-facing, so each needs a Dart
declaration that survives the move. Two mechanisms exist:

- **(i) Scan the new crate.** Add e.g.
  `atomcad_structure_designer::node_type` to `rust_input`. FRB then
  runs `cargo expand -p atomcad-structure-designer`, and the types are
  auto-mirrored with no `#[frb]` needed. Costs: it breaks the
  FRB-confinement invariant (D11); every `pub fn` in the listed
  namespace is auto-exported as a Dart API, so the entry must point at
  a deliberately narrow module; each scanned crate adds a full
  `cargo expand` pass to codegen; and the generated Dart files move to
  a new directory, churning Flutter imports.
- **(ii) The twin pattern.** Keep the domain type in the lower crate
  and declare a twin in `api/` with `From` impls both ways.

**Chosen: (ii).** This is not a new invention — it is the pattern the
codebase already uses for exactly this situation. `CollapseMode`,
`FunctionPinRole`, `FunctionPinDisposition` (`node_network.rs`) and
`PrintLogEntry` (`network_evaluator.rs`) are domain types with
`APICollapseMode`, `APIFunctionPinRole`, `APIFunctionPinDisposition`,
and `APIPrintLogEntry` twins declared in
`structure_designer_api_types.rs:253-353`, each with `From` impls.
That is *why* the measurement in Current state found **zero** domain
types in `frb_generated.rs`: grepping the generated file for
`CollapseMode` yields 17 hits, all of them substrings of
`APICollapseMode`.

**The twin keeps the type's existing name; the *new* copy is the domain
one.** This is the single most important detail in D9a and it inverts
the naming of the precedent above, so it is stated as a rule:

> For every type moved down by D6, D9 or D10.1, the declaration
> remaining in `api/` **keeps its current identifier**, and the copy
> created in the lower crate takes that same identifier in its own
> crate's namespace.

`NodeTypeCategory`, `SelectModifier`, `DragFrozenStatus` and the 13
preferences types are *already* the Dart-facing names — they are what
`lib/src/rust/**.dart` declares and what Flutter calls today. Renaming
them to `APINodeTypeCategory`,
`APISelectModifier` and so on would rename every generated Dart symbol,
break every Flutter reference, and fail the Regression strategy's own
"the Dart symbol set is otherwise unchanged" check. The
`APICollapseMode` / `APIPrintLogEntry` precedent is prefixed only
because in those cases the *domain* name was already in scope in `api/`;
after a crate split it is not, since the two live in different crates
and the domain one need not be imported into `api/` at all. Where both
must be in scope in one file, disambiguate with a path-qualified `use`
(`use atomcad_structure_designer::node_type::NodeTypeCategory as
DomainNodeTypeCategory;`) rather than by renaming the api-side type.

Applying the established pattern keeps D11 intact, keeps the Dart file
layout stable, and adds no codegen cost. Its price is a twin
declaration plus `From` impls per type — cheap for `NodeTypeCategory`
(a 7-variant C-like enum) and for `SelectModifier`, and moderate for
the 13 preferences types.

**Escape hatch:** if twinning the full preferences module proves too
bulky in Phase 6, mechanism (i) is acceptable *for that module only*,
because it holds just 2 `pub fn` (bounding accidental API export). If
taken, record the deviation from D11 here.

### D10. View-builders move up; DTOs do not move down

Two ways to break the residual cycle:

- **(A) Move the view-builder methods into `api/`.** The five methods
  named in Current state stop being methods on `NodeTypeRegistry` /
  `StructureDesigner` and become functions in `api/` that take `&NodeTypeRegistry`
  / `&StructureDesigner`. Cost: ~250–400 relocated lines and ~3 new
  public accessors (the builders touch only three fields).
- **(B) Move the ~12 DTOs down and leave `#[frb(mirror(…))]` shims in
  `api/`.** Faster to type, but leaves ~11 struct definitions
  permanently duplicated — every field addition has to be made twice,
  with a silent Dart-side drift if the second edit is forgotten.

**Chosen: (A).** These builders are presentation logic that was
written in the domain layer for convenience; moving them up is the
architecturally correct outcome and leaves no maintenance tax.
Option (B) is retained as a fallback for any individual type where (A)
turns out to require exposing internals that should stay private.

#### D10.1 The three non-view-builder cases

D9's third item routes "~12 DTO references" here. Only two of the five
affected files hold view-builders (`node_type_registry.rs` and
`structure_designer.rs`, handled by D10's treatment (A)); the remaining
three each need a different treatment, fixed here so they are not left
to improvisation:

- **`cli_runner.rs` (`CliConfig`, `BatchCliConfig`) — move the file
  up.** It exposes exactly two `pub fn`
  (`run_cli_single_mode`, `run_cli_batch_mode`), and its only callers
  are `api/structure_designer/structure_designer_api.rs:8161` and
  `:8187`. A 340-line module whose sole consumer is `api` belongs in
  `api/`. This is treatment (A) applied to a whole file.

- **`nodes/atom_edit/atom_edit_data.rs` (`APIAtomEditTool`) — move the
  method up.** `get_active_tool()` (`:1192`) is a pure projection from
  the domain enum `AtomEditTool` onto its api-side twin
  `APIAtomEditTool`. Both types already exist, so nothing needs
  twinning here — this is a view-builder in everything but location:
  move the method to `api/` and leave `AtomEditTool` where it is.

- **`nodes/atom_edit/operations.rs` (`DragFrozenStatus`) — twin it.**
  Unlike the other two, this value is consumed *inside* the domain
  (`:361`, `:383`, `:452`), so the producing function cannot move up.
  Apply D9a: declare a domain `DragFrozenStatus` in
  `atomcad-structure-designer`, keep the Dart-facing twin in `api/`
  (it is generated at
  `structure_designer_api_types.dart:4173`), and convert at the
  boundary with `From`.

All four types here are Dart-facing (`APIAtomEditTool` at
`structure_designer_api_types.dart:493`, `CliConfig` at `:4111`,
`BatchCliConfig` at `:4089`, `DragFrozenStatus` at `:4173`), so the
generated-Dart diff check in Regression strategy applies to each.

Note the asymmetry with D6/D9: types that are *domain concepts*
(`NodeTypeCategory`, `SelectModifier`, preferences) move **down**, and
leave a same-named twin behind in `api/` per D9a if they are
Dart-facing; types
that are *transport shapes* (`APINodeTypeView`, `APIValidationError`,
…) stay **up**, and the code constructing them moves up to join them.
The test is what the type means, not which direction removes more
references.

### D11. FRB stays confined to the root crate

No `#[frb(...)]` attribute is introduced outside `rust/src/api/`, and
`flutter_rust_bridge.yaml` keeps `rust_root: rust/` with `crate::api::…`
inputs. This is already true (Current state) and is preserved as an
invariant so that no phase of this work has to solve
"flutter_rust_bridge across crate boundaries" — the one genuinely
uncertain part of the problem space.

An earlier draft of this design assumed a re-export
(`pub use atomcad_crystolecule::SelectModifier;` in `api`) would
satisfy FRB, on the grounds that the type becomes nameable at a
`crate::api::…` path. **That assumption is false** and the design no
longer relies on it: `pub_use_transformer` returns early for the self
crate, so the re-export is invisible to codegen and the type would
silently degrade to an opaque handle (see the FRB analysis in Current
state). D9a's twin pattern replaces it.

The invariant is therefore maintained by construction rather than by
luck: no down-moved type is required to be FRB-visible from its new
crate, because the Dart-facing shape stays in `api/` as a twin.

**Verification, Phase 4:** after moving `SelectModifier`, confirm that
`lib/src/rust/api/common_api_types.dart` still declares
`enum SelectModifier` and that no new file appears under
`lib/src/rust/atomcad_crystolecule/`. A generated
`abstract class SelectModifier implements RustOpaqueInterface` is the
signature of the silent-degradation failure and must be treated as a
build break even though codegen exits successfully.

### D12. Honest accounting of the compile-time benefit

Stated plainly so this is not oversold:

**Real gains**
- Editing `api/` stops recompiling 85,565 lines of
  `structure_designer`. The root crate drops from ~191k lines to
  ~45k (api 17,500 + frb_generated 27,873).
- `cargo test -p atomcad-crystolecule` (29,166 test lines) builds
  without `wgpu` and without `frb_generated.rs`.
- `cargo test -p atomcad-structure-designer` (108,987 test lines, less
  the ten api-level files D5.1a leaves at the root) skips `api` and
  `frb_generated.rs`. It still pulls `wgpu` transitively via
  `display`/`renderer`.
- The five lower crates compile in parallel rather than as one unit.
- Relevant to the known Windows memory constraint: smaller
  compilation units lower peak memory per rustc process.

**Not a gain**
- Editing `structure_designer` still forces a root-crate rebuild,
  including `frb_generated.rs`, because the root depends on it. Crate
  splitting does not help edits at the bottom of the graph.
- `frb_generated.rs` is 27,873 lines — comparable to `crystolecule`,
  not dominant. It is ~16% of the post-split root crate.

The primary justification for this work is the enforced DAG (see
Motivation), not build time.

### D13. Cross-crate inlining: enable thin LTO and measure

This is the only way the refactor can plausibly make the *product*
worse, so it is called out rather than assumed away.

**The exposure.** `rust/Cargo.toml` has **no `[profile.release]`
section at all**, so the release build uses cargo defaults — meaning
**LTO is off** and `codegen-units = 16`. (csgrs declares `lto = true`
in its own manifest, but cargo ignores profiles from non-root
packages, so nothing in the build has LTO today.) While everything is
one crate, rustc inlines freely across modules. After the split, the
hottest paths become cross-crate calls:

- `util` — 92 `pub fn` (`imat2`, `imat3`, `mat_utils`, `transform`,
  `daabox`, `hit_test_utils`), **zero** `#[inline]`
- `geo_tree` — 57 `pub fn` including SDF evaluation, **zero**
  `#[inline]`
- `crystolecule` — 331 `pub fn`, only 29 `#[inline]`

`AtomicStructure::get_atom` (`atomic_structure/mod.rs:551`) and
`get_atom_alpha` (`:651`) are ~5-line accessors, and
`display/atomic_tessellator.rs` calls that family 19 times in loops
that run per-atom over structures exceeding 10^6 atoms.

**Why it is a risk and not a certainty.** Rust 1.85+ automatically
marks small functions `cross_crate_inlinable` even without `#[inline]`,
so the tiny accessors will very likely still inline; and
`codegen-units = 16` already limits intra-crate inlining today. The
exposure is concentrated in mid-sized hot functions that fall outside
the automatic heuristic. The honest position is that this is
**unmeasured**.

**Decision.** Add to `rust/Cargo.toml` (the workspace root — cargo
ignores `[profile.*]` in member manifests):

```toml
[profile.release]
lto = "thin"
```

and treat a runtime benchmark as a phase gate rather than trusting the
argument above. Thin LTO restores most cross-crate inlining at a
moderate link-time cost; `lto = "fat"` plus `codegen-units = 1` is
available if thin proves insufficient, at a substantially worse link
time.

**Measurement.** Capture a baseline **before Phase 1** — tessellation
and CSG evaluation wall-clock on a large structure (the ~1.07M-atom
nanobeam is the natural fixture) — and re-measure **after Phase 5**,
by which point `util`, `geo_tree`, `renderer`, `crystolecule`, and
`display` have all left the root crate and the exposure is at its
maximum. A regression beyond a few percent is a signal to escalate the
LTO setting, not to accept the result.

Note this interacts with D12: thin LTO claws back runtime performance
by *spending* link time, partially offsetting the compile-time gains
claimed there.

## Regression strategy

This design changes no runtime behaviour, so the bar is that the
existing suites stay green and the app still builds and runs:

- `cd rust && cargo test -j 4` must pass with the same test count
  after every phase. Test *count* is checked explicitly, because the
  characteristic failure mode of this work is a test file that stops
  being compiled after a move and silently disappears from the run.
  **This command only covers the whole tree because D1 sets
  `default-members`** — if a phase ever reports a suspiciously round
  drop in test count, check that first, before hunting for a lost
  `#[path]`. `cargo test --workspace -j 4` is the belt-and-braces form
  and is worth using at least once per phase.
- `cargo clippy` warning count must not exceed the ~14 baseline;
  `flutter analyze` must not exceed its ~68 baseline. (Both estimates
  turned out to be stale — the measured Phase 0 values are 36 and 139;
  see "Phase 0 — landed" and use those.) The same
  `default-members` caveat applies to clippy: a *falling* warning count
  is as suspicious as a rising one.
- `cargo build --release` followed by launching the app must work
  after each phase — this is what proves cargokit is still satisfied.
  Per the project's Windows note, `flutter run` loads the release DLL,
  so the app must be closed during the rebuild.
- The Flutter smoke test (`flutter test integration_test/`) is a
  **manual step for the maintainer**, listed at the end of each phase;
  agents must not run it.
- Every phase is a separate commit that builds and tests green on its
  own, so any phase can be reverted independently.

Snapshot tests (`cargo test node_snapshots`) must **not** need
`cargo insta review` at any point. A changed snapshot means something
other than a relocation happened. The one place this needs care is
Phase 6.3, which *moves* `.snap` files between the two harnesses
(D5.4): a snapshot left behind reappears as a pending new snapshot on
the other side rather than as an error, so check for stray
`.snap.new` files as well as for failures.

**Generated-Dart diff is a required check for any phase that moves a
type.** FRB degrades an unresolvable type to an opaque handle without
erroring (Current state), so a successful `flutter_rust_bridge_codegen
generate` proves nothing on its own. After regenerating, run
`git diff lib/src/rust/` and confirm:

- no `abstract class … implements RustOpaqueInterface` appears where a
  data class or `enum` previously existed;
- no new directory appears under `lib/src/rust/` (a type's Dart file
  follows its *defining* namespace, so a relocated type silently
  relocates its Dart output);
- no Dart symbol is *renamed* — in particular, a down-moved type's twin
  keeps the original identifier (D9a), so `enum NodeTypeCategory` must
  not become `enum APINodeTypeCategory`;
- the Dart symbol set is otherwise unchanged.

**One expected exception, in Phase 4 only:** `DrawingPlane` and
`UnitCellStruct` lose or relocate their generated files under
`lib/src/rust/crystolecule/`. That diff is understood, harmless, and
described in D6a. It is the only pre-authorised deviation from the
four checks above; anything else is a stop.

`flutter analyze` catches the downstream fallout of these only if the
Flutter code still references the symbol in a way that no longer type
checks — which is likely but not guaranteed, so the diff check is the
primary gate.

## Phases

Phases 1–5 are ordered by dependency depth, so each one only ever
rewrites references in crates that are still in the monolith.

### Phase 0 — Workspace scaffolding (no code moves)

Convert `rust/Cargo.toml` to `[package]` + `[workspace]` +
`[workspace.dependencies]`, with an empty `crates/` directory. Set
`default-members = [".", "crates/*"]` (D1) and move `csgrs` into
`[workspace.dependencies]` with its path still `../csgrs` (D4). Add the
`[profile.release] lto = "thin"` block from D13. Verify
`cargo build`, `cargo test -j 4`, `cargo clippy`, cargokit, and a
`flutter run` launch. A safe, independently revertable checkpoint that
proves the build wiring tolerates a workspace before any code moves.

`default-members` is unobservable in this phase — with `crates/` empty
there is nothing to omit — so it must be set here on principle rather
than validated here. Phase 1 is where it earns its keep: record
Phase 0's exact test count, and expect Phase 1's to be *identical* —
the tests move, they are not added. A drop of exactly `atomcad-util`'s
test total is the signature of a wrong package selection, not of a lost
test file.

**Also capture the D13 runtime baseline here**, before any code moves,
so the post-Phase-5 comparison is meaningful. Record the numbers in
this document.

*Estimate: 0.5 day, plus benchmark setup.*

#### Phase 0 — landed 2026-08-17

**Changes.** `rust/Cargo.toml` is now `[package]` + `[workspace]` +
`[workspace.dependencies]` + `[profile.release]`. All 22 third-party runtime
dependencies plus `insta` and `tempfile` moved to `[workspace.dependencies]`,
and the root package's own `[dependencies]` were converted to
`foo = { workspace = true }` so the convention is exercised from day one.
`flutter_rust_bridge`, `flutter_rust_bridge_codegen`, `serde_with` and
`ab_glyph` stay inlined in the root: FRB is confined to the root crate by D11,
and `ab_glyph` is used by one example. `Cargo.lock` is **byte-identical** after
the change, which is the cheapest available proof that no version resolution
moved. New files: `rust/crates/README.md` (placeholder + the conventions a new
crate must follow) and `rust/examples/crate_split_bench.rs` (the D13 harness).
`rust/AGENTS.md` gained a "Cargo workspace" section.

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo clippy`, `cargo clippy --all-targets`, `cargo build --release`, and
cargokit (`flutter build windows --debug` rebuilt
`build/windows/x64/plugins/rust_lib_flutter_cad/…/rust_lib_flutter_cad.dll`
against the new manifest — cargokit reads `[package] name` with a plain TOML
parse and passes `--manifest-path rust/Cargo.toml -p rust_lib_flutter_cad`, so
the workspace is invisible to it). No `.snap.new` files were produced.
**Pending manual step for the maintainer:** launch the app
(`flutter run`, release DLL) and the Flutter smoke test.

**Test count — the Phase 1 tripwire.** `cd rust && cargo test -j 4`:
**5,054 tests** (5,040 passed, 0 failed, 14 ignored) across 10 binaries.
`cargo test --workspace -j 4` returns the identical count.

| binary | tests |
|---|---|
| `rust_lib_flutter_cad` (lib unittests) | 11 |
| `tests/crystolecule.rs` | 1,171 |
| `tests/display.rs` | 59 |
| `tests/expr.rs` | 477 |
| `tests/geo_tree.rs` | 66 |
| `tests/integration.rs` | 113 |
| `tests/renderer.rs` | 42 |
| `tests/structure_designer.rs` | 3,082 (4 ignored) |
| `tests/util.rs` | **15** |
| doc-tests | 18 (8 run, 10 ignored) |

Phase 1 must still report 5,054. **15** is the number to recognise: a drop to
5,039 means `atomcad-util` stopped being selected (a `default-members`
problem), not that a `#[path]` was lost.

**Lint baselines (measured — the estimates in Regression strategy were
stale).** `cargo clippy -j 4` → **36** warnings (lib);
`cargo clippy --all-targets -j 4` → **112** warning lines across the lib and 8
test binaries; `flutter analyze` → **139** issues, of which 133 are outside the
vendored `packages/` tree. Use these, not the "~14 / ~68" figures above.

**D13 runtime baseline.** Harness: `rust/examples/crate_split_bench.rs` — a
fresh `StructureDesigner` per repetition (so evaluation is always cold and the
geometry cache never carries a result between reps), reporting min and mean.
Machine: the Windows dev box, `-j 4`, `lto = "thin"` in effect.

```text
cd rust
cargo run --release --example crate_split_bench -- \
    C:\machine_phase_systems\t_center_showcase\t_center_showcase.cnnd Main 5
cargo run --release --example crate_split_bench -- ../samples/nut-bolt.cnnd "nut bolt" 10
```

The nanobeam is the fixture D13 asks for, but it lives **outside the
repository** (the showcase was deliberately not committed), so `nut-bolt.cnnd`
is included as the in-repo fallback anyone can reproduce.

| step | nanobeam `Main` — 1,075,748 atoms / 1,951,386 bonds | `nut bolt` — 8,374 / 12,725 |
|---|---|---|
| load (`.cnnd` parse + validate) | 2.6 ms (mean 2.8) | 2.5 ms (mean 2.6) |
| evaluate (SDF/CSG + materialise) | **2,089.2 ms** (mean 2,163.6) | **11.5 ms** (mean 12.1) |
| impostor tessellation | **433.3 ms** (mean 453.7) | 1.7 ms (mean 1.8) |
| triangle-mesh tessellation | skipped¹ | **25.7 ms** (mean 26.0) |

¹ The harness skips the triangle path above 250,000 atoms: at 12×6 sphere
divisions a ball-and-stick mesh costs ~62 vertices per atom, so a million-atom
mesh measures the allocator rather than the tessellator.

**What `lto = "thin"` costs and buys, measured *before* any code moves.** D13
called this unmeasured; it no longer is. Same harness, same machine,
`CARGO_PROFILE_RELEASE_LTO=false` (i.e. cargo's pre-Phase-0 default) into a
separate target directory:

| | `lto = "thin"` | LTO off (pre-Phase-0) | delta |
|---|---|---|---|
| nanobeam evaluate (min) | 2,089.2 ms | 2,183.3 ms | **−4.3 %** |
| nanobeam impostor tessellation (min) | 433.3 ms | 525.4 ms | **−17.5 %** |
| nut-bolt evaluate (min) | 11.5 ms | 11.4 ms | noise |
| nut-bolt triangle tessellation (min) | 25.7 ms | 25.7 ms | noise |
| cold release build, lib + example | 24 m 06 s | 6 m 18 s | **3.8×** |
| warm release rebuild (touch `src/lib.rs`) | 1 m 58 s | 2 m 19 s | −15 % |

Two conclusions worth carrying into Phase 5. First, thin LTO already pays for
itself with the code still in one crate — 17.5 % off the per-atom impostor loop
— so the post-split comparison must be made against the thin-LTO column above,
never against the pre-Phase-0 build. Second, its cost is concentrated entirely
in **from-scratch** release builds; the edit-one-file-and-rebuild loop that
actually dominates development is, if anything, slightly faster. Thin LTO
stays.

### Phase 1 — `atomcad-util`

2,176 lines, no inbound dependencies. ~100 call sites. Establishes the
mechanical recipe (move, `lib.rs` façade, prefix rewrite, move tests)
on the smallest possible target.

*Estimate: 0.5 day.*

#### Phase 1 — landed 2026-08-17

**Changes.** `rust/src/util/` → `rust/crates/atomcad-util/src/` (14 files,
`mod.rs` → `lib.rs`), and `rust/tests/util.rs` + `rust/tests/util/` →
`crates/atomcad-util/tests/`, keeping the D5.1 directory-name rule
(`tests/util.rs` beside `tests/util/`) even though this crate's two `#[path]`
lines would have been cheap to rewrite — the convention is worth more than the
two lines. The crate takes `glam`, `lru` and `serde` from
`[workspace.dependencies]`; the root package gains
`atomcad-util = { path = "crates/atomcad-util" }` and loses `pub mod util;`
from `lib.rs`.

The prefix rewrite was exactly as mechanical as D3 predicts: `crate::util::` →
`atomcad_util::` in 72 source files, `crate::util::` → `crate::` inside the 14
moved files, and `rust_lib_flutter_cad::util::` → `atomcad_util::` in the 12
test files under other harnesses that reach into `util`. **`cargo build`
succeeded on the first attempt** — no visibility escalation was needed, which
confirms the "visibility is already crate-external-ready" measurement in
Current state (`util` contributed zero of the 15 `pub(crate)` items).

**No FRB exposure, confirmed rather than assumed.** `util` looked like it might
be a Dart-facing risk, because `api/api_common.rs` and `api/common_api.rs` both
import `util::transform::Transform`. It is not: `crate::api::api_common` is
*not* in `rust_input`, so its `to_api_transform` / `from_api_transform` are
never exported, and `common_api.rs` names `Transform` only in a function *body*.
The Dart-facing shape is the existing `APITransform` twin — i.e. D9a's pattern
was already in place for the one type that mattered. Regenerating bindings
produced a **byte-identical** `lib/src/rust/` and `frb_generated.rs`, and
`lib/src/rust/crystolecule/` is untouched (it is Phase 4's problem).

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo clippy -j 4`, `cargo clippy --all-targets -j 4`, `cargo fmt -- --check`,
`flutter_rust_bridge_codegen generate` + `git diff lib/src/rust/`,
`cargo build --release`. No `.snap.new` files.
**Pending manual step for the maintainer:** launch the app (`flutter run`,
release DLL) and the Flutter smoke test.

**Test count: 5,054 — identical to Phase 0**, and the `default-members`
tripwire held: `tests/util.rs`'s **15** tests are now reported under
`atomcad-util` rather than `rust_lib_flutter_cad` and did not vanish. Two other
counts moved between binaries without changing the total, which is worth
knowing before reading a future phase's table as a regression:

| binary | Phase 0 | Phase 1 |
|---|---|---|
| `rust_lib_flutter_cad` (lib unittests) | 11 | 5 |
| `atomcad-util` (lib unittests) | — | **6** |
| `tests/util.rs` (root package) | 15 | — |
| `atomcad-util` `tests/util.rs` | — | **15** |
| doc-tests `rust_lib_flutter_cad` | 18 (8 run) | 16 (6 run) |
| doc-tests `atomcad_util` | — | **2** |

(The 6 lib unittests are inline `#[cfg(test)]` modules in `util`, which
predate the `rust/AGENTS.md` "tests go in `tests/`" rule; they were moved as-is
rather than converted, so that this phase stays a pure relocation.)

**Lint baselines held exactly:** `cargo clippy -j 4` → **36** warnings in the
root lib and **0** in `atomcad-util`; `cargo clippy --all-targets -j 4` → **112**
individual warnings across the lib and 8 test binaries (one of which is now
`atomcad-util`'s); `flutter analyze` → **139**.

**Two traps worth recording for Phases 2–6.**

1. **Doc-tests are compiled as external users of their own crate.** The two
   examples in `memory_bounded_lru_cache.rs` said
   `use rust_lib_flutter_cad::util::…`; after the move that crate is not a
   dependency of `atomcad-util`, so they failed to compile — and they failed
   *last*, after `cargo build` and all 5,036 non-doc tests had already gone
   green, which is the worst moment to discover it. Any move of a
   module carrying `///` examples needs its doc-comment prefixes rewritten too;
   grep the moved files for the old crate name, not just for `crate::`.
2. **Never run `cargo fmt --all` in this workspace.** It walks into the
   vendored `../csgrs` path dependency (which is *not* a workspace member, but
   cargo-fmt reaches it anyway) and reformats it, producing a spurious diff over
   the local EPSILON patch. Plain `cargo fmt` honours `default-members`, so it
   already covers `crates/*` — use that. Recorded in `rust/AGENTS.md`.

### Phase 2 — `atomcad-geo-tree`

2,806 lines, depends on `util` only. ~52 call sites. Moves `csgrs`,
`geo`, `nalgebra` out of the root manifest.

*Estimate: 0.5 day.*

### Phase 3 — `atomcad-renderer`

5,473 lines, depends on `util` only. ~87 call sites. Moves `wgpu`,
`bytemuck`, `image` out of the root manifest. First phase after which
some crates build without a GPU stack.

*Estimate: 1 day.*

### Phase 4 — `atomcad-crystolecule`

24,996 lines plus 29,166 test lines; ~250 call sites. Includes D6:
`SelectModifier` and `AtomicStructureVisualization` move down, each
gaining a D9a twin. Note D6's split rule — only
`AtomicStructureVisualization` leaves the preferences module here; the
other 12 types stay until Phase 6.

This is the first phase whose tests depend on shared infrastructure,
so it also creates `atomcad-test-support` (D5.2) and converts the 31
fixture call sites to `fixture_path` (D5.3). Doing the conversion for
*all* 15 files here — including the ones whose tests stay at the root
— keeps the fixture-addressing convention uniform and means Phase 6
inherits no fixture work.

**Gate:** this phase is the first to move a Dart-facing type across a
crate boundary, so it validates D9a's twin pattern end-to-end on
`SelectModifier` before Phase 6 depends on it. Run
`flutter_rust_bridge_codegen generate`, then apply the D11
verification (Dart `enum` preserved under its original name, no stray
opaque class, no new generated directory) and `git diff lib/src/rust/`
to confirm the generated Dart is unchanged apart from intended edits.
The one expected extra diff is `DrawingPlane` / `UnitCellStruct` losing
or relocating their generated files — see D6a, and record there which
of the two happened.

*Estimate: 2–2.5 days* — the largest of Phases 1–5, because it carries
the test-support crate, the 31 fixture conversions, and the D9a
validation in addition to the move itself.

### Phase 5 — `atomcad-display`

4,063 lines; ~65 call sites. Includes D7: `scene_tessellator.rs` moves
up into what is still the monolith, and `param_atomic_number_to_index`
moves down into `atomcad-crystolecule`.

**Gate:** re-run the D13 benchmark. All five lower crates have now
left the root, so cross-crate inlining exposure is at its maximum —
this is the measurement that decides whether `lto = "thin"` suffices.

*Estimate: 1 day.*

### Phase 6 — `atomcad-structure-designer` (with `expr`)

The largest phase, split into four commits:

1. **D9.1 + D9.2** — move `NodeTypeCategory` and
   `structure_designer_preferences.rs` down; ~130 mechanical
   reference updates, plus the D9a twins and `From` impls for the 14
   Dart-facing types involved — **each twin keeping the type's existing
   name** (D9a). Also retarget the 21 test files D5.1a identifies as
   self-resolving onto the new domain paths: the api twin still exists,
   so they would compile unchanged here and only fail in commit 3 when
   they move — doing it now keeps that commit purely mechanical.
   Regenerate FRB bindings and apply the generated-Dart diff check; the
   Dart symbol set should come out byte-identical, since every twin
   keeps both its name and its file. (~1.5 days)
2. **D10 + D10.1** — move the five view-builder methods up into
   `api/`, adding the ~3 accessors they need; move `cli_runner.rs` and
   `get_active_tool()` up; twin `DragFrozenStatus`. This is the only
   genuine refactor in the whole design. (~1.5 days)
3. **Cut the crate** — `structure_designer/` and `expr/` (D8) move to
   `crates/atomcad-structure-designer/src/`; the root crate is left
   with `api/` and `frb_generated.rs`. Its tests move per D5.1 **and
   D5.1a**: partition `tests/structure_designer.rs` into a crate-side
   harness (~162 files, directory name preserved) and a root-side
   `tests/structure_designer_api.rs` (the ten api-level files), split
   `function_pin_test.rs`, and divide
   `tests/structure_designer/snapshots/` between the two per D5.4.
   Fixture addressing already came over in Phase 4. Drive the partition
   by compiling, not by grepping — cut first, then move whatever fails
   to build. (~1.5 days)
4. **Cleanup** — move the cross-layer test files identified in D5 to
   the root `tests/` tree; delete the `#[cfg(not(frb_expand))]` guard
   at `lib.rs:8` and, with it, the now-pointless
   `check-cfg = ['cfg(frb_expand)']` entry in `[lints.rust]`
   (`rust/Cargo.toml:53-54`). (~0.5 day)

*Estimate: 4.5–5 days.*

### Phase 7 — Documentation

- `doc/architecture_overview.md`: describe crates rather than modules;
  record `expr` as a component of `structure_designer` (D8); state
  that the DAG is now compiler-enforced.
- `doc/architecture_diagram.svg`: redraw to match.
- `AGENTS.md` (root): every `rust/src/…` path changes. Specifically
  the "Subdirectory Instructions" list (lines 7–13, covering
  `crystolecule/`, `crystolecule/simulation/`,
  `crystolecule/simulation/uff/`, `geo_tree/`, `structure_designer/`,
  `structure_designer/undo/`), the architecture diagram block
  (line 41), the FFI-regeneration note (line 71), the Flutter Rust
  Bridge section (line 117), both "Adding Features" recipes (lines
  123, 127), and the `nodes/AGENTS.md` reference (line 182). The
  per-directory `AGENTS.md` files move with their crate.
- `rust/AGENTS.md`: document the workspace layout, the
  `[workspace.dependencies]` rule including the `csgrs` path anchoring
  (D4), the `default-members` requirement and why `cargo test` in
  `rust/` would otherwise cover only the root package (D1), the
  FRB-confinement invariant (D11), the down-vs-up test for placing a
  type (D10), and the twin pattern for down-moved Dart-facing types
  (D9a) — including the naming rule, that the twin keeps the existing
  identifier and is *not* renamed to `API…`. Also record the two
  FRB facts most likely to cost a contributor a day: bare `#[frb]` on
  a type is a no-op, and an unresolvable type degrades to an opaque
  handle silently.
- `doc/testing.md`: document `cargo test -p <crate>`, and that
  `structure_designer` tests now live in two harnesses — the domain
  ones in the crate, the api-level ones at
  `rust/tests/structure_designer_api/` (D5.1a) — so a new test's home
  depends on whether it touches `api`.

*Estimate: 0.5 day.*

**Total: roughly 10.5–11.5 working days** (0.5 + 0.5 + 0.5 + 1 + 2–2.5 +
1 + 4.5–5 + 0.5), of which **Phases 0–5 (~5.5–6 days)** deliver the
clean lower half and are useful on their own if Phase 6 is deferred.

These estimate the *edit* work. The binding constraint in practice is
the compile-verify loop: ~146k source and ~157k test lines, a 16 GB
`target/`, and `cargo test` needing `-j 4` on the Windows dev machine
to avoid exhausting the pagefile. Phases 4 and 6 involve many
sed-then-rebuild cycles; plan for up to 1.5× on those two.

Phases 1–3 are small enough to slot between feature work. **Phases 4
and 6 relocate hundreds of files and will conflict with any long-lived
branch — schedule them into a quiet window.**

## Deferred / follow-ups

- **Splitting `nodes/` from `node_type_registry`.** The highest
  remaining build-time prize (39,964 lines of node implementations
  versus a 4,129-line registry), but it requires replacing static
  registration with an inventory or trait-object mechanism. Worth a
  design of its own if backend compile time becomes a real
  complaint; not justified by this one.
- **Making `expr` independent** by generalising it over a value trait
  instead of `NetworkResult` (D8). Would let the expression language
  be tested and reused without the node network.
- **`atomcad-api-types` as a separate crate.** If `api/` (17,500
  lines) later grows unwieldy, the DTO layer could split from the API
  functions. Not worth it at current size.
- **Extracting `atomcad-crystolecule` to its own repository.** After
  Phase 4 it is a genuinely standalone atomic-structure library with
  no atomCAD-specific dependencies. Out of scope here, but this
  design deliberately does not foreclose it.
- **Per-crate `clippy` lint levels.** The workspace makes it possible
  to hold new crates to a stricter standard than the ~14-warning
  baseline. Not attempted during the move, so that lint churn never
  obscures a relocation error.
