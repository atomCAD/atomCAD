use crate::line_lod::{LINE_LOD_SLOTS, LodSlot};
use bytemuck;
use glam::f32::Vec3;

/// One line segment as the GPU draws it: one instance of a six-vertex quad
/// that `line_mesh.wgsl` expands to a width in pixels
/// (`doc/design_screen_space_lines.md` D1).
///
/// 60 bytes. Every screen-space length is in **logical pixels**; the shader
/// multiplies by the render target's `pixel_scale` (D2).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineSegmentInstance {
    /// Segment ends in world space (the mesh's model matrix still applies).
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    /// Linear RGBA, **not** premultiplied.
    pub color: [f32; 4],
    /// Width in logical pixels.
    pub width_px: f32,
    /// Dash pattern in logical pixels; `dash_off_px == 0` means solid (D5).
    pub dash_on_px: f32,
    pub dash_off_px: f32,
    /// World-space arc length of `p0` along its polyline (D5); 0 for a lone
    /// segment.
    pub dash_phase: f32,
    /// Grid level-of-detail slot (D9); 0 = never hidden or faded.
    pub lod_slot: u32,
}

impl LineSegmentInstance {
    /// The instance-rate vertex buffer layout `line_mesh.wgsl` reads.
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
            0 => Float32x3, // p0
            1 => Float32x3, // p1
            2 => Float32x4, // color
            3 => Float32,   // width_px
            4 => Float32,   // dash_on_px
            5 => Float32,   // dash_off_px
            6 => Float32,   // dash_phase
            7 => Uint32,    // lod_slot
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineSegmentInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRIBUTES,
        }
    }
}

/// How a segment is drawn (D7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineStyle {
    /// Linear RGBA.
    pub color: [f32; 4],
    /// Width in logical pixels.
    pub width_px: f32,
    pub dash: Option<LineDash>,
    /// Grid level-of-detail slot, an id returned by [`LineMesh::add_lod_slot`]
    /// (D9); 0 = never hidden or faded.
    pub lod_slot: u32,
}

/// A dash pattern in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineDash {
    pub on_px: f32,
    pub off_px: f32,
}

impl LineStyle {
    /// 1 px, solid, opaque: what every line was before screen-space lines.
    pub const fn solid(color: [f32; 3]) -> Self {
        Self {
            color: [color[0], color[1], color[2], 1.0],
            width_px: 1.0,
            dash: None,
            lod_slot: 0,
        }
    }

    pub const fn with_width(self, width_px: f32) -> Self {
        Self { width_px, ..self }
    }

    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self {
            color: [self.color[0], self.color[1], self.color[2], alpha],
            ..self
        }
    }

    pub const fn dashed(self, on_px: f32, off_px: f32) -> Self {
        Self {
            dash: Some(LineDash { on_px, off_px }),
            ..self
        }
    }

    pub const fn with_lod_slot(self, lod_slot: u32) -> Self {
        Self { lod_slot, ..self }
    }
}

/// A line mesh in CPU memory: the list of segments every line producer fills
/// and `GPUMesh::update_from_line_mesh` uploads as instances.
#[derive(Clone, Debug, Default)]
pub struct LineMesh {
    pub instances: Vec<LineSegmentInstance>,
    /// The grid level-of-detail slots its segments refer to (D9): slot id
    /// `i + 1` is `lod_slots[i]`.
    pub lod_slots: Vec<LodSlot>,
}

impl LineMesh {
    pub fn new() -> Self {
        Self {
            instances: Vec::new(),
            lod_slots: Vec::new(),
        }
    }

    /// Registers a grid level-of-detail slot and returns the id to put in
    /// [`LineStyle::lod_slot`] (D9). Ids run from 1; when the table is full
    /// this warns and returns 0, and those lines never fade.
    pub fn add_lod_slot(&mut self, slot: LodSlot) -> u32 {
        if self.lod_slots.len() >= LINE_LOD_SLOTS - 1 {
            eprintln!(
                "LineMesh: more than {} LOD slots, the extra lines will not fade",
                LINE_LOD_SLOTS - 1
            );
            return 0;
        }
        self.lod_slots.push(slot);
        self.lod_slots.len() as u32
    }

    pub fn add_segment(&mut self, p0: &Vec3, p1: &Vec3, style: &LineStyle) {
        self.push_segment(p0, p1, style, 0.0);
    }

    /// Consecutive segments through `points`, each carrying the world arc
    /// length of its start along the polyline as its dash phase, so a dash
    /// pattern continues across the joints instead of restarting at each one
    /// (D5). `closed` adds the segment from the last point back to the first.
    /// Fewer than two points draw nothing.
    pub fn add_polyline(&mut self, points: &[Vec3], closed: bool, style: &LineStyle) {
        if points.len() < 2 {
            return;
        }
        let closing = if closed {
            Some((points[points.len() - 1], points[0]))
        } else {
            None
        };
        let mut arc_length = 0.0;
        for (p0, p1) in points
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .chain(closing)
        {
            self.push_segment(&p0, &p1, style, arc_length);
            arc_length += p0.distance(p1);
        }
    }

    fn push_segment(&mut self, p0: &Vec3, p1: &Vec3, style: &LineStyle, dash_phase: f32) {
        let (dash_on_px, dash_off_px) = match style.dash {
            Some(dash) => (dash.on_px, dash.off_px),
            None => (0.0, 0.0),
        };
        self.instances.push(LineSegmentInstance {
            p0: p0.to_array(),
            p1: p1.to_array(),
            color: style.color,
            width_px: style.width_px,
            dash_on_px,
            dash_off_px,
            dash_phase,
            lod_slot: style.lod_slot,
        });
    }

    /// `LineStyle::solid(color)`: the shortcut for producers not yet given a
    /// style of their own.
    pub fn add_line_with_uniform_color(
        &mut self,
        start_pos: &Vec3,
        end_pos: &Vec3,
        color: &[f32; 3],
    ) {
        self.add_segment(start_pos, end_pos, &LineStyle::solid(*color));
    }

    /// The segments' ends, in emission order.
    pub fn segments(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        self.instances
            .iter()
            .map(|s| (Vec3::from_array(s.p0), Vec3::from_array(s.p1)))
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}
