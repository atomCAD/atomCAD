//! Validation rules for user-provided node-network names and per-node custom names.
//!
//! See `doc/design_relaxed_node_names.md` for the design rationale. The rule
//! is "blacklist instead of whitelist": almost any character is allowed except
//! a few that would cause round-trip or display problems.

use std::fmt;

/// Reasons a user-provided name can be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidNameReason {
    /// The name was empty.
    Empty,
    /// The name contained a backtick character (reserved as the text-format
    /// quoting delimiter and the rename auto-update marker in prose).
    ContainsBacktick,
    /// The name contained a control character (e.g. `\n`, `\r`, `\t`).
    ContainsControl,
    /// The name had leading or trailing whitespace.
    EdgeWhitespace,
}

impl fmt::Display for InvalidNameReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidNameReason::Empty => f.write_str("name cannot be empty"),
            InvalidNameReason::ContainsBacktick => {
                f.write_str("name cannot contain backticks (reserved as quoting delimiter)")
            }
            InvalidNameReason::ContainsControl => {
                f.write_str("name cannot contain control characters")
            }
            InvalidNameReason::EdgeWhitespace => {
                f.write_str("name cannot start or end with whitespace")
            }
        }
    }
}

/// Validates a user-provided network or per-node name.
pub fn is_valid_user_name(s: &str) -> Result<(), InvalidNameReason> {
    if s.is_empty() {
        return Err(InvalidNameReason::Empty);
    }
    for c in s.chars() {
        if c == '`' {
            return Err(InvalidNameReason::ContainsBacktick);
        }
        if c.is_control() {
            return Err(InvalidNameReason::ContainsControl);
        }
    }
    if s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) {
        return Err(InvalidNameReason::EdgeWhitespace);
    }
    Ok(())
}
