use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
// Primitive nodes
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::nodes::bool::BoolData;
use rust_lib_flutter_cad::structure_designer::nodes::string::StringData;
// 2D vector nodes
use rust_lib_flutter_cad::structure_designer::nodes::ivec2::IVec2Data;
use rust_lib_flutter_cad::structure_designer::nodes::vec2::Vec2Data;
// 3D vector nodes
use rust_lib_flutter_cad::structure_designer::nodes::ivec3::IVec3Data;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
// 2D geometry nodes
use rust_lib_flutter_cad::structure_designer::nodes::circle::CircleData;
use rust_lib_flutter_cad::structure_designer::nodes::rect::RectData;
use rust_lib_flutter_cad::structure_designer::nodes::polygon::PolygonData;
use rust_lib_flutter_cad::structure_designer::nodes::reg_poly::RegPolyData;
use rust_lib_flutter_cad::structure_designer::nodes::half_plane::HalfPlaneData;
// 3D geometry nodes
use rust_lib_flutter_cad::structure_designer::nodes::sphere::SphereData;
use rust_lib_flutter_cad::structure_designer::nodes::cuboid::CuboidData;
use rust_lib_flutter_cad::structure_designer::nodes::half_space::HalfSpaceData;
use rust_lib_flutter_cad::structure_designer::nodes::extrude::ExtrudeData;
// Transformation nodes
use rust_lib_flutter_cad::structure_designer::nodes::lattice_move::LatticeMoveData;
use rust_lib_flutter_cad::structure_designer::nodes::lattice_rot::LatticeRotData;
use rust_lib_flutter_cad::structure_designer::nodes::geo_trans::GeoTransData;
use rust_lib_flutter_cad::structure_designer::nodes::lattice_symop::LatticeSymopData;
// Atomic nodes
use rust_lib_flutter_cad::structure_designer::nodes::unit_cell::UnitCellData;
use rust_lib_flutter_cad::structure_designer::nodes::atom_cut::AtomCutData;
use rust_lib_flutter_cad::structure_designer::nodes::motif::MotifData;
use rust_lib_flutter_cad::structure_designer::nodes::drawing_plane::DrawingPlaneData;
use rust_lib_flutter_cad::structure_designer::nodes::atom_fill::AtomFillData;
// I/O nodes
use rust_lib_flutter_cad::structure_designer::nodes::export_xyz::ExportXYZData;
use rust_lib_flutter_cad::structure_designer::nodes::import_xyz::ImportXYZData;
// Programming nodes
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::comment::CommentData;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::facet_shell::{FacetShellData, Facet};
// glam types
use glam::i32::IVec2;
use glam::i32::IVec3;
use glam::f64::DVec2;
use glam::f64::DVec3;
use std::collections::HashMap;

// ============================================================================
// Helper Functions
// ============================================================================

/// Converts a Vec<(String, TextValue)> to a HashMap for set_text_properties
fn props_to_hashmap(props: Vec<(String, TextValue)>) -> HashMap<String, TextValue> {
    props.into_iter().collect()
}

/// Tests that get_text_properties -> set_text_properties roundtrip preserves values
/// by comparing the properties themselves (since many types don't implement PartialEq)
fn test_roundtrip<T: NodeData + Clone>(original: &T) {
    let props = original.get_text_properties();
    let mut restored = original.clone();
    let props_map = props_to_hashmap(props);
    restored.set_text_properties(&props_map).expect("set_text_properties failed");

    // Get properties again to compare
    let original_props = original.get_text_properties();
    let restored_props = restored.get_text_properties();
    assert_eq!(original_props, restored_props, "Roundtrip failed: properties differ");
}

// ============================================================================
// Primitive Node Tests
// ============================================================================

#[test]
fn test_int_data_text_properties() {
    let data = IntData { value: 42 };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 1);
    assert_eq!(props[0].0, "value");
    assert_eq!(props[0].1, TextValue::Int(42));

    // Test set
    let mut data2 = IntData { value: 0 };
    let mut map = HashMap::new();
    map.insert("value".to_string(), TextValue::Int(99));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.value, 99);
}

