#version 440
// Liquid Glass numerals: vertical three-stop gradient clipped to the alpha of the source (the glyphs).
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 cTop;
    vec4 cMid;
    vec4 cBot;
};
layout(binding = 1) uniform sampler2D src;
void main() {
    float a = texture(src, qt_TexCoord0).a;
    float y = qt_TexCoord0.y;
    vec4 c = y < 0.5 ? mix(cTop, cMid, y / 0.5) : mix(cMid, cBot, (y - 0.5) / 0.5);
    fragColor = c * a * qt_Opacity;            // colour uniforms arrive premultiplied
}
