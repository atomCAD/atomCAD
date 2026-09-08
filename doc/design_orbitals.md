# Orbital analysis (`orbitals` node + PySCF script) — design

> **Status:** draft for review (2026-09-08). Nothing implemented.

## 1. Goal

Let a user look at the electronic structure of a structure they designed in
atomCAD — canonical frontier orbitals, intrinsic bond orbitals (IBOs), spin
density, and a computed-versus-designed bonding comparison — without leaving
the node network and without a simulation server. The quantum-chemistry
calculation runs **offline**, by hand, through a Python script that uses
PySCF; atomCAD writes the script's input, reads the script's output, and does
all the viewing.

The feature is the concrete, compromised version of the "ideal orbital UX"
discussed before this doc: *the selection is the question*. Selecting atoms
(by tag now, by click later) answers "what is the bonding here" with IBOs;
selecting nothing answers "what are the frontier orbitals" with canonical
orbitals; overlaying the computed Lewis structure on the designed bond graph
answers "did the electrons agree with what I drew".

Out of scope: running PySCF from inside atomCAD, periodic calculations,
excited states, geometry optimisation, anything the simulation team's stack
will eventually provide through a server. The exchange formats defined here
are meant to survive that later integration unchanged.

## 2. Background the reader needs

- `import_cube` (`nodes/import_cube.rs`) loads one `.cube` into a
  `ScalarField` + the file's atom block; `isosurface` (`nodes/isosurface.rs`)
  turns a `ScalarField` into an `IsosurfaceData` *specification*, and the
  display conversion (`generate_isosurface_output` →
  `atomcad_display::isosurface::extract_isosurface`) runs marching cubes.
  The level lives in the value, the extraction resolution in preferences.
  The enclosed-fraction level mode (`auto_level`, `LevelBasis`,
  `DEFAULT_LEVEL_FRACTION = 0.72`) is the one that transfers between fields.
  This design reuses all of it and adds no second isosurface pipeline.
- **Effect nodes** (`export_atoms`, `print`, `foreach`) return `Unit` and are
  only evaluated in an Execute pass (`doc/design_node_execution.md`, central
  skip rule). A node with real outputs is *never* evaluated in Execute mode
  with `context.execute == true` as its trigger, so a node that both shows
  results and writes a job cannot do the write from `eval`. See D1.
- `import_cube`'s **Load** is an API action (`import_cube_api::import_cube`)
  that reads the file into `#[serde(skip)]` node data; setters must preserve
  that payload (`ImportCubeData::with_file_name`, the import-node payload
  wipe). Same pattern here.
- `export_visible_atomic_structures` is the precedent for an API action that
  evaluates structures and writes a file *outside* an Execute pass.
- Tags: `AtomicStructure::atoms_with_tag`, `atom_tags`, `intern_tag`,
  `build_tag_remap` (`doc/design_atom_tags.md`). Tags survive diffs and are
  the selection language of this feature.
- A displayed node has **one `NodeOutput` per pin** (`NodeSceneData::pin_outputs`,
  `displayed_pins`), so a node can show lobes on one pin and atoms on another
  at the same time. `NodeOutput::Isosurface(SurfaceMesh)` already exists.
- Adding a `DataType` variant is a known, mechanical cross-language step: the
  fourteen touchpoints listed in `doc/design_scalar_fields.md` §"Touchpoints"
  plus FRB codegen. `ScalarField` was the last one added this way.

## 3. Decisions

**D1 — One viewer node, `orbitals`; the job is written by an API action, not
by `eval`.** The node has real outputs (`OrbitalSet`, `HasAtoms`), so the
central skip rule means its `eval` is a display-pass evaluation and must stay
side-effect free. *Export job* is therefore a property-panel button backed by
`orbitals_api::export_job(scope_path, node_id)`, which evaluates the node's
`molecule` input through the normal evaluator (the way `evaluate_node` /
`export_visible_atomic_structures` do) and writes `request.json`. It is not a
right-click Execute action.

*Why one node and not an `export_job` effect node + an `orbitals` viewer:* a
result is only valid for the geometry it was computed on, and in a parametric
network the geometry changes under the user. A single node that owns both the
job it wrote and the results it loaded can compare the two (D5) and say so;
two nodes coupled by a directory string cannot. The cost is that batch
generation of jobs through `foreach` is not available in phase 1. When it is
wanted, add a `Unit`-returning `orbital_job` effect node that calls the same
`write_request` function; nothing here precludes it.

