//! The shadow map's GPU state (`doc/design_shadows.md` D9): one owner for
//! the map, its placeholder, the comparison sampler, group 2 of the lit
//! pipelines, the `ShadowUniform`, the light cameras and the caster passes.
//!
//! Phase 1 re-renders the map on every draw that has a shadow setup; the
//! cache by the remembered setup comes later.

use crate::gpu_mesh::GPUMesh;
use crate::pipelines::ShadowCasterPipelines;
use crate::renderer::CameraUniform;
use crate::shadow_fit::{MAX_CASCADES, SHADOW_MAP_SIZE, ShadowSetup, ShadowUniform, light_uniform};
use std::sync::Mutex;
use wgpu::*;

/// The shadow map's format: the comparison sampler reads it, the caster
/// pipelines write it.
pub(crate) const SHADOW_MAP_FORMAT: TextureFormat = TextureFormat::Depth32Float;

/// The meshes that cast, and the pipelines they are drawn with (D3, D4).
/// Built by `Renderer::draw_to_target`.
pub(crate) struct ShadowCasters<'a> {
    pub pipelines: &'a ShadowCasterPipelines,
    pub main_mesh: &'a GPUMesh,
    pub atom_impostor_mesh: &'a GPUMesh,
    pub bond_impostor_mesh: &'a GPUMesh,
}

/// The allocated shadow map: a `MAX_CASCADES`-layer array, viewed whole for
/// sampling and one layer at a time for the caster passes.
struct ShadowMapTexture {
    _texture: Texture,
    array_view: TextureView,
    layer_views: Vec<TextureView>,
}

/// What a draw changes (D9): the pass counter, under `&self`.
#[derive(Default)]
struct ShadowState {
    passes_drawn: u64,
}

pub(crate) struct ShadowMap {
    /// Group 2 of the lit pipelines: the `ShadowUniform`, the map, the
    /// comparison sampler.
    layout: BindGroupLayout,
    bind_group: BindGroup,
    uniform_buffer: Buffer,
    sampler: Sampler,
    /// Bound while there is no map. Cleared to depth 0, which shadows
    /// everything, so a path that samples it by mistake shows as a dark
    /// image rather than a silently correct one.
    _placeholder: Texture,
    placeholder_view: TextureView,
    /// `None` while shadows are off: a shadow-free session holds no map.
    map: Option<ShadowMapTexture>,
    /// Group 0 of cascade `k`'s caster pass: its light camera, with the
    /// content shading uniform (which no caster entry point reads).
    camera_buffers: Vec<Buffer>,
    camera_bind_groups: Vec<BindGroup>,
    /// Colour each receiver by its cascade (D9; its setter comes with the
    /// cascades).
    debug_tint: bool,
    state: Mutex<ShadowState>,
}

impl ShadowMap {
    /// Created before the pipeline layouts, which take
    /// [`ShadowMap::bind_group_layout`] for group 2.
    pub fn new(
        device: &Device,
        queue: &Queue,
        camera_bind_group_layout: &BindGroupLayout,
        content_shading_buffer: &Buffer,
    ) -> Self {
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("shadow_bind_group_layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Shadow Uniform Buffer"),
            size: std::mem::size_of::<ShadowUniform>() as BufferAddress,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Linear: each tap is a bilinear 2×2 comparison (D7).
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("Shadow Comparison Sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            compare: Some(CompareFunction::LessEqual),
            ..Default::default()
        });

