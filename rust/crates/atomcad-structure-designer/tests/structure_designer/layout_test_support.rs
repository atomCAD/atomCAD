//! Shared scaffolding for the incremental-layout tests
//! (`doc/design_incremental_layout.md`).
//!
//! Kept beside [`layout_oracle`](super::layout_oracle), which holds the
//! invariants; this module holds only the boring parts — an empty network, a
//! path lookup, and the [`Fixture`] every whole-pass scenario is written
//! against — so no layout test has to grow its own copy.

#![allow(dead_code)]

use std::collections::HashSet;

use glam::DVec2;

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::layout::{
    LayoutAlgorithm, WireKey, collect_all_wires, layout_incremental,
};
use atomcad_structure_designer::node_data::NoData;
use atomcad_structure_designer::node_network::{Node, NodeNetwork};
use atomcad_structure_designer::node_type::{
    NodeType, NodeTypeCategory, OutputPinDefinition, no_data_loader, no_data_saver,
};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::{
    NamePath, PositionSnapshot, edit_network, snapshot_node_positions,
};

use super::layout_oracle::{self, Drawing, Placed, Touched};

/// The vertical clearance the pass keeps, `layout::common::VERTICAL_GAP`.
pub const VERTICAL_GAP: f64 = 30.0;

/// A bare network to run `edit_network` against.
pub fn empty_network() -> NodeNetwork {
    NodeNetwork::new(NodeType {
        name: "test".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(NoData {}),
        node_data_saver: no_data_saver,
        node_data_loader: no_data_loader,
    })
}

/// The node at a name path, descending through zone bodies. Panics with the
/// path if any segment is missing — a layout test that mistypes a name should
/// say so, not silently assert nothing.
pub fn node_by_path<'a>(network: &'a NodeNetwork, path: &[&str]) -> &'a Node {
    let (last, prefix) = path.split_last().expect("empty node path");
    let mut scope = network;
    for name in prefix {
        let node = named(scope, name, path);
        scope = node
            .zone
            .as_deref()
            .unwrap_or_else(|| panic!("`{name}` in {path:?} owns no body"));
    }
    named(scope, last, path)
}

/// The node at a name path, for mutation — descending through `zone_mut()` so
/// the body `Arc`'s copy-on-write stays intact.
///
/// The setup half of every incremental-layout scenario: a test arranges its
/// "before" drawing by hand, because the point of the pass is what it does to
/// an arrangement a human made.
pub fn node_by_path_mut<'a>(network: &'a mut NodeNetwork, path: &[&str]) -> &'a mut Node {
    let (last, prefix) = path.split_last().expect("empty node path");
    let mut scope = network;
    for name in prefix {
        let id = named(scope, name, path).id;
        scope = scope
            .nodes
            .get_mut(&id)
            .and_then(|node| node.zone_mut())
            .unwrap_or_else(|| panic!("`{name}` in {path:?} owns no body"));
    }
    let id = named(scope, last, path).id;
    scope.nodes.get_mut(&id).expect("just resolved")
}

/// The id of the node at a name path.
pub fn id_by_path(network: &NodeNetwork, path: &[&str]) -> u64 {
    node_by_path(network, path).id
}

fn named<'a>(scope: &'a NodeNetwork, name: &str, path: &[&str]) -> &'a Node {
    scope
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named `{name}` on path {path:?}"))
}

// ============================================================================
// The whole-pass harness
// ============================================================================

/// A hand-arranged network, its pre-edit identity pair, and the pass that runs
/// against them.
///
/// Every incremental-layout scenario arranges its "before" drawing **by hand**
/// and then runs one text edit through `layout_incremental`, because the
/// property the whole design exists to buy is about a drawing a human made: the
/// edit's nodes are fitted in, and nothing else moves unless they left it no
/// room.
pub struct Fixture {
    pub network: NodeNetwork,
    pub registry: NodeTypeRegistry,
    pub snapshot: PositionSnapshot,
    pub wires: HashSet<WireKey>,
    pub before: Drawing,
}

impl Fixture {
    /// Build a network from `code` and take the baseline from it.
    pub fn new(code: &str) -> Self {
        let registry = NodeTypeRegistry::new();
        let mut network = empty_network();
        let result = edit_network(&mut network, &registry, code, false);
        assert!(result.success, "setup edit failed: {:?}", result.errors);
        let mut fixture = Self {
            network,
            registry,
            snapshot: PositionSnapshot::new(),
            wires: HashSet::new(),
            before: Drawing::default(),
        };
        fixture.baseline();
        fixture
    }

    /// Re-take the pre-edit pair. Every setup mutator ends with this, so a test
    /// can never measure an edit against a drawing that predates its own setup.
    pub fn baseline(&mut self) {
        self.snapshot = snapshot_node_positions(&self.network, &self.registry);
        self.wires = collect_all_wires(&self.network);
        self.before = Drawing::of(&self.network, &self.registry);
    }

    /// Put a node where the "human" wants it.
    pub fn place(&mut self, path: &[&str], x: f64, y: f64) -> &mut Self {
        node_by_path_mut(&mut self.network, path).position = DVec2::new(x, y);
        self.baseline();
        self
    }

    /// Claim a node as hand-placed, the way the drag handler does (D5).
    ///
    /// Layout never reads this as permission to skip a repair — it is a
    /// tiebreaker, so a test that sets it is asserting a *preference*, and each
    /// one here has a control case with the flag off.
    pub fn hand_moved(&mut self, path: &[&str]) -> &mut Self {
        node_by_path_mut(&mut self.network, path).hand_moved = true;
        self.baseline();
        self
    }

