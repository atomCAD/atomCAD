# Programmability Improvements — Idea Capture

**Date:** 2026-04-29
**Status:** Idea inventory. Not a design. Topics here will be promoted into individual design docs as we pick them up.

## Context

Triggered by the question: how do we make atomCAD node networks more programmable without bloating the evaluator? Initial seed ideas (array indexing in `expr`, switch/case in `expr`, freer partial application, closure perf concerns) were expanded by reading the reference guide (`doc/reference_guide/node_networks.md`, `doc/reference_guide/nodes/math_programming.md`) and the closure machinery (`rust/src/structure_designer/evaluator/function_evaluator.rs`, `rust/src/structure_designer/nodes/map.rs`, `rust/src/structure_designer/evaluator/network_evaluator.rs`). Elm was used as a reference language for which features are worth borrowing.

The list below is filtered down to ideas the team confirmed as valuable. A short "considered and dropped" section at the end records ideas we explicitly chose not to pursue, so we don't re-relitigate them.

---

## A. Expression language extensions

Pure additions to the `expr` language. No changes outside `expr/`. Compose well with the array-literal work that just landed.

- **Array indexing.** `xs[i]`, `xs[i][j]`. Bounds-check error reported the same way `inv3` reports a singular matrix.
- **`case … of` expression.** Switch over `Int`/`Bool`/`String` with `_` default. Avoids deep `if … else if … else if …` ladders. Fits the pattern of the existing `if … then … else …` expression.
- **`let` bindings.** `let r = sqrt(x*x + y*y) in if r > 0 then r else 1`. Lets you share intermediates without spawning extra `expr` nodes.
- **Small built-ins.** `len(xs)`, `min`/`max`/`clamp`, `mod`/`rem` on floats, `pow_int`. Each individually small, but constantly missed.

Why this tier first: tiny scope (parser + evaluator inside `expr`), obvious wins, no node-network changes.

---

## B. Node-network programming primitives

Structural additions to the node-network programming model. Each is its own design.

- **More higher-order function nodes.** `filter`, `fold`/`reduce`, `zip2`/`zip3`, `concat`, `flatten`. Plus a `range_grid` (2D/3D) producing `[IVec2]`/`[IVec3]` — constantly needed for defect lattices and supercell sweeps.
- **Tuples (or records).** Today there's no way to thread two correlated values through a HOF — multi-output pins don't flow through `[T]`. With `(IVec3, Float)` you can write `[(IVec3, Float)]` and `map`/`filter`/`fold` over it.
  - Open question: positional tuples (`.0`, `.1`) vs. labeled records (`{ pos: IVec3, kind: Int }`). Tuples are cheaper to land; records are nicer long-term. Pick when we design.
- **`Maybe` / `Result` as built-in sum types.** With `case` in `expr`, lets `inv3`, parse-failures, etc. propagate cleanly without `NetworkResult::Error` short-circuiting whole subgraphs. Optional but principled. Skip if it doesn't pull its weight against the other items.

---

## C. Closure / partial application changes

### C1. Free-parameter selection in partial application

Today, when a function pin requests a function value from a multi-parameter node, the evaluator captures values for **all** parameters (`network_evaluator.rs:1026-1043`) and `map`-style consumers always overwrite parameter index 0 (`function_evaluator.rs:99-103`). The type-system-level rule "extras must be at the end" enforces this convention statically.

Idea: let the user mark, per wire into a function pin, **which** parameter of the source node is the free slot — instead of always parameter 0. Mechanically:
- Per-wire annotation: "free-pin index" (or set of indices for multi-arg function types).
- `Closure` records which captured slot(s) are placeholder vs. captured.
- `set_argument_value` consults the mapping.
- Editor UI: when wiring a multi-param node into a function pin, the user picks which pin(s) stay free.

Workaround today: a custom subnetwork that reorders parameters. So this is an ergonomics improvement, not a capability gap. Still valuable.

### C2. Closure runtime cleanups (small perf, gated on a benchmark)

Three targeted improvements to the closure path. Worth doing only if a measured workload shows them, but cheap when we touch this code anyway.

