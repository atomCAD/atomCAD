# Design: `mechanosynth` step metadata — the `step` record, layer tags and chapter navigation

Status: **implemented 2026-09-10** (all four phases). Extends the `mechanosynth`
node (`rust/crates/atomcad-crystolecule/src/mechanosynth/`,
`rust/crates/atomcad-structure-designer/src/nodes/mechanosynth.rs`,
`lib/structure_designer/node_data/mechanosynth_editor.dart`, reference guide
`doc/reference_guide/nodes/atomic.md` §mechanosynth). Nothing here changes
what a step *does* to the workpiece; it changes what the node *says* about
the step, to the network and to the user.

## Motivation

A build script today is a flat list of steps, each `op`, `t`, `r` and a
free-text `note`. Everything a generator knows about a step beyond its
geometry — which instrument performs it, which chapter of the process it
belongs to, which terrace it builds, which of several structures in an array
it serves — is either lost or buried in the note's prose, where the panel
can show it and nothing can act on it.

Three things want that information in structured form:

1. **The network.** A `switch` on "which instrument" can pick a style rule
   set per method, an `expr` can build a caption, an `if` can gate a branch.
   Downstream logic needs typed fields, not a string to parse.
2. **The viewport.** Scanning-probe work happens on one flat terrace at a
   time, so "the layer being built" is the natural unit of a build and
   deserves its own highlight, alongside the existing current-step highlight
   and a "built so far" highlight.
3. **The panel.** A 450-step script is scrubbed by chapter, not by step;
   the panel needs chapter boundaries it can jump to, derived from the file
   rather than copied by hand.

The node network language's own answer to "structured, typed, readable
downstream" is a record (`doc/design_record_types.md`). So the node gains a
second output pin carrying a **record describing the last step applied**, the
script format gains the fields that record is built from, and the replay
engine derives two more atom tags from them.

## Scope and non-goals

In scope: four new optional per-step fields in the build script; a built-in
named record type `MechanosynthStep`; a `step` output pin; two derived atom
tags; the panel showing the fields and navigating by chapter.

Out of scope, each a separate design if wanted: rendering a tool molecule
at the step (only the fields it would need are provided, see §Deferred);
showing the last good state on a match failure; editing steps in the panel;
any change to matching, tolerance or the operation library.

## The build script: four optional fields per step

A step may carry, beside `op`, `t`, `r` and `note`:

```json
{
  "op": "habst",
  "t": [12.71, 9.53, 8.02],
  "note": "L1 insert p=3 q=0: habst (C_a p=2 q=0)",
  "method": "probe",
  "phase": "layer1",
  "layer": 1,
  "site": 0
}
```

| field | JSON type | default when absent | meaning |
|---|---|---|---|
| `method` | string | `""` | which instrument or process performs the step — a positional tool, area lithography, a gas exposure, a bulk photochemical or thermal step. The generator chooses the vocabulary; the node never interprets it. |
| `phase` | string | `""` | the chapter of the process the step belongs to. Many steps, possibly of mixed methods. The unit of the panel's chapter navigation. |
| `layer` | integer | `-1` | the terrace the step builds, counted by the generator (e.g. `1` for the first new layer over the seed). `-1` means "no particular layer" — substrate work, bulk steps. |
| `site` | integer | `-1` | which of several structures built in one script the step serves. `-1` means "all" or "none" — a bulk step acts on every site at once. |

Validation in `parse.rs`: a present field of the wrong JSON type is an
`Invalid` error naming the step, like any other malformed step. Absent
fields take the defaults. Unknown keys stay ignored, as today, so a script
written for this version loads in the previous one and vice versa. The
`format` string does not change (`atomcad-msbuild/1`): the fields are
additive and optional.

`Step` in `schema.rs` gains the four fields with the same defaults, and
`Step::new` sets them.

The operation library is untouched. The method is a property of the step,
not of the operation: the same operation can be performed by lithography
in one phase and by a tool in another.

## The `MechanosynthStep` record type

A **built-in named record type**, registered in `node_type_registry.rs`
beside `MaterializeRegion` (`built_in_record_type_defs`), so it appears in
the `record_destructure` schema dropdown and is addressable as
`RecordType::Named("MechanosynthStep")` at validation time.

| field | type | value |
|---|---|---|
| `index` | Int | the number of steps applied, i.e. the kernel's clamp of the `step` property (`steps_applied`); `0` at the untouched base |
| `count` | Int | the script's step count |
| `op` | String | the last applied step's operation name; `""` at step 0 |
| `note` | String | its note; `""` if none or at step 0 |
| `method` | String | its `method` field |
| `phase` | String | its `phase` field |
| `layer` | Int | its `layer` field |
| `site` | Int | its `site` field |
| `t` | Vec3 | its placement point, in workpiece coordinates |

At step 0 the record is `{index: 0, count, op: "", note: "", method: "",
phase: "", layer: -1, site: -1, t: (0, 0, 0)}`. The record describes the
**last step applied**, matching the panel's "current step" wording.