    /// Resize a zone body, as the user's resize drag would.
    pub fn body_size(&mut self, path: &[&str], width: f64, height: f64) -> &mut Self {
        let node = node_by_path_mut(&mut self.network, path);
        node.body_width = width;
        node.body_height = height;
        self.baseline();
        self
    }

    pub fn edit(&mut self, code: &str) {
        let result = edit_network(&mut self.network, &self.registry, code, false);
        assert!(result.success, "edit failed: {:?}", result.errors);
    }

    /// Run the incremental pass and assert every whole-pass invariant.
    pub fn run(&mut self) -> Outcome {
        let moved = layout_incremental(
            &mut self.network,
            &self.registry,
            &self.snapshot,
            &self.wires,
            LayoutAlgorithm::Sugiyama,
        )
        .moved;
        let after = Drawing::of(&self.network, &self.registry);
        let moved_paths: HashSet<NamePath> =
            moved.iter().map(|(path, _, _)| path.clone()).collect();
        layout_oracle::check(
            &self.before,
            &after,
            &Touched::new(self.delta_paths(&after), moved_paths),
        );
        Outcome { moved, after }
    }

    /// The paths the delta accounts for: added, grown or removed. Derived from
    /// the snapshot rather than from `diff_scope`, so the oracle is not checking
    /// the pass against its own input.
    pub fn delta_paths(&self, after: &Drawing) -> HashSet<NamePath> {
        let mut out = HashSet::new();
        for path in after.paths() {
            match self.snapshot.get(&path) {
                None => {
                    out.insert(path);
                }
                Some(state) => {
                    let now = after.get(&path).expect("just enumerated");
                    if now.size.x > state.footprint.x || now.size.y > state.footprint.y {
                        out.insert(path);
                    }
                }
            }
        }
        for path in self.snapshot.keys() {
            if after.get(path).is_none() {
                out.insert(path.clone());
            }
        }
        out
    }

    pub fn id(&self, path: &[&str]) -> u64 {
        id_by_path(&self.network, path)
    }

    /// The rect a node held in the pre-edit drawing.
    pub fn was(&self, path: &[&str]) -> Placed {
        self.before
            .get(&p(path))
            .unwrap_or_else(|| panic!("no node at {path:?} before the pass"))
    }
}

/// What one pass did.
pub struct Outcome {
    pub moved: Vec<(NamePath, DVec2, DVec2)>,
    pub after: Drawing,
}

impl Outcome {
    pub fn rect(&self, path: &[&str]) -> Placed {
        self.after
            .get(&p(path))
            .unwrap_or_else(|| panic!("no node at {path:?} after the pass"))
    }

    pub fn moved_paths(&self) -> Vec<NamePath> {
        let mut paths: Vec<NamePath> = self.moved.iter().map(|(path, _, _)| path.clone()).collect();
        paths.sort();
        paths
    }

    /// Assert the pass moved nothing outside `allowed`.
    ///
    /// A subset rather than an equality, because an added node whose block
    /// lands exactly where the creation-time placer had already dropped it
    /// never *moves*, and that is not a difference worth writing two tests for.
    pub fn assert_only_moved(&self, allowed: &[&[&str]]) {
        let allowed: Vec<NamePath> = allowed.iter().map(|path| p(path)).collect();
        for path in self.moved_paths() {
            assert!(
                allowed.contains(&path),
                "{} moved, but only {allowed:?} were allowed to",
                path.join("/")
            );
        }
    }

    /// How far one node moved during the pass, or `None` if it did not.
    pub fn shift(&self, path: &[&str]) -> Option<DVec2> {
        self.moved
            .iter()
            .find(|(moved, _, _)| moved == &p(path))
            .map(|(_, from, to)| *to - *from)
    }
}

/// A name path from its segments.
pub fn p(segments: &[&str]) -> NamePath {
    segments.iter().map(|s| s.to_string()).collect()
}

/// The [`Touched`] claim for a pass, derived from the two drawings alone.
///
/// The delta half — added, grown, removed — is read off the before/after
/// geometry rather than taken from `diff_scope`, deliberately: an oracle fed
/// the pass's own input would be checking the pass against itself. The `moved`
/// half is the pass's own reported list, which is exactly the claim under test
/// ("everything else is bit-identical").
///
/// Used by the corpus run, where there is no [`Fixture`] to ask.
pub fn touched_between<I>(before: &Drawing, after: &Drawing, moved: I) -> Touched
where
    I: IntoIterator<Item = NamePath>,
{
    let mut delta: HashSet<NamePath> = HashSet::new();
    for path in after.paths() {
        match before.get(&path) {
            // Present only after: the edit added it.
            None => {
                delta.insert(path);
            }
            Some(was) => {
                let now = after.get(&path).expect("just enumerated");
                if now.size.x > was.size.x || now.size.y > was.size.y {
                    delta.insert(path);
                }
            }
        }
    }
    for path in before.paths() {
        if after.get(&path).is_none() {
            delta.insert(path);
        }
    }
    Touched::new(delta, moved)
}

pub fn center(rect: Placed) -> DVec2 {
    rect.position + rect.size * 0.5
}

/// One `expr` statement: `name = expr { x: source, … }`.
///
/// The workhorse of these tests: an `expr` takes a wire, keeps a stable
/// footprint whether that pin is wired or not (its subtitle is the expression
/// string, which never changes), and can be rewired by re-stating it.
pub fn expr(name: &str, source: &str) -> String {
    format!(
        "{name} = expr {{ x: {source}, expression: \"x + 1\", \
         parameters: [{{ name: \"x\", data_type: Int }}] }}\n"
    )
}
