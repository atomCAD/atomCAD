@group(0) @binding(0) // Assume binding 0 is for the uniform buffer
var<uniform> time: f32;

@vertex fn vs(@builtin(vertex_index) vertexIndex : u32) -> @builtin(position) vec4f {
  let offset = sin(time) * 0.5;
  let pos = array(
    vec2f( 0.0 + offset,  0.9),  // top center
    vec2f(-0.9 + offset, -0.9),  // bottom left
    vec2f( 0.9 + offset, -0.9)   // bottom right
  );

  return vec4f(pos[vertexIndex], 0.0, 1.0);
}

@fragment fn fs() -> @location(0) vec4f {
  return vec4f(1, 0, 0, 1);
}
