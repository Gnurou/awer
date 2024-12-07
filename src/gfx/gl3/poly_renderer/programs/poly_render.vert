#version 330 core

layout (location = 0) in vec2 pos;
layout (location = 1) in vec2 vertex;
layout (location = 2) in vec2 bb;
layout (location = 3) in float zoom;
layout (location = 4) in uint color;

flat out uint color_idx;

void main() {
    color_idx = color;

    vec2 bbox_offset = (bb * zoom) / 2.0;
    vec2 fpos = pos + (vertex * zoom) - bbox_offset;

    vec2 normalized_pos = vec2(
        (fpos.x / 320.0) * 2 - 1.0,
        (fpos.y / 200.0) * 2 - 1.0
    );

    gl_Position = vec4(normalized_pos, 0.0, 1.0);
}
