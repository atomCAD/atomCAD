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
    LightingPreset, NodeDisplayPolicy, NodeDisplayPreferences, NodeTitleMode, RenderingPreferences,
    StructureDesignerPreferences,
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
            DocumentTabPlacement::AboveViewport,
            domain::DocumentTabPlacement::AboveViewport,
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
            document_tab_placement: DocumentTabPlacement::AboveViewport,
        },
        ..Default::default()
    };
    let down: domain::StructureDesignerPreferences = (&whole).into();
    assert_eq!(
        down.interface_preferences.document_tab_placement,
        domain::DocumentTabPlacement::AboveViewport
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

/// The lighting preset (`doc/design_depth_perception.md` D8) converts both ways
/// and travels beside the two rendering-quality switches, not in place of one.
#[test]
fn the_lighting_preset_converts_both_ways() {
    for (api, dom) in [
        (LightingPreset::Studio, domain::LightingPreset::Studio),
        (LightingPreset::Headlight, domain::LightingPreset::Headlight),
    ] {
        let prefs = RenderingPreferences {
            anti_aliasing: false,
            full_display_resolution: true,
            lighting: api,
            depth_cue: true,
            depth_cue_strength: 0.5,
            ambient_occlusion: true,
            ambient_occlusion_strength: 1.0,
            shadows: true,
            shadow_strength: 0.7,
        };
        let down: domain::RenderingPreferences = (&prefs).into();
        assert_eq!(down.lighting, dom);
        assert!(!down.anti_aliasing);
        assert!(down.full_display_resolution);
        let up: RenderingPreferences = (&down).into();
        assert_eq!(up, prefs);
    }
}

/// The depth cue (`doc/design_depth_perception.md` D8, Phase 3) converts both
/// ways, its two fields not swapped with a neighbour.
#[test]
fn the_depth_cue_converts_both_ways() {
    let prefs = RenderingPreferences {
        anti_aliasing: true,
        full_display_resolution: true,
        lighting: LightingPreset::Studio,
        depth_cue: false,
        depth_cue_strength: 0.75,
        ambient_occlusion: true,
        ambient_occlusion_strength: 1.0,
        shadows: true,
        shadow_strength: 0.7,
    };
    let down: domain::RenderingPreferences = (&prefs).into();
    assert!(!down.depth_cue);
    assert_eq!(down.depth_cue_strength, 0.75);
    let up: RenderingPreferences = (&down).into();
    assert_eq!(up, prefs);

    // A file from before Phase 3 loads with the cue on at half strength.
    let old: RenderingPreferences = serde_json::from_str(r#"{"anti_aliasing": true}"#).unwrap();
    assert!(old.depth_cue);
    assert_eq!(old.depth_cue_strength, 0.5);
}

/// The renderer sees an unticked *Depth cue* as strength 0 and a ticked one
/// as its strength clamped to `[0, 1]`, without touching the AO strength.
#[test]
fn the_depth_cue_reaches_the_renderer_as_a_strength() {
    use rust_lib_flutter_cad::api::api_common::to_shading_settings;
    let mut prefs = domain::RenderingPreferences::default();
    assert_eq!(to_shading_settings(&prefs).depth_cue_strength, 0.5);
    prefs.depth_cue_strength = 1.7;
    assert_eq!(to_shading_settings(&prefs).depth_cue_strength, 1.0);
    prefs.depth_cue_strength = 0.3;
    prefs.depth_cue = false;
    let settings = to_shading_settings(&prefs);
    assert_eq!(settings.depth_cue_strength, 0.0);
    assert_eq!(settings.ao_strength, 1.0);
}

/// Ambient occlusion (`doc/design_depth_perception.md` D8, Phase 4b)
/// converts both ways, its two fields not swapped with a neighbour, and a
/// file from before it loads with AO on at strength 1.
#[test]
fn ambient_occlusion_converts_both_ways() {
    let prefs = RenderingPreferences {
        anti_aliasing: true,
        full_display_resolution: true,
        lighting: LightingPreset::Studio,
        depth_cue: true,
        depth_cue_strength: 0.5,
        ambient_occlusion: false,
        ambient_occlusion_strength: 0.25,
        shadows: true,
        shadow_strength: 0.7,
    };
    let down: domain::RenderingPreferences = (&prefs).into();
    assert!(!down.ambient_occlusion);
    assert_eq!(down.ambient_occlusion_strength, 0.25);
    let up: RenderingPreferences = (&down).into();
    assert_eq!(up, prefs);

    let old: RenderingPreferences = serde_json::from_str(r#"{"depth_cue": true}"#).unwrap();
    assert!(old.ambient_occlusion);
    assert_eq!(old.ambient_occlusion_strength, 1.0);
}

/// The checkbox travels two ways (D8): as strength to the renderer, where an
/// unticked box is strength 0, and as the on/off switch of the display
/// layer, where it gates the exposure pass and the tessellator.
#[test]
fn ambient_occlusion_reaches_the_renderer_and_the_display_layer() {
    use rust_lib_flutter_cad::api::api_common::{to_display_preferences, to_shading_settings};
    let mut prefs = domain::StructureDesignerPreferences::default();
    prefs.rendering_preferences.ambient_occlusion = true;
    prefs.rendering_preferences.ambient_occlusion_strength = 0.4;
    assert_eq!(
        to_shading_settings(&prefs.rendering_preferences).ao_strength,
        0.4
    );
    assert!(
        to_display_preferences(&prefs)
            .atomic_structure_visualization
            .ambient_occlusion
    );
    prefs.rendering_preferences.ambient_occlusion_strength = 3.0;
    assert_eq!(
        to_shading_settings(&prefs.rendering_preferences).ao_strength,
        1.0
    );
    prefs.rendering_preferences.ambient_occlusion = false;
    assert_eq!(
        to_shading_settings(&prefs.rendering_preferences).ao_strength,
        0.0
    );
    assert!(
        !to_display_preferences(&prefs)
            .atomic_structure_visualization
            .ambient_occlusion
    );
}

/// Shadows (`doc/design_shadows.md` D11, Phase 4) convert both ways, their
/// two fields not swapped with a neighbour, and a file from before them loads
/// with shadows on at 0.7.
#[test]
fn shadows_convert_both_ways() {
    let prefs = RenderingPreferences {
        anti_aliasing: true,
        full_display_resolution: true,
        lighting: LightingPreset::Studio,
        depth_cue: true,
        depth_cue_strength: 0.5,
        ambient_occlusion: true,
        ambient_occlusion_strength: 1.0,
        shadows: false,
        shadow_strength: 0.35,
    };
    let down: domain::RenderingPreferences = (&prefs).into();
    assert!(!down.shadows);
    assert_eq!(down.shadow_strength, 0.35);
    let up: RenderingPreferences = (&down).into();
    assert_eq!(up, prefs);

    let old: RenderingPreferences =
        serde_json::from_str(r#"{"ambient_occlusion": false}"#).unwrap();
    assert!(old.shadows);
    assert_eq!(old.shadow_strength, 0.7);
}

/// The renderer sees an unticked *Shadows* as strength 0 and a ticked one as
/// its strength clamped to `[0, 1]`, without touching the other strengths.
/// The preset is passed through: the renderer, not this conversion, draws no
/// shadow under *Headlight*.
#[test]
fn shadows_reach_the_renderer_as_a_strength() {
    use rust_lib_flutter_cad::api::api_common::to_shading_settings;
    let mut prefs = domain::RenderingPreferences::default();
    assert_eq!(to_shading_settings(&prefs).shadow_strength, 0.7);
    prefs.shadow_strength = 1.4;
    assert_eq!(to_shading_settings(&prefs).shadow_strength, 1.0);
    prefs.shadow_strength = -0.5;
    assert_eq!(to_shading_settings(&prefs).shadow_strength, 0.0);
    prefs.shadow_strength = 0.4;
    prefs.shadows = false;
    let settings = to_shading_settings(&prefs);
    assert_eq!(settings.shadow_strength, 0.0);
    assert_eq!(settings.ao_strength, 1.0);
    assert_eq!(settings.depth_cue_strength, 0.5);
}
