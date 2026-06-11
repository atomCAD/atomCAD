# Design: Parameter Wire Stability (cross-network wire-jumbling regression)

**Status:** Root cause **CONFIRMED and reproduced** with failing tests. The original
*index-shift* hypothesis that the first draft of this doc was built around is **DISPROVEN** by
those same tests (see §3). Fix not yet applied.
**Severity:** High. Silent, cross-network, persistent (saveable) data corruption in a critical spot.

---

## 1. Symptom + confirmed root cause

### 1.1 Symptom (power-user report)

> "Wiring no longer stays stable invisibly in other calling networks when one adds further
> parameters. New ports get connected to the same source as the preceding input (despite a
> type error too) and some more wires slightly jumbled up. ... Too easy to save when one
> unwittingly destroyed half one's codebase in the background."

### 1.2 Confirmed root cause: `next_param_id` is never serialized or restored

- `NodeNetwork.next_param_id` — the per-network counter that hands out unique `param_id`s for
  wire preservation — starts at `1` (`node_network.rs:1017`) and is bumped per added parameter
  (`structure_designer.rs:2508`).
- It is **not** in the serialized form: `serializable_to_node_network` restores `next_node_id`
  but **never** `next_param_id` (`serialization/node_networks_serialization.rs:595-598`), and
  `duplicate_node_network` (`structure_designer.rs:1864`) copies via a serialize round-trip.
- So after **load** or **duplicate**, `next_param_id` resets to `1`. The next parameter added
  to that network is handed `id = 1`, which **collides with the network's existing first
  parameter** (ids start at 1).
- `repair_call_sites_for_network` (`network_validator.rs:124-126`) prefers `param_id` matching,
  so it resolves the new param's recycled id to the **first** param's old index and **clones
  that `Argument` onto the new pin** — exactly "new port connected to the same source as a
  preceding input (despite a type error)".

### 1.3 Why it reads as a regression

The corruption requires `param_id`-based matching to *win* with a recycled id. Id-based
matching was added to support **parameter rename** (see the now-green `*_rename_*` scenarios in
`parameter_wire_preservation_test.rs`). Before that, matching was **name-only**, under which a
recycled id is harmless (a genuinely new name → no match → empty pin). So the unserialized
counter was a latent defect that **became a live bug when id-primary matching landed**. It only
triggers after a project is reopened (load) or a network is duplicated — every pure in-memory
edit is correct in current HEAD (proven by 6 green guard tests). A `git bisect` could confirm
the exposing commit but is **not needed** for the fix.

### 1.4 Reproductions (the acceptance criteria)

`rust/tests/structure_designer/parameter_wire_stability_regression_test.rs` — genuinely red,
no `#[ignore]`:
- `regression_load_then_add_param_clones_neighbor_wire` — load → add param ⇒ new pin clones
  pin 0's wire (`pin0=[i1] pin1=[i2] pin2=[i1]`).
- `regression_load_then_add_param_clones_wrong_typed_wire` — same, distinct types ⇒ a Bool
  wire lands on a new Int pin (the "type error").
- `regression_duplicate_then_add_param_corrupts_instance_wires` — duplicate → add param to the
  copy ⇒ instance wires corrupted (`pin0=[] pin1=[i2] pin2=[]`).
- Plus 6 `guard_*` tests (HOF-body add/reorder, in-memory save/load roundtrip,
  edit-original-after-duplicate, undo/redo, two-step reorder) that are **green and must stay
  green**.

---

## 2. Architecture background (self-contained)

A `NodeNetwork` can be used as a node type inside other networks ("custom node" / "instance").
Its `parameter` nodes become the instance's input pins.

### 2.1 How parameters are modeled
- A `parameter` node carries `ParameterData` (`nodes/parameter.rs:19`): `param_id: Option<u64>`
  (**stable identity** for wire preservation), `param_name`, `data_type`, `sort_order: i32`
  (drives pin ordering), `param_index: usize` (derived position).
- `validate_parameters` (`network_validator.rs:141`) collects parameter nodes, sorts by
  `(sort_order, node_id)` (`compare_parameters`, `:57`), and **rebuilds**
  `network.node_type.parameters: Vec<Parameter>`, propagating `param_id → Parameter.id` (`:196`).
- **It validates `param_name` uniqueness but NOT `param_id` uniqueness** — this is the missing
  guarantee that let the collision through silently (see F2).

### 2.2 How wires are stored
- A `Node` holds `arguments: Vec<Argument>` — one `Argument` per input pin, **by position**.
  `Argument` (`node_network.rs:185`) holds `incoming_wires: Vec<IncomingWire>` (`:149`).
- The wire records its *source* but not which *parameter identity* it feeds; the wire→parameter
  link is purely the array index. Identity (`param_id`) lives on the definition side and is the
  key `repair_call_sites_for_network` uses to translate old→new positions.
- **Note (important correction to the first draft):** positional storage is *not* the proximate
  cause of this bug. The positional reconcile works correctly **as long as `param_id` is a
  unique, persistent identity** — which is exactly the invariant the unserialized counter
  breaks. The fragility is in *identity allocation*, not in *wire storage*.

