//! Multiple open documents, Phase 2 (`doc/design_multiple_documents.md` D9,
//! §9): one clipboard, and a paste that translates names through the file
//! that owns them.
//!
//! The matrix runs every reference kind against every relation between the
//! source and the target document, pasted at top level and into a zone
//! body. Each cell checks the pasted nodes resolve and none is frozen, the
//! wires among them survive, every name has the expected spelling, and undo
//! removes the paste exactly — or, for a refusal, that nothing changed and
//! every offending name is listed.

use super::library_links_refresh_test::{bump_mtime, incr};
use super::library_links_support::*;
use atomcad_structure_designer::clipboard::PasteRefusal;
use atomcad_structure_designer::document_set::{DocumentId, DocumentSet};
use atomcad_structure_designer::library_links::is_frozen;
use atomcad_structure_designer::node_network::{NodeNetwork, walk_all_nodes};
use atomcad_structure_designer::node_type_registry::collect_record_refs_in_node;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

const TIP: &str = "2\ntip\nC 0.0 0.0 0.0\nH 0.0 0.0 1.09\n";

const FOO: &str = "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
y = parameter { param_name: \"y\", data_type: Int, sort_order: 1 }
s = expr { a: x, b: y, expression: \"a * 10 + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
";

/// In every `Main`: a `map` whose body is the zone-body paste target.
const BODY_TARGET: &str = "tr = range { count: 2 }
tm = map { xs: tr, input_type: Int, output_type: Int, body { q = int { value: 1 } output q } }
";

/// Every target's `Main`.
const TARGET_MAIN: &str = concat!(
    "tr = range { count: 2 }
tm = map { xs: tr, input_type: Int, output_type: Int, body { q = int { value: 1 } output q } }
",
    "output tm
"
);

/// The host's `Main`: one fragment per reference kind (see [`KINDS`]).
const HOST_MAIN: &str = "i1 = int { value: 3 }
i2 = int { value: 4 }
f = `demolib.foo` { x: i1, y: i2 }
e1 = expr { a: f, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }] }
f2 = `demolib.foo` {}
ap = apply { f: @f2, arg0: i1, arg1: i2 }
rc = record_construct { schema: \"demolib.Miller\", h: i1, k: i2, l: i1 }
rd = record_destructure { schema: \"demolib.Miller\", record: rc }
r1 = range { count: 2 }
pr = product { target: \"demolib.Miller\", h: r1, k: r1, l: r1 }
m = parameter { param_name: \"m\", data_type: Record(`demolib.Miller`), sort_order: 0 }
arr = array { element_type: Record(`demolib.Miller`), elements: [] }
cl = closure { kind: \"custom\", params: [\"m\"], type_args: [Record(`demolib.Miller`), Int], body { p = int { value: 1 } output p } }
ex = expr { a: rc, expression: \"a\", parameters: [{ name: \"a\", data_type: Record(`demolib.Miller`) }] }
c = `demolib.common.slab` { n: i1 }
pt = record_construct { schema: \"demolib.common.Pt\", x: i1 }
t = import_xyz { file_name: \"../libs/tip.xyz\" }
mp = map { xs: pr, input_type: Record(`demolib.Miller`), output_type: Int, body { g = record_destructure { schema: \"demolib.Miller\", record: $element } h = `demolib.foo` { x: g.h, y: g.k } output h } }
hp = helper {}
output e1
";

/// A reference kind: a name and the host nodes that make up its fragment.
struct Kind {
    name: &'static str,
    nodes: &'static [&'static str],
    /// Whether the fragment can go into a zone body (a `parameter` cannot).
    into_body: bool,
}

const KINDS: &[Kind] = &[
    Kind {
        name: "network instance with wires in and out",
        nodes: &["i1", "i2", "f", "e1"],
        into_body: true,
    },
    Kind {
        name: "function-pin wire",
        nodes: &["i1", "i2", "f2", "ap"],
        into_body: true,
    },
    Kind {
        name: "record_construct / record_destructure",
        nodes: &["i1", "i2", "rc", "rd"],
        into_body: true,
    },
    Kind {
        name: "product",
        nodes: &["r1", "pr"],
        into_body: true,
    },
    Kind {
        name: "parameter typed with a user record",
        nodes: &["m"],
        into_body: false,
    },
    Kind {
        name: "HOF element type",
        nodes: &["arr"],
        into_body: true,
    },
    Kind {
        name: "closure type args",
        nodes: &["cl"],
        into_body: true,
    },
    Kind {
        name: "expr signature",
        nodes: &["i1", "i2", "rc", "ex"],
        into_body: true,
    },
    Kind {
        name: "names of a nested library",
        nodes: &["i1", "c", "pt"],
        into_body: true,
    },
    Kind {
        name: "relative data-file path",
        nodes: &["t"],
        into_body: true,
    },
    Kind {
        name: "references inside a HOF body",
        nodes: &["r1", "pr", "mp"],
        into_body: true,
    },
    Kind {
        name: "a network local to the host",
        nodes: &["hp"],
        into_body: true,
    },
];

// ---------------------------------------------------------------------------
// The workspace and the harness
// ---------------------------------------------------------------------------

/// `libs/common.cnnd` (record `Pt`, network `slab`); `libs/demolib.cnnd`
/// linking it as `common` (record `Miller`, networks `foo` and `tip`, which
/// reads `libs/tip.xyz`); `proj/host.cnnd` linking demolib as `demolib`; and
/// in `other/`: `host2.cnnd` linking demolib under the same alias,
/// `alias.cnnd` under `dl`, `viacommon.cnnd` linking only common as `cm`,
/// and `unrelated.cnnd` linking nothing.
struct Ws {
    _dir: tempfile::TempDir,
    libs: PathBuf,
    host: PathBuf,
    demolib: PathBuf,
    host2: PathBuf,
    alias: PathBuf,
    viacommon: PathBuf,
    unrelated: PathBuf,
}

fn write_design(path: &Path, links: &[(&str, &str)], networks: &[(&str, &str)]) {
    let mut d = new_design(path);
    for (rel, alias) in links {
        d.link_library(rel, alias).unwrap();
    }
    for (name, code) in networks {
        edit(&mut d, name, code);
    }
    d.set_active_node_network_name(Some("Main".to_string()));
    save(&mut d);
}

fn workspace() -> Ws {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let libs = root.join("libs");
    std::fs::create_dir_all(&libs).unwrap();
    std::fs::write(libs.join("tip.xyz"), TIP).unwrap();

    let common = libs.join("common.cnnd");
    {
        let mut d = new_design(&common);
        d.add_record_type_def(int_record("Pt", &["x"])).unwrap();
        edit(
            &mut d,
            "slab",
            "n = parameter { param_name: \"n\", data_type: Int, sort_order: 0 }
d = expr { a: n, expression: \"a * 2\", parameters: [{ name: \"a\", data_type: Int }] }
output d
",
        );
        edit(&mut d, "Main", TARGET_MAIN);
        save(&mut d);
    }
    let demolib = libs.join("demolib.cnnd");
    {
        let mut d = new_design(&demolib);
        d.link_library("common.cnnd", "common").unwrap();
        d.add_record_type_def(int_record("Miller", &["h", "k", "l"]))
            .unwrap();
        edit(&mut d, "foo", FOO);
        edit(
            &mut d,
            "tip",
            "t = import_xyz { file_name: \"tip.xyz\" }\noutput t\n",
        );
        edit(&mut d, "Main", TARGET_MAIN);
        save(&mut d);
    }
    let host = root.join("proj/host.cnnd");
    write_design(
        &host,
        &[("../libs/demolib.cnnd", "demolib")],
        &[
            ("helper", "v = int { value: 5 }\noutput v\n"),
            ("Main", &format!("{}{}", BODY_TARGET, HOST_MAIN)),
        ],
    );
    let other = |name: &str, links: &[(&str, &str)]| {
        let path = root.join("other").join(name);
        write_design(&path, links, &[("Main", TARGET_MAIN)]);
        path
    };
    let host2 = other("host2.cnnd", &[("../libs/demolib.cnnd", "demolib")]);
    let alias = other("alias.cnnd", &[("../libs/demolib.cnnd", "dl")]);
    let viacommon = other("viacommon.cnnd", &[("../libs/common.cnnd", "cm")]);
    let unrelated = other("unrelated.cnnd", &[]);
    Ws {
        _dir: dir,
        libs,
        host,
        demolib,
        host2,
        alias,
        viacommon,
        unrelated,
    }
}

/// A `DocumentSet` and the active slot, as `CADInstance` has them.
struct Tabs {
    set: DocumentSet,
    active: StructureDesigner,
}

impl Tabs {
    fn new() -> Self {
        let mut active = StructureDesigner::new();
        active.new_project_direct_editing();
        let set = DocumentSet::new(&mut active);
        Tabs { set, active }
    }

    fn open(&mut self, path: &Path) -> DocumentId {
        self.set
            .open(&mut self.active, &path.to_string_lossy())
            .unwrap_or_else(|e| panic!("open {}: {}", path.display(), e))
            .id
    }

    fn activate(&mut self, id: DocumentId) {
        self.set.activate(&mut self.active, id).expect("activate");
    }

    /// Activates `id` and copies its `Main` nodes named `names`.
    fn copy(&mut self, id: DocumentId, names: &[&str]) {
        self.activate(id);
        copy_nodes(&mut self.active, names);
    }
}

fn copy_nodes(d: &mut StructureDesigner, names: &[&str]) {
    d.set_active_node_network_name(Some("Main".to_string()));
    let ids: Vec<u64> = names.iter().map(|n| node_id(d, "Main", n)).collect();
    d.select_nodes(ids);
    assert!(d.copy_selection());
}

/// Where a cell pastes: the top level of `Main`, or the body of its `tm`.
#[derive(Clone, Copy, Debug)]
enum Into {
    TopLevel,
    Body,
}

fn scope_of(d: &StructureDesigner, into: Into) -> Vec<u64> {
    match into {
        Into::TopLevel => Vec::new(),
        Into::Body => vec![node_id(d, "Main", "tm")],
    }
}

fn paste_into(d: &mut StructureDesigner, into: Into) -> Result<Vec<u64>, PasteRefusal> {
    d.set_active_node_network_name(Some("Main".to_string()));
    d.clear_selection();
    let scope = scope_of(d, into);
    d.paste_at_position_scoped(&scope, DVec2::new(400.0, 400.0))
}

fn scope_network<'a>(d: &'a StructureDesigner, scope: &[u64]) -> &'a NodeNetwork {
    let mut net = &d.node_type_registry.node_networks["Main"];
    for id in scope {
        net = net.nodes[id].zone.as_deref().expect("a zone body");
    }
    net
}

/// The user names the nodes `ids` of `network` refer to, bodies included.
fn names_in(d: &StructureDesigner, network: &NodeNetwork, ids: &[u64]) -> BTreeSet<String> {
    let registry = &d.node_type_registry;
    let mut out = BTreeSet::new();
    let mut part = NodeNetwork::new_empty();
    let set: HashSet<u64> = ids.iter().copied().collect();
    part.copy_nodes_from(network, &set, DVec2::ZERO);
    walk_all_nodes(&part, &mut |node| {
        if !registry
            .built_in_node_types
            .contains_key(&node.node_type_name)
        {
            out.insert(node.node_type_name.clone());
        }
        collect_record_refs_in_node(node, &mut |name, _| {
            if !name.is_empty() && !registry.built_in_record_type_defs.contains_key(name) {
                out.insert(name.to_string());
            }
        });
    });
    out
}

/// Wires among the nodes `ids` of `network` (function-pin wires included).
fn internal_wires(network: &NodeNetwork, ids: &[u64]) -> usize {
    let set: HashSet<u64> = ids.iter().copied().collect();
    ids.iter()
        .map(|id| {
            network.nodes[id]
                .arguments
                .iter()
                .flat_map(|a| a.iter_source_pins())
                .filter(|(src, _)| set.contains(src))
                .count()
        })
        .sum()
}

fn xyz_path(network: &NodeNetwork, id: u64) -> String {
    network.nodes[&id]
        .data
        .as_any_ref()
        .downcast_ref::<ImportXYZData>()
        .expect("an import_xyz node")
        .file_name
        .clone()
        .unwrap_or_default()
}

fn refused_names(refusal: &PasteRefusal) -> BTreeSet<String> {
    refusal.lines.iter().map(|(n, _)| n.clone()).collect()
}

/// What a fragment is in the source.
struct Copied {
    names: BTreeSet<String>,
    wires: usize,
    node_count: usize,
}

fn copied(d: &StructureDesigner, nodes: &[&str]) -> Copied {
    let net = &d.node_type_registry.node_networks["Main"];
    let ids: Vec<u64> = nodes.iter().map(|n| node_id(d, "Main", n)).collect();
    Copied {
        names: names_in(d, net, &ids),
        wires: internal_wires(net, &ids),
        node_count: ids.len(),
    }
}

/// Pastes the clipboard into the active document and checks the cell: on
/// success that every pasted node resolves and is not frozen, the internal
/// wires survived and the names are what `expected` maps the source's to; on
/// refusal (some name maps to `None`) that the target is unchanged and every
/// unmappable name is listed. Returns the pasted node ids (empty on refusal).
fn check_cell(
    d: &mut StructureDesigner,
    label: &str,
    into: Into,
    copied: &Copied,
    expected: &dyn Fn(&str) -> Option<String>,
) -> Vec<u64> {
    let before = saved_text(d, None);
    let pushes = d.undo_stack.push_count();
    let unmappable: BTreeSet<String> = copied
        .names
        .iter()
        .filter(|n| expected(n).is_none())
        .cloned()
        .collect();
    let outcome = paste_into(d, into);
    if !unmappable.is_empty() {
        let refusal = match outcome {
            Err(r) => r,
            Ok(ids) => panic!("{label}: expected a refusal, pasted {ids:?}"),
        };
        assert_eq!(refused_names(&refusal), unmappable, "{label}: {refusal}");
        assert_eq!(
            saved_text(d, None),
            before,
            "{label}: a refusal changes nothing"
        );
        assert_eq!(d.undo_stack.push_count(), pushes, "{label}");
        return Vec::new();
    }
    let ids = outcome.unwrap_or_else(|r| panic!("{label}: refused: {r}"));
    assert_eq!(ids.len(), copied.node_count, "{label}: node count");
    let scope = scope_of(d, into);
    let net = scope_network(d, &scope);
    let registry = &d.node_type_registry;
    for id in &ids {
        let node = &net.nodes[id];
        assert!(
            !is_frozen(node, registry),
            "{label}: {} is frozen",
            node.node_type_name
        );
    }
    let names = names_in(d, net, &ids);
    for name in &names {
        assert!(
            registry.node_networks.contains_key(name)
                || registry.lookup_record_type_def(name).is_some(),
            "{label}: {name} does not resolve"
        );
    }
    assert_eq!(internal_wires(net, &ids), copied.wires, "{label}: wires");
    let want: BTreeSet<String> = copied.names.iter().map(|n| expected(n).unwrap()).collect();
    assert_eq!(names, want, "{label}: spelling");
    ids
}

fn undo_restores(d: &mut StructureDesigner, before: &str, label: &str) {
    assert!(d.undo(), "{label}: undo");
    assert_eq!(
        saved_text(d, None),
        before,
        "{label}: undo removes the paste exactly"
    );
}

// ---------------------------------------------------------------------------
// The matrix
// ---------------------------------------------------------------------------

/// `demolib.X` → `prefix.X` (`prefix` empty: `X`); `None` for names outside
/// demolib.
fn under(prefix: &'static str) -> impl Fn(&str) -> Option<String> {
    move |name: &str| {
        let rest = name.strip_prefix("demolib.")?;
        Some(if prefix.is_empty() {
            rest.to_string()
        } else {
            format!("{}.{}", prefix, rest)
        })
    }
}

type NameMap = Box<dyn Fn(&str) -> Option<String>>;

#[test]
fn every_reference_kind_pastes_with_the_targets_spelling_or_is_refused() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let targets: Vec<(&str, DocumentId, NameMap, &str)> = vec![
        (
            "same document",
            host,
            Box::new(|n: &str| Some(n.to_string())),
            "../libs/tip.xyz",
        ),
        (
            "same alias elsewhere",
            tabs.open(&ws.host2),
            Box::new(under("demolib")),
            "../libs/tip.xyz",
        ),
        (
            "another alias",
            tabs.open(&ws.alias),
            Box::new(under("dl")),
            "../libs/tip.xyz",
        ),
        (
            "target is the owning file",
            tabs.open(&ws.demolib),
            Box::new(under("")),
            "tip.xyz",
        ),
        (
            "target links only the nested library",
            tabs.open(&ws.viacommon),
            Box::new(|n: &str| {
                n.strip_prefix("demolib.common.")
                    .map(|r| format!("cm.{}", r))
            }),
            "../libs/tip.xyz",
        ),
        (
            "unrelated target",
            tabs.open(&ws.unrelated),
            Box::new(|_: &str| None),
            "../libs/tip.xyz",
        ),
    ];

    for kind in KINDS {
        tabs.activate(host);
        let copied = copied(&tabs.active, kind.nodes);
        for (relation, target, map, xyz) in &targets {
            for into in [Into::TopLevel, Into::Body] {
                if matches!(into, Into::Body) && !kind.into_body {
                    continue;
                }
                let label = format!("{} / {} / {:?}", kind.name, relation, into);
                tabs.copy(host, kind.nodes);
                tabs.activate(*target);
                let before = saved_text(&mut tabs.active, None);
                let ids = check_cell(&mut tabs.active, &label, into, &copied, map.as_ref());
                if ids.is_empty() {
                    continue;
                }
                if kind.nodes == ["t"] {
                    let scope = scope_of(&tabs.active, into);
                    let net = scope_network(&tabs.active, &scope);
                    assert_eq!(xyz_path(net, ids[0]), *xyz, "{label}: path");
                }
                undo_restores(&mut tabs.active, &before, &label);
            }
        }
    }
}

#[test]
fn a_library_copied_in_its_own_tab_pastes_under_the_hosts_alias() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let lib = tabs.open(&ws.demolib);
    tabs.activate(lib);
    edit(
        &mut tabs.active,
        "Main",
        &format!(
            "{}i1 = int {{ value: 3 }}
f = foo {{ x: i1, y: i1 }}
rc = record_construct {{ schema: \"Miller\", h: i1, k: i1, l: i1 }}
c = `common.slab` {{ n: i1 }}
t = tip {{}}
",
            TARGET_MAIN
        ),
    );
    let nodes = ["i1", "f", "rc", "c", "t"];
    let copied = copied(&tabs.active, &nodes);
    for into in [Into::TopLevel, Into::Body] {
        tabs.copy(lib, &nodes);
        tabs.activate(host);
        let before = saved_text(&mut tabs.active, None);
        let map = |n: &str| Some(format!("demolib.{}", n));
        check_cell(&mut tabs.active, "library → host", into, &copied, &map);
        undo_restores(&mut tabs.active, &before, "library → host");
    }
}

#[test]
fn untitled_sources_are_refused_elsewhere_and_untitled_targets_get_absolute_paths() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);

    // A target with no folder: a relative path becomes absolute.
    let untitled = tabs.set.new_document(&mut tabs.active, false).unwrap();
    tabs.copy(host, &["t"]);
    tabs.activate(untitled);
    let ids = paste_into(&mut tabs.active, Into::TopLevel).unwrap();
    let path = xyz_path(scope_network(&tabs.active, &[]), ids[0]);
    assert!(Path::new(&path).is_absolute(), "{path}");
    assert_eq!(
        std::fs::canonicalize(&path).unwrap(),
        std::fs::canonicalize(ws.libs.join("tip.xyz")).unwrap()
    );
    // An Untitled target links nothing.
    tabs.copy(host, &["i1", "i2", "f"]);
    tabs.activate(untitled);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert_eq!(
        refused_names(&refusal),
        BTreeSet::from(["demolib.foo".to_string()])
    );
    assert!(
        refusal.to_string().contains("this Untitled design"),
        "{refusal}"
    );

    // A network of an Untitled source cannot be reached from anywhere else.
    edit(
        &mut tabs.active,
        "local",
        "v = int { value: 5 }\noutput v\n",
    );
    edit(&mut tabs.active, "Main", "l = local {}\n");
    copy_nodes(&mut tabs.active, &["l"]);
    tabs.activate(host);
    let before = saved_text(&mut tabs.active, None);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert_eq!(
        refused_names(&refusal),
        BTreeSet::from(["local".to_string()])
    );
    assert!(refusal.to_string().contains("Untitled"), "{refusal}");
    assert_eq!(saved_text(&mut tabs.active, None), before);
    // In its own document it pastes as always.
    tabs.activate(untitled);
    assert_eq!(
        paste_into(&mut tabs.active, Into::TopLevel).unwrap().len(),
        1
    );
}

#[test]
fn a_host_local_network_is_refused_in_its_library_naming_the_file() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let lib = tabs.open(&ws.demolib);
    tabs.copy(host, &["hp", "f2"]);
    tabs.activate(lib);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert_eq!(
        refused_names(&refusal),
        BTreeSet::from(["helper".to_string()])
    );
    assert_eq!(
        refusal.lines[0].1,
        "`helper` is defined in `host.cnnd`, which `demolib.cnnd` does not link."
    );
}

// ---------------------------------------------------------------------------
// Staleness: the interface check
// ---------------------------------------------------------------------------

/// `foo` with a third parameter.
const FOO3: &str = "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
y = parameter { param_name: \"y\", data_type: Int, sort_order: 1 }
z = parameter { param_name: \"z\", data_type: Int, sort_order: 2 }
s = expr { a: x, b: y, expression: \"a * 10 + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
";

#[test]
fn an_unsaved_interface_change_refuses_the_paste_in_both_directions() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let lib = tabs.open(&ws.demolib);
    tabs.activate(lib);
    incr(&mut tabs.active, "foo", FOO3);

    // Host → library: the library's own `foo` has changed since.
    tabs.copy(host, &["i1", "i2", "f"]);
    tabs.activate(lib);
    let before = saved_text(&mut tabs.active, None);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert_eq!(
        refused_names(&refusal),
        BTreeSet::from(["demolib.foo".to_string()])
    );
    assert!(
        refusal.lines[0]
            .1
            .contains("`foo` has different parameters"),
        "{refusal}"
    );
    assert_eq!(saved_text(&mut tabs.active, None), before);

    // Library → host: the host's mount is the saved version.
    edit(
        &mut tabs.active,
        "Main",
        "i1 = int { value: 1 }\nf = foo { x: i1 }\n",
    );
    copy_nodes(&mut tabs.active, &["i1", "f"]);
    tabs.activate(host);
    let before = saved_text(&mut tabs.active, None);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert_eq!(refused_names(&refusal), BTreeSet::from(["foo".to_string()]));
    assert_eq!(
        refusal.lines[0].1,
        "`demolib.foo` has different parameters in this design than where it was copied — save `demolib.cnnd`, or refresh it here."
    );
    assert_eq!(saved_text(&mut tabs.active, None), before);

    // Saved, the host picks it up on activation and the paste goes through.
    tabs.activate(lib);
    save(&mut tabs.active);
    bump_mtime(&ws.demolib);
    tabs.activate(host);
    let ids = paste_into(&mut tabs.active, Into::TopLevel).unwrap();
    assert_eq!(ids.len(), 2);
}

#[test]
fn a_changed_body_with_the_same_interface_pastes() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let lib = tabs.open(&ws.demolib);
    tabs.activate(lib);
    incr(
        &mut tabs.active,
        "foo",
        &FOO.replace("a * 10 + b", "a * 100 + b"),
    );
    tabs.copy(host, &["i1", "i2", "f"]);
    tabs.activate(lib);
    let ids = paste_into(&mut tabs.active, Into::TopLevel).unwrap();
    assert_eq!(ids.len(), 3);
}

#[test]
fn a_saved_change_held_by_redo_history_refuses_until_refreshed() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let lib = tabs.open(&ws.demolib);
    // Redo history in the host: the library change will be held (D6).
    tabs.activate(host);
    edit(
        &mut tabs.active,
        "helper",
        "v = int { value: 6 }\noutput v\n",
    );
    assert!(tabs.active.undo());
    tabs.activate(lib);
    incr(&mut tabs.active, "foo", FOO3);
    save(&mut tabs.active);
    bump_mtime(&ws.demolib);
    edit(
        &mut tabs.active,
        "Main",
        "i1 = int { value: 1 }\nf = foo { x: i1 }\n",
    );
    copy_nodes(&mut tabs.active, &["i1", "f"]);
    tabs.activate(host);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert!(refusal.to_string().contains("refresh it here"), "{refusal}");
}

// ---------------------------------------------------------------------------
// Upkeep, frozen nodes, headless, app state
// ---------------------------------------------------------------------------

fn clipboard_types(d: &StructureDesigner) -> Vec<String> {
    let mut names: Vec<String> = d
        .clipboard
        .as_ref()
        .expect("a clipboard")
        .nodes
        .nodes
        .values()
        .map(|n| n.node_type_name.clone())
        .collect();
    names.sort();
    names
}

fn owner_names(d: &StructureDesigner) -> Vec<String> {
    let c = d.clipboard.as_ref().expect("a clipboard");
    c.origin.owners.keys().cloned().collect()
}

#[test]
fn renames_follow_the_clipboard_only_in_its_own_document() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let a = tabs.open(&ws.host);
    let b = tabs.open(&ws.host2);
    tabs.copy(a, &["hp"]);
    assert_eq!(clipboard_types(&tabs.active), vec!["helper"]);

    // In A: the clipboard follows the rename, its undo and its redo.
    assert!(tabs.active.rename_node_network("helper", "helper2"));
    assert_eq!(clipboard_types(&tabs.active), vec!["helper2"]);
    assert_eq!(owner_names(&tabs.active), vec!["helper2"]);
    assert!(tabs.active.undo());
    assert_eq!(clipboard_types(&tabs.active), vec!["helper"]);
    assert_eq!(owner_names(&tabs.active), vec!["helper"]);
    assert!(tabs.active.redo());
    assert_eq!(clipboard_types(&tabs.active), vec!["helper2"]);
    assert!(tabs.active.undo());

    // In B: a network of the same name is not the clipboard's.
    tabs.activate(b);
    tabs.active.add_node_network("helper");
    assert!(tabs.active.rename_node_network("helper", "other"));
    assert_eq!(clipboard_types(&tabs.active), vec!["helper"]);
    assert!(tabs.active.undo());
    assert!(tabs.active.redo());
    assert_eq!(clipboard_types(&tabs.active), vec!["helper"]);
    tabs.active.delete_node_network("other").unwrap();
    assert!(tabs.active.clipboard.is_some());

    // Deleting it in A clears the clipboard, as always.
    tabs.activate(a);
    edit(&mut tabs.active, "Main", "k = int { value: 1 }\n");
    tabs.active.delete_node_network("helper").unwrap();
    assert!(tabs.active.clipboard.is_none());
}

#[test]
fn record_and_alias_renames_follow_the_clipboard() {
    let ws = workspace();
    let mut d = open(&ws.host);
    copy_nodes(&mut d, &["i1", "i2", "rc", "f"]);
    assert_eq!(owner_names(&d), vec!["demolib.Miller", "demolib.foo"]);
    d.rename_library_alias("demolib", "dm").unwrap();
    assert_eq!(owner_names(&d), vec!["dm.Miller", "dm.foo"]);
    assert!(d.undo());
    assert_eq!(owner_names(&d), vec!["demolib.Miller", "demolib.foo"]);
    assert!(d.redo());
    assert_eq!(owner_names(&d), vec!["dm.Miller", "dm.foo"]);
    assert!(d.undo());

    d.add_record_type_def(int_record("Loc", &["a"])).unwrap();
    edit(
        &mut d,
        "Main",
        "i1 = int { value: 1 }\nlc = record_construct { schema: \"Loc\", a: i1 }\n",
    );
    copy_nodes(&mut d, &["i1", "lc"]);
    d.rename_record_type_def("Loc", "Loc2").unwrap();
    assert_eq!(owner_names(&d), vec!["Loc2"]);
    assert!(d.undo());
    assert_eq!(owner_names(&d), vec!["Loc"]);
}

#[test]
fn frozen_nodes_paste_in_their_own_document_and_are_refused_elsewhere() {
    let ws = workspace();
    // `demolib.foo` disappears from the library: the host's instances freeze.
    {
        let mut lib = open(&ws.demolib);
        lib.delete_node_network("foo").unwrap();
        save(&mut lib);
    }
    let mut tabs = Tabs::new();
    let host = tabs.open(&ws.host);
    let other = tabs.open(&ws.host2);
    tabs.activate(host);
    let f = node_id(&tabs.active, "Main", "f");
    assert!(is_frozen(
        &tabs.active.node_type_registry.node_networks["Main"].nodes[&f],
        &tabs.active.node_type_registry
    ));
    copy_nodes(&mut tabs.active, &["i1", "i2", "f"]);
    assert_eq!(
        paste_into(&mut tabs.active, Into::TopLevel).unwrap().len(),
        3
    );
    tabs.activate(other);
    let refusal = paste_into(&mut tabs.active, Into::TopLevel).unwrap_err();
    assert_eq!(
        refused_names(&refusal),
        BTreeSet::from(["demolib.foo".to_string()])
    );
    assert!(refusal.lines[0].1.contains("does not resolve"), "{refusal}");
}

#[test]
fn a_bare_designer_copies_and_pastes_as_before() {
    let ws = workspace();
    let mut d = open(&ws.host);
    copy_nodes(&mut d, &["i1", "i2", "f", "rc"]);
    let ids = paste_into(&mut d, Into::TopLevel).unwrap();
    assert_eq!(ids.len(), 4);
    let net = &d.node_type_registry.node_networks["Main"];
    assert_eq!(
        names_in(&d, net, &ids),
        BTreeSet::from(["demolib.Miller".to_string(), "demolib.foo".to_string()])
    );
}

#[test]
fn an_in_place_load_detaches_the_clipboard_from_the_new_content() {
    let ws = workspace();
    let mut d = open(&ws.host);
    copy_nodes(&mut d, &["i1", "i2", "f", "rc"]);

    // The new content does not link demolib: refused, not pasted unchecked.
    d.load_node_networks(&ws.unrelated.to_string_lossy())
        .unwrap();
    let before = saved_text(&mut d, None);
    let refusal = paste_into(&mut d, Into::TopLevel).unwrap_err();
    assert_eq!(
        refused_names(&refusal),
        BTreeSet::from(["demolib.Miller".to_string(), "demolib.foo".to_string()])
    );
    assert_eq!(saved_text(&mut d, None), before);

    // One that links it under another alias gets that spelling.
    d.load_node_networks(&ws.alias.to_string_lossy()).unwrap();
    let ids = paste_into(&mut d, Into::TopLevel).unwrap();
    let net = &d.node_type_registry.node_networks["Main"];
    assert_eq!(
        names_in(&d, net, &ids),
        BTreeSet::from(["dl.Miller".to_string(), "dl.foo".to_string()])
    );

    // A new project is Untitled and links nothing.
    d.new_project();
    assert!(paste_into(&mut d, Into::TopLevel).is_err());
}

#[test]
fn the_clipboard_is_app_state() {
    let ws = workspace();
    let mut tabs = Tabs::new();
    let a = tabs.open(&ws.host);
    let b = tabs.open(&ws.host2);
    tabs.copy(a, &["i1"]);
    tabs.activate(b);
    assert!(tabs.active.clipboard.is_some());
    assert!(tabs.set.parked(a).unwrap().clipboard.is_none());
    assert_eq!(tabs.active.clipboard.as_ref().unwrap().origin.document, a);
}

// ---------------------------------------------------------------------------
// The latent path bug (§4 D9): copying out of a linked network
// ---------------------------------------------------------------------------

#[test]
fn a_paste_out_of_a_linked_network_rebases_relative_paths() {
    let ws = workspace();
    let mut d = open(&ws.host);
    d.set_active_node_network_name(Some("demolib.tip".to_string()));
    let t = node_id(&d, "demolib.tip", "t");
    d.select_node(t);
    assert!(d.copy_selection());
    d.set_active_node_network_name(Some("Main".to_string()));
    let pasted = d.paste_at_position(DVec2::new(0.0, 0.0)).unwrap();
    assert_eq!(pasted.len(), 1);
    let net = &d.node_type_registry.node_networks["Main"];
    assert_eq!(xyz_path(net, pasted[0]), "../libs/tip.xyz");
}

#[test]
fn duplicate_into_my_file_rebases_relative_paths() {
    let ws = workspace();
    let mut d = open(&ws.host);
    let copy = d.duplicate_node_network("demolib.tip").unwrap();
    assert_eq!(copy, "tip");
    let t = node_id(&d, "tip", "t");
    assert_eq!(
        xyz_path(&d.node_type_registry.node_networks["tip"], t),
        "../libs/tip.xyz"
    );
}
