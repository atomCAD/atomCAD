//! Tests for the drag-aware add-node popup (`doc/design_drag_aware_add_node.md`).
//!
//! Phase 1: `DataType::drag_element_type_*` helpers, iterator-node adapters
//! (`map`, `filter`, `fold`, `collect`), `range`'s no-op default,
//! `NodeTypeRegistry::get_compatible_node_types` slow-path, and
//! `StructureDesigner::add_node_with_drag_source` create-time plumbing.
//!
//! Phase 2: array-node adapters (`array_at`, `array_len`, `array_concat`,
//! `array_append`, `sequence`) plus a filter integration test that an
//! `Array[Foo]` drag from output surfaces all of them.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::RecordType;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
use rust_lib_flutter_cad::structure_designer::node_data::{DragDirection, NodeData};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::array_append::ArrayAppendData;
use rust_lib_flutter_cad::structure_designer::nodes::array_at::ArrayAtData;
use rust_lib_flutter_cad::structure_designer::nodes::array_concat::ArrayConcatData;
use rust_lib_flutter_cad::structure_designer::nodes::array_len::ArrayLenData;
use rust_lib_flutter_cad::structure_designer::nodes::closure::{ClosureData, ClosureKind};
use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;
use rust_lib_flutter_cad::structure_designer::nodes::expr::ExprData;
use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::nodes::record_destructure::RecordDestructureData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::structure_designer::{DragSource, StructureDesigner};

// ============================================================================
// drag_element_type_* helpers
// ============================================================================

#[test]
fn drag_element_type_from_output_peels_iter_and_array() {
    assert_eq!(
        DataType::Iterator(Box::new(DataType::Int)).drag_element_type_from_output(),
        Some(DataType::Int)
    );
    assert_eq!(
        DataType::Array(Box::new(DataType::Float)).drag_element_type_from_output(),
        Some(DataType::Float)
    );
}

#[test]
fn drag_element_type_from_output_broadcasts_scalar() {
    assert_eq!(
        DataType::Vec3.drag_element_type_from_output(),
        Some(DataType::Vec3)
    );
    assert_eq!(
        DataType::Int.drag_element_type_from_output(),
        Some(DataType::Int)
    );
}

#[test]
fn drag_element_type_from_output_rejects_abstract_and_function() {
    assert_eq!(DataType::HasAtoms.drag_element_type_from_output(), None);
    assert_eq!(DataType::HasStructure.drag_element_type_from_output(), None);
    assert_eq!(
        DataType::Function(FunctionType {
            parameter_types: vec![DataType::Int],
            output_type: Box::new(DataType::Int),
        })
        .drag_element_type_from_output(),
        None
    );
}

#[test]
fn drag_element_type_from_input_strict_rejects_scalar() {
    assert_eq!(
        DataType::Iterator(Box::new(DataType::Int)).drag_element_type_from_input_strict(),
        Some(DataType::Int)
    );
    assert_eq!(
        DataType::Array(Box::new(DataType::Float)).drag_element_type_from_input_strict(),
        Some(DataType::Float)
    );
    assert_eq!(DataType::Int.drag_element_type_from_input_strict(), None);
    assert_eq!(DataType::Vec3.drag_element_type_from_input_strict(), None);
}

// ============================================================================
// Per-adapter unit tests
// ============================================================================

fn default_data<T: NodeData + Default + 'static>() -> Box<dyn NodeData> {
    Box::new(T::default())
}

fn adapt_map(source: DataType, dir: DragDirection) -> Option<MapData> {
    let registry = NodeTypeRegistry::new();
    let data: Box<dyn NodeData> = Box::new(MapData {
        input_type: DataType::Float,
        output_type: DataType::Float,
    });
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<MapData>().cloned()
}

fn adapt_filter(source: DataType, dir: DragDirection) -> Option<FilterData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<FilterData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<FilterData>().cloned()
}

fn adapt_fold(source: DataType, dir: DragDirection) -> Option<FoldData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<FoldData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<FoldData>().cloned()
}

fn adapt_collect(source: DataType, dir: DragDirection) -> Option<CollectData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<CollectData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<CollectData>().cloned()
}

