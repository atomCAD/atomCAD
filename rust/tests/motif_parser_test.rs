use rust_lib_flutter_cad::structure_designer::evaluator::motif_parser::*;

#[test]
fn test_tokenize_line() {
    assert_eq!(tokenize_line("param PRIMARY C"), vec!["param", "PRIMARY", "C"]);
    assert_eq!(tokenize_line("  site  1  C  0.0  0.0  0.0  "), vec!["site", "1", "C", "0.0", "0.0", "0.0"]);
    assert_eq!(tokenize_line(""), Vec::<String>::new());
}

#[test]
fn test_parse_empty_motif() {
    let result = parse_motif("");
    assert!(result.is_ok());
    let motif = result.unwrap();
    assert_eq!(motif.parameters.len(), 0);
    assert_eq!(motif.sites.len(), 0);
    assert_eq!(motif.bonds.len(), 0);
}

#[test]
fn test_parse_comments_and_empty_lines() {
    let motif_text = "
# This is a comment
   # Another comment

# Empty lines above should be ignored
";
    let result = parse_motif(motif_text);
    assert!(result.is_ok());
    let motif = result.unwrap();
    assert_eq!(motif.parameters.len(), 0);
    assert_eq!(motif.sites.len(), 0);
    assert_eq!(motif.bonds.len(), 0);
}

#[test]
fn test_unknown_command_error() {
    let motif_text = "unknown_command arg1 arg2";
    let result = parse_motif(motif_text);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.line_number, 1);
    assert!(error.message.contains("Unknown command: 'unknown_command'"));
}

#[test]
fn test_is_valid_identifier() {
    // Valid identifiers
    assert!(is_valid_identifier("PRIMARY"));
    assert!(is_valid_identifier("SECONDARY"));
    assert!(is_valid_identifier("param1"));
    assert!(is_valid_identifier("1site"));
    assert!(is_valid_identifier("my_param"));
    assert!(is_valid_identifier("A1B2_C3"));
    
    // Invalid identifiers
    assert!(!is_valid_identifier(""));
    assert!(!is_valid_identifier("param-name"));
    assert!(!is_valid_identifier("param.name"));
    assert!(!is_valid_identifier("param name"));
    assert!(!is_valid_identifier("param@name"));
}

#[test]
fn test_parse_param_command_basic() {
    let tokens = vec!["param".to_string(), "PRIMARY".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_ok());
    let param = result.unwrap();
    assert_eq!(param.name, "PRIMARY");
    assert_eq!(param.default_atomic_number, 6); // Carbon default
}

#[test]
fn test_parse_param_command_with_element() {
    let tokens = vec!["param".to_string(), "SECONDARY".to_string(), "Si".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_ok());
    let param = result.unwrap();
    assert_eq!(param.name, "SECONDARY");
    assert_eq!(param.default_atomic_number, 14); // Silicon
}

#[test]
fn test_parse_param_command_case_sensitive_element() {
    // Element symbols are case-sensitive: "Si" not "si"
    let tokens = vec!["param".to_string(), "TEST".to_string(), "Si".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_ok());
    let param = result.unwrap();
    assert_eq!(param.default_atomic_number, 14); // Silicon
}

#[test]
fn test_parse_param_command_errors() {
    // Missing parameter name
    let tokens = vec!["param".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("requires at least a parameter name"));
    
    // Too many arguments
    let tokens = vec!["param".to_string(), "NAME".to_string(), "C".to_string(), "extra".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("at most 2 arguments"));
    
    // Invalid parameter name
    let tokens = vec!["param".to_string(), "invalid-name".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("not a valid parameter name"));
    
    // Unknown element
    let tokens = vec!["param".to_string(), "NAME".to_string(), "Xx".to_string()];
    let result = parse_param_command(&tokens, 1);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("Unknown chemical element"));
}

