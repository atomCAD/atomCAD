use std::collections::HashMap;
use glam::f64::DVec3;
use glam::i32::IVec3;
use crate::structure_designer::evaluator::motif::{Motif, ParameterElement, Site, SiteSpecifier, MotifBond};
use crate::common::common_constants::CHEMICAL_ELEMENTS;

#[derive(Debug)]
pub struct ParseError {
    pub line_number: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error at line {}: {}", self.line_number, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Tokenizes a line by splitting on whitespace and filtering out empty tokens
pub fn tokenize_line(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// Checks if a string is a valid identifier (alphanumeric characters and underscore, can start with number)
pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    
    s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Converts a relative cell direction character to an integer
/// '+' -> 1, '-' -> -1, '.' -> 0
fn cell_char_to_int(c: char) -> i32 {
    match c {
        '+' => 1,
        '-' => -1,
        '.' => 0,
        _ => unreachable!(), // Should only be called with validated characters
    }
}

/// Parses a site specifier (e.g., "+..1", "2", "-.+site1")
/// Returns (site_id, relative_cell)
pub fn parse_site_specifier(specifier: &str, line_number: usize) -> Result<SiteSpecifier, ParseError> {
    if specifier.len() < 1 {
        return Err(ParseError {
            line_number,
            message: "Site specifier cannot be empty".to_string(),
        });
    }

    // Check if it starts with a 3-character relative cell specifier
    if specifier.len() >= 4 {
        let potential_cell_spec = &specifier[0..3];
        if potential_cell_spec.chars().all(|c| c == '+' || c == '-' || c == '.') {
            // Parse the relative cell specifier
            let chars: Vec<char> = potential_cell_spec.chars().collect();
            let relative_cell = IVec3::new(
                cell_char_to_int(chars[0]), // X direction
                cell_char_to_int(chars[1]), // Y direction
                cell_char_to_int(chars[2]), // Z direction
            );
            
            let site_id = &specifier[3..];
            if !is_valid_identifier(site_id) {
                return Err(ParseError {
                    line_number,
                    message: format!("'{}' is not a valid site ID in site specifier '{}'", site_id, specifier),
                });
            }
            
            return Ok(SiteSpecifier {
                id: site_id.to_string(),
                relative_cell,
            });
        }
    }
    
    // No relative cell specifier, just a site ID
    if !is_valid_identifier(specifier) {
        return Err(ParseError {
            line_number,
            message: format!("'{}' is not a valid site ID", specifier),
        });
    }
    
    Ok(SiteSpecifier {
        id: specifier.to_string(),
        relative_cell: IVec3::ZERO,
    })
}

/// Parses a param command line
/// Format: param PARAMETER_NAME [DEFAULT_ELEMENT]
pub fn parse_param_command(tokens: &[String], line_number: usize) -> Result<ParameterElement, ParseError> {
    if tokens.len() < 2 {
        return Err(ParseError {
            line_number,
            message: "param command requires at least a parameter name".to_string(),
        });
    }

    if tokens.len() > 3 {
        return Err(ParseError {
            line_number,
            message: "param command takes at most 2 arguments (parameter name and optional default element)".to_string(),
        });
    }

    let parameter_name = &tokens[1];
    
    // Validate parameter name is a valid identifier
    if !is_valid_identifier(parameter_name) {
        return Err(ParseError {
            line_number,
            message: format!("'{}' is not a valid parameter name (must contain only alphanumeric characters and underscores)", parameter_name),
        });
    }

    // Get default element (if provided) or use Carbon as default
    let default_atomic_number = if tokens.len() == 3 {
        let element_symbol = &tokens[2];
        
        // Look up the atomic number from the CHEMICAL_ELEMENTS map
        // Note: CHEMICAL_ELEMENTS uses original case (e.g., "Si", "Al"), not uppercase
        match CHEMICAL_ELEMENTS.get(element_symbol) {
            Some(&atomic_number) => atomic_number,
            None => {
                return Err(ParseError {
                    line_number,
                    message: format!("Unknown chemical element: '{}'", element_symbol),
                });
            }
        }
    } else {
        // Default to Carbon (atomic number 6)
        6
    };

    Ok(ParameterElement {
        name: parameter_name.clone(),
        default_atomic_number,
    })
}

/// Parses a site command line
/// Format: site SITE_ID ELEMENT_NAME X Y Z
pub fn parse_site_command(tokens: &[String], line_number: usize, parameters: &[ParameterElement]) -> Result<(String, Site), ParseError> {
    if tokens.len() != 6 {
        return Err(ParseError {
            line_number,
            message: "site command requires exactly 5 arguments: site SITE_ID ELEMENT_NAME X Y Z".to_string(),
        });
    }

    let site_id = &tokens[1];
    let element_name = &tokens[2];
    
    // Validate site ID is a valid identifier
    if !is_valid_identifier(site_id) {
        return Err(ParseError {
            line_number,
            message: format!("'{}' is not a valid site ID (must contain only alphanumeric characters and underscores)", site_id),
        });
    }

    // Parse coordinates
    let x = tokens[3].parse::<f64>().map_err(|_| ParseError {
        line_number,
        message: format!("Invalid X coordinate: '{}' (must be a number)", tokens[3]),
    })?;

    let y = tokens[4].parse::<f64>().map_err(|_| ParseError {
        line_number,
        message: format!("Invalid Y coordinate: '{}' (must be a number)", tokens[4]),
    })?;

    let z = tokens[5].parse::<f64>().map_err(|_| ParseError {
        line_number,
        message: format!("Invalid Z coordinate: '{}' (must be a number)", tokens[5]),
    })?;

    // Determine atomic number
    let atomic_number = if is_valid_identifier(element_name) {
        // Check if it's a chemical element first
        match CHEMICAL_ELEMENTS.get(element_name) {
            Some(&atomic_number) => atomic_number,
            None => {
                // Check if it's a parameter element
                if let Some(param_index) = parameters.iter().position(|p| p.name == *element_name) {
                    // Parameter elements use negative indices (first parameter is -1, second is -2, etc.)
                    -(param_index as i32 + 1)
                } else {
                    return Err(ParseError {
                        line_number,
                        message: format!("Unknown element or parameter: '{}' (not found in chemical elements or declared parameters)", element_name),
                    });
                }
            }
        }
    } else {
        return Err(ParseError {
            line_number,
            message: format!("'{}' is not a valid element name or parameter element (must contain only alphanumeric characters and underscores)", element_name),
        });
    };

    Ok((
        site_id.clone(),
        Site {
            atomic_number,
            position: DVec3::new(x, y, z),
        },
    ))
}

/// Parses a bond command line
/// Format: bond SITE_SPECIFIER1 SITE_SPECIFIER2 [multiplicity]
pub fn parse_bond_command(tokens: &[String], line_number: usize) -> Result<MotifBond, ParseError> {
    if tokens.len() < 3 {
        return Err(ParseError {
            line_number,
            message: "bond command requires at least 2 site specifiers".to_string(),
        });
    }

    if tokens.len() > 4 {
        return Err(ParseError {
            line_number,
            message: "bond command takes at most 3 arguments (2 site specifiers and optional multiplicity)".to_string(),
        });
    }

    // Parse the two site specifiers
    let site_1 = parse_site_specifier(&tokens[1], line_number)?;
    let site_2 = parse_site_specifier(&tokens[2], line_number)?;

    // Parse multiplicity (default to 1 if not provided)
    let multiplicity = if tokens.len() == 4 {
        tokens[3].parse::<i32>().map_err(|_| ParseError {
            line_number,
            message: format!("Invalid multiplicity: '{}' (must be a positive integer)", tokens[3]),
        })?
    } else {
        1
    };

    // Validate multiplicity is positive
    if multiplicity <= 0 {
        return Err(ParseError {
            line_number,
            message: format!("Multiplicity must be positive, got: {}", multiplicity),
        });
    }

    Ok(MotifBond {
        site_1,
        site_2,
        multiplicity,
    })
}

/// Parses the complete motif definition text
pub fn parse_motif(motif_text: &str) -> Result<Motif, ParseError> {
    let mut parameters = Vec::new();
    let mut sites = HashMap::new();
    let mut bonds = Vec::new();

    for (line_index, line) in motif_text.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed_line = line.trim();

        // Skip empty lines
        if trimmed_line.is_empty() {
            continue;
        }

        // Skip comment lines (starting with #)
        if trimmed_line.starts_with('#') {
            continue;
        }

        // Tokenize the line
        let tokens = tokenize_line(trimmed_line);
        
        if tokens.is_empty() {
            continue;
        }

        // Parse based on command type (case insensitive)
        match tokens[0].to_lowercase().as_str() {
            "param" => {
                let param = parse_param_command(&tokens, line_number)?;
                parameters.push(param);
            }
            "site" => {
                let (site_id, site) = parse_site_command(&tokens, line_number, &parameters)?;
                sites.insert(site_id, site);
            }
            "bond" => {
                let bond = parse_bond_command(&tokens, line_number)?;
                bonds.push(bond);
            }
            _ => {
                return Err(ParseError {
                    line_number,
                    message: format!("Unknown command: '{}'", tokens[0]),
                });
            }
        }
    }

    Ok(Motif {
        parameters,
        sites,
        bonds,
    })
}

