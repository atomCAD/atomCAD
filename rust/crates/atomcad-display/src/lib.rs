//! Domain → renderer adapter: turns `atomcad-crystolecule` structures,
//! `atomcad-geo-tree` geometry and display preferences into the meshes
//! `atomcad-renderer` draws.
//!
//! `scene_tessellator` used to live here. It is not an adapter from the domain
//! to the renderer — it adapts the *scene graph* to the renderer, and the scene
//! graph is a `structure_designer` concept. Phase 5 of
//! `doc/design_rust_crate_split.md` (D7) moved it up, which is what let this
//! crate stop depending on `structure_designer`.
//!
//! `half_space_utils` and `xyz_gadget_utils` came the other way, from
//! `structure_designer/src/utils/` (`doc/design_push_domain_code_down.md` §2):
//! shared gadget geometry — half-space discs and handles, the XYZ move/rotate
//! gadget — used by eight node files, filed one layer above the crate it
//! already depended on. They keep their `_utils` names rather than taking the
//! `*_tessellator` convention because each is half hit-test, and they stay two
//! sibling modules rather than merging because both define
//! `tessellate_center_sphere` and a `CENTER_SPHERE_*` constant family. The
//! Miller-index number theory that `half_space_utils` used to carry is *not*
//! here — crystallography does not belong in the adapter layer, so it went down
//! to `atomcad_crystolecule::miller` instead (D5).

pub mod atomic_tessellator;
pub mod coordinate_system_tessellator;
pub mod csg_to_poly_mesh;
pub mod gadget;
pub mod guided_placement_tessellator;
pub mod half_space_utils;
pub mod poly_mesh;
pub mod poly_mesh_tessellator;
pub mod preferences;
pub mod surface_point_cloud;
pub mod surface_point_tessellator;
pub mod unit_cell_wireframe_tessellator;
pub mod xyz_gadget_utils;