### 2.3 When validation runs
- `StructureDesigner::validate_active_network` (`structure_designer.rs:6655`) validates the
  active network, then propagates to other networks when a network's `valid` flips or
  `interface_changed` is true (`:6734`).
- Per-network entry point: `validate_network` (`network_validator.rs:603`). Phase order:
  `check_interface_changed` → snapshot `old_parameters` → `validate_parameters` (rebuild) →
  `repair_call_sites_for_network` (if interface changed; identity-based, walks parents incl.
  HOF bodies) → apply/map post-passes → `repair_network_arguments` (count/index pad-truncate) →
  `repair_output_pin_wires` → `validate_wires` / `validate_zones_recursive`.

---

## 3. DISPROVEN hypothesis (don't re-chase it)

The first draft diagnosed the bug as an **index-shift**: that `repair_network_arguments`
(the unconditional, count/index-based pass) silently pad/truncates a custom instance's wires
onto the wrong pins whenever the identity-based `repair_call_sites_for_network` is skipped or
runs late (reorder → no-op leaves wires misaligned; mid-insert → pad-at-end shifts).

**The 6 `guard_*` tests refute this for the in-memory paths.** In-memory mid-insert, reorder,
retype, HOF-body edits, save/load roundtrip, and undo/redo all preserve wires correctly —
i.e. `repair_call_sites_for_network` **does** fire, **is** identity-correct, and
`repair_network_arguments` is **not observed to corrupt** anything. The actual bug is upstream:
correct machinery fed a **corrupted identity** (a recycled `param_id`).

Consequences for fixers:
- **Do not** restructure the validator around a "two fighting passes" model, and **do not**
  delete or rewrite `repair_network_arguments` on the theory that it is a "loaded gun." No
  repro supports that.
- The one residual factual note worth keeping: `check_interface_changed` (`:226`) compares
  parameters **positionally by `(name, data_type)`, not by `param_id`**. It is not implicated
  in this bug (the add path changes the count, which it detects), but tightening it to compare
  ids is cheap defensive hygiene if F3 surfaces a need.

---

## 4. The fix + durable hardening

These can run in parallel threads; contention is low (see §6). F1 is the fix; F2–F5 are the
"correctness guarantees in a critical spot" the user asked for.

### F1 — Restore `next_param_id` (THE fix)  *(small, do first)*
Set `next_param_id = max(existing param_id) + 1` (derive from the parameter nodes) at **both**
reset sites:
1. `serializable_to_node_network` (`serialization/node_networks_serialization.rs:595-599`) —
   add the restore next to the `next_node_id` line. Deriving from the loaded params is robust
   even for old files that never stored the counter; optionally also serialize it for exactness.
2. `duplicate_node_network` (`structure_designer.rs:1864`) — fix up the copy's counter after the
   serialize round-trip.
**Acceptance:** the 3 `regression_*` tests go green; the 6 `guard_*` tests stay green.
**Files:** `node_networks_serialization.rs`, `structure_designer.rs`.

### F2 — Enforce `param_id` uniqueness invariant  *(the missing guarantee)*
In `validate_parameters` (`network_validator.rs:141`), validate `param_id` uniqueness the same
way `param_name` uniqueness is already validated, and surface a **blocking** error on a
duplicate (corrupting the interface has cross-network blast radius — see
`structure_designer/AGENTS.md` "blocking vs non-blocking"). This converts any future
id-allocation slip (not just this one) from silent cross-network corruption into a loud,
spatially-located error, and is the single highest-leverage guard.
**Files:** `network_validator.rs`. **Independent of F1** (but with F1 landed, the duplicate
should never occur — F2 is the backstop).

### F3 — Audit all parameter-CREATION sites for id allocation  *(same root-cause family)*
The bug is "a param node created with an id that isn't unique/persistent." Audit **every** site
that constructs a `parameter` node / `ParameterData` and confirm it (a) assigns a `param_id`
and (b) keeps `next_param_id` above all existing ids:
`structure_designer.rs:2497` (add), `selection_factoring.rs:485`, `promote_to_parameter.rs:99`,
`closure_network_conversion.rs:902`, `node_inlining.rs`, `node_networks_import_manager.rs`.
Several of these build `ParameterData` literals that (per earlier grep) **omit `param_id`**
(defaulting to `None`) and may not initialize the network's `next_param_id` — latent siblings
of this bug. Fill the §9 table.
**Files:** read/trace many; fixes likely in the listed creation sites.

### F4 — Pre-save / project integrity check  *(defense in depth; serves the user's core fear)*
A scan (on save and/or as a surfaced validation) flagging: any custom-network instance whose
`arguments.len()` ≠ called network's param count; any duplicate `param_id` within a network; any
retained wire whose source type violates its destination pin type. Surface prominently so a
corrupted project cannot be saved unnoticed. Likely a loud **warning** (doesn't blank unrelated
networks).
**Files:** save path (`serialization/`), validation surface, Flutter status/modal.

