//! Tool-selection adapters: `AtomEditData` / `EditAtomData` ⇄ the Dart-facing
//! tool enums.
//!
//! These four functions used to be `get_active_tool` / `set_active_tool`
//! methods on the two node data structs, and they were the reason those two
//! files imported from `api/`. They are pure projections between a *stateful*
//! domain tool enum (`AtomEditTool::AddBond(AddBondToolState { … })`) and a
//! *flat* Dart-facing discriminant (`APIAtomEditTool::AddBond`) — view-builders
//! in everything but location — so D10.1 moves them up rather than twinning the
//! api enums. The domain tool enums stay exactly where they are.
//!
//! Like `view_builders`, this module is deliberately **not** in
//! `flutter_rust_bridge.yaml`'s `rust_input`: its `pub fn`s take domain types,
//! and every `pub fn` in a scanned namespace becomes a Dart API.

use crate::api::structure_designer::structure_designer_api_types::{
    APIAtomEditTool, APIEditAtomTool,
};
use crate::structure_designer::nodes::atom_edit::atom_edit::{
    AddAtomToolState, AddBondInteractionState, AddBondToolState, AtomEditData, AtomEditTool,
    DefaultToolInteractionState, DefaultToolState, GuidelineTool,
};
use crate::structure_designer::nodes::edit_atom::edit_atom::{
    AddAtomToolState as EditAtomAddAtomToolState, AddBondToolState as EditAtomAddBondToolState,
    DefaultToolState as EditAtomDefaultToolState, EditAtomData, EditAtomTool,
};

pub fn atom_edit_active_tool(data: &AtomEditData) -> APIAtomEditTool {
    match &data.active_tool {
        AtomEditTool::Default(_) => APIAtomEditTool::Default,
        AtomEditTool::AddAtom(_) => APIAtomEditTool::AddAtom,
        AtomEditTool::AddBond(_) => APIAtomEditTool::AddBond,
        AtomEditTool::Guideline(_) => APIAtomEditTool::Guideline,
    }
}

pub fn set_atom_edit_active_tool(data: &mut AtomEditData, api_tool: APIAtomEditTool) {
    // Reset interaction state if switching away from Default tool mid-interaction
    if let AtomEditTool::Default(ref mut state) = data.active_tool {
        state.interaction_state = DefaultToolInteractionState::Idle;
    }
    // Cancel guided placement if switching away from AddAtom tool
    // (no special action needed — the new tool state replaces the old one)
    data.active_tool = match api_tool {
        APIAtomEditTool::Default => AtomEditTool::Default(DefaultToolState {
            interaction_state: DefaultToolInteractionState::default(),
            show_gadget: false,
        }),
        APIAtomEditTool::AddAtom => AtomEditTool::AddAtom(AddAtomToolState::Idle),
        APIAtomEditTool::AddBond => AtomEditTool::AddBond(AddBondToolState {
            bond_order: atomcad_crystolecule::atomic_structure::BOND_SINGLE,
            interaction_state: AddBondInteractionState::default(),
            last_atom_id: None,
        }),
        APIAtomEditTool::Guideline => {
            // Enter the Guideline tool in `Define` with an empty defining set.
            // Clear the shared selection so no stale highlight leaks in — the
            // tool drives its own tool-local highlight (issue #368).
            data.selection.clear();
            AtomEditTool::Guideline(GuidelineTool::new())
        }
    }
}

pub fn edit_atom_active_tool(data: &EditAtomData) -> APIEditAtomTool {
    match &data.active_tool {
        EditAtomTool::Default(_) => APIEditAtomTool::Default,
        EditAtomTool::AddAtom(_) => APIEditAtomTool::AddAtom,
        EditAtomTool::AddBond(_) => APIEditAtomTool::AddBond,
    }
}

pub fn set_edit_atom_active_tool(data: &mut EditAtomData, api_tool: APIEditAtomTool) {
    data.active_tool = match api_tool {
        APIEditAtomTool::Default => EditAtomTool::Default(EditAtomDefaultToolState {}),
        APIEditAtomTool::AddAtom => EditAtomTool::AddAtom(EditAtomAddAtomToolState {}),
        APIEditAtomTool::AddBond => {
            EditAtomTool::AddBond(EditAtomAddBondToolState { last_atom_id: None })
        }
    }
}
