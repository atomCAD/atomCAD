//! Kernel seam for the `mechanosynth` node's property panel, and for the
//! `ops_library` / `build_script` / `export_build_script` panels beside it.
//!
//! Three properties (the two deprecated file names and a step number), one
//! read-only readout the panel cannot compute for itself — the step count, the
//! current step's metadata and the script's chapter structure all live in the
//! parsed steps, which never cross the bridge — and the one-shot **Convert to
//! nodes** migration. The readout follows the wired `steps` and `step` pins
//! when they are connected, so it describes what the node evaluates rather
//! than what it stores.
//!
//! Each entry point is a thin FRB wrapper over an `#[frb(ignore)]` function
//! taking an explicit `&StructureDesigner`, so the logic is testable without
//! the global `CAD_INSTANCE` (the `field_distribution_api` pattern).

use crate::api::api_common::{
    refresh_structure_designer_auto, with_cad_instance_or, with_mut_cad_instance,
    with_mut_cad_instance_or,
};
use crate::api::structure_designer::structure_designer_api_types::{
    APIBuildScriptData, APIExportBuildScriptData, APIMechanosynthChapter, APIMechanosynthData,
    APIMechanosynthFeedstockRow, APIMechanosynthInfo, APIMechanosynthToolRow, APIOpsLibraryData,
    APIOpsLibraryEntry, APIOpsLibraryToolType,
};
use atomcad_crystolecule::mechanosynth::resolve_tolerance;
use atomcad_crystolecule::mechanosynth::{
    BuildScript, CLASH_BLOCK, NO_LAYER, NO_SITE, Participant, Scene, ToolMotion, steps_applied,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::build_step::steps_from_array;
use atomcad_structure_designer::nodes::export_build_script::ExportBuildScriptData;
use atomcad_structure_designer::nodes::mechanosynth::{
    MAX_REPORTED_CLEARANCE, MAX_REPORTED_CONTACT, MechanosynthData, OPS_PIN, STEP_PIN, STEPS_PIN,
    TIME_PIN, load_script_at,
};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_util::path_utils::get_parent_directory;

/// The design file's directory, which relative file names resolve against.
fn design_dir(designer: &StructureDesigner) -> Option<String> {
    designer
        .node_type_registry
        .design_file_name
        .as_ref()
        .and_then(|design_path| get_parent_directory(design_path))
}

/// The stored data of a `mechanosynth` node, against an explicit designer.
#[flutter_rust_bridge::frb(ignore)]
pub fn mechanosynth_data(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIMechanosynthData> {
    let data = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()?;
    Some(APIMechanosynthData {
        ops_file: data.ops_file.clone(),
        build_file: data.build_file.clone(),
        step: data.step,
        time: data.time,
        has_legacy_files: data.has_legacy_files(),
    })
}

/// Writes the stored data of a `mechanosynth` node, against an explicit
/// designer.
///
/// A file name that actually **changes** drops its parsed cache and is re-read
/// here; one that does not keeps it. The property setters fire on every focus
/// loss of a path field, not only on a real edit, so re-reading unconditionally
/// would re-parse both files on every stray click, and dropping the cache
/// unconditionally would leave the node reporting a missing library for a file
/// it had loaded seconds earlier (`project_import_node_payload_wipe`).
///
/// Undo comes from the shared `SetNodeDataCommand` that
/// `set_node_network_data_scoped` pushes; its restore path runs the node's
/// loader with the design directory, which repopulates the caches.
#[flutter_rust_bridge::frb(ignore)]
pub fn set_mechanosynth_data(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    data: &APIMechanosynthData,
) {
    let design_dir = design_dir(designer);

    let Some(current) = designer
        .get_node_network_data_scoped(scope_path, node_id)
        .and_then(|node_data| node_data.as_any_ref().downcast_ref::<MechanosynthData>())
    else {
        return;
    };

    let mut updated = current
        .with_ops_file(data.ops_file.clone())
        .with_build_file(data.build_file.clone());
    updated.step = data.step;
    updated.time = data.time;
    updated.reload_missing(design_dir.as_deref());

    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(updated));
}

/// The panel's read-only readout, against an explicit designer.
///
/// Reports what the node **evaluates**, not only what it stores: a build
/// script arriving on the wired `build_file` pin and a step arriving on the
/// wired `step` pin win over the stored properties, exactly as in the node's
/// `eval`. Without this a demo that switches the file name in by wire (one
/// `mechanosynth` node fed by a `switch` on two `string` nodes) replays fine
/// but shows "no build script loaded", a slider with no range and no step
/// readout. The two argument evaluations walk the upstream cone on every
/// panel rebuild, which for a file-name wire is a string or a switch and for
/// the step an int — see `StructureDesigner::evaluate_node_argument` for why
/// there is no memo to lean on. A wired file that fails to load reports the
/// all-zero readout; the failure itself reaches the user on the result pin.
#[flutter_rust_bridge::frb(ignore)]
pub fn mechanosynth_info(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIMechanosynthInfo> {
    let design_dir = design_dir(designer);
    // Cloned out rather than borrowed: the two argument evaluations below need
    // the designer mutably.
    let stored = designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()?
        .clone();

    // The `steps` pin. `None` means nothing is wired, so the deprecated
    // property applies: the parsed cache, else the stored name re-read (the
    // text-format edit path drops the cache, as `eval` knows).
    let script: Option<BuildScript> =
        match designer.evaluate_node_argument(scope_path, node_id, STEPS_PIN) {
            NetworkResult::None => stored.script.clone().or_else(|| {
                stored
                    .build_file
                    .as_deref()
                    .and_then(|name| load_script_at(name, design_dir.as_deref()).ok())
            }),
            NetworkResult::Error(_) => None,
            array => steps_from_array(&array).ok().map(|steps| BuildScript {
                file: "steps".to_string(),
                tolerance: None,
                steps,
            }),
        };
    let step = match designer.evaluate_node_argument(scope_path, node_id, STEP_PIN) {
        NetworkResult::Int(step) => step,
        _ => stored.step,
    };
    // The `time` pin, read and clamped the way `eval` reads and clamps it, so
    // the readout names the point the outputs were computed at.
    let time = match designer.evaluate_node_argument(scope_path, node_id, TIME_PIN) {
        NetworkResult::Float(time) => time,
        NetworkResult::Int(time) => time as f64,
        _ => stored.time,
    }
    .clamp(0.0, 1.0);
    // The `ops` pin, for the current step's method alone. The library is where
    // a reaction's kind is stated, so the readout has to ask it.
    let library = match designer.evaluate_node_argument(scope_path, node_id, OPS_PIN) {
        NetworkResult::OpLibrary(library) => Some(library),
        _ => stored.library.clone().map(std::sync::Arc::new),
    };
    // What counts as a *collision* on the path is the library's clash factor,
    // the same number the engine scanned with.
    let clash = library
        .as_ref()
        .map_or(CLASH_BLOCK, |library| library.clash_factor());

    let Some(script) = script else {
        return Some(APIMechanosynthInfo {
            count: 0,
            applied: 0,
            current_op: String::new(),
            current_note: String::new(),
            current_method: String::new(),
            current_phase: String::new(),
            current_layer: NO_LAYER,
            current_site: NO_SITE,
            chapters: Vec::new(),
            current_tool_type: String::new(),
            current_agent: String::new(),
            tools: tool_rows(stored.last_scene().as_ref()),
            feedstocks: feedstock_rows(stored.last_scene().as_ref()),
            ..motion_lines(stored.last_scene().as_ref(), None, time, clash)
        });
    };
    let count = script.steps.len();
    let applied = steps_applied(step, count);

    // "The current step" is the last one applied, `steps[applied - 1]`. At
    // `applied = 0` nothing has run, so there is nothing to name.
    let current = applied
        .checked_sub(1)
        .and_then(|index| script.steps.get(index));

    // The method is the **operation's** kind, read from the wired library: a
    // step names a reaction, and how the reaction is performed is a fact about
    // the reaction. With no library wired the panel simply has nothing to say.
    let operation = current
        .zip(library.as_ref())
        .and_then(|(step, library)| library.get(&step.op));
    let current_method = operation
        .map(|op| op.method.as_str().to_string())
        .unwrap_or_default();
    let current_tool_type = operation
        .and_then(|op| op.tool.as_ref())
        .map(|tool| tool.tool_type.clone())
        .unwrap_or_default();
    let current_agent = operation
        .and_then(|op| op.agent.clone())
        .unwrap_or_default();
    let scene = stored.last_scene();

    Some(APIMechanosynthInfo {
        count: count as i32,
        applied: applied as i32,
        current_op: current.map(|step| step.op.clone()).unwrap_or_default(),
        current_note: current
            .and_then(|step| step.note.clone())
            .unwrap_or_default(),
        current_method,
        current_phase: current.map(|step| step.phase.clone()).unwrap_or_default(),
        current_layer: current.map_or(NO_LAYER, |step| step.layer),
        current_site: current.map_or(NO_SITE, |step| step.site),
        chapters: chapters(&script),
        current_tool_type,
        current_agent,
        tools: tool_rows(scene.as_ref()),
        feedstocks: feedstock_rows(scene.as_ref()),
        ..motion_lines(scene.as_ref(), stored.last_motion().as_ref(), time, clash)
    })
}

/// The panel's two readout lines, off the last evaluation's motion.
///
/// Returned as a whole `APIMechanosynthInfo` whose other fields are never read,
/// so that both of `mechanosynth_info`'s returns can spread it with `..` and
/// neither can forget a field. A `None` motion is every tool parked: empty
/// words, and both measurements at the caps the `step` record uses, so the two
/// surfaces report the same number for "nothing visits".
fn motion_lines(
    scene: Option<&Scene>,
    motion: Option<&ToolMotion>,
    time: f64,
    clash: f64,
) -> APIMechanosynthInfo {
    let landing = motion.and_then(ToolMotion::landing);
    let worst = motion
        .and_then(ToolMotion::scan)
        .and_then(|scan| scan.worst);
    // A contact only *collides* below the library's clash factor; above it, it
    // is the panel's "path clear (worst …)" line, which the panel builds from
    // `contact_ratio` itself.
    let collision = match (worst, scene) {
        (Some(contact), Some(scene)) if contact.ratio < clash => format!(
            "path collides at {:.0} %: {}",
            contact.at * 100.0,
            contact.describe(scene)
        ),
        _ => String::new(),
    };

    APIMechanosynthInfo {
        time,
        leg: motion
            .map(|motion| motion.leg_at(time).as_str().to_string())
            .unwrap_or_default(),
        tilt_degrees: landing.map_or(0.0, |landing| landing.approach.tilt.to_degrees()),
        approach_clearance: landing
            .map_or(MAX_REPORTED_CLEARANCE, |landing| landing.approach.clearance)
            .min(MAX_REPORTED_CLEARANCE),
        contact_ratio: worst
            .map_or(MAX_REPORTED_CONTACT, |contact| contact.ratio)
            .min(MAX_REPORTED_CONTACT),
        contact_at: worst.map_or(0.0, |contact| contact.at),
        collision,
        // Never read: every caller spreads this value into a literal that has
        // already written each of these.
        count: 0,
        applied: 0,
        current_op: String::new(),
        current_note: String::new(),
        current_method: String::new(),
        current_phase: String::new(),
        current_layer: NO_LAYER,
        current_site: NO_SITE,
        chapters: Vec::new(),
        current_tool_type: String::new(),
        current_agent: String::new(),
        tools: Vec::new(),
        feedstocks: Vec::new(),
    }
}

/// The panel's *Tools* block: one row per bound tool molecule, in pin order.
///
/// Read off the scene the node's last evaluation parked, never by forcing one —
/// a panel rebuild must not cost a replay. A node that has not been evaluated
/// therefore reports no tools, which is also the truth about what is on screen.
#[flutter_rust_bridge::frb(ignore)]
pub fn tool_rows(scene: Option<&Scene>) -> Vec<APIMechanosynthToolRow> {
    scene
        .map(|scene| {
            scene
                .bindings
                .iter()
                .map(|binding| APIMechanosynthToolRow {
                    instance: binding.instance as i32,
                    tool_type: binding.tool_type.clone(),
                    residual: binding.pose.residual,
                    state: binding.state.clone().unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The panel's *Feedstocks* line: one entry per wired reservoir with the atom
/// count it has **at the step** — it grows as a build dumps onto it.
#[flutter_rust_bridge::frb(ignore)]
pub fn feedstock_rows(scene: Option<&Scene>) -> Vec<APIMechanosynthFeedstockRow> {
    let Some(scene) = scene else {
        return Vec::new();
    };
    let mut counts: Vec<(usize, i32)> = Vec::new();
    for (atom_id, participant) in &scene.participants {
        let Participant::Feedstock(index) = participant else {
            continue;
        };
        // A step can delete a reservoir atom, and the map keeps the id; count
        // what is actually there.
        if scene.structure.get_atom(*atom_id).is_none() {
            continue;
        }
        match counts.iter_mut().find(|(known, _)| known == index) {
            Some((_, count)) => *count += 1,
            None => counts.push((*index, 1)),
        }
    }
    counts.sort_unstable();
    counts
        .into_iter()
        .map(|(instance, atom_count)| APIMechanosynthFeedstockRow {
            instance: instance as i32,
            atom_count,
        })
        .collect()
}

/// Splits a script into chapters: maximal runs of consecutive steps sharing a
/// `(phase, layer)`.
///
/// Computed from the cached script on every info call, which is one pass over a
/// few hundred steps — cheaper than the two argument evaluations the caller has
/// already done. Steps that name no phase and no layer are chapters too, so the
/// result always covers the whole script and the panel never has to reason
/// about gaps.
#[flutter_rust_bridge::frb(ignore)]
pub fn chapters(script: &BuildScript) -> Vec<APIMechanosynthChapter> {
    let mut chapters: Vec<APIMechanosynthChapter> = Vec::new();
    for (index, step) in script.steps.iter().enumerate() {
        let number = index as i32 + 1;
        match chapters.last_mut() {
            Some(open) if open.phase == step.phase && open.layer == step.layer => {
                open.last_step = number;
            }
            _ => chapters.push(APIMechanosynthChapter {
                phase: step.phase.clone(),
                layer: step.layer,
                first_step: number,
                last_step: number,
            }),
        }
    }
    chapters
}

// ============================================================================
// `ops_library`, `build_script`, `export_build_script`
// ============================================================================

/// Reads a node's data, downcast to `T`.
#[flutter_rust_bridge::frb(ignore)]
fn node_data<T: atomcad_structure_designer::node_data::NodeData + Clone + 'static>(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<T> {
    designer
        .get_node_network_data_scoped(scope_path, node_id)?
        .as_any_ref()
        .downcast_ref::<T>()
        .cloned()
}

#[flutter_rust_bridge::frb(ignore)]
pub fn ops_library_data(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIOpsLibraryData> {
    let data: OpsLibraryData = node_data(designer, scope_path, node_id)?;
    let library = data.library.as_ref();
    Some(APIOpsLibraryData {
        file: data.file.clone(),
        tolerance: library.map_or(0.0, |library| resolve_tolerance(library)),
        tolerance_stated: library.is_some_and(|library| library.tolerance.is_some()),
        warnings: library
            .map(|library| library.warnings.clone())
            .unwrap_or_default(),
        ops: library
            .map(|library| {
                library
                    .ops
                    .iter()
                    .map(|op| APIOpsLibraryEntry {
                        name: op.name.clone(),
                        note: op.note.clone().unwrap_or_default(),
                        before_atoms: op.before.atoms.len() as i32,
                        after_atoms: op.after.atoms.len() as i32,
                        chiral: op.chiral,
                        method: op.method.as_str().to_string(),
                        tool_type: op
                            .tool
                            .as_ref()
                            .map(|tool| tool.tool_type.clone())
                            .unwrap_or_default(),
                        agent: op.agent.clone().unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        // The tool types, above the operations: what a design has to tag, and
        // the state vocabulary the offers will name.
        tools: library
            .map(|library| {
                library
                    .tools
                    .iter()
                    .map(|tool| APIOpsLibraryToolType {
                        name: tool.name.clone(),
                        note: tool.note.clone().unwrap_or_default(),
                        states: tool.states.clone().unwrap_or_default(),
                        frame_tags: tool.frame.iter().map(|entry| entry.tag.clone()).collect(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Writes an `ops_library` node's file name.
///
/// `reload` forces a re-read even when the name did not change — the panel's
/// Reload button, for a file edited outside the application. Without it the
/// name-unchanged path deliberately keeps the parsed cache
/// (`project_import_node_payload_wipe`), which is right for the focus-loss
/// writes the path field produces and wrong for an explicit Reload.
#[flutter_rust_bridge::frb(ignore)]
pub fn set_ops_library_file(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    file: Option<String>,
    reload: bool,
) {
    let design_dir = design_dir(designer);
    let Some(current) = node_data::<OpsLibraryData>(designer, scope_path, node_id) else {
        return;
    };
    let mut updated = current.with_file(file);
    if reload {
        updated.library = None;
        updated.load_error = None;
    }
    updated.reload_missing(design_dir.as_deref());
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(updated));
}

#[flutter_rust_bridge::frb(ignore)]
pub fn build_script_data(
    designer: &StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
) -> Option<APIBuildScriptData> {
    let data: BuildScriptData = node_data(designer, scope_path, node_id)?;
    Some(APIBuildScriptData {
        file: data.file.clone(),
        step_count: data
            .script
            .as_ref()
            .map_or(0, |script| script.steps.len() as i32),
    })
}

/// The `build_script` counterpart of [`set_ops_library_file`].
#[flutter_rust_bridge::frb(ignore)]
pub fn set_build_script_file(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    file: Option<String>,
    reload: bool,
) {
    let design_dir = design_dir(designer);
    let Some(current) = node_data::<BuildScriptData>(designer, scope_path, node_id) else {
        return;
    };
    let mut updated = current.with_file(file);
    if reload {
        updated.script = None;
        updated.load_error = None;
    }
    updated.reload_missing(design_dir.as_deref());
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(updated));
}

// ============================================================================
// Flutter entry points
// ============================================================================

#[flutter_rust_bridge::frb(sync)]
pub fn get_ops_library_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIOpsLibraryData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| ops_library_data(&cad_instance.structure_designer, &scope_path, node_id),
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_ops_library_data(
    scope_path: Vec<u64>,
    node_id: u64,
    file: Option<String>,
    reload: bool,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            set_ops_library_file(
                &mut cad_instance.structure_designer,
                &scope_path,
                node_id,
                file,
                reload,
            );
            // The refresh paths do not validate, so a stale error would linger
            // until an unrelated edit (`project_refresh_does_not_validate`).
            cad_instance.structure_designer.validate_active_network();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_build_script_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIBuildScriptData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                build_script_data(&cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_build_script_data(
    scope_path: Vec<u64>,
    node_id: u64,
    file: Option<String>,
    reload: bool,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            set_build_script_file(
                &mut cad_instance.structure_designer,
                &scope_path,
                node_id,
                file,
                reload,
            );
            cad_instance.structure_designer.validate_active_network();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_export_build_script_data(
    scope_path: Vec<u64>,
    node_id: u64,
) -> Option<APIExportBuildScriptData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                let data: ExportBuildScriptData =
                    node_data(&cad_instance.structure_designer, &scope_path, node_id)?;
                Some(APIExportBuildScriptData {
                    file_name: data.file_name,
                })
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_export_build_script_data(
    scope_path: Vec<u64>,
    node_id: u64,
    data: APIExportBuildScriptData,
) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            let designer = &mut cad_instance.structure_designer;
            if node_data::<ExportBuildScriptData>(designer, &scope_path, node_id).is_some() {
                designer.set_node_network_data_scoped(
                    &scope_path,
                    node_id,
                    Box::new(ExportBuildScriptData {
                        file_name: data.file_name,
                    }),
                );
                designer.validate_active_network();
            }
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_mechanosynth_data(scope_path: Vec<u64>, node_id: u64) -> Option<APIMechanosynthData> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                mechanosynth_data(&cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_mechanosynth_node_data(scope_path: Vec<u64>, node_id: u64, data: APIMechanosynthData) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            set_mechanosynth_data(
                &mut cad_instance.structure_designer,
                &scope_path,
                node_id,
                &data,
            );
            // The refresh paths do not validate, so a stale error would linger
            // until an unrelated edit (`project_refresh_does_not_validate`).
            cad_instance.structure_designer.validate_active_network();
            refresh_structure_designer_auto(cad_instance);
        });
    }
}

/// **Convert to nodes**: replaces a `mechanosynth` node's deprecated
/// `ops_file` / `build_file` properties with wired `ops_library` /
/// `build_script` nodes. One undo entry; returns the failure message, if any,
/// for the panel to show.
#[flutter_rust_bridge::frb(sync)]
pub fn mechanosynth_convert_files_to_nodes(scope_path: Vec<u64>, node_id: u64) -> Option<String> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let outcome = cad_instance
                    .structure_designer
                    .convert_mechanosynth_files_to_nodes(&scope_path, node_id);
                refresh_structure_designer_auto(cad_instance);
                outcome.err()
            },
            Some("no CAD instance".to_string()),
        )
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_mechanosynth_info(scope_path: Vec<u64>, node_id: u64) -> Option<APIMechanosynthInfo> {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                mechanosynth_info(&mut cad_instance.structure_designer, &scope_path, node_id)
            },
            None,
        )
    }
}
