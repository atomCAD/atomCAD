//! The incremental pass against **hand-drawn** corpora
//! (`doc/design_incremental_layout.md` §Testing, Phase 5).
//!
//! Every other layout test arranges a small network by hand and states what
//! should happen to it. This one does the opposite: it takes drawings real
//! people made — irregular, with pre-existing overlaps and backward wires their
//! authors are content with — runs a generated edit through the real
//! [`StructureDesigner::ai_text_edit`] choke point, and hands the before/after
//! pair to the [oracle](super::layout_oracle). Nothing here knows what the
//! right answer is; it knows what must not have happened.
//!
//! Two corpora, exactly as the design specifies:
//!
//! - **`demolib/baselib_with_demos.cnnd`** is tracked in the repository and is
//!   the CI corpus: 65 networks, 901 nodes, a polished library.
//! - **`from_mechadense.cnnd`** is the maintainer's private working file (82
//!   networks, 2,300 nodes, 177 backward wires) and runs only when
//!   `ATOMCAD_LAYOUT_CORPUS` names it. Set it to any `.cnnd` path to point the
//!   same run at another file.
//!
//! Both load through the **real `.cnnd` loader**, not by parsing JSON, because
//! that is what makes the name match total: the loader assigns a `custom_name`
//! to every legacy node that lacks one, and demolib is a file whose nodes carry
//! none on disk. Diff the JSON instead and every node in the corpus is
//! anonymous and therefore `added`.
//!
//! # What a failure here means, and what to do with it
//!
//! The panic names the network, the edit and the invariant. Reproduce it as a
//! scenario test in `layout_incremental_test.rs` or `layout_repair_test.rs`
//! rather than debugging it here — this file is a net, not a workbench.

use glam::DVec2;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::NamePath;

use super::layout_oracle::{self, Drawing};
use super::layout_test_support::touched_between;

// ============================================================================
// Corpus files
// ============================================================================

/// `demolib/baselib_with_demos.cnnd`, resolved from this crate's manifest
/// directory so it does not depend on `cargo test`'s working directory.
fn demolib_path() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
        .join("demolib/baselib_with_demos.cnnd")
}

/// The private corpus, if the environment names one.
fn private_corpus_path() -> Option<PathBuf> {
    let raw = std::env::var("ATOMCAD_LAYOUT_CORPUS").ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

/// Load a `.cnnd` through the real loader into a fresh designer.
fn load(path: &Path) -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.load_node_networks(&path.to_string_lossy())
        .unwrap_or_else(|e| panic!("failed to load corpus `{}`: {e}", path.display()));
    sd
}

/// The corpus networks worth editing: at least two nodes, matching the
/// measurement the design's corpus table was taken over.
fn editable_networks(sd: &StructureDesigner) -> Vec<String> {
    let mut names: Vec<String> = sd
        .node_type_registry
        .node_networks
        .iter()
        .filter(|(_, network)| network.nodes.len() >= 2)
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names
}

// ============================================================================
// The generator
// ============================================================================

/// A seeded xorshift, so a failure is reproducible from the seed the panic
/// prints. Deliberately not a dependency: the sequence has to stay identical
/// across `rand` versions for a printed seed to mean anything a year from now.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Zero is the xorshift fixed point.
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A uniform-ish index into `0..n`. `n == 0` yields `0`.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }
}

/// The kinds of edit the generator produces, one pass over the corpus each.
///
/// Every kind is chosen to be **type-safe by construction** where it can be:
/// an unwired literal, a deletion, a comment and a body addition parse against
/// any network at all, so the pass cannot silently degrade into "every edit was
/// rejected, nothing was checked". The two that can be refused — the wired
/// addition and the rewire — are counted, and
/// [`at_least_one_edit_of_every_kind_lands`] fails if a kind stops landing
/// anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EditKind {
    /// A fresh node with no wires: the anchorless block of Step 4.
    AddUnwired,
    /// A fresh literal wired into a free input pin of a kept node: the
    /// ordinary "add a node beside its consumer" case, with a real anchor.
    AddWired,
    /// `delete <name>`: the hole of D4, which nothing may compact.
    DeleteNode,
    /// A `Comment` anchored to a kept node: Step 7.
    AddAnchoredComment,
    /// A fresh unwired node inside a random HOF body: the inside-out driver,
    /// and the HOF footprint growth it feeds to the parent scope (D12).
    AddInBody,
}

impl EditKind {
    const ALL: [EditKind; 5] = [
        EditKind::AddUnwired,
        EditKind::AddWired,
        EditKind::DeleteNode,
        EditKind::AddAnchoredComment,
        EditKind::AddInBody,
    ];
}

