# Design: the `proxy` node — cutting a simulation proxy around focus atoms

## 1. Motivation

Simulating a mechanosynthesis step (a tool approach, an abstraction, a
deposition) on a big silicon structure is not affordable at the level of
theory the reaction needs. The standard remedy is a **proxy**: a cluster cut
out of the big structure around the reaction site, capped where the cut
severed bonds, with its outer shell frozen at lattice positions so the
interior still feels bulk. The proxy is what gets relaxed, whether by the
built-in `relax` node or by an external code (a machine-learned potential such
as UMA for the whole cluster, optionally an ONIOM correction on the core).

Today a proxy is assembled by hand: a `sphere` into `atom_cut`, then
`passivate`, then `freeze` with a second, larger region. That has four
defects the `proxy` node removes:

- **A spatial cut keeps atoms that are not bonded to the site** — a second
  tip, the far wall of a trench, a neighbouring feature. They sit in the
  proxy as disconnected fragments.
- **`passivate` caps every dangling bond**, including the ones that are the
  point of the simulation: the radical apex of a tool, the three-coordinated
  carbon of a T-centre, an unsaturated surface site. The only workaround is
  `remove_hydrogen` in a hand-placed region afterwards.
- **There is no discrete size series.** Convergence testing ("grow the proxy
  by one shell and check the energy stops moving") needs a family of proxies
  that differ by one well-defined increment. Sphere radii are not that.
- **Cap pairs on the cut boundary clash.** In the diamond lattice a removed
  atom very often had *two* neighbours that survive the cut, so two caps are
  placed pointing at the same vacated site. On silicon the two hydrogens land
  1.42 Å apart; on carbon 0.74 Å, which is an H₂ molecule. This is the ideal
  symmetric dihydride of a {100} face, the reason a real Si(100) surface
  reconstructs, and it appears on every spherical cut and on a third to a half
  of every hop shell (§4.3). A sphere plus `passivate` produces it silently.

## 2. Idea

Mark the atoms at the reaction site with a tag (default name `focus`). The
node computes the **bond-graph distance** — hops over covalent bonds, counted
over heavy atoms only — from the nearest focus atom to every other atom, keeps
what lies within `hops`, fills in the boundary atoms that would otherwise
leave clashing cap pairs, freezes what lies beyond `free`, caps the severed
bonds, and outputs the result as an ordinary atomic structure.

Bond hops rather than a sphere because:

- **Only reachable atoms are kept.** The cut follows connectivity, so a proxy
  never contains a fragment the site is not bonded to.
- **Hop shells are the ONIOM layers.** Core, relaxed buffer and frozen rim are
  all naturally "N bonds from the site".
- **The size series is discrete.** `hops` = 4, 5, 6, 7 is the convergence
  series, one node each (or one node inside a `map` over a `range`).

In a uniform lattice hop count tracks distance closely, so nothing is lost
versus a sphere; in a mixed system (a carbon tip on silicon) hops are the
chemically meaningful measure anyway.

## 3. Node definition

`proxy` — category *AtomicStructure*, summary "Cut a simulation proxy around
focus atoms". Same family as `tag` / `freeze` / `atom_cut`: `HasAtoms` in,
same type out, every tunable is both a node-data property and an input pin,
and a connected pin overrides the stored value.

### 3.1 Input pins

| # | Pin | Type | Required | Meaning |
|---|---|---|---|---|
| 0 | `molecule` | HasAtoms | yes | Structure to cut, Crystal or Molecule. |
| 1 | `focus` | String | no | Tag name marking the source atoms. Every atom carrying it is at distance 0. |
| 2 | `hops` | Int | no | Keep heavy atoms whose distance is at most this. |
| 3 | `free` | Int | no | Heavy atoms farther than this get the frozen flag. On the plain cut `free >= hops` freezes nothing; atoms `fill` restores beyond `free` are frozen regardless (§4.6). |
| 4 | `rm_single` | Bool | no | Drop heavy atoms the cut left with a single heavy neighbour, repeated until none is left (§4.4). Focus atoms are never dropped. |
| 5 | `passivate` | Bool | no | Cap every severed bond with a terminator along the old bond vector. |
| 6 | `passiv_elem` | Int | no | Terminator element (atomic number), H/F/Cl/Br/I. |
| 7 | `core` | Int | no | Tag heavy atoms with distance at most this as `high` (the ONIOM high layer). Negative disables. |
| 8 | `fill` | Bool | no | Keep every dropped heavy atom that bridges two or more kept heavy atoms, repeated until none is left (§4.3). |

Pins 4–6 deliberately reuse the names and types of the same flags on
`materialize`, so `proxy { passivate: true, rm_single: true }` reads the same
way `materialize { passivate: true, rm_single: true }` does. `fill` is the
counterpart of `rm_single`: one trims singly attached atoms off the boundary,
the other fills in the atoms the boundary left bridging two kept ones. Any
future pin must be **appended** (pin indices are persisted in wires).

### 3.2 Output pin

One pin, `OutputPinDefinition::single_same_as("molecule")`. A Crystal input
stays a Crystal so its lattice survives for downstream nodes; a Molecule stays
a Molecule. The node removes and adds atoms, so it goes through the
atom-mutating `map_atomic` path that `atom_cut` uses, not the metadata-only
`map_atomic_in_region`. There is no `diff` pin (§5), so `snapshot_atoms` and
`eval_output_with_diff` are not called; §7.6 says how one would be appended.

### 3.3 Node data

```rust
pub struct ProxyData {
    pub focus: String,       // default "focus"
    pub hops: i32,           // default 6
    pub free: i32,           // default 3
    pub rm_single: bool,     // default false
    pub passivate: bool,     // default true
    pub passiv_elem: i16,    // default 1 (H)
    pub core: i32,           // default -1 (no `high` tagging)
    pub fill: bool,          // default true
    #[serde(skip)] pub available_tags: RefCell<Vec<String>>, // tag dropdown, as TagData
}

/// Root-eval report, stored in `context.selected_node_eval_cache` exactly as
/// `RelaxEvalCache` is — never on the node data (§7.6).
pub struct ProxyEvalCache {
    pub stats: ProxyStats,
}

// Defined in `atomcad_crystolecule::proxy_cut` (§7.2); the node re-exports it.
pub struct ProxyStats {
    pub formula: String,     // empirical formula of the output, e.g. "Si223H96"
    pub heavy: usize,        // heavy atoms kept (after fill and rm_single)
    pub riders: usize,       // riders kept
    pub caps: usize,         // terminators added
    pub free: usize,         // atoms without the frozen flag
    pub frozen: usize,       // atoms with the frozen flag
    pub filled: usize,       // heavy atoms added by fill
    pub fill_rounds: usize,  // synchronous fill rounds until nothing changed
    pub farthest_hop: u32,   // largest distance among kept heavy atoms
    pub min_rim: i32,        // hops - free, the thinnest frozen shell
    pub open_valences: usize,// unsaturated slots left on kept atoms
    pub min_cap_pair: Option<f64>,    // closest cap-cap distance, Å;
                                      // None when no two caps lie within 3 Å
    pub nearest_dropped: Option<f64>, // closest dropped heavy atom to any free atom
                                      // (heavy or rider), Å; None when nothing
                                      // dropped lies within 8 Å
}
```

All eight persisted fields are text properties. The serializer writes every
text property whatever its value (compare the `materialize` lines in the
text-format snapshots), so a node at its defaults with `core: 1` serializes as

```
proxy_6 = proxy { molecule: surface, focus: "focus", hops: 6, free: 3, rm_single: false, passivate: true, passiv_elem: 1, core: 1, fill: true, visible: true }
```

When *parsing*, a property left out takes its default, so the short form
`proxy { molecule: surface, core: 1 }` is accepted and round-trips to the
full line above. Node subtitle when pins 1–3 are unconnected: `focus · 6 / 3`.

`ProxyStats` feeds the report in the properties panel, the way `relax` shows
its message. It is the user's only feedback on a cut and is what the tuning
loop of §6.1 reads, so it carries more than a count:

- `formula`, `heavy`, `riders`, `caps` — what the output is and what it costs.
- `free`, `frozen` — the relaxation's degrees of freedom.
- `filled`, `fill_rounds`, `farthest_hop` — how much fill grew the cluster.
  `farthest_hop` is a **size** figure, not a shielding figure (§4.3).
- `min_rim` — `hops - free`, the guaranteed thickness of the frozen rim in
  its thinnest direction; the shielding figure.
- `open_valences` — the unsaturated slots the output still carries, computed
  from hybridization the way `passivate` counts them. This is what sets the
  multiplicity of the quantum-chemistry input, so it has to be visible.
- `min_cap_pair` — the closest two terminators come to each other, looked
  for within 3 Å. Below about 2 Å the rim is unphysical; with `fill` on this
  is 2.42 Å for silicon; absent means no two caps come within 3 Å at all.
- `nearest_dropped` — the closest dropped heavy atom to any *free* atom. A
  small value (under about 4 Å) means an unbonded neighbour — a trench wall,
  a second tip — is close enough to matter sterically and was cut away; the
  remedy is to tag one of its atoms `focus` (§4.2).

`available_tags` is snapshotted from the input on every eval so the panel can
offer a dropdown, exactly as `tag` does. It is the only mutable field on the
node data, and it is never *read* from `eval` (subnetwork node state is shared
across call sites); the stats go to the eval cache instead, see §7.6.

## 4. Semantics

The eval runs these steps in order on a clone of the input structure.

### 4.1 Riders

An atom with **exactly one bond** in the input is a *rider*: a hydrogen, a
halogen passivant, any monovalent terminator, and also a singly bonded
adatom of any element. Every other atom is **heavy**. Throughout this
document "heavy" means exactly "not a rider", never "not hydrogen": every
count of heavy neighbours, every hop, every `heavy` figure in the stats uses
this graph definition. Riders

- never count as a hop and are never traversed;
- are kept exactly when their one heavy neighbour (their *host*) is kept;
- inherit the frozen decision of that neighbour;
- if tagged as focus, promote their host to a source instead. A rider whose
  host is itself a rider is an isolated diatomic; it cannot anchor a cut, so
  as a source it is discarded, and it is dropped like any other rider whose
  host is not kept.

This keeps the real surface termination and the tool's own hydrogens intact.
Without the rule a surface hydrogen on a last-shell silicon would land one
hop too far, be dropped, and then be re-invented by a cap. A cap added by an
earlier `proxy` is itself a rider, so a proxy cut from a proxy behaves
correctly with no special casing.

### 4.2 Distance

Multi-source breadth-first search over heavy atoms, every focus atom at
distance 0. Multi-source is what makes the motivating case work: before the
reaction a tool apex is **not bonded** to the surface, so a search from the
apex alone never reaches a surface atom. The user tags the apex and the
target surface atom(s); both grow shells that merge into one proxy. It also
covers a two-tool step and a site spanning a dimer row.

Heavy atoms with distance `> hops`, and heavy atoms unreachable from any
focus, are dropped together with their riders and all bonds to them. The
distance is kept per atom for the later steps; atoms added by `fill` keep
their true BFS distance, which is larger than `hops`.

### 4.3 `fill`

On by default. Repeat until nothing changes: every dropped heavy atom with
**two or more kept heavy neighbours** is kept (with its riders). Then the
caps of §4.5 are placed on the converged boundary.

The rule exists because of how the diamond lattice cuts. A dropped atom `B`
that had two kept neighbours `A1`, `A2` would receive two caps, one from each,
both pointing at `B`'s old position. With the host–H length subtracted from
the host–host length, each cap sits 0.87 Å (Si) or 0.45 Å (C) short of `B`,
at the tetrahedral angle, so the pair lands at

| host | host–host | host–H | H–H of the pair |
|---|---|---|---|
| Si | 2.35 Å | 1.48 Å | 1.42 Å |
| C | 1.54 Å | 1.09 Å | 0.74 Å |

Such shared sites are not rare: every six-ring that crosses the boundary
makes one, and on a {100}-like patch of the boundary every outside atom is
one. Keeping `B` instead turns it into a boundary SiH₂ whose two caps point
away from each other at 2.42 Å. That can create a new shared site one layer
out, which is why the rule iterates. It **converges**, because a boundary
where every outside atom touches only one kept atom is a {111}-like facet,
and the closure of a finite set under this rule is the {111}-faceted hull of
the plain cut. Measured with a throwaway script over the ideal silicon
lattice graph (hydrogen caps on every severed bond; the crate test of §8 Phase 1
reproduces the bulk rows):

| case | `hops` | kept before | shared sites | H–H before | rounds | kept after | H–H after | farthest hop |
|---|---|---|---|---|---|---|---|---|
| bulk | 4 | 83 | 40 | 1.42 | 4 | 165 | 2.42 | 8 |
| bulk | 6 | 239 | 84 | 1.42 | 6 | 455 | 2.42 | 12 |
| Si(100)-2×1, focus on a dimer atom | 4 | 57 | 14 | 1.42 | 4 | 82 | 2.42 | 8 |
| Si(100)-2×1, focus on a dimer atom | 6 | 158 | 27 | 1.42 | 6 | 223 | 2.42 | 12 |
| Si(100)-2×1, two focus atoms 1.5 cells apart | 6 | 251 | 45 | 1.42 | 6 | 365 | 2.42 | 12 |

Three consequences the rest of the design relies on:

- **Cost.** Fill converges in about `hops` rounds and multiplies the atom
  count by 1.4–1.9. Every atom it adds lies beyond `hops`, so with
  `free < hops` all of them are frozen rim: no degrees of freedom are added
  to the relaxation, only evaluation cost. The rim it produces is an
  H-terminated {111}-like surface with every cap on its ideal bond vector,
  which is the one hydrogen termination of silicon that is stable without
  reconstruction.
- **Anisotropy.** Fill grows only where the boundary is {100}-like and adds
  nothing where it is already {111}-like. So the farthest kept atom can sit
  at about `2·hops`, while the thinnest part of the frozen rim is still
  exactly `hops - free`. `farthest_hop` in the stats is a size figure;
  `min_rim` is the shielding figure. Neither `free` nor `hops` should be
  lowered because the farthest hop looks large.
- **Monotonicity.** Fill is a closure operator, so nested plain cuts give
  nested filled cuts: the `hops` series stays a convergence series.

Off, the plain cut is emitted with its shared sites, and `min_cap_pair` in
the stats reports the 1.42 Å. The switch exists for reproducing hand-built
clusters from the literature and for structures where the user has chosen
the boundary so that no shared site occurs; it is not a size optimisation.

### 4.4 `rm_single`

Off by default. When on, it runs on the **converged boundary** (after
`fill`) and repeats until nothing changes: a kept heavy atom whose kept heavy
neighbours number exactly one, **and** which had more than one heavy
neighbour in the input, is dropped with its riders. Both counts use the §4.1
definition of heavy, so an adatom rider hanging off an atom is neither a
lost neighbour nor a remaining one. Atoms singly bonded in the input (an
adatom, a terminal group) are therefore never touched on their own account.
**Focus atoms are exempt**: a source is never dropped, whatever the count
says.

It has to iterate. Before the first removal, singly attached atoms can only
sit in the outermost shell (an atom at distance `k < hops` has all its
neighbours at distance `<= k+1 <= hops`), but removing one lowers the kept
count of its inward neighbour, and an atom at `hops - 1` whose only inward
bond is its parent and whose outward neighbours the first round all dropped
is left with one neighbour itself. ("Single-attached" here always means left
so **by the cut** — an atom singly bonded in the *input* is a rider or an
exempt terminal atom, and neither is ever dropped on its own account.) In a single-source diamond cut more than
half of the outermost shell has a single parent, so this is the common
case. A one-shell trim would leave exactly the atoms the flag promises to
remove; the fixpoint is what `materialize`'s `rm_single` does too
(`remove_single_bond_atoms_filtered` is recursive). Termination is
immediate: the kept set only shrinks.

The test is **exactly** one kept heavy neighbour, not "at most one". An atom
whose last two kept neighbours vanish in the same round is therefore left
with none and survives, floating and capped on every side. `materialize`'s
`rm_single` tests `bonds.len() <= 1` for exactly that reason. Reaching the
case here needs a boundary atom whose only two kept neighbours both become
singly attached in the same round; no fixture has produced one, so the literal
reading stands — widen the test to "at most one" if a real structure ever
strands an atom.

Two consequences to know about. Fill-restored atoms start with two anchors
but are not immune: the cascade can remove an anchor and then the restored
atom. And a chain of atoms inside the cut — an alkyl linker on a tool, a
bare wire — unwinds all the way back to the first atom that keeps two
neighbours, or to a focus atom, which is one more reason the flag is off by
default.

Default off so that the `hops` series stays monotonic: every atom of the
`hops = 5` proxy is in the `hops = 6` proxy.

### 4.5 `passivate`

On by default. For every kept heavy atom `A` and every bond `A–B` where `B`
was dropped (by distance, not restored by `fill`, or removed by
`rm_single`), place a terminator at

```
pos(A) + unit(pos(B) − pos(A)) · L(element(A), passiv_elem)
```

with `L` the host–terminator bond length from the existing passivation
tables (`hydrogen_passivation.rs` for H; the halogen lengths added by
`doc/design_halogen_passivation.md`). Single bond to `A`; the terminator
carries the hydrogen-passivation flag (bit 1) like every other placed
terminator, and the frozen flag of `A`.

This is the **link-atom construction**: the cap sits exactly where the
removed neighbour was, on the true lattice direction. It caps *only severed
bonds*, the same way `materialize` caps only at the missing lattice sites.
Consequently an atom that was unsaturated in the input stays unsaturated —
the tool apex, the T-centre carbon, any radical surface site — without the
user having to mark it. That is the whole reason this is a proxy flag rather
than a downstream `passivate` node, which caps every dangling bond it finds.

### 4.6 `free`

Heavy atoms with distance `> free` get the frozen flag set; their riders and
their caps follow. Atoms added by `fill` have distance `> hops`, so they are
frozen whenever `free < hops`. **Flags are only ever set, never cleared**: an
atom frozen in the input stays frozen whatever its distance, and a rider or a
cap follows whichever of the two froze its host — the distance rule or the
input flag. Boundary atoms come from the lattice, so they are already at bulk
positions — this is the frozen rim that makes a finite cluster stand in for a
big surface. The `relax` node honours the flag today; an exporter for an
external code reads the same bit.

### 4.7 `core`

Off by default (`-1`). When `core >= 0`, heavy atoms with distance `<= core`
get the tag **`high`**; riders and caps inherit the tag of their heavy atom.
Zero tags only the focus atoms. It is legal here, but it is a partition the
ONIOM exporter of §9.4 refuses, because every layer-boundary bond then
touches a focus atom; one is the smallest value that survives export. The
tag is **only added, never removed** — an input that already carries `high`
keeps it, mirroring how `free` treats the frozen flag.

**Untagged means low.** No `low` tag is written: every atom would carry it,
it would spend a second name of the 32-tag budget, and the exporter (§9)
treats "no `high` tag" as low anyway. This is the one place the node spends
a tag name, and the exception to §5's "no per-shell tags".

With `hops: 6, free: 3, core: 1` one node yields the three ONIOM shells:
focus atoms and their first neighbours `high`, everything within three hops
relaxed, everything beyond frozen. A `core` larger than `free` (frozen atoms
in the high layer) is odd but legal; the exporter is where a wrong partition
is diagnosed, not here.

### 4.8 Errors

- Empty `focus`, or no atom carries the tag: localized error naming the tag.
- `hops < 0`: error. `free < 0`: treated as 0.
- `passiv_elem` outside H/F/Cl/Br/I: the same error `passivate` raises.
- Wrong input types: the standard errors from `evaluate_arg_required` /
  `evaluate_or_default`.

## 5. What is deliberately left out

- **No `region` pin.** A spatial gate would reintroduce sphere semantics.
  `atom_cut` upstream already provides it for anyone who wants both.
- **No second output for the dropped atoms.** `structure_invert` on input and
  output can produce it later if a use case appears.
- **No per-shell tags** beyond the single `high` tag of `core`. The tag
  budget is 32 names per structure; carrying a shell index as tags would burn
  it. The `free` threshold lives in this node for that reason, rather than in
  a hop-aware `freeze`.
- **No `only`/`skip` tag on the standalone `passivate` node.** A `skip: String`
  pin there (hosts carrying the tag are excluded from the eligibility
  predicate that `add_hydrogens_filtered` already takes) is a useful,
  independent improvement for tool building, but it is not needed for
  proxies once caps are severed-bond-only. Tracked separately.

## 6. Worked example

Tool approach on Si(100): the tool is placed above the surface, its apex
tagged `focus` with a `tag` node gated by a small region; the target dimer
atom is tagged the same way.

```
site     = tag { molecule: scene, name: "focus", region: apex_ball }
site2    = tag { molecule: site,  name: "focus", region: target_ball }
proxy_6  = proxy { molecule: site2, hops: 6, free: 3, core: 1 }
relaxed  = relax { molecule: proxy_6 }
```

Convergence check: duplicate `proxy_6` as `proxy_7` with `hops: 7`, relax
both, compare. The tool's apex radical is unsaturated in both; every silicon
that lost a neighbour to the cut carries a hydrogen on the old bond vector,
and no cap pair sits on a shared site, because `fill` (on by default) kept
every atom that two survivors shared; everything more than three hops from the
apex or the target is frozen; the apex, the target atom and their first
neighbours carry `high`, ready for the ONIOM exporter of §9. Expect the filled
`proxy_6` to hold roughly 1.5× the atoms of the plain six-hop shell, all of the
extra ones frozen.

**The 2.42 Å of §4.3 is an ideal-lattice figure, and this example is where that
matters.** It is two caps on one host, each at 1.48 Å, on directions subtending
the tetrahedral 109.47°. A cut through a **reconstructed** surface severs bonds
whose partners the dimerisation has already displaced, and §4.5 puts each cap on
the *real* bond vector; the two caps on such a host therefore subtend the real
angle. Measured on the Phase 5 fixture, the seven-hop cut's closest pair is
2.14 Å — two caps at exactly 1.48 Å at 92.4° — while the six-hop cut, whose
boundary stays clear of the reconstructed layer, reports the ideal 2.42 Å. Both
are clean rims. What `fill` guarantees, and what the fixture asserts, is the
*structural* property: no dropped heavy atom is left bridging two kept ones, so
the 1.42 Å shared site never occurs. Read `min_cap_pair` against the ~2 Å
unphysical line of §3.3, not against 2.42.

### 6.1 Choosing the parameters

`free` and `core` are chemistry, `hops` is cost. The stats of §3.3 are laid
out for this loop:

1. Tag the focus atoms. Set `core` from the reaction (the atoms whose bonds
   change, plus one shell) and `free` from how far its strain field reaches
   (two to three dimers on Si(100)). Confirm `free` later with a series in
   `free` at fixed `hops`; that series is monotonic too.
2. Pick `hops` so that `min_rim = hops - free` is at least two or three
   shells, and evaluate.
3. Read the report: `formula` and the atom counts for cost, `min_cap_pair`
   to confirm the rim is clean, `open_valences` for the multiplicity,
   `nearest_dropped` for a cut-away steric neighbour. Raise `hops` until the
   cost is the most you will pay, then run the `hops` series downward.

Do not read `farthest_hop` as rim thickness. Fill makes the rim thick in the
{100} directions and leaves it at `hops - free` in the {111} directions, so a
large farthest hop is not a reason to lower `hops` or raise `free`.

## 7. Architecture: the crystolecule module and the node shell

### 7.1 The layering rule

The algorithm lives in `atomcad-crystolecule`, in a new module
`proxy_cut.rs` beside `hydrogen_passivation.rs`. The node in
`atomcad-structure-designer` is an adapter over it, the way `passivate` is
an adapter over `add_hydrogens_filtered`, `relax` over `minimize_energy` and
`patch_build` over `patch.rs`.

The house reason is that the crate's `tests/` directory is where an algorithm
is tested. The stronger reason is that AI agents increasingly drive
`atomcad-crystolecule` **alone** — build a structure, cut a proxy, relax it,
compare energies across a series — with no node network anywhere. Everything
§4 describes therefore has to be callable with an `AtomicStructure` and a
plain options struct, and has to report its statistics as a plain value.

Two prohibitions follow:

- The module never sees `NetworkResult`, pins, `NodeData`, the text format,
  or a tag-name-as-parameter convention. It takes atom ids and options.
- The node never traverses a bond, places a cap, or decides a distance. It
  reads pins, validates, calls the module once, and stores the report.

The test for whether the split is right: every row of the §4.3 table must be
reproducible from `crates/atomcad-crystolecule/tests/crystolecule/` with no
dependency on `atomcad-structure-designer`.

### 7.2 Public surface of `proxy_cut`

```rust
// crates/atomcad-crystolecule/src/proxy_cut.rs

pub const HIGH_TAG: &str = "high";
/// Search radius for `ProxyStats::nearest_dropped` (Å). The stat is read
/// against a ~4 Å threshold (§3.3); anything farther is reported as `None`.
pub const NEAREST_DROPPED_RADIUS: f64 = 8.0;
/// Search radius for `ProxyStats::min_cap_pair` (Å): past the 2.42 Å of a
/// clean silicon rim and the ~2 Å that marks an unphysical one.
pub const CAP_PAIR_RADIUS: f64 = 3.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ProxyOptions {
    pub hops: u32,                 // default 6
    pub free: u32,                 // default 3
    pub fill: bool,                // default true
    pub rm_single: bool,           // default false
    pub passivate: bool,           // default true
    pub passivant_element: i16,    // default 1
    pub core: Option<u32>,         // default None = no `high` tagging
}
impl Default for ProxyOptions { /* the defaults above */ }

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("no focus atoms")]
    NoFocusAtoms,
    #[error("passivant element {0} is not one of H, F, Cl, Br, I")]
    BadPassivant(i16),
    #[error(transparent)]
    Tag(#[from] TagError),         // the `high` name did not fit the 32-slot table
}

/// One terminator to place: on `host`, along the old bond to `severed`.
#[derive(Debug, Clone, PartialEq)]
pub struct CapPlacement {
    pub host: u32,
    pub severed: u32,
    pub position: DVec3,
}

/// Everything decided, nothing mutated. Every `Vec` is sorted by atom id
/// (`caps` by `(host, severed)`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyPlan {
    pub options: ProxyOptions,         // the options the plan was made with
    pub distance: FxHashMap<u32, u32>, // every heavy atom reachable from a source
    pub kept: Vec<u32>,                // heavy atoms and riders that survive
    pub dropped: Vec<u32>,             // heavy atoms and riders that go
    pub caps: Vec<CapPlacement>,
    pub filled: Vec<u32>,              // heavy atoms `fill` restored, less any
                                       // a later `rm_single` took away
    pub fill_rounds: usize,
    pub frozen: Vec<u32>,              // heavy atoms the cut freezes (riders/caps follow)
    pub high: Vec<u32>,                // heavy atoms the cut tags (riders/caps follow)
    pub nearest_dropped: Option<f64>,  // needs the dropped positions, so it is planned
}

/// Rider id → its one heavy neighbour.
pub fn classify_riders(structure: &AtomicStructure) -> FxHashMap<u32, u32>;

/// Multi-source BFS over heavy atoms. `sources` must already be heavy
/// (`plan_proxy` step 2 promotes riders before calling this). Unbounded:
/// every heavy atom in the sources' components gets a distance, atoms in
/// other components get none.
pub fn bond_distances(
    structure: &AtomicStructure,
    sources: &[u32],
    riders: &FxHashMap<u32, u32>,
) -> FxHashMap<u32, u32>;

/// The severed-bond caps of a keep set: one cap per bond from a kept heavy
/// atom to a heavy atom that is not kept, on the old bond vector at the
/// host–terminator length. `riders` says which atoms are not heavy.
pub fn severed_bond_caps(
    structure: &AtomicStructure,
    is_kept: &dyn Fn(u32) -> bool,
    riders: &FxHashMap<u32, u32>,
    passivant_element: i16,
) -> Vec<CapPlacement>;

/// Analysis only. `sources` are atom ids (a rider promotes its host).
pub fn plan_proxy(
    structure: &AtomicStructure,
    sources: &[u32],
    options: &ProxyOptions,
) -> Result<ProxyPlan, ProxyError>;

/// Mutation only. Must be applied to the structure the plan was made from.
/// The plan carries its options, so nothing can be passed that disagrees
/// with the caps and keep set already decided.
pub fn apply_proxy(
    structure: &mut AtomicStructure,
    plan: &ProxyPlan,
) -> Result<ProxyStats, ProxyError>;

/// The whole thing by tag name: `atoms_with_tag` → `plan_proxy` → `apply_proxy`.
pub fn proxy_cut(
    structure: &mut AtomicStructure,
    focus_tag: &str,
    options: &ProxyOptions,
) -> Result<ProxyStats, ProxyError>;

pub struct ProxyStats { /* §3.3 */ }
```

Decisions behind this shape:

- **Plan / apply split.** Analysis is immutable and complete before the first
  mutation; `apply_proxy` is the only function that touches the structure.
  This is the crate's existing pattern (`add_hydrogens_filtered` and
  `remove_hydrogens_filtered` both scan, then mutate) and it buys three
  things: a `hops` or `free` series can be planned against one structure
  without cloning it per member, tests assert on the plan (pure graph work
  plus cap geometry) without inspecting a mutated structure, and the ONIOM
  exporter of §9 reuses `classify_riders`, `bond_distances` and
  `severed_bond_caps` without touching the cut (the exporter's link atoms
  sit at a scaled length rather than the terminator length, so that call
  will add a length rule to `severed_bond_caps` when it arrives; nothing is
  reserved for it now).
