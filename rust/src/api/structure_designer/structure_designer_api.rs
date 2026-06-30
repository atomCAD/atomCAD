use super::structure_designer_api_types::APIAlignment;
use super::structure_designer_api_types::APIApplyData;
use super::structure_designer_api_types::APIArgumentKind;
use super::structure_designer_api_types::APIArrayAtData;
use super::structure_designer_api_types::APIAtomReplaceData;
use super::structure_designer_api_types::APIAtomReplaceRule;
use super::structure_designer_api_types::APICandidateNode;
use super::structure_designer_api_types::APICircleData;
use super::structure_designer_api_types::APIClosureData;
use super::structure_designer_api_types::APIClosureKind;
use super::structure_designer_api_types::APICollapseMode;
use super::structure_designer_api_types::APICollectData;
use super::structure_designer_api_types::APICommentData;
use super::structure_designer_api_types::APICompatibilityReport;
use super::structure_designer_api_types::APIDataType;
use super::structure_designer_api_types::APIDerivedShapeView;
use super::structure_designer_api_types::APIDragSource;
use super::structure_designer_api_types::APIExecuteResult;
use super::structure_designer_api_types::APIExportXYZData;
use super::structure_designer_api_types::APIExprData;
use super::structure_designer_api_types::APIExprParameter;
use super::structure_designer_api_types::APIExtrudeData;
use super::structure_designer_api_types::APIFilterData;
use super::structure_designer_api_types::APIFoldData;
use super::structure_designer_api_types::APIForeachData;
use super::structure_designer_api_types::APIHalfPlaneData;
use super::structure_designer_api_types::APIHoveredAtomInfo;
use super::structure_designer_api_types::APIImportCIFData;
use super::structure_designer_api_types::APIImportXYZData;
use super::structure_designer_api_types::APIInferBondsData;
use super::structure_designer_api_types::APILiteralField;
use super::structure_designer_api_types::APILiteralValue;
use super::structure_designer_api_types::APIMapData;
use super::structure_designer_api_types::APIMaterializeData;
use super::structure_designer_api_types::APIMeasurement;
use super::structure_designer_api_types::APIMotifData;
use super::structure_designer_api_types::APIMotifParameterInfo;
use super::structure_designer_api_types::APIMotifSubData;
use super::structure_designer_api_types::APINamespaceRenamePreview;
use super::structure_designer_api_types::APINodeEvaluationResult;
use super::structure_designer_api_types::APIParameterData;
use super::structure_designer_api_types::APIPatchBuildData;
use super::structure_designer_api_types::APIPatchLatticeFillData;
use super::structure_designer_api_types::APIPlaneTilingVectorsData;
use super::structure_designer_api_types::APIPrintData;
use super::structure_designer_api_types::APIPrintLogEntry;
use super::structure_designer_api_types::APIRectData;
use super::structure_designer_api_types::APIRegPolyData;
use super::structure_designer_api_types::APISequenceData;
use super::structure_designer_api_types::APISimpleParamType;
use super::structure_designer_api_types::APISourcePin;
use super::structure_designer_api_types::APITextEditResult;
use super::structure_designer_api_types::APITextError;
use super::structure_designer_api_types::APIViewportPickResult;
use super::structure_designer_api_types::OutputPinView;
use super::structure_designer_api_types::ZoneView;
use super::structure_designer_preferences::StructureDesignerPreferences;
use crate::api::api_common::apply_camera_settings;
use crate::api::api_common::from_api_ivec2;
use crate::api::api_common::from_api_ivec3;
use crate::api::api_common::from_api_vec2;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::refresh_structure_designer_auto;
use crate::api::api_common::to_api_ivec2;
use crate::api::api_common::to_api_ivec3;
use crate::api::api_common::to_api_vec2;
use crate::api::api_common::to_api_vec3;
use crate::api::api_common::with_cad_instance;
use crate::api::api_common::with_cad_instance_or;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::common_api_types::APIIVec2;
use crate::api::common_api_types::APIIVec3;
use crate::api::common_api_types::APIResult;
use crate::api::common_api_types::APIVec2;
use crate::api::common_api_types::APIVec3;
use crate::api::structure_designer::structure_designer_api_types::APIApplyDiffData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomComposeDiffData;
use crate::api::structure_designer::structure_designer_api_types::APIAtomCutData;
use crate::api::structure_designer::structure_designer_api_types::APIBoolData;
use crate::api::structure_designer::structure_designer_api_types::APICuboidData;
use crate::api::structure_designer::structure_designer_api_types::APIDiffStats;
use crate::api::structure_designer::structure_designer_api_types::APIDrawingPlaneData;
use crate::api::structure_designer::structure_designer_api_types::APIEditAtomData;
use crate::api::structure_designer::structure_designer_api_types::APIFloatData;
use crate::api::structure_designer::structure_designer_api_types::APIFreeMoveData;
use crate::api::structure_designer::structure_designer_api_types::APIFreeRotData;
use crate::api::structure_designer::structure_designer_api_types::APIGeoTransData;
use crate::api::structure_designer::structure_designer_api_types::APIHalfSpaceData;
use crate::api::structure_designer::structure_designer_api_types::APIIMat2ColsData;
use crate::api::structure_designer::structure_designer_api_types::APIIMat2DiagData;
use crate::api::structure_designer::structure_designer_api_types::APIIMat2RowsData;
use crate::api::structure_designer::structure_designer_api_types::APIIMat3ColsData;
use crate::api::structure_designer::structure_designer_api_types::APIIMat3DiagData;
use crate::api::structure_designer::structure_designer_api_types::APIIMat3RowsData;
use crate::api::structure_designer::structure_designer_api_types::APIIVec2Data;
use crate::api::structure_designer::structure_designer_api_types::APIIVec3Data;
use crate::api::structure_designer::structure_designer_api_types::APIIntData;
use crate::api::structure_designer::structure_designer_api_types::APILatticeVecsData;
use crate::api::structure_designer::structure_designer_api_types::APIMat3ColsData;
use crate::api::structure_designer::structure_designer_api_types::APIMat3DiagData;
use crate::api::structure_designer::structure_designer_api_types::APIMat3RowsData;
use crate::api::structure_designer::structure_designer_api_types::APIRangeData;
use crate::api::structure_designer::structure_designer_api_types::APIRecordSchemaData;
use crate::api::structure_designer::structure_designer_api_types::APIRecordTypeDef;
use crate::api::structure_designer::structure_designer_api_types::APIRecordTypeField;
use crate::api::structure_designer::structure_designer_api_types::APISphereData;
use crate::api::structure_designer::structure_designer_api_types::APIStringData;
use crate::api::structure_designer::structure_designer_api_types::APISupercellData;
use crate::api::structure_designer::structure_designer_api_types::APIVec2Data;
use crate::api::structure_designer::structure_designer_api_types::APIVec3Data;
use crate::api::structure_designer::structure_designer_api_types::InputPinView;
use crate::api::structure_designer::structure_designer_api_types::NodeView;
use crate::api::structure_designer::structure_designer_api_types::WireView;
use crate::api::structure_designer::structure_designer_api_types::{
    APIAtomEditData, APIParameterElement,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APIDataTypeBase, APINetworkWithValidationErrors, APINodeCategoryView, NodeNetworkView,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APILatticeSymopData, APIRotationalSymmetry, APIStructureMoveData, APIStructureRotData,
};
use crate::crystolecule::unit_cell_symmetries::{
    CrystalSystem, analyze_unit_cell_complete, classify_crystal_system,
};
use crate::structure_designer::cli_runner;
use crate::structure_designer::data_type::{DataType, RecordType};
use crate::structure_designer::evaluator::network_result::{
    Alignment, NetworkResult, dmat3_to_rows,
};
use crate::structure_designer::layout;
use crate::structure_designer::node_data::CustomNodeData;
use crate::structure_designer::nodes::apply::ApplyData;
use crate::structure_designer::nodes::apply_diff::ApplyDiffData;
use crate::structure_designer::nodes::array_at::ArrayAtData;
use crate::structure_designer::nodes::atom_composediff::AtomComposeDiffData;
use crate::structure_designer::nodes::atom_cut::AtomCutData;
use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditData;
use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditEvalCache;
use crate::structure_designer::nodes::atom_edit::atom_edit::AtomEditTool;
use crate::structure_designer::nodes::atom_replace::AtomReplaceData;
use crate::structure_designer::nodes::bool::BoolData;
use crate::structure_designer::nodes::circle::CircleData;
use crate::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use crate::structure_designer::nodes::collect::CollectData;
use crate::structure_designer::nodes::comment::CommentData;
use crate::structure_designer::nodes::cuboid::CuboidData;
use crate::structure_designer::nodes::drawing_plane::DrawingPlaneData;
use crate::structure_designer::nodes::drawing_plane::DrawingPlaneEvalCache;
use crate::structure_designer::nodes::edit_atom::edit_atom::EditAtomData;
use crate::structure_designer::nodes::edit_atom::edit_atom::EditAtomTool;
use crate::structure_designer::nodes::export_xyz::ExportXYZData;
use crate::structure_designer::nodes::expr::ExprData;
use crate::structure_designer::nodes::extrude::ExtrudeData;
use crate::structure_designer::nodes::extrude::ExtrudeEvalCache;
use crate::structure_designer::nodes::filter::FilterData;
use crate::structure_designer::nodes::float::FloatData;
use crate::structure_designer::nodes::fold::FoldData;
use crate::structure_designer::nodes::foreach::ForeachData;
use crate::structure_designer::nodes::free_move::FreeMoveData;
use crate::structure_designer::nodes::free_rot::FreeRotData;
use crate::structure_designer::nodes::geo_trans::GeoTransData;
use crate::structure_designer::nodes::half_plane::HalfPlaneData;
use crate::structure_designer::nodes::half_space::HalfSpaceData;
use crate::structure_designer::nodes::imat2_cols::IMat2ColsData;
use crate::structure_designer::nodes::imat2_diag::IMat2DiagData;
use crate::structure_designer::nodes::imat2_rows::IMat2RowsData;
use crate::structure_designer::nodes::imat3_cols::IMat3ColsData;
use crate::structure_designer::nodes::imat3_diag::IMat3DiagData;
use crate::structure_designer::nodes::imat3_rows::IMat3RowsData;
use crate::structure_designer::nodes::import_cif::ImportCifData;
use crate::structure_designer::nodes::import_xyz::ImportXYZData;
use crate::structure_designer::nodes::infer_bonds::InferBondsData;
use crate::structure_designer::nodes::int::IntData;
use crate::structure_designer::nodes::ivec2::IVec2Data;
use crate::structure_designer::nodes::ivec3::IVec3Data;
use crate::structure_designer::nodes::lattice_symop::{LatticeSymopData, LatticeSymopEvalCache};
use crate::structure_designer::nodes::lattice_vecs::LatticeVecsData;
use crate::structure_designer::nodes::map::MapData;
use crate::structure_designer::nodes::mat3_cols::Mat3ColsData;
use crate::structure_designer::nodes::mat3_diag::Mat3DiagData;
use crate::structure_designer::nodes::mat3_rows::Mat3RowsData;
use crate::structure_designer::nodes::materialize::MaterializeData;
use crate::structure_designer::nodes::motif::MotifData;
use crate::structure_designer::nodes::motif_sub::MotifSubData;
use crate::structure_designer::nodes::parameter::ParameterData;
use crate::structure_designer::nodes::patch_build::PatchBuildData;
use crate::structure_designer::nodes::patch_latticefill::{
    CompatibilityReport, PatchLatticeFillData,
};
use crate::structure_designer::nodes::plane_tiling_vectors::PlaneTilingVectorsData;
use crate::structure_designer::nodes::print::PrintData;
use crate::structure_designer::nodes::product::ProductData;
use crate::structure_designer::nodes::range::RangeData;
use crate::structure_designer::nodes::record_construct::RecordConstructData;
use crate::structure_designer::nodes::record_destructure::RecordDestructureData;
use crate::structure_designer::nodes::rect::RectData;
use crate::structure_designer::nodes::reg_poly::RegPolyData;
use crate::structure_designer::nodes::sequence::SequenceData;
use crate::structure_designer::nodes::sphere::SphereData;
use crate::structure_designer::nodes::string::StringData;
use crate::structure_designer::nodes::structure_move::StructureMoveData;
use crate::structure_designer::nodes::structure_rot::{StructureRotData, StructureRotEvalCache};
use crate::structure_designer::nodes::supercell::SupercellData;
use crate::structure_designer::nodes::vec2::Vec2Data;
use crate::structure_designer::nodes::vec3::Vec3Data;
use crate::structure_designer::text_format::TextValue;
use glam::{DVec2, DVec3, IVec2, IVec3};
use std::collections::HashMap;

fn alignment_to_api(alignment: Alignment) -> APIAlignment {
    match alignment {
        Alignment::Aligned => APIAlignment::Aligned,
        Alignment::MotifUnaligned => APIAlignment::MotifUnaligned,
        Alignment::LatticeUnaligned => APIAlignment::LatticeUnaligned,
    }
}

#[flutter_rust_bridge::frb(ignore)]
pub fn api_data_type_to_data_type(api_data_type: &APIDataType) -> Result<DataType, String> {
    let base_type = match api_data_type.data_type_base {
        APIDataTypeBase::None => DataType::None,
        APIDataTypeBase::Bool => DataType::Bool,
        APIDataTypeBase::String => DataType::String,
        APIDataTypeBase::Int => DataType::Int,
        APIDataTypeBase::Float => DataType::Float,
        APIDataTypeBase::Vec2 => DataType::Vec2,
        APIDataTypeBase::Vec3 => DataType::Vec3,
        APIDataTypeBase::IVec2 => DataType::IVec2,
        APIDataTypeBase::IVec3 => DataType::IVec3,
        APIDataTypeBase::IMat2 => DataType::IMat2,
        APIDataTypeBase::IMat3 => DataType::IMat3,
        APIDataTypeBase::Mat3 => DataType::Mat3,
        APIDataTypeBase::LatticeVecs => DataType::LatticeVecs,
        APIDataTypeBase::DrawingPlane => DataType::DrawingPlane,
        APIDataTypeBase::Geometry2D => DataType::Geometry2D,
        APIDataTypeBase::Blueprint => DataType::Blueprint,
        APIDataTypeBase::HasAtoms => DataType::HasAtoms,
        APIDataTypeBase::Crystal => DataType::Crystal,
        APIDataTypeBase::Molecule => DataType::Molecule,
        APIDataTypeBase::HasStructure => DataType::HasStructure,
        APIDataTypeBase::HasFreeLinOps => DataType::HasFreeLinOps,
        APIDataTypeBase::Motif => DataType::Motif,
        APIDataTypeBase::Structure => DataType::Structure,
        APIDataTypeBase::Unit => DataType::Unit,
        APIDataTypeBase::Record => {
            // Empty name is intentionally accepted: a freshly-placed record
            // node with no schema yet round-trips through the API with
            // `custom_data_type: Some("")`. The dangling `Named("")` reference
            // is surfaced by validation, not by this conversion.
            let name = api_data_type
                .custom_data_type
                .clone()
                .ok_or_else(|| "Record data type name is missing".to_string())?;
            let record = DataType::Record(RecordType::Named(name));
            return Ok(if api_data_type.array {
                DataType::Array(Box::new(record))
            } else {
                record
            });
        }
        APIDataTypeBase::Iter => {
            let element = api_data_type
                .children
                .first()
                .ok_or_else(|| "Iter type requires one child".to_string())?;
            let inner = api_data_type_to_data_type(element)?;
            let base = DataType::Iterator(Box::new(inner));
            return Ok(if api_data_type.array {
                DataType::Array(Box::new(base))
            } else {
                base
            });
        }
        APIDataTypeBase::Optional => {
            let element = api_data_type
                .children
                .first()
                .ok_or_else(|| "Optional type requires one child".to_string())?;
            let inner = api_data_type_to_data_type(element)?;
            // Reject the ill-formed shapes (Optional[Optional]/Iter/Unit/None)
            // at this construction site, mirroring the text parser and registry
            // validation. See `doc/design_optional_type.md` §3.
            crate::structure_designer::data_type::validate_optional_inner(&inner)?;
            let base = DataType::Optional(Box::new(inner));
            return Ok(if api_data_type.array {
                DataType::Array(Box::new(base))
            } else {
                base
            });
        }
        APIDataTypeBase::Function => {
            if api_data_type.children.is_empty() {
                return Err("Function type requires at least one child (the return type)".into());
            }
            let n = api_data_type.children.len() - 1;
            let parameter_types: Result<Vec<_>, _> = api_data_type.children[..n]
                .iter()
                .map(api_data_type_to_data_type)
                .collect();
            let output_type = api_data_type_to_data_type(&api_data_type.children[n])?;
            let base = DataType::Function(crate::structure_designer::data_type::FunctionType::new(
                parameter_types?,
                output_type,
            ));
            return Ok(if api_data_type.array {
                DataType::Array(Box::new(base))
            } else {
                base
            });
        }
        APIDataTypeBase::Custom => {
            if let Some(custom_str) = &api_data_type.custom_data_type {
                return DataType::from_string(custom_str);
            } else {
                return Err("Custom data type string is missing".to_string());
            }
        }
    };

    if api_data_type.array {
        Ok(DataType::Array(Box::new(base_type)))
    } else {
        Ok(base_type)
    }
}

#[flutter_rust_bridge::frb(ignore)]
pub fn data_type_to_api_data_type(data_type: &DataType) -> APIDataType {
    let (base_data_type, is_array) = if let DataType::Array(element_type) = data_type {
        (element_type.as_ref(), true)
    } else {
        (data_type, false)
    };

    // Named records get a first-class `Record` base with the def name in
    // `custom_data_type`. Anonymous records fall through to `Custom` (no
    // UI in v1; only the expression language can produce them).
    if let DataType::Record(RecordType::Named(name)) = base_data_type {
        return APIDataType {
            data_type_base: APIDataTypeBase::Record,
            custom_data_type: Some(name.clone()),
            array: is_array,
            children: vec![],
        };
    }

    // Iter[T] and Function((p..) -> R) get first-class structural variants;
    // they must run *before* the `_ => Custom` fallback below so the UI gets
    // the structural form rather than a free-form text string. See
    // `doc/design_structural_function_and_iter_types.md`.
    if let DataType::Iterator(element) = base_data_type {
        return APIDataType {
            data_type_base: APIDataTypeBase::Iter,
            custom_data_type: None,
            array: is_array,
            children: vec![data_type_to_api_data_type(element.as_ref())],
        };
    }
    // Optional[T] is a record-field modifier; it surfaces to the schema editor
    // as a first-class structural variant (one child). See
    // `doc/design_optional_type.md` §7.
    if let DataType::Optional(inner) = base_data_type {
        return APIDataType {
            data_type_base: APIDataTypeBase::Optional,
            custom_data_type: None,
            array: is_array,
            children: vec![data_type_to_api_data_type(inner.as_ref())],
        };
    }
    if let DataType::Function(func) = base_data_type {
        let mut children: Vec<APIDataType> = func
            .parameter_types
            .iter()
            .map(data_type_to_api_data_type)
            .collect();
        children.push(data_type_to_api_data_type(func.output_type.as_ref()));
        return APIDataType {
            data_type_base: APIDataTypeBase::Function,
            custom_data_type: None,
            array: is_array,
            children,
        };
    }

    let data_type_base = match base_data_type {
        DataType::None => APIDataTypeBase::None,
        DataType::Bool => APIDataTypeBase::Bool,
        DataType::String => APIDataTypeBase::String,
        DataType::Int => APIDataTypeBase::Int,
        DataType::Float => APIDataTypeBase::Float,
        DataType::Vec2 => APIDataTypeBase::Vec2,
        DataType::Vec3 => APIDataTypeBase::Vec3,
        DataType::IVec2 => APIDataTypeBase::IVec2,
        DataType::IVec3 => APIDataTypeBase::IVec3,
        DataType::IMat2 => APIDataTypeBase::IMat2,
        DataType::IMat3 => APIDataTypeBase::IMat3,
        DataType::Mat3 => APIDataTypeBase::Mat3,
        DataType::LatticeVecs => APIDataTypeBase::LatticeVecs,
        DataType::DrawingPlane => APIDataTypeBase::DrawingPlane,
        DataType::Geometry2D => APIDataTypeBase::Geometry2D,
        DataType::Blueprint => APIDataTypeBase::Blueprint,
        DataType::HasAtoms => APIDataTypeBase::HasAtoms,
        DataType::Crystal => APIDataTypeBase::Crystal,
        DataType::Molecule => APIDataTypeBase::Molecule,
        DataType::HasStructure => APIDataTypeBase::HasStructure,
        DataType::HasFreeLinOps => APIDataTypeBase::HasFreeLinOps,
        DataType::Motif => APIDataTypeBase::Motif,
        DataType::Structure => APIDataTypeBase::Structure,
        DataType::Unit => APIDataTypeBase::Unit,
        _ => APIDataTypeBase::Custom, // All other types are considered custom
    };

    let custom_data_type = if let APIDataTypeBase::Custom = data_type_base {
        Some(data_type.to_string())
    } else {
        None
    };

    APIDataType {
        data_type_base,
        custom_data_type,
        array: is_array,
        children: vec![],
    }
}

fn api_closure_kind_to_closure_kind(kind: &APIClosureKind) -> ClosureKind {
    match kind {
        APIClosureKind::Map => ClosureKind::Map,
        APIClosureKind::Filter => ClosureKind::Filter,
        APIClosureKind::Fold => ClosureKind::Fold,
        APIClosureKind::Foreach => ClosureKind::Foreach,
        APIClosureKind::Custom => ClosureKind::Custom,
    }
}

fn closure_kind_to_api_closure_kind(kind: &ClosureKind) -> APIClosureKind {
    match kind {
        ClosureKind::Map => APIClosureKind::Map,
        ClosureKind::Filter => APIClosureKind::Filter,
        ClosureKind::Fold => APIClosureKind::Fold,
        ClosureKind::Foreach => APIClosureKind::Foreach,
        ClosureKind::Custom => APIClosureKind::Custom,
    }
}

/// Convert a stored `Vec<DataType>` of closure type-args to the API form.
fn type_args_to_api(type_args: &[DataType]) -> Vec<APIDataType> {
    type_args.iter().map(data_type_to_api_data_type).collect()
}

/// Convert API closure type-args back to `Vec<DataType>`, defaulting any
/// unparseable entry to `DataType::None` (the same fallback the per-type
/// setters use for transient editing states).
fn api_to_type_args(type_args: &[APIDataType]) -> Vec<DataType> {
    type_args
        .iter()
        .map(|t| api_data_type_to_data_type(t).unwrap_or(DataType::None))
        .collect()
}

