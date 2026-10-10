#version 440
// Live wallpaper, full-resolution pass: upscales the half-resolution scene and adds a fixed
// sub-LSB dither so the dark gradients don't band.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
};
layout(binding = 1) uniform sampler2D src;

float hash(vec2 p) { vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }

void main() {
    vec3 col = texture(src, qt_TexCoord0).rgb;
    col += (hash(gl_FragCoord.xy) + hash(gl_FragCoord.xy + 17.3) - 1.0) / 255.0;
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
