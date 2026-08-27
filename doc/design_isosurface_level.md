# Design: choosing the isolevel — enclosed fraction and the distribution readouts

Companion to `doc/design_scalar_fields.md` (ingestion) and
`doc/design_isosurface_node.md` (extraction). Those build a working isosurface;
this one answers **which level to draw it at**. Prompted by
`doc/from_simulation_team/ATOMCAD-DENSITY-VIZ-HANDOFF.md` §1.6.

The node's level is a raw `f64` defaulting to `0.02`. That number is right for
an orbital amplitude and ~10x wrong for a density, a field spans ~10 orders of
magnitude so a text box is unusable, and the picture never states what the
surface encloses. This design adds a second coordinate (enclosed fraction), a
mode that picks the default from the field, and the distribution readouts that
make either coordinate legible.

**In scope:** the fraction parameterization and its `ValueDistribution`; the
`Auto` level mode; the dual readout; the editor's slider and histogram; the
colour domain's fit (a different statistic — §Part 4).

**Out of scope**, with where each belongs:

| Item | Belongs in |
|---|---|
| Nested shells, cutaway, flood animation (§3.3, §3.4, §3.7) | a display-side successor to `design_isosurface_node.md` |
| Periodic/isolated classification, minimum-image bonding (§1.3, §3.5) | beside the loader, in `design_scalar_fields.md` |
| 13-byte data blocks, negative voxel counts (§1.1) | same. All 16 zoo files tokenize cleanly, so that hazard is untested here |
| Declared field kind / `∫v·dV` sniff (§1.8) | its own design. It would **not** change the level — §Field-kind sniffing |
| Slices, MIP, line profiles (Part 5) | a separate instrument design |
| Raymarching, fp16, empty-space skipping (§2B) | not applicable — extraction is CPU `f64` |

## Part 1 — `ValueDistribution`

### Definition

The fraction is the share of the field's **total integrated |v|** inside the
surface — each sample's value read as a density. A *mass* fraction, **not** a
voxel count.

```
sorted  = sort(|v| over all samples, DESCENDING)
cumsum  = prefix sums of sorted
total   = cumsum[last]
iso_for_fraction(f)  = sorted[ smallest k with cumsum[k] >= f * total ]
fraction_for_iso(v)  = cumsum[ last k with sorted[k] >= v ] / total
```

Accumulating from the largest value down makes the enclosed region
`{ |v| >= iso }` — exactly what the extracted surface bounds.

**`fraction_for_iso` is a search, not a lookup.** Its argument is almost never a
stored sample: every caller passes an arbitrary magnitude — a level typed in
`Absolute` mode, `Auto`'s `DENSITY_LEVEL`, the dual readout in all three modes.
It is the mass of `{ |v_i| >= v }` over the total, found by binary search on the
descending array; `0` when `v` exceeds every sample, `1` when it is at or below
the smallest nonzero one. Reading it as an index into `sorted` — which is what
"position of v" would mean — is undefined for every real call.

**Mass, not count — the trap this exists to avoid.** On a 96³ box holding one
`exp(-2r)` blob, mass f = 0.72 gives iso = 2.33e-2 (0.058% of voxels, a
molecular envelope); the *voxel-count* 0.72 percentile gives iso = 1.61e-13 —
the vacuum, effectively the whole box. Eleven orders apart. Almost all of any
cube box is vacuum, so anything weighted by voxel count is dominated by it.

### Type

```rust
// atomcad-crystolecule/src/field/distribution.rs

#[derive(Debug)]
pub struct ValueDistribution {
    /// `sum |v_i|^exponent`.
    total: f64,
    /// Descending-sorted magnitudes + prefix sums. `None` above EXACT_SAMPLE_LIMIT.
    exact: Option<ExactCumulative>,
    /// Always present. 2048 log-spaced bins, each carrying mass and the
    /// running mass above it. **No per-bin counts:** nothing needs them — the
    /// editor plots mass, and the Auto rule is a ratio of two isovalues.
    histogram: LogHistogram,
    nonzero_range: Option<(f64, f64)>,
    /// Exactly-zero samples. Excluded from bins (log space has no home for
    /// them), reported separately so "all vacuum" is visible.
    zero_count: usize,
}
```

Queries: `iso_for_fraction(f)` and `fraction_for_iso(v)`. Both return `Option`,
`None` when `total == 0` — an all-zero field has no level to offer, and both
`Auto` and `Fraction` turn that into a descriptive evaluation error rather than
a divide by zero.

- `EXACT_SAMPLE_LIMIT = 4_000_000`. Below it, sort and prefix-sum; above it,
  the histogram resolves a percentile to within a bin. **A memory decision, not
  a speed one**, and the arithmetic depends on a layout worth stating: the
  magnitudes keep the storage width (`f32`) and only the prefix sums need
  `f64`, so an exact structure costs **12 bytes per sample**. A 144³ cube is
  3.0M samples ≈ 36 MB; the zoo's largest, 247x247x164 = 10,005,476 samples,
  would be 120 MB — affordable once, not once per field in a network holding
  several, and the fields are `Arc`-shared but the structures are not pooled.
  Both branches have real inputs — see Part 6.
