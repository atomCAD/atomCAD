# Higher-order array nodes: `filter` and `fold`

## Scope

`map` already exists. After it, the two most essential higher-order array operations are **filter** (selection) and **fold** (reduction). This doc designs only those two as **node-graph nodes**. Expression-language equivalents are explicitly out of scope (they would require lambdas, which the language does not have). See `rust/src/structure_designer/nodes/map.rs` for the implementation pattern these nodes follow.

Out of scope (deferred):

- `flat_map` / `concat_map`. Useful but not in this drop.
- `any` / `all` / `count` / `sum` / `min` / `max`. All expressible as `fold` plus a small `f`.
- `find` / `find_index`. Needs an optional / sentinel convention we have not designed yet.
- `sort` / `sort_by` / `group_by`.
- `zip` (we have no tuple type).
- `reduce` (the `Acc = T`, no-init variant). See decision table.
- Right-associative `foldr` and any short-circuiting variant.
- Expression-language `filter(...)` / `fold(...)` functions.

## How closures work in this codebase (recap)

Both nodes consume a function value via a function-pin input. A few invariants that drive the design:

- A function pin reads its source with `output_pin_index == -1`. The evaluator eagerly evaluates *every* argument of the source node and packages them into `Closure { node_network_name, node_id, captured_argument_values }` (`evaluator/network_evaluator.rs:1023-1043`). Unwired source pins evaluate to `NetworkResult::None`.
- `FunctionEvaluator::new(closure, registry)` builds a throwaway one-node network whose argument pins are fed by `value` nodes — one per captured arg. `set_argument_value(i, v)` overwrites the i-th value node (`evaluator/function_evaluator.rs:33-113`). `evaluate()` runs the standard evaluator with a fresh `NetworkEvaluationContext` (no caching across calls).
- A closure is therefore a **partially-applied node**. A higher-order operator decides which leading parameters it overwrites per iteration (the "free" parameters); the rest stay as captured at construction time (the "bound" / pre-wired parameters).
- For `map`, only argument 0 is overwritten per iteration (`map.rs:91-107`). For `filter`, this doc keeps that contract: the iterating node overwrites argument 0 only. For `fold`, the iterating node overwrites arguments 0 and 1 (accumulator, then element).

This means the user's `f` node must declare its first one or two parameters as the iteration variable(s), in the right order. Any further parameters are pre-bound when the closure is captured. This matches `map`'s existing convention exactly.

## Design decisions

