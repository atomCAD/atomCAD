//! Save As dependency copy, project bundle, and linking a library that has
//! no relative path (`doc/design_library_linking.md` D6, D11; Phase 5).
//!
//! # The rule
//!
//! A file and everything it depends on form a **fixed relative layout**:
//! every linked library (direct and nested) and every data file its nodes
//! read, host and libraries alike. Paths are never rewritten, so whatever
//! moves the file must move that layout with it. For each dependency the
//! target is computed from **absolute** paths — its path relative to the
//! file's folder, joined onto the new folder — so a library in `../libs/`
//! whose data file is `tip.xyz` lands at `<new>/../libs/tip.xyz`, where the
//! library's own relative path expects it.
//!
//! # Never overwrite what is not in the plan
//!
//! Save As is where a bug would overwrite a file of the user's that has
//! nothing to do with linking. So [`plan_move`] refuses outright — before
//! anything is written — when a copy would land on the new or the original
//! design file, on the source of another dependency, on a folder or through a
//! link, or when two copies would land on one path; and the copy step
//! recomputes every status instead of trusting the dialog, overwriting a
//! *conflict* only when the caller names that exact target (a file that
//! appeared after the dialog was shown is kept). Dependencies are copied
//! first and the design last; a failed copy leaves the design unwritten and
//! the copies already made in place (they are copies — removing them could
//! delete a file that was there before).
//!
//! All disk access goes through [`LinkFs`], and every write is a temp file
//! renamed over the target ([`library_links::write_atomic`]).

use crate::library_links::{self, EntryKind, LinkFs, MountStatus, lexical_join, relative_path};
use crate::library_refresh::{base_dir_of, resolve_data_path};
use crate::node_network::walk_all_nodes;
use crate::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::StructureDesigner;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

// ---------------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyKind {
    Library,
    DataFile,
}

/// A file the design depends on, before any target is computed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencySource {
    pub kind: DependencyKind,
    /// Canonical when the file exists; the lexical resolution otherwise.
    pub path: PathBuf,
    /// Stored as an absolute path: external, never copied (D8).
    pub external: bool,
}

/// Every file `registry` depends on — its linked libraries (direct and
/// nested; a cycle is not a dependency of its own) and every data file its
/// networks and its libraries' networks read — each listed once, plus whether
/// any file-reading node takes its path through a wire (such a path is only
/// known at evaluation time and cannot be collected).
pub fn collect_dependency_sources(registry: &NodeTypeRegistry) -> (Vec<DependencySource>, bool) {
    let fs = registry.library_links.fs();
    let mut out: Vec<DependencySource> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut push = |kind, path: PathBuf, external| {
        let path = fs.canonicalize(&path).unwrap_or(path);
        if seen.insert(path_key(&path)) {
            out.push(DependencySource {
                kind,
                path,
                external,
            });
        }
    };
    for mount in registry.library_links.iter() {
        if mount.status == MountStatus::Cycle || mount.abs_path.as_os_str().is_empty() {
            continue;
        }
        push(DependencyKind::Library, mount.abs_path.clone(), false);
    }
    let mut has_wired_paths = false;
    let mut names: Vec<&String> = registry.node_networks.keys().collect();
    names.sort();
    for name in names {
        let network = &registry.node_networks[name];
        let base = base_dir_of(registry, name);
        walk_all_nodes(network, &mut |node| {
            for stored in node.data.file_paths() {
                let external = Path::new(&stored).is_absolute();
                if let Some(path) = resolve_data_path(base.as_deref(), &stored) {
                    push(DependencyKind::DataFile, path, external);
                }
            }
            let pins = node.data.file_path_pins();
            if !pins.is_empty()
                && let Some(node_type) = registry.get_node_type_for_node(node)
            {
                let wired = node_type
                    .parameters
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| pins.contains(&p.name.as_str()))
                    .any(|(i, _)| {
                        node.arguments
                            .get(i)
                            .is_some_and(|a| !a.incoming_wires.is_empty())
                    });
                has_wired_paths |= wired;
            }
        });
    }
    (out, has_wired_paths)
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// Where a dependency's target lies relative to the folder the user picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyGroup {
    /// Inside the destination folder.
    Inside,
    /// Reached through `..`: the copy lands outside the folder the user
    /// picked, so the dialog shows its full target path.
    Outside,
    /// An absolute path (or one with no relative path from the design's
    /// folder): never copied.
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyStatus {
    /// Nothing at the target yet.
    WillCopy,
    /// The target is the source itself, or has identical content: skipped.
    AlreadyThere,
    /// A different file is at the target.
    Conflict,
    /// The source does not exist (a missing library or data file): nothing
    /// to copy; the moved design will miss it as the current one does.
    SourceMissing,
    /// Not copied (group `External`).
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub source: PathBuf,
    /// Relative to the moved file's current folder (forward slashes, may
    /// start with `..`); `None` for an external one.
    pub rel_path: Option<String>,
    /// Where the copy goes; `None` for an external one.
    pub target: Option<PathBuf>,
    pub group: DependencyGroup,
    pub status: DependencyStatus,
}