#[test]
fn test_float_data_text_properties() {
    let data = FloatData { value: 3.14 };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 1);
    assert_eq!(props[0].0, "value");
    assert_eq!(props[0].1, TextValue::Float(3.14));

    // Test set
    let mut data2 = FloatData { value: 0.0 };
    let mut map = HashMap::new();
    map.insert("value".to_string(), TextValue::Float(2.718));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.value, 2.718);
}

#[test]
fn test_bool_data_text_properties() {
    let data = BoolData { value: true };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 1);
    assert_eq!(props[0].0, "value");
    assert_eq!(props[0].1, TextValue::Bool(true));

    // Test set
    let mut data2 = BoolData { value: true };
    let mut map = HashMap::new();
    map.insert("value".to_string(), TextValue::Bool(false));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.value, false);
}

#[test]
fn test_string_data_text_properties() {
    let data = StringData { value: "hello world".to_string() };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 1);
    assert_eq!(props[0].0, "value");
    assert_eq!(props[0].1, TextValue::String("hello world".to_string()));

    // Test set
    let mut data2 = StringData { value: "".to_string() };
    let mut map = HashMap::new();
    map.insert("value".to_string(), TextValue::String("new value".to_string()));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.value, "new value");
}

// ============================================================================
// Vector Node Tests
// ============================================================================

#[test]
fn test_ivec3_data_text_properties() {
    let data = IVec3Data { value: IVec3::new(1, 2, 3) };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 3);
    assert_eq!(props[0], ("x".to_string(), TextValue::Int(1)));
    assert_eq!(props[1], ("y".to_string(), TextValue::Int(2)));
    assert_eq!(props[2], ("z".to_string(), TextValue::Int(3)));

    // Test set
    let mut data2 = IVec3Data { value: IVec3::ZERO };
    let mut map = HashMap::new();
    map.insert("x".to_string(), TextValue::Int(10));
    map.insert("y".to_string(), TextValue::Int(20));
    map.insert("z".to_string(), TextValue::Int(30));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.value.x, 10);
    assert_eq!(data2.value.y, 20);
    assert_eq!(data2.value.z, 30);
}

#[test]
fn test_vec3_data_text_properties() {
    let data = Vec3Data { value: DVec3::new(1.5, 2.5, 3.5) };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 3);
    assert_eq!(props[0], ("x".to_string(), TextValue::Float(1.5)));
    assert_eq!(props[1], ("y".to_string(), TextValue::Float(2.5)));
    assert_eq!(props[2], ("z".to_string(), TextValue::Float(3.5)));

    // Test set
    let mut data2 = Vec3Data { value: DVec3::ZERO };
    let mut map = HashMap::new();
    map.insert("x".to_string(), TextValue::Float(10.5));
    map.insert("y".to_string(), TextValue::Float(20.5));
    map.insert("z".to_string(), TextValue::Float(30.5));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.value.x, 10.5);
    assert_eq!(data2.value.y, 20.5);
    assert_eq!(data2.value.z, 30.5);
}

// ============================================================================
// Geometry Node Tests
// ============================================================================

#[test]
fn test_sphere_data_text_properties() {
    let data = SphereData {
        center: IVec3::new(1, 2, 3),
        radius: 5
    };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 2);
    assert_eq!(props[0], ("center".to_string(), TextValue::IVec3(IVec3::new(1, 2, 3))));
    assert_eq!(props[1], ("radius".to_string(), TextValue::Int(5)));

    // Test set
    let mut data2 = SphereData {
        center: IVec3::ZERO,
        radius: 0
    };
    let mut map = HashMap::new();
    map.insert("center".to_string(), TextValue::IVec3(IVec3::new(10, 20, 30)));
    map.insert("radius".to_string(), TextValue::Int(15));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.center, IVec3::new(10, 20, 30));
    assert_eq!(data2.radius, 15);
}

#[test]
fn test_cuboid_data_text_properties() {
    let data = CuboidData {
        min_corner: IVec3::new(0, 0, 0),
        extent: IVec3::new(10, 20, 30)
    };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 2);
    assert_eq!(props[0], ("min_corner".to_string(), TextValue::IVec3(IVec3::new(0, 0, 0))));
    assert_eq!(props[1], ("extent".to_string(), TextValue::IVec3(IVec3::new(10, 20, 30))));
}

// ============================================================================
// Programming Node Tests
// ============================================================================

