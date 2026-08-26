# Design: choosing the isolevel — enclosed fraction and the distribution readouts

Companion to `doc/design_scalar_fields.md` (ingestion) and
`doc/design_isosurface_node.md` (the node and its extraction pipeline). Those
two build a working isosurface; this one answers the question they left the
user holding: **which level should I draw it at?**

Prompted by `doc/from_simulation_team/ATOMCAD-DENSITY-VIZ-HANDOFF.md` §1.6,
which reports this as the biggest open problem in their own eight-round
prototype and proposes the parameterization adopted here. That document is
input to several designs; only its level-selection half is in scope here.

## Motivation

The `isosurface` node's level is a raw `f64` in a text field, defaulting to
`0.02`. The node's own description admits the problem:

> This is the **molecular-orbital amplitude** convention, which is an order of
> magnitude too large for an electron density (`0.002`). Accepted rather than
> fixed: a field's meaning is not recoverable from its numbers, so no single
> default serves both.

That reasoning is sound and this design does not overturn it — it takes it
seriously. There is indeed no single number that serves every field, in
*either* coordinate (§Conventional levels are kind-dependent in either
coordinate measures how far apart they are). So this design does two things
instead of looking for that number: it adds a **coordinate** in which the
level can be stated and understood, and it lets the node **read the default
off the field** rather than off a constant (§`LevelMode::Auto`).

Three things are wrong with an absolute isovalue as the primary control:

1. **It has no defensible default.** `0.02` is right for an amplitude and
   ~10x wrong for a density. There is no third number that is right for both.
2. **It is not a control anyone can operate.** A field spans roughly ten
   orders of magnitude (the simulation team measured 6.4e-12 to 1.1e-1 on a
   real 144^3 QE spin cube). A linear text box over that range is not a
   control, it is a guessing game, and the user has no picture of where the
   interesting levels are.
3. **It does not tell you what you got.** `0.001 a.u.` — the field's folk
   convention — encloses anywhere from 95% to over 99% of the electron density
   depending on the system, and the picture does not say which. People have
   argued about this for decades precisely because no viewer prints the number
   that would settle it.

Note the third claim carefully: it is that an absolute level leaves the
enclosed quantity **unstated**, *not* that an absolute level is unstable. For
total electron densities it is in fact the more stable of the two coordinates —
see §What the fraction is and is not better at, which measures it. This
document adds a coordinate and a readout; it does not claim the old one was
wrong.

## Scope

**In scope:**

- the **enclosed fraction** parameterization of the isolevel, and the value
  distribution on `ScalarField` that computes it
- an **auto** level mode that picks the default from the field's own
  signedness, so a density and an orbital each get a sane first render
- the dual readout (`|v| = ... · encloses ...%`) in the pin hover and the editor
- the editor's log slider and the **distribution histogram**
- the **colour domain**, which gets the same treatment adapted to a different
  question (§The colour domain)

**Out of scope**, each named with where it belongs:

| Item | Belongs in |
|---|---|
| Nested shells, cutaway, flood animation (handoff §3.3, §3.4, §3.7) | a display-side successor to `design_isosurface_node.md` |
| Periodic/isolated classification, minimum-image bonding (§1.3, §3.5) | beside the loader, in `design_scalar_fields.md`'s parsing rules |
| 13-byte data blocks, negative voxel counts, the cube zoo (§1.1, Part 7) | same |
| Declared field kind and the `∫v·dV` sniff (§1.8) | its own design, and it would **not change the level** — see §Why field-kind sniffing is *not* part of this. One hook is left for it (§The accumulation exponent) |
| Slices, MIP, line profiles (Part 5) | a separate instrument design |
| Raymarching, fp16, empty-space skipping (§2B.1, §2B.2) | not applicable — extraction is CPU `f64` |

## Background: what "enclosed fraction" means

The level is parameterized by the fraction of the field's **total integrated
|v|** that lies inside the surface — treating each sample's value as a density
and integrating it over the box. It is a *mass* fraction, not a voxel count.

```
sorted  = sort(|v| over all samples, DESCENDING)
cumsum  = prefix sums of sorted
total   = cumsum[last]
iso_for_fraction(f)  = sorted[ smallest k with cumsum[k] >= f * total ]
fraction_for_iso(v)  = cumsum[ position of v in sorted ] / total
```

Because the accumulation runs from the largest value downward, the enclosed
region is `{ |v| >= iso }` — exactly the region the extracted surface bounds.

**The distinction from a voxel-count percentile is not academic.** Measured on
a 96^3 box at 0.2 Å holding a single `exp(-2r)` blob — a deliberately mild
case, far less extreme than a real vacuum-dominated supercell:

| Parameterization | at 0.72 | resulting picture |
|---|---|---|
| **mass fraction** | iso = 2.33e-2, enclosing **0.058%** of voxels | a tight molecular envelope |
| voxel-count percentile | iso = 1.61e-13, enclosing ~28% of voxels | the vacuum; effectively the whole box |

Eleven orders of magnitude apart, and only one of them is a picture. This is
handoff §1.5 restated: almost all of any cube box is tails and vacuum, so
anything parameterized by voxel *count* is dominated by empty space and every
useful level crowds into the top fraction of a percent. Parameterizing by mass
spreads precisely the range that matters.

For the record, the same field across the range:

| f | iso | voxels enclosed |
|---|---|---|
| 0.30 | 1.40e-1 | 0.010% |
| 0.50 | 6.73e-2 | 0.024% |
| 0.72 | 2.33e-2 | 0.058% |
| 0.90 | 5.10e-3 | 0.166% |
| 0.99 | 2.31e-4 | 0.659% |

### What the fraction is good for

- **Invariant to units and normalization.** The same f means the same thing on
  a `.cube` in `e/bohr^3` and on a future Molden-derived density, at any
  resolution. It is *not* invariant to a change of chemical system — see the
  next section, which measures that.
- **It answers the scientific question.** On an electron density,
  `∫|v| dV` *is* the electron count, so f = 0.72 literally means 72% of the
  electrons are inside. On a spin density it is the total absolute spin.
- **It has a defensible default where no absolute convention exists** — spin
  densities, deformation densities, ELF, NCI, anything a user computes
  themselves. This is the originating case: the simulation team's data is QE
  spin-density cubes of T-center supercells, and for those **there is no
  0.002**. They did not reach for the fraction because it beat the convention;
  they reached for it because their fields have no convention to beat.
