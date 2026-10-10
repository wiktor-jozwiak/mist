#version 300 es
// Mist night light (Control centre): warm the whole screen to about 4000 K. Set as Hyprland's
// decoration:screen_shader only while it is on (a screen shader repaints every frame in full).
precision highp float;
in vec2 v_texcoord;
uniform sampler2D tex;
out vec4 fragColor;
void main() {
    vec4 c = texture(tex, v_texcoord);
    fragColor = vec4(c.r, c.g * 0.84, c.b * 0.66, c.a);
}