/// What moving a file to `new_file` means for its dependencies (D11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyPlan {
    pub new_file: PathBuf,
    pub entries: Vec<Dependency>,
    /// A file-reading node takes its path through a wire (D8).
    pub has_wired_paths: bool,
}

impl DependencyPlan {
    /// True when something would be copied or conflicts — only then does
    /// Save As show the dialog (D11: moving inside a workspace costs nothing).
    pub fn needs_confirmation(&self) -> bool {
        self.entries.iter().any(|e| {
            matches!(
                e.status,
                DependencyStatus::WillCopy | DependencyStatus::Conflict
            )
        })
    }

    pub fn with_status(&self, status: DependencyStatus) -> impl Iterator<Item = &Dependency> {
        self.entries.iter().filter(move |e| e.status == status)
    }
}

/// A comparable form of a path: separators unified, the Windows verbatim
/// prefix dropped, case folded where the filesystem folds it.
pub fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").map(str::to_string).unwrap_or(s);
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// `path` canonicalized as far as it exists: the deepest existing ancestor is
/// canonicalized and the rest is joined on lexically. A path that does not
/// exist yet (the Save As target, its folder) still compares with canonical
/// ones.
pub fn canonical_or_lexical(fs: &dyn LinkFs, path: &Path) -> PathBuf {
    if let Ok(p) = fs.canonicalize(path) {
        return p;
    }
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        let Some(name) = cur.file_name().map(|n| n.to_os_string()) else {
            return path.to_path_buf();
        };
        rest.push(name);
        let Some(parent) = cur.parent().map(Path::to_path_buf) else {
            return path.to_path_buf();
        };
        if let Ok(mut base) = fs.canonicalize(&parent) {
            for part in rest.iter().rev() {
                base.push(part);
            }
            return base;
        }
        cur = parent;
    }
}

/// The status of copying `source` to `target`. Errors on a target that must
/// not be written: a folder, or a link that points anywhere but the source.
fn target_status(
    fs: &dyn LinkFs,
    source: &Path,
    target: &Path,
) -> Result<DependencyStatus, String> {
    if fs.stat(source).is_err() {
        return Ok(DependencyStatus::SourceMissing);
    }
    let kind = match fs.entry_kind(target) {
        Ok(kind) => kind,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(DependencyStatus::WillCopy),
        Err(e) => return Err(format!("cannot inspect '{}': {}", target.display(), e)),
    };
    let same_file = fs
        .canonicalize(target)
        .is_ok_and(|t| path_key(&t) == path_key(source));
    match kind {
        EntryKind::Dir => Err(format!(
            "'{}' is a folder; the copy of '{}' cannot go there",
            target.display(),
            source.display()
        )),
        EntryKind::Symlink if !same_file => Err(format!(
            "'{}' is a link to another place; the copy of '{}' will not be written through it",
            target.display(),
            source.display()
        )),
        _ if same_file => Ok(DependencyStatus::AlreadyThere),
        _ => {
            let a = fs
                .read(source)
                .map_err(|e| format!("cannot read '{}': {}", source.display(), e))?;
            let b = fs
                .read(target)
                .map_err(|e| format!("cannot read '{}': {}", target.display(), e))?;
            Ok(if a == b {
                DependencyStatus::AlreadyThere
            } else {
                DependencyStatus::Conflict
            })
        }
    }
}

