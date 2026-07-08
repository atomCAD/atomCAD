//! Network-wide canonicalization of `DataType::Function` values.
//!
//! Phase 1 of `doc/design_currying.md` establishes a canonical (flat) storage
//! invariant for `FunctionType`: `output_type` is never itself a `Function`.
//! Three points enforce it:
//!
//! 1. [`FunctionType::new`](crate::structure_designer::data_type::FunctionType::new)
//!    is the single in-code construction site and absorbs nested `Function`
//!    returns into `parameter_types`.
//! 2. The serde `Deserialize` impl on `FunctionType` routes through `new`, so
//!    JSON-deserialized node-data fields and record-type-def fields arrive
//!    canonical.
//! 3. The data-type string parser also routes through `new`, so pin types
//!    (stored as `data_type: String` in `SerializableNodeType` / `Parameter`)
//!    are canonical by construction.
//!
//! Together, those three cover every load path. This module is the
//! **belt-and-braces** layer: a network-wide walker that re-runs the
//! canonicalization on the already-typed in-memory `NodeNetwork` to guarantee
//! the invariant even for fixtures hand-built via struct literal (in tests)
//! or any future loader path that bypasses the serde hook.
//!
//! The walker recurses through HOF zone bodies via
//! [`walk_all_nodes_mut`](crate::structure_designer::node_network::walk_all_nodes_mut),
//! so body-internal node-data DataType fields are covered the same way as
//! top-level ones.

use std::collections::HashMap;

use crate::structure_designer::data_type::canonicalize_data_type;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network::{NodeNetwork, walk_all_nodes_mut};
use crate::structure_designer::node_type::{NodeType, PinOutputType};
use crate::structure_designer::node_type_registry::RecordTypeDef;
use crate::structure_designer::nodes::apply::ApplyData;
use crate::structure_designer::nodes::array_append::ArrayAppendData;
use crate::structure_designer::nodes::array_at::ArrayAtData;
use crate::structure_designer::nodes::array_concat::ArrayConcatData;
use crate::structure_designer::nodes::array_len::ArrayLenData;
use crate::structure_designer::nodes::closure::ClosureData;
use crate::structure_designer::nodes::collect::CollectData;
use crate::structure_designer::nodes::expr::ExprData;
use crate::structure_designer::nodes::filter::FilterData;
use crate::structure_designer::nodes::fold::FoldData;
use crate::structure_designer::nodes::foreach::ForeachData;
use crate::structure_designer::nodes::if_else::IfData;
use crate::structure_designer::nodes::map::MapData;
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::nodes::sequence::SequenceData;
use crate::structure_designer::nodes::zip_with::ZipWithData;

/// Canonicalize every `DataType::Function` reachable from `network` in place.
///
/// Walks (a) the network's signature pins (`node_type.parameters`,
/// `output_pins`, `zone_input_pins`, `zone_output_pins`), then (b) every
/// node — recursing through HOF zone bodies — and canonicalizes the DataType
/// fields stored on the few node-data variants that carry one.
///
/// A no-op for an already-canonical network (the existing fixture set is
/// already flat, so this is the common case).
pub fn canonicalize_network(network: &mut NodeNetwork) {
    canonicalize_node_type_signature(&mut network.node_type);
    walk_all_nodes_mut(network, &mut |node| {
        canonicalize_node_data(node.data.as_mut());
        // The per-node `custom_node_type` cache is recomputed from the (now
        // canonical) stored data by the next type-resolution pass; clear it
        // defensively so any stale non-canonical type isn't observable.
        //
        // SAFETY CONSTRAINT (Change 2 fallback,
        // doc/design_custom_node_type_cache_invariant.md): this leaves derived
        // nodes in the stale `None` cache state (B). It is safe ONLY because
        // `canonicalize_network` is always immediately followed by
        // `initialize_custom_node_types_for_network` (which repopulates every
        // cache with `refresh_args = false`, preserving wires positionally) —
        // canonicalize is only ever called on the `.cnnd` load path, never
        // before a `repair_all_networks` / `refresh_args = true` pass. Unlike
        // `rewrite_record_name_in_registry`, this function only receives a
        // `&mut NodeNetwork` and has no access to the registry's type maps, so
        // an in-place recompute would require threading them through every
        // caller. Should such a `canonicalize -> repair(refresh_args = true)`
        // path ever be introduced, Change 1's positional-preservation net in
        // `set_custom_node_type` now stops it from dropping wires regardless.
        node.custom_node_type = None;
    });
}

