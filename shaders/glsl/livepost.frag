#version 150

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
};

uniform buf _90;

uniform sampler2D src;

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
    vec3 col = texture(src, qt_TexCoord0).xyz;
    vec2 param = gl_FragCoord.xy;
    vec2 param_1 = gl_FragCoord.xy + vec2(17.299999237060546875);
    col += vec3(((hash(param) + hash(param_1)) - 1.0) / 255.0);
    fragColor = vec4(col, 1.0) * _90.qt_Opacity;
}

