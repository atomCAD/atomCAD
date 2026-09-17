//! The presentation layer: where a tool actually is at a point of step time.
//!
//! Everything here is built **on top of** the feasibility layer's output — a
//! [`Landing`] and the run structure — and nothing here can fail. The scan and
//! the blocked approach are reports, not errors: a collision on a flight is the
//! user's layout and the fix is a re-park they can see at once, and a blocked
//! site is the *generator's* business to refuse, not the viewer's to stop over.
//!
//! **The engine's [`Scene`] is never moved.** A `Scene` always has its tools at
//! their bound poses, because that is what the tool-side match and the sweep
//! assume. [`replay_scene_at`] returns the scene *and* a [`ToolMotion`], and
//! whoever draws it applies the pose to its own copy with [`apply_tool_pose`] —
//! so there is no "do not replay into a moved scene" rule to remember, because
//! there is no moved scene to replay into.
//!
//! Design doc: `doc/design_mechanosynth_trajectory.md` §The path, §Presentation.

use super::landing::Landing;
use super::runs::{Runs, runs};
use crate::atomic_structure::AtomicStructure;
use crate::mechanosynth::apply::{HighlightTags, covalent_radius, paint, resolve_tolerance};
use crate::mechanosynth::scene::{
    LandingPlan, Participant, Scene, StepPlan, apply_plan, build_scene, match_step_in_scene,
    replay_steps,
};
use crate::mechanosynth::schema::{
    BuildScript, CLASH_SEARCH_RADIUS, MechanosynthError, Method, OpLibrary, Step,
};
use glam::{DMat3, DQuat, DVec3};

/// Step time at which the tool reaches the reaction pose.
pub const REACTION_LANDING: f64 = 0.45;
/// Step time at which the rewrite happens — the middle of the dwell, and the
/// switch from *before* to *after* for **every** kind of step.
pub const REACTION: f64 = 0.5;
/// Step time at which the tool starts to ascend.
pub const REACTION_DEPARTURE: f64 = 0.55;
/// How far apart the path scan samples a leg, Å of translation.
pub const PATH_SAMPLE: f64 = 0.5;
/// …and degrees of rotation.
pub const PATH_SAMPLE_ANGLE: f64 = 5.0;

/// A rigid pose of a tool's local frame in design space: `p = r · p_tool + t`.
///
/// [`ToolPose`](crate::mechanosynth::ToolPose) without the residual — a pose the
/// engine *computed* rather than one it solved from tagged atoms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub r: DMat3,
    pub t: DVec3,
}

impl Pose {
    /// Where this pose puts a point of the tool's local frame.
    pub fn apply(&self, local: DVec3) -> DVec3 {
        self.r * local + self.t
    }

    /// The pose a fraction `u` of the way from `self` to `other`: the
    /// translation lerped, the orientation slerped along the shorter arc.
    fn blend(&self, other: &Pose, u: f64) -> Pose {
        let from = DQuat::from_mat3(&self.r);
        let to = DQuat::from_mat3(&other.r);
        Pose {
            r: DMat3::from_quat(from.slerp(to, u)),
            t: self.t.lerp(other.t, u),
        }
    }

    /// The angle between two orientations, radians — what the scan spends its
    /// rotational samples on.
    fn angle_to(&self, other: &Pose) -> f64 {
        DQuat::from_mat3(&self.r)
            .angle_between(DQuat::from_mat3(&other.r))
            .abs()
    }
}

