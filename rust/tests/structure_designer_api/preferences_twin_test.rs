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
use rust_lib_flutter_cad::api::structure_designer::structure_designer_preferences::{
    NodeDisplayPolicy, NodeDisplayPreferences, NodeTitleMode,
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
