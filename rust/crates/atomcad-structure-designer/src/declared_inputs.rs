//! Nodes whose input list is their own data: the [`DeclaredInputs`] capability
//! and the name an open-port drop gives a new input.
//!
//! A node implements [`DeclaredInputs`] and hands it out through
//! [`NodeData::declared_inputs`](crate::node_data::NodeData::declared_inputs);
//! that is all it takes to get the open port (dropping a wire appends an input
//! of the wire's type) and *Take type from wire* (retyping a stale input). The
//! orchestrators that use it live in `declared_input_ops.rs` and never name a
//! node type. Design: `doc/design_wire_declared_inputs.md`.

use crate::data_type::DataType;
use std::collections::HashSet;

/// A node whose input pins are declared by its own data, one pin per
/// declared input: `expr`'s parameters, an anonymous `record_construct`'s
/// fields. Declared inputs are addressed by **pin id**, the id the node puts
/// on the pin's `Parameter` when it builds its node type
/// (`calculate_custom_node_type` for `expr`). Ids survive renames, reorders
/// and the re-derivation of the node type; indices do not.
///
/// Every mutating method edits the node's data only. The caller re-derives
/// the node type (`Node::set_custom_node_type`), revalidates and records
/// undo; see `declared_input_ops.rs`.
pub trait DeclaredInputs {
    /// Type gate shared by the open port (append) and Take type from wire
    /// (retype). `Err` carries the user-facing reason. It is shown beside a
    /// refused `+` tab, and the retype offer is withheld.
    fn accepts_type(&self, data_type: &DataType) -> Result<(), String>;

    /// Names a derived name must avoid beyond the current input names:
    /// keywords and property keys.
    fn reserved_names(&self) -> &'static [&'static str];

    /// The name to use when the source offers none: `"x"` for `expr`,
    /// `"field"` for `record_construct`.
    fn fallback_name(&self) -> &'static str;

    /// Whether `pin_id` names one of the declared inputs. It is a method rather
    /// than an invariant so that a future node can mix fixed pins with
    /// declared ones.
    fn declares_pin(&self, pin_id: u64) -> bool;

    /// Append a declared input. `name` is already derived, sanitized and
    /// deduplicated. Returns the new input's pin id. Errors when
    /// `accepts_type` refuses `data_type`.
    fn append(&mut self, name: String, data_type: DataType) -> Result<u64, String>;

    /// Retype the declared input `pin_id` in place. The name, the id and the
    /// position are kept. Errors when the pin is not declared or
    /// `accepts_type` refuses `data_type`.
    fn retype(&mut self, pin_id: u64, data_type: DataType) -> Result<(), String>;
}

/// What the source of a dropped wire offers as a name for the new input.
pub struct NameSource<'a> {
    /// The source node's `custom_name` (for a zone input, the HOF's).
    pub node_name: Option<&'a str>,
    /// The source output pin's name, or the zone-input pin's name.
    pub pin_name: Option<&'a str>,
    /// Set when the source node has more than one output pin, or the source is
    /// a zone input: the pin name says more than the node name then. Never set
    /// for a function pin (`-1`), which stands for the whole node.
    pub prefer_pin: bool,
}

/// The name an open-port drop gives the new input
/// (`doc/design_wire_declared_inputs.md`, "Name derivation"):
///
/// 1. **Base.** With `prefer_pin`, the pin name, or `<node>_<pin>` if the pin
///    name is taken; otherwise the node name; `fallback` when neither exists.
/// 2. **Sanitize** to `[A-Za-z_][A-Za-z0-9_]*`: other characters become `_`,
///    runs of `_` collapse, leading/trailing `_` are trimmed, a leading digit
///    gets a `_` prefix, and an empty result becomes `fallback`.
/// 3. **Reserved.** A name in `reserved` gets `_` appended.
/// 4. **Deduplicate** against `taken`: `name`, `name_2`, `name_3`, … — the
///    suffix scheme of `NodeNetwork::unique_name_for`.
///
/// The case of the name is left as it is.
pub fn derive_open_port_name(
    src: &NameSource,
    taken: &HashSet<String>,
    reserved: &[&str],
    fallback: &str,
) -> String {
    let node = src.node_name.map(|n| sanitize_name(n, fallback));
    let pin = src.pin_name.map(|p| sanitize_name(p, fallback));

    let base = match (src.prefer_pin, pin, node) {
        (true, Some(pin), Some(node)) if taken.contains(&pin) => {
            sanitize_name(&format!("{}_{}", node, pin), fallback)
        }
        (true, Some(pin), _) => pin,
        (_, _, Some(node)) => node,
        (false, Some(pin), None) => pin,
        _ => fallback.to_string(),
    };

    let base = if reserved.contains(&base.as_str()) {
        format!("{}_", base)
    } else {
        base
    };

    if !taken.contains(&base) {
        return base;
    }
    (2u64..)
        .map(|counter| format!("{}_{}", base, counter))
        .find(|candidate| !taken.contains(candidate))
        .expect("a free suffixed name exists: the taken set is finite")
}

/// Step 2 of [`derive_open_port_name`]: map `raw` onto the identifier rule.
fn sanitize_name(raw: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let ch = if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        };
        if ch == '_' && out.ends_with('_') {
            continue;
        }
        out.push(ch);
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", trimmed)
    } else {
        trimmed.to_string()
    }
}
