//! Renaming a parameter of a network from an old `.cnnd` must keep the wires
//! into that pin at its call sites (reported by mechadense, 2026-10-05).
//!
//! Files written before `param_id` existed carry no id on their parameter
//! nodes, and nothing assigned one, so call sites could match such a
//! parameter only by name: a rename in the properties panel silently dropped
//! its wire at every call site. The load now assigns the missing ids
//! (`network_validator::assign_missing_param_ids`).

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::network_validator::assign_missing_param_ids;
use atomcad_structure_designer::node_network::walk_all_nodes;
use atomcad_structure_designer::nodes::parameter::ParameterData;
use atomcad_structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn demolib_path() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
        .join("demolib/baselib_with_demos.cnnd")
}

fn temp_path(file: &str) -> PathBuf {
    std::env::temp_dir().join(file)
}

fn param_data<'a>(
    designer: &'a StructureDesigner,
    network: &str,
    node_id: u64,
) -> &'a ParameterData {
    designer.node_type_registry.node_networks[network].nodes[&node_id]
        .data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
}

/// What the properties panel does (`set_parameter_data`): rebuild the data
/// with the existing `param_id` and the new name, through
/// `set_node_network_data_scoped`.
fn ui_rename(designer: &mut StructureDesigner, network: &str, node_id: u64, new_name: &str) {
    designer.set_active_node_network_name(Some(network.to_string()));
    let new = ParameterData {
        param_name: new_name.to_string(),
        ..param_data(designer, network, node_id).clone()
    };
    designer.set_node_network_data_scoped(&[], node_id, Box::new(new));
}

/// Number of call sites of `network` whose pin `index` is wired, over every
/// network, bodies included.
fn wired_call_sites(designer: &StructureDesigner, network: &str, index: usize) -> usize {
    let mut count = 0;
    for net in designer.node_type_registry.node_networks.values() {
        walk_all_nodes(net, &mut |node| {
            if node.node_type_name == network
                && node
                    .arguments
                    .get(index)
                    .is_some_and(|a| !a.argument_output_pins().is_empty())
            {
                count += 1;
            }
        });
    }
    count
}

/// `param_id`s of every parameter node of `network`, by node id.
fn param_ids(designer: &StructureDesigner, network: &str) -> Vec<(u64, Option<u64>)> {
    let mut ids: Vec<(u64, Option<u64>)> = designer.node_type_registry.node_networks[network]
        .nodes
        .iter()
        .filter_map(|(&nid, n)| {
            let p = n.data.as_any_ref().downcast_ref::<ParameterData>()?;
            Some((nid, p.param_id))
        })
        .collect();
    ids.sort_unstable();
    ids
}

/// `Sub(a: Int, b: Int)` called from `main` with both pins wired, saved with
/// its parameters stripped of their ids — the shape of a pre-`param_id` file.
/// Returns the loaded designer and `(a, b)` parameter node ids.
fn load_legacy_project(file: &str) -> (StructureDesigner, u64, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Sub");
    designer.set_active_node_network_name(Some("Sub".to_string()));
    let a = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    let b = designer.add_node("parameter", DVec2::new(0.0, 80.0));
    for (node, name, order) in [(a, "a", 0), (b, "b", 1)] {
        let new = ParameterData {
            param_name: name.to_string(),
            data_type: DataType::Int,
            sort_order: order,
            ..param_data(&designer, "Sub", node).clone()
        };
        designer.set_node_network_data_scoped(&[], node, Box::new(new));
    }
    let ret = designer.add_node("int", DVec2::new(200.0, 0.0));
    designer.set_return_node_id(Some(ret));
    designer.validate_active_network();

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let s = designer.add_node("Sub", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, s, 0);
    designer.connect_nodes(i2, 0, s, 1);

    // Strip every id, as a file from before `param_id` has none.
    let sub = designer
        .node_type_registry
        .node_networks
        .get_mut("Sub")
        .unwrap();
    for node in sub.nodes.values_mut() {
        if let Some(p) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
            p.param_id = None;
        }
    }
    for param in sub.node_type.parameters.iter_mut() {
        param.id = None;
    }
    sub.next_param_id = 1;

    let path = temp_path(file);
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text.matches("\"param_id\": null").count(),
        text.matches("\"param_id\"").count(),
        "the fixture must really be id-less"
    );

    let mut loaded = StructureDesigner::new();
    loaded.load_node_networks(path.to_str().unwrap()).unwrap();
    (loaded, a, b)
}

