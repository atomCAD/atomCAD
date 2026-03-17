use rust_lib_flutter_cad::structure_designer::navigation_history::NavigationHistory;

#[test]
fn test_initial_state() {
    let history = NavigationHistory::new();
    assert_eq!(history.current(), None);
    assert!(!history.can_navigate_back());
    assert!(!history.can_navigate_forward());
}

#[test]
fn test_basic_navigation() {
    let mut history = NavigationHistory::new();

    // First navigation replaces initial None, so can't navigate back
    history.navigate_to(Some("Network1".to_string()));
    assert_eq!(history.current(), Some("Network1".to_string()));
    assert!(!history.can_navigate_back()); // Changed: initial None was replaced
    assert!(!history.can_navigate_forward());

    // Second navigation adds to history, now we can navigate back
    history.navigate_to(Some("Network2".to_string()));
    assert_eq!(history.current(), Some("Network2".to_string()));
    assert!(history.can_navigate_back());
    assert!(!history.can_navigate_forward());
}

#[test]
fn test_back_forward() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Network1".to_string()));
    history.navigate_to(Some("Network2".to_string()));
    history.navigate_to(Some("Network3".to_string()));

    // Go back twice
    assert_eq!(history.navigate_back(), Some(Some("Network2".to_string())));
    assert_eq!(history.current(), Some("Network2".to_string()));

    assert_eq!(history.navigate_back(), Some(Some("Network1".to_string())));
    assert_eq!(history.current(), Some("Network1".to_string()));

    // Can't go back further (initial None was replaced)
    assert!(!history.can_navigate_back());

    // Go forward
    assert_eq!(
        history.navigate_forward(),
        Some(Some("Network2".to_string()))
    );
    assert_eq!(
        history.navigate_forward(),
        Some(Some("Network3".to_string()))
    );
    assert!(!history.can_navigate_forward());
}

#[test]
fn test_truncate_forward_history() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Network1".to_string()));
    history.navigate_to(Some("Network2".to_string()));
    history.navigate_to(Some("Network3".to_string()));

    // Go back twice
    history.navigate_back();
    history.navigate_back();
    assert_eq!(history.current(), Some("Network1".to_string()));

    // Navigate to a new network - should truncate forward history
    history.navigate_to(Some("NetworkX".to_string()));
    assert_eq!(history.current(), Some("NetworkX".to_string()));
    assert!(!history.can_navigate_forward());

    // Network2 and Network3 should be gone, can only go back to Network1
    history.navigate_back();
    assert_eq!(history.current(), Some("Network1".to_string()));
    assert!(!history.can_navigate_back()); // At the beginning (initial None was replaced)
}

#[test]
fn test_no_duplicate_consecutive_entries() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Network1".to_string()));
    history.navigate_to(Some("Network1".to_string())); // Should not add duplicate

    // History should only have 1 entry: Network1 (initial None was replaced)
    assert!(!history.can_navigate_back()); // Can't go back (only one entry)
    assert_eq!(history.current(), Some("Network1".to_string()));
}

#[test]
fn test_rename_network() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Math".to_string()));
    history.navigate_to(Some("Physics".to_string())); // Navigate back to Physics

    // Current is Physics, history contains: Physics, Math, Physics (initial None was replaced)
    assert_eq!(history.current(), Some("Physics".to_string()));

    // Rename Physics to Mechanics
    history.rename_network("Physics", "Mechanics");

    // Current should now be Mechanics
    assert_eq!(history.current(), Some("Mechanics".to_string()));

    // Navigate through history to verify all occurrences were renamed
    history.navigate_back();
    assert_eq!(history.current(), Some("Math".to_string()));

    history.navigate_back();
    assert_eq!(history.current(), Some("Mechanics".to_string())); // Was Physics

    // Can't navigate back further (at the beginning)
    assert!(!history.can_navigate_back());
}

