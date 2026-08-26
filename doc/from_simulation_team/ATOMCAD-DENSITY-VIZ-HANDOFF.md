# Density visualization for cube files — field-tested handoff

Audience: the atomCAD team and their implementation agents (Flutter/Dart/Rust
desktop app). Origin: eight iteration rounds of a working browser prototype
("One field, nine instruments") over real QE/PySCF spin-density cubes of
T-center supercells and passivated Si clusters, plus a validation experiment
through an external radiology viewer. Everything below was learned by
building, breaking, and measuring — not speculation. This document is
self-contained: no external references are required to implement it.

Terms: "field" = the scalar volume; "|v|" = absolute value of the field;
"iso" = isosurface level; "enclosed fraction" = defined in §2. Units:
positions in Å, field values in the file's own units (a.u. for densities).

How to use this document (for an agent): Parts 1–2 are constraints and
ground truths — violating them reproduces bugs we already hit and fixed.
Parts 3–4 are the two instruments to implement, with acceptance tests.
Parts 5–7 are secondary features, format guidance, and the trust process.
MUST/SHOULD/MAY have RFC meaning.

VERSION 2 (2026-08-26): reconciled against the production viewer build via an
independent evidence-graded review (companion file
`DELTA-from-viewer-build-session.md`, kept alongside as evidence — its file
and commit citations back every claim below marked [P]). [P] = TESTED in the
production WebGPU viewer on real data. Where the two sources disagree the
disagreement is kept visible and marked UNRESOLVED — resolve it by measuring
on YOUR grids, not by trusting either of us.

---

## Part 1 — Ground truths about cube data (each learned from a real failure)

### 1.1 Parse the cube by its line grammar, never by token scanning
Lines 1–2 are free text. PySCF writes dates there ("Fri Jul 31 ... 2026");
a scanner that hunts for the first integer token will read "31" as the atom
count. We shipped that bug; the fix is structural: line 3 = natoms + origin,
lines 4–6 = voxel counts + step vectors, then |natoms| atom lines, then one
extra line iff natoms<0 (orbital-count line), then whitespace-separated data,
z-fastest (`idx = (i*n2 + j)*n3 + k`). Values sit AT grid points, not voxel
centers. Sign of the voxel count encodes units: positive = bohr, negative = Å.

The DATA BLOCK has its own hazard, hit twice in production [P]. Two field
widths exist in the wild and a reader assuming either breaks on the other:
13 bytes/value (`%13.5E`, no separator — a negative value with a three-digit
exponent fills all 13 characters and ABUTS its neighbour, so whitespace
tokenizing silently returns fewer values than the header promises; a real
damaged line yielded 7 tokens where 18 values live) and 14 bytes/value (safe
to tokenize, destroyed by a hardcoded 13-char reader). Both widths exist in
our own artifact tree (measured: 14.169 vs 13.169 bytes/value on two sibling
cubes). Robust recipe: tokenize, COUNT against n1·n2·n3 from the header; if
short, re-split the block at fixed 13-char boundaries; if still short, report
truncation — never pad. The count check is load-bearing. Mitigating detail:
fusable values are < 1e-99, outside f32 range, so the failure mode is a file
that refuses to load, not a subtly wrong picture — keep it that way.

### 1.2 Non-orthogonal cells: transform, never drop, never guess
The three step vectors form the cell matrix S (columns). Hexagonal,
rhombohedral, and primitive-cell cubes have off-diagonal components.
Mainstream viewers silently ignore them and render a sheared molecule with no
warning — this is a documented source of "your program showed me the wrong
molecule" bug reports. Hierarchy: silent distortion < loud refusal < correct
transform declared on screen. A native app SHOULD render in cell coordinates
by pushing S into the model transform (do NOT resample the way our browser
POC had to). All world↔grid math goes through S and S⁻¹.

### 1.3 Periodic vs isolated is a classification you MUST make before geometry
Wrapping rules differ and using the wrong ones produces visible garbage:
- Wrapping atoms of an ISOLATED cluster teleports edge atoms across the box
  ("ghost atoms carried over from the other side" — we shipped this too).
- NOT wrapping a PERIODIC supercell splits the defect across eight corners
  (the interesting feature routinely sits at the raw cell corner: we measured
  face |v| at 91% of max on a real QE supercell before recentering).

Measured discriminators on real files (do not re-guess these):

| specimen                        | face-max/|v|max | atoms within 0.7 Å of a face |
|---------------------------------|-----------------|------------------------------|
| QE T-center supercell, spin     | 0.91            | 30%                          |
| PySCF Si clusters, spin         | 0.015–0.063     | 0%                           |
| PySCF molecules, density/ESP    | ≤ 3e-5          | 0%                           |