Why fixed fields rather than file-declared ones: a pin's type must be known
at validation, and a type derived from the file's contents would change
whenever the file changed, disconnecting downstream wires exactly the way a
record field rename does. It would also be unknown on a text-format round
trip, where the node has no design directory to load the file from. A fixed
schema has neither problem, and a generator that needs another field asks for
it to be added here. Plain defaults instead of `Optional[T]` fields keep the
record usable in `expr` without unwrapping.

## The `step` output pin

`output_pins` becomes

```rust
vec![
    OutputPinDefinition::same_as_input("result", "base"),
    OutputPinDefinition::fixed("step", DataType::Record(RecordType::Named("MechanosynthStep".into()))),
]
```

The new pin is **appended** (pin 1), never inserted, so saved projects and
existing wires keep their pin indices (`doc/design_multi_output_pins.md`).
`default_display_all_output_pins` stays `false`: the primary output is the
workpiece and the record is an ordinary extra pin, shown on demand like
`structure_unpack`'s.

Evaluation: `eval` already replays for the primary output; the record is
built from the same script and the same `steps_applied` value, so no second
replay happens. When the primary output is an error (missing file, match
failure), the `step` pin is the same error — the two outputs are one
evaluation. A record with `index` reflecting a failed replay would be
misleading.

Hover values (`node_output_strings`) render the record like any other
record value; nothing special is needed.

## Atom tags derived from the step metadata

The engine currently paints one tag, `ms_current`, on the atoms the last
applied step touched. Two more, with the same lifecycle:

| tag | atoms |
|---|---|
| `ms_current` | unchanged: matched-and-kept, moved, replaced or added atoms of the last applied step, plus the surviving bonded neighbours of atoms it deleted |
| `ms_added` | every atom **created** by any applied step (1..index) that still exists |
| `ms_layer` | every atom created by an applied step whose `layer` equals the last applied step's `layer`, that still exists. Empty when that layer is `-1` |

"Created" is the added-ids part of `apply_step`'s touched list (the list is
kept-ids-then-added-ids in `after` order; the engine knows the split because
it builds the list). Membership is therefore defined by the **file's
metadata**, never by geometry: a substrate atom that a step *moved* is kept,
not created, so it stays out of the layer that moved it, which is correct —
it belongs to the layer below. A transient atom that a later step deleted no
longer exists and needs no bookkeeping; the set is filtered against the
workpiece before painting.

Engine change, `apply.rs`:

```rust
/// The atom tags `replay` paints, each optional. `None` everywhere paints nothing
/// and interns no tag name.
#[derive(Default, Clone, Copy)]
pub struct HighlightTags<'a> {
    pub current: Option<&'a str>,
    pub added: Option<&'a str>,
    pub layer: Option<&'a str>,
}

pub fn replay(base, library, script, step, tags: HighlightTags) -> Result<AtomicStructure, MechanosynthError>
```

`replay` clears all three tags from the base clone before applying anything
(the base may come from an upstream `mechanosynth` node), accumulates the
created ids per step during the loop it already runs, and paints at the end.
The active layer is read from step `index` first, so the loop can filter as
it goes. `add_atom_tag` may refuse when the 32-slot tag table is full; the
engine ignores that, as it does for `ms_current` today. Cost: one set insert
per created atom inside a loop that runs anyway.

The node passes `HighlightTags { current: Some("ms_current"), added:
Some("ms_added"), layer: Some("ms_layer") }`; the three names are `pub const`s
beside `MS_CURRENT_TAG`. All three are ordinary tags: `apply_style` colours
them, the tag panel lists them, a downstream node may clear or reuse them.
Cost to the user: three of the 32 slots instead of one.

The library's existing tests that assert "no tag is interned with `None`"
keep passing with `HighlightTags::default()`.

## API and panel

`APIMechanosynthInfo` (root crate, `mechanosynth_api.rs`) gains the record's
fields for the current step (`method`, `phase`, `layer`, `site`; `op`,
`note`, `applied`, `count` exist) **and the chapter list**:

```rust
pub struct APIMechanosynthChapter {
    pub phase: String,   // "" for steps with no phase
    pub layer: i32,
    pub first_step: i32, // 1-based index of the chapter's first step
    pub last_step: i32,
}
```

A chapter is a maximal run of consecutive steps with equal `(phase, layer)`.
Computed once per info call from the cached script; a 450-step script gives
a dozen chapters. Steps with empty phase and layer `-1` form chapters too
(shown as "untitled"), so the list always covers the whole script.

Panel (`mechanosynth_editor.dart`), additions under the existing step line:

- **Chips** for the current step's `method`, `phase`, `layer` and `site`,
  omitted when empty or `-1`. Purely informative.
