use rust_lib_flutter_cad::structure_designer::text_format::{
    TextValue, Parser, Lexer, Statement, PropertyValue, Token, serialize_network,
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

        let result = serialize_network(&network, &registry);
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

        let result = serialize_network(&network, &registry);

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

        let result = serialize_network(&network, &registry);

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

        let result = serialize_network(&network, &registry);

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

        let result = serialize_network(&network, &registry);

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

        let result = serialize_network(&network, &registry);

        assert!(result.contains("int1 = int"));
        assert!(result.contains("float1 = float"));
        assert!(result.contains("bool1 = bool"));
    }
}
