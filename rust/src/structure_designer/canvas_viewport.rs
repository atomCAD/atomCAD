/// Node-canvas viewport (pan offset + zoom level) stored per node network,
/// mirroring [`CameraSettings`](super::camera_settings::CameraSettings) for the
/// 3D camera. Restored on network activation so navigation (including
/// Back/Forward) lands where the user last left off instead of re-framing the
/// top-left node — issue #414 Phase 4, `doc/design_find_usages.md` D7.
///
/// Not undo-tracked (it is view state, like the camera); serialized in `.cnnd`
/// with a serde default so old files load with `None` and fall back to the
/// top-left auto-framing.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasViewport {
    /// Logical-space pan offset x (`NodeNetworkState._panOffset.dx`).
    pub pan_x: f64,
    /// Logical-space pan offset y (`NodeNetworkState._panOffset.dy`).
    pub pan_y: f64,
    /// Discrete zoom level, mirroring the Flutter `ZoomLevel` enum index:
    /// 0 = normal, 1 = medium, 2 = far.
    pub zoom_level: i32,
}
