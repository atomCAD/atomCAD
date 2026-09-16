# Design: muting operations in the `mechanosynth_edit` editor

A per-editor **mute set**: named operations of the wired library that the
offer sweep does not look at, so the popup answers with the part of the
library the author is actually working in.

Requested by mechadense after first use of the editor:

> turning off some options that one does not intend to use ATM like the
> precursor deposit pattern(s).

This extends `doc/design_mechanosynth_editor.md` (the node, the placement
tool, the offer popup) the way `doc/design_mechanosynth_step_metadata.md`
extends it — new stored state, new panel controls, one new seam in the
placement engine. Everything it does not mention is unchanged.

---

## Motivation

The editor's central decision is that **the operation count grows on
purpose**. `doc/design_mechanosynth_editor.md` §*The library answers "what can
be done here"* refuses to merge environment variants, refuses a `family` key,
and accepts `si_donate`, `si_donate_dimer`, `si_donate_site`,
`si_donate_site_relaxed`, `si_donate_core`, `si_donate_edge` as six honest
claims rather than one tolerant one. The atom-first inversion is what keeps
that invisible: click an atom and only the variants that *fit it* are offered,
so the list stays short even though the library is long.

That argument holds for **variants of one reaction** and does not hold for
**whole methods the author is not using right now**. The silicon T-centre
library v3 is nineteen operations across three instruments, plus one bulk
agent each for `Cl2` and `C2HCl3`. A user authoring the hydrogen-abstraction
phase by hand clicks a passivated surface atom and gets the `tip` rows they
want *and* the `bulk` chlorine rows, which fit almost everywhere because a
dose is not fussy about its site. Those rows are not wrong — the library
really can do that here — they are simply not this session's question, and
they are on every popup, every click, for the length of the phase.

Three things follow, and only the first is what was asked for:

- **The list is read on every click.** A popup is scanned, not searched;
  four irrelevant rows in a list of seven is most of the reading cost of the
  tool.
- **A bulk row is the easiest row to click by accident.** It fits, it is
  offerable, it commits, and it writes a step into the block that the author
  then has to notice and delete. The near-miss rule protects against placing
  a reaction the library has *not* computed; nothing protects against placing
  a reaction it has computed and the author did not mean.
- **The sweep costs one `place()` per library operation per click**
  (§*T1 — applicability*). Muting ten of nineteen halves it. This is a real
  saving but it is the third reason, not the first: the sweep is already
  cheap, and a design that muted for speed would mute the wrong things.

So: yes, worth building. With one constraint that decides most of the rest.

## The constraint: this is a view filter, and nothing else

A muted operation is **still in the library, still replays, still exports,
still means exactly what it meant.** Muting changes one thing: which
operations the *offer sweep* asks about. It does not change:

- the replay of the authored block, or of the `steps` prefix — a block
  containing a muted op replays normally, and must, or opening a project
  after muting would silently change its geometry;
- the `steps` or `scene` output pins, the `result` workpiece, any tag, any
  export;
- the `mechanosynth` replayer, which has no mute set and needs none;
- `mechanosynth_edit_choose`, which keeps its two refusals (near miss, tool
  not ready) and gains no third. A muted op is unreachable because it is not
  in the sweep, not because a second gate rejects it. One rule, one place.

This is the invariant to defend in review: **mute is asked about the sweep,
never about a step.**

## Scope and non-goals

In scope: a per-node mute set, its panel controls, a mute affordance on the
popup row, an escape hatch that sweeps the whole library for one anchor,
persistence, undo, the text format, the guide.

Not in scope:

- **A `family` or `group` key in the library schema.** Refused by the editor
  design and still refused; grouping in the panel is derived from fields the
  schema already has (§*Groups are derived, not declared*).
- **Muting in the library file.** A library is a generated artefact and a
  shared wire; what one author is not using this week is not a property of
  it.
- **A global or per-library-file preference.** See §*Considered and
  rejected*.