#[test]
fn test_parse_site_command_with_chemical_element() {
    let tokens = vec!["site".to_string(), "1".to_string(), "C".to_string(), "0.0".to_string(), "0.0".to_string(), "0.0".to_string()];
    let parameters = vec![];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_ok());
    let (site_id, site) = result.unwrap();
    assert_eq!(site_id, "1");
    assert_eq!(site.atomic_number, 6); // Carbon
    assert_eq!(site.position.x, 0.0);
    assert_eq!(site.position.y, 0.0);
    assert_eq!(site.position.z, 0.0);
}

#[test]
fn test_parse_site_command_with_parameter_element() {
    use rust_lib_flutter_cad::structure_designer::evaluator::motif::ParameterElement;
    
    let tokens = vec!["site".to_string(), "site1".to_string(), "PRIMARY".to_string(), "0.25".to_string(), "0.25".to_string(), "0.25".to_string()];
    let parameters = vec![
        ParameterElement { name: "PRIMARY".to_string(), default_atomic_number: 6 },
        ParameterElement { name: "SECONDARY".to_string(), default_atomic_number: 14 },
    ];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_ok());
    let (site_id, site) = result.unwrap();
    assert_eq!(site_id, "site1");
    assert_eq!(site.atomic_number, -1); // First parameter element
    assert_eq!(site.position.x, 0.25);
    assert_eq!(site.position.y, 0.25);
    assert_eq!(site.position.z, 0.25);
}

#[test]
fn test_parse_site_command_with_second_parameter() {
    use rust_lib_flutter_cad::structure_designer::evaluator::motif::ParameterElement;
    
    let tokens = vec!["site".to_string(), "site2".to_string(), "SECONDARY".to_string(), "0.75".to_string(), "0.75".to_string(), "0.75".to_string()];
    let parameters = vec![
        ParameterElement { name: "PRIMARY".to_string(), default_atomic_number: 6 },
        ParameterElement { name: "SECONDARY".to_string(), default_atomic_number: 14 },
    ];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_ok());
    let (site_id, site) = result.unwrap();
    assert_eq!(site_id, "site2");
    assert_eq!(site.atomic_number, -2); // Second parameter element
}

#[test]
fn test_parse_site_command_errors() {
    let parameters = vec![];
    
    // Wrong number of arguments
    let tokens = vec!["site".to_string(), "1".to_string(), "C".to_string()];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("requires exactly 5 arguments"));
    
    // Invalid site ID
    let tokens = vec!["site".to_string(), "invalid-id".to_string(), "C".to_string(), "0.0".to_string(), "0.0".to_string(), "0.0".to_string()];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("not a valid site ID"));
    
    // Invalid coordinates
    let tokens = vec!["site".to_string(), "1".to_string(), "C".to_string(), "not_a_number".to_string(), "0.0".to_string(), "0.0".to_string()];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("Invalid X coordinate"));
    
    // Unknown element/parameter
    let tokens = vec!["site".to_string(), "1".to_string(), "UNKNOWN".to_string(), "0.0".to_string(), "0.0".to_string(), "0.0".to_string()];
    let result = parse_site_command(&tokens, 1, &parameters);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("Unknown element or parameter"));
}

#[test]
fn test_parse_motif_with_params_and_sites() {
    let motif_text = "
param PRIMARY C
param SECONDARY Si
site 1 PRIMARY 0.0 0.0 0.0
site 2 SECONDARY 0.25 0.25 0.25
site 3 N 0.5 0.5 0.5
";
    let result = parse_motif(motif_text);
    assert!(result.is_ok());
    let motif = result.unwrap();
    
    // Check parameters
    assert_eq!(motif.parameters.len(), 2);
    assert_eq!(motif.parameters[0].name, "PRIMARY");
    assert_eq!(motif.parameters[1].name, "SECONDARY");
    
    // Check sites
    assert_eq!(motif.sites.len(), 3);
    assert_eq!(motif.sites[0].atomic_number, -1); // PRIMARY (first parameter)
    assert_eq!(motif.sites[1].atomic_number, -2); // SECONDARY (second parameter)
    assert_eq!(motif.sites[2].atomic_number, 7);  // Nitrogen
}