/// Plans moving `current_file` (whose dependencies are `sources`) to
/// `new_file`: each relative dependency's target, group and status (D11). An
/// error means the move is refused outright; nothing has been written.
pub fn plan_move(
    fs: &dyn LinkFs,
    sources: &[DependencySource],
    has_wired_paths: bool,
    current_file: Option<&Path>,
    new_file: &Path,
) -> Result<DependencyPlan, String> {
    let new_file = canonical_or_lexical(fs, new_file);
    let new_dir = new_file.parent().unwrap_or(Path::new("")).to_path_buf();
    let current_file = current_file.map(|f| canonical_or_lexical(fs, f));
    let current_dir = current_file
        .as_ref()
        .map(|f| f.parent().unwrap_or(Path::new("")).to_path_buf());

    let mut entries = Vec::new();
    for source in sources {
        let rel = match (&current_dir, source.external) {
            (Some(dir), false) => relative_path(dir, &source.path),
            _ => None,
        };
        let Some(rel) = rel else {
            entries.push(Dependency {
                kind: source.kind,
                source: source.path.clone(),
                rel_path: None,
                target: None,
                group: DependencyGroup::External,
                status: DependencyStatus::External,
            });
            continue;
        };
        let target = lexical_join(&new_dir, &rel).map_err(|_| {
            format!(
                "'{}' would be copied above the filesystem root ('{}' from the new folder)",
                source.path.display(),
                rel
            )
        })?;
        let group = if rel == ".." || rel.starts_with("../") {
            DependencyGroup::Outside
        } else {
            DependencyGroup::Inside
        };
        let status = target_status(fs, &source.path, &target)?;
        entries.push(Dependency {
            kind: source.kind,
            source: source.path.clone(),
            rel_path: Some(rel),
            target: Some(target),
            group,
            status,
        });
    }

    // Refusals: never overwrite what is not in the plan.
    let new_key = path_key(&new_file);
    let current_key = current_file.as_ref().map(|f| path_key(f));
    let source_keys: BTreeMap<String, &Path> = entries
        .iter()
        .map(|e| (path_key(&e.source), e.source.as_path()))
        .collect();
    if let Some(source) = source_keys.get(&new_key) {
        return Err(format!(
            "saving to '{}' would overwrite '{}', which the design depends on",
            new_file.display(),
            source.display()
        ));
    }
    let mut targets: BTreeMap<String, &Path> = BTreeMap::new();
    for e in &entries {
        let Some(target) = &e.target else { continue };
        let key = path_key(target);
        if key == new_key {
            return Err(format!(
                "'{}' would be copied onto the design file itself ('{}')",
                e.source.display(),
                target.display()
            ));
        }
        if !matches!(
            e.status,
            DependencyStatus::WillCopy | DependencyStatus::Conflict
        ) {
            continue;
        }
        if current_key.as_ref() == Some(&key) {
            return Err(format!(
                "'{}' would be copied onto the design's current file ('{}')",
                e.source.display(),
                target.display()
            ));
        }
        if let Some(other) = source_keys.get(&key) {
            return Err(format!(
                "'{}' would be copied onto '{}', which the design also depends on",
                e.source.display(),
                other.display()
            ));
        }
        if let Some(other) = targets.insert(key, &e.source) {
            return Err(format!(
                "'{}' and '{}' would both be copied to '{}'",
                other.display(),
                e.source.display(),
                target.display()
            ));
        }
    }

    Ok(DependencyPlan {
        new_file,
        entries,
        has_wired_paths,
    })
}

// ---------------------------------------------------------------------------
// Copying
// ---------------------------------------------------------------------------

/// Copies `source` to `target` (folders created, temp file + rename).
fn copy_file(fs: &dyn LinkFs, source: &Path, target: &Path) -> io::Result<()> {
    let bytes = fs.read(source)?;
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        fs.create_dir_all(parent)?;
    }
    library_links::write_atomic(fs, target, &bytes)
}

