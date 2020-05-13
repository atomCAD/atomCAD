

static VERT_SHADER: &[u32] = include_shader_binary!("compositor.vert");
static FRAG_SHADER: &[u32] = include_shader_binary!("compositor.frag");

/// Used to layer UI and scene on top of each other.
pub struct Compositor {
    bind_group: wgpu::BindGroup,
}

impl Compositor {
    pub fn new(device: &wgpu::Device) -> Self {
        

        Self {
            bind_group: unimplemented!(),
        }
    }
}
