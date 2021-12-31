#version 330 core

flat in uint color_idx;

uniform sampler2D self;
uniform sampler2D buffer0;
uniform vec2 viewport_size;

layout (location = 0) out float color;

void main() {
    if (color_idx == 0x10u) {
        uint source_color = uint(texture(self, gl_FragCoord.xy / viewport_size).r * 256.0);
        color = (source_color | 0x8u) / 256.0;
    }
    else if (color_idx == 0x11u) {
        color = texture(buffer0, gl_FragCoord.xy / viewport_size).r;
    }
    else {
        color = (color_idx & 0xfu) / 256.0;
    }
}
