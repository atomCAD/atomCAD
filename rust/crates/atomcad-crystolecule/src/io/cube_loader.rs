//! Gaussian `.cube` volumetric data import.
//!
//! A `.cube` file is plain ASCII: two free-text comment lines, a header giving
//! the atom count and grid placement, an atom block, and then the samples in
//! x-slowest / z-fastest order.
//!
//! ```text
//!  Comment line 1                                <- free text; read past, not retained
//!  Comment line 2                                <- free text
//!    -3   -5.000000  -5.000000  -5.000000        <- natoms (SIGNED), origin xyz, [NVal]
//!     2    5.000000   0.000000   0.000000        <- N1, step vector along axis 1
//!     2    0.000000   5.000000   0.000000        <- N2, step vector along axis 2
//!     2    0.000000   0.000000   5.000000        <- N3, step vector along axis 3
//!     8   8.000000   0.000000   0.000000   0.220000    <- Z, charge, x, y, z
//!     1   1.000000   0.000000   1.430000  -0.880000
//!     1   1.000000   0.000000  -1.430000  -0.880000
//!     2    5    6                                <- multi-field ONLY: count, then indices
//!    1.0e-05  2.0e-05  3.0e-05  4.0e-05          <- values
//! ```
//!
//! Rules that bite, in the order they do (design doc `doc/design_scalar_fields.md`):
//!
//! 1. **`natoms` is signed and the sign is a flag, not a count.** Negative means
//!    the multi-field variant; the atom block is present and complete either
//!    way. Until multi-field support lands the flag is a *rejection*, not a
//!    branch — misparsing it would silently misalign every subsequent read.
//! 2. **Line 3 has either four or five numbers.** The optional fifth is `NVal`,
//!    values per grid point. This is the one place where line structure matters:
//!    read line 3 as a line and count its tokens.
//! 3. **Everything after line 3 is parsed as a token stream, not line by line.**
//!    Files in the wild vary in whitespace and wrapping more than the format's
//!    description suggests.
//! 4. **Traversal order is x-slowest, z-fastest**, which is exactly
//!    [`SampledField`]'s declared layout, so samples append sequentially with no
//!    transposition. Getting this wrong transposes or mirrors the field and is
//!    invisible in any axis-symmetric test case — hence the asymmetric ramp
//!    fixture.
//! 5. **Coordinates and step vectors are always read as Bohr.** A convention
//!    exists whereby a negative voxel count signals Ångström, but it is
//!    documented inconsistently across sources, so neither reading is relied on.
//!    The atom block is used as a *plausibility check* only, never as an
//!    override — see [`units_plausibility_warning`].

use crate::atomic_constants::{ATOM_INFO, DEFAULT_ATOM_INFO};
use crate::atomic_structure::AtomicStructure;
use crate::atomic_structure_utils::auto_create_bonds;
use crate::field::{FieldError, GridGeometry, SampledField};
use glam::DVec3;
use std::io;
use thiserror::Error;

/// CODATA 2018 Bohr radius in Ångström.
pub const BOHR_TO_ANGSTROM: f64 = 0.529_177_210_903;

/// Median nearest-neighbour / covalent-radii-sum ratio below which the atom
/// block looks implausible under the assumed Bohr units. An Ångström file read
/// as Bohr lands near `0.53`; ordinary chemistry lands near `1.0`.
const UNITS_RATIO_MIN: f64 = 0.7;

/// Upper counterpart of [`UNITS_RATIO_MIN`]. Wide enough that a van der Waals
/// contact does not trip it.
const UNITS_RATIO_MAX: f64 = 1.6;

#[derive(Debug, Error)]
pub enum CubeError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid cube format: {0}")]
    Parse(String),

    #[error("Unsupported cube file: {0}")]
    Unsupported(String),

    #[error("Invalid grid: {0}")]
    Field(#[from] FieldError),
}

