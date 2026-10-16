// Shader structure, checked on the CPU with naga
// (doc/design_screen_space_lines.md, Testing layer 2).
//
// A broken `.wgsl` file otherwise fails only at the app's start-up, and a
// `CameraUniform` that differs between a shader and the Rust struct (issue
// #269) fails only as a wrong picture. Both are caught here, in plain
// `cargo test`, without a GPU.

use atomcad_renderer::pipelines::{LIT_SHADERS, compose_lit_shader};
use atomcad_renderer::renderer::CameraUniform;
use std::mem::offset_of;
use std::path::PathBuf;

/// The shaders that bind the camera uniform. Listed rather than discovered,
/// so a shader that silently stopped declaring the struct fails the test.
const CAMERA_SHADERS: [&str; 6] = [
    "atom_impostor.wgsl",
    "bond_impostor.wgsl",
    "label.wgsl",
    "line_mesh.wgsl",
    "mesh.wgsl",
    "transparent_impostor.wgsl",
];

fn shader_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every shader in `src/`. `lighting.wgsl` is not one: it is a module the lit
/// shaders are composed with, and is checked as part of them.
fn all_shader_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(shader_dir())
        .expect("read src/")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wgsl"))
        .filter(|path| !path.ends_with("lighting.wgsl"))
        .collect();
    files.sort();
    files
}

/// The source of the shader at `path` as the renderer compiles it: a lit
/// shader composed with `lighting.wgsl` (`doc/design_depth_perception.md`
/// D1), any other as written.
fn source(path: &std::path::Path) -> String {
    let source = std::fs::read_to_string(path).expect("read shader");
    let file = path.file_name().unwrap().to_str().unwrap();
    if LIT_SHADERS.contains(&file) {
        compose_lit_shader(&source)
    } else {
        source
    }
}

fn parse(path: &std::path::Path) -> naga::Module {
    let source = source(path);
    naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
        panic!(
            "{} does not parse:\n{}",
            path.display(),
            e.emit_to_string(&source)
        )
    })
}

#[test]
fn every_shader_parses_and_validates() {
    let files = all_shader_files();
    assert!(files.len() >= CAMERA_SHADERS.len(), "found {files:?}");
    for path in files {
        let module = parse(&path);
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{} does not validate: {e:?}", path.display()));
    }
}

/// `(member name, offset)` of the struct `name` the shader at `path`
/// declares, and the struct's size.
fn wgsl_struct_layout(path: &std::path::Path, name: &str) -> (Vec<(String, u32)>, u32) {
    let module = parse(path);
    let (_, ty) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("{} declares no {name}", path.display()));
    match &ty.inner {
        naga::TypeInner::Struct { members, span } => (
            members
                .iter()
                .map(|m| (m.name.clone().unwrap_or_default(), m.offset))
                .collect(),
            *span,
        ),
        other => panic!("{name} is not a struct: {other:?}"),
    }
}

/// The Rust struct's layout, by the WGSL member names (the padding fields
/// have no WGSL counterpart).
fn rust_camera_uniform_layout() -> (Vec<(String, u32)>, u32) {
    let members = [
        ("view_proj", offset_of!(CameraUniform, view_proj)),
        ("view_matrix", offset_of!(CameraUniform, view_matrix)),
        ("proj_matrix", offset_of!(CameraUniform, proj_matrix)),
        (
            "camera_position",
            offset_of!(CameraUniform, camera_position),
        ),
        ("head_light_dir", offset_of!(CameraUniform, head_light_dir)),
        (
            "is_orthographic",
            offset_of!(CameraUniform, is_orthographic),
        ),
        (
            "ortho_half_height",
            offset_of!(CameraUniform, ortho_half_height),
        ),
        (
            "viewport_size_px",
            offset_of!(CameraUniform, viewport_size_px),
        ),
        ("pixel_scale", offset_of!(CameraUniform, pixel_scale)),
    ];
    (
        members
            .iter()
            .map(|(name, offset)| (name.to_string(), *offset as u32))
            .collect(),
        std::mem::size_of::<CameraUniform>() as u32,
    )
}

#[test]
fn every_camera_uniform_in_wgsl_matches_the_rust_struct() {
    let expected = rust_camera_uniform_layout();
    for name in CAMERA_SHADERS {
        let layout = wgsl_struct_layout(&shader_dir().join(name), "CameraUniform");
        assert_eq!(layout, expected, "CameraUniform layout of {name}");
    }
}