/// The full reaction pose of a landing: **the parked tool turned by the single
/// smallest rotation that points its axis down the approach.**
///
/// `R = ΔR · R_park` with `ΔR` the smallest rotation taking the *parked* tool's
/// axis onto the approach direction, and `t = p_r − R · reaction.tool` so that
/// the two reaction points coincide. The roll about the tool axis is free — the
/// envelope is a solid of revolution, so roll cannot matter to collisions — and
/// it is spent staying as close to the pose the designer authored as the
/// approach allows.
///
/// **Memoryless, deliberately.** The first implementation folded each visit's
/// minimal rotation onto the orientation the tool *arrived* with, chained from
/// park through every earlier visit of the run. That made a visit's orientation
/// a function of the whole path taken to reach it, so drawing step `k` meant
/// sweeping every earlier visit of its run — and a sweep is the most expensive
/// thing in the replay. Measured on the silicon demo at step 49: forty sweeps
/// planned, eleven read, 14.6 ms of a 25 ms evaluation. Reading the orientation
/// off the approach direction alone costs **two** sweeps per step (this visit's
/// and the next one's) however long the run is.
///
/// Three things the park-relative rule buys beyond the speed:
///
/// - **continuity across the step boundary is structural.** The standoff a tool
///   flies to at the end of step `k` and the one it descends from at step `j`
///   are now the same function of the same landing, rather than two folds that
///   have to agree;
/// - **the roll cannot drift.** Composed minimal rotations are not the minimal
///   rotation of the composition, so the fold accumulated holonomy over a long
///   run; this never strays further from the authored pose than one rotation;
/// - **a vertical approach from an upright park is the identity**, so the tool
///   is drawn exactly as it was posed.
///
/// It still does not twirl: `ΔR(axis_park → ·)` is continuous in the approach
/// direction, so two visits with similar approaches get similar orientations.
pub fn reaction_pose(landing: &Landing, park: &Pose) -> Pose {
    let parked_axis = (park.r * landing.axis).normalize();
    let turn = DMat3::from_quat(minimal_turn(parked_axis, landing.approach.direction, park));
    let r = turn * park.r;
    Pose {
        r,
        t: landing.reaction_point - r * landing.reaction_tool,
    }
}

/// The smallest rotation taking `from` onto `to`, with the antiparallel case
/// pinned down instead of left to chance.
///
/// `DQuat::from_rotation_arc` has no determined axis when the two are opposite —
/// every axis in the perpendicular plane is a valid half-turn — and picks one
/// arbitrarily. That would make a tool's roll jump as its approach crossed the
/// antipode of its parked axis. The case is a tool parked pointing exactly away
/// from the direction it approaches along, which a sane layout never produces;
/// pinning the axis to the parked frame's `x` makes it deterministic and keeps
/// it a property of the binding rather than of floating-point luck.
fn minimal_turn(from: DVec3, to: DVec3, park: &Pose) -> DQuat {
    if from.dot(to) < -1.0 + ANTIPARALLEL_EPSILON {
        let axis = (park.r * DVec3::X).normalize();
        // Guard the pathological frame whose `x` is itself along the axis.
        let axis = if axis.cross(from).length() < 1e-9 {
            (park.r * DVec3::Y).normalize()
        } else {
            axis
        };
        return DQuat::from_axis_angle(axis, std::f64::consts::PI);
    }
    DQuat::from_rotation_arc(from, to)
}

/// How close to antiparallel the parked axis and the approach must be before
/// [`minimal_turn`] stops trusting the arc rotation's axis.
const ANTIPARALLEL_EPSILON: f64 = 1e-9;

/// The reaction pose lifted to the standoff, up the approach.
pub fn standoff_pose(landing: &Landing, reaction: &Pose) -> Pose {
    Pose {
        r: reaction.r,
        t: reaction.t + landing.approach.direction * landing.standoff_height,
    }
}

/// Which part of its visit a tool is on at a step time — the panel's readout,
/// and the one place the leg structure of a [`Visit`] is named in words.
///
/// The two dwell values are what the reaction at the middle of the dwell buys:
/// a landed-but-unreacted frame and a reacted-but-not-departed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    FlyingFromPark,
    Descending,
    AtSiteBefore,
    AtSiteReacted,
    Ascending,
    FlyingToNextSite,
    ReturningToPark,
    /// Not a leg of a visit at all: a tool waiting over its next site through a
    /// `spontaneous` step.
    Hovering,
}

impl Leg {
    /// The panel's words for this leg.
    pub fn as_str(&self) -> &'static str {
        match self {
            Leg::FlyingFromPark => "flying from park",
            Leg::Descending => "descending",
            Leg::AtSiteBefore => "at site (before)",
            Leg::AtSiteReacted => "at site (reacted)",
            Leg::Ascending => "ascending",
            Leg::FlyingToNextSite => "flying to next site",
            Leg::ReturningToPark => "returning to park",
            Leg::Hovering => "hovering over next site",
        }
    }
}

