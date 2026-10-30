//! The layout read-out: what `atomcad-cli query --layout` prints
//! (`doc/design_deliberate_node_placement.md` A1, "Ask").
//!
//! The AI cannot see the drawing. This gives it the drawing as plain boxes —
//! one line per node, `name  x,y  w×h  [tags]`, in the coordinates
//! `Node.position` is stored in — plus the one inference the read-out makes:
//! **groups**, the maximal columns and rows a person drew. Groups are
//! recomputed on every call and never stored, so a guess can never become
//! indistinguishable from intent.
//!
//! Sizes come from [`rendered_node_size`], the one size function; the group
//! tolerance is [`COLUMN_TOLERANCE`], the constant the window rule uses to
//! recognise a loose column, and the group gap limits are [`Gap::Wide`]. So the
//! read-out and the motion primitives agree on what a column is.

use std::collections::HashSet;

use glam::DVec2;

use super::common::{Gap, is_comment};
use super::motion::{COLUMN_TOLERANCE, Rect};
use super::report::{FINDING_KINDS, Finding, FindingCounts, absolute_findings};
use super::size::rendered_node_size;
use crate::node_network::{NodeNetwork, resolve_body_collapsed};
use crate::node_type_registry::NodeTypeRegistry;
use crate::text_format::{format_identifier, unique_node_names};

/// Which nodes of the scope the read-out lists. Groups and the extent are
/// always computed over the whole scope.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadoutFilter {
    /// Every node in the scope.
    All,
    /// Exactly these names, as `query` spells them (backticks optional). An
    /// unknown name is listed as `name  (no such node)`.
    Nodes(Vec<String>),
    /// Every node whose box intersects this rectangle (zero clearance: a box
    /// that only touches its edge does not intersect it).
    Box(Rect),
}

/// One node of the scope, as the read-out sees it.
struct Item {
    name: String,
    rect: Rect,
    tags: Vec<&'static str>,
}

/// Which way a group runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    Column,
    Row,
}

/// A maximal column or row: indices into the scope's items, in reading order.
struct Group {
    kind: GroupKind,
    members: Vec<usize>,
    gap: f64,
}

/// Parse the read-out's three arguments as the transports deliver them: a
/// `/`-separated scope path, a `,`-separated name list and a
/// `x0,y0,x1,y1` box, each empty when absent.
///
/// `Err` carries a one-line message for the caller to print.
pub fn parse_readout_args(
    scope: &str,
    nodes: &str,
    bbox: &str,
) -> Result<(Vec<String>, ReadoutFilter), String> {
    let scope: Vec<String> = scope
        .split('/')
        .map(|segment| unquote(segment.trim()).to_string())
        .filter(|segment| !segment.is_empty())
        .collect();
    let (nodes, bbox) = (nodes.trim(), bbox.trim());
    if !nodes.is_empty() && !bbox.is_empty() {
        return Err("--nodes and --box are mutually exclusive".to_string());
    }
    if !nodes.is_empty() {
        let names = nodes
            .split(',')
            .map(|name| unquote(name.trim()).to_string())
            .filter(|name| !name.is_empty())
            .collect();
        return Ok((scope, ReadoutFilter::Nodes(names)));
    }
    if !bbox.is_empty() {
        let numbers: Vec<f64> = bbox
            .split(',')
            .map(|n| n.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| format!("--box expects x0,y0,x1,y1, got '{}'", bbox))?;
        let [x0, y0, x1, y1] = numbers[..] else {
            return Err(format!("--box expects x0,y0,x1,y1, got '{}'", bbox));
        };
        // Corners in any order.
        let min = DVec2::new(x0.min(x1), y0.min(y1));
        let max = DVec2::new(x0.max(x1), y0.max(y1));
        return Ok((scope, ReadoutFilter::Box(Rect::new(min, max - min))));
    }
    Ok((scope, ReadoutFilter::All))
}

