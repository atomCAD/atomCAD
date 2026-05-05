# `atom_replace` Programmatic Rules Input

## Status: Draft

## Motivation

The `atom_replace` node today carries its replacement rules as inline node-data (`replacements: Vec<(i16, i16)>`) edited from the Flutter side panel. This is fine for static substitutions ("replace all C with Si"), but it makes a whole class of useful operations awkward or impossible:

- **Computed mappings** — e.g. read a list of "defect descriptors" upstream and turn each one into a `(from, to)` rule.
- **Reused rule sets** — e.g. one `expr` produces a "doping schedule" that several `atom_replace` nodes consume.
- **Programmatic rule generation** — `product` of two element arrays into a sweep of substitutions.

Until now there was no node-network type rich enough to carry "a list of (from, to) pairs" along one wire. With record types (`doc/design_record_types.md`) shipped, there now is. This design adds an `Array[Record(...)]`-typed input pin to `atom_replace`. No other node changes; no new node type.

## Element Representation

**Atomic numbers (`Int`).** Justification:

- `Atom.atomic_number: i16` is the canonical internal representation. The whole crystolecule and atom-edit stack stores and matches on it directly. There is no parallel `Element` enum in the codebase.
- `AtomReplaceData.replacements: Vec<(i16, i16)>` already uses atomic numbers. The new pin reads the same representation; both sources land in the same `(i16, i16)` rule list before eval.
- The two sentinel atomic numbers — `0 = delete marker`, `-1 = unchanged marker` — already integrate with `atom_replace` eval. A record `{from: 6, to: 0}` means "delete all carbon," matching today's behavior. A target of `-1` would be ill-formed for a *replacement* rule and is rejected (see Validation).

The alternative — `String` chemical symbols — is rejected. It pushes parse/lookup into the eval hot path, has no canonical handling for the delete sentinel, and diverges from how every other node identifies elements.

## Built-in `ElementMapping` Record Type

`atomCAD` ships with a **built-in record type def** named `ElementMapping`:

```text
ElementMapping = { from: Int, to: Int }
```

This def is part of the application, not of any project file. It is registered once at registry construction time and lives alongside built-in node types in their own protected map (`built_in_record_type_defs`), parallel to the existing `built_in_node_types` vs. user-`node_networks` split. The user-facing `record_type_defs` map continues to hold *user-declared* defs only.

Properties of built-in record defs (introduced by this design):

- **Discoverable via the same lookup path.** `RecordType::Named(N)` resolution and the dropdowns in the type-selector / `record_construct` / `record_destructure` / `product` panels consult both maps and present a unified list.
- **Immutable.** `add_record_type_def` rejects names that collide with built-ins; `delete_record_type_def`, `rename_record_type_def`, and `update_record_type_def` refuse to operate on built-in names with a clear error message ("`ElementMapping` is a built-in record type").
- **Not serialized.** `.cnnd` save emits only user defs from `record_type_defs` (mirroring how built-in node types are not serialized either). Existing `.cnnd` files remain backward-compatible.
- **Reserved name.** `ElementMapping` is now a reserved identifier in the user-type namespace — networks and user record defs of that name are rejected at creation time. The namespace-collision check in `namespace_utils.dart` and the Rust-side validator both consult the built-in record-defs map.

Why introduce this concept now and not as part of `design_record_types.md`? Because today nothing in the system *needs* a named def to exist before users have a chance to declare one. `atom_replace.rules`, by being the first node-network feature to *require* a specific record shape on a built-in node's pin, is the first place where a built-in def pays off. Future built-in nodes that need shaped input (e.g. a future "isotope substitution" node, a defect-descriptor consumer) would land more such defs in the same map without further architectural change.

The `ElementMapping` def itself is a leaf — its fields are primitive `Int`s, no nested records — so it has no impact on the cycle check.

## Design

### 1. Pin signature

Add one input pin to `atom_replace`:

| Direction | Name    | Type                                          | Required |
|-----------|---------|-----------------------------------------------|----------|
| Input     | `molecule` | `HasAtoms`                                 | Yes      |
| Input     | **`rules`** | **`Array[Record(Named("ElementMapping"))]`** | **No**   |
| Output    | (pin 0) | `SameAsInput("molecule")`                     | —        |

The `rules` pin's record element type is the **named built-in** `ElementMapping` def declared above (`RecordType::Named("ElementMapping")`). This follows the project convention that pin types use named record defs; `RecordType::Anonymous` is reserved for the inferred output type of expression-language record literals and is never authored as a pin signature.

