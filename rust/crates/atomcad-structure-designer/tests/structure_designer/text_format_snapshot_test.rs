//! Snapshot tests for network serialization to text format.
//!
//! These tests verify that the text format output for various node networks
//! is stable and matches expected output. The text format is consumed by AI
//! assistants, so stability is important.

use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
use atomcad_structure_designer::text_format::serialize_network;
use atomcad_test_support::sample_path_str;

/// Load a .cnnd file and serialize it to text format.
fn serialize_cnnd_file(file_path: &str) -> String {
    let mut registry = NodeTypeRegistry::new();
    let load_result =
        load_node_networks_from_file(&mut registry, file_path).expect("Failed to load CNND file");

    let network = registry
        .node_networks
        .get(&load_result.first_network_name)
        .expect("Network not found");

    serialize_network(network, &registry, Some(&load_result.first_network_name))
}

#[test]
fn test_diamond_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("diamond.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_hexagem_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("hexagem.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_extrude_demo_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("extrude-demo.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_mof5_motif_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("MOF5-motif.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_rutile_motif_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("rutile-motif.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_halfspace_demo_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("half-space-and-miller-index-demo.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_rotation_demo_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("rotation-demo.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_pattern_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("pattern.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_nut_bolt_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("nut-bolt.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_truss_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("truss-011.cnnd"));
    insta::assert_snapshot!(text);
}

#[test]
fn test_flexure_delta_robot_network_serialization() {
    let text = serialize_cnnd_file(&sample_path_str("flexure-delta-robot.cnnd"));
    insta::assert_snapshot!(text);
}
