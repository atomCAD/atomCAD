// TODO: these will not be constant, will be set by the user
use glam::i32::IVec3;
use glam::f32::Vec3;
use glam::f64::DVec3;

use std::collections::HashMap;
use lazy_static::lazy_static;

pub const REAL_IMPLICIT_VOLUME_MIN: DVec3 = DVec3::new(-800.0, -800.0, -800.0);
pub const REAL_IMPLICIT_VOLUME_MAX: DVec3 = DVec3::new(800.0, 800.0, 800.0);

pub const MAX_EVAL_CACHE_SIZE: i32 = 1000000;

pub const DIAMOND_UNIT_CELL_SIZE_ANGSTROM: f64 = 3.567;  // Size of one complete unit cell in Ångströms

// Gadget constants
pub const HANDLE_TRIANGLE_SIDE_LENGTH: f64 = 1.2;
pub const HANDLE_RADIUS: f64 = 0.4;
pub const HANDLE_RADIUS_HIT_TEST_FACTOR: f64 = 1.2;
pub const HANDLE_DIVISIONS: u32 = 16;
pub const HANDLE_HEIGHT: f64 = 0.6;
pub const HANDLE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.8); // Dark blue for handles
pub const SELECTED_HANDLE_COLOR: Vec3 = Vec3::new(1.0, 0.6, 0.0); // Orange for selected handle
pub const LINE_COLOR: Vec3 = Vec3::new(0.0, 0.0, 0.6);
pub const LINE_DIVISIONS: u32 = 16;
pub const LINE_RADIUS: f64 = 0.18;
pub const LINE_RADIUS_HIT_TEST_FACTOR: f64 = 1.3;

pub const CONNECTED_PIN_SYMBOL: &str = "⎆";

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
    pub static ref DEFAULT_ZINCBLENDE_MOTIF: crate::structure_designer::evaluator::motif::Motif = {
        use crate::structure_designer::evaluator::motif_parser::parse_motif;
        
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
