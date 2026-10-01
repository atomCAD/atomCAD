//! The api-side preference twins convert both ways without losing a field.
//!
//! The twin pattern (`rust/AGENTS.md`, `doc/design_rust_crate_split.md` D9a)
//! keeps a same-named Dart-facing copy of every persisted setting in
//! `api/structure_designer/structure_designer_preferences.rs`, and the compiler
//! catches a *missing* field (the `From` impls are exhaustive struct literals).
//! What it cannot catch is a field wired to the wrong neighbour, so the
//! round-trip is asserted here.
//!
//! `title_mode` (`doc/design_node_names_in_ui.md` D4) is the current subject:
//! the display panel writes it through `set_structure_designer_preferences`,
//! which is the only place the conversion runs.

use atomcad_structure_designer::preferences as domain;
use rust_lib_flutter_cad::api::common_api_types::APIIVec3;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::{
    AtomicStructureVisualizationPreferences, DocumentTabPlacement, InterfacePreferences,
    NodeDisplayPolicy, NodeDisplayPreferences, NodeTitleMode, StructureDesignerPreferences,
};

#[test]
fn node_title_mode_converts_both_ways() {
    for (api, dom) in [
        (NodeTitleMode::Type, domain::NodeTitleMode::Type),
        (NodeTitleMode::Name, domain::NodeTitleMode::Name),
    ] {
        let down: domain::NodeTitleMode = (&api).into();
        assert_eq!(down, dom);
        let up: NodeTitleMode = (&dom).into();
        assert_eq!(up, api);
    }
}

/// The title mode travels beside the display policy rather than in place of it —
/// the two are independent switches in the same display-panel cluster.
#[test]
fn node_display_preferences_carry_both_switches() {
    let api = NodeDisplayPreferences {
        display_policy: NodeDisplayPolicy::PreferFrontier,
        title_mode: NodeTitleMode::Name,
    };

    let down: domain::NodeDisplayPreferences = (&api).into();
    assert_eq!(
        down.display_policy,
        domain::NodeDisplayPolicy::PreferFrontier
    );
    assert_eq!(down.title_mode, domain::NodeTitleMode::Name);

    let up: NodeDisplayPreferences = (&down).into();
    assert_eq!(up.display_policy, NodeDisplayPolicy::PreferFrontier);
    assert_eq!(up.title_mode, NodeTitleMode::Name);
}

/// The envelope-cage switch and its colour are an ordinary preference pair, and
/// the round-trip is what catches one of them wired to the wrong neighbour —
/// `show_tool_envelopes` is the only `bool` in its struct after
/// `scene_transparency_enabled`, and `tool_envelope_color` the only colour.
#[test]
fn the_tool_envelope_preferences_convert_both_ways() {
    let api = AtomicStructureVisualizationPreferences {
        show_tool_envelopes: true,
        tool_envelope_color: APIIVec3 {
            x: 12,
            y: 34,
            z: 56,
        },
        ..AtomicStructureVisualizationPreferences::default()
    };

    let down: domain::AtomicStructureVisualizationPreferences = (&api).into();
    assert!(down.show_tool_envelopes);
    assert_eq!(down.tool_envelope_color, domain::PrefColor::new(12, 34, 56));
    // The neighbours it must not have been wired to.
    assert!(!down.scene_transparency_enabled);

    let up: AtomicStructureVisualizationPreferences = (&down).into();
    assert!(up.show_tool_envelopes);
    assert_eq!(up.tool_envelope_color.x, 12);
    assert_eq!(up.tool_envelope_color.y, 34);
    assert_eq!(up.tool_envelope_color.z, 56);
}

/// Off, amber, by default — a way of looking at every build rather than a
/// property of one, so it is not on until asked for.
#[test]
fn the_tool_envelope_defaults_are_off_and_amber() {
    let api = AtomicStructureVisualizationPreferences::default();
    assert!(!api.show_tool_envelopes);
    assert_eq!(api.tool_envelope_color.x, 255);
    assert_eq!(api.tool_envelope_color.y, 160);
    assert_eq!(api.tool_envelope_color.z, 0);

    let down: domain::AtomicStructureVisualizationPreferences = (&api).into();
    assert!(down == domain::AtomicStructureVisualizationPreferences::default());
}

/// The document-tab placement (`doc/design_multiple_documents.md` D10)
/// converts both ways, and the twin's default is the domain's: the vertical
/// list left of the viewport. A `preferences.json` without the group loads
/// with it through the twin too, since the dialog round-trips the twin.
#[test]
fn document_tab_placement_converts_both_ways() {
    for (api, dom) in [
        (
            DocumentTabPlacement::LeftOfViewport,
            domain::DocumentTabPlacement::LeftOfViewport,
        ),
        (
            DocumentTabPlacement::AboveNetworkEditor,
            domain::DocumentTabPlacement::AboveNetworkEditor,
        ),
    ] {
        let prefs = InterfacePreferences {
            document_tab_placement: api,
        };
        let down: domain::InterfacePreferences = (&prefs).into();
        assert_eq!(down.document_tab_placement, dom);
        let up: InterfacePreferences = (&down).into();
        assert_eq!(up.document_tab_placement, api);
    }

    let whole = StructureDesignerPreferences {
        interface_preferences: InterfacePreferences {
            document_tab_placement: DocumentTabPlacement::AboveNetworkEditor,
        },
        ..Default::default()
    };
    let down: domain::StructureDesignerPreferences = (&whole).into();
    assert_eq!(
        down.interface_preferences.document_tab_placement,
        domain::DocumentTabPlacement::AboveNetworkEditor
    );

    assert_eq!(
        InterfacePreferences::default().document_tab_placement,
        DocumentTabPlacement::LeftOfViewport
    );
    let old: StructureDesignerPreferences =
        serde_json::from_str(r#"{"layout_preferences": {}}"#).unwrap();
    assert_eq!(
        old.interface_preferences.document_tab_placement,
        DocumentTabPlacement::LeftOfViewport
    );
}