### F5 — Property / invariant tests across mutation × persistence  *(lock it down)*
Extend the existing regression file into a property suite: for arbitrary sequences of param
edits (add/remove/reorder/rename/retype) crossed with **{fresh, after-load, after-duplicate}**,
assert every surviving wire preserves its `(source, destination-parameter-identity)` pair and no
wire ever changes which parameter it feeds. The **{after-load, after-duplicate}** axis is the
one that actually broke and the original property suite would have missed.
**Files:** extend `parameter_wire_stability_regression_test.rs` (or a new
`*_property_test.rs`).

---

## 5. Parked — NOT justified by current evidence

- **Index-shift hardening of `repair_network_arguments` ("drop, don't shift"; harden
  `check_interface_changed`; re-validate repaired parents)** — the first draft's W1/W2/W4. No
  reproduction shows `repair_network_arguments` corrupting a custom instance; the guards prove
  the in-memory reconcile is correct. Revisit **only** if F3/F5 surface a real positional-
  reconcile failure. The cheap exception is the `check_interface_changed` id-comparison tweak
  noted in §3.
- **Identity-keyed wires refactor** — the first draft's W6, and an earlier (now-retracted) claim
  in this doc that it "makes the id collision impossible by construction." **It does not:** a
  recycled `param_id` collides regardless of whether wires are stored by index or keyed by
  `param_id` — keying by a non-unique key is *worse*. The invariant that prevents the collision
  is **param_id uniqueness + robust allocation (F1/F2/F3)**, not a wire-storage change. This is a
  large, broad-surface refactor with weak justification now; **not recommended** as part of this
  effort.

---

## 6. Coordination

The fix and hardening touch mostly disjoint files, so parallelization is low-risk:
- **F1** → `node_networks_serialization.rs` + `structure_designer.rs::duplicate_node_network`.
- **F2** → `network_validator.rs::validate_parameters` (a uniqueness check; does not touch the
  repair passes).
- **F3** → the param-creation sites (factoring/promote/inline/import).
- **F4** → save path + Flutter surface (different subsystem).
- **F5** → test crate only.

Land **F1 first** (turns the 3 reds green), then F2 (backstop), then F3/F4/F5 in any order.
`ParameterData` / `Parameter` shapes can stay frozen — none of F1–F5 needs to change them.

---

## 7. Quick reference — key locations

| What | Location |
|---|---|
| `next_param_id` field + init to 1 | `node_network.rs:799`, `:1017` |
| `param_id` allocation (add path) | `structure_designer.rs:2508` |
| **Load: `next_param_id` NOT restored** | `serialization/node_networks_serialization.rs:595-599` |
| **Duplicate (serialize round-trip)** | `structure_designer.rs:1864` (`duplicate_node_network`) |
| Identity remap that clones on collision | `network_validator.rs:124-126` (`repair_call_sites_for_network`) |
| Param validation (name-unique only) | `network_validator.rs:141` (`validate_parameters`) |
| Param model | `nodes/parameter.rs:19` (`ParameterData`) |
| Rebuild `node_type.parameters` | `network_validator.rs:196` |
| Wire storage (positional) | `node_network.rs:185` (`Argument`), `:149` (`IncomingWire`) |
| Index reconcile (NOT the culprit; see §3) | `network_validator.rs:263` (`repair_network_arguments`) |
| Interface-change detector | `network_validator.rs:226` (`check_interface_changed`) |
| Validation orchestration | `network_validator.rs:603-679` (`validate_network`) |
| Regression + guard tests | `tests/structure_designer/parameter_wire_stability_regression_test.rs` |

---

## 8. Out of scope / non-goals
- Reworking the parameter UI (reorder gestures, etc.).
- Changing the `apply`/HOF derived-pin-layout mechanism.
- Identity-keyed-wire refactor (see §5 — not justified by this bug).
- Iterator/closure semantics.

---

## 9. Appendix — parameter-creation / mutation call-site audit (F3 fills this)

| Path | Assigns `param_id`? | Maintains `next_param_id`? | Status / action |
|---|---|---|---|
| `add_node("parameter")` (`structure_designer.rs:2497`) | yes (`:2508`) | yes (bumps counter) | OK |
| `.cnnd` load (`node_networks_serialization.rs`) | n/a (deserializes) | **NO — resets to 1** | **CONFIRMED BUG** → F1 |
| `duplicate_node_network` (`structure_designer.rs:1864`) | n/a (round-trip) | **NO — resets to 1** | **CONFIRMED BUG** → F1 |
| `selection_factoring.rs:485` | _(tbd — F3; literal appears to omit `param_id`)_ | _(tbd)_ | audit |
| `promote_to_parameter.rs:99` | _(tbd — F3)_ | _(tbd)_ | audit |
| `closure_network_conversion.rs:902` | _(tbd — F3)_ | _(tbd)_ | audit |
| `node_inlining.rs` / import manager | _(tbd — F3)_ | _(tbd)_ | audit |
| In-memory add/remove/reorder/retype (no load/dup) | — | — | **HOLDS** — 6 green guards |
