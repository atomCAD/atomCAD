//! Export of the AI edit log — Phase 4 of `doc/design_ai_edit_history.md`.
//!
//! The log is memory-only (D10), so **export is how a session leaves the
//! process**. That is the feature rather than an afterthought: every downstream
//! use of this log — refining the `atomcad` skill, refining the CLI and the
//! text format, comparing one model against another on the same task — happens
//! outside the application, in a conversation or a diff tool.
//!
//! Two forms, because those two uses want different things:
//!
//! - [`export_json`] is the **canonical** form. Every field of every record,
//!   including both snapshots verbatim, so an export is machine-comparable
//!   across sessions and models and nothing is summarised away at write time.
//!   A summary written now cannot answer a question thought of later (D8).
//! - [`export_markdown`] is the **readable** form, for pasting a session into a
//!   skill-refinement conversation where JSON would be noise. It carries the
//!   verdicts, the counts, the layout numbers, the errors and warnings and the
//!   submitted script — but *not* the snapshots, which are what make the JSON
//!   large and a chat message unreadable.
//!
//! Both live here, in the domain crate, rather than in the FFI module that
//! calls them: the log is domain state, and formatting that is only reachable
//! through the global `CAD_INSTANCE` cannot be tested.

use crate::ai_edit_log::{AiEditLog, AiEditRecord, LayoutPath};

/// The `format` marker written into every JSON export, so a reader can tell
/// one of these files from any other JSON in the same folder.
pub const EXPORT_FORMAT_ID: &str = "atomcad-ai-edit-history";

/// The `version` written into every JSON export. Bump when the shape of an
/// entry changes incompatibly.
pub const EXPORT_FORMAT_VERSION: u32 = 1;

/// The whole log as pretty-printed JSON — the canonical export form.
///
/// Includes both text-format snapshots per entry verbatim, which is what makes
/// an export replayable and diffable outside the application.
pub fn export_json(log: &AiEditLog) -> String {
    let entries: Vec<serde_json::Value> = log.records().map(record_to_json).collect();
    let document = serde_json::json!({
        "format": EXPORT_FORMAT_ID,
        "version": EXPORT_FORMAT_VERSION,
        "session_label": log.session_label(),
        "entry_count": entries.len(),
        "entries": entries,
    });
    // `serde_json::json!` over owned strings and numbers cannot fail to
    // serialize; the fallback is here so an export never panics the UI thread.
    serde_json::to_string_pretty(&document)
        .unwrap_or_else(|error| format!("{{\"error\": \"{}\"}}", error))
}

fn record_to_json(record: &AiEditRecord) -> serde_json::Value {
    serde_json::json!({
        "seq": record.seq,
        "timestamp_ms": record.timestamp_ms,
        "network_name": record.network_name,
        "replace": record.replace,
        "code": record.code,
        "applied": record.applied,
        "success": record.success,
        "nodes_created": record.nodes_created,
        "nodes_updated": record.nodes_updated,
        "nodes_deleted": record.nodes_deleted,
        "connections_made": record.connections_made,
        "description_set": record.description_set,
        "summary_set": record.summary_set,
        "output_set": record.output_set,
        "errors": record.errors,
        "warnings": record.warnings,
        "before_text": record.before_text,
        "after_text": record.after_text,
        "before_complete": record.before_complete,
        "after_complete": record.after_complete,
        "diverged": record.diverged,
        "diverged_by_undo": record.diverged_by_undo,
        "layout": {
            "path": layout_path_key(record.layout.path),
            "node_count": record.layout.node_count,
            "max_displacement": record.layout.max_displacement,
            "moved": record.layout.moved.iter().map(|moved| serde_json::json!({
                "path": moved.path_string(),
                "before": [moved.before.x, moved.before.y],
                "after": [moved.after.x, moved.after.y],
                "displacement": moved.displacement(),
            })).collect::<Vec<_>>(),
            "delta": record.layout.delta.map(|delta| serde_json::json!({
                "nodes_added": delta.nodes_added,
                "nodes_modified": delta.nodes_modified,
                "nodes_removed": delta.nodes_removed,
                "wires_added": delta.wires_added,
                "wires_removed": delta.wires_removed,
            })),
        },
    })
}

/// The stable JSON spelling of a [`LayoutPath`]. Snake-case rather than
/// `Debug`, so a reader outside Rust is not parsing a Rust type name.
fn layout_path_key(path: LayoutPath) -> &'static str {
    match path {
        LayoutPath::None => "none",
        LayoutPath::FullReflow => "full_reflow",
        LayoutPath::Incremental => "incremental",
    }
}

/// The whole log as Markdown — the readable export form.
///
/// Deliberately omits the snapshots: this form exists to be pasted into a
/// conversation, and two full network serializations per entry would bury the
/// thing being discussed. The JSON export is the one that keeps everything.
pub fn export_markdown(log: &AiEditLog) -> String {
    let mut out = String::from("# AI edit history\n\n");
    if !log.session_label().is_empty() {
        out.push_str(&format!("**Session:** {}\n\n", log.session_label()));
    }
    out.push_str(&format!("**Entries:** {}\n", log.len()));
    for record in log.records() {
        out.push_str(&record_to_markdown(record));
    }
    out
}

fn record_to_markdown(record: &AiEditRecord) -> String {
    let mut out = format!(
        "\n---\n\n## #{} — {}{}\n\n",
        record.seq,
        if record.network_name.is_empty() {
            "(no active network)"
        } else {
            &record.network_name
        },
        if record.replace { " (replace)" } else { "" }
    );
    // Both verdicts, always, and named rather than glyphed: "applied but does
    // not validate" is the ordinary two-step of creating a higher-order node
    // and then filling its body, and a single flag would report that clean
    // edit as a failure (D8).
    out.push_str(&format!(
        "- applied: {} | validates: {}\n",
        record.applied, record.success
    ));
    out.push_str(&format!(
        "- created {} | updated {} | deleted {} | connections {}\n",
        record.nodes_created.len(),
        record.nodes_updated.len(),
        record.nodes_deleted.len(),
        record.connections_made.len()
    ));
    if record.diverged {
        out.push_str(if record.diverged_by_undo {
            "- diverged: an AI edit was undone before this one\n"
        } else {
            "- diverged: the network changed outside `edit` before this one\n"
        });
    }
    if !record.before_complete || !record.after_complete {
        out.push_str("- **incomplete snapshot** (the serializer aborted on a wire cycle)\n");
    }
    out.push_str(&format!(
        "- layout: {}, {} of {} nodes moved, max displacement {:.1}\n",
        layout_path_key(record.layout.path),
        record.layout.moved.len(),
        record.layout.node_count,
        record.layout.max_displacement
    ));
    for error in &record.errors {
        out.push_str(&format!("- error: {}\n", error));
    }
    for warning in &record.warnings {
        out.push_str(&format!("- warning: {}\n", warning));
    }
    out.push_str(&format!("\n### Submitted\n\n```\n{}\n```\n", record.code));
    out
}
