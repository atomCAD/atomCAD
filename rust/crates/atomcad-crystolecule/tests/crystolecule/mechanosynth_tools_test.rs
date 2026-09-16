//! Phase 1 tests for the tool model: the `/2` library format, the tool pose,
//! binding, the scene replay with feedstocks and tools, tool-aware
//! applicability, the partial-result form and the event rules. See
//! `doc/design_mechanosynth_tools.md` §Phases, Phase 1.
//!
//! The molecules are `.xyz` fixtures (`.xyz` carries no tags, so every test
//! tags the four frame atoms after loading, which is also what a design does by
//! hand). Everything else is built in code.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::{
    APEX_FRAME_TAG, BuildScript, DEFAULT_DURATION, EXACT_FIT_RESIDUAL, HighlightTags,
    MechanosynthError, Method, OpLibrary, Participant, Step, ToolBinding, applicable_ops,
    event_indices, load_build_script, load_library, parse_build_script, parse_library, replay,
    replay_scene, replay_scene_partial, resolve_tolerance, tool_pose,
};
use atomcad_test_support::fixture_path;
use glam::{DMat3, DVec3};

const H: i16 = 1;
const C: i16 = 6;

/// The frame tags every tool type in `tool_ops.json` uses. Shared on purpose:
/// a user tags every tool the same way and only the type tag differs.
const FRAME_TAGS: [&str; 4] = [APEX_FRAME_TAG, "a", "b", "c"];

/// The six-atom tool skeleton's atom order in every tool fixture: apex, the
/// ethynyl carbon, the handle carbon, then the three legs.
const APEX_INDEX: usize = 0;
const LEG_INDICES: [usize; 3] = [3, 4, 5];

// ============================================================================
// Helpers
// ============================================================================

fn library(name: &str) -> OpLibrary {
    load_library(&fixture_path(&format!("mechanosynth/{name}")))
        .unwrap_or_else(|e| panic!("fixture {name} should parse: {e}"))
}

fn script(name: &str) -> BuildScript {
    load_build_script(&fixture_path(&format!("mechanosynth/{name}")))
        .unwrap_or_else(|e| panic!("fixture {name} should parse: {e}"))
}

fn molecule(name: &str) -> AtomicStructure {
    load_xyz(
        fixture_path(&format!("mechanosynth/{name}"))
            .to_string_lossy()
            .as_ref(),
        true,
    )
    .unwrap_or_else(|e| panic!("fixture {name} should load: {e}"))
}

/// Atom ids in file order. `load_xyz` assigns them sequentially, but the tests
/// say what they mean rather than assuming it.
fn ids(structure: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

/// What a design does by hand: the type name on the whole molecule, then
/// `apex` and the three leg tags on the four frame atoms.
fn tag_tool(structure: &mut AtomicStructure, tool_type: &str) {
    let ids = ids(structure);
    for id in &ids {
        structure
            .add_atom_tag(*id, tool_type)
            .expect("the type tag fits");
    }
    structure
        .add_atom_tag(ids[APEX_INDEX], FRAME_TAGS[0])
        .expect("apex tag fits");
    for (slot, index) in LEG_INDICES.iter().enumerate() {
        structure
            .add_atom_tag(ids[*index], FRAME_TAGS[slot + 1])
            .expect("leg tag fits");
    }
}

fn tagged(name: &str, tool_type: &str) -> AtomicStructure {
    let mut structure = molecule(name);
    tag_tool(&mut structure, tool_type);
    structure
}

fn tip() -> AtomicStructure {
    tagged("tool_tip.xyz", "habst_tool")
}

fn probe() -> AtomicStructure {
    tagged("tool_probe.xyz", "probe")
}

/// The scene replay with no highlights, which is what most of these tests want.
fn replay_plain(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
) -> Result<atomcad_crystolecule::mechanosynth::Scene, MechanosynthError> {
    replay_scene(
        base,
        feedstocks,
        tools,
        library,
        script,
        step,
        HighlightTags::default(),
    )
}

fn error_of(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
) -> MechanosynthError {
    replay_plain(base, feedstocks, tools, library, script, step)
        .err()
        .expect("this replay should fail")
}

fn library_error(text: &str) -> String {
    parse_library(text, "ops.json")
        .err()
        .expect("should be rejected")
        .to_string()
}

/// A `/4` library with one `tools` entry, wrapping whatever ops text is given.
fn with_probe_tools(ops: &str) -> String {
    format!(
        r#"{{ "format": "atomcad-msops/4",
              "tools": [ {{ "name": "probe", "frame": [
                 {{ "tag": "apex", "pos": [0,0,0] }},
                 {{ "tag": "a", "pos": [1.45, 0, -3.17] }},
                 {{ "tag": "b", "pos": [-0.725, 1.2557, -3.17] }},
                 {{ "tag": "c", "pos": [-0.725, -1.2557, -3.17] }} ],
                 "envelope": {{ "half_angle": 30.0, "radius": 2.0 }} }} ],
              "ops": [ {ops} ] }}"#
    )
}

/// The `reaction` block every `tip` operation has to carry since `/4`, for the
/// inline libraries whose subject is something else entirely.
const REACTION: &str = r#""reaction": { "target": [0,0,0], "tool": [0,0,1.5] }"#;

fn count_of_element(structure: &AtomicStructure, element: i16) -> usize {
    structure
        .iter_atoms()
        .filter(|(_, atom)| atom.atomic_number == element)
        .count()
}

// ============================================================================
// Parsing: the `/2` library
// ============================================================================

#[test]
fn a_two_library_with_tools_and_tool_sides_loads() {
    let lib = library("tool_ops.json");
    assert_eq!(lib.tools.len(), 2);

    let habst_tool = lib.tool_type("habst_tool").expect("declared");
    assert_eq!(habst_tool.frame.len(), 4);
    assert_eq!(habst_tool.initial_state(), Some("charged"));
    assert_eq!(
        habst_tool.frame_atom(APEX_FRAME_TAG).map(|a| a.pos),
        Some(DVec3::ZERO)
    );

    // A type without `states` carries no symbolic state at all.
    let probe = lib.tool_type("probe").expect("declared");
    assert_eq!(probe.initial_state(), None);

    let habst = lib.get("habst").expect("declared");
    assert_eq!(habst.method, Method::Tip);
    let tool = habst
        .tool
        .as_ref()
        .expect("a tip operation has a tool side");
    assert_eq!(tool.tool_type, "habst_tool");
    assert_eq!(tool.from.as_deref(), Some("charged"));
    assert_eq!(tool.to.as_deref(), Some("spent"));
    assert_eq!(tool.before.atoms.len(), 3);
    assert_eq!(tool.after.atoms.len(), 4);
}

#[test]
fn an_older_library_is_refused_naming_the_fix() {
    // `/2` as much as `/1`. The bond semantics of `/3` change the *meaning* of
    // a pattern that lists no bonds — from "nothing is said" to "these atoms
    // are not bonded" — so accepting a `/2` file would read an assertion into
    // it that its author never made.
    for older in ["atomcad-msops/1", "atomcad-msops/2", "atomcad-msops/3"] {
        let message = library_error(&format!(r#"{{ "format": "{older}", "ops": [] }}"#));
        assert!(message.contains("atomcad-msops/4"), "{message}");
        assert!(message.contains(older), "{message}");
    }
}

#[test]
fn method_parses_to_its_kind_and_anything_else_names_the_operation() {
    let lib = library("tool_ops.json");
    assert_eq!(lib.get("habst").unwrap().method, Method::Tip);
    assert_eq!(lib.get("expose").unwrap().method, Method::Bulk);
    assert_eq!(lib.get("settle").unwrap().method, Method::Spontaneous);
    assert_eq!(Method::Tip.as_str(), "tip");
    assert_eq!(Method::Bulk.as_str(), "bulk");
    assert_eq!(Method::Spontaneous.as_str(), "spontaneous");

    for ops in [
        r#"{ "name": "x", "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
        r#"{ "name": "x", "method": "lithography",
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
        r#"{ "name": "x", "method": 7,
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ] {
        let message = library_error(&with_probe_tools(ops));
        assert!(message.contains("operation 'x'"), "{message}");
        assert!(message.contains("method"), "{message}");
    }
}

#[test]
fn agent_is_required_on_bulk_and_forbidden_elsewhere() {
    let missing = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "bulk",
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(missing.contains("operation 'x'"), "{missing}");
    assert!(missing.contains("agent"), "{missing}");

    let misplaced = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "spontaneous", "agent": "UV",
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(misplaced.contains("agent"), "{misplaced}");
    assert!(misplaced.contains("bulk"), "{misplaced}");

    assert_eq!(
        library("tool_ops.json")
            .get("expose")
            .unwrap()
            .agent
            .as_deref(),
        Some("X2")
    );
}

#[test]
fn a_tool_side_belongs_to_a_tip_operation_and_to_no_other() {
    let missing = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "tip",
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(missing.contains("operation 'x'"), "{missing}");
    assert!(missing.contains("tool"), "{missing}");

    for method in ["bulk", "spontaneous"] {
        let agent = if method == "bulk" {
            r#", "agent": "UV""#
        } else {
            ""
        };
        let message = library_error(&with_probe_tools(&format!(
            r#"{{ "name": "x", "method": "{method}"{agent}, "tool": {{ "type": "probe" }},
                  "before": {{ "atoms": [] }}, "after": {{ "atoms": [] }} }}"#
        )));
        assert!(message.contains("operation 'x'"), "{message}");
        assert!(message.contains("tip"), "{message}");
    }
}

