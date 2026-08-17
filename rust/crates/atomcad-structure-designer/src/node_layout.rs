//! Node layout utilities for estimating node sizes and positioning.
//!
//! This module provides functions for estimating node dimensions based on their
//! input pin count and other properties. These estimates match the Flutter frontend's
//! `getNodeSize()` function in `lib/structure_designer/node_network/node_network.dart`.
//!
//! Used by:
//! - `duplicate_node()` for positioning duplicated nodes below originals
//! - Phase 4B auto-layout for positioning AI-created nodes

use glam::DVec2;

// =============================================================================
// Constants matching Flutter's BASE_NODE_* values
// See: lib/structure_designer/node_network/node_network.dart
// =============================================================================

/// Base width of all nodes (matches Flutter's BASE_NODE_WIDTH)
pub const NODE_WIDTH: f64 = 160.0;

/// Height of the title bar area
const TITLE_HEIGHT: f64 = 30.0;

/// Height of a single output pin
const OUTPUT_HEIGHT: f64 = 25.0;

/// Height per input parameter pin (matches Flutter's BASE_NODE_VERT_WIRE_OFFSET_PER_PARAM)
pub const PER_PARAM_HEIGHT: f64 = 22.0;

/// Height of the subtitle area when present
const SUBTITLE_HEIGHT: f64 = 20.0;

/// Vertical padding at the bottom of the node
const PADDING: f64 = 8.0;

/// Default vertical gap between nodes for comfortable spacing
pub const DEFAULT_VERTICAL_GAP: f64 = 20.0;

/// Default horizontal gap between nodes
pub const DEFAULT_HORIZONTAL_GAP: f64 = 50.0;

// =============================================================================
// Size estimation functions
// =============================================================================

/// Estimates the height of a node based on its number of input and output pins.
///
/// This matches the formula used in Flutter's `getNodeSize()` function:
/// ```text
/// height = title_height + max(input_pins_height, output_pins_height) + subtitle_height + padding
/// ```
///
/// # Arguments
/// * `num_input_pins` - Number of input parameter pins on the node
/// * `num_output_pins` - Number of output pins on the node (typically 1)
/// * `has_subtitle` - Whether the node displays a subtitle (e.g., node type name)
///
/// # Returns
/// Estimated height in logical units (unscaled, normal zoom level)
pub fn estimate_node_height(
    num_input_pins: usize,
    num_output_pins: usize,
    has_subtitle: bool,
) -> f64 {
    let input_height = num_input_pins as f64 * PER_PARAM_HEIGHT;
    let output_height = num_output_pins as f64 * PER_PARAM_HEIGHT;
    let main_body_height = input_height.max(output_height).max(OUTPUT_HEIGHT);
    let subtitle = if has_subtitle { SUBTITLE_HEIGHT } else { 0.0 };
    TITLE_HEIGHT + main_body_height + subtitle + PADDING
}

/// Estimates the full size (width, height) of a node.
///
/// # Arguments
/// * `num_input_pins` - Number of input parameter pins on the node
/// * `num_output_pins` - Number of output pins on the node (typically 1)
/// * `has_subtitle` - Whether the node displays a subtitle
///
/// # Returns
/// DVec2 with (width, height) in logical units
pub fn estimate_node_size(
    num_input_pins: usize,
    num_output_pins: usize,
    has_subtitle: bool,
) -> DVec2 {
    DVec2::new(
        NODE_WIDTH,
        estimate_node_height(num_input_pins, num_output_pins, has_subtitle),
    )
}

/// Returns the fixed node width.
///
/// All nodes have the same width regardless of content.
#[inline]
pub fn get_node_width() -> f64 {
    NODE_WIDTH
}

// =============================================================================
// HOF (zone-owning) node sizing
//
// Constants match Flutter's `BASE_HOF_BODY_*` / `CLOSURE_BODY_*` in
// `lib/structure_designer/node_network/node_network.dart`.
// =============================================================================

/// External input-column width to the left of a four-HOF body region.
const HOF_BODY_LEFT_OFFSET: f64 = 70.0;
/// External output-column width to the right of a four-HOF body region.
const HOF_BODY_RIGHT_GUTTER: f64 = 70.0;
/// Trimmed left/right pads for the `closure` node (no external input column;
/// its `Function` output renders in the title bar).
const CLOSURE_BODY_LEFT_PAD: f64 = 16.0;
const CLOSURE_BODY_RIGHT_PAD: f64 = 16.0;
/// Padding added to a body region's content bounding box (right and bottom)
/// before it is compared against the stored body size. Matches Flutter's
/// `BASE_HOF_BODY_BOTTOM_PADDING` in `node_network.dart`, used by
/// `_computeBodySize` (`scope_resolver.dart`).
pub const HOF_BODY_BOTTOM_PADDING: f64 = 8.0;

