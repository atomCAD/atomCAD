//! The scene side of feasibility: what counts as an obstacle, where a `tip`
//! step's tool reacts and from which direction, and whether a bound molecule
//! really fits the envelope its type claims.
//!
//! Everything here **measures**; nothing fails a step. The same call serves a
//! sequence generator deciding what to emit, an editor replaying a half-written
//! block and a viewer scrubbing a finished build, and only the first of those
//! wants the verdict to stop it — so [`Landing::reachable`] is a question the
//! caller asks, not an error the engine raises. The one exception is
//! [`check_containment`], which runs at *binding* and is a library error.
//!
//! Design doc: `doc/design_mechanosynth_trajectory.md` §Architecture.

use super::envelope::{Approach, approach_direction};
use crate::atomic_constants::element_symbol;
use crate::mechanosynth::apply::covalent_radius;
use crate::mechanosynth::scene::{Participant, Scene, StepPlan};
use crate::mechanosynth::schema::{MechanosynthError, Method, OpLibrary, ToolType};
use glam::DVec3;

/// How far up the approach direction the standoff sits, Å.
///
/// **A constant, deliberately.** An earlier draft put the standoff where the
/// approach axis met the *park plane*, which tied a visit's geometry to where
/// the tools happened to be parked — and so tied a sequence generator to it too,
/// since the generator has to plan the visit it is about to emit. A fixed height
/// above the reaction point makes a visit a property of the site alone: the
/// tools can be placed, and moved, long after the sequence exists.
pub const STANDOFF_HEIGHT: f64 = 6.0;

/// How far outside the envelope's surface a bound molecule's atom may reach
/// before [`check_containment`] calls it a library error, Å. Floating-point
/// slack only: a library states round numbers and a molecule is built from
/// them.
const CONTAINMENT_EPSILON: f64 = 1e-6;

/// Where one `tip` step's tool reacts and from which direction — the
/// feasibility half of a visit, with no roll and no path in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    /// Index into [`Scene::bindings`].
    pub tool: usize,
    /// `p_r = step.r · reaction.target + step.t`: where the reaction happens,
    /// in design space.
    pub reaction_point: DVec3,
    /// `reaction.tool`, in the tool's local frame — the point the pose brings
    /// onto `reaction_point`.
    pub reaction_tool: DVec3,
    /// The tool's local axis, `±z`; see [`ToolType::axis`].
    pub axis: DVec3,
    /// What the sweep found.
    pub approach: Approach,
    /// How far up the approach the visit begins and ends, Å. Always
    /// [`STANDOFF_HEIGHT`]; a field rather than a constant so that a library
    /// could one day state its own.
    pub standoff_height: f64,
}

impl Landing {
    /// Whether every obstacle sphere is outside the envelope along the approach
    /// the sweep chose.
    ///
    /// A sequence generator refuses a step whose landing is not reachable; a
    /// replay visits along the least-blocked direction anyway and reports it,
    /// because a view of a build is not the place to decide that the build is
    /// impossible.
    pub fn reachable(&self) -> bool {
        self.approach.clearance > 0.0
    }

    /// The standoff point: where the approach axis meets the park plane.
    pub fn standoff_point(&self) -> DVec3 {
        self.reaction_point + self.approach.direction * self.standoff_height
    }
}

/// The one definition of an **obstacle**, so that a generator and the node can
/// never disagree about what the sweep is avoiding.
///
/// Every scene atom that belongs to **no tool** and is not in `exclude` — the
/// target side's matched `before` atoms, because the site is the reaction, not
/// an obstacle. Each obstacle is a sphere of its own covalent radius times the
/// library's clash factor.
///
/// **The tool's radius is not in here.** The envelope is a *solid*: it contains
/// the tool's atoms with their radii, not merely their centres
/// ([`check_containment`]), so the tool's own extent is already accounted for
/// and adding it again would charge for it twice. That double charge is what
/// once made an abstraction unreachable from every direction — the keep-out
/// sphere around the site's own host atom grew past the bond length, and the
/// cone's apex sits inside it.
///
/// **No tool is an obstacle, its own or anyone else's.** A sweep that saw the
/// parked tools would make a visit depend on where they sit, and so would make a
/// *sequence* depend on it: a generator plans the visit it is about to emit, so
/// it would have to know the tool layout before it could emit a step. Leaving
/// them out is what lets the tools be placed — and rearranged — long after the
/// sequence exists.
///
/// The cost is that keeping the tools out of each other's way is the designer's
/// job: park them on **different sides** of the workpiece and its reservoirs,
/// one left, one right, one in front. Two things still watch for the mistakes
/// that makes — the path scan reports a flight that crosses another tool, and
/// the steric rule still refuses a step that places an atom inside a parked one.
pub fn obstacles_for(scene: &Scene, exclude: &[u32], clash: f64) -> Vec<(DVec3, f64)> {
    scene
        .structure
        .iter_atoms()
        .filter(|(atom_id, _)| !matches!(scene.participant(**atom_id), Participant::Tool(_)))
        .filter(|(atom_id, _)| !exclude.contains(atom_id))
        .map(|(_, atom)| (atom.position, clash * covalent_radius(atom.atomic_number)))
        .collect()
}