### 2. Data structure (unchanged storage)

`AtomReplaceData` is unchanged:

```rust
pub struct AtomReplaceData {
    pub replacements: Vec<(i16, i16)>,
}
```

The stored `replacements` continue to act as the **default rule list** when the `rules` pin is unconnected. They are a normal property of the node, edited as today via the side panel.

### 3. Eval semantics

Pseudocode:

```rust
fn eval(...) -> EvalOutput {
    let molecule_input_val = evaluate_arg_required(..., 0);
    if let NetworkResult::Error(_) = molecule_input_val { return EvalOutput::single(molecule_input_val); }

    // New: read the optional rules pin (param index 1).
    let rules = match evaluate_arg(..., 1) {
        NetworkResult::None         => self.replacements.clone(),  // disconnected → use stored
        NetworkResult::Error(e)     => return EvalOutput::single(NetworkResult::Error(e)),
        NetworkResult::Array(items) => parse_rules_from_records(items)?, // see below
        other                       => return EvalOutput::single(NetworkResult::Error(
                                            format!("atom_replace.rules: expected Array[Record], got {:?}", other.infer_data_type()))),
    };

    // Existing eval body, but driven by `rules` instead of `self.replacements`:
    EvalOutput::single(map_atomic(molecule_input_val, move |mut s| {
        apply_rules(&mut s, &rules);
        s
    }))
}
```

`parse_rules_from_records`:

```rust
fn parse_rules_from_records(items: Vec<NetworkResult>) -> Result<Vec<(i16, i16)>, String> {
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        let from = item.extract_record_field("from")
            .ok_or_else(|| format!("atom_replace.rules[{i}]: missing 'from' field"))?
            .extract_int()
            .ok_or_else(|| format!("atom_replace.rules[{i}].from: not an Int"))?;
        let to   = item.extract_record_field("to")
            .ok_or_else(|| format!("atom_replace.rules[{i}]: missing 'to' field"))?
            .extract_int()
            .ok_or_else(|| format!("atom_replace.rules[{i}].to: not an Int"))?;
        out.push((narrow_to_i16(from, "from", i)?, narrow_to_i16(to, "to", i)?));
    }
    Ok(out)
}
```

`narrow_to_i16` accepts the existing valid range — `0..=118` for replacement targets, plus the delete sentinel `0` and the *source* skip-set already enforced in `apply_rules` (sources of `0` or `-1` are silently ignored, exactly as today). Anything outside `0..=118` is an `Error`. Out-of-range targets are user-visible mistakes worth flagging at eval time, not silently truncating to i16.

The existing in-eval rule application logic is unchanged — it just takes a borrowed slice instead of `self.replacements.clone()`.

### 4. Connected-vs-stored semantics

When the pin is connected, the wired rules **replace** the stored `replacements` entirely. The stored list is not appended, not merged per-source-element — it is simply the fallback when the pin is unconnected.

Reasons to prefer "replace" over "merge":

- Mental model is one line: "if `rules` is wired, that's the rule set; otherwise the stored list is the rule set."
- Avoids a third semantic mode where some elements come from the pin and some from the property — debugging which rule won is hard, especially across schema-change repair.
- "Merge with pin overriding" can be expressed by the user themselves using `array_concat`, with no node-level support needed (`array_concat([...stored-shaped expr literal...], rules)`).

See "Open Question 2" if the team would prefer merge semantics instead.

### 5. UI

The existing inline rules editor in the side panel stays. When the `rules` pin is connected:

- The editor is rendered in a **disabled / grayed** state with a small annotation: *"Rules supplied by `rules` input. Disconnect to edit inline."* The stored values remain saved (and become live again on disconnect — no destructive UI side effect).
- The node subtitle drops the rules summary entirely — `get_subtitle` returns `None` when the `rules` pin is connected. (Project convention: when an input pin overrides a property, the subtitle simply omits that property; the upstream source node carries its own subtitle if needed.)
- No new editor types or widgets — the disable affordance is local to `lib/structure_designer/node_data/atom_replace_editor.dart`.

### 6. Validation and edge cases