- **Eliminate redundant captured-arg clone** in `FunctionEvaluator::new` (`function_evaluator.rs:84` clones a value that was just cloned at closure construction in `network_evaluator.rs:1026-1043`). Move-from-`Closure`, or `Rc`-wrap.
- **Special-case expr-node closures.** When the function node is an `expr`, skip building the temp `NodeNetwork` and the `NetworkEvaluator` round-trip; evaluate the cached AST directly against bound args.
- **Reuse `NetworkEvaluationContext`** across `map` iterations instead of `::new()` per element (`function_evaluator.rs:117-124`). Preserves whatever caches survive within an eval.

None of these are transformative on their own. Keep as a single small design once we have a benchmark that justifies it.

---

## D. Inline anonymous subnetworks

The actual gap behind "name a subnetwork just to use it once": when you want a function value whose body is something `expr` can't express (build a `Blueprint`, run `materialize`, …), today the only option is to create a named, top-level subnetwork. For numerical bodies this isn't a problem — `expr` already serves as the anonymous-function form, since any `expr` node with free parameters exposes a function pin.

Idea: let a `NodeNetwork` contain another `NodeNetwork` inline as a single node, with its own internal nodes and a designated return. The factor-selection-into-subnetwork action already exists; the change is to allow the result to live inline (anonymous, scoped to its parent) instead of becoming a top-level named network.

Confirmed as a real user request. Likely the highest-leverage structural item on this list. Most of the work is editor / rendering / scoping, not language semantics.

---

## E. Reference: how closures work today

Recorded for future-us so we don't re-derive it.

- `Closure` (`evaluator/network_result.rs:213-218`) = `{ node_network_name, node_id, captured_argument_values: Vec<NetworkResult> }`.
- Closure construction (`network_evaluator.rs:1023-1043`): when an `output_pin_index == -1` request hits a node, the evaluator evaluates **all N input pins** of that node and stores the results in `captured_argument_values`. There is no per-pin "this is free" vs "this is captured" distinction at construction time.
- `FunctionEvaluator::new` (`function_evaluator.rs:40-97`) builds **one** temp `NodeNetwork` per HOF call (not per element), containing the function's main node plus N `value` nodes (one per captured arg) wired into it. Construction clones each captured value a second time (line 84).
- Per-element call cost in `map` (`map.rs:89-107`): one `set_argument_value(0, …)` (replaces `ValueData` in the temp network) + one full `NetworkEvaluator::evaluate` traversal of the function's subnetwork, with a fresh `NetworkEvaluationContext::new()` each iteration (no cross-iteration memoization beyond `csg_conversion_cache` which lives on the evaluator itself).
- The "free parameter" today is hard-coded to parameter 0 of the source node; the trailing-extras rule in the reference guide is the type-system enforcement of that runtime convention.

---

## Considered and dropped

Recorded so we don't accidentally re-promote them.

- **Textual lambdas inside `expr` (`\x -> x*2 + 1`).** Redundant with what already exists: an `expr` node with a free parameter is already a function value of the appropriate type, reachable via its function pin. The "name a subnetwork just to use it once" pain is real but lives in the *non-numerical* case, which lambdas in `expr` would not solve. The right fix for that case is **D. Inline anonymous subnetworks**.
- **"Pre-compile expr-bodied functions" as a redesign.** There is no first-class concept of an expr-bodied function. The phrase was loose. The salvageable kernel of the idea is captured as **C2: special-case expr-node closures**, which is a small targeted optimization, not a structural change.
- **Generic / polymorphic custom nodes (`length : [a] -> Int`).** Real win for library authors, but significant evaluator + type-checker rework. No concrete user demand yet. Park.
- **Currying as a language feature.** Elm gets it for free; atomCAD doesn't need it. Nodes are inherently multi-arity, the closure-with-extras model handles partial application directly, and **C1: free-parameter selection** covers the missing flexibility. Currying buys nothing on top.
- **Pipe operator `|>`.** Wires already are the pipe operator.

---

## Suggested promotion order

Not a commitment, just a starting point when we pick what to design next:

1. **A** (`let`, indexing, `case`, small built-ins). Bundle as one `expr`-language design.
2. **D** (inline anonymous subnetworks). User-requested, highest structural leverage.
3. **B.1** (more HOFs) and **B.2** (tuples/records). Probably one design per item.
4. **C1** (free-parameter selection). Standalone.
5. **C2** (closure perf cleanups). Only after a benchmark.
6. **B.3** (`Maybe`/`Result`). Optional.
