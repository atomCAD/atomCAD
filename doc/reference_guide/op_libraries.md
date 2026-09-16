# Operation libraries

*The two JSON files a mechanosynthesis build is made of — the **operation
library** and the **build script** — what is in them, and how to author one the
placement tool can offer correctly.*

This page is the normative description of both formats. The nodes that read them
are [`ops_library`](./nodes/atomic.md#ops_library) and
[`build_script`](./nodes/atomic.md#build_script); what the
[`mechanosynth`](./nodes/atomic.md#mechanosynth) node does with a step once it
has one is [*How a step is applied*](./nodes/atomic.md#how-a-step-is-applied),
and what the [`mechanosynth_edit`](./nodes/atomic.md#mechanosynth_edit) tool
does with an operation is [*Placing a
step*](./nodes/atomic.md#placing-a-step).

Both files are meant to be written by a **generator** that already knows every
coordinate, not by hand. The format half below says what the fields are; the
[*Authoring a library*](#authoring-a-library) half is addressed to whoever
writes that generator.

## The operation library

### The rewrite: `before` and `after`, compared by pattern id

An **operation** is a named before/after pair of small atom lists with concrete
positions in a local frame. Comparing the two halves by *pattern id* is the
rewrite — there is no separate diff syntax:

| Situation | Effect on the workpiece |
|---|---|
| id in both, same position and element | atom kept, untouched |
| id in both, different position | atom moved |
| id in both, different element | atom replaced in place |
| id only in `before` | atom deleted, along with every bond it had |
| id only in `after` | atom added |
| bond only in `after` | bond added |
| bond only in `before` | bond deleted |

The element `"*"` means "don't compare elements": in `before` it matches any
element (so one `habst` serves carbon, germanium and silicon hosts), and in
`after` on a kept atom it leaves the element alone. It is rejected on an atom
that only `after` has — an added atom needs a real element.

### Bonds are a complete statement

For every *pair* of atoms a pattern names:

- a listed bond `[a, b, order]` means the workpiece has a bond between those two
  atoms, of that order;
- an **unlisted pair means the workpiece has no bond between them**.

Either way a disagreement is a refusal — the step does not apply, and the
interactive tool does not offer it. Bonds to atoms *outside* the pattern are not
constrained; [`deg`](#deg-how-many-bonds-the-host-has) is what speaks about
those.

So a pattern lists **every bond that exists among the atoms it names**, in
`before` and in `after` alike: a donation lists the host's bonds to its frame
atoms in both halves, a dimer manipulation lists the dimer bond, a tool side
lists the apex's bonds to its legs. Two consequences are worth stating because
they are the point. An operation that bonds two atoms — `bridge` — lists no bond
between them in `before`, which asserts they are apart, so offering it on a pair
that is *already* bonded is impossible and needs no rule of its own. And a
deletion cannot silently do nothing: a step that deletes a bond is a step whose
`before` says the bond is there.

### `deg`: how many bonds the host has

A `before` atom may also state **`deg`**, the number of bonds the matched
workpiece atom must have, of any order, counting each bond once:

```json
{ "id": 1, "el": "*", "pos": [0, 0, 0], "deg": 3 }
```

Absent is "don't care", so a library matched against a structure whose bonds
were never perceived — an `.xyz` import — can leave it out and lose only this
check. Write it **as drawn**: the number of bonds the atom had in the workpiece
the pattern was computed from. It is what says "this donation goes on a
three-coordinate host, not on a bulk atom": the bond list speaks only about
pairs *inside* the pattern, so a bulk atom with three listed neighbours and one
unlisted one satisfies every bond rule, and `deg: 3` is what rejects it. `deg`
belongs on a `before` atom; on an `after` atom it is a load error.

### `anchors`: which atoms a click may play

An operation may state **`anchors`**, how many of its leading `before` ids are
atoms a user may click:

```json
{ "name": "c_insert", "anchors": 2, "method": "tip", ... }
```

The default is **1** — id 1, the atom at the origin — which makes the origin
convention the rule rather than a preference: an operation is offered *only* on
the atoms it acts on, and a click on a frame atom does not list it. Raise it
only when a reaction has primary atoms of **different elements** and a user
might reasonably click either. A symmetric pair of the same element needs
nothing: a click on either already plays id 1. Every anchor must be an atom the
operation actually touches; naming a frame atom is a load error.

### A library, whole

```json
{
  "format": "atomcad-msops/3",
  "tolerance": 0.3,
  "clash": 0.9,
  "tools": [
    {
      "name": "habst_tool",
      "note": "Hydrogen abstraction tool: an ethynyl radical on a handle.",
      "states": ["charged", "spent"],
      "frame": [
        { "tag": "apex", "pos": [0, 0, 0] },
        { "tag": "a",    "pos": [1.45, 0, -3.17] },
        { "tag": "b",    "pos": [-0.73, 1.26, -3.17] },
        { "tag": "c",    "pos": [-0.73, -1.26, -3.17] }
      ]
    }
  ],
  "ops": [
    {
      "name": "habst",
      "method": "tip",
      "before": { "atoms": [ {"id": 1, "el": "H", "pos": [0, 0, 0], "deg": 1} ],
                  "bonds": [] },
      "after":  { "atoms": [], "bonds": [] },
      "tool": {
        "type": "habst_tool",
        "from": "charged",
        "to": "spent",
        "before": { "atoms": [ {"id": 1, "el": "C", "pos": [0, 0, 0]} ], "bonds": [] },
        "after":  { "atoms": [ {"id": 1, "el": "C", "pos": [0, 0, 0]},
                               {"id": 2, "el": "H", "pos": [0, 0, 1.06]} ],
                    "bonds": [ [1, 2] ] }
      }
    }
  ]
}
```

### Methods, and the tool side

**Every operation states its `method`** — `tip`, `bulk` or `spontaneous`, the
three kinds described under
[*Methods*](./nodes/atomic.md#methods-how-a-step-is-performed). The key is
required, and an unknown value is a load error naming the operation: how
a reaction is performed is a fact about the reaction, not a choice a build
script makes, so it is stated once by whoever researched it. A `bulk` operation
must also name the **`agent`** that performs it (`"Cl2"`, `"UV"`) and may not
carry a tool side; a `tip` operation must carry one, and it is there that it
names its tool type.

A `tip` operation's **`tool` side** is the same before/after rewrite as its
target side, written in the tool's own local frame: what the tool looks like
before the reaction and after it. `from` and `to` name states from the tool
type's `states` list, and both halves may be empty — a bare probe performing
lithography names its type so the step records which instrument visited, without
asserting any change.

### The `tools` section

The **`tools` section** describes each tool type the process uses: its `name`,
an optional `note`, an optional `states` list whose **first entry is the state
every tool of that type starts in**, and a `frame` of named atom positions in
the tool's local coordinates — four of them, which is the format's floor rather
than its ceiling but is what every library in practice writes. Exactly one frame
entry is tagged `apex` and sits at the origin, and the four may not be
coplanar — three points
are congruent to their own mirror image in space, so it takes a fourth off their
plane for a molecule built the wrong way round to be refused rather than
accepted mirrored. Take the legs off the tool's **handle**, not off its business
axis, which is usually linear and would leave the four coplanar.

**The frame atoms must be atoms the tool keeps.** The frame and every tool side
of that type share one local coordinate system: the pose solved from the frame
is the transform every tool-side pattern is placed with. So the four named atoms
have to exist, in the same places, in every state the tool passes through — the
handle, never the apex's cargo. A library that names an atom its own tool side
moves or deletes is wrong in a way nothing can check at load time; what you see
is the residual climbing on the next binding. Tag names are unique within a
frame, and a tool type's name may not collide with a frame tag.

The section is required as soon as any operation is `tip`, and every rule above
is a load error naming the tool type.

An operation may also carry an **`approach`** pose (`r` and `t`), the tool frame
relative to the target frame at the moment of reaction. It is read and kept for
a future animation feature and changes nothing today.

### `chiral`

An operation may state **`"chiral": true`**, meaning a mirrored placement is a
different reaction rather than the same one seen from the other side. Replay
ignores it — a build file states the rotation it wants — and the interactive
placement tool reads it as "do not offer the mirrored fit".

### The origin convention

By convention the `before` atom with **id 1 sits at the origin** and is the atom
the operation acts on. A library that breaks the convention still loads and
still replays; `ops_library` shows a warning naming the operation. Following it
is what makes "click the atom the operation acts on" true for every operation
in a library — and since `anchors` defaults to 1, an operation whose `before`
has no id 1 is one the interactive tool never offers, whatever you click.

### Frame atoms

A one-atom `before` pattern carries no orientation, so a
donation that must land in a particular direction lists the host's bonded
neighbours as `"*"` atoms that appear unchanged in `after`. They fix the
rotation and nothing else — and because "what a step touched" is decided from
*effect* rather than from pattern membership, they never light up under
`ms_current` and need no flag to keep them out of it.

Naming them makes a pattern mirror- and distance-sensitive, which is what lets
one reaction on two different hosts be two operations rather than one loose one
— see [*Authoring a library*](#authoring-a-library) rules 2 and 5 for how many
to name and how to name the variants.

### `tolerance`: the match gate

The **library's** `tolerance` is the match tolerance for every replay against
it; a library that states none gets the default, **0.05 Å**. That is tight on
purpose: a generated library's patterns are congruent to the workpiece to
floating-point precision, and the smallest difference between two *environments*
known — an ideal-site host against a reconstructed dimer atom — is 0.12 Å, so a
gate an order of magnitude below that admits the right variant of an operation
and rejects the wrong one. A hand-written library that needs slack states its
own value. A build file's `tolerance` is read and ignored.

### `clash`: the steric factor

A library may state a top-level **`clash`**, its own steric factor, in the same
spirit as its `tolerance`: the fraction of a covalent-radius sum that two atoms
*not bonded to each other* must stay above. The engine's default is **0.9**,
and [*How a step is applied*](./nodes/atomic.md#how-a-step-is-applied) says what
it does. State your own only with a reason in the library's `note` — the idiom that comes closest,
placing an atom at exactly the bond length from a neighbour a later `bridge`
step will bond it to, sits at about 1.05 and clears 0.9 comfortably.

### Warnings the loader raises

None of these refuses a file; `ops_library` lists them, each naming its
operation.

| Warning | What it means |
|---|---|
| id 1 is not at the origin, or the pattern has no id 1 | [the origin convention](#the-origin-convention) |
| `before` spans at most a plane while `after` places an atom off it | the fit cannot tell the pattern's up from its down, so the reaction can be placed upside down into the bulk. Name a frame atom off the plane |
| two atoms of one pattern closer than 1.1 bond lengths with no bond between them | either the bond is missing from the file, or the library really means they are apart — in which case the workpiece had better agree |
| `deg` plus what the operation bonds, minus what it breaks, exceeds the element's covalent valence | the operation would over-coordinate the atom. A warning rather than an error because the element table is a short one, and a metal apex is exactly what it cannot cover |

### Versioning, and unknown keys

**Each library format replaces the one before it outright.** A `/1` or a `/2`
file is refused with a message saying so; the build-script format is still
`atomcad-msbuild/2` and is unchanged. Libraries are written by generators, and
the fix is to regenerate them rather than to keep two readers — and for `/3`
that is not merely convenience: closed-world bonds change what a pattern *means*
when it lists none, from "nothing is said" to "these atoms are not bonded", so
reading a `/2` file as `/3` would put an assertion in it that its author never
made.

Unknown keys are ignored everywhere, so generators are free to add provenance
fields (`"basis": "Freitas & Merkle 2008, RS7"`).

## The build script

A **build script** lists steps, each naming an operation and a rigid transform
that places the operation's local frame into workpiece coordinates
(`p_workpiece = r · p_local + t`; `r` defaults to the identity, and improper
rotations are allowed because they are lattice symmetry operations a generator
may want). The optional `note` is free text describing the step.

```json
{
  "format": "atomcad-msbuild/2",
  "tolerance": 0.3,
  "steps": [
    {
      "op": "habst",
      "t": [3.567, 0.892, 12.40],
      "note": "layer 1, dimer 3, left H",
      "phase": "layer1",
      "layer": 1,
      "site": 0
    }
  ]
}
```

A step may also carry three **optional metadata fields**. They change nothing
about what the step *does*; they are what the node can say about it, to the
network and to you:

| Field | Type | Absent | Meaning |
|---|---|---|---|
| `phase` | string | `""` | the phase of the process the step belongs to. Many steps, possibly of mixed methods. This is what the panel's phase list groups by (together with `layer`). |
| `layer` | integer | `-1` | the terrace the step builds, counted by the generator (`1` for the first new layer over the seed, say). `-1` means "no particular layer" — substrate work, bulk steps. |
| `site` | integer | `-1` | which of several structures built by one script the step serves. `-1` means "all" or "none" — a bulk step acts on every site at once. |

A present field of the wrong JSON type is rejected with a message naming the
step and the field, like any other malformed step.

**A step never states its method.** It used to; the kind is a fact about the
reaction, so it lives on the operation and the step is down to `op`, `t`, `r`,
`note`, `phase`, `layer` and `site`. A `method` key in a build file is ignored
like any other unknown key.

## Authoring a library

Everything above says what the fields *are*. What follows is what to put in
them, each rule with the reason it exists — the difference between a library
that loads and one the placement tool can offer correctly without being told
anything else.

1. **The origin convention, and `anchors`.** The atom the operation primarily
   acts on is **id 1, at the origin**, and it is the atom the user clicks. A
   reaction whose primary atoms are of *different* elements numbers them first
   and says `anchors: n`, so a click on any of them is admitted; a homonuclear
   pair needs no `anchors`, because a click on either already plays id 1. Frame
   atoms are never anchors — a click is a statement about where the reaction
   should happen, and a frame atom is one the reaction does not touch.
2. **Frame atoms: the first shell, and enough of them.** Name the host's bonded
   neighbours as `"*"` atoms present unchanged in both halves — none for an
   abstraction that places nothing, and **enough to span three dimensions**
   whenever `after` places anything off the host. Three coplanar atoms are
   congruent to their own mirror image, so a planar frame lets the fit place the
   reaction upside down, into the bulk; the loader warns about exactly that, and
   a fourth atom off the plane is the fix. Reaching *past* the first shell is
   the opposite error: the pattern then describes more of the surface than the
   reaction depends on, and fails to fit hosts it should serve.
3. **Bonds are complete.** List every bond among the atoms you name, in
   `before` and in `after` alike. An unlisted pair is not "nothing said" — it
   asserts there is no bond, and a workpiece that disagrees refuses the step.
4. **`deg` on every atom whose environment you mean.** The host always; a frame
   atom when the operation depends on it being, say, three-coordinate. The bond
   list speaks only about pairs *inside* the pattern, so `deg` is the only thing
   that tells a three-coordinate surface atom from a bulk atom with a fourth
   neighbour the pattern never names — which is the commonest way a library ends
   up offering a reaction that points into the crystal.
5. **Environment variants, not tolerance.** One operation per computed
   environment, named `<operation>_<environment>` with an environment vocabulary
   shared across families — the silicon library uses five across every donation
   family, `_dimer` (a reconstructed dimer atom), `_site` (an unstrained lattice
   site), `_site_relaxed` (a lattice site with a displaced neighbour), `_core`
   (bonded to the T-centre carbon) and `_edge` (a two-coordinate row end), so a
   reader learns it once and `si_donate_dimer` reads the same way as
   `cl_donate_dimer`. Every variant of a family places its added atom at
   *exactly* the same local position; they differ only in what they require.
   That is what makes a click work: you click a host, and the fit decides which
   variant applies, while the ones that do not are listed in the popup as near
   misses with how far off they are — which is also how a missing variant
   announces itself. The 0.05 Å default gate is what makes the variants
   resolvable; widening `tolerance` to cover a second environment merges two
   reactions that are not the same reaction, and which one gets offered is then
   decided by rounding.
6. **`chiral` when the reaction has a handedness.** Without it a mirrored
   placement is taken to be the same reaction seen from the other side, and the
   tool offers both.
7. **Donate-then-bridge is allowed.** Placing an atom at exactly the bond length
   from a neighbour a later `bridge` step will bond it to sits at about
   1.02–1.08 of the covalent-radius sum — above the 0.9 clash factor,
   comfortably. That margin is the reason not to write a library that goes
   lower.
8. **`clash` only with a stated reason.** If your chemistry genuinely places
   atoms closer than 0.9 of a bond length to atoms they do not bond to, say so
   in the library's `note`. Nothing known needs it.
9. **Tool frames: four non-coplanar handle atoms, apex at the origin, never the
   cargo.** The frame is the coordinate system every tool side of that type is
   placed in, so its atoms must survive every state the tool passes through. An
   atom the tool's own rewrite moves or deletes cannot be caught at load time;
   it shows up as a pose residual climbing on the next binding.
10. **Congruence.** A generated library's patterns should agree with the
    workpiece they were computed from to 1e-4 Å, and the generator should assert
    it. That is what lets the default gate be tight enough to tell environments
    apart, and what makes an authored step's residual chip mean something.