- **It is operable.** A slider over f works. A slider over ten decades does not.

### What the fraction is and is not better at

The fraction is **not** a universally better coordinate than an absolute
isovalue, and the design would be dishonest to imply it. Measured on
promolecular Slater-shell atoms (correct electron counts and exponents; exact
radial integration, because a grid coarse enough to be affordable misses the
core), with the van der Waals radius as a proxy for "the surface you wanted":

| Held fixed across C / N / O / F | resulting `r / r_vdW` | spread |
|---|---|---|
| **absolute 0.002** | 0.695 – 0.763 | **1.097** |
| fraction 0.99 | 0.572 – 0.763 | 1.333 |

**For total electron densities the absolute convention tracks the envelope
better than a fixed fraction does.** The mechanism is systematic rather than
noise: heavier atoms hold more of their density in the core, so "exclude 1% of
the electrons" cuts progressively further in as Z rises — fluorine at f = 0.99
needs iso = 0.0114, nearly six times the convention. `0.002` is an established
convention because it works.

*(Isolated atoms with promolecular Slater shells, not molecular SCF densities;
the direction and the mechanism are solid, the exact spread is indicative.)*

So the two *coordinates* are not a good tool and a legacy one. They serve
different situations:

| | absolute | fraction |
|---|---|---|
| Total electron density | **preferred** — a strong, empirically well-behaved convention | fine, but no better |
| Spin / deformation density, arbitrary fields | no convention to use | **the only defensible option** |
| Reproducing a published figure | **required** — figures quote isovalues | no |
| Saying what the surface actually holds | no | **always** |

Which is why **both numbers print in both modes** (§The dual readout). The
choice of mode is a choice of which one you *steer* with, never a choice of
which one you get told.

### Conventional levels are kind-dependent in either coordinate

A recurring temptation is to read the numbers below as a strike against the
fraction. They are not: the absolute coordinate has exactly the same
kind-dependence, and it is the thing the node's existing description already
apologizes for ("no single default serves both").

Measured with the same model:

| Field kind | Convention | Encloses |
|---|---|---|
| Density, O atom | 0.002 | 99.63% |
| Density, C atom | 0.002 | 99.00% |
| Density, H atom | 0.002 | 88.11% |
| Orbital amplitude (Slater 2p, ζ = 2.25) | 0.05 | 67.49% |
| Orbital amplitude | 0.02 | 82.11% |

Read backwards, the split is stark: on an orbital, **f = 0.72 resolves to
iso = 0.0395** — dead centre of the conventional 0.02–0.05 band, so the
simulation team's default is well tuned. On a density, **f = 0.72 resolves to
iso = 0.26 (C) or 0.86 (O)** — three orders of magnitude above the convention,
deep inside the core, drawing tiny spheres at the nuclei.

**The structural reason** is the one to remember, because it predicts the right
answer for a field nobody has tabulated: a total electron density has a
**nuclear cusp** holding most of the electrons in a minuscule volume, so almost
any sane surface already encloses over 98% and the whole useful range is
0.98–0.999. An orbital amplitude, spin density or deformation density has no
core cusp — the field *is* the diffuse part — so its useful range is 0.5–0.9.
The two regimes barely overlap.

A single default therefore cannot serve both, in either coordinate. That is
what §`LevelMode::Auto` exists to resolve.

### The one case where it is a proxy

On an **orbital amplitude** the conventional enclosed quantity is
`∫|psi|^2 dV` (probability), not `∫|psi| dV`. This design accumulates `|v|`
unconditionally, because it has no declared field kind to switch on.

**Consequence for the readout wording, and it is load-bearing:** the readout
says `encloses 72.2% of ∫|v|`, never "72.2% of the electron density". The
first is true for every field; the second is false for an orbital. Do not
paraphrase this string into something friendlier — the literal integral is
what makes it honest.

### The accumulation exponent

The distribution builder takes the exponent as a parameter (`1` today) rather
than hardcoding `abs`. This is the single hook left for a future declared field
kind (handoff §1.8): a field declared as an amplitude would accumulate `|v|^2`
and its readout would then mean the conventional thing for orbitals. Nothing
else in this design anticipates that feature.

## `ValueDistribution`

```rust
// atomcad-crystolecule/src/field/distribution.rs

/// The distribution of |v| over a field's stored samples, in the form that
/// answers both directions of the level question.
///
/// Built once per field and cached, because both the extractor and every
/// readout ask for it repeatedly and it costs a full pass over the samples.
#[derive(Debug)]
pub struct ValueDistribution {
    /// Total accumulated mass, `sum |v_i|^exponent`.
    total: f64,
    /// Exact representation: the descending-sorted magnitudes and their
    /// prefix sums. `None` above `EXACT_SAMPLE_LIMIT`.
    exact: Option<ExactCumulative>,
    /// Log-space histogram, always present. Bin `b` spans
    /// `[10^(lo + b*w), 10^(lo + (b+1)*w))` in |v|, and carries the mass in
    /// that bin plus the running mass above it.
    histogram: LogHistogram,
    /// Smallest and largest nonzero |v|, the histogram's span.
    nonzero_range: Option<(f64, f64)>,
    /// Count of exactly-zero samples, excluded from the histogram (log space
    /// has no home for them) but reported, so "all vacuum" is visible.
    zero_count: usize,
}
```

### Exact versus histogram

Both, with a size threshold. `EXACT_SAMPLE_LIMIT = 4_000_000` samples.

- **Below the limit**, sort and prefix-sum. A 144^3 cube is 3.0M samples; the
  sorted `f32` array plus an `f64` cumsum is ~36 MB, and queries are exact.
- **Above it**, a **2048-bin log-space histogram** built in one O(N) pass,
  which resolves a percentile to within a bin. A 347x348x220 production cube is
  26.6M samples: the exact structure would be ~320 MB, which this project will
  not spend, and the sort itself is real work.

The histogram is built **always**, at both sizes, because it is also what the
editor plots (§The histogram). The exact structure is an accuracy refinement on
top, not an alternative representation.

**The threshold is a memory decision, not a speed one.** This project already
runs into memory pressure on this machine (`cargo test -j 4` exists for that
reason), and a 320 MB transient allocation inside an evaluation pass is the
kind of thing that turns into a swap storm rather than a slow frame.

### Weighting, and what cancels