/// One tool's visit to one site, for one `tip` step.
#[derive(Debug, Clone)]
pub struct Visit {
    pub landing: Landing,
    /// The binding's pose — where the tool lives between runs.
    pub park: Pose,
    pub reaction: Pose,
    pub standoff: Pose,
    /// `false` when the tool already hovers at its standoff — a chained visit,
    /// whose whole inbound half is one descent.
    pub from_park: bool,
    /// Where the outbound flight ends: the next visit's standoff, or park.
    pub to: Pose,
    pub to_park: bool,
    /// The scan over every leg, the descent and the ascent included.
    pub scan: PathScan,
}

/// What a tool does during a step, if anything.
#[derive(Debug, Clone)]
pub enum ToolMotion {
    /// A `tip` step's visit.
    Visit(Box<Visit>),
    /// A tool between two visits of one run, waiting over its next site through
    /// a `spontaneous` step.
    Hover { tool: usize, pose: Pose },
}

impl ToolMotion {
    /// Which binding is away from park. At most one ever is.
    pub fn tool(&self) -> usize {
        match self {
            ToolMotion::Visit(visit) => visit.landing.tool,
            ToolMotion::Hover { tool, .. } => *tool,
        }
    }

    /// The landing behind this motion, when there is one.
    pub fn landing(&self) -> Option<&Landing> {
        match self {
            ToolMotion::Visit(visit) => Some(&visit.landing),
            ToolMotion::Hover { .. } => None,
        }
    }

    /// The scan of this motion's legs; a hover moves nothing and scans nothing.
    pub fn scan(&self) -> Option<&PathScan> {
        match self {
            ToolMotion::Visit(visit) => Some(&visit.scan),
            ToolMotion::Hover { .. } => None,
        }
    }

    /// The tool's pose at step time `u`, clamped to `[0, 1]`.
    ///
    /// The reaction pose throughout the dwell, the hover pose at every `u` of a
    /// hover. Continuous in `u`, and continuous **across the step boundary
    /// inside a run**: the standoff a tool flies to at the end of one step and
    /// the standoff it descends from at the next are the same number computed
    /// the same way.
    pub fn pose_at(&self, u: f64) -> Pose {
        match self {
            ToolMotion::Hover { pose, .. } => *pose,
            ToolMotion::Visit(visit) => visit.pose_at(u),
        }
    }

    /// Which leg of the motion step time `u` falls on — the same split by path
    /// length [`Self::pose_at`] interpolates along, so the word and the pose
    /// always agree.
    pub fn leg_at(&self, u: f64) -> Leg {
        match self {
            ToolMotion::Hover { .. } => Leg::Hovering,
            ToolMotion::Visit(visit) => visit.leg_at(u),
        }
    }
}

impl Visit {
    /// The inbound legs, in order: the flight in from park (absent on a chained
    /// visit) and the descent.
    fn inbound(&self) -> Vec<(Pose, Pose)> {
        let mut legs = Vec::with_capacity(2);
        if self.from_park {
            legs.push((self.park, self.standoff));
        }
        legs.push((self.standoff, self.reaction));
        legs
    }

    /// The outbound legs, in order: the ascent and the flight out.
    fn outbound(&self) -> Vec<(Pose, Pose)> {
        vec![(self.reaction, self.standoff), (self.standoff, self.to)]
    }

    fn pose_at(&self, u: f64) -> Pose {
        let u = u.clamp(0.0, 1.0);
        if (REACTION_LANDING..=REACTION_DEPARTURE).contains(&u) {
            return self.reaction;
        }
        if u < REACTION_LANDING {
            pose_along(&self.inbound(), u / REACTION_LANDING)
        } else {
            pose_along(
                &self.outbound(),
                (u - REACTION_DEPARTURE) / (1.0 - REACTION_DEPARTURE),
            )
        }
    }