#[test]
fn map_adapter_from_output_iter_int() {
    let adapted = adapt_map(
        DataType::Iterator(Box::new(DataType::Int)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.input_type, DataType::Int);
    assert_eq!(adapted.output_type, DataType::Int);
}

#[test]
fn map_adapter_from_output_array_float() {
    let adapted = adapt_map(
        DataType::Array(Box::new(DataType::Float)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.input_type, DataType::Float);
    assert_eq!(adapted.output_type, DataType::Float);
}

#[test]
fn map_adapter_from_output_broadcast_vec3() {
    let adapted = adapt_map(DataType::Vec3, DragDirection::FromOutput).expect("scalar broadcast");
    assert_eq!(adapted.input_type, DataType::Vec3);
    assert_eq!(adapted.output_type, DataType::Vec3);
}

#[test]
fn map_adapter_rejects_abstract() {
    assert!(adapt_map(DataType::HasAtoms, DragDirection::FromOutput).is_none());
}

#[test]
fn map_adapter_from_input_iter_int() {
    let adapted = adapt_map(
        DataType::Iterator(Box::new(DataType::Int)),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.input_type, DataType::Int);
    assert_eq!(adapted.output_type, DataType::Int);
}

#[test]
fn filter_adapter_from_output_iter() {
    let adapted = adapt_filter(
        DataType::Iterator(Box::new(DataType::IVec3)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::IVec3);
}

#[test]
fn filter_adapter_from_input_array() {
    let adapted = adapt_filter(
        DataType::Array(Box::new(DataType::Bool)),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Bool);
}

#[test]
fn fold_adapter_from_output_iter_float() {
    let adapted = adapt_fold(
        DataType::Iterator(Box::new(DataType::Float)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Float);
    assert_eq!(adapted.accumulator_type, DataType::Float);
}

#[test]
fn fold_adapter_from_input_scalar() {
    let adapted = adapt_fold(DataType::Int, DragDirection::FromInput).expect("scalar accumulator");
    assert_eq!(adapted.element_type, DataType::Int);
    assert_eq!(adapted.accumulator_type, DataType::Int);
}

#[test]
fn collect_adapter_from_output_iter() {
    let adapted = adapt_collect(
        DataType::Iterator(Box::new(DataType::Int)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Int);
}

#[test]
fn collect_adapter_rejects_scalar_from_output() {
    // collect deliberately rejects scalar broadcast on the input side.
    assert!(adapt_collect(DataType::Int, DragDirection::FromOutput).is_none());
}

#[test]
fn collect_adapter_from_input_array() {
    let adapted = adapt_collect(
        DataType::Array(Box::new(DataType::Float)),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Float);
}

#[test]
fn collect_adapter_from_input_iter_rejected() {
    // Output of collect is `Array[T]`, not `Iter[T]` — pulling from an
    // `Iter[T]` consumer pin doesn't make sense for collect.
    assert!(
        adapt_collect(
            DataType::Iterator(Box::new(DataType::Float)),
            DragDirection::FromInput
        )
        .is_none()
    );
}

#[test]
fn range_adapter_returns_none() {
    let registry = NodeTypeRegistry::new();
    let data: Box<dyn NodeData> = Box::new(RangeData {
        start: 0,
        step: 1,
        count: 1,
    });
    assert!(
        data.adapt_for_drag_source(
            &DataType::Iterator(Box::new(DataType::Int)),
            DragDirection::FromInput,
            &registry,
        )
        .is_none(),
        "range has no type properties; default None must be preserved"
    );
}

// ============================================================================
// Filter (popup) integration
// ============================================================================

#[test]
fn iter_int_from_output_surfaces_map_via_adapter() {
    // map's static default is `Iter[Float]`, so before drag-aware adapters
    // a drag of `Iter[Int]` from an output would have hidden it. With the
    // adapter, map should surface — proof that the slow path runs and that
    // the verification step accepts the adapted node type.
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(
        &DataType::Iterator(Box::new(DataType::Int)),
        true, // dragging_from_output
    );
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"map"),
        "map should surface for Iter[Int] from output (slow-path adapter); got {:?}",
        names
    );
    assert!(names.contains(&"filter"), "filter should surface");
    assert!(names.contains(&"fold"), "fold should surface");
    assert!(names.contains(&"collect"), "collect should surface");
}

// ============================================================================
// Create-time tests
// ============================================================================

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn get_map_data(designer: &StructureDesigner, node_id: u64) -> MapData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<MapData>()
        .cloned()
        .expect("MapData")
}

fn get_collect_data(designer: &StructureDesigner, node_id: u64) -> CollectData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<CollectData>()
        .cloned()
        .expect("CollectData")
}

#[test]
fn add_node_with_drag_source_configures_map() {
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "map",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Iterator(Box::new(DataType::Int)),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_map_data(&designer, node_id);
    assert_eq!(data.input_type, DataType::Int);
    assert_eq!(data.output_type, DataType::Int);
}

#[test]
fn add_node_with_drag_source_configures_collect() {
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "collect",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Iterator(Box::new(DataType::Float)),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_collect_data(&designer, node_id);
    assert_eq!(data.element_type, DataType::Float);
}

#[test]
fn add_node_with_drag_source_falls_back_on_overpromise() {
    // collect's adapter rejects scalar `Int` on `FromOutput`, so the create
    // path falls back to default data (CollectData::default → Int element).
    // We pick a different scalar (`Float`) to ensure the fallback to
    // default — not the adapter's output — is what we observe.
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "collect",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Float,
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_collect_data(&designer, node_id);
    assert_eq!(
        data.element_type,
        DataType::Int,
        "collect adapter rejects scalar Float; default Int element_type must remain"
    );
}

#[test]
fn add_node_without_drag_source_uses_defaults() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("map", DVec2::ZERO);
    assert_ne!(node_id, 0);
    let data = get_map_data(&designer, node_id);
    assert_eq!(data.input_type, DataType::Float);
    assert_eq!(data.output_type, DataType::Float);
}

// ============================================================================
// Phase 2: array-node adapters
// ============================================================================

fn adapt_array_at(source: DataType, dir: DragDirection) -> Option<ArrayAtData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<ArrayAtData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<ArrayAtData>().cloned()
}

fn adapt_array_len(source: DataType, dir: DragDirection) -> Option<ArrayLenData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<ArrayLenData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<ArrayLenData>().cloned()
}

fn adapt_array_concat(source: DataType, dir: DragDirection) -> Option<ArrayConcatData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<ArrayConcatData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted
        .as_any_ref()
        .downcast_ref::<ArrayConcatData>()
        .cloned()
}

fn adapt_array_append(source: DataType, dir: DragDirection) -> Option<ArrayAppendData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<ArrayAppendData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted
        .as_any_ref()
        .downcast_ref::<ArrayAppendData>()
        .cloned()
}

fn adapt_sequence(source: DataType, dir: DragDirection) -> Option<SequenceData> {
    let registry = NodeTypeRegistry::new();
    let data = default_data::<SequenceData>();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<SequenceData>().cloned()
}

// --- array_at -------------------------------------------------------------