- **Muting tool types or feedstocks.** A tool that is wired is in the scene;
  removing it is a wire edit, which the network already expresses.

---

## What is stored

On `MechanosynthEditData`, beside `authored` and `cursor`:

```rust
/// Operation names the offer sweep does not ask about. **A view filter over
/// the wired library and nothing else** — a muted operation still replays,
/// still exports, and still means what it means; the sweep simply does not
/// look at it. Names the wired library does not define are inert, which is
/// what makes rewiring `ops` safe.
#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
pub muted: BTreeSet<String>,
```

Four decisions are packed into that.

**The muted set, never the enabled set.** An enabled set has to be
initialised from the library, which means the node needs a library before it
has any state at all, and it means an operation *added* to the library later
is silently absent from the popup — the file grew and the tool got quieter,
with no way to see why. A muted set defaults to empty, costs nothing on a
fresh node, and makes "new op in the library" mean "offered", which is the
only safe default for a coverage report.

**Names, not indices.** The `ops` pin can be rewired, and a generator rewrites
its library file constantly. A name the current library does not define is
kept and ignored: rewire back and the mute comes back with it. The panel shows
such a name in its list, greyed, with an *unknown to this library* note, so a
typo or a renamed op is visible rather than mysterious — and one click drops
it.

**`BTreeSet`, so the order is the name order.** The `.cnnd` diff and the text
dump then do not churn on set operations, which matters because a bulk mute
writes twelve names at once.

**Per node, not per library and not per user.** Two editors in one project can
be authoring two phases of one build with two different working sets, and that
is the normal shape of the mechanosynth demo networks — the mute belongs to
the authoring session the node *is*. It travels with the project, so a
colleague opening the file sees the list the author saw, and it is undoable,
which a preference is not.

## The engine seam

`applicable_ops` gains a sibling rather than a parameter:

```rust
/// [`applicable_ops`], restricted to the operations `admit` accepts.
///
/// The predicate is the caller's policy — the editor passes "not muted" —
/// and this crate deliberately does not know what a muted operation is. The
/// restriction happens **before** `place()` is called, so a filtered sweep
/// costs what its shorter library would.
pub fn applicable_ops_where(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    clicked: u32,
    tolerance: f64,
    bindings: Option<&[ToolBinding]>,
    admit: &dyn Fn(&Operation) -> bool,
) -> Vec<Applicability>;

pub fn applicable_ops(/* … */) -> Vec<Applicability> {
    applicable_ops_where(workpiece, library, clicked, tolerance, bindings, &|_| true)
}
```

A sibling and not a sixth argument because `applicable_ops` has about twenty
call sites in `mechanosynth_place_test.rs` and `mechanosynth_tools_test.rs`
that mean "the whole library", and rewriting them all to say `&|_| true` would
make the diff twenty times the size of the change. It also keeps
`atomcad-crystolecule` free of the word *muted*: the engine takes a predicate,
the policy lives upstairs in `mechanosynth_edit_ops.rs`, which is the split
the crate boundary already enforces everywhere else.

`mechanosynth_edit_offers` then reads:

```rust
let muted = data.muted.clone();          // cloned out before the &mut borrow
let admit = |op: &Operation| !muted.contains(&op.name);
let offers = applicable_ops_where(&scene.structure, &library, atom_id,
                                  resolve_tolerance(&library), bindings, &admit);
```

with `include_muted: true` passing `&|_| true` instead.

---

## The panel: the palette becomes the control

The editor panel already has a collapsed **Operations (19)** section that
`mechanosynth_edit_editor.dart` describes as "a **reference list**, not a
tool" — it lists names, it has a filter box, and clicking a name does nothing
since the armed mode was removed. This gives it its one job, and it is the
right home: the question *which operations am I working with* is a property of
the node, asked once per phase, and it belongs in the panel rather than in a
popup that exists for one click.

Header: `Operations (15 / 19)` when anything is muted, `Operations (19)`
otherwise, so the state is legible without expanding.

Expanded, three controls over one piece of state:

**1. A checkbox per row.** Checked = offered. Beside the name, the op's `note`
as it is now, plus a small **instrument** chip — `si_tool`, `Cl2`,
`spontaneous` — which is new information the panel has never shown and is
exactly what a user groups by.

**2. A row of instrument chips above the list, each tri-state.** Full, partial,
empty; tapping a full or partial chip mutes its whole group, tapping an empty
one unmutes it. For the silicon v3 library that row reads

```
w_probe 1 · c_tool 2 · si_tool 7 · C2HCl3 1 · Cl2 4 · spontaneous 4
```

and mechadense's ask is the `C2HCl3` chip, one click.

**3. Mute these / Unmute these, beside the filter box, acting on the filtered
set.** The filter box already exists and already does substring matching, so
typing `cl_donate` and clicking *Mute these (4)* is prefix-family muting
without a prefix-family concept. This is the general escape from any grouping
the chips do not express.

### Groups are derived, not declared

The chips read `Operation::method`, `Operation::tool.tool_type` and
`Operation::agent`, which the schema already has and which the schema's own
comment calls the instrument: *"one operation, one instrument"*, `tool_type`
for `tip`, `agent` for `bulk`. A `spontaneous` operation has neither, and its
chip is the method name. Nothing is added to the file format, nothing is
inferred from a name, and a library that does not use tools produces a chip
row of three methods — still useful, never wrong.

**Every group control writes individual names into `muted`.** There is no
"muted group" state, so a library that gains a twentieth operation in a group
the user muted last week offers it. That is the same choice as storing the
muted set rather than the enabled set, for the same reason, and it is the
behaviour to state in the guide: *muting is a list of operations, and the
chips are a fast way to edit that list.*

## The popup: one affordance, one line, one escape hatch

**Mute from the row.** The hovered or selected row grows a small eye-off
button, tooltip *Mute — hide `precursor_chemisorb` from offers*. This is the
moment the user notices the clutter, so it is the cheapest possible place to
act on it. It writes node data (one undo entry) and hides the row from the open
list **in place**; it does not re-sweep, because the remaining rows are still
correct — they were computed against the same workpiece a moment ago.
`placement.offers` keeps the muted row, which is harmless: nothing can call
`choose` on a row that is not drawn.

**A footer line whenever the mute set is non-empty**, below the list:

```
4 of 19 operations muted · show all here
```

This is the honesty requirement. The popup's promise (§*A miss becomes a
coverage report*) is that when nothing fits, the empty list is a statement
about the library. A silent filter would turn that into a lie, and an empty
popup with no explanation is the worst possible outcome of this feature. The
line says *I did not look at four of them*, always, whether or not any of them
would have fitted.

It deliberately does **not** say how many of the muted ops would have applied
here. Knowing that means running `place()` for them, which is the cost the mute
was avoiding.

**`show all here` re-sweeps this one anchor with `include_muted: true`.** One
extra sweep, only when asked, and the result replaces `placement.offers` — so
a muted row shown this way is fully live and can be committed, with a small
`muted` chip saying why it was not there before. That is the escape hatch, and
it is what lets the coverage report stay true: you can always ask the whole
library, at the cost of one click.

---

## API

`APIMechanosynthEditData.op_names: Vec<String>` becomes

```rust
pub struct APIMechanosynthOp {
    pub name: String,
    pub note: String,
    /// `tip` | `bulk` | `spontaneous`; empty for a muted name the wired
    /// library does not define.
    pub method: String,
    /// The tool type for a `tip` op, else empty.
    pub tool_type: String,
    /// The agent for a `bulk` op, else empty.
    pub agent: String,
    pub muted: bool,
}
pub ops: Vec<APIMechanosynthOp>,
```

built from the same `OpFacts` map `authored_view` already builds, plus the
node's set. Replacing rather than adding: `op_names` has one consumer, the
palette, and two lists that must stay aligned is a bug waiting to be written.

Muted names the wired library does not define are appended after the library's
own, with `method` empty — that is what the panel renders greyed.