    /// The leg `u` falls on, decided by the same length split as
    /// [`Self::pose_at`]. A half whose legs all have zero length — a tool parked
    /// exactly at its standoff — reports the leg it ends on, which is where
    /// `pose_at` leaves it.
    fn leg_at(&self, u: f64) -> Leg {
        let u = u.clamp(0.0, 1.0);
        if (REACTION_LANDING..REACTION).contains(&u) {
            return Leg::AtSiteBefore;
        }
        if (REACTION..=REACTION_DEPARTURE).contains(&u) {
            return Leg::AtSiteReacted;
        }
        if u < REACTION_LANDING {
            let legs = self.inbound();
            let flight = self.from_park
                && leg_at(&leg_lengths(&legs), u / REACTION_LANDING)
                    .is_some_and(|(index, _, _)| index == 0);
            if flight {
                Leg::FlyingFromPark
            } else {
                Leg::Descending
            }
        } else {
            let legs = self.outbound();
            let f = (u - REACTION_DEPARTURE) / (1.0 - REACTION_DEPARTURE);
            let ascending = leg_at(&leg_lengths(&legs), f).is_some_and(|(index, _, _)| index == 0);
            if ascending {
                Leg::Ascending
            } else if self.to_park {
                Leg::ReturningToPark
            } else {
                Leg::FlyingToNextSite
            }
        }
    }
}

/// The lengths of a chain of legs, which is how the time within a half is
/// shared out: at one speed, so a long flight takes longer than a short one.
fn leg_lengths(legs: &[(Pose, Pose)]) -> Vec<f64> {
    legs.iter()
        .map(|(from, to)| (to.t - from.t).length())
        .collect()
}

/// Which leg a fraction `f ∈ [0, 1]` of a half falls on, with the share of that
/// half the leg starts and ends at.
///
/// A leg of zero length — a tool parked exactly at its standoff — gets no time and
/// is never returned; `None` is a chain with no length at all, which is a tool
/// that does not move within the half.
fn leg_at(lengths: &[f64], f: f64) -> Option<(usize, f64, f64)> {
    let total: f64 = lengths.iter().sum();
    if total <= f64::EPSILON {
        return None;
    }
    let mut travelled = 0.0;
    for (index, length) in lengths.iter().enumerate() {
        if *length <= f64::EPSILON {
            continue;
        }
        let start = travelled / total;
        travelled += length;
        let end = travelled / total;
        if f <= end || end >= 1.0 {
            return Some((index, start, end));
        }
    }
    None
}

/// The pose a fraction `f ∈ [0, 1]` of the way along a chain of legs, time shared
/// **by path length** so the tool moves at one speed within a half, and each leg
/// eased with a smoothstep so the corner at the standoff does not snap.
///
/// A leg of zero length — a tool parked exactly at its standoff — gets no time.
fn pose_along(legs: &[(Pose, Pose)], f: f64) -> Pose {
    let Some((_, last_to)) = legs.last() else {
        // Cannot happen: every half has at least one leg.
        return Pose {
            r: DMat3::IDENTITY,
            t: DVec3::ZERO,
        };
    };

    match leg_at(&leg_lengths(legs), f) {
        Some((index, start, end)) => {
            let (from, to) = legs[index];
            let local = ((f - start) / (end - start)).clamp(0.0, 1.0);
            from.blend(&to, smoothstep(local))
        }
        None => *last_to,
    }
}

/// `3u² − 2u³`: zero slope at both ends, so two legs meet without a corner.
fn smoothstep(u: f64) -> f64 {
    u * u * (3.0 - 2.0 * u)
}

/// The scan of a visit's legs.
///
/// Its own type, not [`Contact`](crate::mechanosynth::Contact), whose fields
/// describe a placed pattern atom against a host.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathScan {
    /// The worst pair along the legs, or `None` when no non-tool atom came
    /// within [`CLASH_SEARCH_RADIUS`] of any tool atom at any sample.
    pub worst: Option<PathContact>,
}

/// The closest a moving tool came to something that is not it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathContact {
    /// Centre-to-centre distance over the sum of the two covalent radii. Below
    /// the library's clash factor the visit *collides*.
    pub ratio: f64,
    pub distance: f64,
    /// Step time of the sample, in `[0, 1]`.
    pub at: f64,
    pub tool_atom: u32,
    pub other: u32,
}

