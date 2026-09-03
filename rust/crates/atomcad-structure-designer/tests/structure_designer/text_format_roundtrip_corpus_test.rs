//! Every network of a corpus round-trips through `query` → `edit --replace`.
//!
//! The AI text format is the assistant's only view of a design, and a
//! `--replace` of a network's own `query` output must be a no-op: it must
//! parse, apply, and serialize back to the same text. Nothing synthetic
//! exercises the format's corners the way real files do — the maintainer's
//! working file was the first thing to hold a function-typed `parameter`, an
//! array-typed one, and a custom node with a dotted parameter name, and each
//! of those broke the round-trip in turn.
//!
//! `demolib/baselib_with_demos.cnnd` is tracked and runs in CI; the private
//! working file runs when `ATOMCAD_LAYOUT_CORPUS` names it, the same switch
//! `layout_corpus_test.rs` uses. Both load through the real `.cnnd` loader.

use std::path::{Path, PathBuf};

use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{serialize_network, snapshot_node_positions};

fn demolib_path() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
        .join("demolib/baselib_with_demos.cnnd")
}

fn private_corpus_path() -> Option<PathBuf> {
    let raw = std::env::var("ATOMCAD_LAYOUT_CORPUS").ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn load(path: &Path) -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.load_node_networks(&path.to_string_lossy())
        .unwrap_or_else(|e| panic!("failed to load corpus `{}`: {e}", path.display()));
    sd
}

fn text_of(sd: &StructureDesigner, name: &str) -> String {
    let network = sd.node_type_registry.node_networks.get(name).unwrap();
    serialize_network(network, &sd.node_type_registry, Some(name))
}

/// Replace every network with its own text; collect every way that fails.
fn run_round_trips(path: &Path) {
    let mut sd = load(path);
    let mut names: Vec<String> = sd
        .node_type_registry
        .node_networks
        .keys()
        .cloned()
        .collect();
    names.sort();

    let mut failures = Vec::new();
    for name in &names {
        sd.set_active_node_network_name(Some(name.clone()));
        let before = text_of(&sd, name);
        let outcome = sd.ai_text_edit(&before, true);
        let applied = sd.ai_edit_log.last().is_some_and(|r| r.applied);
        if !applied {
            failures.push(format!(
                "{name}: --replace of its own text did not apply: {:?}
--- text
{before}",
                outcome.result.errors
            ));
            continue;
        }
        let after = text_of(&sd, name);
        if after != before {
            failures.push(format!(
                "{name}: text changed across --replace\n--- before\n{before}\n--- after\n{after}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} networks in `{}` do not round-trip:\n\n{}",
        failures.len(),
        names.len(),
        path.display(),
        failures.join("\n\n")
    );
}

#[test]
fn every_demolib_network_round_trips_through_replace() {
    run_round_trips(&demolib_path());
}

/// The maintainer's working file, which is not in the repository. Set
/// `ATOMCAD_LAYOUT_CORPUS` to its path to include it.
#[test]
fn every_private_corpus_network_round_trips_through_replace() {
    let Some(path) = private_corpus_path() else {
        return;
    };
    assert!(
        path.exists(),
        "ATOMCAD_LAYOUT_CORPUS names a file that does not exist: {}",
        path.display()
    );
    run_round_trips(&path);
}

// ============================================================================
// The corpus findings, reduced
// ============================================================================
//
// Each of these is the smallest script that reproduced one way the corpus run
// above failed. They stay here so the corpus test can go on being the net and
// these the pins.

fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy =
        atomcad_structure_designer::preferences::NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

fn applied(sd: &StructureDesigner) -> bool {
    sd.ai_edit_log.last().is_some_and(|r| r.applied)
}

/// Author `script`, then `--replace` with the resulting text and expect the
/// text unchanged; returns that text for further assertions.
fn replace_is_a_no_op(script: &str) -> String {
    let mut sd = designer();
    sd.ai_text_edit(script, false);
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    let first = text_of(&sd, "main");
    sd.ai_text_edit(&first, true);
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    assert_eq!(text_of(&sd, "main"), first);
    first
}

#[test]
fn a_structure_rot_axis_survives_a_replace() {
    // `axis_index` was emitted only when set, so the editor — which asks a
    // fresh node which properties take literals — read it as wire-only and
    // dropped `axis_index: 2` on the way back in.
    let text = replace_is_a_no_op("r = structure_rot { axis_index: 2, step: 1 }\n");
    assert!(text.contains("axis_index: 2"), "{text}");
    let text = replace_is_a_no_op("r = structure_rot { step: 1 }\n");
    assert!(
        text.contains("axis_index: -1"),
        "no axis is spelled -1: {text}"
    );
}

#[test]
fn a_nameless_motif_survives_a_replace() {
    // A fresh motif is *named* by default, so an omitted `name` read back as
    // "cubic zincblende".
    let text = replace_is_a_no_op("m = motif { definition: \"\", name: \"\" }\n");
    assert!(text.contains("name: \"\""), "{text}");
}

#[test]
fn an_apply_argument_wire_survives_a_replace() {
    // `apply`'s arg pins are derived from the `f` wire, which the same script
    // is only now making, so `arg0` found no pin the first time round.
    let text = replace_is_a_no_op(
        "sq = expr { expression: \"x * x\" }\n\
         v = int { value: 3 }\n\
         a = apply { f: @sq, arg0: v }\n",
    );
    assert!(text.contains("arg0: v"), "{text}");
}

#[test]
fn array_pin_wire_order_survives_a_replace() {
    // The serializer sorted an array pin's wires by source id, and a replace
    // mints ids in statement order — so `[b, a]` came back as `[a, b]`.
    let text = replace_is_a_no_op(
        "a = sphere { radius: 1 }\nb = sphere { radius: 2 }\nu = union { shapes: [b, a] }\n",
    );
    assert!(text.contains("shapes: [b, a]"), "{text}");
}

#[test]
fn duplicate_node_names_are_written_uniquely_and_match_back() {
    // The GUI does not keep `custom_name` unique (copy/paste carries it), and
    // the text layer keys everything on the name: four nodes called
    // `to_degrees` collapsed into one. The shared rule suffixes later holders.
    let mut sd = designer();
    sd.ai_text_edit("x = int { value: 1 }\ny = int { value: 2 }\n", false);
    assert!(applied(&sd));
    {
        let network = sd.node_type_registry.node_networks.get_mut("main").unwrap();
        let y = network
            .nodes
            .iter()
            .find(|(_, n)| n.custom_name.as_deref() == Some("y"))
            .map(|(id, _)| *id)
            .unwrap();
        network.nodes.get_mut(&y).unwrap().custom_name = Some("x".to_string());
    }
    let before = snapshot_node_positions(
        sd.node_type_registry.node_networks.get("main").unwrap(),
        &sd.node_type_registry,
    );

    let first = text_of(&sd, "main");
    assert!(first.contains("x = int { value: 1 }"), "{first}");
    assert!(first.contains("x_2 = int { value: 2 }"), "{first}");

    sd.ai_text_edit(&first, true);
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    assert_eq!(network.nodes.len(), 2, "both nodes survive");
    let after = snapshot_node_positions(network, &sd.node_type_registry);
    for (path, state) in &before {
        assert_eq!(
            after.get(path).map(|s| s.position),
            Some(state.position),
            "{path:?} kept its position across the rename"
        );
    }
    assert_eq!(text_of(&sd, "main"), first);
}