| Question | Decision |
|---|---|
| Node names | `filter`, `fold` — short, mirrors `map`. No `array_*` prefix because, like `map`, they're higher-order operations rather than data-shape primitives. |
| `filter` element-type configuration | One stored property `element_type: DataType` (matches `map`'s `input_type` / `output_type`). Output element type equals input element type. |
| `fold` shape | Proper foldl: `(xs: Array[T], init: Acc, f: (Acc, T) -> Acc) -> Acc`. Allows reducing `Array[Int]` to a `Float`, `Array[Blueprint]` to a `Blueprint`, etc. The strictly simpler "reduce" form (`Acc = T`, no `init`, runtime error on empty) was rejected: too restrictive, and `init` makes the empty-array case total instead of partial. |
| `fold` type configuration | Two stored properties: `element_type: DataType` (T) and `accumulator_type: DataType` (Acc). |
| `f` argument order in `fold` | `(acc, elem) -> acc` (Rust `Iterator::fold`, Haskell `foldl`). Argument 0 is the accumulator, argument 1 is the element. |
| Iteration direction | Left-to-right (index 0 → len-1). No right-fold variant. |
| Pin order on the node | `xs, f` for `filter`; `xs, init, f` for `fold`. Data first, function last — mirrors `map`'s `xs, f`. |
| Output type | `filter`: `Array[ElementType]`, declared with `OutputPinDefinition::single_fixed(...)`. `fold`: `AccumulatorType`, also `single_fixed`. Both are determined by the node's stored properties, exactly like `map`. No `SameAsArrayElements` polymorphism — the explicit-property pattern is what `map` uses and is consistent with the rest of the array nodes. |
| Unconnected required input | `evaluate_arg_required` produces an `Error("… input is missing")` which propagates. Same convention `map` uses; keeps the higher-order array nodes consistent. |
| Empty input array | `filter` returns `Array([])`. `fold` returns `init` unchanged, no `f` calls. |
| `f` returning the wrong type at runtime | `filter` keeps a single explicit `Bool` check on `f`'s result because the predicate's truthiness is not encoded in the pin types in any sharper way: the `f` pin's declared output is `Bool`, but the result still has to be matched to decide push-vs-skip, so the "non-Bool falls through" branch is not extra defense, it falls out of the `match`. `fold` adds no runtime type check on `f`'s result — validation rejects mistyped `f` connections, and `acc`'s type is whatever `f` returns; we trust validation here rather than re-checking on every iteration. |
| Predicate negation in `filter` | Not configurable. Users wanting a "reject" filter use `expr` returning `!cond`. Keep the node single-purpose. |
| Text-format syntax | Same shape as `map`: `name = filter { element_type: T, xs: src, f: @pred }` and `name = fold { element_type: T, accumulator_type: Acc, xs: src, init: i, f: @combine }`. The `@nodeName` form references a function pin (output pin -1). |

---

## Phase 1 — `filter` node

### Behavior

Returns a new array containing the elements of `xs` for which `f(elem)` is `true`, in the original order.

**Properties**

- `element_type: DataType` — the element type T. Determines both the input array's element type and the output array's element type.

**Input pins**

- `xs: Array[ElementType]` — the array to filter.
- `f: ElementType -> Bool` — the predicate. The user's `f` node must have its first parameter typed `ElementType`; any further parameters are pre-bound at closure capture.

**Output pin**

- `out: Array[ElementType]` — declared via `OutputPinDefinition::single_fixed(DataType::Array(Box::new(self.element_type.clone())))`.

**Runtime semantics**

- Use `evaluate_arg_required` for both `xs` and `f` (same as `map`), in declaration order: `xs` first, then `f`. An unconnected required input therefore surfaces as `Error("… input is missing")` and propagates. Errors in `xs` or `f` also propagate. Required-input checks are **not** short-circuited by an empty `xs`: if `f` is unwired, the node errors with `"f input is missing"` regardless of whether `xs` is empty. This keeps `filter` symmetric with `map` and `fold` and avoids a special-case branch in eval.
- Empty `xs` (with `f` wired) → `Array([])`. `f` is never called.
- For each element in order: build a `FunctionEvaluator` once, then per-element call `set_argument_value(0, elem); evaluate(...)`. Match the result:
  - `Bool(true)` → push `elem` into the output.
  - `Bool(false)` → skip.
  - `Error(_)` → propagate immediately.
  - Anything else (including `None`) → `Error("filter: f returned non-Bool")`. We deliberately collapse `None` into the same error rather than propagating it as `None`: validation has already required `f`'s declared output type to be `Bool`, so a `None` here means a deeper input was unwired *inside* `f`'s subnetwork. Surfacing that as a typed error makes the failure obvious instead of silently producing an empty array.

### Implementation sketch

New file `rust/src/structure_designer/nodes/filter.rs`. Mirror `map.rs` closely:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterData {
    pub element_type: DataType,
}

impl NodeData for FilterData {
    fn calculate_custom_node_type(&self, base: &NodeType) -> Option<NodeType> {
        let mut custom = base.clone();
        custom.parameters[0].data_type =
            DataType::Array(Box::new(self.element_type.clone()));
        custom.parameters[1].data_type = DataType::Function(FunctionType {
            parameter_types: vec![self.element_type.clone()],
            output_type: Box::new(DataType::Bool),
        });
        custom.output_pins =
            OutputPinDefinition::single_fixed(DataType::Array(Box::new(self.element_type.clone())));
        Some(custom)
    }

    fn eval(&self, evaluator, stack, node_id, registry, _decorate, context) -> EvalOutput {
        // 1. xs = evaluator.evaluate_arg_required(..., 0); if Error → propagate.
        //    Extract NetworkResult::Array(xs) (any other variant is an internal-error case
        //    that mirrors map's "Expected array of elements" branch).
        // 2. f = evaluator.evaluate_arg_required(..., 1); if Error → propagate.
        //    Extract NetworkResult::Function(closure).
        // 3. let mut fe = FunctionEvaluator::new(closure, registry);
        //    let mut out = Vec::new();
        //    for elem in xs {
        //        fe.set_argument_value(0, elem.clone());
        //        match fe.evaluate(evaluator, registry) {
        //            NetworkResult::Bool(true)  => out.push(elem),
        //            NetworkResult::Bool(false) => {}
        //            err @ NetworkResult::Error(_) => return EvalOutput::single(err),
        //            _ => return EvalOutput::single(
        //                     NetworkResult::Error("filter: f returned non-Bool".into())),
        //        }
        //    }
        //    EvalOutput::single(NetworkResult::Array(out))
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        vec![("element_type".into(), TextValue::DataType(self.element_type.clone()))]
    }
    fn set_text_properties(&mut self, props) -> Result<(), String> { /* same shape as map */ }
    fn get_parameter_metadata(&self) -> HashMap<...> {
        // both pins required
    }
    fn clone_box(&self) -> Box<dyn NodeData> { Box::new(self.clone()) }
    fn provide_gadget(&self, _) -> Option<Box<dyn NodeNetworkGadget>> { None }
    fn get_subtitle(&self, _) -> Option<String> { None }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "filter".into(),
        description: "Returns the elements of `xs` for which the predicate `f` returns true, \
                      preserving order.".into(),
        summary: None,
        category: NodeTypeCategory::MathAndProgramming,
        parameters: vec![
            Parameter { id: None, name: "xs".into(),
                        data_type: DataType::Array(Box::new(DataType::Float)) },
            Parameter { id: None, name: "f".into(),
                        data_type: DataType::Function(FunctionType {
                            parameter_types: vec![DataType::Float],
                            output_type: Box::new(DataType::Bool),
                        }) },
        ],
        output_pins: OutputPinDefinition::single_fixed(
            DataType::Array(Box::new(DataType::Float))),
        public: true,
        node_data_creator: || Box::new(FilterData { element_type: DataType::Float }),
        node_data_saver: generic_node_data_saver::<FilterData>,
        node_data_loader: generic_node_data_loader::<FilterData>,
    }
}
```

Use **`map`'s `evaluate_arg_required` style** for both `xs` and `f`: missing required inputs surface as `Error("… input is missing")` rather than `None`. This keeps `filter` and `fold` in lockstep with the higher-order operator they sit next to (`map`), at the cost of diverging from `array_at` / `array_concat` / `array_append`, which use the `evaluate_arg`+match-`None` style. We accept that divergence: `map`/`filter`/`fold` are the higher-order family and their conventions should match each other; the data-shape array nodes form a separate cluster. Aligning all six is out of scope for this design.

### Phase 1 tests

New file `rust/tests/structure_designer/filter_test.rs`, registered in `rust/tests/structure_designer.rs`.

The test rig follows the existing `map`-on-test pattern: build a small subnetwork that serves as the predicate (e.g. an `expr` node with one parameter and an expression like `x > 0` or `x % 2 == 0`), wire it into a `filter` node along with a literal array, evaluate, assert.

| Scenario | Expected |
|---|---|
| `element_type: Int`, xs=`[1,2,3,4,5]`, f = `x > 2` | `[3, 4, 5]` |
| `element_type: Int`, xs=`[1,2,3,4]`, f = `x % 2 == 0` | `[2, 4]` |
| `element_type: Int`, xs=`[1,2,3]`, f always returns `true` | `[1, 2, 3]` |
| `element_type: Int`, xs=`[1,2,3]`, f always returns `false` | `[]` |
| `element_type: Int`, xs=`[]`, f wired (body irrelevant) | `[]`, f never called |
| `element_type: IVec3`, xs of 4 ivec3, f checks `x.z > 0` | only positive-z elements, in order |
| `element_type: Int`, xs unconnected, f wired | `Error("xs input is missing")` (or whatever `evaluate_arg_required` emits) |
| `element_type: Int`, xs wired, f unconnected | `Error("f input is missing")` |
| Both unconnected | `Error("xs input is missing")` (first one checked, even if `xs` would have been empty) |
| `xs=[]`, f unconnected | `Error("f input is missing")` (required-input check is not short-circuited by empty `xs`) |
| Predicate evaluates to an error mid-iteration | error propagates immediately |
| Predicate returns `None` (e.g. predicate's `expr` references an unwired pin) | `Error("filter: f returned non-Bool")` |
| Predicate's `f` node has a 2nd pre-bound parameter (e.g. a `threshold` Int captured from a `value` node) | only elements above threshold are kept (proves partial-application) |

Plus:

- **Snapshot test** — register `filter` in `rust/tests/structure_designer/node_snapshot_test.rs`, `cargo insta review`.
- **Text-format roundtrip** — add cases to `text_format_test.rs` for `filter` with `element_type: Int` and `element_type: IVec3`. Roundtrip the network, including the `f: @predicate` reference.
- **`.cnnd` roundtrip** — fixture under `rust/tests/fixtures/` with a `filter` node + tiny predicate subnetwork; assert load → save → reload is stable.

### Phase 1 implementation checklist

1. [ ] `rust/src/structure_designer/nodes/filter.rs`.
2. [ ] Register module in `rust/src/structure_designer/nodes/mod.rs`.
3. [ ] Register in `rust/src/structure_designer/node_type_registry.rs::new()` (next to `map_get_node_type()`).
4. [ ] Tests in `rust/tests/structure_designer/filter_test.rs`, registered in `rust/tests/structure_designer.rs`.
5. [ ] Snapshot in `node_snapshot_test.rs` + `cargo insta review`.
6. [ ] Text-format roundtrip cases.
7. [ ] `.cnnd` roundtrip fixture.
8. [ ] Reference-guide entry for `filter` in `doc/reference_guide/nodes/math_programming.md` (between `map` and any future arrival).
9. [ ] `cd rust && cargo fmt && cargo clippy && cargo test`.
10. [ ] No FRB regen needed (no API surface change beyond the new node type).

---

## Phase 2 — `fold` node

### Behavior

Reduces `xs` to a single accumulator value by repeatedly applying `f(acc, elem)`, starting from `init`, left-to-right.

```
fold([], init, f)            == init
fold([a], init, f)           == f(init, a)
fold([a, b, c], init, f)     == f(f(f(init, a), b), c)
```

**Properties**

- `element_type: DataType` — the element type T.
- `accumulator_type: DataType` — the accumulator / output type Acc.

**Input pins**

- `xs: Array[ElementType]` — the array to reduce.
- `init: AccumulatorType` — the starting accumulator value.
- `f: (AccumulatorType, ElementType) -> AccumulatorType` — the combining function. The user's `f` node must declare its first parameter typed `AccumulatorType` and its second typed `ElementType`. Any further parameters are pre-bound at closure capture.

**Output pin**

- `out: AccumulatorType` — declared via `OutputPinDefinition::single_fixed(self.accumulator_type.clone())`.

**Runtime semantics**

- Use `evaluate_arg_required` for `xs`, `init`, and `f`, in declaration order: `xs`, then `init`, then `f`. Any unconnected required input surfaces as `Error("… input is missing")` and propagates. Errors in any input also propagate. Required-input checks are **not** short-circuited by an empty `xs`: if `init` or `f` is unwired, the node errors regardless of whether `xs` is empty. This keeps `fold` symmetric with `map` and `filter`.
- Empty `xs` (with `init` and `f` both wired) → return `init` unchanged. `f` is never called.
- Build the `FunctionEvaluator` once. For each element, set arg 0 = current accumulator, arg 1 = current element, evaluate, replace accumulator with the result.
- On any iteration: if the result is an `Error`, propagate immediately. Otherwise the result becomes the new accumulator. **No runtime type check on `f`'s output.** Validation rejects mistyped `f` connections at wire time; trust that. If a deeper input inside `f`'s subnetwork is unwired the result will be `None`, and `None` will then become the accumulator — that flows back out as the node's result, which is the same behavior any other node in the network would exhibit when given an unwired input. We do not paper over it with a type check.

### Implementation sketch

New file `rust/src/structure_designer/nodes/fold.rs`. Same skeleton as `filter.rs`, with these differences:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldData {
    pub element_type: DataType,
    pub accumulator_type: DataType,
}

fn calculate_custom_node_type(&self, base: &NodeType) -> Option<NodeType> {
    let mut custom = base.clone();
    custom.parameters[0].data_type =                       // xs
        DataType::Array(Box::new(self.element_type.clone()));
    custom.parameters[1].data_type = self.accumulator_type.clone();   // init
    custom.parameters[2].data_type = DataType::Function(FunctionType {
        parameter_types: vec![
            self.accumulator_type.clone(),
            self.element_type.clone(),
        ],
        output_type: Box::new(self.accumulator_type.clone()),
    });
    custom.output_pins = OutputPinDefinition::single_fixed(self.accumulator_type.clone());
    Some(custom)
}

fn eval(...) -> EvalOutput {
    // 1. xs   = evaluator.evaluate_arg_required(..., 0);
    //          if Error → propagate. Extract Array (other variants → internal-error mirror
    //          of map's "Expected array of elements").
    // 2. init = evaluator.evaluate_arg_required(..., 1);
    //          if Error → propagate. Keep as NetworkResult (its runtime variant is
    //          already correct because validation enforced the pin type).
    // 3. f    = evaluator.evaluate_arg_required(..., 2);
    //          if Error → propagate. Extract Closure.
    //
    // let mut fe = FunctionEvaluator::new(closure, registry);
    // let mut acc = init;
    // for elem in xs {
    //     fe.set_argument_value(0, acc);
    //     fe.set_argument_value(1, elem);
    //     let next = fe.evaluate(evaluator, registry);
    //     if let NetworkResult::Error(_) = next {
    //         return EvalOutput::single(next);
    //     }
    //     acc = next;
    // }
    // EvalOutput::single(acc)
}
```