#[test]
fn array_at_adapter_from_output_array() {
    let adapted = adapt_array_at(
        DataType::Array(Box::new(DataType::IVec3)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::IVec3);
}

#[test]
fn array_at_adapter_from_output_scalar_broadcast() {
    // The implicit `T → [T]` rule means a scalar T can broadcast onto an
    // `Array[T]` pin; the adapter sets element_type=T accordingly.
    let adapted = adapt_array_at(DataType::Float, DragDirection::FromOutput)
        .expect("scalar broadcast onto array pin");
    assert_eq!(adapted.element_type, DataType::Float);
}

#[test]
fn array_at_adapter_from_input_scalar() {
    // Output of array_at is the element type — drag from a scalar consumer
    // pin and the element type matches it directly (no peeling).
    let adapted = adapt_array_at(DataType::Float, DragDirection::FromInput)
        .expect("element type from consumer");
    assert_eq!(adapted.element_type, DataType::Float);
}

#[test]
fn array_at_adapter_from_input_array_preserved() {
    // If the consumer pin's declared type is itself `Array[U]`, the
    // element type should be `Array[U]` — peeling would mis-type the
    // output.
    let adapted = adapt_array_at(
        DataType::Array(Box::new(DataType::Int)),
        DragDirection::FromInput,
    )
    .expect("array as element type");
    assert_eq!(
        adapted.element_type,
        DataType::Array(Box::new(DataType::Int))
    );
}

#[test]
fn array_at_adapter_rejects_abstract_from_input() {
    assert!(adapt_array_at(DataType::HasAtoms, DragDirection::FromInput).is_none());
}

#[test]
fn array_at_adapter_rejects_iter_element_type() {
    // Drag from an `Iter[T]`-typed input pin would otherwise set
    // `element_type = Iter[T]` and render the output pin as `Iter[T]`,
    // misleading users into thinking array_at produces an iterator.
    assert!(
        adapt_array_at(
            DataType::Iterator(Box::new(DataType::Int)),
            DragDirection::FromInput
        )
        .is_none()
    );
    // Same rejection on the FromOutput side when the source is
    // `Array[Iter[T]]` (peel yields `Iter[T]` as element type).
    assert!(
        adapt_array_at(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromOutput
        )
        .is_none()
    );
}

// --- array_len ------------------------------------------------------------

#[test]
fn array_len_adapter_from_output_array() {
    let adapted = adapt_array_len(
        DataType::Array(Box::new(DataType::Bool)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Bool);
}

#[test]
fn array_len_adapter_from_input_returns_none() {
    // Output is always Int — static-match handles drag from an Int
    // consumer pin; the adapter must not return Some.
    assert!(adapt_array_len(DataType::Int, DragDirection::FromInput).is_none());
}

#[test]
fn array_len_adapter_rejects_iter_element_type() {
    // `Array[Iter[T]]` source would otherwise set element_type = Iter[T].
    assert!(
        adapt_array_len(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromOutput
        )
        .is_none()
    );
}

// --- array_concat ---------------------------------------------------------

#[test]
fn array_concat_adapter_from_output_array() {
    let adapted = adapt_array_concat(
        DataType::Array(Box::new(DataType::Bool)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Bool);
}

#[test]
fn array_concat_adapter_from_input_array() {
    let adapted = adapt_array_concat(
        DataType::Array(Box::new(DataType::Float)),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Float);
}

#[test]
fn array_concat_adapter_rejects_scalar() {
    // Strict on both sides — array_concat is for arrays.
    assert!(adapt_array_concat(DataType::Int, DragDirection::FromOutput).is_none());
    assert!(adapt_array_concat(DataType::Int, DragDirection::FromInput).is_none());
}

#[test]
fn array_concat_adapter_rejects_iter_element_type() {
    // `Array[Iter[T]]` source on either side would otherwise set
    // element_type = Iter[T].
    assert!(
        adapt_array_concat(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromOutput
        )
        .is_none()
    );
    assert!(
        adapt_array_concat(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromInput
        )
        .is_none()
    );
}

// --- array_append ---------------------------------------------------------

#[test]
fn array_append_adapter_from_output_array() {
    // FromOutput onto the `array: Array[T]` pin.
    let adapted = adapt_array_append(
        DataType::Array(Box::new(DataType::Int)),
        DragDirection::FromOutput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Int);
}

#[test]
fn array_append_adapter_from_output_element() {
    // FromOutput onto the `element: T` pin (scalar broadcast).
    let adapted = adapt_array_append(DataType::Int, DragDirection::FromOutput)
        .expect("scalar onto element pin");
    assert_eq!(adapted.element_type, DataType::Int);
}

#[test]
fn array_append_adapter_from_input_array() {
    // FromInput consumer expects `Array[T]` (the output).
    let adapted = adapt_array_append(
        DataType::Array(Box::new(DataType::Float)),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Float);
}

#[test]
fn array_append_adapter_rejects_scalar_from_input() {
    // Output is `Array[T]` — a scalar consumer doesn't take an array.
    assert!(adapt_array_append(DataType::Float, DragDirection::FromInput).is_none());
}

#[test]
fn array_append_adapter_rejects_iter_element_type() {
    // FromOutput: `Iter[T]` source peels to `T` — fine — but
    // `Array[Iter[T]]` source peels to `Iter[T]`, which we reject.
    assert!(
        adapt_array_append(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromOutput
        )
        .is_none()
    );
    assert!(
        adapt_array_append(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromInput
        )
        .is_none()
    );
}

// --- sequence -------------------------------------------------------------

#[test]
fn sequence_adapter_from_output_scalar() {
    // sequence's input pins are typed `T` directly; setting element_type
    // to the source as-is is the right move.
    let adapted =
        adapt_sequence(DataType::Float, DragDirection::FromOutput).expect("scalar source");
    assert_eq!(adapted.element_type, DataType::Float);
    assert_eq!(adapted.input_count, 2, "input_count default preserved");
}

#[test]
fn sequence_adapter_from_output_array_preserved_as_element() {
    // sequence accepts Array[T] directly as its element type — its pins
    // become Array[T]-typed and the output becomes Array[Array[T]].
    let adapted = adapt_sequence(
        DataType::Array(Box::new(DataType::Int)),
        DragDirection::FromOutput,
    )
    .expect("array as element");
    assert_eq!(
        adapted.element_type,
        DataType::Array(Box::new(DataType::Int))
    );
}

#[test]
fn sequence_adapter_from_output_rejects_abstract() {
    assert!(adapt_sequence(DataType::HasAtoms, DragDirection::FromOutput).is_none());
}

#[test]
fn sequence_adapter_from_input_array() {
    // FromInput consumer expects `Array[T]` (the output).
    let adapted = adapt_sequence(
        DataType::Array(Box::new(DataType::Float)),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.element_type, DataType::Float);
}

#[test]
fn sequence_adapter_from_input_rejects_scalar() {
    assert!(adapt_sequence(DataType::Float, DragDirection::FromInput).is_none());
}

#[test]
fn sequence_adapter_rejects_iter_element_type() {
    // FromOutput: sequence's input pins are typed `T` directly (not
    // peeled), so an `Iter[T]` source would otherwise set
    // element_type = Iter[T].
    assert!(
        adapt_sequence(
            DataType::Iterator(Box::new(DataType::Int)),
            DragDirection::FromOutput
        )
        .is_none()
    );
    // FromInput: `Array[Iter[T]]` consumer peels to `Iter[T]`.
    assert!(
        adapt_sequence(
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
            DragDirection::FromInput
        )
        .is_none()
    );
}

// --- Filter (popup) integration -------------------------------------------

#[test]
fn array_int_from_output_surfaces_all_array_nodes_via_adapter() {
    // None of these nodes' static defaults match `Array[Int]` directly
    // (they default to either `Array[Int]` or some other element). What we
    // care about is that after Phase 2 adapters, all of them surface in
    // the popup for an `Array[Int]` drag from an output pin.
    let registry = NodeTypeRegistry::new();
    let categories =
        registry.get_compatible_node_types(&DataType::Array(Box::new(DataType::Int)), true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();

    for expected in &[
        "array_at",
        "array_len",
        "array_concat",
        "array_append",
        "sequence",
        "collect",
    ] {
        assert!(
            names.contains(expected),
            "{} should surface for Array[Int] from output; got {:?}",
            expected,
            names
        );
    }
}

// --- Create-time tests ----------------------------------------------------

fn get_array_at_data(designer: &StructureDesigner, node_id: u64) -> ArrayAtData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<ArrayAtData>()
        .cloned()
        .expect("ArrayAtData")
}

fn get_sequence_data(designer: &StructureDesigner, node_id: u64) -> SequenceData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<SequenceData>()
        .cloned()
        .expect("SequenceData")
}

#[test]
fn add_node_with_drag_source_configures_array_at() {
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "array_at",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Array(Box::new(DataType::Vec3)),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_array_at_data(&designer, node_id);
    assert_eq!(data.element_type, DataType::Vec3);
}

#[test]
fn add_node_with_drag_source_configures_sequence() {
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "sequence",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Float,
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_sequence_data(&designer, node_id);
    assert_eq!(data.element_type, DataType::Float);
    assert_eq!(data.input_count, 2);
}

// ============================================================================
// Phase 3: parameter-node adapter
// ============================================================================

fn adapt_parameter(source: DataType, dir: DragDirection) -> Option<ParameterData> {
    let registry = NodeTypeRegistry::new();
    let data: Box<dyn NodeData> = Box::new(ParameterData {
        param_id: None,
        param_index: 0,
        param_name: "param".to_string(),
        data_type: DataType::Int,
        sort_order: 0,
        data_type_str: None,
        error: None,
    });
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .cloned()
}

#[test]
fn parameter_adapter_from_input_concrete() {
    // The motivating case: drag from a consumer pin of type T → spawn a
    // parameter whose output is T.
    let adapted =
        adapt_parameter(DataType::Crystal, DragDirection::FromInput).expect("should adapt");
    assert_eq!(adapted.data_type, DataType::Crystal);
}

#[test]
fn parameter_adapter_from_output_concrete() {
    // Drag from a value pin of type T → spawn a parameter whose `default`
    // input pin (and therefore output pin) is typed T.
    let adapted = adapt_parameter(DataType::Int, DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.data_type, DataType::Int);
}

#[test]
fn parameter_adapter_rejects_abstract() {
    // Abstract phase supertypes can't be a parameter declaration — no
    // concrete value would ever satisfy the pin.
    assert!(adapt_parameter(DataType::HasAtoms, DragDirection::FromInput).is_none());
    assert!(adapt_parameter(DataType::HasStructure, DragDirection::FromInput).is_none());
    assert!(adapt_parameter(DataType::HasFreeLinOps, DragDirection::FromOutput).is_none());
}

#[test]
fn parameter_adapter_rejects_function() {
    let f = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Int),
    });
    assert!(adapt_parameter(f, DragDirection::FromInput).is_none());
}