/// The whole content of one `.cube` file.
pub struct CubeFile {
    pub atoms: AtomicStructure,
    /// One per field in the file. Until multi-field support lands this always
    /// holds exactly one element (rule 1). Every entry carries an identical
    /// [`GridGeometry`].
    pub fields: Vec<SampledField>,
    /// Set when the atom block's interatomic distances look chemically
    /// implausible under the assumed Bohr units. **Advisory only** — coordinates
    /// are always read as Bohr regardless.
    pub units_warning: Option<String>,
}

impl std::fmt::Debug for CubeFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CubeFile")
            .field("atoms", &self.atoms.get_num_of_atoms())
            .field("fields", &self.fields)
            .field("units_warning", &self.units_warning)
            .finish()
    }
}

/// Load a `.cube` file from disk. Mirrors [`crate::io::xyz_loader::load_xyz`]:
/// `create_bonds` runs auto-bonding over the atom block.
pub fn load_cube(file_path: &str, create_bonds: bool) -> Result<CubeFile, CubeError> {
    let text = std::fs::read_to_string(file_path)?;
    load_cube_from_str(&text, create_bonds)
}

/// Load a `.cube` file from text already in memory.
pub fn load_cube_from_str(text: &str, create_bonds: bool) -> Result<CubeFile, CubeError> {
    let mut lines = text.lines();

    // Two free-text comment lines. Read past them; nothing downstream consumes
    // them (see the design doc on why there is no semantic tag or metadata).
    lines
        .next()
        .ok_or_else(|| CubeError::Parse("missing comment line 1".to_string()))?;
    lines
        .next()
        .ok_or_else(|| CubeError::Parse("missing comment line 2".to_string()))?;

    // Rule 2: line 3 is the ONE place where line structure matters, because the
    // trailing `NVal` is optional and only the line break tells it apart from
    // the first token of the next line.
    let header_line = lines
        .next()
        .ok_or_else(|| CubeError::Parse("missing atom-count/origin line".to_string()))?;
    let header: Vec<&str> = header_line.split_whitespace().collect();
    if header.len() != 4 && header.len() != 5 {
        return Err(CubeError::Parse(format!(
            "atom-count/origin line must have 4 or 5 numbers, found {}: {:?}",
            header.len(),
            header_line.trim()
        )));
    }
    let signed_natoms = parse_int(header[0], "atom count")?;
    let origin_bohr = DVec3::new(
        parse_float(header[1], "origin x")?,
        parse_float(header[2], "origin y")?,
        parse_float(header[3], "origin z")?,
    );
    let declared_nval = match header.get(4) {
        Some(token) => Some(parse_int(token, "values per point")?),
        None => None,
    };

    // Rule 1: the sign is a flag, not a count. Reject the multi-field variant
    // rather than misparsing it — the atom loop would still be right, but the
    // value block is interleaved per grid point and would come out as garbage.
    if signed_natoms < 0 || declared_nval.is_some_and(|n| n > 1) {
        return Err(CubeError::Unsupported(
            "multi-field cube files are not yet supported".to_string(),
        ));
    }
    let num_atoms = signed_natoms.unsigned_abs() as usize;

    // Rule 3: everything from here on is a token stream.
    let mut tokens = TokenStream::new(lines);

    let mut dims = [0usize; 3];
    let mut axes = [DVec3::ZERO; 3];
    for (axis, (dim, step)) in dims.iter_mut().zip(axes.iter_mut()).enumerate() {
        let count = parse_int(
            tokens.next(&format!("grid dimension {}", axis + 1))?,
            "grid dimension",
        )?;
        if count <= 0 {
            return Err(CubeError::Parse(format!(
                "grid dimension {} must be positive, found {}",
                axis + 1,
                count
            )));
        }
        *dim = count as usize;
        let x = parse_float(tokens.next("axis vector x")?, "axis vector x")?;
        let y = parse_float(tokens.next("axis vector y")?, "axis vector y")?;
        let z = parse_float(tokens.next("axis vector z")?, "axis vector z")?;
        *step = DVec3::new(x, y, z) * BOHR_TO_ANGSTROM;
    }

    let mut atoms = AtomicStructure::new();
    for index in 0..num_atoms {
        let label = format!("atom {}", index + 1);
        let atomic_number = parse_int(tokens.next(&label)?, "atomic number")?;
        // The charge column is part of the format and carries no information we
        // model; read it so the stream stays aligned.
        let _charge = parse_float(tokens.next(&label)?, "nuclear charge")?;
        let x = parse_float(tokens.next(&label)?, "atom x")?;
        let y = parse_float(tokens.next(&label)?, "atom y")?;
        let z = parse_float(tokens.next(&label)?, "atom z")?;
        if atomic_number <= 0 {
            return Err(CubeError::Parse(format!(
                "{} has non-positive atomic number {}",
                label, atomic_number
            )));
        }
        atoms.add_atom(atomic_number as i16, DVec3::new(x, y, z) * BOHR_TO_ANGSTROM);
    }

    let grid = GridGeometry {
        origin: origin_bohr * BOHR_TO_ANGSTROM,
        axes,
        dims,
    };

    let expected = grid.sample_count();
    let mut samples = Vec::with_capacity(expected);
    // The label here is deliberately a `&'static str` rather than a formatted
    // one: an 80^3 cube has half a million samples, and building a per-sample
    // error message that is thrown away every time is half a million wasted
    // allocations. `TokenStream` already reports how many numbers it read.
    for index in 0..expected {
        let token = tokens.next("the value block")?;
        let value = parse_float(token, "sample value")?;
        if !value.is_finite() {
            return Err(CubeError::Parse(format!(
                "sample {} of {} is not finite ({})",
                index + 1,
                expected,
                token
            )));
        }
        samples.push(value as f32);
    }
    if let Some(extra) = tokens.peek() {
        return Err(CubeError::Parse(format!(
            "expected exactly {} samples for a {}x{}x{} grid, but the file continues with {:?}",
            expected, dims[0], dims[1], dims[2], extra
        )));
    }

    // Rule 4 holds for free: `samples` was appended in the file's own
    // x-slowest / z-fastest order, which is `SampledField`'s declared layout.
    let field = SampledField::new(grid, samples)?;

    let units_warning = units_plausibility_warning(&atoms);

    if create_bonds {
        auto_create_bonds(&mut atoms);
    }

    Ok(CubeFile {
        atoms,
        fields: vec![field],
        units_warning,
    })
}

