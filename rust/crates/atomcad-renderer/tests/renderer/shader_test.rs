// Shader structure, checked on the CPU with naga
// (doc/design_screen_space_lines.md, Testing layer 2).
//
// A broken `.wgsl` file otherwise fails only at the app's start-up, and a
// `CameraUniform` that differs between a shader and the Rust struct (issue
// #269) fails only as a wrong picture. Both are caught here, in plain
// `cargo test`, without a GPU.

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

fn all_shader_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(shader_dir())
        .expect("read src/")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wgsl"))
        .collect();
    files.sort();
    files
}

fn parse(path: &std::path::Path) -> naga::Module {
    let source = std::fs::read_to_string(path).expect("read shader");
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

/// `(member name, offset)` of the `CameraUniform` struct `path` declares,
/// and the struct's size.
fn wgsl_camera_uniform_layout(path: &std::path::Path) -> (Vec<(String, u32)>, u32) {
    let module = parse(path);
    let (_, ty) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some("CameraUniform"))
        .unwrap_or_else(|| panic!("{} declares no CameraUniform", path.display()));
    match &ty.inner {
        naga::TypeInner::Struct { members, span } => (
            members
                .iter()
                .map(|m| (m.name.clone().unwrap_or_default(), m.offset))
                .collect(),
            *span,
        ),
        other => panic!("CameraUniform is not a struct: {other:?}"),
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
        let layout = wgsl_camera_uniform_layout(&shader_dir().join(name));
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
/// depths. Per-instance values are `flat`
/// (doc/design_screen_space_lines.md D4).
#[test]
fn line_shader_outputs_carry_the_d4_interpolation() {
    use naga::Interpolation::{Flat, Linear};
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
    ];
    assert_eq!(
        outputs,
        expected
            .iter()
            .map(|(name, interpolation)| (name.to_string(), Some(*interpolation)))
            .collect::<Vec<_>>()
    );
}
