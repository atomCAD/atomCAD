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
