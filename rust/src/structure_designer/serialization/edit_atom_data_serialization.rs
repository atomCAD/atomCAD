use serde::{Serialize, Deserialize};
use crate::structure_designer::edit_atom_commands::select_command::SelectCommand;
use crate::structure_designer::edit_atom_commands::add_bond_command::AddBondCommand;
use crate::structure_designer::node_data::edit_atom_data::EditAtomData;
use std::io;
use crate::structure_designer::edit_atom_command::EditAtomCommand;
use serde_json;
use crate::common::atomic_structure::AtomicStructure;

/// Serializable representation of an EditAtomCommand, which uses type tagging
#[derive(Serialize, Deserialize)]
pub struct SerializableEditAtomCommand {
    pub command_type: String,
    pub command_data: serde_json::Value,
}

/// Serializable version of EditAtomData without trait objects for JSON serialization
#[derive(Serialize, Deserialize)]
pub struct SerializableEditAtomData {
    pub history: Vec<SerializableEditAtomCommand>,
    pub next_history_index: usize,
}

/// Converts an EditAtomData to its serializable representation
/// 
/// # Returns
/// * `io::Result<SerializableEditAtomData>` - The serializable data or an error if serialization fails
pub fn edit_atom_data_to_serializable(data: &EditAtomData) -> io::Result<SerializableEditAtomData> {
    let mut serializable_commands = Vec::new();
    
    // Convert each EditAtomCommand to a SerializableEditAtomCommand
    for command in &data.history {
        let (command_type, command_data) = if let Some(select_cmd) = command.as_any_ref().downcast_ref::<SelectCommand>() {
            ("select".to_string(), serde_json::to_value(select_cmd)?)
        } else if let Some(add_bond_cmd) = command.as_any_ref().downcast_ref::<AddBondCommand>() {
            ("add_bond".to_string(), serde_json::to_value(add_bond_cmd)?)
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, 
                format!("Unsupported command type in EditAtomData history")));
        };
        
        serializable_commands.push(SerializableEditAtomCommand {
            command_type,
            command_data,
        });
    }
    
    Ok(SerializableEditAtomData {
        history: serializable_commands,
        next_history_index: data.next_history_index,
    })
}

/// Converts a SerializableEditAtomData back to EditAtomData
/// 
/// # Returns
/// * `io::Result<EditAtomData>` - The deserialized data or an error if deserialization fails
pub fn serializable_to_edit_atom_data(serializable: &SerializableEditAtomData) -> io::Result<EditAtomData> {
    let mut commands = Vec::new();
    
    // Convert each SerializableEditAtomCommand back to a Box<dyn EditAtomCommand>
    for cmd in &serializable.history {
        let command: Box<dyn EditAtomCommand> = match cmd.command_type.as_str() {
            "select" => {
                let select_cmd: SelectCommand = serde_json::from_value(cmd.command_data.clone())?;
                Box::new(select_cmd)
            },
            "add_bond" => {
                let add_bond_cmd: AddBondCommand = serde_json::from_value(cmd.command_data.clone())?;
                Box::new(add_bond_cmd)
            },
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, 
                format!("Unknown command type: {}", cmd.command_type))),
        };
        
        commands.push(command);
    }
    
    Ok(EditAtomData {
        history: commands,
        next_history_index: serializable.next_history_index,
        model: AtomicStructure::new(),
    })
}
