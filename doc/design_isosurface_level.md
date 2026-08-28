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
- **The histogram's `iso_for_fraction` returns the crossing bin's lower edge**,
  so like the exact path it always encloses *at least* `f` — and so it is
  biased low by up to one bin rather than scattered around the exact answer.
  Measured on the zoo's only field above the limit (`si-gemcut-...-S3`, whose
  nonzero range is ~44 decades, 5.5% per bin): histogram `8.466e-2` against
  exact `8.922e-2`, ratio **42.3 vs 44.6**. That is one bin, in the direction
  the convention predicts, and an order of magnitude below the 1.7x gap the
  plausibility window decides on.
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

`SampledField` overrides it, caching in a **`OnceLock<Arc<ValueDistribution>>`**.
**Lazy, not eager in `new`:** a field that is only `sample_field`-probed should
not pay for a sort, and `import_cube` builds fields during a file load where a
multi-second pause is very visible. `value_distribution` still returns
`Option<&ValueDistribution>` — `.get().map(Arc::as_ref)` — because the payload is
large and every caller only reads.

**The `Arc` is not decoration; a bare `OnceLock<ValueDistribution>` does not
compile here.** `SampledField` is `#[derive(Clone)]`, and `OnceLock<T>: Clone`
requires `T: Clone`, so the derive fails on a non-`Clone` payload — the first
thing an implementor would hit. The three ways out are not equal:

| | Result |
|---|---|
| derive `Clone` on `ValueDistribution` | a clone deep-copies up to 120 MB — the one thing worth avoiding |
| hand-write `Clone` for `SampledField`, leaving the cache unset | correct, but a clone re-sorts, and it drops a derive for a reason nobody will remember |
| **`OnceLock<Arc<...>>`** | the derive keeps working and a clone **shares** the cache: no copy, no rebuild |

Sharing is sound because `SampledField` is immutable after construction — it
exposes no `&mut self` accessor — so a clone's samples, and therefore its
distribution, are identical by construction.

**`f32` storage flushes deep vacuum to zero.** The zoo's `si-gemcut` density
reaches `1.6e-75`; those samples land in `zero_count`, not the lowest bin. The
histogram's lower edge is `f32`'s floor, not the file's — the readout must not
claim otherwise.

### The two queries are not a bijection

- `f → iso → f` round-trips, and only ever *upward*: `iso_for_fraction`
  returns the tightest stored magnitude still enclosing `f`, so reading the
  fraction back overshoots by the mass of the samples that **tie** with it
  and never undershoots. (Handoff §3.8 test 1.)

  **The overshoot is one tied group, not one sample — ±1/N is the wrong
  bound.** A field with a flat region has many samples at the identical
  stored magnitude, and `fraction_for_iso`'s step includes all of them.
  Measured on the P1 Gaussian oracle, whose octahedral shells are maximally
  degenerate: **8.9% at 17³ for `f = 0.30`**, against a `1/N` of 0.02%. The
  bound that does hold at any resolution is `0 <= f' - f <= tied_mass/total`,
  and that is what P1 asserts.
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
else:
    localized = dist.iso_for_fraction(LOCALIZED_FRACTION)
    if localized is None: error                         # total == 0, an all-zero field
    elif signed:
        level = localized                               # basis: "signed field"
    elif localized <= MAX_LEVEL_RATIO * DENSITY_LEVEL:
        level = DENSITY_LEVEL                           # basis: "non-negative, density-like"
    else:
        level = localized                               # basis: "non-negative, atypical"
```

**Both query results are `Option`, and neither `?` may be an `unwrap`.** An
all-zero field has `total == 0`, so `iso_for_fraction` returns `None` while
`value_distribution` returns `Some` — the one combination easy to miss, and the
one the validation table's third `eval` row is about.

`DENSITY_LEVEL = 0.002`, `LOCALIZED_FRACTION = 0.72`, `MAX_LEVEL_RATIO = 125`,
`NEGATIVE_TOLERANCE = 1e-6`.

**Why Auto exists at all:** `node_data_creator` is
`|| Box::new(IsosurfaceNodeData::default())` — no arguments, no field. A
field-dependent default cannot be computed at node creation, so `eval` is the
only honest place for it.

**Why signedness picks the coordinate:** the property that drives the
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
`MAX_LEVEL_RATIO = 125` sits just under the geometric midpoint of that gap
(`sqrt(72.4 * 228) = 129`, rounded down to a flatter number), leaving 1.7x of
headroom below and 1.8x above.

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

> **Superseded by §The mode is a unit, not a second parked value (Part 5).**
> Every mode switch converts now, not only the ones leaving `Auto`, and a parked
> number that already describes the current surface is kept verbatim. The rest of
> this section still describes what leaving `Auto` does.

Switching **Auto to Fraction pre-fills `level_fraction` with the resolved
fraction**; **Auto to Absolute pre-fills `level` with the resolved magnitude**.
Both are exact, so the surface does not move. Auto is a starting point the user
takes over, never a locked mode.

**Auto is volatile by design** — the level moves when the field changes. For a
figure that must not change, switch to fraction or absolute. Say so in the
reference guide, and note that the only signal in the UI is a passive one: the
`auto:` basis line in the readout. A document reopened against an edited field
shows a moved surface with nothing announcing it. That is the cost of the mode
being the default, and it is accepted — but it should be accepted knowingly, not
discovered.

### Node data

```rust
pub struct IsosurfaceNodeData {
    pub level_mode: LevelMode,   // Auto by default
    pub level: f64,              // live under Absolute; keeps its 0.02 default
    pub level_fraction: f64,     // live under Fraction; (0,1) exclusive; 0.72
    // ... colours, alpha, colormap, colour domain — unchanged
}
```

> **Partly superseded by §The mode is a unit, not a second parked value.** The
> two properties remain in storage and in the text format, for the no-field and
> analytic cases, but they are no longer a UI concept: the panel shows one level
> in one unit, and switching converts. Only the first of the two reasons below
> survived contact with the walkthrough.

**Two stored numbers, not one reinterpreted**, because `absolute -> fraction ->
absolute` is lossy (§bijection) and because the two coordinates have *disjoint*
useful ranges — `0.72` read as an absolute level, or `0.02` as a fraction, both
produce nonsense. With two, the toggle needs no conversion, no API round-trip,
and no wired field.

**No serde migration, and none is needed:** the `isosurface` node *is* on
`origin/main`, but it landed there minutes before this design was written and
the user base is a handful of people who know the app is experimental — so no
saved `.cnnd` in the world contains one. Plain derive defaults throughout.

That is an argument from timing, not from structure, and it expires
immediately. `generic_node_data_loader` is `serde_json::from_value` with no
struct-level default, so a field added without `#[serde(default = "...")]`
makes an older document fail to load outright. The next persisted change after
this one needs explicit per-field defaults, and must **not** lean on the enum's
`#[default]` — `Auto` as the load-time default would override a level the user
deliberately typed, and moving `#[default]` when a variant is added silently
re-reads stored documents.

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

Sketch, not literal: both `?`s abbreviate a conversion. `value_distribution`
yields `Option<&ValueDistribution>` and `iso_for_fraction` yields `Option<f64>`,
while `eval` returns `EvalOutput` — so each becomes an explicit
`NetworkResult::Error` arm, the first with the Part 3 message and the second with
the all-zero message.

`auto_level` returns the basis as well as the number so the readout can show a
guess as a guess. A caller that discards it makes the guess invisible.

```rust
/// How the resolved level was arrived at. Carried out of `eval` for the
/// readout only — nothing downstream branches on it.
pub enum LevelBasis {
    Absolute,
    /// The enclosed fraction that produced it.
    Fraction(f64),
    Auto(AutoBasis),
}

/// The four `Auto` outcomes, in the order the rule tries them. Their
/// user-facing strings are the ones §Readout lists, and are the only place
/// these are rendered.
pub enum AutoBasis {
    DensityLike,   // "non-negative, density-like"
    Atypical,      // "non-negative, atypical"    — the window rejected 0.002
    Signed,        // "signed field"
    Unchecked,     // "non-negative, unchecked"   — analytic, no distribution
}
```

`LevelBasis` is **not** stored in `IsosurfaceNodeData` and **not** in
`IsosurfaceData`: it is derived on every evaluation, like the level itself.

The distribution is over **stored samples**, never the extraction lattice, so
the resolved level does not move when the quality preference changes.

### Readout

`NetworkResult::Isosurface` already carries `field` and the resolved `level`, so
`to_display_string` / `to_detailed_string` need no new plumbing:

```
Isosurface
  level:  0.023270  ·  encloses 72.0% of ∫|v|
  field:  96 x 96 x 96
```

Under `Auto`, append the basis:

```
  level:  0.002  ·  encloses 99.2% of ∫|v|  ·  auto: non-negative, density-like
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
| Weight | mass, `\|v\|` | **surface area** — a third of each vertex's incident triangle areas |
| Sign | folded to `\|v\|` | **kept signed** |
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

**The "never auto-fitted" prose is user-facing now, and this phase falsifies
it.** The comment that phrase describes has since become a paragraph in the
`isosurface` node description *and* in the reference guide. Both must be
rewritten in the same change as the fit — see Part 7 §P4 §The fit contradicts
shipped prose.

**Insensitive to the band definition:** sampling at density `0.002 ±10%` and
`±5%` gives `p98` of 4.0037e-2 and 4.0090e-2 — 0.1% apart across a doubling.
The answer is a property of the surface, not of the tolerance.

**Insensitive to extraction resolution, down to a floor.** Freezing the fitted
number stops it drifting *afterwards* (§Where it is computed), but it does not
make the *fit itself* resolution-free: the statistic is read off the extracted
mesh, whose vertices come from the quality preference, so two presses at two
quality settings must not give two answers. Measured by decimating the grid,
which is the same coarsening the extraction lattice applies:

| Sampling | band samples | `p98` on the surface |
|---|---|---|
| full grid | 5088 | 4.0037e-2 |
| every 2nd voxel | 647 | 3.9933e-2 |
| every 3rd | 182 | 4.0074e-2 |
| every 4th | 78 | **3.61e-2** |

Stable to 0.3% while the surface is adequately sampled, breaking only when the
sample count collapses. So **the fit is gated on a minimum surface-vertex
count** — a few hundred: below it the button refuses and says the surface is too
coarse to fit, instead of writing a plausible wrong number into the document.
Without that gate this is the one path by which a quality setting could still be
baked into a saved `.cnnd`.

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

### The rule this panel already wrote down

`isosurface_editor.dart` states its own layout principle, about the colormap
group:

> They stay visible rather than hidden so the stored values are still
> inspectable, and so the panel does not change shape when a wire is made.

and `FloatInput` exists to implement it:

> `enabled` — when false the field is greyed out and refuses focus. Used where a
> value is stored and worth showing but nothing reads it in the current
> configuration [...] The value still renders, so it stays inspectable.

> **Superseded by §The mode is a unit, not a second parked value.** The premise
> below — that a mode is a wire by another name — is the thing that turned out to
> be wrong. A wire makes a stored value live again *unchanged*, so keeping it
> visible is informative; a mode switch **converts**, so the other coordinate
> holds no independent information and showing it puts two disagreeing numbers on
> screen. The `FloatInput.enabled` contract quoted above is untouched, and still
> governs the colormap group.

**Both rules extend to the level modes, and this design follows them.** A mode
is a wire by another name: it decides which stored number is live. So every
control is present in every mode and the ones that are not live are *disabled,
not hidden*. Nothing here needs a widget the panel does not already have.

That is not a style preference. Part 2 stores **two** numbers precisely so the
dormant one survives a toggle; a layout that hides it throws away the thing the
second property was added to protect, and the panel jumping between three
different shapes as the user explores the modes is how a control loses the
user's place.

### Layout

The node-data panel is **400 px wide** (`main_content_area.dart`), less 8 px of
padding each side, and in the horizontal arrangement it is short rather than
narrow. The level group, assembled:

```
 Level                                                    (heading)
 ┌──────────────────────────────────────────────────────┐
 │ Mode  [ Auto                                    ▾ ]  │
 │                                                      │
 │ Fraction  ───────────────────────●──   [ 0.9920  ]   │  Fraction only
 │ Absolute                               [ 0.0020  ]   │  Absolute only
 │                                                      │
 │ |v| = 0.002  ·  encloses 99.2% of ∫|v|                │
 │ auto: non-negative, density-like                     │  Auto only
 │                                                      │
 │ ┌──────────────────────────────────────────────────┐ │
 │ │                        ▁▄█▇▃        ╱‾‾‾‾‾‾‾ 1.0 │ │
 │ │                    ▁▂▄███████▃▁  ╱               │ │
 │ │              ▁▂▃▅███████████████▅▁               │ │
 │ │ ────┴────┴────┴────┴────┴────┴────┴───  log10|v| │ │
 │ │    -6   -5   -4   -3   -2   -1    0              │ │
 │ └──────────────────────────────────────────────────┘ │
 │ 1 204 samples are exactly zero                       │
 └──────────────────────────────────────────────────────┘
 <caption>
