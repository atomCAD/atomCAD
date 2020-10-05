# Ideas

## Now
- [ ] Can multidraw be used for all parts/fragments that aren't being modified, and then use single bind groups for writable ones?
    - [src/parts.rs](src/parts.rs)
- [x] Refactor rendering into a `Rendering` struct + module.
- [ ] Also, add instancing for drawing multiple copies of a fragment.
    - Maybe parts as well?
- [ ] Try batching many parts/fragments into a single buffer + bind group.
- [ ] Use a compute shader to efficiently patch the transform 'vertex' buffer.
    - Should all modified transforms be sorted into contiguous memory?
- [x] Move `BindGroupLayouts` to `Renderer` with helper function.
- [x] Split into a `render` crate?
- [ ] Make parts "world" and allow importing smaller part collections.
- [ ] Should parts and fragments be in an ecs?
- [ ] Add orientation cube like [this](https://cad.onshape.com/help/Content/Resources/Images/tutorial/viewcube.png).
    - Should this be geometry or a fancy shader (combination?)
- [ ] Move rendering passes to `passes` module.

## Future
- If we run out of gpu memory, try downloading everything to RAM or disk and re-uploading to defragment.