#[test]
fn a_tool_side_with_empty_patterns_loads() {
    let lib = library("tool_ops.json");
    let side = lib
        .get("habst_probe")
        .unwrap()
        .tool
        .as_ref()
        .expect("a tip operation has a tool side");
    assert_eq!(side.tool_type, "probe");
    assert!(side.before.atoms.is_empty());
    assert!(side.after.atoms.is_empty());
    assert_eq!(side.from, None);
    assert_eq!(side.to, None);
}

#[test]
fn the_tool_side_is_checked_against_the_declared_type() {
    let unknown = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "tip", "tool": { "type": "no_such_tool" },
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(unknown.contains("no_such_tool"), "{unknown}");
    assert!(unknown.contains("operation 'x'"), "{unknown}");

    // `from`/`to` must name a state the type declares — and `probe` declares
    // none at all, so any `from` is outside its vocabulary.
    let bad_state = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "tip", "tool": { "type": "probe", "from": "charged" },
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(bad_state.contains("charged"), "{bad_state}");
    assert!(bad_state.contains("probe"), "{bad_state}");
}

#[test]
fn a_wildcard_on_an_added_tool_side_atom_is_rejected() {
    let message = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "tip",
             "tool": { "type": "probe",
                       "before": { "atoms": [] },
                       "after": { "atoms": [ { "id": 1, "el": "*", "pos": [0,0,0] } ] } },
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(message.contains("operation 'x'"), "{message}");
    assert!(message.contains("atom id 1"), "{message}");
    assert!(message.contains("\"el\""), "{message}");
}

#[test]
fn a_duplicate_tool_type_name_is_rejected() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/4",
             "tools": [ { "name": "probe", "frame": [
                 { "tag": "apex", "pos": [0,0,0] },
                 { "tag": "a", "pos": [1,0,-3] },
                 { "tag": "b", "pos": [0,1,-3] },
                 { "tag": "c", "pos": [-1,-1,-3] } ], "envelope": { "half_angle": 30.0, "radius": 2.0 } },
                 { "name": "probe", "frame": [
                 { "tag": "apex", "pos": [0,0,0] },
                 { "tag": "a", "pos": [1,0,-3] },
                 { "tag": "b", "pos": [0,1,-3] },
                 { "tag": "c", "pos": [-1,-1,-3] } ], "envelope": { "half_angle": 30.0, "radius": 2.0 } } ],
             "ops": [] }"#,
    );
    assert!(message.contains("tool type 'probe'"), "{message}");
    assert!(message.contains("duplicate"), "{message}");
}

#[test]
fn an_empty_states_list_is_a_parse_error() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/4",
             "tools": [ { "name": "probe", "states": [], "frame": [
                 { "tag": "apex", "pos": [0,0,0] },
                 { "tag": "a", "pos": [1,0,-3] },
                 { "tag": "b", "pos": [0,1,-3] },
                 { "tag": "c", "pos": [-1,-1,-3] } ], "envelope": { "half_angle": 30.0, "radius": 2.0 } } ],
             "ops": [] }"#,
    );
    assert!(message.contains("tool type 'probe'"), "{message}");
    assert!(message.contains("states"), "{message}");
}

#[test]
fn every_frame_rule_is_a_parse_error_naming_the_type() {
    let frame = |entries: &str| {
        library_error(&format!(
            r#"{{ "format": "atomcad-msops/4",
                  "tools": [ {{ "name": "probe", "frame": [ {entries} ], "envelope": {{ "half_angle": 30.0, "radius": 2.0 }} }} ],
                  "ops": [] }}"#
        ))
    };

    // Three entries: a triangle is congruent to its mirror image.
    let too_few = frame(
        r#"{ "tag": "apex", "pos": [0,0,0] },
           { "tag": "a", "pos": [1,0,-3] },
           { "tag": "b", "pos": [0,1,-3] }"#,
    );
    assert!(too_few.contains("tool type 'probe'"), "{too_few}");
    assert!(too_few.contains("4"), "{too_few}");

    let no_apex = frame(
        r#"{ "tag": "z", "pos": [0,0,0] },
           { "tag": "a", "pos": [1,0,-3] },
           { "tag": "b", "pos": [0,1,-3] },
           { "tag": "c", "pos": [-1,-1,-3] }"#,
    );
    assert!(no_apex.contains("apex"), "{no_apex}");

    let displaced_apex = frame(
        r#"{ "tag": "apex", "pos": [0,0,1.5] },
           { "tag": "a", "pos": [1,0,-3] },
           { "tag": "b", "pos": [0,1,-3] },
           { "tag": "c", "pos": [-1,-1,-3] }"#,
    );
    assert!(displaced_apex.contains("origin"), "{displaced_apex}");

    // Coplanar: the legs taken off the linear business axis rather than off the
    // handle, which is the mistake the rule exists to catch. They still have to
    // sit on one side of the apex's plane, or the axis rule catches them first.
    let coplanar = frame(
        r#"{ "tag": "apex", "pos": [0,0,0] },
           { "tag": "a", "pos": [0,1,-3] },
           { "tag": "b", "pos": [0,2,-3] },
           { "tag": "c", "pos": [0,-1,-1] }"#,
    );
    assert!(coplanar.contains("coplanar"), "{coplanar}");

    let repeated = frame(
        r#"{ "tag": "apex", "pos": [0,0,0] },
           { "tag": "a", "pos": [1,0,-3] },
           { "tag": "a", "pos": [0,1,-3] },
           { "tag": "c", "pos": [-1,-1,-3] }"#,
    );
    assert!(repeated.contains("duplicate frame tag"), "{repeated}");
}

#[test]
fn a_type_name_that_is_also_a_frame_tag_is_rejected() {
    let message = library_error(
        r#"{ "format": "atomcad-msops/4",
             "tools": [ { "name": "apex", "frame": [
                 { "tag": "apex", "pos": [0,0,0] },
                 { "tag": "a", "pos": [1,0,-3] },
                 { "tag": "b", "pos": [0,1,-3] },
                 { "tag": "c", "pos": [-1,-1,-3] } ], "envelope": { "half_angle": 30.0, "radius": 2.0 } } ],
             "ops": [] }"#,
    );
    assert!(message.contains("tool type 'apex'"), "{message}");
    assert!(message.contains("frame tag"), "{message}");
}

#[test]
fn the_retired_approach_key_is_skipped_like_any_other_unknown_one() {
    // `/3` reserved `approach` and nobody ever wrote it; `/4` removed it and the
    // sweep computes what it was for. The fixture still carries one, so this
    // pins that a file keeping it loses nothing.
    let lib = library("tool_ops.json");
    assert!(lib.get("habst_approach").is_some());
}

#[test]
fn duration_reads_its_default_and_refuses_a_non_positive_one() {
    let lib = library("tool_ops.json");
    assert_eq!(lib.get("habst_approach").unwrap().duration, 2.5);
    // Read by nothing in this milestone; the clock half of milestone 2 is what
    // it is reserved for.
    assert_eq!(lib.get("habst").unwrap().duration, DEFAULT_DURATION);

    for bad in ["0", "-1.0"] {
        let message = library_error(&with_probe_tools(&format!(
            r#"{{ "name": "x", "method": "tip", {REACTION}, "duration": {bad},
                  "tool": {{ "type": "probe" }},
                  "before": {{ "atoms": [] }}, "after": {{ "atoms": [] }} }}"#
        )));
        assert!(message.contains("operation 'x'"), "{message}");
        assert!(message.contains("duration"), "{message}");
    }
}

#[test]
fn an_envelope_is_required_on_every_tool_type_and_range_checked() {
    let frame = r#""frame": [
        { "tag": "apex", "pos": [0,0,0] },
        { "tag": "a", "pos": [1.45, 0, -3.17] },
        { "tag": "b", "pos": [-0.725, 1.2557, -3.17] },
        { "tag": "c", "pos": [-0.725, -1.2557, -3.17] } ]"#;
    let with = |envelope: &str| {
        format!(
            r#"{{ "format": "atomcad-msops/4",
                  "tools": [ {{ "name": "probe", {frame}{envelope} }} ], "ops": [] }}"#
        )
    };

    let message = library_error(&with(""));
    assert!(message.contains("tool type 'probe'"), "{message}");
    assert!(message.contains("envelope"), "{message}");

    for bad in [
        r#", "envelope": { "radius": 2.0 }"#,
        r#", "envelope": { "half_angle": 0.0, "radius": 2.0 }"#,
        r#", "envelope": { "half_angle": 90.0, "radius": 2.0 }"#,
        r#", "envelope": { "half_angle": 30.0 }"#,
        r#", "envelope": { "half_angle": 30.0, "radius": 0.0 }"#,
        r#", "envelope": { "half_angle": 30.0, "radius": -1.0 }"#,
    ] {
        let message = library_error(&with(bad));
        assert!(message.contains("tool type 'probe'"), "{message}");
        assert!(message.contains("envelope"), "{message}");
    }

    // Degrees in the file, radians in the engine.
    let lib = library("tool_ops.json");
    let envelope = lib.tool_type("probe").expect("declared").envelope;
    assert!((envelope.half_angle - 31.0_f64.to_radians()).abs() < 1e-12);
    assert_eq!(envelope.radius, 2.3);
}