#[test]
fn only_the_listed_shaders_declare_a_camera_uniform() {
    for path in all_shader_files() {
        let declares = parse(&path)
            .types
            .iter()
            .any(|(_, ty)| ty.name.as_deref() == Some("CameraUniform"));
        let file = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            declares,
            CAMERA_SHADERS.contains(&file),
            "{file}: add it to CAMERA_SHADERS so its layout is checked"
        );
    }
}

/// `(member name, interpolation)` of the struct `entry_point` returns.
fn vertex_output_interpolation(
    path: &std::path::Path,
    entry_point: &str,
) -> Vec<(String, Option<naga::Interpolation>)> {
    let module = parse(path);
    let entry = module
        .entry_points
        .iter()
        .find(|e| e.name == entry_point)
        .unwrap_or_else(|| panic!("{} has no {entry_point}", path.display()));
    let result = entry.function.result.as_ref().expect("returns a value");
    match &module.types[result.ty].inner {
        naga::TypeInner::Struct { members, .. } => members
            .iter()
            .filter_map(|m| match &m.binding {
                Some(naga::Binding::Location { interpolation, .. }) => {
                    Some((m.name.clone().unwrap_or_default(), *interpolation))
                }
                _ => None,
            })
            .collect(),
        other => panic!("{entry_point} does not return a struct: {other:?}"),
    }
}

/// The line shader's screen-space outputs must interpolate `linear`: each
/// corner carries its own `w`, so the default perspective-correct
/// interpolation would bend them on any line whose ends sit at different
/// depths. Per-instance values are `flat`, and the world position the grid
/// fade measures from is `perspective` (doc/design_screen_space_lines.md D4,
/// D9).
#[test]
fn line_shader_outputs_carry_the_d4_interpolation() {
    use naga::Interpolation::{Flat, Linear, Perspective};
    let outputs = vertex_output_interpolation(&shader_dir().join("line_mesh.wgsl"), "vs_main");
    let expected = [
        ("color", Flat),
        ("across", Linear),
        ("along", Linear),
        ("length_px", Flat),
        ("half_width", Flat),
        ("scaled_width", Flat),
        ("dash_offset", Flat),
        ("dash_on", Flat),
        ("dash_period", Flat),
        // D9: the world position interpolates perspective-correctly, the
        // slot's fade distances are per instance.
        ("world_pos", Perspective),
        ("fade", Flat),
    ];
    assert_eq!(
        outputs,
        expected
            .iter()
            .map(|(name, interpolation)| (name.to_string(), Some(*interpolation)))
            .collect::<Vec<_>>()
    );
}

// ---- Viewport anti-aliasing, Phase 2 (doc/design_viewport_antialiasing.md) --

/// The shaders that decide impostor coverage per sample (D4).
const IMPOSTOR_SHADERS: [&str; 3] = [
    "atom_impostor.wgsl",
    "bond_impostor.wgsl",
    "transparent_impostor.wgsl",
];

/// D4: each impostor shader takes the target's sample count as the
/// pipeline-overridable constant `SAMPLE_COUNT` (set by `PipelineSet::new`)
/// and writes `@builtin(sample_mask)` from `fs_main`.
#[test]
fn impostor_shaders_declare_sample_count_and_write_a_sample_mask() {
    for name in IMPOSTOR_SHADERS {
        let module = parse(&shader_dir().join(name));
        let sample_count = module
            .overrides
            .iter()
            .find(|(_, o)| o.name.as_deref() == Some("SAMPLE_COUNT"))
            .unwrap_or_else(|| panic!("{name} has no `override SAMPLE_COUNT`"))
            .1;
        assert_eq!(
            module.types[sample_count.ty].inner,
            naga::TypeInner::Scalar(naga::Scalar::U32),
            "{name}: SAMPLE_COUNT is not a u32"
        );

        let entry = module
            .entry_points
            .iter()
            .find(|e| e.name == "fs_main")
            .unwrap_or_else(|| panic!("{name} has no fs_main"));
        let result = entry.function.result.as_ref().expect("fs_main returns");
        let writes_mask = match &module.types[result.ty].inner {
            naga::TypeInner::Struct { members, .. } => members
                .iter()
                .any(|m| m.binding == Some(naga::Binding::BuiltIn(naga::BuiltIn::SampleMask))),
            other => panic!("{name}: fs_main does not return a struct: {other:?}"),
        };
        assert!(writes_mask, "{name}: fs_main writes no sample_mask");
    }
}