The accumulation is an unweighted sum over stored samples. Two things follow:

- **Voxel volume cancels.** `∫|v| dV = Σ|v_i| · V_cell` with
  `V_cell = |det(axes)|` constant across the grid, so the *ratio* is unaffected
  by spacing or shear. Nothing needs a determinant. Worth the code comment,
  because it is a natural thing to worry about and then re-derive later.
- **Boundary samples are not half-weighted.** A node-centered grid's exact
  trapezoidal integral weights the outermost sample planes by 1/2. For an
  isolated system the field there is ~0 and the difference is far below the
  histogram's own bin resolution. For a **periodic** supercell it is not zero —
  the boundary plane is a periodic image of the opposite face and is genuinely
  double-counted. The effect on a ratio is sub-percent for any realistic
  dimension count, and correcting it would mean the fraction depended on a
  periodicity classification this design does not yet have. Documented, not
  corrected; revisit alongside the periodic work.

### Trait surface

```rust
pub trait ScalarField: Send + Sync + std::fmt::Debug {
    // ... existing methods ...

    /// The |v| distribution over the field's stored data, for level selection.
    ///
    /// `None` for a source with no stored samples (every analytic field), for
    /// the same reason `value_range` and `native_grid` are `None` there: there
    /// is nothing to scan until something samples it. A consumer must handle
    /// `None` — see §Analytic fields.
    fn value_distribution(&self) -> Option<&ValueDistribution> { None }
}
```

`SampledField` overrides it, caching in a `OnceLock<ValueDistribution>`. The
lock, not eager construction in `SampledField::new`: a field that is only ever
`sample_field`-probed should not pay for a sort, and `import_cube` builds fields
during a file load where a spurious multi-second pause is very visible.

`&ValueDistribution` rather than a value, because the payload is large and
every caller only reads. `SampledField` is `Clone`; the `OnceLock` clones as
unset, so a clone rebuilds on first query rather than deep-copying tens of
megabytes.

### The two queries are not a bijection

`iso_for_fraction` and `fraction_for_iso` are inverses of each other, and their
round-trip is the primary acceptance test — but only in one direction:

- `f -> iso -> f` round-trips to within ±1/N. This is handoff §3.8 test 1.
- `iso -> f -> iso` **does not.** Any isovalue between two adjacent sorted
  sample values encloses the same set of samples, so `fraction_for_iso` is a
  step function and `iso_for_fraction` returns one canonical representative per
  step. Type `0.0015`, convert to a fraction and back, and you get whichever
  stored value bounds that step.

That asymmetry drives a node-data decision below (§Two properties, not one).

## Node changes

### `LevelMode`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LevelMode {
    /// The level is chosen from the field itself. Neither stored number is
    /// live. See §`LevelMode::Auto`.
    #[default]
    Auto,
    /// `level` is an isovalue magnitude in the field's own units.
    Absolute,
    /// `level_fraction` is the share of the field's total ∫|v| enclosed.
    Fraction,
}
```

Text-format spelling `auto` / `absolute` / `fraction`, via the same
`*_to_text` / `*_from_text` pair `Colormap` already uses, and for the same
reason: the `.cnnd` JSON form and the text form should be free to diverge.

### `LevelMode::Auto`

**Auto is the default mode, and it exists because a field-dependent default has
nowhere else to live.**

`node_data_creator` is `|| Box::new(IsosurfaceNodeData::default())` — it takes
**no arguments and has no field**. A kind-aware default therefore cannot be
computed at node creation; the field is not known until something is wired. The
alternatives are the editor silently rewriting node data when a wire lands
(magic, and it fights undo) or a "suggest" button the user has to find *after* a
bad first render. Auto resolves at `eval`, where the field exists, which is the
only honest place for it.

The rule, in full:

```
if field.value_range().min >= 0:                 # non-negative: density-like
    candidate = DENSITY_LEVEL                    # 0.002, the vdW convention
    f = distribution.fraction_for_iso(candidate)
    if DENSITY_PLAUSIBLE_MIN <= f <= DENSITY_PLAUSIBLE_MAX:
        level = candidate                        # basis: "non-negative, density-like"
    else:
        level = iso_for_fraction(LOCALIZED_FRACTION)   # basis: "non-negative, atypical"
else:                                            # signed: no absolute convention
    level = iso_for_fraction(LOCALIZED_FRACTION)       # basis: "signed field"
