//! Regression: loading a `.cnnd` repairs each network as it is inserted, in
//! name order, so a HOF body calling a custom network whose name sorts later
//! meets an instance whose type does not resolve yet. The zone-body repair
//! used to compare the body's zone-input wires against `DataType::None` for
//! such a destination and drop them — silently, on every reopen. (Found by
//! library linking Phase 6: *Make local copy* puts `a.*` networks after
//! `Main`; a linked library whose `bar` body calls its own `foo` lost the same
//! wires at every mount.)

use super::library_links_support::*;
use std::collections::BTreeSet;

#[test]
fn a_body_instance_of_a_network_that_sorts_later_keeps_its_zone_input_wires_on_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.cnnd");
    let mut d = new_design(&path);
    edit(
        &mut d,
        "zfoo",
        "x = parameter { param_name: \"x\", data_type: Int, sort_order: 0 }
output x
",
    );
    edit(
        &mut d,
        "Main",
        "r = range { count: 2 }
mp = map { xs: r, input_type: Int, output_type: Int, body { g = zfoo { x: $element } output g } }
output mp
",
    );
    save(&mut d);
    let before = wire_ledger(&d);
    assert!(
        before.iter().any(|w| !w.scope.is_empty()),
        "the body wire must be there to lose"
    );
    let reopened = open(&path);
    ok(check_wire_ledger(
        &before,
        &wire_ledger(&reopened),
        &BTreeSet::new(),
    ));
    ok(check_invariants(&reopened));
}
