# Rust Backend - Agent Instructions

## Module Architecture

Dependencies flow downward (no circular dependencies):

```
┌─────────────────────────────────────────────────────┐
│                  structure_designer                  │
│  (Node network, evaluator, application logic)       │
├─────────────────────────────────────────────────────┤
│        display           │          api             │
│   (Tessellation)         │   (Flutter API layer)    │
│   extracted:             │                          │
│   atomcad-display        │                          │
├──────────────────────────┴──────────────────────────┤
│  crystolecule  │  geo_tree   │  renderer  │   expr  │
│  (Atoms/bonds) │  (CSG/SDF)  │   (wgpu)   │  (Lang) │
│   extracted    │  extracted  │  extracted │         │
│   atomcad-     │ atomcad-    │ atomcad-   │         │
│  crystolecule  │  geo-tree   │  renderer  │         │
├─────────────────────────────────────────────────────┤
│         util  →  crate `atomcad-util` (extracted)    │
└─────────────────────────────────────────────────────┘
```

## Key Modules

- **structure_designer/** - Node network, evaluator, serialization (.cnnd) (see `src/structure_designer/AGENTS.md`)
- **expr/** - Expression language (lexer, parser, validation)
- **api/** - Flutter Rust Bridge API layer
- **`crates/atomcad-util/`** - was `src/util/`; the bottom of the DAG, now its
  own crate. Imported as `atomcad_util::…` (**not** `crate::util::…`), including
  from tests, which use `atomcad_util::daabox::DAABox` rather than
  `rust_lib_flutter_cad::util::…`. Its own tests live in
  `crates/atomcad-util/tests/`.
- **`crates/atomcad-geo-tree/`** - was `src/geo_tree/`; CSG types, SDF
  evaluation, geometry caching (see `crates/atomcad-geo-tree/src/AGENTS.md`).
  Depends on `atomcad-util` only. Imported as `atomcad_geo_tree::…` (**not**
  `crate::geo_tree::…`), including from tests. Its own tests live in
  `crates/atomcad-geo-tree/tests/`. It owns `rayon` and `blake3`, and shares
  `csgrs` / `geo` / `nalgebra` with `atomcad-display` (whose
  `csg_to_poly_mesh.rs` names all three directly). None of them is in the root
  manifest any more.
- **`crates/atomcad-crystolecule/`** - was `src/crystolecule/`; atomic
  structures, unit cells, motifs, lattice operations, CIF/XYZ/MOL I/O, UFF
  simulation (see `crates/atomcad-crystolecule/src/AGENTS.md`). Depends on
  `atomcad-util` and `atomcad-geo-tree` — `GeoNode` is the SDF input to
  `fill_lattice()` — and on nothing above it: the "never depend on `renderer` or
  `display` here" constraint is now enforced by the compiler rather than by
  review. Imported as `atomcad_crystolecule::…` (**not**
  `crate::crystolecule::…`), including from tests. Its own tests live in
  `crates/atomcad-crystolecule/tests/crystolecule/`.

  Phase 4 also moved two Dart-facing types **down** into it (the old
  `crystolecule → api` back-edge): `SelectModifier`
  (`atomic_structure`) and `AtomicStructureVisualization` (`visualization`).
  Each keeps a same-named twin in `api/` with `From` impls both ways — see the
  twin-pattern bullet below. `display::preferences` no longer declares its own
  third copy of `AtomicStructureVisualization`; it re-exports this crate's.
- **`crates/atomcad-test-support/`** - shared test helpers, never a runtime
  dependency: `assert_structures_equivalent` (the `≡` used by two harnesses that
  are now in different packages) and `fixture_path` / `fixture_path_str` /
  `fixtures_root`, the **only** correct way to address `rust/tests/fixtures/`.
- **`crates/atomcad-renderer/`** - was `src/renderer/`; wgpu rendering, shaders
  (`*.wgsl`), mesh management, camera math. Depends on `atomcad-util` only.
  Imported as `atomcad_renderer::…` (**not** `crate::renderer::…`), including
  from tests. It owns the GPU stack (`wgpu`, `bytemuck`) plus `image`, the
  committed `assets/` (SDF font atlas, its source font and license) and
  `examples/gen_font_atlas.rs`, the generator for `assets/font_atlas.png` and
  `src/font_metrics.rs` — run it with
  `cargo run --release -p atomcad-renderer --example gen_font_atlas`. Note
  `src/api/screenshot_api.rs` still names `image` directly, so that one stays in
  the root manifest as well; `wgpu` does not (the readback goes through this
  crate). Its GPU-free tests live in
  `crates/atomcad-renderer/tests/renderer/`; the four api-level
  axis-resolution tests that used to sit in `camera_test.rs` are at
  `rust/tests/renderer_api/` (see Testing below).
- **`crates/atomcad-display/`** - was `src/display/`; tessellates domain objects
  (atoms, bonds, CSG meshes, point clouds, gadgets) into renderer meshes, and
  owns `DisplayPreferences`. The only crate that depends on **both** the domain
  and the renderer — that is what makes it the adapter. Imported as
  `atomcad_display::…` (**not** `crate::display::…`), including from tests; its
  tests live in `crates/atomcad-display/tests/display/`.

  It must not depend on `structure_designer`. Phase 5 deleted the two edges that
  did: `scene_tessellator.rs` moved **up** to
  `src/structure_designer/scene_tessellator.rs` (it adapts the *scene graph*, a
  `structure_designer` concept, not the domain), and the parameter-element
  helpers (`param_atomic_number_to_index` and family) moved **down** into
  `atomcad_crystolecule::atomic_constants`, beside the element table whose
  encoding they describe.

## Cargo workspace

`rust/` is **both** the cdylib package (`rust_lib_flutter_cad`, the one cargokit
builds) **and** the workspace root. Extracted crates live in `rust/crates/` and
are picked up by the `members = ["crates/*"]` glob. This is step 0 of
`doc/design_rust_crate_split.md`, which converts the top-level modules into
crates so the dependency DAG above becomes compiler-enforced rather than
convention-enforced. `atomcad-util` (Phase 1), `atomcad-geo-tree` (Phase 2),
`atomcad-renderer` (Phase 3), `atomcad-crystolecule` (Phase 4, together with
the `atomcad-test-support` dev-only helper crate) and `atomcad-display`
(Phase 5) are extracted; `structure_designer` (with `expr`) follows in Phase 6.

Rules that follow from that layout:

- **Dependency versions go in `[workspace.dependencies]`,** and every package —
  the root included — writes `foo = { workspace = true }`. Two packages on two
  `glam` versions would make `DVec3` two different types.
- **`csgrs` must stay in `[workspace.dependencies]`.** It is a path dependency,
  and a relative path resolves against the manifest that declares it: `../csgrs`
  is correct only from `rust/Cargo.toml`. Inlined into a member manifest it
  would point at `rust/crates/csgrs`.
- **Do not remove `default-members` from `[workspace]`.** In a workspace that
  has a root package, cargo's default selection is the root package *alone*, so
  a bare `cargo test` / `cargo clippy` in `rust/` would silently skip every
  extracted crate — tests would vanish from the run with no error. `cargo test
  --workspace -j 4` is the belt-and-braces form.
- `[profile.*]` is only honoured in the workspace root manifest (this is why
  csgrs's own `lto = true` has never applied). **`[profile.release] lto = "thin"`
  is load-bearing, not a precaution** — it restores the cross-crate inlining the
  split gives up, and the Phase 5 benchmark measured the difference at **2.1×**
  on the million-atom path (5-21 % before any code moved). Deleting it would be
  a 2× runtime regression, silently. See D13.
- **Use `cargo fmt`, never `cargo fmt --all`.** Plain `cargo fmt` honours
  `default-members`, so it already covers `crates/*`. `--all` additionally
  reaches into the *vendored* `../csgrs` path dependency and reformats it,
  clobbering the local EPSILON patch's comment layout — a spurious diff in a
  directory this refactor must not touch.
- A doc-test in an extracted crate must import through that crate's own name
  (`use atomcad_util::…`), not `rust_lib_flutter_cad::…`. Doc-tests are compiled
  as external users of the crate that defines them, so a stale prefix here fails
  the run even though `cargo build` is green.
- **A relative `include_bytes!` / `include_str!` path changes meaning when its
  source file moves.** Extracting a crate reparents every such path by two
  directory levels, and the failure is a compile error only if the file happens
  not to exist at the new location — so move the asset into the crate and
  re-derive the path, don't patch in `../` hops. `atomcad-renderer` owns
  `assets/font_atlas.png` for this reason.
- Committed generated artifacts should be regenerable *in place* after a move:
  re-run the generator and confirm a byte-identical diff. That is the only real
  proof both the read path and the write path survived.
- **A Dart-facing type that moves down keeps a same-named twin in `api/`, and the
  twin is *not* renamed to `API…`.** A `pub use` re-export does **not** make a
  type visible to flutter_rust_bridge (its `pub_use_transformer` returns early
  for the self crate), and an unresolvable type degrades to
  `abstract class X implements RustOpaqueInterface {}` **silently** — codegen
  exits 0. So the authoritative definition goes to the lower crate, the
  declaration in `api/` stays where and as it is, and `From` impls (declared in
  the `api/` file, where both types are in scope) convert at the boundary. Where
  a file needs both in scope, path-qualify the domain one
  (`… as DomainSelectModifier`) rather than renaming the api one — renaming it
  would rename the generated Dart symbol and break Flutter. `SelectModifier` and
  `AtomicStructureVisualization` are the worked examples.
- **Two flutter_rust_bridge facts worth knowing before they cost you a day:** a
  bare `#[frb]` on a type is a **no-op** (it parses to `Noop` and is never read),
  and regenerating without an error proves nothing. After any phase that moves a
  type, run `flutter_rust_bridge_codegen generate`, then `cargo fmt` (codegen
  runs its own rustfmt without the 2024 style edition and leaves
  `frb_generated.rs` spuriously dirty), then read
  `git diff --numstat lib/src/rust rust/src/frb_generated.rs` — `git status`
  always shows all six generated files as modified because codegen writes LF
  where the index holds CRLF.
- **Fixtures: use the resolver.** `rust/tests/fixtures/` is read from three
  packages and stays where it is. Address it only through
  `atomcad_test_support::fixture_path` / `fixture_path_str` / `fixtures_root`.
  A local `env!("CARGO_MANIFEST_DIR")` join or a bare
  `"tests/fixtures/…"` string is anchored to the *package* root and means
  something different in every member crate. These failures are loud
  (file-not-found panics), but they are also entirely avoidable.

## Adding a New Node Type

1. Create `src/structure_designer/nodes/my_node.rs`
2. Add to `src/structure_designer/nodes/mod.rs`
3. Register in `src/structure_designer/node_type_registry.rs`

## Addressing Nodes Across Scopes (zones)

HOF zone bodies (`map` / `filter` / `fold` / `foreach` / `closure`) are nested `NodeNetwork`s with **per-body `next_node_id` counters**, so a node id is only unique *within one network* — a body node and a top-level node routinely share the same numeric id. Any lookup that resolves a node **by bare `u64` id is ambiguous**, and that was the source of the zones property-panel bug (clicking a body `expr` showed the outer one, or the panel spun forever because the id collided with a non-`expr` node).

Rules — do not regress these:

- **`StructureDesigner::get_node_network_data[_mut]` are TOP-LEVEL-ONLY** (they do not recurse into bodies). Use them only for interactive subsystems that act on the top-level *active* node — currently `atom_edit` / `edit_atom` (they resolve through the top-level network's `active_node_id`, so they cannot target a body node by design). **Never** reintroduce a "walk every body and return the first id match" lookup — it silently returns the wrong node on a collision.
- Anything that can target a node in **any** scope — every `get_*_data` / `set_*_data` property API, `execute_node`, comment ops, `facet_shell`/`import_xyz`/`import_cif` actions, etc. — **must take a `scope_path: Vec<u64>` parameter** and resolve through `StructureDesigner::get_scope_network(&scope_path)`: reads via `get_node_network_data_scoped(&scope_path, node_id)`, in-place mutations via `get_node_network_data_mut_scoped(&scope_path, node_id)`, whole-data replacement via `set_node_network_data_scoped(&scope_path, node_id, …)`. `scope_path` empty = top-level active network; non-empty = the chain of HOF node ids down to the body.
- **When you add a new node property getter/setter in `src/api/structure_designer/`, it must take `scope_path` like its siblings.** A bare-`node_id` getter is exactly the mistake to avoid; the Flutter property panel always has the selected node's scope and passes it.

See `src/structure_designer/AGENTS.md` (Zones) for the body model and `walk_all_nodes` (the parallel "bare iteration skips body nodes" lesson).

## Code Conventions

- **Edition:** Rust 2024 (requires Rust 1.85+)
- **Toolchain:** Stable only (`rust-toolchain.toml`)
- **Error handling:** Use `thiserror` for error types
- **Math:** Use `glam` (DVec2, DVec3, DMat4)
- **Parallelism:** Use `rayon` for data parallelism

## Key Types

| Type | Purpose |
|------|---------|
| `StructureDesigner` | Main application state |
| `NodeNetwork` | Graph of connected nodes |
| `NodeType` | Definition of a node kind |
| `NetworkResult` | Node output value |
| `AtomicStructure` | Collection of atoms/bonds |
| `GeoNode` | CSG operation tree |
| `ImplicitGeometry3D` | SDF geometry |

## Testing

**⚠️ IMPORTANT: Never use `#[cfg(test)]` inline test modules in source files.**

When adding new functionality to the Rust codebase:

1. **Write tests for new core logic** - especially for functions in `structure_designer/`, `expr/`, and the extracted crates (`atomcad-crystolecule`, `atomcad-geo-tree`, …)
2. **Tests go in `rust/tests/`** (or, for an extracted crate, in that crate's
   own `crates/<crate>/tests/`), NOT inline in source files
3. **Mirror the source file hierarchy** in the test directory:
   - Source: `src/structure_designer/text_format/`
   - Test: `tests/structure_designer/text_format_test.rs`
4. **Register test modules** in the parent test file (e.g., add to `tests/structure_designer.rs`):
   ```rust
   #[path = "structure_designer/text_format_test.rs"]
   mod text_format_test;
   ```
5. Follow the existing folder structure:
   - `rust/tests/structure_designer/` - Structure designer tests
   - `rust/crates/atomcad-crystolecule/tests/crystolecule/` - Atomic structure tests
   - `rust/crates/atomcad-geo-tree/tests/geo_tree/` - Geometry tests
   - `rust/crates/atomcad-renderer/tests/renderer/` - GPU-free renderer tests
     (camera math, label layout, impostor meshes, transparent sort)
   - `rust/crates/atomcad-display/tests/display/` - tessellation tests. The two
     that drive `tessellate_scene_content` are **not** here: that function is a
     `structure_designer` module now, so they sit in
     `rust/tests/structure_designer/` (`atomic_impostor_alpha_test.rs`,
     `atom_label_test.rs`)
   - `rust/crates/atomcad-util/tests/util/` - Utility tests
   - `rust/tests/expr/` - Expression language tests
   - `rust/tests/integration/` - Integration/roundtrip tests
   - `rust/tests/renderer_api/` - the renderer-adjacent tests that reach *up*
     into `api` / `crystolecule` and so cannot live in `atomcad-renderer`

   An extracted crate keeps the module's original directory name inside its
   `tests/` (`atomcad-crystolecule/tests/crystolecule/`, beside
   `tests/crystolecule.rs`).
   The apparent redundancy is load-bearing: it keeps every `#[path]` string and
   every `CARGO_MANIFEST_DIR`-relative test-data path valid across the move
   (design doc D5.1). Do not tidy it.

   **Which harness a new test belongs in is decided by what it imports, not by
   what it is about.** A member crate cannot depend on the root crate, so a test
   naming anything in `rust_lib_flutter_cad::api` has to sit in a root-package
   harness even when its subject lives in an extracted crate — that is why
   `rust/tests/renderer_api.rs` exists beside
   `crates/atomcad-renderer/tests/renderer.rs`. Prefer splitting a file at that
   seam over dragging its whole subject upward.

**When tests may be skipped:**
- **API wrapper functions** (`rust/src/api/`) - these are thin wrappers; test the underlying core function instead
- **Renderer/GPU code** - difficult to test without a GPU context
- **Trivial getters/setters** - unless they contain logic

**Test file naming:** `<module>_test.rs` (e.g., `structure_designer_test.rs`)

**Running tests:**
```bash
cd rust && cargo test                    # Run all tests
cd rust && cargo test <test_name>        # Run specific test
cd rust && cargo test --test structure_designer  # Run tests in a specific test crate
cd rust && cargo test -p atomcad-crystolecule    # Run one crate's tests (no api, no frb_generated)
```

Never `cargo test` with full parallelism on the Windows dev box — use `-j 4`.

## Debugging

- `println!()` output appears in Flutter console
- `dbg!()` macro for value inspection
