//! Opt-in per-node evaluation profiler — Phase 2 of
//! `doc/design_eval_profiling.md` (D1, D3, D4, D5).
//!
//! Where `refresh_profile` answers "which *phase* of the refresh was slow?",
//! this answers "which node, which node type, how many times, and how much of
//! that was its own work rather than its dependencies'".
//!
//! ## Why this is opt-in and the phase clock is not (D1)
//!
//! A phase boundary costs one `Instant::now()` per refresh; a node boundary
//! costs two clock reads and a hash-map update *per evaluation*. That is
//! microseconds at ~10³ evaluations per pass but not inside a `map` body over
//! 10⁵ elements, and a profiler that inflates the numbers it reports is worse
//! than useless. Switched off, the cost is one thread-local `bool` read
//! ([`ENABLED`]) per evaluation and **no guard at all**.
//!
//! ## Why the state is a thread-local and not a context field (D4/D6)
//!
//! Two independent reasons, either sufficient:
//!
//! - *Borrowck.* A guard holding `&mut NetworkEvaluationContext` for the
//!   duration of a frame would freeze the one thing the whole function body
//!   needs — `context` is threaded into every recursive `evaluate` call.
//! - *The eager-HOF context split.* `apply` / `fold` / `foreach` evaluate their
//!   bodies against a `fresh_inner_for_eager_body` context whose
//!   `drain_inner_context` merges **`print_buffer` and nothing else**. With a
//!   context-owned accumulator stack the body's `total` would never reach the
//!   HOF's child accumulator, so the HOF would be charged the entire body cost
//!   as *self* time — a wrong row, not a missing one — and the body's own
//!   records would vanish. A thread-local is per *pass*, which is the correct
//!   scope: it spans every context a pass constructs, and a new eager-body call
//!   site cannot drop what it never held.
//!
//! `StructureDesigner::with_eval_context` is the single owner of the lifetime:
//! it [`install`]s a profile at the start of a pass and [`take`]s it back at
//! the end, at the same seam as the existing `print_buffer` drain and with the
//! same "regardless of how the closure returned" discipline.
//!
//! ## Two attribution artifacts that are documented, not fixed (D4)
//!
//! - **Lazy iterators shift time to the consumer.** A `map` body node runs when
//!   `collect` pulls it, so its time nests under `collect` and `map`'s own
//!   total looks near-zero.
//! - **A custom-node instance has ~zero self time.** It delegates to its
//!   network's return node, so its *total* covers the subnetwork while its
//!   *self* is bookkeeping only. That is the useful reading, and the panel says
//!   so.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

use crate::evaluator::network_evaluator::{NetworkStackElement, NodeProfileKey};

/// Where a profiled node lives, captured **once on vacant insert** — never on
/// the hot update path, which would clone a `Vec` per evaluation (D5).
///
/// The aggregation key and this struct answer different questions and both are
/// needed: a key cannot be displayed (it is a hash) and cannot be navigated to.
/// Conflating them is what D5 rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLocation {
    /// The network the node actually **lives in** — what click-to-jump
    /// activates. Read off the deepest registry-owned frame, so a node inside a
    /// custom network names that network rather than the caller's; the jump
    /// crosses the boundary the same way the error-navigation jump lands on a
    /// root cause in another network.
    pub host_network: String,
    /// The node's HOF-body chain **within `host_network`**. Together with
    /// `host_network` and `node_id` this is exactly the address the Find Usages
    /// / error-navigation jump already consumes, so click-to-jump needs no new
    /// navigation machinery.
    pub scope_path: Vec<u64>,
    pub node_id: u64,
    /// Human-readable address: `"main/fold#12/add#3 (mysum)"`.
    pub label: String,
    /// The node's type name — the roll-up key of the "By node type" table.
    pub node_type_name: String,
    /// Whether `(host_network, scope_path, node_id)` is an address the canvas
    /// navigation can actually reach.
    ///
    /// True for everything with a home frame, custom-network internals
    /// included. It is false only for a body evaluated off a **body-only**
    /// stack (a lazy `map`/`filter` step) whose fallback scope path turns out
    /// to contain a custom-network instance hop — an id no `Node.zone` walk can
    /// follow. Such rows are still measured and still roll up by type; the
    /// panel just renders them non-clickable rather than offering a jump that
    /// lands nowhere.
    pub navigable: bool,
}

