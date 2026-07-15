# Design: Atom tags — named per-atom groups

## Motivation

Users want per-atom visual customization: per-atom colors, per-atom choice of
ball-and-stick vs. space-filling display, and similar. Directly painting
visual properties onto individual atoms is brittle — the *selection* ("these
atoms") and the *styling* ("look like this") get welded together, and neither
can be reused or edited independently.

This design introduces one level of indirection: **tags**. A tag is a
user-chosen name attached to a set of atoms. Tags are **inert, durable
metadata** — they carry no behavior of their own. Downstream consumers
*interpret* tags: the planned visual-rules system (§Future work) maps
`element` / `(element, tag)` selectors to visual properties, and existing
region-gated nodes can later accept a tag as an alternative selector to a
`Blueprint` region.

This document specifies the tag core in detail — storage, diff semantics,
nodes, atom_edit integration, serialization — with an implementation plan.
The visual-rules system is deliberately **not** designed here; §Future work
records the ideas so they are not lost, to be turned into their own design
doc when the tag core has landed.

## Guiding principle: tags are selectors, never property carriers

Tags answer "*which atoms*", never "*what happens to them*". No semantic
property may *live in* a tag: "atoms tagged `frozen-zone` are frozen" is
forbidden by design. The frozen flag feeds relax/UFF and diff equivalence —
if it were tag-derived, renaming or removing a tag would have
action-at-a-distance physics consequences. Frozen stays a flag; transparency
stays `atom_alpha` in the decorator; future visual properties get their own
decorator fields. Only *selector positions* (rule matching, future tag pins)
ever read tags.

Corollary: the two existing per-atom property families are unaffected by this
design. Durable semantic state stays inline on `Atom` (flags); transient
display decoration stays in `AtomicStructureDecorator` (never serialized,
recomputed each evaluation). Tags join the first family.

## Module placement

Tags are a **crystolecule feature** with consequences in
**structure_designer**; the dependency direction is strictly the existing
downward one (structure_designer → crystolecule), and the boundary is:

- **`crystolecule` owns the entire tag data model** — `Atom.tag_bits`, the
  `tag_names` table, every accessor in §Accessors, `TagError`
  (`thiserror`), the merge/weld semantics, and the name-canonical
  `extract_diff` / `apply_diff` logic. Per `crystolecule/AGENTS.md`, the
  module stays independent of `structure_designer`, `display`, and
  `renderer` — nothing here knows about nodes, regions, undo, or files.
  In particular crystolecule stays **serde-free**: do *not* derive
  `Serialize` on `tag_names`/`tag_bits`; persistence is structure_designer's
  job (below).
- **`structure_designer` consumes the primitives** and adds every
  application-level concern:
  - the `tag`/`untag` nodes (`map_atomic_in_region` + accessors);
  - atom_edit's `*_recorded` ops and name-based undo deltas — *recording*
    is a structure_designer concept; the crystolecule accessors are dumb
    mutators with no history;
  - persistence: `SerializableAtom.tags` lives in
    `structure_designer/serialization/atom_edit_data_serialization.rs`,
    exactly like the inline flags today — the serialized form is owned by
    the node's saver, not by the domain type;
  - error translation: `TagError` crosses the boundary as a value and
    becomes a localized `NetworkResult::Error` at node eval / an
    `APIResult` at the atom_edit API — crystolecule never formats
    user-facing errors.
  - Base-atom nuance: the *workflow* that promotes a tagged base atom to a
    full diff override (§Diff semantics) is atom_edit's; the promoted diff
    atom's tag storage is crystolecule like any atom's.
- **`api`** adds the FRB surface (node-data getters, atom_edit actions,
  `APIHoveredAtomInfo.tags`); **`display`/`renderer` are untouched** by the
  tag core — they enter only with the future visual-rules work, which reads
  tags through the same crystolecule accessors.

The phases align with this layering by construction: Phases 1–2 are
crystolecule work (tests in `rust/tests/crystolecule/`, reviewable with no
node-network context), plus — in Phase 1 only — the mechanical
structure_designer caller updates forced by the `add_atomic_structure`
signature change (§Maintenance lists all five); Phases 3–4 are pure
structure_designer, Phase 5 api/Flutter. No new inter-module dependency
edges appear anywhere in the plan.

## Data model

### Representation: interned name table + per-atom bitmask

```rust
// AtomicStructure gains:
/// Interned tag names. Index = bit position in `Atom.tag_bits`.
/// A slot's name is stable for the structure's lifetime unless the slot is
/// reclaimed (see intern_tag). Max 32 entries.
tag_names: Vec<String>,

// Atom gains:
/// Tag membership bitmask. Bit i set ⇔ this atom carries the tag
/// `structure.tag_names[i]`. Durable state (survives diff extraction),
/// like the DURABLE_FLAGS_MASK bits of `flags`.
pub tag_bits: u32,
```

An atom "has tag `name`" iff `tag_names` contains `name` at index `i` and bit
`i` of `tag_bits` is set. Tag names are case-sensitive, compared exactly,
trimmed of surrounding whitespace at every entry point, and must be non-empty
after trimming.

**Bit indices are per-structure.** The same tag name can sit at different bit
positions in two structures. Every cross-structure operation (merge, diff
extraction, diff application) therefore works at the **name** level and
translates masks through the tables — never compares raw bits across
structures. This is the single most important invariant of the design;
§Diff semantics and §Maintenance spell out each site.

### Size analysis

Current `Atom` (glam `DVec3` 24 B; `SmallVec<[InlineBond; 4]>` = 8 B capacity
+ 16 B inline/heap union = 24 B; `id` 4 B; `in_crystal_depth` 4 B;
`atomic_number` 2 B; `flags` 2 B) has a 60-byte payload in an align-8 struct:
**64 bytes total, with 4 bytes of padding**. Adding `tag_bits: u32` consumes
exactly that padding — `Atom` **stays 64 bytes** (one cache line). A 1M-atom
structure pays zero additional memory for tags.

### Alternatives considered

- **`HashMap<String, HashSet<u32>>` on `AtomicStructure`** (tag → atom-id
  set) — rejected. External id-keyed maps must be manually maintained by
  every path that touches atoms: `atom_union`/weld id remapping, every
  delete path, and — fatally — diff extraction, which compares `Atom`
  structs by id and would be blind to tag changes unless separately taught
  about the maps. Each is a forgettable chore whose failure mode is silent
  data loss. This is the same lesson that moved atom_edit's
  frozen/hybridization metadata from external maps to inline `Atom.flags`
  (see `atom_edit_data_serialization.rs` — the old map fields survive only
  as backward-compat readers). The tag → id-set view is still useful; it is
  provided as a *derived query* (`atoms_with_tag`), not the source of truth.
- **`SmallVec<[u16; 2]>` of tag indices on `Atom`** — rejected. A `SmallVec`
  is never smaller than 24 bytes (8 B capacity + 16 B union), so every
  untagged atom would pay 24 dead bytes and `Atom` grows to 88 bytes
  (+37.5%), off the cache line. Iteration-heavy code (tessellation, relax,
  lattice fill) would feel it.
- **Free bits of `Atom.flags`** — rejected. Only 9 bits are free (0–6
  assigned), tags would compete with future semantic flags for a budget
  that is being consumed steadily (patch-ghost is recent), and 9 names is
  low enough that real projects would hit it. The separate `u32` is free
  anyway.
- **`Option<Box<[u16]>>` on `Atom`** (unlimited tags) — deferred. Costs +8
  bytes (72 B, +12.5%) for a limit nobody has hit yet. This is the
  documented widening path if the 32-tag limit ever binds in practice
  (§Limit); switching representations is mechanical because all access goes
  through the accessors below.

### The 32-tag limit

A structure supports at most **32 distinct tag names**. Every creation
point that can hit the limit ends in a **localized, user-visible error** —
never a panic, never an *unreported* drop. Two mechanisms deliver that,
chosen by what the call site can plumb:

- `tag` node eval → `NetworkResult::Error("tag limit (32 names) reached")`
  on the node.
- atom_edit tag action → error surfaced in the UI, edit not applied.
- `add_atomic_structure` (the merge behind `atom_union` etc.) interns the
  source's names too and can hit the same limit → the call is fallible and
  each caller surfaces the error (§Maintenance lists all five callers).
- Diff application/composition (`apply_diff` / `compose_diffs`) stays
  **infallible**: names that no longer fit are dropped *and reported* via
  `DiffStats.dropped_tag_names` (the existing `orphaned_*` soft-failure
  pattern), and the consuming node — atom_edit, `apply_diff`,
  `atom_composediff` — turns a non-empty report into a localized
  `NetworkResult::Error` naming the dropped tags (§Diff semantics). The
  user sees a localized error either way; the split is plumbing, not
  policy: `apply_diff` runs on interactive hot paths (atom_edit eval,
  continuous-minimization write-back) where a `Result` return would ripple
  through drag-interaction code for no UX gain.

`intern_tag` mitigates before failing: if the table is full, it first looks
for a **reclaimable slot** — the lowest bit that no atom in the structure
carries — and reuses that slot for the new name (the old name is simply
forgotten; no atom referenced it). Only when all 32 bits are live on at
least one atom does interning fail. No background GC, no compaction pass, no
mask rewriting — slot reuse never touches `Atom.tag_bits`.

If the limit ever proves too tight, the widening path is `u32 → u64`
(+8 bytes, 72-byte `Atom`) with no semantic change — a decision better made
later, with evidence, than pre-paid now.

### Accessors on `AtomicStructure`

All tag access goes through these (no direct `tag_bits` fiddling outside
`atomic_structure/`), so a future representation change stays local:

```rust
/// thiserror; two variants: LimitReached (all 32 slots live) and
/// EmptyName (name empty after trimming).
pub enum TagError { ... }

/// Look up or create the bit index for `name` (trimmed). Reuses a dead slot
/// when full (see §Limit). Err(LimitReached) when 32 live names exist;
/// Err(EmptyName) on an empty/whitespace name.
pub fn intern_tag(&mut self, name: &str) -> Result<u8, TagError>;
/// Bit index of `name` if interned (does not create).
pub fn tag_index(&self, name: &str) -> Option<u8>;
pub fn add_atom_tag(&mut self, atom_id: u32, name: &str) -> Result<(), TagError>;
pub fn remove_atom_tag(&mut self, atom_id: u32, name: &str); // absent name/tag = no-op
pub fn clear_atom_tags(&mut self, atom_id: u32);
pub fn atom_has_tag(&self, atom_id: u32, name: &str) -> bool;
/// Names of the tags this atom carries, in bit order.
pub fn atom_tags(&self, atom_id: u32) -> Vec<&str>;
/// Derived query: ids of all atoms carrying `name`. O(atoms).
pub fn atoms_with_tag(&self, name: &str) -> Vec<u32>;
/// The interned tag table (bit order), for serialization and UI. May
/// include dead names — interned but currently carried by no atom
/// (candidates for slot reclamation, see §Limit).
pub fn tag_names(&self) -> &[String];

/// Cross-structure mask translation (`[Option<u8>; 32]`, source bit →
/// target bit), built ONCE per (source → target) pair and passed to every
/// per-atom copy. This type is the compiler-enforced carrier of the
/// per-structure bit-index invariant: any code path that moves atoms
/// between structures must obtain one, so a raw cross-table bit copy
/// cannot happen by omission. `TagRemap::identity()` covers same-structure
/// copies (mask passes through unchanged).
pub struct TagRemap { ... }

/// Interns each of `source`'s *live* tag names (names carried by at least
/// one atom — one O(atoms) OR-sweep computes the live mask) into `self`
/// and returns the translation. Names that no longer fit are left unmapped
/// and returned in the dropped list — never a panic, never silent. When
/// the two tables are already identical the result is the identity remap
/// (cheap common case: clone-mutate diffs, repeated merges of siblings).
///
/// Merging is **append-or-drop; it does NOT reclaim dead slots** (unlike the
/// single-structure `intern_tag`). This is load-bearing for the multi-source
/// builds below: `apply_diff` grows the result table across two calls
/// (base→result, then diff→result) *before adding any atom*, and
/// `compose_two_diffs` does the same (diff1→composed, diff2→composed). Every
/// name a prior call committed is a real union member that no atom carries
/// *yet*; if `build_tag_remap` reclaimed on a full table it would treat that
/// name as a dead slot and silently overwrite it (corrupting the earlier
/// remap and the 32-limit drop). So the budget counts every distinct name,
/// including any the target already holds unused.
pub fn build_tag_remap(&mut self, source: &AtomicStructure)
    -> (TagRemap, Vec<String> /* dropped */);

/// Translate a mask through the remap; unmapped bits are dropped (their
/// names are the ones reported by build_tag_remap).
pub fn remap_tag_bits(bits: u32, remap: &TagRemap) -> u32;
```

### Maintenance touchpoints

- **Clone** — free. `tag_bits` is a plain field; `tag_names` clones with the
  structure. This covers the clone-mutate idiom every atom-op node uses.
- **Delete atom** — free. Tags travel with the atom; no external map to
  clean (this is the point of the inline representation).
- **`add_atomic_structure` (the merge used by `atom_union`)** — union at
  the name level via `build_tag_remap` (intern the source's live names into
  the target, remap each incoming atom's mask), next to the existing
  bond/selection remapping site in `atomic_structure/mod.rs`. A non-empty
  `dropped` list fails the call: `add_atomic_structure` becomes
  **fallible** (`Result<FxHashMap<u32, u32>, TagError>`) — an intentional
  signature change. Its five callers all have clean error channels:
  - `nodes/atom_union.rs` and the two `nodes/patch_latticefill.rs` sites →
    localized `NetworkResult::Error` on the node;
  - the `Array[HasAtoms]` display-union in
    `evaluator/network_evaluator.rs` (merges array elements into one
    viewport output) → return `NodeOutput::None` and record a node error
    in the evaluation context;
  - `StructureDesigner::export_visible_atomic_structures` → already
    returns `Result<(), String>`; the tag error becomes the export-failure
    message.
- **`weld_coincident_atoms`** — the surviving atom's mask becomes the OR of
  the fused atoms' masks. Weld runs within one structure (post-merge), so
  masks share one table and OR is exact.
- **Structure-rebuilding nodes** (`materialize`, `patch_latticefill`'s
  lattice fill) — atoms created from geometry are untagged by construction.
  This is semantically correct (there was no atom to carry a tag), not a
  caveat: tags are applied to *atoms*, so tagging happens after
  materialization. (`patch_latticefill`'s tile atoms are cloned from the
  patch tile Molecule, so tags on the tile flow onto every placed copy —
  a feature: tag the tile once, every placement is tagged.)

## Diff semantics — tags are durable

Tags participate in diff extraction and application exactly like the
`DURABLE_FLAGS_MASK` bits of `flags` (and unlike the transient selected /
display-ghost bits). All comparisons are **by name**, per the per-structure
bit-index invariant.

### `extract_diff(before, after, ε)` (`atomic_structure_diff.rs`)

- **Change detection:** an atom counts as modified when its tag *name set*
  differs between `before` and `after` — alongside the existing
  position/element/durable-flags checks (~line 924). Fast path: when the two
  structures' `tag_names` tables are identical (the overwhelmingly common
  clone-mutate case where the mutation didn't intern anything new), a raw
  `tag_bits` compare is exact and the name-set path is skipped.
- **Recording:** a modified/added diff atom carries the `after` atom's tags,
  interned into the *diff structure's own* table (the diff is an
  `AtomicStructure` and has one like any other) — parallel to the existing
  `diff.set_atom_flags(diff_id, a.flags & DURABLE_FLAGS_MASK)` site.

Consequence for the existing diff output pins (`relax`, movement nodes,
`atom_replace`, `atom_cut`): a node that doesn't touch tags produces no tag
noise in its diff — equal name sets are not a modification.

### `apply_diff` / `compose_diffs`

`apply_diff` does not mutate the base — it builds a **fresh result
structure**, re-adding base pass-through atoms and diff atoms alike
(`atomic_structure_diff.rs`), with per-atom metadata flowing through the
single seam **`copy_atom_metadata`** (`atomic_structure/mod.rs`; all its
call sites are in the diff file). That seam is where tags must be handled,
and it must be impossible to forget: `copy_atom_metadata` gains a
**`&TagRemap` parameter** and translates `tag_bits` through it, so every
existing and future call site is forced by the signature to say which
translation applies. `apply_diff` builds its two remaps up front — intern
the base's live names into the (empty) result table, then the diff's; base
pass-throughs copy through the base→result remap, diff atoms through the
diff→result remap. (Within-structure copies, if any ever appear, pass the
identity remap.)

The combined interning can exceed 32 live names. `apply_diff` stays
**infallible** (see §Limit for why): un-internable names are dropped from
the result and reported in a new `DiffStats.dropped_tag_names: Vec<String>`
— the same drop-and-report convention as the existing
`orphaned_tracked_atoms` / `orphaned_bonds` counters. (`compose_two_diffs`
reports identically through `DiffCompositionResult`.) Consumers (atom_edit
eval, the `apply_diff` and `atom_composediff` nodes) check the report and
surface a localized error naming the dropped tags; all other tags apply
correctly regardless.

**Composition is last-writer-wins, not union.** `compose_two_diffs` (the
pairwise workhorse behind `compose_diffs`) already picks one source atom's
metadata per case (the diff2 entry where both diffs touch an atom — see
its `copy_atom_metadata(cid, diff2_atom)` arms);
tags follow the same rule through the same calls: the composed atom carries
the winning diff atom's tag set, translated by name into the composed
diff's table. A name-set *union* would be wrong — since a modified diff
atom carries its full replacement tag set, unioning would resurrect a tag
that diff1 set and diff2 removed, and composition would stop agreeing with
sequential application. Only the *tables* union (by interning); the
per-atom masks are copied from exactly one source each.

### atom_edit base atoms: full promotion (#386 policy)

Tagging a **base** atom inside atom_edit is an *explicit per-atom edit*, so
it follows the base-anchor override policy (issue #386) exactly as the
frozen / hybridization flags do today: the base atom is **promoted to a
full diff override** — a real element+position diff atom, anchored, base
metadata copied via the `promote_base_atom_metadata` path (extended to also
carry the base atom's current tag *names* into the diff's table) — and the
tag change is then applied to that diff atom. Like every full override,
this pins the atom's element/position/tag set at promotion time, so a later
upstream change to that atom is overridden; that is the existing, accepted
tradeoff for explicit per-atom edits (markers exist only for *adjacency* —
pure bond-endpoint references).

UNCHANGED markers **never carry tags** (`tag_bits` stays 0 on them). This
is forced, not a style choice, for the same two reasons flag deltas don't
ride markers today: (1) `apply_diff`'s marker arm passes the matched base
atom through with the *base* atom's metadata and ignores the marker's own —
a tag stored on a marker would be silently dropped (this is exactly why a
non-Auto toolbar hybridization on an anchor falls back to full promotion,
per `atom_edit/AGENTS.md`); (2) bond tools and `extract_diff` mint bare
endpoint markers with `tag_bits == 0`, so a marker mask would be ambiguous
between "no tag change" and "remove all tags". Marker tag-freedom is an
invariant with a test (Phase 2/4), not a future extension point.

## Nodes: `tag` / `untag`

Both live in `rust/src/structure_designer/nodes/tag.rs` (one module, two
node types — the `freeze.rs` pattern). Both are `HasAtoms`-polymorphic,
metadata-only pass-throughs in the `freeze`/`unfreeze`/`xray` family, built
on `evaluator::atom_op::map_atomic_in_region` with the standard
`DEFAULT_REGION_MARGIN` membership test. Multiple regions = chained nodes.
No `diff` output pin (consistent with `freeze`/`unfreeze` being deferred
from `doc/design_diff_outputs_for_atom_ops.md`).

| | `tag` | `untag` |
|---|---|---|
| **Category** | `AtomicStructure` | `AtomicStructure` |
| **Pin 0** | `molecule: HasAtoms` — required | same |
| **Pin 1** | `name: String` — optional; wired overrides stored property | same |
| **Pin 2** | `region: Blueprint` — optional, last pin (Part A convention) | same |
| **Output** | `single_same_as("molecule")` | same |
| **Property** | `name: String`, default `"tag"` | `name: String`, default `""` |
| **Semantics** | add `name` to every in-region atom (all atoms when `region` disconnected) | remove `name` from in-region atoms; **empty `name` removes all tags** from in-region atoms (the blanket-restore analog of `unfreeze` / `xray` α = 1.0) |
| **Errors** | empty/whitespace name → localized `NetworkResult::Error`; 32-name limit → localized error | none beyond input-type errors (absent name/tag = no-op by design) |
| **Subtitle** | the tag name (hidden while pin 1 is wired) | the name, or `all tags` when empty |

Both implement `get_text_properties` / `set_text_properties` for `name`
(`TextValue::String`) and `get_parameter_metadata` (`molecule` required,
`name`/`region` optional).

Chaining composes naturally: tags accumulate per-atom (a `tag "a"` followed
by `tag "b"` leaves both on the overlap); `untag` is the inverse. Re-tagging
an already-tagged atom is a no-op, so the nodes are idempotent.

### Existing-names suggestions (property editor dropdown)

The Flutter editors offer the **input structure's** tag names as
suggestions, sourced from the evaluated `molecule` input via the
established eval-cache pattern (`MaterializeData::available_parameters`,
also used by `motif_sub` and `patch_latticefill`'s `CompatibilityReport`):

```rust
// on TagData / UntagData:
#[serde(skip)]
pub available_tags: RefCell<Vec<String>>, // input's tag_names(), captured in eval()
```

`eval()` snapshots `input.tag_names()` into the cell before mutating; the
node-data getter exposes it to Flutter alongside the stored `name`.
Inherent caveats of the pattern (same as materialize's): the list is
populated only after an evaluation — empty while the input is unwired, the
upstream cone errors, or the node hasn't evaluated yet — and it is a
snapshot of the *last* eval. That is fine because the list is purely a
suggestion source: the name field stays **free text** (combo box). For
`tag` this is essential — its whole job is usually introducing a *new*
name, and a sibling structure's tags legitimately aren't in this input's
table. For `untag`, a typed name absent from the input is not an error
either (networks are parametric; the input can change), it just currently
removes nothing.

atom_edit's tag picker uses the same idea against its own eval cache (base
structure + diff tag names).

## atom_edit integration

atom_edit gains **Tag…** / **Untag…** actions on the current atom selection
(name entered/picked in the UI; the picker offers the structure's existing
tag names to reduce typo risk):

- Implemented as `add_tag_recorded(name)` / `remove_tag_recorded(name)` /
  `clear_tags_recorded()` on `AtomEditData`, wrapped in `with_atom_edit_undo`
  like every other recorded mutation, extending the existing delta type the
  way `set_flags_recorded` does. **Deltas record tag *names* (old/new name
  set per touched atom), never raw `tag_bits`.** Raw-bit deltas are a trap:
  slot reclamation (§Limit) can re-assign a bit's meaning between recording
  and replay, so a bit-based undo would silently restore a *different* tag
  unless every table mutation (including reclaim-renames) were also
  delta-recorded exactly. Name-based deltas are self-describing — applying
  one interns the name and is correct whichever bit it lands on.
- Diff atoms take the tag directly; base atoms are promoted to full
  overrides first (§Diff semantics), with the base atom's existing tag
  names copied onto the new diff atom, so the recorded delta is a clean
  old-name-set → new-name-set pair.

## Seeing tags: the atom hover popup

Tags are invisible in the viewport by design (no consumer renders them until
the visual-rules system exists), so without an inspection surface the feature
would be un-debuggable: there would be no way to tell which atom carries
which tags. The **atom hover info popup is that surface, and it is a required
part of this design**, not an optional nicety.

The seam already exists: `query_hovered_atom_info`
(`api/structure_designer/structure_designer_api.rs`) hit-tests **all
displayed atomic structures** — any node's output, not just atom_edit — and
returns `APIHoveredAtomInfo`, which already carries the analogous per-atom
metadata (`is_frozen`, `hybridization_override`). It gains:

```rust
/// Names of the tags the hovered atom carries, in bit order.
/// Empty for untagged atoms.
pub tags: Vec<String>,
```

populated via `structure.atom_tags(atom_id)`. The Flutter hover popup renders
the list on its own row (e.g. `Tags: surface, active-site`), omitted entirely
when empty so untagged hovers look exactly as they do today. This works
uniformly for tags applied by the `tag` node, by atom_edit, or arriving
through diffs/merges.

## Serialization

`AtomicStructure` values are never serde-serialized in general (results are
recomputed from the network), so the only persistence surface is atom_edit's
diff — `atom_edit_data_serialization.rs`:

- `SerializableAtom` gains
  `#[serde(default, skip_serializing_if = "Vec::is_empty")] tags: Vec<String>`
  — **names, not bits**, so the file is self-describing, human-readable, and
  independent of table order; the diff structure's table is rebuilt by
  interning on load. Old files deserialize with no tags (serde default);
  new files without tags are byte-identical to today's output
  (`skip_serializing_if`). **No `.cnnd` version bump, no migration pass.**
- The edit_atom (legacy) node is untouched — it predates this feature and
  gets no tag actions.
- Export formats (`.xyz`, `.mol`) do not carry tags — out of scope
  (§Out of scope).

## Phases

Each phase lands green on `cargo fmt && cargo clippy && cargo test -j 4`
(and `flutter analyze` where Dart is touched) with the automated tests
listed.

---

### Phase 1 — Core storage + maintenance

**Implementation**

- `Atom.tag_bits: u32` (init 0; note the struct-size invariant in a comment:
  the field lives in previously-padding bytes, `Atom` stays 64 B).
- `AtomicStructure.tag_names: Vec<String>` + the full accessor set
  (§Accessors), including slot reclamation in `intern_tag` and the
  `TagError` type (`thiserror`).
- `TagRemap` + `build_tag_remap` + `remap_tag_bits` (§Accessors), including
  the identical-tables identity fast path and live-name detection.
- `add_atomic_structure`: name-level union via `build_tag_remap` (fallible
  — §Maintenance), next to the existing bond/selection remap; all five
  callers updated per the list there.
- `weld_coincident_atoms`: OR the fused atoms' masks.

**Automated tests** — `rust/tests/crystolecule/atomic_structure_test.rs`
(existing file) or a new `atom_tags_test.rs` registered in
`tests/crystolecule.rs`:

- add/remove/has/clear round-trips; `atom_tags` returns names in bit order;
  `atoms_with_tag` finds exactly the tagged ids; names trimmed; empty name
  rejected.
- Interning is idempotent (same name → same bit); 33rd *live* name errors;
  a dead slot (name whose bit no atom carries) is reclaimed instead of
  erroring, and the reclaimed slot's old name is gone.
- `add_atomic_structure` with disjoint, overlapping, and
  colliding-at-different-bits tag tables: tags land on the remapped ids
  under the target's table; combined table over 32 live names → error.
- `weld_coincident_atoms`: fusing a tagged with an untagged atom, and two
  atoms with different tags, leaves the survivor with the OR of the masks
  (the implementation is one line; the test guards that it doesn't get
  forgotten — weld currently has no reason to touch `tag_bits`).
- Clone preserves tags; deleting a tagged atom leaves other atoms' tags
  intact.
- `size_of::<Atom>() == 64` guard test — locks the padding claim.

**Manual verification** — none possible (no user-visible surface yet); the
unit tests are the coverage.

---

### Phase 2 — Diff integration

**Implementation**

- `extract_diff`: tag name-set change ⇒ modified (with the identical-tables
  `tag_bits` fast path); record `after` tags on diff atoms via interning
  into the diff's table. (The diff table starts empty and one structure has
  at most 32 live names, so this interning can never overflow.) The lazily
  minted UNCHANGED endpoint markers stay tag-free (§Diff semantics).
- `copy_atom_metadata` gains the `&TagRemap` parameter; `apply_diff` /
  `compose_diffs` build their remaps up front, translate both base
  pass-throughs and diff atoms, and report un-internable names in the new
  `DiffStats.dropped_tag_names` (§Diff semantics). Composition stays
  last-writer-wins per atom — tags ride the existing per-case
  `copy_atom_metadata` choice, never a mask union.

**Automated tests** — the existing diff test files under
`rust/tests/crystolecule/` (extend where `extract_diff` /
`apply_diff` are covered today):

- Tag added / removed / unchanged between before/after ⇒ modified /
  modified / not-modified respectively; a position-only change carries the
  atom's tags onto the diff atom; an **added** atom's tags land on its diff
  atom.
- before/after with *different tables but equal name sets* per atom ⇒ not
  modified (exercises the name-canonical path past the fast path).
- **The fast-path trap in the dangerous direction:** before/after with
  *equal `tag_bits` but different tables* (bit 0 = `"a"` in before, `"b"`
  in after) ⇒ **modified**. A naive implementation that bit-compares
  without first checking table equality passes every other test in this
  list and silently misses this one.
- ε-pruning (`diff_min_move`-style `extract_diff(_, _, ε)`) does **not**
  drop an atom whose movement is below ε but whose tag set changed — a
  tag-only change is a modification regardless of distance.
- Base pass-through and matched atoms keep their tags through `apply_diff`
  when base and diff assign the same names different bits (exercises the
  base→result remap through `copy_atom_metadata` — the test that fails if a
  call site ever copies raw bits).
- `apply_diff` whose combined live names exceed 32 ⇒ the un-internable
  names land in `DiffStats.dropped_tag_names` (no panic, no *silent* drop —
  the report is the surfacing hook for Phase 4's node errors), every other
  tag applies correctly, base structure untouched.
- A tag stored on an UNCHANGED marker (constructed directly in the test) is
  ignored by `apply_diff` — locks the "markers never carry tags" invariant
  from the apply side.
- Round-trip: `apply_diff(base, extract_diff(base, edited))` reproduces
  `edited`'s tags, including when base's table assigns different bits.
- Compose: two diffs touching tags compose to the same result as sequential
  application — including the last-writer case: diff1's entry for an atom
  carries tag `a`, diff2's entry for the same atom does not ⇒ the composed
  diff drops `a` (a name-set union would resurrect it).
- `relax`-style no-tag-change mutation produces a diff without tag noise.

**Manual verification** — none (backend only).

---

### Phase 3 — `tag` / `untag` nodes

**Implementation**

- `nodes/tag.rs`: both node types per §Nodes, modeled on `freeze.rs` (region
  plumbing) + `xray.rs` (wired-overrides-stored property, subtitle, text
  properties), including the `#[serde(skip)] available_tags: RefCell<_>`
  eval cache (§Existing-names suggestions — populated here, consumed by the
  Phase 5 editor). Register in `nodes/mod.rs` + `node_type_registry.rs`.

**Automated tests** — new `rust/tests/structure_designer/tag_test.rs`
(registered in `tests/structure_designer.rs`), modeled on `freeze_test.rs`:

- No region → every atom tagged; with region → only in-region atoms.
- Wired `name` pin overrides the stored property.
- Chained `tag "a"` → `tag "b"` accumulates; `untag "a"` removes only `a`;
  `untag ""` clears all tags in-region and leaves out-of-region tags alone.
- Empty name on `tag` → localized error; 33 distinct live names through
  chained `tag` nodes → localized error on the offending node; upstream
  nodes unaffected (non-blocking by the litmus test — eval already
  localizes the failure).
- Concrete phase flows through (Crystal→Crystal, Molecule→Molecule);
  non-Blueprint `region` → localized error.
- Text-format round-trip of the `name` property, including exotic names
  (spaces, quotes, non-ASCII) — tag names are the first free-form
  user-authored `TextValue::String` node property that routinely reaches
  the text-format serializer, so its quoting/escaping gets exercised here.
- End-to-end `atom_union` integration: two molecules tagged by separate
  `tag` nodes (same name in one case, different names at different bit
  positions in another) wired into `atom_union` → the union carries the
  correct names on the correct atoms (guards the node-level path through
  `add_atomic_structure`, not just the crystolecule-level Phase 1 unit).
- Node-type snapshots (`cargo test node_snapshots` + `cargo insta review`).

**Manual verification** — `flutter run`: nodes addable from the
AtomicStructure category, wire correctly, subtitles track the name, text
format round-trips the property. No visual change in the viewport is
expected — tags are invisible until a consumer exists (hover readout lands
in Phase 5).

---

### Phase 4 — atom_edit ops, undo, persistence

**Implementation**

- `add_tag_recorded` / `remove_tag_recorded` / `clear_tags_recorded` on
  `AtomEditData` + delta extension; base-atom edits promote to full
  overrides via the `promote_base_atom_metadata` path, extended to carry
  the base atom's tag names (§Diff semantics).
- Surface a non-empty `DiffStats.dropped_tag_names` as a localized error
  naming the dropped tags: in atom_edit's eval, and in the `apply_diff` /
  `atom_composediff` nodes' eval.
- `SerializableAtom.tags: Vec<String>` save/load (§Serialization).

**Automated tests**

- `rust/tests/structure_designer/atom_edit_undo_test.rs`: tag/untag on a
  diff-atom selection undoes and redoes — every atom's tag *name set* is
  restored. (The table itself need **not** shrink on undo: undoing a
  tag-add may leave the interned name behind as a dead slot — invisible
  through the name-level accessors and reclaimable later. Do not attempt
  exact table restoration; name-based deltas deliberately don't express
  it.) Promotion path for base atoms undoes cleanly (the promotion itself
  is delta-recorded, as it is for flags today).
- **Reclamation × undo stress:** tag with a new name → untag it (name goes
  dead) → tag with a *different* name that reclaims the slot → undo all →
  redo all. Every step restores the correct *names* on the correct atoms
  (this is the test that fails if deltas ever regress to raw bits).
- Tagging a base atom promotes it to a full override that carries both the
  base atom's pre-existing (upstream) tags and the newly added one; a base
  atom already referenced by a bond-tool UNCHANGED marker gets promoted the
  same way, and no code path ever leaves tags on a marker (the §Diff
  semantics invariant, exercised through the atom_edit path).
- Serialization round-trip (the existing atom_edit save/load tests): tagged
  diff atoms survive save→load with tags intact; a tagless save is
  byte-identical to the pre-feature format; an old-format file (no `tags`
  field) loads.
- Exotic names survive persistence: tags containing spaces, quotes, and
  non-ASCII round-trip through the JSON diff serialization unchanged.

**Manual verification** — deferred to Phase 5 (no UI entry points yet).

---

### Phase 5 — API, Flutter UI, reference guide

**Implementation**

- API (`rust/src/api/structure_designer/`): `get/set_tag_data` +
  `get/set_untag_data` (thin, `#[frb(sync)]`, **scope_path-taking** — hard
  rule), mirroring `xray_api.rs` incl. refresh + undo — the getters carry
  `available_tags` from the eval cache (§Existing-names suggestions), like
  `get_materialize_data` does; atom_edit tag actions
  (add/remove/clear on selection, list existing tag names for the picker);
  `APIHoveredAtomInfo.tags: Vec<String>` populated in
  `query_hovered_atom_info` (§Seeing tags). Run
  `flutter_rust_bridge_codegen generate`.
- Flutter: `tag_editor.dart` / `untag_editor.dart` (name text field,
  existing-names dropdown), following `xray_editor.dart` conventions;
  atom_edit panel Tag/Untag/Clear actions with a name picker; the hover
  popup renders the `tags` row (hidden when empty).
- Reference guide: new `doc/reference_guide/nodes/tag.md` (+ `untag`
  section or page), linked from the node index; atom_edit tagging in
  `doc/reference_guide/direct_editing.md`. Document: the 32-name limit and
  its error, `untag`'s empty-name = all-tags semantics, tags flowing
  through the pipeline (and that `materialize` creates untagged atoms — tag
  after materializing), and that the hover popup is where tags are
  inspected — they have no other visual effect until a consumer (future
  visual rules) uses them.

**Automated tests**

- Rust API-level test: set node data through the `StructureDesigner`-level
  setter; re-eval applies; undo/redo restores ("persisted mutations must be
  undoable").
- `flutter analyze` clean over baseline.

**Manual verification** (thin editor layer policy) — `flutter run`:

- Node editors round-trip with subtitles and the viewport-independent
  behavior above; editing works on a node **inside a zone body** (scope
  chain exercised).
- Hover popup: hovering an atom tagged by a plain `tag` node (no atom_edit
  involved) shows the tag list; multiple tags list in bit order; an
  untagged atom's popup is unchanged from today (no empty `Tags:` row);
  `untag` removes the entry from the popup on the next hover.
- atom_edit: tag a selection, hover shows the tag, undo/redo from the UI
  restores both structure and panel; save → reload → tags persist.
- Wired `name` pin wins over the panel value (pin-over-property precedence).
- Reference-guide pages read correctly and are linked.

---

## Future work: tag-driven visual properties

> **Now designed:** `doc/design_style_rules.md` specifies this system in
> full (including per-atom render style, which that doc assesses rather
> than deferring). This section is kept as the original sketch.

Recorded here so the ideas survive until their own design doc. None of this
is in scope for the phases above; the tag core is deliberately independent
of it.

### Style rules as first-class values

- A **built-in record def** (the `ElementMapping` precedent —
  `NodeTypeRegistry::built_in_record_type_defs`, consumed via
  `lookup_record_type_def`), sketch:

  ```
  StyleRule {
    element:      Optional[Int],     // selector: atomic number
    tag:          Optional[String],  // selector: tag name
    color:        Optional[Vec3],
    alpha:        Optional[Float],
    render_style: Optional[...],     // ball-and-stick | space-filling
    // later: visible: Optional[Bool], radius_scale: Optional[Float], …
  }
  ```

  Selector fields absent ⇒ match-all on that axis; both present ⇒ AND
  (`(element, tag)` rules). Property fields absent ⇒ leave that property
  alone. The `Optional[T]` record-field modifier already exists for exactly
  this shape.
- Rules are an **`Array[StyleRule]` value** — buildable with
  `record_construct` + array nodes, storable in one place, wired into many
  consumers ("create rules once, apply to multiple molecules"). A
  convenience `style_rules` editor node is nice-to-have, not load-bearing.
- An **`apply_style` node** (`HasAtoms` + `rules` → same phase type)
  resolves rules to per-atom entries in the **decorator** at eval time —
  the `xray`/`atom_alpha` layer: runtime-only, never serialized, recomputed
  each evaluation, dropped by rebuilding nodes (place `apply_style` late in
  the chain, like `xray`).
- **Matching semantics: ordered, per-property last-writer-wins.** Rules
  apply in array order; a matching rule overrides only the properties it
  sets. No CSS-style specificity ranking — last-writer-wins is the
  compositional idiom this app already teaches (`xray` chaining,
  `materialize.regions` painter's algorithm) and never needs an
  `!important`.

### Rendering

- **Per-atom color** — cheap: `atom_color: FxHashMap<u32, [f32; 3]>` in the
  decorator; the impostor path already carries per-atom appearance
  (`get_atom_impostor_appearance`) and per-atom alpha, color rides the same
  rails. Impostor mode first, `TriangleMesh` opaque-default fallback
  (the `xray` precedent).
- **Per-atom render style (mixed ball-and-stick / space-filling)** — the
  expensive part; its own project. Per-atom radius selection; a bond
  visibility rule (suggested: a bond renders iff both endpoints are
  ball-and-stick — space-filling spheres at vdW radius swallow their
  bonds); interaction with the two mode-specific cull-depth preferences;
  impostor-only at first. Must not stall the rest — carve out as a separate
  phase/doc.
- `alpha` via rules writes the **same** `atom_alpha` decorator field as the
  `xray` node; downstream-node-wins is the natural pipeline composition, and
  the global `scene_alpha` keeps multiplying on top.

### Tags as selectors elsewhere

- Region-gated nodes (`freeze`, `unfreeze`, `xray`, `atom_replace`,
  `add_hydrogen`, `remove_hydrogen`, `infer_bonds`) can each gain an
  optional **`tag: String` pin** alongside `region: Blueprint` — a second
  membership test feeding the same `in_region`-style predicate (both given
  ⇒ AND). Incremental, per-node, on demand; no migration (new optional
  pin).
- Possible expr-language predicate (`has_tag(...)`) and tag-based selection
  queries — speculative, listed only for completeness.

### Explicitly *not* future work

Making frozen / transparency / any semantic property *derive from* tags —
ruled out by the guiding principle above, permanently, not deferred.

## Explicitly out of scope (this design)

- The visual-rules system itself (§Future work — separate design doc).
- Tags in export formats (`.xyz`, `.mol`).
- Tag rename tooling (rename = untag + tag for now).
- Tagging in the legacy `edit_atom` node.
- Unlimited tags per structure (documented widening path: `u32 → u64`).
