# Incremental layout co-editing session — handoff (2026-09-03)

A working note for continuing this session in a fresh thread. It records what
we are doing, how the setup works, what got fixed along the way, and the next
item with its proposal. Written by the assistant at the maintainer's request.

## What we are doing

Manually testing `doc/design_incremental_layout.md` (Phases 1–5 landed) and,
more broadly, human + AI co-editing of a node network: the maintainer edits in
the GUI (drags nodes, rewires by hand), the assistant edits the same network
through `atomcad-cli` (the `atomcad` skill), and after every AI edit we check
that the layout pass moved only what the design says it may move.

Order of business so far: a small 3-node network (done, one rule changed), then
a real messy design (`layout_test_scratch.cnnd`, a **copy of the maintainer's
private `from_mechadense.cnnd`** — 89 networks, ~2,300 nodes, 48 HOF bodies,
loose columns and pre-existing overlaps everywhere). The private file may not
be for public consumption; the maintainer is checking with its author
(mechadense) before it can become a tracked test corpus. Until then it runs
only via `ATOMCAD_LAYOUT_CORPUS=<path>`.

## How the setup works (gotchas that cost time today)

- atomCAD must be running; the CLI is `./atomcad-cli` from the repo root
  (dev mode: `dart run bin/atomcad_cli.dart`). The Dart CLI is live
  immediately after an edit; **Rust changes are not** — the app loads
  `rust/target/release/rust_lib_flutter_cad.dll`, which the running app locks.
  Cycle: close app → `cd rust && cargo build --release -j 4` → relaunch →
  reopen the scratch file. (`cargo` on this machine: always `-j 4`.)
- Every AI edit is one Ctrl+Z; the AI History panel's *Layout* tab lists
  exactly which nodes moved and how far — that is the ground truth to compare
  screenshots against.
- Piping a `query` printout back in: `./atomcad-cli edit --replace < file` is
  safe now (stdin is read to EOF); `--code="…"` with literal newlines works
  too. Before today's fixes the first truncated at a blank line and once wiped
  a 135-node network with Undo greyed out — both fixed.
- The round-trip check used for step 1 of the big-design plan:
  `query > before; edit --replace < before; query > after; diff before after`,
  then read `success` / `errors` from the JSON response. An error on the
  network that was already there before the edit is not a round-trip failure.

## Test plan and status

Small network (done): insertion with no room → half-plane shift ✅; downstream-
only anchor + slide ✅; delete leaves hole ✅; anchorless node ❌ → **rule
changed to bottom-left** (commit `6d564c26`); backward-wire repair ✅ (and a
no-op after the rule change); single-step undo ✅; `grow_rect` height cascade
✅, pre-existing backward wire left alone ✅.

Big design (`layout_test_scratch.cnnd`, network
`0_precursor_tests_incomplete_V5+covers`, 135 nodes, 3 bodies):

1. `query` → `edit --replace` of the unchanged text → nothing may change.
   **Text is identical now**; node positions/collapse/body sizes/roles are
   asserted identical by the corpus test on the whole private file. The one
   `success: false` left in the app was the `parameter5` type error caused by
   dropped pin roles — fixed in the uncommitted batch below, not yet rebuilt
   into the app. **Redo step 1 in the app after the rebuild** and expect
   `success: true` and zero moves in the Layout tab.
2. Add a `float` wired into `xray1.alpha` (downstream-only anchor in a crowded
   spot → slide/push).
3. Add a second parameter to `expr49` (`grow_rect` among 13–17 px neighbours).
4. Add a comment `on: unfreeze1`, then insert an identity `expr` between
   `unfreeze1` and `apply_style1` (no-room shift + window rule + Step 8
   comment pull-back). Most likely place to see the known limit: the snap
   gives up in dense regions and a loose column gets cut.
5. Add an `expr` fed by `apply1` inside `map4`'s body (inside-out, slack-first).
6. Rewire `xray2.alpha` from `float4` to `float2` (backward wire on a real
   drawing).
7. Delete `Comment57`; then Ctrl+Z six or seven times back to the original.

## What got fixed on the way (all committed unless noted)

| Commit | What |
|---|---|
| `6d564c26` | anchorless blocks go bottom-left (was bottom-right → every later wire backward → 780 px shift) |
| `2b4ede46` | CLI: multi-line `--code` survives `dart.bat` and `package:args` |
| `9ddb682f` | AI edit: a `--replace` that creates nothing is still undoable + dirty; CLI reads piped stdin to EOF |
| `2f22056c` | text format: full type grammar in property position (`A -> B`, `[T]`, `Iter[T]`, `Record(N)`, …) |
| `babdfc93` | every corpus network round-trips through `--replace` (object-literal types, quoted pin names/keys, total `get_text_properties` for `structure_rot.axis_index` / `motif.name`, deferred `apply` arg wires, unique node names `x`/`x_2`, array-pin wire order) |
| `32fe4c24` | pin the object-literal function-type case |
| **uncommitted** | `pin_roles: { input: delayed, translation: supplied }` in the text format; `unique_node_names` used by the layout delta too; corpus test compares the identity snapshot + roles, not just text. 13 files; suggested message: *"text format: pin_roles spelling; layout delta uses the shared unique naming"*. Workspace green (3405 in structure_designer). |

