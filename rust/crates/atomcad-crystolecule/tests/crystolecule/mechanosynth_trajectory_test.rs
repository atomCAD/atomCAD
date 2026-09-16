//! Phase 1 tests for tool trajectories: the `/4` feasibility layer — the
//! envelope, the sweep, obstacles, the landing and containment — and the
//! presentation layer built on it — runs, poses, flights, the hover, the scan
//! and the replay at a step time. See `doc/design_mechanosynth_trajectory.md`
//! §Testing.
//!
//! Two rules shape what is asserted here. The feasibility layer is tested as
//! **pure geometry** first and then *through* `apply_step_in_scene`, which is
//! the call a sequence generator makes; the presentation layer is tested
//! against the feasibility layer's own output, **never against hand-typed
//! coordinates** — a pose is right when it puts the reaction points together,
//! not when it matches a number somebody wrote down.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::{
    APEX_FRAME_TAG, BuildScript, CLASH_BLOCK, CLEAR_MARGIN, Envelope, HighlightTags, Landing, Leg,
    OpLibrary, Participant, Pose, REACTION, REACTION_DEPARTURE, REACTION_LANDING, STANDOFF_HEIGHT,
    SWEEP_DIRECTIONS, Scene, Step, ToolMotion, apply_step_in_scene, apply_tool_pose,
    approach_direction, arriving_pose, build_scene, load_build_script, load_library,
    match_step_in_scene, obstacles_for, plan_landing, reaction_pose, replay_scene, replay_scene_at,
    replay_steps, resolve_tolerance, runs, sweep_directions,
};
use atomcad_test_support::fixture_path;
use glam::DVec3;

/// The frame tags every tool type in `tool_ops.json` uses.
const FRAME_TAGS: [&str; 4] = [APEX_FRAME_TAG, "a", "b", "c"];
const APEX_INDEX: usize = 0;
const LEG_INDICES: [usize; 3] = [3, 4, 5];

// ============================================================================
// Helpers — the same cast `mechanosynth_tools_test.rs` uses
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

