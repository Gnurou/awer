#version 330 core

layout (location = 0) in ivec2 vertex;

uniform ivec2 pos;
uniform uvec2 bb;
uniform uint color_idx;

void main() {
    vec2 offset = bb / 2.0;
    vec2 fpos = pos + vertex - offset;

    vec2 normalized_pos = vec2((fpos.x / 320.0) * 2 - 1.0, (fpos.y / 200.0) * 2 - 1.0);

    gl_Position = vec4(normalized_pos, 0.0, 1.0);
}