#[test]
fn test_parse_site_specifier_simple() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("1".to_string(), 0);
    
    let result = parse_site_specifier("1", 1, &site_id_to_index);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert_eq!(spec.site_index, 0);
    assert_eq!(spec.relative_cell, glam::IVec3::ZERO);
}

#[test]
fn test_parse_site_specifier_with_cell() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("1".to_string(), 0);
    
    let result = parse_site_specifier("+..1", 1, &site_id_to_index);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert_eq!(spec.site_index, 0);
    assert_eq!(spec.relative_cell.x, 1);
    assert_eq!(spec.relative_cell.y, 0);
    assert_eq!(spec.relative_cell.z, 0);
}

#[test]
fn test_parse_site_specifier_complex() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("site_2".to_string(), 1);
    
    let result = parse_site_specifier("+-+site_2", 1, &site_id_to_index);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert_eq!(spec.site_index, 1);
    assert_eq!(spec.relative_cell.x, 1);
    assert_eq!(spec.relative_cell.y, -1);
    assert_eq!(spec.relative_cell.z, 1);
}

#[test]
fn test_parse_site_specifier_all_directions() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("atom1".to_string(), 2);
    
    let result = parse_site_specifier("---atom1", 1, &site_id_to_index);
    assert!(result.is_ok());
    let spec = result.unwrap();
    assert_eq!(spec.site_index, 2);
    assert_eq!(spec.relative_cell.x, -1);
    assert_eq!(spec.relative_cell.y, -1);
    assert_eq!(spec.relative_cell.z, -1);
}

#[test]
fn test_parse_site_specifier_errors() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("valid_id".to_string(), 0);
    
    // Empty specifier
    let result = parse_site_specifier("", 1, &site_id_to_index);
    assert!(result.is_err());
    
    // Invalid site ID (contains hyphen)
    let result = parse_site_specifier("invalid-id", 1, &site_id_to_index);
    assert!(result.is_err());
    
    // Invalid site ID with cell specifier (contains hyphen)
    let result = parse_site_specifier("+..invalid-id", 1, &site_id_to_index);
    assert!(result.is_err());
}

#[test]
fn test_parse_bond_command_basic() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("1".to_string(), 0);
    site_id_to_index.insert("2".to_string(), 1);
    
    let tokens = vec!["bond".to_string(), "1".to_string(), "2".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_ok());
    let bond = result.unwrap();
    assert_eq!(bond.site_1.site_index, 0);
    assert_eq!(bond.site_2.site_index, 1);
    assert_eq!(bond.multiplicity, 1);
    assert_eq!(bond.site_1.relative_cell, glam::IVec3::ZERO);
    assert_eq!(bond.site_2.relative_cell, glam::IVec3::ZERO);
}

#[test]
fn test_parse_bond_command_with_cell_specifiers() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("1".to_string(), 0);
    site_id_to_index.insert("2".to_string(), 1);
    
    let tokens = vec!["bond".to_string(), "2".to_string(), "+..1".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_ok());
    let bond = result.unwrap();
    assert_eq!(bond.site_1.site_index, 1);  // site 2 (index 1)
    assert_eq!(bond.site_2.site_index, 0);  // site 1 (index 0) with +..1 relative cell
    assert_eq!(bond.site_1.relative_cell, glam::IVec3::ZERO);  // site 2 has (0,0,0)
    assert_eq!(bond.site_2.relative_cell.x, 1);  // +..1 means (1,0,0)
    assert_eq!(bond.site_2.relative_cell.y, 0);
    assert_eq!(bond.site_2.relative_cell.z, 0);
}

#[test]
fn test_parse_bond_command_with_multiplicity() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("site1".to_string(), 0);
    site_id_to_index.insert("site2".to_string(), 1);
    
    let tokens = vec!["bond".to_string(), "site1".to_string(), "site2".to_string(), "2".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_ok());
    let bond = result.unwrap();
    assert_eq!(bond.site_1.site_index, 0);
    assert_eq!(bond.site_2.site_index, 1);
    assert_eq!(bond.multiplicity, 2);
}

