mod add_atom_tool;
mod add_bond_tool;
mod atom_edit_data;
mod default_tool;
mod minimization;
mod operations;
mod selection;
mod types;

pub mod atom_edit_gadget;
pub mod text_format;

// Re-export everything through the `atom_edit` path to maintain backward
// compatibility with existing imports like `atom_edit::atom_edit::AtomEditData`.
pub mod atom_edit {
    pub use super::add_atom_tool::*;
    pub use super::add_bond_tool::*;
    pub use super::atom_edit_data::*;
    pub use super::default_tool::*;
    pub use super::minimization::*;
    pub use super::operations::*;
    pub use super::selection::*;
    pub use super::types::*;
}