/// The body of every `fn` in a WGSL source, by name (brace matching; the
/// shaders have no braces in comments or strings).
fn function_bodies(source: &str) -> Vec<(String, String)> {
    let mut bodies = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("fn ") {
        let after = &rest[at + 3..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let open = after.find('{').expect("fn has a body");
        let mut depth = 0;
        let mut close = open;
        for (i, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        bodies.push((name, after[open + 1..close].to_string()));
        rest = &after[close..];
    }
    bodies
}

/// Removes `// …` comments, so the check below reads code only.
fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// D4: the derivatives the per-sample rays are built from must be taken
/// before anything can discard (a derivative after a `discard` reads helper
/// lanes' garbage, or is rejected outright by stricter compilers). Checked on
/// the source: in no function may a `dpdx`/`dpdy`/`fwidth` follow a
/// `discard`, or a call to a function that can discard.
#[test]
fn impostor_shaders_take_derivatives_before_any_discard() {
    const DERIVATIVES: [&str; 3] = ["dpdx", "dpdy", "fwidth"];
    for name in IMPOSTOR_SHADERS {
        let source = strip_line_comments(&source(&shader_dir().join(name)));
        let bodies = function_bodies(&source);

        // The functions that can discard, transitively.
        let mut discarding: Vec<&str> = bodies
            .iter()
            .filter(|(_, body)| body.contains("discard"))
            .map(|(name, _)| name.as_str())
            .collect();
        loop {
            let more: Vec<&str> = bodies
                .iter()
                .filter(|(f, _)| !discarding.contains(&f.as_str()))
                .filter(|(_, body)| discarding.iter().any(|d| body.contains(&format!("{d}("))))
                .map(|(f, _)| f.as_str())
                .collect();
            if more.is_empty() {
                break;
            }
            discarding.extend(more);
        }

        let mut checked_a_derivative = false;
        for (function, body) in &bodies {
            let first_discard = std::iter::once("discard".to_string())
                .chain(discarding.iter().map(|d| format!("{d}(")))
                .filter_map(|needle| body.find(&needle))
                .min();
            for derivative in DERIVATIVES {
                for (at, _) in body.match_indices(derivative) {
                    checked_a_derivative = true;
                    assert!(
                        first_discard.is_none_or(|d| at < d),
                        "{name}: `{derivative}` in `{function}` after something that can discard"
                    );
                }
            }
        }
        assert!(checked_a_derivative, "{name} takes no derivatives");
    }
}

// ---- Viewport anti-aliasing, Phase 3 (doc/design_viewport_antialiasing.md) --

use atomcad_renderer::pipelines::{PER_SAMPLE_SHADING_MARKER, without_per_sample_shading};

/// D5: each impostor shader has the refined frame's entry point,
/// `fs_main_per_sample`, which takes `@builtin(sample_index)` (that is what
/// runs it once per sample) and writes no sample mask.
#[test]
fn impostor_shaders_have_a_per_sample_entry_point() {
    for name in IMPOSTOR_SHADERS {
        let module = parse(&shader_dir().join(name));
        let entry = module
            .entry_points
            .iter()
            .find(|e| e.name == "fs_main_per_sample")
            .unwrap_or_else(|| panic!("{name} has no fs_main_per_sample"));
        assert_eq!(entry.stage, naga::ShaderStage::Fragment, "{name}");
        let takes_sample_index = entry
            .function
            .arguments
            .iter()
            .any(|a| a.binding == Some(naga::Binding::BuiltIn(naga::BuiltIn::SampleIndex)));
        assert!(
            takes_sample_index,
            "{name}: fs_main_per_sample takes no sample_index"
        );

        let result = entry.function.result.as_ref().expect("returns");
        let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
            panic!("{name}: fs_main_per_sample does not return a struct");
        };
        assert!(
            !members
                .iter()
                .any(|m| m.binding == Some(naga::Binding::BuiltIn(naga::BuiltIn::SampleMask))),
            "{name}: fs_main_per_sample writes a sample mask"
        );

        // Phase 2's derivatives-before-discard check reads every function,
        // this one included; make sure it has derivatives to check.
        let source = source(&shader_dir().join(name));
        let (_, body) = function_bodies(&strip_line_comments(&source))
            .into_iter()
            .find(|(f, _)| f == "fs_main_per_sample")
            .unwrap();
        assert!(
            body.contains("dpdx"),
            "{name}: fs_main_per_sample takes no derivatives"
        );
    }
}

/// D5: without `MULTISAMPLED_SHADING` a module that uses `sample_index` does
/// not validate at all, so the renderer cuts the per-sample block off
/// (`without_per_sample_shading`). What is left must validate without the
/// capability and keep `fs_main`.
#[test]
fn impostor_shaders_without_the_per_sample_block_validate_without_per_sample_shading() {
    let caps =
        naga::valid::Capabilities::default() - naga::valid::Capabilities::MULTISAMPLED_SHADING;
    let validate = |source: &str| {
        let module = naga::front::wgsl::parse_str(source).expect("parses");
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), caps)
            .validate(&module)
            .map(|_| module)
    };
    for name in IMPOSTOR_SHADERS {
        // As composed (`doc/design_depth_perception.md` D1): the cut runs
        // after the composition, so it is the composed source that must
        // survive it.
        let source = source(&shader_dir().join(name));
        assert_eq!(
            source.matches(PER_SAMPLE_SHADING_MARKER).count(),
            1,
            "{name}: the marker must appear exactly once"
        );
        assert!(
            validate(&source).is_err(),
            "{name}: the whole shader validates without per-sample shading"
        );

        let stripped = without_per_sample_shading(&source);
        let module = validate(stripped)
            .unwrap_or_else(|e| panic!("{name} without the per-sample block: {e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(entries.contains(&"fs_main"), "{name}: {entries:?}");
        assert!(
            !entries.contains(&"fs_main_per_sample"),
            "{name}: {entries:?}"
        );
    }
    assert_eq!(without_per_sample_shading("no marker"), "no marker");
}

// ---- Depth perception, Phase 1 (doc/design_depth_perception.md) -----------

use atomcad_renderer::pipelines::{LIGHTING_MARKER, LIGHTING_MODULE};
use atomcad_renderer::shading_uniform::ShadingUniform;

/// The Rust `ShadingUniform`'s layout, by the WGSL member names.
fn rust_shading_uniform_layout() -> (Vec<(String, u32)>, u32) {
    let members = [
        ("key_light_dir", offset_of!(ShadingUniform, key_light_dir)),
        ("key_intensity", offset_of!(ShadingUniform, key_intensity)),
        ("fill_light_dir", offset_of!(ShadingUniform, fill_light_dir)),
        ("fill_intensity", offset_of!(ShadingUniform, fill_intensity)),
        ("hemisphere_up", offset_of!(ShadingUniform, hemisphere_up)),
        ("sky", offset_of!(ShadingUniform, sky)),
        ("background", offset_of!(ShadingUniform, background)),
        ("ground", offset_of!(ShadingUniform, ground)),
        (
            "depth_cue_front",
            offset_of!(ShadingUniform, depth_cue_front),
        ),
        ("depth_cue_span", offset_of!(ShadingUniform, depth_cue_span)),
        (
            "depth_cue_strength",
            offset_of!(ShadingUniform, depth_cue_strength),
        ),
        ("ao_strength", offset_of!(ShadingUniform, ao_strength)),
        (
            "shadow_strength",
            offset_of!(ShadingUniform, shadow_strength),
        ),
        ("ao_key", offset_of!(ShadingUniform, ao_key)),
    ];
    (
        members
            .iter()
            .map(|(name, offset)| (name.to_string(), *offset as u32))
            .collect(),
        std::mem::size_of::<ShadingUniform>() as u32,
    )
}

/// D2: every lit shader, as composed, declares the `ShadingUniform` of the
/// Rust struct, byte for byte, and binds it at group 0, binding 1.
#[test]
fn every_shading_uniform_in_wgsl_matches_the_rust_struct() {
    let expected = rust_shading_uniform_layout();
    for name in LIT_SHADERS {
        let path = shader_dir().join(name);
        assert_eq!(
            wgsl_struct_layout(&path, "ShadingUniform"),
            expected,
            "ShadingUniform layout of {name}"
        );
        let module = parse(&path);
        let (_, shading) = module
            .global_variables
            .iter()
            .find(|(_, var)| var.name.as_deref() == Some("shading"))
            .unwrap_or_else(|| panic!("{name} has no `shading`"));
        assert_eq!(
            shading.binding,
            Some(naga::ResourceBinding {
                group: 0,
                binding: 1
            }),
            "{name}"
        );
        assert_eq!(shading.space, naga::AddressSpace::Uniform, "{name}");
    }
}

/// D1: a shader is composed with `lighting.wgsl` exactly when it has the
/// lighting marker, and the lit shaders are the four the design names. A
/// shader that is not lit does not see the `ShadingUniform`.
#[test]
fn the_lighting_marker_is_in_exactly_the_lit_shaders() {
    assert_eq!(
        LIT_SHADERS,
        [
            "atom_impostor.wgsl",
            "bond_impostor.wgsl",
            "mesh.wgsl",
            "transparent_impostor.wgsl"
        ]
    );
    for path in all_shader_files() {
        let raw = std::fs::read_to_string(&path).unwrap();
        let file = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            raw.contains(LIGHTING_MARKER),
            LIT_SHADERS.contains(&file),
            "{file}"
        );
        let declares = parse(&path)
            .types
            .iter()
            .any(|(_, ty)| ty.name.as_deref() == Some("ShadingUniform"));
        assert_eq!(declares, LIT_SHADERS.contains(&file), "{file}");
    }
}