```

with `DENSITY_LEVEL = 0.002`, `LOCALIZED_FRACTION = 0.72`, and the plausibility
window `[0.85, 0.999]`.

**Signedness is the whole discriminator, and that is deliberate.** It is
already read from `value_range` to decide component count, so this costs no new
machinery — no integral, no sniff, no heuristic to defend. It works because the
property that actually drives the difference is the **nuclear cusp**, and
cusped total densities are precisely the non-negative case while the cusp-free
fields (orbitals, spin densities, deformation densities) are precisely the
signed ones.

**Note the two branches choose different *coordinates*, not just different
values.** Non-negative gets an absolute constant, because §What the fraction is
and is not better at measures that constant as the better-behaved choice for
densities. Signed gets a fraction, because for those fields no absolute
convention exists. Both resolve to an absolute magnitude at `eval`, so this
costs nothing structurally.

#### The plausibility window

`0.002` is not trusted blindly. A non-negative field that is *not* a total
electron density — ELF (conventionally drawn at ~0.8 of a 0–1 range), a reduced
density gradient (~0.5) — would take `0.002` and enclose essentially everything,
drawing the box.

Checking the candidate **against the distribution already being built** catches
that without any semantic tag: real densities land at 99.0–99.75% (88% for bare
hydrogen, hence the headroom), while an ELF or RDG field lands at essentially
100%. Outside the window, auto falls back to the fraction and says so.

This is deliberately the same shape as the cube loader's existing units
plausibility check: a derived quantity, a window wide enough that ordinary
chemistry never trips it and narrow enough that the actual failure always does,
and a **fallback rather than a hard error**.

*Evidence status:* the density figures are measured; the ELF/RDG figures are
reasoning from their definitions, not measurements — no such cubes were
available. **Confirm the window against real ELF/RDG cubes before treating the
bounds as settled** (the simulation team's offered cube zoo is the obvious
source).

#### Why field-kind sniffing is *not* part of this

Handoff §1.8 proposes guessing the kind from `∫v·dV ≈ integer N`. It is a good
idea for a different purpose, and wiring it into the level would be wasted work:

| Field | Signedness gives | The sniff would give | Different? |
|---|---|---|---|
| Total electron density | non-neg → 0.002 | `∫ρ = N` → density → 0.002 | no |
| Spin density | signed → f 0.72 | small signed integer → spin → f 0.72 | no |
| Orbital amplitude | signed → f 0.72 | not an integer → unknown → f 0.72 | no |
| Deformation density | signed → f 0.72 | ≈ 0 → unknown → f 0.72 | no |

The sniff's headline distinction — density versus spin density — separates two
kinds that **signedness already separates**, and the level does not need them
separated any further. Nor does the sniff rescue the cases signedness gets
wrong: ELF and RDG have no integer integral either, so it would return
"unknown" and hand them the same fraction, equally wrong. They need their own
conventions, which no integral test identifies.

**So the sniff stays out of scope, and its real payoff is the label** — a
readout line such as `guessed: spin density (∫ = 2.0)`, telling a user what they
are holding, as a hint and never an assertion (§1.8's own words). That belongs
with the provenance work, not here.

#### Taking over from auto

Switching **Auto → Fraction pre-fills `level_fraction` with whatever auto
resolved**, so the surface does not move at the handover. That transition is
exactly lossless — auto produces a fraction and fraction mode stores one —
unlike absolute ↔ fraction, which is lossy and is why those two keep separate
properties (§Two properties, not one). Auto → Absolute pre-fills `level` with
the resolved magnitude, equally exactly.

Auto is therefore a starting point a user takes over, not a locked mode. That
matters beyond convenience: a locked, non-overridable mode would elevate a
heuristic to a persistent assertion about the document, which is precisely what
§1.8 warns against.

#### Auto is volatile, on purpose

In auto mode the level moves when the wired field changes — that is the point,
and it is the same volatility deliberately **refused** for the colour domain
(§Why the colour is a one-shot where the level is a live mode). The difference
is that a level is a property of *this* picture, while a colour domain is what
makes two pictures comparable.

For a figure that must not change, switch to fraction or absolute to freeze it.
The reference guide should say so.

### Two properties, not one

```rust
pub struct IsosurfaceNodeData {
    /// Which of the two level properties is live. `Auto` means neither.
    pub level_mode: LevelMode,
    /// Live when `level_mode == Absolute`. Keeps the node's existing name and
    /// its `0.02` default — the amplitude convention, and as good a starting
    /// constant as any, though in practice the Auto handover pre-fills it.
    pub level: f64,
    /// Live when `level_mode == Fraction`. In `(0, 1)` exclusive.
    pub level_fraction: f64,
    // ... colours, alpha, colormap, colour domain ...
}
```

**No serde migration, and none is needed:** the `isosurface` node has never
been pushed, so no saved `.cnnd` anywhere contains one. Every field can take a
plain derive default and `impl Default` is the only default that exists.

That holds *once*. The next persisted change to this node, after it ships, will
need the usual explicit per-field `#[serde(default = "...")]` treatment — in
particular, relying on an enum's `#[default]` for a persisted field is a trap,
because moving `#[default]` when a later variant is added silently
re-interprets every stored document.

**Two stored numbers rather than one reinterpreted number**, because the
conversion is lossy in the absolute→fraction→absolute direction (§The two
queries are not a bijection). With one property, a user who flips to fraction
mode to read the percentage and flips back finds their hand-typed `0.002` has
drifted to the nearest stored sample value. With two, both survive untouched
and the toggle is free — no conversion, no API round-trip, and no dependence on
which field happens to be wired at the moment of the click.

The cost is one unused number in the `.cnnd`, which is the same trade
`color_min` / `color_max` already make when no colour field is wired.

**The honest objection to two properties is the text format**, and it is
answered below rather than dismissed: a reader looking at two level numbers has
to know which one is live. §The text format emits only one.

### The text format emits only the live property

`get_text_properties` emits `level_mode` and **only the level property that
mode makes live** — `level` under `Absolute`, `level_fraction` under
`Fraction`, and **neither under `Auto`**, where the level is not stored at all.
Never both.

This is the one place where the two-property design could leak confusion, and
suppressing the dead number closes it: a text round-trip never shows a stale
value, and a hand-written network cannot be edited into a state where two
plausible numbers disagree about which one is drawing the surface.

`set_text_properties` reads `level_mode` **first**, then the level properties,
and applies one inference rule:

**Naming a level property implies its mode, unless `level_mode` is given
explicitly.** `level:` implies `Absolute`; `level_fraction:` implies
`Fraction`.

Without that rule, `isosurface { level: 0.002 }` would store `0.002` into the
non-live absolute slot while a defaulted `Auto` mode ignored it and chose its
own level — silently discarding the only number the author wrote. The rule also keeps a
hand-written network readable: naming a level and getting that level is the
only behaviour anyone would predict.

Naming **both** without an explicit `level_mode` is ambiguous and is an error
naming the two properties, not a silent precedence rule.

`Auto` is reachable only by naming `level_mode: auto` explicitly, which is
consistent: it is the one mode with no number of its own to name. So the
shortest isosurface anyone can write — `isosurface { }` — is auto, and the
shortest one that pins a level says which coordinate it is pinning.

### The `level` input pin: one pin, mode-dependent

The existing `level: Float` pin (parameter index 2) keeps its position and
feeds **whichever property is live** — the absolute magnitude in `Absolute`
mode, the fraction in `Fraction` mode.

*Rejected: appending a second `level_fraction` pin.* It would mirror the two
properties, but one of the two pins would always be inert, and
`design_scalar_fields.md` §Node: `import_cube` already argued against shipping
an inert pin (the deferred `index`). One pin meaning "the level, however this
node expresses it" is the honest shape.

**The hazard, stated so it is not discovered:** flipping the mode on a node
with a wired `level` pin silently changes what the wired value means, and
`0.02` is a *valid* fraction, so it produces a deep, dim surface rather than an
error. The mitigations are visibility, not prevention: the subtitle renders
`level: 72.0% (fraction)` versus `level: 0.0200`, and the surface pin's readout
carries both numbers (§The dual readout). A user who deliberately flips the
mode can see what happened.