// --- Create-time test -----------------------------------------------------

fn get_parameter_data(designer: &StructureDesigner, node_id: u64) -> ParameterData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .cloned()
        .expect("ParameterData")
}

#[test]
fn add_node_with_drag_source_configures_parameter() {
    // Verifies the instantiation order: the adapter sets `data_type`, then
    // the existing parameter special-case in `add_node` overwrites
    // `param_id` / `param_name` / `sort_order` on top — both passes
    // compose cleanly because they touch disjoint fields.
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "parameter",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Crystal,
            direction: DragDirection::FromInput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_parameter_data(&designer, node_id);
    assert_eq!(
        data.data_type,
        DataType::Crystal,
        "adapter-set data_type must survive the parameter special-case"
    );
    assert!(
        data.param_id.is_some(),
        "parameter special-case must assign a param_id"
    );
    assert_eq!(
        data.param_name, "param0",
        "first parameter in a fresh network gets param0"
    );
    assert_eq!(data.sort_order, 0);
}

// ============================================================================
// Asymmetric verification: scalar→collection broadcast suppressed at Stage 2
// ============================================================================
//
// Stage 1 (static_match, permissive) keeps the type-system-wide
// `S → Array[T]` / `S → Iter[T]` broadcast rules — node authors who declared
// a collection input pin (e.g. `union.shapes: Array[Blueprint]`) still see
// scalar producers / their nodes still surface for scalar sources.
//
// Stage 2 (static_match_strict, adapter-verification) drops scalar broadcast
// — adapter-shapeshifted nodes whose resolved pin only accepts the source
// via broadcast are silently dropped from both the picker and the create
// path. See `doc/design_drag_aware_add_node.md` §"Asymmetric verification".

#[test]
fn scalar_blueprint_from_output_keeps_union_and_intersect() {
    // Stage-1 candidates that declared `Array[Blueprint]` or `Blueprint`
    // input pins keep showing up — their match goes through the permissive
    // `static_match`, which still allows `Blueprint → Array[Blueprint]`
    // broadcast.
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Blueprint, true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"union"),
        "union (Array[Blueprint] input) must keep surfacing via Stage 1 broadcast"
    );
    assert!(
        names.contains(&"intersect"),
        "intersect (Blueprint input) must keep surfacing"
    );
    assert!(
        names.contains(&"diff"),
        "diff (Blueprint input) must keep surfacing"
    );
}

#[test]
fn scalar_blueprint_from_output_suppresses_broadcast_only_adapter_nodes() {
    // The fix's core assertion: when the user drags a scalar `Blueprint`
    // from an output, adapter-using nodes whose resolved input pins are
    // collection-shaped (`Array[T]` / `Iter[T]`) and only match via the
    // scalar broadcast rule are dropped at Stage-2 verification.
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Blueprint, true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    // `fold` deliberately not in this list — after adapting `element_type`
    // and `accumulator_type` to `Blueprint`, fold's `init: Blueprint` pin
    // matches the source via identity (not broadcast), so the strict rule
    // correctly leaves it surfaced. The user can wire to `init` (genuine
    // intent) or to `xs` via the broadcast-only path the auto-connect
    // pin-picker will offer. Same reasoning for `sequence` (element-typed
    // input pins) and `array_append` (the `element: T` pin) — both keep
    // surfacing for principled, non-broadcast reasons.
    for n in [
        "map",
        "foreach",
        "filter",
        "array_at",
        "array_len",
        "array_concat",
        "collect",
    ] {
        assert!(
            !names.contains(&n),
            "{n} must not surface for scalar Blueprint from output (broadcast-only match); got {:?}",
            names
        );
    }
}

