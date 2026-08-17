//! Glyph metrics for the atom-label SDF font atlas — GENERATED, DO NOT EDIT.
//!
//! Regenerate with `cargo run --release --example gen_font_atlas` (see
//! `rust/examples/gen_font_atlas.rs`) after changing the font or the glyph set.
//! A `const` table means the shipping binary parses no font and reads no metrics
//! file at startup.
//!
//! Units are **em, y up, baseline at 0**; UVs are atlas-relative, y down.
//! `uv`, `size` and `bearing` describe the **padded SDF cell** (the tight glyph
//! box inflated by `SDF_SPREAD_EM` on all four sides), never the tight box —
//! see `doc/design_atom_labels.md` §The font atlas asset.

/// Per-glyph metrics for one padded SDF cell.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    /// Atlas UV of the cell's top-left corner (y down).
    pub uv_min: [f32; 2],
    /// Atlas UV of the cell's bottom-right corner (y down).
    pub uv_max: [f32; 2],
    /// Padded cell size in em. `[0.0, 0.0]` for a blank glyph (e.g. space).
    pub size: [f32; 2],
    /// Offset from the pen to the cell's bottom-left corner, em, y up.
    pub bearing: [f32; 2],
    /// Pen advance, em. Typographic — unaffected by the cell padding.
    pub advance: f32,
}

/// Atlas width in texels.
pub const ATLAS_WIDTH: u32 = 431;
/// Atlas height in texels.
pub const ATLAS_HEIGHT: u32 = 421;

/// The distance range encoded around each glyph edge, in em: an SDF value of
/// 1.0 means `SDF_SPREAD_EM` inside the glyph, 0.0 the same distance outside,
/// and 0.5 is the edge. The fragment shader's outline band must stay well
/// inside this range or it clips flat.
pub const SDF_SPREAD_EM: f32 = 0.125;

/// Cap height in em — the band the layout pass centers on the atom, so that
/// labels of different glyphs share a vertical center.
pub const CAP_HEIGHT_EM: f32 = 0.73046875;

/// First codepoint in the table.
pub const FIRST_CHAR: char = ' ';
/// Last codepoint in the table (inclusive).
pub const LAST_CHAR: char = '~';

