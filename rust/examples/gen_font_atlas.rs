//! Offline generator for the atom-label SDF font atlas
//! (Phase 2 of `doc/design_atom_labels.md`).
//!
//! Run by hand when the font or the glyph set changes, which is approximately
//! never:
//!
//! ```text
//! cd rust && cargo run --release --example gen_font_atlas
//! ```
//!
//! It writes two committed artifacts:
//!
//! - `rust/assets/font_atlas.png` — a single-channel (R8) signed-distance-field
//!   atlas covering printable ASCII `0x20..=0x7E`.
//! - `rust/src/renderer/font_metrics.rs` — a `const` metrics table, so the
//!   shipping binary parses no font and reads no metrics file at startup.
//!
//! `ab_glyph` is a **dev-dependency**: it is used here and nowhere else, so the
//! shipping `cdylib` gains no dependency from this file.
//!
//! ## Coordinate conventions
//!
//! Everything the metrics table records is in **em units with y up and the
//! baseline at y = 0** — the space the CPU layout pass (`renderer::label_atlas`)
//! works in. `ab_glyph`, by contrast, hands back pixels with y down, so this
//! generator negates y when it converts. Atlas UVs stay y-down, as textures are.
//!
//! ## Padded SDF cells
//!
//! `uv`, `size` and `bearing` describe the **padded** SDF cell — the glyph's
//! tight bounding box inflated by the SDF spread on all four sides — never the
//! tight box. The outline band and the antialiasing fringe live outside the
//! tight box, so tight-box quads would clip the outline to a hard rectangle at
//! every glyph edge. Only `advance` is purely typographic: pen movement is
//! unaffected by padding, and the overlapping padded cells of adjacent glyphs
//! are harmless because their SDF texels are empty where the glyphs don't reach.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::fmt::Write as _;
use std::path::PathBuf;

/// First and last codepoint in the atlas (printable ASCII). Element symbols and
/// tag names are ASCII, so this covers the whole feature; anything else degrades
/// to `?` at layout time.
const FIRST_CHAR: char = ' '; // 0x20
const LAST_CHAR: char = '~'; // 0x7E
const GLYPH_COUNT: usize = (LAST_CHAR as usize - FIRST_CHAR as usize) + 1;

/// Resolution of the emitted SDF, in atlas texels per em.
const SDF_PX_PER_EM: f32 = 32.0;

/// Supersampling factor: the glyph is rasterized at `SS ×` the SDF resolution
/// and the distance transform runs against that hi-res coverage bitmap.
const SS: f32 = 8.0;

/// Rasterization resolution, in hi-res pixels per em.
const HI_PX_PER_EM: f32 = SDF_PX_PER_EM * SS;

/// The distance range encoded around each glyph edge, in em. The fragment
/// shader's outline band must stay well inside this: a band wider than the
/// encoded range clips flat and the outline degrades to a hard edge.
const SPREAD_EM: f32 = 0.125;

/// Gap between packed cells, in atlas texels. The atlas is cleared to "far
/// outside", so a bilinear sample that strays across the gap reads empty space
/// and the fragment is discarded rather than picking up a neighbour's glyph.
const CELL_GAP: u32 = 1;

/// Columns in the packing grid. 10 × 10 holds all 95 glyphs.
const GRID_COLS: usize = 10;

