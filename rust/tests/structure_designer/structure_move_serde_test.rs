//! Serde back-compat for `structure_move`'s per-axis subdivision (issue #412).
//!
//! `lattice_subdivision` was a single uniform `i32` before the `subdiv_xyz`
//! feature made it an `IVec3`. Old `.cnnd` files must keep loading: a legacy
//! scalar splats across all three axes via `ivec3_or_int_serializer`.

use glam::i32::IVec3;
use rust_lib_flutter_cad::structure_designer::nodes::structure_move::StructureMoveData;

#[test]
fn legacy_scalar_subdivision_splats_across_all_axes() {
    let data: StructureMoveData =
        serde_json::from_str(r#"{"translation":[1,2,3],"lattice_subdivision":2}"#).unwrap();
    assert_eq!(data.translation, IVec3::new(1, 2, 3));
    assert_eq!(data.lattice_subdivision, IVec3::splat(2));
}

#[test]
fn missing_subdivision_defaults_to_one() {
    let data: StructureMoveData = serde_json::from_str(r#"{"translation":[1,2,3]}"#).unwrap();
    assert_eq!(data.lattice_subdivision, IVec3::ONE);
}

#[test]
fn per_axis_subdivision_roundtrips() {
    let original = StructureMoveData {
        translation: IVec3::new(-1, 0, 5),
        lattice_subdivision: IVec3::new(2, 4, 1),
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: StructureMoveData = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.translation, original.translation);
    assert_eq!(restored.lattice_subdivision, original.lattice_subdivision);
}