- **Chapter list**: one row per chapter, `phase · layer N · steps a–b`,
  the current one highlighted. Clicking a row sets `step` to the chapter's
  **last** step (the chapter's finished state, which is what one wants to
  look at). A second control jumps to the chapter's first step for those
  who want to scrub through it.
- **Previous / next step buttons** beside the numeric step box; Left and
  Right arrow keys do the same while the step box has focus. Each press is
  one undo entry, like a slider release.
- **Tick marks** on the slider at chapter boundaries, when the script has
  more than one chapter.

All of these write `step` through the existing `setMechanosynthData` path
and follow the panel's existing rules: the panel shows `info.applied`, never
the raw `-1`, and never writes `-1` back. Wired `step` pin: the navigation
controls disable with the slider, as today.

## Text format and files

The text format is unaffected: `step` is already a property, the record pin
is addressed as `name.step` like any extra output pin, and the script fields
live in the JSON file, not in the network.

## Reference guide

`doc/reference_guide/nodes/atomic.md` §mechanosynth: an **Output pins**
list (`result`, `step` with the field table and the step-0 values), the two
new tags beside `ms_current`, the four script fields in the file-format
description, and the panel subsection's chips, chapter list and step buttons.
`doc/reference_guide/` wherever built-in record types are listed gains
`MechanosynthStep`.

## Tests

Crystolecule (`tests/crystolecule/mechanosynth_test.rs`): parse the four
fields with defaults and with wrong types; `ms_added` and `ms_layer`
membership on a synthetic three-step script where step 2 deletes an atom
step 1 added and step 3 moves a base atom (moved atom not in the layer,
deleted atom not in the set); tags cleared from a pre-tagged base;
`HighlightTags::default()` interns nothing. A new fixture with metadata
under `rust/tests/fixtures/mechanosynth/`.

Structure designer node tests: the `step` pin's record at step 0, at step
k, and at `-1`; error parity between the two pins.

API tests (`mechanosynth_api_test.rs`): chapter list for a script with
three chapters including an untitled one.

## Implementation phases

1. **Format and record.** `schema.rs` / `parse.rs` fields; the built-in
   record type; the `step` pin and its value in `eval`; node and parse tests.
   **Done.**
2. **Tags.** `HighlightTags`, the two derived tags, engine tests; node
   constants. **Done.**
3. **Panel.** Info struct fields and chapters, FRB regeneration, chips,
   chapter list, step buttons, ticks. **Done.**
4. **Guide.** The reference-guide updates above, with a screenshot slot.
   **Done.**

Phases 1 and 2 are independent of the panel and can ship first.

### What the implementation decided differently

- **`apply_step` returns a `StepEffect { touched, added }`** rather than a bare
  `Vec<u32>`. The design said the engine "knows the split because it builds the
  list"; making the split part of the return type is how `replay` gets at it
  without re-deriving it from `after`-pattern order.
- **The step buttons were already there.** `IntSpinField` grew hold-to-repeat
  `−` / `+` buttons and arrow-key stepping between this design being drafted and
  being implemented, and the step box composes it, so the panel needed no
  buttons of its own. The keys are **↑ / ↓**, not Left / Right, because that is
  now the app-wide convention for every integer box; a second convention for one
  field would be worse than the design's original wording.
- **The slider's ticks are a `SliderTickMarkShape`, not a strip.** Flutter
  *skips tick marks entirely* when they would be dense — which is exactly the
  450-step script the feature is for — so the shape reports a zero width to opt
  out of that gate and recovers each tick's step number by inverting Flutter's
  own placement formula. The alternative, a `CustomPaint` strip beneath the
  slider, would have to guess the track insets and would drift out of alignment
  with the thumb.
- **The chapter list and the ticks are hidden for a single-chapter script**, not
  just the ticks: a list of one row is navigation the slider already provides.

## Considered and rejected

- **A `tags: [String]` array per step.** Nothing in the network language
  can select on an element of a string array; a record has typed fields a
  `switch`, an `expr` and `record_destructure` can read directly.
- **File-declared record fields.** Rejected above: the pin type would
  depend on file contents.
- **Layer membership from coordinates.** The node knows nothing about
  lattices, and a terrace at a step edge is not a z band. The file's
  metadata is the only source that is right by construction.
- **A dedicated tool-molecule output pin.** Premature. With `op` and `t` in
  the record, a `switch` on `step.op` over imported molecules already picks
  the tool; only its orientation is missing (§Deferred).
- **Encoding phase or layer in the `note`.** That is the status quo, and
  the reason for this design.

## Deferred

- **`axis: Vec3`** in the record: the step's outward axis (the operation's
  local `+z` after `r`), enough to orient a tool molecule downstream. Added
  when a tool-rendering network exists to consume it.
- **Last-good-state on failure**: show the workpiece at step `k−1` with the
  error and a "jump to failing step" control. A change to the node's failure
  policy, designed separately.
- **A fading trail** over the last few steps: needs a per-atom scalar in
  styling, not a tag.
