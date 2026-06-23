//! Phase 0 — read-only **invariant checker** for node networks / documents.
//!
//! See `doc/design_identity_vs_naming_phase0.md` (and its parent
//! `doc/design_identity_vs_naming.md`) for the *why*. This module ships **no**
//! data-representation change: it only *reads* a network/document and reports
//! every internal-consistency violation, so the silent rename / reorder /
//! wire-loss corruption class becomes a loud, located failure.
//!
//! ## The guiding rule (severity model)
//!
//! The checker reports bugs in *our* mutation/repair code — **not** user
//! mistakes. Some "bad-looking" states are legitimately user-reachable and are
//! already surfaced as `ValidationError`s (a type-mismatched wire; a dangling
//! record name in a hand-edited `.cnnd`). So the invariant is **not** "every
//! reference resolves"; it is:
//!
//! > Every unresolved / incoherent reference is *accounted for* by a
//! > corresponding `ValidationError` on the same node. None is silent.
//!
//! A violation that is accounted for is **not fatal**; a *silent* one is. Tier 1
//! (structural bookkeeping) is always fatal; Tier 2/3 (reference / type
//! coherence) are fatal only when not accounted for. This is what makes the
//! checker safe to wire into a `debug_assert!` without false-firing in honest
//! tests.

use std::collections::HashMap;

use crate::structure_designer::data_type::walk_data_type_record_names;
use crate::structure_designer::node_network::{NodeNetwork, SourcePin};
use crate::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordRefSite, collect_record_refs_in_node,
};
use crate::structure_designer::nodes::parameter::ParameterData;

/// One kind of internal-consistency violation. Grouped into three severity
/// tiers (see module docs). The discriminant order is irrelevant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantKind {
    // ----- Tier 1 — structural bookkeeping (always fatal) -----
    /// `arguments.len() != resolved params.len()`.
    ArgCountMismatch,
    /// `zone_output_arguments.len() != zone_output_pins.len()`.
    ZoneArgCountMismatch,
    /// A derived-layout node has `custom_node_type == None` (was the standalone
    /// `debug_assert_custom_node_type_cache_invariant`).
    CacheNone,
    /// Two parameter nodes in one network share a `param_id`.
    DuplicateParamId,
    /// `next_param_id <= max(param_id)` in this network.
    ParamIdFloor,
    /// `next_node_id <= max(node id)` in this network / body.
    NextNodeIdFloor,

    // ----- Tier 2 — reference resolution (fatal only if NOT accounted-for) -----
    /// `node_type_name` resolves to neither a built-in nor a network.
    UnresolvedNodeType,
    /// An embedded `RecordType::Named(n)` has no def.
    UnresolvedRecordName,
    /// A `record_construct` / `record_destructure` schema / `product` target
    /// names a record def that does not exist.
    UnresolvedSchema,
    /// A depth-0 wire's `source_node_id` is absent in this network.
    MissingWireSource,
    /// A wire's source pin index is outside the resolved source's pins.
    PinIndexOutOfRange,

    // ----- Tier 3 — type coherence (fatal only if NOT accounted-for) -----
    /// A retained wire's source type cannot be converted to the dest pin type.
    IncompatibleWireType,
}

impl InvariantKind {
    /// Tier 1 is always fatal; Tier 2/3 are fatal only when not accounted for.
    pub fn is_tier1(&self) -> bool {
        matches!(
            self,
            InvariantKind::ArgCountMismatch
                | InvariantKind::ZoneArgCountMismatch
                | InvariantKind::CacheNone
                | InvariantKind::DuplicateParamId
                | InvariantKind::ParamIdFloor
                | InvariantKind::NextNodeIdFloor
        )
    }

