# Design: Pushing Domain Code Down out of `atomcad-structure-designer`

## Status: Ready for implementation.

`doc/design_rust_crate_split.md` made the crate DAG compiler-enforced but
deliberately moved **no** code between layers: every module went to the crate it
already belonged to. That left `atomcad-structure-designer` at 63,985 source
lines — 59 % of the backend — and the question of whether any of it is misfiled.

This design answers "yes, in three places," and moves them. A fourth candidate —
the CIF → structure assembly — was researched, found to conflict with a
deliberate boundary already documented in `crystolecule`, and **dropped**; §4
records it so it is not re-proposed.

**No change to the Dart-facing API surface, no `.cnnd` format change, no
user-visible change.** §1 is a pure relocation: `use` statements are rewritten,
function bodies are not. §2 is the same except for one seam — two Miller-index
helpers leave `half_space_utils` for a different crate than the rest of the
file, which rewrites three call expressions (§2.2). §3 is the real exception:
an extract-and-split of a function that mixes crystallography with node state,
the only phase that changes a signature and the only one that adds tests. It is
kept in its own commit for exactly that reason.

## For the implementor

Each phase is one commit that builds and tests green on its own. Most of the
work is a mechanical import rewrite the compiler verifies. The parts where an
otherwise sensible decision is wrong:

1. **The moving line ranges are not contiguous, and they are item boundaries.**
   In both patch files the node's `*Data` struct and its serde defaults sit
   *between* two moving blocks. Move the blocks named in §1.1, not the span
   between them, or `PatchLatticeFillData` and `PatchBuildData` end up in
   `crystolecule` and the build breaks in a confusing way. Each range also
   opens on a doc comment or section banner rather than on the `fn`/`struct`
   keyword — slicing on the keyword line leaves a `///` block attached to
   nothing, which rustc rejects outright.
2. **Inside a moved file, `atomcad_crystolecule::X` must become `crate::X`.**
   A crate cannot refer to itself by its package name. This applies to every
   import in §1's moved code and to `xyz_gadget_utils`'s
   `use atomcad_display::gadget::GadgetPickContext` in §2. Conversely, a `use`
   you add to a moved file is **private** — it does not re-export the symbol at
   the moved file's path. Three node call sites depend on exactly that and are
   easy to miss; see §2.2.
3. **`patch_build_test.rs` does not move as a unit — one of its ten tests must
   stay behind.** It is the only one that touches `NetworkResult`, and it is
   testing a node-level concern. Splitting the file is correct, not sloppy. **D3**
4. **`half_space_utils` is not purely display code.** Two of its functions are
   Miller-index number theory with no rendering dependency. Dragging them into
   `atomcad-display` would file crystallography in the adapter layer — split
   them out to `crystolecule` instead. **D5**
5. **Nothing here should change `lib/src/rust/`.** No `#[frb]` type moves and no
   api DTO changes shape. Per `doc/design_rust_crate_split.md`'s "For the
   implementor" rule 8, still *diff* `lib/src/rust/` after Phase 1 — an empty
   diff is the assertion, not an assumption.
6. **Run `cargo fmt` as part of each move, not after.**
   `crate::utils::half_space_utils::` and `atomcad_display::half_space_utils::`
   are different widths and several call sites sit near the 100-column limit.
   This bit every phase of the original split.
7. **`cargo test -j 4`** on this machine. Full parallelism OOMs the pagefile.
8. **Phase 3 has no existing test coverage to fall back on.** The facet symmetry
   enumeration is exercised by *nothing* — the only facet tests in the tree are
   text-property round-trips. Derive the characterisation expectations from the
   **current** implementation before rewriting it (§3.3 gives a recipe), or the
   refactor is unguarded. Phase 3 writes **two** test files, one per crate:
   `miller_test.rs` for the extracted domain function and
   `facet_shell_symmetry_test.rs` for the wrapper — the second is the one that
   guards the behaviour change, so do not stop after the first. **D8**
9. **Do not revive §4.** The CIF assembly move is the obvious next candidate and
   looks clean on inspection; §4 explains why it was rejected. **D6**

## Motivation

The organising principle is the Stable Dependencies Principle: depend in the
direction of stability. A function that operates only on `AtomicStructure`,
`UnitCellStruct` and `GeoNode` is *more stable* than the node network that calls
it, and filing it under `nodes/` inverts that — it makes domain logic
unreachable from anywhere but the node network, untestable without the node
network, and invisible to anyone reading `atomcad-crystolecule` to find out what
the domain can do.

Three such pockets are moved here, totalling **~1,144 source lines and ~1,470
test lines** — under 2 % of `atomcad-structure-designer`, which is the honest
size of the prize: the crate is large mostly because ~half of `nodes/` is
`impl NodeData` + `get_node_type()` boilerplate that has nowhere lower to go.
They are worth doing anyway because each is a *complete* domain concept
currently split across a layer boundary, not because they move the line count
much — and the same standard is why §4 was dropped despite being equally
mechanical.

### Non-goals

- **Splitting `nodes/` into its own crate.** Still the largest structural prize;
  still needs its own design (crate-split doc, Deferred).
- **Moving gadget implementations to `display`.** Every `XGadget` implements two
  display traits *and* `NodeNetworkGadget`, so each would need splitting in two
  with an adapter left behind. Poor churn-to-benefit ratio, and the genuinely
  reusable part of it is Item 2, which is in scope.
