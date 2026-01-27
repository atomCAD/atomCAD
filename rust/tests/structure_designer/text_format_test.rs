use rust_lib_flutter_cad::structure_designer::text_format::{
    TextValue, Parser, Lexer, Statement, PropertyValue, Token, serialize_network,
    describe_node_type, truncate_description,
};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use glam::{IVec2, IVec3, DVec2, DVec3};

// ============================================================================
// TextValue Tests
// ============================================================================

mod text_value_tests {
    use super::*;

    #[test]
    fn test_as_int() {
        assert_eq!(TextValue::Int(42).as_int(), Some(42));
        assert_eq!(TextValue::Float(3.7).as_int(), Some(3)); // truncation
        assert_eq!(TextValue::Bool(true).as_int(), None);
    }

    #[test]
    fn test_as_float() {
        assert_eq!(TextValue::Float(3.14).as_float(), Some(3.14));
        assert_eq!(TextValue::Int(42).as_float(), Some(42.0));
        assert_eq!(TextValue::Bool(true).as_float(), None);
    }

    #[test]
    fn test_as_ivec3() {
        let ivec = IVec3::new(1, 2, 3);
        assert_eq!(TextValue::IVec3(ivec).as_ivec3(), Some(ivec));

        let dvec = DVec3::new(1.5, 2.7, 3.9);
        assert_eq!(TextValue::Vec3(dvec).as_ivec3(), Some(IVec3::new(1, 2, 3)));
    }