/// What a copy step did, and where it stopped if it failed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyOutcome {
    /// Targets written.
    pub copied: Vec<PathBuf>,
    /// Conflicting targets left as they were (*keep existing*).
    pub kept: Vec<PathBuf>,
    /// Dependencies not at their target after the move: missing sources, and
    /// everything not copied by *Save without dependencies*.
    pub missing: Vec<PathBuf>,
    /// External files (never copied).
    pub external: Vec<PathBuf>,
    /// Set when a copy failed; the design was then not written.
    pub error: Option<String>,
}

impl CopyOutcome {
    /// The message for a failed copy step: the file that failed, what was
    /// therefore not done (`consequence`), and the copies already made (left
    /// in place, D11).
    pub fn failure_message(&self, consequence: &str) -> Option<String> {
        let error = self.error.as_ref()?;
        let mut msg = format!("{}. {}", error, consequence);
        if !self.copied.is_empty() {
            msg.push_str(" Copies already made (left in place): ");
            msg.push_str(
                &self
                    .copied
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        Some(msg)
    }
}

/// Copies the plan's *will copy* entries, and the *conflict* entries whose
/// target is in `overwrite` (by [`path_key`]); stops at the first failure.
fn copy_planned(
    fs: &dyn LinkFs,
    plan: &DependencyPlan,
    overwrite: &BTreeSet<String>,
) -> CopyOutcome {
    let mut out = CopyOutcome::default();
    for e in &plan.entries {
        match (e.status, &e.target) {
            (DependencyStatus::External, _) => out.external.push(e.source.clone()),
            (DependencyStatus::SourceMissing, _) => out.missing.push(e.source.clone()),
            (DependencyStatus::Conflict, Some(target))
                if !overwrite.contains(&path_key(target)) =>
            {
                out.kept.push(target.clone())
            }
            (DependencyStatus::WillCopy | DependencyStatus::Conflict, Some(target)) => {
                if let Err(err) = copy_file(fs, &e.source, target) {
                    out.error = Some(format!(
                        "cannot copy '{}' to '{}': {}",
                        e.source.display(),
                        target.display(),
                        err
                    ));
                    return out;
                }
                out.copied.push(target.clone());
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The project bundle (.zip)
// ---------------------------------------------------------------------------

/// The deepest folder containing every path of `files` (canonical paths).
fn common_folder(files: &[PathBuf]) -> Option<PathBuf> {
    let mut common: Option<Vec<Component>> = None;
    for file in files {
        let dir: Vec<Component> = file.parent()?.components().collect();
        common = Some(match common {
            None => dir,
            Some(c) => c
                .iter()
                .zip(dir.iter())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    let common = common?;
    if !common
        .iter()
        .any(|c| matches!(c, Component::RootDir | Component::Prefix(_)))
    {
        return None;
    }
    let mut out = PathBuf::new();
    for c in common {
        out.push(c.as_os_str());
    }
    Some(out)
}

/// MS-DOS time and date of now (UTC), for the zip entries.
fn dos_now() -> (u16, u16) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    let year = year.clamp(1980, 2107);
    let time = ((rem / 3600) << 11) | (((rem % 3600) / 60) << 5) | ((rem % 60) / 2);
    let date = ((year - 1980) << 9) | (month << 5) | day;
    (time as u16, date as u16)
}

fn too_big(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{} is too large for a zip file", what),
    )
}

/// A zip archive of `entries` (`name` with forward slashes, bytes), deflated.
/// Plain ZIP 2.0 — no ZIP64, which a design bundle never needs.
pub fn zip_bytes(entries: &[(String, Vec<u8>)]) -> io::Result<Vec<u8>> {
    let (time, date) = dos_now();
    let count = u16::try_from(entries.len()).map_err(|_| too_big("the file list"))?;
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, data) in entries {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data)?;
        let compressed = encoder.finish()?;
        let size = u32::try_from(data.len()).map_err(|_| too_big(name))?;
        let csize = u32::try_from(compressed.len()).map_err(|_| too_big(name))?;
        let offset = u32::try_from(out.len()).map_err(|_| too_big("the bundle"))?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big(name))?;
        // Version 2.0, UTF-8 names (bit 11), deflate.
        let common = |buf: &mut Vec<u8>| {
            buf.extend(20u16.to_le_bytes());
            buf.extend(0x0800u16.to_le_bytes());
            buf.extend(8u16.to_le_bytes());
            buf.extend(time.to_le_bytes());
            buf.extend(date.to_le_bytes());
            buf.extend(crc.sum().to_le_bytes());
            buf.extend(csize.to_le_bytes());
            buf.extend(size.to_le_bytes());
            buf.extend(name_len.to_le_bytes());
            buf.extend(0u16.to_le_bytes()); // extra field length
        };
        out.extend(0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend(name.as_bytes());
        out.extend(&compressed);

        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes()); // version made by
        common(&mut central);
        central.extend(0u16.to_le_bytes()); // comment length
        central.extend(0u16.to_le_bytes()); // disk number
        central.extend(0u16.to_le_bytes()); // internal attributes
        central.extend(0u32.to_le_bytes()); // external attributes
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let cd_offset = u32::try_from(out.len()).map_err(|_| too_big("the bundle"))?;
    let cd_size = u32::try_from(central.len()).map_err(|_| too_big("the bundle"))?;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

/// Reads back what [`zip_bytes`] writes (stored or deflated entries, CRC
/// checked). For the round-trip tests and for inspecting a bundle.
pub fn read_zip(bytes: &[u8]) -> io::Result<Vec<(String, Vec<u8>)>> {
    let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, what.to_string());
    let u16_at = |i: usize| -> io::Result<u16> {
        bytes
            .get(i..i + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| bad("truncated zip"))
    };
    let u32_at = |i: usize| -> io::Result<u32> {
        bytes
            .get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| bad("truncated zip"))
    };
    let eocd = (0..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| u32_at(i).is_ok_and(|s| s == 0x0605_4b50))
        .ok_or_else(|| bad("no end of central directory"))?;
    let count = u16_at(eocd + 10)? as usize;
    let mut at = u32_at(eocd + 16)? as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(at)? != 0x0201_4b50 {
            return Err(bad("bad central directory entry"));
        }
        let method = u16_at(at + 10)?;
        let crc = u32_at(at + 16)?;
        let csize = u32_at(at + 20)? as usize;
        let name_len = u16_at(at + 28)? as usize;
        let extra_len = u16_at(at + 30)? as usize;
        let comment_len = u16_at(at + 32)? as usize;
        let local = u32_at(at + 42)? as usize;
        let name = bytes
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(|| bad("truncated zip"))?;
        let name = String::from_utf8_lossy(name).to_string();
        at += 46 + name_len + extra_len + comment_len;
        if u32_at(local)? != 0x0403_4b50 {
            return Err(bad("bad local header"));
        }
        let start = local + 30 + u16_at(local + 26)? as usize + u16_at(local + 28)? as usize;
        let raw = bytes
            .get(start..start + csize)
            .ok_or_else(|| bad("truncated zip"))?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut data = Vec::new();
                flate2::read::DeflateDecoder::new(raw).read_to_end(&mut data)?;
                data
            }
            _ => return Err(bad("unsupported compression")),
        };
        let mut check = flate2::Crc::new();
        check.update(&data);
        if check.sum() != crc {
            return Err(bad("CRC mismatch"));
        }
        out.push((name, data));
    }
    Ok(out)
}

/// What *Export project bundle* wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BundleOutcome {
    /// The folder the zip's paths are relative to.
    pub root: PathBuf,
    /// The zip's entries, the design first.
    pub files: Vec<String>,
    /// Absolute-path data files, not included.
    pub external: Vec<PathBuf>,
    /// Dependencies that do not exist, not included.
    pub missing: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// StructureDesigner operations
// ---------------------------------------------------------------------------

impl StructureDesigner {
    /// The canonical design file, if the design was saved.
    fn design_file_canonical(&self) -> Option<PathBuf> {
        let host = self.host_file()?;
        let fs = self.node_type_registry.library_links.fs();
        Some(canonical_or_lexical(fs.as_ref(), &host))
    }