/// The read-out of one scope of `network` (named `network_name`).
///
/// `scope` is a path of HOF names from the network's root (`["holes"]` for
/// the body of `holes`); empty is the root itself. Body nodes are listed in
/// body coordinates, the way they are stored. `Err` when a segment names no
/// node or a node without a body.
pub fn layout_readout(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    network_name: &str,
    scope: &[String],
    filter: &ReadoutFilter,
) -> Result<String, String> {
    let (current, _) = resolve_scope(network, network_name, scope)?;

    let items = scope_items(current, registry);
    let groups = detect_groups(&items);

    // Header.
    let mut title = network_name.to_string();
    for segment in scope {
        title.push('/');
        title.push_str(&format_identifier(segment));
    }
    let mut out = String::new();
    let extent = items
        .iter()
        .map(|item| item.rect)
        .reduce(|a, b| bounding_box(&a, &b));
    let count = plural(items.len(), "node", "nodes");
    match extent {
        Some(extent) => out.push_str(&format!(
            "# {}: extent {}, {}",
            title,
            corners(&extent),
            count
        )),
        None => out.push_str(&format!("# {}: no nodes", title)),
    }

    // Selection.
    let mut unknown: Vec<String> = Vec::new();
    let selected: Vec<usize> = match filter {
        ReadoutFilter::All => (0..items.len()).collect(),
        ReadoutFilter::Nodes(names) => {
            let mut picked = Vec::new();
            for name in names {
                match items.iter().position(|item| &item.name == name) {
                    Some(index) => {
                        if !picked.contains(&index) {
                            picked.push(index);
                        }
                    }
                    None => {
                        if !unknown.contains(name) {
                            unknown.push(name.clone());
                        }
                    }
                }
            }
            out.push_str(&format!("; {} listed", picked.len()));
            picked
        }
        ReadoutFilter::Box(area) => {
            let picked: Vec<usize> = (0..items.len())
                .filter(|&index| items[index].rect.overlaps(area, 0.0))
                .collect();
            out.push_str(&format!("; {} in box {}", picked.len(), corners(area)));
            picked
        }
    };
    out.push('\n');

    // Node lines, top then left (`scope_items` already sorted them so).
    let mut lines: Vec<[String; 4]> = Vec::new();
    let mut in_order: Vec<usize> = selected.clone();
    in_order.sort_unstable();
    for &index in &in_order {
        let item = &items[index];
        lines.push([
            format_identifier(&item.name).into_owned(),
            format!("{},{}", px(item.rect.pos.x), px(item.rect.pos.y)),
            format!("{}×{}", px(item.rect.size.x), px(item.rect.size.y)),
            item.tags.join(" "),
        ]);
    }
    // An unknown name shares the name column but not the others, so its note
    // does not widen every position.
    let unknown: Vec<String> = unknown
        .iter()
        .map(|name| format_identifier(name).into_owned())
        .collect();
    let name_width = unknown.iter().map(|name| name.chars().count()).max();
    push_table(&mut out, &lines, name_width.unwrap_or(0));
    let name_width = lines
        .iter()
        .map(|row| row[0].chars().count())
        .chain(name_width)
        .max()
        .unwrap_or(0);
    for name in &unknown {
        let pad = name_width - name.chars().count() + 2;
        out.push_str(&format!("{}{}(no such node)\n", name, " ".repeat(pad)));
    }

    // Groups with at least one selected member.
    let selected: HashSet<usize> = selected.into_iter().collect();
    let listed: Vec<&Group> = groups
        .iter()
        .filter(|group| group.members.iter().any(|m| selected.contains(m)))
        .collect();
    if !listed.is_empty() {
        out.push_str("# groups (inferred)\n");
        let rows: Vec<[String; 4]> = listed
            .iter()
            .map(|group| {
                let mut members: Vec<String> = Vec::new();
                for &member in &group.members {
                    if selected.contains(&member) {
                        members.push(format_identifier(&items[member].name).into_owned());
                    } else if members.last().map(String::as_str) != Some("…") {
                        members.push("…".to_string());
                    }
                }
                [
                    match group.kind {
                        GroupKind::Column => "column".to_string(),
                        GroupKind::Row => "row".to_string(),
                    },
                    members.join(", "),
                    format!("gap {}", px(group.gap)),
                    String::new(),
                ]
            })
            .collect();
        push_table(&mut out, &rows, 0);
    }
    Ok(out)
}