- **Sources are atom ids, not a tag name.** An agent script usually has the
  ids in hand; a tag is one way of producing them. `proxy_cut` is the
  tag-resolving convenience the node and most scripts call.
- **Unsigned options.** The module cannot express "-1 means off" except as
  `Option`, which is what `core` is. The `-1` / negative-to-zero rules of
  §4.8 are node-side validation, before `ProxyOptions` is built.
- **Deterministic ids.** Every list in the plan is sorted by atom id, and
  `apply_proxy` walks them in that order, so the ids new caps receive do not
  depend on hash-map iteration. Snapshot tests, `.cnnd` stability and the
  "proxy of a proxy" property all rely on this.
- **A rider is a graph property, not an element property.** "Exactly one
  bond" makes a singly bonded heavy adatom a rider too: it is never traversed,
  it follows its host, and its own open valences are not the module's
  business. An atom with **no** bond is heavy and unreachable, so it is
  dropped unless it is itself a source.

### 7.3 What `plan_proxy` does, in order

1. **Riders.** `classify_riders`: one pass over the atoms; an atom with
   `bonds.len() == 1` maps to `bonds[0].other_atom_id()`.
2. **Sources.** Each id in `sources` is replaced by its host if it is a
   rider, discarded if that host is itself a rider (§4.1) or the id names no
   atom, then deduplicated. Empty → `NoFocusAtoms`. `passivant_element`
   is checked with `is_allowed_passivant` → `BadPassivant`.
