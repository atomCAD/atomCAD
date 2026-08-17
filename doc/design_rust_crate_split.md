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
    Phase 1 and after Phase 5. **D13** — *measured, Phase 5:* with
    `lto = "thin"` the six-crate build is at parity with the monolith;
    **with LTO off it is 2.1× slower**. Never delete that profile line.

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

**Outcome (Phase 4, 2026-08-17): they vanish** — the preferred branch.
`lib/src/rust/crystolecule/` is deleted and codegen does not recreate it.
One procedural note for anyone re-deriving this: **codegen does not remove
stale output**, so the first regeneration leaves both files sitting there
looking unchanged, and `git diff` reports nothing. The check that actually
answers the question is to delete them and re-run codegen — if they come
back, the type relocated its Dart output; if they do not, it is gone. They
did not come back, and nothing under `lib/` ever imported them.

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
- **`cargo test -p <crate>` must pass for each extracted crate**, not only
  `cargo test -j 4`. Added after Phase 4, where the workspace-wide run was
  green while the per-crate run failed: extracting a crate is the first time
  its *feature* graph is exercised in isolation, and `atomcad-crystolecule`'s
  documented 64-byte `Atom` turned out to depend on `smallvec/union`, a
  feature enabled only by `wgpu-hal`. Any invariant resting on a cargo
  feature — layout, `no_std`-ness, a numeric backend — can have been
  satisfied transitively by a dependency the new crate no longer pulls in,
  and the workspace-wide command cannot see it.
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

#### Phase 2 — landed 2026-08-17