impl PathContact {
    /// `O of si_tool against Si 481, ratio 0.62` — the panel's words, without
    /// the leading clause.
    pub fn describe(&self, scene: &Scene) -> String {
        let element = |atom_id: u32| {
            scene.structure.get_atom(atom_id).map_or_else(
                || "?".to_string(),
                |atom| crate::atomic_constants::element_symbol(atom.atomic_number),
            )
        };
        format!(
            "{} of {} against {} {}, ratio {:.2}",
            element(self.tool_atom),
            scene.label(scene.participant(self.tool_atom)),
            element(self.other),
            self.other,
            self.ratio
        )
    }
}

/// Writes a tool's atoms into `structure` — a copy of `scene.structure` the
/// caller owns — at `pose` instead of at the bound pose:
/// `p' = R · R_parkᵀ · (p − t_park) + t`, through `set_atom_position` so the
/// spatial grid follows. Bonds, ids and tags are untouched.
///
/// The [`Scene`] itself is never moved; this is how a *drawing* of one differs
/// from it.
pub fn apply_tool_pose(structure: &mut AtomicStructure, scene: &Scene, tool: usize, pose: &Pose) {
    let Some(binding) = scene.bindings.get(tool) else {
        return;
    };
    let inverse = binding.pose.r.transpose();
    let park = binding.pose.t;
    for atom_id in tool_atoms(scene, tool) {
        let Some(atom) = structure.get_atom(atom_id) else {
            continue;
        };
        let local = inverse * (atom.position - park);
        structure.set_atom_position(atom_id, pose.apply(local));
    }
}

/// Every live scene atom belonging to one bound tool.
fn tool_atoms(scene: &Scene, tool: usize) -> Vec<u32> {
    let Some(binding) = scene.bindings.get(tool) else {
        return Vec::new();
    };
    let participant = Participant::Tool(binding.instance);
    let mut atoms: Vec<u32> = scene
        .participants
        .iter()
        .filter(|(atom_id, belongs)| {
            **belongs == participant && scene.structure.get_atom(**atom_id).is_some()
        })
        .map(|(atom_id, _)| *atom_id)
        .collect();
    // Sorted so that a reported contact names the same atom every time.
    atoms.sort_unstable();
    atoms
}

/// Scans a visit's legs for contact.
///
/// Nothing is routed: every leg — the inbound flight, the descent, the ascent
/// and the outbound flight — is sampled every [`PATH_SAMPLE`] of translation and
/// [`PATH_SAMPLE_ANGLE`] of rotation, and each tool atom is checked against the
/// scene atoms within [`CLASH_SEARCH_RADIUS`] that are neither the tool's nor
/// the target side's matched `before` atoms.
///
/// **The descent is scanned too.** A reachable descent is collision-free by the
/// envelope's account — the envelope translated up its own axis lies inside the
/// envelope at the reaction point, and the sweep cleared that — but the envelope
/// is the library's claim and the molecule is the fact.
fn scan_visit(scene: &Scene, visit: &Visit, exclude: &[u32]) -> PathScan {
    let atoms = tool_atoms(scene, visit.landing.tool);
    let bound = &scene.bindings[visit.landing.tool];
    let inverse = bound.pose.r.transpose();
    let locals: Vec<(u32, DVec3, f64)> = atoms
        .iter()
        .filter_map(|atom_id| {
            scene.structure.get_atom(*atom_id).map(|atom| {
                (
                    *atom_id,
                    inverse * (atom.position - bound.pose.t),
                    covalent_radius(atom.atomic_number),
                )
            })
        })
        .collect();

    let mut worst: Option<PathContact> = None;
    let mut consider = |pose: &Pose, at: f64| {
        for (tool_atom, local, tool_radius) in &locals {
            let position = pose.apply(*local);
            for other in scene
                .structure
                .get_atoms_in_radius(&position, CLASH_SEARCH_RADIUS)
            {
                if atoms.binary_search(&other).is_ok() || exclude.contains(&other) {
                    continue;
                }
                let Some(atom) = scene.structure.get_atom(other) else {
                    continue;
                };
                let distance = position.distance(atom.position);
                let sum = tool_radius + covalent_radius(atom.atomic_number);
                let ratio = distance / sum;
                if worst.is_some_and(|contact| contact.ratio <= ratio) {
                    continue;
                }
                worst = Some(PathContact {
                    ratio,
                    distance,
                    at,
                    tool_atom: *tool_atom,
                    other,
                });
            }
        }
    };

    let halves = [
        (visit.inbound(), 0.0, REACTION_LANDING),
        (visit.outbound(), REACTION_DEPARTURE, 1.0),
    ];
    for (legs, half_start, half_end) in halves {
        let lengths: Vec<f64> = legs
            .iter()
            .map(|(from, to)| (to.t - from.t).length())
            .collect();
        let total: f64 = lengths.iter().sum();
        let mut travelled = 0.0;
        for ((from, to), length) in legs.iter().zip(&lengths) {
            let turn = from.angle_to(to).to_degrees();
            let samples = ((length / PATH_SAMPLE).ceil() as usize)
                .max((turn / PATH_SAMPLE_ANGLE).ceil() as usize)
                .max(1);
            let start = if total > f64::EPSILON {
                travelled / total
            } else {
                0.0
            };
            travelled += length;
            let end = if total > f64::EPSILON {
                travelled / total
            } else {
                1.0
            };
            for sample in 0..=samples {
                let local = sample as f64 / samples as f64;
                let pose = from.blend(to, smoothstep(local));
                let at = half_start + (half_end - half_start) * (start + (end - start) * local);
                consider(&pose, at);
            }
        }
    }

    PathScan { worst }
}