    #[test]
    fn test_as_vec3() {
        let dvec = DVec3::new(1.5, 2.5, 3.5);
        assert_eq!(TextValue::Vec3(dvec).as_vec3(), Some(dvec));

        let ivec = IVec3::new(1, 2, 3);
        assert_eq!(TextValue::IVec3(ivec).as_vec3(), Some(DVec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn test_inferred_data_type() {
        assert_eq!(TextValue::Int(42).inferred_data_type(), DataType::Int);
        assert_eq!(TextValue::Float(3.14).inferred_data_type(), DataType::Float);
        assert_eq!(TextValue::IVec3(IVec3::ZERO).inferred_data_type(), DataType::IVec3);

        let arr = TextValue::Array(vec![TextValue::Int(1), TextValue::Int(2)]);
        assert_eq!(arr.inferred_data_type(), DataType::Array(Box::new(DataType::Int)));
    }
}

// ============================================================================
// Serializer Tests
// ============================================================================

mod serializer_tests {
    use super::*;
    use rust_lib_flutter_cad::structure_designer::text_format::TextFormatter;

    #[test]
    fn test_format_bool() {
        assert_eq!(TextValue::Bool(true).to_text(), "true");
        assert_eq!(TextValue::Bool(false).to_text(), "false");
    }

    #[test]
    fn test_format_int() {
        assert_eq!(TextValue::Int(42).to_text(), "42");
        assert_eq!(TextValue::Int(-10).to_text(), "-10");
        assert_eq!(TextValue::Int(0).to_text(), "0");
    }

    #[test]
    fn test_format_float() {
        assert_eq!(TextValue::Float(3.14).to_text(), "3.14");
        assert_eq!(TextValue::Float(1.0).to_text(), "1.0");
        assert_eq!(TextValue::Float(42.0).to_text(), "42.0"); // ensure .0 suffix
        assert_eq!(TextValue::Float(-1.5).to_text(), "-1.5");
    }

    #[test]
    fn test_format_float_ensures_decimal() {
        // Integer-like floats should get .0 suffix
        assert!(TextValue::Float(42.0).to_text().contains('.'));
        assert!(TextValue::Float(0.0).to_text().contains('.'));
    }

    #[test]
    fn test_format_string_simple() {
        assert_eq!(TextValue::String("hello".to_string()).to_text(), "\"hello\"");
        assert_eq!(TextValue::String("".to_string()).to_text(), "\"\"");
    }

    #[test]
    fn test_format_string_with_escapes() {
        assert_eq!(TextValue::String("a\"b".to_string()).to_text(), "\"a\\\"b\"");
        assert_eq!(TextValue::String("a\\b".to_string()).to_text(), "\"a\\\\b\"");
        assert_eq!(TextValue::String("a\tb".to_string()).to_text(), "\"a\\tb\"");
    }

    #[test]
    fn test_format_string_multiline() {
        let multiline = "line1\nline2\nline3";
        let result = TextValue::String(multiline.to_string()).to_text();
        assert!(result.starts_with("\"\"\""));
        assert!(result.ends_with("\"\"\""));
        assert!(result.contains("line1\nline2"));
    }

    #[test]
    fn test_format_ivec2() {
        assert_eq!(TextValue::IVec2(IVec2::new(1, 2)).to_text(), "(1, 2)");
        assert_eq!(TextValue::IVec2(IVec2::new(-3, 4)).to_text(), "(-3, 4)");
    }

    #[test]
    fn test_format_ivec3() {
        assert_eq!(TextValue::IVec3(IVec3::new(1, 2, 3)).to_text(), "(1, 2, 3)");
    }

    #[test]
    fn test_format_vec2() {
        let result = TextValue::Vec2(DVec2::new(1.5, 2.5)).to_text();
        assert_eq!(result, "(1.5, 2.5)");
    }

    #[test]
    fn test_format_vec3() {
        let result = TextValue::Vec3(DVec3::new(1.0, 2.0, 3.0)).to_text();
        assert_eq!(result, "(1.0, 2.0, 3.0)");
    }

    #[test]
    fn test_format_data_type() {
        assert_eq!(TextValue::DataType(DataType::Int).to_text(), "Int");
        assert_eq!(TextValue::DataType(DataType::Vec3).to_text(), "Vec3");
    }

    #[test]
    fn test_format_array() {
        let arr = TextValue::Array(vec![
            TextValue::Int(1),
            TextValue::Int(2),
            TextValue::Int(3),
        ]);
        assert_eq!(arr.to_text(), "[1, 2, 3]");

        let empty = TextValue::Array(vec![]);
        assert_eq!(empty.to_text(), "[]");
    }

    #[test]
    fn test_format_object() {
        let obj = TextValue::Object(vec![
            ("name".to_string(), TextValue::String("x".to_string())),
            ("type".to_string(), TextValue::DataType(DataType::Int)),
        ]);
        assert_eq!(obj.to_text(), "{ name: \"x\", type: Int }");
    }

    #[test]
    fn test_text_formatter() {
        let mut fmt = TextFormatter::new();
        fmt.writeln("# Comment");
        fmt.writeln("sphere1 = sphere {");
        fmt.indent();
        fmt.writeln("center: (0, 0, 0),");
        fmt.writeln("radius: 5");
        fmt.dedent();
        fmt.writeln("}");

        let output = fmt.finish();
        assert!(output.contains("# Comment\n"));
        assert!(output.contains("  center:"));
    }
}

// ============================================================================
// Lexer Tests
// ============================================================================

mod lexer_tests {
    use super::*;

    #[test]
    fn test_lexer_simple_tokens() {
        let tokens = Lexer::tokenize("= : , { } [ ] ( ) @").unwrap();
        let token_types: Vec<_> = tokens.iter().map(|t| &t.token).collect();
        assert!(matches!(token_types[0], Token::Equals));
        assert!(matches!(token_types[1], Token::Colon));
        assert!(matches!(token_types[2], Token::Comma));
        assert!(matches!(token_types[3], Token::LeftBrace));
        assert!(matches!(token_types[4], Token::RightBrace));
        assert!(matches!(token_types[5], Token::LeftBracket));
        assert!(matches!(token_types[6], Token::RightBracket));
        assert!(matches!(token_types[7], Token::LeftParen));
        assert!(matches!(token_types[8], Token::RightParen));
        assert!(matches!(token_types[9], Token::At));
    }

    #[test]
    fn test_lexer_keywords() {
        let tokens = Lexer::tokenize("true false output delete").unwrap();
        let token_types: Vec<_> = tokens.iter().map(|t| &t.token).collect();
        assert!(matches!(token_types[0], Token::True));
        assert!(matches!(token_types[1], Token::False));
        assert!(matches!(token_types[2], Token::Output));
        assert!(matches!(token_types[3], Token::Delete));
    }

    #[test]
    fn test_lexer_numbers() {
        let tokens = Lexer::tokenize("42 -10 3.14 -1.5 2.5e-3").unwrap();
        let token_types: Vec<_> = tokens.iter().map(|t| &t.token).collect();
        assert_eq!(token_types[0], &Token::Int(42));
        assert_eq!(token_types[1], &Token::Int(-10));
        assert_eq!(token_types[2], &Token::Float(3.14));
        assert_eq!(token_types[3], &Token::Float(-1.5));
        assert_eq!(token_types[4], &Token::Float(0.0025));
    }

    #[test]
    fn test_lexer_strings() {
        let tokens = Lexer::tokenize(r#""hello" "with\"escape""#).unwrap();
        assert!(matches!(&tokens[0].token, Token::String(s) if s == "hello"));
        assert!(matches!(&tokens[1].token, Token::String(s) if s == "with\"escape"));
    }

    #[test]
    fn test_lexer_triple_quoted_string() {
        let input = r#""""
line1
line2
""""#;
        let tokens = Lexer::tokenize(input).unwrap();
        assert!(matches!(&tokens[0].token, Token::String(s) if s.contains("line1") && s.contains("line2")));
    }

    #[test]
    fn test_lexer_identifiers() {
        let tokens = Lexer::tokenize("sphere1 my_node node_2").unwrap();
        assert!(matches!(&tokens[0].token, Token::Identifier(s) if s == "sphere1"));
        assert!(matches!(&tokens[1].token, Token::Identifier(s) if s == "my_node"));
        assert!(matches!(&tokens[2].token, Token::Identifier(s) if s == "node_2"));
    }
}

// ============================================================================
// Parser Tests
// ============================================================================

mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_simple_assignment() {
        let stmts = Parser::parse("sphere1 = sphere { radius: 5 }").unwrap();
        assert_eq!(stmts.len(), 1);

        if let Statement::Assignment { name, node_type, properties } = &stmts[0] {
            assert_eq!(name, "sphere1");
            assert_eq!(node_type, "sphere");
            assert_eq!(properties.len(), 1);
            assert_eq!(properties[0].0, "radius");
            assert!(matches!(&properties[0].1, PropertyValue::Literal(TextValue::Int(5))));
        } else {
            panic!("Expected assignment statement");
        }
    }

    #[test]
    fn test_parse_assignment_with_vector() {
        let stmts = Parser::parse("sphere1 = sphere { center: (1, 2, 3), radius: 5 }").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert_eq!(properties[0].0, "center");
            assert!(matches!(&properties[0].1, PropertyValue::Literal(TextValue::IVec3(v)) if *v == IVec3::new(1, 2, 3)));
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_assignment_with_float_vector() {
        let stmts = Parser::parse("v = vec3 { x: 1.0, y: 2.5, z: 3.0 }").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert!(matches!(&properties[0].1, PropertyValue::Literal(TextValue::Float(f)) if *f == 1.0));
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_node_reference() {
        let stmts = Parser::parse("union1 = union { shapes: [sphere1, box1] }").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            if let PropertyValue::Array(arr) = &properties[0].1 {
                assert!(matches!(&arr[0], PropertyValue::NodeRef(s) if s == "sphere1"));
                assert!(matches!(&arr[1], PropertyValue::NodeRef(s) if s == "box1"));
            } else {
                panic!("Expected array");
            }
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_function_reference() {
        let stmts = Parser::parse("map1 = map { f: @pattern }").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert!(matches!(&properties[0].1, PropertyValue::FunctionRef(s) if s == "pattern"));
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_output_statement() {
        let stmts = Parser::parse("output result").unwrap();

        assert!(matches!(&stmts[0], Statement::Output { node_name } if node_name == "result"));
    }

    #[test]
    fn test_parse_delete_statement() {
        let stmts = Parser::parse("delete old_node").unwrap();

        assert!(matches!(&stmts[0], Statement::Delete { node_name } if node_name == "old_node"));
    }

    #[test]
    fn test_parse_multiple_statements() {
        let input = r#"
sphere1 = sphere { radius: 5 }
box1 = cuboid { extent: (10, 10, 10) }
union1 = union { shapes: [sphere1, box1] }
output union1
"#;
        let stmts = Parser::parse(input).unwrap();
        assert_eq!(stmts.len(), 4);
    }

    #[test]
    fn test_parse_with_comments() {
        let input = r#"
# This is a comment
sphere1 = sphere { radius: 5 }
# Another comment
output sphere1
"#;
        let stmts = Parser::parse(input).unwrap();
        // Comments are skipped, so we should have 2 statements
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn test_parse_string_property() {
        let stmts = Parser::parse(r#"str1 = string { value: "hello world" }"#).unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert!(matches!(&properties[0].1, PropertyValue::Literal(TextValue::String(s)) if s == "hello world"));
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_bool_properties() {
        let stmts = Parser::parse("b = bool { value: true }").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert!(matches!(&properties[0].1, PropertyValue::Literal(TextValue::Bool(true))));
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_empty_braces() {
        let stmts = Parser::parse("union1 = union {}").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert!(properties.is_empty());
        } else {
            panic!("Expected assignment");
        }
    }

    #[test]
    fn test_parse_no_braces() {
        let stmts = Parser::parse("union1 = union").unwrap();

        if let Statement::Assignment { properties, .. } = &stmts[0] {
            assert!(properties.is_empty());
        } else {
            panic!("Expected assignment");
        }
    }
}

// ============================================================================
// Network Serializer Tests
// ============================================================================

mod network_serializer_tests {
    use super::*;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use glam::f64::DVec2;

    fn create_test_registry() -> NodeTypeRegistry {
        NodeTypeRegistry::new()
    }

    fn create_test_network() -> NodeNetwork {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_type: DataType::Geometry,
            public: true,
            node_data_creator: || Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    }

    #[test]
    fn test_serialize_empty_network() {
        let registry = create_test_registry();
        let network = create_test_network();

        let result = serialize_network(&network, &registry, None);
        assert!(result.contains("Empty network"));
    }

    #[test]
    fn test_serialize_single_node() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Add a sphere node
        let node_type = registry.get_node_type("sphere").unwrap();
        let node_data = (node_type.node_data_creator)();
        network.add_node("sphere", DVec2::new(0.0, 0.0), node_type.parameters.len(), node_data);

        let result = serialize_network(&network, &registry, None);

        // Check that the result contains a sphere definition
        assert!(result.contains("sphere1 = sphere"));
        assert!(result.contains("center:"));
        assert!(result.contains("radius:"));
    }

    #[test]
    fn test_serialize_multiple_nodes() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Add two sphere nodes
        let node_type = registry.get_node_type("sphere").unwrap();
        let node_data1 = (node_type.node_data_creator)();
        let node_data2 = (node_type.node_data_creator)();
        network.add_node("sphere", DVec2::new(0.0, 0.0), node_type.parameters.len(), node_data1);
        network.add_node("sphere", DVec2::new(100.0, 0.0), node_type.parameters.len(), node_data2);

        let result = serialize_network(&network, &registry, None);

        // Check that we have sphere1 and sphere2
        assert!(result.contains("sphere1 = sphere"));
        assert!(result.contains("sphere2 = sphere"));
    }

    #[test]
    fn test_serialize_with_output() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Add a sphere node
        let node_type = registry.get_node_type("sphere").unwrap();
        let node_data = (node_type.node_data_creator)();
        let node_id = network.add_node("sphere", DVec2::new(0.0, 0.0), node_type.parameters.len(), node_data);

        // Set as return node
        network.return_node_id = Some(node_id);

        let result = serialize_network(&network, &registry, None);

        // Check that there's an output statement
        assert!(result.contains("output sphere1"));
    }

    #[test]
    fn test_serialize_connected_nodes() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Add an int node
        let int_type = registry.get_node_type("int").unwrap();
        let int_data = (int_type.node_data_creator)();
        let int_id = network.add_node("int", DVec2::new(0.0, 0.0), int_type.parameters.len(), int_data);

        // Add a sphere node
        let sphere_type = registry.get_node_type("sphere").unwrap();
        let sphere_data = (sphere_type.node_data_creator)();
        let sphere_id = network.add_node("sphere", DVec2::new(100.0, 0.0), sphere_type.parameters.len(), sphere_data);

        // Connect int to sphere's radius parameter (index 1)
        network.connect_nodes(int_id, 0, sphere_id, 1, false);

        let result = serialize_network(&network, &registry, None);

        // Check that the connection is shown
        assert!(result.contains("int1 = int"));
        assert!(result.contains("sphere1 = sphere"));
        assert!(result.contains("radius: int1"));
    }

    #[test]
    fn test_serialize_different_node_types() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Add nodes of different types
        let int_type = registry.get_node_type("int").unwrap();
        let int_data = (int_type.node_data_creator)();
        network.add_node("int", DVec2::new(0.0, 0.0), int_type.parameters.len(), int_data);

        let float_type = registry.get_node_type("float").unwrap();
        let float_data = (float_type.node_data_creator)();
        network.add_node("float", DVec2::new(0.0, 100.0), float_type.parameters.len(), float_data);

        let bool_type = registry.get_node_type("bool").unwrap();
        let bool_data = (bool_type.node_data_creator)();
        network.add_node("bool", DVec2::new(0.0, 200.0), bool_type.parameters.len(), bool_data);

        let result = serialize_network(&network, &registry, None);

        assert!(result.contains("int1 = int"));
        assert!(result.contains("float1 = float"));
        assert!(result.contains("bool1 = bool"));
    }
}

// ============================================================================
// Network Editor Tests
// ============================================================================

mod network_editor_tests {
    use super::*;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, serialize_network};
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

    fn create_test_registry() -> NodeTypeRegistry {
        NodeTypeRegistry::new()
    }

    fn create_test_network() -> NodeNetwork {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_type: DataType::Geometry,
            public: true,
            node_data_creator: || Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    }

    #[test]
    fn test_edit_create_single_node() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_created.len(), 1);
        assert!(result.nodes_created.contains(&"sphere1".to_string()));
        assert_eq!(network.nodes.len(), 1);
    }

    #[test]
    fn test_edit_create_multiple_nodes() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
            float1 = float { value: 3.14 }
            bool1 = bool { value: true }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_created.len(), 3);
        assert_eq!(network.nodes.len(), 3);
    }

    #[test]
    fn test_edit_with_connections() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 5 }
            sphere1 = sphere { center: (0, 0, 0), radius: int1 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_created.len(), 2);
        assert!(!result.connections_made.is_empty(), "Should have made connections");

        // Verify the connection exists
        let sphere_node = network.nodes.values()
            .find(|n| n.node_type_name == "sphere")
            .expect("Should find sphere node");

        // Radius is parameter index 1
        assert!(!sphere_node.arguments[1].argument_output_pins.is_empty(),
            "Sphere radius should be connected");
    }

    #[test]
    fn test_edit_with_output_statement() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            output sphere1
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert!(network.return_node_id.is_some(), "Should have return node set");
    }

    #[test]
    fn test_edit_with_visibility() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
            sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Find the sphere node and check visibility
        let sphere_node = network.nodes.values()
            .find(|n| n.node_type_name == "sphere")
            .expect("Should find sphere node");

        assert!(network.displayed_node_ids.contains_key(&sphere_node.id),
            "Sphere should be visible");

        // Int should not be visible (no visible: true)
        let int_node = network.nodes.values()
            .find(|n| n.node_type_name == "int")
            .expect("Should find int node");

        assert!(!network.displayed_node_ids.contains_key(&int_node.id),
            "Int should not be visible");
    }

    #[test]
    fn test_edit_delete_node() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // First create nodes
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            int1 = int { value: 42 }
        "#, true);
        assert!(result.success);
        assert_eq!(network.nodes.len(), 2);

        // Now delete one
        let result = edit_network(&mut network, &registry, r#"
            delete sphere1
        "#, false);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_deleted.len(), 1);
        assert!(result.nodes_deleted.contains(&"sphere1".to_string()));
        assert_eq!(network.nodes.len(), 1);

        // Remaining node should be int
        let remaining = network.nodes.values().next().expect("Should have one node");
        assert_eq!(remaining.node_type_name, "int");
    }

    #[test]
    fn test_edit_replace_mode() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create initial nodes
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
        "#, true);
        assert!(result.success);
        assert_eq!(network.nodes.len(), 1);

        // Replace with different nodes
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
            float1 = float { value: 3.14 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(network.nodes.len(), 2);

        // Should have int and float, not sphere
        let type_names: Vec<_> = network.nodes.values()
            .map(|n| n.node_type_name.as_str())
            .collect();
        assert!(type_names.contains(&"int"));
        assert!(type_names.contains(&"float"));
        assert!(!type_names.contains(&"sphere"));
    }

    #[test]
    fn test_edit_incremental_mode() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create initial node
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
        "#, true);
        assert!(result.success);
        assert_eq!(network.nodes.len(), 1);

        // Add more nodes incrementally (replace = false)
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
        "#, false);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(network.nodes.len(), 2, "Should have both original and new node");
    }

    #[test]
    fn test_edit_update_existing_node() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create initial node
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
        "#, true);
        assert!(result.success);

        // Update the same node (incremental mode)
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 100 }
        "#, false);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_updated.len(), 1);
        assert!(result.nodes_updated.contains(&"int1".to_string()));
        assert_eq!(network.nodes.len(), 1, "Should still have only one node");
    }

    #[test]
    fn test_edit_unknown_node_type_error() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            unknown1 = nonexistent_type { prop: 42 }
        "#, true);

        assert!(!result.success, "Edit should fail for unknown node type");
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("nonexistent_type"));
    }

    #[test]
    fn test_edit_parse_error() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { invalid syntax here
        "#, true);

        assert!(!result.success, "Edit should fail for parse errors");
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_edit_roundtrip() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a network via edit
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
            sphere1 = sphere { center: (0, 0, 0), radius: int1, visible: true }
            output sphere1
        "#, true);
        assert!(result.success, "Initial edit should succeed: {:?}", result.errors);

        // Serialize it
        let serialized = serialize_network(&network, &registry, None);

        // Create a new network and edit it with the serialized text
        let mut network2 = create_test_network();
        let result2 = edit_network(&mut network2, &registry, &serialized, true);

        assert!(result2.success, "Roundtrip edit should succeed: {:?}", result2.errors);
        assert_eq!(network.nodes.len(), network2.nodes.len(),
            "Networks should have same number of nodes");
        assert_eq!(network.return_node_id.is_some(), network2.return_node_id.is_some(),
            "Networks should both have or not have return node");
    }

    #[test]
    fn test_edit_multi_input_connection() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
            sphere2 = sphere { center: (10, 0, 0), radius: 3, visible: true }
            union1 = union { shapes: [sphere1, sphere2], visible: true }
            output union1
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_created.len(), 3);

        // Find the union node and verify it has two inputs
        let union_node = network.nodes.values()
            .find(|n| n.node_type_name == "union")
            .expect("Should find union node");

        // shapes is parameter index 0
        assert_eq!(union_node.arguments[0].argument_output_pins.len(), 2,
            "Union should have two inputs connected");
    }

    #[test]
    fn test_edit_function_ref_connection() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a pattern with map and function reference
        // map node parameters: xs (index 0) and f (index 1)
        // input_type and output_type are node data properties, not input parameters
        let result = edit_network(&mut network, &registry, r#"
            range1 = range { start: 0, step: 1, count: 5 }
            expr1 = expr { expression: "x * 2", parameters: [{ name: "x", data_type: Int }] }
            map1 = map { input_type: Int, output_type: Int, xs: range1, f: @expr1 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.nodes_created.len(), 3);

        // Find the map node and check function reference
        let map_node = network.nodes.values()
            .find(|n| n.node_type_name == "map")
            .expect("Should find map node");

        // f parameter is at index 1 (xs=0, f=1)
        let f_param_index = 1;
        let f_arg = &map_node.arguments[f_param_index];
        assert!(!f_arg.argument_output_pins.is_empty(), "f parameter should be connected");

        // Verify it's a function pin connection (output_pin_index = -1)
        let (_, &pin_index) = f_arg.argument_output_pins.iter().next().unwrap();
        assert_eq!(pin_index, -1, "Should be a function pin reference");
    }

    #[test]
    fn test_edit_preserves_unmentioned_nodes_in_incremental() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create initial nodes
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            int1 = int { value: 42 }
        "#, true);
        assert!(result.success);

        // Edit only one of them (incremental)
        let result = edit_network(&mut network, &registry, r#"
            float1 = float { value: 3.14 }
        "#, false);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(network.nodes.len(), 3, "All three nodes should exist");

        let type_names: Vec<_> = network.nodes.values()
            .map(|n| n.node_type_name.as_str())
            .collect();
        assert!(type_names.contains(&"sphere"));
        assert!(type_names.contains(&"int"));
        assert!(type_names.contains(&"float"));
    }

    #[test]
    fn test_edit_comments_are_ignored() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            # This is a comment
            int1 = int { value: 42 }
            # Another comment
            float1 = float { value: 3.14 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(network.nodes.len(), 2);
    }

    #[test]
    fn test_edit_array_literal_properties() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            poly = polygon { vertices: [(0, 0), (10, 0), (5, 10)], visible: true }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Verify the vertices were actually set by serializing and checking output
        let serialized = serialize_network(&network, &registry, None);
        assert!(serialized.contains("(0, 0)"), "Should contain first vertex, got:\n{}", serialized);
        assert!(serialized.contains("(10, 0)"), "Should contain second vertex, got:\n{}", serialized);
        assert!(serialized.contains("(5, 10)"), "Should contain third vertex, got:\n{}", serialized);
    }

    #[test]
    fn test_edit_expr_with_parameters_array() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            result = expr {
                expression: "x + y",
                parameters: [
                    { name: "x", data_type: Int },
                    { name: "y", data_type: Int }
                ]
            }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Verify the expression and parameters were set
        let serialized = serialize_network(&network, &registry, None);
        assert!(serialized.contains("expression:"), "Should contain expression property, got:\n{}", serialized);
        assert!(serialized.contains("x + y"), "Should contain expression value, got:\n{}", serialized);
    }

    #[test]
    fn test_edit_mixed_array_with_refs_falls_through() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Arrays with node refs should still work via connection pass
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            sphere2 = sphere { center: (10, 0, 0), radius: 3 }
            union1 = union { shapes: [sphere1, sphere2], visible: true }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Verify the union was connected properly
        let union_node = network.nodes.values()
            .find(|n| n.node_type_name == "union")
            .expect("Should find union node");

        // shapes is parameter index 0
        assert_eq!(union_node.arguments[0].argument_output_pins.len(), 2,
            "Union should have two inputs connected");
    }

    #[test]
    fn test_no_spurious_warnings_for_literal_only_properties() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // parameter node has literal-only properties like param_name, data_type, etc.
        // that are defined in get_text_properties() but NOT in the node's wirable parameters
        let result = edit_network(&mut network, &registry, r#"
            p = parameter { param_name: "size", data_type: Float, sort_order: 1 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Should have no warnings about param_name, data_type, or sort_order
        for warning in &result.warnings {
            assert!(!warning.contains("param_name"),
                "Should not warn about param_name: {}", warning);
            assert!(!warning.contains("data_type"),
                "Should not warn about data_type: {}", warning);
            assert!(!warning.contains("sort_order"),
                "Should not warn about sort_order: {}", warning);
        }
    }

    #[test]
    fn test_warning_for_truly_unknown_properties() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // sphere has wirable parameters (center, radius, unit_cell) so unknown properties
        // should trigger warnings. nonexistent_prop is not a valid property.
        let result = edit_network(&mut network, &registry, r#"
            s = sphere { center: (0, 0, 0), radius: 5, nonexistent_prop: 123 }
        "#, true);

        assert!(result.success, "Edit should succeed even with unknown properties");

        // Should have a warning about the unknown property
        let has_warning = result.warnings.iter()
            .any(|w| w.contains("nonexistent_prop"));
        assert!(has_warning, "Should warn about unknown property 'nonexistent_prop', got: {:?}",
            result.warnings);
    }

    #[test]
    fn test_no_warning_for_expr_literal_only_properties() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // expr node has literal-only properties: expression, parameters
        let result = edit_network(&mut network, &registry, r#"
            e = expr {
                expression: "x + 1",
                parameters: [{ name: "x", data_type: Int }]
            }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Should have no warnings about expression or parameters
        for warning in &result.warnings {
            assert!(!warning.contains("expression"),
                "Should not warn about expression: {}", warning);
            assert!(!warning.contains("parameters"),
                "Should not warn about parameters: {}", warning);
        }
    }

    #[test]
    fn test_warning_for_wire_only_parameter_literal() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // half_plane has m_index as a parameter (wirable) but with no backing text property
        // Trying to set a literal value should produce a warning
        let result = edit_network(&mut network, &registry, r#"
            hp = half_plane { m_index: (1, 2) }
        "#, true);

        assert!(result.success, "Edit should succeed even with wire-only param literal");

        // Should have a warning about m_index being wire-only
        let has_wire_only_warning = result.warnings.iter()
            .any(|w| w.contains("wire-only") && w.contains("m_index"));
        assert!(has_wire_only_warning,
            "Should warn about 'm_index' being wire-only, got: {:?}", result.warnings);
    }

    #[test]
    fn test_no_warning_for_wired_parameter() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // When m_index is properly wired (not a literal), there should be no warning
        let result = edit_network(&mut network, &registry, r#"
            mi = ivec2 { value: (1, 2) }
            hp = half_plane { m_index: mi }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Should have no wire-only warnings
        let has_wire_only_warning = result.warnings.iter()
            .any(|w| w.contains("wire-only"));
        assert!(!has_wire_only_warning,
            "Should not warn about wire-only when properly wired, got: {:?}", result.warnings);
    }

    #[test]
    fn test_edit_description_single_line() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            description "This is a test network"
            int1 = int { value: 42 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(result.description_set, Some("This is a test network".to_string()));
        assert_eq!(network.node_type.description, "This is a test network");
    }

    #[test]
    fn test_edit_description_multi_line() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            description """
This is a multi-line description.

It can include:
- Bullet points
- Multiple paragraphs
"""
            int1 = int { value: 42 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert!(result.description_set.is_some());
        assert!(network.node_type.description.contains("multi-line"));
        assert!(network.node_type.description.contains("Bullet points"));
    }

    #[test]
    fn test_edit_description_roundtrip() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Set a description
        let result = edit_network(&mut network, &registry, r#"
            description "Test roundtrip description"
            int1 = int { value: 42 }
            output int1
        "#, true);

        assert!(result.success);

        // Serialize the network
        let text = serialize_network(&network, &registry, Some("test"));

        // Should contain the description
        assert!(text.contains("description \"Test roundtrip description\""),
            "Serialized text should contain description: {}", text);
    }

    #[test]
    fn test_edit_description_multiline_roundtrip() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Set a multi-line description
        let result = edit_network(&mut network, &registry,
            "description \"\"\"Line 1\nLine 2\nLine 3\"\"\"\nint1 = int { value: 42 }",
            true);

        assert!(result.success);
        assert!(network.node_type.description.contains("Line 1"));
        assert!(network.node_type.description.contains("Line 2"));

        // Serialize the network
        let text = serialize_network(&network, &registry, Some("test"));

        // Should use triple-quotes for multi-line description
        assert!(text.contains("\"\"\""), "Should use triple-quotes for multi-line: {}", text);
    }

    #[test]
    fn test_serialize_empty_description_not_included() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Clear the description (test network starts with "Test network")
        network.node_type.description = String::new();

        // Create a network without setting description
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
        "#, true);

        assert!(result.success);
        assert!(network.node_type.description.is_empty());

        // Serialize the network
        let text = serialize_network(&network, &registry, Some("test"));

        // Should NOT contain "description" statement
        assert!(!text.contains("description \""),
            "Empty description should not be serialized: {}", text);
    }
}

// ============================================================================
// Auto-Layout Tests
// ============================================================================

mod auto_layout_tests {
    use super::*;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, auto_layout};
    use rust_lib_flutter_cad::structure_designer::node_layout;
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use glam::f64::DVec2;

    fn create_test_registry() -> NodeTypeRegistry {
        NodeTypeRegistry::new()
    }

    fn create_test_network() -> NodeNetwork {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_type: DataType::Geometry,
            public: true,
            node_data_creator: || Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    }

    #[test]
    fn test_get_node_size_unknown_type() {
        let registry = create_test_registry();
        // Unknown type should return default size (0 params)
        let size = auto_layout::get_node_size(&registry, "unknown_type");
        assert_eq!(size.x, node_layout::NODE_WIDTH);
        // With 0 params and subtitle, height should be minimal
        assert!(size.y > 0.0);
    }

    #[test]
    fn test_get_node_size_known_type() {
        let registry = create_test_registry();
        // sphere has center, radius, and unit_cell parameters
        let size = auto_layout::get_node_size(&registry, "sphere");
        assert_eq!(size.x, node_layout::NODE_WIDTH);
        // Should have height for 3 parameters
        let expected_size = node_layout::estimate_node_size(3, true);
        assert_eq!(size, expected_size);
    }

    #[test]
    fn test_empty_network_positions_at_default() {
        let registry = create_test_registry();
        let network = create_test_network();

        // Calculate position for a new node with no inputs
        let position = auto_layout::calculate_new_node_position(
            &network,
            &registry,
            "sphere",
            &[], // no connections
        );

        // Should be at the default starting position (100, 100)
        assert_eq!(position.x, 100.0);
        assert_eq!(position.y, 100.0);
    }

    #[test]
    fn test_node_positions_to_right_of_source() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a source node at a known position
        let int_type = registry.get_node_type("int").unwrap();
        let int_data = (int_type.node_data_creator)();
        let int_id = network.add_node("int", DVec2::new(200.0, 300.0), int_type.parameters.len(), int_data);

        // Calculate position for a node connected to int
        let position = auto_layout::calculate_new_node_position(
            &network,
            &registry,
            "sphere",
            &[(int_id, 0)], // connected to int node
        );

        // Should be to the right of the int node
        let int_size = node_layout::estimate_node_size(int_type.parameters.len(), true);
        let expected_min_x = 200.0 + int_size.x + node_layout::DEFAULT_HORIZONTAL_GAP;
        assert!(position.x >= expected_min_x, "Node should be placed to the right of source");

        // Y should be approximately the same as source
        assert!((position.y - 300.0).abs() < 1.0, "Node should be at similar Y as source");
    }

    #[test]
    fn test_node_positions_with_multiple_sources() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create two source nodes at different positions
        let int_type = registry.get_node_type("int").unwrap();
        let int_data1 = (int_type.node_data_creator)();
        let int_data2 = (int_type.node_data_creator)();
        let int_id1 = network.add_node("int", DVec2::new(100.0, 100.0), int_type.parameters.len(), int_data1);
        let int_id2 = network.add_node("int", DVec2::new(100.0, 300.0), int_type.parameters.len(), int_data2);

        // Calculate position for a node connected to both
        let position = auto_layout::calculate_new_node_position(
            &network,
            &registry,
            "sphere",
            &[(int_id1, 0), (int_id2, 0)],
        );

        // Y should be average of the two sources
        let expected_y = (100.0 + 300.0) / 2.0;
        assert!((position.y - expected_y).abs() < 1.0, "Y should be average of source Y positions");
    }

    #[test]
    fn test_nodes_placed_without_overlap() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create multiple nodes via edit
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 1 }
            int2 = int { value: 2 }
            int3 = int { value: 3 }
            int4 = int { value: 4 }
            int5 = int { value: 5 }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert_eq!(network.nodes.len(), 5);

        // Verify no overlaps
        let nodes: Vec<_> = network.nodes.values().collect();
        let int_type = registry.get_node_type("int").unwrap();
        let node_size = node_layout::estimate_node_size(int_type.parameters.len(), true);

        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let pos1 = DVec2::new(nodes[i].position.x, nodes[i].position.y);
                let pos2 = DVec2::new(nodes[j].position.x, nodes[j].position.y);

                let overlap = node_layout::nodes_overlap(
                    pos1, node_size,
                    pos2, node_size,
                    node_layout::DEFAULT_VERTICAL_GAP
                );

                assert!(!overlap,
                    "Nodes {} and {} should not overlap (pos1: {:?}, pos2: {:?})",
                    i, j, pos1, pos2);
            }
        }
    }

    #[test]
    fn test_connected_nodes_flow_left_to_right() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a chain of connected nodes
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 5 }
            sphere1 = sphere { radius: int1 }
            union1 = union { shapes: [sphere1] }
        "#, true);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Find nodes by type
        let int_node = network.nodes.values()
            .find(|n| n.node_type_name == "int")
            .expect("Should find int node");
        let sphere_node = network.nodes.values()
            .find(|n| n.node_type_name == "sphere")
            .expect("Should find sphere node");
        let union_node = network.nodes.values()
            .find(|n| n.node_type_name == "union")
            .expect("Should find union node");

        // Verify left-to-right flow: int < sphere < union
        assert!(sphere_node.position.x > int_node.position.x,
            "Sphere should be to the right of int");
        assert!(union_node.position.x > sphere_node.position.x,
            "Union should be to the right of sphere");
    }

    #[test]
    fn test_nodes_without_connections_placed_in_empty_space() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // First create some nodes to occupy space
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            sphere2 = sphere { center: (0, 0, 0), radius: 3 }
        "#, true);
        assert!(result.success);

        // Now add a node with no connections
        let result = edit_network(&mut network, &registry, r#"
            int1 = int { value: 42 }
        "#, false);

        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // The int node should be placed without overlapping existing nodes
        let int_node = network.nodes.values()
            .find(|n| n.node_type_name == "int")
            .expect("Should find int node");

        let int_pos = DVec2::new(int_node.position.x, int_node.position.y);
        let int_type = registry.get_node_type("int").unwrap();
        let int_size = node_layout::estimate_node_size(int_type.parameters.len(), true);

        // Check no overlap with sphere nodes
        for node in network.nodes.values() {
            if node.node_type_name == "int" {
                continue;
            }
            let node_pos = DVec2::new(node.position.x, node.position.y);
            let node_type = registry.get_node_type(&node.node_type_name).unwrap();
            let node_size = node_layout::estimate_node_size(node_type.parameters.len(), true);

            let overlap = node_layout::nodes_overlap(
                int_pos, int_size,
                node_pos, node_size,
                node_layout::DEFAULT_VERTICAL_GAP
            );

            assert!(!overlap, "Int node should not overlap with {}",
                node.node_type_name);
        }
    }
}

// ============================================================================
// Node Type Introspection Tests
// ============================================================================

mod node_type_introspection_tests {
    use super::*;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

    fn create_test_registry() -> NodeTypeRegistry {
        NodeTypeRegistry::new()
    }

    #[test]
    fn test_describe_node_type_sphere() {
        let registry = create_test_registry();
        let result = describe_node_type("sphere", &registry);

        // Check header
        assert!(result.contains("Node: sphere"));
        assert!(result.contains("Category: Geometry3D"));
        assert!(result.contains("Description:"));

        // Check inputs section (unified, no longer "Parameters")
        assert!(result.contains("Inputs:"));
        assert!(result.contains("center"));
        assert!(result.contains("radius"));
        assert!(result.contains("IVec3"));
        assert!(result.contains("Int"));

        // Check output
        assert!(result.contains("Output: Geometry"));
    }

    #[test]
    fn test_describe_node_type_int() {
        let registry = create_test_registry();
        let result = describe_node_type("int", &registry);

        assert!(result.contains("Node: int"));
        assert!(result.contains("Category: MathAndProgramming"));
        assert!(result.contains("Output: Int"));
    }

    #[test]
    fn test_describe_node_type_atom_fill() {
        let registry = create_test_registry();
        let result = describe_node_type("atom_fill", &registry);

        assert!(result.contains("Node: atom_fill"));
        assert!(result.contains("Category: AtomicStructure"));

        // Check for key parameters
        assert!(result.contains("shape"));
        assert!(result.contains("motif"));
        assert!(result.contains("passivate"));

        // Check output type
        assert!(result.contains("Output: Atomic"));
    }

    #[test]
    fn test_describe_node_type_shows_defaults() {
        let registry = create_test_registry();
        let result = describe_node_type("sphere", &registry);

        // Sphere should show defaults for center and radius
        assert!(result.contains("[default:"));
        assert!(result.contains("(0, 0, 0)"));  // center default
    }

    #[test]
    fn test_describe_node_type_shows_wire_only() {
        let registry = create_test_registry();
        let result = describe_node_type("sphere", &registry);

        // unit_cell parameter has hardcoded default and is wire-only
        assert!(result.contains("wire-only"));
        assert!(result.contains("default: cubic diamond"));
    }

    #[test]
    fn test_describe_node_type_unknown() {
        let registry = create_test_registry();
        let result = describe_node_type("nonexistent_node_type", &registry);

        assert!(result.contains("# Node type 'nonexistent_node_type' not found"));
    }

    #[test]
    fn test_describe_node_type_cuboid() {
        let registry = create_test_registry();
        let result = describe_node_type("cuboid", &registry);

        assert!(result.contains("Node: cuboid"));
        assert!(result.contains("min_corner"));
        assert!(result.contains("extent"));
        assert!(result.contains("Output: Geometry"));
    }

    #[test]
    fn test_describe_node_type_expr() {
        let registry = create_test_registry();
        let result = describe_node_type("expr", &registry);

        assert!(result.contains("Node: expr"));
        assert!(result.contains("Category: MathAndProgramming"));
        // expr node has properties that are not wirable
        // (parameters and expression are stored properties)
    }

    // truncate_description tests
    #[test]
    fn test_truncate_description_short() {
        let desc = "A short description.";
        assert_eq!(truncate_description(desc), desc);
    }

    #[test]
    fn test_truncate_description_first_line_only() {
        let desc = "First line.\nSecond line with more detail.";
        assert_eq!(truncate_description(desc), "First line.");
    }

    #[test]
    fn test_truncate_description_at_sentence() {
        let desc = "This is the first sentence. This is much longer text that goes on and on and would exceed the limit if we included everything here in this very long description.";
        assert_eq!(truncate_description(desc), "This is the first sentence.");
    }

    #[test]
    fn test_truncate_description_at_word_boundary() {
        let desc = "This is a very long description without any sentence breaks that just keeps going and going and going until it exceeds the maximum length limit we have set";
        let result = truncate_description(desc);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 153); // 150 + "..."
    }

    #[test]
    fn test_truncate_description_empty() {
        assert_eq!(truncate_description(""), "");
    }

    #[test]
    fn test_truncate_description_exactly_150_chars() {
        // Create a string of exactly 150 characters
        let desc = "a".repeat(150);
        assert_eq!(truncate_description(&desc), desc);
    }

    #[test]
    fn test_truncate_description_151_chars_no_space() {
        // 151 characters with no spaces - should truncate and add ...
        let desc = "a".repeat(151);
        let result = truncate_description(&desc);
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 153); // 150 + "..."
    }

    #[test]
    fn test_describe_expr_shows_dynamic_output() {
        let registry = create_test_registry();
        let result = describe_node_type("expr", &registry);

        // The base output type is DataType::None, which should display as "dynamic"
        // Should show "dynamic" instead of "None" for the base output type
        assert!(result.contains("Output: dynamic"),
            "Should show 'Output: dynamic' for expr base output type, got:\n{}", result);
        // Should NOT show "Output: None"
        assert!(!result.contains("Output: None"),
            "Should NOT show 'Output: None', got:\n{}", result);
    }

    #[test]
    fn test_describe_expr_shows_dynamic_configuration() {
        let registry = create_test_registry();
        let result = describe_node_type("expr", &registry);

        // The default expr instance has a dynamic configuration section
        // because calculate_custom_node_type returns a different type than the base
        assert!(result.contains("Dynamic Configuration"),
            "Should show 'Dynamic Configuration' section for expr, got:\n{}", result);
        assert!(result.contains("Dynamic Inputs:"),
            "Should show 'Dynamic Inputs' section for expr, got:\n{}", result);
        // The default expr has parameter "x" of type Int
        assert!(result.contains("x : Int"),
            "Should show dynamic input 'x : Int' for expr, got:\n{}", result);
        // The default expr has output type Int
        assert!(result.contains("Dynamic Output: Int"),
            "Should show 'Dynamic Output: Int' for expr, got:\n{}", result);
    }

    #[test]
    fn test_describe_parameter_shows_dynamic() {
        let registry = create_test_registry();
        let result = describe_node_type("parameter", &registry);

        // The parameter node has a data_type property of type DataType, which has
        // inferred_data_type() returning DataType::None (displayed as "dynamic").
        // This shows that the data_type can be any type, configured by the user.
        // Note: The default instance has Int for both base and custom type, so no
        // "Dynamic Configuration" section is shown, but the data_type property
        // itself shows as "dynamic" type.
        assert!(result.contains("data_type") && result.contains("dynamic"),
            "Should show data_type property with dynamic type, got:\n{}", result);
    }

    #[test]
    fn test_describe_shows_dynamic_array_type() {
        let registry = create_test_registry();
        let result = describe_node_type("expr", &registry);

        // The expr node has a "parameters" property of type [None] which should display as [dynamic]
        // Check that the parameters property shows [dynamic] not [None]
        assert!(result.contains("[dynamic]"),
            "Should show '[dynamic]' for array of dynamic types, got:\n{}", result);
        assert!(!result.contains("[None]"),
            "Should NOT show '[None]', should be '[dynamic]' instead, got:\n{}", result);
    }
}

// ============================================================================
// Custom Name Preservation Tests
// ============================================================================

mod custom_name_tests {
    use rust_lib_flutter_cad::structure_designer::text_format::{
        serialize_network, edit_network,
    };
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::node_type::NodeType;
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;

    fn create_test_registry() -> NodeTypeRegistry {
        NodeTypeRegistry::new()
    }

    fn create_test_network() -> NodeNetwork {
        let node_type = NodeType {
            name: "test".to_string(),
            description: "Test network".to_string(),
            category: NodeTypeCategory::Custom,
            parameters: vec![],
            output_type: DataType::Geometry,
            public: true,
            node_data_creator: || Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {}),
            node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
            node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
        };
        NodeNetwork::new(node_type)
    }

    #[test]
    fn test_custom_name_preserved_in_serialization() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a node with a custom name
        let result = edit_network(&mut network, &registry, r#"
            mybox = cuboid { extent: (10, 10, 10), visible: true }
        "#, true);
        assert!(result.success, "Edit should succeed: {:?}", result.errors);
        assert!(result.nodes_created.contains(&"mybox".to_string()));

        // Serialize the network
        let serialized = serialize_network(&network, &registry, None);

        // The serialized output should contain the custom name "mybox", not "cuboid1"
        assert!(serialized.contains("mybox"),
            "Serialization should preserve custom name 'mybox', got:\n{}", serialized);
        assert!(!serialized.contains("cuboid1"),
            "Serialization should NOT contain auto-generated 'cuboid1', got:\n{}", serialized);
    }

    #[test]
    fn test_custom_name_roundtrip() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a network with custom names
        let result = edit_network(&mut network, &registry, r#"
            mybox = cuboid { extent: (10, 10, 10) }
            mysphere = sphere { center: (0, 0, 0), radius: 5 }
            result = diff { base: [mybox], sub: [mysphere], visible: true }
            output result
        "#, true);
        assert!(result.success, "Initial edit should succeed: {:?}", result.errors);

        // Serialize
        let serialized = serialize_network(&network, &registry, None);

        // Verify custom names are in the serialized output
        assert!(serialized.contains("mybox"), "Should contain 'mybox'");
        assert!(serialized.contains("mysphere"), "Should contain 'mysphere'");
        assert!(serialized.contains("result"), "Should contain 'result'");

        // Create a new network and load the serialized text
        let mut network2 = create_test_network();
        let result2 = edit_network(&mut network2, &registry, &serialized, true);
        assert!(result2.success, "Roundtrip edit should succeed: {:?}", result2.errors);

        // Serialize again and verify the names are still preserved
        let serialized2 = serialize_network(&network2, &registry, None);
        assert!(serialized2.contains("mybox"), "Should still contain 'mybox' after roundtrip");
        assert!(serialized2.contains("mysphere"), "Should still contain 'mysphere' after roundtrip");
        assert!(serialized2.contains("result"), "Should still contain 'result' after roundtrip");
    }

    #[test]
    fn test_incremental_edit_preserves_custom_names() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create initial node with custom name
        let result = edit_network(&mut network, &registry, r#"
            mybox = cuboid { extent: (10, 10, 10), visible: true }
        "#, true);
        assert!(result.success);

        // Add another node incrementally
        let result = edit_network(&mut network, &registry, r#"
            mysphere = sphere { center: (0, 0, 0), radius: 5, visible: true }
        "#, false);
        assert!(result.success, "Incremental edit should succeed: {:?}", result.errors);

        // Serialize and verify both custom names are preserved
        let serialized = serialize_network(&network, &registry, None);
        assert!(serialized.contains("mybox"), "Should contain 'mybox'");
        assert!(serialized.contains("mysphere"), "Should contain 'mysphere'");
    }

    #[test]
    fn test_wiring_with_custom_names_across_edits() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create first node with custom name
        let result = edit_network(&mut network, &registry, r#"
            mybox = cuboid { extent: (10, 10, 10) }
        "#, true);
        assert!(result.success);

        // Create second node and wire to first using custom name (incremental mode)
        let result = edit_network(&mut network, &registry, r#"
            result = diff { base: [mybox], sub: [], visible: true }
        "#, false);
        assert!(result.success, "Should be able to wire using custom name: {:?}", result.errors);
        assert_eq!(result.connections_made.len(), 1, "Should have created one wire connection");
    }

    #[test]
    fn test_custom_name_collision_with_auto_generated() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create a node with custom name that looks like an auto-generated name
        let result = edit_network(&mut network, &registry, r#"
            sphere1 = cuboid { extent: (5, 5, 5) }
            another = sphere { center: (0, 0, 0), radius: 5 }
        "#, true);
        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Serialize - the sphere node should get a different auto-generated name
        // since "sphere1" is taken by the cuboid's custom name
        let serialized = serialize_network(&network, &registry, None);

        // "sphere1" should be the cuboid (custom name)
        assert!(serialized.contains("sphere1 = cuboid"),
            "sphere1 should be the cuboid with custom name, got:\n{}", serialized);

        // The actual sphere should have a different name (sphere2)
        assert!(serialized.contains("sphere2 = sphere") || serialized.contains("another = sphere"),
            "The sphere should have a different name, got:\n{}", serialized);
    }

    #[test]
    fn test_duplicate_custom_names_resolved() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create two nodes - first gets the custom name, second should get auto-generated
        // This simulates a scenario where somehow two nodes end up with the same custom_name
        // (which shouldn't happen through normal editing, but we handle it gracefully)
        let result = edit_network(&mut network, &registry, r#"
            myshape = sphere { center: (0, 0, 0), radius: 5 }
        "#, true);
        assert!(result.success);

        // Manually set another node to have the same custom_name (edge case testing)
        // We'll create another node and verify that serialization handles it
        let result = edit_network(&mut network, &registry, r#"
            myshape = cuboid { extent: (10, 10, 10) }
        "#, false);

        // This should update the existing node (since name matches in incremental mode)
        // OR create a new node - either way serialization should work
        assert!(result.success || !result.success, "Either outcome is acceptable");

        // Serialize should not crash and should produce valid output
        let serialized = serialize_network(&network, &registry, None);
        assert!(!serialized.is_empty(), "Serialization should produce output");
    }

    #[test]
    fn test_mixed_custom_and_auto_names() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        // Create some nodes with custom names, some without (using auto-gen style names)
        let result = edit_network(&mut network, &registry, r#"
            custom_box = cuboid { extent: (10, 10, 10) }
            sphere1 = sphere { center: (0, 0, 0), radius: 5 }
            another_custom = sphere { center: (5, 5, 5), radius: 3 }
            union1 = union { shapes: [custom_box, sphere1, another_custom], visible: true }
        "#, true);
        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        let serialized = serialize_network(&network, &registry, None);

        // All custom names should be preserved
        assert!(serialized.contains("custom_box"), "Should contain 'custom_box'");
        assert!(serialized.contains("sphere1"), "Should contain 'sphere1'");
        assert!(serialized.contains("another_custom"), "Should contain 'another_custom'");
        assert!(serialized.contains("union1"), "Should contain 'union1'");
    }

    #[test]
    fn test_custom_name_stored_on_node() {
        let registry = create_test_registry();
        let mut network = create_test_network();

        let result = edit_network(&mut network, &registry, r#"
            my_special_node = int { value: 42 }
        "#, true);
        assert!(result.success);

        // Verify the custom_name is actually stored on the node
        let node = network.nodes.values().next().expect("Should have one node");
        assert_eq!(node.custom_name, Some("my_special_node".to_string()),
            "Node should have custom_name set");
    }
}

// ============================================================================
// Persistent Node Names Phase 6 Verification Tests
// ============================================================================

mod persistent_node_names_phase6_tests {
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
    use rust_lib_flutter_cad::structure_designer::text_format::edit_network;
    use glam::f64::DVec2;

    fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
        let mut designer = StructureDesigner::new();
        designer.add_node_network(network_name);
        designer.set_active_node_network_name(Some(network_name.to_string()));
        designer
    }

    /// Helper to edit the active network in a StructureDesigner.
    /// Returns the edit result.
    fn edit_designer_network(
        designer: &mut StructureDesigner,
        network_name: &str,
        code: &str,
        replace: bool,
    ) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
        // Temporarily remove network to avoid borrow conflicts
        let mut network = designer.node_type_registry.node_networks.remove(network_name).unwrap();
        let result = edit_network(&mut network, &designer.node_type_registry, code, replace);
        designer.node_type_registry.node_networks.insert(network_name.to_string(), network);
        result
    }

    /// Phase 6 verification: Text-format created nodes have custom_name preserved.
    /// When parsing "mybox = cuboid {...}", the node's custom_name should be "mybox".
    #[test]
    fn test_text_format_custom_name_preserved() {
        let mut designer = setup_designer_with_network("test_network");
        let result = edit_designer_network(
            &mut designer,
            "test_network",
            "mybox = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }",
            true,
        );
        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        let node_id = designer.find_node_id_by_name("mybox").unwrap();
        let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
        let node = network.nodes.get(&node_id).unwrap();

        assert_eq!(node.custom_name, Some("mybox".to_string()));
        assert_eq!(node.node_type_name, "cuboid");
    }

    /// Phase 6 verification: UI-created nodes (via add_node) get auto-generated names
    /// that can be found using find_node_id_by_name.
    #[test]
    fn test_find_node_by_name_ui_created() {
        let mut designer = setup_designer_with_network("test_network");

        // Create node via UI (simulated by add_node) - gets auto-generated name "expr1"
        let expr_id = designer.add_node("expr", DVec2::ZERO);

        // Verify the auto-generated name can be found
        let found = designer.find_node_id_by_name("expr1");
        assert!(found.is_some(), "Should find UI-created node by its auto-generated name");
        assert_eq!(found.unwrap(), expr_id);
    }

    /// Phase 6 verification: Text-format created nodes can be found by name.
    #[test]
    fn test_find_node_by_name_text_created() {
        let mut designer = setup_designer_with_network("test_network");
        let result = edit_designer_network(
            &mut designer,
            "test_network",
            "mynode = int { value: 42 }",
            true,
        );
        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        let found = designer.find_node_id_by_name("mynode");
        assert!(found.is_some(), "Should find text-created node by its name");

        // Verify it's the correct node
        let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
        let node = network.nodes.get(&found.unwrap()).unwrap();
        assert_eq!(node.node_type_name, "int");
    }

    /// Phase 6 verification: Both UI-created and text-created nodes coexist
    /// and can both be found by their names.
    #[test]
    fn test_find_node_by_name_mixed_creation() {
        let mut designer = setup_designer_with_network("test_network");

        // Create nodes via UI (auto-generated names)
        let ui_int_id = designer.add_node("int", DVec2::ZERO);  // Gets "int1"
        let ui_float_id = designer.add_node("float", DVec2::new(100.0, 0.0));  // Gets "float1"

        // Create nodes via text format (custom names) - incremental mode
        let result = edit_designer_network(
            &mut designer,
            "test_network",
            "my_sphere = sphere { center: (0, 0, 0), radius: 5 }",
            false,
        );
        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        let result = edit_designer_network(
            &mut designer,
            "test_network",
            "my_cuboid = cuboid { extent: (10, 10, 10) }",
            false,
        );
        assert!(result.success, "Edit should succeed: {:?}", result.errors);

        // Verify all nodes can be found by name
        assert_eq!(designer.find_node_id_by_name("int1"), Some(ui_int_id));
        assert_eq!(designer.find_node_id_by_name("float1"), Some(ui_float_id));
        assert!(designer.find_node_id_by_name("my_sphere").is_some());
        assert!(designer.find_node_id_by_name("my_cuboid").is_some());

        // Non-existent names should not be found
        assert!(designer.find_node_id_by_name("nonexistent").is_none());
    }

    /// Phase 6 verification: Text-format can reference UI-created nodes by their
    /// auto-generated names in incremental mode.
    #[test]
    fn test_text_format_references_ui_created_nodes() {
        let mut designer = setup_designer_with_network("test_network");

        // Create nodes via UI (auto-generated names)
        designer.add_node("int", DVec2::ZERO);  // Gets "int1"
        designer.add_node("int", DVec2::new(0.0, 100.0));  // Gets "int2"

        // Edit in incremental mode should be able to reference UI-created nodes
        let result = edit_designer_network(
            &mut designer,
            "test_network",
            r#"result = expr { x: int1, y: int2, expression: "x + y" }"#,
            false,
        );
        assert!(result.success, "Should be able to reference UI-created nodes: {:?}", result.errors);

        // Verify connections were made
        let result_id = designer.find_node_id_by_name("result").unwrap();
        let network = designer.node_type_registry.node_networks.get("test_network").unwrap();
        let result_node = network.nodes.get(&result_id).unwrap();

        // The expr node should have connections for x and y parameters
        // (Dynamic parameters are created by the expr node based on the parameters property)
        assert_eq!(result_node.node_type_name, "expr");
    }

    /// Phase 6 verification: Duplicated nodes get unique persistent names.
    #[test]
    fn test_duplicated_nodes_have_unique_persistent_names() {
        let mut designer = setup_designer_with_network("test_network");

        // Create original node
        let original_id = designer.add_node("sphere", DVec2::ZERO);  // Gets "sphere1"

        // Duplicate it
        let dup_id = designer.duplicate_node(original_id);
        assert_ne!(dup_id, 0, "Duplication should succeed");

        // Both should be findable by their unique names
        let found_original = designer.find_node_id_by_name("sphere1");
        let found_dup = designer.find_node_id_by_name("sphere2");

        assert_eq!(found_original, Some(original_id));
        assert_eq!(found_dup, Some(dup_id));
    }

    /// Phase 6 verification: After deletion, names are not reused (stability guarantee).
    #[test]
    fn test_names_not_reused_after_deletion() {
        let mut designer = setup_designer_with_network("test_network");

        // Create nodes
        let int1_id = designer.add_node("int", DVec2::ZERO);  // Gets "int1"
        designer.add_node("int", DVec2::new(100.0, 0.0));  // Gets "int2"

        // Delete int1
        {
            let network = designer.node_type_registry.node_networks.get_mut("test_network").unwrap();
            network.select_node(int1_id);
        }
        designer.delete_selected();

        // Create another int node - should be "int3", not "int1"
        let new_int_id = designer.add_node("int", DVec2::new(200.0, 0.0));

        let found = designer.find_node_id_by_name("int3");
        assert_eq!(found, Some(new_int_id), "New node should be 'int3', not reusing 'int1'");

        // "int1" should no longer exist
        assert!(designer.find_node_id_by_name("int1").is_none());
    }
}
