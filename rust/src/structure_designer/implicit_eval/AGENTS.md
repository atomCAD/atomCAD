# Implicit Eval - Agent Instructions

SDF (Signed Distance Field) evaluation and visualization for implicit geometry.

## Files

| File | Purpose |
|------|---------|
| `implicit_geometry.rs` | Re-exports `ImplicitGeometry2D/3D` from `geo_tree` |
| `ray_tracing.rs` | Sphere-marching ray tracer for SDF visualization |
| `surface_splatting_3d.rs` | 3D surface point sampling via SDF |
| `surface_splatting_2d.rs` | 2D surface point sampling via SDF |

## How It Works

Geometry nodes produce `ImplicitGeometry3D` (or 2D) objects — SDF functions that return signed distance to the nearest surface for any point in space.

Visualization approaches:
- **Surface splatting:** Sample points near the surface → display as point cloud
- **Ray tracing:** Sphere-march rays from camera → find surface intersections

## Ray Tracing Constants

- `MAX_STEPS = 100` per ray
- `MAX_DISTANCE = 5000.0` world units
- `SURFACE_THRESHOLD = 0.01`

## Volume Bounds

From `common_constants.rs`: implicit volumes evaluated within (-800, -800, -800) to (800, 800, 800).

## Dependencies

Uses `ImplicitGeometry2D`/`ImplicitGeometry3D` and `BATCH_SIZE` from the `geo_tree` module.