        let placeholder = create_map_texture(device, "Shadow Map Placeholder", 1, 1);
        let placeholder_view = placeholder.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2Array),
            ..Default::default()
        });
        // A depth texture cannot be a copy destination, so an empty pass
        // clears it.
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Shadow Map Placeholder Clear"),
        });
        {
            let layer = placeholder.create_view(&TextureViewDescriptor::default());
            encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Shadow Map Placeholder Clear"),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth_attachment(&layer, 0.0)),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        queue.submit(Some(encoder.finish()));

        let camera_buffers: Vec<Buffer> = (0..MAX_CASCADES)
            .map(|k| {
                device.create_buffer(&BufferDescriptor {
                    label: Some(&format!("Shadow Cascade {k} Camera Buffer")),
                    size: std::mem::size_of::<CameraUniform>() as BufferAddress,
                    usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let camera_bind_groups = camera_buffers
            .iter()
            .enumerate()
            .map(|(k, camera)| {
                device.create_bind_group(&BindGroupDescriptor {
                    label: Some(&format!("shadow_cascade_{k}_bind_group")),
                    layout: camera_bind_group_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: camera.as_entire_binding(),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: content_shading_buffer.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect();

        let bind_group = create_bind_group(
            device,
            &layout,
            &uniform_buffer,
            &placeholder_view,
            &sampler,
        );
        Self {
            layout,
            bind_group,
            uniform_buffer,
            sampler,
            _placeholder: placeholder,
            placeholder_view,
            map: None,
            camera_buffers,
            camera_bind_groups,
            debug_tint: false,
            state: Mutex::new(ShadowState::default()),
        }
    }

    /// Group 2's layout, for `PipelineLayouts::lit`.
    pub fn bind_group_layout(&self) -> &BindGroupLayout {
        &self.layout
    }

    /// Group 2 of the lit pipelines, bound after every lit `set_pipeline`
    /// (D8).
    pub fn bind_group(&self) -> &BindGroup {
        &self.bind_group
    }

    /// Creates the map when shadows turn on and drops it when they turn off,
    /// rebinding group 2 over it or over the placeholder (D9). From
    /// `Renderer::set_shading`, the one place that knows whether shadows are
    /// effectively on.
    pub fn set_enabled(&mut self, device: &Device, on: bool) {
        if on == self.map.is_some() {
            return;
        }
        self.map = on.then(|| {
            let texture =
                create_map_texture(device, "Shadow Map", SHADOW_MAP_SIZE, MAX_CASCADES as u32);
            let array_view = texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..Default::default()
            });
            let layer_views = (0..MAX_CASCADES as u32)
                .map(|layer| {
                    texture.create_view(&TextureViewDescriptor {
                        dimension: Some(TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect();
            ShadowMapTexture {
                _texture: texture,
                array_view,
                layer_views,
            }
        });
        let view = self
            .map
            .as_ref()
            .map_or(&self.placeholder_view, |map| &map.array_view);
        self.bind_group = create_bind_group(
            device,
            &self.layout,
            &self.uniform_buffer,
            view,
            &self.sampler,
        );
    }

    /// Whether the map exists (shadows are on).
    pub fn allocated(&self) -> bool {
        self.map.is_some()
    }

    /// How many cascade passes have been recorded (D9), for tests.
    pub fn passes_drawn(&self) -> u64 {
        self.state.lock().unwrap().passes_drawn
    }

    /// Before a draw: writes the `ShadowUniform` for `setup` and, when there
    /// is one, the light cameras and one caster pass per cascade into
    /// `encoder`, ahead of the main pass in the same submit (D4). A setup
    /// is only ever passed while the map exists.
    pub fn prepare(
        &self,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        setup: Option<&ShadowSetup>,
        casters: &ShadowCasters,
    ) {
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[ShadowUniform::new(setup, self.debug_tint)]),
        );
        let (Some(setup), Some(map)) = (setup, &self.map) else {
            return;
        };
        let mut state = self.state.lock().unwrap();
        for k in 0..setup.cascades.len().min(MAX_CASCADES) {
            queue.write_buffer(
                &self.camera_buffers[k],
                0,
                bytemuck::cast_slice(&[light_uniform(setup, k)]),
            );
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Shadow Caster Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(depth_attachment(&map.layer_views[k], 1.0)),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.camera_bind_groups[k], &[]);
            for (pipeline, mesh) in [
                (&casters.pipelines.triangle, casters.main_mesh),
                (&casters.pipelines.atom_impostor, casters.atom_impostor_mesh),
                (&casters.pipelines.bond_impostor, casters.bond_impostor_mesh),
            ] {
                if mesh.num_indices == 0 {
                    continue;
                }
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &mesh.model_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }
            state.passes_drawn += 1;
        }
    }
}

/// A depth array texture of `layers` square layers of `size` texels, drawn
/// into by the caster passes and sampled by the receivers.
fn create_map_texture(device: &Device, label: &str, size: u32, layers: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some(label),
        size: Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: SHADOW_MAP_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// A depth attachment cleared to `depth`. A caster pass clears to 1, the far
/// end, so a texel nothing covers lights every receiver.
fn depth_attachment(view: &TextureView, depth: f32) -> RenderPassDepthStencilAttachment<'_> {
    RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(Operations {
            load: LoadOp::Clear(depth),
            store: StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

fn create_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    uniform_buffer: &Buffer,
    view: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("shadow_bind_group"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(view),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}
