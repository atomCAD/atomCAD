//! The `empty` node: a `Blueprint` bounding **no volume at all**.
//!
//! Before this node existed the only way to say "nothing" was to intersect two
//! opposing `half_space`s and rely on the result cancelling — the "empty volume
//! hack". That is fragile twice over: `csgrs` cannot represent emptiness (a
//! polygon-less mesh reads as the *universe*, see `atomcad-geo-tree`'s
//! `src/AGENTS.md`), and whether the cancellation happens at all is decided by
//! floating point. `empty` puts the emptiness in the tree instead, as
//! `GeoNode::empty_3d`, whose SDF is `f64::MAX` everywhere.
//!
//! Semantics follow from that constant with no special cases downstream:
//! `union` treats it as the identity, `intersect` as the annihilator, and
//! `diff` subtracts nothing. Its natural use is as a neutral branch — an `if`
//! or `switch` arm that contributes no geometry — and as a `materialize` region
//! that yields no atoms.
//!
//! `empty_2d.rs` is the `Geometry2D` counterpart.
//!
//! Data-free (`EmptyData {}`, the `intersect.rs` pattern): no gadget, no
//! editor, no text properties. The one input is the usual optional `structure`
//! (default diamond) that every `Blueprint` primitive carries — it is what the
//! boolean nodes compare in `BlueprintData::all_have_same_structure` and what
//! `materialize` reads downstream, so wire the same `Structure` you feed the
//! shapes it is combined with.

use crate::data_type::DataType;
use crate::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::evaluator::network_evaluator::NetworkEvaluator;
use crate::evaluator::network_evaluator::NetworkStackElement;
use crate::evaluator::network_result::Alignment;
use crate::evaluator::network_result::BlueprintData;
use crate::evaluator::network_result::NetworkResult;
use crate::node_data::{EvalOutput, NodeData};
use crate::node_network_gadget::NodeNetworkGadget;
use crate::node_type::NodeTypeCategory;
use crate::node_type::{
    NodeType, OutputPinDefinition, Parameter, generic_node_data_loader, generic_node_data_saver,
};
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use atomcad_crystolecule::structure::Structure;
use atomcad_geo_tree::GeoNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyData {}

impl NodeData for EmptyData {
    fn provide_gadget(
        &self,
        _structure_designer: &StructureDesigner,
    ) -> Option<Box<dyn NodeNetworkGadget>> {
        None
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &[NetworkStackElement<'a>],
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext,
    ) -> EvalOutput {
        let structure = match network_evaluator.evaluate_or_default(
            network_stack,
            node_id,
            registry,
            context,
            0,
            Structure::diamond(),
            NetworkResult::extract_structure,
        ) {
            Ok(value) => value,
            Err(error) => return EvalOutput::single(error),
        };

        EvalOutput::single(NetworkResult::Blueprint(BlueprintData {
            structure,
            geo_tree_root: GeoNode::empty_3d(),
            // An empty region imposes no orientation of its own, so it cannot
            // degrade alignment — same reasoning as `free_sphere`'s cutter.
            alignment: Alignment::Aligned,
            alignment_reason: None,
        }))
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(
        &self,
        _connected_input_pins: &std::collections::HashSet<String>,
    ) -> Option<String> {
        None
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        let mut m = HashMap::new();
        m.insert(
            "structure".to_string(),
            (false, Some("diamond".to_string())),
        );
        m
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "empty".to_string(),
        description: "Outputs an empty volume — a geometry containing no points at all. \
            It is the identity for `union`, cancels any `intersect` it takes part in, and \
            subtracts nothing in `diff`. Use it as the neutral branch of an `if`/`switch`, \
            or wherever a shape input must contribute nothing."
            .to_string(),
        summary: None,
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![Parameter {
            id: None,
            name: "structure".to_string(),
            data_type: DataType::Structure,
        }],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(EmptyData {}),
        node_data_saver: generic_node_data_saver::<EmptyData>,
        node_data_loader: generic_node_data_loader::<EmptyData>,
    }
}
