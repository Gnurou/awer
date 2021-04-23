#version 330 core

layout (location = 0) in ivec2 vertex;

uniform ivec2 pos;
uniform ivec2 offset;
uniform uint zoom;
uniform uvec2 bb;
uniform uint color_idx;

void main() {
    // Compute zoom factor to apply to all our points.
    float zoom_factor = float(zoom) / 64.0;

    vec2 bbox_offset = bb * zoom_factor / 2.0;
    vec2 fpos = pos + ((vertex + offset) * zoom_factor) - bbox_offset;

    vec2 normalized_pos = vec2((fpos.x / 320.0) * 2 - 1.0, (fpos.y / 200.0) * 2 - 1.0);

    gl_Position = vec4(normalized_pos, 0.0, 1.0);
}
