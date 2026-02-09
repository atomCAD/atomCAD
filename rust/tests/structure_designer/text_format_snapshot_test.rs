//! Snapshot tests for network serialization to text format.
//!
//! These tests verify that the text format output for various node networks
//! is stable and matches expected output. The text format is consumed by AI
//! assistants, so stability is important.

use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
use rust_lib_flutter_cad::structure_designer::text_format::serialize_network;

/// Load a .cnnd file and serialize it to text format.
fn serialize_cnnd_file(file_path: &str) -> String {
    let mut registry = NodeTypeRegistry::new();
    let first_network_name =
        load_node_networks_from_file(&mut registry, file_path).expect("Failed to load CNND file");

    let network = registry
        .node_networks
        .get(&first_network_name)
        .expect("Network not found");

    serialize_network(network, &registry, Some(&first_network_name))
}

#[test]
fn test_diamond_network_serialization() {
    let text = serialize_cnnd_file("../samples/diamond.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_hexagem_network_serialization() {
    let text = serialize_cnnd_file("../samples/hexagem.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_extrude_demo_network_serialization() {
    let text = serialize_cnnd_file("../samples/extrude-demo.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_mof5_motif_network_serialization() {
    let text = serialize_cnnd_file("../samples/MOF5-motif.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_rutile_motif_network_serialization() {
    let text = serialize_cnnd_file("../samples/rutile-motif.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_halfspace_demo_network_serialization() {
    let text = serialize_cnnd_file("../samples/half-space-and-miller-index-demo.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_rotation_demo_network_serialization() {
    let text = serialize_cnnd_file("../samples/rotation-demo.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_pattern_network_serialization() {
    let text = serialize_cnnd_file("../samples/pattern.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_nut_bolt_network_serialization() {
    let text = serialize_cnnd_file("../samples/nut-bolt.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_truss_network_serialization() {
    let text = serialize_cnnd_file("../samples/truss-011.cnnd");
    insta::assert_snapshot!(text);
}

#[test]
fn test_flexure_delta_robot_network_serialization() {
    let text = serialize_cnnd_file("../samples/flexure-delta-robot.cnnd");
    insta::assert_snapshot!(text);
}