/// One row of the per-node table: an aggregate over every evaluation of one
/// `(home frame, node)` pair during one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeProfileRecord {
    pub location: NodeLocation,
    /// Times `NodeData::eval` actually ran for this node in this environment.
    ///
    /// Phase 3 adds `lookups` (requests) and `distinct_envs` alongside it, and
    /// with the memo the two diverge. Phase 2 deliberately does **not** show a
    /// `Lookups` column filled with this value: they are equal only until the
    /// memo lands, and a column that quietly changes meaning is how a
    /// regression hides (D8b).
    pub evaluations: u64,
    /// Time in this node's own `eval`, with time spent evaluating its upstream
    /// dependencies subtracted (D4).
    pub self_ns: u64,
    /// Wall time of the whole evaluation including everything it pulled.
    pub total_ns: u64,
}

/// The live accumulator **and** the finished report — one type, no separate
/// `EvalProfiler`. While a pass runs it lives in [`PROFILE`]; when the pass
/// ends `with_eval_context` takes it out and it becomes an immutable snapshot
/// hanging off the pass's `RefreshProfile` row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvalProfile {
    /// Records in first-seen order. A `Vec` rather than the map's values
    /// because the RAII guard carries a `u32` index into it — a plain integer
    /// with no borrow, which is what keeps the guard cheap enough to add a
    /// frame to a recursion that already runs near the debug-build stack limit
    /// (D4).
    records: Vec<NodeProfileRecord>,
    /// Aggregation key → index into `records` (D5).
    index: HashMap<NodeProfileKey, u32>,
    /// Child-accumulator stack: one entry per live guard, holding the summed
    /// `total_ns` of the evaluations that completed beneath it. Must be empty
    /// at end of pass — a leaked frame silently corrupts every ancestor's self
    /// time, which is why release is by `Drop` and not by hand at each of the
    /// two hook functions' many early exits.
    child_acc: Vec<u64>,
    /// The pass's root network — the fallback host for a record whose stack
    /// has no registry-owned frame to read one from (see `build_location`).
    root_network_name: String,
}

impl EvalProfile {
    /// Every recorded node, in first-seen order. The "By node" table sorts a
    /// copy of this by self time; the "By node type" table rolls it up.
    pub fn records(&self) -> &[NodeProfileRecord] {
        &self.records
    }

    /// Total `NodeData::eval` invocations the pass made.
    pub fn total_evaluations(&self) -> u64 {
        self.records.iter().map(|r| r.evaluations).sum()
    }

    /// Summed self time over every record. Equal to the summed *total* time of
    /// the outermost evaluations, which is the useful sanity check against the
    /// refresh's `eval_ms` phase.
    pub fn total_self_ns(&self) -> u64 {
        self.records.iter().map(|r| r.self_ns).sum()
    }

    /// Roll-up by node type (D5: "both tables come from one map"), returned in
    /// no particular order — the UI sorts.
    pub fn by_node_type(&self) -> Vec<NodeTypeProfileRecord> {
        let mut by_type: HashMap<&str, NodeTypeProfileRecord> = HashMap::new();
        for record in &self.records {
            let entry = by_type
                .entry(record.location.node_type_name.as_str())
                .or_insert_with(|| NodeTypeProfileRecord {
                    node_type_name: record.location.node_type_name.clone(),
                    nodes: 0,
                    evaluations: 0,
                    self_ns: 0,
                    total_ns: 0,
                });
            entry.nodes += 1;
            entry.evaluations += record.evaluations;
            entry.self_ns += record.self_ns;
            entry.total_ns += record.total_ns;
        }
        by_type.into_values().collect()
    }

    /// Depth of the child-accumulator stack. Zero everywhere outside an
    /// evaluation; the guard-release tests assert it is zero at end of pass.
    pub fn live_frame_count(&self) -> usize {
        self.child_acc.len()
    }
}

/// One row of the "By node type" table — a roll-up of every
/// [`NodeProfileRecord`] sharing a type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeTypeProfileRecord {
    pub node_type_name: String,
    /// How many distinct nodes of this type were evaluated.
    pub nodes: u64,
    pub evaluations: u64,
    pub self_ns: u64,
    pub total_ns: u64,
}

thread_local! {
    /// The off-path fast check. Separate from [`PROFILE`] so a pass with
    /// profiling off costs one `Cell` read per evaluation rather than a
    /// `RefCell` borrow (D1).
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    /// The live accumulator for the current pass, or `None` when profiling is
    /// off. Owned by `StructureDesigner::with_eval_context`.
    static PROFILE: RefCell<Option<EvalProfile>> = const { RefCell::new(None) };
}