- **De-duplicating the three parallel copies of the preference structs**
  (`api/`, `structure-designer/src/preferences.rs`,
  `display/src/preferences.rs`). Real, ~150 lines, but it is a de-duplication
  with a serde/`PrefColor` conversion question attached, not a relocation. See
  Deferred.
- **`atom_edit`'s motif helpers**, `measurement.rs`, `guideline.rs`, and
  `implicit_eval/`. All genuinely misfiled, all small. See Deferred; fold them
  in opportunistically when touching those files.

## 1. The `patch` domain model → `atomcad-crystolecule::patch`

**~476 source lines, ~1,470 test lines, 32 tests.** The largest and cleanest of
the three moves.

### 1.1 What moves

The patch core is already labelled in its own comment as *"the node-free core so
the model is testable on plain `AtomicStructure`s without the node-network
machinery"*. It lives in **four blocks across two files** — not two contiguous
spans, because each file's `*Data` struct is interleaved with it.

Every range below is an **item boundary**: it starts at the item's first `///`
doc line (or the `// ====` section banner above it) and ends at the item's
closing brace. Do not slice on the `fn`/`struct`/`const`/`#[derive]` keyword
line — a range that starts one line late strands a doc comment in the source
file, which is `error: expected item after doc comments`, not a warning.

| File | Moving lines | Net |
|---|---|---:|
| `nodes/patch_latticefill.rs` | `85-123` (doc comments, the three consts, `CompatibilityReport`) | 10 |
| `nodes/patch_latticefill.rs` | `181-676` (the "Cell selection" banner, `free_directions` … `apply_patch`) | 370 |
| `nodes/patch_build.rs` | `36-40` (`DEFAULT_BUILD_THRESHOLD` + its doc comment) | 1 |
| `nodes/patch_build.rs` | `61-191` (`validate_tiling_vectors`, `extract_patch_tile`, both with their doc comments) | 95 |

**`patch_latticefill.rs:125-179` and `patch_build.rs:42-59` sit between those
blocks and must NOT move** — they are `PatchLatticeFillData` /
`PatchBuildData` (from their `#[derive]` line), their `serde` default fns and
their `impl Default`. See §1.2.

Both second blocks open on a comment that is easy to cut off, and
`patch_latticefill.rs`'s also closes just short of one:

- `patch_latticefill.rs` — `impl Default` closes at **179**; `181-183` is the
  `// ==== Cell selection` banner and `185-189` is `free_directions`'s doc
  comment, so the block opens at **181**, not 190. At the far end `apply_patch`
  closes at **676**; `678-680` is the `// ==== Node wrapper` banner and `682` is
  `PatchFields`'s doc comment, both of which **stay**, so the block ends at
  **676**, not 682.
- `patch_build.rs` — `61-62` is `validate_tiling_vectors`'s doc comment, so the
  block opens at **61**, not 63. The far end is clean: `extract_patch_tile`
  closes at **191**, `192` is blank and `193` opens `impl NodeData`.

The symbols in the moving blocks:

| Symbol | Role |
|---|---|
| `CompatibilityReport` | welded / orphaned / over-coordination stats |
| `DEFAULT_WELD_TOLERANCE`, `CUT_MEMBERSHIP_EPSILON`, `REGION_MEMBERSHIP_EPSILON` | domain constants |
| `free_directions`, `project_to_test_plane`, `point_in_region` | test-plane geometry |
| `SelectedCell`, `region_center_depths`, `select_patch_cells`, `compute_frontier`, `iter_step_tuples` | cell selection (design §5) |
| `atom_aabb`, `count_overcoordinated`, `place_debug_tile` | helpers |
| `apply_patch` | the cut → place → weld → drop → passivate pipeline (design §5–6) |
| `DEFAULT_BUILD_THRESHOLD` | domain constant (from `patch_build.rs`) |
| `validate_tiling_vectors`, `extract_patch_tile` | the authoring half (design §4) |

**Verified:** every moving block listed above contains **zero** references to
`crate::`, `NetworkResult`, `StructureDesigner`, `NodeType`, `EvalOutput`,
`TextValue` or `DataType`. Their
only imports are `AtomicStructure`, `UnitCellStruct`, `Structure`, `GeoNode`,
`ImplicitGeometry3D`, `DAABox`, `weld_coincident_atoms`, `add_hydrogens`,
`TagError`, `Hybridization`, `covalent_max_neighbors` — every one of them
already in `atomcad-crystolecule` or below it. No new dependency edge is
created. All of those imports become `crate::…` once inside the crate.

### 1.2 What stays

`PatchLatticeFillData` / `PatchBuildData` (serde node data, `RefCell` report
cache) with their `default_*` fns and `impl Default`, plus `read_patch_record`,
`region_structure`, both `impl NodeData` blocks and both `get_node_type()` —
everything that speaks `NetworkResult`. `patch_latticefill.rs` drops from 1,035
to ~500 raw lines; `patch_build.rs` from 402 to ~265.

The `default_tolerance()` / `default_epsilon()` fns stay but now import their
constants from `crystolecule::patch` (**D2**).

### 1.3 Why `crystolecule` is the right home

`weld_coincident_atoms` — the primitive the whole feature rests on — was added
to `crystolecule` for exactly this feature (`doc/design_surface_patches.md` §3).
`Atom`'s **patch-ghost flag is already bit 6 of `Atom.flags`** in
`atomic_structure/atom.rs`, with a doc comment naming `patch_build`. The domain
crate already owns the patch data model; it just doesn't own the algorithm that
reads it. A `crystolecule::patch` module sits naturally beside `weld.rs` and
`lattice_fill/`.