#[test]
fn test_range_data_text_properties() {
    let data = RangeData { start: 0, step: 2, count: 10 };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 3);
    assert_eq!(props[0], ("start".to_string(), TextValue::Int(0)));
    assert_eq!(props[1], ("step".to_string(), TextValue::Int(2)));
    assert_eq!(props[2], ("count".to_string(), TextValue::Int(10)));

    // Test set
    let mut data2 = RangeData { start: 0, step: 1, count: 1 };
    let mut map = HashMap::new();
    map.insert("start".to_string(), TextValue::Int(5));
    map.insert("step".to_string(), TextValue::Int(3));
    map.insert("count".to_string(), TextValue::Int(20));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.start, 5);
    assert_eq!(data2.step, 3);
    assert_eq!(data2.count, 20);
}

#[test]
fn test_map_data_text_properties() {
    let data = MapData {
        input_type: DataType::Int,
        output_type: DataType::Float
    };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 2);
    assert_eq!(props[0], ("input_type".to_string(), TextValue::DataType(DataType::Int)));
    assert_eq!(props[1], ("output_type".to_string(), TextValue::DataType(DataType::Float)));

    // Test set
    let mut data2 = MapData {
        input_type: DataType::Float,
        output_type: DataType::Int
    };
    let mut map = HashMap::new();
    map.insert("input_type".to_string(), TextValue::DataType(DataType::Vec3));
    map.insert("output_type".to_string(), TextValue::DataType(DataType::Geometry));
    data2.set_text_properties(&map).unwrap();
    assert_eq!(data2.input_type, DataType::Vec3);
    assert_eq!(data2.output_type, DataType::Geometry);
}

#[test]
fn test_parameter_data_text_properties() {
    let data = ParameterData {
        param_index: 0,
        param_name: "my_param".to_string(),
        data_type: DataType::Float,
        sort_order: 1,
        data_type_str: Some("Float".to_string()),
        error: None,
    };
    let props = data.get_text_properties();

    // Should have param_index, param_name, data_type, sort_order, data_type_str
    assert!(props.iter().any(|(k, v)| k == "param_index" && *v == TextValue::Int(0)));
    assert!(props.iter().any(|(k, v)| k == "param_name" && *v == TextValue::String("my_param".to_string())));
    assert!(props.iter().any(|(k, v)| k == "data_type" && *v == TextValue::DataType(DataType::Float)));
    assert!(props.iter().any(|(k, v)| k == "sort_order" && *v == TextValue::Int(1)));
    assert!(props.iter().any(|(k, v)| k == "data_type_str" && *v == TextValue::String("Float".to_string())));
}

// ============================================================================
// Comment Node Test
// ============================================================================

#[test]
fn test_comment_data_text_properties() {
    let data = CommentData {
        label: "My Label".to_string(),
        text: "Description text".to_string(),
        width: 200.0,
        height: 100.0,
    };
    let props = data.get_text_properties();

    assert_eq!(props.len(), 4);
    assert_eq!(props[0], ("label".to_string(), TextValue::String("My Label".to_string())));
    assert_eq!(props[1], ("text".to_string(), TextValue::String("Description text".to_string())));
    assert_eq!(props[2], ("width".to_string(), TextValue::Float(200.0)));
    assert_eq!(props[3], ("height".to_string(), TextValue::Float(100.0)));
}

// ============================================================================
// Complex Node Tests (with nested structures)
// ============================================================================

