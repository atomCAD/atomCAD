/*
 * Full-screen blit (full-screen triangle)
 * - Vertex shader
 */
#version 450

layout (location = 0) out vec2 uv;

void main(void) {
    uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(vec2(uv.x * 2.0 - 1.0, -uv.y * 2.0 + 1.0), 0.0, 1.0);
}
