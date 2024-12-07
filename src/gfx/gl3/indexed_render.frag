#version 330 core

in vec2 scene_pos;

uniform sampler2D game_scene;
uniform uint palette[16];

layout (location = 0) out vec4 color;

void main() {
    uint pixel = uint(texture(game_scene, scene_pos).r * 256.0);
    uint palette_color = palette[pixel];
    uint r = (palette_color >> 0u) % 256u;
    uint g = (palette_color >> 8u) % 256u;
    uint b = (palette_color >> 16u) % 256u;
    color = vec4(r / 255.0, g / 255.0, b / 255.0, 1.0);
}
