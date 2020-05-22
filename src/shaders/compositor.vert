#version 450

layout (location = 0) out vec2 tex_coords;

void main()  {
    tex_coords = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(tex_coords.x * 2.0f - 1.0f, tex_coords.y * -2.0f + 1.0f, 0.0f, 1.0f);
}