**D2 — Tags select the job; the panel filters the view.** The job carries a
`focus_tag`; the script produces lobes (cube files) for every IBO that touches
a focus atom and for the requested frontier canonical orbitals. Viewing
filters the orbital list by a tag (phase 1) or by viewport selection (phase
3). A `cap_tag` marks the hydrogen caps of a carved cluster so orbitals that
spill onto them can be flagged as boundary-contaminated.

**D3 — Tables and fields travel separately.** `manifest.json` always carries
the *complete* per-orbital tables (energies, occupations, per-atom
populations, kinds, partners) for every orbital; they are kilobytes. Cube
files are megabytes and are written only for requested orbitals. The script
keeps the PySCF checkpoint (`scf.chk`); when `run` finds a checkpoint whose
geometry hash matches the request it skips the SCF and only writes the cubes
that are missing, so asking for more lobes later costs seconds, not a new
SCF. The user sees **one** button (*Export job*) and **one** command (`run`)
in both cases — there is no separate "request lobes" action, because two
ways of sending work to the script is one too many. This is the practical
stand-in for "any orbital on the fly", and it is what makes the Lewis overlay
and every list in the panel work with **no** cube loaded.

**D4 — Bonding comparison happens in Python.** `request.json` carries the
designed bond graph; the script classifies IBOs into bond / lone pair /
unpaired / core from their per-atom populations, diffs the bond IBOs against
the designed graph (presence *and* order), and writes the verdict (`lewis`)
into the manifest. The Rust side reads verdicts and draws; it never
re-derives chemistry.

*What the check can and cannot say.* It compares the electrons with the
drawing **at the designed positions**. It flags a bond drawn at a
non-bonding distance, a bond present at bonding distance but not drawn,
wrong bond orders, and radicals that are not where the drawing expects
them. It cannot flag a geometry that is self-consistent but not what the
atoms would do — an unreconstructed surface whose drawing also has no dimer
bonds passes. That second question is answered by **forces** (D4b), which
is why they are part of the same job.

**D4b — Forces are computed and shown alongside the orbitals.** The SCF's
nuclear gradient costs about one more SCF's worth of work and gives the
force on every atom at the designed geometry. The manifest carries the per-
atom force vectors and the largest magnitude; the node shows "largest force
*f* eV/Å on atom *N*" next to the bonding summary and, as a display toggle,
force arrows on the atoms. Large forces mean "these atoms want to move" even
when the bonding check is green — the signature of a reconstruction routine
that did nothing, a strained cover, or a bond drawn slightly too long.
Forces say nothing about where the atoms would end up; that is a relaxation
and remains the simulation team's job. `produce.forces` defaults to `true`
and can be switched off for speed.

**D5 — Staleness is detected by geometry hash.** Both files carry a
`geometry_hash` computed identically on both sides (§5.3). At `eval` the node
recomputes the hash of its current input and compares it with the loaded
manifest's. A mismatch is a **non-blocking** `NodeDataError::warning`
("results were computed for a different geometry — export the job again");
the results are still shown, because the user may be looking at exactly the
before/after they intended.

**D6 — Results live in memory, not in the `.cnnd`.** The node persists the job
directory, the job settings, the `shown` list and the modification time of
the manifest it last loaded. The parsed manifest and any loaded fields are
`#[serde(skip)]`, reloaded by the node-data loader after deserialisation
(exactly `import_cube_data_loader`), and **every property setter preserves the
payload** (the `with_file_name` rule). Loading results is not an undo step;
toggling `shown` is (it is persisted node data and goes through the generic
snapshot).

**D7 — One level for all lobes, in enclosed-fraction mode.** The node stores a
single `level_fraction` (default `0.72`) that applies to every shown orbital,
resolved per field through the existing `auto_level` / fraction machinery.
A per-orbital absolute isovalue is deliberately not offered: the handoff's
rule "every level-like parameter shares one parameterisation across all
views" is what keeps two orbitals comparable on screen. `orbital_field` +
`isosurface` remains available for anyone who needs an absolute level.

**D8 — Display through per-pin outputs.** Pin 0 (`orbitals: OrbitalSet`)
converts to one `NodeOutput::Isosurface(SurfaceMesh)` holding the union of
the shown lobes (components concatenated, so the existing transparency sort
keeps working). Pin 1 (`molecule`) converts to `NodeOutput::Atomic` carrying
the input structure plus, in phase 2, the Lewis-overlay styling. The node
overrides `default_display_all_output_pins() -> true` so a freshly added
node shows both, which is the integrated view.

**D9 — New `DataType::OrbitalSet`.** Needed so that `orbital_field` can select
one field out of a result and so that hover readouts / the panel can describe
what is loaded. It goes through the same touchpoints `ScalarField` did. No
other node consumes it in phase 1.

## 4. Job directory