#[test]
fn test_expr_data_text_properties() {
    let data = ExprData {
        parameters: vec![
            ExprParameter {
                name: "x".to_string(),
                data_type: DataType::Float,
                data_type_str: Some("Float".to_string()),
            },
            ExprParameter {
                name: "y".to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            },
        ],
        expression: "x + y".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let props = data.get_text_properties();

    // Should have expression and parameters
    assert!(props.iter().any(|(k, _)| k == "expression"));
    assert!(props.iter().any(|(k, _)| k == "parameters"));

    // Find expression
    let expr_val = props.iter().find(|(k, _)| k == "expression").map(|(_, v)| v);
    assert_eq!(expr_val, Some(&TextValue::String("x + y".to_string())));

    // Find parameters array
    let params_val = props.iter().find(|(k, _)| k == "parameters").map(|(_, v)| v);
    if let Some(TextValue::Array(params)) = params_val {
        assert_eq!(params.len(), 2);

        // Check first parameter
        if let TextValue::Object(obj) = &params[0] {
            assert!(obj.iter().any(|(k, v)| k == "name" && *v == TextValue::String("x".to_string())));
            assert!(obj.iter().any(|(k, v)| k == "data_type" && *v == TextValue::DataType(DataType::Float)));
        } else {
            panic!("Expected Object for parameter");
        }
    } else {
        panic!("Expected Array for parameters");
    }
}

#[test]
fn test_expr_data_set_text_properties() {
    let mut data = ExprData {
        parameters: vec![],
        expression: "".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };

    // Create new properties
    let params = TextValue::Array(vec![
        TextValue::Object(vec![
            ("name".to_string(), TextValue::String("z".to_string())),
            ("data_type".to_string(), TextValue::DataType(DataType::Vec3)),
        ]),
    ]);

    let mut map = HashMap::new();
    map.insert("expression".to_string(), TextValue::String("z * 2".to_string()));
    map.insert("parameters".to_string(), params);

    data.set_text_properties(&map).unwrap();

    assert_eq!(data.expression, "z * 2");
    assert_eq!(data.parameters.len(), 1);
    assert_eq!(data.parameters[0].name, "z");
    assert_eq!(data.parameters[0].data_type, DataType::Vec3);
}

#[test]
fn test_facet_shell_data_text_properties() {
    let data = FacetShellData {
        max_miller_index: 2,
        center: IVec3::new(0, 0, 0),
        facets: vec![
            Facet {
                miller_index: IVec3::new(1, 0, 0),
                shift: 5,
                symmetrize: true,
                visible: true,
            },
            Facet {
                miller_index: IVec3::new(0, 1, 0),
                shift: 3,
                symmetrize: false,
                visible: true,
            },
        ],
        selected_facet_index: None,
        cached_facets: vec![],
        cached_facet_to_original_index: vec![],
    };
    let props = data.get_text_properties();

    assert!(props.iter().any(|(k, v)| k == "max_miller_index" && *v == TextValue::Int(2)));
    assert!(props.iter().any(|(k, v)| k == "center" && *v == TextValue::IVec3(IVec3::ZERO)));

    // Check facets array
    let facets_val = props.iter().find(|(k, _)| k == "facets").map(|(_, v)| v);
    if let Some(TextValue::Array(facets)) = facets_val {
        assert_eq!(facets.len(), 2);

        // Check first facet
        if let TextValue::Object(obj) = &facets[0] {
            assert!(obj.iter().any(|(k, v)| k == "miller_index" && *v == TextValue::IVec3(IVec3::new(1, 0, 0))));
            assert!(obj.iter().any(|(k, v)| k == "shift" && *v == TextValue::Int(5)));
            assert!(obj.iter().any(|(k, v)| k == "symmetrize" && *v == TextValue::Bool(true)));
        } else {
            panic!("Expected Object for facet");
        }
    } else {
        panic!("Expected Array for facets");
    }
}

#[test]
fn test_facet_shell_data_set_text_properties() {
    let mut data = FacetShellData {
        max_miller_index: 1,
        center: IVec3::ZERO,
        facets: vec![],
        selected_facet_index: None,
        cached_facets: vec![],
        cached_facet_to_original_index: vec![],
    };

    // Create facets
    let facets = TextValue::Array(vec![
        TextValue::Object(vec![
            ("miller_index".to_string(), TextValue::IVec3(IVec3::new(1, 1, 1))),
            ("shift".to_string(), TextValue::Int(10)),
            ("symmetrize".to_string(), TextValue::Bool(true)),
            ("visible".to_string(), TextValue::Bool(false)),
        ]),
    ]);

    let mut map = HashMap::new();
    map.insert("max_miller_index".to_string(), TextValue::Int(3));
    map.insert("center".to_string(), TextValue::IVec3(IVec3::new(1, 2, 3)));
    map.insert("facets".to_string(), facets);

    data.set_text_properties(&map).unwrap();

    assert_eq!(data.max_miller_index, 3);
    assert_eq!(data.center, IVec3::new(1, 2, 3));
    assert_eq!(data.facets.len(), 1);
    assert_eq!(data.facets[0].miller_index, IVec3::new(1, 1, 1));
    assert_eq!(data.facets[0].shift, 10);
    assert_eq!(data.facets[0].symmetrize, true);
    assert_eq!(data.facets[0].visible, false);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_set_text_properties_wrong_type() {
    let mut data = IntData { value: 0 };
    let mut map = HashMap::new();
    map.insert("value".to_string(), TextValue::String("not an int".to_string()));

    let result = data.set_text_properties(&map);
    assert!(result.is_err());
}

#[test]
fn test_set_text_properties_partial_update() {
    // Setting only some properties should leave others unchanged
    let mut data = SphereData {
        center: IVec3::new(1, 2, 3),
        radius: 5
    };

    let mut map = HashMap::new();
    map.insert("radius".to_string(), TextValue::Int(10));
    // Don't set center

    data.set_text_properties(&map).unwrap();

    assert_eq!(data.center, IVec3::new(1, 2, 3)); // unchanged
    assert_eq!(data.radius, 10); // changed
}

// ============================================================================
// Roundtrip Tests
// ============================================================================

#[test]
fn test_int_roundtrip() {
    test_roundtrip(&IntData { value: 42 });
}

#[test]
fn test_float_roundtrip() {
    test_roundtrip(&FloatData { value: 3.14159 });
}

#[test]
fn test_bool_roundtrip() {
    test_roundtrip(&BoolData { value: true });
    test_roundtrip(&BoolData { value: false });
}

#[test]
fn test_string_roundtrip() {
    test_roundtrip(&StringData { value: "hello".to_string() });
    test_roundtrip(&StringData { value: "".to_string() });
    test_roundtrip(&StringData { value: "multi\nline\nstring".to_string() });
}

#[test]
fn test_ivec3_roundtrip() {
    test_roundtrip(&IVec3Data { value: IVec3::new(10, 20, 30) });
    test_roundtrip(&IVec3Data { value: IVec3::new(-5, 0, 100) });
}

#[test]
fn test_vec3_roundtrip() {
    test_roundtrip(&Vec3Data { value: DVec3::new(1.5, 2.5, 3.5) });
}

#[test]
fn test_sphere_roundtrip() {
    test_roundtrip(&SphereData {
        center: IVec3::new(1, 2, 3),
        radius: 5
    });
}

#[test]
fn test_range_roundtrip() {
    test_roundtrip(&RangeData { start: 0, step: 2, count: 10 });
}

#[test]
fn test_map_roundtrip() {
    test_roundtrip(&MapData {
        input_type: DataType::Int,
        output_type: DataType::Float
    });
}

#[test]
fn test_comment_roundtrip() {
    test_roundtrip(&CommentData {
        label: "Test".to_string(),
        text: "Description".to_string(),
        width: 150.0,
        height: 75.0,
    });
}

// ============================================================================
// 2D Vector Node Roundtrip Tests
// ============================================================================

#[test]
fn test_ivec2_roundtrip() {
    test_roundtrip(&IVec2Data { value: IVec2::new(10, 20) });
    test_roundtrip(&IVec2Data { value: IVec2::new(-5, 0) });
}

#[test]
fn test_vec2_roundtrip() {
    test_roundtrip(&Vec2Data { value: DVec2::new(1.5, 2.5) });
    test_roundtrip(&Vec2Data { value: DVec2::new(-3.14, 0.0) });
}

// ============================================================================
// 2D Geometry Node Roundtrip Tests
// ============================================================================

#[test]
fn test_circle_roundtrip() {
    test_roundtrip(&CircleData {
        center: IVec2::new(5, 10),
        radius: 3,
    });
}

#[test]
fn test_rect_roundtrip() {
    test_roundtrip(&RectData {
        min_corner: IVec2::new(-2, -2),
        extent: IVec2::new(4, 6),
    });
}

#[test]
fn test_polygon_roundtrip() {
    test_roundtrip(&PolygonData {
        vertices: vec![
            IVec2::new(-1, -1),
            IVec2::new(1, -1),
            IVec2::new(0, 1),
        ],
    });
    // Test with more vertices
    test_roundtrip(&PolygonData {
        vertices: vec![
            IVec2::new(0, 0),
            IVec2::new(2, 0),
            IVec2::new(3, 2),
            IVec2::new(1, 3),
            IVec2::new(-1, 2),
        ],
    });
}

#[test]
fn test_reg_poly_roundtrip() {
    test_roundtrip(&RegPolyData {
        num_sides: 6,
        radius: 5,
    });
}

#[test]
fn test_half_plane_roundtrip() {
    test_roundtrip(&HalfPlaneData {
        point1: IVec2::new(0, 0),
        point2: IVec2::new(1, 1),
    });
}

// ============================================================================
// 3D Geometry Node Roundtrip Tests
// ============================================================================

#[test]
fn test_cuboid_roundtrip() {
    test_roundtrip(&CuboidData {
        min_corner: IVec3::new(-1, -2, -3),
        extent: IVec3::new(2, 4, 6),
    });
}

#[test]
fn test_half_space_roundtrip() {
    test_roundtrip(&HalfSpaceData {
        max_miller_index: 2,
        miller_index: IVec3::new(1, 0, 0),
        center: IVec3::new(0, 0, 0),
        shift: 5,
        subdivision: 2,
    });
}

#[test]
fn test_extrude_roundtrip() {
    test_roundtrip(&ExtrudeData {
        height: 10,
        extrude_direction: IVec3::new(0, 0, 1),
        infinite: false,
        subdivision: 1,
    });
    // Test with infinite extrusion
    test_roundtrip(&ExtrudeData {
        height: 0,
        extrude_direction: IVec3::new(1, 1, 0),
        infinite: true,
        subdivision: 2,
    });
}

// ============================================================================
// Transformation Node Roundtrip Tests
// ============================================================================

#[test]
fn test_lattice_move_roundtrip() {
    test_roundtrip(&LatticeMoveData {
        translation: IVec3::new(5, 10, 15),
        lattice_subdivision: 1,
    });
    test_roundtrip(&LatticeMoveData {
        translation: IVec3::new(-1, 2, 3),
        lattice_subdivision: 4,
    });
}

#[test]
fn test_lattice_rot_roundtrip() {
    // Test with axis_index set (Note: axis_index is optional,
    // but roundtrip only works when it's Some because get_text_properties
    // conditionally includes it)
    test_roundtrip(&LatticeRotData {
        axis_index: Some(0),
        step: 2,
        pivot_point: IVec3::new(1, 2, 3),
    });
}

#[test]
fn test_geo_trans_roundtrip() {
    test_roundtrip(&GeoTransData {
        translation: IVec3::new(1, 2, 3),
        rotation: IVec3::new(0, 1, 0),
        transform_only_frame: false,
    });
    test_roundtrip(&GeoTransData {
        translation: IVec3::ZERO,
        rotation: IVec3::ZERO,
        transform_only_frame: true,
    });
}

#[test]
fn test_lattice_symop_roundtrip() {
    // Test with rotation_axis set (Note: rotation_axis is optional,
    // but roundtrip only works when it's Some because get_text_properties
    // conditionally includes it)
    test_roundtrip(&LatticeSymopData {
        translation: IVec3::new(1, 2, 3),
        rotation_axis: Some(DVec3::new(0.0, 0.0, 1.0)),
        rotation_angle_degrees: 90.0,
        transform_only_frame: false,
    });
    test_roundtrip(&LatticeSymopData {
        translation: IVec3::ZERO,
        rotation_axis: Some(DVec3::new(1.0, 1.0, 0.0).normalize()),
        rotation_angle_degrees: 180.0,
        transform_only_frame: true,
    });
}

// ============================================================================
// Atomic Node Roundtrip Tests
// ============================================================================

#[test]
fn test_unit_cell_roundtrip() {
    // Cubic unit cell (diamond)
    test_roundtrip(&UnitCellData {
        cell_length_a: 3.567,
        cell_length_b: 3.567,
        cell_length_c: 3.567,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 90.0,
        cell_angle_gamma: 90.0,
    });
    // Non-cubic unit cell
    test_roundtrip(&UnitCellData {
        cell_length_a: 4.0,
        cell_length_b: 5.0,
        cell_length_c: 6.0,
        cell_angle_alpha: 90.0,
        cell_angle_beta: 100.0,
        cell_angle_gamma: 90.0,
    });
}

#[test]
fn test_atom_cut_roundtrip() {
    test_roundtrip(&AtomCutData {
        cut_sdf_value: 0.0,
        unit_cell_size: 3.567,
    });
    test_roundtrip(&AtomCutData {
        cut_sdf_value: 1.5,
        unit_cell_size: 5.43,
    });
}

#[test]
fn test_motif_roundtrip() {
    test_roundtrip(&MotifData {
        definition: "C 0 0 0".to_string(),
        name: Some("diamond".to_string()),
        error: None,
        motif: None,
    });
    // Test with empty name
    test_roundtrip(&MotifData {
        definition: "".to_string(),
        name: None,
        error: None,
        motif: None,
    });
}

#[test]
fn test_drawing_plane_roundtrip() {
    test_roundtrip(&DrawingPlaneData {
        max_miller_index: 2,
        miller_index: IVec3::new(1, 1, 0),
        center: IVec3::new(0, 0, 0),
        shift: 2,
        subdivision: 1,
    });
    test_roundtrip(&DrawingPlaneData {
        max_miller_index: 3,
        miller_index: IVec3::new(0, 0, 1),
        center: IVec3::new(1, 2, 3),
        shift: -5,
        subdivision: 4,
    });
}

#[test]
fn test_atom_fill_roundtrip() {
    test_roundtrip(&AtomFillData {
        parameter_element_value_definition: "C=6".to_string(),
        motif_offset: DVec3::new(0.0, 0.0, 0.0),
        hydrogen_passivation: true,
        remove_single_bond_atoms_before_passivation: false,
        surface_reconstruction: false,
        invert_phase: false,
        error: None,
        parameter_element_values: HashMap::new(),
    });
    test_roundtrip(&AtomFillData {
        parameter_element_value_definition: "".to_string(),
        motif_offset: DVec3::new(0.25, 0.25, 0.25),
        hydrogen_passivation: false,
        remove_single_bond_atoms_before_passivation: true,
        surface_reconstruction: true,
        invert_phase: true,
        error: None,
        parameter_element_values: HashMap::new(),
    });
}

// ============================================================================
// I/O Node Roundtrip Tests
// ============================================================================

#[test]
fn test_export_xyz_roundtrip() {
    test_roundtrip(&ExportXYZData {
        file_name: "test_output.xyz".to_string(),
    });
}

#[test]
fn test_import_xyz_roundtrip() {
    test_roundtrip(&ImportXYZData {
        file_name: Some("test_input.xyz".to_string()),
        atomic_structure: None,
    });
    // Test with None file name
    test_roundtrip(&ImportXYZData {
        file_name: None,
        atomic_structure: None,
    });
}

// ============================================================================
// Complex Node Roundtrip Tests
// ============================================================================

#[test]
fn test_parameter_roundtrip() {
    test_roundtrip(&ParameterData {
        param_index: 0,
        param_name: "test_param".to_string(),
        data_type: DataType::Float,
        sort_order: 1,
        data_type_str: Some("Float".to_string()),
        error: None,
    });
}

#[test]
fn test_expr_roundtrip() {
    test_roundtrip(&ExprData {
        parameters: vec![
            ExprParameter {
                name: "x".to_string(),
                data_type: DataType::Float,
                data_type_str: Some("Float".to_string()),
            },
        ],
        expression: "x * 2".to_string(),
        expr: None,
        error: None,
        output_type: None,
    });
    // Test with multiple parameters
    test_roundtrip(&ExprData {
        parameters: vec![
            ExprParameter {
                name: "a".to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            },
            ExprParameter {
                name: "b".to_string(),
                data_type: DataType::Vec3,
                data_type_str: Some("Vec3".to_string()),
            },
        ],
        expression: "a + b.x".to_string(),
        expr: None,
        error: None,
        output_type: None,
    });
}

#[test]
fn test_facet_shell_roundtrip() {
    test_roundtrip(&FacetShellData {
        max_miller_index: 2,
        center: IVec3::new(0, 0, 0),
        facets: vec![
            Facet {
                miller_index: IVec3::new(1, 0, 0),
                shift: 5,
                symmetrize: true,
                visible: true,
            },
            Facet {
                miller_index: IVec3::new(0, 1, 0),
                shift: 3,
                symmetrize: false,
                visible: true,
            },
        ],
        selected_facet_index: None,
        cached_facets: vec![],
        cached_facet_to_original_index: vec![],
    });
}