/// D1: the lighting is in `lighting.wgsl` only. No lit shader keeps its own
/// copy of a function the module provides, and the unused
/// `calculate_bond_lighting` is gone.
#[test]
fn no_lit_shader_keeps_a_copy_of_the_lighting() {
    let module_functions: Vec<String> = function_bodies(&strip_line_comments(LIGHTING_MODULE))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    for required in [
        "light_surface",
        "to_display",
        "apply_depth_cue",
        "camera_backward",
        "camera_view_vector",
        "fresnel_schlick",
    ] {
        assert!(
            module_functions.iter().any(|f| f == required),
            "lighting.wgsl has no {required}"
        );
    }
    for name in LIT_SHADERS {
        let raw = std::fs::read_to_string(shader_dir().join(name)).unwrap();
        for (function, _) in function_bodies(&strip_line_comments(&raw)) {
            assert!(
                !module_functions.contains(&function),
                "{name} declares its own `{function}`"
            );
            assert!(
                !function.starts_with("calculate_"),
                "{name} still has `{function}`"
            );
        }
    }
}

/// D5: every lit colour is fogged, and fogged in display space: each lit
/// shader passes every `to_display` result straight to `apply_depth_cue`.
/// Where the rim is mixed in, it is mixed into the fogged colour (D4), so a
/// selected atom at the back keeps its rim.
#[test]
fn every_lit_colour_goes_through_the_depth_cue_before_the_rim() {
    for name in LIT_SHADERS {
        let raw = strip_line_comments(&std::fs::read_to_string(shader_dir().join(name)).unwrap());
        let lit = raw.matches("to_display(").count();
        assert!(lit > 0, "{name} lights nothing");
        assert_eq!(
            raw.matches("apply_depth_cue(to_display(").count(),
            lit,
            "{name}: a `to_display` result skips the depth cue"
        );
        for (index, _) in raw.match_indices("apply_rim(") {
            assert!(
                raw[index..].starts_with("apply_rim(lit,"),
                "{name}: the rim is not mixed into `lit`"
            );
            let lit = raw[..index]
                .rfind("let lit = ")
                .expect("`lit` before the rim");
            assert!(
                raw[lit..].starts_with("let lit = apply_depth_cue("),
                "{name}: the rim is not mixed into the fogged colour"
            );
        }
    }
}

