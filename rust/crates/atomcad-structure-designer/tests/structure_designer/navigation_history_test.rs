//! `NavigationHistory` on its own: the session's back/forward list, whose
//! entries name a document and a network. Cross-tab behaviour through
//! `DocumentSet` is in `document_navigation_test.rs`.

use atomcad_structure_designer::document_set::DocumentId;
use atomcad_structure_designer::navigation_history::{NavigationEntry, NavigationHistory};

const A: DocumentId = DocumentId(1);
const B: DocumentId = DocumentId(2);

fn at(document: DocumentId, network: &str) -> NavigationEntry {
    NavigationEntry::new(document, Some(network.to_string()))
}

fn any(_: &NavigationEntry) -> bool {
    true
}

fn visit(history: &mut NavigationHistory, document: DocumentId, network: &str) {
    history.navigate_to(at(document, network));
}

/// The networks of `document` in history order.
fn names(history: &NavigationHistory) -> Vec<String> {
    history
        .entries()
        .iter()
        .map(|e| format!("{}:{}", e.document, e.network.as_deref().unwrap_or("-")))
        .collect()
}

#[test]
fn starts_empty() {
    let history = NavigationHistory::new();
    assert_eq!(history.current(), None);
    assert!(history.entries().is_empty());
    assert!(!history.can_navigate_back(any));
    assert!(!history.can_navigate_forward(any));
}

#[test]
fn the_first_visit_has_nothing_behind_it() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    assert_eq!(history.current(), Some(&at(A, "N1")));
    assert!(!history.can_navigate_back(any));

    visit(&mut history, A, "N2");
    assert!(history.can_navigate_back(any));
    assert!(!history.can_navigate_forward(any));
}

#[test]
fn back_and_forward_walk_the_visits() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, A, "N2");
    visit(&mut history, B, "N3");

    assert_eq!(history.navigate_back(any), Some(at(A, "N2")));
    assert_eq!(history.navigate_back(any), Some(at(A, "N1")));
    assert_eq!(history.navigate_back(any), None, "nothing before the first");
    assert_eq!(history.current(), Some(&at(A, "N1")));

    assert_eq!(history.navigate_forward(any), Some(at(A, "N2")));
    assert_eq!(history.navigate_forward(any), Some(at(B, "N3")));
    assert_eq!(history.navigate_forward(any), None);
    assert_eq!(
        names(&history),
        ["1:N1", "1:N2", "2:N3"],
        "moving records nothing"
    );
}

#[test]
fn a_visit_after_going_back_drops_the_forward_history() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, B, "N2");
    visit(&mut history, A, "N3");
    history.navigate_back(any);
    history.navigate_back(any);

    visit(&mut history, B, "X");
    assert!(!history.can_navigate_forward(any));
    assert_eq!(names(&history), ["1:N1", "2:X"]);
}

#[test]
fn a_visit_to_the_current_place_records_nothing() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, A, "N1");
    assert_eq!(names(&history), ["1:N1"]);

    // The same network name in another document is another place.
    visit(&mut history, B, "N1");
    assert_eq!(names(&history), ["1:N1", "2:N1"]);
}

#[test]
fn unusable_entries_are_stepped_over_not_removed() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, B, "gone");
    visit(&mut history, A, "N3");
    let usable = |e: &NavigationEntry| e.network.as_deref() != Some("gone");

    assert_eq!(history.back_target(usable), Some(0));
    assert_eq!(history.navigate_back(usable), Some(at(A, "N1")));
    assert_eq!(history.navigate_forward(usable), Some(at(A, "N3")));
    assert_eq!(
        names(&history),
        ["1:N1", "2:gone", "1:N3"],
        "kept for an undo"
    );

    // With every earlier entry unusable there is no Back.
    assert!(!history.can_navigate_back(|e| e.document == A && e.network.as_deref() == Some("N3")));
}

#[test]
fn entries_equal_to_the_current_place_are_stepped_over() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, B, "gone");
    visit(&mut history, A, "N1");
    // "gone" is unusable, so the only earlier usable entry is where we are.
    let usable = |e: &NavigationEntry| e.network.as_deref() != Some("gone");
    assert!(!history.can_navigate_back(usable), "Back must visibly move");
}

