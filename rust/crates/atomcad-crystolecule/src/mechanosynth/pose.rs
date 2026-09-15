//! Solving a tool molecule's pose from its tagged frame atoms.
//!
//! A design never types a tool's position. It tags four atoms — the apex and
//! three legs — and the library states where those four sit in the tool's own
//! local frame; the rigid fit of one onto the other *is* the pose, and its
//! residual is the check that the molecule really has the geometry the library
//! was computed for.
//!
//! That is the whole binding geometry: **no sweep, no candidate list, no mirror
//! rule**. Three non-collinear correspondences already fix a proper rotation,
//! and with the frame non-coplanar by parse rule (see
//! [`convert_frame`](super::parse)) a molecule that is the mirror image of the
//! library's cannot be superimposed by any proper rotation — so it fails the
//! residual gate rather than binding mirrored. A tip of thousands of atoms
//! binds in the time it takes to look up four tags.

use super::fit::{RANK_EPSILON, rank_of, rigid_fit};
use super::schema::MechanosynthError;
use glam::{DMat3, DVec3};

/// Where a bound tool molecule sits: `p_design = r · p_local + t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolPose {
    pub r: DMat3,
    pub t: DVec3,
    /// Max per-atom distance of the fit, Å — the **max**, as everywhere in this
    /// subsystem. `f64::INFINITY` never reaches here: a fit that does not exist
    /// is an `Err`.
    pub residual: f64,
}

/// The rigid transform taking a tool type's frame onto the tagged atoms of the
/// molecule that plays it.
///
/// `correspondences` pairs each frame entry's **local** position with the
/// **design** position of the atom carrying its tag, in frame order.
///
/// `tolerance` gates acceptance and nothing else; the pose it returns is exact
/// iff the residual is. Degenerate input — fewer than three correspondences, or
/// a set that spans no plane — is an `Err` rather than an arbitrary rotation:
/// a mis-tagged molecule whose tagged atoms happen to be collinear must not
/// bind silently, and the eigen solver would happily answer an undetermined
/// question with whichever vector its sweeps produced.
pub fn tool_pose(
    correspondences: &[(DVec3, DVec3)],
    tolerance: f64,
) -> Result<ToolPose, MechanosynthError> {
    let degenerate = |detail: &str| MechanosynthError::ToolPoseResidual {
        molecule: String::new(),
        tool_type: String::new(),
        residual: f64::INFINITY,
        detail: detail.to_string(),
    };

    if correspondences.len() < 3 {
        return Err(degenerate(&format!(
            "{} tagged atoms determine no orientation; three non-collinear ones are the minimum",
            correspondences.len()
        )));
    }

    let local: Vec<DVec3> = correspondences.iter().map(|(local, _)| *local).collect();
    let design: Vec<DVec3> = correspondences.iter().map(|(_, design)| *design).collect();

    if rank_of(&local) < 2 {
        return Err(degenerate(
            "the frame positions are collinear, so no orientation is determined",
        ));
    }
    if rank_of(&design) < 2 {
        return Err(degenerate(
            "the tagged atoms are collinear, so no orientation is determined",
        ));
    }

    let Some(fit) = rigid_fit(&local, &design, false) else {
        return Err(degenerate("no rigid fit exists for the tagged atoms"));
    };
    if !fit.residual.is_finite() || fit.r.determinant().abs() < RANK_EPSILON {
        return Err(degenerate("the fit onto the tagged atoms is degenerate"));
    }
    if fit.residual > tolerance {
        return Err(MechanosynthError::ToolPoseResidual {
            molecule: String::new(),
            tool_type: String::new(),
            residual: fit.residual,
            detail: format!(
                "residual {:.4} Å exceeds the {:.2} Å tolerance; the molecule is not the tool \
                 this library was computed for, is tagged wrongly, or is the mirror image of it",
                fit.residual, tolerance
            ),
        });
    }

    Ok(ToolPose {
        r: fit.r,
        t: fit.t,
        residual: fit.residual,
    })
}

/// Names the molecule and the type on an error [`tool_pose`] raised without
/// knowing either. Binding is the only caller, and it knows both.
pub(super) fn name_pose_error(
    error: MechanosynthError,
    molecule: &str,
    tool_type: &str,
) -> MechanosynthError {
    match error {
        MechanosynthError::ToolPoseResidual {
            residual, detail, ..
        } => MechanosynthError::ToolPoseResidual {
            molecule: molecule.to_string(),
            tool_type: tool_type.to_string(),
            residual,
            detail,
        },
        other => other,
    }
}