One directory per job, addressed by the node's `job_dir` property, stored and
resolved like `export_atoms` paths (relative to the design when under the
design tree). Layout:

```
<job_dir>/
  request.json      written by atomCAD (Export job; rewritten on every export)
  manifest.json     written by the script
  scf.chk           PySCF checkpoint, written by `run`, read by `cubes`
  cubes/*.cube      one file per produced field
  log.txt           script log
```

Command (run inside WSL on Windows, see §7.5):

```
python scripts/atomcad_orbitals.py run <job_dir>
```

`run` does the SCF + localisation + tables + requested cubes on a fresh
directory, and **only the missing cubes** when `scf.chk` exists and its
geometry hash matches the request (§7.3). `--force` discards the checkpoint.

## 5. `request.json`

### 5.1 Shape

```json
{
  "format": "atomcad-orbital-request/1",
  "atomcad_version": "0.4.0",
  "written_at": "2026-09-08T14:31:00Z",
  "geometry_hash": "blake3:3f9c…",
  "atoms": [
    {"id": 17, "Z": 14, "pos": [1.3578, 1.3578, 1.3578], "tags": ["focus"]},
    {"id": 90, "Z": 1,  "pos": [4.2100, 0.0000, 0.0000], "tags": ["cap"]}
  ],
  "bonds": [[17, 23, 1], [17, 41, 1]],
  "charge": 0,
  "spin": "auto",
  "method": "pbe",
  "basis": "def2-svp",
  "focus_tag": "focus",
  "cap_tag": "cap",
  "produce": {
    "canonical": {"frontier": 3},
    "ibo": {"touching_focus": true, "keys": []},
    "densities": ["spin"],
    "forces": true
  },
  "grid": {"spacing": 0.25, "margin": 3.0}
}
```

### 5.2 Semantics

| Field | Meaning |
|---|---|
| `atoms[].id` | atomCAD atom id. Results are keyed by it; the script never renumbers. |
| `atoms[].pos` | Ångström, 4 decimals. |
| `bonds` | Designed bond graph `[id_a, id_b, order]`, for D4 only. PySCF ignores bonds. |
| `charge` | Total charge, default 0. |
| `spin` | PySCF convention, n_alpha − n_beta. `"auto"` = 0 for an even electron count, 1 for odd. A T-center cluster is odd and gets the doublet without the user knowing the word. Explicit integers allowed. |
| `method` | `hf`, `pbe`, `b3lyp`, `r2scan`. Default `pbe`. Unrestricted whenever `spin > 0`. |
| `basis` | Any PySCF basis name. Default `def2-svp`. |
| `focus_tag`, `cap_tag` | Tag names; either may be empty. |
| `produce.canonical.frontier` | N: write cubes for the N highest occupied and N lowest virtual canonical orbitals **per spin set**, plus the singly occupied one if present. |
| `produce.ibo.touching_focus` | Write cubes for every IBO whose population on any focus atom ≥ `IBO_TOUCH_THRESHOLD` (0.15). |
| `produce.ibo.keys`, `produce.canonical.keys` | Explicit orbital keys (§6.4): every key the user has ticked in the panel whose cube is still missing. Written by *Export job* from the node's `shown` list. |
| `produce.densities` | Any of `spin`, `total`. |
| `produce.forces` | Compute the nuclear gradient and write per-atom forces (D4b). Default `true`. |
| `grid.spacing` | Cube grid step, Å. `margin` is padding around the atoms, Å. |

### 5.3 `geometry_hash`

BLAKE3 over the UTF-8 string formed by joining, for atoms sorted by `id`,
`"{id}:{Z}:{x:.4}:{y:.4}:{z:.4}"` with `\n`, followed by
`"\ncharge={charge}\nspin={spin}"` after `auto` has been resolved to an
integer. Both sides implement it against this sentence; a shared test vector
lives in `tests/` and in the script's self-test. Rounding to 1e-4 Å is what
makes the hash stable across the f64 → JSON → f64 round trip. Method and
basis are **not** in the hash: the manifest's provenance names them, and
"same geometry, different method" is a comparison the user is allowed to make
without a warning.

## 6. `manifest.json`

### 6.1 Shape