### 1.4 Tests

**Paths below are as they stand after `doc/design_rust_crate_split.md`:** the
structure-designer suite is `crates/atomcad-structure-designer/tests/…`, the
crystolecule suite is `crates/atomcad-crystolecule/tests/…`, and only the root
package's own suites (`tests/integration/`, `tests/structure_designer_api/`,
`tests/renderer_api/`) still hang off `rust/tests/`. A bare
`tests/structure_designer/` no longer exists.

`crates/atomcad-structure-designer/tests/structure_designer/patch_latticefill_test.rs`
(1,224 lines, 23 tests) imports **only**
`atomcad_crystolecule`, `atomcad_geo_tree`, `atomcad_util`,
`glam`, and exactly four symbols from the moving set — `CompatibilityReport`,
`apply_patch`, `region_center_depths`, `select_patch_cells`. Zero
`StructureDesigner`, zero network construction. It moves wholesale to
`crates/atomcad-crystolecule/tests/crystolecule/patch_test.rs`.

`crates/atomcad-structure-designer/tests/structure_designer/patch_build_test.rs`
(278 lines, 10 tests) is **mixed**: nine tests call `extract_patch_tile` / `validate_tiling_vectors` on
hand-built structures and move; `crystal_and_molecule_sources_yield_same_tile`
constructs `NetworkResult::Crystal` and `NetworkResult::Molecule` to assert
`extract_atomic()` yields the same tile from both — a node-level concern that
stays. **D3**

`crates/atomcad-structure-designer/tests/structure_designer/patch_record_test.rs`
(167 lines) tests the built-in `Patch` *record type*, not the algorithm.
It stays.

### 1.5 Callers to update

- `nodes/patch_latticefill.rs`, `nodes/patch_build.rs` —
  `use atomcad_crystolecule::patch::…`
- `src/api/structure_designer/structure_designer_api.rs:220` — imports
  `CompatibilityReport` from `nodes::patch_latticefill`; repoint to
  `atomcad_crystolecule::patch`. `PatchLatticeFillData` stays where it is, so
  the `use` splits in two. The `APICompatibilityReport` mapping at line 4909 is
  unchanged.
- Test harness registration in
  `crates/atomcad-structure-designer/tests/structure_designer.rs` and
  `crates/atomcad-crystolecule/tests/crystolecule.rs`.

The root package's `rust/tests/integration/patch_roundtrip_test.rs` needs no
change (it only names node types and `*Data` structs).

## 2. Gadget geometry utilities → `atomcad-display` (+ 50 lines to `crystolecule`)

**621 source lines: ~571 to `atomcad-display`, ~50 to `atomcad-crystolecule`.
No test lines** — neither file has any coverage today; §3.3 adds it for the
crystolecule half.

### 2.1 What moves and why

`utils/half_space_utils.rs` (432 raw / 298 net) and `utils/xyz_gadget_utils.rs`
(373 raw / 323 net) contain **zero `crate::` references** — verified across
both files, imports and bodies. They import `crystolecule::UnitCellStruct`,
`renderer::{Mesh, Material, tessellator}`, `util::hit_test_utils`, and —
decisively — **`atomcad_display::gadget::GadgetPickContext`**. They already
depend on the display crate; they are simply filed one layer above it.

They are shared infrastructure, not one node's private helper: eight node files
use them (`drawing_plane`, `facet_shell`, `half_space`, `free_move`,
`geo_trans`, `lattice_symop`, `structure_move`, `atom_edit/atom_edit_gadget`).
That is exactly the shape of an adapter-layer utility.

Nothing outside `atomcad-structure-designer/src/` references either module —
verified across `src/api/`, `rust/tests/` and every other crate — so this move
has no api-side fallout at all.

### 2.2 The Miller-index exception — D5

`half_space_utils` is **not** homogeneous. Two of its functions are pure
crystallography with no rendering dependency whatsoever:

- `simplify_miller_index(IVec3) -> IVec3` — reduce `(h,k,l)` by its GCD
- `generate_possible_miller_indices(max) -> HashSet<IVec3>` — enumerate reduced
  indices within a bound

Moving these into `atomcad-display` would file crystallography in the
domain→renderer adapter. They go to `atomcad-crystolecule` instead — either
appended to `unit_cell_symmetries.rs` or a **new `miller.rs`**. Use `miller.rs`:
§3 fills it out with two more Miller-index functions in the very next phase, so
it is a module from the start rather than a two-function drop box.

**This is the one part of Phase 2 that is not an import rewrite, and it reaches
three call sites the §2.1 count does not include.** The two functions are used
asymmetrically:

- `simplify_miller_index` has **no caller outside the file** — only
  `half_space_utils` itself, at line 232 (`tessellate_miller_indices_discs`) and
  line 422 (inside `generate_possible_miller_indices`). A plain `use` at the top
  of the moved `display::half_space_utils` covers it.
- `generate_possible_miller_indices` **is called from three node files by module
  path**, not through a `use`: `drawing_plane.rs:537`, `facet_shell.rs:479` and
  `half_space.rs:451`, all as
  `half_space_utils::generate_possible_miller_indices(...)`. A `use` inside
  `half_space_utils` is private and does **not** re-export, so those three paths
  stop resolving; `pub use`-ing it back into `display::half_space_utils` would
  fix them but is exactly the shim D7 forbids. **Repoint all three at
  `atomcad_crystolecule::miller::generate_possible_miller_indices` instead.**