/// Build a [`ZoneView`] for an HOF node's body.
///
/// Resolves the zone-input / zone-output pin definitions against the node's
/// `NodeType`, surfaces the stored body dimensions, and recursively builds
/// `NodeView` / `WireView` for the body's contents (Phase U4). Returns `None`
/// for non-HOF nodes (whose `Node.zone` is `None`).
///
/// Cross-scope wires (captures, iteration-value references, body-return wires)
/// are deferred to U5 — U4 surfaces only wires confined to the body's scope.
fn build_zone_view(
    node: &crate::structure_designer::node_network::Node,
    node_type: &crate::structure_designer::node_type::NodeType,
    cad_instance: &crate::api::api_common::CADInstance,
    scope_path: &[u64],
) -> Option<ZoneView> {
    if !node_type.has_zone() {
        return None;
    }
    let body = node.zone.as_ref()?;

    // From the body's perspective zone-input pins are sources (carry values
    // into the body), so we reuse the OutputPinView shape. The body-side
    // resolution doesn't have an upstream wire to chase, so `resolved_data_type`
    // stays None and the declared type is the rendered one.
    let zone_input_pins: Vec<OutputPinView> = node_type
        .zone_input_pins
        .iter()
        .enumerate()
        .map(|(i, pin_def)| OutputPinView {
            name: pin_def.name.clone(),
            data_type: pin_def.data_type.to_string(),
            resolved_data_type: None,
            resolved_via_fallback: false,
            index: i as i32,
            alignment: None,
            alignment_reason: None,
        })
        .collect();

    // Zone-output pins are destinations from the body's perspective —
    // reuse the InputPinView shape.
    let zone_output_pins: Vec<InputPinView> = node_type
        .zone_output_pins
        .iter()
        .map(|param| InputPinView {
            name: param.name.clone(),
            data_type: param.data_type.to_string(),
            multi: param.data_type.is_array(),
            // Zone-output (body destination) pins are not drag-from sources for
            // the add-node popup, so no hint is needed.
            drag_hint_type: None,
        })
        .collect();

    // Body nodes live one scope deeper than this HOF: their scope path is this
    // HOF's scope path plus this HOF's id.
    let mut body_scope_path = scope_path.to_vec();
    body_scope_path.push(node.id);
    let mut nodes = HashMap::new();
    for (body_node_id, body_node) in body.nodes.iter() {
        if let Some(view) = build_node_view(body_node, body, cad_instance, &body_scope_path) {
            nodes.insert(*body_node_id, view);
        }
    }

    let mut wires = build_wires_for_network(body);
    // Body-return wires live on the HOF's `zone_output_arguments`, not on a
    // body-internal node's arguments. Surface them with the HOF as the
    // destination so the Flutter wire renderer draws them from the body
    // node's output pin to the HOF's zone-output (inner-right) pin.
    for (zone_output_index, argument) in node.zone_output_arguments.iter().enumerate() {
        for (source_node_id, source_output_pin_index) in argument.iter_source_pins() {
            wires.push(WireView {
                source_node_id,
                source_output_pin_index,
                dest_node_id: node.id,
                dest_param_index: zone_output_index,
                // Zone-output wires don't surface a per-wire `selected` flag
                // yet — they're rendered as part of the body but selection
                // for zone-output wires is a U5 polish item.
                selected: false,
                destination_argument_kind: APIArgumentKind::ZoneOutput,
                source_pin: APISourcePin::NodeOutput {
                    pin_index: source_output_pin_index,
                },
                source_scope_depth: 0,
            });
        }
    }

    Some(ZoneView {
        zone_input_pins,
        zone_output_pins,
        nodes,
        wires,
        stored_width: node.body_width,
        stored_height: node.body_height,
        collapse_mode: node.collapse_mode.into(),
        collapsed: crate::structure_designer::node_network::resolve_body_collapsed(node, node_type),
        collapsable: crate::structure_designer::node_network::collapsable_type_name(
            &node.node_type_name,
        ),
    })
}

/// Build a [`NodeView`] for a single node living in [`node_network`].
///
/// Pulled out of `get_node_network_view` so [`build_zone_view`] can call it
/// recursively for nodes inside an HOF's body (Phase U4). Returns `None` if
/// the registry can't resolve the node's type (kept as `Option` to mirror the
/// existing top-level path's behavior).
fn build_node_view(
    node: &crate::structure_designer::node_network::Node,
    node_network: &crate::structure_designer::node_network::NodeNetwork,
    cad_instance: &crate::api::api_common::CADInstance,
    scope_path: &[u64],
) -> Option<NodeView> {
    let node_type = cad_instance
        .structure_designer
        .node_type_registry
        .get_node_type_for_node(node)?;

    let num_of_params = node_type.parameters.len();
    let mut input_pins: Vec<InputPinView> = Vec::with_capacity(num_of_params);
    for i in 0..num_of_params {
        let param = &node_type.parameters[i];
        let data_type = &cad_instance
            .structure_designer
            .node_type_registry
            .get_node_param_data_type(node, i);
        input_pins.push(InputPinView {
            name: param.name.clone(),
            data_type: data_type.to_string(),
            multi: data_type.is_array(),
            // Lossy-declared pins (e.g. `map.f`'s `AnyFunction`) expose a
            // concrete drag hint so a wire dragged off them infers the new
            // node's types fully. See `doc/design_drag_aware_add_node.md`.
            drag_hint_type: node
                .data
                .drag_hint_for_input_pin(i)
                .map(|dt| dt.to_string()),
        });
    }

    let mut error_messages = Vec::new();
    for validation_error in &node_network.validation_errors {
        if validation_error.node_id == Some(node.id) {
            error_messages.push(validation_error.error_text.clone());
        }
    }
    if node_network.validation_errors.is_empty()
        && let Some(eval_error) = cad_instance
            .structure_designer
            .last_generated_structure_designer_scene
            .get_node_error(scope_path, node.id)
    {
        error_messages.push(eval_error);
    }
    let error = if error_messages.is_empty() {
        None
    } else {
        Some(error_messages.join("\n"))
    };

    let output_pin_strings = cad_instance
        .structure_designer
        .last_generated_structure_designer_scene
        .get_node_output_strings(scope_path, node.id)
        .unwrap_or_default();

    let mut connected_input_pins = std::collections::HashSet::new();
    for (param_index, argument) in node.arguments.iter().enumerate() {
        if !argument.is_empty() && param_index < node_type.parameters.len() {
            connected_input_pins.insert(node_type.parameters[param_index].name.clone());
        }
    }
    let subtitle = node.data.get_subtitle(&connected_input_pins);

    let (comment_label, comment_text, comment_width, comment_height) =
        if let Some(comment_data) = node.data.as_any_ref().downcast_ref::<CommentData>() {
            (
                Some(comment_data.label.clone()),
                Some(comment_data.text.clone()),
                Some(comment_data.width),
                Some(comment_data.height),
            )
        } else {
            (None, None, None, None)
        };

    let closure_custom_label = node
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .and_then(|d| d.custom_label.clone());

    // Enclosing-zone chain for `node_network`, so polymorphic output pins fed
    // by a body's delayed-argument (zone-input) pin resolve to the concrete
    // element type rather than dead-ending. Empty for a top-level network.
    let (scope_ancestors, scope_hof_ids) = cad_instance
        .structure_designer
        .get_scope_ancestors(scope_path)
        .unwrap_or_default();

    let output_type = node_type.output_type().clone();
    // The `-1` (function) pin's type is wiring-aware
    // (`doc/design_node_function_pin_captures.md`): its parameters are the
    // node's *unwired* input pins, with wired inputs frozen as captures. Route
    // the displayed/hover type through the same `resolve_output_type(-1)` path
    // the validator and connect-gate use, so the captured (wired) pins drop out
    // of the advertised signature. Fall back to the all-parameters declaration
    // if the wiring-aware type can't resolve (e.g. polymorphic pin 0).
    let function_type = cad_instance
        .structure_designer
        .node_type_registry
        .resolve_output_type_scoped(node, node_network, -1, &scope_ancestors, &scope_hof_ids)
        .unwrap_or_else(|| node_type.get_function_type());

    let scene_node_data = cad_instance
        .structure_designer
        .last_generated_structure_designer_scene
        .node_data
        .get(&node.id);

    let output_pins: Vec<OutputPinView> = node_type
        .output_pins
        .iter()
        .enumerate()
        .map(|(i, pin_def)| {
            let needs_resolution = match &pin_def.data_type {
                crate::structure_designer::node_type::PinOutputType::Fixed(t) => t.is_abstract(),
                _ => true,
            };
            let (resolved_data_type, resolved_via_fallback) = if needs_resolution {
                match cad_instance
                    .structure_designer
                    .node_type_registry
                    .resolve_output_type_detailed_scoped(
                        node,
                        node_network,
                        i as i32,
                        &scope_ancestors,
                        &scope_hof_ids,
                    ) {
                    Some(r) => (Some(r.data_type.to_string()), r.via_fallback),
                    None => (None, false),
                }
            } else {
                (None, false)
            };
            let pin_output = scene_node_data
                .and_then(|d| d.pin_outputs.iter().find(|p| p.pin_index == i as i32));
            let alignment = pin_output.and_then(|p| p.alignment).map(alignment_to_api);
            let alignment_reason = pin_output.and_then(|p| p.alignment_reason.clone());
            OutputPinView {
                name: pin_def.name.clone(),
                data_type: pin_def.data_type.to_string(),
                resolved_data_type,
                resolved_via_fallback,
                index: i as i32,
                alignment,
                alignment_reason,
            }
        })
        .collect();

    let displayed_pins: Vec<i32> = node_network
        .get_displayed_pins(node.id)
        .map(|pins| {
            let mut sorted: Vec<i32> = pins.iter().copied().collect();
            sorted.sort();
            sorted
        })
        .unwrap_or_default();

    let zone = build_zone_view(node, node_type, cad_instance, scope_path);

    let derived_shape = build_derived_shape_view(node, node_network, cad_instance);

    Some(NodeView {
        id: node.id,
        node_type_name: node.node_type_name.clone(),
        custom_name: node.custom_name.clone(),
        position: to_api_vec2(&node.position),
        input_pins,
        output_type: output_type.to_string(),
        output_pins,
        displayed_pins,
        function_type: function_type.to_string(),
        function_pin_consumed: node_network.function_pin_consumed(node.id),
        selected: node_network.is_node_selected(node.id),
        active: node_network.is_node_active(node.id),
        displayed: node_network.is_node_displayed(node.id),
        return_node: node_network.return_node_id == Some(node.id),
        error,
        output_pin_strings,
        subtitle,
        comment_label,
        comment_text,
        comment_width,
        comment_height,
        closure_custom_label,
        zone,
        derived_shape,
    })
}

/// Build an [`APIDerivedShapeView`] for nodes whose layout / output type is
/// derived from a wired input pin: `apply` (arg pins materialise from `f`) and
/// `map` (output type derives from `f` via the starts-with rule). Returns
/// `None` for every other node type. The resulting `derived_from_input_pin`
/// is `Some("f")` only when the relevant source resolves to a usable type
/// (`Function(_)` for apply; `Function(_)` whose params start with the map's
/// element type for map). See
/// `doc/design_function_pin_unification.md` (Phase D).
fn build_derived_shape_view(
    node: &crate::structure_designer::node_network::Node,
    node_network: &crate::structure_designer::node_network::NodeNetwork,
    cad_instance: &crate::api::api_common::CADInstance,
) -> Option<APIDerivedShapeView> {
    use crate::structure_designer::data_type::DataType;
    let registry = &cad_instance.structure_designer.node_type_registry;
    match node.node_type_name.as_str() {
        "apply" => {
            let f_arg = node.arguments.first()?;
            let f_wire = f_arg.incoming_wires.first()?;
            if f_wire.source_scope_depth != 0 {
                return Some(APIDerivedShapeView {
                    derived_from_input_pin: None,
                });
            }
            let (src_node_id, src_pin_index) = f_wire.as_legacy_pair()?;
            let src_node = node_network.nodes.get(&src_node_id)?;
            let src_type = registry.resolve_output_type(src_node, node_network, src_pin_index);
            let derived = matches!(src_type, Some(DataType::Function(_)));
            Some(APIDerivedShapeView {
                derived_from_input_pin: derived.then(|| "f".to_string()),
            })
        }
        "map" => {
            let f_arg = node.arguments.get(1)?;
            let f_wire = f_arg.incoming_wires.first()?;
            if f_wire.source_scope_depth != 0 {
                return Some(APIDerivedShapeView {
                    derived_from_input_pin: None,
                });
            }
            let (src_node_id, src_pin_index) = f_wire.as_legacy_pair()?;
            let src_node = node_network.nodes.get(&src_node_id)?;
            let src_type = registry.resolve_output_type(src_node, node_network, src_pin_index)?;
            let DataType::Function(src_ft) = src_type else {
                return Some(APIDerivedShapeView {
                    derived_from_input_pin: None,
                });
            };
            // Read the element_type from the map's f-pin declared AnyFunction
            // leading_params (set by MapData::calculate_custom_node_type).
            let element_type = {
                let nt = registry.get_node_type_for_node(node)?;
                let f_param = nt.parameters.get(1)?;
                match &f_param.data_type {
                    DataType::AnyFunction { leading_params } => leading_params.first()?.clone(),
                    _ => {
                        return Some(APIDerivedShapeView {
                            derived_from_input_pin: None,
                        });
                    }
                }
            };
            let starts_with = src_ft.parameter_types.first() == Some(&element_type);
            Some(APIDerivedShapeView {
                derived_from_input_pin: starts_with.then(|| "f".to_string()),
            })
        }
        _ => None,
    }
}

/// Collect every wire stored on [node_network] as [`WireView`]s — used by
/// both the top-level `get_node_network_view` and the recursive
/// `build_zone_view`. Surfaces every `IncomingWire` regardless of
/// `source_scope_depth` or [`SourcePin`] kind, so phase U5's captures and
/// iteration-value references are visible to the Flutter painter.
fn build_wires_for_network(
    node_network: &crate::structure_designer::node_network::NodeNetwork,
) -> Vec<WireView> {
    use crate::structure_designer::node_network::SourcePin;
    let mut wires = Vec::new();
    for (_id, node) in node_network.nodes.iter() {
        for (index, argument) in node.arguments.iter().enumerate() {
            for incoming in argument.incoming_wires.iter() {
                let (source_output_pin_index, source_pin) = match incoming.source_pin {
                    SourcePin::NodeOutput { pin_index } => {
                        (pin_index, APISourcePin::NodeOutput { pin_index })
                    }
                    SourcePin::ZoneInput { pin_index } => (
                        pin_index as i32,
                        APISourcePin::ZoneInput {
                            pin_index: pin_index as u32,
                        },
                    ),
                };
                // Full-identity match: captures (`source_scope_depth ≥ 1`) and
                // iteration-value references (`ZoneInput` source) are selectable
                // too, so report their `selected` flag the same as regular
                // wires. `is_incoming_wire_selected` canonicalizes the stored
                // identity, so any `External`-destination wire is covered.
                let selected =
                    node_network.is_incoming_wire_selected(incoming.source_node_id, node.id, index);
                wires.push(WireView {
                    source_node_id: incoming.source_node_id,
                    source_output_pin_index,
                    dest_node_id: node.id,
                    dest_param_index: index,
                    selected,
                    destination_argument_kind: APIArgumentKind::External,
                    source_pin,
                    source_scope_depth: incoming.source_scope_depth as u32,
                });
            }
        }
    }
    wires
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_network_view() -> Option<NodeNetworkView> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_network_name =
                    match &cad_instance.structure_designer.active_node_network_name {
                        Some(name) => name,
                        None => return None,
                    };

                let node_network = match cad_instance
                    .structure_designer
                    .node_type_registry
                    .node_networks
                    .get(node_network_name)
                {
                    Some(network) => network,
                    None => return None,
                };

                let mut nodes = HashMap::new();
                for (node_id, node) in node_network.nodes.iter() {
                    if let Some(view) = build_node_view(node, node_network, cad_instance, &[]) {
                        nodes.insert(*node_id, view);
                    }
                }
                let wires = build_wires_for_network(node_network);

                Some(NodeNetworkView {
                    name: node_network.node_type.name.clone(),
                    nodes,
                    wires,
                })
            },
            None,
        )
    }
}

/// Move a node. The targeted network is identified by `scope_path` — empty
/// means the active top-level network (existing behavior); a non-empty path
/// names a chain of HOF body owners to descend through via `Node.zone_mut()`.
/// Phase U2 of `doc/design_zones_ui.md` plumbs the parameter through every
/// mutation API.
#[flutter_rust_bridge::frb(sync)]
pub fn move_node(scope_path: Vec<u64>, node_id: u64, position: APIVec2) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.move_node_scoped(
                &scope_path,
                node_id,
                from_api_vec2(&position),
            );
        });
    }
}

/// Add a node to the network identified by `scope_path` (empty = active
/// top-level network; non-empty = walk `Node.zone` down the chain). Phase U2
/// plumbs the parameter through; body-scope adds run a simpler path without
/// the top-level orchestration (undo / display policy / drag-source
/// adapter) — those re-enter under a scope-aware code path in U4. See
/// `doc/design_zones_ui.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn add_node(
    scope_path: Vec<u64>,
    node_type_name: &str,
    position: APIVec2,
    drag_source: Option<APIDragSource>,
) -> u64 {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let drag = drag_source.and_then(|ds| {
                    // String that fails to round-trip through `DataType::from_string`
                    // (e.g. anonymous-record syntax `{x: Int, y: Int}`) is treated
                    // as if no drag source were supplied. See
                    // `doc/design_drag_aware_add_node.md` "Known limitation".
                    let parsed = DataType::from_string(&ds.source_pin_type).ok()?;
                    let direction = if ds.dragging_from_output {
                        crate::structure_designer::node_data::DragDirection::FromOutput
                    } else {
                        crate::structure_designer::node_data::DragDirection::FromInput
                    };
                    Some(crate::structure_designer::structure_designer::DragSource {
                        source_type: parsed,
                        direction,
                    })
                });
                let ret = cad_instance.structure_designer.add_node_scoped(
                    &scope_path,
                    node_type_name,
                    from_api_vec2(&position),
                    drag,
                );
                refresh_structure_designer_auto(cad_instance);
                ret
            },
            0, // Default value if CAD_INSTANCE is None
        )
    }
}

/// Duplicate a node within `scope_path`'s network. Phase U4 — body-scope
/// dispatch routes through `duplicate_node_scoped`.
#[flutter_rust_bridge::frb(sync)]
pub fn duplicate_node(scope_path: Vec<u64>, node_id: u64) -> u64 {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let ret = cad_instance
                    .structure_designer
                    .duplicate_node_scoped(&scope_path, node_id);
                refresh_structure_designer_auto(cad_instance);
                ret
            },
            0,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn can_connect_nodes(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_output_pin_index: i32,
    dest_node_id: u64,
    dest_param_index: usize,
) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance.structure_designer.can_connect_nodes_scoped(
                    &scope_path,
                    source_node_id,
                    source_output_pin_index,
                    dest_node_id,
                    dest_param_index,
                )
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn connect_nodes(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_output_pin_index: i32,
    dest_node_id: u64,
    dest_param_index: usize,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.connect_nodes_scoped(
                &scope_path,
                source_node_id,
                source_output_pin_index,
                dest_node_id,
                dest_param_index,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// General-shape wire-creation entry point for cross-scope wires. Used by
/// zones UI phase U5 to author captures (depth ≥ 1) and iteration-value
/// references (`ZoneInput` source). For local-scope `NodeOutput` wires the
/// existing [`connect_nodes`] is equivalent and slightly cheaper; for body-
/// return wires use [`connect_zone_output_wire`].
///
/// `dest_scope_path` is the network where the wire is stored — the body
/// containing the destination node. `source_scope_depth` measures how many
/// ancestor frames up from that storage scope the source lives.
#[flutter_rust_bridge::frb(sync)]
pub fn connect_wire(
    dest_scope_path: Vec<u64>,
    source_node_id: u64,
    source_pin: APISourcePin,
    source_scope_depth: u32,
    dest_node_id: u64,
    dest_param_index: usize,
) {
    use crate::structure_designer::node_network::SourcePin;
    let source_pin = match source_pin {
        APISourcePin::NodeOutput { pin_index } => SourcePin::NodeOutput { pin_index },
        APISourcePin::ZoneInput { pin_index } => SourcePin::ZoneInput {
            pin_index: pin_index as usize,
        },
    };
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.connect_wire_scoped(
                &dest_scope_path,
                source_node_id,
                source_pin,
                source_scope_depth as u8,
                dest_node_id,
                dest_param_index,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Predicate pair for [`connect_wire`]. Returns `true` when a cross-scope
/// wire with this shape is type-compatible and structurally valid.
#[flutter_rust_bridge::frb(sync)]
pub fn can_connect_wire(
    dest_scope_path: Vec<u64>,
    source_node_id: u64,
    source_pin: APISourcePin,
    source_scope_depth: u32,
    dest_node_id: u64,
    dest_param_index: usize,
) -> bool {
    use crate::structure_designer::node_network::SourcePin;
    let source_pin = match source_pin {
        APISourcePin::NodeOutput { pin_index } => SourcePin::NodeOutput { pin_index },
        APISourcePin::ZoneInput { pin_index } => SourcePin::ZoneInput {
            pin_index: pin_index as usize,
        },
    };
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance.structure_designer.can_connect_wire_scoped(
                    &dest_scope_path,
                    source_node_id,
                    source_pin,
                    source_scope_depth as u8,
                    dest_node_id,
                    dest_param_index,
                )
            },
            false,
        )
    }
}

/// Connect a body-return wire: source is a body node, destination is the
/// containing HOF's zone-output pin. `body_scope_path` identifies the body
/// (the last element is the HOF id whose `zone_output_arguments` receives
/// the wire). Phase U4 — see `doc/design_zones_ui.md` §"Wire-creation API
/// generalisation".
#[flutter_rust_bridge::frb(sync)]
pub fn connect_zone_output_wire(
    body_scope_path: Vec<u64>,
    source_node_id: u64,
    source_output_pin_index: i32,
    zone_output_index: usize,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.connect_zone_output_wire(
                &body_scope_path,
                source_node_id,
                source_output_pin_index,
                zone_output_index,
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Auto-connects a source pin to the first compatible pin on a target node.
///
/// - `source_node_id`: The node where the wire was dragged from
/// - `source_pin_index`: The pin index on the source node
/// - `source_is_output`: true if dragging from output pin, false if from input pin
/// - `target_node_id`: The newly created node to connect to
///
/// Returns true if a connection was made, false otherwise.
#[flutter_rust_bridge::frb(sync)]
pub fn auto_connect_to_node(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_pin_index: i32,
    source_is_output: bool,
    target_node_id: u64,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if !scope_path.is_empty() {
                    return false;
                }
                let result = cad_instance.structure_designer.auto_connect_to_node(
                    source_node_id,
                    source_pin_index,
                    source_is_output,
                    target_node_id,
                );
                refresh_structure_designer_auto(cad_instance);
                result
            },
            false,
        )
    }
}

/// Returns all compatible pins on the target node for auto-connection.
/// Each element contains (pin_index, pin_name, data_type_string).
/// When source_is_output is true, returns compatible INPUT pins on target.
/// When source_is_output is false, returns the OUTPUT pin if compatible.
#[flutter_rust_bridge::frb(sync)]
pub fn get_compatible_pins_for_auto_connect(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_pin_index: i32,
    source_is_output: bool,
    target_node_id: u64,
) -> Vec<(i32, String, String)> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                if !scope_path.is_empty() {
                    return Vec::new();
                }
                cad_instance
                    .structure_designer
                    .get_compatible_pins_for_auto_connect(
                        source_node_id,
                        source_pin_index,
                        source_is_output,
                        target_node_id,
                    )
            },
            Vec::new(),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_type_views() -> Option<Vec<APINodeCategoryView>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                Some(
                    cad_instance
                        .structure_designer
                        .node_type_registry
                        .get_node_type_views(),
                )
            },
            None,
        )
    }
}

/// Returns node types that have at least one pin compatible with the given type.
///
/// - `source_type_str`: The data type being dragged (serialized string, e.g., "Blueprint", "Float")
/// - `dragging_from_output`: true if dragging from output pin, false if from input pin
///
/// When dragging from OUTPUT: find nodes with compatible INPUT pins
/// When dragging from INPUT: find nodes with compatible OUTPUT pins
#[flutter_rust_bridge::frb(sync)]
pub fn get_compatible_node_types(
    source_type_str: String,
    dragging_from_output: bool,
) -> Option<Vec<APINodeCategoryView>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let source_type = DataType::from_string(&source_type_str).ok()?;
                Some(
                    cad_instance
                        .structure_designer
                        .node_type_registry
                        .get_compatible_node_types(&source_type, dragging_from_output),
                )
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_network_names() -> Option<Vec<String>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                Some(
                    cad_instance
                        .structure_designer
                        .node_type_registry
                        .get_node_network_names(),
                )
            },
            None,
        )
    }
}

/// Returns the deliberately-created empty-folder paths (sorted). The tree view
/// merges these with the folders implied by entity names. See
/// `doc/design_empty_folders.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_folder_names() -> Option<Vec<String>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                Some(
                    cad_instance
                        .structure_designer
                        .node_type_registry
                        .get_folder_names(),
                )
            },
            None,
        )
    }
}

/// Returns every **user-declared** record type def name in the project,
/// sorted alphabetically. Used by the user-types panel so built-in defs are
/// not listed there. Dropdowns (type selector, `record_construct` /
/// `record_destructure` / `product` editors) should call
/// `get_all_record_type_def_names` instead so they see built-ins too.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_type_def_names() -> Option<Vec<String>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let mut names: Vec<String> = cad_instance
                    .structure_designer
                    .node_type_registry
                    .record_type_defs
                    .keys()
                    .cloned()
                    .collect();
                names.sort();
                Some(names)
            },
            None,
        )
    }
}