3. **Distances.** `bond_distances`: a queue BFS from all sources at once,
   skipping riders when expanding. It is not bounded by `hops`, because the
   atoms `fill` restores need their true distance (§4.2) and `farthest_hop`
   reads it. Linear in the component; on a million-atom workpiece it is
   still cheaper than deleting the dropped atoms afterwards.
4. **Plain cut.** `kept_heavy = { a | distance[a] <= hops }`.
5. **Fill** (when `options.fill`). Synchronous rounds. `pending[b]` counts,
   for every dropped heavy atom `b` adjacent to a kept heavy atom, how many
   kept heavy neighbours it has; the frontier is the set of heavy atoms kept
   in the previous round (initially all of `kept_heavy`). A round walks the
   frontier's heavy neighbours, increments their `pending`, and collects every
   `b` whose `pending` reaches 2 during the round; the collected set becomes the next
   frontier and joins `kept_heavy` at the end of the round. Stops when a round
   collects nothing. `fill_rounds` is the number of rounds that collected
   something, which is what the §4.3 table reports.
6. **`rm_single`** (when on). A worklist to a fixpoint, the shape of
   `remove_single_bond_atoms_filtered`: scan `kept_heavy` once for atoms
   that are not sources, have exactly one kept heavy neighbour and had more
   than one heavy neighbour in the input (riders excluded from both counts);
   remove that batch from `kept_heavy`; re-check only the removed atoms'
   kept heavy neighbours against the same test; repeat until a batch is
   empty. The kept set only shrinks, so it terminates. §4.4 is why the
   cascade is required rather than a single pass.
