//! The atom-label SDF font atlas: asset decoding, glyph lookup, and CPU text
//! layout (Phase 2 of `doc/design_atom_labels.md`).
//!
//! Layout runs on the CPU so the label shader stays trivial: given a string,
//! [`layout_label`] returns positioned, UV'd glyph boxes that the tessellator
//! scales into world units and turns into billboard quads.
//!
//! **This module needs no atlas handle to lay text out.** Glyph metrics are a
//! `const` table ([`crate::renderer::font_metrics`]), so the layout path is
//! pure arithmetic and unit-testable without a GPU. The PNG is decoded exactly
//! once, renderer-side, to upload the texture.
//!
//! Units are **em, y up**, with the label's cap band centered on the origin
//! (which is the atom's anchor). The caller multiplies by the world-space label
//! scale; see `doc/design_atom_labels.md` §Label size.

use crate::renderer::font_metrics::{
    ATLAS_HEIGHT, ATLAS_WIDTH, CAP_HEIGHT_EM, FIRST_CHAR, GLYPH_METRICS, GlyphMetrics, LAST_CHAR,
};

/// The glyph substituted for anything the atlas does not cover.
///
/// Non-ASCII is deliberately **degraded, not validated**: `{tag}` expands to
/// arbitrary user text, so the fallback has to exist regardless, and a per-atom
/// expansion error would have nowhere good to surface. A user who types `Ω` sees
/// `?`, which is visible and self-explanatory.
const FALLBACK_CHAR: char = '?';

/// The decoded font atlas, ready to upload as an R8 texture.
pub struct FontAtlasImage {
    /// Single-channel SDF texels, row-major, y down. 128 (0.5) is the glyph edge.
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decode the committed SDF atlas.
///
/// The PNG is embedded with `include_bytes!` — the same idea as the WGSL
/// sources' `include_str!`, so the asset ships inside the binary rather than
/// alongside it. Called once, from `Renderer::new`.
///
/// Panics if the asset fails to decode or its dimensions disagree with the
/// generated metrics table: both mean the committed atlas and `font_metrics.rs`
/// were not generated together, which would garble every glyph on screen.
pub fn decode_font_atlas() -> FontAtlasImage {
    const ATLAS_PNG: &[u8] = include_bytes!("../../assets/font_atlas.png");

    let image = image::load_from_memory_with_format(ATLAS_PNG, image::ImageFormat::Png)
        .expect("font_atlas.png failed to decode");
    let luma = image.to_luma8();
    let (width, height) = luma.dimensions();
    assert_eq!(
        (width, height),
        (ATLAS_WIDTH, ATLAS_HEIGHT),
        "font_atlas.png is {width}×{height} but font_metrics.rs expects \
         {ATLAS_WIDTH}×{ATLAS_HEIGHT} — regenerate both with \
         `cargo run --release --example gen_font_atlas`"
    );

    FontAtlasImage {
        data: luma.into_raw(),
        width,
        height,
    }
}

/// Metrics for `c`, falling back to [`FALLBACK_CHAR`] outside the atlas.
pub fn glyph_metrics(c: char) -> &'static GlyphMetrics {
    fn index_of(c: char) -> Option<usize> {
        if c < FIRST_CHAR || c > LAST_CHAR {
            return None;
        }
        Some(c as usize - FIRST_CHAR as usize)
    }
    let index = index_of(c).or_else(|| index_of(FALLBACK_CHAR)).expect(
        "the fallback glyph must be inside the atlas range — the generated table is corrupt",
    );
    &GLYPH_METRICS[index]
}

/// One laid-out glyph: a rectangle in em units plus its atlas UV rect.
///
/// The rectangle is the **padded SDF cell**, not the glyph's tight box, so
/// adjacent boxes overlap slightly and the box extends past the advance span on
/// both sides. That is intended — the outline band and the antialiasing fringe
/// live in the padding (see `doc/design_atom_labels.md` §The font atlas asset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphBox {
    /// Bottom-left corner, em, y up.
    pub min: [f32; 2],
    /// Top-right corner, em, y up.
    pub max: [f32; 2],
    /// Atlas UV of the corner at `[min.x, max.y]` (top-left; UVs are y down).
    pub uv_min: [f32; 2],
    /// Atlas UV of the corner at `[max.x, min.y]` (bottom-right; UVs are y down).
    pub uv_max: [f32; 2],
    /// Pen x this glyph was placed at, em. Layout metadata: the tessellator does
    /// not need it, but it is what makes the advance-based centering contract
    /// checkable.
    pub pen_x: f32,
    /// This glyph's advance, em.
    pub advance: f32,
}

/// A laid-out label: glyph boxes plus the advance span they were centered on.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelLayout {
    /// One box per **visible** glyph. Blank glyphs (spaces) advance the pen but
    /// emit no box, so this can be shorter than the string.
    pub glyphs: Vec<GlyphBox>,
    /// Sum of every char's advance, em — including blanks. The layout is
    /// centered on this span: the pen runs `[-total_width / 2, +total_width / 2]`.
    pub total_width: f32,
}

/// Lay `text` out in em units, centered on the origin.
///
/// Horizontal centering is **advance-based** (the pen span is centered, not the
/// union of the padded boxes); vertical centering puts the cap band on the
/// origin, so labels of different glyphs share a vertical center — `"C"` and
/// `"Si"` line up rather than drifting by their own ink extents.
pub fn layout_label(text: &str) -> LabelLayout {
    let total_width: f32 = text.chars().map(|c| glyph_metrics(c).advance).sum();

    // Cap band centered on the anchor: the baseline sits half a cap height below.
    let baseline_y = -CAP_HEIGHT_EM / 2.0;

    let mut pen_x = -total_width / 2.0;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let m = glyph_metrics(c);
        // A blank glyph (space) has an empty cell: advance, emit nothing.
        if m.size[0] > 0.0 && m.size[1] > 0.0 {
            let min = [pen_x + m.bearing[0], baseline_y + m.bearing[1]];
            glyphs.push(GlyphBox {
                min,
                max: [min[0] + m.size[0], min[1] + m.size[1]],
                uv_min: m.uv_min,
                uv_max: m.uv_max,
                pen_x,
                advance: m.advance,
            });
        }
        pen_x += m.advance;
    }

    LabelLayout {
        glyphs,
        total_width,
    }
}
