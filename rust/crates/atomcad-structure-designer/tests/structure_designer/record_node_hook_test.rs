//! Record nodes are reached through the `NodeData` record-node hook
//! (`record_schema_ref`, `build_record_node_type`, …), never through a list of
//! them by name (`doc/design_record_get_set.md` Phase 0). A site that
//! downcasts to one record node's data is how such a list starts again, and a
//! record node it leaves out is silently mistreated (`recorded_layout` used to
//! give any record node it did not list `product`'s layout). This test scans
//! the crate's sources for those downcasts.

use std::path::{Path, PathBuf};

const RECORD_NODE_DATA: [&str; 3] = [
    "RecordConstructData",
    "RecordDestructureData",
    "ProductData",
];

/// The files that define the types; a downcast there is the node's own.
const OWN_FILES: [&str; 3] = [
    "nodes/record_construct.rs",
    "nodes/record_destructure.rs",
    "nodes/product.rs",
];

/// The one allowed exception: it edits `record_construct`'s name-keyed
/// literals on a field rename and is not a record-node list.
const ALLOWED_FN: (&str, &str) = ("node_type_registry.rs", "fn rekey_record_field_literals");

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The line range (0-based, inclusive) of the body of the top-level-or-method
/// fn whose signature line contains `signature`: from that line to the first
/// later line that closes it at the signature's indentation.
fn fn_line_range(lines: &[&str], signature: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.contains(signature))?;
    let indent = lines[start].len() - lines[start].trim_start().len();
    let close = format!("{}}}", " ".repeat(indent));
    let end = (start + 1..lines.len()).find(|&i| lines[i].trim_end() == close)?;
    Some((start, end))
}

/// A line downcasting to one of the record nodes' data types:
/// `downcast_ref::<…RecordConstructData>` / `downcast_mut::<…>` (with or
/// without a path prefix).
fn downcast_target(line: &str) -> Option<&'static str> {
    for op in ["downcast_ref::<", "downcast_mut::<"] {
        let mut rest = line;
        while let Some(i) = rest.find(op) {
            let after = &rest[i + op.len()..];
            let ty = after.split('>').next().unwrap_or("");
            let last = ty.rsplit("::").next().unwrap_or("").trim();
            if let Some(name) = RECORD_NODE_DATA.iter().find(|n| **n == last) {
                return Some(name);
            }
            rest = after;
        }
    }
    None
}

#[test]
fn no_site_downcasts_to_a_record_node_outside_its_own_file() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() > 50, "found only {} source files", files.len());

    let mut offences = Vec::new();
    let mut allowed_hits = 0;
    for file in &files {
        let rel = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if OWN_FILES.contains(&rel.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let allowed = if rel == ALLOWED_FN.0 {
            Some(fn_line_range(&lines, ALLOWED_FN.1).expect("the allowed fn exists"))
        } else {
            None
        };
        for (i, line) in lines.iter().enumerate() {
            let Some(target) = downcast_target(line) else {
                continue;
            };
            if allowed.is_some_and(|(a, b)| a <= i && i <= b) {
                allowed_hits += 1;
                continue;
            }
            offences.push(format!("{}:{}: downcast to {}", rel, i + 1, target));
        }
    }
    assert!(
        offences.is_empty(),
        "record nodes must be reached through the NodeData record-node hook, \
         not by downcast:\n{}",
        offences.join("\n")
    );
    // The scan sees the exception, so it would see an offence too.
    assert_eq!(
        allowed_hits, 1,
        "the scan no longer finds the allowed downcast"
    );
}

#[test]
fn the_scan_recognises_a_downcast() {
    assert_eq!(
        downcast_target("if let Some(d) = data.downcast_ref::<ProductData>() {"),
        Some("ProductData")
    );
    assert_eq!(
        downcast_target(
            ".downcast_mut::<crate::nodes::record_destructure::RecordDestructureData>()"
        ),
        Some("RecordDestructureData")
    );
    assert_eq!(downcast_target("data.downcast_ref::<SwitchData>()"), None);
    assert_eq!(
        downcast_target("// Record* / ProductData store schema names"),
        None
    );
}