Everything else in the file — `HalfSpaceGeometry`, `HalfSpaceVisualization`, the
handle/disc constants, the `tessellate_*` and `hit_test_*` functions — is
genuinely display code and moves to `atomcad-display`.

### 2.3 Module naming

Keep both filenames (`half_space_utils.rs`, `xyz_gadget_utils.rs`) rather than
renaming to match display's `*_tessellator` convention: they are half hit-test,
so `_tessellator` would be a worse name, and keeping them makes the diff a pure
import rewrite. Both define `tessellate_center_sphere` and a
`CENTER_SPHERE_*` constant family — keeping them as two sibling modules keeps
those distinct, so **do not merge them.**

## 3. Miller-index symmetry families → `atomcad-crystolecule::miller`

**~47 source lines down, ~10 left behind as a wrapper.** The smallest item, and
the only one that changes a signature and rewrites a caller.

### 3.1 What moves

`facet_shell.rs:283-372` holds two functions that answer a purely
crystallographic question — *given a Miller index `(hkl)`, what is its symmetry
family `{hkl}`?*:

- `generate_unique_permutations(a, b, c) -> Vec<(i32, i32, i32)>` — the six
  permutations of a triple, deduplicated and sorted for determinism. An
  associated function taking no `self`; it moves **verbatim**.
- `get_symmetric_variants` — permutes the absolute values, then enumerates every
  sign combination, skipping the sign flip on zero components. For `(1,1,0)`
  that yields the 12 members of `{110}`; for `(1,0,0)`, the 6 of `{100}`.

§1.1's item-boundary rule applies here too: `283` is `get_symmetric_variants`'s
`//` comment (not `///`), `generate_unique_permutations` closes at **372**, and
`374-378` is `hit_facet_by_ray`'s doc comment, which **stays**.

They land beside `simplify_miller_index` and `generate_possible_miller_indices`
in the `crystolecule::miller` module that Phase 2 creates. All four answer
questions about Miller indices and nothing else; `miller.rs` becomes a coherent
module rather than a two-function drop box.

### 3.2 Why this one is a refactor, not a move — D8

`get_symmetric_variants` cannot travel as it stands, for two reasons that took
reading the body to see:

1. **It returns `Vec<Facet>`, and `Facet` is node data** — a serde struct
   carrying `symmetrize` and `visible` alongside `miller_index` and `shift`.
   Those two fields are editor state, not crystallography, and the function
   hard-codes them (`symmetrize: false, visible: true`) on every variant it
   emits. `crystolecule` must not learn about them.
2. **It takes `&self` but never reads it.** Verified: the body touches only its
   `facet` argument and `Self::generate_unique_permutations`. The receiver is
   vestigial, which is what makes the extraction possible at all.

So it splits:

```rust
// crystolecule::miller — the domain question
pub fn symmetry_equivalent_indices(miller: IVec3) -> Vec<IVec3>

// facet_shell.rs — the node-state wrapper that stays (~10 lines)
fn get_symmetric_variants(&self, facet: &Facet) -> Vec<Facet> {
    symmetry_equivalent_indices(facet.miller_index)
        .into_iter()
        .map(|miller_index| Facet {
            miller_index,
            shift: facet.shift,
            symmetrize: false,
            visible: true,
        })
        .collect()
}
```

Three of the four call sites — `facet_shell.rs:162`, `213` and `469` — keep
calling the wrapper unchanged. The fourth is the exception below.

**Free cleanup, and the one place a behaviour change could sneak in:**
`split_symmetry_members` (`facet_shell.rs:239`, doc comment at `237-238`,
calling `get_symmetric_variants` at line 264) builds a
`temp_facet` with `symmetrize: true` and the original `visible` purely to feed
`get_symmetric_variants` — which reads neither field — and then overwrites
`variant.visible` on every result anyway. The extraction deletes that dance
naturally: call the wrapper on the facet's own `miller_index`/`shift` and drop
`temp_facet`. **The `variant.visible = visible` assignment must survive** —
the wrapper emits `visible: true`, and `split_symmetry_members` is the one
caller that needs the original value instead.

### 3.3 Tests — the reason this is its own phase

**The symmetry enumeration has no test coverage whatsoever.** The only facet
tests in the tree are `text_properties_test.rs`'s property round-trips, which
never call `get_symmetric_variants`, `generate_unique_permutations` or
`ensure_cached_facets`. Nothing outside `facet_shell.rs` calls either function,
and no snapshot test covers the resulting geometry.

That cuts both ways. Left alone it would make this the one phase without a
safety net — which is why this section specifies **two** new test files
rather than none. It also makes the move worth doing: `{hkl}` family enumeration is exactly
the kind of thing that should have a table test and cannot easily have one while
it is a private method returning node data.

The tests go in `crates/atomcad-crystolecule/tests/crystolecule/miller_test.rs`:

| Input | Expected family size | Note |
|---|---:|---|
| `(1,0,0)` | 6 | `{100}` |
| `(1,1,0)` | 12 | `{110}` |
| `(1,1,1)` | 8 | `{111}` |
| `(1,1,2)` | 24 | `{hhl}`, two equal components |
| `(1,2,3)` | 48 | general `{hkl}`, the full point-group orbit |
| `(-1,2,-3)` | 48 | same family as `(1,2,3)`; the function takes absolute values first |
| `(0,0,0)` | 1 | degenerate; pins today's behaviour rather than asserting it is right |

