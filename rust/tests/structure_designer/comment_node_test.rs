use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::nodes::comment::{CommentData, get_node_type};

#[test]
fn test_comment_data_default() {
    let data = CommentData::default();
    assert_eq!(data.label, "");
    assert_eq!(data.text, "");
    assert_eq!(data.width, 200.0);
    assert_eq!(data.height, 100.0);
}

#[test]
fn test_comment_node_type() {
    let node_type = get_node_type();
    assert_eq!(node_type.name, "Comment");
    assert!(node_type.parameters.is_empty());
    assert!(node_type.public);
}

#[test]
fn test_comment_node_has_no_output() {
    let node_type = get_node_type();
    assert_eq!(node_type.output_type, DataType::None);
}

#[test]
fn test_comment_serialization_roundtrip() {
    let original = CommentData {
        label: "Test Label".to_string(),
        text: "Test content with special chars: <>&\"'".to_string(),
        width: 250.0,
        height: 150.0,
    };

    let json = serde_json::to_value(&original).unwrap();
    let restored: CommentData = serde_json::from_value(json).unwrap();

    assert_eq!(restored.label, original.label);
    assert_eq!(restored.text, original.text);
    assert_eq!(restored.width, original.width);
    assert_eq!(restored.height, original.height);
}

#[test]
fn test_comment_data_with_multiline_text() {
    let data = CommentData {
        label: "Notes".to_string(),
        text: "Line 1\nLine 2\nLine 3".to_string(),
        width: 300.0,
        height: 200.0,
    };

    let json = serde_json::to_value(&data).unwrap();
    let restored: CommentData = serde_json::from_value(json).unwrap();

    assert_eq!(restored.text, "Line 1\nLine 2\nLine 3");
}

#[test]
fn test_comment_data_with_empty_fields() {
    let data = CommentData {
        label: String::new(),
        text: String::new(),
        width: 100.0,
        height: 60.0,
    };

    let json = serde_json::to_value(&data).unwrap();
    let restored: CommentData = serde_json::from_value(json).unwrap();

    assert_eq!(restored.label, "");
    assert_eq!(restored.text, "");
}