/// The ids of the HOF chain a name path of scopes leads to inside `network`
/// (empty for the root), or `None` when a segment names no node or a node
/// without a body.
pub fn scope_ids_by_name(network: &NodeNetwork, scope: &[String]) -> Option<Vec<u64>> {
    resolve_scope(network, "", scope).ok().map(|(_, ids)| ids)
}

/// How many findings [`findings_readout`] lists before it stops and says how
/// many more there are. A tangled network has dozens; the counts line always
/// carries the whole number, and `--nodes` / `--box` narrow the list.
pub const MAX_LISTED_FINDINGS: usize = 20;

/// The `# findings` section of `query --layout --findings`: every layout
/// problem the scope has now ([`absolute_findings`]), not only what an edit
/// created. A counts line per kind, then one line per finding, at most
/// [`MAX_LISTED_FINDINGS`].
///
/// With a filter, only findings involving a selected node are counted and
/// listed, and the counts line also gives the scope's total.
pub fn findings_readout(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    network_name: &str,
    scope: &[String],
    filter: &ReadoutFilter,
) -> Result<String, String> {
    let (current, ids) = resolve_scope(network, network_name, scope)?;
    let all = absolute_findings(network, registry, &ids, scope);

    let selected: Option<HashSet<String>> = match filter {
        ReadoutFilter::All => None,
        ReadoutFilter::Nodes(names) => Some(names.iter().cloned().collect()),
        ReadoutFilter::Box(area) => Some(
            scope_items(current, registry)
                .into_iter()
                .filter(|item| item.rect.overlaps(area, 0.0))
                .map(|item| item.name)
                .collect(),
        ),
    };
    let shown: Vec<&Finding> = all
        .iter()
        .filter(|finding| {
            selected.as_ref().is_none_or(|names| {
                finding
                    .nodes
                    .iter()
                    .any(|path| path.last().is_some_and(|name| names.contains(name)))
            })
        })
        .collect();

    let counts = FindingCounts::of(shown.iter().copied());
    let mut out = String::from("# findings");
    if selected.is_some() {
        out.push_str(" involving the listed nodes");
    }
    out.push_str(": ");
    out.push_str(
        &FINDING_KINDS
            .iter()
            .map(|&kind| format!("{} {}", counts.get(kind), kind.as_str()))
            .collect::<Vec<_>>()
            .join(", "),
    );
    if selected.is_some() {
        out.push_str(&format!("; {} in the whole scope", all.len()));
    }
    out.push('\n');

    let width = shown
        .iter()
        .map(|finding| finding.kind.as_str().len())
        .max()
        .unwrap_or(0);
    for finding in shown.iter().take(MAX_LISTED_FINDINGS) {
        let kind = finding.kind.as_str();
        out.push_str(&format!(
            "{}{}  {}\n",
            kind,
            " ".repeat(width - kind.len()),
            finding.message
        ));
    }
    if shown.len() > MAX_LISTED_FINDINGS {
        out.push_str(&format!(
            "# {} more not listed; narrow with --nodes or --box\n",
            shown.len() - MAX_LISTED_FINDINGS
        ));
    }
    Ok(out)
}

/// The network `scope` names inside `network`, with the ids of the HOF chain
/// that leads to it. `Err` when a segment names no node or a node without a
/// body.
fn resolve_scope<'n>(
    network: &'n NodeNetwork,
    network_name: &str,
    scope: &[String],
) -> Result<(&'n NodeNetwork, Vec<u64>), String> {
    let mut current = network;
    let mut ids = Vec::new();
    for (depth, segment) in scope.iter().enumerate() {
        let names = unique_node_names(current);
        let where_ = if depth == 0 {
            network_name.to_string()
        } else {
            format!("{}/{}", network_name, scope[..depth].join("/"))
        };
        let Some(id) = names
            .iter()
            .find_map(|(id, name)| (name == segment).then_some(*id))
        else {
            return Err(format!("no node '{}' in {}", segment, where_));
        };
        let Some(body) = current.nodes[&id].zone.as_deref() else {
            return Err(format!("'{}' in {} has no body", segment, where_));
        };
        ids.push(id);
        current = body;
    }
    Ok((current, ids))
}

/// Whitespace around the nodes an image frames, in canvas px. Not used for a
/// `--box`, which is the frame exactly.
pub const IMAGE_FRAME_MARGIN: f64 = 40.0;

