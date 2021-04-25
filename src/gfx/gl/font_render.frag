#version 330 core

in vec2 char_pos;
flat in uint char_color;
flat in uint char_off;

uniform uint font[192];

layout (location = 0) out float pix_color;

void main() {
    uvec2 char_upos = uvec2(char_pos.x, char_pos.y);
    uint char_yoff = (char_off * 8u) + char_upos.y;
    uint word = font[char_yoff / 4u];
    uint byte = (word >> (8u * (char_yoff % 4u)) & 0xffu);
    uint bit = (byte >> (8u - char_upos.x)) & 1u;

    if (bit != 0u) {
        pix_color = (char_color & 0xfu) / 256.0;
    } else {
        discard;
    }
}