    /// The id-counter **floor** invariants (`ParamIdFloor`, `NextNodeIdFloor`).
    ///
    /// These are genuine Tier-1 invariants, but they only constitute *silent
    /// corruption* across the **persistence / duplicate axis** — the path that
    /// produced the `next_param_id`-reset bug
    /// (`doc/design_parameter_wire_stability.md`). On the *hot* validation path
    /// they false-fire on perfectly honest transients: an in-memory network
    /// assembled node-by-node (or a hand-built test fixture) legitimately has
    /// its counter lagging the node set until the next allocation. `validate_network`
    /// runs on every such edit, so the hot-path debug assert excludes these
    /// (see [`debug_assert_network_invariants`]); they remain fatal in
    /// [`check_document_invariants`], which is what the property/fuzz suite and
    /// the lint tool drive across save→load. This is the "refine the placement,
    /// don't weaken the invariant" resolution from the design doc §7/§8.
    pub fn is_id_counter_floor(&self) -> bool {
        matches!(
            self,
            InvariantKind::ParamIdFloor | InvariantKind::NextNodeIdFloor
        )
    }
}

/// A single reported violation, located in the zone tree.
#[derive(Debug, Clone)]
pub struct InvariantViolation {
    /// Chain of HOF node ids down to the body holding the offending node.
    /// Empty = top-level network.
    pub scope_path: Vec<u64>,
    /// The offending node, if the violation is attributable to one.
    pub node_id: Option<u64>,
    pub kind: InvariantKind,
    /// Human-readable detail. For `CacheNone` this MUST contain the legacy
    /// substring `custom_node_type cache invariant violated` (see §7 of the
    /// design doc — the existing `#[should_panic]` regression test keys on it).
    pub detail: String,
    /// True iff a `ValidationError` sits on the same node (Tier 2/3 only;
    /// Tier 1 is always `false`). Loose by design — any surfaced error on the
    /// node means the user is already being told it is broken.
    pub accounted_for: bool,
}

impl InvariantViolation {
    pub fn is_fatal(&self) -> bool {
        self.kind.is_tier1() || !self.accounted_for
    }
}

/// Per-network checker (the hot path). Walks `network` and every nested zone
/// body, reporting every violation. Read-only and non-panicking.
pub fn check_network_invariants(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
) -> Vec<InvariantViolation> {
    let mut out = Vec::new();
    check_one_scope(network, registry, &[], &mut out);
    out
}

/// Document checker (for tests + the lint tool). Runs `check_network_invariants`
/// over every network in `registry`, plus document-level checks that aren't
/// per-network (record-def → record-def references resolve). Pure and
/// non-panicking, so it is also the lint entry point.
pub fn check_document_invariants(registry: &NodeTypeRegistry) -> Vec<InvariantViolation> {
    let mut out = Vec::new();
    for network in registry.node_networks.values() {
        check_one_scope(network, registry, &[], &mut out);
    }
    // Record-def field types must reference only defs that exist (record→record
    // references). `node_id: None` — these aren't attributable to a node.
    for def in registry.record_type_defs.values() {
        for (_field_name, ty) in &def.fields {
            walk_data_type_record_names(ty, &mut |name| {
                if registry.lookup_record_type_def(name).is_none() {
                    out.push(InvariantViolation {
                        scope_path: Vec::new(),
                        node_id: None,
                        kind: InvariantKind::UnresolvedRecordName,
                        detail: format!(
                            "record type def '{}' references unknown record def '{}'",
                            def.name, name
                        ),
                        accounted_for: false,
                    });
                }
            });
        }
    }
    out
}

/// Debug-only assertion wrapper. Panics if any **fatal** violation is found.
/// Wired into the end of `validate_network`, where initialization is guaranteed
/// complete — never on the post-deserialize / pre-init transient.
#[cfg(debug_assertions)]
pub fn debug_assert_network_invariants(network: &NodeNetwork, registry: &NodeTypeRegistry) {
    let violations = check_network_invariants(network, registry);
    // The id-counter floors are deliberately NOT fatal here — see
    // `InvariantKind::is_id_counter_floor`. They stay fatal in
    // `check_document_invariants` (the persistence-axis guard).
    let fatal: Vec<&InvariantViolation> = violations
        .iter()
        .filter(|v| v.is_fatal() && !v.kind.is_id_counter_floor())
        .collect();
    debug_assert!(
        fatal.is_empty(),
        "network invariant(s) violated: {:#?}\nSee doc/design_identity_vs_naming_phase0.md",
        fatal,
    );
}