```json
{
  "format": "atomcad-orbital-manifest/1",
  "request_hash": "blake3:…",
  "geometry_hash": "blake3:3f9c…",
  "provenance": {
    "program": "pyscf 2.6.2", "script": "atomcad_orbitals.py 1",
    "method": "UKS/PBE", "basis": "def2-SVP", "density_fitting": true,
    "charge": 0, "spin": 1, "n_electrons": 297,
    "converged": true, "scf_energy_hartree": -10234.5678, "wall_s": 612.4,
    "n_atoms": 76, "cap_atoms": 36,
    "max_force_ev_per_a": 1.42, "max_force_atom": 23
  },
  "canonical": [
    {"key": "can:a:148", "spin": "a", "index": 148, "energy_ev": -5.21, "occ": 1,
     "label": "SOMO", "populations": {"41": 0.62, "42": 0.18, "17": 0.07},
     "cube": "cubes/can_a148.cube"}
  ],
  "ibo": [
    {"key": "ibo:a:3", "spin": "a", "index": 3, "kind": "bond", "atoms": [41, 42],
     "populations": {"41": 0.51, "42": 0.47}, "touches_cap": false,
     "cube": "cubes/ibo_a003.cube"},
    {"key": "ibo:a:9", "spin": "a", "index": 9, "kind": "unpaired", "atoms": [41],
     "populations": {"41": 0.83, "17": 0.06}, "touches_cap": false, "cube": null}
  ],
  "atoms": [
    {"id": 41, "iao_charge": -0.12, "spin_pop": 0.79, "force": [0.03, -0.11, 0.05]},
    {"id": 23, "iao_charge": 0.04, "spin_pop": 0.00, "force": [1.39, 0.20, -0.22]}
  ],
  "lewis": {
    "confirmed": [[41, 42], [17, 41]],
    "missing": [[17, 23]],
    "unexpected": [[23, 29]],
    "order_mismatch": [{"atoms": [41, 42], "drawn": 1, "computed": 2}],
    "unpaired": [41],
    "lone": []
  },
  "densities": {"spin": "cubes/spin.cube"}
}
```

### 6.2 Rules the script follows

- **Complete tables, partial cubes.** Every canonical orbital and every IBO
  appears in its table; `cube` is `null` where no file was produced. Core
  orbitals are included with `"kind": "core"` so the tables are exhaustive,
  and the panel hides them by default.
- **Kind classification** from per-atom populations `p` (IAO charges of the
  orbital, summing to ≈ 2 for a spatial orbital in a restricted calculation
  and ≈ 1 per spin orbital in an unrestricted one; the script normalises to
  a sum of 1 before classifying): sorted descending, `p₁ ≥ 0.85` and the
  orbital is spatially contracted (radial extent below `CORE_EXTENT`) →
  `core`; `p₁ ≥ 0.75` → one-centre; two-centre when `p₁ + p₂ ≥ 0.85` and
  `p₂ ≥ 0.15` → `bond` with `atoms = [a₁, a₂]`; otherwise `multi` with all
  atoms above 0.10. A one-centre orbital is `lone` when a beta IBO with
  overlap ≥ 0.8 exists on the same atom, and `unpaired` when it does not
  (restricted calculations have no `unpaired`). Thresholds are constants at
  the top of the script and are echoed in `provenance`.
- **Lewis diff.** Designed bonds → set of unordered id pairs. IBO bonds → the
  `atoms` pairs of every `bond`-kind alpha IBO (beta IBOs of a closed-shell
  pair are duplicates and are not counted twice). `confirmed` = intersection,
  `missing` = designed only, `unexpected` = computed only. Multiple bonds
  contribute one pair to those three lists; the number of bond IBOs on a
  confirmed pair is compared with the drawn order and disagreements go to
  `order_mismatch` (a dimer whose dangling bonds paired into a π bond shows
  up here as drawn 1 / computed 2). `unpaired` and `lone` list atom ids.
- **Forces** (when `produce.forces`): `atoms[].force` in eV/Å, the force on
  the nucleus (negative gradient), in the request's coordinate frame;
  `provenance.max_force_ev_per_a` and `max_force_atom` summarise them. Forces
  on cap-tagged atoms are reported but excluded from the maximum, since caps
  are artefacts of the carving.
- **Labels** per spin set: `HOMO`, `LUMO`, `HOMO-1`, `LUMO+1`, …; in an
  unrestricted calculation the alpha HOMO with no beta partner is also
  `SOMO`.
- **Sign convention.** Each orbital's coefficient vector is multiplied by ±1
  so that its largest-magnitude IAO coefficient is positive. This makes the
  colouring stable between runs and between frames of a series.
- **`touches_cap`** is true when the summed population on cap-tagged atoms is
  ≥ 0.10.
- **Failure.** A non-converged SCF still writes a manifest with
  `converged: false` and whatever tables exist; the node shows a blocking
  error for the lobes but leaves the provenance readable. A crash writes no
  manifest and leaves `log.txt`.

### 6.3 Units

Energies in eV in the manifest (Hartree only for the total SCF energy, which
chemists quote that way); forces in eV/Å. Positions in the cubes are whatever `cubegen`
writes (Bohr), which `load_cube` already converts.

