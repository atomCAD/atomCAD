#version 450

layout(location = 0) flat in vec4 v_Normal;
layout(location = 0) out vec4 o_Color;
layout(location = 1) out vec4 o_Normal;

void main() {
    o_Color = vec4(0.96, 0.26, 0.82, 1.0);
    o_Normal = v_Normal;
}