Assert the **set**, not the order — except for one test that pins
`generate_unique_permutations`'s documented sort. That sort exists so downstream
intersection geometry is deterministic; dropping it would be a silent
regression. Phase 2's `simplify_miller_index` and
`generate_possible_miller_indices` arrive untested too; add coverage for them in
the same file while it is open.

**Deriving the expectations.** `get_symmetric_variants` is a *private* method,
so it cannot be imported and characterised from a test crate as it stands. Two
honest ways to pin its behaviour, in order of preference:

1. **Scratch commit.** Temporarily mark it `pub`, add a throwaway binary or test
   that prints the variant set for each table row, record the output, then
   discard the scratch commit and use the recorded sets as literals in
   `miller_test.rs`. This pins *actual* behaviour, not intended behaviour.
2. **By hand.** The table's sizes are derivable from the algorithm — permute the
   absolute values, then take all sign combinations, skipping the flip on any
   zero component — and the sizes above were computed that way and cross-checked
   against the code. If a hand-derived expectation disagrees with the new
   implementation, **assume the test is wrong before assuming the code is**, and
   fall back to method 1 to settle it.

Do **not** write the expectations by running the newly-extracted function: that
asserts only that the code equals itself.

**A second test file, in the *other* crate — `miller_test.rs` cannot reach the
part of Phase 3 that can actually regress.** It lives in `atomcad-crystolecule`
and can only see `symmetry_equivalent_indices`. But the only *behaviour* this
phase changes is in `facet_shell.rs`: the new wrapper's hard-coded
`visible: true`, and the deleted `temp_facet` in `split_symmetry_members`
(§3.2). Leave that to the manual walkthrough and the phase stays unguarded in
exactly the place §3.2 flags as at risk.

It does not have to be, and this half needs **no** scratch commit:
`split_symmetry_members` is already `pub`, and `FacetShellData` and `Facet` are
`pub` with `pub` fields — no `StructureDesigner`, no network, no
`NetworkResult`, so it can be characterised from a test crate directly against
today's code. Add
`crates/atomcad-structure-designer/tests/structure_designer/facet_shell_symmetry_test.rs`
and register it in
`crates/atomcad-structure-designer/tests/structure_designer.rs`. The load-bearing
case: build a `FacetShellData` holding one facet
`{ miller_index: (1,1,1), symmetrize: true, visible: false }`, call
`split_symmetry_members(0)`, and assert it returns `true`, yields **8** facets,
and that **every** one has `visible == false` and `symmetrize == false`. Two
more cheap rows while the file is open: splitting a non-`symmetrize` facet
returns `false` and leaves `facets` untouched, and an out-of-range index returns
`false`.

## 4. Considered and dropped — CIF → structure assembly

**Decision: not done.** Recorded in full because the candidate looks clean on
inspection and will be re-proposed by anyone who greps for `crate::`-free code
under `nodes/`. It is clean. It is still the wrong move, for the reason in §4.2.

### 4.1 What it would have moved

`nodes/import_cif.rs:276-530` (204 source lines) plus the `CifImportResult`
struct at line 39, into `crystolecule::io::cif`:

| Symbol | Role |
|---|---|
| `CifImportResult` | `{ unit_cell, atomic_structure, motif }` |
| `build_cif_import_result` | assemble all three from a `CifLoadResultExtended` |
| `build_motif_bonds_from_cif` | map `_geom_bond_*` records onto expanded sites |
| `resolve_cif_bond_atom` | resolve a label + symmetry code to a site index |
| `add_cif_bonds_to_structure` | apply CIF bonds to the `AtomicStructure` |

**Verified:** zero `crate::` references in that range. Every type it names —
`CifLoadResultExtended`, `CifBond`, `CifAtomSite`, `SymmetryOperation`,
`ExpandedAtomSite`, `Motif`, `MotifBond`, `Site`, `SiteSpecifier`,
`UnitCellStruct`, `AtomicStructure`, `infer_motif_bonds`,
`auto_create_bonds_with_tolerance` — already lives in `atomcad-crystolecule`,
most of it in `io/cif/` itself.

All of it verified `crate::`-free, and `CifImportResult` is `#[serde(skip)]`
with no `Serialize`/`Deserialize` derive of its own, so there would have been no
cross-crate serialization contract and no `.cnnd` implication. Mechanically this
would have been the easiest of the four candidates.

### 4.2 Why it was dropped — D6

`crystolecule/src/io/cif/mod.rs` documents `load_cif` as:

> The caller is responsible for converting to AtomicStructure/Motif and for bond
> inference.

That boundary is **deliberate and written down**, not an accident of where
someone stopped typing. Moving the assembly across it would overturn a recorded
decision, and the gain does not justify that:

- **It costs `io/cif` its leaf status inside `crystolecule`.** Today `io/cif`
  depends on nothing but `atomic_constants` and `unit_cell_struct`. After the
  move it would depend on `motif`, `motif_bond_inference` and
  `atomic_structure_utils` as well. There is no cycle and no build-graph change
  — they are intra-crate peers — but a file-format parser that pulls in the
  bond-inference stack is a heavier thing than one that hands back raw sites.