/// Returns every record type def name in the project (user-declared plus
/// built-in), sorted alphabetically. Used by the Flutter type-selector
/// Record branch and by the `record_construct` / `record_destructure` /
/// `product` node-property dropdowns. See
/// `doc/design_atom_replace_rules_input.md` Phase A.
#[flutter_rust_bridge::frb(sync)]
pub fn get_all_record_type_def_names() -> Option<Vec<String>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let registry = &cad_instance.structure_designer.node_type_registry;
                let mut names: Vec<String> = registry
                    .record_type_defs
                    .keys()
                    .chain(registry.built_in_record_type_defs.keys())
                    .cloned()
                    .collect();
                names.sort();
                names.dedup();
                Some(names)
            },
            None,
        )
    }
}

/// Returns every built-in record type def name, sorted alphabetically.
/// Used by Flutter-side namespace-collision checks so the UI can pre-validate
/// before round-tripping to Rust. See
/// `doc/design_atom_replace_rules_input.md` Phase A.
#[flutter_rust_bridge::frb(sync)]
pub fn get_built_in_record_type_def_names() -> Option<Vec<String>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let mut names: Vec<String> = cad_instance
                    .structure_designer
                    .node_type_registry
                    .built_in_record_type_defs
                    .keys()
                    .cloned()
                    .collect();
                names.sort();
                Some(names)
            },
            None,
        )
    }
}

/// Returns the full record type def for `name`, or `None` if the name is
/// not registered. Used by the schema editor in the user-types panel.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_type_def(name: String) -> Option<APIRecordTypeDef> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .node_type_registry
                    .record_type_defs
                    .get(&name)
                    .map(|def| APIRecordTypeDef {
                        name: def.name.clone(),
                        fields: def
                            .fields
                            .iter()
                            .map(|field| APIRecordTypeField {
                                id: Some(field.id.0),
                                name: field.name.clone(),
                                data_type: data_type_to_api_data_type(&field.data_type),
                            })
                            .collect(),
                    })
            },
            None,
        )
    }
}

/// Adds a new record type def with `name` and an empty field list.
/// Empty record defs are valid (top of the subtype lattice).
#[flutter_rust_bridge::frb(sync)]
pub fn add_record_type_def(name: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let def =
                    crate::structure_designer::node_type_registry::RecordTypeDef::new(name.clone());
                let result = instance.structure_designer.add_record_type_def(def);
                refresh_structure_designer_auto(instance);
                match result {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Deletes the record type def with the given name. Wires that depended on
/// the now-dangling references are disconnected by `repair_node_network`.
#[flutter_rust_bridge::frb(sync)]
pub fn delete_record_type_def(name: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.delete_record_type_def(&name);
                refresh_structure_designer_auto(instance);
                match result {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Renames a record type def. Walks every embedded `Named(old_name)`
/// reference in the project and rewrites it to `Named(new_name)`. No wires
/// are disconnected — every reference resolves to the same schema, just
/// under a new name.
#[flutter_rust_bridge::frb(sync)]
pub fn rename_record_type_def(old_name: String, new_name: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .rename_record_type_def(&old_name, &new_name);
                refresh_structure_designer_auto(instance);
                match result {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Replaces the field list of an existing record type def. Authored field
/// order is preserved; cycle introduction is rejected. Networks are
/// repaired afterward so `record_construct` / `record_destructure` /
/// `product` pin layouts re-derive and now-incompatible wires are dropped.
#[flutter_rust_bridge::frb(sync)]
pub fn update_record_type_def(name: String, fields: Vec<APIRecordTypeField>) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                // Convert API fields to identity-aware edits. `id == Some` is an
                // existing field (preserves wires by `FieldId`); `id == None` is
                // a newly added row. Bail with a clear error if any field's
                // APIDataType cannot be parsed (e.g. a malformed Custom string).
                use crate::structure_designer::node_type_registry::{FieldId, RecordFieldEdit};
                let mut converted: Vec<RecordFieldEdit> = Vec::with_capacity(fields.len());
                for f in &fields {
                    match api_data_type_to_data_type(&f.data_type) {
                        Ok(dt) => converted.push(RecordFieldEdit {
                            id: f.id.map(FieldId),
                            name: f.name.clone(),
                            data_type: dt,
                        }),
                        Err(e) => {
                            return APIResult {
                                success: false,
                                error_message: format!(
                                    "Field '{}' has invalid type: {}",
                                    f.name, e
                                ),
                            };
                        }
                    }
                }
                let result = instance
                    .structure_designer
                    .update_record_type_def_with_ids(&name, converted);
                refresh_structure_designer_auto(instance);
                match result {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Checks if a node type name corresponds to a custom node (i.e., a user-defined node network).
/// Returns false if the CAD instance is not available.
#[flutter_rust_bridge::frb(sync)]
pub fn is_custom_node_type(node_type_name: String) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .node_type_registry
                    .is_custom_node_type(&node_type_name)
            },
            false,
        )
    }
}

/// Gets the description of the active node network
#[flutter_rust_bridge::frb(sync)]
pub fn get_active_network_description() -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .get_active_network_description()
            },
            None,
        )
    }
}

/// Sets the description of the active node network
#[flutter_rust_bridge::frb(sync)]
pub fn set_active_network_description(description: String) -> Result<(), String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .set_active_network_description(description)
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// Gets the summary of the active node network
#[flutter_rust_bridge::frb(sync)]
pub fn get_active_network_summary() -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.get_active_network_summary(),
            None,
        )
    }
}

/// Sets the summary of the active node network
/// Pass None or empty string to clear the summary
#[flutter_rust_bridge::frb(sync)]
pub fn set_active_network_summary(summary: Option<String>) -> Result<(), String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .set_active_network_summary(summary)
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// Gets the description of a specific node network
#[flutter_rust_bridge::frb(sync)]
pub fn get_network_description(network_name: String) -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .get_network_description(&network_name)
                    .map(|(_name, description)| description)
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_node_networks_with_validation() -> Option<Vec<APINetworkWithValidationErrors>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                Some(
                    cad_instance
                        .structure_designer
                        .node_type_registry
                        .get_node_networks_with_validation(),
                )
            },
            None,
        )
    }
}

/// Add a node network with an auto-generated unique name and activate it.
/// Returns the generated name so the Flutter side can select the new network
/// (the network registry is a HashMap, so list order is not reliable — issue
/// #315).
#[flutter_rust_bridge::frb(sync)]
pub fn add_new_node_network() -> String {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let name = instance.structure_designer.add_new_node_network();
                // New networks don't have camera settings, but we still call the method
                let camera_settings = instance
                    .structure_designer
                    .set_active_node_network_name(Some(name.clone()));
                apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
                refresh_structure_designer_auto(instance);
                name
            },
            String::new(),
        )
    }
}

/// Add a node network with an auto-generated unique name under `namespace`
/// (a dot-delimited prefix; empty string = root) and activate it. Returns the
/// generated qualified name so the Flutter side can select the new network.
#[flutter_rust_bridge::frb(sync)]
pub fn add_new_node_network_in_namespace(namespace: String) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let name = instance
                    .structure_designer
                    .add_new_node_network_in_namespace(&namespace);
                // New networks don't have camera settings, but we still call the method
                let camera_settings = instance
                    .structure_designer
                    .set_active_node_network_name(Some(name.clone()));
                apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
                refresh_structure_designer_auto(instance);
                name
            },
            String::new(),
        )
    }
}

/// Add a record type def with an auto-generated unique name under `namespace`
/// (a dot-delimited prefix; empty string = root) and activate it. Returns the
/// generated qualified name, or an empty string on failure.
#[flutter_rust_bridge::frb(sync)]
pub fn add_new_record_type_def_in_namespace(namespace: String) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |instance| match instance
                .structure_designer
                .add_new_record_type_def_in_namespace(&namespace)
            {
                Ok(name) => {
                    refresh_structure_designer_auto(instance);
                    name
                }
                Err(_) => String::new(),
            },
            String::new(),
        )
    }
}

/// Add a node network with a specific name.
/// Returns success/error. Auto-activates the new network.
#[flutter_rust_bridge::frb(sync)]
pub fn add_node_network_with_name(name: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                // Check if name already exists across the whole user-type
                // namespace (networks, user record defs, built-in record
                // defs, built-in node types).
                if instance
                    .structure_designer
                    .node_type_registry
                    .name_is_taken(&name)
                {
                    return APIResult {
                        success: false,
                        error_message: format!("Name '{}' is already taken", name),
                    };
                }
                if let Err(reason) = instance
                    .structure_designer
                    .add_node_network_with_undo(&name)
                {
                    return APIResult {
                        success: false,
                        error_message: format!("Invalid network name: {}", reason),
                    };
                }
                // New networks don't have camera settings, but we still call the method
                let camera_settings = instance
                    .structure_designer
                    .set_active_node_network_name(Some(name));
                apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
                refresh_structure_designer_auto(instance);
                APIResult {
                    success: true,
                    error_message: String::new(),
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_active_node_network(node_network_name: &str) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let camera_settings = instance
                .structure_designer
                .set_active_node_network_name(Some(node_network_name.to_string()));
            apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
            refresh_structure_designer_auto(instance);
        });
    }
}

/// Navigates back in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn navigate_back() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let (result, camera_settings) = instance.structure_designer.navigate_back();
                if result {
                    apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
                    refresh_structure_designer_auto(instance);
                }
                result
            },
            false,
        )
    }
}

/// Navigates forward in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn navigate_forward() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let (result, camera_settings) = instance.structure_designer.navigate_forward();
                if result {
                    apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
                    refresh_structure_designer_auto(instance);
                }
                result
            },
            false,
        )
    }
}

/// Checks if we can navigate backward in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn can_navigate_back() -> bool {
    unsafe {
        with_cad_instance_or(
            |instance| instance.structure_designer.can_navigate_back(),
            false,
        )
    }
}

/// Checks if we can navigate forward in node network history
#[flutter_rust_bridge::frb(sync)]
pub fn can_navigate_forward() -> bool {
    unsafe {
        with_cad_instance_or(
            |instance| instance.structure_designer.can_navigate_forward(),
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rename_node_network(old_name: &str, new_name: &str) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .rename_node_network(old_name, new_name);
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_node_network(network_name: &str) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .delete_node_network(network_name);
                refresh_structure_designer_auto(instance);

                match result {
                    Ok(_) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e,
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Duplicate a named node network under a fresh unique name (`<name>_copy`,
/// then `<name>_copy_2`, …). The copy is a shallow duplicate: inline zone
/// bodies are copied; references to other named networks stay references.
/// Auto-activates the new copy. Returns success/error.
#[flutter_rust_bridge::frb(sync)]
pub fn duplicate_node_network(source_name: &str) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| match instance
                .structure_designer
                .duplicate_node_network(source_name)
            {
                Ok(new_name) => {
                    let camera_settings = instance
                        .structure_designer
                        .set_active_node_network_name(Some(new_name));
                    apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
                    refresh_structure_designer_auto(instance);
                    APIResult {
                        success: true,
                        error_message: String::new(),
                    }
                }
                Err(e) => {
                    refresh_structure_designer_auto(instance);
                    APIResult {
                        success: false,
                        error_message: e,
                    }
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn rename_namespace(old_prefix: &str, new_prefix: &str) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .rename_namespace(old_prefix, new_prefix);
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

/// Read-only preview of moving/renaming the namespace `old_prefix` to
/// `new_prefix` (empty `new_prefix` => promote contents to the root). Returns
/// the full list of affected networks with their resulting names plus
/// conflict/validity flags so the move-namespace dialog can show the user
/// exactly what will happen — and whether it is allowed — before committing.
/// Does not mutate state.
#[flutter_rust_bridge::frb(sync)]
pub fn preview_namespace_rename(old_prefix: &str, new_prefix: &str) -> APINamespaceRenamePreview {
    unsafe {
        with_cad_instance_or(
            |instance| {
                instance
                    .structure_designer
                    .compute_namespace_rename(old_prefix, new_prefix)
                    .into()
            },
            APINamespaceRenamePreview {
                items: Vec::new(),
                is_empty: true,
                has_invalid_names: false,
                has_conflicts: false,
                applicable: false,
            },
        )
    }
}

/// Read-only preview of moving/renaming a single leaf `old_name` (a node
/// network **or** a record type def) to the fully-qualified `new_name`. The
/// kind is detected by the backend (`compute_leaf_rename`), so the move dialog
/// can render both kinds uniformly. Returns the same single-item preview shape
/// as `preview_namespace_rename`. Does not mutate state.
#[flutter_rust_bridge::frb(sync)]
pub fn preview_leaf_rename(old_name: &str, new_name: &str) -> APINamespaceRenamePreview {
    unsafe {
        with_cad_instance_or(
            |instance| {
                instance
                    .structure_designer
                    .compute_leaf_rename(old_name, new_name)
                    .into()
            },
            APINamespaceRenamePreview {
                items: Vec::new(),
                is_empty: true,
                has_invalid_names: false,
                has_conflicts: false,
                applicable: false,
            },
        )
    }
}

/// The user record type def currently open in the schema editor, or `None`.
/// Backend-owned source of truth (see `doc/design_hierarchical_records.md` §8);
/// the Flutter model mirrors this in `refreshFromKernel` so the selection
/// survives undo/redo of a record rename/move/delete.
#[flutter_rust_bridge::frb(sync)]
pub fn get_active_record_def_name() -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |instance| instance.structure_designer.get_active_record_def_name(),
            None,
        )
    }
}

/// Set the active record def (the one open in the schema editor). Pass `None`
/// to clear the selection (fall back to the network editor). This is plain
/// selection state — not undoable — but it is backend-owned so undo/redo of
/// record rename/delete can remap/clear it correctly.
#[flutter_rust_bridge::frb(sync)]
pub fn set_active_record_def_name(name: Option<String>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance.structure_designer.set_active_record_def_name(name);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_namespace(prefix: &str) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.delete_namespace(prefix);
                refresh_structure_designer_auto(instance);

                match result {
                    Ok(_) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e,
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Create an empty folder at `path` (dot-delimited, e.g. `"Physics.Mechanics"`).
/// Returns success/error (collision or invalid name). See
/// `doc/design_empty_folders.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn add_folder(path: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.add_folder(&path);
                refresh_structure_designer_auto(instance);
                match result {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e,
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_node_display(scope_path: Vec<u64>, node_id: u64, is_displayed: bool) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance
                .structure_designer
                .set_node_display_scoped(&scope_path, node_id, is_displayed);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_output_pin_display(scope_path: Vec<u64>, node_id: u64, pin_index: i32) {
    unsafe {
        with_mut_cad_instance(|instance| {
            if !scope_path.is_empty() {
                // Per-pin display undo on body nodes lands in U4 alongside
                // body authoring (see `doc/design_zones_ui.md`). For U2 we
                // accept the parameter but body paths are inert.
                return;
            }
            instance
                .structure_designer
                .toggle_output_pin_display(node_id, pin_index);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_node(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .select_node_scoped(&scope_path, node_id);
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wire(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_output_pin_index: i32,
    destination_node_id: u64,
    destination_argument_index: usize,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.select_wire_scoped(
                    &scope_path,
                    source_node_id,
                    source_output_pin_index,
                    destination_node_id,
                    destination_argument_index,
                );
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn clear_selection(scope_path: Vec<u64>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance
                .structure_designer
                .clear_selection_scoped(&scope_path);
            refresh_structure_designer_auto(instance);
        });
    }
}

/// Clear selection in every scope reachable from the active top-level
/// network. Used when the user clicks empty top-level space so an active body
/// node doesn't keep its `.active` flag. Phase U4.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_selection_all_scopes() {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance.structure_designer.clear_selection_all_scopes();
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_node_selection(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .toggle_node_selection_scoped(&scope_path, node_id);
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_node_to_selection(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .add_node_to_selection_scoped(&scope_path, node_id);
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_nodes(scope_path: Vec<u64>, node_ids: Vec<u64>) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance
                    .structure_designer
                    .select_nodes_scoped(&scope_path, node_ids);
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_nodes_selection(scope_path: Vec<u64>, node_ids: Vec<u64>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance
                .structure_designer
                .toggle_nodes_selection_scoped(&scope_path, node_ids);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_node_ids() -> Vec<u64> {
    unsafe {
        with_cad_instance_or(
            |instance| instance.structure_designer.get_selected_node_ids(),
            Vec::new(),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn move_selected_nodes(scope_path: Vec<u64>, delta_x: f64, delta_y: f64) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance
                .structure_designer
                .move_selected_nodes_scoped(&scope_path, glam::f64::DVec2::new(delta_x, delta_y));
        });
    }
}

/// Called by Flutter when a node drag begins. Captures current positions for
/// undo coalescing. `scope_path` identifies the body whose nodes are being
/// dragged (empty = top-level); body-scope drags coalesce into a single
/// scope-aware `MoveNodesCommand`. See `doc/design_zones_ui.md` §"Undo/redo".
#[flutter_rust_bridge::frb(sync)]
pub fn begin_move_nodes(scope_path: Vec<u64>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance
                .structure_designer
                .begin_move_nodes_scoped(&scope_path);
        });
    }
}

/// Called by Flutter when a node drag ends. Creates a single MoveNodesCommand.
/// The target scope is recorded in the pending move captured by
/// `begin_move_nodes`, so `scope_path` here is informational only.
#[flutter_rust_bridge::frb(sync)]
pub fn end_move_nodes(scope_path: Vec<u64>) {
    let _ = scope_path;
    unsafe {
        with_mut_cad_instance(|instance| {
            instance.structure_designer.end_move_nodes();
        });
    }
}

/// Undo the last command. Returns true if an undo was performed.
#[flutter_rust_bridge::frb(sync)]
pub fn undo() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.undo();
                if result {
                    refresh_structure_designer_auto(instance);
                }
                result
            },
            false,
        )
    }
}

/// Redo the last undone command. Returns true if a redo was performed.
#[flutter_rust_bridge::frb(sync)]
pub fn redo() -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.redo();
                if result {
                    refresh_structure_designer_auto(instance);
                }
                result
            },
            false,
        )
    }
}

/// Returns true if there is a command that can be undone.
#[flutter_rust_bridge::frb(sync)]
pub fn can_undo() -> bool {
    unsafe {
        with_cad_instance_or(
            |instance| instance.structure_designer.undo_stack.can_undo(),
            false,
        )
    }
}

/// Returns true if there is a command that can be redone.
#[flutter_rust_bridge::frb(sync)]
pub fn can_redo() -> bool {
    unsafe {
        with_cad_instance_or(
            |instance| instance.structure_designer.undo_stack.can_redo(),
            false,
        )
    }
}

/// Returns the description of the command that would be undone, or null if nothing to undo.
#[flutter_rust_bridge::frb(sync)]
pub fn undo_description() -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |instance| {
                instance
                    .structure_designer
                    .undo_stack
                    .undo_description()
                    .map(|s| s.to_string())
            },
            None,
        )
    }
}

/// Returns the description of the command that would be redone, or null if nothing to redo.
#[flutter_rust_bridge::frb(sync)]
pub fn redo_description() -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |instance| {
                instance
                    .structure_designer
                    .undo_stack
                    .redo_description()
                    .map(|s| s.to_string())
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_wire_selection(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_output_pin_index: i32,
    destination_node_id: u64,
    destination_argument_index: usize,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.toggle_wire_selection_scoped(
                    &scope_path,
                    source_node_id,
                    source_output_pin_index,
                    destination_node_id,
                    destination_argument_index,
                );
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_wire_to_selection(
    scope_path: Vec<u64>,
    source_node_id: u64,
    source_output_pin_index: i32,
    destination_node_id: u64,
    destination_argument_index: usize,
) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |instance| {
                let result = instance.structure_designer.add_wire_to_selection_scoped(
                    &scope_path,
                    source_node_id,
                    source_output_pin_index,
                    destination_node_id,
                    destination_argument_index,
                );
                refresh_structure_designer_auto(instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_selected_wires() -> Vec<WireView> {
    unsafe {
        with_cad_instance_or(
            |instance| {
                instance
                    .structure_designer
                    .get_selected_wires()
                    .into_iter()
                    .map(|wire| {
                        let pin_index = wire.expect_node_output_pin();
                        WireView {
                            source_node_id: wire.source_node_id,
                            source_output_pin_index: pin_index,
                            dest_node_id: wire.destination_node_id,
                            dest_param_index: wire.destination_argument_index,
                            selected: true,
                            destination_argument_kind: APIArgumentKind::External,
                            source_pin: APISourcePin::NodeOutput { pin_index },
                            source_scope_depth: 0,
                        }
                    })
                    .collect()
            },
            Vec::new(),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_nodes_to_selection(scope_path: Vec<u64>, node_ids: Vec<u64>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance
                .structure_designer
                .add_nodes_to_selection_scoped(&scope_path, node_ids);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_wires(wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
                .into_iter()
                .map(|w| {
                    crate::structure_designer::node_network::Wire::node_output(
                        w.source_node_id,
                        w.source_output_pin_index,
                        w.destination_node_id,
                        w.destination_argument_index,
                    )
                })
                .collect();
            instance.structure_designer.select_wires(wire_structs);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_wires_to_selection(wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
                .into_iter()
                .map(|w| {
                    crate::structure_designer::node_network::Wire::node_output(
                        w.source_node_id,
                        w.source_output_pin_index,
                        w.destination_node_id,
                        w.destination_argument_index,
                    )
                })
                .collect();
            instance
                .structure_designer
                .add_wires_to_selection(wire_structs);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_wires_selection(wires: Vec<super::structure_designer_api_types::WireIdentifier>) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
                .into_iter()
                .map(|w| {
                    crate::structure_designer::node_network::Wire::node_output(
                        w.source_node_id,
                        w.source_output_pin_index,
                        w.destination_node_id,
                        w.destination_argument_index,
                    )
                })
                .collect();
            instance
                .structure_designer
                .toggle_wires_selection(wire_structs);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_nodes_and_wires(
    scope_path: Vec<u64>,
    node_ids: Vec<u64>,
    wires: Vec<super::structure_designer_api_types::WireIdentifier>,
) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
                .into_iter()
                .map(|w| {
                    crate::structure_designer::node_network::Wire::node_output(
                        w.source_node_id,
                        w.source_output_pin_index,
                        w.destination_node_id,
                        w.destination_argument_index,
                    )
                })
                .collect();
            instance.structure_designer.select_nodes_and_wires_scoped(
                &scope_path,
                node_ids,
                wire_structs,
            );
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn add_nodes_and_wires_to_selection(
    scope_path: Vec<u64>,
    node_ids: Vec<u64>,
    wires: Vec<super::structure_designer_api_types::WireIdentifier>,
) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
                .into_iter()
                .map(|w| {
                    crate::structure_designer::node_network::Wire::node_output(
                        w.source_node_id,
                        w.source_output_pin_index,
                        w.destination_node_id,
                        w.destination_argument_index,
                    )
                })
                .collect();
            instance
                .structure_designer
                .add_nodes_and_wires_to_selection_scoped(&scope_path, node_ids, wire_structs);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn toggle_nodes_and_wires_selection(
    scope_path: Vec<u64>,
    node_ids: Vec<u64>,
    wires: Vec<super::structure_designer_api_types::WireIdentifier>,
) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let wire_structs: Vec<crate::structure_designer::node_network::Wire> = wires
                .into_iter()
                .map(|w| {
                    crate::structure_designer::node_network::Wire::node_output(
                        w.source_node_id,
                        w.source_output_pin_index,
                        w.destination_node_id,
                        w.destination_argument_index,
                    )
                })
                .collect();
            instance
                .structure_designer
                .toggle_nodes_and_wires_selection_scoped(&scope_path, node_ids, wire_structs);
            refresh_structure_designer_auto(instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_extrude_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIExtrudeData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let extrude_data = match node_data.as_any_ref().downcast_ref::<ExtrudeData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIExtrudeData {
                    height: extrude_data.height,
                    extrude_direction: to_api_ivec3(&extrude_data.extrude_direction),
                    infinite: extrude_data.infinite,
                    subdivision: extrude_data.subdivision,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_extrude_drawing_plane_miller_direction(
    node_id: u64,
) -> Option<crate::api::common_api_types::APIIVec3> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let selected_node_id = match cad_instance
                    .structure_designer
                    .get_selected_node_id_with_type("extrude")
                {
                    Some(id) => id,
                    None => return None,
                };

                if selected_node_id != node_id {
                    return None;
                }

                let eval_cache = cad_instance
                    .structure_designer
                    .get_selected_node_eval_cache()?;
                let extrude_cache = eval_cache.downcast_ref::<ExtrudeEvalCache>()?;
                Some(to_api_ivec3(&extrude_cache.drawing_plane_miller_direction))
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_int_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIntData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let int_data = match node_data.as_any_ref().downcast_ref::<IntData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIIntData {
                    value: int_data.value,
                })
            },
            None,
        )
    }
}

/// Reads the `schema` property of a `record_construct` node — the name of
/// the record type def its pin layout is bound to. An empty string means
/// "no schema chosen yet". Returns `None` if the node does not exist or is
/// not a `record_construct`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_construct_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIRecordSchemaData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let data = node_data
                    .as_any_ref()
                    .downcast_ref::<RecordConstructData>()?;
                Some(APIRecordSchemaData {
                    schema: data.schema.clone(),
                })
            },
            None,
        )
    }
}

/// Reads the `schema` property of a `record_destructure` node — same shape
/// and semantics as `get_record_construct_data`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_destructure_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIRecordSchemaData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let data = node_data
                    .as_any_ref()
                    .downcast_ref::<RecordDestructureData>()?;
                Some(APIRecordSchemaData {
                    schema: data.schema.clone(),
                })
            },
            None,
        )
    }
}