#[test]
fn test_parse_bond_command_errors() {
    let mut site_id_to_index = std::collections::HashMap::new();
    site_id_to_index.insert("1".to_string(), 0);
    site_id_to_index.insert("2".to_string(), 1);
    
    // Too few arguments
    let tokens = vec!["bond".to_string(), "1".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("requires at least 2 site specifiers"));
    
    // Too many arguments
    let tokens = vec!["bond".to_string(), "1".to_string(), "2".to_string(), "1".to_string(), "extra".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("at most 3 arguments"));
    
    // Invalid multiplicity
    let tokens = vec!["bond".to_string(), "1".to_string(), "2".to_string(), "not_a_number".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("Invalid multiplicity"));
    
    // Zero multiplicity
    let tokens = vec!["bond".to_string(), "1".to_string(), "2".to_string(), "0".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("must be positive"));
    
    // Negative multiplicity
    let tokens = vec!["bond".to_string(), "1".to_string(), "2".to_string(), "-1".to_string()];
    let result = parse_bond_command(&tokens, 1, &site_id_to_index);
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("must be positive"));
}

#[test]
fn test_parse_complete_motif() {
    let motif_text = "
# Zincblende motif example
param PRIMARY C
param SECONDARY Si

site 1 PRIMARY 0.0 0.0 0.0
site 2 SECONDARY 0.25 0.25 0.25

bond 1 2
bond 2 +..1 2
bond 2 .+.1
";
    let result = parse_motif(motif_text);
    assert!(result.is_ok());
    let motif = result.unwrap();
    
    // Check parameters
    assert_eq!(motif.parameters.len(), 2);
    assert_eq!(motif.parameters[0].name, "PRIMARY");
    assert_eq!(motif.parameters[1].name, "SECONDARY");
    
    // Check sites
    assert_eq!(motif.sites.len(), 2);
    assert_eq!(motif.sites[0].atomic_number, -1); // PRIMARY (first parameter)
    assert_eq!(motif.sites[1].atomic_number, -2); // SECONDARY (second parameter)
    
    // Check bonds
    assert_eq!(motif.bonds.len(), 3);
    
    // First bond: 1 2
    assert_eq!(motif.bonds[0].site_1.site_index, 0);
    assert_eq!(motif.bonds[0].site_2.site_index, 1);
    assert_eq!(motif.bonds[0].multiplicity, 1);
    assert_eq!(motif.bonds[0].site_1.relative_cell, glam::IVec3::ZERO);
    
    // Second bond: 2 +..1 2
    assert_eq!(motif.bonds[1].site_1.site_index, 1);  // site 2 (index 1)
    assert_eq!(motif.bonds[1].site_2.site_index, 0);  // site 1 (index 0) with +..1 relative cell
    assert_eq!(motif.bonds[1].multiplicity, 2);
    assert_eq!(motif.bonds[1].site_1.relative_cell, glam::IVec3::ZERO);  // site 2 has (0,0,0)
    assert_eq!(motif.bonds[1].site_2.relative_cell.x, 1);  // +..1 means (1,0,0)
    assert_eq!(motif.bonds[1].site_2.relative_cell.y, 0);
    assert_eq!(motif.bonds[1].site_2.relative_cell.z, 0);
    
    // Third bond: 2 .+.1
    assert_eq!(motif.bonds[2].site_1.site_index, 1);  // site 2 (index 1)
    assert_eq!(motif.bonds[2].site_2.site_index, 0);  // site 1 (index 0) with .+.1 relative cell
    assert_eq!(motif.bonds[2].multiplicity, 1);
    assert_eq!(motif.bonds[2].site_1.relative_cell, glam::IVec3::ZERO);  // site 2 has (0,0,0)
    assert_eq!(motif.bonds[2].site_2.relative_cell.x, 0);  // .+.1 means (0,1,0)
    assert_eq!(motif.bonds[2].site_2.relative_cell.y, 1);
    assert_eq!(motif.bonds[2].site_2.relative_cell.z, 0);
}
