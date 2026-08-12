// R1/R2 regression harness for the error-management redesign
// (`doc/design_error_management.md`, §"Regression strategy").
//
// R1 (`validation_corpus_snapshot`): loads EVERY fixture under
// `tests/fixtures/**/*.cnnd` (new fixtures join automatically), runs the full
// production load + validate pass, and snapshots, per network:
// `(name, valid, [(scope, node_id, blocking, error_text)])`, sorted
// deterministically. Fixtures that fail to *load* (the corpus includes
// deliberately corrupt/legacy files) record their load outcome as the
// snapshot entry instead of panicking the harness.
//
// R2 (`eval_outcome_corpus_snapshot`): for every fixture network with no
// blocking validation error anywhere in its scope tree, evaluates each
// displayed (node, pin) and snapshots the *outcome* — `Ok` or the error text.
// Note: clean validation does NOT imply no runtime errors (a missing required
// input is deliberately a runtime error), so the assertion across behavioral
// phases is that this snapshot is byte-identical, not that it is empty.
//
// The baseline was recorded BEFORE the first behavioral phase (Phase 2,
// validation accumulation) landed, so every later phase's effect on real
// designs shows up as a reviewable `cargo insta review` diff.
//
// Note on the pre-Phase-2 baseline: the short-circuiting `validate_wires`
// reported only the FIRST blocking error it hit, and "first" followed HashMap
// iteration order — on `rename_wire_loss/before.cnnd`'s
// `z_ISSUE.proxy(111)att1` (two independently-unresolved nodes, 8 and 11)
// the reported row flipped between runs. Phase 2's accumulation (with
// sorted-node-id iteration) is what made this snapshot deterministic.

use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::path::{Path, PathBuf};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// All `.cnnd` files under `tests/fixtures`, recursively, sorted by their
/// forward-slash relative path so the snapshot order is stable across
/// platforms and directory-iteration orders.
fn collect_fixture_paths() -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir).expect("fixtures dir readable");
        for entry in entries {
            let path = entry.expect("fixtures dir entry readable").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("cnnd") {
                out.push(path);
            }
        }
    }
    let root = fixtures_root();
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    let mut named: Vec<(String, PathBuf)> = paths
        .into_iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&root)
                .expect("fixture under root")
                .to_string_lossy()
                .replace('\\', "/");
            (rel, p)
        })
        .collect();
    named.sort();
    named
}

/// Recursively collect `(scope, node_id, blocking, error_text)` rows from a
/// network and every nested zone body. `scope` is the chain of zone-owning
/// node ids from the top level down to the body holding the error (empty for
/// the network's own list) — body errors land on the body network's
/// `validation_errors` (see `validate_zones_recursive`).
fn collect_error_rows(network: &NodeNetwork, scope: &str, rows: &mut Vec<String>) {
    for err in &network.validation_errors {
        let node = match err.node_id {
            Some(id) => format!("{}", id),
            None => "-".to_string(),
        };
        rows.push(format!(
            "    scope=[{}] node={} blocking={} \"{}\"",
            scope, node, err.blocking, err.error_text
        ));
    }
    let mut node_ids: Vec<u64> = network.nodes.keys().copied().collect();
    node_ids.sort_unstable();
    for node_id in node_ids {
        if let Some(body) = network.nodes.get(&node_id).and_then(|n| n.zone.as_deref()) {
            let child_scope = if scope.is_empty() {
                format!("{}", node_id)
            } else {
                format!("{}/{}", scope, node_id)
            };
            collect_error_rows(body, &child_scope, rows);
        }
    }
}

/// Whether the network (including nested zone bodies) carries any *blocking*
/// validation error. Deliberately computed from the error lists rather than
/// `NodeNetwork::valid`, so the R2 filter keeps meaning "no blocking errors"
/// even after Phase 3 redefines `valid` to the interface residue.
fn has_blocking_error(network: &NodeNetwork) -> bool {
    if network.validation_errors.iter().any(|e| e.blocking) {
        return true;
    }
    network
        .nodes
        .values()
        .filter_map(|n| n.zone.as_deref())
        .any(has_blocking_error)
}

/// Loads a fixture through the full production path (`load_node_networks`:
/// param-id dedupe, dependency-order validation, display policy) and returns
/// the designer, or the load error's text.
fn load_fixture(path: &Path) -> Result<StructureDesigner, String> {
    let mut designer = StructureDesigner::new();
    match designer.load_node_networks(&path.to_string_lossy()) {
        Ok(_) => Ok(designer),
        Err(e) => Err(format!("{}", e)),
    }
}

#[test]
fn validation_corpus_snapshot() {
    let mut out = String::new();
    for (rel, path) in collect_fixture_paths() {
        out.push_str(&format!("=== {}\n", rel));
        let designer = match load_fixture(&path) {
            Ok(d) => d,
            Err(e) => {
                out.push_str(&format!("  LOAD FAILED: {}\n", e));
                continue;
            }
        };
        let mut names: Vec<&String> = designer.node_type_registry.node_networks.keys().collect();
        names.sort();
        for name in names {
            let network = &designer.node_type_registry.node_networks[name];
            out.push_str(&format!("  network '{}' valid={}\n", name, network.valid));
            let mut rows = Vec::new();
            collect_error_rows(network, "", &mut rows);
            rows.sort();
            for row in rows {
                out.push_str(&row);
                out.push('\n');
            }
        }
    }
    insta::assert_snapshot!("validation_corpus", out);
}

#[test]
fn eval_outcome_corpus_snapshot() {
    let mut out = String::new();
    for (rel, path) in collect_fixture_paths() {
        let Ok(designer) = load_fixture(&path) else {
            // Load outcomes are covered by `validation_corpus_snapshot`.
            continue;
        };
        let registry = &designer.node_type_registry;
        let mut names: Vec<&String> = registry.node_networks.keys().collect();
        names.sort();
        let mut fixture_lines = Vec::new();
        for name in names {
            let network = &registry.node_networks[name];
            if has_blocking_error(network) {
                continue;
            }
            let mut displayed: Vec<u64> = network.displayed_nodes.keys().copied().collect();
            displayed.sort_unstable();
            for node_id in displayed {
                let mut pins: Vec<i32> = network.displayed_nodes[&node_id]
                    .displayed_pins
                    .iter()
                    .copied()
                    .filter(|&p| p >= 0)
                    .collect();
                pins.sort_unstable();
                for pin in pins {
                    let evaluator = NetworkEvaluator::new();
                    let mut context = NetworkEvaluationContext::new();
                    let stack = vec![NetworkStackElement {
                        is_zone_body: false,
                        node_network: network,
                        node_id: 0,
                    }];
                    let result =
                        evaluator.evaluate(&stack, node_id, pin, registry, false, &mut context);
                    let outcome = match result {
                        NetworkResult::Error(text) => format!("Error: {}", text),
                        _ => "Ok".to_string(),
                    };
                    fixture_lines.push(format!(
                        "  network '{}' node={} pin={} -> {}",
                        name, node_id, pin, outcome
                    ));
                }
            }
        }
        if !fixture_lines.is_empty() {
            out.push_str(&format!("=== {}\n", rel));
            for line in fixture_lines {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    insta::assert_snapshot!("eval_outcome_corpus", out);
}
