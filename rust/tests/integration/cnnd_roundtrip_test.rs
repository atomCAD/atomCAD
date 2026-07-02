use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use tempfile::tempdir;

fn roundtrip_cnnd_file(file_path: &str) {
    let mut registry = NodeTypeRegistry::new();
    let load_result =
        load_node_networks_from_file(&mut registry, file_path).expect("Failed to load CNND file");
    let first_network_name = load_result.first_network_name;

    assert!(
        !first_network_name.is_empty(),
        "Expected at least one network"
    );

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let temp_file_path = temp_dir.path().join("roundtrip.cnnd");

    save_node_networks_to_file(
        &mut registry,
        &temp_file_path,
        load_result.direct_editing_mode,
        &load_result.cli_access_rules,
    )
    .expect("Failed to save CNND file");

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
            .unwrap_or_else(|| panic!("Network '{}' missing after roundtrip", name));

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
            network1.displayed_nodes.len(),
            network2.displayed_nodes.len(),
            "displayed_nodes count mismatch in network '{}'",
            name
        );

        for (node_id, node1) in &network1.nodes {
            let node2 = network2.nodes.get(node_id).unwrap_or_else(|| {
                panic!(
                    "Node {} missing after roundtrip in network '{}'",
                    node_id, name
                )
            });

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
                    arg1.len(),
                    arg2.len(),
                    "argument {} output_pins count mismatch for node {} in network '{}'",
                    i,
                    node_id,
                    name
                );
                for (pin_node_id, pin_index) in arg1.iter_source_pins() {
                    let pin_index2 = arg2.get_source_pin(pin_node_id).unwrap_or_else(|| {
                        panic!(
                            "argument {} pin {} missing for node {} in network '{}'",
                            i, pin_node_id, node_id, name
                        )
                    });
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
            network1.node_type.output_type(),
            network2.node_type.output_type(),
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

/// Phase 4 (unpack nodes): a network wiring all three stateless destructure
/// nodes (`structure_unpack`, `lattice_vecs_unpack`, `lattice_vecs_params`)
/// survives a `.cnnd` save → load round-trip with its nodes present and its
/// multi-output pin wires (including non-zero pins) intact. These nodes follow
/// the ordinary fixed-pin serialization path, so this is greenfield coverage,
/// not new plumbing — see `doc/design_structure_lattice_unpack_nodes.md`.
#[test]
fn unpack_nodes_cnnd_roundtrip() {
    use glam::f64::DVec2;
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

    // Build: structure → structure_unpack → { lattice_vecs_unpack, lattice_vecs_params }.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));

    let s = designer.add_node("structure", DVec2::new(0.0, 0.0));
    let su = designer.add_node("structure_unpack", DVec2::new(200.0, 0.0));
    let lvu = designer.add_node("lattice_vecs_unpack", DVec2::new(400.0, 0.0));
    let lvp = designer.add_node("lattice_vecs_params", DVec2::new(400.0, 200.0));
    // Rebuild a structure from the unpacked pieces — exercises pin 1 (motif) and
    // pin 2 (motif_offset) wires, not just pin 0.
    let s2 = designer.add_node("structure", DVec2::new(600.0, 0.0));
    designer.validate_active_network();

    // `structure` params: [structure(0), lattice_vecs(1), motif(2), motif_offset(3)].
    designer.connect_nodes(s, 0, su, 0);
    designer.connect_nodes(su, 0, lvu, 0);
    designer.connect_nodes(su, 0, lvp, 0);
    designer.connect_nodes(su, 0, s2, 1); // lattice_vecs (out pin 0 → in pin 1)
    designer.connect_nodes(su, 1, s2, 2); // motif (out pin 1 → in pin 2)
    designer.connect_nodes(su, 2, s2, 3); // motif_offset (out pin 2 → in pin 3)

    // Save → reload into a fresh registry.
    let temp_dir = tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("unpack_nodes.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("reload");

    let network = registry2.node_networks.get("Main").unwrap();

    // All five nodes survive with their types.
    let type_of = |id: u64| network.nodes.get(&id).unwrap().node_type_name.as_str();
    assert_eq!(type_of(s), "structure");
    assert_eq!(type_of(su), "structure_unpack");
    assert_eq!(type_of(lvu), "lattice_vecs_unpack");
    assert_eq!(type_of(lvp), "lattice_vecs_params");
    assert_eq!(type_of(s2), "structure");

    // Wires into the unpack node and out of its three pins all survive.
    assert_eq!(
        network.nodes.get(&su).unwrap().arguments[0].get_source_pin(s),
        Some(0),
        "structure_unpack.structure ← structure pin 0"
    );
    assert_eq!(
        network.nodes.get(&lvu).unwrap().arguments[0].get_source_pin(su),
        Some(0),
        "lattice_vecs_unpack ← structure_unpack pin 0"
    );
    assert_eq!(
        network.nodes.get(&lvp).unwrap().arguments[0].get_source_pin(su),
        Some(0),
        "lattice_vecs_params ← structure_unpack pin 0"
    );

    let s2_node = network.nodes.get(&s2).unwrap();
    assert_eq!(
        s2_node.arguments[1].get_source_pin(su),
        Some(0),
        "rebuilt structure.lattice_vecs ← structure_unpack pin 0"
    );
    assert_eq!(
        s2_node.arguments[2].get_source_pin(su),
        Some(1),
        "rebuilt structure.motif ← structure_unpack pin 1"
    );
    assert_eq!(
        s2_node.arguments[3].get_source_pin(su),
        Some(2),
        "rebuilt structure.motif_offset ← structure_unpack pin 2"
    );
}

/// A user record def at a *dotted* (namespaced) name, referenced by a
/// `record_construct` node, survives a save → load round-trip with its fields,
/// the `Named` reference, and the derived pin layout intact. Greenfield —
/// hierarchical record names did not previously round-trip through `.cnnd`.
/// See `doc/design_hierarchical_records.md` (Testing → Phase 1).
#[test]
fn dotted_record_def_cnnd_roundtrip() {
    use glam::f64::DVec2;
    use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
    use rust_lib_flutter_cad::structure_designer::node_type_registry::RecordTypeDef;
    use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

    // Build a designer with a dotted record def + a record_construct referencing it.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "Physics.ElementMapping".to_string(),
            vec![
                ("from".to_string(), DataType::Int),
                ("to".to_string(), DataType::Int),
            ],
        ))
        .unwrap();
    let cons = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    // Set the node's schema, then refresh its derived pin cache (re-fetch the
    // node inside the populate call to avoid an overlapping borrow).
    {
        let registry = &mut designer.node_type_registry;
        registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&cons)
            .unwrap()
            .data = Box::new(RecordConstructData {
            schema: "Physics.ElementMapping".to_string(),
            ..Default::default()
        });
        let node = registry
            .node_networks
            .get_mut("Main")
            .unwrap()
            .nodes
            .get_mut(&cons)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    // Save to a temp file, reload into a fresh registry.
    let temp_dir = tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("dotted_record.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("reload");

    // The dotted record def survives with its fields intact.
    let def = registry2
        .record_type_defs
        .get("Physics.ElementMapping")
        .expect("dotted record def missing after roundtrip");
    assert_eq!(
        def.fields
            .iter()
            .map(|f| (f.name.clone(), f.data_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("from".to_string(), DataType::Int),
            ("to".to_string(), DataType::Int),
        ]
    );

    // The record_construct node's `Named` reference (its `schema`) survives,
    // and its derived pin layout matches the authored field order.
    let network = registry2.node_networks.get("Main").unwrap();
    let node = network.nodes.get(&cons).unwrap();
    let schema = node
        .data
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap()
        .schema
        .clone();
    assert_eq!(schema, "Physics.ElementMapping");

    let nt = registry2.get_node_type_for_node(node).unwrap();
    let pin_names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(pin_names, vec!["from", "to"]);
    assert_eq!(
        nt.output_pins[0].fixed_type(),
        Some(&DataType::Record(RecordType::Named(
            "Physics.ElementMapping".to_string()
        )))
    );
}

/// Phase 3 (issue #381): a network using both `free_sphere` and `free_circle`,
/// with a `vec3` wired into `free_sphere.center` and a `float` into its
/// `radius`, survives a `.cnnd` save → load round-trip with its stored
/// real-space (Å) float data and its wires intact. These nodes serialize their
/// `center` / `radius` through the ordinary fixed-pin path (`dvec3_serializer` /
/// `dvec2_serializer` + `f64`), so this is greenfield coverage of that path.
#[test]
fn free_geometry_nodes_cnnd_roundtrip() {
    use glam::f64::{DVec2, DVec3};
    use rust_lib_flutter_cad::structure_designer::nodes::free_circle::FreeCircleData;
    use rust_lib_flutter_cad::structure_designer::nodes::free_sphere::FreeSphereData;
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));

    // free_sphere with stored (soon-overridden) center/radius, plus a wired
    // vec3 → center (pin 0) and float → radius (pin 1).
    let fs = designer.add_node("free_sphere", DVec2::new(0.0, 0.0));
    let fc = designer.add_node("free_circle", DVec2::new(0.0, 200.0));
    let vc = designer.add_node("vec3", DVec2::new(-200.0, 0.0));
    let fr = designer.add_node("float", DVec2::new(-200.0, 100.0));

    // Set fractional stored data (not representable in whole cells).
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("Main")
            .unwrap();
        network.set_node_network_data(
            fs,
            Box::new(FreeSphereData {
                center: DVec3::new(1.5, 2.25, -0.75),
                radius: 4.2,
            }),
        );
        network.set_node_network_data(
            fc,
            Box::new(FreeCircleData {
                center: DVec2::new(0.5, 1.25),
                radius: 3.0,
            }),
        );
    }
    designer.validate_active_network();
    designer.connect_nodes(vc, 0, fs, 0); // vec3 → free_sphere.center
    designer.connect_nodes(fr, 0, fs, 1); // float → free_sphere.radius

    // Save → reload into a fresh registry.
    let temp_dir = tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("free_geometry.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &std::collections::HashMap::new(),
    )
    .expect("save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("reload");

    let network = registry2.node_networks.get("Main").unwrap();

    // Node types survive.
    assert_eq!(
        network.nodes.get(&fs).unwrap().node_type_name,
        "free_sphere"
    );
    assert_eq!(
        network.nodes.get(&fc).unwrap().node_type_name,
        "free_circle"
    );

    // Stored float data survives byte-exactly.
    let fs_data = network
        .nodes
        .get(&fs)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<FreeSphereData>()
        .expect("fs should be FreeSphereData");
    assert_eq!(fs_data.center, DVec3::new(1.5, 2.25, -0.75));
    assert_eq!(fs_data.radius, 4.2);

    let fc_data = network
        .nodes
        .get(&fc)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<FreeCircleData>()
        .expect("fc should be FreeCircleData");
    assert_eq!(fc_data.center, DVec2::new(0.5, 1.25));
    assert_eq!(fc_data.radius, 3.0);

    // Wires into center (pin 0) and radius (pin 1) survive.
    let fs_node = network.nodes.get(&fs).unwrap();
    assert_eq!(
        fs_node.arguments[0].get_source_pin(vc),
        Some(0),
        "free_sphere.center ← vec3 pin 0"
    );
    assert_eq!(
        fs_node.arguments[1].get_source_pin(fr),
        Some(0),
        "free_sphere.radius ← float pin 0"
    );
}
