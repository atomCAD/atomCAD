//! Phase 2 of `doc/design_atom_labels.md` — the atom-label font atlas asset,
//! its generated metrics table, and the CPU text layout pass.
//!
//! All of this is GPU-free: the atlas is a PNG decoded on the CPU and the layout
//! pass is arithmetic over a `const` table, which is exactly why the design puts
//! layout in `label_atlas.rs` rather than in the shader.
//!
//! Two things here guard against silent breakage rather than obvious bugs:
//!
//! - The atlas PNG and `font_metrics.rs` are generated **together** by
//!   `cargo run --release --example gen_font_atlas`. Committing one without the
//!   other would garble every glyph, so the dimensions are asserted to agree.
//! - Glyph quads are **padded SDF cells**, not tight boxes. If the generator
//!   ever regressed to tight boxes the text would still render — just with the
//!   outline clipped flat at every glyph edge — so the padding is asserted
//!   explicitly.

use rust_lib_flutter_cad::renderer::font_metrics::{
    ATLAS_HEIGHT, ATLAS_WIDTH, CAP_HEIGHT_EM, FIRST_CHAR, GLYPH_METRICS, LAST_CHAR, SDF_SPREAD_EM,
};
use rust_lib_flutter_cad::renderer::label_atlas::{decode_font_atlas, glyph_metrics, layout_label};

/// Every printable-ASCII codepoint the atlas promises to cover.
fn ascii_glyphs() -> impl Iterator<Item = char> {
    (FIRST_CHAR as u32..=LAST_CHAR as u32).map(|cp| char::from_u32(cp).unwrap())
}

/// The committed PNG decodes, and its dimensions agree with the metrics table
/// the same generator run produced.
#[test]
fn atlas_decodes_with_the_dimensions_the_metrics_expect() {
    let atlas = decode_font_atlas();
    assert_eq!((atlas.width, atlas.height), (ATLAS_WIDTH, ATLAS_HEIGHT));
    assert_eq!(
        atlas.data.len(),
        (ATLAS_WIDTH * ATLAS_HEIGHT) as usize,
        "single-channel R8: one byte per texel"
    );
    // An SDF is not a blank image: it must contain both interior (> 0.5) and
    // exterior (< 0.5) texels.
    assert!(atlas.data.iter().any(|&v| v > 128));
    assert!(atlas.data.iter().any(|&v| v < 128));
}

/// The table covers exactly `0x20..=0x7E`, and every glyph's UV rect lies inside
/// the atlas.
#[test]
fn every_ascii_glyph_resolves_with_uvs_in_range() {
    assert_eq!(
        GLYPH_METRICS.len(),
        (LAST_CHAR as usize - FIRST_CHAR as usize) + 1,
        "the table must cover printable ASCII exactly"
    );

    for c in ascii_glyphs() {
        let m = glyph_metrics(c);
        for (name, uv) in [("uv_min", m.uv_min), ("uv_max", m.uv_max)] {
            for (axis, v) in uv.iter().enumerate() {
                assert!(
                    (0.0..=1.0).contains(v),
                    "{c:?} {name}[{axis}] = {v} is outside [0, 1]"
                );
            }
        }
        assert!(
            m.uv_min[0] <= m.uv_max[0] && m.uv_min[1] <= m.uv_max[1],
            "{c:?} has an inverted UV rect"
        );
        assert!(m.advance > 0.0, "{c:?} must advance the pen");
    }
}

/// Glyph quads are padded SDF cells: the tight ink box inflated by the spread on
/// all four sides. A tight-box quad would clip the outline band at every glyph
/// edge — the classic first bug of SDF text.
#[test]
fn glyph_quads_are_padded_sdf_cells_not_tight_boxes() {
    // A generated const, so this is a compile-time guard rather than a runtime one.
    const {
        assert!(
            SDF_SPREAD_EM > 0.0,
            "the atlas must encode a positive distance range"
        )
    };

    for c in ascii_glyphs() {
        let m = glyph_metrics(c);
        // A space has no ink and therefore no cell at all.
        if m.size == [0.0, 0.0] {
            continue;
        }
        for (axis, name) in [(0, "width"), (1, "height")] {
            assert!(
                m.size[axis] >= 2.0 * SDF_SPREAD_EM,
                "{c:?} {name} = {} is under 2 × spread ({}) — the cell is not padded",
                m.size[axis],
                2.0 * SDF_SPREAD_EM
            );
        }
    }
}