fn ids(structure: &AtomicStructure) -> Vec<u32> {
    let mut ids: Vec<u32> = structure.iter_atoms().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

fn tagged(name: &str, tool_type: &str) -> AtomicStructure {
    let mut structure = molecule(name);
    let ids = ids(&structure);
    for id in &ids {
        structure
            .add_atom_tag(*id, tool_type)
            .expect("type tag fits");
    }
    structure
        .add_atom_tag(ids[APEX_INDEX], FRAME_TAGS[0])
        .expect("apex tag fits");
    for (slot, index) in LEG_INDICES.iter().enumerate() {
        structure
            .add_atom_tag(ids[*index], FRAME_TAGS[slot + 1])
            .expect("leg tag fits");
    }
    structure
}

fn tip() -> AtomicStructure {
    tagged("tool_tip.xyz", "habst_tool")
}

fn probe() -> AtomicStructure {
    tagged("tool_probe.xyz", "probe")
}

/// The probe skeleton with its tags stripped, so that it enters the scene as a
/// **reservoir** rather than as a second molecule of type `probe` — which
/// `build_scene` refuses as `ToolDuplicate`. Translated to sit over a site, it
/// is the obstacle every blocked-approach assertion here is built from.
fn obstacle_over(position: DVec3) -> AtomicStructure {
    let mut structure = molecule("tool_probe.xyz");
    // tool_probe.xyz is parked at (0, -30, 0) with its apex first.
    let apex = structure
        .get_atom(ids(&structure)[APEX_INDEX])
        .expect("the apex")
        .position;
    let shift = position - apex;
    for atom_id in ids(&structure) {
        let moved = structure.get_atom(atom_id).expect("live").position + shift;
        structure.set_atom_position(atom_id, moved);
    }
    structure
}

/// A single carbon hung over a site: the narrowest obstacle there is, so the
/// sweep has to tilt only as far as it takes to slip past this one atom.
fn atom_over(position: DVec3) -> AtomicStructure {
    let mut structure = AtomicStructure::new();
    structure.add_atom(6, position);
    structure
}

/// A shell of loose carbons around a site, each its own one-atom reservoir, so
/// that no direction out of it is free.
///
/// One atom apiece on purpose: the scene's input checks compare atoms **within**
/// one participant, so single-atom reservoirs need no bond model and cannot
/// clash with each other. Boxing a site in is the only reliable way to make the
/// sweep report a blocked landing now that the envelope is a solid — a single
/// obstacle overhead merely tilts the approach.
fn cage_around(centre: DVec3, radius: f64) -> Vec<AtomicStructure> {
    sweep_directions()
        .iter()
        .step_by(16)
        .map(|direction| atom_over(centre + *direction * radius))
        .collect()
}

fn empty_script() -> BuildScript {
    BuildScript {
        file: "empty".to_string(),
        tolerance: None,
        steps: Vec::new(),
    }
}

/// The scene of the trajectory fixture, with the whole cast and no highlights.
fn trajectory_scene(feedstocks: &[AtomicStructure]) -> (Scene, OpLibrary, BuildScript) {
    scene_with(feedstocks, &[tip(), probe()])
}

fn scene_with(
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
) -> (Scene, OpLibrary, BuildScript) {
    let lib = library("tool_ops.json");
    let build = script("trajectory_build.json");
    let mut cast: Vec<AtomicStructure> = vec![molecule("tool_dump.xyz")];
    cast.extend(feedstocks.iter().cloned());
    let scene =
        build_scene(&molecule("tool_scene.xyz"), &cast, tools, &lib).expect("the cast binds");
    (scene, lib, build)
}

/// `replay_scene_at` over the trajectory fixture, with no highlights.
fn at(step: i32, time: f64, feedstocks: &[AtomicStructure]) -> (Scene, Option<ToolMotion>) {
    at_in("trajectory_build.json", step, time, feedstocks)
}

fn at_in(
    name: &str,
    step: i32,
    time: f64,
    feedstocks: &[AtomicStructure],
) -> (Scene, Option<ToolMotion>) {
    let lib = library("tool_ops.json");
    let build = script(name);
    let mut cast: Vec<AtomicStructure> = vec![molecule("tool_dump.xyz")];
    cast.extend(feedstocks.iter().cloned());
    replay_scene_at(
        &molecule("tool_scene.xyz"),
        &cast,
        &[tip(), probe()],
        &lib,
        &build,
        step,
        time,
        HighlightTags::default(),
    )
    .expect("the fixture replays")
}

/// Poses are computed through quaternion slerp, so they carry float noise a
/// bitwise comparison would trip over; a picometre and a microradian are far
/// below anything this design means by "the same pose".
#[track_caller]
fn assert_pose_close(actual: Pose, expected: Pose, what: &str) {
    assert!(
        (actual.t - expected.t).length() < 1e-9,
        "{what}: {:?} vs {:?}",
        actual.t,
        expected.t
    );
    for column in 0..3 {
        assert!(
            (actual.r.col(column) - expected.r.col(column)).length() < 1e-9,
            "{what}: column {column}"
        );
    }
}

/// The landing of one step of the trajectory fixture, taken the way a generator
/// takes it: match, then plan, with nothing applied.
fn landing_of(step_index: usize, feedstocks: &[AtomicStructure]) -> Landing {
    landing_with(step_index, feedstocks, &[tip(), probe()])
}

fn landing_with(
    step_index: usize,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
) -> Landing {
    let (mut scene, lib, build) = scene_with(feedstocks, tools);
    let tolerance = resolve_tolerance(&lib);
    let clash = lib.clash_factor();
    let (failure, _) = replay_steps(
        &mut scene,
        &lib,
        &build,
        step_index as i32,
        HighlightTags::default(),
    )
    .expect("the script is valid");
    assert!(failure.is_none(), "{failure:?}");

    let step = &build.steps[step_index];
    let op = lib.get(&step.op).expect("declared");
    let plan = match_step_in_scene(&scene, op, step, step_index + 1, tolerance, clash, true)
        .expect("the step matches");
    plan_landing(&scene, &plan)
}

// ============================================================================
// Feasibility — the envelope, as pure geometry
// ============================================================================

/// 30° and 4 Å: the cone reaches the cylinder at s = 4/tan30 ≈ 6.93.
fn cone() -> Envelope {
    Envelope {
        half_angle: 30.0_f64.to_radians(),
        radius: 4.0,
    }
}

#[test]
fn the_gap_is_zero_on_every_face_of_the_envelope() {
    let envelope = cone();
    let tan = envelope.half_angle.tan();

    // On the slant, short of the rim.
    for s in [1.0, 3.0, 6.0] {
        assert!(envelope.gap(s, s * tan).abs() < 1e-12, "slant at s = {s}");
    }
    // The rim itself, and the cylinder wall beyond it.
    let rim = envelope.radius / tan;
    assert!(envelope.gap(rim, envelope.radius).abs() < 1e-12);
    for s in [rim + 1.0, rim + 10.0] {
        assert!(
            envelope.gap(s, envelope.radius).abs() < 1e-12,
            "wall at {s}"
        );
    }
    // The apex is a surface point too.
    assert!(envelope.gap(0.0, 0.0).abs() < 1e-12);
}

#[test]
fn a_point_inside_the_envelope_has_a_negative_gap() {
    let envelope = cone();
    // On the axis, well inside: the nearest surface is the slant.
    let gap = envelope.gap(3.0, 0.0);
    assert!(gap < 0.0, "{gap}");
    assert!(
        (gap + 3.0 * envelope.half_angle.sin()).abs() < 1e-12,
        "{gap}"
    );

    // Inside the cylinder: the nearest surface is the wall.
    let deep = envelope.gap(40.0, 3.0);
    assert!((deep + 1.0).abs() < 1e-12, "{deep}");

    assert!(envelope.contains(3.0, 0.0));
    assert!(!envelope.contains(-1.0, 0.0));
    assert!(!envelope.contains(40.0, 5.0));
}

#[test]
fn the_gap_beside_the_slant_is_a_distance_and_not_a_radial_excess() {
    // The assertion that pins the fourth review's correction: a point at radial
    // excess `x` beside the slant is `x · cos α` from the surface, **not** `x`.
    // The radial gap overstates the room by 1/cos α — fifteen per cent at 30°,
    // seventy-four at 55° — so a sphere it called clear could sit inside.
    let envelope = cone();
    let (sin_a, cos_a) = envelope.half_angle.sin_cos();
    let s = 3.0;
    for excess in [0.2, 1.0, 2.0] {
        let rho = s * (sin_a / cos_a) + excess;
        let gap = envelope.gap(s, rho);
        assert!(
            (gap - excess * cos_a).abs() < 1e-12,
            "excess {excess}: {gap}"
        );
        assert!(gap < excess, "the distance is the shorter of the two");
    }
}

#[test]
fn a_point_behind_the_apex_measures_to_the_apex() {
    let envelope = cone();
    for (s, rho) in [(-1.0, 0.0), (-2.0, 1.0), (-0.5, 0.5)] {
        let gap = envelope.gap(s, rho);
        assert!(
            (gap - (s * s + rho * rho).sqrt()).abs() < 1e-12,
            "({s}, {rho}): {gap}"
        );
    }
}

#[test]
fn a_point_beside_the_cylinder_is_never_called_inside() {
    // Past the rim the infinite cone's slant would place a point beside the
    // cylinder *inside* the envelope; the rim and the wall are what count there.
    let envelope = cone();
    let rim = envelope.radius / envelope.half_angle.tan();
    let gap = envelope.gap(rim + 5.0, envelope.radius + 0.5);
    assert!((gap - 0.5).abs() < 1e-12, "{gap}");
    assert!(!envelope.contains(rim + 5.0, envelope.radius + 0.5));
}

#[test]
fn clearance_is_the_worst_obstacle_minus_its_margin() {
    let envelope = cone();
    let at = DVec3::ZERO;
    let up = DVec3::Z;

    // No obstacles at all is open sky.
    assert!(envelope.clearance(at, up, &[]).is_infinite());

    // One obstacle on the axis, 3 Å up: inside, so the clearance is negative
    // whatever its margin.
    let inside = envelope.clearance(at, up, &[(DVec3::new(0.0, 0.0, 3.0), 0.0)]);
    assert!(inside < 0.0, "{inside}");

    // One obstacle beside the cone at a known excess, margin subtracted.
    let (sin_a, cos_a) = envelope.half_angle.sin_cos();
    let s = 3.0;
    let rho = s * (sin_a / cos_a) + 1.0;
    let outside = envelope.clearance(at, up, &[(DVec3::new(rho, 0.0, s), 0.25)]);
    assert!((outside - (cos_a - 0.25)).abs() < 1e-12, "{outside}");
}

// ============================================================================
// Feasibility — the sweep
// ============================================================================

#[test]
fn the_sweep_walks_the_sphere_from_the_north_pole_down() {
    let directions = sweep_directions();
    assert_eq!(directions.len(), SWEEP_DIRECTIONS);
    assert!((directions[0] - DVec3::Z).length() < 1e-12);
    assert!((directions[SWEEP_DIRECTIONS - 1] + DVec3::Z).length() < 1e-12);

    // The spacing a sphere of this many points can manage, in radians.
    let spacing = (4.0 * std::f64::consts::PI / SWEEP_DIRECTIONS as f64).sqrt();
    let mut previous = 0.0;
    for direction in directions {
        assert!((direction.length() - 1.0).abs() < 1e-12);
        let tilt = direction.z.clamp(-1.0, 1.0).acos();
        assert!(tilt >= previous - 1e-12, "tilt went backwards");
        assert!(tilt - previous <= spacing, "a jump of {}", tilt - previous);
        previous = tilt;
    }
}

#[test]
fn an_unobstructed_site_is_approached_from_straight_above() {
    let approach = approach_direction(&cone(), DVec3::ZERO, &[]);
    assert_eq!(approach.direction, DVec3::Z);
    assert_eq!(approach.tilt, 0.0);
    assert!(approach.clearance.is_infinite());
}

#[test]
fn one_obstacle_overhead_tilts_the_approach_as_little_as_it_can() {
    let envelope = cone();
    let obstacle = (DVec3::new(0.0, 0.0, 8.0), 1.0);
    let approach = approach_direction(&envelope, DVec3::ZERO, &[obstacle]);

    assert!(approach.clearance >= CLEAR_MARGIN, "{approach:?}");
    assert!(approach.tilt > 0.0);

    // The refinement leaves no free direction between this one and vertical:
    // anything less tilted is blocked.
    let less = ((approach.tilt - 0.02).max(0.0)).sin_cos();
    let nearer = DVec3::new(less.0, 0.0, less.1);
    let rotated = rotate_toward(approach.direction, nearer.z.acos());
    assert!(
        envelope.clearance(DVec3::ZERO, rotated, &[obstacle]) < CLEAR_MARGIN,
        "a less tilted direction was free, so the refinement stopped early"
    );
}

/// `direction`, tilted back toward `+z` until its own tilt is `tilt`.
fn rotate_toward(direction: DVec3, tilt: f64) -> DVec3 {
    let axis = DVec3::Z.cross(direction);
    if axis.length() < 1e-12 {
        return DVec3::Z;
    }
    let axis = axis.normalize();
    let perpendicular = axis.cross(DVec3::Z).normalize();
    (DVec3::Z * tilt.cos() + perpendicular * tilt.sin()).normalize()
}

#[test]
fn an_enclosed_site_reports_the_least_blocked_direction_rather_than_failing() {
    let envelope = cone();
    // A shell of obstacles around the point: no direction can be free.
    let obstacles: Vec<(DVec3, f64)> = sweep_directions()
        .iter()
        .map(|direction| (*direction * 2.0, 1.5))
        .collect();
    let approach = approach_direction(&envelope, DVec3::ZERO, &obstacles);

    assert!(approach.clearance < 0.0, "{approach:?}");
    assert!((approach.direction.length() - 1.0).abs() < 1e-12);
    // It is the best of a bad set: no sampled direction does better.
    for direction in sweep_directions() {
        assert!(
            envelope.clearance(DVec3::ZERO, *direction, &obstacles) <= approach.clearance + 1e-9
        );
    }
}

#[test]
fn the_sweep_gives_the_same_answer_for_the_same_scene() {
    let obstacles = [
        (DVec3::new(0.0, 0.0, 6.0), 1.2),
        (DVec3::new(1.0, 1.0, 4.0), 0.9),
    ];
    let first = approach_direction(&cone(), DVec3::ZERO, &obstacles);
    let second = approach_direction(&cone(), DVec3::ZERO, &obstacles);
    assert_eq!(first, second);
}

// ============================================================================
// Feasibility — through the scene
// ============================================================================

#[test]
fn no_tool_is_an_obstacle_and_the_excluded_set_is_dropped() {
    let (scene, lib, _) = trajectory_scene(&[]);
    // **Every** tool's atoms are left out, not just the visiting one's. A sweep
    // that saw the parked tools would make a sequence depend on the tool layout,
    // and the layout is meant to be the designer's to change afterwards.
    let tool_atoms: Vec<u32> = scene
        .participants
        .iter()
        .filter(|(_, participant)| matches!(participant, Participant::Tool(_)))
        .map(|(atom_id, _)| *atom_id)
        .collect();
    assert!(tool_atoms.len() > 6, "both tools are wired");

    let all = obstacles_for(&scene, &[], lib.clash_factor());
    let live = scene.structure.iter_atoms().count();
    assert_eq!(all.len(), live - tool_atoms.len());

    // Excluding an atom drops exactly one obstacle.
    let victim = scene
        .structure
        .iter_atoms()
        .map(|(atom_id, _)| *atom_id)
        .find(|atom_id| !tool_atoms.contains(atom_id))
        .expect("the scene has atoms that are not a tool's");
    let fewer = obstacles_for(&scene, &[victim], lib.clash_factor());
    assert_eq!(fewer.len(), all.len() - 1);

    // The margin is `clash · r_obstacle` and **nothing else**: the envelope is a
    // solid that already contains the tool's atoms with their radii, so adding
    // the tool's radius here would charge for it twice. The cast is hydrogen and
    // carbon.
    for (_, margin) in &all {
        assert!(*margin >= CLASH_BLOCK * 0.31 - 1e-9, "{margin}");
        assert!(*margin <= CLASH_BLOCK * 0.76 + 1e-9, "{margin}");
    }
}

#[test]
fn a_landing_places_the_reaction_point_where_the_step_puts_it() {
    let landing = landing_of(0, &[]);
    let (_, _, build) = trajectory_scene(&[]);
    let step = &build.steps[0];

    // `p_r = step.r · reaction.target + step.t`; `habst`'s target point is the
    // hydrogen's own place, so the reaction point *is* the step's translation.
    assert!((landing.reaction_point - (step.r * DVec3::ZERO + step.t)).length() < 1e-12);
    assert_eq!(landing.reaction_tool, DVec3::new(0.0, 0.0, 1.06));
    assert_eq!(landing.standoff_height, STANDOFF_HEIGHT);
}

#[test]
fn the_standoff_is_a_constant_height_up_the_approach_wherever_the_tool_is_parked() {
    // The visit's geometry is a property of the *site*, not of the layout: it is
    // `STANDOFF_HEIGHT` up the approach direction from the reaction point, and
    // nothing about the parked tools enters it. That is what lets a sequence be
    // generated before anyone decides where the tools go.
    let vertical = landing_of(0, &[]);
    assert_eq!(vertical.approach.direction, DVec3::Z);
    assert_eq!(vertical.standoff_height, STANDOFF_HEIGHT);
    assert!(
        (vertical.standoff_point() - (vertical.reaction_point + DVec3::Z * STANDOFF_HEIGHT))
            .length()
            < 1e-12
    );

    // A tilted approach travels the same distance, along its own axis.
    let tilted = landing_of(0, &[atom_over(DVec3::new(0.0, 0.0, 10.0))]);
    assert!(tilted.approach.tilt > 0.0, "{:?}", tilted.approach);
    assert_eq!(tilted.standoff_height, STANDOFF_HEIGHT);
    assert!(
        (tilted.standoff_point()
            - (tilted.reaction_point + tilted.approach.direction * STANDOFF_HEIGHT))
            .length()
            < 1e-12
    );
}

#[test]
fn an_obstacle_over_the_site_tilts_the_approach_and_moving_it_away_makes_it_vertical_again() {
    let clear = landing_of(0, &[]);
    assert_eq!(clear.approach.direction, DVec3::Z);

    // The probe skeleton hung directly over the site, untagged so that it joins
    // the scene as a reservoir rather than as a second `probe`.
    let blocked = landing_of(0, &[obstacle_over(DVec3::new(0.0, 0.0, 5.0))]);
    assert!(blocked.approach.tilt > 0.0, "{:?}", blocked.approach);
    assert!(blocked.approach.direction != DVec3::Z);

    // Far away it is not in the way at all.
    let elsewhere = landing_of(0, &[obstacle_over(DVec3::new(0.0, 40.0, 5.0))]);
    assert_eq!(elsewhere.approach.direction, DVec3::Z);
}

#[test]
fn a_tool_parked_over_the_site_does_not_tilt_anything() {
    // The same skeleton in the same place, but wired to `tools` rather than to
    // `feedstocks`, is invisible to the sweep. Keeping the tools out of each
    // other's way is the designer's job, not the sequence's.
    let over_the_site = {
        let mut probe = molecule("tool_probe.xyz");
        let apex = probe
            .get_atom(ids(&probe)[APEX_INDEX])
            .expect("apex")
            .position;
        let shift = DVec3::new(0.0, 0.0, 5.0) - apex;
        for atom_id in ids(&probe) {
            let moved = probe.get_atom(atom_id).expect("live").position + shift;
            probe.set_atom_position(atom_id, moved);
        }
        let ids = ids(&probe);
        for id in &ids {
            probe.add_atom_tag(*id, "probe").expect("type tag fits");
        }
        probe
            .add_atom_tag(ids[APEX_INDEX], FRAME_TAGS[0])
            .expect("apex tag fits");
        for (slot, index) in LEG_INDICES.iter().enumerate() {
            probe
                .add_atom_tag(ids[*index], FRAME_TAGS[slot + 1])
                .expect("leg tag fits");
        }
        probe
    };
    let landing = landing_with(0, &[], &[tip(), over_the_site]);
    assert_eq!(landing.approach.direction, DVec3::Z);
    assert!(landing.reachable());
}

#[test]
fn a_bound_molecule_outside_its_types_envelope_is_refused_at_binding() {
    // The tip on its handle is 250 atoms wide; a library claiming the bare
    // skeleton's envelope for it is wrong, and wrong *before* any step.
    let text = std::fs::read_to_string(fixture_path("mechanosynth/tool_ops.json"))
        .expect("the fixture reads");
    let narrow = text.replace(
        r#""envelope": { "half_angle": 46.0, "radius": 5.2 }"#,
        r#""envelope": { "half_angle": 31.0, "radius": 2.3 }"#,
    );
    let lib = atomcad_crystolecule::mechanosynth::parse_library(&narrow, "narrow.json")
        .expect("the narrowed library still parses");

    let on_handle = tagged("tool_tip_on_handle.xyz", "habst_tool");
    let failure = build_scene(&molecule("tool_scene.xyz"), &[], &[on_handle], &lib)
        .expect_err("the molecule does not fit the envelope");
    let message = failure.to_string();
    assert!(message.contains("tool 0 (habst_tool)"), "{message}");
    // The check is on the atom's *sphere*, not its centre: the envelope is the
    // solid the tool occupies.
    assert!(message.contains("habst_tool"), "{message}");
    assert!(message.contains("outside the envelope"), "{message}");
    // It names the operation whose reaction point the envelope was placed at.
    assert!(
        message.contains("habst") || message.contains("hdump"),
        "{message}"
    );

    // The envelope the fixture really states holds it.
    let honest = library("tool_ops.json");
    let on_handle = tagged("tool_tip_on_handle.xyz", "habst_tool");
    build_scene(&molecule("tool_scene.xyz"), &[], &[on_handle], &honest)
        .expect("the stated envelope contains the molecule");
}

#[test]
fn the_binding_carries_the_types_envelope_and_axis() {
    let (scene, lib, _) = trajectory_scene(&[]);
    for binding in &scene.bindings {
        let declared = lib.tool_type(&binding.tool_type).expect("declared");
        assert_eq!(binding.envelope, declared.envelope);
        assert_eq!(binding.axis, declared.axis());
        // These fixtures put their legs below the apex, so the axis is `-z`.
        assert_eq!(binding.axis, -DVec3::Z);
    }
}

// ============================================================================
// Feasibility — the generator's path
// ============================================================================

#[test]
fn applying_a_step_returns_the_landing_planning_it_would_have_given() {
    let (mut scene, lib, build) = trajectory_scene(&[]);
    let tolerance = resolve_tolerance(&lib);
    let clash = lib.clash_factor();
    let step = &build.steps[0];
    let op = lib.get(&step.op).expect("declared");

    // The generator's "ask before committing" path leaves the scene alone.
    let before = scene.structure.iter_atoms().count();
    let plan = match_step_in_scene(&scene, op, step, 1, tolerance, clash, true).expect("matches");
    let planned = plan_landing(&scene, &plan);
    assert_eq!(scene.structure.iter_atoms().count(), before);

    let effect = apply_step_in_scene(&mut scene, op, step, 1, tolerance, clash, true)
        .expect("the step applies");
    assert_eq!(effect.landing, Some(planned));
}

#[test]
fn a_blocked_site_applies_and_reports_rather_than_failing() {
    // The site boxed in from above: the apply still succeeds and still rewrites
    // the workpiece, and the verdict rides back on the effect for a generator
    // to refuse over.
    let cage = cage_around(DVec3::new(0.0, 0.0, 1.09), 3.0);
    let (mut scene, lib, build) = trajectory_scene(&cage);
    let step = &build.steps[0];
    let op = lib.get(&step.op).expect("declared");

    let site = scene.structure.get_atoms_in_radius(&step.t, 1e-6);
    assert_eq!(site.len(), 1, "the hydrogen the step abstracts");
    let effect = apply_step_in_scene(
        &mut scene,
        op,
        step,
        1,
        resolve_tolerance(&lib),
        lib.clash_factor(),
        true,
    )
    .expect("a blocked approach never fails a step");

    let landing = effect.landing.expect("a tip step with a bound tool lands");
    assert!(!landing.reachable(), "{:?}", landing.approach);
    assert!(landing.approach.clearance < 0.0);
    // The abstraction happened all the same: the hydrogen left the workpiece.
    assert!(scene.structure.get_atom(site[0]).is_none());
}

#[test]
fn a_replay_reports_one_landing_per_applied_tip_step_and_none_elsewhere() {
    let (mut scene, lib, build) = trajectory_scene(&[]);
    let (failure, landings) = replay_steps(&mut scene, &lib, &build, -1, HighlightTags::default())
        .expect("the script is valid");
    assert!(failure.is_none(), "{failure:?}");
    assert_eq!(landings.len(), build.steps.len());

    // habst, settle, hdump, habst, habst_probe, expose.
    let landed: Vec<bool> = landings.iter().map(Option::is_some).collect();
    assert_eq!(landed, vec![true, false, true, true, true, false]);
    assert_eq!(landings[0].unwrap().tool, 0);
    assert_eq!(landings[4].unwrap().tool, 1);
}

#[test]
fn nothing_lands_when_no_tool_is_wired() {
    let lib = library("tool_ops.json");
    let build = script("trajectory_build.json");
    let mut scene = build_scene(
        &molecule("tool_scene.xyz"),
        &[molecule("tool_dump.xyz")],
        &[],
        &lib,
    )
    .expect("a workpiece-only scene builds");
    let (failure, landings) = replay_steps(&mut scene, &lib, &build, -1, HighlightTags::default())
        .expect("the script is valid");
    assert!(failure.is_none(), "{failure:?}");
    assert!(landings.iter().all(Option::is_none));
}

// ============================================================================
// Presentation — runs
// ============================================================================

#[test]
fn a_run_spans_its_settles_and_is_ended_by_a_bulk_or_another_tool() {
    let lib = library("tool_ops.json");
    let build = script("trajectory_build.json");
    let structure = runs(&build, &lib);

    // habst(0) — settle(1) — hdump(2) — habst(3) are one run of habst_tool.
    assert_eq!(structure.previous_visit(0), None);
    assert_eq!(structure.next_visit(0), Some(2));
    assert_eq!(structure.previous_visit(2), Some(0));
    assert_eq!(structure.next_visit(2), Some(3));
    assert_eq!(structure.previous_visit(3), Some(2));
    assert_eq!(structure.next_visit(3), None);
    assert_eq!(structure.run_start(3), 0);

    // The settle inside the run reports the two visits it lies between.
    assert_eq!(structure.spanned_by(1), Some((0, 2)));

    // The probe's `tip` step is what ended the first run, and opens its own.
    assert_eq!(structure.tool_type(4), Some("probe"));
    assert_eq!(structure.previous_visit(4), None);
    assert_eq!(structure.next_visit(4), None);

    // A bulk step is no visit at all.
    assert_eq!(structure.tool_type(5), None);
    assert_eq!(structure.spanned_by(5), None);
}

#[test]
fn a_bulk_step_between_two_visits_of_one_tool_makes_two_runs() {
    let lib = library("tool_ops.json");
    let two = |between: &str| {
        let mut build = empty_script();
        build.steps = vec![
            Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
            Step::new(between, DVec3::ZERO),
            Step::new("habst", DVec3::new(1.027662, 0.0, -0.363333)),
        ];
        runs(&build, &lib)
    };

    // A settle between them keeps one run…
    assert_eq!(two("settle").next_visit(0), Some(2));
    // …a dose does not, because the instrument retracts from an exposure…
    assert_eq!(two("expose").next_visit(0), None);
    // …and neither does another tool's visit.
    assert_eq!(two("habst_probe").next_visit(0), None);
    // A settle outside any run is spanned by nothing.
    assert_eq!(two("expose").spanned_by(1), None);
}

// ============================================================================
// Presentation — poses
// ============================================================================

#[test]
fn the_reaction_pose_brings_the_two_reaction_points_together() {
    let landing = landing_of(0, &[]);
    let (scene, _, _) = trajectory_scene(&[]);
    let park = park_of(&scene, landing.tool);
    let reaction = reaction_pose(&landing, &park);

    // The tool-side point lands on the target's, exactly.
    assert!((reaction.apply(landing.reaction_tool) - landing.reaction_point).length() < 1e-9);
    // …and the tool's axis is the approach direction.
    let axis = reaction.r * landing.axis;
    assert!(
        (axis - landing.approach.direction).length() < 1e-9,
        "{axis}"
    );
}

#[test]
fn the_reaction_pose_turns_the_tool_by_the_axis_angle_and_no_more() {
    // The roll is free — the envelope is a solid of revolution — so it is spent
    // moving the tool as little as possible: it never twirls.
    let landing = landing_of(0, &[obstacle_over(DVec3::new(0.0, 0.0, 5.0))]);
    let (scene, _, _) = trajectory_scene(&[obstacle_over(DVec3::new(0.0, 0.0, 5.0))]);
    let park = park_of(&scene, landing.tool);
    let reaction = reaction_pose(&landing, &park);

    let arriving_axis = (park.r * landing.axis).normalize();
    let axis_angle = arriving_axis
        .dot(landing.approach.direction)
        .clamp(-1.0, 1.0)
        .acos();
    let turn = glam::DQuat::from_mat3(&reaction.r)
        .angle_between(glam::DQuat::from_mat3(&park.r))
        .abs();
    assert!((turn - axis_angle).abs() < 1e-9, "{turn} vs {axis_angle}");
}

#[test]
fn the_arriving_pose_folds_the_runs_earlier_visits_in_order() {
    let (mut scene, lib, build) = trajectory_scene(&[]);
    let (failure, landings) =
        replay_steps(&mut scene, &lib, &build, -1, HighlightTags::default()).expect("valid");
    assert!(failure.is_none());
    let structure = runs(&build, &lib);
    let park = park_of(&scene, 0);

    // The run's first visit arrives parked…
    assert_pose_close(
        arriving_pose(&structure, &landings, &park, 0),
        park,
        "first",
    );
    // …the second arrives in the first's reaction orientation…
    let first = reaction_pose(&landings[0].unwrap(), &park);
    assert_pose_close(
        arriving_pose(&structure, &landings, &park, 2),
        first,
        "second",
    );
    // …and the third in the second's, folded in order.
    let second = reaction_pose(&landings[2].unwrap(), &first);
    assert_pose_close(
        arriving_pose(&structure, &landings, &park, 3),
        second,
        "third",
    );

    // The probe's own run starts from its own park, not from the tip's.
    let probe_park = park_of(&scene, 1);
    assert_pose_close(
        arriving_pose(&structure, &landings, &probe_park, 4),
        probe_park,
        "the probe",
    );
}

/// Halfway along the tip's inbound flight on step 1: from its park to the
/// standoff the landing puts over the site.
fn flight_midpoint() -> DVec3 {
    let landing = landing_of(0, &[]);
    let (scene, _, _) = trajectory_scene(&[]);
    (scene.bindings[landing.tool].pose.t + landing.standoff_point()) / 2.0
}

fn park_of(scene: &Scene, tool: usize) -> Pose {
    Pose {
        r: scene.bindings[tool].pose.r,
        t: scene.bindings[tool].pose.t,
    }
}

// ============================================================================
// Presentation — the motion over a step
// ============================================================================

#[test]
fn a_first_visit_starts_at_park_and_a_chained_one_at_its_standoff() {
    // Step 1 opens the run: the tool is at park at u = 0.
    let (scene, motion) = at(1, 0.0, &[]);
    let motion = motion.expect("a tip step with a bound tool moves");
    assert_pose_close(
        motion.pose_at(0.0),
        park_of(&scene, motion.tool()),
        "at park",
    );

    // Step 3 is chained: the tool already hovers at this visit's standoff.
    let (_, chained) = at(3, 0.0, &[]);
    let chained = chained.expect("a tip step moves");
    let landing = chained.landing().expect("a visit carries one");
    assert!(
        (chained.pose_at(0.0).apply(landing.reaction_tool) - landing.standoff_point()).length()
            < 1e-9
    );
}

#[test]
fn a_visit_inside_a_run_ends_at_the_next_visits_standoff_and_the_last_goes_home() {
    // The end of step 1 is the standoff of step 3 — the same number computed the
    // same way, which is what makes the step boundary continuous.
    let (_, first) = at(1, 1.0, &[]);
    let (_, next) = at(3, 0.0, &[]);
    let (first, next) = (first.expect("moves"), next.expect("moves"));
    assert!((first.pose_at(1.0).t - next.pose_at(0.0).t).length() < 1e-9);
    assert!(
        (first.pose_at(1.0).r.col(0) - next.pose_at(0.0).r.col(0)).length() < 1e-9,
        "the orientation is continuous across the boundary too"
    );

    // The run's last visit goes home.
    let (scene, last) = at(4, 1.0, &[]);
    let last = last.expect("moves");
    assert_pose_close(
        last.pose_at(1.0),
        park_of(&scene, last.tool()),
        "home again",
    );
}

#[test]
fn a_tool_hovers_over_its_next_site_through_a_settle() {
    let (_, hover) = at(2, 0.3, &[]);
    let hover = hover.expect("a settle inside a run is hovered through");
    assert!(matches!(hover, ToolMotion::Hover { .. }));
    assert_eq!(hover.tool(), 0);

    // It arrived there at the end of the previous visit and leaves at the start
    // of the next, so all three poses are one pose.
    let (_, previous) = at(1, 1.0, &[]);
    let (_, following) = at(3, 0.0, &[]);
    let previous = previous.expect("moves").pose_at(1.0);
    let following = following.expect("moves").pose_at(0.0);
    for u in [0.0, 0.3, 0.5, 1.0] {
        assert!((hover.pose_at(u).t - previous.t).length() < 1e-9, "u = {u}");
        assert!(
            (hover.pose_at(u).t - following.t).length() < 1e-9,
            "u = {u}"
        );
    }
}

#[test]
fn a_bulk_step_moves_nothing() {
    let (_, motion) = at(6, 0.5, &[]);
    assert!(motion.is_none());
}

#[test]
fn the_tool_dwells_at_the_reaction_pose_across_the_middle() {
    let (_, motion) = at(1, REACTION, &[]);
    let motion = motion.expect("moves");
    let landing = motion.landing().expect("a visit carries one");
    for u in [REACTION_LANDING, 0.48, REACTION, 0.52, REACTION_DEPARTURE] {
        let pose = motion.pose_at(u);
        assert!(
            (pose.apply(landing.reaction_tool) - landing.reaction_point).length() < 1e-9,
            "u = {u}"
        );
    }
}

#[test]
fn the_pose_is_continuous_in_step_time() {
    let (_, motion) = at(1, 0.0, &[]);
    let motion = motion.expect("moves");
    let steps = 1000;
    let mut previous = motion.pose_at(0.0);
    for sample in 1..=steps {
        let pose = motion.pose_at(sample as f64 / steps as f64);
        assert!(
            (pose.t - previous.t).length() < atomcad_crystolecule::mechanosynth::PATH_SAMPLE,
            "a jump at sample {sample}"
        );
        previous = pose;
    }
}

#[test]
fn a_failing_look_ahead_sends_the_tool_home_instead() {
    // Break the run's next visit — the recharge — by taking the reservoir away.
    // The tool has nowhere to fly to, so it parks; the failure is reported when
    // the replay reaches that step, not here.
    let lib = library("tool_ops.json");
    let build = script("trajectory_build.json");
    let (scene, motion) = replay_scene_at(
        &molecule("tool_scene.xyz"),
        &[],
        &[tip(), probe()],
        &lib,
        &build,
        1,
        1.0,
        HighlightTags::default(),
    )
    .expect("step 1 itself is fine");
    let motion = motion.expect("moves");
    assert_pose_close(
        motion.pose_at(1.0),
        park_of(&scene, motion.tool()),
        "home instead",
    );

    // …and the replay does fail when it reaches the recharge.
    let failed = replay_scene_at(
        &molecule("tool_scene.xyz"),
        &[],
        &[tip(), probe()],
        &lib,
        &build,
        3,
        1.0,
        HighlightTags::default(),
    );
    assert!(failed.is_err());
}

#[test]
fn a_blocked_site_is_still_visited_and_the_motion_carries_the_verdict() {
    // The replay describes the build; it does not gate it. So the tool visits
    // along the sweep's best direction, the motion is as continuous as any
    // other, and the negative clearance is what says the site is blocked.
    let boxed_in = cage_around(DVec3::new(0.0, 0.0, 1.09), 3.0);
    let (_, motion) = at(1, 0.3, &boxed_in);
    let motion = motion.expect("a blocked site is visited all the same");
    let landing = motion.landing().expect("a visit carries one");
    assert!(!landing.reachable(), "{:?}", landing.approach);

    let steps = 500;
    let mut previous = motion.pose_at(0.0);
    for sample in 1..=steps {
        let pose = motion.pose_at(sample as f64 / steps as f64);
        assert!(
            (pose.t - previous.t).length() < atomcad_crystolecule::mechanosynth::PATH_SAMPLE,
            "a jump at sample {sample}"
        );
        previous = pose;
    }
    // And it really does land on the site.
    let reaction = motion.pose_at(REACTION);
    assert!((reaction.apply(landing.reaction_tool) - landing.reaction_point).length() < 1e-9);
}

// ============================================================================
// Presentation — the scan
// ============================================================================

#[test]
fn a_clear_visit_scans_clear_and_an_obstacle_on_the_line_is_reported() {
    let (_, clear) = at(1, 0.5, &[]);
    let scan = clear.expect("moves").scan().expect("a visit scans").clone();
    let clear_ratio = scan.worst.map_or(f64::INFINITY, |contact| contact.ratio);
    assert!(clear_ratio >= CLASH_BLOCK, "{scan:?}");

    // Something on the flight line: the tip parks at (0, 30, 0) and flies in to
    // a standoff over the origin, so the midpoint of that line is in the way —
    // and it is far enough from the site not to tilt the approach or to touch
    // the descent, which is vertical.
    let slab = obstacle_over(flight_midpoint());
    let (_, blocked) = at(1, 0.5, &[slab]);
    let scan = blocked.expect("moves").scan().expect("scans").clone();
    let contact = scan.worst.expect("the slab is reported");
    assert!(contact.ratio < CLASH_BLOCK, "{contact:?}");
    assert!(contact.at < REACTION_LANDING, "on the inbound flight");
}

#[test]
fn a_chained_visits_inbound_scan_covers_only_its_descent() {
    // Step 3 flies in from nowhere — it is already at its standoff — so what
    // catches step 1's flight cannot catch step 3's first half.
    let slab = obstacle_over(flight_midpoint());
    let (_, motion) = at(3, 0.5, &[slab]);
    let scan = motion.expect("moves").scan().expect("scans").clone();
    if let Some(contact) = scan.worst {
        assert!(
            contact.at >= REACTION_LANDING || contact.ratio >= CLASH_BLOCK,
            "a chained visit has no inbound flight to collide on: {contact:?}"
        );
    }
}

// ============================================================================
// The compatibility assertion
// ============================================================================

#[test]
fn the_scene_at_a_finished_step_is_milestone_ones_exactly() {
    // The engine never moves a scene, so `replay_scene_at(k, 1.0)` is
    // `replay_scene(k)` atom for atom — tools included. What a *viewer* sees
    // differs inside a run, because the node applies the motion; the engine's
    // scene does not.
    let lib = library("tool_ops.json");
    for name in ["trajectory_build.json", "tool_build.json"] {
        let build = script(name);
        for step in 0..=build.steps.len() as i32 {
            let expected = replay_scene(
                &molecule("tool_scene.xyz"),
                &[molecule("tool_dump.xyz")],
                &[tip(), probe()],
                &lib,
                &build,
                step,
                HighlightTags::default(),
            )
            .expect("milestone 1 replays");
            let (actual, _) = at_in(name, step, 1.0, &[]);
            assert_structures_match(&expected, &actual, &format!("{name} step {step}"));
        }
    }
}

#[test]
fn before_the_reaction_the_workpiece_is_the_previous_steps() {
    let lib = library("tool_ops.json");
    let build = script("trajectory_build.json");
    let milestone_one = |step: i32| {
        replay_scene(
            &molecule("tool_scene.xyz"),
            &[molecule("tool_dump.xyz")],
            &[tip(), probe()],
            &lib,
            &build,
            step,
            HighlightTags::default(),
        )
        .expect("replays")
    };

    for step in 1..=build.steps.len() as i32 {
        for time in [0.0, 0.2, 0.49] {
            let (scene, _) = at(step, time, &[]);
            assert_structures_match(
                &milestone_one(step - 1),
                &scene,
                &format!("step {step} at {time}"),
            );
        }
        for time in [REACTION, 0.8, 1.0] {
            let (scene, _) = at(step, time, &[]);
            assert_structures_match(
                &milestone_one(step),
                &scene,
                &format!("step {step} at {time}"),
            );
        }
    }
}

fn assert_structures_match(expected: &Scene, actual: &Scene, what: &str) {
    let mut left: Vec<(u32, i16, [i64; 3])> = expected
        .structure
        .iter_atoms()
        .map(|(id, atom)| (*id, atom.atomic_number, quantise(atom.position)))
        .collect();
    let mut right: Vec<(u32, i16, [i64; 3])> = actual
        .structure
        .iter_atoms()
        .map(|(id, atom)| (*id, atom.atomic_number, quantise(atom.position)))
        .collect();
    left.sort_unstable();
    right.sort_unstable();
    assert_eq!(left, right, "{what}");
}

/// Positions to a tenth of a picometre, so a comparison is exact without being
/// a float equality.
fn quantise(position: DVec3) -> [i64; 3] {
    [
        (position.x * 1e7).round() as i64,
        (position.y * 1e7).round() as i64,
        (position.z * 1e7).round() as i64,
    ]
}

#[test]
fn applying_a_pose_moves_the_tool_rigidly_and_leaves_everything_else_alone() {
    let (scene, motion) = at(1, 0.2, &[]);
    let motion = motion.expect("moves");
    let mut shown = scene.structure.clone();
    apply_tool_pose(&mut shown, &scene, motion.tool(), &motion.pose_at(0.2));

    let tool = Participant::Tool(scene.bindings[motion.tool()].instance);
    let tool_ids: Vec<u32> = scene
        .participants
        .iter()
        .filter(|(_, participant)| **participant == tool)
        .map(|(atom_id, _)| *atom_id)
        .collect();

    // Nothing that is not the tool moved, and nothing at all was added or lost.
    assert_eq!(
        shown.iter_atoms().count(),
        scene.structure.iter_atoms().count()
    );
    for (atom_id, atom) in scene.structure.iter_atoms() {
        let moved = shown.get_atom(*atom_id).expect("still there");
        assert_eq!(moved.atomic_number, atom.atomic_number);
        assert_eq!(moved.bonds.len(), atom.bonds.len());
        if !tool_ids.contains(atom_id) {
            assert!(
                (moved.position - atom.position).length() < 1e-12,
                "{atom_id}"
            );
        }
    }

    // The tool moved rigidly: every pairwise distance is what it was.
    for pair in tool_ids.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        let before = scene
            .structure
            .get_atom(a)
            .unwrap()
            .position
            .distance(scene.structure.get_atom(b).unwrap().position);
        let after = shown
            .get_atom(a)
            .unwrap()
            .position
            .distance(shown.get_atom(b).unwrap().position);
        assert!((before - after).abs() < 1e-9);
    }

    // And it really did move.
    let apex = tool_ids
        .iter()
        .copied()
        .find(|atom_id| scene.structure.atom_has_tag(*atom_id, APEX_FRAME_TAG))
        .expect("the apex is tagged");
    assert!(
        shown
            .get_atom(apex)
            .unwrap()
            .position
            .distance(scene.structure.get_atom(apex).unwrap().position)
            > 1e-3
    );
}

