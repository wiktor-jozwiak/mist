#version 150

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float fov;
    float pitch;
    vec2 size;
    vec4 sunDir;
    vec4 scene;
    vec4 wx;
    vec4 wx2;
    vec4 bolt;
    vec4 fx;
};

uniform buf _312;

in vec2 qt_TexCoord0;
out vec4 fragColor;
float HOR;
float base0;
float thick;
float cov;

float hash(vec2 p)
{
    vec3 q = fract(p.xyx * 0.103100001811981201171875);
    q += vec3(dot(q, q.yzx + vec3(33.3300018310546875)));
    return fract((q.x + q.y) * q.z);
}

float _noise(vec2 p)
{
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = (f * f) * (vec2(3.0) - (f * 2.0));
    vec2 param = i;
    vec2 param_1 = i + vec2(1.0, 0.0);
    vec2 param_2 = i + vec2(0.0, 1.0);
    vec2 param_3 = i + vec2(1.0);
    return mix(mix(hash(param), hash(param_1), u.x), mix(hash(param_2), hash(param_3), u.x), u.y);
}

float hg(float c, float g)
{
    float g2 = g * g;
    return (1.0 - g2) / pow((1.0 + g2) - ((2.0 * g) * c), 1.5);
}

float hash3(inout vec3 p)
{
    p = fract(p * 0.103100001811981201171875);
    p += vec3(dot(p, p.zyx + vec3(31.31999969482421875)));
    return fract((p.x + p.y) * p.z);
}

float _noise3(vec3 p)
{
    vec3 i = floor(p);
    vec3 f = fract(p);
    vec3 u = (f * f) * (vec3(3.0) - (f * 2.0));
    vec3 param = i;
    float _122 = hash3(param);
    vec3 param_1 = i + vec3(1.0, 0.0, 0.0);
    float _129 = hash3(param_1);
    vec3 param_2 = i + vec3(0.0, 1.0, 0.0);
    float _137 = hash3(param_2);
    vec3 param_3 = i + vec3(1.0, 1.0, 0.0);
    float _142 = hash3(param_3);
    vec3 param_4 = i + vec3(0.0, 0.0, 1.0);
    float _153 = hash3(param_4);
    vec3 param_5 = i + vec3(1.0, 0.0, 1.0);
    float _158 = hash3(param_5);
    vec3 param_6 = i + vec3(0.0, 1.0, 1.0);
    float _166 = hash3(param_6);
    vec3 param_7 = i + vec3(1.0);
    float _171 = hash3(param_7);
    return mix(mix(mix(_122, _129, u.x), mix(_137, _142, u.x), u.y), mix(mix(_153, _158, u.x), mix(_166, _171, u.x), u.y), u.z);
}

float fbm3(inout vec3 p, int oct)
{
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++)
    {
        if (i >= oct)
        {
            break;
        }
        vec3 param = p;
        v += (a * _noise3(param));
        p = (p * 2.0299999713897705078125) + vec3(17.1000003814697265625, 3.7000000476837158203125, 9.30000019073486328125);
        a *= 0.5;
    }
    return v;
}

float density(vec3 pos, bool detail)
{
    float h = (pos.y - base0) / thick;
    if ((h < 0.0) || (h > 1.0))
    {
        return 0.0;
    }
    vec3 q = (pos * 0.62000000476837158203125) + vec3(-_312.scene.z, 0.0, _312.scene.w * 0.3499999940395355224609375);
    vec3 param = q;
    int param_1 = 3;
    float _330 = fbm3(param, param_1);
    float b = _330;
    float thr = mix(0.579999983310699462890625, 0.2800000011920928955078125, cov) + (h * 0.1599999964237213134765625);
    float prof = smoothstep(0.0, 0.100000001490116119384765625, h) * smoothstep(1.0, 0.60000002384185791015625, h);
    float d = ((b - thr) * 3.2000000476837158203125) * prof;
    if (d <= 0.0)
    {
        return 0.0;
    }
    if (detail)
    {
        vec3 param_2 = (q * 3.2999999523162841796875) + vec3(0.0, _312.scene.w * 0.800000011920928955078125, 0.0);
        int param_3 = 2;
        float _378 = fbm3(param_2, param_3);
        float det = _378;
        d -= (((1.0 - clamp(d, 0.0, 1.0)) * det) * 0.550000011920928955078125);
    }
    return clamp(d, 0.0, 1.0);
}