- Disconnected pin → use stored `replacements`. Empty stored list → structure passes through unchanged. (No behavior delta from today.)
- Empty rules array (whether from disconnected pin with empty stored list, or from a wired empty array) → structure passes through unchanged.
- Rule with `from == 0` or `from == -1` → silently skipped, matching today's source-side filtering.
- Rule with `to` outside `0..=118` → eval-time `NetworkResult::Error`. (See Open Question 3 — strict reject vs. silent skip.)
- Rule with missing `from` or `to` field → eval-time `NetworkResult::Error`. (Type validation already enforces field presence at pin-connect time, since `ElementMapping` requires both fields. This branch is defensive against runtime values constructed via paths that bypass static checking.)
- Duplicate `from` keys in the rule list → "last rule wins" (HashMap insert order), unchanged from today.

### 7. Text-format properties

`get_text_properties` / `set_text_properties` continue to expose only the stored `replacements` list. The wired-pin rule list is, by definition, not stored — it is recomputed on every eval. Text-format authoring of `atom_replace` therefore behaves the same as today: there is no `rules:` text property.

### 8. Migration & compatibility

- Existing `.cnnd` files: no change. The new `rules` pin appears on every existing `atom_replace` node, unconnected. Stored `replacements` continue to drive eval. No serialization version bump.
- The `ElementMapping` built-in def is always present in the registry from `new()` onward; older projects do not need to (and cannot) declare it. Old projects that happen to have a *user* record def named `ElementMapping` will be flagged by the on-load namespace check — see "Open Question 1".
- Custom-network passthrough: `atom_replace` already lives behind `OutputPinDefinition::single_same_as("molecule")`; nothing about that changes.
- Undo: no new commands. The pin's connection state is captured by the existing `ConnectWire` undo machinery; the stored `replacements` are captured by the existing `SetNodeData`-style commands.

## Phasing

Two small phases. Phase A is the prerequisite (built-in record-def infrastructure plus the `ElementMapping` def itself); Phase B is the `atom_replace` pin. Phase A is independently useful — it is the same machinery future built-in-shape nodes will reuse.

### Phase A — Built-in record-def infrastructure + `ElementMapping`

1. **Registry split**: add `built_in_record_type_defs: HashMap<String, RecordTypeDef>` to `NodeTypeRegistry`, alongside the existing `built_in_node_types` and `record_type_defs`.
2. **Lookup chain**: every `record_type_defs.get(name)` site (registry resolution, dropdown population, validator) consults `built_in_record_type_defs` as a fallback. Centralize this in a small accessor (`fn lookup_record_type_def(&self, name: &str) -> Option<&RecordTypeDef>`).
3. **Mutation guard**: `add_record_type_def`, `delete_record_type_def`, `rename_record_type_def`, `update_record_type_def` reject names present in `built_in_record_type_defs` with a clear error.
4. **Namespace-collision check**: extend the user-type-name validator (Rust + `lib/structure_designer/namespace_utils.dart`) to refuse any new network or record-def name that collides with a built-in record-def name.
5. **Serialization**: leave `.cnnd` save unchanged — only `record_type_defs` is emitted; `built_in_record_type_defs` is never serialized. On load, no special handling is needed (built-ins are populated by `new()` before deserialization runs).
6. **Register `ElementMapping`** in `NodeTypeRegistry::new()`, immediately after the built-in node types are added:
   ```rust
   ret.built_in_record_type_defs.insert(
       "ElementMapping".to_string(),
       RecordTypeDef {
           name: "ElementMapping".to_string(),
           fields: vec![
               ("from".to_string(), DataType::Int),
               ("to".to_string(),   DataType::Int),
           ],
       },
   );
   ```
7. **Flutter UI**: the type-selector Record branch and the `record_construct` / `record_destructure` / `product` dropdowns iterate the unified list (built-ins ∪ user defs), sorted alphabetically. Built-in record defs are **not** listed in the user-types panel — same convention as built-in node types — so the schema editor and the rename/delete affordances need no special-case handling.

Tests (automated, `rust/tests/structure_designer/`):
- Lookup of `ElementMapping` resolves via `built_in_record_type_defs` regardless of whether `record_type_defs` is empty.
- `add_record_type_def("ElementMapping", ...)` is rejected (collides with built-in).
- `delete_record_type_def("ElementMapping")` is rejected with a clear error.
- `rename_record_type_def("ElementMapping", "X")` is rejected.
- `update_record_type_def("ElementMapping", new_fields)` is rejected.
- Adding a *network* named `ElementMapping` is rejected (namespace collision).
- `.cnnd` round-trip: build a project that uses `Record(Named("ElementMapping"))` on a pin or property, save, reload — the def resolves on the loaded side without `ElementMapping` ever appearing in the `record_type_defs` JSON section.
- Backward compat: load a fixture `.cnnd` saved before this feature (no `record_type_defs`, no built-ins) — the registry still has `ElementMapping` available at runtime.