Rule: periodic ⟺ (max |v| over the six boundary faces > 0.15·max|v|) OR
(>15% of atoms lie within 0.7 Å of a face). Neither signal alone suffices:
tight cluster boxes put spin tails at 1.5–6% of max on faces, and a periodic
cube with a mid-cell feature has quiet faces but crystal atoms at fractional
0/1. Then:
- PERIODIC: recenter on the |v|max voxel; wrap sampling in fractional
  coordinates; wrap every atom to the image nearest the center (fractional
  delta into [−n/2, n/2)).
- ISOLATED: center on the DATA BOX, not the feature (feature-centering wasted
  ~half our frame as dead space on a vacancy cluster: 50%→29% after the
  switch); NO wrap for anything.

### 1.4 The boundary is sacred: no clamp, no cap, no extrusion
Outside an isolated cube's data box there is NO data. Clamping extends face
values as constant columns, and a constant-along-axis field renders as an
extruded contour ("extrusion" artifact — shipped and fixed). Zero-filling
without bookkeeping caps surfaces with a fake flat wall at the face. Correct:
zero the outside, keep a validity mask, and REFUSE to build any surface
element between a valid and invalid voxel — the isosurface then ends OPEN at
the data boundary, which is honest and (bonus) lets the user see inside.
Draw the data box as a faint wireframe whenever it is smaller than the view
frame so the open cuts explain themselves.

### 1.5 Vacuum and dynamic range: the field spans ~10 orders of magnitude
Ground truth about densities: almost all of the box is tails/vacuum, and the
measured span on a real 144³ QE spin cube is min nonzero 6.4e-12 to max
1.1e-1 — 10.2 orders of magnitude [P]. Consequences, all load-bearing:
- Linear anything (colormaps, sliders in value space, histograms) shows one
  blob and then nothing. Every mapping MUST be log or asinh
  (asinh(v/w) with a selectable pivot w handles signed fields cleanly).
- Total densities look like atoms everywhere; the information is in
  DIFFERENCES and localized fields (spin density, deformation density,
  orbitals). Offer difference tooling early.
- If you ever quantize/compress the field (we used signed 8-bit log for the
  embedded prototype data), there is a noise floor below which "surfaces"
  are dither foam, not physics. Any automatic level animation MUST be bounded
  above ~6× that floor; manual controls may descend into the foam but the UI
  must say what it is. (Our flood animation shipped as noise before this rule.)
- Smoothing: keep it OFF by default and disclosed when on. Smoothing moves
  the isosurface; a well-known desktop viewer applies a hidden Laplacian pass
  by default and it appears in no figure — treat that as the cautionary tale.
  Prefer showing the grid's true chunkiness (it honestly communicates
  resolution) or render analytically if basis data exists (see Part 6).
  ONE exception is legitimate and distinct [P]: RECONSTRUCTION smoothing —
  smoothstep applied to the fractional texel coordinate before the lookup —
  is C1-continuous, exact at sample points, costs no extra fetch, and does
  NOT move the isosurface; it removes trilinear faceting only. Allowed,
  still worth a word on the identity strip.

### 1.6 THE answer to "which iso level should the user watch": enclosed fraction
This is your team's stated biggest problem and it has a clean solution.
Parameterize the level slider by the FRACTION OF TOTAL |FIELD| the surface
encloses, not by raw value:

```
sorted  = sort(|v| over all voxels, descending)     // once per field
cumsum  = prefix sums of sorted
total   = cumsum[last]
isoForFrac(f): binary-search smallest k with cumsum[k] >= f*total → sorted[k]
fracForIso(iso): binary-search position of iso in sorted → cumsum[pos]/total
```

