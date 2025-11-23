use std::collections::HashMap;
use glam::f64::DVec3;
use glam::i32::IVec3;

#[derive(Debug, Clone)]
pub struct ParameterElement {
  pub name: String,
  pub default_atomic_number: i16,
}

#[derive(Debug, Clone)]
pub struct Site {
  // negative numbers are parameter elements (first is represented by -1)
  pub atomic_number: i16,
  // Fractional lattice coordinates
  pub position: DVec3,
}

#[derive(Debug, Clone)]
pub struct SiteSpecifier {
  pub site_index: usize,
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
  pub sites: Vec<Site>,
  pub bonds: Vec<MotifBond>,
  /// Precomputed mapping: for each site index, stores indices of bonds where that site is site_1
  /// This optimizes bond creation by avoiding iteration through all bonds for each atom
  pub bonds_by_site1_index: Vec<Vec<usize>>,
  /// Precomputed mapping: for each site index, stores indices of bonds where that site is site_2
  /// This optimizes hydrogen passivation Case 2 by avoiding iteration through all bonds
  pub bonds_by_site2_index: Vec<Vec<usize>>,
}

impl Motif {
  /// Returns a complete HashMap of parameter element values, filling in default values
  /// for any parameter elements that are not specified in the input map.
  pub fn get_effective_parameter_element_values(
    &self,
    parameter_element_values: &HashMap<String, i16>
  ) -> HashMap<String, i16> {
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

  /// Compares two motifs for structural equality.
  /// 
  /// This method compares the essential structural components of two motifs:
  /// - Parameter elements (name and default atomic number)
  /// - Sites (atomic number and position)
  /// - Bonds (site specifiers and multiplicity)
  /// 
  /// The precomputed bond index mappings are NOT compared as they are derived
  /// from the bonds themselves.
  /// 
  /// # Arguments
  /// * `other` - The other motif to compare with
  /// 
  /// # Returns
  /// * `true` if the motifs are structurally identical
  /// * `false` otherwise
  pub fn is_structurally_equal(&self, other: &Motif) -> bool {
    // Quick size checks first
    if self.parameters.len() != other.parameters.len() ||
       self.sites.len() != other.sites.len() ||
       self.bonds.len() != other.bonds.len() {
      return false;
    }

    // Compare parameter elements
    for (p1, p2) in self.parameters.iter().zip(other.parameters.iter()) {
      if p1.name != p2.name || p1.default_atomic_number != p2.default_atomic_number {
        return false;
      }
    }

    // Compare sites
    for (s1, s2) in self.sites.iter().zip(other.sites.iter()) {
      if s1.atomic_number != s2.atomic_number || s1.position != s2.position {
        return false;
      }
    }

    // Compare bonds
    for (b1, b2) in self.bonds.iter().zip(other.bonds.iter()) {
      if b1.site_1.site_index != b2.site_1.site_index ||
         b1.site_1.relative_cell != b2.site_1.relative_cell ||
         b1.site_2.site_index != b2.site_2.site_index ||
         b1.site_2.relative_cell != b2.site_2.relative_cell ||
         b1.multiplicity != b2.multiplicity {
        return false;
      }
    }

    true
  }
}




