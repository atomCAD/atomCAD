# Catastrophic Wire Loss on Network Rename — Investigation Report

**Date:** 2026-06-17
**Reporter of bug:** mechadense (user), recurrence of an earlier rename-corruption report
**Investigated by:** Claude (Opus 4.8)
**Status:** Root cause identified (high confidence); fix + regression test not yet written

## Source files compared

- **Before (known-good):** `TTPL_tool-to-proxy-linking_2026-06-16_17-51-CET (1).cnnd`
- **After (corrupted):** `TTPL_tool-to-proxy-linking_2026-06-17_08-33-CET_CATASTROPHIC_DAMAGE_TO_WIRING.cnnd`

Both files: version 5, 22 node networks, 2 record type defs.

## 1. What operations happened

The session performed a batch of namespace / leaf renames and record-def moves:

| Old name | New name |
|---|---|
| `TTPL.0_combinatorics` | `proxy(111).0_combinatorics` |
| `TTPL.0_proxy(111)` | `proxy(111).0_proxy(111)` |
| `TTPL.0_tool-on-proxy(111)` | `proxy(111).0_tool-on-proxy(111)` |
| `TTPL.hexirod(111)` | `proxy(111).hexirod(111)` |
| `TTPL.hexirod(111)_R30°` | `proxy(111).hexirod(111)_R30°` |
| `TTPL.hexrod(111)` | `proxy(111).hexrod(111)` |
| `TTPL.hexrod(111)_R30°` | `proxy(111).hexrod(111)_R30°` |
| `TTPL.wall_centered` | `proxy(111).wall_centered` |
| `TTPL.z_combinatorics_zipWith-ISSUE` | `proxy(111).z_combinatorics_zipWith-ISSUE` |
| `TTPL.tool-example_no-toothnails` | `proxy(111)-to-tool.tool-example_no-toothnails` |
| `TTPL.tool-scaffold` | `proxy(111)-to-tool.tool-scaffold` |
| `TTPL` (leaf) | `proxy(111).0-proxy(111)` |
| `UNTITLED` | `random-test` |
| record def `proxy(111)_config` | `proxy(111).proxy(111)_config` |
| record def `named_Crystal` | `util.named_Crystal` |

I.e. a `TTPL → proxy(111)` namespace rename, two networks shifted to `proxy(111)-to-tool`, two standalone leaf renames, and two record-def moves. This matches the user's description.

## 2. What wires were deleted

**127 of 357 wires (36%) were deleted. Pure deletion — `GAINED = 0`.** No wire was rerouted to a different pin; wires were simply dropped.

The loss is **perfectly confined by destination node type**:

| Destination node type | lost | survived |
|---|---|---|
| `parameter` | **52** | 0 |
| `expr` | **41** | 0 |
| `map` | 9 | 1 |
| `collect` | 7 | 0 |
| `record_destructure` | 6 | 0 |
| `product` | 4 | 0 |
| `record_construct` | 4 | 0 |
| `filter` | 2 | 0 |
| `foreach` | 2 | 0 |
| **all other types** (intersect, half_space, vec3, structure, structure_move, free_move, sphere, extrude, materialize, **custom-network instances**, closure, apply, export_xyz, atom_edit, …) | **0** | hundreds |

Per-network loss (renamed **and** unrelated networks were hit):

```
 39  TTPL.0_combinatorics
 18  TTPL.0_proxy(111)
 13  TTPL.z_combinatorics_zipWith-ISSUE
 13  experiment.freefrom_iprism      <- NOT part of any rename
  9  TTPL.hexrod(111)
  9  TTPL.hexrod(111)_R30°
  7  experiment.freeform_prism       <- NOT part of any rename
  5  TTPL.wall_centered
  3  TTPL.hexirod(111)
  3  TTPL.hexirod(111)_R30°
  3  BIG_tip                         <- NOT part of any rename
  3  experiment.hexirod(111)_copy    <- NOT part of any rename
  2  z_ISSUE.proxy(111)att2
```

The unrelated networks being affected confirms a **global** pass, not a per-rename cascade.

### Direct spot check

`TTPL.hexirod(111)` node 20 (`parameter "half_width"`, `param_id: 1`) is byte-identical before/after (same id, name, position, param_id) except:

- before: `arguments[0].incoming_wires = [{source_node_id: 19, NodeOutput pin 0}]`
- after:  `arguments[0].incoming_wires = []`