    /// What saving the design as `new_file` means for its dependencies (D11):
    /// every library and data file with its target, group and status. An
    /// error means Save As to that path is refused (nothing would be
    /// written). No disk writes.
    pub fn collect_file_dependencies(&self, new_file: &str) -> Result<DependencyPlan, String> {
        let (sources, wired) = collect_dependency_sources(&self.node_type_registry);
        let fs = self.node_type_registry.library_links.fs();
        let current = self.host_file();
        plan_move(
            fs.as_ref(),
            &sources,
            wired,
            current.as_deref(),
            Path::new(new_file),
        )
    }

    /// *Save As* with the dependency copy of D11. With `copy`, the plan is
    /// **recomputed now** (never trusted from the dialog), every *will copy*
    /// entry is copied, and a *conflict* is overwritten only when its target
    /// is in `overwrite` — the conflicts the user saw and chose to overwrite;
    /// one that appeared since is kept. Dependencies first, the design last:
    /// if any copy fails, the design is not written and the outcome's `error`
    /// says which file, with the copies already made (left in place). Without
    /// `copy`, the design alone is written (paths verbatim), and `missing`
    /// lists what the moved design will not find.
    ///
    /// An `Err` is a refusal or a failed design write; a failed copy is an
    /// `Ok` outcome with `error` set, so the caller can list the copies.
    pub fn save_as_with_dependencies(
        &mut self,
        new_file: &str,
        copy: bool,
        overwrite: &[String],
    ) -> Result<CopyOutcome, String> {
        let plan = self.collect_file_dependencies(new_file)?;
        let fs = self.node_type_registry.library_links.fs();
        let outcome = if copy {
            let overwrite: BTreeSet<String> = overwrite
                .iter()
                .map(|p| path_key(&canonical_or_lexical(fs.as_ref(), Path::new(p))))
                .collect();
            copy_planned(fs.as_ref(), &plan, &overwrite)
        } else {
            let mut out = CopyOutcome::default();
            for e in &plan.entries {
                match e.status {
                    DependencyStatus::External => out.external.push(e.source.clone()),
                    DependencyStatus::AlreadyThere => {}
                    _ => out.missing.push(e.source.clone()),
                }
            }
            out
        };
        if outcome.error.is_some() {
            return Ok(outcome);
        }
        self.save_node_networks_as(new_file)
            .map_err(|e| format!("cannot write '{}': {}", new_file, e))?;
        Ok(outcome)
    }