7. **Riders follow.** `kept = kept_heavy ∪ { r | riders[r] ∈ kept_heavy }`;
   everything else in the structure is `dropped`.
8. **Caps** (when `options.passivate`). `severed_bond_caps` over
   `kept_heavy`: for each bond `a–b` with `b` heavy and not kept, a
   `CapPlacement` at `pos(a) + unit(pos(b) − pos(a)) · terminator_bond_length(Z(a), passivant)`.
   Elements are read through `effective_atomic_number` so a parameter
   element resolves. A kept atom never has a dropped rider (step 7), so
   riders need no case here.
9. **Frozen.** `frozen = { a ∈ kept_heavy | distance[a] > free }`. This is the
   whole rule, the one §3.1 and §4.6 state: `hops` plays no part in it, so on
   the plain cut `free >= hops` freezes nothing, and an atom `fill` restored
   at a distance beyond `free` is frozen whatever `hops` is.
10. **High.** When `core` is `Some(c)`: `high = { a ∈ kept_heavy | distance[a] <= c }`.
11. **`nearest_dropped`.** For every kept atom that will be free after the
    cut — a heavy atom not in `frozen` and not already frozen in the input,
    or a rider of such an atom — query the input's spatial grid with
    `get_atoms_in_radius(pos, NEAREST_DROPPED_RADIUS)`, keep dropped heavy
    hits, take the minimum distance. Computed here because the dropped atoms
    are gone once `apply_proxy` has run. The grid makes it linear in the
    number of free atoms.