#[test]
fn every_leg_of_a_frame_is_on_one_side_of_the_apex_plane() {
    // The sign is the tool axis, so a frame that straddles the plane — or
    // touches it — says nothing about which way the tool points.
    let with = |legs: &str| {
        format!(
            r#"{{ "format": "atomcad-msops/4",
                  "tools": [ {{ "name": "probe", "frame": [
                     {{ "tag": "apex", "pos": [0,0,0] }}, {legs} ],
                     "envelope": {{ "half_angle": 30.0, "radius": 2.0 }} }} ],
                  "ops": [] }}"#
        )
    };

    let straddling = library_error(&with(
        r#"{ "tag": "a", "pos": [1.45, 0, -3.17] },
           { "tag": "b", "pos": [-0.725, 1.2557, 3.17] },
           { "tag": "c", "pos": [-0.725, -1.2557, -3.17] }"#,
    ));
    assert!(straddling.contains("tool type 'probe'"), "{straddling}");
    assert!(straddling.contains("both sides"), "{straddling}");

    let on_the_plane = library_error(&with(
        r#"{ "tag": "a", "pos": [1.45, 0, -3.17] },
           { "tag": "b", "pos": [-0.725, 1.2557, 0.0] },
           { "tag": "c", "pos": [-0.725, -1.2557, -3.17] }"#,
    ));
    assert!(on_the_plane.contains("\"b\""), "{on_the_plane}");
    assert!(on_the_plane.contains("z = 0"), "{on_the_plane}");

    // Neither sign is privileged: the fixture tools keep their legs below the
    // apex and read a `-z` axis, and the silicon library's legs sit above.
    let lib = library("tool_ops.json");
    assert_eq!(lib.tool_type("probe").unwrap().axis(), -DVec3::Z);
}

#[test]
fn reaction_is_required_on_tip_and_forbidden_elsewhere() {
    let missing = library_error(&with_probe_tools(
        r#"{ "name": "x", "method": "tip", "tool": { "type": "probe" },
             "before": { "atoms": [] }, "after": { "atoms": [] } }"#,
    ));
    assert!(missing.contains("operation 'x'"), "{missing}");
    assert!(missing.contains("reaction"), "{missing}");

    let present = library_error(&with_probe_tools(&format!(
        r#"{{ "name": "x", "method": "spontaneous", {REACTION},
              "before": {{ "atoms": [] }}, "after": {{ "atoms": [] }} }}"#
    )));
    assert!(present.contains("operation 'x'"), "{present}");
    assert!(present.contains("reaction"), "{present}");

    for bad in [
        r#""reaction": { "tool": [0,0,1.5] }"#,
        r#""reaction": { "target": [0,0,0] }"#,
        r#""reaction": { "target": [0,0], "tool": [0,0,1.5] }"#,
    ] {
        let message = library_error(&with_probe_tools(&format!(
            r#"{{ "name": "x", "method": "tip", {bad}, "tool": {{ "type": "probe" }},
                  "before": {{ "atoms": [] }}, "after": {{ "atoms": [] }} }}"#
        )));
        assert!(message.contains("operation 'x'"), "{message}");
        assert!(message.contains("reaction"), "{message}");
    }

    let lib = library("tool_ops.json");
    let reaction = lib
        .get("habst")
        .unwrap()
        .reaction
        .expect("a tip states one");
    assert_eq!(reaction.target, DVec3::ZERO);
    assert_eq!(reaction.tool, DVec3::new(0.0, 0.0, 1.06));
    assert!(lib.get("settle").unwrap().reaction.is_none());
}

// ============================================================================
// Parsing: the `/2` build script
// ============================================================================

#[test]
fn a_build_step_with_a_method_key_parses_with_the_key_skipped() {
    let parsed = parse_build_script(
        r#"{ "format": "atomcad-msbuild/2", "steps": [
             { "op": "habst", "t": [0,0,0], "method": "probe", "phase": "p" } ] }"#,
        "build.json",
    )
    .expect("a method key is now just an unknown key");
    assert_eq!(parsed.steps.len(), 1);
    assert_eq!(parsed.steps[0].phase, "p");
}

#[test]
fn build_step_has_exactly_seven_fields() {
    // The field list is the thing under test, so it is written out. A new field
    // on `Step` fails this destructuring, which is the point.
    let Step {
        op: _,
        t: _,
        r: _,
        note: _,
        phase: _,
        layer: _,
        site: _,
    } = Step::new("habst", DVec3::ZERO);

    // And the fixture uses no other key.
    let text = std::fs::read_to_string(fixture_path("mechanosynth/tool_build.json"))
        .expect("the fixture is readable");
    let document: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    for step in document["steps"].as_array().expect("a steps array") {
        for key in step.as_object().expect("a step object").keys() {
            assert!(
                matches!(
                    key.as_str(),
                    "op" | "t" | "r" | "note" | "phase" | "layer" | "site"
                ),
                "tool_build.json step carries an eighth key {key:?}"
            );
        }
    }
}

// ============================================================================
// The pose
// ============================================================================

#[test]
fn the_frame_fitted_onto_the_tagged_atoms_recovers_the_pose() {
    let lib = library("tool_ops.json");
    let tool_type = lib.tool_type("habst_tool").unwrap();
    let structure = tip();
    let ids = ids(&structure);

    let correspondences: Vec<(DVec3, DVec3)> = tool_type
        .frame
        .iter()
        .map(|entry| {
            let index = match entry.tag.as_str() {
                APEX_FRAME_TAG => APEX_INDEX,
                "a" => LEG_INDICES[0],
                "b" => LEG_INDICES[1],
                _ => LEG_INDICES[2],
            };
            (entry.pos, structure.get_atom(ids[index]).unwrap().position)
        })
        .collect();

    let pose = tool_pose(&correspondences, resolve_tolerance(&lib)).expect("the fit exists");
    assert!(
        pose.residual < EXACT_FIT_RESIDUAL,
        "residual {} should be exact",
        pose.residual
    );
    // The fixture was placed by a 30° turn about z after a 90° turn about x,
    // translated to (0, 30, 0), and the fit has to say exactly that.
    assert!((pose.t - DVec3::new(0.0, 30.0, 0.0)).length() < 1e-5);
    assert!((pose.r * DVec3::Z - DVec3::new(0.0, 0.0, 1.0).cross(DVec3::ZERO)).length() > 0.0);
    for (local, design) in &correspondences {
        assert!((pose.r * *local + pose.t - *design).length() < 1e-5);
    }
}

#[test]
fn a_degenerate_correspondence_set_is_an_error_and_never_a_rotation() {
    // Fewer than three.
    let two = tool_pose(&[(DVec3::ZERO, DVec3::ZERO), (DVec3::X, DVec3::Y)], 0.05);
    assert!(two.is_err(), "two correspondences determine no orientation");

    // Collinear.
    let collinear = tool_pose(
        &[
            (DVec3::ZERO, DVec3::ZERO),
            (DVec3::X, DVec3::X),
            (DVec3::X * 2.0, DVec3::X * 2.0),
            (DVec3::X * 3.0, DVec3::X * 3.0),
        ],
        0.05,
    );
    let message = collinear
        .err()
        .expect("collinear points determine no orientation")
        .to_string();
    assert!(message.contains("collinear"), "{message}");
}

// ============================================================================
// Binding
// ============================================================================

#[test]
fn the_tip_and_the_probe_bind_to_their_types_in_either_wiring_order() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    for order in [0, 1] {
        let tools = if order == 0 {
            vec![tip(), probe()]
        } else {
            vec![probe(), tip()]
        };
        let scene = replay_plain(&base, &[], &tools, &lib, &empty, 0).expect("both bind");
        assert_eq!(scene.bindings.len(), 2);
        let types: Vec<&str> = scene
            .bindings
            .iter()
            .map(|b| b.tool_type.as_str())
            .collect();
        if order == 0 {
            assert_eq!(types, vec!["habst_tool", "probe"]);
        } else {
            assert_eq!(types, vec!["probe", "habst_tool"]);
        }
        // Every bound tool starts in its type's initial state.
        assert_eq!(
            scene.binding_of("habst_tool").unwrap().state.as_deref(),
            Some("charged")
        );
        assert_eq!(scene.binding_of("probe").unwrap().state, None);
    }
}

