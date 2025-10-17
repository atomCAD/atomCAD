use std::collections::HashMap;
use glam::f64::DVec3;
use glam::i32::IVec3;

#[derive(Debug, Clone)]
pub struct ParameterElement {
  pub name: String,
  pub default_atomic_number: i32,
}

#[derive(Debug, Clone)]
pub struct Site {
  // negative numbers are parameter elements (first is represented by -1)
  pub atomic_number: i32,
  // Fractional lattice coordinates
  pub position: DVec3,
}

#[derive(Debug, Clone)]
pub struct SiteSpecifier {
  pub id: String,
  pub relative_cell: IVec3,
}

#[derive(Debug, Clone)]
pub struct MotifBond {
  pub site_1: SiteSpecifier,
  pub site_2: SiteSpecifier,
  pub multiplicity: i32,
}

#[derive(Debug, Clone)]
pub struct Motif {
  pub parameters: Vec<ParameterElement>,
  pub sites: HashMap<String, Site>,
  pub bonds: Vec<MotifBond>,  
}

impl Motif {
  /// Returns a complete HashMap of parameter element values, filling in default values
  /// for any parameter elements that are not specified in the input map.
  pub fn get_effective_parameter_element_values(
    &self,
    parameter_element_values: &HashMap<String, i32>
  ) -> HashMap<String, i32> {
    let mut effective_values = HashMap::new();
    
    // Iterate through all parameter elements defined in the motif
    for parameter in &self.parameters {
      let effective_value = match parameter_element_values.get(&parameter.name) {
        Some(&value) => value, // Use provided value if available
        None => parameter.default_atomic_number, // Use default value if not provided
      };
      effective_values.insert(parameter.name.clone(), effective_value);
    }
    
    effective_values
  }
}