### 7.4 What `apply_proxy` does, in order

1. **Intern `high` first** (`intern_tag(HIGH_TAG)`) when the plan has any
   high atom — the one fallible step runs before the first mutation, so an
   error leaves the structure untouched.
2. Delete `plan.dropped` in id order (`delete_atom` clears bonds on both
   sides).
3. Place caps in plan order: `add_atom(passivant, position)`,
   `set_atom_hydrogen_passivation(id, true)`, `add_bond(host, id, BOND_SINGLE)`.
   Because the plan's cap list is sorted and the ids are handed out in that
   order, two runs on equal inputs give equal outputs.
4. Set the frozen flag on `plan.frozen`, then on every kept rider and every
   cap whose host carries it *after* that step — which is both the atoms this
   cut froze and the ones the input had already frozen (§4.6). Never cleared.
5. Add `high` to `plan.high`, their riders, and their hosts' caps. Never
   removed.
6. Compute `ProxyStats`: `heavy`, `riders`, `caps`, `filled`, `fill_rounds`,
   `farthest_hop`, `min_rim` and `nearest_dropped` from the plan; `free`,
   `frozen`, `formula` and `open_valences` by walking the result;
   `min_cap_pair` by querying the result's grid within `CAP_PAIR_RADIUS`
   around every cap and keeping cap–cap hits, linear in the number of caps.

### 7.5 Shared pieces, factored rather than copied

- **Bond length.** `hydrogen_passivation.rs` gets a public
  `terminator_bond_length(host: i16, passivant: i16) -> f64`, which is the
  existing branch at its one call site: the private `XH_BOND_LENGTHS` table
  for hydrogen, `halogen_bond_length` otherwise. `proxy_cut` calls it. This
  is a fourth *caller*, not a fourth *table*: `doc/design_halogen_passivation.md`
  keeps its three hydrogen sites, which differ by context on purpose, and
  the proxy's cap is a molecular-context cap on an arbitrary structure, so
  the general path's values (Si–H 1.48 Å, the figure §4.3 uses) are the right
  ones.