### 6.4 Orbital keys

`"{set}:{spin}:{index}"` with `set ∈ {can, ibo}`, `spin ∈ {a, b, r}` (`r` for
restricted), and `index` the set's 0-based index in the script's ordering.
Densities use `"den:spin"` / `"den:total"`. Keys are opaque strings on the
Rust side; they are what `shown` and `produce.ibo.keys` contain.

## 7. The script — `scripts/atomcad_orbitals.py`

### 7.1 Dependencies

`pyscf`, `numpy`; nothing else. PySCF has no Windows wheels, so on this
project's Windows machines the script runs in WSL (§7.5). It has a `--threads`
option that sets `OMP_NUM_THREADS` before importing PySCF.

### 7.2 `run`

1. Read `request.json`; validate `format`; resolve `spin: auto`.
2. Build `gto.M(atom=…, basis=…, charge=…, spin=…, unit="Angstrom")`.
   Atom ordering = request ordering; a table `pyscf_index ↔ atomcad_id` is
   kept for every output.
3. SCF: `scf.RHF/UHF` for `hf`, `dft.RKS/UKS` otherwise, `.density_fit()`,
   `chkfile = scf.chk`, `conv_tol = 1e-8`, `max_cycle = 100`. On
   non-convergence retry once with `newton()`, then give up (§6.2 Failure).
4. Canonical table: energies, occupations, labels; populations from IAO
   charges of each orbital (`lo.iao.iao` reference basis `minao`,
   orthogonalised, then `|c_iao|²` summed per atom).
5. IBOs: `lo.ibo.ibo(mol, occ_coeff, iaos=…)` per spin set; populations as
   above; kinds and cap flag per §6.2; Lewis diff.
6. Atom table: IAO charges (`Z − Σ populations`) and spin populations
   (alpha − beta).
6b. Forces (when requested): `mf.nuc_grad_method().kernel()` (density-fitted
   gradient for DF SCF), negated and converted from Hartree/Bohr to eV/Å;
   maximum over non-cap atoms. Skipped, with a log line, if the SCF did not
   converge.
7. Cubes for the produced set via `tools.cubegen.orbital` / `.density` with
   `resolution = grid.spacing / BOHR` and `margin`. File names as in §6.1.
   The two comment lines carry `atomcad {key} {label}` and the request hash,
   so a stray cube is still identifiable in `import_cube`.
8. Write `manifest.json` atomically (write to a temp file, rename).

### 7.3 Incremental `run` (checkpoint reuse)

Before step 3, `run` looks for `scf.chk` + `manifest.json`. If both exist,
the manifest's `geometry_hash`, method and basis equal the request's, and
`--force` is absent, it loads the checkpoint instead of running the SCF,
rebuilds IAOs/IBOs deterministically (same reference basis, same ordering,
sign convention re-applied), produces the cubes for every key in
`produce.*.keys` (and any newly widened `frontier` / `touching_focus` set)
that has `cube: null`, and rewrites the manifest with the new paths. Forces
are recomputed from the checkpoint only if the manifest lacks them and the
request now asks for them. Any mismatch falls through to a full run. The
log states which path was taken.

### 7.4 Determinism

Localisation is iterative; the same input must give the same orbital indices
so that `shown` keys survive an incremental run. The script fixes the IAO
reference basis, starts Pipek–Mezey from the canonical orbitals in energy
order, and sorts the resulting IBOs by (kind, first atom id, second atom id,
centroid) before assigning indices. Cross-run stability is tested by running
twice on the fixture and diffing the manifests.

### 7.5 Running on Windows

`job_dir` is written as a Windows path relative to the design; in WSL the
user passes `/mnt/c/...`. Everything inside the directory is relative, so
the script never sees a Windows path. The reference guide documents the
venv setup once (`python3 -m venv ~/pyscf-venv && … pip install pyscf`).

## 8. Rust data model

New module `atomcad_crystolecule::orbitals` (sibling of `field/`):

```rust
pub struct OrbitalManifest { … serde twin of §6.1 … }
pub struct OrbitalEntry { key: OrbitalKey, spin: Spin, index: u32, kind: OrbitalKind,
                          atoms: Vec<u32>, populations: Vec<(u32, f32)>,
                          energy_ev: Option<f64>, occ: Option<f32>, label: Option<String>,
                          touches_cap: bool, cube: Option<PathBuf> }
pub struct AtomAnalysis { id: u32, iao_charge: f32, spin_pop: f32, force: Option<Vec3> }
pub struct OrbitalSet {
    pub dir: PathBuf,
    pub manifest: OrbitalManifest,          // includes atoms: Vec<AtomAnalysis>
    pub manifest_mtime: SystemTime,
    fields: Mutex<HashMap<OrbitalKey, Arc<SampledField>>>,   // lazily loaded cubes
}
impl OrbitalSet {
    pub fn field(&self, key: &OrbitalKey) -> Result<Arc<dyn ScalarField>, OrbitalError>; // loads on first use
    pub fn entries_touching(&self, atom_ids: &[u32]) -> Vec<&OrbitalEntry>;
    pub fn geometry_hash(structure: &AtomicStructure, charge: i32, spin: i32) -> String;
}
```