#[test]
fn scalar_blueprint_from_output_keeps_array_append_via_element_pin() {
    // `array_append` has two input pins after adaptation:
    //   - `array: Array[Blueprint]` — only via broadcast, rejected by strict
    //   - `element: Blueprint`      — identity, accepted by strict
    // At least one pin must accept under strict, so `array_append` keeps
    // surfacing. The auto-connect pin-picker disambiguates which pin gets
    // wired.
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Blueprint, true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"array_append"),
        "array_append must surface for scalar Blueprint (matches via `element` pin under strict); got {:?}",
        names
    );
}

#[test]
fn scalar_int_from_output_keeps_parameter_via_identity() {
    // `parameter`'s adapter sets `data_type = source_type` directly (no
    // peel), so the resolved input pin equals the source — identity match,
    // strict passes.
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Int, true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"parameter"),
        "parameter must surface for scalar Int (identity after adaptation); got {:?}",
        names
    );
}

#[test]
fn iter_int_from_output_still_surfaces_iterator_consumers() {
    // Sanity: the strict rule must not regress the Iter-source path.
    // map/filter/fold/collect all have adapters that peel the element
    // type; after adaptation, the resolved input pin is `Iter[Int]` —
    // identity with the source, no broadcast involved, strict passes.
    let registry = NodeTypeRegistry::new();
    let categories =
        registry.get_compatible_node_types(&DataType::Iterator(Box::new(DataType::Int)), true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    for n in ["map", "filter", "fold", "collect"] {
        assert!(
            names.contains(&n),
            "{n} must still surface for Iter[Int] from output; got {:?}",
            names
        );
    }
}

#[test]
fn array_blueprint_from_output_still_surfaces_array_and_iter_consumers() {
    // Sanity: dragging an `Array[Blueprint]` from an output still surfaces
    // `array_at`/`array_len`/`array_concat`/`collect` (identity on the
    // adapted `Array[Blueprint]` pin) and `map`/`foreach`/`filter` (the
    // `Array[S] → Iter[T]` eager-wrap rule survives the strict predicate).
    let registry = NodeTypeRegistry::new();
    let categories =
        registry.get_compatible_node_types(&DataType::Array(Box::new(DataType::Blueprint)), true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    for n in [
        "array_at",
        "array_len",
        "array_concat",
        "collect",
        "map",
        "filter",
        "foreach",
    ] {
        assert!(
            names.contains(&n),
            "{n} must still surface for Array[Blueprint] from output; got {:?}",
            names
        );
    }
}

#[test]
fn add_node_with_drag_source_falls_back_when_only_broadcast_match() {
    // Create-time mirror of the picker rule. When the adapter would match
    // only via scalar→collection broadcast, the strict verification
    // rejects it and we keep the default `MapData` instead of installing
    // `MapData { input_type: Blueprint, output_type: Blueprint }`. This
    // protects callers that bypass the popup (CLI, scripted, stale popup
    // after concurrent edits).
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "map",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Blueprint,
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_map_data(&designer, node_id);
    // map's `node_data_creator` builds `MapData { input_type: Float,
    // output_type: Float }`. Strict verification must have rejected the
    // adapter, leaving us with these defaults (not Blueprint).
    assert_eq!(
        data.input_type,
        DataType::Float,
        "adapter must be rejected: scalar Blueprint into Iter[T] is broadcast-only"
    );
    assert_eq!(data.output_type, DataType::Float);
}

// ============================================================================
// expr adapter
// ============================================================================
//
// `expr` adapts by replacing its parameter list with a single passthrough
// parameter `x: T`, body `"x"`, and pre-running `parse_and_validate` so the
// new node has `expr: Some(_)` and `output_type: Some(T)` immediately.
// Both drag directions adapt identically — see
// `doc/design_drag_aware_add_node.md` §"Expr node specifics".

fn adapt_expr(source: DataType, dir: DragDirection) -> Option<ExprData> {
    let registry = NodeTypeRegistry::new();
    // Use the default created by `expr_get_node_type().node_data_creator`
    // to mirror what the popup actually feeds the adapter.
    let data = (rust_lib_flutter_cad::structure_designer::nodes::expr::get_node_type()
        .node_data_creator)();
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<ExprData>().cloned()
}

#[test]
fn expr_adapter_from_output_int() {
    let adapted = adapt_expr(DataType::Int, DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.parameters.len(), 1);
    assert_eq!(adapted.parameters[0].name, "x");
    assert_eq!(adapted.parameters[0].data_type, DataType::Int);
    assert_eq!(adapted.expression, "x");
    assert!(
        adapted.expr.is_some(),
        "parse_and_validate must have populated `expr`"
    );
    assert_eq!(adapted.output_type, Some(DataType::Int));
    assert!(
        adapted.error.is_none(),
        "identity body against a valid parameter type must validate cleanly; got {:?}",
        adapted.error
    );
}

#[test]
fn expr_adapter_from_output_float() {
    let adapted = adapt_expr(DataType::Float, DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.parameters[0].data_type, DataType::Float);
    assert_eq!(adapted.output_type, Some(DataType::Float));
}

#[test]
fn expr_adapter_from_output_vec3() {
    let adapted = adapt_expr(DataType::Vec3, DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.parameters[0].data_type, DataType::Vec3);
    assert_eq!(adapted.output_type, Some(DataType::Vec3));
}

#[test]
fn expr_adapter_from_output_concrete_phase_type() {
    // Concrete phase types (`Blueprint`, `Crystal`, `Molecule`) are valid
    // expr parameter types — the body `x` just passes the value through.
    // Useful as a typed pass-through the user immediately replaces with a
    // real expression.
    let adapted = adapt_expr(DataType::Crystal, DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.parameters[0].data_type, DataType::Crystal);
    assert_eq!(adapted.output_type, Some(DataType::Crystal));
}

#[test]
fn expr_adapter_from_output_record() {
    // Named record types are valid expr parameter types — expressions can
    // member-access them (`x.from`, `x.to`). `ElementMapping` is a built-in
    // record def, so it's always present in the registry.
    let source = DataType::Record(RecordType::Named("ElementMapping".to_string()));
    let adapted = adapt_expr(source.clone(), DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.parameters[0].data_type, source);
    assert_eq!(adapted.output_type, Some(source));
}

#[test]
fn expr_adapter_from_output_array() {
    // `Array[T]` is a valid expr parameter type — `len(x)` and `x[i]` work.
    let source = DataType::Array(Box::new(DataType::Int));
    let adapted = adapt_expr(source.clone(), DragDirection::FromOutput).expect("should adapt");
    assert_eq!(adapted.parameters[0].data_type, source);
    assert_eq!(adapted.output_type, Some(source));
}

#[test]
fn expr_adapter_from_input_int() {
    // FromInput direction: the user dragged from a target input pin of type
    // Int and wants a node that produces Int. Identity body means
    // output type == input type, so the adapter is symmetric.
    let adapted = adapt_expr(DataType::Int, DragDirection::FromInput).expect("should adapt");
    assert_eq!(adapted.parameters[0].data_type, DataType::Int);
    assert_eq!(adapted.output_type, Some(DataType::Int));
}

#[test]
fn expr_adapter_rejects_abstract() {
    assert!(adapt_expr(DataType::HasAtoms, DragDirection::FromOutput).is_none());
    assert!(adapt_expr(DataType::HasStructure, DragDirection::FromOutput).is_none());
    assert!(adapt_expr(DataType::HasFreeLinOps, DragDirection::FromOutput).is_none());
}

#[test]
fn expr_adapter_rejects_function() {
    let fn_ty = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Int),
    });
    assert!(adapt_expr(fn_ty, DragDirection::FromOutput).is_none());
}