/// Plans the landing of a matched `tip` step on the scene **before** it: the
/// reaction point placed, the sweep run, the standoff height fixed.
///
/// Infallible. The envelope rides on the binding, the reaction points are on the
/// operation the plan holds, and the sweep always answers — which is what makes
/// applying a step no more able to fail than it was in milestone 1.
///
/// Called by `apply_step_in_scene` for every `tip` step whose tool is bound, and
/// public for a generator that wants to ask before committing.
///
/// # Panics
///
/// If `plan` is not a `tip` step with a bound tool and a parsed `reaction`. The
/// parser makes `reaction` required on `tip`, and the two callers check the
/// method and the binding first.
pub fn plan_landing(scene: &Scene, plan: &StepPlan<'_>) -> Landing {
    let tool = plan
        .tool_binding()
        .expect("plan_landing is called only for a tip step whose tool is bound");
    let binding = &scene.bindings[tool];
    let reaction = plan
        .op
        .reaction
        .expect("a tip operation carries a reaction block since /4");

    let reaction_point = plan.step_r * reaction.target + plan.step_t;
    let obstacles = obstacles_for(scene, &plan.target_atoms(), plan.clash);
    let approach = approach_direction(&binding.envelope, reaction_point, &obstacles);

    Landing {
        tool,
        reaction_point,
        reaction_tool: reaction.tool,
        axis: binding.axis,
        standoff_height: STANDOFF_HEIGHT,
        approach,
    }
}

/// Checks that every atom of a bound molecule lies inside the envelope its type
/// claims — **with its radius**, not merely its centre — at the tool-side
/// reaction point of **every** `tip` operation of that type.
///
/// The envelope is the solid the tool occupies, so an atom is contained when its
/// whole sphere is: `gap ≤ −r`. That is what lets the sweep charge an obstacle
/// its own radius alone ([`obstacles_for`]).
///
/// One atom is exempt: the **cargo**, whose own sphere swallows the apex because
/// the apex *is* where it sits. A donation's transferred atom is at the reaction
/// point by construction, and no cone with its apex there can contain it.
///
/// One pass over the tool's atoms per operation, run once at binding — so a
/// library whose envelope is narrower than the molecule playing it is reported
/// before any step, where the frame residual is, rather than as a site that
/// mysteriously will not clear.
///
/// `atoms` are the molecule's design-space atom ids; the envelope lives in the
/// tool's local frame, so each is carried back through the pose.
pub(in crate::mechanosynth) fn check_containment(
    scene: &Scene,
    tool: usize,
    tool_type: &ToolType,
    library: &OpLibrary,
    atoms: &[u32],
) -> Result<(), MechanosynthError> {
    let binding = &scene.bindings[tool];
    let inverse = binding.pose.r.transpose();

    for op in &library.ops {
        if op.method != Method::Tip {
            continue;
        }
        if op.tool.as_ref().map(|side| side.tool_type.as_str()) != Some(tool_type.name.as_str()) {
            continue;
        }
        let Some(reaction) = op.reaction else {
            continue;
        };

        for atom_id in atoms {
            let Some(atom) = scene.structure.get_atom(*atom_id) else {
                continue;
            };
            let local = inverse * (atom.position - binding.pose.t);
            let relative = local - reaction.tool;
            let s = relative.dot(binding.axis);
            let rho = (relative - s * binding.axis).length();
            // The cargo sits at the apex; no cone anchored there contains it.
            if relative.length() <= covalent_radius(atom.atomic_number) {
                continue;
            }
            let excess = tool_type.envelope.gap(s, rho) + covalent_radius(atom.atomic_number);
            if excess > CONTAINMENT_EPSILON {
                return Err(MechanosynthError::ToolOutsideEnvelope {
                    tool: binding.label(),
                    tool_type: tool_type.name.clone(),
                    op: op.name.clone(),
                    atom: format!(
                        "the {} of atom {atom_id}",
                        element_symbol(atom.atomic_number)
                    ),
                    excess,
                });
            }
        }
    }

    Ok(())
}