**Changes.** `rust/src/geo_tree/` → `rust/crates/atomcad-geo-tree/src/`
(9 files, `mod.rs` → `lib.rs`, plus the module's `AGENTS.md`/`CLAUDE.md`), and
`rust/tests/geo_tree.rs` + `rust/tests/geo_tree/` →
`crates/atomcad-geo-tree/tests/`, keeping the D5.1 directory-name rule. The
crate takes `atomcad-util` plus `blake3`, `csgrs`, `geo`, `glam`, `nalgebra`
and `rayon` from `[workspace.dependencies]`; the root package gains
`atomcad-geo-tree = { path = "crates/atomcad-geo-tree" }` and loses
`pub mod geo_tree;` from `lib.rs`. `csgrs` resolved correctly from
`[workspace.dependencies]` exactly as D4 predicts — the member manifest writes
only `csgrs = { workspace = true }` and `../csgrs` stayed anchored to the
workspace root.

The prefix rewrite was again purely mechanical: `crate::geo_tree::` →
`atomcad_geo_tree::` in 41 source files, `crate::geo_tree::` → `crate::` inside
the 4 moved files that self-reference, and `rust_lib_flutter_cad::geo_tree::` →
`atomcad_geo_tree::` in the 19 test files under other harnesses. **`cargo build`
succeeded on the first attempt**; `geo_tree` contributed zero of the 15
`pub(crate)` items, so again no visibility escalation. Phase 1's doc-test trap
did not bite: `src/geo_tree/` contained no `///` examples and no
`rust_lib_flutter_cad` mentions at all (checked before moving, not after).

**Only `rayon` actually left the root manifest, not the three deps the phase
description names.** `csgrs`, `geo` and `nalgebra` are named *directly* by
`src/display/csg_to_poly_mesh.rs:7-11`, which stays in the root crate until
Phase 5 extracts `atomcad-display`. `blake3` likewise stays, for
`structure_designer/nodes/export_atoms.rs:261`. So the "Moves `csgrs`, `geo`,
`nalgebra` out of the root manifest" line above is a **Phase 5** outcome; Phase
2 only makes them *shared*. `rayon`, whose only user in the whole tree was
`geo_tree`, was removed from the root `[dependencies]` — the one genuine
manifest shrink here. A comment in `rust/Cargo.toml` records why the other
three stayed, so a later reader does not take their presence for an oversight.

**One non-relocation edit was unavoidable.** `mod.rs:3` carried `use blake3;`,
which is inert inside a module but is a redundant single-component import at a
*crate root* — clippy's `single_component_path_imports` fires on it, and the
line had to go for the lint baseline to hold. `blake3::` still resolves from the
extern prelude, so nothing else changed. This is the first case of the
mod.rs → lib.rs promotion changing what a lint sees; expect the same class of
warning in later phases and treat it as part of the move rather than as new
debt.

**The `#[path]` back-reference D5 warns about was incidental here.**
`tests/geo_tree/batched_implicit_evaluator_test.rs:4` imported `BATCH_SIZE`,
`ImplicitGeometry2D` and `ImplicitGeometry3D` from
`rust_lib_flutter_cad::structure_designer::implicit_eval::implicit_geometry`.
That module is a one-line `pub use` of `crate::geo_tree::implicit_geometry`, so
the up-reach was pure re-export and the fix was to import from
`atomcad_geo_tree::implicit_geometry` directly. This is the "the dependency is
incidental and gets dropped" branch of D5; no geo_tree test had to be left
behind at the root.

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo clippy -j 4`, `cargo clippy --all-targets -j 4`, `cargo fmt -- --check`,
`flutter_rust_bridge_codegen generate` + `git diff lib/src/rust/`,
`flutter analyze`, `cargo build --release`, and cargokit (`flutter build windows
--debug` rebuilt `rust_lib_flutter_cad.dll` and linked `atomCAD.exe`). No
`.snap.new` files. `git status` shows `../csgrs` untouched.
**Pending manual step for the maintainer:** launch the app (`flutter run`,
release DLL) and the Flutter smoke test.

**Test count: 5,054 — identical to Phases 0 and 1**, under both `cargo test
-j 4` and `cargo test --workspace -j 4`. `tests/geo_tree.rs`'s **66** tests
moved intact and are now reported under `atomcad-geo-tree`:

| binary | Phase 1 | Phase 2 |
|---|---|---|
| `tests/geo_tree.rs` (root package) | 66 | — |
| `atomcad-geo-tree` `tests/geo_tree.rs` | — | **66** |

Every other binary's count is unchanged. Unlike Phase 1, no lib unittests or
doc-tests moved: `geo_tree` had neither.

**Lint baselines held exactly:** `cargo clippy -j 4` → **36** warnings in the
root lib, **0** in `atomcad-geo-tree` and `atomcad-util`;
`cargo clippy --all-targets -j 4` → **112** individual warnings (36 lib + 61
`structure_designer` + 5 `crystolecule` + 4 `expr` + 4 `geo_tree` + 1 `display`
+ 1 `util`); `flutter analyze` → **139** issues.

**Generated bindings: zero content change** — `git diff --numstat lib/src/rust
rust/src/frb_generated.rs` is empty, `lib/src/rust/crystolecule/` is untouched
(Phase 4's problem), and no directory appeared or disappeared under
`lib/src/rust/`. `geo_tree` was never FRB-reachable: no `api/` file names it.

**A regeneration artefact worth knowing before Phase 4's gate.**
`flutter_rust_bridge_codegen generate` runs its own `rustfmt` over
`frb_generated.rs` *without* the 2024 style edition, so it rewrites three
`use flutter_rust_bridge::for_generated::{…}` lines into 2015 import order and
leaves the file dirty even when nothing semantic changed. Plain `cargo fmt`
puts them back. **Run `cargo fmt` after codegen and before reading the diff**,
or the gate reports a spurious three-line change every time. Separately, all
six generated files always show as ` M` in `git status` after codegen because
it writes LF where the index holds CRLF; `git diff --numstat` (which normalises)
is the check to trust, not `git status`.

### Phase 3 — `atomcad-renderer`

5,473 lines, depends on `util` only. ~87 call sites. Moves `wgpu`,
`bytemuck`, `image` out of the root manifest. First phase after which
some crates build without a GPU stack.

*Estimate: 1 day.*

#### Phase 3 — landed 2026-08-17

**Changes.** `rust/src/renderer/` → `rust/crates/atomcad-renderer/src/`
(22 files, `mod.rs` → `lib.rs`; 6 of them `.wgsl` shader sources, plus the
`tessellator/` subdirectory's 3), and `rust/tests/renderer.rs` +
`rust/tests/renderer/` → `crates/atomcad-renderer/tests/`, keeping the D5.1
directory-name rule. The crate takes `atomcad-util` plus `bytemuck`, `glam`,
`image` and `wgpu` from `[workspace.dependencies]`; the root package gains
`atomcad-renderer = { path = "crates/atomcad-renderer" }` and loses
`pub mod renderer;` from `lib.rs`.

`~87 call sites` was exact: of the 113 `crate::renderer::` occurrences, 26 were
internal self-references (rewritten to `crate::`) and **87** were external
(rewritten to `atomcad_renderer::`) across 26 source files. Another 7 files —
6 test files under the `display` / `structure_designer` harnesses plus
`examples/crate_split_bench.rs` — had
`rust_lib_flutter_cad::renderer::` → `atomcad_renderer::`. `renderer`
contributed zero of the 15 `pub(crate)` items, so once again no visibility
escalation was needed. Phase 1's doc-test trap did not bite: the only fenced
block in the module is a ` ```text ` diagram in `camera.rs`, and no file
mentioned `rust_lib_flutter_cad` (both checked before moving).

**Only `bytemuck` actually left the root manifest, not the three deps the phase
description names** — the same correction Phase 2 needed. `src/api/screenshot_api.rs`
reads the rendered texture back with `wgpu` and encodes the PNG with `image`, so
those two stay in the root `[dependencies]` and are now shared. `bytemuck` had no
user outside `renderer/` and was removed. A comment in `rust/Cargo.toml` records
why the other two stayed. The claim "first phase after which some crates build
without a GPU stack" was **already true after Phase 1**: `wgpu` was only ever a
dependency of the root package, so `cargo test -p atomcad-util` never built it.
What Phase 3 actually changes is that the GPU stack is now *owned* by one crate
rather than by the 191k-line monolith.

**The one genuinely non-mechanical problem was a committed asset, not code.**
`label_atlas.rs` embeds the SDF font atlas with
`include_bytes!("../../assets/font_atlas.png")`. That path is resolved against
the *source file*, so extracting the crate reparents it by two levels and it
silently means something else. `rust/assets/` (the atlas, `DejaVuSans-Bold.ttf`
and its license) therefore moved **into** the crate as
`crates/atomcad-renderer/assets/`, and the path became `../assets/…`. Two things
travelled with it, because they address the same files through
`CARGO_MANIFEST_DIR`:

- `examples/gen_font_atlas.rs`, the offline generator, → `crates/atomcad-renderer/examples/`,
  with `manifest_dir.join("src/renderer/font_metrics.rs")` becoming
  `…join("src/font_metrics.rs")` and its usage line becoming
  `cargo run --release -p atomcad-renderer --example gen_font_atlas` (the `-p`
  is now required: `cargo run` in a root-package workspace defaults to the root).
- the `ab_glyph` dev-dependency, which that example is the sole user of. It is
  the only dependency this phase moved wholesale.

The alternative — leaving `assets/` at `rust/assets/` and writing
`../../../assets/…` — works but re-encodes the crate's depth in the tree, the
same objection D4 raises against inlining `csgrs`'s path in a member manifest.
Keeping the asset with the code that embeds it also keeps `atomcad-renderer`
self-contained, which matters for the same reason it does for
`atomcad-crystolecule` (see Deferred / follow-ups).

**Verification worth copying in later phases:** re-running the generator
in place produced a **byte-identical** `assets/font_atlas.png` and
`src/font_metrics.rs`. `cargo build` proves the *read* path
(`include_bytes!`); only re-running the writer proves the *write* path, and a
committed generated artifact has one of each.

This trap will not recur: after Phase 3 there is **no** `include_bytes!` or
`include_str!` left anywhere in `rust/src/` or `rust/tests/`, so Phases 4–6 have
no embedded assets to reparent. The `CARGO_MANIFEST_DIR`-relative *runtime*
paths D5.1 and D5.3 deal with are a separate mechanism and still apply.

Beyond the asset, `cargo build` succeeded on the first attempt.
`rustfmt` did have to reflow four files inside the crate
(`gpu_mesh.rs`, `renderer.rs`, `label_atlas.rs`, `occludable_mesh.rs`) and two
outside it (`api/common_api.rs`, `display/atomic_tessellator.rs`), because
`crate::renderer::X` and `atomcad_renderer::X` are different widths and several
call sites sat near the 100-column limit. That is expected of every phase from
here on: **run `cargo fmt` as part of the move, not as an afterthought**, or
`cargo fmt -- --check` fails on a purely mechanical rewrite.

**`camera_test.rs` had to be split — the first instance of D5.1a's pattern
outside `structure_designer`.** D5 lists `renderer/camera_test.rs` among the 6
files that "cannot travel downward with their crate" and leaves the treatment to
the implementor. Neither D5 branch was right as stated: the up-reach is not
incidental (the file genuinely exercises `api::common_api`'s
`resolve_miller_plane_up` / `resolve_lattice_direction_up` / `drawing_plane_up`
against `crystolecule`'s `DrawingPlane` and `UnitCellStruct`), but moving the
whole file up would have stranded 12 pure `Camera`-math tests above the crate
that owns `Camera`. The file already had a clean seam — a
`// --- Phase 2: axis resolution helpers ---` banner separating the two halves,
and the api-side half touches no `Camera` at all — so it was cut there:

```
crates/atomcad-renderer/tests/renderer/camera_test.rs   # 12 camera-math tests
rust/tests/renderer_api.rs                              # new root harness
rust/tests/renderer_api/camera_axis_resolution_test.rs  # the 4 api-level tests
```

The root harness is named after D5.1a's `structure_designer_api` precedent, so
the convention is established before Phase 6 needs it at scale: **a test's home
is decided by what it imports, not by what it is about.** The other four
`tests/renderer/` files reach no further than `atomcad_renderer` and moved
intact.

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo clippy -j 4`, `cargo clippy --all-targets -j 4`, `cargo fmt -- --check`,
`flutter_rust_bridge_codegen generate` + `git diff --numstat lib/src/rust`,
`flutter analyze`, `cargo build --release`,
`cargo run --release -p atomcad-renderer --example gen_font_atlas` (byte-identical
output), and cargokit (`flutter build windows --debug` rebuilt
`rust_lib_flutter_cad.dll` and linked `atomCAD.exe`). No `.snap.new` files.
`Cargo.lock` gained only the `atomcad-renderer` package node and moved
`ab_glyph` / `bytemuck` between packages — no version resolution changed.
`git status` shows `../csgrs` untouched.
**Pending manual step for the maintainer:** launch the app (`flutter run`,
release DLL) and the Flutter smoke test.

**Test count: 5,054 — identical to Phases 0, 1 and 2** (5,040 passed, 14
ignored), under both `cargo test -j 4` and `cargo test --workspace -j 4`. The
42 renderer tests split across the harness boundary rather than moving as a
block, which is the number to recognise if a later phase reads this table:

| binary | Phase 2 | Phase 3 |
|---|---|---|
| `tests/renderer.rs` (root package) | 42 | — |
| `atomcad-renderer` `tests/renderer.rs` | — | **38** |
| `tests/renderer_api.rs` (root package) | — | **4** |

Every other binary's count is unchanged. Like `geo_tree`, `renderer` had no lib
unittests and no doc-tests, so nothing moved between those buckets.

**Lint baselines held exactly:** `cargo clippy -j 4` → **36** warnings in the
root lib, **0** in `atomcad-renderer`, `atomcad-geo-tree` and `atomcad-util`;
`cargo clippy --all-targets -j 4` → **112** individual warnings (36 lib + 61
`structure_designer` + 5 `crystolecule` + 4 `expr` + 4 `geo_tree` + 1 `display`
+ 1 `util`, with the new `renderer` and `renderer_api` binaries and the moved
`gen_font_atlas` example contributing zero); `flutter analyze` → **139** issues.
Unlike Phase 2, the `mod.rs` → `lib.rs` promotion introduced no new lint: the
`#![allow(clippy::module_inception)]` that `mod.rs` carried for
`renderer::renderer` simply became crate-level and now also covers
`tessellator::tessellator`, whose own `#![allow]` is left in place as redundant
but harmless.

**Generated bindings: zero content change.**
`git diff --numstat lib/src/rust rust/src/frb_generated.rs` is empty, no
directory appeared or disappeared under `lib/src/rust/`, and
`lib/src/rust/crystolecule/` is still untouched (Phase 4's problem).
`renderer` looked like it might be FRB-reachable, since `api/common_api.rs` —
which *is* in `rust_input` — both imports `Renderer` and names
`CameraCanonicalView` in function signatures. It is not: `Renderer` appears only
in a function body (`:120`), and the two `CameraCanonicalView` converters
(`to_renderer_camera_canonical_view`, `to_api_camera_canonical_view`) are
private `fn`, so neither type is reachable from an exported signature. The
Dart-facing shape is the existing `APICameraCanonicalView` twin — D9a's pattern
was, as in Phase 1, already in place for the one type that mattered.
`frb_generated.rs` contains zero occurrences of `renderer::`.

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

#### Phase 4 — landed 2026-08-17

**Changes.** `rust/src/crystolecule/` → `rust/crates/atomcad-crystolecule/src/`
(53 files, `mod.rs` → `lib.rs`, plus the three `AGENTS.md`/`CLAUDE.md` pairs), and
`rust/tests/crystolecule.rs` + `rust/tests/crystolecule/` →
`crates/atomcad-crystolecule/tests/`, keeping the D5.1 directory-name rule. The
crate takes `atomcad-util` and `atomcad-geo-tree` plus `glam`, `indexmap`,
`lazy_static`, `rustc-hash`, `serde`, `smallvec` and `thiserror` from
`[workspace.dependencies]`, with `nalgebra` / `serde_json` / `tempfile` as
test-only dev-deps; the root package gains
`atomcad-crystolecule = { path = "crates/atomcad-crystolecule" }` and loses
`pub mod crystolecule;` from `lib.rs`. `rust/tests/test_support/` became
`crates/atomcad-test-support/` (D5.2), and all 31 fixture sites across 15 files
now go through its resolver (D5.3). No dependency left the root manifest this
phase — `crystolecule` shared everything it used.

`~250 call sites` was close: of the 364 `crate::crystolecule::` occurrences, 96
were internal self-references (rewritten to `crate::`) and **268** were external
(rewritten to `atomcad_crystolecule::`) across 30 source files; another 153
occurrences of `rust_lib_flutter_cad::crystolecule::` were rewritten across 64
test files and `examples/crate_split_bench.rs`. `crystolecule` held 10 of the 15
`pub(crate)` items measured in Current state, and **not one of them needed
escalating** — they were all `pub(crate)` *within* what is now the crate, so the
boundary landed outside them. `cargo build` succeeded on the first attempt after
the two down-moves were in place.

**Phase 1's doc-test trap bit, and was caught before it could.**
`unit_cell_symmetries.rs:458` carries a ```` ```rust ```` example importing
`rust_lib_flutter_cad::crystolecule::…`, which after the move is not a dependency
of the crate that defines it. Grepping the moved files for the old crate name
*before* moving (as Phase 1 advises) found it in seconds; discovered the other
way round it would have failed last, after 5,000 green tests.

**D6 — the two down-moves.** `SelectModifier` now lives in
`atomic_structure/mod.rs` (beside `apply_select_modifier`, its only in-crate
consumer) and `AtomicStructureVisualization` in a new `visualization.rs`. Both
keep a same-named twin in the `api/` file they occupied before, with four `From`
impls each (owned + borrowed, both directions) declared **in the api file**,
where both types are in scope — the orphan rule allows
`impl From<&LocalApiType> for ForeignDomainType` because the parameter type is
local. Conversion happens at the `api/` boundary and nowhere else: **seven** call
sites, which is exactly what the compiler asked for and the whole cost of the
back-edge deletion.

Three consequences worth carrying forward:

- **`display::preferences` had a *third* copy of
  `AtomicStructureVisualization`,** which D6 does not mention. The tree therefore
  held two identical two-variant enums (`api/` and `display/`) and none in the
  domain — which is precisely *why* `crystolecule` reached up into `api/`: the
  copy it needed was above it and the other was beside it. Rather than add a
  fourth, `display::preferences` now re-exports the domain one
  (`pub use atomcad_crystolecule::visualization::AtomicStructureVisualization;`).
  This is a deliberate deviation from a literal reading of D6/D9a — the design
  says "declare a twin", and here one of the would-be twins already existed and
  was redundant. Net effect: 3 copies → 2 (domain + Dart-facing twin), the ~8
  `api → display_prefs` match blocks in `structure_designer` keep compiling
  unchanged because their target type is now the domain type, and one of FRB's
  silent "multiple objects with same key … randomly pick one" collisions
  disappeared from the codegen log. Five such collisions remain, all pre-existing
  `api/`-vs-`display/` preference-struct duplicates; they are Phase 5/6's to
  resolve.
- **Every `hit_test` call site already had the converted value in hand.** All 11
  computed a `display_visualization` local one line earlier and then passed the
  *api*-typed `visualization` to `hit_test`; the edit was uniformly
  `visualization,` → `&display_visualization,`. No new conversion code.
- **`raytrace` / `raytrace_per_node` now take the domain type.** Both only ever
  forward it to `hit_test`, so their internal api→display match blocks became
  identity and were deleted, and their three api callers convert instead. That is
  what lets `tests/structure_designer/raytrace_per_node_test.rs` drop its `api`
  import onto the domain path — one of the 21 files D5.1a expects to
  self-resolve, resolved a phase early and verified by the compiler rather than
  predicted.

**D5's "cannot move down" list came out at zero for this phase.** Of the six
files D5 names, `crystolecule/atomic_structure_test.rs` and
`display/atomic_render_style_test.rs` were live here, and **both** turned out to
be the "dependency is incidental and gets dropped" branch — each reached up only
for a type this phase moved down. `atomic_structure_test.rs` switched its one
import and travelled with the crate; `atomic_render_style_test.rs` had imported
*both* the api enum (for `hit_test`) and the display enum (for prefs), and since
those are now the same type its `AtomicStructureVisualization as ApiVisualization`
alias was deleted outright. So the whole `tests/crystolecule/` harness moved
intact — no `crystolecule_api.rs` counterpart to `renderer_api.rs` was needed,
and the D5.1 "zero edits" claim held for all 42 test files and all six UFF
`CARGO_MANIFEST_DIR` test-data paths.

**The one genuinely serious problem: a memory-layout invariant was riding on the
GPU driver's dependency list.** `cargo test -p atomcad-crystolecule` — the
capability D12 advertises as a headline gain — failed one test:

```text
atom_tags_test::atom_is_still_one_cache_line
  assertion `left == right` failed: tag_bits must fit in Atom's existing
  padding — Atom stays 64 bytes
  left: 72   right: 64
```

`Atom` holds `bonds: SmallVec<[InlineBond; 4]>`, and `smallvec`'s **`union`**
feature is what stores that inline buffer in a union and keeps `Atom` at one
cache line. `cargo tree -p smallvec --invert -e features` shows exactly one crate
in the entire tree asking for it: **`wgpu-hal`**. The invariant the test guards —
documented in `AGENTS.md` and in the module's own header — has therefore been
enabled *by accident*, by the graphics backend, and it held only because until
Phase 4 there was no way to build `crystolecule` without the GPU stack in the
graph. The fix is to declare it where it belongs:

```toml
smallvec = { version = "1.13", features = ["union"] }
```

This changes nothing about the shipped binary (the feature was already on in
every real build; `Cargo.lock` is unchanged apart from the two new package nodes,
with **zero** removed lines) and makes the layout a property of our own manifest.
Two lessons generalize past this phase:

1. **Extracting a crate is the first time its feature graph is tested in
   isolation.** Any invariant that depends on a cargo feature — layout,
   `no_std`-ness, a numeric backend — may have been satisfied transitively.
   Phases 5 and 6 should run `cargo test -p <crate>` explicitly, not just
   `cargo test -j 4`: the workspace-wide run **passed** while the per-crate run
   failed, so the regression gate as written would not have caught this.
2. A test asserting a layout is worth its weight. Without
   `atom_is_still_one_cache_line`, `Atom` would silently have become 72 bytes for
   anyone building the crate standalone — a 12 % memory regression on the
   1M-atom path, reported by nothing.

**D13 interim reading — do not carry the numbers into Phase 5, but do carry the
method.** The benchmark is Phase 5's gate, not this phase's, but the harness was
smoke-run to prove it survived the move (`examples/crate_split_bench.rs` needed
only the prefix rewrite). On `nut-bolt.cnnd`, 10 reps, `lto = "thin"`, it reports
**evaluate 13.5 ms / impostors 2.0 ms / triangles 32.7 ms** against Phase 0's
**11.5 / 1.7 / 25.7** — a uniform ~20 % worse, which looks alarming and is not
what it appears to be. Two independent arguments say so:

- **Turning LTO off changes nothing.** `CARGO_PROFILE_RELEASE_LTO=false` into a
  scratch target dir gives **13.3 / 2.2 / 32.1** — within noise of the thin-LTO
  run. Cross-crate inlining loss is *precisely* what thin LTO claws back, so if
  the split were the cause, removing LTO would have made it markedly worse. It
  did not. Whatever moved, it is not inlining across the four new crate
  boundaries.
- **The most-regressed path has the least Phase 4 exposure.** `triangles`
  (+27 %) runs `display::atomic_tessellator` → `atomcad_renderer::tessellator`, a
  boundary that has existed since Phase 3 and that this phase did not touch;
  `evaluate` (+17 %) is the path that actually crosses the new
  `atomcad-crystolecule` boundary. A regression caused by the split would be
  ordered the other way round.

The conclusion is that the machine is simply ~20 % slower than during the Phase 0
session, and that `nut-bolt` — 8 k atoms, sub-35 ms steps — is too small to
separate a real effect from that, exactly as Phase 0 already found when it labeled
the nut-bolt LTO deltas "noise". **Phase 5 must run its gate on the 1.07 M-atom
nanobeam and re-baseline Phase 0's own numbers on the same machine session**,
rather than comparing against a table measured months earlier; otherwise the gate
will fire on drift. The LTO-on/LTO-off pair is the cheap sanity check to run
first — it costs one build and it separates "inlining" from "everything else"
without needing a pre-split control build at all.

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo test -p atomcad-crystolecule -j 4`, `cargo clippy -j 4`,
`cargo clippy --all-targets -j 4`, `cargo fmt -- --check`, `cargo metadata` (the
D5.2 dev-dependency cycle resolves without error, as predicted),
`flutter_rust_bridge_codegen generate` + `git diff --numstat lib/src/rust
rust/src/frb_generated.rs`, `flutter analyze`, `cargo build --release`, and
cargokit (`flutter build windows --debug` rebuilt `rust_lib_flutter_cad.dll` and
linked `atomCAD.exe`). No `.snap.new` files; `git status` shows `../csgrs`
untouched.
**Pending manual step for the maintainer:** launch the app (`flutter run`,
release DLL) and the Flutter smoke test.

**Test count: 5,054 — identical to Phases 0–3** (5,040 passed, 14 ignored), under
both `cargo test -j 4` and `cargo test --workspace -j 4`. The 1,171 crystolecule
tests and one inline lib unittest moved intact:

| binary | Phase 3 | Phase 4 |
|---|---|---|
| `tests/crystolecule.rs` (root package) | 1,171 | — |
| `atomcad-crystolecule` `tests/crystolecule.rs` | — | **1,171** |
| `rust_lib_flutter_cad` (lib unittests) | 5 | 4 |
| `atomcad-crystolecule` (lib unittests) | — | **1** |
| doc-tests `rust_lib_flutter_cad` | 16 (6 run) | 15 (5 run) |
| doc-tests `atomcad_crystolecule` | — | **1** |

Every other binary is unchanged. One near-miss worth recording, because it makes
the tripwire slightly sharper than the design describes: the `fixture_path`
doc-comment example was first written as a ```` ```ignore ```` fence, which
rustdoc counts as an **ignored doc-test** and pushed the total to 5,055. A
*rising* count is as much a signal as a falling one — the example is a ```` ```text ````
block now, and the number is 5,054 on the nose.

**Lint baselines held exactly:** `cargo clippy -j 4` → **36** warnings
(20 root lib + 16 `atomcad-crystolecule`, which is where those 16 have always
lived — they simply had a different owner before), **0** in the other three
crates; `cargo clippy --all-targets -j 4` → **112** individual warnings
(20 lib + 61 `structure_designer` + 16 `crystolecule` lib + 5 `crystolecule`
tests + 4 `expr` + 4 `geo_tree` + 1 `display` + 1 `util`); `flutter analyze` →
**139** issues. Unlike Phase 2, the `mod.rs` → `lib.rs` promotion introduced no
new lint.

**Generated Dart: the D9a gate passes, and D6a's preferred outcome occurred.**
`git diff --numstat lib/src/rust rust/src/frb_generated.rs` touches exactly two
files, `+8/-1` and `+12/-1`:

- `enum SelectModifier` is still a Dart `enum`, still named `SelectModifier`,
  still in `lib/src/rust/api/common_api_types.dart`. Same for
  `enum AtomicStructureVisualization` in
  `.../structure_designer_preferences.dart`. No
  `abstract class … implements RustOpaqueInterface` anywhere, no new directory
  under `lib/src/rust/`, no renamed symbol, and the Dart symbol *set* is
  unchanged.
- The entire content of those diffs is (a) the doc comments written on the two
  twins, which FRB propagates into the generated Dart, and (b) four/five new
  `from` entries in the "these functions are ignored because they are on traits
  not defined in current crate" header comment — the direct, benign trace of the
  `From` impls. **`rust/src/frb_generated.rs` is byte-identical.**
- **D6a: the files vanish.** `lib/src/rust/crystolecule/drawing_plane.dart` and
  `unit_cell_struct.dart` are deleted. This was confirmed rather than assumed:
  codegen does not remove stale output, so the first regeneration left them
  in place looking unchanged; deleting them and re-running codegen confirmed they
  are **not** re-emitted. They were already dead — each held a bare
  `abstract class … implements RustOpaqueInterface {}`, nothing under `lib/`
  imported them, and no generated Dart file referenced the classes (they date
  from `0d45d315`, "view up axis phase 2"). `lib/src/rust/crystolecule/` is gone,
  which D6a names as the expected end state.

**Back-edge audit.** `crystolecule → api` is **0** references (the only remaining
mention of `crate::api` under the crate is a sentence in its `AGENTS.md`
explaining why it used to be there) — and it is now a *build failure* to
reintroduce, since the root crate is not in `atomcad-crystolecule`'s manifest.
Two of the four back-edges in Current state are closed (`geo_tree`'s incidental
one in Phase 2, `crystolecule`'s here); `display → structure_designer` is
Phase 5's and `structure_designer → api` is Phase 6's.

That last edge now measures **145 sites across 123 files** — *higher* than the
131/125 in Current state, which is worth stating plainly so a Phase 6 implementor
does not read it as this phase having made things worse. The design's figure
predates several intervening features; nothing here added an api reference. The
composition is what matters and it is unchanged in shape:

| type | sites | handled by |
|---|---|---|
| `NodeTypeCategory` | 113 | D9.1 |
| `AtomicStructureVisualization` | 6 | D9.2 (the preferences *reads* — the type itself already moved) |
| other preferences types | 7 | D9.2 |
| the genuine DTOs (`APINodeTypeView`, `APIValidationError`, `DragFrozenStatus`, …) | 13 | D10 / D10.1 |
| bare-module imports | 6 | — |

The 6 `SelectModifier` references D9's table counted are **gone**, absorbed by D6
exactly as the design predicted. Note the 6 remaining
`AtomicStructureVisualization` sites are *not* a failure of D6: the type is now
domain-owned, and what those files still reach up for is
`preferences.atomic_structure_visualization_preferences`, i.e. the containing
preferences struct, which D9.2 moves in Phase 6.

### Phase 5 — `atomcad-display`

4,063 lines; ~65 call sites. Includes D7: `scene_tessellator.rs` moves
up into what is still the monolith, and `param_atomic_number_to_index`
moves down into `atomcad-crystolecule`.

**Gate:** re-run the D13 benchmark. All five lower crates have now
left the root, so cross-crate inlining exposure is at its maximum —
this is the measurement that decides whether `lto = "thin"` suffices.

*Estimate: 1 day.*

#### Phase 5 — landed 2026-08-17

**Changes.** `rust/src/display/` → `rust/crates/atomcad-display/src/` (11 files,
`mod.rs` → `lib.rs`), and `rust/tests/display.rs` + `rust/tests/display/` →
`crates/atomcad-display/tests/`, keeping the D5.1 directory-name rule. The crate
takes `atomcad-crystolecule`, `atomcad-geo-tree`, `atomcad-renderer` and
`atomcad-util` plus `csgrs`, `geo`, `glam` and `nalgebra` from
`[workspace.dependencies]`; the root package gains
`atomcad-display = { path = "crates/atomcad-display" }` and loses
`pub mod display;` from `lib.rs`. `rust/src/` is now `api/` + `expr/` +
`structure_designer/` + `frb_generated.rs`.

The prefix rewrite was again purely mechanical: `crate::display::` →
`atomcad_display::` in 53 sites across 29 source files, `crate::display::` →
`crate::` for the 10 internal self-references, and
`rust_lib_flutter_cad::display::` → `atomcad_display::` in 17 sites across 8
test/example files. **`cargo build` succeeded on the first attempt** once the two
D7 moves were in place; `display` held 2 of the 15 `pub(crate)` items measured in
Current state and neither needed escalating. Phase 1's doc-test trap did not
bite — the moved files contain no `///` examples and no `rust_lib_flutter_cad`
mentions (checked *before* moving, as Phase 1 advises), and Phase 3's
`include_bytes!` trap is gone for good.

**Three dependencies finally left the root manifest, and a fourth turned out not
to belong there at all.** `csgrs`, `geo` and `nalgebra` are named directly by
`csg_to_poly_mesh.rs` and moved with it — this is the Phase 5 outcome Phase 2's
entry predicted. The surprise is **`wgpu`**: Phase 3's note claimed
`api/screenshot_api.rs` "reads the rendered texture back with `wgpu`", but the
root crate names no `wgpu` type anywhere (the single occurrence is a comment;
the readback goes through `atomcad-renderer`). `wgpu` was removed from the root
`[dependencies]` here and the stale comment corrected. Only `image` — which
`screenshot_api.rs` really does use for PNG encoding — and `blake3`
(`export_atoms.rs`) remain as non-workspace-crate root dependencies.

**D7, up-move: `scene_tessellator.rs` → `src/structure_designer/`.** It consumes
`StructureDesignerScene` / `NodeOutput` and continues to call *down* into
`atomcad_display` for per-object tessellation. Its one caller
(`api/api_common.rs:480`) changed path; nothing else. Registered in
`structure_designer/mod.rs`, so it travels into `atomcad-structure-designer`
with the rest of the module in Phase 6 at no extra cost.

**D7, down-move: the parameter-element helpers → `atomcad_crystolecule::atomic_constants`.**
D7 names only `param_atomic_number_to_index`, but the constants it reads
(`PARAM_ELEMENT_BASE`, `MAX_PARAM_ELEMENTS`) have to move for it to compile, and
once they have, leaving `param_index_to_atomic_number` (its exact inverse),
`param_atomic_number_to_motif` and `is_param_element` behind buys nothing. All
five moved as a block, out of
`structure_designer/nodes/atom_edit/types.rs`. They are a property of the
atomic-number encoding, so `atomic_constants.rs` is the right home; a comment at
each end records the widening. **No re-export was left behind** — D7 says
"`structure_designer` updated to use it from there", so all nine call sites
(2 in `api/`, 3 in `structure_designer/`, 2 test files) now name the crystolecule
path. That is deliberate: a `pub use` in `types.rs` would have made the phase a
one-line diff, but it would also have left a phantom
`atom_edit::atom_edit::param_*` path that reads as if the encoding still belonged
to the node.

**Two test files had to move *up*, and they went into `tests/structure_designer/`
rather than into a new root harness.** D5 lists `display/atom_label_test.rs` and
`display/atomic_impostor_alpha_test.rs` among the six files that "cannot travel
downward"; both call `tessellate_scene_content`, which this phase moved up, so
neither D5 branch (drop an incidental dependency / move to the root tree beside
`tests/integration/`) applies literally. Phase 3's rule decided it: **a test's
home is what it imports**, and what these import is `structure_designer`. They
were appended to `tests/structure_designer.rs` with `#[path]` lines rather than
given a `display_api.rs` counterpart to `renderer_api.rs` — that name would have
been wrong twice over (they touch no `api`, and in Phase 6 they belong in the
crate-side harness, which is where they now already sit). The other four
`tests/display/` files reach no further than `atomcad_display` and moved intact.

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo test -p atomcad-display -j 4` (the Phase 4 lesson — no feature-graph
surprise this time), `cargo clippy -j 4`, `cargo clippy --all-targets -j 4`,
`cargo fmt -- --check`, `flutter_rust_bridge_codegen generate` + `cargo fmt` +
`git diff --numstat lib/src/rust rust/src/frb_generated.rs`, `flutter analyze`,
`cargo build --release`, the D13 benchmark (below), and cargokit
(`flutter build windows --debug` rebuilt `rust_lib_flutter_cad.dll` and linked
`atomCAD.exe`). No `.snap.new` files; `git status` shows `csgrs/` untouched.
**Pending manual step for the maintainer:** launch the app (`flutter run`,
release DLL) and the Flutter smoke test.

**Test count: 5,054 — identical to Phases 0–4** (5,040 passed, 14 ignored), under
both `cargo test -j 4` and `cargo test --workspace -j 4`. The 59 display tests
split across the harness boundary:

| binary | Phase 4 | Phase 5 |
|---|---|---|
| `tests/display.rs` (root package) | 59 | — |
| `atomcad-display` `tests/display.rs` | — | **28** |
| `tests/structure_designer.rs` (root package) | 3,082 | **3,113** |

Every other binary is unchanged. `display` had no lib unittests and no
doc-tests, so nothing moved between those buckets.

**Lint baselines held exactly:** `cargo clippy -j 4` → **36** warnings
(16 root lib + 16 `atomcad-crystolecule` + **4** `atomcad-display`, the four
`unused_variables`/`unused_assignments` in `atomic_tessellator.rs`'s
cull-counter debug code, which have simply changed owner);
`cargo clippy --all-targets -j 4` → **112** individual warnings (16 lib + 62
`structure_designer` + 16 `crystolecule` lib + 5 `crystolecule` tests + 4
`atomcad-display` lib + 4 `expr` + 4 `geo_tree` + 1 `util`); `flutter analyze` →
**139** issues. Note the two redistributions, so a future phase does not read
either as a regression: the root lib's 20 → 16 + 4 (display's warnings moved with
display), and `structure_designer`'s 61 → 62 with the display test binary's 1 →
0 (the warning rode along with the two relocated test files).

**Generated Dart: zero content change.**
`git diff --numstat lib/src/rust rust/src/frb_generated.rs` is empty, no
directory appeared or disappeared under `lib/src/rust/`, and no Dart symbol
changed. This phase moved no Dart-facing type, so that is the expected result —
but it also settles the loose end Phase 4 left. Phase 4 recorded that five FRB
"multiple objects with same key … randomly pick one" collisions remained, all of
them `api/`-vs-`display/` preference-struct duplicates (`MeshSmoothing`,
`AtomicRenderingMethod`, `GeometryVisualizationPreferences`,
`AtomicStructureVisualizationPreferences`, `BackgroundPreferences`). Moving
`display` out of the expanded self-crate **removes all five from the codegen log
with no output change**, which proves FRB had been picking the `api/` side every
time. That was luck, not design — had it picked the `display` copy for any of
them, this phase's diff would have been a silent Dart break — and it is worth
knowing that `git diff` was the check that could confirm it, not the absence of
an error.

**Back-edge audit.** `display → structure_designer` is **0** references (the only
remaining mentions under the crate are two lines of `lib.rs`'s module doc
explaining why they used to be there), and reintroducing one is now a build
failure: the root crate is not in `atomcad-display`'s manifest. Three of the four
back-edges in Current state are closed. Only `structure_designer → api` remains,
still measuring **145 sites across 123 files** — unchanged from Phase 4's
recount, since nothing here touched it. It is Phase 6's.

**D13 — the gate. Thin LTO fully absorbs the split; without it the split would
have cost 2.1×.**

All five lower crates have now left the root, so cross-crate inlining exposure is
at its maximum. Phase 4 warned that this machine drifts between sessions and that
comparing against Phase 0's recorded table would make the gate fire on drift, so
the control was **rebuilt and re-measured in the same session**: a `git worktree`
at commit `19086690` (Phase 0 — the workspace scaffolding, thin LTO and the bench
harness all in place, but *no code moved*), built into its own target directory.
Fixture: the 1,075,748-atom / 1,951,386-bond nanobeam, 5 reps, `-j 4`, minima.

| nanobeam, `lto = "thin"` | Phase 0 monolith (re-measured today) | post-Phase-5 split | delta |
|---|---|---|---|
| load | 2.7 ms | 2.7 ms | — |
| evaluate | 2,196.1 ms | **2,143.4 ms** | **−2.4 %** |
| impostor tessellation | 451.2 ms | **443.9 ms** | **−1.6 %** |

**The split is at parity — marginally on the faster side, i.e. inside noise.** No
escalation to `lto = "fat"` / `codegen-units = 1` is warranted. Against Phase 0's
*recorded* table (2,089.2 / 433.3) the same run reads +2.6 % / +2.4 %, and the
same-session control shows that entire gap is session drift, not the refactor —
Phase 4's diagnosis was right, and its instruction to re-baseline is what makes
this number trustworthy. (`nut-bolt.cnnd`, 10 reps: evaluate 11.8 ms, impostors
1.7 ms, triangles 26.3 ms — against Phase 0's 11.5 / 1.7 / 25.7. Also parity, and
also far better than Phase 4's 13.5 / 2.0 / 32.7 interim reading, which is a
second, independent confirmation that that session was simply slow.)

The LTO-on/off pair — Phase 4's "cheap sanity check" — is where the interesting
result is:

| nanobeam | thin LTO | LTO off | penalty |
|---|---|---|---|
| Phase 0 (one crate) | 2,089.2 ms / 433.3 ms | 2,183.3 ms / 525.4 ms | 1.05× / 1.21× |
| Phase 5 (six crates) | **2,143.4 ms / 443.9 ms** | **4,560.7 ms / 961.3 ms** | **2.13× / 2.17×** |

Before the split, turning LTO off cost 5–21 %. After it, **2.1×**. D13's
"exposure" is therefore entirely real and considerably larger than the entry's
own hedging suggested — the per-atom accessor loops really do stop inlining
across the new boundaries — and `[profile.release] lto = "thin"` is what makes
the whole refactor performance-neutral. It has gone from a precaution to a
load-bearing setting: **removing it would now be a 2× runtime regression, not the
5–20 % it would have been before Phase 1.** That is the single most important
thing this phase established, and it is why the line in `rust/Cargo.toml` carries
a comment saying so.

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

#### Phase 6 — landed 2026-08-17

Four commits, as planned. The `structure_designer → api` back-edge measured
**145 sites across 123 files** at the start and is **0** at the end; all four
back-edges named in Current state are now closed, and every one of them is a
build failure to reintroduce.

**6.1 — D9.1 + D9.2 (types down).** `NodeTypeCategory` moved to
`structure_designer::node_type` (109 files) and the 12 remaining preferences
types to `structure_designer::preferences`, which now holds the settings
themselves as well as their load/save. `StructureDesigner::preferences` and
`preferences.json` are the domain type from here on; the api converts in exactly
one place (`get`/`set_structure_designer_preferences`). Each type keeps a
same-named twin in the `api/` file it already occupied, with `From` impls both
ways — 26 impls, written longhand so the compiler catches a field added on only
one side. Three things worth carrying forward:

- **A twin's *methods* can be Dart-facing too.** `NodeTypeCategory::order` and
  `::display_name` are exported (they appear in `frb_generated.rs`), so deleting
  them from the api side broke the build. The twin keeps both; `order` delegates
  to the domain, `display_name` cannot (it returns `&str` borrowed from `self`).
- **`APIIVec3` does not move down, so the colour fields needed a domain
  counterpart.** `glam::IVec3` is not an option: glam's `serde` feature is off,
  and its impl serializes a vector as a *sequence*, which would silently
  invalidate the colour fields in every user's existing `preferences.json`. The
  new `preferences::PrefColor { x, y, z }` keeps the JSON byte-compatible; the
  field names are `x`/`y`/`z` rather than `r`/`g`/`b` for exactly that reason.
- **Nine api→display `match` blocks became identity and were deleted.** Once the
  preferences hold `atomcad_crystolecule`'s `AtomicStructureVisualization` (which
  `display::preferences` re-exports), `let display_visualization = match … {}`
  is a clone. The same collapse hit `to_display_atomic_structure_visualization`
  in `api_common.rs` and two `raytrace_per_node` call sites. This is the tail of
  the Phase 4 note about three copies of that enum; there are now two (domain +
  Dart twin).

`AtomicStructureVisualization` gained `Serialize`/`Deserialize` in
`atomcad-crystolecule`: it is a persisted preference now, not only a `hit_test`
input, which also makes its variant names load-bearing.

**6.2 — D10 + D10.1 (view-builders up).** Two new modules under
`api/structure_designer/`, both deliberately **absent from
`flutter_rust_bridge.yaml`'s `rust_input`** exactly like `api_common` — their
`pub fn`s take domain types, and every `pub fn` in a scanned namespace becomes a
Dart API, which would drag `NodeTypeRegistry` into codegen as an opaque handle:

- `view_builders.rs` — the five methods D10 names plus `get_node_root_cause`,
  which the design's table missed (it also returns `APIErrorRootCause`). They
  take `&NodeTypeRegistry` / `&StructureDesigner`. **No accessors were needed**:
  the design budgeted "~3 new public accessors" and the answer was zero, because
  every field they touch is already `pub`.
- `tool_adapters.rs` — `get`/`set_active_tool` for `AtomEditData` **and**
  `EditAtomData` (the design named only the former).
- `cli_runner.rs` moved whole, as D10.1 says.

Three groups of types the design's tables do not list turned up, and all three
resolve by D9a/D10.1's own rule rather than by improvisation — *the producing
code is domain logic, so the type is twinned instead of the function moved*:

- **The five `default_tool` pointer-result types travel with
  `DragFrozenStatus`.** D10.1 mandates twinning `DragFrozenStatus` (it is
  consumed inside `operations.rs`), and `PointerMoveResult` has a
  `DragFrozenStatus` *field* — so twinning one forces the other. They are also
  the return shapes of a 400-line state machine that reads and writes
  `AtomEditData` throughout, which is the opposite of a view-builder. Domain
  copies live in `nodes/atom_edit/types.rs`; conversion is one-way
  (`Domain → twin`) at three `atom_edit_api` call sites.
- **`APINodeEvaluationResult` / `APIExecuteResult` gained domain originals**
  (`NodeEvaluationResult` / `ExecuteResult`). `evaluate_node_for_cli` and
  `execute_node` run real evaluation passes through `with_eval_context`; moving
  them up would have been treatment (A) applied to genuine domain behaviour, and
  would additionally have dragged six large test files (`evaluate_node_test`,
  `motif_edit_test`, `execute_node_test`, …) into the root harness. This is the
  `PrintLogEntry` / `APIPrintLogEntry` precedent the codebase already had.
- `APIEditAtomTool`, alongside `APIAtomEditTool`, in `tool_adapters.rs`.

**6.3 — cut the crate.** `rust/src/structure_designer/` →
`crates/atomcad-structure-designer/src/` (`mod.rs` → `lib.rs`) with `rust/src/expr/`
in beside it as a submodule (D8). `rust/src/` is now `api/` + `frb_generated.rs`
and `lib.rs` is two lines. The prefix rewrite was as mechanical as D3 predicts —
`crate::structure_designer::` → `crate::` inside the crate, →
`atomcad_structure_designer::` outside — and **every relative `super::` path
survived untouched**, because extracting a module tree shifts them all by
exactly one level in the same direction. `cargo build` failed on precisely two
things, both listed below.

Test partition per D5.1a, driven by compiling rather than grepping: **11** files
stay at the root, not ten. `preferences_test.rs` self-resolved in 6.1 (its
`APIIVec3` uses were incidental, as D5.1a's judgement call anticipated), while
`scoped_validation_errors_test.rs` and `parameter_in_zone_body_test.rs` joined
because 6.2 moved the view-builders they call — precisely the "ten is a floor"
case the design warns about, and precisely the reason it says to cut first and
let the compiler decide. Two files were **split** rather than moved:
`function_pin_test.rs` (as D5.1a predicts — though the api-touching part is one
test near the end, not the whole trailing block from `:1290`) and
`parameter_in_zone_body_test.rs`. `tests/expr.rs` + `tests/expr/` moved with the
crate untouched.

**D5.4 turned out not to bite at all.** Every `.snap` under
`tests/structure_designer/snapshots/` is written by `record_types_phase8_test`
or `text_format_snapshot_test`, both of which stayed with the crate; none of the
11 root-side files uses `insta`. No snapshot moved and none needed
`cargo insta review`.

Two problems the design did not anticipate:

- **`"../samples/…"` breaks exactly the way `tests/fixtures/` did — and D5.3
  does not cover it.** 32 sites across three files address the committed `.cnnd`
  demos relative to `cargo test`'s working directory, i.e. the *package* root,
  which is no longer `rust/`. `atomcad-test-support` gains `sample_path` /
  `sample_path_str` beside `fixture_path`, so the `../` hop lives in one line.
  The failure was loud (19 file-not-found panics), which is the property D5.3
  claims for its own class — but note the failure surfaced only when the tests
  *ran*, well after `cargo build` and `cargo check --all-targets` were both
  green. **When extracting a crate, grep its tests for every relative path, not
  just for `CARGO_MANIFEST_DIR`.**
- **Two `#[flutter_rust_bridge::frb(ignore)]` attributes existed in
  `structure_designer`,** on `AtomEditData` and `EditAtomData`. Current state
  measured *zero* FRB annotations outside `api/`; they were added later. They
  are deleted rather than carried: the crate is not scanned by codegen at all,
  so there is nothing to opt out of (D11). This is the one place the "no
  extracted crate has to carry codegen annotations" claim needed enforcing
  rather than merely observing.

**6.4 — cleanup.** The cross-layer test files D5 lists needed nothing: all six
were resolved in Phases 2–5 as those phases record. The
`#[cfg(not(frb_expand))]` guard went with the module it hid.

**The `check-cfg = ['cfg(frb_expand)']` entry, however, must stay — the design
is wrong about it.** Phase 6.4's instruction to delete it "with" the attribute
assumes `lib.rs:8` was its only user. It was not: the
`#[flutter_rust_bridge::frb(...)]` proc macro *expands to*
`#[cfg(not(frb_expand))]`, once per annotated item — **489** of them across
`src/api/`. Deleting the line turns 489 `unexpected_cfgs` warnings back on and
removes no workaround. `rust/Cargo.toml` now carries a comment saying so.

**Verified.** `cargo build`, `cargo test -j 4`, `cargo test --workspace -j 4`,
`cargo test -p <crate> -j 4` for all six crates, `cargo clippy -j 4`,
`cargo clippy --all-targets -j 4`, `cargo fmt -- --check`,
`flutter_rust_bridge_codegen generate` + `cargo fmt` +
`git diff --numstat lib/src/rust rust/src/frb_generated.rs`, `flutter analyze`,
`cargo build --release`, the D13 harness (below), and cargokit
(`flutter build windows --debug` rebuilt `rust_lib_flutter_cad.dll` and linked
`atomCAD.exe`). No `.snap.new` files; `git status` shows `csgrs/` untouched.
**Pending manual step for the maintainer:** launch the app (`flutter run`,
release DLL) and the Flutter smoke test.

**Test count: 5,054 — identical to Phases 0–5** (5,040 passed, 14 ignored). The
`structure_designer` binary split across the package boundary; nothing else
moved:

| binary | Phase 5 | Phase 6 |
|---|---|---|
| `tests/structure_designer.rs` (root package) | 3,113 | — |
| `atomcad-structure-designer` `tests/structure_designer.rs` | — | **2,871** |
| `tests/structure_designer_api.rs` (root package) | — | **242** |
| `tests/expr.rs` (root package) | 477 | — |
| `atomcad-structure-designer` `tests/expr.rs` | — | **477** |
| `rust_lib_flutter_cad` (lib unittests) | 4 | 0 |
| `atomcad-structure-designer` (lib unittests) | — | **4** |
| doc-tests `rust_lib_flutter_cad` | 15 (5 run) | 11 (1 run) |
| doc-tests `atomcad_structure_designer` | — | **4** |

**Lint baselines held exactly:** `cargo clippy -j 4` → **36** warnings
(16 `atomcad-crystolecule` + 15 `atomcad-structure-designer` + 4
`atomcad-display` + 1 root lib — the root's 16 moved out with the code, and one
of `structure_designer`'s was *fixed* rather than carried, see below);
`cargo clippy --all-targets -j 4` → **112**; `flutter analyze` → **139**.

The fixed one is worth recording as a recurring class: shortening
`crate::structure_designer::network_validator::dedupe_param_ids_in_network` to
`crate::network_validator::…` let the call fit on one line, at which point
clippy's `redundant_closure` fired on a closure it had been silently tolerating
across a line break. Phase 2 saw the same shape with
`single_component_path_imports` on the `mod.rs` → `lib.rs` promotion: **a
purely mechanical rewrite can change what a lint sees**; treat it as part of the
move, not as new debt.

**Generated Dart: three comment-only diffs across the four commits, and nothing
else.** `rust/src/frb_generated.rs` is **byte-identical** throughout. Under
`lib/src/rust/` the only changes are (a) the doc comments written on the two
`NodeTypeCategory` / `AtomicStructureVisualization` twins, which FRB propagates
into the Dart, and (b) new `from` entries in each file's "these functions are
ignored because they are on traits not defined in current crate" header comment
— the direct trace of the `From` impls. No `abstract class … implements
RustOpaqueInterface` appeared, no directory appeared or disappeared, and the
Dart symbol set is unchanged. **The 14 twins D9a asks for came out to 26 types
across four files, and not one generated symbol moved** — which is the whole
point of the pattern.

**D13 — no re-gate needed, and a note on why.** Phase 5 is the D13 gate (all
five lower crates out, inlining exposure at maximum); Phase 6 adds one more
boundary, `api → structure_designer`, which is **not on a hot path** — the
per-atom loops live entirely inside `structure_designer` / `display` /
`crystolecule`, whose mutual boundaries this phase did not touch. The harness
was run anyway, same fixture and protocol as Phase 5 (1,075,748-atom nanobeam,
5 reps, `-j 4`, minima): **evaluate 2,197.5 ms / impostors 457.2 ms** against
Phase 5's **2,143.4 / 443.9** — +2.5 % / +3.0 %, inside the session-drift band
both Phase 4 and Phase 5 measured and documented. `[profile.release] lto =
"thin"` remains load-bearing for the reasons Phase 5 established.

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

#### Phase 7 — landed 2026-08-17

One commit, documentation only: no `.rs`, `.dart` or `.toml` file was touched,
so the 5,054-test / clippy-36 / analyze-139 baselines are untouched by
construction.

Two of the five bullets were already done. `AGENTS.md` (root) had its paths
rewritten by commit 6.4, and `rust/AGENTS.md` had accumulated the workspace
rules phase by phase as each was learned — which is the right way round, since
a rule written at the moment it cost someone an afternoon is more accurate than
one written from the design doc at the end. What was missing from them was
found by re-reading this section as a checklist rather than by grepping:
`rust/AGENTS.md` had the twin pattern and the two FRB gotchas but never stated
**D11 itself** (the confinement invariant, and specifically "do not add a member
crate to `rust_input`" — the escape hatch D9a leaves open and Phase 6 declined
to take); the root `AGENTS.md` still said "Tests go in `rust/tests/`" and
"Keep modules independent". Both are now explicit.

**The diagram is generated, and that is what "redraw to match" actually meant.**
`doc/architecture_diagram.svg` is the output of
`scripts/architecture_diagram/{count_loc,generate_architecture_diagram}.py`,
whose module table pointed at `rust/src/<module>/` — every path stale, so the
next person to run it would have got seven `Warning: Module path does not
exist` lines and a diagram of nothing. Hand-editing the SVG would have left that
trap armed. The scripts now read `rust/crates/<crate>/src/` plus `rust/src/api/`,
and three substantive things changed in the picture:

- **`expr` is gone as a circle** (D8) and `api` gained one. The old diagram
  folded `api` into `structure_designer`; now that they are separate *packages*
  with a compiler-enforced boundary, showing the FFI layer as its own node is
  what makes the diagram match the workspace.
- **`DEPENDENCIES` is now a transcription of the manifests**, with the arrows
  that would clutter the picture listed separately in `ELIDED_ARROWS` rather
  than omitted from the data. The previous version hard-coded a
  "skip `util` edges except from domain modules" rule inline, which is the same
  elision but indistinguishable from a missing dependency.
- **`LOC_SCALE` had to drop from 3.0 to 0.7.** The committed `loc_counts.json`
  was badly stale (`structure_designer` 14,508 versus the 63,985 it measures
  today), and at the old scale the two largest circles overlapped their
  neighbouring layers. Circle *area* is proportional to LOC, so this constant is
  a growth-sensitive layout parameter, not a style choice — it is now commented
  as such.

`doc/architecture_overview.md` was rewritten around the crate table, with `expr`
recorded as a component rather than a peer and the two rules a newcomer needs
before adding code (the down-vs-up test, FRB confinement) stated inline with a
pointer to `rust/AGENTS.md` for the detail. `doc/testing.md` gained the
`cargo test -p <crate>` forms and a "where a new test goes" table — the
two-harness split is not intuitive, and the rule that decides it (*what the test
imports, not what it is about*) is worth stating in the place someone looks when
writing a test rather than only here.

Also folded in, being the same class of staleness: `rust/crates/README.md` was
missing its own Phase 6 row.

**This closes the design.** All seven phases are landed; the four back-edges of
Current state are gone and are build failures to reintroduce.

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
