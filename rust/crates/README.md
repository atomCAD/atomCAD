# `rust/crates/`

Home of the extracted backend crates from `doc/design_rust_crate_split.md`.

The workspace in `rust/Cargo.toml` picks crates up here with
`members = ["crates/*"]`; this README is not a package and the glob tolerates
it.

| crate | was | phase |
|---|---|---|
| `atomcad-util` | `rust/src/util/` | 1 |
| `atomcad-geo-tree` | `rust/src/geo_tree/` | 2 |

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