#[test]
fn the_type_tag_may_sit_on_the_frame_atoms_alone() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    let mut tool = molecule("tool_tip.xyz");
    let ids = ids(&tool);
    for index in [APEX_INDEX, LEG_INDICES[0], LEG_INDICES[1], LEG_INDICES[2]] {
        tool.add_atom_tag(ids[index], "habst_tool").unwrap();
    }
    tool.add_atom_tag(ids[APEX_INDEX], FRAME_TAGS[0]).unwrap();
    for (slot, index) in LEG_INDICES.iter().enumerate() {
        tool.add_atom_tag(ids[*index], FRAME_TAGS[slot + 1])
            .unwrap();
    }

    let scene = replay_plain(&base, &[], &[tool], &lib, &empty, 0).expect("binds");
    assert_eq!(scene.bindings[0].tool_type, "habst_tool");
}

#[test]
fn a_tip_of_hundreds_of_atoms_binds_to_the_same_pose() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    let bare = replay_plain(&base, &[], &[tip()], &lib, &empty, 0).expect("binds");
    let on_handle = tagged("tool_tip_on_handle.xyz", "habst_tool");
    assert!(on_handle.iter_atoms().count() > 200);
    let big = replay_plain(&base, &[], &[on_handle], &lib, &empty, 0).expect("binds");

    assert!((bare.bindings[0].pose.t - big.bindings[0].pose.t).length() < 1e-9);
    for column in 0..3 {
        assert!(
            (bare.bindings[0].pose.r.col(column) - big.bindings[0].pose.r.col(column)).length()
                < 1e-9
        );
    }
}

#[test]
fn every_binding_deviation_is_an_error_naming_the_molecule() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };
    let fail = |tools: Vec<AtomicStructure>| error_of(&base, &[], &tools, &lib, &empty, 0);

    // No type tag at all.
    let message = fail(vec![molecule("tool_tip.xyz")]).to_string();
    assert!(message.contains("tool 0"), "{message}");
    assert!(message.contains("habst_tool"), "{message}");
    assert!(message.contains("probe"), "{message}");

    // Both type tags.
    let mut both = tip();
    for id in ids(&both) {
        both.add_atom_tag(id, "probe").unwrap();
    }
    let message = fail(vec![both]).to_string();
    assert!(message.contains("two tool type tags"), "{message}");

    // Two molecules of one type.
    let message = fail(vec![tip(), tip()]).to_string();
    assert!(message.contains("tool 0"), "{message}");
    assert!(message.contains("tool 1"), "{message}");
    assert!(message.contains("habst_tool"), "{message}");

    // A frame tag on no atom.
    let mut missing_leg = tip();
    let leg = ids(&missing_leg)[LEG_INDICES[2]];
    missing_leg.remove_atom_tag(leg, "c");
    let message = fail(vec![missing_leg]).to_string();
    assert!(message.contains("'c'"), "{message}");
    assert!(message.contains("found 0"), "{message}");

    // A frame tag on two atoms.
    let mut doubled = tip();
    let other = ids(&doubled)[1];
    doubled.add_atom_tag(other, "c").unwrap();
    let message = fail(vec![doubled]).to_string();
    assert!(message.contains("found 2"), "{message}");

    // A frame atom displaced beyond tolerance.
    let mut bent = tip();
    let leg = ids(&bent)[LEG_INDICES[0]];
    let position = bent.get_atom(leg).unwrap().position;
    bent.set_atom_position(leg, position + DVec3::new(0.4, 0.0, 0.0));
    let message = fail(vec![bent]).to_string();
    assert!(message.contains("habst_tool"), "{message}");
    assert!(message.contains("residual"), "{message}");
}

#[test]
fn a_mirrored_molecule_fails_the_residual_rather_than_binding_mirrored() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    let mut mirrored = tip();
    for id in ids(&mirrored) {
        let p = mirrored.get_atom(id).unwrap().position;
        mirrored.set_atom_position(id, DVec3::new(p.x, p.y, -p.z));
    }
    let message = error_of(&base, &[], &[mirrored], &lib, &empty, 0).to_string();
    assert!(message.contains("habst_tool"), "{message}");
    assert!(
        message.contains("mirror") || message.contains("residual"),
        "{message}"
    );
}

#[test]
fn binding_errors_are_reported_before_any_step_is_applied() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let build = script("tool_build.json");

    // Even at step 0, where no step would run at all.
    let message = error_of(&base, &[], &[molecule("tool_tip.xyz")], &lib, &build, 0).to_string();
    assert!(message.contains("tool 0"), "{message}");
    assert!(!message.contains("step 1"), "{message}");
}

#[test]
fn a_type_with_no_molecule_is_fine_until_a_step_needs_it() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");

    // The probe alone: binding succeeds, and so does everything up to step 4,
    // which is the first one that asks for `habst_tool`.
    let scene = replay_plain(&base, &[dump.clone()], &[probe()], &lib, &build, 0);
    assert!(scene.is_ok(), "binding the probe alone is fine");

    let message = error_of(&base, &[dump], &[probe()], &lib, &build, 1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("habst"), "{message}");
    assert!(message.contains("habst_tool"), "{message}");
}

#[test]
fn with_tools_unwired_nothing_is_bound_and_no_tool_error_can_occur() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");

    let scene = replay_plain(&base, &[dump], &[], &lib, &build, -1)
        .expect("the whole build replays with no tools at all");
    assert!(scene.bindings.is_empty());
}

#[test]
fn the_tool_tags_reach_the_scene_and_never_the_workpiece() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    let scene = replay_plain(&base, &[], &[tip()], &lib, &empty, 0).expect("binds");
    assert!(!scene.structure.atoms_with_tag("habst_tool").is_empty());
    assert!(!scene.structure.atoms_with_tag(APEX_FRAME_TAG).is_empty());

    let workpiece = scene.workpiece();
    assert!(workpiece.atoms_with_tag("habst_tool").is_empty());
    assert!(workpiece.atoms_with_tag(APEX_FRAME_TAG).is_empty());
}

#[test]
fn a_base_whose_tag_table_is_full_is_a_scene_tag_error_before_binding() {
    let lib = library("tool_ops.json");
    let mut base = molecule("tool_scene.xyz");
    let first = ids(&base)[0];
    for i in 0..32 {
        base.add_atom_tag(first, &format!("t{i}"))
            .expect("the table holds exactly 32 names");
    }
    let empty = BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    let message = error_of(&base, &[], &[tip()], &lib, &empty, 0).to_string();
    assert!(message.contains("tool 0"), "{message}");
    assert!(message.contains("32"), "{message}");
}

// ============================================================================
// Replay
// ============================================================================

/// The shared assertion: **the base is unchanged by the tool model.**
#[test]
fn result_split_from_the_scene_equals_a_tool_free_replay() {
    for (ops_name, build_name, base_name) in [
        ("methylate_ops.json", "methylate_build.json", "methane.xyz"),
        ("tool_ops.json", "tool_build.json", "tool_scene.xyz"),
    ] {
        let lib = library(ops_name);
        let build = script(build_name);
        let base = molecule(base_name);
        // `tool_build.json` touches a reservoir, so only its first step — which
        // does not — is comparable; the methylate build is comparable at every
        // step.
        let last = if build_name == "tool_build.json" {
            1
        } else {
            3
        };

        for step in 0..=last {
            let plain = replay(&base, &lib, &build, step, HighlightTags::default());
            let tools: Vec<AtomicStructure> = if ops_name == "tool_ops.json" {
                vec![tip(), probe()]
            } else {
                Vec::new()
            };
            let scene = replay_plain(&base, &[], &tools, &lib, &build, step);
            match (plain, scene) {
                (Ok(plain), Ok(scene)) => {
                    let workpiece = scene.workpiece();
                    assert_eq!(
                        workpiece.iter_atoms().count(),
                        plain.iter_atoms().count(),
                        "{build_name} step {step}"
                    );
                    for (id, atom) in plain.iter_atoms() {
                        let same = workpiece
                            .get_atom(*id)
                            .unwrap_or_else(|| panic!("{build_name} step {step}: atom {id} gone"));
                        assert_eq!(same.atomic_number, atom.atomic_number);
                        assert!((same.position - atom.position).length() < 1e-9);
                    }
                }
                (Err(_), Err(_)) => {}
                (plain, scene) => panic!(
                    "{build_name} step {step}: the two replays disagree ({}, {})",
                    plain.is_ok(),
                    scene.is_ok()
                ),
            }
        }
    }
}