`get_text_properties` / `set_text_properties` carry both `element_type` and `accumulator_type`. `get_parameter_metadata` marks all three input pins as required. The base `get_node_type()` defaults both type properties to `DataType::Float` (matching `map`'s default).

### Phase 2 tests

New file `rust/tests/structure_designer/fold_test.rs`, registered in `rust/tests/structure_designer.rs`.

The combining-function rig uses an `expr` node whose parameters are named `acc` and `elem`. Same partial-application story as `map` and `filter`: extra captured parameters are pre-bound at closure capture.

| Scenario | Expected |
|---|---|
| `element_type: Int`, `accumulator_type: Int`, xs=`[1,2,3,4]`, init=`0`, f = `acc + elem` | `Int(10)` |
| Same as above, init=`100` | `Int(110)` |
| `element_type: Int`, `accumulator_type: Int`, xs=`[]`, init=`42`, f wired (body irrelevant) | `Int(42)` (f never called) |
| `element_type: Int`, `accumulator_type: Int`, xs=`[5]`, init=`0`, f = `acc + elem` | `Int(5)` (one call) |
| `element_type: Int`, `accumulator_type: Int`, xs=`[1,2,3]`, init=`1`, f = `acc * elem` (product) | `Int(6)` |
| `element_type: Int`, `accumulator_type: Int`, xs=`[3,1,4,1,5,9,2,6]`, init=`i32::MIN`, f = `if elem > acc then elem else acc` | `Int(9)` (max via fold) |
| **Acc differs from T**: `element_type: IVec3`, `accumulator_type: Int`, xs=`[ivec3(1,2,3), ivec3(4,5,6)]`, init=`0`, f's signature `(Int, IVec3) -> Int`, body = `acc + elem.x + elem.y + elem.z` | `Int(21)` (uses only operations already proven elsewhere — `.x`/`.y`/`.z` access on IVec3, integer `+` — no speculative type promotion in expr; verifies Acc and T can be genuinely different types) |
| Order matters — left-to-right: `element_type: Int`, `accumulator_type: Int`, xs=`[1,2,3]`, init=`0`, f = `acc * 10 + elem` | `Int(123)` (proves left-to-right order) |
| xs unconnected | `Error("xs input is missing")` |
| init unconnected | `Error("init input is missing")` |
| f unconnected | `Error("f input is missing")` |
| All three unconnected | `Error("xs input is missing")` (declaration order: xs, init, f) |
| `xs=[]`, init wired, f unconnected | `Error("f input is missing")` (required-input check is not short-circuited by empty `xs`) |
| `xs=[]`, init unconnected, f wired | `Error("init input is missing")` |
| f errors on the second call | error propagates, no third call |
| f's `f` node has a 3rd pre-bound parameter (a `factor` Int) — `acc + factor * elem` | uses factor (proves partial application of trailing args) |

Plus the standard insta snapshot, text-format roundtrip (one case with matching types, one with `accumulator_type: Int, element_type: IVec3` to exercise the cross-type roundtrip), and `.cnnd` roundtrip fixture.

### Phase 2 implementation checklist

1. [ ] `rust/src/structure_designer/nodes/fold.rs`.
2. [ ] Register module in `rust/src/structure_designer/nodes/mod.rs`.
3. [ ] Register in `rust/src/structure_designer/node_type_registry.rs::new()` (next to `filter`).
4. [ ] Tests in `rust/tests/structure_designer/fold_test.rs`, registered in `rust/tests/structure_designer.rs`.
5. [ ] Snapshot in `node_snapshot_test.rs` + `cargo insta review`.
6. [ ] Text-format roundtrip cases.
7. [ ] `.cnnd` roundtrip fixture.
8. [ ] Reference-guide entry for `fold` in `doc/reference_guide/nodes/math_programming.md` (next to `filter`).
9. [ ] `cd rust && cargo fmt && cargo clippy && cargo test`.
10. [ ] No FRB regen needed.

---

## Reference-guide updates

`doc/reference_guide/nodes/math_programming.md` gets two new top-level entries, placed after `map`. Drafted text below (drop straight into the file as-is — these are the actual section bodies, not a wrapping code block):

---

### filter

Returns the elements of `xs` for which the predicate `f` returns `true`, preserving order.

**Properties**

- `Element type` — the element type T of the input and output arrays.

**Input pins**

- `xs: Array[ElementType]` — the array to filter.
- `f: ElementType -> Bool` — the predicate.

**Behavior**

If either input is unconnected, the node produces an error (`xs input is missing` / `f input is missing`); both inputs must be wired even when `xs` would have been empty. Otherwise the node returns a new array containing every element of `xs` for which `f(elem)` evaluated to `true`, in the original order. An empty `xs` produces an empty array; `f` is never called. If `f` returns anything other than `Bool` (including `None` because a deeper input inside `f` is unwired), the node produces `Error("filter: f returned non-Bool")`.

The `f` function is supplied via the function pin (typically a small subnetwork or an `expr` node). Any extra parameters of `f` beyond the first are pre-bound at the time the function pin is wired — this is partial application, the same convention `map` uses (see the `map` section).

---

### fold

Reduces `xs` to a single value by repeatedly applying `f(acc, elem)`, starting from `init`, left-to-right:

- `fold([], init, f)        == init`
- `fold([a, b, c], init, f) == f(f(f(init, a), b), c)`

**Properties**

- `Element type` — the element type T of the input array.
- `Accumulator type` — the accumulator and output type Acc. Acc may differ from T; the closure's parameter pins use the same `Int ↔ Float` (and similar) conversions that any other pin connection does, so e.g. folding an `Array[Float]` into an `Int` accumulator works exactly because Float→Int truncation is already a supported pin conversion.

**Input pins**

- `xs: Array[ElementType]` — the array to reduce.
- `init: AccumulatorType` — the initial accumulator value.
- `f: (AccumulatorType, ElementType) -> AccumulatorType` — the combining function. Argument 0 is the accumulator, argument 1 is the current element.

**Behavior**

If any input is unconnected, the node produces an error (`xs input is missing` / `init input is missing` / `f input is missing`); all three inputs must be wired even when `xs` would have been empty. With everything wired, an empty `xs` returns `init` unchanged (`f` is never called). Otherwise the node walks `xs` left-to-right, replacing the accumulator with `f(acc, elem)` at each step, and returns the final accumulator value. If `f` errors on any iteration, the error propagates immediately and remaining elements are skipped.

`fold` is the universal aggregator: sum, product, min, max, "all true", "any true", and chained CSG (e.g. unioning a list of blueprints) are all special cases.

## Open questions

1. **Type-property count: one for `filter`, two for `fold`.** Is the asymmetry OK? `map` has two (`input_type`, `output_type`) because it can transform `Array[T] → Array[U]`. `filter`'s output element type is structurally identical to its input element type, so a second knob would be redundant — one property is the right count, not a regression from `map`. `fold` genuinely needs two because `Acc` is independent of `T`. No change recommended; flagged here only because the property counts differ across the three higher-order nodes.
2. **Should we add `flat_map` next?** Strong recommendation but explicitly deferred; this doc only delivers `filter` + `fold`.