#[test]
fn the_current_highlight_is_the_matched_site_before_the_reaction() {
    let lib = library("tool_ops.json");
    let build = script("trajectory_build.json");
    let replay_at = |time: f64| {
        replay_scene_at(
            &molecule("tool_scene.xyz"),
            &[molecule("tool_dump.xyz")],
            &[tip(), probe()],
            &lib,
            &build,
            1,
            time,
            HighlightTags {
                current: Some("ms_current"),
                ..HighlightTags::default()
            },
        )
        .expect("replays")
        .0
    };

    // `habst` takes one hydrogen with a three-atom tool side, so before the
    // reaction four atoms light up: the site and the apex that will react.
    let approaching = replay_at(0.2);
    let lit = approaching.structure.atoms_with_tag("ms_current");
    assert_eq!(lit.len(), 4, "the matched before atoms of both sides");

    // From the reaction on it is the step's `touched`, which no longer includes
    // the hydrogen, because that atom is gone.
    let reacted = replay_at(0.8);
    let lit = reacted.structure.atoms_with_tag("ms_current");
    assert!(!lit.is_empty());
    assert!(lit.len() < 4);
}

#[test]
fn a_step_that_cannot_match_is_an_error_at_every_time() {
    // The step is selected, and its checks are what selecting it means — so the
    // failure is the same at `u = 0` as at `u = 1`, and the partial state is
    // reachable by asking for one step fewer.
    let lib = library("tool_ops.json");
    let mut build = empty_script();
    build.steps = vec![
        Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
        Step::new("habst", DVec3::new(0.0, 0.0, 50.0)),
    ];
    for time in [0.0, 0.3, 0.7, 1.0] {
        let outcome = replay_scene_at(
            &molecule("tool_scene.xyz"),
            &[molecule("tool_dump.xyz")],
            &[tip(), probe()],
            &lib,
            &build,
            2,
            time,
            HighlightTags::default(),
        );
        assert!(outcome.is_err(), "time {time}");
    }
    // One step fewer is fine.
    let (_, _) = at(1, 1.0, &[]);
}