    /// *File > Export project bundle…* (D11): a zip of the design — as it is
    /// in memory, unsaved edits included — and every relative dependency that
    /// exists, with their paths relative to the deepest folder containing
    /// them all. Unzipping anywhere reproduces the layout. External and
    /// missing files are left out and listed.
    pub fn export_project_bundle(&mut self, zip_path: &str) -> Result<BundleOutcome, String> {
        let host = self
            .design_file_canonical()
            .ok_or_else(|| "save the design before exporting a bundle".to_string())?;
        let fs = self.node_type_registry.library_links.fs();
        let (sources, _) = collect_dependency_sources(&self.node_type_registry);
        let mut outcome = BundleOutcome::default();
        let mut included: Vec<PathBuf> = Vec::new();
        for source in &sources {
            if source.external {
                outcome.external.push(source.path.clone());
            } else if fs.stat(&source.path).is_err() {
                outcome.missing.push(source.path.clone());
            } else {
                included.push(source.path.clone());
            }
        }
        let zip_path = canonical_or_lexical(fs.as_ref(), Path::new(zip_path));
        let zip_key = path_key(&zip_path);
        if zip_key == path_key(&host) || included.iter().any(|p| path_key(p) == zip_key) {
            return Err(format!(
                "the bundle would overwrite '{}', which it contains",
                zip_path.display()
            ));
        }
        let mut all = vec![host.clone()];
        all.extend(included.iter().cloned());
        let root = common_folder(&all)
            .ok_or_else(|| "the design and its dependencies share no folder".to_string())?;
        let name_of = |p: &Path| {
            relative_path(&root, p)
                .ok_or_else(|| format!("'{}' is not under '{}'", p.display(), root.display()))
        };

        let design_dir = self
            .node_type_registry
            .design_file_name
            .as_ref()
            .and_then(|f| {
                Path::new(f)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
            });
        let rules = self.cli_access_rules.clone();
        let text = crate::serialization::node_networks_serialization::serialize_registry_to_string(
            &mut self.node_type_registry,
            design_dir.as_deref(),
            self.direct_editing_mode,
            &rules,
        )
        .map_err(|e| format!("cannot serialize the design: {}", e))?;
        let mut entries = vec![(name_of(&host)?, text.into_bytes())];
        for path in &included {
            let bytes = fs
                .read(path)
                .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
            entries.push((name_of(path)?, bytes));
        }
        let zip = zip_bytes(&entries).map_err(|e| e.to_string())?;
        if let Some(parent) = zip_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs.create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        library_links::write_atomic(fs.as_ref(), &zip_path, &zip)
            .map_err(|e| format!("cannot write '{}': {}", zip_path.display(), e))?;
        outcome.files = entries.into_iter().map(|(name, _)| name).collect();
        outcome.root = root;
        Ok(outcome)
    }