```

*Both rows are drawn above for reference; **only one is ever on screen**, the one
the mode makes live, and under `Auto` neither is (§The mode is a unit). The
readout is what carries both numbers in every mode.*

Reading order is mode → the number → what it means → where it sits in the
data. The **histogram goes last on purpose**: it is an aid, not the control of
record, so in a short panel it is the thing that scrolls away rather than the
thing the user came for.

The rest of the panel — phase colours, opacity, colormap — is unchanged and
keeps its current order.

### The level group

> **Superseded in two places by §The mode is a unit, not a second parked value:**
> the dropdown converts on *every* switch, and the non-live row is **hidden**
> rather than greyed. Everything else here — the dropdown being the only way to
> leave `Auto`, the two rejected alternatives, the wired-`level` response —
> stands.

**Mode** is a dropdown, `Auto` / `Absolute` / `Fraction`, and it is the **only**
way to leave `Auto`. Switching it performs the §Handover pre-fill from Part 2:
to `Fraction` fills `level_fraction` with the resolved fraction, to `Absolute`
fills `level` with the resolved magnitude, and in both cases the surface does
not move.

**Not two "Take over" buttons.** Two buttons that both mean *stop being
automatic* force the user to choose between the two coordinates before either
has been explained — at the moment they know least. The dropdown has to exist
anyway, so the buttons are a second control for a job the first one already
does.

**And not take-over-by-touch either**, which is the tempting alternative: leave
the controls live under `Auto` and let a drag or a keystroke flip the mode as a
side effect. It loses to two concrete objections. `FloatInput.enabled == false`
*refuses focus*, so a box that is "disabled but typeable" is not the widget the
app has. And a mode change hidden inside a drag has no clean undo: one Ctrl-Z
would have to restore both the number and the mode, or the user watches the
value return while the mode does not. Explicit is one extra click and no
ambiguity in either direction.

**Under `Auto` both numeric rows are disabled and display the resolved pair** —
greyed, inspectable, exactly the `FloatInput.enabled` contract. Not the stored
numbers, which are dormant and would be misleading: the panel shows what the
surface is actually being drawn at. *(Superseded: under `Auto` there is now no
numeric row at all. The instinct here — never show a dormant number where a live
one is expected — is exactly what §The mode is a unit generalised.)*

**When the `level` pin is wired, both rows are disabled in every mode**, with
the caption saying the wire drives the level. The node already behaves this way
elsewhere — `get_subtitle` returns `None` when `level` is connected — and the
colormap group already greys out on the state of a *different* pin, so this is
the established response to "a wire owns this value", not a new idea. Under
`Auto` the wire is ignored entirely and the non-blocking warning from Part 2
says so; the panel must not imply the number is doing something.

**With no field wired**, the mode dropdown stays live and both rows are
disabled and blank — there is no resolved pair to show and the stored numbers
are not what would be used. The caption carries the reason (§Empty states).
*(Still true of the one row that is now rendered. This is also the case that
keeps both properties in storage: with no distribution there is nothing to
convert through, so a mode switch here changes only the mode.)*

### The fraction slider

`Slider` + an 80 px `FloatInput` in a `Row`, `divisions: 100` — the same shape
as **Opacity**, ten lines above it in the same file. Consistency here is free
and its absence would be conspicuous.

```
f = 1 - 10^-(0.155 + 2.845 * s),   s in [0, 1]
```

Spans **f = 0.3002 at s = 0 to f = 0.999 at s = 1**. The travel is linear in the
**number of nines**, not in `f`, which is what puts ~70 % of it in 0.9–0.999
where densities live (Part 2: 0.98–0.999) while still covering the orbital band
(0.5–0.9) and reaching the `0.30` the handoff's §3.7 flood animation starts
from. 100 divisions is fine across it: the coarsest step is 0.046 in `f` at the
bottom, where nothing is, and 6.6e-5 at the top, where everything is.

**The offset cannot be 0.** `1 - 10^0` is exactly `0`, which validation rejects
and which means "enclose nothing", so the bottom stop has to sit strictly above
it; the only question is how far. *(This shipped at `0.25` — bottom stop
`0.4377` — with a note here that `0.155` would reach the flood animation's
`0.30`. Taken, at the cost of a 3.5 % coarser step at the top: the span widened
from `2.75` to `2.845` to keep the top stop at `0.999`, which is a documented
endpoint.)*

**The window is smaller than the property's legal range, and the slider must
say so rather than lie about it.** Validation allows `0 < f < 1`, and a value
outside `[0.30, 0.999]` arrives easily — from the text format, the CLI, a
`.cnnd` written by an older build. Rendering it at the left stop makes `0.1`
indistinguishable from `0.30`, and the next drag silently triples it.

So: **when `f` falls outside the slider's window the slider is disabled**, with
its handle parked at the nearer stop, and the box stays live and authoritative.
Typing a value back inside re-enables it. That reuses the same
disabled-but-visible contract as everything else here, and it removes the silent
jump instead of decorating it. The box is the control of record in every case;
the slider is a fast way to move within the useful window.

### The absolute box

The existing `FloatInput`, unchanged and primary. Users type conventional
constants — `0.002`, `0.02` — and a slider over ten decades is worse than a box.
The histogram beneath it is what shows where that number falls.

### The readout

One line, **identical in all three modes** — same wording, same position,
directly under the two rows; only the numbers change:

```
|v| = 0.002  ·  encloses 99.2% of ∫|v|
```

with the basis appended on its own line under `Auto`:

```
auto: non-negative, density-like
```

**No unit suffix.** An earlier draft wrote `a.u.` here and it is wrong: this
design never converts field values and never assumes what they are (Part 1
§Definition, and `field/mod.rs`'s "values are passed through in whatever atomic
unit the source quantity uses"). Two of the four impostors in Part 2 are
dimensionless — ELF is a 0–1 ratio, RDG a reduced gradient — so a fixed suffix
mislabels them. The magnitude stands alone.

This is not byte-identical to the pin readout in Part 2 §Readout, which is a
labelled multi-line block for a different surface. What the two share, and what
§Readout pins, is the **fraction clause**: `encloses X% of ∫|v|`, verbatim.

**`∫|v|` needs a tooltip, and Part 2's wording rule is the reason.** §Readout
forbids the friendly paraphrase — on an orbital amplitude the conventionally
enclosed quantity is `∫|psi|²`, so "% of the electron density" would be false —
which leaves a symbol on screen with no way to learn it. Every other control in
this editor carries a `captionStyle` explanation; this one gets a tooltip on the
readout saying that the fraction is the share of the field's total integrated
magnitude that lies inside the surface, and that it is not a share of volume.
Correct-but-opaque is only half a decision; the other half is where the
explanation lives.

### The histogram

- **x**: `log10 |v|`, **cropped by mass** — see §The axis is cropped by mass
  below, which corrects "over the nonzero range" here. Linear shows one spike.
- **y**: **mass per bin**, not sample count — count-weighted it is one vacuum
  spike, the same failure as the count percentile in Part 1. The axis is
  unlabelled and unscaled: only its shape is meaningful.
- **overlay**: the cumulative mass curve, on **its own 0–1 scale** pinned to the
  plot height, with a right-hand tick at 1.0 so it is not read as mass. This is
  what makes `f` legible.
- **marker**: a vertical line at the active level, enclosed side shaded.
- **zeros**: a count in a line beneath the plot, never a bin.
- **size**: full panel width, ~120 px tall, and it must shrink rather than
  overflow — in the horizontal arrangement height is the scarce dimension.

**The marker is draggable in `Fraction` and `Absolute`, and inert under
`Auto`.** A vertical line on a plot is the most draggable-looking object in the
panel; leaving it inert in the modes where the value *is* editable guarantees
people try it and conclude the panel is broken. Dragging it edits whichever
property is live — the isovalue directly in `Absolute`, the enclosed fraction in
`Fraction` — and coalesces into one undo entry per drag, like the slider. Under
`Auto` nothing in the group is editable and the marker follows suit.

**The slider's travel is not the plot's x axis**, and the layout must not imply
otherwise: one is in fraction space, the other in `log10 |v|`. The cumulative
curve is the only thing that relates them, which is a second reason it is drawn
rather than left to the reader.

### The panel says a clause, the description says the paragraph — a P3 correction

Part 5 gives every group a `captionStyle` explanation, and shipped they added up
to roughly a dozen lines of prose in a 400 px panel — more text than control.
Reported from the walkthrough as taking too much space, and the diagnosis is
that the captions were doing a manual's job.

**The rule now: a caption earns its place only if what it says is non-obvious
*and* fits in a line.** Three survive in the level group — the two-lobe
consequence of a magnitude, what a fraction is a fraction *of*, and that `auto`
silently re-picks — plus one-clause empty states. Everything longer moved to the
**node description**, which is Markdown rendered in a dialog by the ⓘ button
(`node_description_button.dart`), and which is under no space pressure at all.
The `expr` node was already using that surface for its syntax reference; this is
the same trade.

Three specific moves worth recording, because each was load-bearing prose that
had to land somewhere rather than be deleted:

- **The swap button's rationale** ("an orbital's overall sign is arbitrary")
  moved into the button's own **tooltip**, where it costs no layout at all and
  is exactly where a puzzled user points.
- **"The colour domain is never fitted automatically"** and its whole
  justification moved to the description. It argues for something the panel does
  *not* do, which is the least earning kind of caption; the unwired case keeps a
  one-line "wire `color_field` to paint by a second quantity", which is
  actionable.
- **The opacity caption** went entirely. "How see-through the surface is" under
  a heading reading *Opacity* is the definition of a caption saying nothing.

The histogram's sub-captions are now all **conditional**: each reports something
the plot is *not* showing — exact zeros, a cropped axis, a binned distribution —
and none has anything to say in the ordinary case. "No samples are exactly zero"
was printed on every field and is not news.

### Numbers read plainly unless they need an exponent — a P3 correction

Every magnitude in these readouts was printed in scientific notation
unconditionally — `{:.4e}` in the pin readout, `toStringAsExponential` in the
panel. Reported from the walkthrough: `1.2233e-2` says exactly what `0.012233`
says, with an exponent the reader has to decode first, and the fields this
serves are full of levels like `0.002` and ranges like `0 .. 123`.

So both sides now switch on the exponent: **plain decimal for
`1e-4 <= |v| < 1e6`, scientific outside it**, with trailing zeros trimmed in
either form (`0.002`, not `0.002000` — padding to the requested precision claims
a precision the number does not have). The bounds are where the notation stops
earning its keep: `1e-4` is `0.0001`, three leading zeros and still countable;
`1e-5` is not.

**The rule lives in two places that must agree**, because the same level appears
in a pin readout (Rust) and in the isosurface panel (Dart):
`atomcad_util::number_format::format_natural` and
`lib/common/number_format.dart`'s `formatNatural`. Their test tables mirror each
other case for case, and a change to either belongs in both — the divergence
that table caught on the first run is real: **Dart's `toStringAsExponential`
writes `1e+6` where Rust's `{:e}` writes `1e6`**, so the Dart side strips the
plus.

Sites converted: the isosurface level in both `to_display_string` and
`to_detailed_string`, the `ScalarField` hover text's `values:` range, the
isosurface colour-domain `range:`, the panel's `|v| = …` readout, and the
histogram's cropped-axis caption. Deliberately left alone: `SampledField`'s
`Debug` impl, which is developer-facing and feeds no readout.

### The mode is a unit, not a second parked value — a P3 correction

**Two decisions in Part 2 and Part 5 are reversed.** The mode dropdown now
**converts**, and the panel shows **one** numeric row — the live coordinate's —
hiding the other entirely rather than greying it.

Found by running the walkthrough. In `Fraction` mode the greyed `Absolute` row
showed the *stored dormant* `0.02` while the readout two lines below said
`|v| = 2.327e-2`: two differently-valued absolute levels on screen at once, with
nothing saying they meant different things. It read as a stale field, and the
first thing the maintainer did was report it as a bug.

The collision was structural, not cosmetic. §The level group specified **two
different rules for the same widget**: under `Auto` a non-live row shows the
*resolved* value ("the panel shows what the surface is actually being drawn at"),
under a manual mode it shows the *stored* one (walkthrough step 6). Both are
defensible; together they are not.

#### What replaces them

**`level_mode` selects the unit one quantity is expressed in.** At any moment
there is one level, and the mode says whether you are reading it as a magnitude
or as an enclosed fraction. So:

- **Switching converts.** `Absolute ⇄ Fraction` fills the newly-live coordinate
  from the level currently drawn, and the surface does not move. The handover
  Part 2 §Handover specified for leaving `Auto` is now simply what every switch
  does.
- **Switching *to* `Auto` is the one switch that moves the surface**, back to
  what the field itself suggests. `Auto` consults neither stored number by
  design; that is what picking it asks for.
- **The row for the non-live coordinate is hidden.** Under `Auto`, both are —
  there is no numeric row at all. Nothing is lost: the readout carries **both**
  coordinates in every mode, which is what makes hiding them lossless. This is
  also why the readout stopped being an aid and became the group's answer of
  record.

The `FloatInput.enabled` contract Part 5 leans on is untouched, and so is the
colormap group. The distinction is worth stating because it is the reason the
two groups now differ: a dormant *colour domain* is a value nothing reads right
now **but a wire would make live again unchanged**, so it stays visible and
greyed. A dormant *level coordinate* is not that — it is the same quantity in
the other unit, and after a conversion it holds no independent information.

#### The one real cost, and what it buys back

§The two queries are not a bijection is the argument that survives: `iso → f →
iso` does not round-trip, because `iso_for_fraction` can only return a *stored
sample*. Converting unconditionally would hand a typed `0.002` back as
`0.00200034…` after a trip through fraction mode — and `0.002` is a number
people quote from papers.

So **a parked number that already describes the current surface is kept
verbatim**. `APIValueDistribution` carries `stored_level_matches` /
`stored_fraction_matches`, computed in the kernel as "does this stored
coordinate enclose the same fraction the surface encloses". Both are compared on
the **enclosed fraction**, never on the magnitude — every isovalue between two
adjacent samples encloses the same set, and "same set" is what "the surface did
not move" means. Exact equality is right rather than a tolerance: every value
involved comes out of the same two queries over the same samples.

The result is lossless in the case that bites (switch away and back having
changed nothing) and honest in the case that does not (switch back after
dragging, and you get the converted value).

The other two arguments Part 2 gave for two properties do not survive:

| Argument | Status |
|---|---|
| `0.72` read as an absolute level, or `0.02` as a fraction, is nonsense | Argued against **reinterpreting** one number. This converts. Does not apply |
| The toggle needs no conversion, no API round-trip, no wired field | Convenience, and outweighed. The conversion is two `f64`s already on a struct the panel fetches anyway |
| `iso → f → iso` is not a bijection | **Survives** — and is answered by the match flags above |

#### The two properties stay in storage

`IsosurfaceNodeData` keeps both, and the text format still emits both. With no
field wired — or an analytic one — there is no distribution to convert through,
and the parked number is the only sane fallback. They simply stop being a **UI**
concept: nothing shows two level numbers any more, and no user has to hold
"there are two independent levels" in their head.

#### Degenerate fractions

A derived fraction is clamped into the property's legal open interval
(`IsosurfaceEditor.FRACTION_EPSILON`). The enclosed fraction is exactly `1.0`
for any level at or below the smallest sample, and exactly `0.0` above the
largest, while validation requires `0 < f < 1`. At the top the clamp is sound
rather than a fudge — `iso_for_fraction` is a step function, so `1 - ε` resolves
to the same isovalue as `1` would. At the bottom it is not: `0` means the surface
encloses nothing, and `ε` gives a speck at the field's maximum. That is the
already-broken "a level above anything in the field draws nothing" state, and a
speck is a more legible answer than an empty viewport.

#### Layout

The group's height now differs between modes — `Fraction` carries a slider,
`Absolute` a lone box, `Auto` neither. Part 5's "the panel does not change shape"
rule is about a **wire** being made, state arriving from outside the panel. A
mode change is a deliberate action inside it, on the control directly above the
row, so the change is legible rather than disorienting.

### Drags commit on release — a P3 correction

**As shipped, the fraction slider, the histogram marker and the opacity slider
write node data once, when the drag ends.** The value under the pointer is held
in the editor's own state until then, and the panel — slider handle, numeric
box, marker, readout — renders from it, so the drag is fully live; only the
*surface* waits for the release.

This contradicts what §The fraction slider and §The histogram imply, and the
contradiction was found by running the walkthrough below. Writing per tick made
the application unusable. Every write goes
`set_isosurface_data` → `refresh_structure_designer_auto`, which re-evaluates
**and re-extracts the surface** — marching cubes over the whole grid — on the UI
thread, because the FFI is `frb(sync)` (`CAD_INSTANCE` has no synchronization,
so there is nowhere else to run it). At the ~0.1 s a modest field costs, a 60 Hz
pointer stream cannot be serviced: the app froze and the intermediate surfaces
were never painted anyway, so the cost bought nothing.

Live extraction during a drag is not a tuning problem and is **not** in scope
here. It needs the whole domain `Send + Sync`, a real lock around the global,
snapshot evaluation and a worker thread — that is
`doc/design_background_evaluation.md`, five phases, estimated 5–8 weeks. Once it
lands, restoring per-tick writes is a three-line change in the editor and the
undo-coalescing session below is already in place for it.

**One number in the readout is a preview during a drag.** The readout shows both
coordinates but a drag moves only one, and the kernel has not been asked. The
other is read off the *cumulative curve the histogram is already drawing* —
accurate to one bin — and is replaced by the kernel's exact value the instant
the drag ends. The preview lookups are deliberately separate from anything that
writes: a previewed number never reaches node data.

**Undo coalescing stays**, even though one write per drag would already give one
undo entry. `begin_node_data_drag` / `end_node_data_drag` bracket the gesture,
which keeps the "one drag, one undo entry" contract enforced at the kernel
rather than resting on the editor happening to write once, and it is what
per-tick writes would need again. The editor's `dispose` closes an open session,
so a panel torn down mid-gesture cannot leave recording suppressed.

### The axis is cropped by mass, not fitted to the extremes — a P3 correction

§The histogram says the x axis runs over the field's **nonzero range**, and on a
real `.cube` that is a useless range to plot. Reported from the walkthrough with
a screenshot: an axis running `-16 … 2` with the leftmost twelve decades empty,
the cumulative curve pinned flat at 1.0 across all of them, and every bar
squeezed into the last fifth of the width.

Nothing was wrong with the data. A Gaussian-basis density decays exponentially
away from the nuclei and the writer prints every sample in `%13.5E`, so the far
corners of the box hold genuine values around `1e-16`, and `zero_count` was
zero — nothing had been excluded. **One** such sample sets `nonzero_min` and
stretches the axis by twelve decades.

Which is an argument this design already makes one part over. Part 4, on the
colour domain: *"these quantities span orders of magnitude near the nuclei, so
fitting to the extremes paints the whole surface one flat colour."* An axis
fitted to `nonzero_min` has the same defect for the same reason, and the fix is
the same shape — go by mass.

**The plotted range starts at the tightest magnitude that still leaves no more
than `PLOT_MASS_CROP` (0.01 %) of `∫|v|` below it**, with two guards:

- **never fewer than `MIN_PLOT_DECADES`**, so a field whose mass sits in one bin
  gets an axis with context instead of a sliver;
- **never cropping past the marker**, kept `MARKER_MARGIN` off the left edge. An
  absolute level typed out in the tail is precisely when the user needs to see
  where it falls, and an off-plot marker would be the plot lying about the level
  it exists to locate. **Only when the marker would otherwise fall outside**: a
  marker drag maps the pointer's x through this very range, so widening for a
  marker already on the plot would rescale the axis every frame while the finger
  sat still at the left edge. The range has to be stable under its own output,
  and there is a test that iterates it to say so.

Three things follow, and they are the reason this is worth writing down rather
than treating as a rendering tweak:

- **The crop is presentation only.** `ValueDistribution` and both of its queries
  still cover every sample, so no resolved level, fraction or `Auto` decision
  moves. Only pixels change.
- **The bar heights are normalized over the *plotted* bins.** That is the other
  half of what cropping buys: a peak set by an off-plot bin would flatten
  everything on screen even after the axis was fixed.
- **The caption says when a tail was left out**, naming the magnitude the axis
  starts at and the share left below it. An axis that silently starts above the
  field's smallest value is a zoom the reader did not ask for; the labels alone
  do not say that a zoom happened.

The preview lookups the drag readout uses are deliberately **not** cropped: they
answer questions about values, not about pixels, and cropping them would make a
number depend on the plot's zoom.

### The colour group (P4)

> **Read the five P3 corrections above before this section.** Four of them
> change what it may assume, and the changes are called out below rather than
> left for the implementer to collide with.

The colour domain keeps its current shape and gains two things, both inside the
existing group so the unwired-`color_field` disabling already in place covers
them unchanged:

- **the fit button, beside the "Colormap" heading** — the same placement as the
  swap button beside "Phase colors", which is the panel's existing idiom for
  "an action that belongs to this group". Symmetric or span according to the
  colour field's signedness (§Part 4), disabled when the surface is below the
  vertex floor, with the tooltip saying why. A tooltip is the right home for
  that sentence: it costs no layout, which §The panel says a clause makes the
  governing constraint.
- **the span histogram, below the two range fields**: the surface-restricted
  area-weighted distribution, a **signed** x axis (linear is fine when the
  surface range is narrow; ±0.08 needs no log), and the domain drawn as a
  **span** with two handles. The handles edit `color_min` / `color_max`; the fit
  button sets both. Build it as a **sibling** of the level histogram — see below.

#### What this group does *not* inherit from the level group

**The colour rows stay visible and greyed. They are not hidden.** §The mode is a
unit reversed that for the *level* rows, and the reversal must not be
generalised. A dormant colour domain is a value nothing reads right now **but
which a wire would make live again unchanged**, so it stays inspectable and the
panel keeps its shape when the wire is made. A dormant level coordinate was the
same quantity in the other unit, which is why hiding it lost nothing. The two
groups differ for a stated reason; keep them different.

#### The span histogram is a sibling widget, not three substitutions

An earlier draft of this section called it "the same widget with three
substitutions". That was true of the widget as designed and is not true of the
widget as built. `IsosurfaceHistogram` has since acquired, all of it log-axis
machinery:

- a `PlotRange` mapping position ⇄ magnitude in `log10` space,
- a **mass-based crop** of the leading tail (§The axis is cropped by mass),
- bar heights normalised over the **plotted** bins,
- a stability requirement — the range must not move under its own output,
- a cumulative *mass-at-or-above* overlay whose height *is* the enclosed
  fraction, which has no counterpart on a paint.

A signed, **linear**, span-with-two-handles plot inherits none of that. Write it
beside the level one, sharing whatever painter helpers fall out naturally (the
axis ticks, the label painter), rather than parameterising one widget over two
coordinate systems and two parameter shapes.

**Decide the crop rather than inheriting it.** The level histogram's rule is
"drop the leading tail once it holds under `PLOT_MASS_CROP` of `∫|v|`" — a
statement about a *log* axis and about *mass*, and the colour distribution is
signed, area-weighted and read with percentiles. The equivalent question is
which percentile band the axis should span, and the likely answer is the fitted
band widened by a margin, so a domain set outside it is still visible rather
than silently off-plot. One property does carry over unchanged, because it is
not about logs at all: **the axis must never crop a handle out of view, and must
be stable under its own output** — the handles map the pointer's x through the
very range they set, so a range that widens for a handle already on the plot
walks away from a stationary finger.

#### The handles are drag controls

Both fall under §Drags commit on release: hold the dragged value in the editor's
state, render the panel from it, and write **once** at the gesture's end,
bracketed by `begin_node_data_drag` / `end_node_data_drag`. A colour-domain edit
re-runs the display conversion exactly as a level edit does, so per-tick writes
freeze the application in the same way and for the same reason.

The **fit button** needs no bracket — it is one write.

#### Numbers

`color_min` / `color_max`, and anything the span readout prints, go through
`format_natural` / `formatNatural` (§Numbers read plainly). A fitted `±0.04`
must not come back as `4.0000e-2`.

### Empty states

**Three, all normal, none an error:** no field wired; an analytic field (Part 3);
an upstream not yet evaluated. In each, the mode dropdown stays live, the live
mode's numeric row is disabled and blank (§The mode is a unit — there is only one
row, and none under `Auto`), the plot is replaced by a single line of caption
text naming which of the three it is, and the group keeps its height so the panel
does not jump when a wire is made.

**The colour group has a fourth**, because its distribution comes from the
extracted mesh rather than from the field: *the surface has not been extracted —
display the node*. Listed here so "three" is not read as the complete set; Part 7
§P4 §Where the data comes from says why it exists.

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

`APIValueDistribution` carries bin edges, per-bin mass, the cumulative curve,
the nonzero range, the zero count, and the resolved `(iso, fraction, basis)` for
the node's current setting — so the editor never recomputes a query locally and
the printed number comes from the same code path as extraction. As built it also
carries a `state` naming the empty cases, an `is_exact` flag, and the two
`stored_*_matches` flags §The mode is a unit needs; the entry point is a thin
wrapper over an `frb(ignore)` core taking the designer explicitly, so the logic
is testable without the global `CAD_INSTANCE`.

**The colour signature is the same shape and a completely different data
path** — the mesh it describes does not exist until the display conversion, so
it cannot use the probe helper below. Part 7 §P4 §Where the data comes from is
the section to read before implementing it; do not infer the fetch from the
level one.

**Every edit routes through `_update` -> `model.setIsosurfaceData`**, the path
the existing two text fields already use, which is how the mode, both numbers
and the colour fit all become undoable without any of them being special
(`feedback_persisted_mutations_must_be_undoable`). The slider and the histogram
marker coalesce a drag into one entry.

**Reaching the field:** evaluate the node's `field` argument on demand via a
small `evaluate_node_argument(scope_path, node_id, pin_index)` helper. **This is
not a memo hit.** `eval_memo` is a thread-local installed and dropped by
`with_eval_context` — "alive for exactly one refresh pass" — so an
editor-initiated call outside a pass starts with an empty table and re-walks the
upstream cone every time. It is cheap regardless, for two reasons worth knowing
rather than rediscovering: the field is an `Arc<dyn ScalarField>`, so the walk
hands back a handle instead of re-reading the `.cube`, and the distribution
behind it is `OnceLock`-cached **on the field**, so the sort happens once no
matter how often the editor asks. Do not design against a cross-call memo that
does not exist. **Do not record field handles during the pass** keyed by
`NodeRef` like `node_output_strings` either — within a pass a memo hit *skips*
that write, so a memoized node would leave no entry and the editor would show a
stale or empty plot depending on cache state.

**Invalidation:** refetch on any network-change notification, not only on
selection change. Rewiring `field` or reloading the `.cube` must invalidate; the
failure mode (a correct-looking histogram belonging to the previous field) is
silent.

## Part 6 — Fixtures

The simulation team's zoo lives at `C:\cube-examples-2026-08-25` (16 real
PySCF/QE cubes + README, 375 MB). It is where every constant in this design was
calibrated, and every number below was measured from it.

**It is not a test dependency** — see Part 7 §Where the test data comes from.
Its value is precisely that it is *real*: a synthetic fixture can be tuned to
accept or reject on demand, so only real producers can say whether `0.72` and
`125` are right. That evidence belongs in this document, which is where it is.

| File | Grid | Sign | Auto picks |
|---|---|---|---|
| `ch3-radical/ch3_density` | 80³ | non-neg | abs 0.002 (ratio 34.4) |
| `ch3-radical/ch3_spin` | 80³ | signed | f 0.72 -> 9.08e-3 |
| `ch3-radical/ch3_homo_alpha` | 80³ | signed | f 0.72 -> **3.27e-2** |
| `ch3-radical/ch3_homo_beta` | 80³ | signed | f 0.72 -> **2.68e-2** |
| `esp-sigma-hole/ch3cl` | 80³ | non-neg | abs 0.002 (ratio 65.0 — the tightest) |
| `esp-sigma-hole/ch3cl_esp` | 80³ | signed | colour field — surface p98 **4.00e-2** |
| `nacl-ecp/nacl` | 80x80x110 | non-neg | abs 0.002 (ratio 21.5) — ECP, missing core |
| `si-cluster-vacancy/si-cluster-S3-vacancy` | 80³ | non-neg | abs 0.002 (ratio 43.5) |
| `si-cluster-vacancy/..._spin` | 80³ | signed | f 0.72 -> **1.64e-3** |
| `si-cluster-derivatives/..._elf` | 80³ | non-neg | window rejects (ratio 248) -> 0.497 |
| `si-cluster-derivatives/..._rdg` | 80³ | non-neg | window rejects (ratio 2.7e5) -> 542 |
| `si-cluster-derivatives/..._lapl` | 80³ | signed | f 0.72 -> 2985 (known-wrong) |
| `si-cluster-derivatives/..._sign_l2_rho` | 80³ | signed | f 0.72 -> 8.7e-2 |
| `periodic-tcenter/tcenter_spin_hse` | 144³ | signed | f 0.72 -> **3.02e-4** |
| `si-gemcut-2D3R-149/...-S3` | 247x247x164 | non-neg | abs 0.002 (ratio 44.6); **10,005,476 samples**, histogram path |
| `si-gemcut-2D3R-149/..._spin` | 247x247x164 | signed | f 0.72 -> 1.28e-3 |

All 16 files are listed. Auto is correct on all **eleven** mainline fields
(every density including ECP, 2–149 atoms; every spin density; both orbitals).
The four failures are all post-processed analysis fields and are documented in
Part 2; the sixteenth, `ch3cl_esp`, is a colour field and never sets a level.

**Three external confirmations that `LOCALIZED_FRACTION = 0.72` is right:**
`ch3_homo_alpha` resolves to 0.0327 and `ch3_homo_beta` to 0.0268, both inside
the conventional 0.02–0.05 orbital band; `tcenter_spin_hse` resolves to 3.02e-4
against the handoff's own §1.6 worked example of `3.4e-4` — the closest thing to
an external oracle here.

## Part 7 — Implementation plan

Each phase ends green: `cargo test -j 4`, `cargo clippy`, `flutter analyze`.

### Where the test data comes from

The repo already answers this and an earlier draft of this design ignored it.
`scripts/make_cube_fixtures.py` sorts fixtures by what they are *for*; this plan
uses all three of its tiers, plus one that is not a file at all.

| Tier | Where | What it carries |
|---|---|---|
| **in code** | `SampledField::new(grid, samples)` | everything about `ValueDistribution`. `field_test.rs`'s `unit_ramp` is the precedent — a distribution test wants a field whose answer is computable by hand, and a loop gives that better than any file |
| **committed** | `rust/tests/fixtures/cube/` | only the paths that must cross the loader: the `Auto` branches and the colour fit. Three new files (below), written by the `tests` subcommand |
| **manual** | `sample_data/cube/`, gitignored | the walkthroughs. Regenerated on demand, never asserted against |
| **zoo** | `C:\cube-examples-2026-08-25`, 375 MB, outside the repo | calibration evidence; an **optional** confirmation run |

**The zoo must not become a test dependency.** Keying fifteen assertions to an
absolute path on one machine makes the suite unrunnable for anyone else — and
because no CI runs tests at all (`.github/workflows/release.yml` is a
`workflow_dispatch` release job with no `cargo test`), a skip-when-missing tier
would go quietly dead the day that directory moved and nobody would learn of it.
What survives is an explicit optional tier: gated on `ATOMCAD_CUBE_ZOO` naming
the directory, **skipped with a printed reason** when unset, run by hand before
a release. Every row below marked *zoo* lives there; nothing else may.

The walkthroughs are the one place the zoo is a hard requirement, and that is
fine — they are human steps on the machine that has it.

#### The three new committed fixtures

One grid for all three, so the density and its ESP are a matched pair: 0.3 Å
spacing, **17 × 15 × 19** = 4845 samples, ~66 KB each. Three *different*
dimensions per `doc/testing.md` — a cubic grid hides axis transposition. That
cannot actually bite here (a value distribution is invariant under any
permutation of its samples, and the pair transposes together), but the rule is
free to keep and the next fixture added beside these may not be so forgiving.

Coarse enough to stay tiny, fine enough that the ESP percentile on the 0.002
envelope has converged — against the same analytic functions at 0.1 Å it moves
0.0909 → 0.0899, 1%, for a file 25x larger.

| Fixture | Sign | Pins |
|---|---|---|
| `water_density_17x15x19.cube` | non-neg | the `Auto` **accept** branch. `value_range` `[1.0876e-07, 6.3508e-01]`, `iso_for_fraction(0.72)` = `1.8393e-02` → ratio **9.2**, under `MAX_LEVEL_RATIO`. Also P4's `field` |
| `elf_like_17x15x19.cube` | non-neg | the `Auto` **reject** branch. Bounded 0.5–1.0, `iso_for_fraction(0.72)` = `0.5000` → ratio **250** |
| `water_esp_17x15x19.cube` | signed | P4's `color_field`. `value_range` `[-1.4215, 0.5819]`, but on the 0.002 envelope `p2 = -0.0909`, `p98 = +0.0656` — **15.6x** narrower |

The signed `Auto` branch needs no new file: the committed `p2z_11x11x11.cube`
is signed and resolves to `3.0799e-01` at `f = 0.72`.

`water_density_17x15x19`'s ratio of 9.2 sits *below* the 21.5–65.0 the real zoo
densities give, because `promolecular_density` is valence-only and has no core
cusp. That is the right division of labour, and worth stating plainly: **the
fixture pins the branch, the zoo pins the calibration.** A synthetic field can
be tuned to land wherever you like, so it can never be evidence that `125` is
the right threshold — only that the code takes the branch the ratio implies.

### P1 — `ValueDistribution` (backend only)

`field/distribution.rs`: the type, `LogHistogram`, `ExactCumulative`, the two
queries, the exponent parameter, `EXACT_SAMPLE_LIMIT`; the trait method and
`SampledField`'s `OnceLock` override.

**No new fixture files.** Every field here is built in code.

**Make `EXACT_SAMPLE_LIMIT` injectable** — a `with_limit` constructor or an
equivalent test seam. Without one, the only way to reach the histogram branch is
a file with more than four million samples, which in the zoo means 132 MB of
ASCII floats parsed on every `cargo test` run, on a machine `rust/AGENTS.md`
already says to run `-j 4` on for memory. With one, an 11³ field and a limit of
500 crosses the boundary in microseconds. The "forced histogram vs exact" row
below already presumes this seam; this makes it explicit.

**Two oracles do most of the work.**

*The two-level field* — 4×4×4, four samples at `10.0` and sixty at `1.0`, so the
total mass is exactly `100` and every fraction is a percentage you can check in
your head:

| Query | Exact answer |
|---|---|
| `iso_for_fraction(0.25)`, `iso_for_fraction(0.40)` | `10` |
| `iso_for_fraction(0.41)` | `1` |
| `fraction_for_iso(5.0)` — between the two levels | `0.40` |
| `fraction_for_iso(20.0)` / `fraction_for_iso(0.5)` | `0` / `1` |

It also carries the mass-vs-count trap in one artifact: the high shell is **40%
of the mass and 6.25% of the samples**, so a count-weighted 0.40 percentile
returns `1` where the mass-weighted one returns `10`.

*The Gaussian analytic oracle* — `exp(-r²/2σ²)` on a 65³ grid spanning ±4σ. Read
as a density, a spherical Gaussian's enclosed-mass fraction has a closed form,
so the expected isovalue is analytic and independent of both σ and the grid:
`iso_for_fraction(f) = exp(-x_f)` where `P(3/2, x_f) = f`, the regularized lower
incomplete gamma.

| f | expected iso |
|---|---|
| 0.30 | 0.490747 |
| 0.50 | 0.306362 |
| 0.72 | 0.147076 |
| 0.90 | 0.043906 |

**Tolerance 2%, measured rather than assumed.** `iso_for_fraction` returns a
*stored sample*, so it approaches the continuum answer only as the grid refines:
the measured maximum error over those four fractions is **18% at 17³, 3.0% at
33³, 1.1% at 65³**. 65³ is 275k samples — nothing in code, impossible as a
committed file, which is exactly why this oracle belongs in the in-code tier. Do
not run it coarse, and do not tighten the tolerance to make it look better: the
round-trip below is the tight check, and it is tight at any resolution.

| Tier | Test | Asserts |
|---|---|---|
| code | Two-level 4×4×4 | the four literal query answers above |
| code | Two-level 4×4×4, count-weighted comparison | 40% of the mass is 6.25% of the samples — the trap Part 1 exists to avoid |
| code | Gaussian 65³ against the analytic oracle | four isovalues within 2% |
| code | `f -> iso -> f`, f in {0.3, 0.5, 0.72, 0.9} | never undershoots, overshoots by at most the tied group's mass — resolution-free, the tight check. **Not** ±1/N (§bijection) |
| code | Gaussian at 0.25 Å and 0.125 Å | same f gives the same iso within a bin — grid independence |
| code | Gaussian samples on a sheared grid | identical fraction to the unsheared same samples — the determinant cancels |
| code | Forced histogram vs exact, same field | agree within one bin |
| code | Forced histogram vs exact, near the ratio threshold | **the same `Auto` branch.** 2048 bins over ~48 decades is ~5% per bin against a 1.7x decision gap, so this holds with room — but the ratio is now a *decision*, not just a number, and quantization must not flip it |
| code | 11³ field, limit forced to 500 | crosses `EXACT_SAMPLE_LIMIT`; both paths have a real input, with no 132 MB file |
| code | Samples at `1e-75` | land in `zero_count`, not the lowest bin; `total` unaffected |
| code | All-zero field | `Some`, `total == 0`, both queries `None` — no divide by zero |
| code | Zeros + nonzeros | `zero_count` right, zeros excluded from bins |
| code | `fraction_for_iso` above the largest sample, and below the smallest | `0` and `1` — never an index lookup |
| code | Exponent 2 on a signed field | accumulates `v²` |
| code | `SampledField` clone after the cache is warm | the clone **shares** it — `Arc::ptr_eq` on the two, and no second sort. A deep copy or a rebuild both pass a naive "the clone has a distribution" test, so assert identity, not presence |
| *zoo* | `iso_for_fraction(0.72)` on the five densities | 4.30e-2 – 1.30e-1, ratio 21.5–65.0; ELF 248, RDG 2.7e5 |
| *zoo* | the same five, box cropped to 0.8x / 0.6x / 0.5x | every density ratio stays under 125, every impostor over it — the padding invariance the window rests on |
| *zoo* | `si-gemcut-...-S3`, 10,005,476 samples | the real limit crossing, at real scale |

### P2 — `LevelMode`, `auto_level`, readout

`LevelMode`, the two properties, text spellings and the inference rule,
`auto_level` / `LevelBasis`, `eval` resolution and validation, the subtitle, and
the `NetworkResult::Isosurface` readout arms.

| Tier | Test | Asserts |
|---|---|---|
| code | New node's `Default` | `Auto` |
| committed | `water_density_17x15x19` | `0.002`, basis `non-negative, density-like` |
| committed | `elf_like_17x15x19` | `0.5000`, basis `non-negative, atypical` — the window rejects |
| committed | `p2z_11x11x11` | `3.0799e-01`, basis `signed field` |
| code | Density with one sample forced to `-1e-12` | still the density branch — what `NEGATIVE_TOLERANCE` buys over `min >= 0` |
| code | All-zero field, Auto and Fraction | descriptive error; no panic, no silent zero level |
| code | Analytic field (no distribution), Auto | non-negative → bare `DENSITY_LEVEL`, basis `unchecked`; **signed → the Part 3 error** |
| code | Analytic field, Fraction | the "needs stored samples" error, not a panic |
| code | Auto → Fraction / Absolute handover | pre-filled; extracted surface unchanged |
| code | Wired `level` pin under Auto | non-blocking warning, pin ignored, surface still produced |
| code | `.cnnd` round-trip, all modes | mode and both numbers survive |
| code | Text round-trip | mode **and both numbers** emitted; the dormant number survives unchanged |
| code | `isosurface { }` applied to an existing Fraction node | mode unchanged — omission does not reset to auto |
| code | Text `level: 0.002` alone / `level_fraction: 0.9` alone | infers `Absolute` / `Fraction` |
| code | Text naming both, no mode | error naming both, not silent precedence |
| code | `eval` in fraction mode | equals `iso_for_fraction(f)` |
| code | Fraction out of range (0, 1, 1.5, NaN) | descriptive error |
| code | Absolute mode | byte-identical `IsosurfaceData` to before this change |
| code | Quality multiplier changed | resolved level does not move |
| code | The readout string | `encloses 72.0% of ∫\|v\|` **verbatim**, and the basis appended under Auto. §Readout calls this wording load-bearing; pin it, or the friendlier paraphrase it warns against ships unnoticed |
| code | Undo of a mode toggle, and of a `level_fraction` change | one undo restores the previous pair — `feedback_persisted_mutations_must_be_undoable` covers both new properties, not only P4's colour fit |
| *zoo* | Auto on the eleven mainline fields | Part 6's numbers exactly, including `1.64e-3`, `3.27e-2` and `3.02e-4` |

**Deliverable:** fraction and auto work end to end from the text format and CLI;
the pin readout prints both numbers. First user-visible milestone.

### P3 — Editor: slider, readout, histogram

`field_distribution_api.rs` + its `rust_input` entry; `evaluate_node_argument`;
`APIValueDistribution`; mode toggle, log slider, dual readout, histogram widget
in `isosurface_editor.dart`; refetch on network change.

Widget *behaviour* is covered by the walkthrough, per
`feedback_manual_test_for_editor_ui`. Two things are not widget behaviour and
must not hide behind that rule:

| Tier | Test | Asserts |
|---|---|---|
| code | `APIValueDistribution` shape, and the three empty states | no field wired / analytic field / upstream not yet evaluated each report themselves, and none is an error |
| code | **Invalidation across a rewire** | two `get_isosurface_level_distribution` calls, with the `field` pin rewired between them, return *different* distributions. This is the failure §Invalidation calls silent — a correct-looking histogram belonging to the previous field — and it is a Rust assertion at the API seam, not a widget concern |
| code | Drag **coalescing**, slider and histogram marker | a drag from 0.9 to 0.99 leaves **one** undo entry, not one per tick, from either control. A log slider is the canonical undo-flooding case, and the coalescing rule belongs in the model, where it is testable |
| code | The drag **preview** lookups | a magnitude and a fraction read back consistently off the cumulative curve, and an empty distribution previews `null` rather than throwing. These are what the readout shows mid-drag (§Drags commit on release); they must never be written into node data |
| code | **Natural number formatting** | the Rust and Dart twins agree case for case: plain decimals in `[1e-4, 1e6)`, scientific outside, trailing zeros trimmed in both forms, only the mantissa trimmed (`1e-10` keeps its exponent), and no `+` on a positive exponent. Two tables, one rule — they are the thing that would drift silently |
| code | The **axis crop** | a vacuum tail carrying under 0.01 % of `∫|v|` is dropped; a distribution with mass everywhere is *not* cropped; the marker is never left off-plot; a one-bin mass spike still gets `MIN_PLOT_DECADES`; and a position round-trips to a magnitude over the **cropped** span, which is what a marker drag reads. An axis that quietly starts above the field's smallest value is where a plot begins lying (§The axis is cropped by mass) |
| code | Mode switch **conversion** | every switch away from `Auto` fills the newly-live coordinate from the resolved value, so `Absolute ⇄ Fraction` never moves the surface; switching **to** `Auto` does move it, back to the field's own level. (Supersedes the handover row this replaces — §The mode is a unit) |
| code | A typed constant survives a round trip | `Absolute 0.002` → `Fraction` → `Absolute` returns **`0.002`**, not the nearby sample an unconditional conversion would give. Drag the fraction in between and it must return the *converted* value instead — both halves of the `stored_*_matches` rule |
| code | A mode switch with **no field** | falls back to the parked number for each coordinate, which is why both properties stay in storage even though only one is ever on screen |

**Manual walkthrough**

Steps 1, 2, 5, 6, 9 and 10 were rewritten after the first run — see §The mode is
a unit, not a second parked value and §Drags commit on release.

1. `import_cube` -> `si-cluster-vacancy/si-cluster-S3-vacancy.cube` ->
   `isosurface`. **Expect** auto mode, a vdW envelope, the basis line
   `auto: non-negative, density-like`, a readout reading `|v| = 0.002 ·
   encloses …% of ∫|v|`, and the marker where the cumulative curve is about
   0.995. **No numeric row at all** — under auto neither coordinate is live, and
   the readout carries both. Tiny spheres at the nuclei instead means auto took
   the fraction branch — check `value_range().min`.
2. Set **Mode** to `Fraction`. **Expect** a single *Fraction* row to appear, live,
   pre-filled with about 0.995, **no absolute row**, and **the surface not to
   move at all**.
3. Drag the slider toward 0.999. **Expect** the fraction, the isovalue and the
   marker to track the pointer live, and **the envelope to grow when you let
   go** — not during the drag (§Drags commit on release). The application must
   stay responsive throughout. Then Ctrl-Z once: the fraction returns to its
   step-2 value in a **single** undo, not a walk back through the drag.
4. Drag the **histogram marker** instead. **Expect** the same edit through a
   different control — both numbers update live, the surface redraws on release,
   one undo entry for the drag.
5. Switch to `Absolute`. **Expect** the fraction row to be replaced by a single
   *Absolute* row holding the isovalue the fraction resolved to, and **the
   surface not to move**. Now type `0.002`: the conventional envelope, and a
   readout naming the fraction it encloses.
6. Switch back to `Fraction`, then to `Absolute` again. **Expect** the fraction
   that `0.002` encloses on the way out, **the surface not to move in either
   direction**, and `0.002` — *exactly*, not `0.00200034…` — back in the absolute
   row at the end. That last part is what the match flags buy: `iso → f → iso` is
   not a bijection, so an unconditional conversion would mangle the constant you
   typed. Then drag the fraction somewhere else and switch to `Absolute` again:
   this time the number **must** be the converted one, not a resurrected `0.002`.
7. Type `0.1` into the fraction box. **Expect** it accepted, and the slider
   **disabled with its handle at the left stop** — outside its window, not
   pretending `0.1` is `0.30`. Type `0.9`: the slider comes back live.
8. Switch to auto, rewire to `..._spin.cube`. **Expect** basis `signed field`
   and level **1.64e-3** — an exact expected number, not an impression. Switching
   *to* auto is the one switch that may move the surface, and here it should.
9. Wire anything into `level` while in auto. **Expect** an amber non-blocking
   warning, an unchanged surface, and still no numeric row — not a red error, not
   a silently dropped wire. Switch to `Absolute`: the row appears, **greyed**,
   showing what the wire drives.
10. Delete the `field` wire. **Expect** the mode dropdown still live, the live
    mode's row blank and greyed, a caption saying there is no field, and no
    error, no stale histogram, no layout jump. Switching modes here changes
    nothing but the mode — there is no distribution to convert through, so each
    coordinate keeps its parked number.

### P4 — Colour domain: surface distribution and fit

Area-weighted surface-restricted distribution in `atomcad-display`'s extractor
output; `get_isosurface_color_distribution`; the signed span histogram and the
symmetric / span fit buttons.

**Start by reading the five P3 corrections in Part 5.** They were written after
the walkthrough and they change four things this phase would otherwise assume:
the non-live colour rows stay greyed (they are *not* hidden — §The mode is a
unit says why the two groups differ), the span handles commit on release, the
span plot is a sibling widget rather than a reskin of the level histogram, and
every printed number goes through `format_natural`. Part 5 §The colour group
(P4) carries the details.

#### Where the data comes from — and why it is not P3's probe

P3 gave the editor `StructureDesigner::evaluate_node_argument` /
`evaluate_node_output`, so the obvious move is to reuse them. **It does not work
here**, and the reason is worth knowing before an hour goes into it: those
return `NetworkResult::Isosurface(IsosurfaceData)`, which is the
*specification*. The mesh does not exist yet. Marching cubes runs one stage
later, in the display conversion
(`NetworkEvaluator::generate_isosurface_output` →
`atomcad_display::isosurface::extract_isosurface`), because that is the first
stage that can see the extraction preferences — the split
`design_isosurface_node.md` opens with.

So the surface-restricted distribution is computed **where the mesh is**: in the
display conversion, and parked on `NodeSceneData` beside the
`NodeOutput::Isosurface(mesh)` it is derived from. The API reads the scene.
Three consequences:

- **It exists only while the node is displayed.** That is a **fourth empty
  state** for the colour group, alongside Part 5 §Empty states' three: *the
  surface has not been extracted — display the node*. Say so, rather than
  offering a fit button that silently does nothing.
- **Do not recompute it per panel rebuild.** P3's level distribution is fetched
  from `build`, which is safe only because it hands back an `Arc` handle to a
  `OnceLock`-cached sort. A re-extraction is the ~0.1 s marching-cubes run that
  §Drags commit on release exists because of. Compute once per refresh, read
  many times.
- **`NodeSceneData` gains a field**, so its `new()`, the `generate_scene` struct
  literal and the direct-construction sites in
  `tests/structure_designer/multi_output_unit_test.rs` all need updating — the
  rule `crates/atomcad-structure-designer/src/AGENTS.md` already states for that
  struct.

#### Shape of the API

Mirror P3's `field_distribution_api.rs`: a
`#[flutter_rust_bridge::frb(ignore)] pub fn` core taking the designer
explicitly, with the `frb(sync)` entry point a thin wrapper over it, so the
logic is testable without the global `CAD_INSTANCE`.
`APISurfaceValueDistribution` is a **separate type** from
`APIValueDistribution` — signed, area-weighted, percentile-queried — and should
not be forced through the level struct. If P4 does evaluate anything, the probe
hygiene rule applies: `evaluate_in_scope` saves and restores `print_log`,
`last_eval_profile` and `last_memo_counts`, and a new evaluation path must do
the same or a panel repaint spams the Console.