/// D1: the composed shader is the host's header (up to the lighting
/// marker), then the module, then the rest of the host. The module comes
/// after the uniforms it reads, and the AA D5 marker stays after the module,
/// so the per-sample cut still removes only the per-sample block.
#[test]
fn compose_lit_shader_puts_the_host_header_before_the_module() {
    for name in LIT_SHADERS {
        let host = std::fs::read_to_string(shader_dir().join(name)).unwrap();
        let composed = compose_lit_shader(&host);
        let split = host.find(LIGHTING_MARKER).unwrap();
        let (header, rest) = host.split_at(split);

        assert!(composed.starts_with(header), "{name}");
        assert!(composed.ends_with(rest), "{name}");
        assert_eq!(
            &composed[header.len()..header.len() + LIGHTING_MODULE.len()],
            LIGHTING_MODULE,
            "{name}"
        );
        assert_eq!(composed.len(), host.len() + LIGHTING_MODULE.len() + 1);

        let at = |needle: &str| {
            composed
                .find(needle)
                .unwrap_or_else(|| panic!("{name}: no `{needle}`"))
        };
        assert!(at("var<uniform> camera") < at("struct ShadingUniform"));
        assert!(at("var<uniform> model") < at("struct ShadingUniform"));
        if IMPOSTOR_SHADERS.contains(&name) {
            assert!(at("fn light_surface(") < at(PER_SAMPLE_SHADING_MARKER));
            assert!(without_per_sample_shading(&composed).contains("fn light_surface("));
        }
    }
}