#[test]
fn renaming_a_legacy_parameter_keeps_its_call_site_wire() {
    let (mut designer, a, _) = load_legacy_project("legacy_param_rename.cnnd");
    assert_eq!(wired_call_sites(&designer, "Sub", 0), 1);

    ui_rename(&mut designer, "Sub", a, "renamed");

    let sub = &designer.node_type_registry.node_networks["Sub"];
    assert_eq!(sub.node_type.parameters[0].name, "renamed");
    assert_eq!(
        wired_call_sites(&designer, "Sub", 0),
        1,
        "renaming a parameter loaded without an id dropped its call-site wire"
    );
    assert_eq!(
        wired_call_sites(&designer, "Sub", 1),
        1,
        "the other pin is untouched"
    );
}

#[test]
fn load_assigns_unique_ids_in_interface_order() {
    let (designer, a, b) = load_legacy_project("legacy_param_ids.cnnd");
    assert_eq!(
        param_ids(&designer, "Sub"),
        vec![(a, Some(1)), (b, Some(2))]
    );

    // The interface agrees with its nodes, and the counter is above both.
    let sub = &designer.node_type_registry.node_networks["Sub"];
    let interface: Vec<(String, Option<u64>)> = sub
        .node_type
        .parameters
        .iter()
        .map(|p| (p.name.clone(), p.id))
        .collect();
    assert_eq!(
        interface,
        vec![("a".to_string(), Some(1)), ("b".to_string(), Some(2))]
    );
    assert_eq!(sub.next_param_id, 3);
}

#[test]
fn assigned_ids_survive_save_and_reload() {
    let (mut designer, _, _) = load_legacy_project("legacy_param_ids_save.cnnd");
    let first = param_ids(&designer, "Sub");

    let path = temp_path("legacy_param_ids_resaved.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();
    let mut reloaded = StructureDesigner::new();
    reloaded.load_node_networks(path.to_str().unwrap()).unwrap();

    assert_eq!(param_ids(&reloaded, "Sub"), first);
}

#[test]
fn assign_missing_param_ids_keeps_existing_ids_and_is_idempotent() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Sub");
    designer.set_active_node_network_name(Some("Sub".to_string()));
    let a = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    let b = designer.add_node("parameter", DVec2::new(0.0, 80.0));
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut("Sub")
        .unwrap();
    let kept = net.nodes[&a]
        .data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .param_id;
    net.nodes
        .get_mut(&b)
        .unwrap()
        .data
        .as_any_mut()
        .downcast_mut::<ParameterData>()
        .unwrap()
        .param_id = None;
    let counter = net.next_param_id;

    assert_eq!(assign_missing_param_ids(net), 1);
    assert_eq!(
        assign_missing_param_ids(net),
        0,
        "a second run assigns nothing"
    );
    assert_eq!(
        param_ids(&designer, "Sub"),
        vec![(a, kept), (b, Some(counter))],
        "an existing id is kept; the missing one comes from the counter"
    );
}