- **Open valences.** The analysis half of `add_hydrogens_filtered` — the
  hybridization detection, `covalent_max_neighbors`, `count_active_neighbors`
  and the host-skip rule — becomes a public
  `open_valence_slots(structure, atom_id, passivant_element) -> usize`, which
  `ProxyStats::open_valences` sums. The placement path needs the hybridization
  the count was derived from as well (it picks the open directions from it), so
  both go through one private
  `open_valence_analysis(…) -> Option<(Hybridization, usize)>` and the public
  function is its `map_or(0, …)`. One analysis, so the multiplicity the panel
  shows and the number of hydrogens `passivate` would add cannot disagree. The
  *options* of the passivation path — `selected_only`,
  `skip_already_passivated`, the region predicate — stay in its loop: they are
  caller-side filters, not a property of the atom.
- **Formula.** `empirical_formula(&AtomicStructure) -> String` in
  `atomic_structure_utils.rs`: elements by descending count, ties by symbol,
  hydrogen always last, counts of one written bare (`Si223H96`, `CH4`,
  `Si3C2NOH5`). Uses `effective_atomic_number`; markers (`Z <= 0`) are skipped.
  Hydrogen-last is what keeps a proxy reading `Si223H96` rather than
  `H96Si223`, and it applies whatever the counts are: water is **`OH2`**. This
  is deliberately *not* Hill notation — the readout this serves is a cluster
  formula, not a chemical index entry.

### 7.6 The node shell

`nodes/proxy.rs` holds `ProxyData` (§3.3), `ProxyEvalCache`, and
`get_node_type()`, and nothing else. Its `eval`:

1. `evaluate_arg_required(…, 0)`; an `Error` is returned as-is (never
   re-wrapped, per the nodes `AGENTS.md`).
2. `*self.available_tags.borrow_mut() = tag names of the input` — the same
   write-only snapshot `tag` takes, for the panel's dropdown.
3. Pins 1–8 through `evaluate_or_default` with `extract_string` /
   `extract_int` / `extract_bool`, each defaulting to the stored property.
4. Validation, in the node's own words: empty `focus` → `proxy: focus tag
   name is empty`; `hops < 0` → `proxy: hops must be >= 0`; `free < 0` →
   clamped to 0; `core < 0` → `None`; `passiv_elem` not allowed → the same
   text `passivate` raises, built from `ALLOWED_PASSIVANTS`.
5. `map_atomic(input, |mut s| { … proxy_cut(&mut s, &focus, &options) … })`.
   The closure returns the structure, so the `Result` is captured in a local
   the way `tag` captures its tag error, and surfaced after the map as
   `proxy: no atom carries the tag "focus"` / `proxy: <ProxyError>`.
6. When `network_stack.len() == 1`, store
   `ProxyEvalCache { stats }` in `context.selected_node_eval_cache` — the
   relax pattern. Nothing mutable on `ProxyData` is read or written from
   `eval` except the write-only `available_tags` snapshot.

Two things §3 leaves open, resolved here:

- **One output pin means `map_atomic` alone.** `snapshot_atoms` and
  `eval_output_with_diff` exist to feed a `diff` pin, and §5 declines a
  second output. Should a `diff` pin ever be wanted, it is appended as pin 1
  with those two helpers, exactly as `atom_cut` has it; nothing here has to
  change.
- **Stats never live on the node data.** A `RefCell<Option<ProxyStats>>`
  written from `eval` would be shared across every call site of a
  subnetwork; the eval cache is per root evaluation of the selected node,
  which is what the panel wants.

`map_atomic` preserves the wrapper, so Crystal in gives Crystal out with its
lattice intact; the module itself is phase-agnostic.

### 7.7 API layer and Flutter

- `rust/src/api/structure_designer/proxy_api.rs`: `get_proxy_data(scope_path,
  node_id) -> Option<APIProxyData>`, `set_proxy_data(scope_path, node_id,
  APIProxyData)` (via `set_node_network_data_scoped` +
  `refresh_structure_designer_auto`, as `set_tag_data`), and
  `get_proxy_stats() -> Option<APIProxyStats>` (selected-node eval cache
  downcast, as `get_relax_message`). `APIProxyData` carries the eight
  persisted fields plus `available_tags`; `APIProxyStats` mirrors
  `ProxyStats` with the two `Option<f64>` fields kept optional. Both twins
  live in `structure_designer_api_types.rs`; the module is added to
  `flutter_rust_bridge.yaml`'s `rust_input` (a new api module is invisible to
  codegen until it is listed).
- `lib/structure_designer/node_data/proxy_editor.dart`, dispatched from
  `node_data_widget.dart` with `scopePath` forwarded, and
  `StructureDesignerModel.getProxyData / setProxyData / getProxyStats`
  wrappers forwarding `propertyEditorScopeChain`. The editor reuses the
  tag-name suggestion dropdown of `tag_editor.dart`, the passivant dropdown
  of `passivate_editor.dart`, `IntSpinField` for `hops` / `free` / `core`,
  and the report block of `relax_editor.dart`, re-read after every refresh.
  `core` shows `-1` as "off".

### 7.8 Using the module without atomCAD

```rust
use atomcad_crystolecule::proxy_cut::{plan_proxy, apply_proxy, proxy_cut, ProxyOptions};

// One cut, by tag. `workpiece` is an `AtomicStructure` with `focus` tags.
let mut proxy = workpiece.clone();
let stats = proxy_cut(&mut proxy, "focus", &ProxyOptions {
    hops: 6, free: 3, core: Some(1), ..Default::default()
})?;
println!("{} — {} free, {} frozen, {} open valences", stats.formula, stats.free, stats.frozen, stats.open_valences);

// A convergence series: plan against the untouched workpiece, apply to a clone.
let focus = workpiece.atoms_with_tag("focus");
for hops in 4..=8 {
    let options = ProxyOptions { hops, free: 3, ..Default::default() };
    let plan = plan_proxy(&workpiece, &focus, &options)?;
    let mut proxy = workpiece.clone();
    let stats = apply_proxy(&mut proxy, &plan)?;
    // relax `proxy` with `simulation::minimize_energy`, compare energies …
}
```

## 8. Implementation plan

Five phases, each shippable on its own and each ending with a green Rust
suite. Standing rules for every phase:

- Run the suite as `cargo test -j 4 -p <crate>` from `rust/`, then
  `cargo fmt` (the crate, never `--all`) and `cargo clippy`.
- Tests go in the owning crate's `tests/` directory, declared in its
  `tests/<crate>.rs` beside their siblings. No `#[cfg(test)]` modules.
- The Dart layer gets no automated tests (house rule); it gets `flutter
  analyze` and a manual walkthrough. The Flutter smoke test is the
  maintainer's to run.
- After any text-format change, the round-trip corpus test must still show
  `query → --replace` as a no-op.

### Phase 1 — Crate core: riders, distances, keep set, caps (no mutation)

**Files.** `crates/atomcad-crystolecule/src/proxy_cut.rs` (new; `pub mod` in
`lib.rs`), `hydrogen_passivation.rs` (`terminator_bond_length` made public),
`tests/crystolecule/proxy_cut_test.rs` (new, declared in
`tests/crystolecule.rs`).

**Deliverables.** `ProxyOptions`, `ProxyError`, `CapPlacement`, `ProxyPlan`,
`classify_riders`, `bond_distances`, `severed_bond_caps`, `plan_proxy`
complete through §7.3 step 11. No `apply_proxy` yet.