/// The names of a scope's nodes, sorted — the pool every generator draws from.
fn node_names(network: &NodeNetwork) -> Vec<String> {
    let mut names: Vec<String> = network
        .nodes
        .values()
        .filter_map(|node| node.custom_name.clone())
        .collect();
    names.sort();
    names
}

/// `(hof name, body node count)` for every zone-owning node in `network`.
fn bodies(network: &NodeNetwork) -> Vec<String> {
    let mut out: Vec<String> = network
        .nodes
        .values()
        .filter(|node| node.zone.is_some())
        .filter_map(|node| node.custom_name.clone())
        .collect();
    out.sort();
    out
}

/// A free `Float` or `Int` input pin somewhere in `network`, as
/// `(node name, node type name, pin name, literal type)`.
///
/// "Free" means the pin currently has no incoming wire, so assigning one adds a
/// wire rather than replacing one — a `grown`-free, purely additive edit.
fn free_scalar_pins(
    network: &NodeNetwork,
    sd: &StructureDesigner,
) -> Vec<(String, String, String, &'static str)> {
    let mut out = Vec::new();
    let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &network.nodes[&id];
        let Some(name) = node.custom_name.clone() else {
            continue;
        };
        let Some(node_type) = sd.node_type_registry.get_node_type(&node.node_type_name) else {
            continue;
        };
        for (index, parameter) in node_type.parameters.iter().enumerate() {
            let literal = match parameter.data_type {
                DataType::Float => "float",
                DataType::Int => "int",
                _ => continue,
            };
            let wired = node
                .arguments
                .get(index)
                .is_some_and(|argument| !argument.incoming_wires.is_empty());
            if wired {
                continue;
            }
            out.push((
                name.clone(),
                node.node_type_name.clone(),
                parameter.name.clone(),
                literal,
            ));
        }
    }
    out
}

/// Build one edit script for `network`, or `None` when this network has nothing
/// of the shape the kind needs (no body, no free scalar pin, …).
fn script_for(
    kind: EditKind,
    network: &NodeNetwork,
    sd: &StructureDesigner,
    rng: &mut Rng,
) -> Option<String> {
    // A name no corpus node can collide with, so the edit is always a creation
    // and never an accidental in-place update.
    const PROBE: &str = "zz_layout_probe";
    let names = node_names(network);

    match kind {
        EditKind::AddUnwired => Some(format!("{PROBE} = int {{ value: 7 }}\n")),
        EditKind::AddWired => {
            let pins = free_scalar_pins(network, sd);
            let (node, node_type, pin, literal) = rng.pick(&pins)?.clone();
            Some(format!(
                "{PROBE} = {literal} {{ value: 1 }}\n{node} = {node_type} {{ {pin}: {PROBE} }}\n"
            ))
        }
        EditKind::DeleteNode => {
            let victim = rng.pick(&names)?;
            Some(format!("delete {victim}\n"))
        }
        EditKind::AddAnchoredComment => {
            let anchor = rng.pick(&names)?;
            Some(format!(
                "{PROBE} = Comment {{ text: \"probe\", width: 200, height: 100, on: {anchor} }}\n"
            ))
        }
        EditKind::AddInBody => {
            let hof = rng.pick(&bodies(network))?.clone();
            Some(format!("{hof}/{PROBE} = int {{ value: 7 }}\n"))
        }
    }
}

// ============================================================================
// The run
// ============================================================================

/// One edit's outcome, for the counters the assertions below read.
struct RunStats {
    /// Edits that parsed and landed, per kind.
    applied: HashMap<EditKind, usize>,
    /// Edits the editor refused (a type rule, a name the network does not have
    /// after an earlier deletion, …). Not a failure — the generator is blind.
    refused: usize,
    /// Every network the run touched.
    networks: usize,
}

impl RunStats {
    fn new() -> Self {
        Self {
            applied: EditKind::ALL.iter().map(|&k| (k, 0)).collect(),
            refused: 0,
            networks: 0,
        }
    }
}

