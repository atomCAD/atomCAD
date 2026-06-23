# Design: Phase 0 — Invariant Checker + Property Suite (implementation spec)

**Status:** Ready to implement.
**Parent:** `doc/design_identity_vs_naming.md` — read it first for the *why*.
This doc is the **Phase 0** work item from that doc's §9, expanded into a
concrete, self-contained implementation brief.
**Related tactical docs:** `doc/design_custom_node_type_cache_invariant.md`
(the existing single invariant we generalize), `doc/design_parameter_wire_stability.md`
(source of the F5 property suite and the `param_id` invariant).

> **One-paragraph summary.** Phase 0 ships *no* data-representation change. It
> adds (1) a read-only **invariant checker** that walks a network/document and
> reports every internal-consistency violation, wired into the existing
> debug-only validation completion point, and (2) a **property/fuzz test suite**
> that drives random structure-preserving mutations crossed with persistence and
> asserts a wire-identity oracle while the checker runs live. Together they turn
> the silent rename/reorder/wire-loss corruption class into loud, located
> failures *before* any later phase changes a single representation. It is
> cheap, reversible, and is the test fixture every later phase plugs into.

---

## 0. Guiding rule (do not violate)

The checker reports **internal-consistency** violations — bugs in *our* mutation
/ repair code — not user mistakes. Some "bad-looking" states are legitimately
user-reachable and **already surfaced** as `ValidationError`s (a type-mismatched
wire; a dangling record name in a hand-edited `.cnnd`). The invariant is
therefore **not** "every reference resolves"; it is:

> **Every unresolved / incoherent reference is *accounted for* by a
> corresponding `ValidationError`. None is silent.**

A violation that is accounted for (some `ValidationError` sits on the same node)
is **not fatal**; a *silent* one is. This is what makes the checker safe to turn
into a `debug_assert!` without false-firing in honest tests. See §3 for the
hard-vs-accounted-for split.

---

## 1. Deliverables (definition of done)

1. A read-only DataType name walker (§2).
2. A new module `structure_designer/invariants.rs` with the violation type, the
   per-network checker, the document checker, and the debug-assert wrapper (§3–§5).
3. The wrapper wired into `validate_network`, **replacing** the existing
   single-purpose cache assert (§5), preserving its panic substring (§7).
4. Unit tests: one per invariant kind, each forcing the violation and asserting
   it is reported (and that the debug wrapper panics for the fatal ones) (§6.1).
5. The property/fuzz suite with the persistence matrix (§6.2).
6. A lint entry point that runs the checker over real `.cnnd` files (§6.3).
7. **The entire existing test suite stays green** — zero false positives. This
   is the gating acceptance criterion; see the staged rollout in §8.

No `.cnnd` format change. No migration. Fully reversible.

---

## 2. Prerequisite helper: read-only record-name walker

`data_type.rs` has `walk_data_type_record_names_mut(&mut DataType, &mut impl FnMut(&mut String))`
but **no read-only counterpart**. The checker needs to *read* embedded
`RecordType::Named` names without cloning. Add the mirror:

```rust
// data_type.rs — mirror of walk_data_type_record_names_mut, read-only.
pub fn walk_data_type_record_names(dt: &DataType, f: &mut impl FnMut(&str)) {
    match dt {
        DataType::Record(RecordType::Named(name)) => f(name),
        DataType::Record(RecordType::Anonymous(fields)) => {
            for (_, ty) in fields { walk_data_type_record_names(ty, f); }
        }
        DataType::Array(inner) | DataType::Iterator(inner) => {
            walk_data_type_record_names(inner, f);
        }
        DataType::Function(ft) => { /* recurse params + return, mirror the _mut arm exactly */ }
        DataType::AnyFunction { leading_params } => {
            for p in leading_params { walk_data_type_record_names(p, f); }
        }
        _ => {}
    }
}
```

**Match the `_mut` version's recursion arms exactly** (Array, Iterator, Function,
AnyFunction, anonymous-record fields) so the two never diverge — a divergence
here would reintroduce the "walker misses a case" bug class at the leaf level.

---

## 3. The violation type and severity model