/// A single glyph's pen span is centered on zero.
///
/// Centering is **advance-based**, so the assertion is about the pen span, not
/// the box: the padded box deliberately overhangs the advance on both sides.
#[test]
fn one_glyph_centers_on_zero() {
    let layout = layout_label("C");
    assert_eq!(layout.glyphs.len(), 1);

    let g = layout.glyphs[0];
    assert!(
        (g.pen_x - -layout.total_width / 2.0).abs() < 1e-6,
        "the pen must start at -total_width / 2"
    );
    assert!(
        (g.pen_x + g.advance - layout.total_width / 2.0).abs() < 1e-6,
        "the pen must end at +total_width / 2"
    );
}

/// A multi-glyph string's width is the sum of its advances, and its pen span
/// stays centered on zero.
#[test]
fn three_glyph_string_sums_advances_and_stays_centered() {
    let layout = layout_label("CO2");
    assert_eq!(layout.glyphs.len(), 3);

    let expected: f32 = "CO2".chars().map(|c| glyph_metrics(c).advance).sum();
    assert!((layout.total_width - expected).abs() < 1e-6);

    let first = layout.glyphs.first().unwrap();
    let last = layout.glyphs.last().unwrap();
    assert!((first.pen_x - -layout.total_width / 2.0).abs() < 1e-6);
    assert!((last.pen_x + last.advance - layout.total_width / 2.0).abs() < 1e-6);

    // Pens run left to right, each advancing by the previous glyph's advance.
    for pair in layout.glyphs.windows(2) {
        assert!((pair[1].pen_x - (pair[0].pen_x + pair[0].advance)).abs() < 1e-6);
    }
}

/// A space advances the pen but emits no box — it has an advance and no ink.
#[test]
fn space_advances_without_emitting_a_box() {
    let layout = layout_label("A B");
    assert_eq!(layout.glyphs.len(), 2, "the space must not emit a quad");

    let space_advance = glyph_metrics(' ').advance;
    assert!(space_advance > 0.0);
    assert!(
        (layout.total_width - "A B".chars().map(|c| glyph_metrics(c).advance).sum::<f32>()).abs()
            < 1e-6,
        "the space's advance still counts toward the width"
    );

    // The gap between the two visible glyphs includes the space's advance.
    let gap = layout.glyphs[1].pen_x - layout.glyphs[0].pen_x;
    assert!((gap - (glyph_metrics('A').advance + space_advance)).abs() < 1e-6);
}

/// A char outside the atlas degrades to `?` rather than erroring or vanishing.
#[test]
fn non_ascii_falls_back_to_question_mark() {
    assert_eq!(
        format!("{:?}", glyph_metrics('Ω')),
        format!("{:?}", glyph_metrics('?')),
        "an out-of-atlas char must resolve to the fallback glyph"
    );

    let fallback = layout_label("Ω");
    let question = layout_label("?");
    assert_eq!(
        fallback, question,
        "laying out a non-ASCII char must match laying out `?`"
    );
}

/// Vertical centering is cap-height based, so labels made of different glyphs
/// share a vertical center rather than drifting by their own ink extents.
#[test]
fn cap_height_centering_is_stable_across_strings() {
    const {
        assert!(
            CAP_HEIGHT_EM > 0.0,
            "the generated cap height must be positive"
        )
    };

    // 'C' and 'S' are both cap-height glyphs; 'i' is shorter with a dot. The cap
    // band is centered regardless, so the baseline sits at the same y for both.
    let baseline_of = |text: &str| -> f32 {
        let layout = layout_label(text);
        let g = layout.glyphs[0];
        // min.y is the padded cell's bottom: baseline + bearing.y.
        g.min[1] - glyph_metrics(text.chars().next().unwrap()).bearing[1]
    };

    let c_baseline = baseline_of("C");
    let si_baseline = baseline_of("Si");
    assert!(
        (c_baseline - si_baseline).abs() < 1e-6,
        "\"C\" and \"Si\" must share a baseline (and so a vertical center)"
    );
    assert!(
        (c_baseline - -CAP_HEIGHT_EM / 2.0).abs() < 1e-6,
        "the baseline sits half a cap height below the anchor"
    );
}

/// The empty string lays out to nothing — `label: \"\"` is the documented reset
/// value, so it must not produce a degenerate quad.
#[test]
fn empty_string_lays_out_to_nothing() {
    let layout = layout_label("");
    assert!(layout.glyphs.is_empty());
    assert_eq!(layout.total_width, 0.0);
}