Manual verification:
- Open a fresh project — `ElementMapping` appears in the type-selector Record branch and in the `record_construct` schema dropdown without any user setup.
- The user-types panel does **not** list `ElementMapping` (built-ins are not user-owned types).

### Phase B — `atom_replace.rules` pin

Depends on Phase A.

1. **`get_node_type`** in `nodes/atom_replace.rs`: append a second `Parameter`:
   ```rust
   Parameter {
       id: None,
       name: "rules".to_string(),
       data_type: DataType::Array(Box::new(DataType::Record(
           RecordType::Named("ElementMapping".to_string()),
       ))),
   }
   ```
   Mark it optional in `get_parameter_metadata` (`m.insert("rules".to_string(), (false, None))`).
2. **`eval`**: read pin index 1 with `evaluate_arg` (not `_required`); branch on `None` / `Array` / other, as in section 3 above.
3. **`get_subtitle`**: when called with a `connected_input_pins` set that contains `"rules"`, return `None`. Otherwise the existing rule-summary behavior is unchanged.
4. **`AtomReplaceEditor`** (`lib/structure_designer/node_data/atom_replace_editor.dart`): take the connected-pins set as input, render the rule list in a disabled state with the annotation when `rules` is in the set.

Tests (automated, `rust/tests/structure_designer/`):
- Disconnected pin → stored `replacements` drive output, identical to pre-change snapshot.
- Connected pin with a wired `Array[Record(ElementMapping)]` value → those rules drive the output; stored `replacements` not consulted.
- Connected pin with `to == 0` rule → matching atoms deleted (and bonds cleaned up), via existing `delete_atom` path.
- Connected pin with empty array → input passes through unchanged regardless of stored `replacements`.
- Out-of-range target (`to: 999`) → `NetworkResult::Error`.
- Missing `from` field at runtime (defensive case) → `NetworkResult::Error`.
- Custom-network passthrough preserved: a Crystal in, a Crystal out, with a wired `rules` pin.

Manual verification:
- Drop `atom_replace`, leave `rules` unconnected, edit stored rules — works as today.
- Wire any `Array[Record(ElementMapping)]` source into `rules`. Editor grays out; subtitle drops the rule summary; viewport reflects pin-driven rules.
- Disconnect — stored rules become live again; editor re-enables.

## Open Questions

1. **Pre-existing user defs named `ElementMapping`.** A project saved before this feature could in principle contain a user record def named `ElementMapping` (the name was free). On load, the `built_in_record_type_defs` entry already exists, and the `add_record_type_def` call for the user def would now collide. Options: (a) refuse to load with a clear error suggesting the user rename their def in a backup save before opening; (b) silently rename the user def to `ElementMapping_user` on load and emit a warning; (c) detect and drop the user def on load if its schema is identical to the built-in (most common case in practice — they declared the same thing). I'd lean (c)+(a) — drop on schema match, refuse otherwise — but worth confirming.

2. **Pin replaces vs. merges with stored.** This doc proposes "pin entirely replaces stored when connected." The merge alternative — "stored rules apply to elements not mentioned by the pin; pin rules win for matching `from` keys" — gives users a way to combine a baseline with a programmatic override on a single node, but introduces a third semantic mode and complicates debugging. If users frequently want partial override, merge becomes the right default; otherwise `array_concat` covers it without node-level support.

3. **Out-of-range target handling.** Strict error vs. silent skip vs. silent saturate-to-i16. This doc proposes strict error on the grounds that an out-of-range atomic number is almost always a bug. If the team wants to be more permissive (e.g. for experimental atomic numbers ≥ 1000 used by debug-color elements like `DEBUG_CARBON_GRAY = 1000`), the validation range needs to widen accordingly.

4. **Field naming.** This doc uses `from` / `to`. Alternatives considered: `source` / `target`, `match` / `replace`. `from`/`to` is the shortest, has obvious semantics in the dotted-access form (`r.from`, `r.to`) and matches the existing internal data structure. But `from` is a Python/SQL-ish keyword in some users' muscle memory — flag if there's a project preference. Whatever is chosen here is baked into the built-in `ElementMapping` def and is therefore harder to change later than for a user-declared def.

5. **Pin name.** `rules` (chosen) vs. `replacements` (matches the stored-property name) vs. `mappings`. `rules` is short and reads naturally on the node; `replacements` would line up with the property but invites confusion when both editor and pin are visible.