#[test]
fn test_remove_network_not_current() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Math".to_string()));
    history.navigate_to(Some("Chemistry".to_string()));

    // Go back to Math
    history.navigate_back();
    assert_eq!(history.current(), Some("Math".to_string()));

    // Remove Chemistry (which is in forward history)
    history.remove_network("Chemistry");

    // Current should still be Math
    assert_eq!(history.current(), Some("Math".to_string()));

    // Should not be able to navigate forward to Chemistry anymore
    assert!(!history.can_navigate_forward());

    // Can still navigate back
    history.navigate_back();
    assert_eq!(history.current(), Some("Physics".to_string()));
}

#[test]
fn test_remove_network_current() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Math".to_string()));
    history.navigate_to(Some("Chemistry".to_string()));

    // Current is Chemistry
    assert_eq!(history.current(), Some("Chemistry".to_string()));

    // Remove the current network
    history.remove_network("Chemistry");

    // Current should now be Math (the previous entry)
    assert_eq!(history.current(), Some("Math".to_string()));

    // Should still be able to navigate backward
    assert!(history.can_navigate_back());
    history.navigate_back();
    assert_eq!(history.current(), Some("Physics".to_string()));
}

#[test]
fn test_remove_network_multiple_occurrences() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Math".to_string()));
    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Chemistry".to_string()));

    // History: Physics, Math, Physics, Chemistry (initial None was replaced)
    // Current: Chemistry

    history.remove_network("Physics");

    // History should now be: Math, Chemistry
    // Current should still be Chemistry
    assert_eq!(history.current(), Some("Chemistry".to_string()));

    history.navigate_back();
    assert_eq!(history.current(), Some("Math".to_string()));

    // Can't navigate back further (at the beginning)
    assert!(!history.can_navigate_back());
}

#[test]
fn test_remove_all_networks_leaves_none() {
    let mut history = NavigationHistory::new();

    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Physics".to_string()));

    // Remove all Physics entries
    history.remove_network("Physics");

    // Should be left with just None
    assert_eq!(history.current(), None);
    assert!(!history.can_navigate_back());
    assert!(!history.can_navigate_forward());
}

#[test]
fn test_clear() {
    let mut history = NavigationHistory::new();

    // Build up some history
    history.navigate_to(Some("Physics".to_string()));
    history.navigate_to(Some("Math".to_string()));
    history.navigate_to(Some("Chemistry".to_string()));

    // Go back to create forward history too
    history.navigate_back();
    assert_eq!(history.current(), Some("Math".to_string()));
    assert!(history.can_navigate_back());
    assert!(history.can_navigate_forward());

    // Clear should reset to initial state
    history.clear();

    // Should be back to initial state
    assert_eq!(history.current(), None);
    assert!(!history.can_navigate_back());
    assert!(!history.can_navigate_forward());

    // Should be able to start navigating again
    // First navigation replaces initial None, so can't navigate back yet
    history.navigate_to(Some("NewNetwork".to_string()));
    assert_eq!(history.current(), Some("NewNetwork".to_string()));
    assert!(!history.can_navigate_back()); // Changed: initial None was replaced
}

#[test]
fn test_initial_none_replacement() {
    let mut history = NavigationHistory::new();

    // Initial state has one None entry
    assert_eq!(history.current(), None);
    assert!(!history.can_navigate_back());

    // First navigation should REPLACE the initial None, not append to it
    history.navigate_to(Some("FirstNetwork".to_string()));
    assert_eq!(history.current(), Some("FirstNetwork".to_string()));
    assert!(!history.can_navigate_back()); // Can't go back to a state we never experienced

    // Second navigation should now append normally
    history.navigate_to(Some("SecondNetwork".to_string()));
    assert_eq!(history.current(), Some("SecondNetwork".to_string()));
    assert!(history.can_navigate_back()); // Now we can go back to FirstNetwork

    // Verify we go back to FirstNetwork, not None
    history.navigate_back();
    assert_eq!(history.current(), Some("FirstNetwork".to_string()));
    assert!(!history.can_navigate_back()); // At the beginning of history
}
