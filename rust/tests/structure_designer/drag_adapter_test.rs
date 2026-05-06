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
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, FunctionType};
use rust_lib_flutter_cad::structure_designer::node_data::{DragDirection, NodeData};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::array_append::ArrayAppendData;
use rust_lib_flutter_cad::structure_designer::nodes::array_at::ArrayAtData;
use rust_lib_flutter_cad::structure_designer::nodes::array_concat::ArrayConcatData;
use rust_lib_flutter_cad::structure_designer::nodes::array_len::ArrayLenData;
use rust_lib_flutter_cad::structure_designer::nodes::collect::CollectData;
use rust_lib_flutter_cad::structure_designer::nodes::filter::FilterData;
use rust_lib_flutter_cad::structure_designer::nodes::fold::FoldData;
use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::nodes::range::RangeData;
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
    let adapted =
        adapt_map(DataType::Vec3, DragDirection::FromOutput).expect("scalar broadcast");
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
    assert!(adapt_collect(
        DataType::Iterator(Box::new(DataType::Float)),
        DragDirection::FromInput
    )
    .is_none());
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
    assert_eq!(adapted.element_type, DataType::Array(Box::new(DataType::Int)));
}

#[test]
fn array_at_adapter_rejects_abstract_from_input() {
    assert!(adapt_array_at(DataType::HasAtoms, DragDirection::FromInput).is_none());
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

// --- sequence -------------------------------------------------------------

#[test]
fn sequence_adapter_from_output_scalar() {
    // sequence's input pins are typed `T` directly; setting element_type
    // to the source as-is is the right move.
    let adapted = adapt_sequence(DataType::Float, DragDirection::FromOutput)
        .expect("scalar source");
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

// --- Filter (popup) integration -------------------------------------------

#[test]
fn array_int_from_output_surfaces_all_array_nodes_via_adapter() {
    // None of these nodes' static defaults match `Array[Int]` directly
    // (they default to either `Array[Int]` or some other element). What we
    // care about is that after Phase 2 adapters, all of them surface in
    // the popup for an `Array[Int]` drag from an output pin.
    let registry = NodeTypeRegistry::new();
    let categories = registry
        .get_compatible_node_types(&DataType::Array(Box::new(DataType::Int)), true);
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