**In `Auto` mode the pin is ignored**, because auto by definition takes its
level from the field rather than from an input. A wired pin under `Auto` raises
a **non-blocking validation warning** — "the level pin is ignored in auto mode;
switch to absolute or fraction to use it" — via `ValidationError::warning()`.
Ignoring a wire silently is the failure this avoids: a user who wired something
deliberately gets told why nothing happened, and the fix is one dropdown.

Warning rather than error, by the blocking litmus in
`doc/design_error_management.md`: the node still produces a perfectly good
surface, so nothing downstream should be prevented from evaluating.

### Validation

In `eval`, after the pin is resolved and before the value is built:

| Mode | Rule | Message |
|---|---|---|
| `Absolute` | `level > 0` and not NaN | unchanged from today |
| `Fraction` | `0 < f < 1` and not NaN | `isosurface: fraction must be between 0 and 1 exclusive (got {f})` |
| `Fraction` | field has a distribution | §Analytic fields |
| `Auto` | field has a distribution | §Analytic fields — but only on the branch that needs one |
| `Auto` | `level` pin wired | non-blocking **warning**, pin ignored (§The `level` input pin) |

NaN is spelled out rather than left to `!(f > 0.0)`, matching the existing
level check and for the same reason.

**`Auto` needs a distribution only on the fraction branch.** The non-negative
branch's plausibility check consults the distribution too, so in practice both
branches want one — but if a future analytic field reports `value_range` and no
distribution, auto should fall back to the bare `DENSITY_LEVEL` constant for a
non-negative field rather than failing. A number chosen from a convention beats
an error when the convention is all there is.

### `IsosurfaceData` does not change

This is the load-bearing architectural claim of the design, and it is what
keeps the change small.

`eval` resolves the mode to an **absolute magnitude** before constructing the
value:

```rust
let (level, basis) = match self.level_mode {
    LevelMode::Absolute => (level_input, LevelBasis::Absolute),
    LevelMode::Fraction => (
        field
            .value_distribution()
            .ok_or_else(|| /* §Analytic fields */)?
            .iso_for_fraction(level_input)?,
        LevelBasis::Fraction(level_input),
    ),
    // §`LevelMode::Auto`. Returns the resolved magnitude *and* why it chose
    // it, so the readout can show the guess as a guess.
    LevelMode::Auto => auto_level(field.as_ref())?,
};
```

`auto_level` returning a `(f64, LevelBasis)` pair rather than a bare number is
what lets the readout say *"non-negative, density-like"* versus *"signed
field"* versus *"non-negative, atypical — fell back to fraction"*. A caller
that discards the basis turns a visible guess into an invisible one.

So `IsosurfaceData`, `NetworkResult::Isosurface`, `atomcad-display`'s
extractor, the lattice policy, the tessellator and every renderer path are
**untouched**. The mode is node data and nothing else.

It also does not violate the design's own rule that semantic parameters live in
the value and quality parameters live in preferences. The distribution is
computed over the field's **stored samples**, not over the extraction lattice,
so the resolved level is independent of extraction resolution. Turning the
quality multiplier up does not move the surface.

### The dual readout

Handoff §3.6 requires both numbers to be visible whenever the instrument is
active. Two homes, and one of them is free:

**The `surface` output pin's hover readout.** `NetworkResult::Isosurface`
already carries `field: Arc<dyn ScalarField>` *and* the resolved `level`, so
`to_display_string` / `to_detailed_string` can call `fraction_for_iso` and
print the pair with **no new plumbing at all**:

```
Isosurface
  level:  2.3270e-2  ·  encloses 72.0% of ∫|v|
  field:  96 x 96 x 96
```

In `Absolute` mode this is the payoff the folk convention never had: type
`0.002`, and the readout tells you the fraction it encloses.

In `Auto` mode the line also carries the **basis**, so the guess reads as a
guess rather than as a fact:

```
  level:  2.0000e-3  ·  encloses 99.2% of ∫|v|  ·  auto: non-negative, density-like
```

The three bases are `non-negative, density-like` (the `0.002` branch),
`signed field` (the fraction branch), and `non-negative, atypical` (the
plausibility window rejected the constant and auto fell back to the fraction) —
the last of which is the one a user most needs to see, because it means the
field is something the rule does not recognise.

**The editor**, live while the slider moves (§The editor).

The node **subtitle** shows only what it has without the field — the mode and
the stored number — since `get_subtitle` receives no evaluation context.

## Analytic fields

`value_distribution()` returns `None` for any field with no stored samples.
Because `Auto` is the *default* mode and a signed field sends it down the
fraction branch, a future Molden orbital wired into a freshly created
`isosurface` node would hit this immediately.

A **non-negative** analytic field is the one case that survives: auto falls
back to the bare `DENSITY_LEVEL` constant, skipping the plausibility check it
cannot run (§Validation). A number chosen from a convention beats an error
when the convention is all there is.

**Decision: a clear evaluation error, not a fallback.**

```
isosurface: fraction mode needs a field with stored samples, and this field is
analytic (no native grid). Switch the level mode to absolute.
```

*Rejected: synthesizing a distribution by sampling `suggested_bounds` at some
spacing.* It would make the resolved level depend on that spacing, which is
exactly the quality-parameter-in-the-value problem the sibling design exists to
avoid, and it would be speculative machinery for a field type that does not
exist yet.

**The right answer when Molden lands** is for `AnalyticField` to own its
distribution: the field itself declares a canonical sampling box and spacing at
construction and builds the distribution from that, so `value_distribution`
returns `Some` and every consumer stays resolution-free from the outside. That
is a decision for the Molden design, recorded here so it is not re-derived.

## The colour domain

The `color_field` pin paints per-vertex colour through
`IsosurfaceColoring::Field { range, colormap }`, where `range` is today two
hand-typed numbers defaulting to `±0.05`. It has the same "which number?"
problem, and it deserves the same treatment — but **enclosed fraction is the
wrong statistic for it**, and copying it across would be a mistake.

### Why the fraction does not transfer

Three reasons, each fatal on its own:

1. **Nothing is enclosed.** The colour field is a paint, not a threshold. There
   is no inside and outside to take a share of.
2. **The parameter is a two-ended interval**, not a single magnitude.
3. **The relevant population is different.** The colour field is sampled *only
   at the surface's vertices*. Its distribution over the whole volume is not
   the distribution of what the picture actually shows.

Point 3 is the important one, and it is the concrete case the sibling design
already documents. From `design_scalar_fields.md`, on the ESP fixture:

