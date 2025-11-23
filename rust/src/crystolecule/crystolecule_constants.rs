use lazy_static::lazy_static;
use crate::crystolecule::motif::Motif;
use crate::crystolecule::motif_parser::parse_motif;

pub const DIAMOND_UNIT_CELL_SIZE_ANGSTROM: f64 = 3.567;  // Size of one complete unit cell in Ångströms

// Zincblende motif site indexes (basis indexes)
// These correspond to the order of SITE declarations in DEFAULT_ZINCBLENDE_MOTIF
pub const ZINCBLENDE_SITE_CORNER: usize = 0;
pub const ZINCBLENDE_SITE_FACE_Z: usize = 1;
pub const ZINCBLENDE_SITE_FACE_Y: usize = 2;
pub const ZINCBLENDE_SITE_FACE_X: usize = 3;
pub const ZINCBLENDE_SITE_INTERIOR1: usize = 4;
pub const ZINCBLENDE_SITE_INTERIOR2: usize = 5;
pub const ZINCBLENDE_SITE_INTERIOR3: usize = 6;
pub const ZINCBLENDE_SITE_INTERIOR4: usize = 7;

lazy_static! {
    /// Default cubic zincblende motif used when no motif is connected to atom_fill node
    pub static ref DEFAULT_ZINCBLENDE_MOTIF: Motif = {

        let motif_text = r#"
# cubic zincblende motif

PARAM PRIMARY C
PARAM SECONDARY C

SITE CORNER PRIMARY 0 0 0

SITE FACE_Z PRIMARY 0.5 0.5 0
SITE FACE_Y PRIMARY 0.5 0 0.5
SITE FACE_X PRIMARY 0 0.5 0.5

SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
SITE INTERIOR2 SECONDARY 0.25 0.75 0.75
SITE INTERIOR3 SECONDARY 0.75 0.25 0.75
SITE INTERIOR4 SECONDARY 0.75 0.75 0.25

BOND INTERIOR1 ...CORNER
BOND INTERIOR1 ...FACE_Z
BOND INTERIOR1 ...FACE_Y
BOND INTERIOR1 ...FACE_X

BOND INTERIOR2 .++CORNER
BOND INTERIOR2 ..+FACE_Z
BOND INTERIOR2 .+.FACE_Y
BOND INTERIOR2 ...FACE_X

BOND INTERIOR3 +.+CORNER
BOND INTERIOR3 ..+FACE_Z
BOND INTERIOR3 ...FACE_Y
BOND INTERIOR3 +..FACE_X

BOND INTERIOR4 ++.CORNER
BOND INTERIOR4 ...FACE_Z
BOND INTERIOR4 .+.FACE_Y
BOND INTERIOR4 +..FACE_X

"#;
        
        parse_motif(motif_text).expect("Failed to parse default zincblende motif")
    };
}
