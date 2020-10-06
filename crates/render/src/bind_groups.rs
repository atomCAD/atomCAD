pub trait AsBindingResource {
    fn as_binding_resource(&self) -> wgpu::BindingResource;
}