`NetworkResult::OrbitalSet(Arc<OrbitalSet>)`, `DataType::OrbitalSet`, colour
and display string per the `ScalarField` touchpoints. The display string
names the directory, method/basis, convergence, counts of orbitals and cubes,
and the staleness verdict. The renderer never sees `OrbitalSet`; the display
crate sees only `IsosurfaceData` per shown lobe.

## 9. The `orbitals` node

### 9.1 Pins

| Pin | Type | Notes |
|---|---|---|
| `molecule` | `HasAtoms` | required; the structure to analyse (already carved and capped by upstream nodes) |
| `job_dir` | `String` | optional pin **and** property; wired value wins, as `export_atoms.file_name` |

Outputs: `orbitals: OrbitalSet` (pin 0), `molecule` same-as-input (pin 1).
Category: `Atomic`.

### 9.2 Properties (all in the text format, `get_text_properties`)

| Property | Default | Role |
|---|---|---|
| `job_dir` | `""` | job directory |
| `method`, `basis` | `pbe`, `def2-svp` | job |
| `charge`, `spin` | `0`, `auto` | job |
| `focus_tag`, `cap_tag` | `focus`, `cap` | job |
| `frontier` | `3` | job |
| `densities` | `[spin]` | job |
| `grid_spacing`, `grid_margin` | `0.25`, `3.0` | job |
| `shown` | `[]` | orbital keys the user ticked; drawn when their cube exists, exported as `produce.*.keys` when it does not |
| `level_fraction` | `0.72` | shared level, D7 |
| `opacity` | `0.4` | lobes |
| `forces` | `true` | job: compute forces (D4b) |
| `show_lewis`, `show_forces`, `show_spin_density`, `show_caps` | `true`, `false`, `false`, `true` | overlay toggles (`show_forces` in phase 1, the rest phase 2) |
| `loaded_mtime` | — | mtime of the manifest last loaded (for the "newer results" nudge) |

`#[serde(skip)] loaded: Option<Arc<OrbitalSet>>`.

### 9.3 `eval`

1. Evaluate `molecule` (forward errors verbatim on **both** pins).
2. If `loaded` is `None`, try to load `<job_dir>/manifest.json` (the loader
   also does this after deserialisation, so a saved design comes back with
   results). No manifest → pin 0 is a localized error "no results in
   `<job_dir>` — export the job and run the script", pin 1 still passes the
   molecule through, so the node is usable as a wire even before any
   calculation.
3. Staleness: compare `OrbitalSet::geometry_hash(input, charge, spin)` with
   the manifest's; mismatch → `NodeDataError::warning` (non-blocking, D5).
   `converged == false` → blocking error on pin 0 only.
4. Build the `OrbitalSet` result (Arc clone) and the pass-through molecule
   (phase 2: with per-atom charge/spin/force properties and Lewis styling).

### 9.4 Display conversion