    /// True when `path` (absolute) has no path relative to the design's
    /// folder — another drive — so it can only be linked by copying it next
    /// to the design first ([`Self::link_library_copying`], D6).
    pub fn library_needs_copy(&self, path: &str) -> bool {
        let Some(host) = self.design_file_canonical() else {
            return false;
        };
        let picked = Path::new(path);
        if !picked.is_absolute() {
            return false;
        }
        let fs = self.node_type_registry.library_links.fs();
        let picked = canonical_or_lexical(fs.as_ref(), picked);
        relative_path(host.parent().unwrap_or(Path::new("")), &picked).is_none()
    }

    /// Links a library that has no relative path (D6): copies it to
    /// `target_rel_path` (relative to the design's folder) together with its
    /// own dependencies, each landing where the library's relative paths
    /// expect it, then links the copy under `alias` (one undo step; the copies
    /// are files and stay). Refused before anything is copied when a
    /// different file already sits at any target. Returns the files copied.
    pub fn link_library_copying(
        &mut self,
        path: &str,
        target_rel_path: &str,
        alias: &str,
    ) -> Result<Vec<PathBuf>, String> {
        let host = self
            .design_file_canonical()
            .ok_or_else(|| "save the design before linking a library".to_string())?;
        self.check_new_alias(alias)?;
        let fs = self.node_type_registry.library_links.fs();
        let rel = library_links::normalize_rel_path(target_rel_path)?;
        if !rel.to_ascii_lowercase().ends_with(".cnnd") {
            return Err(format!("'{}' is not a .cnnd file", rel));
        }
        let target = lexical_join(host.parent().unwrap_or(Path::new("")), &rel)?;
        let source = fs
            .canonicalize(Path::new(path))
            .map_err(|e| format!("cannot open '{}': {}", path, e))?;

        let mut library = NodeTypeRegistry::new();
        library.library_links.set_fs(fs.clone());
        crate::serialization::node_networks_serialization::load_node_networks_from_file(
            &mut library,
            &source.to_string_lossy(),
        )
        .map_err(|e| format!("cannot load '{}': {}", path, e))?;
        let (sources, _) = collect_dependency_sources(&library);
        let plan = plan_move(fs.as_ref(), &sources, false, Some(&source), &target)?;
        let library_status = target_status(fs.as_ref(), &source, &target)?;
        let conflicts: Vec<String> = plan
            .with_status(DependencyStatus::Conflict)
            .filter_map(|e| e.target.as_ref())
            .chain((library_status == DependencyStatus::Conflict).then_some(&target))
            .map(|p| p.display().to_string())
            .collect();
        if !conflicts.is_empty() {
            return Err(format!(
                "a different file already exists at {}; nothing was copied",
                conflicts.join(", ")
            ));
        }
        let outcome = copy_planned(fs.as_ref(), &plan, &BTreeSet::new());
        let mut copied = outcome.copied.clone();
        if let Some(message) = outcome.failure_message("Nothing was linked.") {
            return Err(message);
        }
        if library_status == DependencyStatus::WillCopy {
            copy_file(fs.as_ref(), &source, &target).map_err(|e| {
                format!(
                    "cannot copy '{}' to '{}': {}",
                    source.display(),
                    target.display(),
                    e
                )
            })?;
            copied.push(target.clone());
        }
        self.link_library(&rel, alias).map_err(|e| {
            format!(
                "{} (copied files left in place: {})",
                e,
                copied
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        Ok(copied)
    }
}