/// Sanity-check the atom block against covalent radii, assuming the coordinates
/// were Bohr (which is what the loader assumed when it read them).
///
/// For each atom, take the distance to its nearest neighbour and divide by the
/// sum of the two covalent radii; a correctly-read Bohr file lands near `1.0`,
/// while an Ångström file read as Bohr is scaled by 0.529 and lands near `0.53`.
/// The **median** of those ratios is compared against a deliberately wide window.
///
/// **The check never re-interprets the file.** Short contacts are not the only
/// thing an atom block can hold — an ion pair, a van der Waals cluster, two
/// separated fragments, or one stretched bond all produce distances the
/// heuristic cannot distinguish from an Ångström file. Rescaling on that guess
/// would silently move every coordinate, the grid, and every threshold read off
/// it by a factor of 1.89: a wrong answer the user cannot see, traded against a
/// wrong answer the warning already names.
///
/// A file with fewer than two atoms has no distances to check; stay silent.
pub fn units_plausibility_warning(atoms: &AtomicStructure) -> Option<String> {
    let positions: Vec<(i16, DVec3)> = atoms
        .atoms_values()
        .map(|atom| (atom.atomic_number, atom.position))
        .collect();
    if positions.len() < 2 {
        return None;
    }

    let covalent_radius = |z: i16| {
        ATOM_INFO
            .get(&(z as i32))
            .unwrap_or(&DEFAULT_ATOM_INFO)
            .covalent_radius
    };

    let mut ratios: Vec<f64> = Vec::with_capacity(positions.len());
    for (i, (zi, pi)) in positions.iter().enumerate() {
        let mut best: Option<(f64, i16)> = None;
        for (j, (zj, pj)) in positions.iter().enumerate() {
            if i == j {
                continue;
            }
            let d = pi.distance(*pj);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, *zj));
            }
        }
        let Some((distance, zj)) = best else { continue };
        let radii_sum = covalent_radius(*zi) + covalent_radius(zj);
        if radii_sum > 0.0 {
            ratios.push(distance / radii_sum);
        }
    }
    if ratios.is_empty() {
        return None;
    }

    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = ratios.len() / 2;
    let median = if ratios.len().is_multiple_of(2) {
        (ratios[mid - 1] + ratios[mid]) * 0.5
    } else {
        ratios[mid]
    };

    if median < UNITS_RATIO_MIN {
        Some(format!(
            "Atom positions look too close together for the assumed Bohr units: the median \
             nearest-neighbour distance is {median:.2}x the sum of covalent radii (expected \
             near 1.0). If this file is written in Ångström, every coordinate and the grid \
             with them are 1.89x too small. Coordinates were read as Bohr regardless."
        ))
    } else if median > UNITS_RATIO_MAX {
        Some(format!(
            "Atom positions look too far apart for the assumed Bohr units: the median \
             nearest-neighbour distance is {median:.2}x the sum of covalent radii (expected \
             near 1.0). This is also what separated fragments or an ion pair look like. \
             Coordinates were read as Bohr regardless."
        ))
    } else {
        None
    }
}

