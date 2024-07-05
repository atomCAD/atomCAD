use std::f32;
use std::f32::consts::PI;

#[allow(dead_code)]
pub struct Angles {
    // Angle from the +x axis to the projection of the target in the meridian plane, measured counterclockwise (i.e. the +y axis is at pi/2 radians)
    pub azimuthal: f32,
    // Angle from the +z axis to the target direction
    pub polar: f32,
}

#[allow(dead_code)]
pub static TETRAHEDRAL_ANGLE: f32 = 1.910_633_2; // acos(-1 / 3)
#[allow(dead_code)]
pub static BOND_SHAPES: [Option<&[Angles]>; 7] = [
    // There are no bond angles for an atom with zero bonding sites
    None,
    // s orbital only (linear, i.e. H2)
    Some(&[Angles {
        azimuthal: 0.0,
        polar: 0.0,
    }]),
    // sp hybridization (linear, i.e. CO2)
    Some(&[
        Angles {
            polar: 0.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: PI,
            azimuthal: 0.0,
        },
    ]),
    // sp2 hybridization (trigonal planar)
    Some(&[
        Angles {
            polar: 0.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: 2.0 * PI / 3.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: 2.0 * PI / 3.0,
            azimuthal: PI,
        },
    ]),
    // sp3 hybridization (tetrahedral)
    Some(&[
        Angles {
            polar: 0.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: TETRAHEDRAL_ANGLE,
            azimuthal: 0.0,
        },
        Angles {
            polar: TETRAHEDRAL_ANGLE,
            azimuthal: TETRAHEDRAL_ANGLE,
        },
        Angles {
            polar: TETRAHEDRAL_ANGLE,
            azimuthal: -TETRAHEDRAL_ANGLE,
        },
    ]),
    // sp3d hybridization
    Some(&[
        Angles {
            polar: 0.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: 2.0 * PI / 3.0,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: -2.0 * PI / 3.0,
        },
        Angles {
            polar: PI,
            azimuthal: 0.0,
        },
    ]),
    // sp3d2 hybridization
    Some(&[
        Angles {
            polar: 0.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: 0.0,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: PI / 2.0,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: PI,
        },
        Angles {
            polar: PI / 2.0,
            azimuthal: -PI / 2.0,
        },
        Angles {
            polar: PI,
            azimuthal: 0.0,
        },
    ]),
    // TODO: Investigate wether or not we need to support hypervalent bonding or if this is enough.
];