/// Estimates the full (width, height) of a higher-order-function / zone-owning
/// node whose body region is **expanded** (not collapsed).
///
/// A regular node's footprint is just title + pins + subtitle + padding, but an
/// expanded HOF (`map` / `filter` / `fold` / `foreach` / `closure`) is dominated
/// by its body region (`body_width` × `body_height`), flanked by the external
/// pin columns. This mirrors Flutter's `effectiveNodeSizeLogical`
/// (`scope_resolver.dart`) using the node's *stored* body dimensions.
///
/// `is_closure` trims the left/right chrome (the `closure` node has no external
/// input column and renders its function output in the title bar).
///
/// Callers that include HOFs in a bounding-box computation (e.g.
/// `node_inlining::content_bounding_box`) must use this for expanded HOFs;
/// [`estimate_node_size`] alone undersizes them by the whole body and causes
/// downstream nodes to overlap.
#[allow(clippy::too_many_arguments)]
pub fn estimate_hof_node_size(
    num_input_pins: usize,
    num_output_pins: usize,
    num_zone_input_pins: usize,
    num_zone_output_pins: usize,
    body_width: f64,
    body_height: f64,
    has_subtitle: bool,
    is_closure: bool,
) -> DVec2 {
    let (left, right) = if is_closure {
        (CLOSURE_BODY_LEFT_PAD, CLOSURE_BODY_RIGHT_PAD)
    } else {
        (HOF_BODY_LEFT_OFFSET, HOF_BODY_RIGHT_GUTTER)
    };
    let width = left + body_width + right;

    // The vertical mid-band is the tallest of: the external pins, the inner
    // zone pins, and the body itself (never smaller than one output row).
    let main_body_height = [
        num_input_pins as f64 * PER_PARAM_HEIGHT,
        num_output_pins as f64 * PER_PARAM_HEIGHT,
        num_zone_input_pins as f64 * PER_PARAM_HEIGHT,
        num_zone_output_pins as f64 * PER_PARAM_HEIGHT,
        body_height,
        OUTPUT_HEIGHT,
    ]
    .into_iter()
    .fold(0.0_f64, f64::max);

    let subtitle = if has_subtitle { SUBTITLE_HEIGHT } else { 0.0 };
    DVec2::new(width, TITLE_HEIGHT + main_body_height + subtitle + PADDING)
}

/// Calculates the vertical offset for placing a duplicate node below the original.
///
/// This accounts for the original node's height plus a gap for visual separation.
///
/// # Arguments
/// * `num_input_pins` - Number of input pins on the original node
/// * `num_output_pins` - Number of output pins on the original node
/// * `has_subtitle` - Whether the original node has a subtitle
///
/// # Returns
/// Vertical offset in logical units to add to the original node's Y position
pub fn duplicate_node_vertical_offset(
    num_input_pins: usize,
    num_output_pins: usize,
    has_subtitle: bool,
) -> f64 {
    estimate_node_height(num_input_pins, num_output_pins, has_subtitle) + DEFAULT_VERTICAL_GAP
}

// =============================================================================
// Bounding box utilities for overlap detection
// =============================================================================

/// Checks if two axis-aligned bounding boxes overlap.
///
/// # Arguments
/// * `pos1` - Position (top-left corner) of first node
/// * `size1` - Size (width, height) of first node
/// * `pos2` - Position (top-left corner) of second node
/// * `size2` - Size (width, height) of second node
/// * `gap` - Minimum gap to maintain between nodes (added to bounding boxes)
///
/// # Returns
/// `true` if the bounding boxes (expanded by gap) overlap
pub fn nodes_overlap(pos1: DVec2, size1: DVec2, pos2: DVec2, size2: DVec2, gap: f64) -> bool {
    let half_gap = gap / 2.0;

    // Expand both boxes by half the gap
    let left1 = pos1.x - half_gap;
    let right1 = pos1.x + size1.x + half_gap;
    let top1 = pos1.y - half_gap;
    let bottom1 = pos1.y + size1.y + half_gap;

    let left2 = pos2.x - half_gap;
    let right2 = pos2.x + size2.x + half_gap;
    let top2 = pos2.y - half_gap;
    let bottom2 = pos2.y + size2.y + half_gap;

    // Check for overlap (boxes overlap if they don't NOT overlap in any dimension)
    left1 < right2 && right1 > left2 && top1 < bottom2 && bottom1 > top2
}

/// Checks if a node at a given position would overlap with any existing node.
///
/// # Arguments
/// * `proposed_pos` - Proposed position for the new node
/// * `proposed_size` - Size of the new node
/// * `existing_nodes` - Iterator of (position, size) tuples for existing nodes
/// * `gap` - Minimum gap to maintain between nodes
///
/// # Returns
/// `true` if the proposed position would cause an overlap
pub fn overlaps_any<I>(
    proposed_pos: DVec2,
    proposed_size: DVec2,
    existing_nodes: I,
    gap: f64,
) -> bool
where
    I: IntoIterator<Item = (DVec2, DVec2)>,
{
    for (pos, size) in existing_nodes {
        if nodes_overlap(proposed_pos, proposed_size, pos, size, gap) {
            return true;
        }
    }
    false
}