// ============================================================================
// The legs, in words
// ============================================================================

/// Every leg the motion passes through as `u` runs from 0 to 1, consecutive
/// duplicates collapsed — the sequence a panel readout walks while the slider
/// is dragged.
fn legs_over(motion: &ToolMotion) -> Vec<Leg> {
    let mut seen: Vec<Leg> = Vec::new();
    for sample in 0..=1000 {
        let leg = motion.leg_at(sample as f64 / 1000.0);
        if seen.last() != Some(&leg) {
            seen.push(leg);
        }
    }
    seen
}

#[test]
fn a_first_visit_with_another_coming_names_its_six_legs_in_order() {
    // Step 1 is `habst_tool`'s first visit, and its run continues at step 3, so
    // this one visit covers both flights that are not a return home.
    let (_, motion) = at(1, 0.0, &[]);
    let motion = motion.expect("a tip step with a bound tool visits");
    assert_eq!(
        legs_over(&motion),
        vec![
            Leg::FlyingFromPark,
            Leg::Descending,
            Leg::AtSiteBefore,
            Leg::AtSiteReacted,
            Leg::Ascending,
            Leg::FlyingToNextSite,
        ]
    );
    // The words are the panel's, and the dwell pair is what the reaction at the
    // middle of the dwell buys: a landed-but-unreacted frame and a
    // reacted-but-not-departed one.
    assert_eq!(motion.leg_at(REACTION_LANDING).as_str(), "at site (before)");
    assert_eq!(motion.leg_at(REACTION).as_str(), "at site (reacted)");
    assert_eq!(
        motion.leg_at(REACTION_DEPARTURE).as_str(),
        "at site (reacted)"
    );
}