/// Canonicalize every record type def in place.
pub fn canonicalize_record_type_defs(defs: &mut HashMap<String, RecordTypeDef>) {
    for def in defs.values_mut() {
        for field in def.fields.iter_mut() {
            canonicalize_data_type(&mut field.data_type);
        }
    }
}

/// Canonicalize the DataType fields embedded in a `NodeType`'s pin
/// definitions. Used both for stored network signatures and as a helper for
/// any caller building a `NodeType` programmatically.
fn canonicalize_node_type_signature(node_type: &mut NodeType) {
    for p in node_type.parameters.iter_mut() {
        canonicalize_data_type(&mut p.data_type);
    }
    for pin in node_type.output_pins.iter_mut() {
        canonicalize_pin_output_type(&mut pin.data_type);
    }
    for pin in node_type.zone_input_pins.iter_mut() {
        canonicalize_pin_output_type(&mut pin.data_type);
    }
    for p in node_type.zone_output_pins.iter_mut() {
        canonicalize_data_type(&mut p.data_type);
    }
}

fn canonicalize_pin_output_type(t: &mut PinOutputType) {
    match t {
        PinOutputType::Fixed(d) => canonicalize_data_type(d),
        PinOutputType::SameAsInput {
            fallback_if_disconnected,
            ..
        } => {
            if let Some(d) = fallback_if_disconnected.as_mut() {
                canonicalize_data_type(d);
            }
        }
        PinOutputType::SameAsArrayElements(_) => {}
    }
}

/// Canonicalize the DataType fields stored on a single node's data.
///
/// Each branch handles one node-data variant that embeds a `DataType`. Nodes
/// not listed here either carry no DataType in their data (`NoData`,
/// primitives) or carry only schema-name strings (record nodes, `product`).
fn canonicalize_node_data(data: &mut dyn NodeData) {
    let any = data.as_any_mut();
    if let Some(d) = any.downcast_mut::<ClosureData>() {
        for t in d.type_args.iter_mut() {
            canonicalize_data_type(t);
        }
    } else if let Some(d) = any.downcast_mut::<ApplyData>() {
        for t in d.type_args.iter_mut() {
            canonicalize_data_type(t);
        }
    } else if let Some(d) = any.downcast_mut::<MapData>() {
        canonicalize_data_type(&mut d.input_type);
        canonicalize_data_type(&mut d.output_type);
    } else if let Some(d) = any.downcast_mut::<FilterData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<FoldData>() {
        canonicalize_data_type(&mut d.element_type);
        canonicalize_data_type(&mut d.accumulator_type);
    } else if let Some(d) = any.downcast_mut::<ForeachData>() {
        canonicalize_data_type(&mut d.input_type);
    } else if let Some(d) = any.downcast_mut::<ZipWithData>() {
        for lane in d.lanes.iter_mut() {
            canonicalize_data_type(&mut lane.data_type);
        }
        canonicalize_data_type(&mut d.output_type);
    } else if let Some(d) = any.downcast_mut::<ParameterData>() {
        canonicalize_data_type(&mut d.data_type);
        if d.data_type_str.is_some() {
            d.data_type_str = Some(d.data_type.to_string());
        }
    } else if let Some(d) = any.downcast_mut::<ExprData>() {
        for p in d.parameters.iter_mut() {
            canonicalize_data_type(&mut p.data_type);
            if p.data_type_str.is_some() {
                p.data_type_str = Some(p.data_type.to_string());
            }
        }
        if let Some(out) = d.output_type.as_mut() {
            canonicalize_data_type(out);
        }
    } else if let Some(d) = any.downcast_mut::<SequenceData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<ArrayAtData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<ArrayAppendData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<ArrayConcatData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<ArrayLenData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<CollectData>() {
        canonicalize_data_type(&mut d.element_type);
    } else if let Some(d) = any.downcast_mut::<IfData>() {
        canonicalize_data_type(&mut d.value_type);
    }
    // Record* / ProductData store schema names (Strings), not DataTypes —
    // nothing to canonicalize.
}