> the potential's `value_range()` reaches about −1.4 hartree/e, twenty times
> the ±0.08 that the surface actually spans, because the potential keeps
> climbing inside the core.

That is a factor of twenty between what the volume holds and what the surface
sees, and it is why `IsosurfaceColoring::Field::range` carries the comment
**"Never auto-fitted"**.

### The adaptation: restrict the population to the surface

The objection is entirely about the *volume* extrema. A distribution taken over
the **surface vertices** excludes the nuclear cusp by construction — the
surface never goes there — so the thing that made auto-fitting dangerous is
gone.

So the colour field gets a distribution too, but a different one:

| | surface field (`field`) | colour field (`color_field`) |
|---|---|---|
| Population | all stored samples in the box | the colour field sampled at the extracted surface's vertices |
| Weight | **mass**, `|v|` | **surface area**, each vertex weighted by a third of its incident triangles' area |
| Signed? | folded to `|v|` | **kept signed** — the sign is the content of an ESP map |
| Parameter | one magnitude | an interval `(min, max)` |
| Query | `iso_for_fraction(f)` | robust percentiles `p2` / `p98` |

**Area weighting, not vertex count**, because marching cubes puts vertices
where the geometry is busy, not where the area is. A crumpled region with many
small triangles would otherwise dominate the percentile and pull the colour
ramp toward whatever the crumples happen to sit on. Weight each vertex by a
third of the summed area of its incident triangles — the standard barycentric
lumping, one pass over the index buffer.

### Where it is computed, and why not at extraction time

The surface-restricted distribution only exists *after* extraction, which
happens in `atomcad-display` at a resolution that comes from preferences. If
the domain were auto-fitted there, the colour of a saved document would depend
on a quality setting — the exact split the sibling design is built to preserve.

**Decision: the fit is an editor action that writes concrete numbers into
`color_min` / `color_max`.** Not a mode, not a flag in the value, and
**no new node property** — the colour half of this design adds nothing to
`IsosurfaceNodeData`.

- The `.cnnd` keeps two explicit numbers. A document renders identically on
  another machine with different preferences.
- `IsosurfaceColoring::Field::range`'s "never auto-fitted" comment stays
  *literally true*: the value is never fitted. The **user** fits it, once, with
  the histogram and a button — which is the difference between an automatic
  guess and an informed choice.
- No new value-side machinery, no second out-of-bounds convention, nothing for
  the extractor to learn.

### Why the colour is a one-shot where the level is a live mode

The asymmetry is deliberate, and the deeper reason is not the resolution
argument above — it is what each parameter is *for*.

The whole point of `f` is that it is **invariant across systems**: the same
0.72 gives a comparable surface on any molecule at any resolution, which is why
it should be re-resolved against whatever field is currently wired.

A colour domain is the exact opposite. **A colour map is only comparable
between two figures when the domain is the same number.** Auto-fitting per
field would mean two ESP maps placed side by side silently use different
scales, and a reader comparing hues would be comparing quantities that do not
mean the same thing — destroying the one thing a fixed domain buys. The
conventional practice in the literature is to state the domain and hold it
across a figure set, for precisely this reason.

So: the level holds its *fraction* constant and lets the isovalue move; the
colour holds its *domain* constant and lets nothing move. Both follow from
wanting figures that can be compared.

*Rejected: a live `color_range_mode` fitted over the colour field at the stored
samples where `|v| ≈ level`* — a shell in sample space rather than vertex
space. It is resolution-free, so it *could* be a live mode, and it is neater
than it first sounds: a constant-thickness shell has volume proportional to
area, so a plain sample count approximates the area weighting for free. It
loses on the comparability argument above, and it would add a band-width
constant and a per-eval pass over the samples to buy a noisier approximation of
a population the extractor already has exactly.

**The fit must be undoable, and gets that for free by not being special.** It
writes persisted node data, so it routes through the editor's existing
`_update` → `model.setIsosurfaceData` path — the same one the two text fields
already use — rather than a bespoke API call. A one-click action that silently
replaces two hand-tuned numbers with no way back would be the worst kind of
convenience.

The button offers two fits, chosen by the colour field's signedness:

- **Signed colour field** (the ESP case): `symmetric`, domain `[-q, +q]` with
  `q = p98` of `|v|` over the surface. Symmetry is not cosmetic — `BlueWhiteRed`
  is a **diverging** ramp whose white must sit at zero, and an asymmetric domain
  slides the neutral point off zero so the sign can no longer be read off the
  picture. Handoff §2B.4 makes the same point about never letting a theme
  reverse a diverging ramp: for a signed map, the mapping of sign to hue is
  meaning, not style.
- **Non-negative colour field**: `span`, domain `[p2, p98]`.

### Interaction with the open colormap-orientation question

`design_isosurface_node.md` §Colormap orientation records an open decision:
`BlueWhiteRed` runs blue at `range.0` to red at `range.1` (matplotlib `bwr`),
which is backwards from the chemistry convention that colours electron-rich
regions red. It lists three answers — leave it and let a user negate the
potential upstream with an `expr`, reverse the constants, or add a second
variant — and declines to pick.

**This design does not settle that, but it raises the stakes.** Today a user
picking the domain by hand is already guessing, so a backwards ramp is one
surprise among several. After the fit, the domain is *right* and the picture is
confidently, conventionally-shaped, and still hue-inverted — which is a more
misleading artefact than the status quo, because everything else about it looks
correct.

Two consequences for whoever implements P4:

- The fit must not paper over the orientation by fitting an inverted domain.
  An inverted domain fails the `max > min` guard and flattens the surface to
  the ramp midpoint — behaviour that is deliberate and test-pinned, and which
  §Colormap orientation depends on.
- If the orientation question is resolved in the same release, resolve it
  **first**. Changing the ramp after the fit ships means every fitted document
  in the wild changes colour.

## The editor

### Level control

**Auto mode** — no editable number, by construction: the level comes from the
field. What the editor shows instead is the resolved pair plus the basis, and
the histogram with its marker, so the choice is inspectable even though it is
not typed. Two buttons hand control over — **Take over (fraction)** and **Take
over (absolute)** — each switching mode with the resolved value pre-filled, so
the surface does not move (§Taking over from auto).

**Fraction mode** — a log slider, primary:

```
f = 1 - 10^-(0.25 + 2.75 * s),   s in [0, 1]
```