void main()
{
    HOR = 0.86000001430511474609375;
    vec2 uv = qt_TexCoord0;
    if (_312.fx.y > 0.0)
    {
        HOR = _312.fx.y;
    }
    float asp = _312.size.x / _312.size.y;
    vec3 rc = normalize(vec3(((uv.x - 0.5) * asp) * _312.fov, (HOR - uv.y) * _312.fov, 1.0));
    float cp = cos(_312.pitch);
    float sp = sin(_312.pitch);
    vec3 rd = vec3(rc.x, (rc.y * cp) + (rc.z * sp), ((-rc.y) * sp) + (rc.z * cp));
    vec3 L = _312.sunDir.xyz;
    float s = _312.sunDir.w;
    cov = _312.wx.x;
    float rain = _312.wx.y;
    float fog = _312.wx.w;
    float storm = _312.wx2.x;
    float moonLight = _312.wx2.y;
    float dayK = smoothstep(-6.0, 7.0, s);
    float nightK = 1.0 - smoothstep(-16.0, -5.0, s);
    float warm = exp(-pow((s - 2.0) / 6.0, 2.0));
    float gloom = smoothstep(0.60000002384185791015625, 1.0, cov);
    vec3 sunCol = mix(vec3(1.0, 0.4799999892711639404296875, 0.2199999988079071044921875), vec3(1.0, 0.949999988079071044921875, 0.87000000476837158203125), vec3(smoothstep(0.0, 20.0, s)));
    float e = rd.y;
    vec3 zen = mix(vec3(0.02500000037252902984619140625, 0.04500000178813934326171875, 0.10999999940395355224609375), vec3(0.0500000007450580596923828125, 0.07999999821186065673828125, 0.20000000298023223876953125), vec3(smoothstep(-18.0, -9.0, s)));
    zen = mix(zen, vec3(0.10999999940395355224609375, 0.180000007152557373046875, 0.4000000059604644775390625), vec3(smoothstep(-9.0, -2.0, s)));
    zen = mix(zen, vec3(0.119999997317790985107421875, 0.319999992847442626953125, 0.680000007152557373046875), vec3(smoothstep(-2.0, 12.0, s)));
    vec3 hor = mix(vec3(0.070000000298023223876953125, 0.0900000035762786865234375, 0.180000007152557373046875), vec3(0.17000000178813934326171875, 0.180000007152557373046875, 0.3400000035762786865234375), vec3(smoothstep(-18.0, -7.0, s)));
    hor = mix(hor, vec3(0.560000002384185791015625, 0.699999988079071044921875, 0.87999999523162841796875), vec3(smoothstep(-1.0, 14.0, s)));
    zen += (vec3(0.0199999995529651641845703125, 0.039999999105930328369140625, 0.07999999821186065673828125) * moonLight);
    hor += (vec3(0.039999999105930328369140625, 0.0500000007450580596923828125, 0.07999999821186065673828125) * moonLight);
    float toSun = max(dot(normalize(rd.xz + vec2(9.9999997473787516355514526367188e-05)), normalize(L.xz + vec2(9.9999997473787516355514526367188e-05))), 0.0);
    hor = mix(hor, vec3(0.980000019073486328125, 0.5, 0.2599999904632568359375), vec3(warm * (0.3499999940395355224609375 + (0.64999997615814208984375 * pow(toSun, 3.0)))));
    zen = mix(zen, vec3(0.300000011920928955078125, 0.23999999463558197021484375, 0.4600000083446502685546875), vec3(warm * 0.4000000059604644775390625));
    hor += ((vec3(0.0900000035762786865234375, 0.0599999986588954925537109375, 0.039999999105930328369140625) * (1.0 - dayK)) * (0.4000000059604644775390625 + (0.60000002384185791015625 * cov)));
    vec3 gz = mix(vec3(0.0900000035762786865234375, 0.100000001490116119384765625, 0.14000000059604644775390625), vec3(0.37999999523162841796875, 0.4199999868869781494140625, 0.4900000095367431640625), vec3(dayK)) * ((1.0 - (0.300000011920928955078125 * rain)) - (0.3499999940395355224609375 * storm));
    vec3 gh = mix(vec3(0.17000000178813934326171875, 0.1599999964237213134765625, 0.189999997615814208984375), vec3(0.519999980926513671875, 0.560000002384185791015625, 0.62000000476837158203125), vec3(dayK)) * ((1.0 - (0.25 * rain)) - (0.300000011920928955078125 * storm));
    zen = mix(zen, gz, vec3(gloom * 0.85000002384185791015625));
    hor = mix(hor, gh, vec3(gloom * 0.85000002384185791015625));
    vec3 sky = mix(hor, zen, vec3(smoothstep(0.0, 0.75, e)));
    float mu = dot(rd, L);
    sky += (((sunCol * ((pow(max(mu, 0.0), 24.0) * 0.1599999964237213134765625) + (pow(max(mu, 0.0), 4.0) * 0.0500000007450580596923828125))) * smoothstep(-2.0, 1.0, s)) * (1.0 - (0.699999988079071044921875 * gloom)));
    if (e < 0.0)
    {
        vec3 field = mix(vec3(0.0350000001490116119384765625, 0.04500000178813934326171875, 0.039999999105930328369140625), vec3(0.20000000298023223876953125, 0.25, 0.20000000298023223876953125), vec3(dayK)) * (1.0 - (0.300000011920928955078125 * rain));
        field = mix(field, mix(vec3(0.1599999964237213134765625, 0.180000007152557373046875, 0.2199999988079071044921875), vec3(0.7799999713897705078125, 0.819999992847442626953125, 0.87999999523162841796875), vec3(dayK)), vec3(_312.fx.z));
        sky = mix(hor * 0.89999997615814208984375, field, vec3(smoothstep(0.0, -0.119999997317790985107421875, e)));
    }
    if ((_312.wx2.z > 0.0) && (e > 0.0))
    {
        float base = (0.180000007152557373046875 + (0.02999999932944774627685546875 * sin((rd.x * 6.0) + (_312.time * 0.119999997317790985107421875)))) + (0.0199999995529651641845703125 * sin((rd.x * 17.0) - (_312.time * 0.25)));
        float above = e - (base * 0.4000000059604644775390625);
        vec2 param = vec2((rd.x * 60.0) + (_312.time * 0.4000000059604644775390625), _312.time * 0.100000001490116119384765625);
        vec2 param_1 = vec2((rd.x * 9.0) - (_312.time * 0.100000001490116119384765625), 2.0);
        float rays = (0.5 + (0.5 * _noise(param))) * (0.5 + (0.5 * _noise(param_1)));
        float _824;
        if (above > 0.0)
        {
            _824 = exp((-above) * 7.0) * smoothstep(0.0, 0.0199999995529651641845703125, above);
        }
        else
        {
            _824 = 0.0;
        }
        float a = _824;
        sky += (((((mix(vec3(0.20000000298023223876953125, 0.949999988079071044921875, 0.550000011920928955078125), vec3(0.75, 0.300000011920928955078125, 0.85000002384185791015625), vec3(smoothstep(0.02999999932944774627685546875, 0.25, above))) * a) * rays) * _312.wx2.z) * nightK) * 0.5);
    }
    float T = 1.0;
    vec3 scat = vec3(0.0);
    float tHit = 0.0;
    if ((cov > 0.00999999977648258209228515625) && (e > 0.014999999664723873138427734375))
    {
        base0 = 1.0;
        thick = mix(0.449999988079071044921875, 0.949999988079071044921875, cov);
        float t0 = base0 / e;
        float t1 = (base0 + thick) / e;
        if (t0 < 30.0)
        {
            tHit = t0;
            t1 = min(t1, t0 + 5.0);
            float dt = (t1 - t0) / 12.0;
            vec2 param_2 = gl_FragCoord.xy;
            float tt = t0 + (dt * hash(param_2));
            vec3 Ll = normalize(mix(vec3(0.0, 1.0, 0.0), L, vec3(smoothstep(-3.0, 3.0, s) * 0.85000002384185791015625)));
            float cosT = dot(rd, Ll);
            float param_3 = cosT;
            float param_4 = 0.60000002384185791015625;
            float param_5 = cosT;
            float param_6 = -0.25;
            float phase = (0.699999988079071044921875 * hg(param_3, param_4)) + (0.300000011920928955078125 * hg(param_5, param_6));
            vec3 sunL = (((sunCol * dayK) * (1.0 - (0.64999997615814208984375 * gloom))) * 1.14999997615814208984375) + ((vec3(0.319999992847442626953125, 0.37000000476837158203125, 0.5) * moonLight) * 0.3499999940395355224609375);
            vec3 ambTop = mix(vec3(0.14000000059604644775390625, 0.1500000059604644775390625, 0.189999997615814208984375), mix(vec3(0.5, 0.579999983310699462890625, 0.7200000286102294921875), vec3(0.5, 0.5299999713897705078125, 0.579999983310699462890625), vec3(gloom)), vec3(dayK));
            vec3 ambBot = mix(vec3(0.17000000178813934326171875, 0.16500000655651092529296875, 0.189999997615814208984375), mix(vec3(0.300000011920928955078125, 0.319999992847442626953125, 0.37000000476837158203125), vec3(0.23999999463558197021484375, 0.2599999904632568359375, 0.300000011920928955078125), vec3(gloom)), vec3(dayK));
            float sigma = 7.0 + (5.0 * cov);
            for (int i = 0; i < 12; i++)
            {
                vec3 pos = rd * tt;
                vec3 param_7 = pos;
                bool param_8 = true;
                float d = density(param_7, param_8);
                if (d > 0.0040000001899898052215576171875)
                {
                    vec3 param_9 = pos + (Ll * 0.119999997317790985107421875);
                    bool param_10 = false;
                    vec3 param_11 = pos + (Ll * 0.319999992847442626953125);
                    bool param_12 = false;
                    float dl = density(param_9, param_10) + (density(param_11, param_12) * 0.800000011920928955078125);
                    float Tl = exp((-dl) * 2.599999904632568359375);
                    float h = clamp((pos.y - base0) / thick, 0.0, 1.0);
                    float powder = 1.0 - exp((-d) * 5.0);
                    vec3 lum = (sunL * (((Tl * phase) * 0.550000011920928955078125) + ((pow(Tl, 0.300000011920928955078125) * 0.449999988079071044921875) * (0.5 + (0.5 * powder))))) + mix(ambBot, ambTop, vec3(h));
                    lum += (((vec3(0.85000002384185791015625, 0.85000002384185791015625, 1.0) * _312.bolt.x) * exp((-abs(uv.x - _312.bolt.y)) * 4.0)) * 1.60000002384185791015625);
                    lum *= ((1.0 - (0.300000011920928955078125 * rain)) - (0.3499999940395355224609375 * storm));
                    float Ts = exp(((-d) * sigma) * dt);
                    scat += (lum * (T * (1.0 - Ts)));
                    T *= Ts;
                    if (T < 0.02999999932944774627685546875)
                    {
                        break;
                    }
                }
                tt += dt;
            }
        }
    }
    float fade = exp((-tHit) * 0.07500000298023223876953125);
    vec3 col = sky + ((scat - (sky * (1.0 - T))) * fade);
    float occ = (1.0 - T) * fade;
    if ((_312.fx.x > 0.0) && (e > 0.0))
    {
        float ang = degrees(acos(clamp(dot(rd, -L), -1.0, 1.0)));
        float k = (ang - 40.40000152587890625) / 2.2000000476837158203125;
        if ((k > 0.0) && (k < 1.0))
        {
            vec3 rb = clamp(abs(mod(vec3((1.0 - k) * 4.5) + vec3(0.0, 4.0, 2.0), vec3(6.0)) - vec3(3.0)) - vec3(1.0), vec3(0.0), vec3(1.0));
            col += ((((rb * sin(k * 3.1415998935699462890625)) * _312.fx.x) * 0.2800000011920928955078125) * smoothstep(0.0, 0.07999999821186065673828125, e));
        }
    }
    col /= vec3(1.0 + (max(max(col.x, col.y), col.z) * 0.180000007152557373046875));
    fragColor = vec4(col * 1.12000000476837158203125, occ);
}