`convert_result_to_node_output` for pin 0 walks `shown`, loads each field
lazily, builds an `IsosurfaceData` per orbital with the shared fraction
level (via `auto_level`'s fraction path), phase colours and opacity, extracts
each surface, and concatenates the `SurfaceMesh`es (components appended) into
one `NodeOutput::Isosurface`. A key in `shown` whose cube is missing is
skipped silently here and reported in the panel (it is the normal state
between ticking a row and the next export + run).

### 9.5 Subtitle and readouts

Subtitle: `"{n_shown} shown · {method}/{basis}"`, or `"no results"`, or
`"stale results"`. The pin-0 hover readout is the display string of §8,
which includes the largest force when forces were computed.

## 10. The `orbital_field` node

`(orbitals: OrbitalSet) → field: ScalarField`. Selector properties: `key`
(explicit orbital key) **or** `pick = {set, spin, atoms_tag_a, atoms_tag_b}`
resolved at eval as "the first `bond`-kind IBO whose two atoms carry the two
tags". Missing cube → localized error naming the key and telling the user to
tick it and export the job again. This node is what keeps `isosurface`, `sample_field` and
`print` usable on orbital data and is the escape hatch from the integrated
viewer's shared level.

## 11. API (`rust/src/api/structure_designer/orbitals_api.rs`)

All take `scope_path`. Register the module in `flutter_rust_bridge.yaml`.

| Function | Effect |
|---|---|
| `orbitals_export_job(scope_path, node_id) -> APIResult` | evaluates the input molecule, writes `request.json` (including the cube-less `shown` keys as `produce.*.keys`), sets the node's "job exported" state, refreshes |
| `orbitals_load_results(scope_path, node_id) -> APIResult` | reads the manifest into `loaded`, stores `loaded_mtime`, marks the node dirty, full refresh + validate |
| `orbitals_get_view(scope_path, node_id) -> APIOrbitalsView` | provenance strip, staleness, newer-results flag (stat of manifest mtime vs `loaded_mtime`), the orbital table filtered by an optional tag, atom table, Lewis summary |
| `orbitals_set_shown(scope_path, node_id, keys)` / `orbitals_set_property(...)` | property setters; all preserve `loaded` |

`APIOrbitalsView` rows: key, set, spin, kind, label, energy, partner atom
labels (element + id), population bars as `Vec<(String, f64)>`, `has_cube`,
`touches_cap`, `shown`.

## 12. Flutter panel (`lib/structure_designer/node_data/orbitals_editor.dart`)

Four groups, top to bottom.

- **Job.** Method, basis, charge, spin (`auto` / integer), focus tag and cap
  tag pickers populated from the input structure's `tag_names`, frontier
  count, densities, grid, job directory with Browse, and **Export job** — the
  only button that sends work out, whether a first calculation or extra lobes.
  After export: "Job exported HH:MM · run: `python scripts/atomcad_orbitals.py
  run <dir>`" with a copy button for the command.
- **Results.** Provenance strip (program, method/basis, charge/spin,
  converged, wall time), the staleness line ("matches current geometry" /
  amber "computed for a different geometry"), "newer results on disk" when
  the stat says so, the force line ("largest force 1.42 eV/Å on Si23", with
  a jump-to-atom link; amber above `FORCE_WARN = 0.5 eV/Å`, since a relaxed
  structure sits below ~0.05), and **Load results**. The stat runs when the panel is
  built and when the app regains focus; no file watcher.
- **Orbitals.** Filter row: tag dropdown ("touching tag …", default the
  focus tag), set toggle (IBO / canonical / both), show-core checkbox. Rows
  as in §11, sorted IBOs by kind then atoms, canonical by energy. Each row
  has a *show* checkbox (writes `shown`) and, when `has_cube` is false, a
  greyed "no lobe yet" marker that becomes "in next export" once ticked. The
  next **Export job** carries those keys and the next `run` produces only
  them. The Lewis summary (n confirmed /
  missing / unexpected / unpaired) sits at the bottom of the group in
  phase 2 and is clickable to filter the list to the offending orbitals.
- **Display.** Fraction slider (reuse `isosurface_editor`'s slider and
  readout, one for all lobes), opacity, phase colours, overlay toggles
  including *Force arrows* (scale slider, caps excluded by default).

Model methods follow the standard pattern: call API → `refreshFromKernel()`
→ `notifyListeners()`. Property edits go through the generic node-data
setter so they are undoable; Export/Load/Request are not undo steps.

## 13. Rendering

Phase 1 lobes reuse the isosurface tessellation and the three transparent
pipelines unchanged. Phase 2 overlay on the pass-through molecule:

- Confirmed bonds: default. Missing bonds: thin radius, `LEWIS_MISSING`
  colour. Unexpected bonds: added to the pass-through structure's bond list
  for display only (the node's output structure is a display decoration, the
  wire still carries the input) in `LEWIS_UNEXPECTED` colour. There is no
  dashed-bond primitive; colour + radius carry the distinction.
- Unpaired electron / lone pair: atom labels `•` / `••` through the existing
  label pipeline, plus a colour ring via the per-atom style map.
- Caps: rendered at reduced opacity through the style map when `show_caps`
  is on, hidden otherwise.
- Charges / spin populations: attached as per-atom properties for
  `apply_style` colour-by once the property channel exists; until then a
  label mode shows the number.
- Force arrows (phase 1): one line segment per atom from the nucleus along
  the force, length `scale · |f|` clamped to a maximum, drawn through the
  existing bond/line tessellation in a fixed arrow colour, with the largest
  one highlighted. Arrows are a `NodeOutput` decoration of the pass-through
  molecule pin, so they toggle with that pin.

## 14. Text format

Properties of §9.2 in the usual `name = orbitals { molecule: $x, job_dir:
"jobs/tcenter", method: "pbe", shown: ["ibo:a:3", "can:a:148"] }` form.
`shown` round-trips as a string array. `get_text_properties` must be total
(the round-trip corpus test requires it).

## 15. Testing

- **Rust unit tests** (`tests/crystolecule/orbitals/`): manifest parsing
  including `cube: null`, key parsing, `geometry_hash` against the shared
  test vector, `entries_touching`, lazy field loading, staleness detection,
  `orbital_field` selection by key and by tag pair, force parsing and the
  cap-excluded maximum.
- **Node tests** (`tests/structure_designer/orbitals_test.rs`): eval with no
  manifest (error on pin 0, molecule on pin 1), with a fixture manifest,
  stale warning, shown-key display conversion, cnnd round trip, text-format
  round trip added to the corpus.
- **Fixture:** a tiny real PySCF job checked in (`SiH4` radical cation or
  `CH3•`, def2-SVP, ~50 KB of cubes at coarse spacing) generated once by the
  script and stored under `tests/.../test_data/orbitals/`; plus a synthetic
  manifest produced by `make_cube_fixtures.py` for tests that need no cubes.
- **Script tests** (`scripts/test_atomcad_orbitals.py`, pytest, run manually
  in WSL): hash test vector, determinism (two runs, identical manifests),
  incremental run (second `run` with extra keys writes only new cubes and
  skips the SCF), kind classification on hand-built population vectors,
  Lewis diff including order mismatch, force units (a stretched H₂ gives a
  force along the bond of the expected sign and magnitude).
- **Manual walkthrough** in the reference guide: carve a Si cluster around a
  tagged defect, export, run, load, show the unpaired IBO.

## 16. Phases

1. **P1 — Loop closed.** `OrbitalSet` type; `orbitals` node with job
   properties, Export job, Load results, staleness, orbital list with tag
   filter and show checkboxes, lobes at the shared fraction level; forces
   with the largest-force readout and force arrows; `orbital_field`; the
   script's `run` with checkpoint reuse; tests; reference-guide page. Deliverable: the unpaired electron and the bonds around the focus
   atoms of a defect cluster on screen from one export, one command, one
   load.
2. **P2 — Lewis overlay.** `lewis` rendering (presence and order mismatch)
   on the pass-through molecule, per-atom charge and spin population
   readouts, cap dimming.
3. **P3 — Selection.** Viewport atom/bond picking for the active `orbitals`
   node filtering the list (reusing `atom_edit`'s picking path without its
   editing); ticking from the selection feeds the same export.
4. **P4 — Series.** Script `match` subcommand (overlap-matching IBOs against
   a previous manifest) and a timeline that keeps a selected orbital selected
   across frames; canonical energy ladder widget.

## 17. Open questions for review

1. Should P1 already include the `orbital_job` effect node for `foreach`
   batching (D1), or is the panel button enough until the approach-series
   use case is real?
2. Default method: `pbe` (fast, fine for shapes) or `b3lyp` (what many
   chemists expect to see quoted)?
3. Open-shell treatment: unrestricted (`UKS`, separate alpha/beta IBOs, spin
   contamination possible) versus restricted open-shell (`ROKS`, one set of
   spatial orbitals, cleaner pictures, harder localisation). Proposed: UKS.
4. Should beta IBOs be listed at all for closed-shell pairs, or collapsed
   into their alpha twin with an "×2" marker? Proposed: collapse in the
   panel, keep in the manifest.
5. Does `shown` belong in the `.cnnd`? It is view state, but it is also what
   makes a saved figure reopen the same way. Proposed: yes.
6. Grid spacing default `0.25 Å` — a 76-atom cluster with 3 Å margin gives
   roughly 90³ points, 3 MB per cube. Acceptable?
7. Naming: `orbitals` / `orbital_field` versus `qc_orbitals`; and whether
   `job_dir` should be `directory`.
8. Forces on by default (D4b)? They roughly double a PBE single point. The
   argument for the default is that the question they answer ("would these
   atoms stay put") is the one the bonding check cannot, and a user who
   wants speed can untick one box.

## 18. Documentation to update with the implementation

- `doc/reference_guide/nodes/atomic.md`: `orbitals`, `orbital_field`,
  and a "Computing orbitals with PySCF" workflow section including the WSL
  setup.
- `doc/reference_guide/nodes/math_programming.md`: `OrbitalSet` in the type
  list.
- `rust/crates/atomcad-crystolecule/src/AGENTS.md`: the `orbitals` module
  and the "tables vs fields" invariant.
- `rust/crates/atomcad-structure-designer/src/nodes/AGENTS.md`: the
  API-action-not-eval rule for a node that both shows results and writes a
  job (D1), since it is the first of its kind.
- `flutter_rust_bridge.yaml`: the new API module.
