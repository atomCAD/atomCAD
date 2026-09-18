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
| 3 | `free` | Int | no | Heavy atoms farther than this get the frozen flag. `free >= hops` freezes nothing. |
| 4 | `rm_single` | Bool | no | Drop outermost-shell heavy atoms left with a single heavy neighbour by the cut. |
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
`snapshot_atoms` / `map_atomic` / `eval_output_with_diff` path that `atom_cut`
uses, not the metadata-only `map_atomic_in_region`.

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
    #[serde(skip)] pub stats: RefCell<Option<ProxyStats>>,   // panel report, as RelaxEvalCache
}

pub struct ProxyStats {
    pub formula: String,     // empirical formula of the output, e.g. "Si223H96"
    pub heavy: usize,        // heavy atoms kept (after fill and rm_single)
    pub riders: usize,       // riders kept
    pub caps: usize,         // terminators added
    pub free: usize,         // atoms without the frozen flag
    pub frozen: usize,       // atoms with the frozen flag
    pub filled: usize,       // heavy atoms added by fill
    pub fill_rounds: usize,  // iterations fill needed to converge
    pub farthest_hop: i32,   // largest distance among kept heavy atoms
    pub min_rim: i32,        // hops - free, the thinnest frozen shell
    pub open_valences: usize,// unsaturated slots left on kept atoms
    pub min_cap_pair: f64,   // closest cap-cap distance, Å
    pub nearest_dropped: f64,// closest dropped heavy atom to any free atom, Å
}
```

All eight persisted fields are text properties, so the text form is

```
proxy_6 = proxy { molecule: surface, focus: "focus", hops: 6, free: 3, core: 1 }
```

with `rm_single`, `passivate`, `passiv_elem` and `fill` omitted at their
defaults, as the text format does for every node (`fill: false` appears only
when it is switched off). Node subtitle when pins 1–3 are unconnected:
`focus · 6 / 3`.

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
- `min_cap_pair` — the closest two terminators come to each other. Below
  about 2 Å the rim is unphysical; with `fill` on this is 2.42 Å for silicon.
- `nearest_dropped` — the closest dropped heavy atom to any *free* atom. A
  small value (under about 4 Å) means an unbonded neighbour — a trench wall,
  a second tip — is close enough to matter sterically and was cut away; the
  remedy is to tag one of its atoms `focus` (§4.2).

`available_tags` is snapshotted from the input on every eval so the panel can
offer a dropdown, exactly as `tag` does. Neither `RefCell` field is ever
*read* from `eval` (subnetwork node state is shared across call sites).

## 4. Semantics

The eval runs these steps in order on a clone of the input structure.

### 4.1 Riders

An atom with **exactly one bond** in the input is a *rider*: a hydrogen, a
halogen passivant, any monovalent terminator. Riders

- never count as a hop and are never traversed;
- are kept exactly when their one heavy neighbour is kept;
- inherit the frozen decision of that neighbour;
- if tagged as focus, promote their heavy neighbour to a source instead.

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
lattice graph (hydrogen caps on every severed bond; the crate test in §7
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

Off by default. When on, one pass over the **converged boundary** (after
`fill`): a kept heavy atom whose kept heavy neighbours number exactly one,
**and** which had more than one heavy neighbour in the input, is dropped with
its riders. Atoms singly bonded in the input (an adatom, a terminal group)
are not touched. Without `fill`, singly attached atoms can only appear in the
outermost shell (an atom at distance `k < hops` has all its neighbours at
distance `<= k+1 <= hops`); with `fill` they can only be atoms fill did not
touch, since fill adds atoms with at least two kept neighbours and only ever
raises the neighbour count of what is already kept. One pass is complete in
both cases. It runs *after* fill so it cannot strip an atom that fill would
have given a second neighbour.

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
frozen whenever `free < hops`. **Flags are only ever set, never cleared**: an atom frozen
in the input stays frozen whatever its distance. Boundary atoms come from the
lattice, so they are already at bulk positions — this is the frozen rim that
makes a finite cluster stand in for a big surface. The `relax` node honours
the flag today; an exporter for an external code reads the same bit.

### 4.7 `core`

Off by default (`-1`). When `core >= 0`, heavy atoms with distance `<= core`
get the tag **`high`**; riders and caps inherit the tag of their heavy atom.
Zero is meaningful: only the focus atoms are high. The tag is **only added,
never removed** — an input that already carries `high` keeps it, mirroring
how `free` treats the frozen flag.

**Untagged means low.** No `low` tag is written: every atom would carry it,
it would spend a second name of the 32-tag budget, and the exporter (§8)
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
and no two of those hydrogens are closer than 2.42 Å because `fill` (on by
default) kept every atom that two survivors shared; everything more than
three hops from the apex or the target is frozen; the apex, the target atom
and their first neighbours carry `high`, ready for the ONIOM exporter of §8.
Expect the filled `proxy_6` to hold roughly 1.5× the atoms of the plain
six-hop shell, all of the extra ones frozen.

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

## 7. Implementation notes

- Node file `rust/crates/atomcad-structure-designer/src/nodes/proxy.rs`,
  registered in `nodes/mod.rs` and `node_type_registry.rs`; the reference
  guide page `doc/reference_guide/nodes/atomic.md` gets a `## proxy` section
  next to `atom_cut`.