#[test]
fn expr_adapter_rejects_iterator() {
    // Iter[T] is rejected: lazy walkers can't be re-read from the
    // variables map across multiple uses, so they don't behave like data
    // values in the expression language.
    assert!(
        adapt_expr(
            DataType::Iterator(Box::new(DataType::Int)),
            DragDirection::FromOutput
        )
        .is_none()
    );
}

#[test]
fn expr_adapter_rejects_unit() {
    assert!(adapt_expr(DataType::Unit, DragDirection::FromOutput).is_none());
}

// --- Filter (popup) integration -------------------------------------------

#[test]
fn int_from_output_surfaces_expr_via_adapter() {
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Int, true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"expr"),
        "expr must surface for scalar Int from output; got {:?}",
        names
    );
}

#[test]
fn float_from_output_surfaces_expr_via_adapter() {
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Float, true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"expr"),
        "expr must surface for scalar Float from output"
    );
}

#[test]
fn iter_int_from_output_does_not_surface_expr() {
    // Sanity: the expr adapter rejects Iter[T], so the filter must not
    // surface `expr` for an iterator drag source.
    let registry = NodeTypeRegistry::new();
    let categories =
        registry.get_compatible_node_types(&DataType::Iterator(Box::new(DataType::Int)), true);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        !names.contains(&"expr"),
        "expr must not surface for Iter[Int] from output (adapter rejects iterators); got {:?}",
        names
    );
}

#[test]
fn crystal_from_input_surfaces_expr_via_adapter() {
    // FromInput direction: dragging from a Crystal-typed consumer pin
    // should surface `expr` (with x: Crystal, output Crystal).
    let registry = NodeTypeRegistry::new();
    let categories = registry.get_compatible_node_types(&DataType::Crystal, false);
    let names: Vec<&str> = categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect();
    assert!(
        names.contains(&"expr"),
        "expr must surface for Crystal-typed input drag; got {:?}",
        names
    );
}

// --- Create-time tests ----------------------------------------------------

fn get_expr_data(designer: &StructureDesigner, node_id: u64) -> ExprData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<ExprData>()
        .cloned()
        .expect("ExprData")
}

#[test]
fn add_node_with_drag_source_configures_expr_float() {
    // The full create-time path: popup picks `expr` after dragging from a
    // Float output, the adapter primes the data, and we verify the new
    // node arrives with `expr` parsed and `output_type` derived — not the
    // default-data Int template.
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "expr",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Float,
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_expr_data(&designer, node_id);
    assert_eq!(data.parameters.len(), 1);
    assert_eq!(data.parameters[0].name, "x");
    assert_eq!(data.parameters[0].data_type, DataType::Float);
    assert_eq!(data.expression, "x");
    assert_eq!(data.output_type, Some(DataType::Float));
    assert!(
        data.expr.is_some(),
        "expr must arrive parsed — without `parse_and_validate` in the adapter, eval would return \"Expression not parsed\""
    );
}