**Fixtures.** A bulk silicon cube built with `fill_lattice` (the `fill_box`
helper of `lattice_fill_test.rs`, hydrogen passivation on, no
reconstruction) of at least 8 unit cells a side with the source at the centre
atom, so no shell of the `hops = 6` filled cut reaches the surface; and small
hand-built graphs (`AtomicStructure::new` + `add_atom` + `add_bond`) for the
rules that need a specific topology.

**Automated tests** (`proxy_cut_test.rs`):

- Riders: a hydrogen with one bond is a rider mapped to its host; a singly
  bonded heavy adatom is a rider; an isolated atom is not; a source that is a
  rider promotes its host and the rider is kept.
- Distances: on a hydrogen-capped chain and a six-ring the distances are the
  graph distances; riders are never traversed and get no distance; a second
  component gets no distance. (The chain has to be capped: the end atoms of a
  bare one have a single bond, so §4.1 makes them riders and they get no
  distance at all.)
- Multi-source merge: two unbonded fragments (a "tool" above a "surface")
  with one source each are both kept; a third fragment bonded to neither is
  dropped entirely; with `fill: false`, `hops = 0` keeps exactly the sources
  and their riders, and with `fill` on a heavy atom bonded to two sources is
  restored as well.
- The §4.3 fill table on the bulk cube, one source: `hops = 4` → 83 kept
  heavy atoms before fill, 40 shared sites (dropped heavy atoms with two kept
  heavy neighbours), closest planned cap pair 1.42 Å; after fill 165 kept,
  `fill_rounds = 4`, no shared site, closest pair 2.42 Å, `farthest_hop = 8`;
  `hops = 6` → 239 before, 455 after, 6 rounds, `farthest_hop = 12`. Every
  atom in `filled` has `distance > hops`. With `fill: false` the before
  figures are what the plan keeps.
- Cap geometry: each `CapPlacement.position` equals
  `pos(host) + unit(pos(severed) − pos(host)) · L` with `L = 1.48` for
  Si–H, and the fluorine length from `halogen_bond_length` when
  `passivant_element = 9`; a severed bond from a saturated focus atom
  produces exactly one cap; a focus atom that was unsaturated in the input
  (three bonds on silicon) gets no cap for its missing bond.
- `rm_single`: a hand-built case where the cut leaves an atom with one kept
  neighbour drops it and its riders; an atom singly bonded in the input is
  untouched, and so is an atom whose only neighbours are a heavy adatom rider
  and one other atom — the rider counts in neither tally, which leaves one
  heavy neighbour in the input and so the input exemption, not the rider,
  is what saves it; a case where `fill` gives the atom its second neighbour
  keeps it (order: fill before `rm_single`); the cascade: a chain hanging off
  a ring unwinds a round at a time once its end is cut by distance, and on the
  bulk cube at `hops = 4` no kept non-source heavy atom is left with a single
  kept heavy neighbour; a source reduced to one kept neighbour by the cascade
  (a chain hanging off a lone focus atom) survives.
- Monotonicity: with `fill` on and off, `kept(hops = k) ⊆ kept(hops = k + 1)`
  for `k` in 3..7 on the cube; `frozen(free = f) ⊇ frozen(free = f + 1)`.
- Frozen list is exactly `distance > free`; empty for the plain cut when
  `free >= hops`; fill-restored atoms beyond `free` are in it.
- High list: `core = None` → empty; `Some(0)` → the sources only;
  `Some(1)` → sources plus first neighbours; riders are not in it (they
  follow at apply time).
- `nearest_dropped`: on the two-fragment fixture with a dropped wall 3.0 Å
  from a free atom the value is `Some(3.0 ± 1e-9)`; on the bulk cube with
  `hops = 6, free = 3` it is `Some(> 4.0)`; on a structure where nothing is
  dropped it is `None`.
- Errors: no sources → `NoFocusAtoms`; `passivant_element = 2` →
  `BadPassivant(2)`.
- Determinism: two plans of the same input are equal, and every list is
  sorted ascending.
- `terminator_bond_length(14, 1) == 1.48` and `(6, 1) == 1.09`, and the
  halogen branch equals `halogen_bond_length` — the values the general
  passivation path already uses, so the factoring changed nothing.

### Phase 2 — Crate core: apply, stats, convenience

**Files.** `proxy_cut.rs` (`apply_proxy`, `proxy_cut`, `ProxyStats`),
`hydrogen_passivation.rs` (`open_valence_slots` public, used by
`add_hydrogens_filtered`), `atomic_structure_utils.rs` (`empirical_formula`),
`proxy_cut_test.rs` extended, `hydrogen_passivation_test.rs` extended for
the factoring.

**Automated tests:**

- Apply on the bulk cube: atom count equals `heavy + riders + caps` from
  the stats; every cap is bonded once, single order, to its host, sits at the
  planned position, carries the passivation flag; no atom retains a bond to a
  dropped id; `num_bonds` equals the bonds counted by walking the atoms.
- Frozen only set: an atom frozen in the input at distance 0 is still
  frozen; riders and caps of a frozen host are frozen; free riders of a free
  host are not; `stats.free + stats.frozen` equals the atom count.
- `high`: riders and caps inherit; an input already carrying `high` on an
  atom beyond `core` keeps it; `core = None` leaves `tag_names()` unchanged;
  with 32 live tags and no `high` among them, `apply_proxy` returns
  `ProxyError::Tag(LimitReached)` and the structure is byte-identical to the
  input (the intern-first rule).
- Radical preservation: a focus silicon with one hydrogen removed keeps
  three bonds and no cap; `open_valences == 1`; `add_hydrogens` on a clone
  adds exactly `open_valences` atoms (the shared `open_valence_slots`).
- Proxy of a proxy: cutting `hops = 4` from the `hops = 6` proxy gives the
  same heavy-atom set and the same cap positions as cutting `hops = 4` from
  the workpiece (caps are riders, §4.1).
- Stats: `formula` on the cube proxy has the `Si…H…` shape with counts
  matching the atoms; `min_cap_pair` is `Some(2.42 ± 0.01)` with fill and
  `Some(1.42 ± 0.01)` without, and `None` on a hand-built cut whose two caps are
  farther apart than `CAP_PAIR_RADIUS`; `min_rim == hops − free`;
  `filled.len() == stats.filled`.
- `empirical_formula`: `CH4`, water as `OH2` (hydrogen is last whatever its
  count — the §7.5 rule, not Hill notation), a mixed structure's descending
  order with ties by symbol, bare count of one, markers skipped, parameter
  elements resolved.
- `proxy_cut` by tag: missing tag → `NoFocusAtoms`; tag on a rider promotes
  the host; result equals `plan_proxy` + `apply_proxy` with the same ids.
- Passivation regression: the existing `hydrogen_passivation_test.rs` suite
  is unchanged and green after `open_valence_slots` is factored out.

### Phase 3 — The node

**Files.** `crates/atomcad-structure-designer/src/nodes/proxy.rs` (new),
`nodes/mod.rs`, `node_type_registry.rs`,
`tests/structure_designer/proxy_node_test.rs` (new, declared in
`tests/structure_designer.rs`), the round-trip corpus fixture gains a proxy
network.

**Deliverables.** `ProxyData` with serde defaults for every field (an old
`.cnnd` without `fill` loads `true`), `ProxyEvalCache`, the eval of §7.6,
text properties for all eight fields, the `focus · 6 / 3` subtitle,
parameter metadata (`molecule` required, the rest optional), registration.

