#version 450

layout(set = 0, binding = 0) uniform Globals {
    mat4 u_Transform;
};

layout(location = 0) in vec3 a_Pos;
layout(location = 1) in vec3 a_Normal;

layout(location = 0) out vec4 v_Normal;

void main() {
    v_Normal = vec4(a_Normal, 0.0);
    gl_Position = u_Transform * vec4(a_Pos, 1.0);
}