/// Installs a fresh profile for the pass that is about to run, or clears any
/// previous one when `profile` is `None` (profiling off).
///
/// Called only from `StructureDesigner::with_eval_context`, which pairs it with
/// [`take`] on every exit path.
pub fn install(profile: Option<EvalProfile>) {
    ENABLED.set(profile.is_some());
    PROFILE.with_borrow_mut(|slot| *slot = profile);
}

/// Takes the finished profile back out at end of pass. Returns `None` when
/// profiling was off.
pub fn take() -> Option<EvalProfile> {
    ENABLED.set(false);
    let profile = PROFILE.with_borrow_mut(|slot| slot.take());
    debug_assert!(
        profile.as_ref().is_none_or(|p| p.child_acc.is_empty()),
        "eval profiler: {} guard frame(s) leaked — an early return in \
         `evaluate` / `evaluate_all_outputs` bypassed the RAII guard",
        profile.as_ref().map_or(0, |p| p.child_acc.len())
    );
    profile
}

/// Whether a profile is currently installed. The hot-path check.
#[inline]
pub fn is_enabled() -> bool {
    ENABLED.get()
}

/// Records the network this pass is rooted at, for the location of every
/// record the pass produces. Called at the top of each displayed-root
/// evaluation; a pass covers exactly one active network, so the repeated writes
/// all carry the same name. A no-op when profiling is off.
pub fn note_root_network(name: &str) {
    if !is_enabled() {
        return;
    }
    PROFILE.with_borrow_mut(|slot| {
        if let Some(profile) = slot.as_mut()
            && profile.root_network_name != name
        {
            profile.root_network_name = name.to_string();
        }
    });
}

/// The RAII bookkeeping for one node evaluation (D4).
///
/// **Release must be by `Drop`, not by hand.** Both hook functions have many
/// early exits — the poison check, the cycle guard, the central `Unit`-skip
/// rule, the invalid-network and missing-return-node bails — and a leaked frame
/// corrupts every ancestor's self time *silently*. That is the opposite trade
/// from the `eval_in_progress` bracket next to it, whose manual cleanup is
/// deliberate: a leak there produces a caught error, not silent corruption.
///
/// The struct is a plain `Instant` + `u32` — no borrows, no closure wrapper, no
/// `Box` — so it respects the STACK-SIZE WARNING on `evaluate_all_outputs`, and
/// it is **not constructed at all** when profiling is off.
pub struct NodeEvalGuard {
    start: Instant,
    record_index: u32,
}

impl Drop for NodeEvalGuard {
    fn drop(&mut self) {
        let total_ns = self.start.elapsed().as_nanos() as u64;
        let record_index = self.record_index;
        PROFILE.with_borrow_mut(|slot| {
            let Some(profile) = slot.as_mut() else {
                // The pass ended under us. Impossible in the evaluator (the
                // take happens after `f` returns, so every guard is already
                // dropped), but dropping the sample beats panicking in `Drop`.
                return;
            };
            let children_ns = profile.child_acc.pop().unwrap_or(0);
            let record = &mut profile.records[record_index as usize];
            record.evaluations += 1;
            // `saturating_sub` rather than an assert: on a coarse or
            // non-monotonic-across-cores clock a child can measure marginally
            // longer than its parent, and the invariant the tables rely on is
            // `self_ns <= total_ns`, which this preserves.
            record.self_ns += total_ns.saturating_sub(children_ns);
            record.total_ns += total_ns;
            // Charge the whole subtree to the parent, so *its* self time
            // excludes everything it pulled. This is the one line that makes
            // "time spent evaluating upstream dependencies is not charged to
            // the consumer" true.
            if let Some(parent) = profile.child_acc.last_mut() {
                *parent += total_ns;
            }
        });
    }
}

