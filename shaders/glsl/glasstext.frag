#version 150

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 cTop;
    vec4 cMid;
    vec4 cBot;
};

uniform buf _40;

uniform sampler2D src;

in vec2 qt_TexCoord0;
out vec4 fragColor;

void main()
{
    float a = texture(src, qt_TexCoord0).w;
    float y = qt_TexCoord0.y;
    vec4 _34;
    if (y < 0.5)
    {
        _34 = mix(_40.cTop, _40.cMid, vec4(y / 0.5));
    }
    else
    {
        _34 = mix(_40.cMid, _40.cBot, vec4((y - 0.5) / 0.5));
    }
    vec4 c = _34;
    fragColor = (c * a) * _40.qt_Opacity;
}

