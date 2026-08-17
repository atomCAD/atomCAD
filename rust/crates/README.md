# `rust/crates/`

Home of the extracted backend crates from `doc/design_rust_crate_split.md`.

The workspace in `rust/Cargo.toml` picks crates up here with
`members = ["crates/*"]`; this README is not a package and the glob tolerates
it.

| crate | was | phase |
|---|---|---|
| `atomcad-util` | `rust/src/util/` | 1 |
| `atomcad-geo-tree` | `rust/src/geo_tree/` | 2 |
| `atomcad-renderer` | `rust/src/renderer/` | 3 |
| `atomcad-crystolecule` | `rust/src/crystolecule/` | 4 |
| `atomcad-test-support` | `rust/tests/test_support/` | 4 |
| `atomcad-display` | `rust/src/display/` | 5 |

Conventions for anything added here:

- name the directory `atomcad-<module>` (hyphens); it is imported as
  `atomcad_<module>` (underscores)
- take every third-party dependency from `[workspace.dependencies]`
  (`glam = { workspace = true }`), never with an inline version
- `publish = false` — the workspace is an internal structuring device
- **no `#[frb(...)]` attributes**: flutter_rust_bridge stays confined to the
  root crate's `src/api/` (D11). A Dart-facing type that moves down here keeps
  a same-named twin in `api/` (D9a) — a `pub use` re-export does *not* make it
  visible to codegen
- tests live in `<crate>/tests/`, keeping the module's original directory name
  (`atomcad-crystolecule/tests/crystolecule/…`); the repetition is load-bearing
  because it keeps ~250 `#[path]` declarations and the test-data paths valid
  (D5.1)
- a test that reaches *up* out of the crate cannot come along; it goes to a
  root-package harness named `<module>_api.rs` (`atomcad-renderer` left four
  such tests behind in `rust/tests/renderer_api/`), per D5 and D5.1a
- committed assets a crate `include_bytes!`/`include_str!`s belong **inside**
  the crate (`atomcad-renderer/assets/`), together with any example that
  generates them — a relative `include_*!` path is resolved against the *source
  file*, so it silently changes meaning when the file moves
- a test that reads `rust/tests/fixtures/` must address it through
  `atomcad_test_support::fixture_path` (or `fixture_path_str`), never through its
  own `CARGO_MANIFEST_DIR` or a working-directory-relative `"tests/fixtures/…"`
  string — both are anchored to the *package* root and silently mean something
  different in a member crate (D5.3). The fixture tree itself stays put; it is
  shared by three packages and duplicating it would fork the migration corpus.

`atomcad-test-support` is the odd one out: it is a helper crate rather than an
extracted module, it is only ever a `[dev-dependencies]` entry, and it depends on
`atomcad-crystolecule`, which dev-depends back on it. That cycle is legal
precisely because the return edge is a dev edge — `cargo metadata` shows the loop
and it is not an error (D5.2).