/// What `query --layout --image` draws: a rectangle in one scope's
/// coordinates (body-local inside a body, as the read-out prints them).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageFrame {
    /// The ids of the HOF chain down to the scope; empty for the root.
    pub scope_ids: Vec<u64>,
    pub rect: Rect,
}

/// The frame of the image the read-out with the same arguments describes.
///
/// [`ReadoutFilter::Box`] is the box itself: an image crops, it does not
/// filter, so everything inside it is drawn. [`ReadoutFilter::Nodes`] frames the
/// named nodes (unknown names are skipped; the read-out reports them) and
/// [`ReadoutFilter::All`] the whole scope, both with [`IMAGE_FRAME_MARGIN`].
/// `Err` when there is nothing to frame, or the scope is a body that is not
/// drawn because it is collapsed.
pub fn layout_image_frame(
    network: &NodeNetwork,
    registry: &NodeTypeRegistry,
    network_name: &str,
    scope: &[String],
    filter: &ReadoutFilter,
) -> Result<ImageFrame, String> {
    let (current, scope_ids) = resolve_scope(network, network_name, scope)?;
    // Every HOF on the way down must draw its body, or nothing inside is on
    // the canvas.
    let mut owner_scope = network;
    for (depth, id) in scope_ids.iter().enumerate() {
        let owner = &owner_scope.nodes[id];
        let collapsed = registry
            .get_node_type_for_node(owner)
            .is_none_or(|nt| !nt.has_zone() || resolve_body_collapsed(owner, nt));
        if collapsed {
            return Err(format!(
                "the body of '{}' is collapsed, so nothing in it is drawn",
                scope[..=depth].join("/")
            ));
        }
        owner_scope = owner.zone.as_deref().expect("resolve_scope checked it");
    }

    let items = scope_items(current, registry);
    let union = |rects: &mut dyn Iterator<Item = Rect>| -> Option<Rect> {
        rects.reduce(|a, b| bounding_box(&a, &b)).map(|r| {
            Rect::new(
                r.pos - DVec2::splat(IMAGE_FRAME_MARGIN),
                r.size + DVec2::splat(2.0 * IMAGE_FRAME_MARGIN),
            )
        })
    };
    let rect = match filter {
        ReadoutFilter::Box(area) => {
            if area.size.x <= 0.0 || area.size.y <= 0.0 {
                return Err("--box has no area".to_string());
            }
            Some(*area)
        }
        ReadoutFilter::All => union(&mut items.iter().map(|item| item.rect)),
        ReadoutFilter::Nodes(names) => union(
            &mut items
                .iter()
                .filter(|item| names.contains(&item.name))
                .map(|item| item.rect),
        ),
    };
    let rect = rect.ok_or_else(|| match filter {
        ReadoutFilter::Nodes(_) => {
            "none of the named nodes exist, so there is nothing to frame".to_string()
        }
        _ => "the scope has no nodes, so there is nothing to frame".to_string(),
    })?;
    Ok(ImageFrame { scope_ids, rect })
}

/// Every node of `scope` with its name, box and tags, sorted by top, then
/// left, then name: the order the read-out lists them in.
fn scope_items(scope: &NodeNetwork, registry: &NodeTypeRegistry) -> Vec<Item> {
    let names = unique_node_names(scope);
    let mut items: Vec<Item> = scope
        .nodes
        .values()
        .filter_map(|node| {
            let name = names.get(&node.id)?.clone();
            let mut tags = Vec::new();
            if node.ai_placed {
                tags.push("ai");
            }
            if is_comment(node) {
                tags.push("comment");
            }
            if node.zone.is_some()
                && registry
                    .get_node_type_for_node(node)
                    .is_some_and(|nt| nt.has_zone() && !resolve_body_collapsed(node, nt))
            {
                tags.push("body");
            }
            Some(Item {
                name,
                rect: Rect::new(node.position, rendered_node_size(node, registry)),
                tags,
            })
        })
        .collect();
    items.sort_by(|a, b| {
        a.rect
            .top()
            .total_cmp(&b.rect.top())
            .then(a.rect.left().total_cmp(&b.rect.left()))
            .then_with(|| a.name.cmp(&b.name))
    });
    items
}