New module `structure_designer/invariants.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantKind {
    // Tier 1 — structural bookkeeping (always fatal).
    ArgCountMismatch,     // arguments.len() != resolved params.len()
    ZoneArgCountMismatch, // zone_output_arguments.len() != zone_output_pins.len()
    CacheNone,            // derived-layout node with custom_node_type == None  (was the standalone assert)
    DuplicateParamId,     // two parameter nodes share a param_id in one network
    ParamIdFloor,         // next_param_id <= max(param_id)
    NextNodeIdFloor,      // next_node_id <= max(node id) in this network/body

    // Tier 2 — reference resolution (fatal only if NOT accounted-for).
    UnresolvedNodeType,   // node_type_name resolves to neither built-in nor a network
    UnresolvedRecordName, // an embedded RecordType::Named(n) has no def
    UnresolvedSchema,     // record_construct/destructure schema / product target has no def
    MissingWireSource,    // a depth-0 wire's source_node_id absent in this network
    PinIndexOutOfRange,   // source pin index outside the resolved source's pins

    // Tier 3 — type coherence (optional in Phase 0; fatal only if NOT accounted-for).
    IncompatibleWireType, // retained wire source type !can_be_converted_to dest type
}

#[derive(Debug, Clone)]
pub struct InvariantViolation {
    pub scope_path: Vec<u64>,   // chain of HOF node ids to the body; empty = top-level
    pub node_id: Option<u64>,
    pub kind: InvariantKind,
    pub detail: String,         // human-readable; for CacheNone MUST contain the legacy substring (see §7)
    pub accounted_for: bool,    // true if a ValidationError sits on this node (Tier 2/3 only; Tier 1 always false)
}

impl InvariantKind {
    /// Tier 1 is always fatal; Tier 2/3 are fatal only when not accounted for.
    pub fn is_tier1(&self) -> bool { /* match the Tier-1 variants */ }
}

impl InvariantViolation {
    pub fn is_fatal(&self) -> bool { self.kind.is_tier1() || !self.accounted_for }
}
```

`accounted_for` is computed as: **does `network.validation_errors` contain any
entry whose `node_id == Some(this node)`?** This is a deliberately *loose*
accounting — it does not try to match the specific error to the specific
reference. Loose is correct here: the goal is "no *silent* corruption," and any
surfaced error on the node means the user is already being told that node is
broken. Tightening it risks false-positives that block the staged rollout.

---

## 4. The checkers

### 4.1 Per-network checker (the hot path)

```rust
pub fn check_network_invariants(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Vec<InvariantViolation>;
```

It must be **scope-aware** for wire-source checks, so structure it as a recursive
helper that mirrors `validate_zones_recursive`'s shape (it carries the ancestor
chain), **not** a flat `walk_all_nodes` (which loses which network a node belongs
to — fatal for "source exists in *this* network" checks):

```rust
fn check_one_scope(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    ancestors: &[&NodeNetwork],   // ancestors[len-1] == immediate parent
    scope_path: &[u64],
    out: &mut Vec<InvariantViolation>,
) {
    // Network-signature record refs (parameters + Fixed output pins): R2.
    // Per-network param_id uniqueness + floors: B3, B4.
    for node in network.nodes.values() {
        // R1 node_type resolves
        // B1/B2 resolve node type via registry.get_node_type_for_node(node):
        //   - arguments.len() vs params.len()           -> ArgCountMismatch
        //   - zone_output_arguments.len() vs zone_pins  -> ZoneArgCountMismatch
        //   - derived-layout && custom_node_type.is_none() -> CacheNone
        // R2 embedded record names in node.data DataType fields (use the SAME
        //    downcast set as collect_record_refs_in_network, via the new
        //    read-only walker) -> UnresolvedRecordName
        // R3 record_construct/destructure schema, product target -> UnresolvedSchema
        // R4 for each argument's incoming_wires with source_scope_depth == 0:
        //    source_node_id must be in network.nodes -> MissingWireSource
        //    (depth >= 1 capture/zone-input resolution is already enforced by
        //     validate_zones_recursive rules 2/3; do NOT duplicate it here —
        //     accounted-for via those ValidationErrors.)
        // R5 source pin index in range vs resolved source node output pins
        //    (-1 = function pin allowed) -> PinIndexOutOfRange
        // T1 (optional) retained wire type compatibility -> IncompatibleWireType
    }
    // Recurse into each node.zone with ancestors extended by `network` and
    // scope_path extended by the HOF node id.
}
```

Resolution helpers to reuse (do not reinvent):
`registry.get_node_type_for_node(node)` (resolved layout, reads the cache),
`registry.resolve_output_type(...)` (source pin type / count),
`registry.lookup_record_type_def(name)` (record-def resolution — tries user then
built-in defs), `walk_data_type_record_names` (the new helper), and the existing
parameter-collection logic in `validate_parameters` for B3.

