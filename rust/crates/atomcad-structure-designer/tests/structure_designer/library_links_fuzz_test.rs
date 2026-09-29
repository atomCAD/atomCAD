//! Library linking: the randomized harness of `doc/design_library_linking.md`
//! §11.1 — sequences nobody thought of, judged by the oracles after every step.
//!
//! One run is ~30 steps drawn from library edits written to disk (as another
//! process would), host edits, and app actions (check, refresh, undo, redo,
//! save and reopen, rename alias, make local). After every step: O2 on the
//! operations that must not change the host's wiring except as reported
//! (check, refresh, reopen, host edits, rename alias, make local), O5 on undo
//! and redo, O3 (the library file changes only in the
//! harness's own library steps), O6. At the end: O4 on the final state, then
//! undo all the way back to the last open and require its O1.
//!
//! Deterministic per seed. CI runs a fixed seed set; a longer run is
//! `LIBRARY_LINKING_FUZZ_CASES=500 cargo test -j 4 library_links_fuzz`. A
//! failure prints the seed and the step list; minimize it by hand into a named
//! test in `library_links_refresh_test.rs` before fixing it.

use super::library_links_refresh_test::{BAR, FOO, Ws, bump_mtime, edit_lib, workspace};
use super::library_links_support::*;
use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::library_refresh::{RefreshReport, frozen_nodes_of};
use atomcad_structure_designer::network_validator::live_parameters;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use std::collections::{BTreeMap, BTreeSet};

/// An incremental library edit that may leave the library itself with
/// validation errors (a retype that its own `expr` rejects is still a state a
/// library can be saved in).
fn soft_incr(d: &mut StructureDesigner, network: &str, code: &str) {
    d.set_active_node_network_name(Some(network.to_string()));
    d.ai_text_edit(code, false);
    d.validate_active_network();
}

/// `foo` re-created with parameters of other names than the original's.
const FOO_OTHER_NAMES: &str = "u = parameter { param_name: \"u\", data_type: Int, sort_order: 0 }
v = parameter { param_name: \"v\", data_type: Int, sort_order: 1 }
s = expr { a: u, b: v, expression: \"a * 10 + b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }] }
output s
";

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1))
    }
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n.max(1)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        (!items.is_empty()).then(|| &items[self.below(items.len() as u64) as usize])
    }
}

type DestKey = (String, Vec<u64>, u64);

/// What O2 compares across one step.
struct Snapshot {
    records: BTreeSet<WireRecord>,
    frozen: BTreeSet<DestKey>,
}

fn snapshot(d: &StructureDesigner) -> Snapshot {
    let registry = &d.node_type_registry;
    let locals: Vec<String> = registry
        .node_networks
        .keys()
        .filter(|n| registry.library_links.mount_containing(n).is_none())
        .cloned()
        .collect();
    Snapshot {
        records: wire_records(d),
        frozen: frozen_nodes_of(registry, &locals).into_keys().collect(),
    }
}

fn dest_of(r: &WireRecord) -> DestKey {
    (r.network.clone(), r.scope.clone(), r.dest)
}