/// Reads the `target` property of a `product` node. Surfaced through
/// `APIRecordSchemaData` (the API's `schema` field carries the target's
/// def-name, since the Flutter dropdown is the same widget).
#[flutter_rust_bridge::frb(sync)]
pub fn get_product_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRecordSchemaData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let data = node_data.as_any_ref().downcast_ref::<ProductData>()?;
                Some(APIRecordSchemaData {
                    schema: data.target.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_string_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIStringData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let string_data = match node_data.as_any_ref().downcast_ref::<StringData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIStringData {
                    value: string_data.value.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_bool_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIBoolData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let bool_data = match node_data.as_any_ref().downcast_ref::<BoolData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIBoolData {
                    value: bool_data.value,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_print_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIPrintData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let print_data = node_data.as_any_ref().downcast_ref::<PrintData>()?;
                Some(APIPrintData {
                    execute_only: print_data.execute_only,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_float_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIFloatData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let float_data = match node_data.as_any_ref().downcast_ref::<FloatData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIFloatData {
                    value: float_data.value,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_ivec2_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIVec2Data> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let ivec2_data = match node_data.as_any_ref().downcast_ref::<IVec2Data>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIIVec2Data {
                    value: to_api_ivec2(&ivec2_data.value),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_ivec3_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIVec3Data> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let ivec3_data = match node_data.as_any_ref().downcast_ref::<IVec3Data>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIIVec3Data {
                    value: to_api_ivec3(&ivec3_data.value),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_supercell_data(scope_path: Vec<u64>, node_id: u64) -> Option<APISupercellData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let supercell_data = node_data.as_any_ref().downcast_ref::<SupercellData>()?;
                let m = supercell_data.matrix;
                Some(APISupercellData {
                    a: to_api_ivec3(&glam::IVec3::new(m[0][0], m[0][1], m[0][2])),
                    b: to_api_ivec3(&glam::IVec3::new(m[1][0], m[1][1], m[1][2])),
                    c: to_api_ivec3(&glam::IVec3::new(m[2][0], m[2][1], m[2][2])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_imat2_rows_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIMat2RowsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<IMat2RowsData>()?;
                let m = d.matrix;
                Some(APIIMat2RowsData {
                    a: to_api_ivec2(&glam::IVec2::new(m[0][0], m[0][1])),
                    b: to_api_ivec2(&glam::IVec2::new(m[1][0], m[1][1])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_imat2_cols_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIMat2ColsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<IMat2ColsData>()?;
                let m = d.matrix;
                Some(APIIMat2ColsData {
                    a: to_api_ivec2(&glam::IVec2::new(m[0][0], m[1][0])),
                    b: to_api_ivec2(&glam::IVec2::new(m[0][1], m[1][1])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_imat2_diag_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIMat2DiagData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<IMat2DiagData>()?;
                Some(APIIMat2DiagData {
                    v: to_api_ivec2(&d.v),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_plane_tiling_vectors_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIPlaneTilingVectorsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data
                    .as_any_ref()
                    .downcast_ref::<PlaneTilingVectorsData>()?;
                let m = d.matrix;
                Some(APIPlaneTilingVectorsData {
                    a: to_api_ivec2(&glam::IVec2::new(m[0][0], m[0][1])),
                    b: to_api_ivec2(&glam::IVec2::new(m[1][0], m[1][1])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_imat3_rows_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIMat3RowsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<IMat3RowsData>()?;
                let m = d.matrix;
                Some(APIIMat3RowsData {
                    a: to_api_ivec3(&glam::IVec3::new(m[0][0], m[0][1], m[0][2])),
                    b: to_api_ivec3(&glam::IVec3::new(m[1][0], m[1][1], m[1][2])),
                    c: to_api_ivec3(&glam::IVec3::new(m[2][0], m[2][1], m[2][2])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_imat3_cols_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIMat3ColsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<IMat3ColsData>()?;
                let m = d.matrix;
                Some(APIIMat3ColsData {
                    a: to_api_ivec3(&glam::IVec3::new(m[0][0], m[1][0], m[2][0])),
                    b: to_api_ivec3(&glam::IVec3::new(m[0][1], m[1][1], m[2][1])),
                    c: to_api_ivec3(&glam::IVec3::new(m[0][2], m[1][2], m[2][2])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_imat3_diag_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIIMat3DiagData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<IMat3DiagData>()?;
                Some(APIIMat3DiagData {
                    v: to_api_ivec3(&d.v),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_mat3_rows_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMat3RowsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<Mat3RowsData>()?;
                let m = d.matrix;
                Some(APIMat3RowsData {
                    a: to_api_vec3(&glam::DVec3::new(m[0][0], m[0][1], m[0][2])),
                    b: to_api_vec3(&glam::DVec3::new(m[1][0], m[1][1], m[1][2])),
                    c: to_api_vec3(&glam::DVec3::new(m[2][0], m[2][1], m[2][2])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_mat3_cols_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMat3ColsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<Mat3ColsData>()?;
                let m = d.matrix;
                Some(APIMat3ColsData {
                    a: to_api_vec3(&glam::DVec3::new(m[0][0], m[1][0], m[2][0])),
                    b: to_api_vec3(&glam::DVec3::new(m[0][1], m[1][1], m[2][1])),
                    c: to_api_vec3(&glam::DVec3::new(m[0][2], m[1][2], m[2][2])),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_mat3_diag_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMat3DiagData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let d = node_data.as_any_ref().downcast_ref::<Mat3DiagData>()?;
                Some(APIMat3DiagData {
                    v: to_api_vec3(&d.v),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_range_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRangeData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let range_data = match node_data.as_any_ref().downcast_ref::<RangeData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIRangeData {
                    start: range_data.start,
                    step: range_data.step,
                    count: range_data.count,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_vec2_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIVec2Data> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let vec2_data = match node_data.as_any_ref().downcast_ref::<Vec2Data>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIVec2Data {
                    value: to_api_vec2(&vec2_data.value),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_vec3_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIVec3Data> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let vec3_data = match node_data.as_any_ref().downcast_ref::<Vec3Data>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIVec3Data {
                    value: to_api_vec3(&vec3_data.value),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_rect_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRectData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let rect_data = match node_data.as_any_ref().downcast_ref::<RectData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIRectData {
                    min_corner: to_api_ivec2(&rect_data.min_corner),
                    extent: to_api_ivec2(&rect_data.extent),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_reg_poly_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIRegPolyData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let reg_poly_data = match node_data.as_any_ref().downcast_ref::<RegPolyData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIRegPolyData {
                    num_sides: reg_poly_data.num_sides,
                    radius: reg_poly_data.radius,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_circle_data(scope_path: Vec<u64>, node_id: u64) -> Option<APICircleData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let circle_data = match node_data.as_any_ref().downcast_ref::<CircleData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APICircleData {
                    center: to_api_ivec2(&circle_data.center),
                    radius: circle_data.radius,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_half_plane_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIHalfPlaneData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let half_plane_data = match node_data.as_any_ref().downcast_ref::<HalfPlaneData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIHalfPlaneData {
                    point1: to_api_ivec2(&half_plane_data.point1),
                    point2: to_api_ivec2(&half_plane_data.point2),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_cuboid_data(scope_path: Vec<u64>, node_id: u64) -> Option<APICuboidData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let cuboid_data = match node_data.as_any_ref().downcast_ref::<CuboidData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APICuboidData {
                    min_corner: to_api_ivec3(&cuboid_data.min_corner),
                    extent: to_api_ivec3(&cuboid_data.extent),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_cut_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIAtomCutData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let atom_cut_data = match node_data.as_any_ref().downcast_ref::<AtomCutData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIAtomCutData {
                    cut_sdf_value: atom_cut_data.cut_sdf_value,
                    unit_cell_size: atom_cut_data.unit_cell_size,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_apply_diff_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIApplyDiffData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let apply_diff_data = match node_data.as_any_ref().downcast_ref::<ApplyDiffData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIApplyDiffData {
                    tolerance: apply_diff_data.tolerance,
                    error_on_stale: apply_diff_data.error_on_stale,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_composediff_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIAtomComposeDiffData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let composediff_data =
                    match node_data.as_any_ref().downcast_ref::<AtomComposeDiffData>() {
                        Some(data) => data,
                        None => return None,
                    };
                Some(APIAtomComposeDiffData {
                    tolerance: composediff_data.tolerance,
                    error_on_stale: composediff_data.error_on_stale,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_import_xyz_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIImportXYZData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let import_xyz_data = match node_data.as_any_ref().downcast_ref::<ImportXYZData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIImportXYZData {
                    file_name: import_xyz_data.file_name.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_export_xyz_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIExportXYZData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let export_xyz_data = match node_data.as_any_ref().downcast_ref::<ExportXYZData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIExportXYZData {
                    file_name: export_xyz_data.file_name.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_sphere_data(scope_path: Vec<u64>, node_id: u64) -> Option<APISphereData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let sphere_data = match node_data.as_any_ref().downcast_ref::<SphereData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APISphereData {
                    center: to_api_ivec3(&sphere_data.center),
                    radius: sphere_data.radius,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_half_space_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIHalfSpaceData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let half_space_data = match node_data.as_any_ref().downcast_ref::<HalfSpaceData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIHalfSpaceData {
                    max_miller_index: half_space_data.max_miller_index,
                    miller_index: to_api_ivec3(&half_space_data.miller_index),
                    center: to_api_ivec3(&half_space_data.center),
                    shift: half_space_data.shift,
                    subdivision: half_space_data.subdivision,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_drawing_plane_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIDrawingPlaneData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let drawing_plane_data =
                    match node_data.as_any_ref().downcast_ref::<DrawingPlaneData>() {
                        Some(data) => data,
                        None => return None,
                    };
                // Expose the *resolved* Miller index from the eval cache (derived
                // in case D) for read-only display. Only available when this node
                // is the selected node, since the eval cache is per-selection.
                let resolved_miller_index = if cad_instance
                    .structure_designer
                    .get_selected_node_id_with_type("drawing_plane")
                    == Some(node_id)
                {
                    cad_instance
                        .structure_designer
                        .get_selected_node_eval_cache()
                        .and_then(|cache| cache.downcast_ref::<DrawingPlaneEvalCache>())
                        .map(|cache| to_api_ivec3(&cache.resolved_miller))
                } else {
                    None
                };
                Some(APIDrawingPlaneData {
                    max_miller_index: drawing_plane_data.max_miller_index,
                    miller_index: drawing_plane_data.miller_index.as_ref().map(to_api_ivec3),
                    center: to_api_ivec3(&drawing_plane_data.center),
                    shift: drawing_plane_data.shift,
                    subdivision: drawing_plane_data.subdivision,
                    u_axis: drawing_plane_data.u_axis.as_ref().map(to_api_ivec3),
                    v_axis: drawing_plane_data.v_axis.as_ref().map(to_api_ivec3),
                    resolved_miller_index,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_geo_trans_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIGeoTransData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let geo_trans_data = match node_data.as_any_ref().downcast_ref::<GeoTransData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIGeoTransData {
                    translation: to_api_ivec3(&geo_trans_data.translation),
                    rotation: to_api_ivec3(&geo_trans_data.rotation),
                    transform_only_frame: geo_trans_data.transform_only_frame,
                })
            },
            None,
        )
    }
}

/// Helper function to convert CrystalSystem enum to string
fn crystal_system_to_string(crystal_system: CrystalSystem) -> String {
    match crystal_system {
        CrystalSystem::Cubic => "Cubic".to_string(),
        CrystalSystem::Tetragonal(_) => "Tetragonal".to_string(),
        CrystalSystem::Orthorhombic => "Orthorhombic".to_string(),
        CrystalSystem::Hexagonal(_) => "Hexagonal".to_string(),
        CrystalSystem::Trigonal => "Trigonal".to_string(),
        CrystalSystem::Monoclinic(_) => "Monoclinic".to_string(),
        CrystalSystem::Triclinic => "Triclinic".to_string(),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_lattice_symop_data(scope_path: Vec<u64>, node_id: u64) -> Option<APILatticeSymopData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let lattice_symop_data =
                    match node_data.as_any_ref().downcast_ref::<LatticeSymopData>() {
                        Some(data) => data,
                        None => return None,
                    };

                // Try to get the evaluation cache to access unit cell and compute symmetries and crystal system
                let (api_symmetries, crystal_system_str) = if let Some(eval_cache) = cad_instance
                    .structure_designer
                    .get_selected_node_eval_cache()
                {
                    if let Some(lattice_symop_cache) =
                        eval_cache.downcast_ref::<LatticeSymopEvalCache>()
                    {
                        // Analyze unit cell symmetries and crystal system
                        let (crystal_system, symmetries) =
                            analyze_unit_cell_complete(&lattice_symop_cache.unit_cell);

                        // Convert symmetries to API format
                        let api_symmetries = symmetries
                            .into_iter()
                            .map(|sym| APIRotationalSymmetry {
                                axis: to_api_vec3(&sym.axis),
                                n_fold: sym.n_fold,
                            })
                            .collect();

                        (api_symmetries, crystal_system_to_string(crystal_system))
                    } else {
                        // No lattice symop cache available - return empty symmetries and unknown crystal system
                        (Vec::new(), "Unknown".to_string())
                    }
                } else {
                    // No evaluation cache available - return empty symmetries and unknown crystal system
                    (Vec::new(), "Unknown".to_string())
                };

                Some(APILatticeSymopData {
                    translation: to_api_ivec3(&lattice_symop_data.translation),
                    rotation_axis: lattice_symop_data
                        .rotation_axis
                        .map(|axis| to_api_vec3(&axis)),
                    rotation_angle_degrees: lattice_symop_data.rotation_angle_degrees,
                    transform_only_frame: lattice_symop_data.transform_only_frame,
                    rotational_symmetries: api_symmetries,
                    crystal_system: crystal_system_str,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_structure_move_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIStructureMoveData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let structure_move_data =
                    match node_data.as_any_ref().downcast_ref::<StructureMoveData>() {
                        Some(data) => data,
                        None => return None,
                    };

                Some(APIStructureMoveData {
                    translation: to_api_ivec3(&structure_move_data.translation),
                    lattice_subdivision: structure_move_data.lattice_subdivision,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_structure_rot_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIStructureRotData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let structure_rot_data =
                    match node_data.as_any_ref().downcast_ref::<StructureRotData>() {
                        Some(data) => data,
                        None => return None,
                    };

                // Try to get the evaluation cache to access unit cell and compute symmetries and crystal system
                let (api_symmetries, crystal_system_str) = if let Some(eval_cache) = cad_instance
                    .structure_designer
                    .get_selected_node_eval_cache()
                {
                    if let Some(structure_rot_cache) =
                        eval_cache.downcast_ref::<StructureRotEvalCache>()
                    {
                        // Analyze unit cell symmetries and crystal system
                        let (crystal_system, symmetries) =
                            analyze_unit_cell_complete(&structure_rot_cache.unit_cell);

                        // Convert symmetries to API format
                        let api_symmetries = symmetries
                            .into_iter()
                            .map(|sym| APIRotationalSymmetry {
                                axis: to_api_vec3(&sym.axis),
                                n_fold: sym.n_fold,
                            })
                            .collect();

                        (api_symmetries, crystal_system_to_string(crystal_system))
                    } else {
                        // No structure rot cache available - return empty symmetries and unknown crystal system
                        (Vec::new(), "Unknown".to_string())
                    }
                } else {
                    // No evaluation cache available - return empty symmetries and unknown crystal system
                    (Vec::new(), "Unknown".to_string())
                };

                Some(APIStructureRotData {
                    axis_index: structure_rot_data.axis_index,
                    step: structure_rot_data.step,
                    pivot_point: to_api_ivec3(&structure_rot_data.pivot_point),
                    rotational_symmetries: api_symmetries,
                    crystal_system: crystal_system_str,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_free_move_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIFreeMoveData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let free_move_data = match node_data.as_any_ref().downcast_ref::<FreeMoveData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIFreeMoveData {
                    translation: to_api_vec3(&free_move_data.translation),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_free_rot_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIFreeRotData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let free_rot_data = match node_data.as_any_ref().downcast_ref::<FreeRotData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIFreeRotData {
                    angle: free_rot_data.angle,
                    rot_axis: to_api_vec3(&free_rot_data.rot_axis),
                    pivot_point: to_api_vec3(&free_rot_data.pivot_point),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_edit_atom_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIEditAtomData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let edit_atom_data = match node_data.as_any_ref().downcast_ref::<EditAtomData>() {
                    Some(data) => data,
                    None => return None,
                };

                let bond_tool_last_atom_id = match &edit_atom_data.active_tool {
                    EditAtomTool::AddBond(state) => state.last_atom_id,
                    _ => None,
                };

                // Get the atomic structure from the selected node to check for selections
                let atomic_structure = cad_instance
                    .structure_designer
                    .get_atomic_structure_from_selected_node();

                // Default values if no atomic structure is found
                let has_selected_atoms =
                    atomic_structure.map_or(false, |structure| structure.has_selected_atoms());
                let has_selection =
                    atomic_structure.map_or(false, |structure| structure.has_selection());

                Some(APIEditAtomData {
                    active_tool: edit_atom_data.get_active_tool(),
                    can_undo: edit_atom_data.can_undo(),
                    can_redo: edit_atom_data.can_redo(),
                    bond_tool_last_atom_id,
                    selected_atomic_number: edit_atom_data.selected_atomic_number,
                    has_selected_atoms,
                    has_selection,
                    selection_transform: edit_atom_data
                        .selection_transform
                        .as_ref()
                        .map(|transform| crate::api::api_common::to_api_transform(transform)),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_edit_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIAtomEditData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let atom_edit_data = match node_data.as_any_ref().downcast_ref::<AtomEditData>() {
                    Some(data) => data,
                    None => return None,
                };

                let (bond_tool_last_atom_id, bond_tool_bond_order) =
                    match &atom_edit_data.active_tool {
                        AtomEditTool::AddBond(state) => (state.last_atom_id, state.bond_order),
                        _ => (None, 0),
                    };

                // Read selection state from the evaluated result (correct for visible atoms)
                let atomic_structure = cad_instance
                    .structure_designer
                    .get_atomic_structure_from_selected_node();
                let has_selected_atoms =
                    atomic_structure.map_or(false, |structure| structure.has_selected_atoms());
                let has_selection =
                    atomic_structure.map_or(false, |structure| structure.has_selection());

                // Compute selected bond info
                let selected_bond_count = atom_edit_data.selection.selected_bonds.len() as u32;
                let has_selected_bonds = selected_bond_count > 0;

                let selected_bond_order = if has_selected_bonds {
                    // Look up bond orders from the result structure
                    let mut unique_order: Option<u8> = None;
                    let mut mixed = false;
                    if let Some(structure) = atomic_structure {
                        for bond_ref in &atom_edit_data.selection.selected_bonds {
                            if let Some(atom) = structure.get_atom(bond_ref.atom_id1) {
                                if let Some(bond) = atom
                                    .bonds
                                    .iter()
                                    .find(|b| b.other_atom_id() == bond_ref.atom_id2)
                                {
                                    let order = bond.bond_order();
                                    match unique_order {
                                        None => unique_order = Some(order),
                                        Some(prev) if prev != order => {
                                            mixed = true;
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    if mixed { None } else { unique_order }
                } else {
                    None
                };

                // Read diff stats and measurement from eval cache
                let eval_cache = cad_instance
                    .structure_designer
                    .get_selected_node_eval_cache()
                    .and_then(|cache| cache.downcast_ref::<AtomEditEvalCache>());

                let diff_stats = eval_cache
                    .map(|cache| APIDiffStats {
                        atoms_added: cache.stats.atoms_added,
                        atoms_deleted: cache.stats.atoms_deleted,
                        atoms_modified: cache.stats.atoms_modified,
                        bonds_added: cache.stats.bonds_added,
                        bonds_deleted: cache.stats.bonds_deleted,
                        orphaned_tracked_atoms: cache.stats.orphaned_tracked_atoms,
                        unmatched_delete_markers: cache.stats.unmatched_delete_markers,
                        orphaned_bonds: cache.stats.orphaned_bonds,
                        unchanged_references: cache.stats.unchanged_references,
                    })
                    .unwrap_or(APIDiffStats {
                        atoms_added: 0,
                        atoms_deleted: 0,
                        atoms_modified: 0,
                        bonds_added: 0,
                        bonds_deleted: 0,
                        orphaned_tracked_atoms: 0,
                        unmatched_delete_markers: 0,
                        orphaned_bonds: 0,
                        unchanged_references: 0,
                    });

                // Compute measurement from selected atoms (2-4 atoms)
                let measurement = compute_selection_measurement(
                    atom_edit_data,
                    atomic_structure,
                    eval_cache,
                    cad_instance
                        .structure_designer
                        .is_selected_node_in_diff_view(),
                );

                // Compute last-selected result atom ID for dialog defaults
                let last_selected_result_atom_id = compute_last_selected_result_atom_id(
                    atom_edit_data,
                    eval_cache,
                    cad_instance
                        .structure_designer
                        .is_selected_node_in_diff_view(),
                );

                let is_in_guided_placement = matches!(
                    &atom_edit_data.active_tool,
                    AtomEditTool::AddAtom(
                        crate::structure_designer::nodes::atom_edit::atom_edit::AddAtomToolState::GuidedPlacement { .. }
                    ) | AtomEditTool::AddAtom(
                        crate::structure_designer::nodes::atom_edit::atom_edit::AddAtomToolState::GuidedFreeSphere { .. }
                    ) | AtomEditTool::AddAtom(
                        crate::structure_designer::nodes::atom_edit::atom_edit::AddAtomToolState::GuidedFreeRing { .. }
                    )
                );

                Some(APIAtomEditData {
                    active_tool: atom_edit_data.get_active_tool(),
                    bond_tool_last_atom_id,
                    bond_tool_bond_order,
                    selected_atomic_number: atom_edit_data.selected_atomic_number,
                    is_in_guided_placement,
                    has_selected_atoms,
                    has_selected_bonds,
                    selected_bond_count,
                    selected_bond_order,
                    has_selection,
                    selection_transform: atom_edit_data
                        .selection
                        .selection_transform
                        .as_ref()
                        .map(|transform| crate::api::api_common::to_api_transform(transform)),
                    output_diff: cad_instance
                        .structure_designer
                        .is_selected_node_in_diff_view(),
                    show_anchor_arrows: atom_edit_data.show_anchor_arrows,
                    include_base_bonds_in_diff: atom_edit_data.include_base_bonds_in_diff,
                    tolerance: atom_edit_data.tolerance,
                    error_on_stale_entries: atom_edit_data.error_on_stale_entries,
                    show_gadget: match &atom_edit_data.active_tool {
                        AtomEditTool::Default(state) => state.show_gadget,
                        _ => false,
                    },
                    diff_stats,
                    measurement,
                    last_selected_result_atom_id,
                    has_frozen_atoms: atom_edit_data.diff.iter_atoms().any(|(_, a)| a.is_frozen()),
                    continuous_minimization: atom_edit_data.continuous_minimization,
                    is_motif_mode: atom_edit_data.is_motif_mode,
                    parameter_elements: atom_edit_data
                        .parameter_elements
                        .iter()
                        .enumerate()
                        .map(|(i, (name, default_z))| {
                            use crate::structure_designer::nodes::atom_edit::atom_edit::param_index_to_atomic_number;
                            APIParameterElement {
                                name: name.clone(),
                                default_atomic_number: *default_z,
                                reserved_atomic_number: param_index_to_atomic_number(i),
                                color: param_element_color_u32(i),
                            }
                        })
                        .collect(),
                    neighbor_depth: atom_edit_data.neighbor_depth,
                })
            },
            None,
        )
    }
}

/// Look up the element symbol for a result-space atom.
fn atom_symbol(
    structure: &crate::crystolecule::atomic_structure::AtomicStructure,
    result_id: u32,
) -> String {
    use crate::crystolecule::atomic_constants::ATOM_INFO;
    use crate::structure_designer::nodes::atom_edit::atom_edit::param_atomic_number_to_index;
    structure
        .get_atom(result_id)
        .map(|a| {
            if let Some(idx) = param_atomic_number_to_index(a.atomic_number) {
                format!("P{}", idx + 1)
            } else {
                ATOM_INFO
                    .get(&(a.atomic_number as i32))
                    .map(|info| info.symbol.clone())
                    .unwrap_or_else(|| "?".to_string())
            }
        })
        .unwrap_or_else(|| "?".to_string())
}

/// Convert a parameter element index to a 0xRRGGBB color value.
/// Must match the PARAM_ELEMENT_COLORS array in atomic_tessellator.rs.
fn param_element_color_u32(index: usize) -> u32 {
    const COLORS: &[(u8, u8, u8)] = &[
        (230, 102, 230), // Purple-pink (PARAM_1)
        (51, 204, 153),  // Teal-green (PARAM_2)
        (230, 179, 51),  // Gold (PARAM_3)
        (77, 128, 230),  // Blue (PARAM_4)
        (230, 128, 77),  // Coral (PARAM_5)
        (128, 230, 102), // Lime (PARAM_6)
    ];
    let (r, g, b) = COLORS[index % COLORS.len()];
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Resolve selected atom positions and compute measurement.
fn compute_selection_measurement(
    atom_edit_data: &AtomEditData,
    result_structure: Option<&crate::crystolecule::atomic_structure::AtomicStructure>,
    eval_cache: Option<&AtomEditEvalCache>,
    is_diff_view: bool,
) -> Option<APIMeasurement> {
    use crate::structure_designer::nodes::atom_edit::measurement::{
        MeasurementResult, SelectedAtomInfo, compute_measurement,
    };

    let result_structure = result_structure?;

    // Count selected atoms; only compute for 1-4
    let total_selected = atom_edit_data.selection.selected_base_atoms.len()
        + atom_edit_data.selection.selected_diff_atoms.len();
    if !(1..=4).contains(&total_selected) {
        return None;
    }

    // Use selection_order for deterministic ordering that matches
    // gather_measurement_data() in modify_measurement.rs. This ensures that
    // atom1_id/arm_a_id/chain_a_id reported here correspond to the same atoms
    // that DistanceMoveChoice::First / AngleMoveChoice::ArmA / etc. will move.
    use crate::structure_designer::nodes::atom_edit::atom_edit::SelectionProvenance;

    let mut selected_atoms: Vec<SelectedAtomInfo> = Vec::with_capacity(total_selected);

    if is_diff_view {
        for &(prov, id) in &atom_edit_data.selection.selection_order {
            if prov == SelectionProvenance::Diff
                && atom_edit_data.selection.selected_diff_atoms.contains(&id)
                && let Some(atom) = result_structure.get_atom(id)
            {
                selected_atoms.push(SelectedAtomInfo {
                    result_atom_id: id,
                    position: atom.position,
                });
            }
        }
    } else {
        // Result view: resolve through provenance
        let cache = eval_cache?;
        for &(prov, id) in &atom_edit_data.selection.selection_order {
            match prov {
                SelectionProvenance::Base => {
                    if atom_edit_data.selection.selected_base_atoms.contains(&id)
                        && let Some(&result_id) = cache.provenance.base_to_result.get(&id)
                        && let Some(atom) = result_structure.get_atom(result_id)
                    {
                        selected_atoms.push(SelectedAtomInfo {
                            result_atom_id: result_id,
                            position: atom.position,
                        });
                    }
                }
                SelectionProvenance::Diff => {
                    if atom_edit_data.selection.selected_diff_atoms.contains(&id)
                        && let Some(&result_id) = cache.provenance.diff_to_result.get(&id)
                        && let Some(atom) = result_structure.get_atom(result_id)
                    {
                        selected_atoms.push(SelectedAtomInfo {
                            result_atom_id: result_id,
                            position: atom.position,
                        });
                    }
                }
            }
        }
    }

    if !(1..=4).contains(&selected_atoms.len()) {
        return None;
    }

    // Single atom: return element info instead of a geometric measurement.
    if selected_atoms.len() == 1 {
        use crate::crystolecule::atomic_constants::ATOM_INFO;
        let atom_id = selected_atoms[0].result_atom_id;
        if let Some(atom) = result_structure.get_atom(atom_id) {
            let info = ATOM_INFO.get(&(atom.atomic_number as i32));
            let symbol = info
                .map(|i| i.symbol.clone())
                .unwrap_or_else(|| "?".to_string());
            let element_name = info
                .map(|i| i.element_name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let inferred_hybridization = {
                use crate::crystolecule::guided_placement::{Hybridization, detect_hybridization};
                match detect_hybridization(result_structure, atom_id, None) {
                    Hybridization::Sp3 => 1,
                    Hybridization::Sp2 => 2,
                    Hybridization::Sp1 => 3,
                }
            };
            return Some(APIMeasurement::AtomInfo {
                symbol,
                element_name,
                bond_count: atom.bonds.len() as u32,
                x: atom.position.x,
                y: atom.position.y,
                z: atom.position.z,
                hybridization_override: atom.hybridization_override(),
                inferred_hybridization,
            });
        }
        return None;
    }

    let measurement = compute_measurement(&selected_atoms, result_structure)?;
    Some(match measurement {
        MeasurementResult::Distance { distance } => {
            let id1 = selected_atoms[0].result_atom_id;
            let id2 = selected_atoms[1].result_atom_id;
            let is_bonded = result_structure.has_bond_between(id1, id2);
            APIMeasurement::Distance {
                distance,
                atom1_id: id1,
                atom2_id: id2,
                atom1_symbol: atom_symbol(result_structure, id1),
                atom2_symbol: atom_symbol(result_structure, id2),
                is_bonded,
            }
        }
        MeasurementResult::Angle {
            angle_degrees,
            vertex_index,
        } => {
            let (arm_a_idx, arm_b_idx) = match vertex_index {
                0 => (1, 2),
                1 => (0, 2),
                _ => (0, 1),
            };
            let v_id = selected_atoms[vertex_index].result_atom_id;
            let a_id = selected_atoms[arm_a_idx].result_atom_id;
            let b_id = selected_atoms[arm_b_idx].result_atom_id;
            APIMeasurement::Angle {
                angle_degrees,
                vertex_id: v_id,
                vertex_symbol: atom_symbol(result_structure, v_id),
                arm_a_id: a_id,
                arm_a_symbol: atom_symbol(result_structure, a_id),
                arm_b_id: b_id,
                arm_b_symbol: atom_symbol(result_structure, b_id),
            }
        }
        MeasurementResult::Dihedral {
            angle_degrees,
            chain,
        } => {
            let a_id = selected_atoms[chain[0]].result_atom_id;
            let b_id = selected_atoms[chain[1]].result_atom_id;
            let c_id = selected_atoms[chain[2]].result_atom_id;
            let d_id = selected_atoms[chain[3]].result_atom_id;
            APIMeasurement::Dihedral {
                angle_degrees,
                chain_a_id: a_id,
                chain_a_symbol: atom_symbol(result_structure, a_id),
                chain_b_id: b_id,
                chain_b_symbol: atom_symbol(result_structure, b_id),
                chain_c_id: c_id,
                chain_c_symbol: atom_symbol(result_structure, c_id),
                chain_d_id: d_id,
                chain_d_symbol: atom_symbol(result_structure, d_id),
            }
        }
    })
}

/// Compute the result-space atom ID of the most recently selected atom.
fn compute_last_selected_result_atom_id(
    atom_edit_data: &AtomEditData,
    eval_cache: Option<&AtomEditEvalCache>,
    is_diff_view: bool,
) -> Option<u32> {
    use crate::structure_designer::nodes::atom_edit::atom_edit::SelectionProvenance;

    let last = atom_edit_data.selection.selection_order.last()?;
    let (prov, id) = *last;

    if is_diff_view {
        // In diff view, diff IDs are result IDs
        if prov == SelectionProvenance::Diff {
            Some(id)
        } else {
            None
        }
    } else {
        let cache = eval_cache?;
        match prov {
            SelectionProvenance::Base => cache.provenance.base_to_result.get(&id).copied(),
            SelectionProvenance::Diff => cache.provenance.diff_to_result.get(&id).copied(),
        }
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_parameter_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIParameterData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let parameter_data = node_data.as_any_ref().downcast_ref::<ParameterData>()?;

                let api_data_type = if parameter_data.data_type == DataType::None {
                    if let Some(dt_str) = &parameter_data.data_type_str {
                        // If parsing failed, reconstruct the APIDataType from the stored string
                        APIDataType {
                            data_type_base: APIDataTypeBase::Custom,
                            custom_data_type: Some(dt_str.clone()),
                            array: false, // This is inferred from the custom string itself
                            children: vec![],
                        }
                    } else {
                        // Fallback for safety
                        data_type_to_api_data_type(&parameter_data.data_type)
                    }
                } else {
                    // If parsing succeeded, convert as usual
                    data_type_to_api_data_type(&parameter_data.data_type)
                };

                Some(APIParameterData {
                    param_index: parameter_data.param_index,
                    param_name: parameter_data.param_name.clone(),
                    data_type: api_data_type,
                    sort_order: parameter_data.sort_order,
                    error: parameter_data.error.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_expr_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIExprData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let expr_data = match node_data.as_any_ref().downcast_ref::<ExprData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIExprData {
                    parameters: expr_data
                        .parameters
                        .iter()
                        .map(|param| {
                            let api_data_type = if param.data_type == DataType::None {
                                if let Some(dt_str) = &param.data_type_str {
                                    // If parsing failed, reconstruct the APIDataType from the stored string
                                    APIDataType {
                                        data_type_base: APIDataTypeBase::Custom,
                                        custom_data_type: Some(dt_str.clone()),
                                        array: false, // This is inferred from the custom string itself
                                        children: vec![],
                                    }
                                } else {
                                    // Fallback for safety, though this case should ideally not happen
                                    data_type_to_api_data_type(&param.data_type)
                                }
                            } else {
                                // If parsing succeeded, convert as usual
                                data_type_to_api_data_type(&param.data_type)
                            };

                            APIExprParameter {
                                name: param.name.clone(),
                                data_type: api_data_type,
                            }
                        })
                        .collect(),
                    expression: expr_data.expression.clone(),
                    error: expr_data.error.clone(),
                    output_type: expr_data
                        .output_type
                        .as_ref()
                        .map(data_type_to_api_data_type),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_map_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMapData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let map_data = node_data.as_any_ref().downcast_ref::<MapData>()?;

                Some(APIMapData {
                    input_type: data_type_to_api_data_type(&map_data.input_type),
                    output_type: data_type_to_api_data_type(&map_data.output_type),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_filter_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIFilterData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let filter_data = node_data.as_any_ref().downcast_ref::<FilterData>()?;

                Some(APIFilterData {
                    element_type: data_type_to_api_data_type(&filter_data.element_type),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_foreach_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIForeachData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let foreach_data = node_data.as_any_ref().downcast_ref::<ForeachData>()?;

                Some(APIForeachData {
                    input_type: data_type_to_api_data_type(&foreach_data.input_type),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_patch_build_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIPatchBuildData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let data = node_data.as_any_ref().downcast_ref::<PatchBuildData>()?;

                Some(APIPatchBuildData {
                    epsilon: data.epsilon,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_patch_latticefill_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIPatchLatticeFillData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let data = node_data
                    .as_any_ref()
                    .downcast_ref::<PatchLatticeFillData>()?;

                let report = data
                    .last_report
                    .borrow()
                    .as_ref()
                    .map(|r: &CompatibilityReport| APICompatibilityReport {
                        placed_cells: r.placed_cells,
                        welded_ghosts: r.welded_ghosts,
                        orphaned_ghosts: r.orphaned_ghosts,
                        overcoordinated_atoms: r.overcoordinated_atoms,
                    });

                Some(APIPatchLatticeFillData {
                    passivate: data.passivate,
                    tolerance: data.tolerance,
                    test_height_at_origin: data.test_height_at_origin,
                    debug_project_to_test_plane: data.debug_project_to_test_plane,
                    debug_show_frontier_tiles: data.debug_show_frontier_tiles,
                    report,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_collect_data(scope_path: Vec<u64>, node_id: u64) -> Option<APICollectData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let collect_data = node_data.as_any_ref().downcast_ref::<CollectData>()?;

                Some(APICollectData {
                    element_type: data_type_to_api_data_type(&collect_data.element_type),
                    limit: collect_data.limit,
                    offset: collect_data.offset,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_array_at_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIArrayAtData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let array_at_data = node_data.as_any_ref().downcast_ref::<ArrayAtData>()?;

                Some(APIArrayAtData {
                    element_type: data_type_to_api_data_type(&array_at_data.element_type),
                    index: array_at_data.index,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_fold_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIFoldData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let fold_data = node_data.as_any_ref().downcast_ref::<FoldData>()?;

                Some(APIFoldData {
                    element_type: data_type_to_api_data_type(&fold_data.element_type),
                    accumulator_type: data_type_to_api_data_type(&fold_data.accumulator_type),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_closure_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIClosureData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let closure_data = node_data.as_any_ref().downcast_ref::<ClosureData>()?;

                Some(APIClosureData {
                    kind: closure_kind_to_api_closure_kind(&closure_data.kind),
                    type_args: type_args_to_api(&closure_data.type_args),
                    param_names: closure_data.param_names.clone(),
                    custom_label: closure_data.custom_label.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_apply_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIApplyData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let apply_data = node_data.as_any_ref().downcast_ref::<ApplyData>()?;

                Some(APIApplyData {
                    kind: closure_kind_to_api_closure_kind(&apply_data.kind),
                    type_args: type_args_to_api(&apply_data.type_args),
                    param_names: apply_data.param_names.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_sequence_data(scope_path: Vec<u64>, node_id: u64) -> Option<APISequenceData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let seq_data = node_data.as_any_ref().downcast_ref::<SequenceData>()?;

                Some(APISequenceData {
                    element_type: data_type_to_api_data_type(&seq_data.element_type),
                    input_count: seq_data.input_count as i32,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_motif_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMotifData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let motif_data = node_data.as_any_ref().downcast_ref::<MotifData>()?;

                Some(APIMotifData {
                    definition: motif_data.definition.clone(),
                    name: motif_data.name.clone(),
                    error: motif_data.error.clone(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_int_data(scope_path: Vec<u64>, node_id: u64, data: APIIntData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let int_data = Box::new(IntData { value: data.value });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, int_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_string_data(scope_path: Vec<u64>, node_id: u64, data: APIStringData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let string_data = Box::new(StringData { value: data.value });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, string_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Writes the `schema` property of a `record_construct` node. After the
/// write, the node-network refresh re-runs the registry-aware cache
/// populator, which rebuilds the per-field input pin layout from the chosen
/// def. An empty string clears the schema. Preserves any existing
/// `literal_values` — entries whose keys no longer match a current field of
/// the new def are inert (see `RecordConstructData::literal_values`).
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_construct_data(scope_path: Vec<u64>, node_id: u64, data: APIRecordSchemaData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let existing = cad_instance
                .structure_designer
                .get_scope_network(&scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
                .cloned()
                .unwrap_or_default();
            let record_data = Box::new(RecordConstructData {
                schema: data.schema,
                literal_values: existing.literal_values,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, record_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Writes the `schema` property of a `record_destructure` node. Same
/// post-write behavior as `set_record_construct_data` — pin layout is
/// re-derived from the chosen def, dangling references leave the node with
/// a placeholder layout and broken downstream wires.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_destructure_data(scope_path: Vec<u64>, node_id: u64, data: APIRecordSchemaData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let record_data = Box::new(RecordDestructureData {
                schema: data.schema,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, record_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Writes the `target` property of a `product` node. The API's `schema`
/// field is mapped onto the underlying `ProductData.target`. After the
/// write, the registry-aware cache populator rebuilds the per-field
/// `Array[FieldType]` input pins and the `Array[Record(Named(target))]`
/// output pin from the chosen def.
#[flutter_rust_bridge::frb(sync)]
pub fn set_product_data(scope_path: Vec<u64>, node_id: u64, data: APIRecordSchemaData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let product_data = Box::new(ProductData {
                target: data.schema,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, product_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_bool_data(scope_path: Vec<u64>, node_id: u64, data: APIBoolData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let bool_data = Box::new(BoolData { value: data.value });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, bool_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_print_data(scope_path: Vec<u64>, node_id: u64, data: APIPrintData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let print_data = Box::new(PrintData {
                execute_only: data.execute_only,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, print_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_float_data(scope_path: Vec<u64>, node_id: u64, data: APIFloatData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let float_data = Box::new(FloatData { value: data.value });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, float_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_vec2_data(scope_path: Vec<u64>, node_id: u64, data: APIVec2Data) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let vec2_data = Box::new(Vec2Data {
                value: from_api_vec2(&data.value),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, vec2_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_vec3_data(scope_path: Vec<u64>, node_id: u64, data: APIVec3Data) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let vec3_data = Box::new(Vec3Data {
                value: from_api_vec3(&data.value),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, vec3_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_ivec2_data(scope_path: Vec<u64>, node_id: u64, data: APIIVec2Data) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let ivec2_data = Box::new(IVec2Data {
                value: from_api_ivec2(&data.value),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, ivec2_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_ivec3_data(scope_path: Vec<u64>, node_id: u64, data: APIIVec3Data) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let ivec3_data = Box::new(IVec3Data {
                value: from_api_ivec3(&data.value),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, ivec3_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_supercell_data(scope_path: Vec<u64>, node_id: u64, data: APISupercellData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_ivec3(&data.a);
            let b = from_api_ivec3(&data.b);
            let c = from_api_ivec3(&data.c);
            let supercell_data = Box::new(SupercellData {
                matrix: [[a.x, a.y, a.z], [b.x, b.y, b.z], [c.x, c.y, c.z]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, supercell_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_imat2_rows_data(scope_path: Vec<u64>, node_id: u64, data: APIIMat2RowsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_ivec2(&data.a);
            let b = from_api_ivec2(&data.b);
            let payload = Box::new(IMat2RowsData {
                matrix: [[a.x, a.y], [b.x, b.y]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_imat2_cols_data(scope_path: Vec<u64>, node_id: u64, data: APIIMat2ColsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_ivec2(&data.a);
            let b = from_api_ivec2(&data.b);
            let payload = Box::new(IMat2ColsData {
                matrix: [[a.x, b.x], [a.y, b.y]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_imat2_diag_data(scope_path: Vec<u64>, node_id: u64, data: APIIMat2DiagData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let payload = Box::new(IMat2DiagData {
                v: from_api_ivec2(&data.v),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_plane_tiling_vectors_data(
    scope_path: Vec<u64>,
    node_id: u64,
    data: APIPlaneTilingVectorsData,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_ivec2(&data.a);
            let b = from_api_ivec2(&data.b);
            let payload = Box::new(PlaneTilingVectorsData {
                matrix: [[a.x, a.y], [b.x, b.y]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_imat3_rows_data(scope_path: Vec<u64>, node_id: u64, data: APIIMat3RowsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_ivec3(&data.a);
            let b = from_api_ivec3(&data.b);
            let c = from_api_ivec3(&data.c);
            let payload = Box::new(IMat3RowsData {
                matrix: [[a.x, a.y, a.z], [b.x, b.y, b.z], [c.x, c.y, c.z]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_imat3_cols_data(scope_path: Vec<u64>, node_id: u64, data: APIIMat3ColsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_ivec3(&data.a);
            let b = from_api_ivec3(&data.b);
            let c = from_api_ivec3(&data.c);
            let payload = Box::new(IMat3ColsData {
                matrix: [[a.x, b.x, c.x], [a.y, b.y, c.y], [a.z, b.z, c.z]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_imat3_diag_data(scope_path: Vec<u64>, node_id: u64, data: APIIMat3DiagData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let payload = Box::new(IMat3DiagData {
                v: from_api_ivec3(&data.v),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_mat3_rows_data(scope_path: Vec<u64>, node_id: u64, data: APIMat3RowsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_vec3(&data.a);
            let b = from_api_vec3(&data.b);
            let c = from_api_vec3(&data.c);
            let payload = Box::new(Mat3RowsData {
                matrix: [[a.x, a.y, a.z], [b.x, b.y, b.z], [c.x, c.y, c.z]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_mat3_cols_data(scope_path: Vec<u64>, node_id: u64, data: APIMat3ColsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let a = from_api_vec3(&data.a);
            let b = from_api_vec3(&data.b);
            let c = from_api_vec3(&data.c);
            let payload = Box::new(Mat3ColsData {
                matrix: [[a.x, b.x, c.x], [a.y, b.y, c.y], [a.z, b.z, c.z]],
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_mat3_diag_data(scope_path: Vec<u64>, node_id: u64, data: APIMat3DiagData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let payload = Box::new(Mat3DiagData {
                v: from_api_vec3(&data.v),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, payload);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_range_data(scope_path: Vec<u64>, node_id: u64, data: APIRangeData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let range_data = Box::new(RangeData {
                start: data.start,
                step: data.step,
                count: data.count,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, range_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_rect_data(scope_path: Vec<u64>, node_id: u64, data: APIRectData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let rect_data = Box::new(RectData {
                min_corner: from_api_ivec2(&data.min_corner),
                extent: from_api_ivec2(&data.extent),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, rect_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_reg_poly_data(scope_path: Vec<u64>, node_id: u64, data: APIRegPolyData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let reg_poly_data = Box::new(RegPolyData {
                num_sides: data.num_sides,
                radius: data.radius,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, reg_poly_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_circle_data(scope_path: Vec<u64>, node_id: u64, data: APICircleData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let circle_data = Box::new(CircleData {
                center: from_api_ivec2(&data.center),
                radius: data.radius,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, circle_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_half_plane_data(scope_path: Vec<u64>, node_id: u64, data: APIHalfPlaneData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let half_plane_data = Box::new(HalfPlaneData {
                point1: from_api_ivec2(&data.point1),
                point2: from_api_ivec2(&data.point2),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, half_plane_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_extrude_data(scope_path: Vec<u64>, node_id: u64, data: APIExtrudeData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let extrude_data = Box::new(ExtrudeData {
                height: data.height,
                extrude_direction: from_api_ivec3(&data.extrude_direction),
                infinite: data.infinite,
                subdivision: data.subdivision,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, extrude_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_cuboid_data(scope_path: Vec<u64>, node_id: u64, data: APICuboidData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let cuboid_data = Box::new(CuboidData {
                min_corner: from_api_ivec3(&data.min_corner),
                extent: from_api_ivec3(&data.extent),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, cuboid_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_sphere_data(scope_path: Vec<u64>, node_id: u64, data: APISphereData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let sphere_data = Box::new(SphereData {
                center: from_api_ivec3(&data.center),
                radius: data.radius,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, sphere_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_half_space_data(scope_path: Vec<u64>, node_id: u64, data: APIHalfSpaceData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let half_space_data = Box::new(HalfSpaceData {
                max_miller_index: data.max_miller_index,
                miller_index: from_api_ivec3(&data.miller_index),
                center: from_api_ivec3(&data.center),
                shift: data.shift,
                subdivision: data.subdivision,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, half_space_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_drawing_plane_data(scope_path: Vec<u64>, node_id: u64, data: APIDrawingPlaneData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let drawing_plane_data = Box::new(DrawingPlaneData {
                max_miller_index: data.max_miller_index,
                miller_index: data.miller_index.as_ref().map(from_api_ivec3),
                center: from_api_ivec3(&data.center),
                shift: data.shift,
                subdivision: data.subdivision,
                u_axis: data.u_axis.as_ref().map(from_api_ivec3),
                v_axis: data.v_axis.as_ref().map(from_api_ivec3),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, drawing_plane_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_geo_trans_data(scope_path: Vec<u64>, node_id: u64, data: APIGeoTransData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let geo_trans_data = Box::new(GeoTransData {
                transform_only_frame: data.transform_only_frame,
                translation: from_api_ivec3(&data.translation),
                rotation: from_api_ivec3(&data.rotation),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, geo_trans_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_lattice_symop_data(scope_path: Vec<u64>, node_id: u64, data: APILatticeSymopData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let lattice_symop_data = Box::new(LatticeSymopData {
                translation: from_api_ivec3(&data.translation),
                rotation_axis: data.rotation_axis.map(|axis| from_api_vec3(&axis)),
                rotation_angle_degrees: data.rotation_angle_degrees,
                transform_only_frame: data.transform_only_frame,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, lattice_symop_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_structure_move_data(scope_path: Vec<u64>, node_id: u64, data: APIStructureMoveData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let structure_move_data = Box::new(StructureMoveData {
                translation: from_api_ivec3(&data.translation),
                lattice_subdivision: data.lattice_subdivision,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, structure_move_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_structure_rot_data(scope_path: Vec<u64>, node_id: u64, data: APIStructureRotData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let structure_rot_data = Box::new(StructureRotData {
                axis_index: data.axis_index,
                step: data.step,
                pivot_point: from_api_ivec3(&data.pivot_point),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, structure_rot_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_free_move_data(scope_path: Vec<u64>, node_id: u64, data: APIFreeMoveData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let free_move_data = Box::new(FreeMoveData {
                translation: from_api_vec3(&data.translation),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, free_move_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_free_rot_data(scope_path: Vec<u64>, node_id: u64, data: APIFreeRotData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let free_rot_data = Box::new(FreeRotData {
                angle: data.angle,
                rot_axis: from_api_vec3(&data.rot_axis),
                pivot_point: from_api_vec3(&data.pivot_point),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, free_rot_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_cut_data(scope_path: Vec<u64>, node_id: u64, data: APIAtomCutData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let atom_cut_data = Box::new(AtomCutData {
                cut_sdf_value: data.cut_sdf_value,
                unit_cell_size: data.unit_cell_size,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, atom_cut_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_apply_diff_data(scope_path: Vec<u64>, node_id: u64, data: APIApplyDiffData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let apply_diff_data = Box::new(ApplyDiffData {
                tolerance: data.tolerance,
                error_on_stale: data.error_on_stale,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, apply_diff_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_composediff_data(scope_path: Vec<u64>, node_id: u64, data: APIAtomComposeDiffData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let composediff_data = Box::new(AtomComposeDiffData {
                tolerance: data.tolerance,
                error_on_stale: data.error_on_stale,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, composediff_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_import_xyz_data(scope_path: Vec<u64>, node_id: u64, data: APIImportXYZData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let import_xyz_data = Box::new(ImportXYZData {
                file_name: data.file_name.clone(),
                atomic_structure: None,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, import_xyz_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_import_cif_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIImportCIFData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let import_cif_data = match node_data.as_any_ref().downcast_ref::<ImportCifData>() {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIImportCIFData {
                    file_name: import_cif_data.file_name.clone(),
                    block_name: import_cif_data.block_name.clone(),
                    use_cif_bonds: import_cif_data.use_cif_bonds,
                    infer_bonds: import_cif_data.infer_bonds,
                    bond_tolerance: import_cif_data.bond_tolerance,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_import_cif_data(scope_path: Vec<u64>, node_id: u64, data: APIImportCIFData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let import_cif_data = Box::new(ImportCifData {
                file_name: data.file_name.clone(),
                block_name: data.block_name.clone(),
                use_cif_bonds: data.use_cif_bonds,
                infer_bonds: data.infer_bonds,
                bond_tolerance: data.bond_tolerance,
                cached_result: None,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, import_cif_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_infer_bonds_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIInferBondsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let infer_bonds_data = match node_data.as_any_ref().downcast_ref::<InferBondsData>()
                {
                    Some(data) => data,
                    None => return None,
                };
                Some(APIInferBondsData {
                    additive: infer_bonds_data.additive,
                    bond_tolerance: infer_bonds_data.bond_tolerance,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_infer_bonds_data(scope_path: Vec<u64>, node_id: u64, data: APIInferBondsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let infer_bonds_data = Box::new(InferBondsData {
                additive: data.additive,
                bond_tolerance: data.bond_tolerance,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, infer_bonds_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_atom_replace_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIAtomReplaceData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let atom_replace_data =
                    match node_data.as_any_ref().downcast_ref::<AtomReplaceData>() {
                        Some(data) => data,
                        None => return None,
                    };
                Some(APIAtomReplaceData {
                    replacements: atom_replace_data
                        .replacements
                        .iter()
                        .map(|(from, to)| APIAtomReplaceRule {
                            from_atomic_number: *from as i32,
                            to_atomic_number: *to as i32,
                        })
                        .collect(),
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_atom_replace_data(scope_path: Vec<u64>, node_id: u64, data: APIAtomReplaceData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let atom_replace_data = Box::new(AtomReplaceData {
                replacements: data
                    .replacements
                    .iter()
                    .map(|r| (r.from_atomic_number as i16, r.to_atomic_number as i16))
                    .collect(),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, atom_replace_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_export_xyz_data(scope_path: Vec<u64>, node_id: u64, data: APIExportXYZData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let export_xyz_data = Box::new(ExportXYZData {
                file_name: data.file_name.clone(),
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, export_xyz_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_parameter_data(scope_path: Vec<u64>, node_id: u64, data: APIParameterData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let (data_type, data_type_str, error) =
                match api_data_type_to_data_type(&data.data_type) {
                    Ok(parsed_data_type) => (parsed_data_type, None, None),
                    Err(e) => (
                        DataType::None,                  // Set to None on error
                        data.data_type.custom_data_type, // Preserve the original string
                        Some(e),
                    ),
                };

            // Preserve existing param_id from the current node data (for wire preservation across renames)
            let existing_param_id = cad_instance
                .structure_designer
                .get_scope_network(&scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<ParameterData>())
                .and_then(|param_data| param_data.param_id);

            let parameter_data = Box::new(ParameterData {
                param_id: existing_param_id, // Preserve existing ID for wire preservation
                param_index: data.param_index,
                param_name: data.param_name,
                data_type,
                sort_order: data.sort_order,
                data_type_str,
                error,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, parameter_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

// --- Custom node property panel ---

/// Maps a `DataType` to its `APISimpleParamType`, or `None` for any pin type
/// that the custom-node panel does not render inline.
fn data_type_to_simple_param_type(data_type: &DataType) -> Option<APISimpleParamType> {
    Some(match data_type {
        DataType::Bool => APISimpleParamType::Bool,
        DataType::Int => APISimpleParamType::Int,
        DataType::Float => APISimpleParamType::Float,
        DataType::String => APISimpleParamType::Str,
        DataType::IVec2 => APISimpleParamType::IVec2,
        DataType::IVec3 => APISimpleParamType::IVec3,
        DataType::Vec2 => APISimpleParamType::Vec2,
        DataType::Vec3 => APISimpleParamType::Vec3,
        DataType::IMat3 => APISimpleParamType::IMat3,
        DataType::Mat3 => APISimpleParamType::Mat3,
        _ => return None,
    })
}

fn rows_i32_to_vecs(m: &[[i32; 3]; 3]) -> Vec<Vec<i32>> {
    m.iter().map(|r| r.to_vec()).collect()
}

fn rows_f64_to_vecs(m: &[[f64; 3]; 3]) -> Vec<Vec<f64>> {
    m.iter().map(|r| r.to_vec()).collect()
}

/// Pads/truncates a (possibly malformed) `Vec<Vec<_>>` from FFI into a fixed
/// row-major 3x3 array. Missing cells default to `0`.
fn vecs_to_rows_i32(rows: Vec<Vec<i32>>) -> [[i32; 3]; 3] {
    let mut out = [[0i32; 3]; 3];
    for (r, row) in rows.into_iter().take(3).enumerate() {
        for (c, val) in row.into_iter().take(3).enumerate() {
            out[r][c] = val;
        }
    }
    out
}

fn vecs_to_rows_f64(rows: Vec<Vec<f64>>) -> [[f64; 3]; 3] {
    let mut out = [[0.0f64; 3]; 3];
    for (r, row) in rows.into_iter().take(3).enumerate() {
        for (c, val) in row.into_iter().take(3).enumerate() {
            out[r][c] = val;
        }
    }
    out
}

/// Converts a stored `TextValue` to an `APILiteralValue` for the parameter's
/// current `data_type`. Returns `None` when the stored value cannot be coerced
/// to that type — the parameter was retyped in the subnetwork and the stale
/// literal should render as a placeholder instead.
fn text_value_to_api_literal(value: &TextValue, data_type: &DataType) -> Option<APILiteralValue> {
    Some(match data_type {
        DataType::Bool => APILiteralValue::Bool(value.as_bool()?),
        DataType::Int => APILiteralValue::Int(value.as_int()?),
        DataType::Float => APILiteralValue::Float(value.as_float()?),
        DataType::String => APILiteralValue::Str(value.as_string()?.to_string()),
        DataType::IVec2 => {
            let v = value.as_ivec2()?;
            APILiteralValue::IVec2(APIIVec2 { x: v.x, y: v.y })
        }
        DataType::IVec3 => {
            let v = value.as_ivec3()?;
            APILiteralValue::IVec3(APIIVec3 {
                x: v.x,
                y: v.y,
                z: v.z,
            })
        }
        DataType::Vec2 => {
            let v = value.as_vec2()?;
            APILiteralValue::Vec2(APIVec2 { x: v.x, y: v.y })
        }
        DataType::Vec3 => {
            let v = value.as_vec3()?;
            APILiteralValue::Vec3(APIVec3 {
                x: v.x,
                y: v.y,
                z: v.z,
            })
        }
        DataType::IMat3 => APILiteralValue::IMat3(rows_i32_to_vecs(&value.as_imat3()?)),
        DataType::Mat3 => APILiteralValue::Mat3(rows_f64_to_vecs(&value.as_mat3()?)),
        _ => return None,
    })
}

/// Converts a resolved `default`-pin `NetworkResult` to an `APILiteralValue`
/// for the parameter's `data_type`. Returns `None` for non-simple results.
fn network_result_to_api_literal(
    result: &NetworkResult,
    data_type: &DataType,
) -> Option<APILiteralValue> {
    Some(match (result, data_type) {
        (NetworkResult::Bool(b), DataType::Bool) => APILiteralValue::Bool(*b),
        (NetworkResult::Int(i), DataType::Int) => APILiteralValue::Int(*i),
        (NetworkResult::Float(f), DataType::Float) => APILiteralValue::Float(*f),
        (NetworkResult::Int(i), DataType::Float) => APILiteralValue::Float(*i as f64),
        (NetworkResult::Float(f), DataType::Int) => APILiteralValue::Int(*f as i32),
        (NetworkResult::String(s), DataType::String) => APILiteralValue::Str(s.clone()),
        (NetworkResult::IVec2(v), DataType::IVec2) => {
            APILiteralValue::IVec2(APIIVec2 { x: v.x, y: v.y })
        }
        (NetworkResult::IVec3(v), DataType::IVec3) => APILiteralValue::IVec3(APIIVec3 {
            x: v.x,
            y: v.y,
            z: v.z,
        }),
        (NetworkResult::Vec2(v), DataType::Vec2) => {
            APILiteralValue::Vec2(APIVec2 { x: v.x, y: v.y })
        }
        (NetworkResult::Vec3(v), DataType::Vec3) => APILiteralValue::Vec3(APIVec3 {
            x: v.x,
            y: v.y,
            z: v.z,
        }),
        (NetworkResult::IMat3(m), DataType::IMat3) => APILiteralValue::IMat3(rows_i32_to_vecs(m)),
        (NetworkResult::Mat3(m), DataType::Mat3) => {
            APILiteralValue::Mat3(rows_f64_to_vecs(&dmat3_to_rows(m)))
        }
        _ => return None,
    })
}

/// Converts an `APILiteralValue` from FFI into the `TextValue` stored in
/// `CustomNodeData.literal_values`.
fn api_literal_to_text_value(value: APILiteralValue) -> TextValue {
    match value {
        APILiteralValue::Bool(b) => TextValue::Bool(b),
        APILiteralValue::Int(i) => TextValue::Int(i),
        APILiteralValue::Float(f) => TextValue::Float(f),
        APILiteralValue::Str(s) => TextValue::String(s),
        APILiteralValue::IVec2(v) => TextValue::IVec2(IVec2::new(v.x, v.y)),
        APILiteralValue::IVec3(v) => TextValue::IVec3(IVec3::new(v.x, v.y, v.z)),
        APILiteralValue::Vec2(v) => TextValue::Vec2(DVec2::new(v.x, v.y)),
        APILiteralValue::Vec3(v) => TextValue::Vec3(DVec3::new(v.x, v.y, v.z)),
        APILiteralValue::IMat3(rows) => TextValue::IMat3(vecs_to_rows_i32(rows)),
        APILiteralValue::Mat3(rows) => TextValue::Mat3(vecs_to_rows_f64(rows)),
    }
}

/// Returns `None` if `node_id` is not a custom node (its `node_type_name` is
/// not a key in `registry.node_networks`). Returns `Some(vec)` — possibly
/// empty — for a custom node, listing only its simple-typed parameters in pin
/// order.
///
/// Runs through `with_mut_cad_instance`: resolving each parameter's default
/// (`resolve_parameter_default`) evaluates the subnetwork and needs `&mut`.
#[flutter_rust_bridge::frb(sync)]
pub fn get_custom_node_params(scope_path: Vec<u64>, node_id: u64) -> Option<Vec<APILiteralField>> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let sd = &mut cad_instance.structure_designer;

                // Gather all immutable info first; the borrows of `sd` end
                // with this block, before the `&mut self` default-resolution
                // pass below.
                let (subnetwork_name, params_info) = {
                    let network = sd.get_scope_network(&scope_path)?;
                    let node = network.nodes.get(&node_id)?;
                    // A custom node's `node_type_name` is a key in `node_networks`.
                    if !sd
                        .node_type_registry
                        .node_networks
                        .contains_key(&node.node_type_name)
                    {
                        return None;
                    }
                    let subnetwork_name = node.node_type_name.clone();
                    let node_type = sd.node_type_registry.get_node_type_for_node(node)?;
                    let custom_data = node.data.as_any_ref().downcast_ref::<CustomNodeData>();

                    let mut params_info = Vec::new();
                    for (i, param) in node_type.parameters.iter().enumerate() {
                        let Some(simple_type) = data_type_to_simple_param_type(&param.data_type)
                        else {
                            continue;
                        };
                        let is_wired = node
                            .arguments
                            .get(i)
                            .map(|arg| !arg.is_empty())
                            .unwrap_or(false);
                        let stored_value = custom_data
                            .and_then(|cd| cd.literal_values.get(&param.name))
                            .and_then(|tv| text_value_to_api_literal(tv, &param.data_type));
                        params_info.push((
                            param.name.clone(),
                            param.data_type.clone(),
                            simple_type,
                            is_wired,
                            stored_value,
                        ));
                    }
                    (subnetwork_name, params_info)
                };

                // Resolve each parameter's default pin (needs `&mut self`).
                let mut result = Vec::with_capacity(params_info.len());
                for (name, data_type, simple_type, is_wired, stored_value) in params_info {
                    let default_value = sd
                        .resolve_parameter_default(&subnetwork_name, &name)
                        .and_then(|nr| network_result_to_api_literal(&nr, &data_type));
                    result.push(APILiteralField {
                        name,
                        data_type: simple_type,
                        stored_value,
                        default_value,
                        is_wired,
                    });
                }
                Some(result)
            },
            None,
        )
    }
}

/// Inserts/updates `literal_values[param_name]` on a custom node. Goes through
/// `set_node_network_data`, so it gets the existing `SetNodeData` undo command
/// and `refresh_structure_designer_auto` for free.
#[flutter_rust_bridge::frb(sync)]
pub fn set_custom_node_literal(
    scope_path: Vec<u64>,
    node_id: u64,
    param_name: String,
    value: APILiteralValue,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let existing = cad_instance
                .structure_designer
                .get_scope_network(&scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<CustomNodeData>())
                .cloned();
            let Some(mut data) = existing else {
                return;
            };
            data.literal_values
                .insert(param_name, api_literal_to_text_value(value));
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, Box::new(data));
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Removes `literal_values[param_name]` from a custom node. Same
/// `set_node_network_data` path as `set_custom_node_literal`.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_custom_node_literal(scope_path: Vec<u64>, node_id: u64, param_name: String) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let existing = cad_instance
                .structure_designer
                .get_scope_network(&scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<CustomNodeData>())
                .cloned();
            let Some(mut data) = existing else {
                return;
            };
            data.literal_values.remove(&param_name);
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, Box::new(data));
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Returns `None` if `node_id` is not a `record_construct` node, or if its
/// chosen `schema` is empty / not in the registry. Returns `Some(vec)` —
/// possibly empty — listing only the def's simple-typed fields, in authored
/// field order.
///
/// Pure read — `&self`, no subnetwork evaluation. Safe to call on every
/// panel rebuild.
#[flutter_rust_bridge::frb(sync)]
pub fn get_record_construct_fields(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<Vec<APILiteralField>> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let sd = &cad_instance.structure_designer;
                let network = sd.get_scope_network(&scope_path)?;
                let node = network.nodes.get(&node_id)?;
                if node.node_type_name != "record_construct" {
                    return None;
                }
                let data = node
                    .data
                    .as_any_ref()
                    .downcast_ref::<RecordConstructData>()?;
                let def = sd.node_type_registry.lookup_record_type_def(&data.schema)?;

                let mut result = Vec::new();
                for (i, field) in def.fields.iter().enumerate() {
                    let field_name = &field.name;
                    let field_type = &field.data_type;
                    // An `Optional[T]` field is exposed at the value/literal layer
                    // as a plain `T` (Core Decision 2). Peel the Optional so the
                    // literal editor renders `T`'s input; the tri-state "unset"
                    // (no `literal_values` entry, no wire ⇒ `None` ⇒ inherit) is
                    // already handled by the editor's placeholder/clear states.
                    // See `doc/design_optional_type.md` §5.
                    let value_type = field_type.record_field_pin_type();
                    let Some(simple_type) = data_type_to_simple_param_type(&value_type) else {
                        continue;
                    };
                    let is_wired = node
                        .arguments
                        .get(i)
                        .map(|arg| !arg.is_empty())
                        .unwrap_or(false);
                    let stored_value = data
                        .literal_values
                        .get(field_name)
                        .and_then(|tv| text_value_to_api_literal(tv, &value_type));
                    result.push(APILiteralField {
                        name: field_name.clone(),
                        data_type: simple_type,
                        stored_value,
                        // No default layer for record_construct fields.
                        default_value: None,
                        is_wired,
                    });
                }
                Some(result)
            },
            None,
        )
    }
}

/// Inserts/updates `RecordConstructData.literal_values[field_name]`. Routes
/// through `set_node_network_data`, picking up the existing `SetNodeData`
/// undo command and `refresh_structure_designer_auto` for free.
#[flutter_rust_bridge::frb(sync)]
pub fn set_record_construct_literal(
    scope_path: Vec<u64>,
    node_id: u64,
    field_name: String,
    value: APILiteralValue,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let existing = cad_instance
                .structure_designer
                .get_scope_network(&scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
                .cloned();
            let Some(mut data) = existing else {
                return;
            };
            data.literal_values
                .insert(field_name, api_literal_to_text_value(value));
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, Box::new(data));
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Removes `RecordConstructData.literal_values[field_name]`. Same
/// `set_node_network_data` path as `set_record_construct_literal`.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_record_construct_literal(scope_path: Vec<u64>, node_id: u64, field_name: String) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let existing = cad_instance
                .structure_designer
                .get_scope_network(&scope_path)
                .and_then(|network| network.nodes.get(&node_id))
                .and_then(|node| node.data.as_any_ref().downcast_ref::<RecordConstructData>())
                .cloned();
            let Some(mut data) = existing else {
                return;
            };
            data.literal_values.remove(&field_name);
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, Box::new(data));
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_map_data(scope_path: Vec<u64>, node_id: u64, data: APIMapData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let input_type = match api_data_type_to_data_type(&data.input_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None, // Fallback to None on error
            };

            let output_type = match api_data_type_to_data_type(&data.output_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None, // Fallback to None on error
            };

            let map_data = Box::new(MapData {
                input_type,
                output_type,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, map_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_filter_data(scope_path: Vec<u64>, node_id: u64, data: APIFilterData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let element_type = match api_data_type_to_data_type(&data.element_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let filter_data = Box::new(FilterData { element_type });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, filter_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_foreach_data(scope_path: Vec<u64>, node_id: u64, data: APIForeachData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let input_type = match api_data_type_to_data_type(&data.input_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let foreach_data = Box::new(ForeachData { input_type });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, foreach_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_patch_build_data(scope_path: Vec<u64>, node_id: u64, data: APIPatchBuildData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let patch_build_data = Box::new(PatchBuildData {
                epsilon: data.epsilon,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, patch_build_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_patch_latticefill_data(
    scope_path: Vec<u64>,
    node_id: u64,
    data: APIPatchLatticeFillData,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            // The cached compatibility report is transient; it repopulates on
            // the next evaluation, so a fresh default is correct here.
            let patch_data = Box::new(PatchLatticeFillData {
                passivate: data.passivate,
                tolerance: data.tolerance,
                test_height_at_origin: data.test_height_at_origin,
                debug_project_to_test_plane: data.debug_project_to_test_plane,
                debug_show_frontier_tiles: data.debug_show_frontier_tiles,
                ..Default::default()
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, patch_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_collect_data(scope_path: Vec<u64>, node_id: u64, data: APICollectData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let element_type = match api_data_type_to_data_type(&data.element_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let collect_data = Box::new(CollectData {
                element_type,
                limit: data.limit,
                offset: data.offset,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, collect_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_array_at_data(scope_path: Vec<u64>, node_id: u64, data: APIArrayAtData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let element_type = match api_data_type_to_data_type(&data.element_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let array_at_data = Box::new(ArrayAtData {
                element_type,
                index: data.index,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, array_at_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_fold_data(scope_path: Vec<u64>, node_id: u64, data: APIFoldData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let element_type = match api_data_type_to_data_type(&data.element_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let accumulator_type = match api_data_type_to_data_type(&data.accumulator_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let fold_data = Box::new(FoldData {
                element_type,
                accumulator_type,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, fold_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_closure_data(scope_path: Vec<u64>, node_id: u64, data: APIClosureData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let closure_data = Box::new(ClosureData {
                kind: api_closure_kind_to_closure_kind(&data.kind),
                type_args: api_to_type_args(&data.type_args),
                param_names: data.param_names.clone(),
                custom_label: data.custom_label.clone(),
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, closure_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_apply_data(scope_path: Vec<u64>, node_id: u64, data: APIApplyData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let apply_data = Box::new(ApplyData {
                kind: api_closure_kind_to_closure_kind(&data.kind),
                type_args: api_to_type_args(&data.type_args),
                param_names: data.param_names.clone(),
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, apply_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_sequence_data(scope_path: Vec<u64>, node_id: u64, data: APISequenceData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let element_type = match api_data_type_to_data_type(&data.element_type) {
                Ok(parsed_data_type) => parsed_data_type,
                Err(_) => DataType::None,
            };

            let input_count = (data.input_count as usize).max(1);

            let seq_data = Box::new(SequenceData {
                element_type,
                input_count,
            });

            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, seq_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_expr_data(scope_path: Vec<u64>, node_id: u64, data: APIExprData) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                // Get existing expr data to preserve parameter IDs for wire preservation
                let existing_params: Vec<crate::structure_designer::nodes::expr::ExprParameter> =
                    cad_instance
                        .structure_designer
                        .get_scope_network(&scope_path)
                        .and_then(|network| network.nodes.get(&node_id))
                        .and_then(|node| node.data.as_any_ref().downcast_ref::<ExprData>())
                        .map(|expr_data| expr_data.parameters.clone())
                        .unwrap_or_default();

                // Find the next available ID for new parameters
                let mut next_id = existing_params
                    .iter()
                    .filter_map(|p| p.id)
                    .max()
                    .unwrap_or(0)
                    + 1;

                // Track which IDs have been assigned to avoid duplicates
                let mut used_ids = std::collections::HashSet::new();

                let mut parameters = Vec::new();
                let mut first_error = None;

                for (new_index, api_param) in data.parameters.into_iter().enumerate() {
                    // Preserve ID: first match by name (handles reordering), then by position (handles renames)
                    // Also check that the ID hasn't already been used (prevents duplicates when reordering + adding)
                    let id = if let Some(existing) =
                        existing_params.iter().find(|p| p.name == api_param.name)
                    {
                        // Match by name first (handles reordering)
                        if let Some(existing_id) = existing.id {
                            if !used_ids.contains(&existing_id) {
                                existing.id
                            } else {
                                // ID already used, generate new one
                                let id = next_id;
                                next_id += 1;
                                Some(id)
                            }
                        } else {
                            existing.id
                        }
                    } else if new_index < existing_params.len()
                        && existing_params[new_index].id.is_some()
                    {
                        // Fall back to position (handles renames - name changed but position same)
                        let pos_id = existing_params[new_index].id.unwrap();
                        if !used_ids.contains(&pos_id) {
                            existing_params[new_index].id
                        } else {
                            // ID already used, generate new one
                            let id = next_id;
                            next_id += 1;
                            Some(id)
                        }
                    } else {
                        // New parameter - generate new ID
                        let id = next_id;
                        next_id += 1;
                        Some(id)
                    };

                    // Track the assigned ID
                    if let Some(assigned_id) = id {
                        used_ids.insert(assigned_id);
                    }

                    match api_data_type_to_data_type(&api_param.data_type) {
                        Ok(dt) => {
                            parameters.push(
                                crate::structure_designer::nodes::expr::ExprParameter {
                                    id,
                                    name: api_param.name,
                                    data_type: dt,
                                    data_type_str: None, // Successfully parsed, no need to store the string
                                },
                            );
                        }
                        Err(e) => {
                            if first_error.is_none() {
                                first_error = Some(e.clone());
                            }
                            parameters.push(
                                crate::structure_designer::nodes::expr::ExprParameter {
                                    id,
                                    name: api_param.name,
                                    data_type: DataType::None, // Set to None on error
                                    data_type_str: if api_param.data_type.data_type_base
                                        == APIDataTypeBase::Custom
                                    {
                                        api_param.data_type.custom_data_type
                                    } else {
                                        None
                                    },
                                },
                            );
                        }
                    }
                }

                let expr_data = Box::new(ExprData {
                    parameters,
                    expression: data.expression,
                    expr: None,
                    error: first_error,
                    output_type: None,
                });

                cad_instance
                    .structure_designer
                    .set_node_network_data_scoped(&scope_path, node_id, expr_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult {
                    success: true,
                    error_message: String::new(),
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_motif_data(scope_path: Vec<u64>, node_id: u64, data: APIMotifData) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut motif_data = Box::new(MotifData {
                    definition: data.definition,
                    name: data.name,
                    motif: None,
                    error: None,
                });
                motif_data.parse_and_validate(node_id);
                cad_instance
                    .structure_designer
                    .set_node_network_data_scoped(&scope_path, node_id, motif_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult {
                    success: true,
                    error_message: String::new(),
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_materialize_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMaterializeData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let materialize_data = node_data.as_any_ref().downcast_ref::<MaterializeData>()?;

                use crate::crystolecule::atomic_constants::ATOM_INFO;

                let available_parameters = materialize_data
                    .available_parameters
                    .borrow()
                    .iter()
                    .map(|(name, atomic_number)| {
                        let symbol = ATOM_INFO
                            .get(&(*atomic_number as i32))
                            .map(|info| info.symbol.clone())
                            .unwrap_or_else(|| format!("Z{}", atomic_number));
                        APIMotifParameterInfo {
                            name: name.clone(),
                            default_atomic_number: *atomic_number,
                            default_element_symbol: symbol,
                        }
                    })
                    .collect();

                Some(APIMaterializeData {
                    parameter_element_value_definition: materialize_data
                        .parameter_element_value_definition
                        .clone(),
                    hydrogen_passivation: materialize_data.hydrogen_passivation,
                    remove_unbonded_atoms: materialize_data.remove_unbonded_atoms,
                    remove_single_bond_atoms_before_passivation: materialize_data
                        .remove_single_bond_atoms_before_passivation,
                    surface_reconstruction: materialize_data.surface_reconstruction,
                    invert_phase: materialize_data.invert_phase,
                    error: materialize_data.error.clone(),
                    available_parameters,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_materialize_data(
    scope_path: Vec<u64>,
    node_id: u64,
    data: APIMaterializeData,
) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut materialize_data = Box::new(MaterializeData {
                    parameter_element_value_definition: data.parameter_element_value_definition,
                    hydrogen_passivation: data.hydrogen_passivation,
                    remove_unbonded_atoms: data.remove_unbonded_atoms,
                    remove_single_bond_atoms_before_passivation: data
                        .remove_single_bond_atoms_before_passivation,
                    surface_reconstruction: data.surface_reconstruction,
                    invert_phase: data.invert_phase,
                    error: None,
                    parameter_element_values: HashMap::new(),
                    available_parameters: std::cell::RefCell::new(Vec::new()),
                });
                materialize_data.parse_and_validate(node_id);
                cad_instance
                    .structure_designer
                    .set_node_network_data_scoped(&scope_path, node_id, materialize_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult {
                    success: true,
                    error_message: String::new(),
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_motif_sub_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMotifSubData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let data = node_data.as_any_ref().downcast_ref::<MotifSubData>()?;

                use crate::crystolecule::atomic_constants::ATOM_INFO;

                let available_parameters = data
                    .available_parameters
                    .borrow()
                    .iter()
                    .map(|(name, atomic_number)| {
                        let symbol = ATOM_INFO
                            .get(&(*atomic_number as i32))
                            .map(|info| info.symbol.clone())
                            .unwrap_or_else(|| format!("Z{}", atomic_number));
                        APIMotifParameterInfo {
                            name: name.clone(),
                            default_atomic_number: *atomic_number,
                            default_element_symbol: symbol,
                        }
                    })
                    .collect();

                Some(APIMotifSubData {
                    parameter_element_value_definition: data
                        .parameter_element_value_definition
                        .clone(),
                    error: data.error.clone(),
                    available_parameters,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_motif_sub_data(scope_path: Vec<u64>, node_id: u64, data: APIMotifSubData) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut motif_sub_data = Box::new(MotifSubData {
                    parameter_element_value_definition: data.parameter_element_value_definition,
                    error: None,
                    parameter_element_values: HashMap::new(),
                    available_parameters: std::cell::RefCell::new(Vec::new()),
                });
                motif_sub_data.parse_and_validate(node_id);
                cad_instance
                    .structure_designer
                    .set_node_network_data_scoped(&scope_path, node_id, motif_sub_data);
                refresh_structure_designer_auto(cad_instance);

                APIResult {
                    success: true,
                    error_message: String::new(),
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_selected(scope_path: Vec<u64>) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .delete_selected_scoped(&scope_path);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Set the return node of the active *top-level* network (a body has no
/// return — its outputs flow through the HOF's zone-output pins). Per
/// `doc/design_zones_ui.md` §"Mutation APIs grow a `scope_path` parameter",
/// `scope_path` is plumbed for shape but must be empty.
#[flutter_rust_bridge::frb(sync)]
pub fn set_return_node_id(scope_path: Vec<u64>, node_id: Option<u64>) -> bool {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                if !scope_path.is_empty() {
                    return false;
                }
                let result = cad_instance.structure_designer.set_return_node_id(node_id);
                refresh_structure_designer_auto(cad_instance);
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn save_node_networks_as(file_path: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                // Call the method in StructureDesigner
                match cad_instance
                    .structure_designer
                    .save_node_networks_as(&file_path)
                {
                    Ok(_) => {
                        crate::structure_designer::recent_files::add_recent_file(&file_path);
                        APIResult {
                            success: true,
                            error_message: String::new(),
                        }
                    }
                    Err(e) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "No CAD instance".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn save_node_networks() -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                // Call the method in StructureDesigner
                match cad_instance.structure_designer.save_node_networks() {
                    Some(Ok(_)) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Some(Err(e)) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                    None => APIResult {
                        success: false,
                        error_message: "No file path available. Use 'Save As' first.".to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "No CAD instance".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_design_dirty() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.is_dirty(),
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_design_file_path() -> Option<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.get_file_path().cloned(),
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_recent_files() -> Vec<String> {
    crate::structure_designer::recent_files::load_recent_files()
}

#[flutter_rust_bridge::frb(sync)]
pub fn load_node_networks(file_path: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                // Call the method in StructureDesigner
                let result = cad_instance
                    .structure_designer
                    .load_node_networks(&file_path);

                print!("Result: {:?}", result);

                // Apply camera settings returned from load
                if let Ok(ref camera_settings) = result {
                    apply_camera_settings(&mut cad_instance.renderer, camera_settings.as_ref());
                }

                // Refresh the renderer to reflect any loaded structures (even if there was an error)
                refresh_structure_designer_auto(cad_instance);

                match result {
                    Ok(_) => {
                        crate::structure_designer::recent_files::add_recent_file(&file_path);
                        APIResult {
                            success: true,
                            error_message: String::new(),
                        }
                    }
                    Err(e) => APIResult {
                        success: false,
                        error_message: e.to_string(),
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Creates a new empty project, clearing all networks and resetting state.
///
/// This is equivalent to File > New:
/// - Clears all networks
/// - Creates a fresh "Main" network
/// - Clears the file path
/// - Clears the dirty flag
#[flutter_rust_bridge::frb(sync)]
pub fn new_project() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.new_project();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Creates a new project in direct editing mode with a single atom_edit node.
#[flutter_rust_bridge::frb(sync)]
pub fn new_project_direct_editing() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.new_project_direct_editing();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Returns the current direct editing mode state.
#[flutter_rust_bridge::frb(sync)]
pub fn get_direct_editing_mode() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.direct_editing_mode,
            false,
        )
    }
}

/// Sets direct editing mode and marks the design as dirty.
#[flutter_rust_bridge::frb(sync)]
pub fn set_direct_editing_mode(mode: bool) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .set_direct_editing_mode(mode);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Returns whether the current state allows switching to direct editing mode.
#[flutter_rust_bridge::frb(sync)]
pub fn can_switch_to_direct_editing_mode() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .can_switch_to_direct_editing_mode()
            },
            false,
        )
    }
}

/// Imports an XYZ file into the active atom_edit node's diff layer.
/// Atoms and bonds are merged directly as pure additions (incremental import).
/// Returns an empty string on success, or an error message on failure.
#[flutter_rust_bridge::frb(sync)]
pub fn import_xyz_into_atom_edit(file_path: String) -> String {
    use crate::structure_designer::nodes::atom_edit::atom_edit::with_atom_edit_undo;
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let mut error = String::new();
                with_atom_edit_undo(&mut cad_instance.structure_designer, "Import XYZ", |sd| {
                    if let Err(e) = sd.import_xyz_into_atom_edit(&file_path) {
                        error = e;
                    }
                });
                if error.is_empty() {
                    refresh_structure_designer_auto(cad_instance);
                }
                error
            },
            "No CAD instance".to_string(),
        )
    }
}

/// Returns the number of node networks in the current project.
#[flutter_rust_bridge::frb(sync)]
pub fn get_network_count() -> i32 {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .node_type_registry
                    .node_networks
                    .len() as i32
            },
            0,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_node_type_active(node_type: String) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .is_node_type_active(&node_type)
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_api_data_type_display_name(api_data_type: APIDataType) -> String {
    match api_data_type_to_data_type(&api_data_type) {
        Ok(data_type) => data_type.to_string(),
        Err(_) => api_data_type
            .custom_data_type
            .unwrap_or_else(|| "Invalid Type".to_string()),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_structure_designer_preferences() -> Option<StructureDesignerPreferences> {
    unsafe { with_cad_instance(|cad_instance| cad_instance.structure_designer.preferences.clone()) }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_structure_designer_preferences(preferences: StructureDesignerPreferences) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .set_preferences(preferences.clone());
            refresh_structure_designer_auto(cad_instance);
            // Persist preferences to config file (non-blocking, logs errors)
            crate::structure_designer::preferences::save_preferences(&preferences);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn export_visible_atomic_structures(file_path: String) -> APIResult {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                // Call the method in StructureDesigner
                match cad_instance
                    .structure_designer
                    .export_visible_atomic_structures(&file_path)
                {
                    Ok(_) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e,
                    },
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_lattice_vecs_data(scope_path: Vec<u64>, node_id: u64) -> Option<APILatticeVecsData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = match cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)
                {
                    Some(data) => data,
                    None => return None,
                };
                let lattice_vecs_data =
                    match node_data.as_any_ref().downcast_ref::<LatticeVecsData>() {
                        Some(data) => data,
                        None => return None,
                    };
                // Convert to UnitCellStruct and detect crystal system
                let unit_cell_struct = lattice_vecs_data.to_unit_cell_struct();
                let crystal_system = classify_crystal_system(&unit_cell_struct);
                let crystal_system_str = crystal_system_to_string(crystal_system);

                Some(APILatticeVecsData {
                    cell_length_a: lattice_vecs_data.cell_length_a,
                    cell_length_b: lattice_vecs_data.cell_length_b,
                    cell_length_c: lattice_vecs_data.cell_length_c,
                    cell_angle_alpha: lattice_vecs_data.cell_angle_alpha,
                    cell_angle_beta: lattice_vecs_data.cell_angle_beta,
                    cell_angle_gamma: lattice_vecs_data.cell_angle_gamma,
                    crystal_system: crystal_system_str,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_lattice_vecs_data(scope_path: Vec<u64>, node_id: u64, data: APILatticeVecsData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let lattice_vecs_data = Box::new(LatticeVecsData {
                cell_length_a: data.cell_length_a,
                cell_length_b: data.cell_length_b,
                cell_length_c: data.cell_length_c,
                cell_angle_alpha: data.cell_angle_alpha,
                cell_angle_beta: data.cell_angle_beta,
                cell_angle_gamma: data.cell_angle_gamma,
            });
            cad_instance
                .structure_designer
                .set_node_network_data_scoped(&scope_path, node_id, lattice_vecs_data);
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn validate_active_network() {
    unsafe {
        with_mut_cad_instance(|instance| {
            instance.structure_designer.validate_active_network();
            refresh_structure_designer_auto(instance);
        });
    }
}

/// Run atomCAD in headless CLI mode with a single configuration
#[flutter_rust_bridge::frb(sync)]
pub fn run_cli_single(config: super::structure_designer_api_types::CliConfig) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match cli_runner::run_cli_single_mode(
                &mut cad_instance.structure_designer,
                config,
            ) {
                Ok(_) => APIResult {
                    success: true,
                    error_message: String::new(),
                },
                Err(e) => APIResult {
                    success: false,
                    error_message: e,
                },
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Run atomCAD in headless CLI batch mode
#[flutter_rust_bridge::frb(sync)]
pub fn run_cli_batch(config: super::structure_designer_api_types::BatchCliConfig) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match cli_runner::run_cli_batch_mode(
                &mut cad_instance.structure_designer,
                config,
            ) {
                Ok(_) => APIResult {
                    success: true,
                    error_message: String::new(),
                },
                Err(e) => APIResult {
                    success: false,
                    error_message: e,
                },
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            },
        )
    }
}

/// Set the stored body width/height of an HOF (zone-owning) node, identified
/// by the body the resize applies to. `scope_path` is the chain of HOF node
/// IDs leading to the body's owner — empty means the active top-level network,
/// `[hof_id]` means the body owned by `hof_id` at the top level, deeper paths
/// address bodies nested inside bodies. `hof_node_id` is the HOF whose stored
/// body size is changing (located inside the scope's body).
///
/// Minimums clamp at 100×60 logical pixels so the body region is always large
/// enough to render its inner pins. The body's *rendered* size is
/// `max(stored, content_bbox + padding)` (zones UI design doc §"Body sizing"),
/// so the user can shrink stored size down to the clamp regardless of content.
///
/// Phase U3 lands the API; the drag handles that drive it land alongside body
/// content rendering. No undo command yet — that's tracked in U4 alongside
/// node-move begin/end coalescing.
#[flutter_rust_bridge::frb(sync)]
pub fn set_zone_size(scope_path: Vec<u64>, hof_node_id: u64, width: f64, height: f64) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .set_zone_size(&scope_path, hof_node_id, width, height);
        });
    }
}

/// Called when an HOF body resize drag begins. Captures the body's pre-drag
/// dimensions so the matching `end_zone_resize` records a single coalesced
/// `SetZoneSizeCommand` (mirrors `begin_move_nodes`). See
/// `doc/design_zones_ui.md` §"Resize handles".
#[flutter_rust_bridge::frb(sync)]
pub fn begin_zone_resize(scope_path: Vec<u64>, hof_node_id: u64) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .begin_zone_resize(&scope_path, hof_node_id);
        });
    }
}

/// Called when an HOF body resize drag ends. Pushes one undoable
/// `SetZoneSizeCommand` if the body changed size.
#[flutter_rust_bridge::frb(sync)]
pub fn end_zone_resize() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.end_zone_resize();
        });
    }
}

/// Set an HOF node's collapse mode (Auto / Collapsed / Expanded). Thin wrapper;
/// the mutation + undo command live on `StructureDesigner::set_collapse_mode`.
/// `scope_path` identifies the (possibly nested) body the HOF lives in. No-op
/// for non-collapsable nodes. See `doc/design_hof_node_collapse.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn set_collapse_mode(scope_path: Vec<u64>, hof_node_id: u64, mode: APICollapseMode) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.set_collapse_mode(
                &scope_path,
                hof_node_id,
                mode.into(),
            );
        });
    }
}

/// Resize a comment node.
/// This performs a direct mutation without undo — call begin_edit_comment_node/end_edit_comment_node
/// around the resize drag to get a single coalesced undo entry.
#[flutter_rust_bridge::frb(sync)]
pub fn resize_comment_node(scope_path: Vec<u64>, node_id: u64, width: f64, height: f64) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            if let Some(network) = cad_instance
                .structure_designer
                .get_scope_network_mut(&scope_path)
            {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>()
                    {
                        comment_data.width = width.max(100.0);
                        comment_data.height = height.max(60.0);
                    }
                }
            }
        });
    }
}

/// Update a comment node's label and text.
/// This performs a direct mutation without undo — call begin_edit_comment_node/end_edit_comment_node
/// around the editing session to get a single coalesced undo entry.
#[flutter_rust_bridge::frb(sync)]
pub fn update_comment_node(scope_path: Vec<u64>, node_id: u64, label: String, text: String) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            if let Some(network) = cad_instance
                .structure_designer
                .get_scope_network_mut(&scope_path)
            {
                if let Some(node) = network.nodes.get_mut(&node_id) {
                    if let Some(comment_data) = node.data.as_any_mut().downcast_mut::<CommentData>()
                    {
                        comment_data.label = label;
                        comment_data.text = text;
                    }
                }
            }
        });
    }
}

/// Called when a comment node text field gains focus or resize drag begins.
/// Captures a snapshot for undo coalescing.
#[flutter_rust_bridge::frb(sync)]
pub fn begin_edit_comment_node(scope_path: Vec<u64>, node_id: u64) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .begin_comment_edit(scope_path, node_id);
        });
    }
}

/// Called when a comment node text field loses focus or resize drag ends.
/// Pushes a single undo command if the comment data changed.
#[flutter_rust_bridge::frb(sync)]
pub fn end_edit_comment_node() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.end_comment_edit();
        });
    }
}

/// Get comment node data for property panel editing
#[flutter_rust_bridge::frb(sync)]
pub fn get_comment_data(scope_path: Vec<u64>, node_id: u64) -> Option<APICommentData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let node_data = cad_instance
                    .structure_designer
                    .get_node_network_data_scoped(&scope_path, node_id)?;
                let comment_data = node_data.as_any_ref().downcast_ref::<CommentData>()?;

                Some(APICommentData {
                    label: comment_data.label.clone(),
                    text: comment_data.text.clone(),
                    width: comment_data.width,
                    height: comment_data.height,
                })
            },
            None,
        )
    }
}

/// Evaluate a node and return its result string.
///
/// # Arguments
/// * `node_identifier` - Either a numeric node ID or the node's custom name
/// * `verbose` - If true, return detailed output for complex types
///
/// # Returns
/// * `Ok(APINodeEvaluationResult)` - The evaluation result
/// * `Err(String)` - If node not found or evaluation fails
#[flutter_rust_bridge::frb(sync)]
pub fn evaluate_node(
    node_identifier: String,
    verbose: bool,
) -> Result<APINodeEvaluationResult, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let designer = &mut cad_instance.structure_designer;

                // Try parsing as numeric ID first, then fall back to name lookup
                let node_id = node_identifier
                    .parse::<u64>()
                    .ok()
                    .or_else(|| designer.find_node_id_by_name(&node_identifier))
                    .ok_or_else(|| format!("Node not found: {}", node_identifier))?;

                designer.evaluate_node_for_cli(node_id, verbose)
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// Run an explicit Execute pass on a node — the right-click → Execute action
/// in the node-graph UI. Sets `execute = true` for one evaluation pass on the
/// targeted node, which is what gates side-effect nodes (`export_xyz`,
/// `foreach`, future effect nodes) to actually fire. See
/// `doc/design_node_execution.md` (Phase 3 — Triggering execute mode from
/// the UI).
///
/// # Arguments
/// * `network_name` - Network containing the node to execute
/// * `node_id` - Numeric node id of the node to execute
///
/// # Returns
/// * `Ok(APIExecuteResult)` on a successful pass (with `ok` indicating whether
///   the node itself produced an error)
/// * `Err(String)` on structural problems (missing network/node, invalid network)
#[flutter_rust_bridge::frb(sync)]
pub fn execute_node(
    network_name: String,
    scope_path: Vec<u64>,
    node_id: u64,
) -> Result<APIExecuteResult, String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .execute_node(&network_name, &scope_path, node_id)
            },
            Err("CAD instance not available".to_string()),
        )
    }
}

/// Drain and return the accumulated print-log entries.
///
/// The Flutter Console panel calls this at a sensible cadence (after each
/// evaluation triggered through the model layer, plus on Console-panel open).
/// Drain-on-read prevents the buffer from growing indefinitely as long as the
/// panel is occasionally opened. See `doc/design_node_execution.md`
/// (Phase 4 — FFI).
#[flutter_rust_bridge::frb(sync)]
pub fn take_print_log() -> Vec<APIPrintLogEntry> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .take_print_log()
                    .iter()
                    .map(Into::into)
                    .collect()
            },
            Vec::new(),
        )
    }
}

/// Clear all entries in the print-log buffer (Console panel "Clear" button).
#[flutter_rust_bridge::frb(sync)]
pub fn clear_print_log() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.clear_print_log();
        });
    }
}

/// Drain the parameter-id repair messages from the most recent project load
/// (F6 of `doc/design_parameter_wire_stability.md`). Returns an empty list when
/// the loaded project needed no repair. The UI reads this once right after
/// `load_node_networks` to decide whether to show the "auto-repaired" modal.
#[flutter_rust_bridge::frb(sync)]
pub fn take_load_param_id_repairs() -> Vec<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.take_load_param_id_repairs(),
            Vec::new(),
        )
    }
}

/// Apply auto-layout to the active node network.
///
/// This function recomputes positions for all nodes in the active network
/// using the user's preferred layout algorithm from preferences.
///
/// # Behavior
/// - Uses the layout algorithm specified in StructureDesignerPreferences
/// - Reorganizes all nodes in the active network for improved readability
/// - Automatically refreshes the UI after layout
#[flutter_rust_bridge::frb(sync)]
pub fn layout_active_network() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let structure_designer = &mut cad_instance.structure_designer;

            // Get the active network name
            let network_name = match &structure_designer.active_node_network_name {
                Some(name) => name.clone(),
                None => return,
            };

            // Get the layout algorithm from preferences
            let algorithm = structure_designer
                .preferences
                .layout_preferences
                .layout_algorithm
                .into();

            // Get a const pointer to the registry for layout computation
            let registry_ptr = &structure_designer.node_type_registry
                as *const crate::structure_designer::node_type_registry::NodeTypeRegistry;

            // Apply layout to the network
            if let Some(network) = structure_designer
                .node_type_registry
                .node_networks
                .get_mut(&network_name)
            {
                layout::layout_network(network, &*registry_ptr, algorithm);
            }

            // Refresh the UI
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// Get information about whether/how the current selection can be factored into a subnetwork.
///
/// Returns information that can be used to populate the "Factor into Subnetwork" dialog,
/// including suggested names for the subnetwork and its parameters.
#[flutter_rust_bridge::frb(sync)]
pub fn get_factor_selection_info() -> super::structure_designer_api_types::FactorSelectionInfo {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let info = cad_instance.structure_designer.get_factor_selection_info();
                super::structure_designer_api_types::FactorSelectionInfo {
                    can_factor: info.can_factor,
                    invalid_reason: info.invalid_reason,
                    suggested_name: info.suggested_name,
                    suggested_param_names: info.suggested_param_names,
                }
            },
            super::structure_designer_api_types::FactorSelectionInfo {
                can_factor: false,
                invalid_reason: Some("CAD instance not available".to_string()),
                suggested_name: String::new(),
                suggested_param_names: Vec::new(),
            },
        )
    }
}

/// Factor the current selection into a new subnetwork.
///
/// Creates a new custom node type from the selected nodes and replaces
/// the selection with an instance of that node type.
///
/// # Arguments
/// * `request` - The factoring request containing the subnetwork name and parameter names
///
/// # Returns
/// A result indicating success or failure, with the new node ID on success
#[flutter_rust_bridge::frb(sync)]
pub fn factor_selection_into_subnetwork(
    request: super::structure_designer_api_types::FactorSelectionRequest,
) -> super::structure_designer_api_types::FactorSelectionResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                match cad_instance
                    .structure_designer
                    .factor_selection_into_subnetwork(&request.subnetwork_name, request.param_names)
                {
                    Ok(new_node_id) => {
                        // Refresh the UI
                        refresh_structure_designer_auto(cad_instance);
                        super::structure_designer_api_types::FactorSelectionResult {
                            success: true,
                            error: None,
                            new_node_id: Some(new_node_id),
                        }
                    }
                    Err(error) => super::structure_designer_api_types::FactorSelectionResult {
                        success: false,
                        error: Some(error),
                        new_node_id: None,
                    },
                }
            },
            super::structure_designer_api_types::FactorSelectionResult {
                success: false,
                error: Some("CAD instance not available".to_string()),
                new_node_id: None,
            },
        )
    }
}

/// Whether the node at `(scope_path, node_id)` can be inlined — i.e. it is a
/// custom-network instance (built-ins, HOFs, `apply`, `closure` are not custom
/// types and so are rejected). Used to gate the context-menu item.
#[flutter_rust_bridge::frb(sync)]
pub fn can_inline_node(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let sd = &cad_instance.structure_designer;
                match sd
                    .get_scope_network(&scope_path)
                    .and_then(|network| network.nodes.get(&node_id))
                {
                    Some(node) => sd
                        .node_type_registry
                        .is_custom_node_type(&node.node_type_name),
                    None => false,
                }
            },
            false,
        )
    }
}

/// Inline a custom-network instance: replace the node at `(scope_path, node_id)`
/// with a copy of its custom network's contents, spliced into the parent
/// network (or zone body) in place. The named definition is left untouched.
///
/// See `doc/design_inline_custom_node.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn inline_custom_node(
    scope_path: Vec<u64>,
    node_id: u64,
) -> super::structure_designer_api_types::InlineResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match cad_instance
                .structure_designer
                .inline_custom_node(scope_path, node_id)
            {
                Ok(()) => {
                    refresh_structure_designer_auto(cad_instance);
                    super::structure_designer_api_types::InlineResult {
                        success: true,
                        error: None,
                    }
                }
                Err(error) => super::structure_designer_api_types::InlineResult {
                    success: false,
                    error: Some(error),
                },
            },
            super::structure_designer_api_types::InlineResult {
                success: false,
                error: Some("CAD instance not available".to_string()),
            },
        )
    }
}

/// Whether the node at `(scope_path, node_id)` can be converted to a closure —
/// i.e. it is a custom-network instance used as a function (or unconsumed), with
/// a return node. Used to gate the context-menu item.
/// See `doc/design_closure_network_conversion.md` (Direction A).
#[flutter_rust_bridge::frb(sync)]
pub fn can_convert_instance_to_closure(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .can_convert_instance_to_closure(&scope_path, node_id)
            },
            false,
        )
    }
}

/// Convert a custom-network instance node into a `closure` node
/// (*Network → Closure*): replaces the instance `I` at `(scope_path, node_id)`
/// with a `closure` node `C` whose inline body is a copy of `I`'s network. `I`'s
/// wired input pins become captures in the body; its unwired input pins become
/// the closure's parameters. See `doc/design_closure_network_conversion.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn convert_instance_to_closure(
    scope_path: Vec<u64>,
    node_id: u64,
) -> super::structure_designer_api_types::ConversionResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match cad_instance
                .structure_designer
                .convert_instance_to_closure(scope_path, node_id)
            {
                Ok(()) => {
                    refresh_structure_designer_auto(cad_instance);
                    super::structure_designer_api_types::ConversionResult {
                        success: true,
                        error: None,
                    }
                }
                Err(error) => super::structure_designer_api_types::ConversionResult {
                    success: false,
                    error: Some(error),
                },
            },
            super::structure_designer_api_types::ConversionResult {
                success: false,
                error: Some("CAD instance not available".to_string()),
            },
        )
    }
}

/// Whether the node at `(scope_path, node_id)` can be extracted to a network —
/// i.e. it is a `closure` node with a result wire. Used to gate the
/// context-menu item. See `doc/design_closure_network_conversion.md`
/// (Direction B).
#[flutter_rust_bridge::frb(sync)]
pub fn can_extract_closure_to_network(scope_path: Vec<u64>, node_id: u64) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .can_extract_closure_to_network(&scope_path, node_id)
            },
            false,
        )
    }
}

/// Extract a `closure` node into a new named custom network
/// (*Closure → Network*): lifts the closure `C`'s inline body into a fresh
/// standalone network `N` (with parameter nodes for both the closure's
/// parameters and its captures) and replaces `C` with an instance of `N`, wired
/// so its function pin reproduces `C`. See
/// `doc/design_closure_network_conversion.md`.
#[flutter_rust_bridge::frb(sync)]
pub fn extract_closure_to_network(
    scope_path: Vec<u64>,
    node_id: u64,
    name: String,
) -> super::structure_designer_api_types::ConversionResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match cad_instance
                .structure_designer
                .extract_closure_to_network(scope_path, node_id, &name)
            {
                Ok(_instance_id) => {
                    refresh_structure_designer_auto(cad_instance);
                    super::structure_designer_api_types::ConversionResult {
                        success: true,
                        error: None,
                    }
                }
                Err(error) => super::structure_designer_api_types::ConversionResult {
                    success: false,
                    error: Some(error),
                },
            },
            super::structure_designer_api_types::ConversionResult {
                success: false,
                error: Some("CAD instance not available".to_string()),
            },
        )
    }
}

/// Promote a node to a parameter.
///
/// Inserts a `parameter` node typed after the given node's output pin 0,
/// wires that pin into the parameter's default input, and rewires every
/// downstream consumer of the source's pin 0 — including a return-node
/// reference — to read from the parameter instead.
#[flutter_rust_bridge::frb(sync)]
pub fn promote_node_to_parameter(
    node_id: u64,
) -> super::structure_designer_api_types::APIPromoteToParameterResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| match cad_instance
                .structure_designer
                .promote_node_to_parameter(node_id)
            {
                Ok(new_node_id) => {
                    refresh_structure_designer_auto(cad_instance);
                    super::structure_designer_api_types::APIPromoteToParameterResult {
                        success: true,
                        error: None,
                        new_node_id: Some(new_node_id),
                    }
                }
                Err(error) => super::structure_designer_api_types::APIPromoteToParameterResult {
                    success: false,
                    error: Some(error),
                    new_node_id: None,
                },
            },
            super::structure_designer_api_types::APIPromoteToParameterResult {
                success: false,
                error: Some("CAD instance not available".to_string()),
                new_node_id: None,
            },
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn copy_selection(scope_path: Vec<u64>) -> bool {
    // `scope_path` is accepted for API symmetry but unused: copy locates the
    // selection's scope itself (the single-scope selection invariant means it
    // is unambiguous), so it works on a zone-body selection regardless of the
    // caller's active scope.
    let _ = &scope_path;
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.copy_selection(),
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn paste_at_position(scope_path: Vec<u64>, x: f64, y: f64) -> Vec<u64> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let position = glam::f64::DVec2::new(x, y);
                let new_ids = cad_instance
                    .structure_designer
                    .paste_at_position_scoped(&scope_path, position);
                refresh_structure_designer_auto(cad_instance);
                new_ids
            },
            vec![],
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn cut_selection(scope_path: Vec<u64>) -> bool {
    // Like `copy_selection`, the scope is located internally; `scope_path` is
    // accepted only for API symmetry.
    let _ = &scope_path;
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance.structure_designer.cut_selection();
                if result {
                    refresh_structure_designer_auto(cad_instance);
                }
                result
            },
            false,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn has_clipboard_content() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| cad_instance.structure_designer.has_clipboard_content(),
            false,
        )
    }
}

/// Serialize the active node network to text format for the text editor tab.
#[flutter_rust_bridge::frb(sync)]
pub fn serialize_active_network_to_text() -> String {
    use crate::structure_designer::text_format::serialize_network;
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let structure_designer = &cad_instance.structure_designer;
                let network_name = match &structure_designer.active_node_network_name {
                    Some(name) => name,
                    None => return String::new(),
                };
                let network = match structure_designer
                    .node_type_registry
                    .node_networks
                    .get(network_name)
                {
                    Some(network) => network,
                    None => return String::new(),
                };
                serialize_network(
                    network,
                    &structure_designer.node_type_registry,
                    Some(network_name),
                )
            },
            String::new(),
        )
    }
}

/// Apply text format edits to the active node network (replace mode with position preservation).
#[flutter_rust_bridge::frb(sync)]
pub fn apply_text_to_active_network(code: String) -> APITextEditResult {
    use crate::structure_designer::network_validator::validate_network;
    use crate::structure_designer::text_format::{Parser, edit_network as text_edit_network};
    use glam::DVec2;
    use std::collections::HashMap;

    let error_result = |msg: String| -> APITextEditResult {
        APITextEditResult {
            success: false,
            nodes_created: vec![],
            nodes_updated: vec![],
            nodes_deleted: vec![],
            connections_made: vec![],
            errors: vec![APITextError {
                message: msg,
                line: 0,
                column: 0,
            }],
            warnings: vec![],
        }
    };

    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let structure_designer = &mut cad_instance.structure_designer;

                let network_name = match &structure_designer.active_node_network_name {
                    Some(name) => name.clone(),
                    None => return error_result("No active node network".to_string()),
                };

                // Temporarily remove network from registry (same borrow pattern as ai_edit_network)
                let mut network = match structure_designer
                    .node_type_registry
                    .node_networks
                    .remove(&network_name)
                {
                    Some(network) => network,
                    None => return error_result(format!("Network '{}' not found", network_name)),
                };

                // Save node positions by custom_name before replacement
                let saved_positions: HashMap<String, DVec2> = network
                    .nodes
                    .values()
                    .filter_map(|node| {
                        node.custom_name
                            .as_ref()
                            .map(|name| (name.clone(), node.position))
                    })
                    .collect();

                // Save active node's custom_name so we can restore selection after replace
                let active_custom_name: Option<String> = network
                    .active_node_id
                    .and_then(|id| network.nodes.get(&id))
                    .and_then(|node| node.custom_name.clone());

                // Validate parse first to extract line/column errors
                let parse_errors: Vec<APITextError> = match Parser::parse(&code) {
                    Ok(_) => vec![],
                    Err(e) => vec![APITextError {
                        message: e.message.clone(),
                        line: e.line as i32,
                        column: e.column as i32,
                    }],
                };

                if !parse_errors.is_empty() {
                    // Put network back and return parse errors
                    structure_designer
                        .node_type_registry
                        .node_networks
                        .insert(network_name, network);
                    return APITextEditResult {
                        success: false,
                        nodes_created: vec![],
                        nodes_updated: vec![],
                        nodes_deleted: vec![],
                        connections_made: vec![],
                        errors: parse_errors,
                        warnings: vec![],
                    };
                }

                // Snapshot network BEFORE text edit (for undo)
                use crate::structure_designer::serialization::node_networks_serialization::node_network_to_serializable;
                let before_snapshot = node_network_to_serializable(
                    &mut network,
                    &structure_designer.node_type_registry.built_in_node_types,
                    None,
                )
                .ok();

                // Apply edits in replace mode
                let result = text_edit_network(
                    &mut network,
                    &structure_designer.node_type_registry,
                    &code,
                    true,
                );

                // Restore saved positions for nodes that were re-created
                for node in network.nodes.values_mut() {
                    if let Some(ref name) = node.custom_name {
                        if let Some(&saved_pos) = saved_positions.get(name) {
                            node.position = saved_pos;
                        }
                    }
                }

                // Restore active/selected node by custom_name
                if let Some(ref active_name) = active_custom_name {
                    let node_id = network
                        .nodes
                        .values()
                        .find(|n| n.custom_name.as_deref() == Some(active_name.as_str()))
                        .map(|n| n.id);
                    if let Some(id) = node_id {
                        network.select_node(id);
                    }
                }

                // Snapshot network AFTER text edit (for undo), before putting it back
                let after_snapshot = node_network_to_serializable(
                    &mut network,
                    &structure_designer.node_type_registry.built_in_node_types,
                    None,
                )
                .ok();

                // Put network back
                structure_designer
                    .node_type_registry
                    .node_networks
                    .insert(network_name.clone(), network);

                // Validate network
                {
                    let registry_ptr = &mut structure_designer.node_type_registry
                        as *mut crate::structure_designer::node_type_registry::NodeTypeRegistry;
                    if let Some(network) = (*registry_ptr).node_networks.get_mut(&network_name) {
                        validate_network(network, &mut *registry_ptr, None);
                    }
                }

                // Push undo command if the text edit made changes
                let made_changes = result.success
                    && (!result.nodes_created.is_empty()
                        || !result.nodes_updated.is_empty()
                        || !result.nodes_deleted.is_empty()
                        || !result.connections_made.is_empty()
                        || result.description_set.is_some()
                        || result.summary_set.is_some()
                        || result.output_set.is_some());
                if made_changes {
                    if let (Some(before), Some(after)) = (before_snapshot, after_snapshot) {
                        use crate::structure_designer::undo::commands::text_edit_network::TextEditNetworkCommand;
                        cad_instance
                            .structure_designer
                            .push_command(TextEditNetworkCommand {
                                network_name: network_name.clone(),
                                before_snapshot: before,
                                after_snapshot: after,
                            });
                    }
                }

                // Mark full refresh and dirty
                cad_instance.structure_designer.mark_full_refresh();
                if made_changes {
                    cad_instance.structure_designer.set_dirty(true);
                }

                refresh_structure_designer_auto(cad_instance);

                // Convert EditResult errors (strings) to APITextError (without line info)
                let errors: Vec<APITextError> = result
                    .errors
                    .iter()
                    .map(|e| APITextError {
                        message: e.clone(),
                        line: 0,
                        column: 0,
                    })
                    .collect();

                APITextEditResult {
                    success: result.success,
                    nodes_created: result.nodes_created,
                    nodes_updated: result.nodes_updated,
                    nodes_deleted: result.nodes_deleted,
                    connections_made: result.connections_made,
                    errors,
                    warnings: result.warnings,
                }
            },
            error_result("Could not access structure designer".to_string()),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn query_hovered_atom_info(
    ray_origin: APIVec3,
    ray_direction: APIVec3,
) -> Option<APIHoveredAtomInfo> {
    let ray_origin = from_api_vec3(&ray_origin);
    let ray_direction = from_api_vec3(&ray_direction);

    /// Overlap threshold in Angstroms — atoms from different nodes within
    /// this distance along the ray are considered overlapping.
    const OVERLAP_EPSILON: f64 = 0.1;

    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let (atom_id, structure, closest_node_id, closest_distance) = cad_instance
                    .structure_designer
                    .hit_test_all_atomic_structures_with_node_id(&ray_origin, &ray_direction)?;

                let atom = structure.get_atom(atom_id)?;

                // Resolve element identity. Check decorator's name overrides first
                // (set by motif_edit for parameter elements), then fall back to
                // the standard element database.
                let (symbol, element_name, display_atomic_number, effective_element) = if let Some(
                    name,
                ) =
                    structure
                        .decorator()
                        .element_name_overrides
                        .get(&atom.atomic_number)
                {
                    use crate::structure_designer::nodes::atom_edit::atom_edit::param_atomic_number_to_index;
                    let sym = param_atomic_number_to_index(atom.atomic_number)
                        .map(|idx| format!("P{}", idx + 1))
                        .unwrap_or_else(|| "?".to_string());

                    // Resolve effective element for display
                    let eff_z = structure.effective_atomic_number(atom);
                    let eff_str = if eff_z != atom.atomic_number {
                        let eff_info = crate::crystolecule::atomic_constants::ATOM_INFO
                            .get(&(eff_z as i32))
                            .unwrap_or(&crate::crystolecule::atomic_constants::DEFAULT_ATOM_INFO);
                        format!("{} ({})", eff_info.symbol, eff_info.element_name)
                    } else {
                        String::new()
                    };

                    (sym, name.clone(), atom.atomic_number as i32, eff_str)
                } else {
                    let atom_info = crate::crystolecule::atomic_constants::ATOM_INFO
                        .get(&(atom.atomic_number as i32))
                        .unwrap_or(&crate::crystolecule::atomic_constants::DEFAULT_ATOM_INFO);
                    (
                        atom_info.symbol.clone(),
                        atom_info.element_name.clone(),
                        atom_info.atomic_number,
                        String::new(),
                    )
                };

                let bond_count = atom.bonds.len() as u32;

                let node_name = cad_instance
                    .structure_designer
                    .get_node_display_name(closest_node_id);

                // Detect overlapping nodes within OVERLAP_EPSILON of the closest hit.
                let visualization = &cad_instance
                    .structure_designer
                    .preferences
                    .atomic_structure_visualization_preferences
                    .visualization;
                let per_node_hits = cad_instance.structure_designer.raytrace_per_node(
                    &ray_origin,
                    &ray_direction,
                    visualization,
                );
                let overlapping_node_names: Vec<String> = per_node_hits
                    .iter()
                    .filter(|hit| {
                        hit.node_id != closest_node_id
                            && (hit.distance - closest_distance).abs() < OVERLAP_EPSILON
                    })
                    .map(|hit| {
                        cad_instance
                            .structure_designer
                            .get_node_display_name(hit.node_id)
                    })
                    .collect();

                let inferred_hybridization = {
                    use crate::crystolecule::guided_placement::{
                        Hybridization, detect_hybridization,
                    };
                    match detect_hybridization(structure, atom_id, None) {
                        Hybridization::Sp3 => 1,
                        Hybridization::Sp2 => 2,
                        Hybridization::Sp1 => 3,
                    }
                };

                Some(APIHoveredAtomInfo {
                    symbol,
                    element_name,
                    atomic_number: display_atomic_number,
                    effective_element,
                    x: atom.position.x,
                    y: atom.position.y,
                    z: atom.position.z,
                    bond_count,
                    is_frozen: atom.is_frozen(),
                    hybridization_override: atom.hybridization_override(),
                    inferred_hybridization,
                    node_name,
                    overlapping_node_names,
                })
            },
            None,
        )
    }
}

/// Performs a viewport pick for click-to-activate.
///
/// Casts a ray through the scene and determines whether the click should:
/// - Pass through to normal handling (active node hit or no hit)
/// - Activate a different node (unambiguous non-active node hit)
/// - Show a disambiguation popup (overlapping non-active node hits)
#[flutter_rust_bridge::frb(sync)]
pub fn viewport_pick(ray_origin: APIVec3, ray_direction: APIVec3) -> APIViewportPickResult {
    let ray_origin = from_api_vec3(&ray_origin);
    let ray_direction = from_api_vec3(&ray_direction);

    /// Overlap threshold in Angstroms — hits from different nodes within
    /// this distance along the ray are considered overlapping.
    const OVERLAP_EPSILON: f64 = 0.1;

    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let visualization = &cad_instance
                    .structure_designer
                    .preferences
                    .atomic_structure_visualization_preferences
                    .visualization;

                let hits = cad_instance.structure_designer.raytrace_per_node(
                    &ray_origin,
                    &ray_direction,
                    visualization,
                );

                if hits.is_empty() {
                    return APIViewportPickResult::NoHit;
                }

                // Get the active node ID from the current network.
                let active_node_id = cad_instance
                    .structure_designer
                    .get_active_node_network()
                    .and_then(|network| network.active_node_id);

                let closest = &hits[0];

                // If the closest hit belongs to the active node, pass through.
                if active_node_id == Some(closest.node_id) {
                    return APIViewportPickResult::ActiveNodeHit;
                }

                // Collect all hits within OVERLAP_EPSILON of the closest hit.
                let overlapping: Vec<
                    &crate::structure_designer::structure_designer::PerNodeRayHit,
                > = hits
                    .iter()
                    .filter(|hit| (hit.distance - closest.distance).abs() < OVERLAP_EPSILON)
                    .collect();

                // If the active node is among the overlapping hits, treat as active node hit.
                if overlapping
                    .iter()
                    .any(|hit| active_node_id == Some(hit.node_id))
                {
                    return APIViewportPickResult::ActiveNodeHit;
                }

                // If only one node in the overlap set, unambiguous activation.
                if overlapping.len() == 1 {
                    let node_name = cad_instance
                        .structure_designer
                        .get_node_display_name(closest.node_id);
                    return APIViewportPickResult::ActivateNode {
                        node_id: closest.node_id,
                        node_name,
                    };
                }

                // Multiple overlapping non-active nodes — disambiguation needed.
                let candidates = overlapping
                    .iter()
                    .map(|hit| APICandidateNode {
                        node_id: hit.node_id,
                        node_name: cad_instance
                            .structure_designer
                            .get_node_display_name(hit.node_id),
                    })
                    .collect();

                APIViewportPickResult::Disambiguation { candidates }
            },
            APIViewportPickResult::NoHit,
        )
    }
}

// =============================================================================
// CLI Access Rules
// =============================================================================

/// Check whether CLI write access is locked for a given network name.
/// Returns true if the network is locked from CLI write access.
#[flutter_rust_bridge::frb(sync)]
pub fn is_cli_write_locked(network_name: String) -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .is_cli_write_locked(&network_name)
            },
            false,
        )
    }
}

/// Set CLI access for a namespace or network name.
/// `allowed = true` means CLI can write, `allowed = false` means CLI is locked out.
/// Setting a rule prunes all descendant rules to keep the map minimal.
#[flutter_rust_bridge::frb(sync)]
pub fn set_cli_access(name: String, allowed: bool) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance
                .structure_designer
                .set_cli_access(&name, allowed);
        });
    }
}

/// Get all CLI access rules as a list of (prefix, allowed) pairs.
/// This is used by the Flutter UI to display lock state in the tree view.
#[flutter_rust_bridge::frb(sync)]
pub fn get_cli_access_rules() -> Vec<(String, bool)> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance
                    .structure_designer
                    .get_cli_access_rules()
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect()
            },
            vec![],
        )
    }
}