- The BFS, rider classification and severed-bond cap belong in
  `atomcad-crystolecule` (a `proxy_cut` module beside `hydrogen_passivation`),
  so the node is a thin adapter like its siblings and the algorithm is
  testable from the crate's `tests/` directory.
- Bond-length lookup must be **shared** with `hydrogen_passivation.rs`, not a
  fourth copy (the halogen design doc already counts three sites).
- The BFS distance of every kept atom, including the atoms `fill` adds, is
  kept through the pipeline: `free`, `core` and the `farthest_hop` stat all
  read it.
- Stats that need geometry (`min_cap_pair`, `nearest_dropped`) are computed
  once at the end of the eval over the caps and the dropped set; the
  structure sizes here are a few hundred atoms, so a brute-force pass is
  fine, but a dropped set the size of the input (a proxy cut from a
  million-atom workpiece) needs a spatial grid for `nearest_dropped` — reuse
  whatever `infer_bonds` uses.
- Tests: rider retention, multi-source merge across an unbonded tool, radical
  preservation at the focus, `rm_single` restricted to cut-created singles
  and running after `fill`, cap direction equals the old bond vector,
  frozen-only-set, monotonic `hops` series with `fill` on and off, `core`
  tagging (riders inherit, existing `high` kept, `-1` writes nothing),
  Crystal-in/Crystal-out, a text-format round trip (`fill: false` survives,
  the default is omitted), and the fill table of §4.3: on a bulk diamond
  block with one focus atom, `hops = 4` gives 83 atoms with 40 shared sites
  and a 1.42 Å cap pair before fill, and 165 atoms, no shared site, a
  2.42 Å closest pair and `farthest_hop = 8` after four rounds; `hops = 6`
  gives 239 → 455 in six rounds. Fill-added atoms must all carry the frozen
  flag when `free < hops`.

## 8. Future: ONIOM export

Not part of this node's implementation; recorded here so the node's data
model is already shaped for it.

### 8.1 What an ONIOM input needs

1. **A layer label per atom** — high or low, occasionally a middle layer.
2. **Link atoms** at every bond that crosses a layer boundary, placed along
   the bond at a scaled distance (the ratio of the host–H to the host–host
   bond length, ≈ 0.71 for C–C). Some codes generate them from the labels
   (Gaussian), others want them listed (ORCA, an ASE driver).
3. **Per-layer bookkeeping** — charge and multiplicity of the high-layer
   model system, and the frozen set, which is independent of the layers.

### 8.2 How atomCAD already carries it

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

### 8.3 The one new node: `export_oniom`

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

### 8.4 A rule the exporter enforces

A layer boundary must cut only **single bonds between like atoms** (Si–Si,
C–C) and never a bond touching a focus atom. The exporter walks the crossing
bonds and refuses with a localized error naming the offending bond. A refused
export is far cheaper than a quietly wrong barrier.
