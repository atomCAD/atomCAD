//! The still-camera refined frame (`doc/design_viewport_antialiasing.md` D5):
//! when the live view has not changed for [`REFINE_DELAY`], the renderer draws
//! one frame with per-sample impostor shading, keeps its bytes and hands them
//! out again until something changes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How long the live view must stay unchanged before the refined frame is
/// drawn. Long enough that an unrelated repaint in the middle of a drag does
/// not trigger it, short enough to read as "as soon as I let go".
pub const REFINE_DELAY: Duration = Duration::from_millis(150);

/// What one live frame is (D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveFrameKind {
    /// Drawn with per-pixel impostor shading (D4).
    Normal,
    /// Drawn with per-sample impostor shading, and kept.
    Refined,
    /// The kept refined frame, handed out again with no GPU work.
    Kept,
}

/// D5's table. `refine_after` is the delay after the last change at which
/// the refined frame is due, `None` when there is no refinement at all (a
/// 1-sample live target, no refined pipelines, or the delay switched off).
pub fn live_frame_kind(
    generation_changed: bool,
    since_change: Duration,
    already_refined: bool,
    refine_after: Option<Duration>,
) -> LiveFrameKind {
    let Some(delay) = refine_after else {
        return LiveFrameKind::Normal;
    };
    if generation_changed || since_change < delay {
        LiveFrameKind::Normal
    } else if already_refined {
        LiveFrameKind::Kept
    } else {
        LiveFrameKind::Refined
    }
}

/// How long after a frame of `kind` the refined frame is due, `None` when
/// none is owed: after a refined or kept frame, or without refinement. This
/// is what Flutter arms its timer with (`APIFrameInfo::refine_in`), since a
/// viewport at rest asks for no frames on its own.
pub fn refine_due_in(
    kind: LiveFrameKind,
    since_change: Duration,
    refine_after: Option<Duration>,
) -> Option<Duration> {
    match (kind, refine_after) {
        (LiveFrameKind::Normal, Some(delay)) => Some(delay.saturating_sub(since_change)),
        _ => None,
    }
}

/// The renderer's refinement state. The generation is atomic because it is
/// bumped through `&self` (`Renderer::write_view_uniforms`); the rest changes
/// only in `Renderer::render`.
pub struct LiveRefinement {
    generation: AtomicU64,
    /// The generation of the last live render.
    seen_generation: u64,
    /// The background of the last live render.
    background: Option<[u8; 3]>,
    /// When the generation or the background last changed.
    changed_at: Instant,
    delay: Option<Duration>,
    last_kind: Option<LiveFrameKind>,
    refine_in: Option<Duration>,
    /// The refined frame's bytes, until the next change.
    kept: Option<Vec<u8>>,
}

impl Default for LiveRefinement {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRefinement {
    pub fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            seen_generation: 0,
            background: None,
            changed_at: Instant::now(),
            delay: Some(REFINE_DELAY),
            last_kind: None,
            refine_in: None,
            kept: None,
        }
    }

    /// Records that what the live frame shows may have changed. Over-bumping
    /// only postpones the refined frame; a missed bump leaves a stale kept
    /// image on screen.
    pub fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// `None`: never refine on its own (tests use it to stay deterministic).
    pub fn set_delay(&mut self, delay: Option<Duration>) {
        self.delay = delay;
    }

    /// Decides the next live frame at `now` on `background`. `available` is
    /// whether the live target can be refined at all.
    pub fn decide(&mut self, background: [u8; 3], now: Instant, available: bool) -> LiveFrameKind {
        let generation = self.generation();
        let changed = generation != self.seen_generation || self.background != Some(background);
        if changed {
            self.seen_generation = generation;
            self.background = Some(background);
            self.changed_at = now;
            self.kept = None;
        }
        let refine_after = self.delay.filter(|_| available);
        let since_change = now.saturating_duration_since(self.changed_at);
        let kind = live_frame_kind(changed, since_change, self.kept.is_some(), refine_after);
        self.last_kind = Some(kind);
        self.refine_in = refine_due_in(kind, since_change, refine_after);
        kind
    }

    /// Keeps the bytes of the refined frame just drawn.
    pub fn keep(&mut self, pixels: Vec<u8>) {
        self.kept = Some(pixels);
    }

    pub fn kept(&self) -> Option<&[u8]> {
        self.kept.as_deref()
    }

    pub fn last_kind(&self) -> Option<LiveFrameKind> {
        self.last_kind
    }

    /// From the last decision: how long until the refined frame is due.
    pub fn refine_in(&self) -> Option<Duration> {
        self.refine_in
    }
}