The R2 per-node enumeration must cover **the same node-data variants** as
`collect_record_refs_in_network` — i.e. the list now including
`closure`/`apply` (`type_args`) and `collect` (`element_type`). To prevent a
*third* copy of that list drifting, prefer factoring the "for each embedded
record name in this node's data, call `f(name)`" enumeration into one shared
function that both `collect_record_refs_in_network` and the checker call. (This
is optional polish, but it is the structural fix for the drift that caused the
bug this whole effort traces back to — flag it for the implementer.)

### 4.2 Document checker (for tests + lint)

```rust
pub fn check_document_invariants(registry: &NodeTypeRegistry) -> Vec<InvariantViolation>;
```

Runs `check_network_invariants` over every `registry.node_networks` value, **plus**
document-level checks that aren't per-network:

- Every `RecordTypeDef.fields` type's embedded `RecordType::Named` resolves
  (record→record references) — `UnresolvedRecordName`, `node_id: None`.
- (When later phases add type/record/field ids: id uniqueness + floors land here.)

Because it is pure and non-panicking, this is also the lint entry point (§6.3).

---

## 5. Wiring it in

Replace the existing call at the end of `validate_network`
(`network_validator.rs:784`):

```rust
#[cfg(debug_assertions)]
debug_assert_custom_node_type_cache_invariant(network, node_type_registry);
```

with

```rust
#[cfg(debug_assertions)]
debug_assert_network_invariants(network, node_type_registry);
```

and define the wrapper in `invariants.rs`:

```rust
#[cfg(debug_assertions)]
pub fn debug_assert_network_invariants(network: &NodeNetwork, registry: &NodeTypeRegistry) {
    let violations = check_network_invariants(network, registry);
    let fatal: Vec<_> = violations.iter().filter(|v| v.is_fatal()).collect();
    debug_assert!(
        fatal.is_empty(),
        "network invariant(s) violated: {:#?}\nSee doc/design_identity_vs_naming_phase0.md",
        fatal,
    );
}
```

Keep the placement reasoning from the cache-invariant doc: this runs **only** at
`validate_network`'s end, where initialization is guaranteed complete — **never**
in `get_node_type_for_node` or any path that executes during the
post-deserialize / pre-init transient (derived nodes legitimately hold a `None`
cache there; references aren't wired yet). The old standalone
`debug_assert_custom_node_type_cache_invariant` function is folded into the
`CacheNone` check and deleted.

---

## 6. Test plan

All tests live under `rust/tests/structure_designer/` (never inline
`#[cfg(test)]`); register new modules in `rust/tests/structure_designer.rs`. New
file: `invariants_test.rs`.

### 6.1 One unit test per invariant kind

For each `InvariantKind`, build a minimal `StructureDesigner` / `NodeNetwork`,
**force** the violation (white-box — `Node`/`ParameterData` fields are `pub`),
and assert `check_network_invariants` reports that kind. For the Tier-1 kinds and
a silent Tier-2 kind, add a `#[cfg(debug_assertions)] #[should_panic]` test that
`validate_active_network()` (or `validate_network`) panics. Concrete seeds:

- `ArgCountMismatch` — push/pop an `Argument` so `arguments.len()` ≠ params.
- `CacheNone` — set a derived node's `custom_node_type = None` (this is the
  existing `invariant_assertion_fires_on_forced_none_cache` test; keep it green).
- `DuplicateParamId` / `ParamIdFloor` — two `parameter` nodes with the same
  `param_id`; `next_param_id` below the max.
- `UnresolvedNodeType` — a node whose `node_type_name` is a non-existent name.
- `UnresolvedRecordName` — **the load-bearing one**: a `closure` with
  `type_args = [Record(Named("ghost"))]` and no `ghost` def. This is the exact
  shape of the bug that motivated this whole effort; assert it is reported and
  (being silent) fatal.
- `MissingWireSource` / `PinIndexOutOfRange` — a wire to a non-existent source id
  / an out-of-range pin index.
- `IncompatibleWireType` (if T1 implemented) — a retained Bool→Int wire with no
  validation error → fatal; with a validation error on the node → not fatal.

### 6.2 Property / fuzz suite (the F5 generalization)

`design_parameter_wire_stability.md` F5, generalized. A seeded deterministic
generator (hand-rolled LCG is fine; print the seed on failure for repro) emits
sequences from the **mutation alphabet**:

- params: add / remove / **reorder** / **rename** / retype
- record-def fields: add / remove / reorder / rename
- record defs & networks: rename (incl. namespace-move) / add / delete
- graph: add node / delete node / connect / disconnect
- structural: factor selection / inline / duplicate network / convert closure⇄network