/// Recursive per-scope worker. Mirrors `validate_zones_recursive`'s shape so
/// wire-source checks resolve against the correct network. `scope_path` is the
/// chain of HOF node ids from the root down to (but not including) `network`.
fn check_one_scope(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    scope_path: &[u64],
    out: &mut Vec<InvariantViolation>,
) {
    let push = |out: &mut Vec<InvariantViolation>,
                node_id: Option<u64>,
                kind: InvariantKind,
                detail: String,
                accounted_for: bool| {
        out.push(InvariantViolation {
            scope_path: scope_path.to_vec(),
            node_id,
            kind,
            detail,
            accounted_for,
        });
    };

    // --- B3 / B4: per-network param_id uniqueness + next_param_id floor. ---
    let mut seen_param_ids: HashMap<u64, u64> = HashMap::new(); // param_id -> first node id
    let mut max_param_id: Option<u64> = None;
    for node in network.nodes.values() {
        if node.node_type_name != "parameter" {
            continue;
        }
        let Some(pd) = node.data.as_any_ref().downcast_ref::<ParameterData>() else {
            continue;
        };
        let Some(pid) = pd.param_id else {
            continue; // None = backward-compat file without ids; not counted.
        };
        max_param_id = Some(max_param_id.map_or(pid, |m| m.max(pid)));
        if let Some(prev) = seen_param_ids.insert(pid, node.id) {
            push(
                out,
                Some(node.id),
                InvariantKind::DuplicateParamId,
                format!(
                    "param_id {} is shared by parameter nodes {} and {}",
                    pid, prev, node.id
                ),
                false,
            );
        }
    }
    if let Some(mx) = max_param_id
        && network.next_param_id <= mx
    {
        push(
            out,
            None,
            InvariantKind::ParamIdFloor,
            format!(
                "next_param_id ({}) <= max param_id ({}) — recycling will collide",
                network.next_param_id, mx
            ),
            false,
        );
    }

    // --- B4 (node ids): next_node_id floor. ---
    if let Some(&max_node_id) = network.nodes.keys().max()
        && network.next_node_id <= max_node_id
    {
        push(
            out,
            None,
            InvariantKind::NextNodeIdFloor,
            format!(
                "next_node_id ({}) <= max node id ({}) — recycling will collide",
                network.next_node_id, max_node_id
            ),
            false,
        );
    }

    // --- Per-node checks. ---
    for node in network.nodes.values() {
        let node_id = node.id;
        // Loose accounting: any validation error on this node means the user is
        // already being told it is broken, so a co-located reference violation
        // is not silent.
        let accounted = network
            .validation_errors
            .iter()
            .any(|e| e.node_id == Some(node_id));

        // R1: node type resolves.
        let Some(node_type) = registry.get_node_type_for_node(node) else {
            push(
                out,
                Some(node_id),
                InvariantKind::UnresolvedNodeType,
                format!(
                    "node type '{}' resolves to neither built-in nor network",
                    node.node_type_name
                ),
                accounted,
            );
            // Without a resolved type the count / pin checks are meaningless.
            continue;
        };

        // B1: argument count matches resolved parameter count.
        if node.arguments.len() != node_type.parameters.len() {
            push(
                out,
                Some(node_id),
                InvariantKind::ArgCountMismatch,
                format!(
                    "node {} ('{}') has {} arguments but its type has {} params",
                    node_id,
                    node.node_type_name,
                    node.arguments.len(),
                    node_type.parameters.len()
                ),
                false,
            );
        }

        // B2 (zone arg count): only meaningful for zone-owning nodes.
        if node_type.has_zone()
            && node.zone_output_arguments.len() != node_type.zone_output_pins.len()
        {
            push(
                out,
                Some(node_id),
                InvariantKind::ZoneArgCountMismatch,
                format!(
                    "node {} ('{}') has {} zone_output_arguments but its type has {} zone-output pins",
                    node_id,
                    node.node_type_name,
                    node.zone_output_arguments.len(),
                    node_type.zone_output_pins.len()
                ),
                false,
            );
        }

        // B2 (cache): a derived-layout node must hold a populated cache. This
        // folds in the old standalone cache-invariant assert. Only built-in
        // node types have a base type and can be derived-layout; custom-network
        // instances carry `NoData` and are looked up elsewhere.
        if let Some(base) = registry.built_in_node_types.get(&node.node_type_name)
            && node.data.calculate_custom_node_type(base).is_some()
            && node.custom_node_type.is_none()
        {
            push(
                out,
                Some(node_id),
                InvariantKind::CacheNone,
                // MUST contain the legacy substring (design §7).
                format!(
                    "custom_node_type cache invariant violated: node {} ('{}') has a \
                     derived custom node type but its cache is None",
                    node_id, node.node_type_name
                ),
                false,
            );
        }

        // R2 / R3: embedded record names + schema/target names resolve.
        collect_record_refs_in_node(node, &mut |name, site| match site {
            RecordRefSite::EmbeddedType => {
                if registry.lookup_record_type_def(name).is_none() {
                    push(
                        out,
                        Some(node_id),
                        InvariantKind::UnresolvedRecordName,
                        format!(
                            "node {} ('{}') embeds unknown record def '{}'",
                            node_id, node.node_type_name, name
                        ),
                        accounted,
                    );
                }
            }
            RecordRefSite::Schema => {
                // Empty schema = user hasn't picked one yet; not a reference.
                if !name.is_empty() && registry.lookup_record_type_def(name).is_none() {
                    push(
                        out,
                        Some(node_id),
                        InvariantKind::UnresolvedSchema,
                        format!(
                            "node {} ('{}') names unknown record def '{}'",
                            node_id, node.node_type_name, name
                        ),
                        accounted,
                    );
                }
            }
        });

        // R4 / R5: wire source + pin index. Only depth-0 regular wires in
        // `arguments` are checked here. Captures (depth >= 1) and zone-input
        // references are enforced by `validate_zones_recursive` (rules 2/3) —
        // duplicating them here would double-report. `zone_output_arguments`
        // reference body-internal nodes (not nodes in `network`), so they are
        // deliberately not checked here.
        //
        // T1 (`IncompatibleWireType`) is intentionally **not emitted live** in
        // Phase 0 (the design marks it optional). `validate_wires`
        // short-circuits on the first type error, so a second incompatible wire
        // would be un-accounted-for and would fire spuriously; and the repair
        // pass normally disconnects incompatible wires before validation
        // anyway. The variant is retained (and unit-tested for its tier
        // semantics) for later phases to turn on once wire validation
        // accumulates rather than bails.
        for (arg_index, arg) in node.arguments.iter().enumerate() {
            for wire in &arg.incoming_wires {
                if wire.source_scope_depth != 0 {
                    continue;
                }
                let SourcePin::NodeOutput { pin_index } = wire.source_pin else {
                    continue; // depth-0 ZoneInput is not produced; skip defensively.
                };
                let Some(source) = network.nodes.get(&wire.source_node_id) else {
                    push(
                        out,
                        Some(node_id),
                        InvariantKind::MissingWireSource,
                        format!(
                            "node {} ('{}') arg {} references missing source node {}",
                            node_id, node.node_type_name, arg_index, wire.source_node_id
                        ),
                        accounted,
                    );
                    continue;
                };

                // R5: source pin index in range. -1 is the function pin (always
                // allowed); 0..output_pin_count are the regular outputs.
                if pin_index != -1
                    && let Some(source_type) = registry.get_node_type_for_node(source)
                {
                    let count = source_type.output_pin_count();
                    if pin_index < 0 || (pin_index as usize) >= count {
                        push(
                            out,
                            Some(node_id),
                            InvariantKind::PinIndexOutOfRange,
                            format!(
                                "node {} ('{}') arg {} references pin {} of source {} which has {} output pins",
                                node_id,
                                node.node_type_name,
                                arg_index,
                                pin_index,
                                wire.source_node_id,
                                count
                            ),
                            accounted,
                        );
                    }
                }
            }
        }
    }

    // --- Recurse into zone bodies. ---
    for node in network.nodes.values() {
        if let Some(body) = node.zone.as_ref() {
            let mut child_scope = scope_path.to_vec();
            child_scope.push(node.id);
            check_one_scope(body, registry, &child_scope, out);
        }
    }
}