**Automated tests** (`proxy_node_test.rs`, driving `StructureDesigner`
through the text format the way sibling node tests do):

- Crystal in → Crystal out with the input lattice; Molecule in → Molecule
  out.
- Stored property versus wired pin: an `int` node wired to `hops` overrides
  the stored value; disconnecting it restores the stored value.
- Defaults: a freshly created node evaluates as `hops 6 / free 3 / fill on /
  passivate on / H / core off`.
- Text format: the full line of §3.3 parses, evaluates and serializes back
  to itself byte for byte; the short form `proxy { molecule: x, core: 1 }`
  parses with every omitted property at its default and serializes to the
  full line; `fill: false` and `passiv_elem: 9` survive a round trip. The
  corpus test still reports `--replace` as a no-op.
- `.cnnd` round trip of a network containing the node, including a file
  written without the `fill` key.
- Localized errors: no atom carries the tag → the message names the tag;
  empty `focus` → the empty-name error; `hops: -1` → error; `free: -1`
  evaluates as `free: 0` (same output); `passiv_elem: 2` → the same text
  `passivate` produces; an upstream `Error` on `molecule` passes through
  unchanged.
- Eval cache: after a root evaluation with the node selected,
  `get_selected_node_eval_cache` downcasts to `ProxyEvalCache` and its stats
  match a direct `proxy_cut` on the same input; a nested evaluation (the node
  inside a custom network) stores nothing.
- Subtitle: `focus · 6 / 3` with pins 1–3 unconnected; `None` once any of
  them is wired.
- Registry: `get_compatible_node_types` from a Crystal output lists `proxy`
  under *AtomicStructure*, alongside the existing cases in
  `rust/tests/structure_designer_api/node_type_registry_test.rs`.

### Phase 4 — API, property panel, documentation

**Files.** `rust/src/api/structure_designer/proxy_api.rs` (new) and its
`mod.rs` entry, `structure_designer_api_types.rs` (`APIProxyData`,
`APIProxyStats`, `From` impls), `flutter_rust_bridge.yaml`, the generated
bindings, `lib/structure_designer/node_data/proxy_editor.dart` (new),
`node_data_widget.dart`, `structure_designer_model.dart`,
`doc/reference_guide/nodes/atomic.md` (`## proxy` after `## atom_cut`),
`crates/atomcad-crystolecule/src/AGENTS.md` (module map row and a
`ProxyPlan` / `ProxyOptions` row in Key Types), `rust/tests/structure_designer_api/proxy_api_test.rs`
(new).

**Automated tests** (`proxy_api_test.rs`, the `structure_designer_api`
harness, run explicitly with `cargo test -j 4 --test structure_designer_api`):

- `set_proxy_data` then `get_proxy_data` round-trips all eight fields
  through the global instance, with `scope_path` respected inside a custom
  network.
- `get_proxy_data` on a node of another type returns `None`.
- `get_proxy_stats` returns `Some` with the crate's figures after the proxy
  node is selected and evaluated, and `None` when a `relax` node is selected
  instead.
- `set_proxy_data` marks the network dirty and is undoable (the persisted-
  mutation rule), checked through the existing undo API tests' helpers.
- `flutter analyze` clean; `dart format` applied.

**Manual walkthrough** (maintainer): tag dropdown lists the input's tags;
spin fields clamp `hops >= 0`; the report updates after each edit and reads
"—" for an absent `min_cap_pair` / `nearest_dropped`; `core = -1` displays as
off; undo reverts a property edit; the Flutter smoke test.

### Phase 5 — Worked example and scale

**Files.** A fixture `.cnnd` under `rust/tests/fixtures/` with the §6
network (Si(100) slab, a tool, two `tag` nodes, `proxy_6`, `proxy_7`),
`doc/reference_guide/nodes/atomic.md` example, `doc/testing.md`.

**Automated tests** (`proxy_node_test.rs` and `proxy_cut_test.rs`):

- The fixture evaluates; `proxy_6`'s heavy atoms are a subset of `proxy_7`'s;
  the tool apex is unsaturated in both; no dropped heavy atom is left bridging
  two kept ones and no cap pair is under the ~2 Å unphysical line (the
  `>= 2.42 Å` this plan first asked for is an **ideal-lattice** figure and does
  not survive a cut through the reconstructed surface — see §6); every
  fill-restored atom is frozen; the apex, the target and their first
  neighbours carry `high` and nothing else does.
- A `map` over `range 4..8` of a `proxy` in a **zone body** yields four
  structures with strictly increasing atom counts. (A `closure` would do, but
  the body reads `molecule` from one scope out and `hops` from the zone input,
  which is the shape the text format spells `^site2` / `$element`.)
- Independence from workpiece size: the §4.3 figures for `hops = 4` and
  `hops = 6` are identical on an 8-cell and a 16-cell cube, which exercises
  the unbounded BFS and the grid-backed `nearest_dropped` on ~33k silicon
  atoms plus their surface hydrogens, with no timing assertion.

## 9. Future: ONIOM export

Not part of this node's implementation; recorded here so the node's data
model is already shaped for it.

### 9.1 What an ONIOM input needs

1. **A layer label per atom** — high or low, occasionally a middle layer.
2. **Link atoms** at every bond that crosses a layer boundary, placed along
   the bond at a scaled distance (the ratio of the host–H to the host–host
   bond length, ≈ 0.71 for C–C). Some codes generate them from the labels
   (Gaussian), others want them listed (ORCA, an ASE driver).
3. **Per-layer bookkeeping** — charge and multiplicity of the high-layer
   model system, and the frozen set, which is independent of the layers.

### 9.2 How atomCAD already carries it

- **Layer membership is the `high` tag** (§4.7), painted by `proxy { core }`
  or by hand with `tag` + a region. Untagged is low. A `mid` tag can be added
  by the same convention if a three-layer scheme is ever needed. Nothing new
  on `Atom`.
- **Frozen atoms are the existing frozen flag** that `relax` honours.
- **Link atoms are the proxy's severed-bond cap** (§4.5) applied at the
  layer boundary instead of the cut, with the ONIOM scaled length, and
  generated **at export time** — never inserted into the structure. The
  crystolecule module that owns the cap owns this too.
- **Nothing downstream may assume the shell index is bounded by `hops`.**
  With `fill` on, a `hops: 6` proxy contains atoms at distance 12 (§4.3).
  An exporter that orders or groups atoms by shell reads the per-atom
  distance, not the parameter.

### 9.3 The one new node: `export_oniom`

A sibling of `export_atoms` that reads the `high` tag and the frozen flag
and writes the target format:

- a Gaussian ONIOM block — layer letter per atom line, link-atom specs on
  the crossing bonds;
- an ORCA `QMAtoms` list plus the link-atom lines;
- an ASE script wiring a UMA calculator for the low layer and a DFT
  calculator for the high layer, with the subtractive energy assembled in
  Python.

Charge and multiplicity of the model system are node properties on the
exporter, not per-atom data. The driver itself — the subtractive energy,
the optimizer, the barrier search — stays outside atomCAD.

### 9.4 A rule the exporter enforces

A layer boundary must cut only **single bonds between like atoms** (Si–Si,
C–C) and never a bond touching a focus atom. The exporter walks the crossing
bonds and refuses with a localized error naming the offending bond. A refused
export is far cheaper than a quietly wrong barrier.
