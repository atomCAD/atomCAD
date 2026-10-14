// The still-camera refined frame: when to refine, and the state that
// decides it (doc/design_viewport_antialiasing.md D5; Phase 3).

use atomcad_renderer::live_refinement::{
    LiveFrameKind, LiveRefinement, REFINE_DELAY, live_frame_kind, refine_due_in,
};
use std::time::{Duration, Instant};

const DELAY: Duration = REFINE_DELAY;
const EARLY: Duration = Duration::from_millis(10);
const LATE: Duration = Duration::from_secs(1);

// ---- `live_frame_kind`: the rows of D5's table ------------------------------

#[test]
fn without_refinement_every_frame_is_normal() {
    for changed in [false, true] {
        for since in [Duration::ZERO, EARLY, LATE] {
            for refined in [false, true] {
                assert_eq!(
                    live_frame_kind(changed, since, refined, None),
                    LiveFrameKind::Normal
                );
            }
        }
    }
}

#[test]
fn a_change_gives_a_normal_frame() {
    for refined in [false, true] {
        assert_eq!(
            live_frame_kind(true, LATE, refined, Some(DELAY)),
            LiveFrameKind::Normal
        );
    }
}

#[test]
fn before_the_delay_the_frame_is_normal() {
    assert_eq!(
        live_frame_kind(false, EARLY, false, Some(DELAY)),
        LiveFrameKind::Normal
    );
}

#[test]
fn after_the_delay_the_frame_is_refined_once_then_kept() {
    assert_eq!(
        live_frame_kind(false, DELAY, false, Some(DELAY)),
        LiveFrameKind::Refined
    );
    assert_eq!(
        live_frame_kind(false, LATE, true, Some(DELAY)),
        LiveFrameKind::Kept
    );
}

#[test]
fn a_zero_delay_refines_the_frame_after_a_change() {
    assert_eq!(
        live_frame_kind(true, Duration::ZERO, false, Some(Duration::ZERO)),
        LiveFrameKind::Normal
    );
    assert_eq!(
        live_frame_kind(false, Duration::ZERO, false, Some(Duration::ZERO)),
        LiveFrameKind::Refined
    );
}

#[test]
fn a_refined_frame_is_owed_only_after_a_normal_one() {
    assert_eq!(
        refine_due_in(LiveFrameKind::Normal, Duration::ZERO, Some(DELAY)),
        Some(DELAY)
    );
    assert_eq!(
        refine_due_in(LiveFrameKind::Normal, EARLY, Some(DELAY)),
        Some(DELAY - EARLY)
    );
    assert_eq!(
        refine_due_in(LiveFrameKind::Normal, Duration::ZERO, None),
        None
    );
    assert_eq!(
        refine_due_in(LiveFrameKind::Refined, Duration::ZERO, Some(DELAY)),
        None
    );
    assert_eq!(refine_due_in(LiveFrameKind::Kept, LATE, Some(DELAY)), None);
}

// ---- `LiveRefinement`: the state over a sequence of frames -----------------

const BG: [u8; 3] = [30, 30, 30];

/// One frame at `at`; a refined one is kept, as `Renderer::render` does.
fn frame(state: &mut LiveRefinement, background: [u8; 3], at: Instant) -> LiveFrameKind {
    let kind = state.decide(background, at, true);
    if kind == LiveFrameKind::Refined {
        state.keep(vec![1, 2, 3, 4]);
    }
    kind
}

#[test]
fn a_resting_view_refines_after_the_delay_and_then_keeps_the_image() {
    let mut state = LiveRefinement::new();
    let t0 = Instant::now();
    assert_eq!(frame(&mut state, BG, t0), LiveFrameKind::Normal);
    assert_eq!(state.refine_in(), Some(DELAY));
    assert_eq!(frame(&mut state, BG, t0 + EARLY), LiveFrameKind::Normal);
    assert_eq!(state.refine_in(), Some(DELAY - EARLY));
    assert_eq!(frame(&mut state, BG, t0 + DELAY), LiveFrameKind::Refined);
    assert_eq!(state.refine_in(), None);
    assert_eq!(frame(&mut state, BG, t0 + LATE), LiveFrameKind::Kept);
    assert_eq!(state.kept(), Some(&[1u8, 2, 3, 4][..]));
    assert_eq!(state.last_kind(), Some(LiveFrameKind::Kept));
}