- The histogram is built at **every** size, because it is also what the editor
  plots. The exact structure is a refinement on top, not an alternative.
- **The accumulation exponent is a parameter** (`1` today), not a hardcoded
  `abs`. Sole hook for a future declared field kind, which would use `2` for
  amplitudes.

### Weighting — two things that would otherwise be re-derived

- **`V_cell` cancels.** `∫|v|dV = Σ|v_i| · V_cell` with
  `V_cell = |det(axes)|` constant, so the ratio is unaffected by spacing or
  shear. No determinant anywhere. Worth a code comment.
- **Boundary samples are not half-weighted.** Exact trapezoidal integration on
  a node-centered grid weights the outermost planes by 1/2. Negligible for an
  isolated system; for a periodic supercell the boundary plane is a periodic
  image and genuinely double-counted, sub-percent on a ratio. Documented, not
  corrected — fixing it needs a periodicity classification this design lacks.

### Trait surface

```rust
/// `None` for any source with no stored samples (every analytic field), same
/// as `value_range` / `native_grid`. Consumers must handle `None`.
fn value_distribution(&self) -> Option<&ValueDistribution> { None }
```

`SampledField` overrides it, caching in a `OnceLock`. **Lazy, not eager in
`new`:** a field that is only `sample_field`-probed should not pay for a sort,
and `import_cube` builds fields during a file load where a multi-second pause is
very visible. Returns a reference — the payload is large, every caller reads.
`SampledField: Clone`, and the `OnceLock` clones as unset, so a clone rebuilds
rather than deep-copying.

**`f32` storage flushes deep vacuum to zero.** The zoo's `si-gemcut` density
reaches `1.6e-75`; those samples land in `zero_count`, not the lowest bin. The
histogram's lower edge is `f32`'s floor, not the file's — the readout must not
claim otherwise.

### The two queries are not a bijection

- `f → iso → f` round-trips within ±1/N. (Handoff §3.8 test 1.)
- `iso → f → iso` **does not.** Any isovalue between two adjacent sorted samples
  encloses the same set, so `fraction_for_iso` is a step function and
  `iso_for_fraction` returns one representative per step.

That asymmetry is why the node stores two numbers (Part 2).

## Part 2 — Node changes

### `LevelMode`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LevelMode {
    #[default]
    Auto,      // level chosen from the field; neither stored number is live
    Absolute,  // `level` is an isovalue magnitude in the field's units
    Fraction,  // `level_fraction` is the enclosed share of ∫|v|
}
```

Text spellings `auto` / `absolute` / `fraction`, via a `*_to_text` /
`*_from_text` pair as `Colormap` already does.

### `LevelMode::Auto`

```
r      = field.value_range()
scale  = max(|r.min|, |r.max|)
signed = r.min < -NEGATIVE_TOLERANCE * scale
dist   = field.value_distribution()                     # None for an analytic field

if dist is None:
    if signed: error                                    # Part 3 — no honest level exists
    level = DENSITY_LEVEL                               # basis: "non-negative, unchecked"
elif not signed:
    localized = dist.iso_for_fraction(LOCALIZED_FRACTION)
    if localized <= MAX_LEVEL_RATIO * DENSITY_LEVEL:
        level = DENSITY_LEVEL                           # basis: "non-negative, density-like"
    else:
        level = localized                               # basis: "non-negative, atypical"
else:
    level = dist.iso_for_fraction(LOCALIZED_FRACTION)   # basis: "signed field"