/// Whitespace-separated tokens over the remaining lines, with the running token
/// index kept so that a truncated or malformed file can say *where* it broke.
struct TokenStream<'a, L: Iterator<Item = &'a str>> {
    lines: L,
    current: std::str::SplitWhitespace<'a>,
    pending: Option<&'a str>,
    consumed: usize,
}

impl<'a, L: Iterator<Item = &'a str>> TokenStream<'a, L> {
    fn new(lines: L) -> Self {
        Self {
            lines,
            current: "".split_whitespace(),
            pending: None,
            consumed: 0,
        }
    }

    /// The next token without consuming it.
    fn peek(&mut self) -> Option<&'a str> {
        if self.pending.is_none() {
            self.pending = loop {
                match self.current.next() {
                    Some(token) => break Some(token),
                    None => match self.lines.next() {
                        Some(line) => self.current = line.split_whitespace(),
                        None => break None,
                    },
                }
            };
        }
        self.pending
    }

    fn next(&mut self, expecting: &str) -> Result<&'a str, CubeError> {
        match self.peek() {
            Some(token) => {
                self.pending = None;
                self.consumed += 1;
                Ok(token)
            }
            None => Err(CubeError::Parse(format!(
                "file ended after {} numbers, while reading {}",
                self.consumed, expecting
            ))),
        }
    }
}

fn parse_int(token: &str, what: &str) -> Result<i64, CubeError> {
    token
        .parse::<i64>()
        // Some writers emit counts as `3.0`; accept that rather than failing on
        // a file that is otherwise perfectly well formed.
        .or_else(|_| {
            token
                .parse::<f64>()
                .ok()
                .filter(|v| v.fract() == 0.0 && v.is_finite())
                .map(|v| v as i64)
                .ok_or(())
        })
        .map_err(|_| CubeError::Parse(format!("{} is not an integer: {:?}", what, token)))
}

fn parse_float(token: &str, what: &str) -> Result<f64, CubeError> {
    token
        .parse::<f64>()
        .map_err(|_| CubeError::Parse(format!("{} is not a number: {:?}", what, token)))
}