which spans **f = 0.4377 at s = 0 to f = 0.999 at s = 1**, putting most of the
travel in 0.9–0.99 where the levels of interest live.

Note the low end does **not** reach `0.30`, which handoff §3.7 uses for its
flood animation. Their document does not reconcile this. If the animation is
ever built and should share the slider's range, drop `0.25` to `0.155`; until
then the slider's range is the one specified above and the discrepancy is
recorded rather than silently patched.

The fraction stays typeable alongside the slider — reproducing a published
figure needs an exact number, not a drag.

**Absolute mode** — the existing text field stays primary. Users type
conventional constants (`0.02`, `0.002`), and a slider over ten orders of
magnitude is worse than a box for that.

**All three modes show both numbers.** In fraction mode the readout resolves
the isovalue; in absolute mode it resolves the fraction; in auto mode it shows
both and names the basis. Same string, same order, so the eye finds it in the
same place:

```
|v| = 2.327e-2 a.u.  ·  encloses 72.0% of ∫|v|
```

### The histogram

A small plot above the level control, and the substance of this half of the
design — the slider without it is still a blind control, just a smoother one.

- **x**: `log10 |v|`, spanning the field's nonzero range. Log is not optional
  (§Motivation, handoff §1.5): a linear axis shows one spike and nothing else.
- **y**: **mass per bin** (`Σ|v|` within the bin), not sample count. Count-
  weighted, the plot is one enormous vacuum spike — the same failure mode as
  the count percentile in §Background.
- **overlay**: the cumulative mass curve, rising 0→1 left to right. This is the
  curve that makes f legible: the user sees where the field's mass actually
  lives and how steeply it is changing under their current level.
- **marker**: a vertical line at the active isolevel, with the enclosed side
  shaded.
- **zeros**: reported as a count beside the plot, never as a bin. Log space has
  no home for them, and silently dropping them would make "mostly empty box"
  and "no empty voxels" look identical.

The plot is a direct rendering of `LogHistogram`, which the distribution builds
at every field size. Once the fraction machinery exists, this is a widget and a
transport, not an algorithm — which is the argument for doing them together.

### The colour histogram

The same widget, three substitutions: the surface-restricted area-weighted
distribution as the source, a **signed** x axis (symmetric log, or linear when
the surface range is narrow — an ESP surface spanning ±0.08 does not need log),
and the domain drawn as a **span** with two handles instead of one line. The
fit button sets the span.

### Plumbing

The isosurface editor is today pure node-data editing with no kernel access at
all. `comment_editor`, `function_output_editor` and `record_def_dropdown` show
the precedent for an editor calling `sd_api.`, but this one needs a new channel.

