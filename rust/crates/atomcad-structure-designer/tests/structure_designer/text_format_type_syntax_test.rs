//! Type syntax in property position.
//!
//! `DataType`'s `Display` is what `query` writes for a type-valued property,
//! and the parser must read every spelling it can produce: `A -> B`,
//! `(A, B) -> C`, `() -> T`, `[T]`, `Iter[T]`, `Optional[T]`, and nestings.
//! Until this landed only a bare builtin name parsed, so a network holding a
//! function- or array-typed `parameter` could not round-trip through
//! `query` → `edit --replace` — the maintainer's working file had both.
//!
//! Each case is checked the way it failed: author the type, serialize, assert
//! the canonical spelling, then `--replace` with that text and assert the
//! serialization is unchanged.

use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;

fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd.add_node_network("main");
    sd.set_active_node_network_name(Some("main".to_string()));
    sd
}

fn text(sd: &StructureDesigner) -> String {
    let network = sd.node_type_registry.node_networks.get("main").unwrap();
    serialize_network(network, &sd.node_type_registry, Some("main"))
}

/// Author `spelling` as a parameter's `data_type`, expect `canonical` in the
/// serialization, and expect a `--replace` of that serialization to be exact.
fn round_trip(spelling: &str, canonical: &str) {
    let mut sd = designer();
    let script =
        format!("p = parameter {{ param_name: \"a\", data_type: {spelling}, sort_order: 0 }}\n");
    sd.ai_text_edit(&script, false);
    assert!(
        applied(&sd),
        "{spelling}: {:?}",
        sd.ai_edit_log.last().unwrap().errors
    );

    let first = text(&sd);
    assert!(
        first.contains(&format!("data_type: {canonical}")),
        "{spelling} should serialize as `{canonical}`, got:\n{first}"
    );

    sd.ai_text_edit(&first, true);
    assert!(
        applied(&sd),
        "replacing with its own text: {:?}",
        sd.ai_edit_log.last().unwrap().errors
    );
    assert_eq!(
        text(&sd),
        first,
        "`{canonical}` does not survive a --replace"
    );
}

#[test]
fn an_array_type_is_read_from_a_one_element_type_list() {
    round_trip("[String]", "[String]");
}

#[test]
fn a_nested_array_type_folds_recursively() {
    round_trip("[[Int]]", "[[Int]]");
}

#[test]
fn a_unary_function_type() {
    round_trip(
        "HasStructure -> HasStructure",
        "HasStructure -> HasStructure",
    );
}

#[test]
fn an_array_of_function_types() {
    round_trip(
        "[HasStructure -> HasStructure]",
        "[HasStructure -> HasStructure]",
    );
}

#[test]
fn a_binary_function_type_with_or_without_the_space() {
    round_trip("(Int, Float) -> Bool", "(Int,Float) -> Bool");
    round_trip("(Int,Float) -> Bool", "(Int,Float) -> Bool");
}

#[test]
fn a_nullary_function_type() {
    round_trip("() -> Int", "() -> Int");
}

#[test]
fn a_function_returning_a_function_is_flattened() {
    round_trip("Int -> Float -> Bool", "(Int,Float) -> Bool");
}

#[test]
fn an_iterator_and_an_optional() {
    round_trip("Iter[Int]", "Iter[Int]");
    round_trip("Optional[Vec3]", "Optional[Vec3]");
    round_trip("Iter[[Int]]", "Iter[[Int]]");
}

#[test]
fn a_one_element_type_list_is_still_a_list_for_type_args() {
    // `closure { type_args: [Crystal] }` reads a *list* of one type, which is
    // why the fold lives in the consumer and not in the parser.
    let mut sd = designer();
    sd.ai_text_edit(
        "c = closure { kind: \"custom\", params: [], type_args: [Crystal] }\n",
        false,
    );
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    let first = text(&sd);
    assert!(first.contains("type_args: [Crystal]"), "{first}");
}

/// Whether the last edit's statements parsed and landed — the editor's own
/// verdict, before whole-network validation is folded in.
fn applied(sd: &StructureDesigner) -> bool {
    sd.ai_edit_log.last().is_some_and(|record| record.applied)
}

#[test]
fn a_function_type_inside_an_expr_parameter_object_literal() {
    // Object-literal values used to go through a separate mini-parser that
    // knew no type syntax at all, so `expr { parameters: [{ …, data_type:
    // (Crystal,Molecule) -> Crystal }] }` — the serializer's own output for
    // an identity expr over a function value — was a parse error.
    let mut sd = designer();
    sd.ai_text_edit(
        "e = expr { expression: \"x\", parameters: [{ name: \"x\", data_type: (Crystal,Molecule) -> Crystal }] }\n",
        false,
    );
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    let first = text(&sd);
    assert!(
        first.contains("data_type: (Crystal,Molecule) -> Crystal }"),
        "{first}"
    );
    sd.ai_text_edit(&first, true);
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    assert_eq!(text(&sd), first);
}