/// Apply every [`EditKind`] to every editable network of `path` and assert the
/// oracle after each one.
///
/// Each edit starts from a **fresh load**, so one kind's deletion cannot make
/// the next kind's script unparseable and the failure message always names a
/// state that can be reproduced from the file plus one script.
fn run_corpus(path: &Path, seed: u64) -> RunStats {
    let mut stats = RunStats::new();
    let probe = load(path);
    let networks = editable_networks(&probe);
    stats.networks = networks.len();
    assert!(
        stats.networks >= 2,
        "corpus `{}` has {} editable networks; that is not a corpus",
        path.display(),
        stats.networks
    );
    drop(probe);

    for kind in EditKind::ALL {
        for (index, network_name) in networks.iter().enumerate() {
            let mut sd = load(path);
            sd.set_active_node_network_name(Some(network_name.clone()));

            let network = sd
                .node_type_registry
                .node_networks
                .get(network_name)
                .expect("just listed");
            let mut rng = Rng::new(seed ^ ((index as u64) << 8) ^ (kind as u64));
            let Some(script) = script_for(kind, network, &sd, &mut rng) else {
                continue;
            };
            let before = Drawing::of(network, &sd.node_type_registry);

            let outcome = sd.ai_text_edit(&script, false);
            // `result.success` folds in a *whole-network* validation verdict, so
            // a corpus network that was already broken would fail every edit
            // however clean. `applied` is the editor's own verdict, which is
            // what "did this edit reach the drawing" means.
            let applied = sd.ai_edit_log.last().expect("every edit is logged").applied;
            if !applied {
                stats.refused += 1;
                continue;
            }
            *stats.applied.get_mut(&kind).expect("seeded") += 1;

            let network = sd
                .node_type_registry
                .node_networks
                .get(network_name)
                .expect("the edit did not delete the network");
            let after = Drawing::of(network, &sd.node_type_registry);
            let moved: HashSet<NamePath> = sd
                .ai_edit_log
                .last()
                .expect("logged")
                .layout
                .moved
                .iter()
                .map(|m| m.path.clone())
                .collect();

            let context = || {
                format!(
                    "corpus `{}`, network `{network_name}`, {kind:?}, seed {seed}\nscript:\n{script}\nerrors: {:?}",
                    path.display(),
                    outcome.result.errors
                )
            };
            let touched = touched_between(&before, &after, moved);
            // `catch_unwind` only to attach the context; the panic is re-raised.
            let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                layout_oracle::check(&before, &after, &touched);
            }));
            if let Err(payload) = checked {
                let message = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<non-string panic>".to_string());
                panic!("{}\n\n{}", message, context());
            }
        }
    }

    stats
}

// ============================================================================
// The tests
// ============================================================================

/// The CI corpus. Every generated edit on every demolib network with at least
/// two nodes, checked against all four whole-pass invariants.
#[test]
fn demolib_survives_every_generated_edit() {
    let path = demolib_path();
    assert!(
        path.exists(),
        "the tracked corpus is missing: {}",
        path.display()
    );
    let stats = run_corpus(&path, 0x5eed_1a70_u64);
    assert!(stats.networks >= 20, "demolib should have many networks");
}

/// The generator is blind, so a kind that silently stopped producing anything
/// would make the run above pass vacuously. This is the guard.
#[test]
fn at_least_one_edit_of_every_kind_lands() {
    let path = demolib_path();
    if !path.exists() {
        return;
    }
    let stats = run_corpus(&path, 0x5eed_1a70_u64);
    for kind in EditKind::ALL {
        assert!(
            stats.applied[&kind] > 0,
            "no `{kind:?}` edit landed anywhere in the corpus; the generator has \
             stopped producing valid scripts for it"
        );
    }
}

/// The maintainer's working file, which is not in the repository. Set
/// `ATOMCAD_LAYOUT_CORPUS` to its path to include it.
#[test]
fn the_private_corpus_survives_every_generated_edit() {
    let Some(path) = private_corpus_path() else {
        return;
    };
    assert!(
        path.exists(),
        "ATOMCAD_LAYOUT_CORPUS names a file that does not exist: {}",
        path.display()
    );
    run_corpus(&path, 0x5eed_1a70_u64);
}

// ============================================================================
// Moved-list snapshots
// ============================================================================

/// One fixed edit per demolib network, snapshotting the **name-path keyed
/// moved list** — the AI edit log's own measurement, not test bookkeeping.
///
/// Positions are deliberately *not* snapshotted; the oracle covers those, and a
/// position snapshot would churn on every cosmetic constant. What this pins is
/// the answer to "which drawings did this algorithm change touch, and how far
/// did it push things" — so an algorithm change shows its blast radius and
/// `cargo insta review` is the review.
#[test]
fn demolib_moved_lists_are_stable() {
    let path = demolib_path();
    if !path.exists() {
        return;
    }

    let probe = load(&path);
    let networks = editable_networks(&probe);
    drop(probe);

    let mut report = String::new();
    for network_name in &networks {
        let mut sd = load(&path);
        sd.set_active_node_network_name(Some(network_name.clone()));
        // One *wired* addition per network — the case with a real anchor, and
        // so the one that can force a shift — with the unwired addition as the
        // fallback for a network that offers no free scalar pin. The seed is
        // fixed, so the script for a given network is the same every run and
        // the snapshot compares drawings rather than scripts.
        let network = sd
            .node_type_registry
            .node_networks
            .get(network_name)
            .expect("just listed");
        let mut rng = Rng::new(0x05a4_5407);
        let script = script_for(EditKind::AddWired, network, &sd, &mut rng)
            .unwrap_or_else(|| "zz_layout_probe = int { value: 7 }\n".to_string());
        let outcome = sd.ai_text_edit(&script, false);
        let record = sd.ai_edit_log.last().expect("logged");
        if !record.applied {
            report.push_str(&format!(
                "{network_name}: REFUSED ({:?})\n",
                outcome.result.errors
            ));
            continue;
        }
        let mut moved: Vec<String> = record
            .layout
            .moved
            .iter()
            .map(|m| m.path_string())
            .collect();
        moved.sort();
        report.push_str(&format!(
            "{network_name}: {} node(s), moved [{}]\n",
            record.layout.node_count,
            moved.join(", ")
        ));
    }

    insta::assert_snapshot!(report);
}