- **The gain is 204 lines and one test.** §1–§3 each move a *complete* domain
  concept — the patch model, gadget geometry, Miller-index symmetry — whose data
  model or siblings already live downstairs. This moves a conversion step across
  a boundary that was drawn on purpose.

The counter-arguments were real and are recorded so they are not mistaken for
oversights: `xyz_loader::load_xyz` does return a finished `AtomicStructure` and
run `auto_create_bonds`, so the CIF rule is one module's local choice rather
than a crate-wide convention; and the "caller" is singular — the node's `eval`,
the node's loader and `api/structure_designer/import_cif_api.rs` all do the
identical thing. Neither outweighs overturning a deliberate boundary for 204
lines.

**If this is ever revisited**, the trigger to look for is a *second* consumer
that needs assembled CIF output and is not the `import_cif` node — at that point
the assembly genuinely belongs downstairs, and the `load_cif` doc comment must
be rewritten in the same commit so the module tells one story.

## Decision log

- **D1 — One module per concept, named for the concept.** `crystolecule::patch`,
  not `crystolecule::patch_latticefill`. The domain module is not named after
  the node that happens to call it. Same reason the node keeps its own name.
- **D2 — Domain constants move with the algorithm.** `DEFAULT_WELD_TOLERANCE`
  and `DEFAULT_BUILD_THRESHOLD` are properties of the patch model, not of the
  node's serde defaults; the node's `default_tolerance()` imports them from
  `crystolecule::patch`. They have no other referent in the tree.
- **D3 — Split `patch_build_test.rs`; do not move it whole and do not leave it
  whole.** A test's home is decided by what it imports
  (`atomcad-structure-designer/src/AGENTS.md`), and nine of the ten import
  nothing above `crystolecule`.
- **D4 — `validate_tiling_vectors` keeps its `patch_build:` error prefixes.**
  After the move a domain function names a node in its error text, which is
  mildly wrong. Left alone deliberately: no test and no Dart string asserts them
  (verified), so changing them is safe but would be a behaviour change smuggled
  into a relocation commit. Fix it separately if it ever matters. (The
  `patch_latticefill:` strings are all in `read_patch_record`, which stays.)
- **D5 — Miller-index helpers go to `crystolecule::miller`, not to `display`.**
  See §2.2. Splitting one file across two targets is more work than dragging it
  all into `display` — but that split is the whole point of the exercise, and
  taking the cheaper option would file crystallography in the adapter layer.
- **D6 — The CIF assembly stays in `nodes/import_cif.rs`.** Moving it would
  overturn `load_cif`'s documented boundary and cost `io/cif` its leaf status
  inside `crystolecule`, for 204 lines. See §4.2. "It has no `crate::`
  references" is a necessary condition for pushing code down, not a sufficient
  one — the receiving module has to *want* it.
- **D7 — No `pub use` shims left behind.** The crate-split doc's rule 5 is about
  flutter_rust_bridge visibility and does not apply here (nothing moving is
  Dart-facing), but leaving re-exports at the old paths would preserve exactly
  the misfiling this design removes. Update the ~13 call sites instead — plus
  the three in §2.2, which are the only ones where a shim would actually be
  tempting because the caller names the module rather than importing it.
- **D8 — §3 is a split, and it goes in its own commit with its own tests.** It
  is tempting to fold it into Phase 2, since both fill the same new `miller.rs`.
  Don't: Phase 2 moves function bodies unchanged and the compiler verifies every
  path it touches — including §2.2's three call sites, which fail to resolve if
  missed. §3 changes a signature and rewrites a call path where a wrong result
  still compiles, with **zero** existing test coverage. Mixing them means a
  Phase-2 regression and a Phase-3 regression are indistinguishable in the
  history. Characterisation tests go in **before** the move, against the current
  implementation.

## Phases

Each phase is one commit, green on its own.

**Ordering.** Phase 1 is independent and can go first or last. **Phase 3 depends
on Phase 2**, which creates `crystolecule::miller`. Phase 4 is documentation and
comes last. If only part of this lands, Phase 1 alone or Phases 2+3 together are
both coherent stopping points; Phase 3 without Phase 2 is not.

### Phase 1 — `crystolecule::patch`

1. Create `crates/atomcad-crystolecule/src/patch.rs`; add `pub mod patch;` to
   `lib.rs`.
2. Move the **four blocks** in §1.1's table into it — not the spans between
   them. Rewrite each `use atomcad_crystolecule::X` to `use crate::X`. No body
   edits otherwise.
3. Repoint `nodes/patch_latticefill.rs`, `nodes/patch_build.rs` and
   `structure_designer_api.rs:220`. The api `use` splits in two:
   `CompatibilityReport` from `atomcad_crystolecule::patch`,
   `PatchLatticeFillData` still from `atomcad_structure_designer`.
