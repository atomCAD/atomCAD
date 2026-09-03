//! Identifier positions the serializer must quote.
//!
//! Node names and node-type names were already backtick-quoted when they are
//! not bare identifiers; a **property key** was not, so a custom node whose
//! parameter is named after a dotted network (`structure.14Si3_cap`, from a
//! `parameter` node auto-named after its default's network) serialized as
//! `{ structure.14Si3_cap: x }` and read back as `structure` `.` `14Si3_cap`
//! — "Expected Colon, found Dot" — which made `query` → `edit --replace`
//! fail on the maintainer's working file.

use atomcad_structure_designer::preferences::NodeDisplayPolicy;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::serialize_network;

fn designer() -> StructureDesigner {
    let mut sd = StructureDesigner::new();
    sd.preferences.node_display_preferences.display_policy = NodeDisplayPolicy::Manual;
    sd
}

fn activate(sd: &mut StructureDesigner, name: &str) {
    sd.set_active_node_network_name(Some(name.to_string()));
}

fn applied(sd: &StructureDesigner) -> bool {
    sd.ai_edit_log.last().is_some_and(|record| record.applied)
}

fn text(sd: &StructureDesigner, name: &str) -> String {
    let network = sd.node_type_registry.node_networks.get(name).unwrap();
    serialize_network(network, &sd.node_type_registry, Some(name))
}

#[test]
fn a_dotted_property_key_is_quoted_and_reads_back() {
    let mut sd = designer();

    // A custom node whose one parameter carries a dotted name.
    sd.add_node_network("sub");
    activate(&mut sd, "sub");
    sd.ai_text_edit(
        "v = int { value: 1 }\n\
         p = parameter { param_name: \"structure.cap\", data_type: Int, sort_order: 0, default: v }\n\
         output p\n",
        false,
    );
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);

    // An instance wiring that parameter, written with the quoted key.
    sd.add_node_network("main");
    activate(&mut sd, "main");
    sd.ai_text_edit(
        "x = int { value: 7 }\ninst = sub { `structure.cap`: x }\n",
        false,
    );
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);

    let first = text(&sd, "main");
    assert!(
        first.contains("`structure.cap`: x"),
        "the key must be backtick-quoted:\n{first}"
    );

    // The serializer's own output is accepted back, and is stable.
    sd.ai_text_edit(&first, true);
    assert!(applied(&sd), "{:?}", sd.ai_edit_log.last().unwrap().errors);
    assert_eq!(text(&sd, "main"), first);
}
