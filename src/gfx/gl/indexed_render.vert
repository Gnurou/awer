#version 330 core

layout (location = 0) in vec2 position;
layout (location = 1) in vec2 scene_position;

out vec2 scene_pos;

void main() {
    scene_pos = scene_position;
    gl_Position = vec4(position, 0.0, 1.0);
}