#[test]
#[should_panic(expected = "exactly once, found 0")]
fn compose_lit_shader_panics_on_a_host_without_the_marker() {
    compose_lit_shader("struct CameraUniform { x: f32 };");
}

#[test]
#[should_panic(expected = "exactly once, found 2")]
fn compose_lit_shader_panics_on_a_host_with_two_markers() {
    compose_lit_shader(&format!("{LIGHTING_MARKER}\n{LIGHTING_MARKER}\n"));
}

/// D1: `lighting.wgsl` reads the host's `camera` without declaring it, so on
/// its own it is not a shader. (If it ever compiles alone, it has grown its
/// own `CameraUniform`, a seventh copy for `shader_test.rs` to miss.)
#[test]
fn the_lighting_module_does_not_compile_on_its_own() {
    assert!(naga::front::wgsl::parse_str(LIGHTING_MODULE).is_err());
    assert!(!LIGHTING_MODULE.contains("struct CameraUniform"));
}

// ---- Shadows, Phase 1 (doc/design_shadows.md D4, D7, D8) ------------------

use atomcad_renderer::shadow_fit::{ShadowCascade, ShadowUniform};

fn layout_of(members: &[(&str, usize)], size: usize) -> (Vec<(String, u32)>, u32) {
    (
        members
            .iter()
            .map(|(name, offset)| (name.to_string(), *offset as u32))
            .collect(),
        size as u32,
    )
}

/// D8: every lit shader, as composed, declares the `ShadowCascade` and
/// `ShadowUniform` of the Rust structs, byte for byte, and binds the
/// uniform, the map and the comparison sampler at group 2.
#[test]
fn every_shadow_uniform_in_wgsl_matches_the_rust_struct() {
    let cascade = layout_of(
        &[
            ("world_to_clip", offset_of!(ShadowCascade, world_to_clip)),
            ("split_far", offset_of!(ShadowCascade, split_far)),
            ("texel_world", offset_of!(ShadowCascade, texel_world)),
            (
                "filter_radius_uv",
                offset_of!(ShadowCascade, filter_radius_uv),
            ),
            ("depth_range", offset_of!(ShadowCascade, depth_range)),
        ],
        std::mem::size_of::<ShadowCascade>(),
    );
    let uniform = layout_of(
        &[
            ("cascades", offset_of!(ShadowUniform, cascades)),
            ("count", offset_of!(ShadowUniform, count)),
            ("blend_fraction", offset_of!(ShadowUniform, blend_fraction)),
            ("debug_tint", offset_of!(ShadowUniform, debug_tint)),
            ("_padding", offset_of!(ShadowUniform, _padding)),
        ],
        std::mem::size_of::<ShadowUniform>(),
    );
    for name in LIT_SHADERS {
        let path = shader_dir().join(name);
        assert_eq!(
            wgsl_struct_layout(&path, "ShadowCascade"),
            cascade,
            "{name}"
        );
        assert_eq!(
            wgsl_struct_layout(&path, "ShadowUniform"),
            uniform,
            "{name}"
        );
        let module = parse(&path);
        for (global, binding) in [("shadow", 0), ("shadow_map", 1), ("shadow_sampler", 2)] {
            let (_, var) = module
                .global_variables
                .iter()
                .find(|(_, var)| var.name.as_deref() == Some(global))
                .unwrap_or_else(|| panic!("{name} has no `{global}`"));
            assert_eq!(
                var.binding,
                Some(naga::ResourceBinding { group: 2, binding }),
                "{name}: {global}"
            );
        }
    }
}