/// A glyph rasterized to SDF texels, before packing.
struct RenderedGlyph {
    /// Padded cell dimensions, in atlas texels. `(0, 0)` for a blank glyph
    /// (a space has an advance but no outline).
    width_px: u32,
    height_px: u32,
    /// Padded cell size in em — always `dim_px / SDF_PX_PER_EM`, so texels map
    /// exactly onto the quad.
    size_em: [f32; 2],
    /// Offset from the pen (baseline origin) to the cell's bottom-left, em, y up.
    bearing_em: [f32; 2],
    /// Pen movement, em. Purely typographic — unaffected by the padding.
    advance_em: f32,
    /// R8 SDF texels, row-major, y down. `0.5` (128) is the glyph edge.
    sdf: Vec<u8>,
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let font_path = manifest_dir.join("assets/DejaVuSans-Bold.ttf");
    let font_bytes = std::fs::read(&font_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", font_path.display()));
    let font = FontRef::try_from_slice(&font_bytes).expect("not a valid font file");

    println!("rendering {GLYPH_COUNT} glyphs at {SDF_PX_PER_EM} px/em (spread {SPREAD_EM} em)…");

    let glyphs: Vec<RenderedGlyph> = (FIRST_CHAR as u32..=LAST_CHAR as u32)
        .map(|cp| render_glyph(&font, char::from_u32(cp).unwrap()))
        .collect();

    // Cap height drives vertical centering at layout time: the label's cap band
    // is what gets centered on the atom, so "C" and "Si" share a vertical
    // center. Take it from 'H', whose tight top is the cap line by definition.
    let cap_height_em = tight_bounds_em(&font, 'H')
        .map(|b| b.top)
        .expect("the font must have an 'H' glyph to derive cap height from");

    let (atlas, atlas_w, atlas_h, uvs) = pack(&glyphs);

    let atlas_path = manifest_dir.join("assets/font_atlas.png");
    image::save_buffer(
        &atlas_path,
        &atlas,
        atlas_w,
        atlas_h,
        image::ExtendedColorType::L8,
    )
    .expect("failed to write the atlas PNG");
    println!("wrote {} ({atlas_w}×{atlas_h})", atlas_path.display());

    let metrics_path = manifest_dir.join("src/renderer/font_metrics.rs");
    let source = emit_metrics(&glyphs, &uvs, atlas_w, atlas_h, cap_height_em);
    std::fs::write(&metrics_path, source).expect("failed to write the metrics table");
    println!("wrote {}", metrics_path.display());
    println!("run `cargo fmt` to normalize the generated table");
}

/// The `PxScale` at which one **em square** rasterizes to `HI_PX_PER_EM` pixels.
///
/// This indirection is load-bearing: `ab_glyph` scales by the font's *height*
/// (`ascent - descent`), **not** by the em square, so handing it `HI_PX_PER_EM`
/// directly would silently make every "em" in the metrics table mean
/// `ascent - descent` instead — off by ~16% for DejaVu, and wrong in a way
/// nothing downstream could notice. Cancelling the ratio here is what lets the
/// table say "em" and mean it.
fn em_px_scale(font: &FontRef<'_>) -> PxScale {
    PxScale::from(HI_PX_PER_EM * font.height_unscaled() / font.units_per_em().unwrap())
}

/// A glyph's tight bounding box in em units, y up, baseline at 0.
struct TightBounds {
    left: f32,
    right: f32,
    /// Top edge, y up: positive above the baseline.
    top: f32,
    /// Bottom edge, y up: negative below the baseline (descenders).
    bottom: f32,
}

fn tight_bounds_em(font: &FontRef<'_>, c: char) -> Option<TightBounds> {
    let glyph = font.glyph_id(c).with_scale(em_px_scale(font));
    let outlined = font.outline_glyph(glyph)?;
    let b = outlined.px_bounds();
    // `px_bounds` is y-down px relative to the glyph origin; negate y for y-up em.
    Some(TightBounds {
        left: b.min.x / HI_PX_PER_EM,
        right: b.max.x / HI_PX_PER_EM,
        top: -b.min.y / HI_PX_PER_EM,
        bottom: -b.max.y / HI_PX_PER_EM,
    })
}

fn render_glyph(font: &FontRef<'_>, c: char) -> RenderedGlyph {
    let advance_em = font
        .as_scaled(em_px_scale(font))
        .h_advance(font.glyph_id(c))
        / HI_PX_PER_EM;

    let glyph = font.glyph_id(c).with_scale(em_px_scale(font));
    let Some(outlined) = font.outline_glyph(glyph) else {
        // Blank glyph (space): advance only, no cell.
        return RenderedGlyph {
            width_px: 0,
            height_px: 0,
            size_em: [0.0, 0.0],
            bearing_em: [0.0, 0.0],
            advance_em,
            sdf: Vec::new(),
        };
    };

    let px_bounds = outlined.px_bounds();
    let hi_w = px_bounds.width().ceil() as usize;
    let hi_h = px_bounds.height().ceil() as usize;

    // Hi-res coverage bitmap, origin at `px_bounds.min`. Coverage outside the
    // tight bounds is zero by construction, so the bitmap need not be padded —
    // the padding is handled by sampling out-of-bounds as "outside".
    let mut coverage = vec![0.0f32; hi_w * hi_h];
    outlined.draw(|x, y, c| {
        let (x, y) = (x as usize, y as usize);
        if x < hi_w && y < hi_h {
            coverage[y * hi_w + x] = c;
        }
    });
    let inside = |x: isize, y: isize| -> bool {
        if x < 0 || y < 0 || x as usize >= hi_w || y as usize >= hi_h {
            return false;
        }
        coverage[y as usize * hi_w + x as usize] >= 0.5
    };

    // Edge points: the midpoint between every inside/outside pair of adjacent
    // hi-res pixels, in hi-res pixel coordinates. This is the set the distance
    // transform measures against.
    let mut edges: Vec<(f32, f32)> = Vec::new();
    for y in 0..hi_h as isize {
        for x in 0..hi_w as isize {
            let here = inside(x, y);
            if here != inside(x + 1, y) {
                edges.push((x as f32 + 1.0, y as f32 + 0.5));
            }
            if here != inside(x, y + 1) {
                edges.push((x as f32 + 0.5, y as f32 + 1.0));
            }
        }
    }

    // The padded cell: tight box inflated by the spread on all four sides,
    // rounded out to whole texels (the slack lands outside the glyph, so every
    // side keeps at least `spread` of padding).
    let tight = TightBounds {
        left: px_bounds.min.x / HI_PX_PER_EM,
        right: px_bounds.max.x / HI_PX_PER_EM,
        top: -px_bounds.min.y / HI_PX_PER_EM,
        bottom: -px_bounds.max.y / HI_PX_PER_EM,
    };
    let bearing_em = [tight.left - SPREAD_EM, tight.bottom - SPREAD_EM];
    let width_px = (((tight.right + SPREAD_EM) - bearing_em[0]) * SDF_PX_PER_EM).ceil() as u32;
    let height_px = (((tight.top + SPREAD_EM) - bearing_em[1]) * SDF_PX_PER_EM).ceil() as u32;
    let size_em = [
        width_px as f32 / SDF_PX_PER_EM,
        height_px as f32 / SDF_PX_PER_EM,
    ];

    let mut sdf = vec![0u8; (width_px * height_px) as usize];
    for j in 0..height_px {
        for i in 0..width_px {
            // Texel center in em (y up), then in hi-res pixel space.
            let x_em = bearing_em[0] + (i as f32 + 0.5) / SDF_PX_PER_EM;
            let y_em = bearing_em[1] + size_em[1] - (j as f32 + 0.5) / SDF_PX_PER_EM;
            let hx = x_em * HI_PX_PER_EM - px_bounds.min.x;
            let hy = -y_em * HI_PX_PER_EM - px_bounds.min.y;

            let mut nearest_sq = f32::INFINITY;
            for &(ex, ey) in &edges {
                let d = (hx - ex) * (hx - ex) + (hy - ey) * (hy - ey);
                if d < nearest_sq {
                    nearest_sq = d;
                }
            }
            let dist_em = nearest_sq.sqrt() / HI_PX_PER_EM;
            let signed = if inside(hx.floor() as isize, hy.floor() as isize) {
                dist_em
            } else {
                -dist_em
            };
            // 0.5 is the edge; ±spread maps to 1.0 / 0.0.
            let v = (0.5 + signed / (2.0 * SPREAD_EM)).clamp(0.0, 1.0);
            sdf[(j * width_px + i) as usize] = (v * 255.0).round() as u8;
        }
    }

    RenderedGlyph {
        width_px,
        height_px,
        size_em,
        bearing_em,
        advance_em,
        sdf,
    }
}

/// Pack the rendered cells into a uniform grid and return the atlas plus each
/// glyph's UV rect (`[u_min, v_min, u_max, v_max]`, y down).
fn pack(glyphs: &[RenderedGlyph]) -> (Vec<u8>, u32, u32, Vec<[f32; 4]>) {
    let cell_w = glyphs.iter().map(|g| g.width_px).max().unwrap() + CELL_GAP;
    let cell_h = glyphs.iter().map(|g| g.height_px).max().unwrap() + CELL_GAP;
    let rows = glyphs.len().div_ceil(GRID_COLS);
    let atlas_w = cell_w * GRID_COLS as u32 + CELL_GAP;
    let atlas_h = cell_h * rows as u32 + CELL_GAP;

    // Cleared to 0 = "far outside", so gaps and slack read as empty space.
    let mut atlas = vec![0u8; (atlas_w * atlas_h) as usize];
    let mut uvs = Vec::with_capacity(glyphs.len());

    for (idx, g) in glyphs.iter().enumerate() {
        let x0 = CELL_GAP + (idx % GRID_COLS) as u32 * cell_w;
        let y0 = CELL_GAP + (idx / GRID_COLS) as u32 * cell_h;
        for j in 0..g.height_px {
            for i in 0..g.width_px {
                atlas[((y0 + j) * atlas_w + (x0 + i)) as usize] =
                    g.sdf[(j * g.width_px + i) as usize];
            }
        }
        uvs.push([
            x0 as f32 / atlas_w as f32,
            y0 as f32 / atlas_h as f32,
            (x0 + g.width_px) as f32 / atlas_w as f32,
            (y0 + g.height_px) as f32 / atlas_h as f32,
        ]);
    }

    (atlas, atlas_w, atlas_h, uvs)
}

fn emit_metrics(
    glyphs: &[RenderedGlyph],
    uvs: &[[f32; 4]],
    atlas_w: u32,
    atlas_h: u32,
    cap_height_em: f32,
) -> String {
    let mut s = String::new();
    s.push_str(
        "//! Glyph metrics for the atom-label SDF font atlas — GENERATED, DO NOT EDIT.\n\
         //!\n\
         //! Regenerate with `cargo run --release --example gen_font_atlas` (see\n\
         //! `rust/examples/gen_font_atlas.rs`) after changing the font or the glyph set.\n\
         //! A `const` table means the shipping binary parses no font and reads no metrics\n\
         //! file at startup.\n\
         //!\n\
         //! Units are **em, y up, baseline at 0**; UVs are atlas-relative, y down.\n\
         //! `uv`, `size` and `bearing` describe the **padded SDF cell** (the tight glyph\n\
         //! box inflated by `SDF_SPREAD_EM` on all four sides), never the tight box —\n\
         //! see `doc/design_atom_labels.md` §The font atlas asset.\n\n",
    );
    writeln!(s, "/// Per-glyph metrics for one padded SDF cell.").unwrap();
    s.push_str(
        "#[derive(Debug, Clone, Copy)]\n\
         pub struct GlyphMetrics {\n\
         \x20   /// Atlas UV of the cell's top-left corner (y down).\n\
         \x20   pub uv_min: [f32; 2],\n\
         \x20   /// Atlas UV of the cell's bottom-right corner (y down).\n\
         \x20   pub uv_max: [f32; 2],\n\
         \x20   /// Padded cell size in em. `[0.0, 0.0]` for a blank glyph (e.g. space).\n\
         \x20   pub size: [f32; 2],\n\
         \x20   /// Offset from the pen to the cell's bottom-left corner, em, y up.\n\
         \x20   pub bearing: [f32; 2],\n\
         \x20   /// Pen advance, em. Typographic — unaffected by the cell padding.\n\
         \x20   pub advance: f32,\n\
         }\n\n",
    );
    writeln!(s, "/// Atlas width in texels.").unwrap();
    writeln!(s, "pub const ATLAS_WIDTH: u32 = {atlas_w};").unwrap();
    writeln!(s, "/// Atlas height in texels.").unwrap();
    writeln!(s, "pub const ATLAS_HEIGHT: u32 = {atlas_h};\n").unwrap();
    s.push_str(
        "/// The distance range encoded around each glyph edge, in em: an SDF value of\n\
         /// 1.0 means `SDF_SPREAD_EM` inside the glyph, 0.0 the same distance outside,\n\
         /// and 0.5 is the edge. The fragment shader's outline band must stay well\n\
         /// inside this range or it clips flat.\n",
    );
    writeln!(
        s,
        "pub const SDF_SPREAD_EM: f32 = {cap:?};",
        cap = SPREAD_EM
    )
    .unwrap();
    s.push_str(
        "\n/// Cap height in em — the band the layout pass centers on the atom, so that\n\
         /// labels of different glyphs share a vertical center.\n",
    );
    writeln!(s, "pub const CAP_HEIGHT_EM: f32 = {cap_height_em:?};\n").unwrap();
    writeln!(s, "/// First codepoint in the table.").unwrap();
    writeln!(s, "pub const FIRST_CHAR: char = {FIRST_CHAR:?};").unwrap();
    writeln!(s, "/// Last codepoint in the table (inclusive).").unwrap();
    writeln!(s, "pub const LAST_CHAR: char = {LAST_CHAR:?};\n").unwrap();
    s.push_str(
        "/// Metrics for every printable-ASCII glyph, indexed by\n\
         /// `codepoint - FIRST_CHAR as u32`.\n",
    );
    writeln!(
        s,
        "pub const GLYPH_METRICS: [GlyphMetrics; {}] = [",
        glyphs.len()
    )
    .unwrap();
    for (idx, (g, uv)) in glyphs.iter().zip(uvs).enumerate() {
        let c = char::from_u32(FIRST_CHAR as u32 + idx as u32).unwrap();
        writeln!(s, "    // {c:?}").unwrap();
        writeln!(s, "    GlyphMetrics {{").unwrap();
        writeln!(s, "        uv_min: [{:?}, {:?}],", uv[0], uv[1]).unwrap();
        writeln!(s, "        uv_max: [{:?}, {:?}],", uv[2], uv[3]).unwrap();
        writeln!(s, "        size: [{:?}, {:?}],", g.size_em[0], g.size_em[1]).unwrap();
        writeln!(
            s,
            "        bearing: [{:?}, {:?}],",
            g.bearing_em[0], g.bearing_em[1]
        )
        .unwrap();
        writeln!(s, "        advance: {:?},", g.advance_em).unwrap();
        writeln!(s, "    }},").unwrap();
    }
    s.push_str("];\n");
    s
}