#[test]
fn a_bump_after_a_refined_frame_gives_a_normal_frame_and_drops_the_image() {
    let mut state = LiveRefinement::new();
    let t0 = Instant::now();
    frame(&mut state, BG, t0);
    assert_eq!(frame(&mut state, BG, t0 + DELAY), LiveFrameKind::Refined);

    let before = state.generation();
    state.bump();
    assert!(state.generation() > before);
    assert_eq!(frame(&mut state, BG, t0 + LATE), LiveFrameKind::Normal);
    assert_eq!(state.kept(), None);
    // The delay runs from the change, not from the first frame.
    assert_eq!(state.refine_in(), Some(DELAY));
    assert_eq!(
        frame(&mut state, BG, t0 + LATE + DELAY),
        LiveFrameKind::Refined
    );
}

#[test]
fn a_new_background_counts_as_a_change() {
    let mut state = LiveRefinement::new();
    let t0 = Instant::now();
    frame(&mut state, BG, t0);
    assert_eq!(frame(&mut state, BG, t0 + DELAY), LiveFrameKind::Refined);
    assert_eq!(
        frame(&mut state, [0, 0, 0], t0 + LATE),
        LiveFrameKind::Normal
    );
}

#[test]
fn no_delay_or_no_refinement_never_refines() {
    let t0 = Instant::now();
    let mut no_delay = LiveRefinement::new();
    no_delay.set_delay(None);
    let mut unavailable = LiveRefinement::new();
    for i in 0..4 {
        let at = t0 + LATE * i;
        assert_eq!(frame(&mut no_delay, BG, at), LiveFrameKind::Normal);
        assert_eq!(unavailable.decide(BG, at, false), LiveFrameKind::Normal);
        assert_eq!(no_delay.refine_in(), None);
        assert_eq!(unavailable.refine_in(), None);
    }
}

// ---- Every mutating `pub fn` on `Renderer` bumps the live generation --------

/// The calls that bump: the bump itself, and the two helpers that write the
/// live uniforms (`write_view_uniforms(View::Live, ..)` bumps).
const BUMPS: [&str; 3] = [
    "self.bump_live_generation()",
    "self.update_camera_buffer()",
    "self.update_live_target()",
];

/// The mutating functions that draw rather than change what is drawn.
const ALLOWED: [&str; 4] = ["render", "render_still", "render_thumbnail", "render_image"];

/// The body of the `fn` whose `{` is the first one in `after`, and the text
/// after it.
fn split_body(after: &str) -> (&str, &str) {
    let open = after.find('{').expect("fn has a body");
    let mut depth = 0;
    for (i, c) in after[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (&after[open..open + i], &after[open + i..]);
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces");
}

/// D5: a mutating `pub fn` on `Renderer` that forgot to bump would leave the
/// kept refined image on screen after its change. Read from the source, the
/// way `shader_test.rs` reads the `.wgsl` files, so it also catches the next
/// setter, not only the ones that exist today. Mutation: remove the bump
/// from `set_surface_transparency_mode`.
#[test]
fn every_mutating_pub_fn_on_renderer_bumps_the_live_generation() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/renderer.rs"),
    )
    .unwrap();
    let start = source.find("\nimpl Renderer {").expect("impl Renderer");
    let mut rest = &source[start..];

    let mut checked = Vec::new();
    while let Some(at) = rest.find("\n    pub fn ") {
        let after = &rest[at + "\n    pub fn ".len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let signature = &after[..after.find('{').expect("fn has a body")];
        let (body, next) = split_body(after);
        rest = next;

        if !signature.contains("&mut self") || ALLOWED.contains(&name.as_str()) {
            continue;
        }
        assert!(
            BUMPS.iter().any(|bump| body.contains(bump)),
            "Renderer::{name} takes &mut self but does not bump the live generation"
        );
        checked.push(name);
    }
    // Some it must have found, so a parser that silently stopped finding
    // them fails.
    for expected in [
        "move_camera",
        "set_viewport_request",
        "set_render_quality",
        "update_all_gpu_meshes",
        "update_background_mesh",
        "set_surface_transparency_mode",
        "set_orthographic_mode",
        "set_camera_transform",
        "set_refine_delay",
    ] {
        assert!(
            checked.iter().any(|name| name == expected),
            "{expected} not checked; found {checked:?}"
        );
    }
}
