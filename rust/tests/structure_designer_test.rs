use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

#[test]
fn test_rename_node_network_prevents_builtin_name_conflict() {
    let mut designer = StructureDesigner::new();
    
    // Add a test node network
    designer.add_node_network("test_network");
    
    // Try to rename to a built-in node type name - should fail
    let result = designer.rename_node_network("test_network", "parameter");
    assert_eq!(result, false, "Should not allow renaming to built-in node type 'parameter'");
    
    // Try to rename to another built-in node type name - should fail
    let result = designer.rename_node_network("test_network", "string");
    assert_eq!(result, false, "Should not allow renaming to built-in node type 'string'");
    
    // Try to rename to another built-in node type name - should fail
    let result = designer.rename_node_network("test_network", "int");
    assert_eq!(result, false, "Should not allow renaming to built-in node type 'int'");
    
    // Try to rename to a valid name - should succeed
    let result = designer.rename_node_network("test_network", "my_custom_network");
    assert_eq!(result, true, "Should allow renaming to valid custom name");
    
    // Verify the network was actually renamed
    assert!(designer.node_type_registry.node_networks.contains_key("my_custom_network"));
    assert!(!designer.node_type_registry.node_networks.contains_key("test_network"));
}

#[test]
fn test_rename_node_network_existing_validations_still_work() {
    let mut designer = StructureDesigner::new();
    
    // Add two test node networks
    designer.add_node_network("network1");
    designer.add_node_network("network2");
    
    // Try to rename to existing network name - should fail
    let result = designer.rename_node_network("network1", "network2");
    assert_eq!(result, false, "Should not allow renaming to existing network name");
    
    // Try to rename non-existent network - should fail
    let result = designer.rename_node_network("nonexistent", "new_name");
    assert_eq!(result, false, "Should not allow renaming non-existent network");
    
    // Valid rename should still work
    let result = designer.rename_node_network("network1", "renamed_network");
    assert_eq!(result, true, "Should allow valid rename");
}




