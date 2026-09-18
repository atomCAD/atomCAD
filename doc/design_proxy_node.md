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
`passivate`, then `freeze` with a second, larger region. That has three
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

## 2. Idea

Mark the atoms at the reaction site with a tag (default name `focus`). The
node computes the **bond-graph distance** — hops over covalent bonds, counted
over heavy atoms only — from the nearest focus atom to every other atom, keeps
what lies within `hops`, freezes what lies beyond `free`, caps the severed
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

Pins 4–6 deliberately reuse the names and types of the same flags on
`materialize`, so `proxy { passivate: true, rm_single: true }` reads the same
way `materialize { passivate: true, rm_single: true }` does. Any future pin
must be **appended** (pin indices are persisted in wires).

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
    #[serde(skip)] pub available_tags: RefCell<Vec<String>>, // tag dropdown, as TagData
    #[serde(skip)] pub stats: RefCell<Option<ProxyStats>>,   // panel report, as RelaxEvalCache
}
```

All six persisted fields are text properties, so the text form is

```
proxy_6 = proxy { molecule: surface, focus: "focus", hops: 6, free: 3 }
```

with `rm_single`, `passivate` and `passiv_elem` omitted at their defaults, as
the text format does for every node. Node subtitle when pins 1–3 are
unconnected: `focus · 6 / 3`. `ProxyStats` (atoms kept, atoms free, caps
added) feeds a one-line report in the properties panel, the way `relax`
shows its message. `available_tags` is snapshotted from the input on every
eval so the panel can offer a dropdown, exactly as `tag` does. Neither
`RefCell` field is ever *read* from `eval` (subnetwork node state is shared
across call sites).

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
focus, are dropped together with their riders and all bonds to them.

### 4.3 `rm_single`

Off by default. When on, one pass over the **outermost shell** (distance
exactly `hops`): a heavy atom whose kept heavy neighbours number exactly one,
**and** which had more than one heavy neighbour in the input, is dropped with
its riders. Atoms singly bonded in the input (an adatom, a terminal group)
are not touched. Singly attached atoms can only appear in the outermost
shell (an atom at distance `k < hops` has all its neighbours at distance
`<= k+1 <= hops`), so one pass over that shell is complete.

Default off so that the `hops` series stays monotonic: every atom of the
`hops = 5` proxy is in the `hops = 6` proxy.

### 4.4 `passivate`

On by default. For every kept heavy atom `A` and every bond `A–B` where `B`
was dropped (by distance or by `rm_single`), place a terminator at

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

### 4.5 `free`

Heavy atoms with distance `> free` get the frozen flag set; their riders and
their caps follow. **Flags are only ever set, never cleared**: an atom frozen
in the input stays frozen whatever its distance. Boundary atoms come from the
lattice, so they are already at bulk positions — this is the frozen rim that
makes a finite cluster stand in for a big surface. The `relax` node honours
the flag today; an exporter for an external code reads the same bit.

### 4.6 Errors

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
- **No per-shell tags.** The tag budget is 32 names per structure; carrying a
  shell index as tags would burn it. The `free` threshold lives in this node
  for that reason, rather than in a hop-aware `freeze`.
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
proxy_6  = proxy { molecule: site2, hops: 6, free: 3 }
relaxed  = relax { molecule: proxy_6 }
```

Convergence check: duplicate `proxy_6` as `proxy_7` with `hops: 7`, relax
both, compare. The tool's apex radical is unsaturated in both; every silicon
that lost a neighbour to the cut carries a hydrogen on the old bond vector;
everything more than three hops from the apex or the target is frozen.

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
- Tests: rider retention, multi-source merge across an unbonded tool, radical
  preservation at the focus, `rm_single` restricted to cut-created singles,
  cap direction equals the old bond vector, frozen-only-set, monotonic `hops`
  series, Crystal-in/Crystal-out, and a text-format round trip.
