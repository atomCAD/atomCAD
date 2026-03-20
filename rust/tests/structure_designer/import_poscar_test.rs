use rust_lib_flutter_cad::structure_designer::nodes::import_poscar::ImportPoscarData;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// Silicon diamond cubic POSCAR content
const SILICON_POSCAR: &str = "\
Si diamond cubic
1.0
5.431 0.000 0.000
0.000 5.431 0.000
0.000 0.000 5.431
Si
2
Direct
0.000 0.000 0.000
0.250 0.250 0.250
";

#[test]
fn test_import_poscar_from_content() {
    let data = ImportPoscarData::from_content(SILICON_POSCAR).unwrap();

    assert!(data.cached_unit_cell.is_some());
    assert!(data.cached_motif.is_some());

    let unit_cell = data.cached_unit_cell.as_ref().unwrap();
    assert!((unit_cell.cell_length_a - 5.431).abs() < 1e-6);

    let motif = data.cached_motif.as_ref().unwrap();
    assert_eq!(motif.sites.len(), 2);
    assert_eq!(motif.sites[0].atomic_number, 14); // Si
}

#[test]
fn test_import_poscar_from_content_invalid() {
    let result = ImportPoscarData::from_content("invalid poscar data");
    assert!(result.is_err());
}

#[test]
fn test_import_poscar_text_properties_roundtrip() {
    let mut data = ImportPoscarData::new();
    data.file_name = Some("test.poscar".to_string());

    let props = data.get_text_properties();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].0, "file_name");
    if let TextValue::String(ref s) = props[0].1 {
        assert_eq!(s, "test.poscar");
    } else {
        panic!("Expected String TextValue");
    }

    // Roundtrip
    let mut data2 = ImportPoscarData::new();
    let mut prop_map = HashMap::new();
    for (key, value) in &props {
        prop_map.insert(key.clone(), value.clone());
    }
    data2.set_text_properties(&prop_map).unwrap();
    assert_eq!(data2.file_name, Some("test.poscar".to_string()));
}

#[test]
fn test_import_poscar_text_properties_empty() {
    let data = ImportPoscarData::new();
    let props = data.get_text_properties();
    assert!(props.is_empty());
}

#[test]
fn test_import_poscar_subtitle_with_file() {
    let mut data = ImportPoscarData::new();
    data.file_name = Some("crystal.poscar".to_string());

    let connected = std::collections::HashSet::new();
    let subtitle = data.get_subtitle(&connected);
    assert_eq!(subtitle, Some("crystal.poscar".to_string()));
}

#[test]
fn test_import_poscar_subtitle_when_connected() {
    let mut data = ImportPoscarData::new();
    data.file_name = Some("crystal.poscar".to_string());

    let mut connected = std::collections::HashSet::new();
    connected.insert("file_name".to_string());
    let subtitle = data.get_subtitle(&connected);
    assert!(subtitle.is_none());
}

#[test]
fn test_import_poscar_node_type() {
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::nodes::import_poscar::get_node_type;

    let nt = get_node_type();
    assert_eq!(nt.name, "import_poscar");
    assert!(nt.public);
    assert_eq!(nt.parameters.len(), 1);
    assert_eq!(nt.parameters[0].name, "file_name");
    assert_eq!(nt.parameters[0].data_type, DataType::String);
    assert_eq!(nt.output_type, DataType::UnitCell);
    assert_eq!(nt.additional_output_types.len(), 1);
    assert_eq!(nt.additional_output_types[0], DataType::Motif);
}

#[test]
fn test_import_poscar_get_output_pin_type() {
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::nodes::import_poscar::get_node_type;

    let nt = get_node_type();

    // Pin 0 should be UnitCell
    assert_eq!(nt.get_output_pin_type(0), DataType::UnitCell);

    // Pin 1 should be Motif
    assert_eq!(nt.get_output_pin_type(1), DataType::Motif);

    // Pin 2 should be None (out of range)
    assert_eq!(nt.get_output_pin_type(2), DataType::None);
}

// Test that import_poscar node is registered in the registry
#[test]
fn test_import_poscar_registered() {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("import_poscar");
    assert!(node_type.is_some(), "import_poscar should be registered");
    assert_eq!(node_type.unwrap().name, "import_poscar");
}

use rust_lib_flutter_cad::structure_designer::node_data::NodeData;

#[test]
fn test_import_poscar_clone_box() {
    let mut data = ImportPoscarData::new();
    data.file_name = Some("test.poscar".to_string());
    let cloned = data.clone_box();
    let cloned_data = cloned
        .as_any_ref()
        .downcast_ref::<ImportPoscarData>()
        .unwrap();
    assert_eq!(cloned_data.file_name, Some("test.poscar".to_string()));
}