4. Move the tests out of `crates/atomcad-structure-designer/tests/` into
   `crates/atomcad-crystolecule/tests/`, keeping both filenames so git follows
   the renames (paths below are relative to each crate's `tests/`):
   - `structure_designer/patch_latticefill_test.rs` → `crystolecule/patch_test.rs`
     (whole file)
   - `structure_designer/patch_build_test.rs` → `crystolecule/patch_build_test.rs`
     (nine tests), leaving `crystal_and_molecule_sources_yield_same_tile` behind
     in the original file along with any helper it needs
   - register both new files in
     `crates/atomcad-crystolecule/tests/crystolecule.rs`; in
     `crates/atomcad-structure-designer/tests/structure_designer.rs` drop the
     `#[path]` entry for `patch_latticefill_test` (currently at lines 440-441)
     and keep the one for the now-single-test `patch_build_test`.
     `count_real_and_ghost` is used on both sides — by the moving tests at
     `patch_build_test.rs:65/89/122/142` and by the staying one at `221-222` —
     so it is copied, not moved.
5. `cargo fmt && cargo clippy && cargo test -j 4`.
6. **Diff `lib/src/rust/`** — must be empty.

### Phase 2 — gadget utils to `display`, Miller helpers to `crystolecule`

1. Create `crates/atomcad-crystolecule/src/miller.rs` with
   `simplify_miller_index` + `generate_possible_miller_indices`; add to `lib.rs`.
2. Move `utils/half_space_utils.rs` and `utils/xyz_gadget_utils.rs` to
   `crates/atomcad-display/src/`; add both to
   `crates/atomcad-display/src/lib.rs`. Inside `xyz_gadget_utils`, rewrite
   `use atomcad_display::gadget::…` → `use crate::gadget::…`. Inside
   `half_space_utils`, add `use atomcad_crystolecule::miller::simplify_miller_index;`
   — that is the *only* one of the two extracted functions it still calls
   (line 232); its own call to `generate_possible_miller_indices` left with the
   function in step 1.
3. Rewrite the ten `use crate::utils::…` sites across the eight node files named
   in §2.1, **and** the three module-path calls to
   `half_space_utils::generate_possible_miller_indices` at `drawing_plane.rs:537`,
   `facet_shell.rs:479` and `half_space.rs:451`, which now point at
   `atomcad_crystolecule::miller` (§2.2). Those three are not covered by the ten
   — do not leave a `pub use` behind to spare them (D7).
4. Delete `crates/atomcad-structure-designer/src/utils/` — both moved files and
   the two-line `mod.rs` that declares them — and the `pub mod utils;` line at
   `crates/atomcad-structure-designer/src/lib.rs:64`. Nothing else lives in that
   directory, so it goes away entirely.
5. `cargo fmt` (**expect reflow**: the import prefix gets longer) then clippy,
   test.

### Phase 3 — Miller symmetry families to `crystolecule::miller`

Not a relocation; read §3.2 and §3.3 in full first.

1. **Capture the current behaviour before changing anything.** Two outputs, both
   derived from the *unmodified* code:
   - the expected `IVec3` family for each of §3.3's seven table rows, recorded
     as literals for step 3 — §3.3 gives two recipes, since
     `get_symmetric_variants` is private today;
   - `facet_shell_symmetry_test.rs` itself (§3.3) — written, registered in
     `crates/atomcad-structure-designer/tests/structure_designer.rs`, and **run
     green against the current `temp_facet` code path** before step 4 touches
     anything. No scratch commit needed: `split_symmetry_members` is already
     `pub`.
2. Add `symmetry_equivalent_indices(IVec3) -> Vec<IVec3>` and
   `generate_unique_permutations` to `crystolecule::miller`. The latter moves
   verbatim; the former is `get_symmetric_variants` with the `Facet`
   construction replaced by bare `IVec3`s.
3. Add `crates/atomcad-crystolecule/tests/crystolecule/miller_test.rs` with the
   step-1 literals, plus coverage for Phase 2's `simplify_miller_index` and
   `generate_possible_miller_indices`. Register it in
   `crates/atomcad-crystolecule/tests/crystolecule.rs`.
4. Replace `facet_shell.rs`'s `get_symmetric_variants` body with the ~10-line
   wrapper in §3.2. The three call sites at lines 162, 213 and 469 are untouched.
5. Simplify `split_symmetry_members`: drop the `temp_facet` construction and
   call the wrapper on the facet's own `miller_index`/`shift`. **Keep the
   `variant.visible = visible` assignment** — see §3.2.
6. Re-run step 1's `facet_shell_symmetry_test.rs` unchanged. It must still be
   green: it is the only automated cover for steps 4–5, and the whole point is
   that the rewrite did not move it. Do **not** adjust its expectations to
   match the new code — a red result here means step 4 or 5 changed behaviour
   (almost certainly the `variant.visible = visible` assignment).
7. `cargo fmt && cargo clippy && cargo test -j 4`. Expect the test count to
   *rise* here — the only phase where that is correct.

Steps 1–6 land as **one commit**. The characterisation literals are worthless as
a guard if they are authored after the rewrite, which is why step 1 comes first
even though nothing is committed until the end.

### Phase 4 — documentation

1. `crates/atomcad-crystolecule/src/AGENTS.md` — add `patch.rs` and `miller.rs`
   to the module map and key-types table. Say what `miller.rs` is *for*
   (Miller-index arithmetic and symmetry families, no rendering), so the next
   `simplify_*`-shaped helper lands there instead of in a node file.
2. `crates/atomcad-structure-designer/src/AGENTS.md` — drop the `utils/` line
   (currently line 76) from the directory-structure block, and note that the
   patch core now lives in `crystolecule`.
3. `crates/atomcad-display/src/lib.rs` — its module list is the crate's own
   overview; keep the doc comment honest about the two new modules.
4. Root `AGENTS.md` — check the Subdirectory Instructions list for any path that
   no longer exists.
5. `doc/architecture_overview.md` — regenerate the crate-size table with
   `python scripts/architecture_diagram/crate_size_table.py` and update the
   "59 % of the backend" sentence.
6. `doc/design_surface_patches.md` and `doc/design_patch_cell_selection.md` —
   update the file paths they name for `apply_patch` / `select_patch_cells` /
   `extract_patch_tile`.
7. `doc/design_rust_crate_split.md` — note in Deferred / follow-ups that this
   design covered the domain-code-relocation follow-ups.

**No reference-guide update.** Nothing user-visible changes: no node, pin,
default, error message, menu item or panel is touched.

## Regression strategy

The whole design is behaviour-preserving, so the gate is simply that nothing
changes:

| Check | Expectation |
|---|---|
| `cd rust && cargo test -j 4` after Phases 1–2 | same pass count as before, ~5,054; **no test deleted, none added** |
| … after Phase 3 | count **rises** by the new `miller_test.rs` and `facet_shell_symmetry_test.rs` cases; still nothing deleted |
| `cargo clippy` | at or below the ~36-warning baseline |
| `flutter analyze` | unchanged, ~139 |
| `git diff --stat lib/src/rust/` after Phase 1 | **empty** |
| `.cnnd` round-trip (`patch_roundtrip_test`) | green, untouched |

The test-count invariant is the real safety net: a moved test that silently
stops being registered (a missing `#[path]` line in `crystolecule.rs` or
`structure_designer.rs`) shows up as a *drop*, which is otherwise very easy to
miss. Record the count before Phase 1. Phase 3 is the only phase allowed to move
the number, and only upward.

Two manual walkthroughs for the human maintainer, covering the parts with no
automated coverage. The Flutter smoke test is a pending manual step as always —
agents must not run it.

- **After Phase 2** (gadget rendering and picking): open a design with a
  `half_space`, `drawing_plane`, `facet_shell` and a `structure_move` node;
  confirm each gadget draws and each handle drags.
- **After Phase 3** (facet symmetry reaches the screen): on a `facet_shell`
  node, toggle `symmetrize` on a `(1,1,1)` facet and confirm 8 faces appear;
  then use *split symmetry members* and confirm the split facets inherit the
  original facet's `visible` state. This is confirmation that the behaviour
  reaches the UI, **not** the gate — §3.3's
  `facet_shell_symmetry_test.rs` is the gate, and it must be green before the
  walkthrough is worth doing.

## What was checked and rejected

Recorded so the next person does not re-derive it. The CIF assembly belongs on
this list too, but has its own section (§4) because it is the one candidate that
*passed* the mechanical test and was rejected on judgement.

- **`atom_edit/{selection,operations,minimization,hydrogen_passivation}.rs`
  (~2,300 lines).** Looks like domain code, is not: the math already lives in
  `crystolecule` (`AtomicStructure::hit_test`, `MolecularTopology`,
  `UffForceField`, `add_hydrogens`) and these files are provenance mapping, undo
  recording and active-node orchestration over `&mut StructureDesigner`.
  `selection.rs`'s 590-line `select_atom_or_bond_by_ray` is provenance
  bookkeeping around a single `hit_test` call.
- **`layout/` (814 lines).** Sugiyama layering and topological grid look
  generic, but every entry point takes `&NodeNetwork` + `&NodeTypeRegistry`.
  Extracting it means generalising over a graph trait — a redesign, same class
  as the `expr` decision (`doc/design_rust_crate_split.md`'s D8, unrelated to
  this document's D8).
- **The geometry primitive nodes** (`sphere`, `cuboid`, `extrude`,
  `free_sphere`, `rect`, …). Already thin wrappers over `GeoNode`; only
  `cuboid::create_parallelepiped_from_lattice` (~70 lines) is a candidate, and
  it is below the threshold on its own.
- **`apply_style.rs`'s rule parser** (~230 lines). Takes `Vec<NetworkResult>`
  directly; not portable without an intermediate representation.

## Deferred / follow-ups

Smaller misfilings, all verified, none worth their own commit. Fold each in when
next touching the file:

- **`atom_edit_data.rs:2671-2971`** (215 lines) — `atomic_structure_to_motif`,
  `generate_ghost_atoms`, `min_distance_to_unit_cube`. Pure
  `AtomicStructure`↔`Motif`↔`UnitCellStruct` conversion with zero `crate::`
  references, buried at the bottom of a 3,306-line node-data file. Belongs
  beside `crystolecule/motif.rs`.
- **`atom_edit/measurement.rs`** (181) and **`atom_edit/guideline.rs`** (84) —
  import nothing but `AtomicStructure` and `glam`.
- **`implicit_eval/ray_tracing.rs`** (49) — imports only `ImplicitGeometry3D`;
  belongs in `atomcad-geo-tree`.
- **`implicit_eval/surface_splatting_{2d,3d}.rs`** (267) — generic adaptive
  subdivision producing `SurfacePointCloud`, blocked only by
  `NetworkEvaluationContext` being threaded through. Generalise to a callback
  and it drops into `atomcad-display` beside `surface_point_tessellator.rs`.
- **Preference triplication** (~150 lines). `GeometryVisualizationPreferences`,
  `AtomicStructureVisualizationPreferences`, `BackgroundPreferences`,
  `MeshSmoothing` and `AtomicRenderingMethod` are each declared three times.
  The `api/` twin is the deliberate D9a Dart boundary; the
  structure-designer↔display pair is genuine duplication maintained in
  parallel, and a drift hazard. Making display's definitions authoritative
  needs serde on the display side and a `PrefColor` conversion decision.