```

`DENSITY_LEVEL = 0.002`, `LOCALIZED_FRACTION = 0.72`, `MAX_LEVEL_RATIO = 125`,
`NEGATIVE_TOLERANCE = 1e-6`.

**Why Auto exists at all:** `node_data_creator` is
`|| Box::new(IsosurfaceNodeData::default())` — no arguments, no field. A
field-dependent default cannot be computed at node creation, so `eval` is the
only honest place for it.

**Why signedness is the whole discriminator:** the property that drives the
difference is the **nuclear cusp**. A total density has one and holds most of
its electrons in a minuscule volume (useful fractions 0.98–0.999); orbitals,
spin and deformation densities do not (0.5–0.9). Cusped fields are dominated by
densities, which are non-negative. Costs no new machinery — `value_range` is
already read for component count.

**Signedness is a tolerance, not `min >= 0`.** The two branches are an order of
magnitude and a half apart — on `si-cluster-S3-vacancy` the density branch gives
`0.002` and the fraction branch `8.70e-2`, a factor of 43 — so a raw sign test
turns one voxel of numerical noise at `-1e-12` into a 43x wrong level, silently.
Densities from a Gaussian basis are non-negative by construction, but a
plane-wave density interpolated onto a grid rings slightly negative, and the zoo
has no periodic *total* density to catch it. Scaling the tolerance by the
field's own magnitude costs nothing and closes the class. `1e-6` is safe by a
wide margin: the smallest genuine negative lobe in the zoo is
`si-cluster-S3-vacancy_spin`'s, at **2.7% of the field's scale** — four orders
of magnitude above the tolerance.

**The branches choose different *coordinates*, deliberately.** For densities a
fixed `0.002` tracks the vdW envelope better than a fixed fraction does
(measured `r/r_vdW` spread 1.097 vs 1.333 across C/N/O/F: heavier atoms hold
more density in the core, so a fixed fraction cuts further in as Z rises). For
signed fields no absolute convention exists — and an orbital's nominal
0.02–0.05 convention degrades with delocalization (`~1/√N`, since normalization
fixes the total), so the fraction is the better coordinate there too. Both
branches resolve to an absolute magnitude, so this costs nothing structurally.

#### The plausibility window

`0.002` is not trusted blindly: ELF (conventionally ~0.8 of a 0–1 range) and
RDG (~0.5) would take it and swallow the box. **The check compares `0.002`
against the field's own localized scale** — `iso_for_fraction(0.72)`, the number
the other branch would have picked. The question it asks is exactly the one that
matters: *is `0.002` a plausible level in this field's units at all?* Measured
on the zoo, with the box cropped about its centre to vary the padding:

| Field | `iso@0.72` | ratio to `0.002` | at 0.8x box | at 0.6x box | at 0.5x box |
|---|---|---|---|---|---|
| ch3 density (4 atoms) | 6.88e-2 | 34.4 | 36.5 | 45.0 | 56.9 |
| ch3cl density (5 atoms) | 1.30e-1 | 65.0 | 65.1 | 67.0 | **72.4** |
| NaCl density, ECP (2 atoms) | 4.30e-2 | 21.5 | 21.6 | 23.5 | 29.0 |
| Si-vacancy density (59 atoms) | 8.70e-2 | 43.5 | 45.9 | 47.8 | 54.3 |
| Si-gemcut density (149 atoms) | 8.92e-2 | 44.6 | — | — | — |
| **ELF** | 4.97e-1 | 248 | 247 | 240 | **228** |
| **RDG** | 5.43e+2 | 271284 | 8878 | 726 | 371 |

No density exceeds **72.4** at any padding; no impostor drops below **228**.
`MAX_LEVEL_RATIO = 125` sits at the geometric midpoint of that gap
(`sqrt(72.4 * 228) = 129`), leaving 1.7x of headroom below and 1.8x above.

**Why not the enclosed voxel share, which is the obvious statistic.** It reads
as the geometric failure directly — the surface swallows the box (ELF at 52.1%
of voxels above `0.002`) or finds no crossings at all and renders nothing (RDG
at 100%) against 9.7–36.7% for the densities — and it does not drift with system
size, where the mass share does (0.9945 to 0.9960 from 59 to 149 atoms). Both
true, and both beside the point: **the voxel share is a measurement of the box,
not of the field.** Cropping the padding moves it far harder than changing the
molecule does.

| Field | as shipped | 0.8x box | 0.6x box | 0.5x box |
|---|---|---|---|---|
| ch3 density | 36.7% | **65.2%** | 92.9% | 99.2% |
| NaCl density | 9.7% | 19.0% | **43.0%** | 59.1% |

A threshold of `0.45` rejects an ordinary CH3 density the moment its box is
drawn 20% tighter — sending it to `6.88e-2`, **34x** the right level, silently.
Every file in the zoo carries one padding convention (PySCF's default), so that
variable is precisely the one the calibration could not see. The ratio moves by
a factor of 1.7 across the same crops and never leaves its band. It is also
cheaper: it needs no per-bin counts and no third query, only the
`iso_for_fraction` the other branch already calls.

**What the window does not buy:** the right level for ELF/RDG. The fallback
gives ELF `0.497` (convention ~0.8) and RDG `542` (~0.5). It converts "nothing,
or the whole box" into "adjustable and wrong". The histogram is what actually
rescues those — and RDG shows why nothing better is on offer here, since its
fallback is itself padding-junk, swinging from `542` to `0.74` across the crops
above. *The `125` threshold rests on two impostor examples — calibrated, not
proven — but it is now calibrated against the confounder as well as the
examples.*

#### Known limitation: signed does not imply cusp-free

A field *derived* from the density inherits its cusp and can still be signed.
The zoo has two:

| Field | Range | f = 0.72 gives | Conventional |
|---|---|---|---|
| Laplacian of rho | −1.6e4 … +2.4e4 | 2985 | ~0.1–1 |
| sign(lambda2)*rho | −282 … +0.089 | 0.087 | ±0.05 (as a colour axis) |

Accepted, not fixed: both are exotic post-processed fields, visibly wrong at a
glance, with the histogram beside the control. **Do not claim they work.**

#### Field-kind sniffing

Handoff §1.8's `∫v·dV ≈ integer N` guess would **not change the level**. Its
headline distinction (density vs spin density) is one signedness already makes,
and it does not rescue ELF/RDG either — neither has an integer integral, so both
would fall to the same wrong fraction. Its payoff is the **label**
(`guessed: spin density`), which belongs with provenance work.

#### Handover

Switching **Auto to Fraction pre-fills `level_fraction` with the resolved
fraction**; **Auto to Absolute pre-fills `level` with the resolved magnitude**.
Both are exact, so the surface does not move. Auto is a starting point the user
takes over, never a locked mode.

**Auto is volatile by design** — the level moves when the field changes. For a
figure that must not change, switch to fraction or absolute. Say so in the
reference guide.

### Node data

```rust
pub struct IsosurfaceNodeData {
    pub level_mode: LevelMode,   // Auto by default
    pub level: f64,              // live under Absolute; keeps its 0.02 default
    pub level_fraction: f64,     // live under Fraction; (0,1) exclusive; 0.72
    // ... colours, alpha, colormap, colour domain — unchanged
}
```

**Two stored numbers, not one reinterpreted**, because `absolute -> fraction ->
absolute` is lossy (§bijection) and because the two coordinates have *disjoint*
useful ranges — `0.72` read as an absolute level, or `0.02` as a fraction, both
produce nonsense. With two, the toggle needs no conversion, no API round-trip,
and no wired field.

**No serde migration, and none is needed:** the `isosurface` node was never
pushed, so no saved `.cnnd` contains one. Plain derive defaults throughout.
This holds exactly once — the next persisted change after it ships needs
explicit per-field `#[serde(default = "...")]`, and must **not** lean on the
enum's `#[default]` (moving that when a variant is added silently re-reads
stored documents).