This is the "wires to defaults" case the user described (a `parameter` node's arg0 is its optional default-value input).

### The common property of the 9 victim types

All nine have a **derived / computed pin layout** (`custom_node_type`), not a statically fixed one:

- `parameter` — default-value pin, typed from the parameter's data type
- `expr` — variadic args = free variables parsed from the expression
- `product`, `record_construct`, `record_destructure` — args = record schema fields
- `map`, `filter`, `foreach` — HOF args derived from element/input types (and the `f` pin)
- `collect` — derived element type

Every fixed-arity node type was untouched.

## 3. Root cause (high confidence)

Not `apply_rename_core` (it only swaps name strings) and **not** the by-name rebuild of rename-target instances (custom-network instances all survived). The culprit is the **full-refresh repair pass** the rename triggers via `mark_full_refresh()` → `NodeTypeRegistry::repair_node_network`:

1. `repair_node_network` (`rust/src/structure_designer/node_type_registry.rs:2020-2040`) re-derives every node's `custom_node_type` with **`refresh_args = true`**, special-cased to `false` **only for `apply`** (line 2032).
2. `custom_node_type` is **not serialized** (verified — absent from the `.cnnd` JSON); it is a cache rebuilt after load.
3. In `NodeNetwork::set_custom_node_type(Some(custom), refresh_args = true)` (`rust/src/structure_designer/node_network.rs:717-787`): when the node's *existing* cache is `None` (or its parameters do not match the freshly-derived ones by id/name), `can_preserve` is false, the wire-copy block is skipped, and `self.arguments = new_arguments` — a fresh vector of empty `Argument`s. **All incoming wires on that node are silently dropped.**

So the repair pass runs a by-name argument rebuild over derived-layout nodes whose cache is not in a matchable state at that instant, wiping their wires.

This is the **same bug class already documented and fixed for `apply`** (see the comments at `node_type_registry.rs:2021-2031` and the AGENTS.md note: "a by-name rebuild against an under-derived layout silently drops arg wires"). `apply` received a `refresh_args = false` positional-preservation guard; the other eight derived-layout node types (`parameter`, `expr`, `product`, `record_construct`, `record_destructure`, `map`, `filter`, `foreach`, `collect`) did **not**, so they remain exposed.

### Why this explains every observation

- **"wires to defaults"** → `parameter` arg0 (default-value wire), 52 of 127.
- **"sometimes much more"** → networks rich in `expr` / HOF / record nodes.
- **"sometimes none"** → networks built only from fixed-arity nodes.
- **"networks not involved in the rename affected"** → `mark_full_refresh()` repairs all networks, not just renamed ones.

## 4. Relationship to the undo question

`RenameNetworkCommand` / `RenameNamespaceCommand` are **replay-based, not snapshot-based** — they store only old/new name pairs and re-run `apply_rename_core` in reverse. Because the wire loss happens in the refresh/repair pass (which `apply_rename_core` does not capture), **undo cannot restore the dropped wires**, and a namespace undo re-runs `repair_all_networks()`, which can drop more. This is why undo is not a safety net here.

## 5. Suggested next steps

1. **Regression test (cheap, do first):** load a minimal before-network with a `parameter` node carrying a default-value wire (plus a second network so a rename is meaningful), trigger a rename, assert the wire survives. The before-file here is already a complete repro seed.
2. **Fix direction:** extend the positional-preservation currently special-cased for `apply` to all data-derived-layout node types — ideally by having `repair_node_network` preserve `arguments` positionally for **any** node whose layout is data-derived, rather than maintaining a per-type allowlist. Verify against `parameter`, `expr`, `product`, `record_construct`, `record_destructure`, `map`, `filter`, `foreach`, `collect`.
3. **Investigate the precise ordering trigger:** confirm *why* the derived-node caches are `None`/mismatched at `repair_node_network` time during a rename (cache-population ordering vs. cache invalidation on rename). The regression test will expose this directly.
4. **Switch rename undo to snapshot-based** (consistency with `DeleteNetworkCommand` / `FactorSelectionCommand` / `TextEditNetworkCommand`) so undo becomes genuinely lossless and non-destructive.

## Appendix — method

Wires were canonicalized per node as `(arg_index, source_node_id, source_pin, source_scope_depth)`, recursing into HOF zone bodies (`Node.zone`) and including `zone_output_arguments`. Networks were matched old→new by the rename map above; node identity within a network is by `id` (stable across rename — verified). Lost = present in before, absent in after for the same `(scope_path, node_id)` key; gained = the reverse. Gained was 0 everywhere, establishing pure deletion.