O(N log N) once, O(log N) per query — but scale it to the grid [P]: the
full sort is fine up to a few million voxels; at 26.6M voxels (a real
347×348×220 production cube) it is real work, and a 2048-bin LOG-SPACE
histogram built in one O(N) pass gives the same percentile within a bin
(stride-sampling ~200k voxels is a defensible cheaper variant). Use the
exact cumsum when N permits, the histogram above it; either way the READOUT
number must come from the exact CPU data (see Part 2B). UI: slider in
fraction space with a
LIVE dual readout: `|v| = 3.4e-4 a.u. · encloses 72.2% of ∫|v|`. Slider
mapping SHOULD be log in (1−f): f = 1 − 10^−(0.25 + 2.75·s), s∈[0,1], which
gives fine control where it matters. Default 0.72 for localized fields.
Why this is right: it is invariant to units, grid, and normalization; it
answers the scientific question ("how much of the electron/spin is inside
this surface?"); and it resolves a 26-year-old ambiguity — the field's
folk convention "0.001 a.u." encloses anywhere from 95% to >99% depending on
system, and people argue about it on mailing lists because no viewer prints
the number. Print the number, always, on the figure.
Also display the value range and a log-scale histogram (the team already
shows the range — good; add the histogram with the active window marked).
Corollary that cost us a view-dependent figure [P]: EVERY level-like
parameter — dead bands, display floors, skip thresholds — must share ONE
parameterization across ALL views. A fraction-based dead band in one view
and a physical isovalue in another made the same spin state look different
per view. One f, everywhere, always.

### 1.7 Messaging: display lineage is provenance
Everything between the file and the pixels is part of the result and MUST be
declared in a persistent, compact identity strip on the render, not in a
buried dialog: source filename · declared/guessed field kind · grid dims ·
any downsampling/quantization ("144³→72³ mean-pool · 8-bit log, floor
2e-4·max") · periodic/isolated classification and centering convention ·
cell-shape handling · smoothing state · renderer caps (see below) · the
enclosed-fraction readout. Principles:
- "Not shown" and "not there" must not look the same.
- "Not checked" and "checked and fine" must not look the same.
- Renderer limits are provenance too: if you cap vertex/splat/line budgets or
  apply LOD, silent decimation punches holes users WILL interpret as data
  (we shipped exactly this; a user read renderer decimation as physics).
  Either don't cap, or declare the cap on the strip.
- Fail closed and loudly. A refusal with a reason builds more trust than a
  best-effort render that might be wrong.

### 1.8 Cube semantics (responding to the "no semantic information" point)
Correct: cube is a generic scalar-field container. Recommendations:
- Preserve and DISPLAY both comment lines verbatim (they often carry the
  generator and date — provenance for free).
- Let the user declare the field kind (density / spin / ESP / orbital /
  difference) — it changes defaults: signed↔unsigned palette, default
  fraction, whether ± surfaces are drawn. Persist the declaration next to
  the file (sidecar), never inside it.
- Sniff, but only as a labeled guess: ∫v·dV over the cell ≈ integer N →
  likely an electron density of N electrons; ≈ small integer with signed
  values → spin density (≈ 2S); large ± values peaking at nuclei → ESP.
  Show "guessed: spin density (∫=2.0)" — a hint, never an assertion.
- A filename convention helps humans; the viewer MUST NOT depend on it.

### 1.9 Production-data truths that files will not tell you  [P]
- A cube's PRODUCED grid is not its CONFIGURED grid. A producer asked for
  0.2 bohr and wrote steps of 0.200527 / 0.200535 / 0.200300 — three
  different values, none the request — because it fits an integer point
  count across a padded extent. Two cubes are DIFFERENCEABLE only if their
  produced headers match: compare the four header lines byte-for-byte;
  comparing input settings proves nothing.
- Pseudopotential/ECP cubes are missing core electrons BY CONSTRUCTION.
  They are not comparable electron-for-electron with all-electron partners,
  and a viewer that infers atom positions from density maxima will place
  atoms wrongly. The file gives no warning — this must come from declared
  method or a sidecar; carry an ECP mask.

---

## Part 2 — What worked, what didn't (verdicts from ten built instruments)

| Instrument | Verdict | Transferable lesson |
|---|---|---|
| Additive raymarch fog | ANTI-PATTERN | Additive blending sums opposite signs to white mud and destroys occlusion+shading, the two strongest depth cues. Keep one as an A/B control if you like; never as default. |
| Shaded volume splats (gradient-lit, back-to-front) | OK | Fog can carry form if lit by ∇v and composited, not added. Still loses to surfaces for structure. |
| Monte-Carlo point swarm | Surprising win | Discrete points restore occlusion/parallax cheaply; samples of a distribution are honest. Great as underlay/context. |
| Sea-level isosurface + enclosed % + nested shells + cutaway | WIN (chosen) | Part 3. |
| Nodal (v=0) surfaces | Needs guidance | Physically meaningful (sign walls) but illegible without ± context ghosts and an explaining caption. |
| Gradient-ascent "rain" (basin tracers) | Nice overlay | Best over a faint swarm; as standalone it reads as abstract. |
| 2D slices: cine-scroll + asinh windows + contour overlay + click-to-pin printed values | ESSENTIAL | The quantitative workhorse. Radiologists rejected 3D for reading and scroll slices — copy them. Pinned printed values ("soundings") are the most honest mark on any figure. |
| Sweep: phosphor plane + static contour stack, x/y/z/all | WIN (chosen) | Part 4. "all"+stack (egg-crate) may be the best static representation of a field there is. |
| Shadowgraph/schlieren projections | Niche | Beautiful, physics-adjacent (∇² of column density), keep for later. |
| Column-integral terrain topograph | Good as minimap | Log column integral hillshaded as terrain reads instantly; also the orientation minimap 3D views need. |
| MIP (maximum intensity projection) | Validated externally | We ran our cubes through a medical viewer: full-slab MIP showed defect physics (satellite spin sites) instantly. Cheap; build it. |
| Audio probe (value→click rate under cursor) | Cheap win | A second sensory channel for exploration; trivial to add. |

Distilled perception rules: (1) occlusion + motion parallax beat every other
depth cue — a slow ROCKING camera is the cheapest 3D you will ever buy;
(2) motion must map to the camera or to a physical parameter, never be
decoration; (3) sparse beats dense — contours, points, wireframes over fog;
(4) signed fields need a two-hue palette (warm positive / cool negative)
with a DARK midpoint on dark background, and hue reserved for data only
(monochrome UI chrome); (5) the eye is a change detector — A/B blink
comparison outperforms side-by-side; (6) the instruments users keep are
procedures they can watch (a plane sweeping, water rising), not renderings
they receive.

Dead ends from the production build — warn the agent off these [P unless
noted]: fixed step COUNT for ray traversal (looks fine at 80³, breaks at
347³, artifact comes and goes with rotation — see Part 2B); deciding step
size from the locally sampled value (unsafe by construction — a vacuum ray
leaps a thin sheet of signal); splat fog as a first-class view (deleted;
the reusable idioms were sprite instancing and histogram top-K) [OBSERVED];
point cloud as the flagship (retired for per-scrub CPU rebuild cost, not
image quality) [OBSERVED]; mixed parameterizations across views (§1.6);
linear windows on molecular density (cannot spread it; measured on a real
cation cube); feature-centering an isolated system (measured 50%→29% dead
frame after the fix — §1.3).

---

## Part 2B — Renderer ground rules from the production build  [all P]

These come from the shipping WebGPU raymarch viewer, with file/commit
citations in the companion delta. They are renderer-architecture facts and
apply regardless of Flutter/Rust specifics.

### 2B.1 GPU float precision is a correctness boundary — BLOCKING
Measured on the 144³ QE spin cube (1.8M values): fp16's smallest normal is
6.1e-05; 72.97% of nonzero density values land in fp16's DENORMAL range,
0.11% flush to zero, and relative error on survivors runs median 5.3e-04,
p99 5.1e-02, max ~100%. An fp16 volume texture still DISPLAYS correctly
when the display floor sits above the crushed region — but NO PRINTED
NUMBER may be sourced from that texture. Enclosed fraction, pinned values,
line profiles, integrals: CPU-side, from the source array, always. If the
budget allows, use an f32 texture (4 B/voxel = 106 MB at 26.6M voxels);
either way the GPU copy is for pixels, never for numbers.

### 2B.2 Traversal (if raymarching)
- Derive the step SIZE from the finest data axis; a fixed step COUNT is a
  bug in disguise. Fixed count gave 0.42 samples/step at 80³ but 2.8 on the
  body diagonal at 347³ against a Nyquist target of ≤0.5 — and the small
  case looked fine for months, which is why this survives review. Fixing
  SIZE also beat fixing COUNT on cost: short rays exit early; the small
  case got FASTER while sampling 45% more finely. Ship voxels-per-step
  ≈ 0.5 and print it on the HUD.
- Empty-space skipping needs a BOUND, not a sample: precompute per-block
  (8³) maxima DILATED by one block in every direction, so an under-threshold
  cell guarantees its whole 3×3×3 block neighbourhood is skippable. Sample
  the occupancy with NEAREST, never LINEAR (interpolation reads below
  threshold between two occupied cells). Keep blocks CPU-side so
  re-thresholding scans thousands of blocks, not millions of voxels. Derive
  the threshold FROM THE DISPLAY MAPPING, not a constant, or a later display
  change reveals signal the traversal already skips.
- Skipping is not free: 39% cheaper per step at 66% empty, net overhead at
  16% empty. Auto-disable below ~20% empty and report which branch ran.

### 2B.3 Camera and coordinates
- Orthographic rays are PARALLEL: direction = the camera forward vector
  through the LINEAR part (w=0) of world→grid, never fragment−eye (that
  fans the rays). Under orthographic, zoom is a projection scale and the
  camera never dollies; fit routines widen near/far instead of moving the
  eye. Chemistry figures conventionally want orthographic.
- Axis permutation on upload: cube data is z-fastest, 3D textures are
  x-fastest. Create the texture as (nz, ny, nx) and bake the (k,j,i)
  permutation plus a HALF-TEXEL offset into world→uvw. A swapped axis pair
  yields a transposed volume that still looks like a molecule — invisible
  in review, caught only by a round-trip test (see §3.8).

### 2B.4 Transfer functions
- Colour and opacity need SEPARATE curves: one shared curve produced a pale
  pink wash on real data. Production maps colour through the asinh remap
  and opacity through t^γ (γ=8), composited Beer–Lambert.
- asinh knee placement is counterintuitive: BELOW the knee asinh is linear
  (crushes), ABOVE it is logarithmic (spreads). A knee at p99.5 put 99.5%
  of the data in the crushing half and the bonding region rendered
  invisibly. Correct: knee ~6 DECADES BELOW a p99.9 top. And the user
  control is "decades spanned", not "log base" (a normalised log is
  base-invariant).
- A diverging (signed) ramp must NEVER be reversed by theme: sequential
  ramps flip luminance with the theme, but flipping a signed ramp swaps the
  MEANING of the sign. Pin it.

### 2B.5 Scheduling, benchmarking, and hygiene
- Idle GPU burn: a capable laptop went system-sluggish with the viewer tab
  merely open. Render ONLY on control change (including damping coast),
  parameter change, or resize — never on a bare frame tick. Exception:
  while the rocking camera or an animation mode is active AND the window is
  focused/visible, continuous rendering is the point; suspend it when
  unfocused.
- Quality on demand: coarse step (×3) while the camera moves, full quality
  after ~180 ms settle. Measured 1.77× on a 144³ signed cube.
- Benchmark methodology: a single-pass A/B reported a genuine 1.77×
  speedup as 0.86× because the first arm carried warm-up. ALTERNATE arms
  within one run and synchronise on the GPU queue's completion signal;
  wall-clock around a submit measures nothing. Also: background tabs
  throttle frame callbacks to zero — assert the surface is visible before
  believing any timing (or any "hang").
- Theme contrast is data integrity: after a background tweak, bond grey sat
  at 1.09:1 and bonds vanished. Graphical objects need ≥3:1 (WCAG);
  structure colours are per-theme and the check lives in TESTS.

### 2B.6 Mesh extraction vs raymarching — UNRESOLVED, measure it
This document's Part 3 specifies marching cubes; the production viewer ships
single-pass raymarch. The production evidence: raymarch measured SLOWER than
a point cloud per file in every configuration tried, and raymarch was chosen
anyway for scrub-free level morphing and transfer-function headroom — not
speed. Nobody has measured marching cubes at 26.6M voxels on this data.
Production raymarch envelope (one laptop GPU, 3420×1742 @ DPR 2):
347×348×220 isolated density, 1206 steps, skip on: 38.6 ms; 144³ periodic
spin: 24.9 ms full / 14.1 ms draft; 80³ isolated, skip off: ~15 ms.
Instruction to the implementing agent: build ONE of the two paths only after
benchmarking both on YOUR target grid sizes and hardware floor; Part 3 §3.2
is the mesh-option spec, §2B.2 is the march-option spec. If you need 512³
(134M voxels) interactive, the balance shifts again.

---

## Part 3 — SPEC: Instrument "Sea Level" (isosurface explorer)

Purpose: the primary 3D instrument. An isosurface of |v| parameterized by
enclosed fraction, with nested shells, sign coloring, cutaway, and an
always-on accuracy readout.

### 3.1 Data pipeline
- Per loaded field, build the sorted-|v| cumsum structure of §1.6 once
  (Rust; trivially parallel).
- Level state = enclosed fraction f (not raw iso). All views share it.

### 3.2 Surface extraction
- Marching cubes at FULL grid resolution over |v| at iso(f). (Our prototype
  used per-edge point splats as a browser shortcut.) NOTE: mesh-vs-raymarch
  is an UNRESOLVED architecture question — read §2B.6 and measure before
  committing; this section is the mesh-option spec. Vertex normals from central-difference ∇|v| at vertex
  positions, NOT from face averaging: gradient normals are smooth even on
  chunky grids and cost nothing.
- Sign attribute per vertex: sign of underlying v (for spin/orbital fields
  the |v| surface is the union of the +v and −v surfaces; color them
  differently — see palette).
- Respect the validity mask (§1.4): no cell that touches an invalid voxel
  emits geometry. Surfaces end open at the data boundary.
- NO mesh smoothing by default (§1.5). If offered: off by default, iteration
  count on the identity strip.

### 3.3 Nested shells
- Modes: 1 shell, ×2, ×4. Shell ℓ uses fraction f·0.55^ℓ (log-spaced
  inward). Inner shells are BRIGHTER (they read as "higher ground").
- Palette (dark background assumed):
  positive: outer→inner [235,140,70] → [255,178,110] → [255,208,150] → [255,234,200]
  negative: outer→inner [88,140,235] → [130,175,250] → [175,205,255] → [215,232,255]
  Multiply by Lambert shade (light dir ≈ normalize(−0.42, 0.58, 0.70) in
  view space, two-sided: use |n·L|), floor 0.28.
- Rationale: gaps or cuts in an outer shell reveal the next contour down
  instead of void — hypsometric layers in 3D. This solved a real complaint
  ("empty shells").

### 3.4 Cutaway ("tunnel through shells")
- Toggle. Clip the camera-side half-space: discard fragments with
  view-space z in front of a plane through the volume center, normal =
  view direction. With nested shells this looks INTO the onion; with the
  rocking camera the cut plane sweeps the structure. Implement in the
  fragment shader (one dot product); do not rebuild geometry.
- Data-boundary open cuts (§1.4) are separate and always on.

### 3.5 Context layers (the "basemap")
- Atom skeleton, dimmed: bonds where dist < (r_cov(a)+r_cov(b))·1.18
  (r_cov: H 0.31, C 0.76, Si 1.11 Å...). For PERIODIC systems the distance
  test MUST use the minimum-image convention [P]: on the T-center supercell
  a plain Cartesian test left 42 of 63 silicons under-coordinated (mean
  degree 2.64 against the 4.0 the lattice requires); with minimum image:
  130 bonds, mean degree 3.94, zero under-coordinated. If you spatial-hash,
  size the cells by the cell's PERPENDICULAR widths, not vector lengths, or
  sheared lattices drop pairs. Non-Si atoms get accent color
  (gold) — in defect systems the hetero-atoms are the landmarks. Compute
  bonds PER LOADED FIELD (a cached global bond list against a new file's
  atoms was a crash we shipped).
- Faint floor grid + soft blob shadows under the surface (projected,
  alpha ≈ 0.35 accumulated): grounding costs little and helps depth.
- Data-box wireframe when data box ≠ view frame (§1.4).

### 3.6 The readout (non-negotiable)
Always visible while the instrument is active:
`|v| = {iso:.2e} {units} · encloses {100f:.1f}% of ∫|v|`
plus the identity strip of §1.7. If shells are on, the readout refers to the
OUTER shell. The fraction and value MUST be computed CPU-side from the
source array (§2B.1) — never read back from a GPU texture.

### 3.7 Optional: flood animation
Animate f through [0.30, 0.92] (never into the noise floor, §1.5) with
TRIANGLE pacing (sine dwells at its extremes — ours parked in the noise and
users called it "just noise"), ~24 s round trip, per-step readout updating.
Precompute a ladder of ~30 meshes lazily. SHOULD log birth/merge events of
connected components with their iso values — that log is the field's
persistence barcode, a future instrument for free.

### 3.8 Acceptance tests (picture-level invariants — test the OUTPUT)
1. fracForIso(isoForFrac(f)) = f ± 1/N for f ∈ {0.3, 0.5, 0.72, 0.9}.
2. Direct voxel integration of |v| inside the surface / total = readout
   fraction ± 0.5% on a synthetic Gaussian and on a real spin cube.
3. Synthetic two-Gaussian fixture: at high f one component, crossing a
   computable threshold two components appear (validates topology handling).
4. Isolated-cluster fixture: zero mesh faces cross the validity mask;
   every atom inside the view frame; max nearest-neighbor distance over
   atoms ≈ a bond length (ghost-atom detector: 2.34 Å good, 3.17 Å = bug).
5. Signed fixture (±lobes): + and − regions get distinct palettes; no face
   spans a sign flip without a vertex near v=0.
6. Axis round-trip [P-motivated]: write a cube with an asymmetric marker
   voxel at known (i,j,k); assert the rendered feature appears at the
   corresponding world position for all three axes (catches the transposed-
   volume bug that "still looks like a molecule").
7. Precision independence: the enclosed-fraction readout is identical
   (to 1e-6 rel) whether the render path is f16, f32, or mesh — because it
   never touches the render path (§2B.1).

---

## Part 4 — SPEC: Instrument "Sweep / Contour stack"

Purpose: the field as woven contour wireframes — the second chosen
instrument. Two modes sharing machinery; the level is the SAME enclosed
fraction f as Sea Level (shared state, one slider).

### 4.1 Contour machinery
- Marching squares per axis-aligned slice at levels +iso(f) and −iso(f)
  (signed field: two families, two colors — same palette as Part 3 outer
  shells). Segments in slice coordinates → world via the cell matrix.
- Line rendering MUST be anti-aliased with ~1.5 px weight; the whole
  instrument is lines.

### 4.2 Mode A — "stack" (static, the priority)
- Axis choice x / y / z / ALL. ALL is the star: the three orthogonal
  contour families woven together ("egg-crate"). Why it works: the families
  close mesh cells, so the eye reads a genuine surface with zero shading,
  while gaps between lines keep interior structure (e.g. a minority-spin
  channel) visible inside the cage.
- Density: DENSER than our prototype per the product decision — target
  step 1 (every slice) for single-axis, step 2 for ALL, with an explicit
  line-segment budget (e.g. 200k segments) and LOD stepping 2→3→4 only if
  the budget trips — and when it trips, SAY SO on the identity strip
  (§1.7: renderer caps are provenance).
- Cache the stack per (field, f, axis-set); invalidate on any change.
  Estimates from real spin cubes at 72³: single axis step 2 ≈ 5–15k
  segments; ALL step 3 ≈ 30–60k. At your native resolutions expect ~4×
  per halving of step.
- Depth cue: per-segment alpha by camera depth (0.35 far → 0.8 near) now
  that a native renderer can afford per-primitive sorting or depth fade in
  shader.

### 4.3 Mode B — "animate" (radar phosphor)
- A plane sweeps the chosen axis (w = 0.46·L·sin(4.2e-4·t), or three
  planes phase-shifted 2π/3 for ALL). On each new slice index, emit its
  contours; segments decay with alpha = exp(−age/950 ms), lifetime 2.6 s;
  draw the plane rectangle faintly. The eye integrates the sweep into a 3D
  model — this mode is the "procedure you can watch" and demos better than
  any still.

### 4.4 Rocking camera (shared with Sea Level; ship ON by default)
- yaw(t) = yaw_user + 0.36·sin(3.1e-4·t) rad
- pitch(t) = pitch_user + 0.30 + 0.05·sin(2.05e-4·t) rad
- User drag is ADDITIVE to the rock (never pause the rock on interaction —
  pausing kills the parallax exactly when the user is looking). Toggle
  available; wheel = dolly. These constants were tuned by eye across eight
  rounds; start from them.
- Justification: motion parallax is the strongest depth cue after occlusion
  and costs nothing. Even the "static" stack mode should rock.
- Interplay with render-on-demand (§2B.5): the rock is a continuous
  animation, so it renders continuously — but ONLY while the window is
  focused and visible; on blur/occlusion suspend both rock and rendering.
  The idle-burn rule wins whenever nobody is watching.

### 4.5 Acceptance tests
1. Every emitted segment endpoint satisfies |field| = iso(f) ± half a voxel
   of linear interpolation error (sample-check 1000 random segments).
2. ALL mode on a synthetic sphere: the three families agree — segment
   endpoints from different axes lie on the same sphere within ε.
3. Budget trip is visible: force a tiny budget in a test; assert the strip
   reports the LOD step.
4. Slider latency: f change → new stack visible < 200 ms at native res
   (precompute/parallelize until true).

---

## Part 5 — Secondary but required: 2D inspection

- SLICES: cine-scroll through any axis (mouse wheel), asinh windowing with
  three named presets (pivot w = max/2 "core", max/40 "valence", max/600
  "tails"), ±iso contour overlay tied to the shared f, and CLICK-TO-PIN
  printed values at points (small dot + "3.42e-4"). Plane label + slice
  coordinate always on screen (e.g. "x–y plane · z = +0.08 Å · 37/72").
- TOPOGRAPH minimap: log-compressed column integral along the view axis,
  hillshaded; doubles as an orientation inset for the 3D instruments.
- MIP: single-button full-slab maximum-|v| projection — validated to reveal
  defect satellite structure instantly.
- Line profile: two clicked points (or a picked bond) → v along the segment,
  log scale. Chemists live on these plots. Values from the CPU source array
  (§2B.1); with basis data (Part 6) evaluate analytically instead —
  resolution-free profiles.

## Part 6 — Molden support (your plan): confirmed pro, with traps

Supporting Molden (basis + MO coefficients) is worth it: semantics arrive
(orbitals, occupations, spin), and densities become analytic Gaussian
mixtures — evaluable at ANY point, which removes cube-resolution limits and
enables exact values in line profiles/pinned readouts. Traps we verified:
- Orbital SIGN is arbitrary (any per-orbital global flip is legal). Never
  compare/diff orbitals by sign; ± lobes of ONE orbital are meaningful,
  signs ACROSS files/calculations are not.
- Degenerate sets: individual orbitals within a degenerate set are an
  arbitrary rotation of each other. A "degenerate set" badge in the orbital
  list is mandatory or users will over-interpret.
- Symmetry labels: common codes (e.g. PySCF) only label abelian subgroups
  and will SILENTLY print wrong labels for C3v/Td systems; use a dedicated
  library (posym, libmsym) if you show irreps.
- Molden normalization conventions vary between generators (Cartesian vs
  spherical d/f, [5D]/[7F] flags) — validate ingestion against a reference
  implementation with fixtures (Part 7), or renormalize defensively.
- Licensing: do NOT integrate NBO (its license forbids redistribution/
  bundling; a public project died of this). If you want localized orbitals,
  IBO/IAO are license-clean and deterministic.
- Strategic note [P]: if the analytic (basis-set) path is adopted, the
  fp16/quantization concerns of §2B.1 become transitional — printed numbers
  come from analytic evaluation and the GPU texture is display-only by
  construction. That shifts effort toward the basis path and is a point in
  Molden's favor. Also carry the ECP mask (§1.9) into this path.

## Part 7 — Trust process (adopt from day one; cheap now, impossible later)

- ACCURACY LEDGER: a regenerable artifact (markdown + JSON, CI-produced):
  synthetic fixtures checked against EXTERNAL oracles (cube reading vs
  cclib/ASE/critic2; geometry vs a hand-computed affine; enclosed fraction
  vs direct integration), with stated tolerance, per-fixture max error, the
  exact code path each check covers, and an explicit "not covered yet"
  list. This pattern is shipping today in a solo-dev medical viewer
  (VoxelLab, MIT-licensed — worth reading) and it is the difference between
  claimed and demonstrated correctness.
- CUBE ZOO: keep every cube that ever failed or misrendered as a regression
  fixture and run the whole zoo on every parser/renderer change. Both of our
  nastiest bugs were found by contact with real files, not by reasoning.
  (We can provide a starter zoo: QE periodic spin cubes 144³, PySCF cluster
  spin/density/ESP cubes 80³, a synthetic hexagonal-cell cube, stretched-H₂,
  plus [P] real damaged-data-line fixtures and sibling cubes in BOTH data
  field widths — 13 and 14 bytes/value — that break each other's naive
  readers.)
- PICTURE-LEVEL INVARIANTS: test the rendered result, not just the code
  paths — the acceptance tests in §3.8/§4.5 are the pattern. One number like
  "max nearest-neighbor distance over displayed atoms" catches whole bug
  families no unit test sees. Another [P]: mean coordination number on a
  known lattice (Si must read ~3.9–4.0; 2.64 meant broken periodic bonding).
- CROSS-CHECK CHANNEL: cube→NIfTI is ~5 lines (nibabel, diagonal affine,
  declare mm≡Å); any oracle-validated medical viewer then provides an
  independent second opinion on what your renderer shows. We used this to
  validate our own displays.

## Part 8 — Open, unhandled, and unresolved (do NOT assume these work)

Carried forward VERBATIM from our internal rules ledger so no implementing
agent believes these cases are covered. As of 2026-08-26 the reference
implementation does NOT handle:
- non-cubic PERIODIC display frames (the frame is a cube sized by the
  longest cell vector; Wigner–Seitz or true-parallelepiped framing is
  unbuilt — periodic images at the edges of short axes are shown and
  declared, not eliminated);
- multi-molecule cubes;
- ultra-elongated cells beyond a 512-voxel-per-axis parser guard;
- a residual clamp inside the reference implementation's sampling helper on
  the isolated path (harmless behind the validity mask, but unify it if you
  port the code rather than the spec).
And the explicitly UNRESOLVED disputes an implementer must settle by
measurement: mesh vs raymarch (§2B.6) and exact-cumsum vs histogram
percentile at large N (§1.6 gives the decision rule).

Questions from us whose answers would change this document's advice —
please send back: (1) wgpu or custom Vulkan, and the minimum GPU target
(storage-texture and 3D-filtering support decide whether fp16 is even a
choice); (2) will any printed number be sourced from the GPU copy — if yes
§2B.1 is blocking; (3) what grid sizes must stay interactive (26.6M voxels
≈ 39 ms is our measured envelope; 512³ changes the architecture);
(4) orthographic or perspective by default; (5) does the Flutter shell or
the Rust layer own camera and picking (decides where the rocking camera,
render-on-demand, and click-to-pin live); (6) is analytic Molden evaluation
planned (see Part 6 strategic note); (7) a repo link, so corrections can
cite your code instead of describing ours.

## Appendix — reference implementation and materials on request
A self-contained HTML prototype implements every instrument named here
(browser, no dependencies) and can be handed over as executable
documentation, along with the starter cube zoo and the synthetic fixtures.
The two chosen instruments (Parts 3–4) map to its "Sea level" and "Sweep"
views; the constants quoted in this document are the ones running there.
Honesty note about our own house: the doctrine surfaces (identity strip,
enclosed-fraction control, declared-constant panel, provenance handling)
are implemented in that frozen reference implementation; our production
viewer is still porting them and currently ships a HUD (step count,
voxels/step, draft state, skip state, periodic/isolated inference + reason,
domain, ramp) — which is itself a reasonable minimum HUD checklist. The
companion `DELTA-from-viewer-build-session.md` carries the file-level
citations for every [P] claim in this document.