#[test]
fn a_dumped_atom_stays_with_the_reservoir_and_out_of_the_workpiece() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");

    let tags = HighlightTags {
        added: Some("ms_added"),
        tool: Some("ms_tool"),
        feedstock: Some("ms_feedstock"),
        ..HighlightTags::default()
    };
    // After step 2 the recharge has put an H on the reservoir.
    let scene = replay_scene(
        &base,
        &[dump.clone()],
        &[tip(), probe()],
        &lib,
        &build,
        2,
        tags,
    )
    .expect("the recharge replays");

    let dumped: Vec<u32> = scene
        .structure
        .atoms_with_tag("ms_feedstock")
        .into_iter()
        .filter(|id| scene.structure.get_atom(*id).unwrap().atomic_number == H)
        .collect();
    assert_eq!(dumped.len(), 1, "the reservoir received exactly one H");
    let dumped = dumped[0];
    assert_eq!(scene.participant(dumped), Participant::Feedstock(0));
    // It is cargo, not construction: no `ms_added`.
    assert!(!scene.structure.atom_has_tag(dumped, "ms_added"));

    let workpiece = scene.workpiece();
    assert!(workpiece.get_atom(dumped).is_none());
    assert_eq!(
        workpiece.iter_atoms().count(),
        base.iter_atoms().count() - 1,
        "the workpiece lost the abstracted H and gained nothing"
    );
    // And the `ms_added` sets of the two agree.
    assert_eq!(
        scene.structure.atoms_with_tag("ms_added").len(),
        workpiece.atoms_with_tag("ms_added").len()
    );
}

#[test]
fn a_feedstock_is_nothing_but_a_participant_label() {
    // The same build with the cluster merged into the base replays to the same
    // scene, atom for atom.
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");

    let split = replay_plain(&base, &[dump.clone()], &[tip(), probe()], &lib, &build, -1)
        .expect("the whole build replays");

    let mut merged_base = base.clone();
    merged_base
        .add_atomic_structure(&dump)
        .expect("the tags fit");
    let merged = replay_plain(&merged_base, &[], &[tip(), probe()], &lib, &build, -1)
        .expect("the whole build replays");

    assert_eq!(
        split.structure.iter_atoms().count(),
        merged.structure.iter_atoms().count()
    );
    assert_eq!(
        count_of_element(&split.structure, H),
        count_of_element(&merged.structure, H)
    );
    assert_eq!(
        count_of_element(&split.structure, C),
        count_of_element(&merged.structure, C)
    );
}

#[test]
fn the_two_array_pins_are_independent() {
    // The dump wired and `tools` empty replays the whole build, with the
    // recharge as a plain rewrite of the reservoir.
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");

    let scene = replay_plain(&base, &[dump], &[], &lib, &build, -1)
        .expect("the build replays with no tool model at all");
    assert!(scene.bindings.is_empty());
    assert_eq!(
        scene
            .participants
            .values()
            .filter(|p| matches!(p, Participant::Feedstock(0)))
            .count()
            > 0,
        true
    );
}

#[test]
fn a_step_matching_across_two_participants_is_refused_and_changes_nothing() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let touching = molecule("tool_dump_touching.xyz");
    let bridging = BuildScript {
        file: "bridge".to_string(),
        tolerance: None,
        steps: vec![Step::new("bridge", DVec3::ZERO)],
    };

    let touching_count = touching.iter_atoms().count();
    let message = error_of(&base, &[touching.clone()], &[], &lib, &bridging, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("base"), "{message}");
    assert!(message.contains("feedstock 0"), "{message}");

    // The scene is unchanged: the same replay at step 0 has the same atoms.
    let untouched = replay_plain(&base, &[touching], &[], &lib, &bridging, 0).expect("step 0");
    assert_eq!(
        untouched.structure.iter_atoms().count(),
        base.iter_atoms().count() + touching_count
    );
}

#[test]
fn the_tool_state_moves_with_the_steps_and_a_mis_sequenced_build_names_it() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");
    let tools = vec![tip(), probe()];

    // At step 0 the tip is in its type's initial state.
    let start = replay_plain(&base, &[dump.clone()], &tools, &lib, &build, 0).expect("binds");
    assert_eq!(
        start.binding_of("habst_tool").unwrap().state.as_deref(),
        Some("charged")
    );

    // After the abstraction it is spent and carries one more H at its apex.
    let after = replay_plain(&base, &[dump.clone()], &tools, &lib, &build, 1).expect("step 1");
    assert_eq!(
        after.binding_of("habst_tool").unwrap().state.as_deref(),
        Some("spent")
    );
    let pose = after.binding_of("habst_tool").unwrap().pose;
    let apex_cargo = pose.r * DVec3::new(0.0, 0.0, 1.06) + pose.t;
    let found = after.structure.get_atoms_in_radius(&apex_cargo, 1e-4);
    assert_eq!(found.len(), 1, "the abstracted H rides on the apex");
    assert_eq!(after.participant(found[0]), Participant::Tool(0));

    // After the recharge it is charged again and the cluster has one atom more.
    let recharged = replay_plain(&base, &[dump.clone()], &tools, &lib, &build, 2).expect("step 2");
    assert_eq!(
        recharged.binding_of("habst_tool").unwrap().state.as_deref(),
        Some("charged")
    );

    // A recharge first is a state error, because the tip starts charged.
    let dump_first = BuildScript {
        file: "dump_first".to_string(),
        tolerance: None,
        steps: vec![Step::new("hdump", DVec3::new(20.0, 1.54, 0.0))],
    };
    let message = error_of(&base, &[dump.clone()], &tools, &lib, &dump_first, -1).to_string();
    assert!(message.contains("habst_tool"), "{message}");
    assert!(message.contains("charged"), "{message}");
    assert!(message.contains("spent"), "{message}");

    // And two abstractions in a row are the same error the other way round.
    let twice = BuildScript {
        file: "twice".to_string(),
        tolerance: None,
        steps: vec![
            Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
            Step::new("habst", DVec3::new(1.027662, 0.0, -0.363333)),
        ],
    };
    let message = error_of(&base, &[dump], &tools, &lib, &twice, -1).to_string();
    assert!(message.contains("step 2"), "{message}");
    assert!(message.contains("spent"), "{message}");
}

#[test]
fn a_tool_whose_apex_is_wrong_fails_at_the_step_and_not_at_binding() {
    // The frame atoms are on the handle, so binding says nothing about the
    // business end: a molecule whose apex region is not what the library states
    // binds, and the first tool-side match is what catches it, with the
    // nearest-atom message.
    //
    // A *spent* tip wired while the label says `charged` is the same story from
    // the other side: `hdump` names the cargo H in its `before`, so it is the
    // geometric check that reports a tool that has none.
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let mut hollow = tip();
    let ethynyl = ids(&hollow)[1];
    hollow.delete_atom(ethynyl);

    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };

    // Binding succeeds — the four frame atoms are all still there and exact.
    let bound = replay_plain(&base, &[], &[hollow.clone(), probe()], &lib, &one, 0)
        .expect("the frame atoms are untouched, so it binds");
    assert_eq!(
        bound.binding_of("habst_tool").unwrap().state.as_deref(),
        Some("charged"),
        "and the label still says charged, which is the thing that is wrong"
    );

    let message = error_of(&base, &[], &[hollow, probe()], &lib, &one, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("nearest atom"), "{message}");
}

#[test]
fn a_step_whose_target_side_matches_inside_a_tool_is_refused() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let tool = tip();
    let apex = tool.get_atom(ids(&tool)[APEX_INDEX]).unwrap().position;

    // `expose` takes any atom; aimed at the tip's apex it lands on the tool.
    let on_tool = BuildScript {
        file: "on_tool".to_string(),
        tolerance: None,
        steps: vec![Step::new("expose", apex)],
    };
    let message = error_of(&base, &[], &[tool], &lib, &on_tool, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("tool 0 (habst_tool)"), "{message}");
    assert!(message.contains("tool"), "{message}");
}

#[test]
fn a_tool_side_matching_off_the_tool_is_refused() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let touching = tagged("tool_tip_touching.xyz", "habst_tool");

    // Step 1 of the build abstracts the +z hydrogen; the parked tip's tool side
    // then finds the workpiece carbon before its own ethynyl carbon.
    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };
    let message = error_of(&base, &[], &[touching], &lib, &one, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("habst_tool"), "{message}");
    assert!(message.contains("base"), "{message}");
}

#[test]
fn an_empty_tool_side_leaves_its_tool_alone() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let tools = vec![tip(), probe()];
    let probe_only = BuildScript {
        file: "probe_only".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst_probe", DVec3::new(0.0, 0.0, 1.09))],
    };

    let before = replay_plain(&base, &[], &tools, &lib, &probe_only, 0).expect("binds");
    let after = replay_plain(&base, &[], &tools, &lib, &probe_only, -1).expect("replays");

    let probe_atoms = |scene: &atomcad_crystolecule::mechanosynth::Scene| {
        scene
            .participants
            .iter()
            .filter(|(_, p)| **p == Participant::Tool(1))
            .count()
    };
    assert_eq!(probe_atoms(&before), probe_atoms(&after));
    // …and the workpiece did lose its hydrogen, so the step really ran.
    assert_eq!(
        after.workpiece().iter_atoms().count(),
        before.workpiece().iter_atoms().count() - 1
    );
}