// ===========================================================================
// The replay at a step time
// ===========================================================================

/// The scene at `(step, time)` — tools at their **bound** poses, as always — and
/// what its tool is doing.
///
/// `result` semantics: the workpiece is the one after `step − 1` for
/// `time < 0.5` and the one after `step` from `0.5`, which is the same switch
/// every kind of step gets. The motion is `None` when nothing is away from park:
/// a `bulk` step, a `spontaneous` step outside any run, tools unwired, or a
/// `step` of 0 or past the end.
///
/// The caller draws the tool by applying the motion to its own copy:
///
/// ```ignore
/// let mut shown = scene.structure.clone();
/// if let Some(motion) = &motion {
///     apply_tool_pose(&mut shown, &scene, motion.tool(), &motion.pose_at(time));
/// }
/// ```
#[allow(clippy::too_many_arguments)]
pub fn replay_scene_at(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    time: f64,
    tags: HighlightTags<'_>,
) -> Result<(Scene, Option<ToolMotion>), MechanosynthError> {
    let time = time.clamp(0.0, 1.0);
    let reacted = time >= REACTION;
    let selected = crate::mechanosynth::steps_applied(step, script.steps.len());
    let applied = if reacted {
        selected
    } else {
        selected.saturating_sub(1)
    };

    let mut scene = build_scene(base, feedstocks, tools, library)?;
    // Only the last applied step's landing is ever read: when `time` has already
    // applied the selected step, that step's landing is the one the motion is
    // built from. Every other visit's orientation is a function of its own
    // approach direction (`reaction_pose`), so the replay owes the presentation
    // layer nothing else — which is what takes a step-49 evaluation from forty
    // sweeps to two.
    let (failure, mut landings) = replay_steps(
        &mut scene,
        library,
        script,
        applied as i32,
        tags,
        LandingPlan::Last,
    )?;
    if let Some(failure) = failure {
        return Err(failure);
    }

    // Nothing is selected: step 0, or a script with no steps.
    if selected == 0 {
        return Ok((scene, None));
    }
    let structure = runs(script, library);
    let tolerance = resolve_tolerance(library);
    let clash = library.clash_factor();
    let tools_wired = !scene.bindings.is_empty();
    let index = selected - 1;

    // Step `k`'s own checks are what selecting it means, so a match failure here
    // is an error at every `u`; the partial state is reachable by asking for one
    // step fewer. When `u` has already applied the step, `replay_steps` ran them.
    let op = library
        .get(&script.steps[index].op)
        .expect("validate_script_ops checked every op name");
    let plan = if reacted {
        None
    } else {
        Some(match_step_in_scene(
            &scene,
            op,
            &script.steps[index],
            selected,
            tolerance,
            clash,
            tools_wired,
        )?)
    };

    let landing = match &plan {
        Some(plan) => plan
            .tool_binding()
            .map(|_| crate::mechanosynth::plan_landing(&scene, plan)),
        None => landings.get(index).copied().flatten(),
    };
    landings.resize(selected, None);
    landings[index] = landing;

    // Before the reaction the highlight is on the **matched `before` atoms** of
    // both sides: the site lights up as the tool approaches, and the apex that
    // will react lights with it. From the reaction on it is the step's
    // `touched`, which `replay_steps` has already painted.
    if let (Some(plan), Some(current)) = (&plan, tags.current) {
        paint(&mut scene.structure, Some(current), plan.matched_atoms());
    }

    let motion = plan_motion(
        &scene,
        library,
        script,
        &structure,
        &landings,
        index,
        op.method,
        plan,
        tolerance,
        clash,
        tools_wired,
    );
    Ok((scene, motion))
}