#[test]
fn move_to_out_of_range_changes_nothing() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    assert_eq!(history.move_to(5), None);
    assert_eq!(history.current(), Some(&at(A, "N1")));
}

#[test]
fn a_rename_follows_only_its_own_document() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Physics");
    visit(&mut history, B, "Physics");
    visit(&mut history, A, "Math");
    visit(&mut history, A, "Physics");

    history.rename_network(A, "Physics", "Mechanics");
    assert_eq!(
        names(&history),
        ["1:Mechanics", "2:Physics", "1:Math", "1:Mechanics"]
    );
    assert_eq!(history.current(), Some(&at(A, "Mechanics")));
}

#[test]
fn removing_a_network_not_current() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Physics");
    visit(&mut history, A, "Math");
    visit(&mut history, A, "Chemistry");
    history.navigate_back(any);

    history.remove_network(A, "Chemistry");
    assert_eq!(history.current(), Some(&at(A, "Math")));
    assert!(!history.can_navigate_forward(any));
    assert_eq!(history.navigate_back(any), Some(at(A, "Physics")));
}

#[test]
fn removing_the_current_network_makes_the_previous_one_current() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Physics");
    visit(&mut history, A, "Math");
    visit(&mut history, A, "Chemistry");
    history.navigate_back(any); // at Math, Chemistry ahead

    history.remove_network(A, "Math");
    assert_eq!(history.current(), Some(&at(A, "Physics")));
    assert_eq!(history.navigate_forward(any), Some(at(A, "Chemistry")));
}

#[test]
fn removing_the_first_current_entry_keeps_the_next_one() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Physics");
    visit(&mut history, A, "Math");
    history.navigate_back(any);

    history.remove_network(A, "Physics");
    assert_eq!(history.current(), Some(&at(A, "Math")));
    assert!(!history.can_navigate_back(any));
}

#[test]
fn removing_a_network_is_scoped_to_its_document() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Main");
    visit(&mut history, B, "Main");
    visit(&mut history, A, "Other");

    history.remove_network(B, "Main");
    assert_eq!(names(&history), ["1:Main", "1:Other"]);
}

#[test]
fn removal_merges_neighbours_that_became_equal() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Physics");
    visit(&mut history, A, "Math");
    visit(&mut history, A, "Physics");
    visit(&mut history, A, "Chemistry");

    history.remove_network(A, "Math");
    assert_eq!(names(&history), ["1:Physics", "1:Chemistry"]);
    assert_eq!(history.current(), Some(&at(A, "Chemistry")));
    assert_eq!(history.navigate_back(any), Some(at(A, "Physics")));
    assert!(!history.can_navigate_back(any));
}

#[test]
fn removing_every_entry_leaves_an_empty_history() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "Physics");
    visit(&mut history, B, "Physics");
    history.remove_document(A);
    history.remove_document(B);
    assert_eq!(history.current(), None);
    assert!(!history.can_navigate_back(any));
    assert!(!history.can_navigate_forward(any));

    visit(&mut history, A, "Again");
    assert_eq!(names(&history), ["1:Again"]);
}

#[test]
fn removing_a_document_forgets_all_its_visits() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, B, "M1");
    visit(&mut history, A, "N2");
    visit(&mut history, B, "M2");
    history.navigate_back(any); // at A:N2

    history.remove_document(B);
    assert_eq!(names(&history), ["1:N1", "1:N2"]);
    assert_eq!(history.current(), Some(&at(A, "N2")));
    assert!(!history.can_navigate_forward(any));
}

#[test]
fn a_visit_without_a_network_is_a_place_too() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    history.navigate_to(NavigationEntry::new(B, None));
    assert_eq!(history.navigate_back(any), Some(at(A, "N1")));
    assert_eq!(
        history.navigate_forward(any),
        Some(NavigationEntry::new(B, None))
    );
}

#[test]
fn clear_forgets_everything() {
    let mut history = NavigationHistory::new();
    visit(&mut history, A, "N1");
    visit(&mut history, B, "N2");
    history.navigate_back(any);
    history.clear();
    assert_eq!(history.current(), None);
    assert!(!history.can_navigate_back(any));
    assert!(!history.can_navigate_forward(any));
}