### Text format

`get_text_properties` emits `level_mode` **and both level properties, always.**

The temptation is to emit only the live one — `level` under `Absolute`,
`level_fraction` under `Fraction`, neither under `Auto` — and it is wrong. The
two properties exist precisely so the dormant one survives a mode toggle
(§Node data), and the text format is a round-trip path: copy/paste, the CLI, an
AI edit. Emitting only the live number silently discards the other, so a node
that has been through the text path fails walkthrough step 5 — the step whose
whole purpose is to demonstrate why there are two. The cost is one inert-looking
number, and the `level_mode` beside it says which one is inert.

`set_text_properties` reads `level_mode` first, then applies:

**Naming a level property implies its mode unless `level_mode` is explicit.**
`level:` implies `Absolute`; `level_fraction:` implies `Fraction`. Without this,
`isosurface { level: 0.002 }` would store into the dead slot while a defaulted
`Auto` ignored it and chose its own level. Naming **both** without a mode is an
error naming both properties, not a silent precedence rule — and the emitted
form never trips it, since that always names the mode.

**`Auto` is reachable only by naming it**, so a *newly created* `isosurface { }`
is auto — from the node's `Default`, not from the text path.
`set_text_properties` is applied to the **existing** node data and is only
called when at least one literal property is present
(`text_format/network_editor.rs`), so `isosurface { }` applied to a node already
in `Fraction` leaves it in `Fraction`. Omission cannot *set* the mode back to
auto; `level_mode: auto` can.

### The `level` pin

The existing `level: Float` pin (parameter index 2) keeps its position and feeds
**whichever property is live**. One pin, not two: a second would always be
inert, which `design_scalar_fields.md` already argued against.

- Flipping the mode with the pin wired silently changes what the value means,
  and `0.02` is a *valid* fraction. Mitigation is visibility: the subtitle
  renders `level: 72.0% (fraction)` versus `level: 0.0200`.
