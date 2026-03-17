/// Manages navigation history for node networks, similar to back/forward in code IDEs
pub struct NavigationHistory {
    /// Stack of visited node network names
    history: Vec<Option<String>>,
    /// Current position in the history (index into history vector)
    current_index: usize,
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl NavigationHistory {
    pub fn new() -> Self {
        Self {
            history: vec![None], // Start with None (no network)
            current_index: 0,
        }
    }

    /// Records a navigation to a new network
    /// Truncates forward history if we're not at the end
    pub fn navigate_to(&mut self, network_name: Option<String>) {
        // Don't record if we're navigating to the same network
        if self.history.get(self.current_index) == Some(&network_name) {
            return;
        }

        // Special case: if history only contains the initial None entry, replace it
        // This prevents users from navigating back to a state they never experienced
        if self.history.len() == 1 && self.history[0].is_none() && self.current_index == 0 {
            self.history[0] = network_name;
            return;
        }

        // Truncate forward history
        self.history.truncate(self.current_index + 1);

        // Add new entry
        self.history.push(network_name);
        self.current_index += 1;
    }

    /// Navigates backward in history
    /// Returns the network name to navigate to, or None if can't go back
    pub fn navigate_back(&mut self) -> Option<Option<String>> {
        if !self.can_navigate_back() {
            return None;
        }

        self.current_index -= 1;
        Some(self.history[self.current_index].clone())
    }

    /// Navigates forward in history
    /// Returns the network name to navigate to, or None if can't go forward
    pub fn navigate_forward(&mut self) -> Option<Option<String>> {
        if !self.can_navigate_forward() {
            return None;
        }

        self.current_index += 1;
        Some(self.history[self.current_index].clone())
    }

    /// Checks if we can navigate backward
    pub fn can_navigate_back(&self) -> bool {
        self.current_index > 0
    }

    /// Checks if we can navigate forward
    pub fn can_navigate_forward(&self) -> bool {
        self.current_index < self.history.len() - 1
    }

    /// Gets the current network name
    pub fn current(&self) -> Option<String> {
        self.history.get(self.current_index).cloned().flatten()
    }

    /// Clears the navigation history and resets to initial state
    /// Used when loading a new design file
    pub fn clear(&mut self) {
        self.history = vec![None];
        self.current_index = 0;
    }

    /// Updates all occurrences of an old network name to a new name
    pub fn rename_network(&mut self, old_name: &str, new_name: &str) {
        for entry in &mut self.history {
            if let Some(name) = entry {
                if name == old_name {
                    *entry = Some(new_name.to_string());
                }
            }
        }
    }

    /// Removes all occurrences of a deleted network from history
    /// Adjusts current_index if necessary
    pub fn remove_network(&mut self, network_name: &str) {
        // Track if current position is being removed
        let current_being_removed = match &self.history[self.current_index] {
            Some(name) => name == network_name,
            None => false,
        };

        // Filter out the deleted network
        let mut new_history = Vec::new();
        let mut new_index = self.current_index;

        for (i, entry) in self.history.iter().enumerate() {
            let should_keep = match entry {
                Some(name) => name != network_name,
                None => true, // Keep None entries
            };

            if should_keep {
                new_history.push(entry.clone());
            } else {
                // This entry is being removed
                if i < self.current_index {
                    // Entry before current position - decrement index
                    new_index = new_index.saturating_sub(1);
                }
            }
        }

        // Ensure we have at least one entry (None)
        if new_history.is_empty() {
            new_history.push(None);
            new_index = 0;
        } else if current_being_removed {
            // Current position was removed, clamp index to valid range
            new_index = new_index.min(new_history.len() - 1);
        }

        self.history = new_history;
        self.current_index = new_index;
    }
}