/// O2 across a step, following nodes that froze or unfroze: a node frozen
/// before or after keeps its arguments exactly, so it is judged by position;
/// a node that resolved throughout, by identity; a node that unfroze moved
/// from position to identity, so only its wire count is conserved. Every
/// wire that is gone must be in the report; none may appear. Wires the report
/// warns about (§7.3) are excluded.
fn check_o2(before: &Snapshot, after: &Snapshot, report: &RefreshReport) -> Result<(), String> {
    let warned: BTreeSet<(DestKey, u64, u8)> = report
        .output_pin_warnings
        .iter()
        .map(|w| {
            (
                (w.network.clone(), w.scope_path.clone(), w.node_id),
                w.source_node_id,
                w.source_scope_depth,
            )
        })
        .collect();
    let unwarned = |r: &&WireRecord| !warned.contains(&(dest_of(r), r.source, r.depth));
    let dropped = reported_drops(report);
    let was_reported = |r: &WireRecord, slot: &Slot| {
        dropped.contains(&WireEntry {
            network: r.network.clone(),
            scope: r.scope.clone(),
            dest: r.dest,
            slot: slot.clone(),
            source: r.source,
            source_pin: r.source_pin.clone(),
            depth: r.depth,
        })
    };
    let same = |a: &WireRecord, b: &WireRecord, positional: bool| {
        dest_of(a) == dest_of(b)
            && a.source == b.source
            && a.source_pin == b.source_pin
            && a.depth == b.depth
            && if positional {
                a.pos == b.pos
            } else {
                // Same parameter by id — or, for a network re-created under
                // its old name (fresh ids, matched by name), by name. Names
                // are unique per node, so a name match cannot be a wrong pin.
                // (That recycled ids never move a wire is pinned down by the
                // named tests in `library_links_refresh_test.rs`.)
                a.id == b.id || (a.name.is_some() && a.name == b.name)
            }
    };
    // Nodes that unfroze moved from positional to identity keys — and so did
    // an `apply` fed by an unfrozen node's function pin, whose `arg…` inputs
    // are keyed by that node's parameter ids (which may be new: a re-created
    // network gets fresh ids and is matched by name).
    let mut unfrozen: BTreeSet<DestKey> =
        before.frozen.difference(&after.frozen).cloned().collect();
    for r in before.records.iter().chain(after.records.iter()) {
        let source_key = (r.network.clone(), r.scope.clone(), r.source);
        if r.pos == Slot::Pos(0)
            && r.depth == 0
            && r.source_pin == "out-1"
            && unfrozen.contains(&source_key)
        {
            unfrozen.insert(dest_of(r));
        }
    }
    let mut problems = Vec::new();
    let mut counts: BTreeMap<DestKey, (usize, usize)> = BTreeMap::new();
    for r in before.records.iter().filter(unwarned) {
        let key = dest_of(r);
        let (bf, af) = (before.frozen.contains(&key), after.frozen.contains(&key));
        if unfrozen.contains(&key) {
            counts.entry(key).or_default().0 += 1;
            continue;
        }
        let positional = bf || af;
        let present = after.records.iter().any(|a| same(r, a, positional));
        let reported = was_reported(r, if positional { &r.pos } else { &r.id });
        if !present && !reported {
            problems.push(format!("silent loss: {:?}", r));
        }
    }
    for a in after.records.iter().filter(unwarned) {
        let key = dest_of(a);
        let (bf, af) = (before.frozen.contains(&key), after.frozen.contains(&key));
        if unfrozen.contains(&key) {
            counts.entry(key).or_default().1 += 1;
            continue;
        }
        let positional = bf || af;
        if !before.records.iter().any(|r| same(r, a, positional)) {
            problems.push(format!("appeared: {:?}", a));
        }
    }
    for (key, (b, a)) in counts {
        let reported = report
            .dropped_wires
            .iter()
            .filter(|w| (w.network.clone(), w.scope_path.clone(), w.node_id) == key)
            .count();
        if a + reported != b {
            problems.push(format!(
                "unfrozen node {:?}: {} wires before, {} after, {} reported",
                key, b, a, reported
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// O1 plus every mount fingerprint: what O5 compares.
fn state(d: &mut StructureDesigner) -> (String, BTreeMap<String, String>) {
    (local_fingerprint(d), mount_fingerprints(d))
}

struct Run {
    ws: Ws,
    d: StructureDesigner,
    rng: Rng,
    log: Vec<String>,
    seed: u64,
    /// What the library file must hold, as the harness last wrote it.
    lib_bytes: Option<Vec<u8>>,
    /// The last parseable library, to restore after a corruption or delete.
    good_lib: Vec<u8>,
    /// O5: the state before each command on the undo stack / redo stack.
    undo_states: Vec<(String, BTreeMap<String, String>)>,
    redo_states: Vec<(String, BTreeMap<String, String>)>,
    /// O1 right after the last open (the end-of-run undo target).
    opened: String,
    next_name: u64,
}

impl Run {
    #[track_caller]
    fn fail(&self, what: &str) -> ! {
        panic!(
            "library_links_fuzz seed={} failed after steps:\n  {}\n{}",
            self.seed,
            self.log.join("\n  "),
            what
        );
    }

    fn check(&self, r: Result<(), String>) {
        if let Err(e) = r {
            self.fail(&e);
        }
    }

    /// The library file exists and parses (it is the last good version).
    fn lib_ok(&self) -> bool {
        self.lib_bytes.as_ref() == Some(&self.good_lib)
    }

    /// A library edit, as another process would make it.
    fn lib_edit(&mut self, what: String, f: impl FnOnce(&mut StructureDesigner)) {
        if !self.lib_ok() {
            self.log
                .push(format!("{} (skipped: library not readable)", what));
            return;
        }
        self.log.push(what);
        edit_lib(&self.ws.lib, f);
        let bytes = std::fs::read(&self.ws.lib).unwrap();
        self.good_lib = bytes.clone();
        self.lib_bytes = Some(bytes);
    }

    /// Runs an operation that must pass O2 against its report, and tracks
    /// whether it pushed an undo step.
    fn judged(&mut self, what: &str, op: impl FnOnce(&mut StructureDesigner) -> RefreshReport) {
        self.log.push(what.to_string());
        let before = snapshot(&self.d);
        let pre = state(&mut self.d);
        let pushes = self.d.undo_stack.push_count();
        let report = op(&mut self.d);
        let after = snapshot(&self.d);
        self.check(
            check_o2(&before, &after, &report).map_err(|e| format!("O2 after {}:\n{}", what, e)),
        );
        if self.d.undo_stack.push_count() != pushes {
            self.undo_states.push(pre);
            self.redo_states.clear();
        }
    }

    /// The *node* names of `foo`'s parameter nodes (a rename changes the
    /// parameter's name, not its node's), in interface order.
    fn param_names(&self) -> Vec<String> {
        let d = open(&self.ws.lib);
        let Some(net) = d.node_type_registry.node_networks.get("foo") else {
            return Vec::new();
        };
        let mut nodes: Vec<(String, String)> =
            net.nodes
                .values()
                .filter(|n| n.node_type_name == "parameter")
                .filter_map(|n| {
                    let p = n
                    .data
                    .as_any_ref()
                    .downcast_ref::<atomcad_structure_designer::nodes::parameter::ParameterData>()?;
                    Some((p.param_name.clone(), n.custom_name.clone()?))
                })
                .collect();
        let order: Vec<String> = live_parameters(net).into_iter().map(|p| p.name).collect();
        nodes.sort_by_key(|(param, _)| order.iter().position(|o| o == param));
        nodes.into_iter().map(|(_, node)| node).collect()
    }

    fn field_names(&self) -> Vec<String> {
        let d = open(&self.ws.lib);
        d.node_type_registry
            .lookup_record_type_def("Miller")
            .map(|def| def.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default()
    }

    /// The library's alias now — renamed by a rename step, gone after a
    /// *Make local copy* (until that is undone).
    fn alias(&self) -> Option<String> {
        self.d
            .node_type_registry
            .library_links
            .direct_mounts()
            .next()
            .map(|m| m.alias.clone())
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.next_name += 1;
        format!("{}{}", prefix, self.next_name)
    }

    fn step(&mut self) {
        match self.rng.below(21) {
            // --- library edits -------------------------------------------
            0 => {
                let name = self.fresh("p");
                let order = self.rng.below(5) as i32 - 2;
                self.lib_edit(format!("lib: add parameter {} at {}", name, order), |l| {
                    if l.node_type_registry.node_networks.contains_key("foo") {
                        soft_incr(
                            l,
                            "foo",
                            &format!(
                                "{n} = parameter {{ param_name: \"{n}\", data_type: Int, sort_order: {o} }}\n",
                                n = name,
                                o = order
                            ),
                        );
                    }
                });
            }
            1 => {
                let names = if self.lib_ok() {
                    self.param_names()
                } else {
                    vec![]
                };
                if let Some(p) = self.rng.pick(&names).cloned() {
                    self.lib_edit(format!("lib: remove parameter {}", p), |l| {
                        soft_incr(l, "foo", &format!("delete {}\n", p));
                    });
                }
            }
            2 => {
                let names = if self.lib_ok() {
                    self.param_names()
                } else {
                    vec![]
                };
                if let Some(p) = self.rng.pick(&names).cloned() {
                    let order = self.rng.below(7) as i32 - 3;
                    self.lib_edit(format!("lib: reorder {} to {}", p, order), |l| {
                        soft_incr(
                            l,
                            "foo",
                            &format!("{} = parameter {{ sort_order: {} }}\n", p, order),
                        );
                    });
                }
            }
            3 => {
                let names = if self.lib_ok() {
                    self.param_names()
                } else {
                    vec![]
                };
                if let Some(p) = self.rng.pick(&names).cloned() {
                    let new = self.fresh("r");
                    self.lib_edit(format!("lib: rename parameter {} to {}", p, new), |l| {
                        soft_incr(
                            l,
                            "foo",
                            &format!("{} = parameter {{ param_name: \"{}\" }}\n", p, new),
                        );
                    });
                }
            }
            4 => {
                let names = if self.lib_ok() {
                    self.param_names()
                } else {
                    vec![]
                };
                if let Some(p) = self.rng.pick(&names).cloned() {
                    let t = *self.rng.pick(&["Float", "Int", "String"]).unwrap();
                    self.lib_edit(format!("lib: retype {} to {}", p, t), |l| {
                        soft_incr(
                            l,
                            "foo",
                            &format!("{} = parameter {{ data_type: {} }}\n", p, t),
                        );
                    });
                }
            }
            5 => {
                let mut fields = if self.lib_ok() {
                    self.field_names()
                } else {
                    vec![]
                };
                if fields.is_empty() {
                    return;
                }
                let desc = match self.rng.below(4) {
                    0 => {
                        let f = self.fresh("f");
                        let at = self.rng.below(fields.len() as u64 + 1) as usize;
                        fields.insert(at, f.clone());
                        format!("add field {} at {}", f, at)
                    }
                    1 if fields.len() > 1 => {
                        let at = self.rng.below(fields.len() as u64) as usize;
                        format!("remove field {}", fields.remove(at))
                    }
                    2 => {
                        let at = self.rng.below(fields.len() as u64) as usize;
                        let f = fields.remove(at);
                        fields.push(f.clone());
                        format!("move field {} last", f)
                    }
                    _ => {
                        let at = self.rng.below(fields.len() as u64) as usize;
                        let f = self.fresh("g");
                        let old = std::mem::replace(&mut fields[at], f.clone());
                        format!("rename field {} to {} (by name: a new field)", old, f)
                    }
                };
                self.lib_edit(format!("lib: Miller {}", desc), |l| {
                    let _ = l.update_record_type_def(
                        "Miller",
                        fields.iter().map(|f| (f.clone(), DataType::Int)).collect(),
                    );
                });
            }
            6 => {
                self.lib_edit("lib: remove foo (and bar)".to_string(), |l| {
                    let _ = l.delete_node_network("bar");
                    let _ = l.delete_node_network("foo");
                });
            }
            7 => {
                // Re-add `foo` as it was, or with parameters of other names:
                // either way it must not take the old network's ids.
                let renamed = self.rng.below(2) == 0;
                let what = if renamed {
                    "lib: re-add foo (other parameter names) and bar"
                } else {
                    "lib: re-add foo and bar"
                };
                self.lib_edit(what.to_string(), |l| {
                    if !l.node_type_registry.node_networks.contains_key("foo") {
                        if renamed {
                            edit(l, "foo", FOO_OTHER_NAMES);
                            edit(l, "bar", "c = foo {}\noutput c\n");
                        } else {
                            edit(l, "foo", FOO);
                            edit(l, "bar", BAR);
                        }
                    }
                });
            }
            8 => {
                // Delete the file, or restore it.
                if self.lib_ok() {
                    self.log.push("lib: delete file".to_string());
                    std::fs::remove_file(&self.ws.lib).unwrap();
                    self.lib_bytes = None;
                } else if self.lib_bytes.is_none() {
                    self.log.push("lib: restore file".to_string());
                    std::fs::write(&self.ws.lib, &self.good_lib).unwrap();
                    bump_mtime(&self.ws.lib);
                    self.lib_bytes = Some(self.good_lib.clone());
                }
            }
            9 => {
                // Make it unparseable, or fix it.
                if self.lib_ok() {
                    self.log.push("lib: corrupt file".to_string());
                    let bytes = b"{ not a design".to_vec();
                    std::fs::write(&self.ws.lib, &bytes).unwrap();
                    bump_mtime(&self.ws.lib);
                    self.lib_bytes = Some(bytes);
                } else {
                    self.log.push("lib: fix file".to_string());
                    std::fs::write(&self.ws.lib, &self.good_lib).unwrap();
                    bump_mtime(&self.ws.lib);
                    self.lib_bytes = Some(self.good_lib.clone());
                }
            }
            // --- app actions ---------------------------------------------
            10 | 11 => self.judged("check_dependencies", |d| {
                d.check_dependencies().unwrap_or_default()
            }),
            12 => {
                let alias = self.alias().unwrap_or_else(|| "lib".to_string());
                self.judged("refresh_library", |d| {
                    d.refresh_library(&alias).unwrap_or_default()
                })
            }
            13 | 14 => {
                self.log.push("undo".to_string());
                let pre = state(&mut self.d);
                if self.d.undo() {
                    let expected = self
                        .undo_states
                        .pop()
                        .unwrap_or_else(|| self.fail("undo without a recorded state"));
                    let now = state(&mut self.d);
                    if now != expected {
                        self.fail(&format!(
                            "O5: undo is not the inverse\n--- expected\n{:#?}\n--- got\n{:#?}",
                            expected, now
                        ));
                    }
                    self.redo_states.push(pre);
                }
            }
            15 => {
                self.log.push("redo".to_string());
                let pre = state(&mut self.d);
                if self.d.redo() {
                    let expected = self
                        .redo_states
                        .pop()
                        .unwrap_or_else(|| self.fail("redo without a recorded state"));
                    let now = state(&mut self.d);
                    if now != expected {
                        self.fail(&format!("O5: redo does not restore the post-state\n--- expected\n{:#?}\n--- got\n{:#?}", expected, now));
                    }
                    self.undo_states.push(pre);
                }
            }
            16 => {
                self.log.push("save and reopen".to_string());
                let before = snapshot(&self.d);
                save(&mut self.d);
                let mut d = open(&self.ws.host);
                let report = d.take_load_library_report().unwrap_or_default();
                let after = snapshot(&d);
                self.check(
                    check_o2(&before, &after, &report)
                        .map_err(|e| format!("O2 across reopen:\n{}", e)),
                );
                self.d = d;
                self.undo_states.clear();
                self.redo_states.clear();
                self.opened = local_fingerprint(&mut self.d);
            }
            // --- the two operations that change what the link is (P6) -----
            19 => {
                let Some(old) = self.alias() else {
                    self.log
                        .push("rename alias (skipped: not linked)".to_string());
                    return;
                };
                let new = self.fresh("lib");
                self.judged(&format!("rename alias {} to {}", old, new), |d| {
                    d.rename_library_alias(&old, &new)
                        .unwrap_or_else(|e| panic!("rename refused: {}", e));
                    RefreshReport::default()
                });
            }
            20 => {
                let Some(alias) = self.alias() else {
                    self.log
                        .push("make local (skipped: not linked)".to_string());
                    return;
                };
                // O2 on the networks that were the host's before: vendoring
                // adds the library's networks — and their wires — to them.
                let locals: BTreeSet<String> = {
                    let registry = &self.d.node_type_registry;
                    registry
                        .node_networks
                        .keys()
                        .filter(|n| registry.library_links.mount_containing(n).is_none())
                        .cloned()
                        .collect()
                };
                let before = snapshot(&self.d);
                let pre = state(&mut self.d);
                let pushes = self.d.undo_stack.push_count();
                match self.d.make_library_local(&alias) {
                    Ok(()) => self.log.push(format!("make {} local", alias)),
                    Err(e) => {
                        self.log
                            .push(format!("make {} local (refused: {})", alias, e));
                        if state(&mut self.d) != pre {
                            self.fail("a refused make-local changed the design");
                        }
                    }
                }
                let mut after = snapshot(&self.d);
                after.records.retain(|r| locals.contains(&r.network));
                after.frozen.retain(|(n, _, _)| locals.contains(n));
                self.check(
                    check_o2(&before, &after, &RefreshReport::default()).map_err(|e| {
                        format!(
                            "O2 across make local:
{}",
                            e
                        )
                    }),
                );
                if self.d.undo_stack.push_count() != pushes {
                    self.undo_states.push(pre);
                    self.redo_states.clear();
                }
            }
            // --- host edits ----------------------------------------------
            _ => {
                let n = self.fresh("n");
                self.judged(&format!("host: add node {}", n), |d| {
                    // Frozen nodes make the network report errors; the edit
                    // itself must still apply.
                    d.set_active_node_network_name(Some("Main".to_string()));
                    let outcome = d.ai_text_edit(&format!("{} = int {{ value: 7 }}\n", n), false);
                    assert!(
                        outcome.result.nodes_created.contains(&n),
                        "{:?}",
                        outcome.result.errors
                    );
                    d.validate_active_network();
                    RefreshReport::default()
                });
            }
        }
    }

    /// O3: the library file holds what the harness last wrote.
    fn check_library_untouched(&self) {
        let now = std::fs::read(&self.ws.lib).ok();
        if now != self.lib_bytes {
            self.fail("O3: the library file changed outside a library step");
        }
    }
}

fn run(seed: u64, steps: usize) {
    let ws = workspace();
    let d = open(&ws.host);
    let good_lib = std::fs::read(&ws.lib).unwrap();
    let mut r = Run {
        d,
        rng: Rng::new(seed),
        log: Vec::new(),
        seed,
        lib_bytes: Some(good_lib.clone()),
        good_lib,
        undo_states: Vec::new(),
        redo_states: Vec::new(),
        opened: String::new(),
        next_name: 0,
        ws,
    };
    r.opened = local_fingerprint(&mut r.d);
    for _ in 0..steps {
        r.step();
        r.check_library_untouched();
        r.check(check_invariants(&r.d));
    }
    // O4 on the final state, with the library readable and memory up to date.
    if !r.lib_ok() {
        std::fs::write(&r.ws.lib, &r.good_lib).unwrap();
        bump_mtime(&r.ws.lib);
        r.lib_bytes = Some(r.good_lib.clone());
    }
    r.judged("refresh_all_dependencies", |d| {
        d.refresh_all_dependencies().unwrap_or_default()
    });
    r.log.push("O4".to_string());
    let o4 = check_save_reopen_identity(&mut r.d);
    r.check(o4);
    // Undo all the way back to the last open.
    r.log.push("undo to the start".to_string());
    while r.d.undo() {}
    if local_fingerprint(&mut r.d) != r.opened {
        r.fail("undoing everything does not return to the state after the last open");
    }
}

#[test]
fn library_links_fuzz_fixed_seeds() {
    let cases: u64 = std::env::var("LIBRARY_LINKING_FUZZ_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6);
    for seed in 0..cases {
        run(seed * 7919 + 17, 30);
    }
}
