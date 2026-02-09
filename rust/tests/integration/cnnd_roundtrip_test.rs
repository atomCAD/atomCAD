use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use tempfile::tempdir;

fn roundtrip_cnnd_file(file_path: &str) {
    let mut registry = NodeTypeRegistry::new();
    let first_network_name =
        load_node_networks_from_file(&mut registry, file_path).expect("Failed to load CNND file");

    assert!(
        !first_network_name.is_empty(),
        "Expected at least one network"
    );

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_file_path = temp_dir.path().join("roundtrip.cnnd");

    save_node_networks_to_file(&mut registry, &temp_file_path).expect("Failed to save CNND file");

    let mut registry2 = NodeTypeRegistry::new();
    let _first_network_name2 =
        load_node_networks_from_file(&mut registry2, temp_file_path.to_str().unwrap())
            .expect("Failed to reload CNND file");

    assert_eq!(
        registry.node_networks.len(),
        registry2.node_networks.len(),
        "Network count mismatch"
    );

    assert!(
        registry2.node_networks.contains_key(&first_network_name),
        "Original first network '{}' missing after roundtrip",
        first_network_name
    );

    for (name, network1) in &registry.node_networks {
        let network2 = registry2
            .node_networks
            .get(name)
            .expect(&format!("Network '{}' missing after roundtrip", name));

        assert_eq!(
            network1.nodes.len(),
            network2.nodes.len(),
            "Node count mismatch in network '{}'",
            name
        );
        assert_eq!(
            network1.next_node_id, network2.next_node_id,
            "next_node_id mismatch in network '{}'",
            name
        );
        assert_eq!(
            network1.return_node_id, network2.return_node_id,
            "return_node_id mismatch in network '{}'",
            name
        );
        assert_eq!(
            network1.displayed_node_ids.len(),
            network2.displayed_node_ids.len(),
            "displayed_node_ids count mismatch in network '{}'",
            name
        );

        for (node_id, node1) in &network1.nodes {
            let node2 = network2.nodes.get(node_id).expect(&format!(
                "Node {} missing after roundtrip in network '{}'",
                node_id, name
            ));

            assert_eq!(
                node1.node_type_name, node2.node_type_name,
                "node_type_name mismatch for node {} in network '{}'",
                node_id, name
            );
            assert_eq!(
                node1.position, node2.position,
                "position mismatch for node {} in network '{}'",
                node_id, name
            );
            assert_eq!(
                node1.arguments.len(),
                node2.arguments.len(),
                "arguments count mismatch for node {} in network '{}'",
                node_id,
                name
            );

            for (i, arg1) in node1.arguments.iter().enumerate() {
                let arg2 = &node2.arguments[i];
                assert_eq!(
                    arg1.argument_output_pins.len(),
                    arg2.argument_output_pins.len(),
                    "argument {} output_pins count mismatch for node {} in network '{}'",
                    i,
                    node_id,
                    name
                );
                for (pin_node_id, pin_index) in &arg1.argument_output_pins {
                    let pin_index2 = arg2.argument_output_pins.get(pin_node_id).expect(&format!(
                        "argument {} pin {} missing for node {} in network '{}'",
                        i, pin_node_id, node_id, name
                    ));
                    assert_eq!(
                        pin_index, pin_index2,
                        "argument {} pin {} index mismatch for node {} in network '{}'",
                        i, pin_node_id, node_id, name
                    );
                }
            }
        }

        assert_eq!(
            network1.node_type.name, network2.node_type.name,
            "node_type.name mismatch in network '{}'",
            name
        );
        assert_eq!(
            network1.node_type.output_type, network2.node_type.output_type,
            "node_type.output_type mismatch in network '{}'",
            name
        );
        assert_eq!(
            network1.node_type.parameters.len(),
            network2.node_type.parameters.len(),
            "node_type.parameters count mismatch in network '{}'",
            name
        );
    }
}

#[test]
fn test_diamond_roundtrip() {
    roundtrip_cnnd_file("../samples/diamond.cnnd");
}

#[test]
fn test_hexagem_roundtrip() {
    roundtrip_cnnd_file("../samples/hexagem.cnnd");
}

#[test]
fn test_extrude_demo_roundtrip() {
    roundtrip_cnnd_file("../samples/extrude-demo.cnnd");
}

#[test]
fn test_mof5_motif_roundtrip() {
    roundtrip_cnnd_file("../samples/MOF5-motif.cnnd");
}

#[test]
fn test_rutile_motif_roundtrip() {
    roundtrip_cnnd_file("../samples/rutile-motif.cnnd");
}

#[test]
fn test_halfspace_demo_roundtrip() {
    roundtrip_cnnd_file("../samples/half-space-and-miller-index-demo.cnnd");
}

#[test]
fn test_rotation_demo_roundtrip() {
    roundtrip_cnnd_file("../samples/rotation-demo.cnnd");
}

#[test]
fn test_pattern_roundtrip() {
    roundtrip_cnnd_file("../samples/pattern.cnnd");
}

#[test]
fn test_nut_bolt_roundtrip() {
    roundtrip_cnnd_file("../samples/nut-bolt.cnnd");
}

#[test]
fn test_truss_roundtrip() {
    roundtrip_cnnd_file("../samples/truss-011.cnnd");
}

#[test]
fn test_flexure_delta_robot_roundtrip() {
    roundtrip_cnnd_file("../samples/flexure-delta-robot.cnnd");
}

#[test]
fn test_demolib_proxy_roundtrip() {
    roundtrip_cnnd_file("../samples/demolib+(111)-proxy-generator.cnnd");
}
