//! P3 of `doc/design_isosurface_node.md`: the `isosurface` node's stored
//! properties survive a `.cnnd` round-trip.
//!
//! The interesting one is `colormap`. It is an enum owned by
//! `atomcad-crystolecule` (which is why that crate, not `atomcad-display`, is
//! where the value type lives — `display` has no `serde` dependency and should
//! not acquire one to host a persisted enum), and its serialized form has to
//! stay forward-compatible so that adding a second ramp does not invalidate
//! every project written before it.
//!
//! `alpha` is here for the same reason it is editable in P3 while having no
//! render effect: the file written today must already be correct for when the
//! transparent draw path arrives.

use atomcad_crystolecule::field::Colormap;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::isosurface::{IsosurfaceNodeData, LevelMode};
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn isosurface_node_cnnd_roundtrip() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    // Every property deliberately off its default, so a field that silently
    // falls back to the default fails the assertion rather than passing by
    // coincidence.
    let node_id = designer.add_node("isosurface", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(
        node_id,
        Box::new(IsosurfaceNodeData {
            level_mode: LevelMode::Fraction,
            level: 0.0035,
            level_fraction: 0.935,
            positive_color: DVec3::new(0.11, 0.22, 0.33),
            negative_color: DVec3::new(0.44, 0.55, 0.66),
            alpha: 0.85,
            colormap: Colormap::BlueWhiteRed,
            color_min: -0.125,
            color_max: 0.375,
        }),
    );

    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("isosurface.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");

    let mut reloaded = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut reloaded, path.to_str().unwrap()).expect("load");

    let node = reloaded
        .node_networks
        .get("test")
        .expect("the network reloads")
        .nodes
        .get(&node_id)
        .expect("the isosurface node reloads");
    assert_eq!(node.node_type_name, "isosurface");

    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<IsosurfaceNodeData>()
        .expect("the reloaded node carries IsosurfaceNodeData");

    assert_eq!(data.level_mode, LevelMode::Fraction);
    assert_eq!(data.level, 0.0035);
    assert_eq!(data.level_fraction, 0.935);
    assert_eq!(data.positive_color, DVec3::new(0.11, 0.22, 0.33));
    assert_eq!(data.negative_color, DVec3::new(0.44, 0.55, 0.66));
    assert_eq!(data.alpha, 0.85);
    assert_eq!(data.colormap, Colormap::BlueWhiteRed);
    assert_eq!(data.color_min, -0.125);
    assert_eq!(data.color_max, 0.375);
}

/// A file written before the colormap fields existed must still load — the
/// `#[serde(default)]` contract, exercised against a hand-written payload
/// rather than against a struct literal, which would prove nothing.
#[test]
fn a_node_saved_without_the_colormap_fields_still_loads() {
    let json = r#"{
        "level": 0.02,
        "positive_color": [0.2, 0.4, 0.9],
        "negative_color": [0.9, 0.3, 0.25],
        "alpha": 0.4
    }"#;

    let data: IsosurfaceNodeData =
        serde_json::from_str(json).expect("the pre-colormap shape must still deserialize");
    assert_eq!(data.colormap, Colormap::BlueWhiteRed);
    // The load-time default is `Absolute`, NOT the enum's `Auto`: a document
    // written before the level modes existed carries a `level` its author
    // typed, and defaulting to auto would ignore it and move the surface.
    // `IsosurfaceNodeData::default()` still gives `Auto`, for a *new* node.
    assert_eq!(data.level_mode, LevelMode::Absolute);
    assert_eq!(data.level_fraction, 0.72);
    assert_eq!(data.color_min, -0.05);
    assert_eq!(data.color_max, 0.05);
}

/// The text format is the AI/CLI authoring surface, and `colormap` is the one
/// property whose spelling could drift from what the parser accepts — the other
/// six are ordinary floats and vectors.
#[test]
fn isosurface_node_text_format_roundtrip() {
    use atomcad_structure_designer::data_type::DataType;
    use atomcad_structure_designer::node_network::NodeNetwork;
    use atomcad_structure_designer::node_type::{NodeType, NodeTypeCategory, OutputPinDefinition};
    use atomcad_structure_designer::text_format::{edit_network, serialize_network};

    fn empty_network() -> NodeNetwork {
        NodeNetwork::new(NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            summary: None,
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_pins: OutputPinDefinition::single(DataType::Isosurface),
            zone_input_pins: vec![],
            zone_output_pins: vec![],
            public: true,
            node_data_creator: || Box::new(atomcad_structure_designer::node_data::NoData {}),
            node_data_saver: atomcad_structure_designer::node_type::no_data_saver,
            node_data_loader: atomcad_structure_designer::node_type::no_data_loader,
        })
    }

    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();

    let source = r#"
        iso = isosurface { level_mode: "fraction", level: 0.0035, level_fraction: 0.935, alpha: 0.85, colormap: "blue_white_red", color_min: -0.125, color_max: 0.375 }
    "#;
    let result = edit_network(&mut network, &registry, source, true);
    assert!(result.success, "initial edit succeeds: {:?}", result.errors);

    let serialized = serialize_network(&network, &registry, Some("test"));
    assert!(
        serialized.contains("blue_white_red"),
        "the colormap is written out by name: {serialized}"
    );

    let mut network2 = empty_network();
    let result2 = edit_network(&mut network2, &registry, &serialized, true);
    assert!(
        result2.success,
        "round-trip edit succeeds: {:?}",
        result2.errors
    );
    assert_eq!(
        serialize_network(&network2, &registry, Some("test")),
        serialized,
        "text round-trip is stable"
    );
}