/// Opens a profiling frame for one evaluation of `node_id` against
/// `network_stack`, or returns `None` when profiling is off.
///
/// The returned guard must be bound to a local (`let _guard = …`) so it lives
/// for the whole evaluation; binding it to `_` would drop it immediately and
/// charge the node nothing.
pub fn begin(
    network_stack: &[NetworkStackElement],
    node_id: u64,
    scope_path: &[u64],
) -> Option<NodeEvalGuard> {
    if !is_enabled() {
        return None;
    }
    let key = crate::evaluator::network_evaluator::node_profile_key(network_stack, node_id);
    let record_index = PROFILE.with_borrow_mut(|slot| {
        let profile = slot.as_mut()?;
        let record_index = match profile.index.get(&key) {
            Some(index) => *index,
            None => {
                // Vacant insert is the **only** place a location is built: it
                // clones a `Vec` and formats a `String`, which must never
                // happen once per evaluation (D5).
                let location = build_location(
                    network_stack,
                    node_id,
                    scope_path,
                    &profile.root_network_name,
                );
                let index = profile.records.len() as u32;
                profile.records.push(NodeProfileRecord {
                    location,
                    evaluations: 0,
                    self_ns: 0,
                    total_ns: 0,
                });
                profile.index.insert(key, index);
                index
            }
        };
        profile.child_acc.push(0);
        Some(record_index)
    })?;
    Some(NodeEvalGuard {
        start: Instant::now(),
        record_index,
    })
}

/// Builds the display/navigation half of a record (D5). Cold path — vacant
/// insert only.
fn build_location(
    network_stack: &[NetworkStackElement],
    node_id: u64,
    scope_path: &[u64],
    root_network_name: &str,
) -> NodeLocation {
    let node = network_stack
        .last()
        .and_then(|frame| frame.node_network.nodes.get(&node_id));
    let node_type_name = node.map_or_else(|| "?".to_string(), |n| n.node_type_name.clone());

    // The deepest **registry-owned** frame: the network the node's home body
    // chain hangs off. `None` when the stack is body-only, which is how the
    // lazy walkers call `run_closure_once`.
    let home = network_stack.iter().rposition(|frame| !frame.is_zone_body);

    let mut label = match home {
        Some(index) => network_stack[index].node_network.node_type.name.clone(),
        None => root_network_name.to_string(),
    };
    if label.is_empty() {
        label.push('?');
    }
    // One segment per body frame above the home network, naming the node that
    // *owns* the body (`fold#12`) rather than the anonymous body itself. The
    // owner lives in the frame below, which is missing for a body-only stack —
    // there the segment degenerates to `#12`.
    let body_start = home.map_or(0, |index| index + 1);
    for index in body_start..network_stack.len() {
        let owner_id = network_stack[index].node_id;
        let owner_type = index
            .checked_sub(1)
            .and_then(|below| network_stack[below].node_network.nodes.get(&owner_id))
            .map(|owner| owner.node_type_name.as_str())
            .unwrap_or("");
        label.push_str(&format!("/{}#{}", owner_type, owner_id));
    }
    label.push_str(&format!("/{}#{}", node_type_name, node_id));
    if let Some(custom_name) = node.and_then(|n| n.custom_name.as_ref()) {
        label.push_str(&format!(" ({})", custom_name));
    }

    // The jump address is **home-relative**, derived from the same frame the
    // label and the aggregation key are: the network the node actually lives
    // in, plus the body chain above it. A node inside a custom network is
    // therefore navigable — the jump activates *that* network by name, exactly
    // as the error-navigation jump lands on a root cause across a network
    // boundary.
    //
    // Deriving it from the network stack rather than from `eval_scope_path` is
    // also what makes it survive a **parameter stack excursion**: a custom
    // network's `parameter` resolves its argument by popping the network-stack
    // frame while the instance's eval scope stays pushed
    // (`nodes/parameter.rs`), so a caller-side node evaluated through one would
    // otherwise record a scope path carrying an instance id it does not live
    // under.
    let (host_network, address_scope_path, navigable) = match home {
        Some(index) => {
            let host = network_stack[index].node_network.node_type.name.clone();
            let path = network_stack[index + 1..]
                .iter()
                .map(|frame| frame.node_id)
                .collect();
            let named = !host.is_empty();
            (host, path, named)
        }
        None => {
            // Body-only stack — how the lazy walkers call `run_closure_once`.
            // There is no home frame to read, so fall back to the pass's root
            // network plus the eval scope path, which *is* maintained on the
            // real scene context for these bodies. That is exact only when the
            // whole path is body hops: an instance hop in it would put an id
            // there that no `Node.zone` walk can follow, and the two lengths
            // agreeing is precisely the check for that.
            let exact = scope_path.len() == network_stack.len();
            (root_network_name.to_string(), scope_path.to_vec(), exact)
        }
    };

    NodeLocation {
        host_network,
        scope_path: address_scope_path,
        node_id,
        label,
        node_type_name,
        navigable,
    }
}
