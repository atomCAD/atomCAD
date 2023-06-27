// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/*
 * Full-screen blit (full-screen triangle)
 * - Vertex shader
 */
#version 450

layout (location = 0) out vec2 uv;

out gl_PerVertex {
    vec4 gl_Position;
};

void main(void) {
    uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(vec2(uv.x * 2.0 - 1.0, -uv.y * 2.0 + 1.0), 0.0, 1.0);
}

// End of File