#### The fit contradicts shipped prose

The `isosurface` node description **and** `doc/reference_guide/nodes/atomic.md`
both argue at length that *"the colour range is never fitted for you, and this
is deliberate"*. That is true today and stops being true the moment this phase
lands. **Rewrite both in the same change.** The argument survives in modified
form — the *volume* extrema are still the wrong thing to fit to, and what the
button does is fit to the **surface**, which is the whole point of Part 4 — but
a user who presses a fit button and then reads that the range is never fitted
has been told the application is broken.

| Tier | Test | Asserts |
|---|---|---|
| committed | `water_density_17x15x19` + `water_esp_17x15x19` | surface `p98(\|v\|)` **more than 10x** below the volume's `\|range\|` (measured 0.0909 against 1.4215, **15.6x**), and the symmetric fit lands in 0.05 – 0.12. **Not three significant figures:** the code computes an area-weighted percentile over mesh vertices at a preference-driven resolution, while the numbers here are a voxel-band proxy on the file grid |
| code | **Area weighting**, constructed mesh | a linear colour ramp over a surface whose two halves carry equal area but 4:1 triangle counts: the area-weighted median is the geometric midpoint within a few percent, the vertex-count-weighted one is pulled measurably toward the fine half. Without a constructed answer this row cannot fail — "different, correct" asserts nothing |
| code | Same fit at two quality multipliers | agree within a few percent |
| code | Surface below the vertex floor | the fit refuses and says so; it does not write a number |
| code | Symmetric fit, signed colour field | exactly `[-q, +q]`; zero maps to the ramp midpoint |
| code | Span fit, non-negative colour field | `[p2, p98]` |
| code | Fit writes node data | `color_min` / `color_max` concrete; no mode flag in the `.cnnd` |
| code | Fit is undoable | one undo restores the previous domain, via `setIsosurfaceData` |
| code | Quality multiplier changed after a fit | the saved domain does not move |
| code | The **fourth empty state** | an `isosurface` node that is not displayed reports *not extracted* rather than erroring or offering a fit button that does nothing — the distribution lives on the scene, and there is no scene entry for a hidden node |
| code | The span handles **coalesce** | a two-handle drag leaves **one** undo entry, not one per tick — the same rule §Drags commit on release pins for the level controls, and the same `begin_node_data_drag` bracket |
| code | The span axis is **stable under its own output** | feeding the range its own handle position must not move it; the handles map the pointer's x through the range they set, so a range that widens for a handle already on the plot walks away from a stationary finger (§The axis is cropped by mass) |
| code | The fitted numbers **read plainly** | `±0.04` prints as `0.04`, not `4.0000e-2` — the `format_natural` / `formatNatural` twins |
| *zoo* | `ch3cl` + `ch3cl_esp` | p98 **4.0e-2** against the volume's **9.7e+1** — the 2427x that motivates the whole of Part 4 |