/// D4: the caster entry points (`fs_shadow`, and the vertex stages the
/// caster pipelines use) read neither group 2 nor `shading`: they are drawn
/// on a layout without group 2, into the map group 2 would sample. From
/// naga's per-entry-point global usage.
#[test]
fn the_caster_entry_points_use_neither_group_2_nor_shading() {
    let casters: [(&str, &[&str]); 3] = [
        ("atom_impostor.wgsl", &["vs_main", "fs_shadow"]),
        ("bond_impostor.wgsl", &["vs_main", "fs_shadow"]),
        ("mesh.wgsl", &["vs_main"]),
    ];
    for (name, entries) in casters {
        let module = parse(&shader_dir().join(name));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name} does not validate: {e:?}"));
        for entry in entries {
            let index = module
                .entry_points
                .iter()
                .position(|e| e.name == *entry)
                .unwrap_or_else(|| panic!("{name} has no {entry}"));
            let usage = info.get_entry_point(index);
            for (handle, var) in module.global_variables.iter() {
                let forbidden = var.name.as_deref() == Some("shading")
                    || var.binding.as_ref().is_some_and(|b| b.group == 2);
                if forbidden {
                    assert!(
                        usage[handle].is_empty(),
                        "{name}: {entry} uses `{}`",
                        var.name.as_deref().unwrap_or("?")
                    );
                }
            }
        }
        // And the lit `fs_main` does sample the map, or this test proves
        // nothing.
        if let Some(index) = module.entry_points.iter().position(|e| e.name == "fs_main") {
            let usage = info.get_entry_point(index);
            let samples = module.global_variables.iter().any(|(handle, var)| {
                var.name.as_deref() == Some("shadow_map") && !usage[handle].is_empty()
            });
            assert!(samples, "{name}: fs_main does not sample the shadow map");
        }
    }
}

/// D4: `fs_shadow` sits above the per-sample marker, so the cut on an
/// adapter without per-sample shading keeps it.
#[test]
fn fs_shadow_survives_the_per_sample_cut() {
    for name in ["atom_impostor.wgsl", "bond_impostor.wgsl"] {
        let composed = source(&shader_dir().join(name));
        let cut = without_per_sample_shading(&composed);
        assert!(cut.contains("fn fs_shadow("), "{name}");
    }
}

/// The argument list of each call to `function` in `source`, split at its
/// top-level commas.
fn call_arguments(source: &str, function: &str) -> Vec<Vec<String>> {
    let needle = format!("{function}(");
    let mut calls = Vec::new();
    for (at, _) in source.match_indices(&needle) {
        // Not a longer name ending in `function`, and not the definition.
        let before = source[..at].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            || source[..at].trim_end().ends_with("fn")
        {
            continue;
        }
        let open = at + needle.len();
        let (mut depth, mut start, mut args) = (0, open, Vec::new());
        for (i, c) in source[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => {
                    args.push(source[start..open + i].trim().to_string());
                    break;
                }
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(source[start..open + i].trim().to_string());
                    start = open + i + 1;
                }
                _ => {}
            }
        }
        args.retain(|a| !a.is_empty());
        calls.push(args);
    }
    calls
}

/// D7: every `light_surface` call in a host passes, as its `key_visibility`,
/// `key_visibility(shadow_visibility(…))` of the point it lights, or the
/// literal `1.0`. A host that forgot the shadow would draw it unshadowed,
/// silently.
#[test]
fn every_light_surface_call_passes_a_shadow_lookup() {
    for name in LIT_SHADERS {
        let raw = strip_line_comments(&std::fs::read_to_string(shader_dir().join(name)).unwrap());
        let calls = call_arguments(&raw, "light_surface");
        assert!(!calls.is_empty(), "{name} lights nothing");
        for args in calls {
            assert_eq!(args.len(), 7, "{name}: {args:?}");
            let visibility = &args[6];
            let position = &args[0];
            assert!(
                visibility == "1.0"
                    || visibility
                        .starts_with(&format!("key_visibility(shadow_visibility({position},")),
                "{name}: light_surface({position}, …) passes `{visibility}`"
            );
        }
    }
}