/// Assembles the [`ToolMotion`] of the selected step: the visit, the hover, or
/// nothing.
#[allow(clippy::too_many_arguments)]
fn plan_motion(
    scene: &Scene,
    library: &OpLibrary,
    script: &BuildScript,
    runs: &Runs,
    landings: &[Option<Landing>],
    index: usize,
    method: Method,
    plan: Option<StepPlan<'_>>,
    tolerance: f64,
    clash: f64,
    tools_wired: bool,
) -> Option<ToolMotion> {
    if !tools_wired {
        return None;
    }

    match method {
        Method::Tip => {
            let landing = (*landings.get(index)?)?;
            let park = park_pose(scene, landing.tool);
            let reaction = reaction_pose(&landing, &park);
            let standoff = standoff_pose(&landing, &reaction);

            // Where the tool goes next: the standoff of its run's next visit, or
            // home. The look-ahead is the only place the engine reads past the
            // selected step.
            let next = runs.next_visit(index).and_then(|next| {
                look_ahead(
                    scene,
                    library,
                    script,
                    &plan,
                    index,
                    next,
                    tolerance,
                    clash,
                    tools_wired,
                )
                .map(|landing| {
                    // The same function of the same landing step `j` will use
                    // when the replay reaches it, which is what makes
                    // `pose_at(k, 1.0) == pose_at(j, 0.0)` hold by construction.
                    let arriving = reaction_pose(&landing, &park);
                    standoff_pose(&landing, &arriving)
                })
            });

            let mut visit = Visit {
                landing,
                park,
                reaction,
                standoff,
                from_park: runs.previous_visit(index).is_none(),
                to: next.unwrap_or(park),
                to_park: next.is_none(),
                scan: PathScan::default(),
            };
            let exclude = plan
                .as_ref()
                .map(StepPlan::target_atoms)
                .unwrap_or_default();
            visit.scan = scan_visit(scene, &visit, &exclude);
            Some(ToolMotion::Visit(Box::new(visit)))
        }
        Method::Spontaneous => {
            // Only the *next* visit is needed: the tool is waiting over the site
            // it will descend on, in the orientation that site's approach gives
            // it, and the flight that brought it here aimed at exactly this
            // pose (`reaction_pose` reads the approach alone).
            let (_, next) = runs.spanned_by(index)?;
            let landing = look_ahead(
                scene,
                library,
                script,
                &plan,
                index,
                next,
                tolerance,
                clash,
                tools_wired,
            )?;
            let park = park_pose(scene, landing.tool);
            let reaction = reaction_pose(&landing, &park);
            Some(ToolMotion::Hover {
                tool: landing.tool,
                pose: standoff_pose(&landing, &reaction),
            })
        }
        // A bulk step is an exposure: every tool is at park, and the scene
        // simply switches from before to after at the middle.
        Method::Bulk => None,
    }
}