/// The maximal columns and rows of the scope, columns first, each kind in the
/// order of its first member.
///
/// A **column** is ≥ 2 nodes whose lefts lie within [`COLUMN_TOLERANCE`] of
/// one another, each starting at or below the previous one's bottom with a
/// gap of at most [`Gap::Wide`] down. A **row** is the same across: tops
/// within the tolerance, horizontal gaps at most `Gap::Wide` across. Nodes are
/// taken in reading order and each joins the open chain it continues with the
/// smallest gap, so the result does not depend on hash order.
fn detect_groups(items: &[Item]) -> Vec<Group> {
    let mut groups = chains(items, GroupKind::Column);
    groups.extend(chains(items, GroupKind::Row));
    groups
}

/// One edge of a box, as a coordinate.
type Edge = fn(&Rect) -> f64;

fn chains(items: &[Item], kind: GroupKind) -> Vec<Group> {
    // Along: the axis the group runs on. Across: the axis its members align on.
    let (start, end, align, max_gap): (Edge, Edge, Edge, f64) = match kind {
        GroupKind::Column => (Rect::top, Rect::bottom, Rect::left, Gap::Wide.down()),
        GroupKind::Row => (Rect::left, Rect::right, Rect::top, Gap::Wide.across()),
    };
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (&items[a].rect, &items[b].rect);
        start(ra)
            .total_cmp(&start(rb))
            .then(align(ra).total_cmp(&align(rb)))
            .then(a.cmp(&b))
    });

    // Each chain: members, and the min / max of their aligned edge.
    let mut open: Vec<(Vec<usize>, f64, f64)> = Vec::new();
    for index in order {
        let rect = &items[index].rect;
        let edge = align(rect);
        let best = open
            .iter()
            .enumerate()
            .filter_map(|(c, (members, lo, hi))| {
                let last = &items[*members.last().unwrap()].rect;
                let gap = start(rect) - end(last);
                let spread = hi.max(edge) - lo.min(edge);
                (gap >= 0.0 && gap <= max_gap && spread <= COLUMN_TOLERANCE).then_some((c, gap))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        match best {
            Some((c, _)) => {
                let chain = &mut open[c];
                chain.0.push(index);
                chain.1 = chain.1.min(edge);
                chain.2 = chain.2.max(edge);
            }
            None => open.push((vec![index], edge, edge)),
        }
    }

    open.into_iter()
        .filter(|(members, _, _)| members.len() >= 2)
        .map(|(members, _, _)| {
            let mut gaps: Vec<f64> = members
                .windows(2)
                .map(|pair| start(&items[pair[1]].rect) - end(&items[pair[0]].rect))
                .collect();
            gaps.sort_by(f64::total_cmp);
            let mid = gaps.len() / 2;
            let gap = if gaps.len() % 2 == 1 {
                gaps[mid]
            } else {
                (gaps[mid - 1] + gaps[mid]) / 2.0
            };
            Group { kind, members, gap }
        })
        .collect()
}

/// Append `rows` as left-aligned columns separated by two spaces, trailing
/// blanks trimmed. The first column is at least `min_first` wide.
fn push_table(out: &mut String, rows: &[[String; 4]], min_first: usize) {
    let mut widths = [min_first, 0, 0, 0];
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            line.push_str(cell);
            if column + 1 < row.len() {
                let pad = widths[column] - cell.chars().count() + 2;
                line.extend(std::iter::repeat_n(' ', pad));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
}

fn bounding_box(a: &Rect, b: &Rect) -> Rect {
    let min = a.pos.min(b.pos);
    let max = (a.pos + a.size).max(b.pos + b.size);
    Rect::new(min, max - min)
}

/// `(x0,y0)–(x1,y1)`, whole pixels.
fn corners(rect: &Rect) -> String {
    format!(
        "({},{})–({},{})",
        px(rect.left()),
        px(rect.top()),
        px(rect.right()),
        px(rect.bottom())
    )
}

/// A coordinate as the read-out prints it: whole pixels, rounded.
fn px(value: f64) -> i64 {
    value.round() as i64
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", n, if n == 1 { one } else { many })
}

/// A name with its backticks, if any, removed: `` `all` `` → `all`.
fn unquote(name: &str) -> &str {
    name.strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or(name)
}
