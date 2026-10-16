#version 150

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;
};

uniform buf _55;

in vec2 qt_TexCoord0;
out vec4 fragColor;

float hash(vec2 p)
{
    vec3 q = fract(p.xyx * 0.103100001811981201171875);
    q += vec3(dot(q, q.yzx + vec3(33.3300018310546875)));
    return fract((q.x + q.y) * q.z);
}

void main()
{
    vec2 px = qt_TexCoord0 * _55.size;
    float w = _55.size.x;
    float h = _55.size.y;
    float d = length(px - vec2(w * 0.5, h * 0.4600000083446502685546875));
    float r0 = h * 0.3499999940395355224609375;
    float r1 = length(_55.size) * 0.579999983310699462890625;
    float a = 0.37999999523162841796875 * clamp((d - r0) / (r1 - r0), 0.0, 1.0);
    float _109;
    if (px.y < 90.0)
    {
        _109 = 0.2199999988079071044921875 * (1.0 - (px.y / 90.0));
    }
    else
    {
        _109 = 0.0;
    }
    float at = _109;
    float _127;
    if (px.y > (h - 140.0))
    {
        _127 = (0.2199999988079071044921875 * (px.y - (h - 140.0))) / 140.0;
    }
    else
    {
        _127 = 0.0;
    }
    float ab = _127;
    a += (at * (1.0 - a));
    a += (ab * (1.0 - a));
    vec2 param = px;
    a += ((hash(param) - 0.5) / 255.0);
    fragColor = (vec4(0.01568627543747425079345703125, 0.02745098061859607696533203125, 0.062745101749897003173828125, 1.0) * a) * _55.qt_Opacity;
}