A new `rust/src/api/structure_designer/field_distribution_api.rs`, added to
**`flutter_rust_bridge.yaml`'s `rust_input`** — a new FRB module that is not
listed there generates nothing, silently.

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn get_isosurface_level_distribution(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIValueDistribution>;

#[flutter_rust_bridge::frb(sync)]
pub fn get_isosurface_color_distribution(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APISurfaceValueDistribution>;
```

`APIValueDistribution` carries bin edges, per-bin mass, the cumulative curve,
the nonzero range, the zero count, and the resolved `(iso, fraction)` pair for
the node's current setting — so the editor never recomputes either query
locally and the printed number always comes from the same code path as the
extraction.

**Three empty states, all normal, none an error:** no field wired; an analytic
field with no distribution; an upstream that has not evaluated yet. The editor
shows the control without a plot in all three and says which.

**Reaching the evaluated field.** The evaluator keeps no per-node
`NetworkResult` after a pass — only display strings — so the getter evaluates
the node's `field` argument on demand through a small
`evaluate_node_argument(scope_path, node_id, pin_index)` helper. With eval
memoization on this is a memo hit returning an `Arc` clone, and for a
non-memoized `import_cube` it is a clone of an already-loaded payload. Cheap
either way.

*Rejected: recording the field handles during the pass*, keyed by `NodeRef`
like `node_output_strings`. It looks cheaper, but the evaluator's AGENTS notes
that **a memo hit skips the `node_output_strings` write**, so a memoized
isosurface node would leave no entry and the editor would show a stale or empty
plot depending on cache state — a bug that reproduces only under specific cache
conditions. On-demand evaluation is always correct.

**Invalidation.** The distribution is field-derived, so rewiring `field`,
reloading the `.cube`, or editing the upstream must refetch. This is the piece
most likely to be missed, and its failure mode — a correct-looking histogram
belonging to the previous field — is silent. The editor refetches on any
network-change notification, not only on selection change.

## Implementation plan

Each step ends green: `cargo test -j 4`, `cargo clippy`, `flutter analyze`.

### P1 — `ValueDistribution` (backend only)

**Work:** `field/distribution.rs` — `ValueDistribution`, `LogHistogram`,
`ExactCumulative`, the exponent parameter, `EXACT_SAMPLE_LIMIT`; the
`value_distribution` trait method and `SampledField`'s `OnceLock` override.

**Tests**

| Test | Asserts |
|---|---|
| `f -> iso -> f` on the ramp fixture, f ∈ {0.3, 0.5, 0.72, 0.9} | within ±1/N (handoff §3.8 test 1) |
| Direct integration on a synthetic Gaussian | mass of samples above `iso_for_fraction(f)` over total = f, within 0.5% (§3.8 test 2) |
| Analytic two-Gaussian fixture | the fraction is invariant to grid spacing: same f gives the same iso within a bin at 0.2 Å and 0.1 Å |
| Forced-histogram path vs exact path on one fixture | agree to within one bin |
| All-zero field | `value_distribution` is `Some`, `total == 0`, queries return `None` rather than dividing by zero |
| Field with zeros and nonzeros | `zero_count` right; zeros excluded from bins; total unaffected |
| Sheared grid | identical fraction to the same samples on an unsheared grid — the determinant cancels |
| Exponent 2 on a signed fixture | accumulates `v^2`; the hook works |
| `SampledField` clone | the clone rebuilds its distribution; no deep copy of the cached structure |

**Deliverable:** the machinery, with the round-trip and the integration check
green. Nothing user-visible.

### P2 — `LevelMode` and the dual readout

**Work:** `LevelMode` (three variants) and the two level properties,
text-format spellings and the mode-inference rule, `auto_level` and
`LevelBasis`, `eval` resolution and validation, the subtitle, and the
`NetworkResult::Isosurface` readout arms.

**Tests**

| Test | Asserts |
|---|---|
| New node's `Default` | `Auto` |
| **Auto on a non-negative fixture** | resolves to `0.002` and reports basis `density-like` |
| **Auto on a signed fixture** | resolves to `iso_for_fraction(0.72)`, basis `signed field` |
| **Auto on a non-negative field where `0.002` encloses ~100%** | falls back to the fraction, basis `atypical` — the plausibility window |
| Auto → Fraction handover | `level_fraction` pre-filled with the resolved f; the extracted surface is unchanged |
| Auto → Absolute handover | `level` pre-filled with the resolved magnitude; surface unchanged |
| Wired `level` pin under `Auto` | non-blocking warning, pin ignored, surface still produced |
| `.cnnd` round-trip, both modes | mode and both numbers survive |
| Text format round-trip | `absolute` / `fraction` spellings; **only the live level property is emitted** |
| Text `level: 0.002` alone | infers `Absolute` — the number the author wrote is the one that draws |
| Text `level_fraction: 0.9` alone | infers `Fraction` |
| Text naming both, no `level_mode` | error naming both properties, not a silent precedence rule |
| Text naming both **with** `level_mode` | explicit mode wins; both values stored |
| `isosurface { }` | round-trips as auto, emitting neither level property |
| `eval` in fraction mode | resolved `IsosurfaceData::level` equals `iso_for_fraction(f)` |
| Fraction out of range (0, 1, 1.5, NaN) | descriptive error naming the rule |
| Absolute mode | byte-identical `IsosurfaceData` to before this change |
| Wired `level` pin, both modes | drives the live property |
| Analytic field in fraction mode | the "needs stored samples" error, not a panic |
| Detail readout | carries both numbers; absolute mode resolves the fraction |
| Quality multiplier changed | the resolved level does not move |

**Deliverable:** fraction mode works end to end from the text format and the
CLI, and the pin readout prints both numbers. **First user-visible milestone.**

### P3 — Editor: slider, readout, histogram

**Work:** `field_distribution_api.rs` and its `rust_input` entry;
`evaluate_node_argument`; `APIValueDistribution`; the mode toggle, log slider,
dual readout and histogram widget in `isosurface_editor.dart`; refetch on
network change.

**Tests:** Rust-side coverage of the API types and the three empty states;
`flutter analyze` clean. Widget behaviour is covered by the walkthrough, per
`feedback_manual_test_for_editor_ui`.

**Manual walkthrough**

1. `import_cube` → `sample_data/cube/water_density.cube` → `isosurface`.
   **Expect** **auto** mode, a level at `0.002`, the readout naming the basis
   `non-negative, density-like`, the conventional vdW envelope, and a
   histogram whose marker sits where the cumulative curve is around 0.99.
   A tight cluster of spheres at the nuclei instead means auto took the
   fraction branch on a non-negative field — check `value_range().min`.
2. Press **Take over (fraction)**. **Expect** the mode to become fraction with
   the value pre-filled near 0.99, and **the surface not to move at all** — the
   handover is exact.
3. Drag the slider toward 0.999. **Expect** the envelope to grow, the readout's
   isovalue to fall, and the marker to track the drag.
4. Switch to absolute and type `0.002`. **Expect** the conventional vdW
   envelope back, and the readout to name the fraction it encloses.
5. Switch back to fraction. **Expect** the value from step 3, exactly as left —
   *not* a converted approximation of `0.002`. This is what the second stored
   property buys.
6. Switch to auto, then rewire `field` to a **signed** field (`ramp_3x4x5.cube`
   has negatives, or the P5 orbital fixture). **Expect** the basis to change to
   `signed field` and the level to land mid-histogram, not at `0.002`.
7. Wire anything into the `level` pin while still in auto. **Expect** an amber
   non-blocking warning saying the pin is ignored, and the surface unchanged —
   not a red error, and not a silently dropped wire.
8. Delete the wire into `field`. **Expect** the control to remain usable and the
   plot to say there is no field — no error, no stale histogram.

**Deliverable:** the level can be chosen with a picture of the data in front of
you, and a freshly wired node already shows something sensible.

### P4 — Colour domain: surface distribution and fit

**Work:** area-weighted surface-restricted distribution in
`atomcad-display`'s extractor output; `get_isosurface_color_distribution`;
the signed histogram with a span, and the symmetric / span fit buttons.

**Tests**

| Test | Asserts |
|---|---|
| Surface distribution on the density+ESP fixture pair | the p2/p98 span is near ±0.08, **not** the volume's −1.4 — the whole point |
| Area weighting | a fixture with deliberately uneven triangle density gives a different, correct percentile than vertex-count weighting |
| Symmetric fit on a signed colour field | domain is exactly `[-q, +q]`; zero maps to the ramp's midpoint |
| Span fit on a non-negative colour field | `[p2, p98]` |
| Fit writes node data | `color_min` / `color_max` become concrete numbers; the `.cnnd` holds no mode flag |
| Fit is undoable | one undo restores the previous domain — it goes through `setIsosurfaceData`, not a bespoke path |
| Quality multiplier changed after a fit | the saved domain does not move |

**Manual walkthrough:** load `water_density.cube` as `field` and
`water_esp.cube` as `color_field`; press the symmetric fit; **expect** the
classic ESP map with structure visible across the envelope rather than a flat
wash, and the domain fields populated with real numbers.

**Deliverable:** both level and colour are chosen with a picture of the data
rather than by guessing.

## Documentation touchpoints

Per `AGENTS.md`, in the same change as the code:

- `doc/reference_guide/nodes/atomic.md` — the `isosurface` entry: the three
  level modes, what the fraction means, what auto decides and on what basis,
  the note that auto is volatile and a figure should be frozen in fraction or
  absolute mode, and the readout (P2); then the colour fit (P4)
- `doc/design_isosurface_node.md` — a pointer to this document from §The
  `Isosurface` value, whose "never auto-fitted" comment this design deliberately
  preserves rather than relaxes
- `crates/atomcad-crystolecule/src/AGENTS.md` — `field/distribution.rs` in the
  module map, and the invariant that the distribution is over **stored samples**
  so a resolved level never depends on extraction resolution (P1)
- `doc/testing.md` — the round-trip and direct-integration checks as the pattern
  for distribution fixtures (P1)