- **Under `Auto` the pin is ignored** — raise a non-blocking
  `ValidationError::warning()` ("the level pin is ignored in auto mode; switch
  to absolute or fraction"). Warning, not error: the node still produces a good
  surface, so nothing downstream should be blocked.

### Validation

Two passes, not one. **Validation** runs over the network without evaluating
it, so it sees node data and wires and nothing else; every rule that needs the
*field* is an **evaluation** error. Splitting them is not bookkeeping — a rule
filed under the wrong pass either never runs or has no field to run against.

| Pass | Mode | Rule |
|---|---|---|
| validation | `Absolute` | `level > 0`, not NaN (unchanged) |
| validation | `Fraction` | `0 < f < 1`, not NaN, message naming the rule and the value |
| validation | `Auto` | wired `level` pin raises the non-blocking warning above |
| eval | `Fraction` | field has a distribution, else the Part 3 error |
| eval | `Auto` | a non-negative analytic field falls back to bare `DENSITY_LEVEL`, basis `unchecked`; a **signed** one is the Part 3 error |
| eval | `Fraction`, `Auto` | field is entirely zero (`total == 0`, both queries `None`) — a descriptive error, not a panic and not a silent zero level |

Spell NaN out rather than relying on `!(f > 0.0)`, matching the existing check.

### `IsosurfaceData` does not change

`eval` resolves the mode to an absolute magnitude, so the value type, the
display conversion, the extractor, the lattice policy, the tessellator and every
renderer path are **untouched**.

```rust
let (level, basis) = match self.level_mode {
    LevelMode::Absolute => (level_input, LevelBasis::Absolute),
    LevelMode::Fraction => (
        field.value_distribution().ok_or_else(|| /* Part 3 */)?
             .iso_for_fraction(level_input)?,
        LevelBasis::Fraction(level_input),
    ),
    LevelMode::Auto => auto_level(field.as_ref())?,   // returns (f64, LevelBasis)
};
```

`auto_level` returns the basis as well as the number so the readout can show a
guess as a guess. A caller that discards it makes the guess invisible.

The distribution is over **stored samples**, never the extraction lattice, so
the resolved level does not move when the quality preference changes.

### Readout

`NetworkResult::Isosurface` already carries `field` and the resolved `level`, so
`to_display_string` / `to_detailed_string` need no new plumbing:

```
Isosurface
  level:  2.3270e-2  ·  encloses 72.0% of ∫|v|
  field:  96 x 96 x 96
```

Under `Auto`, append the basis:

```
  level:  2.0000e-3  ·  encloses 99.2% of ∫|v|  ·  auto: non-negative, density-like
```

Bases: `non-negative, density-like` / `signed field` / `non-negative, atypical`
/ `non-negative, unchecked` (an analytic field, where the ratio test has no
distribution to run against). The last two are the ones a user most needs to
see.

**Wording is load-bearing: `encloses X% of ∫|v|`, never "% of the electron
density".** On an orbital amplitude the conventional enclosed quantity is
`∫|psi|²`, so the friendlier paraphrase would be false. Do not "improve" it.

The subtitle shows the mode and the stored number — `get_subtitle` has no
evaluation context, so it cannot show a resolved level. Under `Auto` it shows
**`auto` alone**: neither stored number is live there, and printing one that
`eval` will not use is worse than printing none.

## Part 3 — Analytic fields

`value_distribution()` is `None` for any field with no stored samples. Since
`Auto` is the default and a signed field takes the fraction branch, a future
Molden orbital hits this immediately.

**Decision: a clear evaluation error, not a synthesized distribution.**

```
isosurface: fraction mode needs a field with stored samples, and this field is
analytic (no native grid). Switch the level mode to absolute.
```

Sampling `suggested_bounds` to fake a distribution would make the resolved level
depend on that spacing — the quality-parameter-in-the-value problem the sibling
design exists to avoid. **When Molden lands**, `AnalyticField` should own a
canonical distribution grid chosen at construction, so `value_distribution`
returns `Some` and consumers stay resolution-free.

## Part 4 — The colour domain

`IsosurfaceColoring::Field { range, colormap }` is two hand-typed numbers
defaulting to ±0.05. It gets a distribution too, but **a different statistic —
the enclosed fraction does not transfer.** Nothing is enclosed (it is a paint),
the parameter is an interval, and the relevant population is not the volume.

| | surface field | colour field |
|---|---|---|
| Population | all stored samples | colour field **at the extracted surface's vertices** |
| Weight | mass, `|v|` | **surface area** — a third of each vertex's incident triangle areas |
| Sign | folded to `|v|` | **kept signed** |
| Parameter | one magnitude | interval `(min, max)` |
| Query | `iso_for_fraction(f)` | percentiles `p2` / `p98` |

**Area weighting, not vertex count:** marching cubes puts vertices where the
geometry is busy, not where the area is; a crumpled region would otherwise
dominate the percentile.

### Why restricting to the surface is the whole point

Measured on the zoo's `ch3cl.cube` / `ch3cl_esp.cube` — a real density and
potential on one grid — taking the ESP at voxels the `0.002` density surface
passes through:

| Quantity | Value |
|---|---|
| ESP over the whole volume (`value_range`) | −2.56e-2 … **+9.72e+1** |
| ESP on the surface, p2 … p98 | −2.49e-2 … **+4.00e-2** |
| Symmetric fit `q = p98(abs)` | **±4.00e-2** — the conventional ESP domain |
| **Volume max / surface p98** | **2427x** |

Fitting to `value_range` would set ±97 and paint the envelope one flat colour.
This is why `range` carries the comment "Never auto-fitted" — the objection is
entirely about *volume* extrema, which restricting to the surface removes.

**Insensitive to the band definition:** sampling at density `0.002 ±10%` and
`±5%` gives `p98` of 4.0037e-2 and 4.0090e-2 — 0.1% apart across a doubling.
The answer is a property of the surface, not of the tolerance.

### Where it is computed

**The fit is an editor action writing concrete numbers into `color_min` /
`color_max`. Not a mode, not a flag in the value, and no new node property.**

- The surface only exists after extraction, at a preference-driven resolution.
  Auto-fitting there would make a saved document's colours depend on a quality
  setting.
- A colour map is only comparable between two figures when the domain is the
  **same number**. Auto-fitting per field would silently give two ESP maps
  different scales. (This is the opposite of the level, where `f` is meant to be
  invariant and *should* re-resolve — hence one is live and one is frozen.)

**It must be undoable, and gets that by not being special:** route through the
editor's existing `_update` -> `model.setIsosurfaceData`, the same path the two
text fields already use.

Two fits, chosen by the colour field's signedness:

- **Signed** (the ESP case): symmetric, `[-q, +q]` with `q = p98` of `|v|` on
  the surface. Symmetry is not cosmetic — `BlueWhiteRed` is diverging and its
  white must sit at zero, or the sign can no longer be read off the picture.
- **Non-negative:** `[p2, p98]`.

**Interaction with the open colormap-orientation question**
(`design_isosurface_node.md` §Colormap orientation): the ramp runs blue-low to
red-high, backwards from the chemistry convention. The fit *raises the stakes* —
a correctly-fitted domain with inverted hues is more misleading than the status
quo. If that question is resolved in the same release, resolve it **first**;
changing the ramp afterwards recolours every fitted document. The fit must not
paper over it by emitting an inverted domain: that fails the `max > min` guard
and flattens the surface to the midpoint, which is deliberate and test-pinned.

## Part 5 — The editor

### Level control

**Auto** — no editable number. Show the resolved pair, the basis, and the
histogram with its marker. Two buttons hand over: **Take over (fraction)** and
**Take over (absolute)**, each pre-filling the resolved value.

**Fraction** — a log slider, primary:

```
f = 1 - 10^-(0.25 + 2.75 * s),   s in [0, 1]
```

Spans **f = 0.4377 at s = 0 to f = 0.999 at s = 1**, most travel in 0.9–0.99.
It does **not** reach the `0.30` handoff §3.7 uses for its flood animation; drop
`0.25` to `0.155` if that is ever wanted. The fraction stays typeable —
reproducing a published figure needs an exact number, not a drag.

**Absolute** — the existing text field stays primary. Users type conventional
constants; a slider over ten decades is worse than a box.

**All three modes show both numbers**, same string, same position:

```
|v| = 2.327e-2 a.u.  ·  encloses 72.0% of ∫|v|
```

### The histogram

- **x**: `log10 |v|` over the nonzero range. Linear shows one spike.
- **y**: **mass per bin**, not sample count — count-weighted it is one vacuum
  spike, the same failure as the count percentile in Part 1.
- **overlay**: the cumulative mass curve, 0 to 1 left to right. This is what
  makes `f` legible.
- **marker**: vertical line at the active level, enclosed side shaded.
- **zeros**: a count beside the plot, never a bin.

The colour histogram is the same widget with three substitutions: the
surface-restricted area-weighted distribution as source, a **signed** x axis
(linear is fine when the surface range is narrow — ±0.08 needs no log), and the
domain drawn as a **span** with two handles. The fit button sets the span.

### Plumbing

The isosurface editor is today pure node-data editing with no kernel access.
(`comment_editor`, `function_output_editor`, `record_def_dropdown` show the
`sd_api.` precedent.)

New `rust/src/api/structure_designer/field_distribution_api.rs`, **added to
`flutter_rust_bridge.yaml`'s `rust_input`** — a module missing from that list
generates nothing, silently.

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn get_isosurface_level_distribution(scope_path: Vec<u64>, node_id: u64)
    -> Option<APIValueDistribution>;

#[flutter_rust_bridge::frb(sync)]
pub fn get_isosurface_color_distribution(scope_path: Vec<u64>, node_id: u64)
    -> Option<APISurfaceValueDistribution>;
```

`APIValueDistribution` carries bin edges, per-bin mass, per-bin count, the
cumulative curve, the nonzero range, the zero count, and the resolved
`(iso, fraction, basis)` for the node's current setting — so the editor never
recomputes a query locally and the printed number comes from the same code path
as extraction.

**Three empty states, all normal, none an error:** no field wired; an analytic
field; an upstream not yet evaluated. Show the control without a plot, and say
which.

**Reaching the field:** evaluate the node's `field` argument on demand via a
small `evaluate_node_argument(scope_path, node_id, pin_index)` helper. With eval
memoization this is a memo hit returning an `Arc` clone. **Do not record field
handles during the pass** keyed by `NodeRef` like `node_output_strings` — a memo
hit *skips* that write, so a memoized node would leave no entry and the editor
would show a stale or empty plot depending on cache state.

**Invalidation:** refetch on any network-change notification, not only on
selection change. Rewiring `field` or reloading the `.cube` must invalidate; the
failure mode (a correct-looking histogram belonging to the previous field) is
silent.

## Part 6 — Fixtures

The simulation team's zoo lives at `C:\cube-examples-2026-08-25` (16 real
PySCF/QE cubes + README). Prefer it over the gitignored synthetic
`sample_data/cube/` files: it gives **exact expected numbers** for tests.

| File | Grid | Sign | Auto picks |
|---|---|---|---|
| `ch3-radical/ch3_density` | 80³ | non-neg | abs 0.002 (36.7% occupancy) |
| `ch3-radical/ch3_spin` | 80³ | signed | f 0.72 -> 9.08e-3 |
| `ch3-radical/ch3_homo_alpha` | 80³ | signed | f 0.72 -> **3.27e-2** |
| `esp-sigma-hole/ch3cl` | 80³ | non-neg | abs 0.002 (13.6%) |
| `esp-sigma-hole/ch3cl_esp` | 80³ | signed | colour field — surface p98 **4.00e-2** |
| `nacl-ecp/nacl` | 80x80x110 | non-neg | abs 0.002 (9.7%) — ECP, missing core |
| `si-cluster-vacancy/si-cluster-S3-vacancy` | 80³ | non-neg | abs 0.002 (27.3%) |
| `si-cluster-vacancy/..._spin` | 80³ | signed | f 0.72 -> **1.64e-3** |
| `si-cluster-derivatives/..._elf` | 80³ | non-neg | window rejects (52.1%) -> 0.497 |
| `si-cluster-derivatives/..._rdg` | 80³ | non-neg | window rejects (100%) -> 542 |
| `si-cluster-derivatives/..._lapl` | 80³ | signed | f 0.72 -> 2985 (known-wrong) |
| `si-cluster-derivatives/..._sign_l2_rho` | 80³ | signed | f 0.72 -> 8.7e-2 |
| `periodic-tcenter/tcenter_spin_hse` | 144³ | signed | f 0.72 -> **3.02e-4** |
| `si-gemcut-2D3R-149/...-S3` | 247x247x164 | non-neg | abs 0.002 (20.2%); **10,005,476 samples**, histogram path |
| `si-gemcut-2D3R-149/..._spin` | 247x247x164 | signed | f 0.72 -> 1.28e-3 |

Auto is correct on all nine mainline fields (every density including ECP,
2–149 atoms; every spin density; the orbital). The four failures are all
post-processed analysis fields and are documented in Part 2.

**Two external confirmations that `LOCALIZED_FRACTION = 0.72` is right:**
`ch3_homo_alpha` resolves to 0.0327, inside the conventional 0.02–0.05 orbital
band; `tcenter_spin_hse` resolves to 3.02e-4 against the handoff's own §1.6
worked example of `3.4e-4` — the closest thing to an external oracle here.

## Part 7 — Implementation plan

Each phase ends green: `cargo test -j 4`, `cargo clippy`, `flutter analyze`.

### P1 — `ValueDistribution` (backend only)

`field/distribution.rs`: the type, `LogHistogram`, `ExactCumulative`, the three
queries, the exponent parameter, `EXACT_SAMPLE_LIMIT`; the trait method and
`SampledField`'s `OnceLock` override.

| Test | Asserts |
|---|---|
| `f -> iso -> f`, f in {0.3, 0.5, 0.72, 0.9} | within ±1/N |
| Direct integration, synthetic Gaussian | mass above `iso_for_fraction(f)` / total = f, within 0.5% |
| Same field at 0.2 Å and 0.1 Å | same f gives the same iso within a bin — grid independence |
| Forced-histogram vs exact on one fixture | agree within one bin |
| `si-gemcut-...-S3` (10M samples) | crosses `EXACT_SAMPLE_LIMIT`; both paths have a real input |
| `f32` flush on the same file | `1.6e-75` lands in `zero_count`, not the lowest bin; `total` unaffected |
| All-zero field | `Some`, `total == 0`, queries return `None` — no divide by zero |
| Zeros + nonzeros | `zero_count` right, zeros excluded from bins |
| Sheared grid | identical fraction to the unsheared same samples — the determinant cancels |
| Exponent 2 on a signed fixture | accumulates `v²` |
| `SampledField` clone | rebuilds; no deep copy of the cache |
| `voxel_fraction_at_or_above(0.002)` on the five zoo densities | 9.7–36.7%; ELF 52.1%, RDG 100% |

### P2 — `LevelMode`, `auto_level`, readout

`LevelMode`, the two properties, text spellings and the inference rule,
`auto_level` / `LevelBasis`, `eval` resolution and validation, the subtitle, and
the `NetworkResult::Isosurface` readout arms.

| Test | Asserts |
|---|---|
| New node's `Default` | `Auto` |
| Auto on `si-cluster-S3-vacancy` | `0.002`, basis `density-like` |
| Auto on `..._spin` | **1.64e-3**, basis `signed field` |
| Auto on `ch3_homo_alpha` | **3.27e-2**, inside 0.02–0.05 |
| Auto on `..._elf` / `..._rdg` | both fall back, basis `atypical` |
| Auto on all five zoo densities | all inside the window, ECP included |
| Auto -> Fraction / Absolute handover | pre-filled; extracted surface unchanged |
| Wired `level` pin under Auto | non-blocking warning, pin ignored, surface produced |
| `.cnnd` round-trip, all modes | mode and both numbers survive |
| Text round-trip | only the live property emitted; `isosurface { }` is auto |
| Text `level: 0.002` alone / `level_fraction: 0.9` alone | infers `Absolute` / `Fraction` |
| Text naming both, no mode | error naming both, not silent precedence |
| `eval` in fraction mode | equals `iso_for_fraction(f)` |
| Fraction out of range (0, 1, 1.5, NaN) | descriptive error |
| Absolute mode | byte-identical `IsosurfaceData` to before this change |
| Analytic field, fraction mode | the "needs stored samples" error, not a panic |
| Quality multiplier changed | resolved level does not move |

**Deliverable:** fraction and auto work end to end from the text format and CLI;
the pin readout prints both numbers. First user-visible milestone.

### P3 — Editor: slider, readout, histogram

`field_distribution_api.rs` + its `rust_input` entry; `evaluate_node_argument`;
`APIValueDistribution`; mode toggle, log slider, dual readout, histogram widget
in `isosurface_editor.dart`; refetch on network change.

Rust-side tests for the API types and the three empty states; `flutter analyze`
clean. Widget behaviour is covered by the walkthrough, per
`feedback_manual_test_for_editor_ui`.

**Manual walkthrough**

1. `import_cube` -> `si-cluster-vacancy/si-cluster-S3-vacancy.cube` ->
   `isosurface`. **Expect** auto mode, level `0.002`, basis
   `non-negative, density-like`, a vdW envelope, and the marker where the
   cumulative curve is about 0.995. Tiny spheres at the nuclei instead means
   auto took the fraction branch — check `value_range().min`.
2. **Take over (fraction).** **Expect** about 0.995 pre-filled and **the surface
   not to move at all**.
3. Drag toward 0.999. **Expect** the envelope to grow, the isovalue to fall, the
   marker to track.
4. Switch to absolute, type `0.002`. **Expect** the conventional envelope and a
   readout naming the fraction.
5. Switch back to fraction. **Expect** the step-3 value exactly — not a
   conversion of `0.002`. This is what the second property buys.
6. Switch to auto, rewire to `..._spin.cube`. **Expect** basis `signed field`
   and level **1.64e-3** — an exact expected number, not an impression.
7. Wire anything into `level` while in auto. **Expect** an amber non-blocking
   warning and an unchanged surface — not a red error, not a silently dropped
   wire.
8. Delete the `field` wire. **Expect** a usable control and a plot saying there
   is no field — no error, no stale histogram.

### P4 — Colour domain: surface distribution and fit

Area-weighted surface-restricted distribution in `atomcad-display`'s extractor
output; `get_isosurface_color_distribution`; the signed span histogram and the
symmetric / span fit buttons.

| Test | Asserts |
|---|---|
| Surface distribution on `ch3cl` + `ch3cl_esp` | p98 = **4.0e-2**, not the volume's **9.7e+1** — a 2427x error if fitted to `value_range` |
| Band-width insensitivity | ±10% and ±5% agree within 1% (measured 0.1%) |
| Area weighting | uneven triangle density gives a different, correct percentile than vertex-count weighting |
| Symmetric fit, signed colour field | exactly `[-q, +q]`; zero maps to the ramp midpoint |
| Span fit, non-negative colour field | `[p2, p98]` |
| Fit writes node data | `color_min` / `color_max` concrete; no mode flag in the `.cnnd` |
| Fit is undoable | one undo restores the previous domain, via `setIsosurfaceData` |
| Quality multiplier changed after a fit | the saved domain does not move |

**Manual walkthrough:** `esp-sigma-hole/ch3cl.cube` as `field`,
`ch3cl_esp.cube` as `color_field`. Press the symmetric fit. **Expect** the
domain near **±0.040**, and the **sigma hole** visible — a positive patch on the
chlorine along the C–Cl axis. That feature is the acceptance criterion: it
disappears if the surface is coloured by density, and washes out entirely if the
domain is fitted to the volume's ±97.

## Documentation touchpoints

Per `AGENTS.md`, in the same change as the code:

- `doc/reference_guide/nodes/atomic.md` — the three level modes, what the
  fraction means, what auto decides and on what basis, the note that auto is
  volatile and a figure should be frozen in fraction or absolute mode, and the
  readout (P2); then the colour fit (P4)
- `doc/design_isosurface_node.md` — a pointer here from §The `Isosurface` value
- `crates/atomcad-crystolecule/src/AGENTS.md` — `field/distribution.rs` in the
  module map, and the invariant that the distribution is over **stored samples**
  so a resolved level never depends on extraction resolution (P1)
- `doc/testing.md` — the round-trip and direct-integration checks as the pattern
  for distribution fixtures (P1)