#[test]
fn a_bulk_and_a_spontaneous_step_touch_no_tool() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let tools = vec![tip(), probe()];
    let no_tool_steps = BuildScript {
        file: "no_tool".to_string(),
        tolerance: None,
        // Both act on methane B, on sites the steric rule leaves free: exposing
        // the carbon itself would put the F 0.31 Å from the hydrogen already
        // above it, which is a clash and now a refusal.
        steps: vec![
            Step::new("expose", DVec3::new(11.027662, 0.0, -0.363333)),
            Step::new("settle", DVec3::new(10.0, 0.0, 1.09)),
        ],
    };

    let scene = replay_plain(&base, &[], &tools, &lib, &no_tool_steps, -1).expect("replays");
    // Neither step has a tool side, so both tools are still in their initial
    // states and unchanged.
    assert_eq!(
        scene.binding_of("habst_tool").unwrap().state.as_deref(),
        Some("charged")
    );
    let tool_atoms = scene
        .participants
        .values()
        .filter(|p| matches!(p, Participant::Tool(_)))
        .count();
    assert_eq!(tool_atoms, 12, "six atoms per tool, unchanged");
}

#[test]
fn every_atom_of_the_scene_has_a_participant_after_every_step() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let build = script("tool_build.json");
    let tools = vec![tip(), probe()];

    for step in 0..=(build.steps.len() as i32) {
        let scene = replay_plain(&base, &[dump.clone()], &tools, &lib, &build, step)
            .unwrap_or_else(|e| panic!("step {step}: {e}"));
        for (id, _) in scene.structure.iter_atoms() {
            assert!(
                scene.participants.contains_key(id),
                "step {step}: atom {id} has no participant"
            );
        }
    }
}

#[test]
fn the_current_tag_covers_both_sides_of_a_step() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let build = script("tool_build.json");
    let tools = vec![tip(), probe()];

    let tags = HighlightTags {
        current: Some("ms_current"),
        ..HighlightTags::default()
    };
    let scene = replay_scene(&base, &[], &tools, &lib, &build, 1, tags).expect("step 1");
    let current = scene.structure.atoms_with_tag("ms_current");
    assert!(
        current
            .iter()
            .any(|id| matches!(scene.participant(*id), Participant::Tool(0))),
        "the tip lights up with the site it visited"
    );
    assert!(
        current
            .iter()
            .any(|id| scene.participant(*id) == Participant::Base),
        "so does the workpiece atom the abstraction left behind"
    );
}

#[test]
fn a_match_failure_names_the_participant_it_looked_in() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    // No reservoir wired, so the recharge's `*` finds nothing at x = 20 — and
    // the nearest atom is a base atom, which is what the message says.
    let dump_step = BuildScript {
        file: "dump".to_string(),
        tolerance: None,
        steps: vec![
            Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
            Step::new("hdump", DVec3::new(20.0, 1.54, 0.0)),
        ],
    };
    let message = error_of(&base, &[], &[tip(), probe()], &lib, &dump_step, -1).to_string();
    assert!(message.contains("step 2"), "{message}");
    assert!(message.contains("(in base)"), "{message}");
}

#[test]
fn asking_for_one_step_fewer_than_a_failing_step_succeeds() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let twice = BuildScript {
        file: "twice".to_string(),
        tolerance: None,
        steps: vec![
            Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
            Step::new("habst", DVec3::new(1.027662, 0.0, -0.363333)),
        ],
    };
    let tools = vec![tip(), probe()];
    assert!(replay_plain(&base, &[], &tools, &lib, &twice, 1).is_ok());
    assert!(replay_plain(&base, &[], &tools, &lib, &twice, 2).is_err());
}

// ============================================================================
// The partial-result form
// ============================================================================

#[test]
fn the_partial_form_keeps_the_scene_after_the_last_successful_step() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let tools = vec![tip(), probe()];
    let twice = BuildScript {
        file: "twice".to_string(),
        tolerance: None,
        steps: vec![
            Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
            Step::new("habst", DVec3::new(1.027662, 0.0, -0.363333)),
        ],
    };

    let (partial, error) = replay_scene_partial(
        &base,
        &[],
        &tools,
        &lib,
        &twice,
        -1,
        HighlightTags::default(),
    )
    .expect("the scene builds");
    assert!(error.is_some(), "step 2 fails on the spent tip");

    let good = replay_plain(&base, &[], &tools, &lib, &twice, 1).expect("step 1 alone");
    assert_eq!(
        partial.structure.iter_atoms().count(),
        good.structure.iter_atoms().count()
    );
    for (id, atom) in good.structure.iter_atoms() {
        let same = partial
            .structure
            .get_atom(*id)
            .unwrap_or_else(|| panic!("atom {id} missing from the partial scene"));
        assert_eq!(same.atomic_number, atom.atomic_number);
        assert!((same.position - atom.position).length() < 1e-9);
    }
}

#[test]
fn the_partial_form_returns_the_untouched_scene_when_step_one_fails() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let tools = vec![tip(), probe()];
    let dump_first = BuildScript {
        file: "dump_first".to_string(),
        tolerance: None,
        steps: vec![Step::new("hdump", DVec3::new(20.0, 1.54, 0.0))],
    };

    let (partial, error) = replay_scene_partial(
        &base,
        &[],
        &tools,
        &lib,
        &dump_first,
        -1,
        HighlightTags::default(),
    )
    .expect("the scene builds");
    assert!(error.is_some());
    assert_eq!(
        partial.structure.iter_atoms().count(),
        base.iter_atoms().count() + 12,
        "six atoms per tool, and not one step applied"
    );
}

#[test]
fn a_binding_error_yields_no_scene_at_all() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let build = script("tool_build.json");
    let outcome = replay_scene_partial(
        &base,
        &[],
        &[molecule("tool_tip.xyz")],
        &lib,
        &build,
        -1,
        HighlightTags::default(),
    );
    assert!(
        outcome.is_err(),
        "nothing was replayed, so there is nothing to hand back"
    );
}

#[test]
fn the_partial_form_agrees_with_the_all_or_nothing_one_on_a_good_block() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let tools = vec![tip(), probe()];
    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };

    let (partial, error) =
        replay_scene_partial(&base, &[], &tools, &lib, &one, -1, HighlightTags::default())
            .expect("the scene builds");
    assert!(error.is_none());
    let whole = replay_plain(&base, &[], &tools, &lib, &one, -1).expect("replays");
    assert_eq!(
        partial.structure.iter_atoms().count(),
        whole.structure.iter_atoms().count()
    );
}

// ============================================================================
// Applicability
// ============================================================================

/// The workpiece the editor would be looking at: the whole scene.
fn offer_scene(build: &BuildScript, step: i32) -> (AtomicStructure, Vec<ToolBinding>, OpLibrary) {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let dump = molecule("tool_dump.xyz");
    let scene = replay_plain(&base, &[dump], &[tip(), probe()], &lib, build, step)
        .expect("the prefix replays");
    (scene.structure, scene.bindings, lib)
}

fn empty_script() -> BuildScript {
    BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    }
}

/// The workpiece's `+z` hydrogen of methane A.
fn workpiece_hydrogen(structure: &AtomicStructure) -> u32 {
    let found = structure.get_atoms_in_radius(&DVec3::new(0.0, 0.0, 1.09), 1e-4);
    assert_eq!(found.len(), 1);
    found[0]
}

#[test]
fn applicable_ops_with_no_bindings_answers_exactly_what_it_did_before() {
    let (structure, bindings, lib) = offer_scene(&empty_script(), 0);
    let atom = workpiece_hydrogen(&structure);
    let tolerance = resolve_tolerance(&lib);

    let without = applicable_ops(&structure, &lib, atom, tolerance, None);
    let with = applicable_ops(&structure, &lib, atom, tolerance, Some(&bindings));

    assert!(without.iter().all(|row| row.tool.is_none()));
    // The same operations turn up either way; only the annotation differs.
    let names = |rows: &[atomcad_crystolecule::mechanosynth::Applicability]| {
        let mut names: Vec<String> = rows.iter().map(|row| row.op.clone()).collect();
        names.sort();
        names
    };
    assert_eq!(names(&without), names(&with));
}

#[test]
fn a_charged_tip_makes_the_abstraction_ready_and_a_spent_one_blocks_it() {
    let tolerance = resolve_tolerance(&library("tool_ops.json"));

    // Charged: `habst` is ready.
    let (structure, bindings, lib) = offer_scene(&empty_script(), 0);
    let atom = workpiece_hydrogen(&structure);
    let rows = applicable_ops(&structure, &lib, atom, tolerance, Some(&bindings));
    let habst = rows.iter().find(|row| row.op == "habst").expect("offered");
    let tool = habst.tool.as_ref().expect("a tip row carries one");
    assert!(tool.ready, "reason: {:?}", tool.reason);
    assert_eq!(tool.state.as_deref(), Some("charged"));
    assert!(habst.offerable());

    // Spent: blocked, with the state in the reason.
    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };
    let (structure, bindings, lib) = offer_scene(&one, -1);
    let other = structure.get_atoms_in_radius(&DVec3::new(1.027662, 0.0, -0.363333), 1e-4)[0];
    let rows = applicable_ops(&structure, &lib, other, tolerance, Some(&bindings));
    let habst = rows.iter().find(|row| row.op == "habst").expect("offered");
    let tool = habst.tool.as_ref().expect("a tip row carries one");
    assert!(!tool.ready);
    assert!(!habst.offerable());
    let reason = tool.reason.clone().unwrap_or_default();
    assert!(reason.contains("spent"), "{reason}");
    assert!(reason.contains("charged"), "{reason}");
}