Two entry points change or appear:

```rust
/// Mutes or unmutes `ops`, in one undo entry however many names it carries.
/// Names not in the wired library are stored anyway — the pin may be rewired.
#[frb(sync)]
pub fn set_mechanosynth_edit_muted(
    scope_path: Vec<u64>, node_id: u64, ops: Vec<String>, muted: bool,
) -> Option<String>;

/// `include_muted` sweeps the whole library for this anchor, ignoring the
/// node's mute set. The popup's *show all here*; every other caller passes
/// `false`.
#[frb(sync)]
pub fn mechanosynth_edit_offers(
    scope_path: Vec<u64>, node_id: u64, atom_id: u32, include_muted: bool,
) -> Result<APIMechanosynthOffers, String>;
```

The **domain** methods behind that flag are a named pair rather than one
`bool` parameter — `mechanosynth_edit_offers` and
`mechanosynth_edit_offers_including_muted` — because "the ordinary sweep" is
what thirty existing call sites mean, and a bare `false` at each of them says
less than the method name does. The boolean stops at the bridge, where Dart
needs one value rather than two entry points.

and `APIMechanosynthOffers` gains `muted_count: i32` — the size of the set the
sweep skipped, so the footer needs no second call — while `APIMechanosynthOffer`
gains `muted: bool`, true only on rows an `include_muted` sweep brought back.

One call for single and bulk on purpose: *Mute these (12)* is one user action
and must be one Ctrl+Z, and a loop of twelve single calls would be twelve
entries and twelve refreshes.

## Undo

A new `MechanosynthEditMuteCommand` in
`rust/crates/atomcad-structure-designer/src/undo/commands/`, shaped like
`MechanosynthEditBlockCommand` and storing `before`/`after` as whole sets.

**Separate from the block command, not an extension of it.** The block command
stores `(authored, cursor)` and is the single restore path for four editors of
that pair; muting touches neither, and folding a third field in would make
every block edit carry a copy of the mute set and every mute carry a copy of
the block. Two independent pieces of state, two commands.

Like the block command, `undo`/`redo` calls `placement.reset()` — the open
offer list was swept under the other mute set — and `invalidate_input_cache()`,
since it reaches into node data outside a refresh. It does **not** need to
invalidate anything downstream: mute changes no pin.

Muting sets the project dirty. It is persisted state, so per
`feedback_persisted_mutations_must_be_undoable` it is undoable; it is *not*
the cursor, whose exemption is that a scrub navigates within one evaluation
and stores nothing a colleague would notice.

## Text format and files

`.cnnd`: one new `muted` array on the node's data, omitted when empty, so every
existing project file is byte-identical.

The text format is the opposite: `get_text_properties` is **total** for this
node, and a property absent from the list is treated as wire-only and a literal
for it is silently dropped (`network_editor.rs` reads the names off the
*current* instance). So `muted` is emitted always, `[]` included, exactly as
`authored` is:

```
edit = mechanosynth_edit { base: slab, ops: lib, cursor: 2,
  muted: ["cl_donate_core", "precursor_chemisorb"],
  authored: [ … ] }
```

Two consequences to budget for: every text snapshot containing a
`mechanosynth_edit` node grows a `muted: []` line (`cargo insta review`), and
the round-trip corpus must be re-run — `query` → `--replace` stays a no-op
(`project_text_format_roundtrip`).

## Reference guide

`doc/reference_guide/nodes/atomic.md`, `mechanosynth_edit`:

- §*The panel* — the palette section rewritten: checkboxes, instrument chips,
  filter + bulk buttons, the `15 / 19` header.
- §*The offer popup* — the eye-off button, the muted footer, *show all here*.
- §*The text format* — the `muted` property.
- One sentence where it cannot be missed: **muting hides operations from the
  offer list; it never changes what a step does, what replays, or what
  exports.**

## Tests

Rust, in the owning crates' `tests/` directories:

- `applicable_ops_where` with an `admit` that rejects everything returns
  nothing; with one that accepts everything it equals `applicable_ops` row for
  row (the same shape as
  `applicable_ops_with_no_bindings_answers_exactly_what_it_did_before`).
- A muted op does not appear in `mechanosynth_edit_offers`; the same call with
  `include_muted: true` brings it back with `muted: true`.
- **A block containing a muted op replays unchanged** — the invariant test.
  Mute the op, re-evaluate, assert the `result` atoms and the `steps` output
  are identical.
- `choose` on an op that is muted but *is* in `placement.offers` (i.e. after a
  *show all here* sweep) succeeds. Mute adds no third refusal.
- Bulk mute of N names is one undo entry; undo restores the whole set.
- A muted name absent from the wired library survives a save/load round trip
  and appears in the API list with an empty `method`.
- `.cnnd` round trip with an empty set writes no `muted` key; text round trip
  writes `muted: []`.

Dart:

- Palette widget: header counts, checkbox toggles call the API once, chip
  tri-state for full/partial/empty groups, *Mute these* passes exactly the
  filtered names.
- Popup: footer appears only when `mutedCount > 0`; the eye-off button removes
  the row without a re-query; a row with `muted: true` renders its chip.

Smoke test: a pending **manual** step for the maintainer, per
`feedback_no_agent_smoke_tests`.

## Phases

1. **Kernel.** `applicable_ops_where`, `muted` on the node data, the mute ops
   on `StructureDesigner`, the undo command, serde, text format. Rust tests
   green. — **DONE**
2. **API + panel.** `APIMechanosynthOp`, the two entry points, codegen, the
   palette rewrite. This is the phase that closes mechadense's ask.
3. **Popup.** The eye-off button, the footer, *show all here*.
4. **Guide**, and the manual walkthrough handed to the maintainer.

Phases 1 and 2 are independently useful; 3 can be dropped or deferred without
leaving anything half-built, since the panel alone makes the feature complete.

---

## Considered and rejected

- **A global preference, or one keyed by the library file path.** "I never use
  precursor deposits" does feel like a fact about the user rather than about a
  node, and this was close. It loses on three counts: it is not undoable, it
  does not travel with the project (a colleague opening the file sees a
  different popup, which makes a shared design unreproducible), and it is wrong
  for the actual shape of these networks, where two editors author two phases
  with two working sets. If per-node muting turns out to be tedious to repeat,
  the cheap follow-up is *copy the mute set from another editor*, not a hidden
  global.
- **Muting in the library file.** A library is a generated artefact — the
  silicon generator rewrites its ops file on every run — so any state stored
  there is state the next generation destroys.
- **A `disabled: true` flag on `Operation` in the schema.** Same objection,
  plus it would mean the *replayer* has to decide what a disabled op does, and
  there is no good answer: refusing to replay it breaks old files, and
  replaying it anyway makes the flag a lie.
- **An `enabled` set instead of a `muted` set** — §*What is stored*.
- **A `family` key to group variants.** Refused by the editor design, and this
  feature does not need it: variants of one reaction are already collapsed by
  the fit, and what the user wants to mute is a *method*, which the schema
  already states.
- **Making `choose` refuse a muted op.** A second gate that can only fire after
  the first one was deliberately bypassed. It would make mute feel semantic,
  which is precisely what it must not be.
- **Counting how many muted ops would have applied at the anchor**, to say
  "2 muted operations apply here". Better information, and it costs exactly the
  sweep the mute exists to avoid. *show all here* buys the same answer on
  demand.
- **Hiding muted ops from the panel list too.** Then there is no way back.

## Open questions

- Whether the popup's *show all here* should stick for the rest of the session,
  or reset per anchor as designed. Per-anchor is the conservative choice; if
  users find themselves clicking it repeatedly, that is evidence the mute set
  is wrong, and stickiness would hide that evidence.
- Whether the instrument chip row should also offer a `method` row when a
  library's instruments are many — twelve chips is not a row. Deferred until a
  library has twelve.