**Manual walkthrough:** `esp-sigma-hole/ch3cl.cube` as `field`,
`ch3cl_esp.cube` as `color_field`. Press the symmetric fit. **Expect** the
domain near **±0.040**, and the **sigma hole** visible — a positive patch on the
chlorine along the C–Cl axis. That feature is the acceptance criterion: it
disappears if the surface is coloured by density, and washes out entirely if the
domain is fitted to the volume's ±97.

Then four steps the P3 corrections add:

1. **Hide the node** (eye off) and reopen the panel. **Expect** the span plot
   replaced by a caption saying the surface has not been extracted, and the fit
   button disabled — not an error, and not a button that does nothing.
2. **Drag a span handle.** **Expect** the number and the handle to track the
   pointer live, the surface to recolour **when you release**, the application
   to stay responsive throughout, and **one** undo entry for the drag.
3. **Drag a handle to the plot's edge and hold it there.** **Expect** the axis
   not to creep.
4. **Check the two numeric rows are still there and greyed** with
   `color_field` unwired. They are *not* hidden: unlike the level coordinates,
   a wire would make them live again unchanged.

## Documentation touchpoints

Per `AGENTS.md`, in the same change as the code:

- `doc/reference_guide/nodes/atomic.md` — the three level modes, what the
  fraction means, what auto decides and on what basis, the note that auto is
  volatile and a figure should be frozen in fraction or absolute mode, and the
  readout (P2); the editor's controls — the mode dropdown as the only way to
  leave auto, that only the live coordinate's row is shown, and how to read the
  histogram (P3); then the colour fit (P4)
