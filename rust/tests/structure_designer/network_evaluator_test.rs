use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;

#[test]
fn test_network_result_from_string_bool() {
    let result = NetworkResult::from_string("true", &DataType::Bool);
    assert!(matches!(result, Ok(NetworkResult::Bool(true))));

    let result = NetworkResult::from_string("false", &DataType::Bool);
    assert!(matches!(result, Ok(NetworkResult::Bool(false))));

    let result = NetworkResult::from_string("invalid", &DataType::Bool);
    assert!(result.is_err());
}

#[test]
fn test_network_result_from_string_int() {
    let result = NetworkResult::from_string("42", &DataType::Int);
    assert!(matches!(result, Ok(NetworkResult::Int(42))));

    let result = NetworkResult::from_string("-100", &DataType::Int);
    assert!(matches!(result, Ok(NetworkResult::Int(-100))));

    let result = NetworkResult::from_string("not_a_number", &DataType::Int);
    assert!(result.is_err());
}

#[test]
fn test_network_result_from_string_float() {
    let result = NetworkResult::from_string("3.14", &DataType::Float);
    match result {
        Ok(NetworkResult::Float(f)) => assert!((f - 3.14).abs() < 0.001),
        _ => panic!("Expected Float result"),
    }

    let result = NetworkResult::from_string("not_a_float", &DataType::Float);
    assert!(result.is_err());
}

#[test]
fn test_network_result_from_string_vec2() {
    let result = NetworkResult::from_string("1.5, 2.5", &DataType::Vec2);
    match result {
        Ok(NetworkResult::Vec2(v)) => {
            assert!((v.x - 1.5).abs() < 0.001);
            assert!((v.y - 2.5).abs() < 0.001);
        }
        _ => panic!("Expected Vec2 result"),
    }

    let result = NetworkResult::from_string("1.0", &DataType::Vec2);
    assert!(result.is_err());
}

#[test]
fn test_network_result_from_string_vec3() {
    let result = NetworkResult::from_string("1.0, 2.0, 3.0", &DataType::Vec3);
    match result {
        Ok(NetworkResult::Vec3(v)) => {
            assert!((v.x - 1.0).abs() < 0.001);
            assert!((v.y - 2.0).abs() < 0.001);
            assert!((v.z - 3.0).abs() < 0.001);
        }
        _ => panic!("Expected Vec3 result"),
    }
}

#[test]
fn test_network_result_from_string_ivec2() {
    let result = NetworkResult::from_string("10, 20", &DataType::IVec2);
    match result {
        Ok(NetworkResult::IVec2(v)) => {
            assert_eq!(v.x, 10);
            assert_eq!(v.y, 20);
        }
        _ => panic!("Expected IVec2 result"),
    }
}

#[test]
fn test_network_result_from_string_ivec3() {
    let result = NetworkResult::from_string("1, 2, 3", &DataType::IVec3);
    match result {
        Ok(NetworkResult::IVec3(v)) => {
            assert_eq!(v.x, 1);
            assert_eq!(v.y, 2);
            assert_eq!(v.z, 3);
        }
        _ => panic!("Expected IVec3 result"),
    }
}

#[test]
fn test_network_result_is_error() {
    let error = NetworkResult::Error("test error".to_string());
    assert!(error.is_error());

    let float = NetworkResult::Float(1.0);
    assert!(!float.is_error());

    let none = NetworkResult::None;
    assert!(!none.is_error());
}

#[test]
fn test_network_result_extract_methods() {
    let float = NetworkResult::Float(3.14);
    assert!(matches!(float.extract_float(), Some(f) if (f - 3.14).abs() < 0.001));

    let int = NetworkResult::Int(42);
    assert!(matches!(int.extract_int(), Some(42)));

    let string = NetworkResult::String("test".to_string());
    assert!(matches!(string.extract_string(), Some(s) if s == "test"));

    let bool_val = NetworkResult::Bool(true);
    assert!(matches!(bool_val.extract_bool(), Some(true)));

    let vec3 = NetworkResult::Vec3(DVec3::new(1.0, 2.0, 3.0));
    assert!(matches!(vec3.extract_vec3(), Some(v) if v == DVec3::new(1.0, 2.0, 3.0)));

    let ivec3 = NetworkResult::IVec3(IVec3::new(1, 2, 3));
    assert!(matches!(ivec3.extract_ivec3(), Some(v) if v == IVec3::new(1, 2, 3)));
}

#[test]
fn test_network_result_propagate_error() {
    let error = NetworkResult::Error("test".to_string());
    assert!(error.propagate_error().is_some());

    let float = NetworkResult::Float(1.0);
    assert!(float.propagate_error().is_none());
}

#[test]
fn test_network_result_to_display_string() {
    let float = NetworkResult::Float(3.14);
    let display = float.to_display_string();
    assert!(display.contains("3.14"));

    let int = NetworkResult::Int(42);
    let display = int.to_display_string();
    assert!(display.contains("42"));

    let string = NetworkResult::String("hello".to_string());
    let display = string.to_display_string();
    assert!(display.contains("hello"));

    let bool_val = NetworkResult::Bool(true);
    let display = bool_val.to_display_string();
    assert!(display.contains("true"));

    let error = NetworkResult::Error("error msg".to_string());
    let display = error.to_display_string();
    assert!(display.contains("Error"));
}

#[test]
fn test_network_result_default() {
    let default: NetworkResult = Default::default();
    assert!(matches!(default, NetworkResult::None));
}

#[test]
fn test_network_result_vec2_extraction() {
    let vec2 = NetworkResult::Vec2(DVec2::new(5.0, 10.0));
    match vec2.extract_vec2() {
        Some(v) => {
            assert!((v.x - 5.0).abs() < 0.001);
            assert!((v.y - 10.0).abs() < 0.001);
        }
        None => panic!("Expected Vec2 extraction to succeed"),
    }

    let float = NetworkResult::Float(1.0);
    assert!(float.extract_vec2().is_none());
}

#[test]
fn test_network_result_ivec2_extraction() {
    let ivec2 = NetworkResult::IVec2(IVec2::new(3, 4));
    match ivec2.extract_ivec2() {
        Some(v) => {
            assert_eq!(v.x, 3);
            assert_eq!(v.y, 4);
        }
        None => panic!("Expected IVec2 extraction to succeed"),
    }
}