#[test]
fn a_charged_tip_blocks_the_recharge_by_state() {
    let (structure, bindings, lib) = offer_scene(&empty_script(), 0);
    let reservoir = structure.get_atoms_in_radius(&DVec3::new(20.0, 1.54, 0.0), 1e-4)[0];
    let rows = applicable_ops(
        &structure,
        &lib,
        reservoir,
        resolve_tolerance(&lib),
        Some(&bindings),
    );
    let hdump = rows.iter().find(|row| row.op == "hdump").expect("offered");
    let tool = hdump.tool.as_ref().expect("a tip row carries one");
    assert!(!tool.ready);
    let reason = tool.reason.clone().unwrap_or_default();
    assert!(reason.contains("charged"), "{reason}");
    assert!(reason.contains("spent"), "{reason}");
}

#[test]
fn the_geometric_check_runs_even_when_the_label_agrees() {
    // The tip says *charged* and its apex has no atom to give: the row is
    // blocked with the nearest-atom reason, not with a state one.
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let mut hollow = tip();
    // Remove the ethynyl carbon the tool side's `before` names.
    let ethynyl = ids(&hollow)[1];
    hollow.delete_atom(ethynyl);

    let scene = replay_plain(&base, &[], &[hollow, probe()], &lib, &empty_script(), 0)
        .expect("binding does not look at the apex's cargo");
    let atom = workpiece_hydrogen(&scene.structure);
    let rows = applicable_ops(
        &scene.structure,
        &lib,
        atom,
        resolve_tolerance(&lib),
        Some(&scene.bindings),
    );
    let habst = rows.iter().find(|row| row.op == "habst").expect("offered");
    let tool = habst.tool.as_ref().expect("a tip row carries one");
    assert!(!tool.ready);
    assert_eq!(tool.state.as_deref(), Some("charged"));
    let reason = tool.reason.clone().unwrap_or_default();
    assert!(reason.contains("nearest"), "{reason}");
}

#[test]
fn a_tool_side_bond_failure_names_the_tool_at_both_ends() {
    // The pattern checks run on the tool side exactly as on the target side,
    // because the replay's match is the same function and the placement-side
    // tool check calls the same predicate. Here the tool side claims a bond
    // between the apex and the handle carbon 2.66 Å away, which the fixture tip
    // does not have — a tool the library was not computed for.
    let lib = parse_library(&apex_bond_library(), "apex_bond.json").expect("parses");
    let base = molecule("tool_scene.xyz");
    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };

    let message = error_of(&base, &[], &[tip()], &lib, &one, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("pattern atoms 1 and 2"), "{message}");
    assert!(message.contains("carry none"), "{message}");
    assert!(
        message.contains("habst_tool"),
        "the failure names the tool: {message}"
    );

    // …and the same tool is dimmed at placement, through `tool_readiness`'s own
    // nearest-atom loop rather than through the replay's match.
    let scene = replay_plain(&base, &[], &[tip()], &lib, &empty_script(), 0).expect("binds");
    let atom = workpiece_hydrogen(&scene.structure);
    let rows = applicable_ops(
        &scene.structure,
        &lib,
        atom,
        resolve_tolerance(&lib),
        Some(&scene.bindings),
    );
    let habst = rows.iter().find(|row| row.op == "habst").expect("offered");
    let tool = habst.tool.as_ref().expect("a tip row carries one");
    assert!(!tool.ready, "reason: {:?}", tool.reason);
    assert!(!habst.offerable());
    let reason = tool.reason.clone().unwrap_or_default();
    assert!(reason.contains("habst_tool"), "{reason}");
    assert!(reason.contains("missing"), "{reason}");
}

#[test]
fn a_tool_side_degree_failure_names_the_tool() {
    // `deg` on a tool-side atom is the same predicate as on the target side.
    // Here the apex must carry exactly two bonds — its ethynyl partner and a
    // cargo it does not have — and the wired tip's apex carries one.
    let lib = parse_library(&apex_degree_library(), "apex_deg.json").expect("parses");
    let base = molecule("tool_scene.xyz");
    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };

    let message = error_of(&base, &[], &[tip()], &lib, &one, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("needs 2 bond(s)"), "{message}");
    assert!(message.contains("has 1"), "{message}");
    assert!(
        message.contains("habst_tool"),
        "the failure names the tool: {message}"
    );
}

/// `tool_ops.json`'s abstraction, with the tool side's own `before` altered so
/// that exactly one pattern check fails on the fixture tip.
///
/// `after_atoms` repeats the same atoms without any `deg`, which is a
/// `before`-only key.
fn tool_side_library(before_atoms: &str, after_atoms: &str, before_bonds: &str) -> String {
    format!(
        r#"{{
      "format": "atomcad-msops/4",
      "tools": [
        {{ "name": "habst_tool", "states": ["charged", "spent"], "frame": [
          {{ "tag": "apex", "pos": [0.0, 0.0, 0.0] }},
          {{ "tag": "a", "pos": [1.45, 0.0, -3.17] }},
          {{ "tag": "b", "pos": [-0.725, 1.2557, -3.17] }},
          {{ "tag": "c", "pos": [-0.725, -1.2557, -3.17] }} ],
          "envelope": {{ "half_angle": 46.0, "radius": 2.3 }} }}
      ],
      "ops": [
        {{
          "name": "habst",
          "method": "tip",
          "reaction": {{ "target": [0.0, 0.0, 0.0], "tool": [0.0, 0.0, 1.06] }},
          "before": {{ "atoms": [ {{ "id": 1, "el": "H", "pos": [0.0, 0.0, 0.0] }} ], "bonds": [] }},
          "after": {{ "atoms": [], "bonds": [] }},
          "tool": {{
            "type": "habst_tool",
            "from": "charged",
            "to": "spent",
            "before": {{ "atoms": [{before_atoms}], "bonds": [{before_bonds}] }},
            "after": {{ "atoms": [{after_atoms},
              {{ "id": 9, "el": "H", "pos": [0.0, 0.0, 1.06] }}
            ], "bonds": [{before_bonds}, [1, 9]] }}
          }}
        }}
      ]
    }}"#
    )
}

/// A tool side claiming a bond between the apex and the handle carbon, which
/// sit 2.66 Å apart and are not bonded.
fn apex_bond_library() -> String {
    let atoms = r#"{ "id": 1, "el": "C", "pos": [0.0, 0.0, 0.0] },
                    { "id": 2, "el": "C", "pos": [0.0, 0.0, -2.66] }"#;
    tool_side_library(atoms, atoms, "[1, 2]")
}

/// A tool side requiring the apex to carry two bonds, which it does not.
fn apex_degree_library() -> String {
    tool_side_library(
        r#"{ "id": 1, "el": "C", "pos": [0.0, 0.0, 0.0], "deg": 2 },
           { "id": 2, "el": "C", "pos": [0.0, 0.0, -1.21] }"#,
        r#"{ "id": 1, "el": "C", "pos": [0.0, 0.0, 0.0] },
           { "id": 2, "el": "C", "pos": [0.0, 0.0, -1.21] }"#,
        "[1, 2]",
    )
}

#[test]
fn an_unbound_type_blocks_its_rows_with_a_reason_naming_the_tag() {
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let scene = replay_plain(&base, &[], &[tip()], &lib, &empty_script(), 0).expect("binds");
    let atom = workpiece_hydrogen(&scene.structure);
    let rows = applicable_ops(
        &scene.structure,
        &lib,
        atom,
        resolve_tolerance(&lib),
        Some(&scene.bindings),
    );
    let row = rows
        .iter()
        .find(|row| row.op == "habst_probe")
        .expect("offered");
    let tool = row.tool.as_ref().expect("a tip row carries one");
    assert!(!tool.ready);
    let reason = tool.reason.clone().unwrap_or_default();
    assert!(reason.contains("probe"), "{reason}");
    assert!(reason.contains("tools pin"), "{reason}");
}

#[test]
fn bulk_and_spontaneous_rows_carry_no_tool_annotation() {
    let (structure, bindings, lib) = offer_scene(&empty_script(), 0);
    let atom = workpiece_hydrogen(&structure);
    let rows = applicable_ops(
        &structure,
        &lib,
        atom,
        resolve_tolerance(&lib),
        Some(&bindings),
    );
    for name in ["expose", "settle", "expose_uv"] {
        if let Some(row) = rows.iter().find(|row| row.op == name) {
            assert!(row.tool.is_none(), "{name} is not a tip operation");
        }
    }
}