#[test]
fn add_node_with_drag_source_falls_back_for_expr_on_iter() {
    // expr's adapter rejects `Iter[T]`, so create-time should keep the
    // default Int template (not adopt some half-resolved iter shape).
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "expr",
        DVec2::ZERO,
        Some(DragSource {
            source_type: DataType::Iterator(Box::new(DataType::Int)),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_ne!(node_id, 0);
    let data = get_expr_data(&designer, node_id);
    // Default ExprData from `node_data_creator`: x: Int, expr "x",
    // output Int.
    assert_eq!(data.parameters[0].data_type, DataType::Int);
    assert_eq!(data.expression, "x");
}

// ============================================================================
// Closure adapter (Tier 1) + map.f drag hint (Tier 2)
// `doc/design_drag_aware_add_node.md`
// ============================================================================

fn adapt_closure(source: DataType, dir: DragDirection) -> Option<ClosureData> {
    let registry = NodeTypeRegistry::new();
    let data: Box<dyn NodeData> = Box::new(ClosureData::default());
    let adapted = data.adapt_for_drag_source(&source, dir, &registry)?;
    adapted.as_any_ref().downcast_ref::<ClosureData>().cloned()
}

fn fn_type(params: Vec<DataType>, ret: DataType) -> DataType {
    DataType::Function(FunctionType::new(params, ret))
}

// --- concrete Function sources (filter.f / fold.f / foreach.f shapes) -------

#[test]
fn closure_adapter_filter_shape() {
    // filter.f is `Function([elem], Bool)` → Filter closure.
    let adapted = adapt_closure(
        fn_type(vec![DataType::IVec3], DataType::Bool),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Filter);
    assert_eq!(adapted.type_args, vec![DataType::IVec3]);
}

#[test]
fn closure_adapter_foreach_shape() {
    // foreach.f is `Function([elem], Unit)` → Foreach closure.
    let adapted = adapt_closure(
        fn_type(vec![DataType::Int], DataType::Unit),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Foreach);
    assert_eq!(adapted.type_args, vec![DataType::Int]);
}

#[test]
fn closure_adapter_fold_shape() {
    // fold.f is `Function([acc, elem], acc)` → Fold closure.
    let adapted = adapt_closure(
        fn_type(vec![DataType::Float, DataType::Int], DataType::Float),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Fold);
    assert_eq!(adapted.type_args, vec![DataType::Float, DataType::Int]);
}

#[test]
fn closure_adapter_map_shape_from_concrete_function() {
    // A concrete `(T) -> U` with non-Bool/Unit return → Map closure. This is
    // also the Tier 2 path for map.f once the drag hint supplies the concrete
    // signature with output_type != input_type.
    let adapted = adapt_closure(
        fn_type(vec![DataType::Int], DataType::Crystal),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Map);
    assert_eq!(adapted.type_args, vec![DataType::Int, DataType::Crystal]);
}

#[test]
fn closure_adapter_custom_shape_for_arity_three() {
    // No preset matches arity 3 → Custom closure reproducing the signature.
    let adapted = adapt_closure(
        fn_type(
            vec![DataType::Int, DataType::Bool, DataType::Float],
            DataType::String,
        ),
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Custom);
    assert_eq!(adapted.param_names, vec!["p0", "p1", "p2"]);
    assert_eq!(
        adapted.type_args,
        vec![
            DataType::Int,
            DataType::Bool,
            DataType::Float,
            DataType::String
        ]
    );
}

// --- AnyFunction sources (map.f declared type, lossy) -----------------------

#[test]
fn closure_adapter_anyfunction_defaults_return_to_param() {
    // map.f is `AnyFunction { leading_params: [Int] }` — the return type is not
    // carried, so it defaults to the (last) parameter → `(Int) -> Int`. This is
    // the user's motivating example (map Int,Int → closure Int -> Int).
    let adapted = adapt_closure(
        DataType::AnyFunction {
            leading_params: vec![DataType::Int],
        },
        DragDirection::FromInput,
    )
    .expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Map);
    assert_eq!(adapted.type_args, vec![DataType::Int, DataType::Int]);
}

#[test]
fn closure_adapter_anyfunction_empty_is_none() {
    // apply.f is `AnyFunction { leading_params: [] }` ("any function") — nothing
    // to infer, leave the default closure.
    assert!(
        adapt_closure(
            DataType::AnyFunction {
                leading_params: vec![],
            },
            DragDirection::FromInput,
        )
        .is_none()
    );
}

// --- rejected cases ---------------------------------------------------------

#[test]
fn closure_adapter_from_output_is_none() {
    // A closure produces a function; dragging from an output to place a closure
    // is not a real workflow.
    assert!(
        adapt_closure(
            fn_type(vec![DataType::Int], DataType::Int),
            DragDirection::FromOutput,
        )
        .is_none()
    );
}

#[test]
fn closure_adapter_non_function_source_is_none() {
    assert!(adapt_closure(DataType::Int, DragDirection::FromInput).is_none());
    assert!(adapt_closure(DataType::Crystal, DragDirection::FromInput).is_none());
}

// --- create-time: closure picked from a function-typed input pin ------------

#[test]
fn add_node_with_drag_source_configures_closure_from_filter_f() {
    let mut designer = setup_designer();
    let node_id = designer.add_node_with_drag_source(
        "closure",
        DVec2::ZERO,
        Some(DragSource {
            source_type: fn_type(vec![DataType::Int], DataType::Bool),
            direction: DragDirection::FromInput,
        }),
    );
    assert_ne!(node_id, 0);
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let data = net
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .cloned()
        .expect("ClosureData");
    assert_eq!(data.kind, ClosureKind::Filter);
    assert_eq!(data.type_args, vec![DataType::Int]);
}

// --- Tier 2: map.f exposes a concrete drag hint -----------------------------

#[test]
fn map_drag_hint_exposes_concrete_function_signature() {
    let data = MapData {
        input_type: DataType::Int,
        output_type: DataType::Crystal,
    };
    // The `f` pin (index 1) hints the concrete `(input_type) -> output_type`.
    assert_eq!(
        data.drag_hint_for_input_pin(1),
        Some(fn_type(vec![DataType::Int], DataType::Crystal))
    );
    // The `xs` pin (index 0) has no hint — its declared type is concrete.
    assert_eq!(data.drag_hint_for_input_pin(0), None);
}

#[test]
fn map_drag_hint_round_trips_through_string_into_closure() {
    // End-to-end Tier 2 path: the API layer serializes the hint via
    // `to_string()` and re-parses it via `from_string()` before handing it to
    // the closure adapter. Verify a map `Int -> Crystal` hint survives that
    // round-trip and produces a `(Int) -> Crystal` Map closure.
    let data = MapData {
        input_type: DataType::Int,
        output_type: DataType::Crystal,
    };
    let hint = data.drag_hint_for_input_pin(1).expect("hint");
    let reparsed = DataType::from_string(&hint.to_string()).expect("round-trip");
    let adapted = adapt_closure(reparsed, DragDirection::FromInput).expect("should adapt");
    assert_eq!(adapted.kind, ClosureKind::Map);
    assert_eq!(adapted.type_args, vec![DataType::Int, DataType::Crystal]);
}

// ============================================================================
// Record node drag adapters (issue #312)
//
// Dragging from `record_construct`'s output (a `Record(Named(..))`) should
// surface `record_destructure`, and dragging from `record_destructure`'s input
// should surface `record_construct` — each instantiated with the dragged
// record's schema already chosen. Both directions are verified against the
// registry-aware resolved pin layout (`resolve_drag_candidate_type`), not the
// placeholder `Record(Named(""))` base pins.
// ============================================================================

fn point_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Point".to_string(),
        vec![
            ("x".to_string(), DataType::Float),
            ("y".to_string(), DataType::Float),
        ],
    )
}

fn point_type() -> DataType {
    DataType::Record(RecordType::Named("Point".to_string()))
}

fn setup_designer_with_point() -> StructureDesigner {
    let mut designer = setup_designer();
    designer.add_record_type_def(point_def()).expect("add def");
    designer
}

fn names_of(
    categories: &[rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APINodeCategoryView],
) -> Vec<&str> {
    categories
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| n.name.as_str()))
        .collect()
}