After **each** mutation, fork the state through the **persistence axis** and at
every fork (a) assert the **wire-identity oracle** and (b) assert
`check_document_invariants` returns no fatal violation:

- **fresh** (in-memory)
- **save → load** (the axis that exposed `next_param_id` reset)
- **duplicate-network → edit the copy**
- **export-to-library → import** (exercises the future remap path)

**The oracle.** Key every wire by
`(source_node_id, source_pin, dest_node_id, dest_param_identity)`, where
`dest_param_identity` is the destination parameter's **`param_id`**, resolved
from the positional index via the node type at assert time (`param_id` already
exists, so the oracle works *before* Phase 1 stores it on the wire). A
structure-*preserving* op (rename, reorder) must leave this set **unchanged**; a
structure-*changing* op (delete a param) must remove exactly that param's tuples
and touch no others.

### 6.3 Lint real projects

A `#[ignore]`d test (or a tiny example binary) `lint_projects` that reads
`.cnnd` paths from an env var (e.g. `ATOMCAD_LINT_FILES`, `;`-separated),
loads each, runs `check_document_invariants`, and prints all violations grouped
by network. Use it to calibrate the catalogue against the real TTPL projects
before flipping asserts hard (§8). Also commit a small healthy fixture so the
test has a default to run against.

---

## 7. Compatibility constraints (do not break these)

- **Existing `should_panic` test.** `rename_wire_loss_regression_test::invariant_assertion_fires_on_forced_none_cache`
  asserts `#[should_panic(expected = "custom_node_type cache invariant violated")]`.
  The `CacheNone` violation's `detail` (and therefore the debug-assert message)
  **must still contain the substring** `custom_node_type cache invariant violated`,
  or that test must be updated in the same change. Preserving the substring is
  preferred.
- **No release-build cost.** Everything that runs in the hot validation path is
  `#[cfg(debug_assertions)]`. The pure `check_*` functions may be compiled in
  release (they're used by the lint tool) but are **never called** from
  `validate_network` in release.
- **No new false positives.** The full `structure_designer` + `integration`
  crates must stay green with the debug asserts active. If a legitimate state
  trips a check, the fix is to refine the *accounting* (§3) or the *placement*
  (§5), **not** to weaken the invariant.

---

## 8. Staged rollout *within* Phase 0 (de-risk the asserts)

Do not turn the asserts hard until calibrated:

1. **Land the checker returning violations + all unit tests** (§4, §6.1). Do
   **not** wire `debug_assert_network_invariants` into `validate_network` yet.
2. **Run the lint tool** (§6.3) over the real TTPL files and the existing test
   fixtures. Investigate every reported violation: each is either a real latent
   bug (fix it / file it) or a too-strict check (refine accounting/placement).
3. **Wire the debug assert into `validate_network`** and run the full suite.
   Green ⇒ done. Any panic ⇒ a real invariant break the old code hid, or a
   calibration miss — resolve per §7's rule.
4. **Land the property suite** (§6.2) last, once the checker is trusted, so its
   failures are unambiguous.

---

## 9. Why these checks (traceability)

| Invariant | Bug it would have caught |
| --- | --- |
| `UnresolvedRecordName` (R2) | `closure`/`collect` left with a stale record name after the rewrite-walk omission — *this session's bug*. Crucially R2 checks the **outcome** (does it resolve?), so it catches a missed walk **regardless of which node type was forgotten** — the guarantee the hand-maintained downcast lists lack. |
| `CacheNone` (B2) | the original cache-invariant rename wire-loss. |
| `DuplicateParamId` / `ParamIdFloor` (B3) + the oracle under save→load / duplicate | the `param_id` recycling corruption (`design_parameter_wire_stability.md`). |
| `ArgCountMismatch` (B1) | any future repair pass that pads/truncates onto the wrong pins. |

---

## 10. Extensibility (why this is Phase 0, not a one-off)

The catalogue is the fixture every later phase plugs into:

- **Phase 1** (param-id wire dests) adds one invariant: `dest_param_id` resolves
  to a real param, and the derived positional index agrees with it.
- **Phase 2** (record-def id interning) flips `UnresolvedRecordName` from "name
  resolves" to "interned id resolves" — same check, stronger guarantee.
- **Phases 4–5** (type/pin ids) add id-uniqueness + floor checks to
  `check_document_invariants`, exactly where B3/B4 already live.

The checker grows monotonically with the migration and guards each step as it
lands. See `doc/design_identity_vs_naming.md` §9 for the full phase sequence.