#[test]
fn ready_rows_sort_above_tool_blocked_rows() {
    let one = BuildScript {
        file: "one".to_string(),
        tolerance: None,
        steps: vec![Step::new("habst", DVec3::new(0.0, 0.0, 1.09))],
    };
    let (structure, bindings, lib) = offer_scene(&one, -1);
    let other = structure.get_atoms_in_radius(&DVec3::new(1.027662, 0.0, -0.363333), 1e-4)[0];
    let rows = applicable_ops(
        &structure,
        &lib,
        other,
        resolve_tolerance(&lib),
        Some(&bindings),
    );

    let offerable: Vec<bool> = rows.iter().map(|row| row.offerable()).collect();
    let first_blocked = offerable.iter().position(|ok| !ok);
    if let Some(index) = first_blocked {
        assert!(
            offerable[index..].iter().all(|ok| !*ok),
            "every offerable row comes before every blocked one: {offerable:?}"
        );
    }
    // `habst` is blocked by its tool, `habst_probe` by its missing molecule —
    // both below whatever still fits.
    let habst = rows
        .iter()
        .position(|row| row.op == "habst")
        .expect("listed");
    assert!(!rows[habst].offerable());
}

#[test]
fn the_tool_check_is_not_run_for_an_operation_that_does_not_fit() {
    let (structure, bindings, lib) = offer_scene(&empty_script(), 0);
    // The central carbon of methane A: `habst` wants an H there and finds one
    // only at a hydrogen, so its row is a near miss or absent — and a row that
    // does not fit carries no tool annotation.
    let carbon = structure.get_atoms_in_radius(&DVec3::ZERO, 1e-4)[0];
    let rows = applicable_ops(
        &structure,
        &lib,
        carbon,
        resolve_tolerance(&lib),
        Some(&bindings),
    );
    for row in &rows {
        if !row.fits {
            assert!(row.tool.is_none(), "{} is a near miss", row.op);
        }
    }
}

// ============================================================================
// Events
// ============================================================================

#[test]
fn the_event_rules_group_a_script_the_way_the_methods_table_says() {
    let lib = library("tool_ops.json");
    let at = |op: &str| Step::new(op, DVec3::ZERO);

    // Two consecutive exposures of one agent are one event; a different agent
    // starts a new one; a settle joins the event of the step before it.
    let steps = vec![
        at("habst"),     // 0: a tip visit, its own event
        at("expose"),    // 1: X2
        at("expose"),    // 2: X2, same event as 1
        at("expose_uv"), // 3: UV, a new event
        at("settle"),    // 4: joins the UV event
        at("habst"),     // 5: a new tip visit
    ];
    assert_eq!(event_indices(&steps, &lib), vec![0, 1, 1, 2, 2, 3]);

    // A settle at index 0 is its own event, and a following one joins it.
    let leading = vec![at("settle"), at("settle"), at("expose")];
    assert_eq!(event_indices(&leading, &lib), vec![0, 0, 1]);

    // A settle after a recharge joins the recharge's event.
    let after_dump = vec![at("hdump"), at("settle"), at("hdump")];
    assert_eq!(event_indices(&after_dump, &lib), vec![0, 0, 1]);

    assert!(event_indices(&[], &lib).is_empty());
}

#[test]
fn two_exposures_of_one_event_do_not_commute() {
    // An event is a display grouping, never a replay semantics: the order of
    // the steps inside it *is* their meaning. Here the first exposure's product
    // is what the second one matches, so one order builds a chain and the other
    // finds nothing at all.
    let lib = library("tool_ops.json");
    let mut base = AtomicStructure::new();
    base.add_atom(C, DVec3::ZERO);

    let outward = Step::new("expose", DVec3::ZERO);
    let onward = Step::new("expose", DVec3::new(0.0, 0.0, 1.4));

    let forward = BuildScript {
        file: "forward".to_string(),
        tolerance: None,
        steps: vec![outward.clone(), onward.clone()],
    };
    let backward = BuildScript {
        file: "backward".to_string(),
        tolerance: None,
        steps: vec![onward, outward],
    };

    // Both orders are one event, by the agent rule.
    assert_eq!(event_indices(&forward.steps, &lib), vec![0, 0]);
    assert_eq!(event_indices(&backward.steps, &lib), vec![0, 0]);

    let forward_scene = replay_plain(&base, &[], &[], &lib, &forward, -1)
        .expect("the second exposure lands on the first one's product");
    assert_eq!(forward_scene.structure.iter_atoms().count(), 3);

    let backward_error = error_of(&base, &[], &[], &lib, &backward, -1).to_string();
    assert!(backward_error.contains("step 1"), "{backward_error}");
    assert!(backward_error.contains("not found"), "{backward_error}");
}

#[test]
fn the_plain_replay_is_the_scene_replay_with_nothing_wired() {
    // `replay` is a wrapper, and the assertion is that it stayed one.
    let lib = library("methylate_ops.json");
    let build = script("methylate_build.json");
    let base = molecule("methane.xyz");

    for step in 0..=3 {
        let plain = replay(&base, &lib, &build, step, HighlightTags::default()).expect("replays");
        let scene = replay_plain(&base, &[], &[], &lib, &build, step).expect("replays");
        assert_eq!(
            plain.iter_atoms().count(),
            scene.structure.iter_atoms().count()
        );
        assert_eq!(
            plain.iter_atoms().count(),
            scene.workpiece().iter_atoms().count()
        );
        assert!(scene.bindings.is_empty());
        assert!(scene.participants.values().all(|p| *p == Participant::Base));
    }

    // And the rotation the build states survives the wrapper unchanged.
    assert_eq!(DMat3::IDENTITY.determinant(), 1.0);
}

// ============================================================================
// Structure sanity: the same rules, applied to the inputs
// (doc/design_mechanosynth_pattern_checks.md §6)
// ============================================================================

#[test]
fn a_participant_with_no_bond_model_is_refused_by_name() {
    // The xyz-import case. Without this, every `deg` and every closed-world
    // bond check against that structure would pass vacuously, because a
    // structure with no bonds has no degrees to disagree with.
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let empty = BuildScript {
        file: "none".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    let mut bondless = AtomicStructure::new();
    bondless.add_atom(H, DVec3::new(30.0, 0.0, 0.0));
    bondless.add_atom(H, DVec3::new(30.0, 0.0, 5.0));

    let message = error_of(&base, &[bondless], &[], &lib, &empty, -1).to_string();
    assert!(message.contains("feedstock 0"), "{message}");
    assert!(message.contains("no bonds"), "{message}");
    assert!(message.contains("rebond"), "{message}");

    // One atom is not a bond model missing; it is a structure with no bond to
    // have.
    let mut lone = AtomicStructure::new();
    lone.add_atom(H, DVec3::new(30.0, 0.0, 0.0));
    replay_plain(&base, &[lone], &[], &lib, &empty, -1).expect("a single atom is fine");
}

#[test]
fn two_unbonded_atoms_of_one_participant_too_close_are_refused_by_name() {
    let lib = library("tool_ops.json");
    let empty = BuildScript {
        file: "none".to_string(),
        tolerance: None,
        steps: Vec::new(),
    };

    // A workpiece with an unbonded hydrogen jammed against one of its own.
    let mut base = molecule("tool_scene.xyz");
    base.add_atom(H, DVec3::new(0.0, 0.3, 1.09));

    let message = error_of(&base, &[], &[], &lib, &empty, -1).to_string();
    assert!(message.contains("base"), "{message}");
    assert!(message.contains("no bond between them"), "{message}");
    assert!(message.contains("0.90"), "{message}");

    // A **tool** parked in contact with the workpiece is a modelling choice,
    // not a broken input: the pair check is per participant.
    let base = molecule("tool_scene.xyz");
    let touching = tagged("tool_tip_touching.xyz", "habst_tool");
    replay_plain(&base, &[], &[touching], &lib, &empty, -1)
        .expect("a tool may be parked against the workpiece");
}

#[test]
fn a_placed_atom_inside_a_parked_tool_is_refused() {
    // "The whole scene, feedstocks and tools included, since a placed atom
    // that lands inside a parked tool is a collision whoever it belongs to."
    // `settle` is spontaneous, so nothing about the tool is even consulted —
    // the tool is simply in the way.
    let lib = library("tool_ops.json");
    let base = molecule("tool_scene.xyz");
    let touching = tagged("tool_tip_touching.xyz", "habst_tool");
    let nudge = BuildScript {
        file: "nudge".to_string(),
        tolerance: None,
        steps: vec![Step::new("settle", DVec3::new(0.0, 0.0, 1.09))],
    };

    // With no tool wired the same step is fine: the site above the hydrogen is
    // empty.
    replay_plain(&base, &[], &[], &lib, &nudge, -1).expect("nothing is in the way");

    let message = error_of(&base, &[], &[touching], &lib, &nudge, -1).to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("habst_tool"), "{message}");
    assert!(message.contains("does not bond"), "{message}");
}