// ============================================================================
// One hand-checked case: the window rule on a real drawing
// ============================================================================

/// The [window rule](`doc/design_incremental_layout.md`#the-window-rule) on a
/// drawing nobody wrote for this test.
///
/// Half the nodes in both measured corpora sit within a few pixels of another
/// node's x, and `demo_zincblende-to-wurtzite-transition` has one such pair:
/// `ivec34` and `ivec39`, 19 px apart, plainly one column as far as the person
/// who drew it is concerned. `expr1`'s right edge falls **between** them, so a
/// shift line dropped at the site's default splits the pair — `ivec39` moves,
/// `ivec34` does not, and an alignment a human made is gone.
///
/// The rule exists for exactly that: the line snaps left, out of the column, so
/// both travel as one rigid unit. Asserted here as the oracle's invariant 4,
/// against the real file.
///
/// **A known limit, found by writing this test.** The snap searches at most one
/// node width left of the default for an x where no movable node's box straddles
/// the line — and in a *dense* region of a hand-drawn network there is no such
/// x, because 160 px-wide boxes tile the space. Every strictly-8 px column in
/// demolib (`lib_icorner_z_cotahedral{111}`'s four `half_space` nodes are the
/// clearest) therefore keeps the default and is cut. The rule helps where there
/// is whitespace within reach and gives up where there is not; this test pins
/// the half that works, and the other half is a design question, not a bug in
/// the code below it.
#[test]
fn a_loose_column_straddling_the_shift_line_moves_as_a_whole() {
    use atomcad_structure_designer::layout::{grow_rect, measure_scope, rendered_node_size};

    let path = demolib_path();
    if !path.exists() {
        return;
    }
    let mut sd = load(&path);
    let name = "demo_zincblende-to-wurtzite-transition".to_string();
    const COLUMN: [&str; 2] = ["ivec34", "ivec39"];
    const GROWN: &str = "expr1";
    const DW: f64 = 40.0;

    let mut network = sd
        .node_type_registry
        .node_networks
        .remove(&name)
        .expect("demolib holds this network");
    let sizes = measure_scope(&network, &sd.node_type_registry);
    let id_of = |network: &NodeNetwork, want: &str| -> u64 {
        network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some(want))
            .unwrap_or_else(|| panic!("demolib network `{name}` has no node `{want}`"))
            .id
    };
    let grown = id_of(&network, GROWN);
    let column: Vec<u64> = COLUMN.iter().map(|n| id_of(&network, n)).collect();

    // The premise, restated as assertions, so a future demolib edit that moves
    // these nodes fails here rather than quietly making the test vacuous.
    let old = rendered_node_size(&network.nodes[&grown], &sd.node_type_registry);
    let right_edge = network.nodes[&grown].position.x + old.x;
    let xs: Vec<f64> = column
        .iter()
        .map(|id| network.nodes[id].position.x)
        .collect();
    let (lo, hi) = xs
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    assert!(
        hi - lo <= 20.0,
        "these two are supposed to read as one column: {xs:?}"
    );
    assert!(
        lo < right_edge && right_edge < hi,
        "the grown node's right edge ({right_edge}) is supposed to fall between them: {xs:?}"
    );

    let before: HashMap<u64, f64> = column
        .iter()
        .map(|&id| (id, network.nodes[&id].position.x))
        .collect();

    grow_rect(
        &mut network,
        &sizes,
        grown,
        old,
        DVec2::new(old.x + DW, old.y),
    );

    for &id in &column {
        assert_eq!(
            network.nodes[&id].position.x,
            before[&id] + DW,
            "`{}` did not travel with its column",
            network.nodes[&id].custom_name.clone().unwrap_or_default()
        );
    }
    // Invariant 4, stated directly: a shift is rigid, so the offset inside the
    // column is exactly what it was.
    assert_eq!(
        network.nodes[&column[0]].position.x - network.nodes[&column[1]].position.x,
        before[&column[0]] - before[&column[1]],
        "the column was distorted, not translated"
    );

    sd.node_type_registry.node_networks.insert(name, network);
}