fn get_record_construct_data(designer: &StructureDesigner, node_id: u64) -> RecordConstructData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    net.nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .cloned()
        .expect("RecordConstructData")
}

fn get_record_destructure_data(
    designer: &StructureDesigner,
    node_id: u64,
) -> RecordDestructureData {
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    net.nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<RecordDestructureData>()
        .cloned()
        .expect("RecordDestructureData")
}

// --- Popup filtering ---------------------------------------------------------

#[test]
fn record_destructure_surfaces_for_record_output_drag() {
    let designer = setup_designer_with_point();
    let categories = designer
        .node_type_registry
        .get_compatible_node_types(&point_type(), true);
    let names = names_of(&categories);
    assert!(
        names.contains(&"record_destructure"),
        "expected record_destructure for a record output drag, got {names:?}"
    );
}

#[test]
fn record_construct_surfaces_for_record_input_drag() {
    let designer = setup_designer_with_point();
    let categories = designer
        .node_type_registry
        .get_compatible_node_types(&point_type(), false);
    let names = names_of(&categories);
    assert!(
        names.contains(&"record_construct"),
        "expected record_construct for a record input drag, got {names:?}"
    );
}

#[test]
fn record_nodes_respect_drag_direction_in_popup() {
    // construct only adapts FromInput; destructure only adapts FromOutput.
    let designer = setup_designer_with_point();
    let from_output_cats = designer
        .node_type_registry
        .get_compatible_node_types(&point_type(), true);
    let from_input_cats = designer
        .node_type_registry
        .get_compatible_node_types(&point_type(), false);
    let from_output = names_of(&from_output_cats);
    let from_input = names_of(&from_input_cats);
    assert!(
        !from_output.contains(&"record_construct"),
        "record_construct should not surface dragging from an output pin"
    );
    assert!(
        !from_input.contains(&"record_destructure"),
        "record_destructure should not surface dragging from an input pin"
    );
}

#[test]
fn unknown_record_name_surfaces_no_record_nodes() {
    // A dangling named record (no registered def) must not pre-set a schema,
    // so neither record node should appear.
    let designer = setup_designer_with_point();
    let missing = DataType::Record(RecordType::Named("Nope".to_string()));
    let from_output_cats = designer
        .node_type_registry
        .get_compatible_node_types(&missing, true);
    let from_input_cats = designer
        .node_type_registry
        .get_compatible_node_types(&missing, false);
    let from_output = names_of(&from_output_cats);
    let from_input = names_of(&from_input_cats);
    assert!(!from_output.contains(&"record_destructure"));
    assert!(!from_input.contains(&"record_construct"));
}

// --- Adapter units -----------------------------------------------------------

#[test]
fn record_construct_adapter_sets_schema_from_input() {
    let designer = setup_designer_with_point();
    let adapted = RecordConstructData::default()
        .adapt_for_drag_source(
            &point_type(),
            DragDirection::FromInput,
            &designer.node_type_registry,
        )
        .expect("should adapt FromInput");
    let data = adapted
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .expect("RecordConstructData");
    assert_eq!(data.schema, "Point");
}

#[test]
fn record_construct_adapter_rejects_from_output_and_unknowns() {
    let designer = setup_designer_with_point();
    let reg = &designer.node_type_registry;
    // Wrong direction.
    assert!(
        RecordConstructData::default()
            .adapt_for_drag_source(&point_type(), DragDirection::FromOutput, reg)
            .is_none()
    );
    // Unknown named def.
    assert!(
        RecordConstructData::default()
            .adapt_for_drag_source(
                &DataType::Record(RecordType::Named("Nope".to_string())),
                DragDirection::FromInput,
                reg,
            )
            .is_none()
    );
    // Anonymous record (no def name to store).
    let anon = DataType::Record(RecordType::anonymous(vec![(
        "x".to_string(),
        DataType::Float,
    )]));
    assert!(
        RecordConstructData::default()
            .adapt_for_drag_source(&anon, DragDirection::FromInput, reg)
            .is_none()
    );
    // Non-record source.
    assert!(
        RecordConstructData::default()
            .adapt_for_drag_source(&DataType::Int, DragDirection::FromInput, reg)
            .is_none()
    );
}

#[test]
fn record_destructure_adapter_sets_schema_from_output() {
    let designer = setup_designer_with_point();
    let adapted = RecordDestructureData::default()
        .adapt_for_drag_source(
            &point_type(),
            DragDirection::FromOutput,
            &designer.node_type_registry,
        )
        .expect("should adapt FromOutput");
    let data = adapted
        .as_any_ref()
        .downcast_ref::<RecordDestructureData>()
        .expect("RecordDestructureData");
    assert_eq!(data.schema, "Point");
}

#[test]
fn record_destructure_adapter_rejects_from_input() {
    let designer = setup_designer_with_point();
    assert!(
        RecordDestructureData::default()
            .adapt_for_drag_source(
                &point_type(),
                DragDirection::FromInput,
                &designer.node_type_registry
            )
            .is_none()
    );
}

// --- Create-time plumbing ----------------------------------------------------

#[test]
fn add_node_with_drag_source_configures_record_destructure() {
    let mut designer = setup_designer_with_point();
    let node_id = designer.add_node_with_drag_source(
        "record_destructure",
        DVec2::ZERO,
        Some(DragSource {
            source_type: point_type(),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_eq!(
        get_record_destructure_data(&designer, node_id).schema,
        "Point"
    );
}

#[test]
fn add_node_with_drag_source_configures_record_construct() {
    let mut designer = setup_designer_with_point();
    let node_id = designer.add_node_with_drag_source(
        "record_construct",
        DVec2::ZERO,
        Some(DragSource {
            source_type: point_type(),
            direction: DragDirection::FromInput,
        }),
    );
    assert_eq!(
        get_record_construct_data(&designer, node_id).schema,
        "Point"
    );
}

#[test]
fn add_node_with_drag_source_record_construct_falls_back_on_wrong_direction() {
    // construct's adapter rejects FromOutput, so create falls back to default
    // data (empty schema) rather than mis-configuring the node.
    let mut designer = setup_designer_with_point();
    let node_id = designer.add_node_with_drag_source(
        "record_construct",
        DVec2::ZERO,
        Some(DragSource {
            source_type: point_type(),
            direction: DragDirection::FromOutput,
        }),
    );
    assert_eq!(get_record_construct_data(&designer, node_id).schema, "");
}