- **`doc/reference_guide/nodes/atomic.md` and the `isosurface` node's own
  description, in the P4 change specifically.** Both currently argue that *"the
  colour range is never fitted for you, and this is deliberate"* — true today,
  false the moment the fit button ships. The argument survives in modified form
  (the *volume* extrema are still the wrong thing to fit to; the button fits to
  the **surface**), but leaving it as written tells a user who just pressed the
  button that the application is broken. **The node description is the primary
  home for anything longer than a clause** (§The panel says a clause): it is
  Markdown, rendered by the ⓘ button, and P4's material — what the two fits do,
  why the vertex floor exists, the ramp-orientation caveat — belongs there
  rather than in a panel caption
- `doc/design_isosurface_node.md` — a pointer here from §The `Isosurface` value
- `rust/crates/atomcad-crystolecule/src/AGENTS.md` — `field/distribution.rs` in the
  module map, and the invariant that the distribution is over **stored samples**
  so a resolved level never depends on extraction resolution (P1)
- `doc/testing.md` — the two-level and analytic-Gaussian oracles as the pattern
  for distribution fixtures, and the tiering rule: a distribution test builds its
  field in code, a fixture file earns its place only by crossing the loader (P1)
- `rust/crates/atomcad-structure-designer/src/AGENTS.md` — the new
  `NodeSceneData` field carrying the surface distribution, in the list of
  derived-at-`generate_scene` state, beside `unit_cell` and
  `construction_plane`. That section already states the rule this follows: a
  semantic property of a node's output must be derived at scene time rather than
  pattern-matched off the lossy `NodeOutput` afterwards (P4)
- `lib/structure_designer/node_data/AGENTS.md` — the span histogram as a
  **sibling** of `isosurface_histogram.dart`, and why the colour rows stay
  greyed where the level rows are hidden (P4)
- `scripts/make_cube_fixtures.py` — already extended: `elf_like`, and the three
  committed fixtures the `tests` subcommand now writes. Their header comments
  carry the measured values the Rust tests assert, so regenerating and
  re-measuring are the same act
