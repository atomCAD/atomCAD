# Ideas

## Now

- Can multidraw be used for all parts/fragments that aren't being modified, and then use single bind groups for writable ones?
    - [src/parts.rs](src/parts.rs)
- Refactor rendering into a `Rendering` struct + module.
- Also, add instancing for drawing multiple copies of a fragment.
    - Maybe parts as well?
- Try batching many parts/fragments into a single buffer + bind group.

## Future
- If we run out of gpu memory, try downloading everything to RAM or disk and re-uploading to defragment.