#[test]
fn a_lone_visit_flies_out_from_park_and_returns_to_it() {
    // Step 5 is the bare probe's whole run: one visit, out and back.
    let (_, motion) = at(5, 0.0, &[]);
    let motion = motion.expect("the probe visits");
    assert_eq!(
        legs_over(&motion),
        vec![
            Leg::FlyingFromPark,
            Leg::Descending,
            Leg::AtSiteBefore,
            Leg::AtSiteReacted,
            Leg::Ascending,
            Leg::ReturningToPark,
        ]
    );
    assert_eq!(motion.leg_at(1.0).as_str(), "returning to park");
}

#[test]
fn a_chained_visit_spends_its_whole_inbound_half_descending() {
    // Step 3 is reached without going home, so there is no flight in: the tool
    // is already at its standoff when the step begins.
    let (_, motion) = at(3, 0.0, &[]);
    let motion = motion.expect("the run's second visit");
    assert_eq!(
        legs_over(&motion),
        vec![
            Leg::Descending,
            Leg::AtSiteBefore,
            Leg::AtSiteReacted,
            Leg::Ascending,
            Leg::FlyingToNextSite,
        ],
        "a chained visit never flies from park"
    );
}

#[test]
fn a_hover_is_one_leg_at_every_time() {
    // Step 2 is the `settle` inside the run: the tool waits, so every `u` of it
    // reports the same thing.
    let (_, motion) = at(2, 0.0, &[]);
    let motion = motion.expect("a settle inside a run is a hover");
    assert_eq!(legs_over(&motion), vec![Leg::Hovering]);
    assert_eq!(motion.leg_at(0.5).as_str(), "hovering over next site");
}

#[test]
fn the_leg_and_the_pose_agree_about_where_the_tool_is() {
    // The two read the same length split, and the readout would be a lie if
    // they ever disagreed: at the moment the leg becomes the descent, the tool
    // has to be at its standoff.
    let (_, motion) = at(1, 0.0, &[]);
    let motion = motion.expect("a visit");
    let visit = match &motion {
        ToolMotion::Visit(visit) => visit,
        ToolMotion::Hover { .. } => unreachable!("step 1 is a tip step"),
    };
    let first_descent = (0..=1000)
        .map(|sample| sample as f64 / 1000.0)
        .find(|u| motion.leg_at(*u) == Leg::Descending)
        .expect("the visit descends");
    assert!(
        (motion.pose_at(first_descent).t - visit.standoff.t).length() < 0.2,
        "the descent begins at the standoff"
    );
}
