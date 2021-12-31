#version 330 core

layout (location = 0) in ivec2 pos;
layout (location = 1) in ivec2 char_position;
layout (location = 2) in uint color;
layout (location = 3) in uint char_offset;

out vec2 char_pos;
flat out uint char_color;
flat out uint char_off;

void main() {
    char_pos = vec2(char_position.x, char_position.y);
    char_color = color;
    char_off = char_offset;

    vec2 normalized_pos = vec2((pos.x / 320.0) * 2 - 1.0, (pos.y / 200.0) * 2 - 1.0);
    gl_Position = vec4(normalized_pos, 0.0, 1.0);
}