/// The landing of the run's next visit, planned on a **clone** of the scene
/// after the selected step with the settles between applied.
///
/// A match failure along the way — or a step naming an operation the library
/// lost — means the tool has nowhere to fly to, so it goes home instead; the
/// failure is reported when the replay reaches that step, as it would have been
/// anyway. A *blocked* landing at the next visit is still a standoff.
#[allow(clippy::too_many_arguments)]
fn look_ahead(
    scene: &Scene,
    library: &OpLibrary,
    script: &BuildScript,
    plan: &Option<StepPlan<'_>>,
    index: usize,
    next: usize,
    tolerance: f64,
    clash: f64,
    tools_wired: bool,
) -> Option<Landing> {
    let mut ahead = scene.clone();
    // The selected step is applied on the clone when `time` has not applied it.
    if let Some(plan) = plan {
        apply_plan(&mut ahead, plan);
    }

    for (offset, step) in script.steps.iter().enumerate().take(next).skip(index + 1) {
        let op = library.get(&step.op)?;
        let plan = match_step_in_scene(&ahead, op, step, offset + 1, tolerance, clash, tools_wired)
            .ok()?;
        apply_plan(&mut ahead, &plan);
    }

    let step: &Step = script.steps.get(next)?;
    let op = library.get(&step.op)?;
    let plan =
        match_step_in_scene(&ahead, op, step, next + 1, tolerance, clash, tools_wired).ok()?;
    plan.tool_binding()
        .map(|_| crate::mechanosynth::plan_landing(&ahead, &plan))
}

/// Where a bound tool lives between runs.
fn park_pose(scene: &Scene, tool: usize) -> Pose {
    let binding = &scene.bindings[tool];
    Pose {
        r: binding.pose.r,
        t: binding.pose.t,
    }
}

// ===========================================================================
// The envelope cage
// ===========================================================================

/// The wireframe cage of every bound tool's envelope, in design space — one
/// entry per binding, in pin order, each a list of line segments.
///
/// The envelope is the one thing in this design a viewer cannot otherwise see,
/// and it is what a tilt or a blocked site is explained by. The cone's apex sits
/// at the tool-side reaction point of the tool's **nearest visit**
/// ([`cage_apex`]), and the segments are posed with the same [`Pose`] the tool's
/// atoms get — the binding's for a parked tool, `motion.pose_at(time)` for the
/// one that flies — so the cage descends with the tool and sits on the site at
/// the reaction.
///
/// Computed unconditionally, whether or not anyone has switched the overlay on:
/// a preference must not reach into an evaluation
/// (`doc/design_mechanosynth_trajectory.md` §The envelope cage).
pub fn tool_envelope_cages(
    scene: &Scene,
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    motion: Option<&ToolMotion>,
    time: f64,
) -> Vec<Vec<(DVec3, DVec3)>> {
    let selected = crate::mechanosynth::steps_applied(step, script.steps.len());
    scene
        .bindings
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            let pose = match motion {
                Some(motion) if motion.tool() == index => motion.pose_at(time),
                _ => Pose {
                    r: binding.pose.r,
                    t: binding.pose.t,
                },
            };
            let apex = cage_apex(library, script, selected, &binding.tool_type);
            binding
                .envelope
                .cage(apex, binding.axis)
                .into_iter()
                .map(|(from, to)| (pose.apply(from), pose.apply(to)))
                .collect()
        })
        .collect()
}

/// Where a tool's cage is anchored in its own frame: the tool-side reaction
/// point of its **nearest visit** — the selected step when that step is this
/// tool's, else its next `tip` step in the script, else its last, else the local
/// origin for a tool no step uses.
///
/// `selected` is the 1-based number of the selected step, `0` for none.
pub fn cage_apex(
    library: &OpLibrary,
    script: &BuildScript,
    selected: usize,
    tool_type: &str,
) -> DVec3 {
    let reaction_of = |index: usize| -> Option<DVec3> {
        let step = script.steps.get(index)?;
        let op = library.get(&step.op)?;
        if op.method != Method::Tip {
            return None;
        }
        if op.tool.as_ref()?.tool_type != tool_type {
            return None;
        }
        op.reaction.map(|reaction| reaction.tool)
    };

    let current = selected.checked_sub(1);
    if let Some(reaction) = current.and_then(reaction_of) {
        return reaction;
    }
    // The next visit after the selected step, then — failing that — the last one
    // before it, so a tool whose work is done still shows its cage where it last
    // reached rather than at its own origin.
    if let Some(reaction) = (selected..script.steps.len()).find_map(reaction_of) {
        return reaction;
    }
    if let Some(reaction) = (0..current.unwrap_or(0)).rev().find_map(reaction_of) {
        return reaction;
    }
    DVec3::ZERO
}
