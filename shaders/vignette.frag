#version 440
// Desktop vignette (drawn over the wallpaper, LiveWallpaper.qml, primary monitor): soft darkening at the
// edges and behind the bar / dock, so the cards sit on an even backdrop whatever the wallpaper does.
// Same gradients as the old full-screen Canvas, no CPU image.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;
};
float hash(vec2 p) { vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
void main() {
    vec2 px = qt_TexCoord0 * size;
    float w = size.x, h = size.y;
    // radial: transparent inside 0.35 h, 0.38 at 0.58 of the diagonal
    float d = length(px - vec2(w * 0.5, h * 0.46));
    float r0 = h * 0.35, r1 = length(size) * 0.58;
    float a = 0.38 * clamp((d - r0) / (r1 - r0), 0.0, 1.0);
    // top 90 px and bottom 140 px bands (source-over on top of the radial)
    float at = px.y < 90.0 ? 0.22 * (1.0 - px.y / 90.0) : 0.0;
    float ab = px.y > h - 140.0 ? 0.22 * (px.y - (h - 140.0)) / 140.0 : 0.0;
    a = a + at * (1.0 - a);
    a = a + ab * (1.0 - a);
    a += (hash(px) - 0.5) / 255.0;
    fragColor = vec4(vec3(4.0, 7.0, 16.0) / 255.0, 1.0) * a * qt_Opacity;
}