/// The real file mechadense hit it in: every `unit_cell` parameter of the
/// demolib with a wired call site, renamed the way the properties panel
/// renames it.
#[test]
fn renaming_unit_cell_parameters_in_demolib_keeps_call_site_wires() {
    let mut designer = StructureDesigner::new();
    designer
        .load_node_networks(demolib_path().to_str().unwrap())
        .unwrap();

    let mut checked = 0;
    let mut names: Vec<String> = designer
        .node_type_registry
        .node_networks
        .keys()
        .cloned()
        .collect();
    names.sort();
    for network in names {
        let Some((index, node_id)) = designer.node_type_registry.node_networks[&network]
            .nodes
            .iter()
            .find_map(|(&id, n)| {
                let p = n.data.as_any_ref().downcast_ref::<ParameterData>()?;
                (p.param_name == "unit_cell").then_some((p.param_index, id))
            })
        else {
            continue;
        };
        let before = wired_call_sites(&designer, &network, index);
        if before == 0 {
            continue;
        }
        ui_rename(&mut designer, &network, node_id, "structure");
        assert_eq!(
            wired_call_sites(&designer, &network, index),
            before,
            "{network}: renaming `unit_cell` dropped call-site wires"
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "the demolib has no wired `unit_cell` parameter left to rename"
    );
}

/// Strips `"id"` from every recorded parameter in the host's `uses` tables —
/// what a host saved against a pre-`param_id` library records.
fn strip_recorded_param_ids(value: &mut serde_json::Value, in_params: bool) {
    match value {
        serde_json::Value::Object(map) => {
            if in_params {
                map.remove("id");
            }
            for (key, child) in map.iter_mut() {
                strip_recorded_param_ids(child, key == "params");
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_recorded_param_ids(item, in_params);
            }
        }
        _ => {}
    }
}

/// A host wired against an id-less library recorded id-less interfaces in
/// its `uses`. Opening it now that the library's parameters get ids on load
/// must read as "nothing changed" — no reconciled node, no dropped wire — and
/// the library's parameters must then rename safely.
#[test]
fn host_linking_a_legacy_library_opens_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let (mut lib, _, _) = load_legacy_project("legacy_param_lib_src.cnnd");
    // The library is the legacy `Sub` alone, still id-less on disk.
    lib.node_type_registry.node_networks.remove("main");
    let sub = lib.node_type_registry.node_networks.get_mut("Sub").unwrap();
    for node in sub.nodes.values_mut() {
        if let Some(p) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
            p.param_id = None;
        }
    }
    for param in sub.node_type.parameters.iter_mut() {
        param.id = None;
    }
    sub.next_param_id = 1;
    lib.node_type_registry.param_id_floor = 1;
    save_node_networks_to_file(
        &mut lib.node_type_registry,
        &dir.path().join("lib.cnnd"),
        false,
        &HashMap::new(),
    )
    .unwrap();

    let host = dir.path().join("host.cnnd");
    let mut d = StructureDesigner::new();
    d.new_project();
    d.save_node_networks_as(&host.to_string_lossy()).unwrap();
    d.link_library("lib.cnnd", "lib").unwrap();
    d.set_active_node_network_name(Some("Main".to_string()));
    let i1 = d.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = d.add_node("int", DVec2::new(0.0, 80.0));
    let s = d.add_node("lib.Sub", DVec2::new(150.0, 0.0));
    d.connect_nodes(i1, 0, s, 0);
    d.connect_nodes(i2, 0, s, 1);
    d.save_node_networks().unwrap().unwrap();

    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&host).unwrap()).unwrap();
    strip_recorded_param_ids(&mut json, false);
    std::fs::write(&host, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    let mut d = StructureDesigner::new();
    d.load_node_networks(&host.to_string_lossy()).unwrap();
    let report = d.take_load_library_report();
    assert!(
        report
            .as_ref()
            .is_none_or(|r| r.reconciled_nodes.is_empty() && r.dropped_wires.is_empty()),
        "opening reported a change that is not one: {report:?}"
    );
    assert_eq!(wired_call_sites(&d, "lib.Sub", 0), 1);
    assert_eq!(wired_call_sites(&d, "lib.Sub", 1), 1);
    assert!(
        d.node_type_registry.node_networks["lib.Sub"]
            .node_type
            .parameters
            .iter()
            .all(|p| p.id.is_some()),
        "the mounted library's parameters got ids"
    );
}