The net that found most of it:
`rust/crates/atomcad-structure-designer/tests/structure_designer/text_format_roundtrip_corpus_test.rs`
— demolib in CI, the private file with
`ATOMCAD_LAYOUT_CORPUS="C:/machine_phase_systems/flutter_cad/from_mechadense.cnnd" cargo test -j 4 -p atomcad-structure-designer --test structure_designer text_format_roundtrip_corpus`.
Run it after any text-format or `get/set_text_properties` change. Rules it
enforces are written up in `rust/crates/atomcad-structure-designer/src/text_format/AGENTS.md`
("Round-trip invariants").

## Next: pin visibility in the text format

Found while answering "is pin visibility in the text format?" — it is not.

**Today:** `visible: true` per node. On the way in, `set_node_display(id, true)`
installs `NodeDisplayState::normal()` = display type *Normal*, **pin 0 only**.
On the way out, `visible: true` is written whenever the node has *any* display
state. Not represented: which output pins are displayed
(`NodeDisplayState::displayed_pins: HashSet<i32>`) and
`NodeDisplayType::Ghost`. The private file has 11 nodes showing more than pin 0
(ten with pins 0+1+2, one with 0+1 — three-output nodes such as
`record_destructure` / `unpack`); a `--replace` demotes them to pin 0. Ghost
has zero occurrences in both corpora. The corpus test's state comparison does
not look at `displayed_nodes` yet, which is why this went unnoticed.

**Proposal** (keeps `visible: true` exactly as it is):

```
d = record_destructure { record: r, visible: true }        # Normal, pin 0 — as today
d = record_destructure { record: r, visible: [x, y, z] }   # Normal, these output pins by name
d = record_destructure { record: r, visible: ghost }       # Ghost, pin 0
```

- `query` writes `true` for the default state and the pin-name list only when
  the set differs from `{0}`, so existing printouts do not change.
- Pin names come from `get_output_pin_name`, the same vocabulary the `.pin`
  wire syntax uses (`d.y` in a wire ⇔ `y` in the list). Pin 0's name is
  written like any other; an unknown name is a warning and the rest applies.
- Ghost + a custom pin set would need an object form
  (`visible: { pins: [x, y], ghost: true }`); with zero real uses of Ghost,
  add only `visible: ghost` now and leave the combination unless it appears.
- Implementation is the documented three-site shape for network-level
  properties (`text_format/AGENTS.md`): serializer pass (`visible` is the
  "third pass" in `serialize_node`), `apply_literal_properties` already skips
  `visible`, and `collect_connections` already intercepts it into
  `visible_nodes: Vec<(ScopePath, String)>` — extend that entry to carry the
  parsed state and have `apply_visibility` install a full `NodeDisplayState`
  instead of calling `set_node_display(id, true)`.
- Tests: a `text_format_visibility_test.rs` beside `text_format_pin_roles_test.rs`
  (author `[x, y]`, query it back, round-trip; `true` unchanged; ghost;
  unknown pin name warns); and add `displayed_nodes` (by name path, incl.
  bodies) to the corpus test's state comparison, the way `roles_by_path`
  does it for roles.
- Docs: `doc/node_network_text_format.md` §Visibility, the skill's
  `references/text-format.md` (Special Inputs), `text_format/AGENTS.md`
  (network-level properties list).

## Other open items

- **Private corpus into the repo** — only with the author's OK; then as a
  frozen fixture (`rust/tests/fixtures/corpus/…`), tracked, with the live
  file still reachable through `ATOMCAD_LAYOUT_CORPUS`.
- **Dart CLI has no tests** — `_splitMultilineOptions` / `_readStdinToEof`
  were verified by hand; moving them from `bin/` to `lib/` would let
  `flutter_test` cover them.
- **`axis_index: -1` for "no axis"** is a spelling the assistant chose; one
  line in `structure_rot.rs` if a different one is preferred.
- **Step 6 refinement (later):** when a new backward wire's source is a
  free-standing leaf, re-place the source as a block instead of shifting the
  half-plane.
- **Known limit of the window rule:** in dense regions the snap gives up and
  a loose column can be cut by a shift (`layout/AGENTS.md`). Step 4 of the big
  design plan is where to see whether that is tolerable.
