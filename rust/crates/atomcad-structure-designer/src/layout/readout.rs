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
    let mut current = network;
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
        current = body;
    }

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