/// Metrics for every printable-ASCII glyph, indexed by
/// `codepoint - FIRST_CHAR as u32`.
pub const GLYPH_METRICS: [GlyphMetrics; 95] = [
    // ' '
    GlyphMetrics {
        uv_min: [0.0023201855, 0.002375297],
        uv_max: [0.0023201855, 0.002375297],
        size: [0.0, 0.0],
        bearing: [0.0, 0.0],
        advance: 0.34814453,
    },
    // '!'
    GlyphMetrics {
        uv_min: [0.10208817, 0.002375297],
        uv_max: [0.13457076, 0.0783848],
        size: [0.4375, 1.0],
        bearing: [0.01171875, -0.125],
        advance: 0.4560547,
    },
    // '"'
    GlyphMetrics {
        uv_min: [0.20185615, 0.002375297],
        uv_max: [0.24593967, 0.042755343],
        size: [0.59375, 0.53125],
        bearing: [-0.03125, 0.33203125],
        advance: 0.5209961,
    },
    // '#'
    GlyphMetrics {
        uv_min: [0.30162412, 0.002375297],
        uv_max: [0.37354988, 0.076009504],
        size: [0.96875, 0.96875],
        bearing: [-0.05859375, -0.125],
        advance: 0.8378906,
    },
    // '$'
    GlyphMetrics {
        uv_min: [0.4013921, 0.002375297],
        uv_max: [0.46171695, 0.09263658],
        size: [0.8125, 1.1875],
        bearing: [-0.046875, -0.2734375],
        advance: 0.6958008,
    },
    // '%'
    GlyphMetrics {
        uv_min: [0.5011601, 0.002375297],
        uv_max: [0.5916473, 0.08076009],
        size: [1.21875, 1.03125],
        bearing: [-0.09375, -0.140625],
        advance: 1.0019531,
    },
    // '&'
    GlyphMetrics {
        uv_min: [0.60092807, 0.002375297],
        uv_max: [0.6774942, 0.08076009],
        size: [1.03125, 1.03125],
        bearing: [-0.06640625, -0.140625],
        advance: 0.8720703,
    },
    // '\''
    GlyphMetrics {
        uv_min: [0.70069605, 0.002375297],
        uv_max: [0.7285383, 0.042755343],
        size: [0.375, 0.53125],
        bearing: [-0.03125, 0.33203125],
        advance: 0.30615234,
    },
    // '('
    GlyphMetrics {
        uv_min: [0.80046403, 0.002375297],
        uv_max: [0.8422274, 0.09026128],
        size: [0.5625, 1.15625],
        bearing: [-0.0390625, -0.2578125],
        advance: 0.45703125,
    },
    // ')'
    GlyphMetrics {
        uv_min: [0.900232, 0.002375297],
        uv_max: [0.9419954, 0.09026128],
        size: [0.5625, 1.15625],
        bearing: [-0.046875, -0.2578125],
        advance: 0.45703125,
    },
    // '*'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.10213777],
        uv_max: [0.05800464, 0.1567696],
        size: [0.75, 0.71875],
        bearing: [-0.10546875, 0.15234375],
        advance: 0.5229492,
    },
    // '+'
    GlyphMetrics {
        uv_min: [0.10208817, 0.10213777],
        uv_max: [0.16937356, 0.17102137],
        size: [0.90625, 0.90625],
        bearing: [-0.01953125, -0.125],
        advance: 0.8378906,
    },
    // ','
    GlyphMetrics {
        uv_min: [0.20185615, 0.10213777],
        uv_max: [0.23897912, 0.14726841],
        size: [0.5, 0.59375],
        bearing: [-0.07421875, -0.26953125],
        advance: 0.3798828,
    },
    // '-'
    GlyphMetrics {
        uv_min: [0.30162412, 0.10213777],
        uv_max: [0.34338748, 0.13301663],
        size: [0.5625, 0.40625],
        bearing: [-0.07421875, 0.08984375],
        advance: 0.41503906,
    },
    // '.'
    GlyphMetrics {
        uv_min: [0.4013921, 0.10213777],
        uv_max: [0.4338747, 0.13776723],
        size: [0.4375, 0.46875],
        bearing: [-0.0234375, -0.125],
        advance: 0.3798828,
    },
    // '/'
    GlyphMetrics {
        uv_min: [0.5011601, 0.10213777],
        uv_max: [0.5475638, 0.18527316],
        size: [0.625, 1.09375],
        bearing: [-0.125, -0.21875],
        advance: 0.36523438,
    },
    // '0'
    GlyphMetrics {
        uv_min: [0.60092807, 0.10213777],
        uv_max: [0.66589326, 0.18052256],
        size: [0.875, 1.03125],
        bearing: [-0.078125, -0.140625],
        advance: 0.6958008,
    },
    // '1'
    GlyphMetrics {
        uv_min: [0.70069605, 0.10213777],
        uv_max: [0.75870067, 0.17814727],
        size: [0.78125, 1.0],
        bearing: [-0.015625, -0.125],
        advance: 0.6958008,
    },
    // '2'
    GlyphMetrics {
        uv_min: [0.80046403, 0.10213777],
        uv_max: [0.85846865, 0.17814727],
        size: [0.78125, 1.0],
        bearing: [-0.046875, -0.125],
        advance: 0.6958008,
    },
    // '3'
    GlyphMetrics {
        uv_min: [0.900232, 0.10213777],
        uv_max: [0.96055686, 0.18052256],
        size: [0.8125, 1.03125],
        bearing: [-0.05859375, -0.140625],
        advance: 0.6958008,
    },
    // '4'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.20190024],
        uv_max: [0.06728538, 0.27790973],
        size: [0.875, 1.0],
        bearing: [-0.08203125, -0.125],
        advance: 0.6958008,
    },
    // '5'
    GlyphMetrics {
        uv_min: [0.10208817, 0.20190024],
        uv_max: [0.16241299, 0.27790973],
        size: [0.8125, 1.0],
        bearing: [-0.05078125, -0.140625],
        advance: 0.6958008,
    },
    // '6'
    GlyphMetrics {
        uv_min: [0.20185615, 0.20190024],
        uv_max: [0.26450115, 0.28028503],
        size: [0.84375, 1.03125],
        bearing: [-0.06640625, -0.140625],
        advance: 0.6958008,
    },
    // '7'
    GlyphMetrics {
        uv_min: [0.30162412, 0.20190024],
        uv_max: [0.36194897, 0.27790973],
        size: [0.8125, 1.0],
        bearing: [-0.05859375, -0.125],
        advance: 0.6958008,
    },
    // '8'
    GlyphMetrics {
        uv_min: [0.4013921, 0.20190024],
        uv_max: [0.46403712, 0.28028503],
        size: [0.84375, 1.03125],
        bearing: [-0.06640625, -0.140625],
        advance: 0.6958008,
    },
    // '9'
    GlyphMetrics {
        uv_min: [0.5011601, 0.20190024],
        uv_max: [0.5638051, 0.28028503],
        size: [0.84375, 1.03125],
        bearing: [-0.07421875, -0.140625],
        advance: 0.6958008,
    },
    // ':'
    GlyphMetrics {
        uv_min: [0.60092807, 0.20190024],
        uv_max: [0.6334107, 0.26365796],
        size: [0.4375, 0.8125],
        bearing: [-0.015625, -0.125],
        advance: 0.39990234,
    },
    // ';'
    GlyphMetrics {
        uv_min: [0.70069605, 0.20190024],
        uv_max: [0.737819, 0.27553445],
        size: [0.5, 0.96875],
        bearing: [-0.0625, -0.26953125],
        advance: 0.39990234,
    },
    // '<'
    GlyphMetrics {
        uv_min: [0.80046403, 0.20190024],
        uv_max: [0.8677494, 0.26603326],
        size: [0.90625, 0.84375],
        bearing: [-0.01953125, -0.09765625],
        advance: 0.8378906,
    },
    // '='
    GlyphMetrics {
        uv_min: [0.900232, 0.20190024],
        uv_max: [0.9675174, 0.24703088],
        size: [0.90625, 0.59375],
        bearing: [-0.01953125, 0.015625],
        advance: 0.8378906,
    },
    // '>'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.3016627],
        uv_max: [0.06960557, 0.36579573],
        size: [0.90625, 0.84375],
        bearing: [-0.01953125, -0.09765625],
        advance: 0.8378906,
    },
    // '?'
    GlyphMetrics {
        uv_min: [0.10208817, 0.3016627],
        uv_max: [0.15545243, 0.3776722],
        size: [0.71875, 1.0],
        bearing: [-0.05859375, -0.125],
        advance: 0.5800781,
    },
    // '@'
    GlyphMetrics {
        uv_min: [0.20185615, 0.3016627],
        uv_max: [0.28538284, 0.3895487],
        size: [1.125, 1.15625],
        bearing: [-0.0625, -0.30078125],
        advance: 1.0,
    },
    // 'A'
    GlyphMetrics {
        uv_min: [0.30162412, 0.3016627],
        uv_max: [0.37819025, 0.3776722],
        size: [1.03125, 1.0],
        bearing: [-0.12109375, -0.125],
        advance: 0.7739258,
    },
    // 'B'
    GlyphMetrics {
        uv_min: [0.4013921, 0.3016627],
        uv_max: [0.46635732, 0.3776722],
        size: [0.875, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.76220703,
    },
    // 'C'
    GlyphMetrics {
        uv_min: [0.5011601, 0.3016627],
        uv_max: [0.5661253, 0.3800475],
        size: [0.875, 1.03125],
        bearing: [-0.078125, -0.140625],
        advance: 0.7338867,
    },
    // 'D'
    GlyphMetrics {
        uv_min: [0.60092807, 0.3016627],
        uv_max: [0.6728538, 0.3776722],
        size: [0.96875, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.8300781,
    },
    // 'E'
    GlyphMetrics {
        uv_min: [0.70069605, 0.3016627],
        uv_max: [0.75870067, 0.3776722],
        size: [0.78125, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.68310547,
    },
    // 'F'
    GlyphMetrics {
        uv_min: [0.80046403, 0.3016627],
        uv_max: [0.85846865, 0.3776722],
        size: [0.78125, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.68310547,
    },
    // 'G'
    GlyphMetrics {
        uv_min: [0.900232, 0.3016627],
        uv_max: [0.9721578, 0.3800475],
        size: [0.96875, 1.03125],
        bearing: [-0.078125, -0.140625],
        advance: 0.8208008,
    },
    // 'H'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.40142518],
        uv_max: [0.06960557, 0.47743466],
        size: [0.90625, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.83691406,
    },
    // 'I'
    GlyphMetrics {
        uv_min: [0.10208817, 0.40142518],
        uv_max: [0.13689095, 0.47743466],
        size: [0.46875, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.3720703,
    },
    // 'J'
    GlyphMetrics {
        uv_min: [0.20185615, 0.40142518],
        uv_max: [0.24593967, 0.49168646],
        size: [0.59375, 1.1875],
        bearing: [-0.18359375, -0.328125],
        advance: 0.3720703,
    },
    // 'K'
    GlyphMetrics {
        uv_min: [0.30162412, 0.40142518],
        uv_max: [0.37354988, 0.47743466],
        size: [0.96875, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.77490234,
    },
    // 'L'
    GlyphMetrics {
        uv_min: [0.4013921, 0.40142518],
        uv_max: [0.45939675, 0.47743466],
        size: [0.78125, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.63720703,
    },
    // 'M'
    GlyphMetrics {
        uv_min: [0.5011601, 0.40142518],
        uv_max: [0.5823666, 0.47743466],
        size: [1.09375, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.9951172,
    },
    // 'N'
    GlyphMetrics {
        uv_min: [0.60092807, 0.40142518],
        uv_max: [0.6682135, 0.47743466],
        size: [0.90625, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.83691406,
    },
    // 'O'
    GlyphMetrics {
        uv_min: [0.70069605, 0.40142518],
        uv_max: [0.77726215, 0.47980997],
        size: [1.03125, 1.03125],
        bearing: [-0.078125, -0.140625],
        advance: 0.85009766,
    },
    // 'P'
    GlyphMetrics {
        uv_min: [0.80046403, 0.40142518],
        uv_max: [0.8654292, 0.47743466],
        size: [0.875, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.73291016,
    },
    // 'Q'
    GlyphMetrics {
        uv_min: [0.900232, 0.40142518],
        uv_max: [0.9767981, 0.48931116],
        size: [1.03125, 1.15625],
        bearing: [-0.078125, -0.2734375],
        advance: 0.85009766,
    },
    // 'R'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.5011876],
        uv_max: [0.07192575, 0.57719713],
        size: [0.9375, 1.0],
        bearing: [-0.03515625, -0.125],
        advance: 0.77001953,
    },
    // 'S'
    GlyphMetrics {
        uv_min: [0.10208817, 0.5011876],
        uv_max: [0.16473317, 0.57957244],
        size: [0.84375, 1.03125],
        bearing: [-0.0546875, -0.140625],
        advance: 0.72021484,
    },
    // 'T'
    GlyphMetrics {
        uv_min: [0.20185615, 0.5011876],
        uv_max: [0.27146173, 0.57719713],
        size: [0.9375, 1.0],
        bearing: [-0.12109375, -0.125],
        advance: 0.6821289,
    },
    // 'U'
    GlyphMetrics {
        uv_min: [0.30162412, 0.5011876],
        uv_max: [0.3689095, 0.57719713],
        size: [0.90625, 1.0],
        bearing: [-0.03515625, -0.140625],
        advance: 0.8120117,
    },
    // 'V'
    GlyphMetrics {
        uv_min: [0.4013921, 0.5011876],
        uv_max: [0.47795823, 0.57719713],
        size: [1.03125, 1.0],
        bearing: [-0.12109375, -0.125],
        advance: 0.7739258,
    },
    // 'W'
    GlyphMetrics {
        uv_min: [0.5011601, 0.5011876],
        uv_max: [0.5986079, 0.57719713],
        size: [1.3125, 1.0],
        bearing: [-0.09765625, -0.125],
        advance: 1.1030273,
    },
    // 'X'
    GlyphMetrics {
        uv_min: [0.60092807, 0.5011876],
        uv_max: [0.675174, 0.57719713],
        size: [1.0, 1.0],
        bearing: [-0.109375, -0.125],
        advance: 0.7709961,
    },
    // 'Y'
    GlyphMetrics {
        uv_min: [0.70069605, 0.5011876],
        uv_max: [0.774942, 0.57719713],
        size: [1.0, 1.0],
        bearing: [-0.13671875, -0.125],
        advance: 0.7241211,
    },
    // 'Z'
    GlyphMetrics {
        uv_min: [0.80046403, 0.5011876],
        uv_max: [0.8677494, 0.57719713],
        size: [0.90625, 1.0],
        bearing: [-0.08203125, -0.125],
        advance: 0.72509766,
    },
    // '['
    GlyphMetrics {
        uv_min: [0.900232, 0.5011876],
        uv_max: [0.9419954, 0.58907366],
        size: [0.5625, 1.15625],
        bearing: [-0.0390625, -0.2578125],
        advance: 0.45703125,
    },
    // '\\'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.6009501],
        uv_max: [0.0487239, 0.6840855],
        size: [0.625, 1.09375],
        bearing: [-0.125, -0.21875],
        advance: 0.36523438,
    },
    // ']'
    GlyphMetrics {
        uv_min: [0.10208817, 0.6009501],
        uv_max: [0.1438515, 0.6888361],
        size: [0.5625, 1.15625],
        bearing: [-0.05859375, -0.2578125],
        advance: 0.45703125,
    },
    // '^'
    GlyphMetrics {
        uv_min: [0.20185615, 0.6009501],
        uv_max: [0.26914153, 0.6413302],
        size: [0.90625, 0.53125],
        bearing: [-0.02734375, 0.33203125],
        advance: 0.8378906,
    },
    // '_'
    GlyphMetrics {
        uv_min: [0.30162412, 0.6009501],
        uv_max: [0.3573086, 0.62945366],
        size: [0.75, 0.375],
        bearing: [-0.125, -0.36328125],
        advance: 0.5,
    },
    // '`'
    GlyphMetrics {
        uv_min: [0.4013921, 0.6009501],
        uv_max: [0.44083527, 0.63420427],
        size: [0.53125, 0.4375],
        bearing: [-0.08203125, 0.48828125],
        advance: 0.5,
    },
    // 'a'
    GlyphMetrics {
        uv_min: [0.5011601, 0.6009501],
        uv_max: [0.56148493, 0.6650831],
        size: [0.8125, 0.84375],
        bearing: [-0.08203125, -0.140625],
        advance: 0.6748047,
    },
    // 'b'
    GlyphMetrics {
        uv_min: [0.60092807, 0.6009501],
        uv_max: [0.6635731, 0.67933494],
        size: [0.84375, 1.03125],
        bearing: [-0.04296875, -0.140625],
        advance: 0.7158203,
    },
    // 'c'
    GlyphMetrics {
        uv_min: [0.70069605, 0.6009501],
        uv_max: [0.7563805, 0.6650831],
        size: [0.75, 0.84375],
        bearing: [-0.08203125, -0.140625],
        advance: 0.59277344,
    },
    // 'd'
    GlyphMetrics {
        uv_min: [0.80046403, 0.6009501],
        uv_max: [0.86310905, 0.67933494],
        size: [0.84375, 1.03125],
        bearing: [-0.08203125, -0.140625],
        advance: 0.7158203,
    },
    // 'e'
    GlyphMetrics {
        uv_min: [0.900232, 0.6009501],
        uv_max: [0.96287704, 0.6650831],
        size: [0.84375, 0.84375],
        bearing: [-0.08203125, -0.140625],
        advance: 0.67822266,
    },
    // 'f'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.70071256],
        uv_max: [0.05336427, 0.7790974],
        size: [0.6875, 1.03125],
        bearing: [-0.109375, -0.125],
        advance: 0.4350586,
    },
    // 'g'
    GlyphMetrics {
        uv_min: [0.10208817, 0.70071256],
        uv_max: [0.16473317, 0.7790974],
        size: [0.84375, 1.03125],
        bearing: [-0.08203125, -0.34375],
        advance: 0.7158203,
    },
    // 'h'
    GlyphMetrics {
        uv_min: [0.20185615, 0.70071256],
        uv_max: [0.26218098, 0.7790974],
        size: [0.8125, 1.03125],
        bearing: [-0.04296875, -0.125],
        advance: 0.71191406,
    },
    // 'i'
    GlyphMetrics {
        uv_min: [0.30162412, 0.70071256],
        uv_max: [0.33410674, 0.7790974],
        size: [0.4375, 1.03125],
        bearing: [-0.04296875, -0.125],
        advance: 0.34277344,
    },
    // 'j'
    GlyphMetrics {
        uv_min: [0.4013921, 0.70071256],
        uv_max: [0.44315544, 0.79572445],
        size: [0.5625, 1.25],
        bearing: [-0.16015625, -0.34375],
        advance: 0.34277344,
    },
    // 'k'
    GlyphMetrics {
        uv_min: [0.5011601, 0.70071256],
        uv_max: [0.5661253, 0.7790974],
        size: [0.875, 1.03125],
        bearing: [-0.04296875, -0.125],
        advance: 0.66503906,
    },
    // 'l'
    GlyphMetrics {
        uv_min: [0.60092807, 0.70071256],
        uv_max: [0.6334107, 0.7790974],
        size: [0.4375, 1.03125],
        bearing: [-0.04296875, -0.125],
        advance: 0.34277344,
    },
    // 'm'
    GlyphMetrics {
        uv_min: [0.70069605, 0.70071256],
        uv_max: [0.78654295, 0.7624703],
        size: [1.15625, 0.8125],
        bearing: [-0.04296875, -0.125],
        advance: 1.0419922,
    },
    // 'n'
    GlyphMetrics {
        uv_min: [0.80046403, 0.70071256],
        uv_max: [0.8607889, 0.7624703],
        size: [0.8125, 0.8125],
        bearing: [-0.04296875, -0.125],
        advance: 0.71191406,
    },
    // 'o'
    GlyphMetrics {
        uv_min: [0.900232, 0.70071256],
        uv_max: [0.9651972, 0.7648456],
        size: [0.875, 0.84375],
        bearing: [-0.08203125, -0.140625],
        advance: 0.6870117,
    },
    // 'p'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.80047506],
        uv_max: [0.064965196, 0.8788599],
        size: [0.84375, 1.03125],
        bearing: [-0.04296875, -0.3359375],
        advance: 0.7158203,
    },
    // 'q'
    GlyphMetrics {
        uv_min: [0.10208817, 0.80047506],
        uv_max: [0.16473317, 0.8788599],
        size: [0.84375, 1.03125],
        bearing: [-0.08203125, -0.3359375],
        advance: 0.7158203,
    },
    // 'r'
    GlyphMetrics {
        uv_min: [0.20185615, 0.80047506],
        uv_max: [0.25290024, 0.8622328],
        size: [0.6875, 0.8125],
        bearing: [-0.04296875, -0.125],
        advance: 0.49316406,
    },
    // 's'
    GlyphMetrics {
        uv_min: [0.30162412, 0.80047506],
        uv_max: [0.3573086, 0.86460805],
        size: [0.75, 0.84375],
        bearing: [-0.07421875, -0.140625],
        advance: 0.59521484,
    },
    // 't'
    GlyphMetrics {
        uv_min: [0.4013921, 0.80047506],
        uv_max: [0.45475638, 0.87410927],
        size: [0.71875, 0.96875],
        bearing: [-0.11328125, -0.125],
        advance: 0.47802734,
    },
    // 'u'
    GlyphMetrics {
        uv_min: [0.5011601, 0.80047506],
        uv_max: [0.56148493, 0.8622328],
        size: [0.8125, 0.8125],
        bearing: [-0.046875, -0.140625],
        advance: 0.71191406,
    },
    // 'v'
    GlyphMetrics {
        uv_min: [0.60092807, 0.80047506],
        uv_max: [0.6682135, 0.8622328],
        size: [0.90625, 0.8125],
        bearing: [-0.11328125, -0.125],
        advance: 0.65185547,
    },
    // 'w'
    GlyphMetrics {
        uv_min: [0.70069605, 0.80047506],
        uv_max: [0.7842227, 0.8622328],
        size: [1.125, 0.8125],
        bearing: [-0.08984375, -0.125],
        advance: 0.9238281,
    },
    // 'x'
    GlyphMetrics {
        uv_min: [0.80046403, 0.80047506],
        uv_max: [0.8654292, 0.8622328],
        size: [0.875, 0.8125],
        bearing: [-0.11328125, -0.125],
        advance: 0.64501953,
    },
    // 'y'
    GlyphMetrics {
        uv_min: [0.900232, 0.80047506],
        uv_max: [0.9651972, 0.8788599],
        size: [0.875, 1.03125],
        bearing: [-0.11328125, -0.34375],
        advance: 0.65185547,
    },
    // 'z'
    GlyphMetrics {
        uv_min: [0.0023201855, 0.9002375],
        uv_max: [0.05800464, 0.96199524],
        size: [0.75, 0.8125],
        bearing: [-0.08203125, -0.125],
        advance: 0.58203125,
    },
    // '{'
    GlyphMetrics {
        uv_min: [0.10208817, 0.9002375],
        uv_max: [0.15545243, 0.99049884],
        size: [0.71875, 1.1875],
        bearing: [0.0, -0.2890625],
        advance: 0.71191406,
    },
    // '|'
    GlyphMetrics {
        uv_min: [0.20185615, 0.9002375],
        uv_max: [0.22969837, 0.9976247],
        size: [0.375, 1.28125],
        bearing: [0.0, -0.36328125],
        advance: 0.36523438,
    },
    // '}'
    GlyphMetrics {
        uv_min: [0.30162412, 0.9002375],
        uv_max: [0.3549884, 0.99049884],
        size: [0.71875, 1.1875],
        bearing: [0.0, -0.2890625],
        advance: 0.71191406,
    },
    // '~'
    GlyphMetrics {
        uv_min: [0.4013921, 0.9002375],
        uv_max: [0.4686775, 0.935867],
        size: [0.90625, 0.46875],
        bearing: [-0.01953125, 0.0859375],
        advance: 0.8378906,
    },
];
