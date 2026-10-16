#version 150

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float mode;
    vec2 size;
    vec4 c0;
    vec4 c1;
    vec4 c2;
    vec4 c3;
    vec4 opt;
    vec4 sun;
};

uniform buf _352;

in vec2 qt_TexCoord0;
out vec4 fragColor;
float AA;

float hash(vec2 p)
{
    vec3 q = fract(p.xyx * 0.103100001811981201171875);
    q += vec3(dot(q, q.yzx + vec3(33.3300018310546875)));
    return fract((q.x + q.y) * q.z);
}

vec3 sWarp(vec2 q, float T)
{
    float r = length(q);
    float a = atan(q.y, q.x);
    vec3 col = _352.c0.xyz + ((_352.c1.xyz * 0.3499999940395355224609375) * exp((-r) * 5.0));
    for (int l = 0; l < 3; l++)
    {
        float L = float(l);
        float n = 120.0 + (70.0 * L);
        float ai = floor(((a / 6.283185482025146484375) + 0.5) * n);
        vec2 param = vec2(ai, L + 1.0);
        float h = hash(param);
        if (h < 0.3499999940395355224609375)
        {
            continue;
        }
        float ac = (((ai + 0.5) / n) * 6.283185482025146484375) - 3.1415927410125732421875;
        float da = abs(a - ac) * r;
        float z = fract((h * 7.0) + ((T * (0.180000007152557373046875 + (0.100000001490116119384765625 * L))) * (0.60000002384185791015625 + h)));
        float rs = (z * z) * 1.2999999523162841796875;
        float len = 0.0199999995529651641845703125 + ((0.300000011920928955078125 * z) * z);
        float on = smoothstep(rs - len, rs - (len * 0.60000002384185791015625), r) * smoothstep(rs, rs - 0.00999999977648258209228515625, r);
        col += (((mix(_352.c2.xyz, _352.c3.xyz, vec3(fract(h * 5.0))) * on) * smoothstep(0.00120000005699694156646728515625 + (0.0030000000260770320892333984375 * z), 0.0, da)) * (0.25 + (0.89999997615814208984375 * z)));
    }
    return col;
}

vec2 rot(vec2 p, float a)
{
    float c = cos(a);
    float s = sin(a);
    return vec2((c * p.x) - (s * p.y), (s * p.x) + (c * p.y));
}

vec3 sTunnel(inout vec2 q, float T)
{
    vec2 param = q;
    float param_1 = 0.1500000059604644775390625 * sin(T * 0.20000000298023223876953125);
    q = rot(param, param_1);
    float r = length(q);
    float a = atan(q.y, q.x);
    float z = (0.60000002384185791015625 / (r + 0.014999999664723873138427734375)) + (T * 1.60000002384185791015625);
    vec2 uv = vec2((a / 6.283185482025146484375) * 16.0, z);
    vec2 w = fwidth(uv);
    vec2 dl = abs(fract(uv + vec2(0.5)) - vec2(0.5));
    float lx = 1.0 - smoothstep(0.0199999995529651641845703125 + (0.5 * w.x), 0.0199999995529651641845703125 + (1.5 * w.x), dl.x);
    float lz = 1.0 - smoothstep(0.039999999105930328369140625 + (0.5 * w.y), 0.039999999105930328369140625 + (1.5 * w.y), dl.y);
    float line = max(lx * 0.60000002384185791015625, lz);
    vec3 lc = mix(_352.c2.xyz, _352.c3.xyz, vec3(0.5 + (0.5 * sin((z * 0.4000000059604644775390625) + (T * 0.300000011920928955078125)))));
    vec3 col = _352.c0.xyz + (((lc * line) * smoothstep(0.00999999977648258209228515625, 0.3499999940395355224609375, r)) * 0.89999997615814208984375);
    col += ((_352.c1.xyz * 0.4000000059604644775390625) * exp((-r) * 9.0));
    return col;
}

vec3 sCode(vec2 uv, float T)
{
    vec2 px = uv * _352.size;
    vec2 cell = vec2(14.0, 18.0);
    vec2 id = floor(px / cell);
    vec2 f = fract(px / cell);
    float rows = _352.size.y / cell.y;
    vec3 col = _352.c0.xyz;
    float _820;
    for (int l = 0; l < 2; l++)
    {
        float L = float(l);
        vec2 lid = id + vec2(L * 0.5, 0.0);
        vec2 param = vec2(lid.x, 3.0 + L);
        float h = hash(param);
        float sp = (5.0 + (9.0 * h)) * (1.0 - (0.4000000059604644775390625 * L));
        float head = mod((T * sp) + (h * 200.0), rows + 40.0);
        float d = head - id.y;
        if ((d >= 0.0) && (d < 30.0))
        {
            _820 = exp((-d) * 0.10999999940395355224609375);
        }
        else
        {
            _820 = 0.0;
        }
        float b = _820;
        vec2 gp = floor(f * vec2(5.0, 7.0));
        vec2 param_1 = lid;
        vec2 param_2 = (gp + (lid * 7.0)) + vec2(floor((T * 1.5) + (hash(param_1) * 10.0)) * 13.0);
        float glyph = step(0.449999988079071044921875, hash(param_2));
        glyph *= (((step(0.5, gp.x) * step(gp.x, 3.5)) * step(0.5, gp.y)) * step(gp.y, 5.5));
        float k = (L > 0.5) ? 0.3499999940395355224609375 : 1.0;
        col += ((((_352.c2.xyz * glyph) * b) * 0.85000002384185791015625) * k);
        col += (((((_352.c3.xyz * glyph) * step(0.0, d)) * step(d, 1.0)) * 0.89999997615814208984375) * k);
        if (L < 0.5)
        {
            break;
        }
    }
    return col;
}

vec3 pal(float t)
{
    vec3 _345;
    if (t < 0.5)
    {
        _345 = mix(_352.c1.xyz, _352.c2.xyz, vec3(t * 2.0));
    }
    else
    {
        _345 = mix(_352.c2.xyz, _352.c3.xyz, vec3((t * 2.0) - 1.0));
    }
    return _345;
}

vec3 sPlasma(vec2 q, float T)
{
    float t = T * 0.3499999940395355224609375;
    float v = ((sin((q.x * 5.0) + t) + sin((q.y * 6.0) - (t * 0.800000011920928955078125))) + sin(((q.x + q.y) * 4.0) + (t * 0.60000002384185791015625))) + sin((length(q - vec2(0.3499999940395355224609375 * sin(t * 0.5), 0.20000000298023223876953125 * cos(t * 0.699999988079071044921875))) * 10.0) - (t * 1.39999997615814208984375));
    v = 0.5 + (0.125 * v);
    float param = fract(v + (t * 0.0500000007450580596923828125));
    vec3 col = pal(param);
    col *= (0.550000011920928955078125 + (0.449999988079071044921875 * smoothstep(0.20000000298023223876953125, 0.89999997615814208984375, 0.5 + (0.5 * sin((v * 6.283185482025146484375) * 2.0)))));
    return mix(_352.c0.xyz, col, vec3(0.550000011920928955078125));
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

float fbm(inout vec2 p)
{
    float v = 0.0;
    float a = 0.5;
    mat2 m = mat2(vec2(1.60000002384185791015625, 1.2000000476837158203125), vec2(-1.2000000476837158203125, 1.60000002384185791015625));
    for (int i = 0; i < 5; i++)
    {
        vec2 param = p;
        v += (a * _noise(param));
        p = m * p;
        a *= 0.5;
    }
    return v;
}

vec2 hash2(vec2 p)
{
    vec2 param = p;
    vec2 param_1 = p + vec2(19.1900005340576171875);
    return vec2(hash(param), hash(param_1));
}

float sdSeg(vec2 p, vec2 a, vec2 b)
{
    vec2 pa = p - a;
    vec2 ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - (ba * h));
}

vec3 sStorm(vec2 q, float T)
{
    vec3 col = mix(_352.c0.xyz, _352.c1.xyz, vec3(smoothstep(-0.5, 0.5, q.y) * 0.60000002384185791015625));
    vec2 param = (q * 3.0) - vec2(T * 0.039999999105930328369140625);
    float _1035 = fbm(param);
    vec2 param_1 = vec2((q.x * 2.0) + (T * 0.0599999986588954925537109375), (q.y * 3.0) - (T * 0.0199999995529651641845703125)) + vec2(_1035);
    float _1039 = fbm(param_1);
    float cl = _1039;
    col = mix(col, _352.c1.xyz * 1.2000000476837158203125, vec3(smoothstep(0.3499999940395355224609375, 0.85000002384185791015625, cl) * smoothstep(-0.300000011920928955078125, 0.300000011920928955078125, q.y)));
    float per = floor(T / 7.0);
    float ph = T - (per * 7.0);
    vec2 param_2 = vec2(per, 4.0);
    vec2 h = hash2(param_2);
    float env = (smoothstep(0.0, 0.07999999821186065673828125, ph) * exp((-ph) * 3.0)) * (0.699999988079071044921875 + (0.300000011920928955078125 * sin(ph * 40.0)));
    vec2 sp = vec2((h.x - 0.5) * 1.39999997615814208984375, 0.25);
    col += ((((_352.c3.xyz * env) * 0.5) * exp((-length(q - sp)) * 3.0)) * (0.4000000059604644775390625 + cl));
    if (ph < 0.60000002384185791015625)
    {
        float x = sp.x;
        float dmin = 1.0;
        vec2 prev = sp;
        for (int i = 1; i < 10; i++)
        {
            float fi = float(i);
            vec2 param_3 = vec2(per, fi);
            vec2 nxt = vec2(x + ((((hash(param_3) - 0.5) * 0.07999999821186065673828125) * fi) * 0.4000000059604644775390625), sp.y - (fi * 0.070000000298023223876953125));
            vec2 param_4 = q;
            vec2 param_5 = prev;
            vec2 param_6 = nxt;
            dmin = min(dmin, sdSeg(param_4, param_5, param_6));
            prev = nxt;
            x = nxt.x;
        }
        col += ((vec3(0.85000002384185791015625, 0.89999997615814208984375, 1.0) * env) * ((smoothstep(0.0040000001899898052215576171875, 0.0, dmin) * 0.89999997615814208984375) + (exp((-dmin) * 60.0) * 0.25)));
    }
    vec2 rq = vec2((q.x * 60.0) + (q.y * 12.0), (q.y * 2.0) + (T * 3.0));
    vec2 param_7 = vec2(floor(rq.x), 1.0);
    vec2 param_8 = vec2(floor(rq.x), 2.0);
    float rr = step(0.920000016689300537109375, hash(param_7)) * smoothstep(0.699999988079071044921875, 1.0, fract(rq.y + hash(param_8)));
    col += ((vec3(0.5, 0.60000002384185791015625, 0.75) * rr) * 0.119999997317790985107421875);
    return col;
}

float stars(vec2 q, float sc, float t)
{
    vec2 g = q * sc;
    vec2 id = floor(g);
    vec2 f = fract(g) - vec2(0.5);
    vec2 param = id;
    float h = hash(param);
    if (h < 0.920000016689300537109375)
    {
        return 0.0;
    }
    vec2 param_1 = id;
    vec2 o = (hash2(param_1) - vec2(0.5)) * 0.60000002384185791015625;
    float tw = 0.550000011920928955078125 + (0.449999988079071044921875 * sin((t * (0.800000011920928955078125 + (2.5 * fract(h * 91.0)))) + (h * 40.0)));
    return smoothstep(0.07999999821186065673828125 + (0.0599999986588954925537109375 * fract(h * 13.0)), 0.0, length(f - o)) * tw;
}

float fill(float d)
{
    return smoothstep(AA, -AA, d);
}

vec3 sFire(vec2 q, float T)
{
    vec3 col = mix(_352.c0.xyz, _352.c1.xyz, vec3(smoothstep(0.5, -0.4000000059604644775390625, q.y) * 0.60000002384185791015625));
    vec2 param = q;
    float param_1 = 55.0;
    float param_2 = T;
    col += (((vec3(1.0) * stars(param, param_1, param_2)) * 0.60000002384185791015625) * smoothstep(0.0, 0.300000011920928955078125, q.y));
    vec2 base = vec2(0.0, -0.300000011920928955078125);
    col += ((_352.c3.xyz * 0.3499999940395355224609375) * exp((-length((q - base) * vec2(1.0, 2.0))) * 3.0));
    for (int i = 0; i < 10; i++)
    {
        float fi = float(i);
        vec2 param_3 = vec2(fi, 1.0);
        float x0 = ((-0.949999988079071044921875) + (fi * 0.20000000298023223876953125)) + (0.0599999986588954925537109375 * hash(param_3));
        if (abs(x0) < 0.25)
        {
            continue;
        }
        vec2 param_4 = vec2(fi, 2.0);
        float h = 0.25 + (0.20000000298023223876953125 * hash(param_4));
        float yb = -0.3499999940395355224609375;
        float w = (h - (q.y - yb)) * 0.2800000011920928955078125;
        bool _1324 = q.y > yb;
        bool _1333;
        if (_1324)
        {
            _1333 = q.y < (yb + h);
        }
        else
        {
            _1333 = _1324;
        }
        bool _1352;
        if (_1333)
        {
            _1352 = abs(q.x - x0) < (w * (0.800000011920928955078125 + (0.20000000298023223876953125 * sin((q.y - yb) * 60.0))));
        }
        else
        {
            _1352 = _1333;
        }
        if (_1352)
        {
            col = _352.c0.xyz * 0.5;
        }
    }
    if (q.y < (-0.3300000131130218505859375))
    {
        col = mix(_352.c0.xyz * 0.60000002384185791015625, _352.c3.xyz * 0.300000011920928955078125, vec3(exp((-length((q - base) * vec2(1.0, 4.0))) * 4.0)));
    }
    vec2 param_5 = q;
    vec2 param_6 = base + vec2(-0.0900000035762786865234375, -0.0199999995529651641845703125);
    vec2 param_7 = base + vec2(0.07999999821186065673828125, 0.00999999977648258209228515625);
    vec2 param_8 = q;
    vec2 param_9 = base + vec2(-0.070000000298023223876953125, 0.014999999664723873138427734375);
    vec2 param_10 = base + vec2(0.0900000035762786865234375, -0.0199999995529651641845703125);
    float logs = min(sdSeg(param_5, param_6, param_7), sdSeg(param_8, param_9, param_10)) - 0.01400000043213367462158203125;
    float param_11 = logs;
    col = mix(col, vec3(0.20000000298023223876953125, 0.100000001490116119384765625, 0.0500000007450580596923828125), vec3(fill(param_11)));
    vec2 fq = ((q - base) - vec2(0.0, 0.00999999977648258209228515625)) * vec2(1.0, 0.75);
    vec2 param_12 = vec2(fq.x * 9.0, (fq.y * 6.0) - (T * 2.2000000476837158203125));
    float _1444 = fbm(param_12);
    float n = _1444;
    float shape = ((1.0 - length((fq * vec2(5.5, 3.0)) - vec2(0.0, 0.3499999940395355224609375))) + (n * 0.800000011920928955078125)) - (fq.y * 1.5);
    float fl = smoothstep(0.1500000059604644775390625, 0.800000011920928955078125, shape) * step(-0.0199999995529651641845703125, fq.y);
    vec3 fc = mix(vec3(0.800000011920928955078125, 0.1500000059604644775390625, 0.0500000007450580596923828125), vec3(1.0, 0.550000011920928955078125, 0.100000001490116119384765625), vec3(smoothstep(0.20000000298023223876953125, 0.60000002384185791015625, shape)));
    fc = mix(fc, vec3(1.0, 0.920000016689300537109375, 0.60000002384185791015625), vec3(smoothstep(0.64999997615814208984375, 1.0, shape)));
    col = mix(col, fc, vec3(fl));
    col += (((_352.c3.xyz * 0.1500000059604644775390625) * smoothstep(-0.20000000298023223876953125, 0.60000002384185791015625, shape)) * (1.0 - fl));
    for (int i_1 = 0; i_1 < 18; i_1++)
    {
        float fi_1 = float(i_1);
        vec2 param_13 = vec2(fi_1, 5.0);
        vec2 param_14 = vec2(fi_1, 6.0);
        float ph = fract((T * (0.25 + (0.20000000298023223876953125 * hash(param_13)))) + hash(param_14));
        vec2 param_15 = vec2(fi_1, 7.0);
        vec2 sp = base + vec2(((hash(param_15) - 0.5) * 0.07999999821186065673828125) + ((0.119999997317790985107421875 * sin((ph * 4.0) + fi_1)) * ph), 0.0500000007450580596923828125 + (ph * 0.550000011920928955078125));
        col += (((vec3(1.0, 0.60000002384185791015625, 0.20000000298023223876953125) * exp((-length(q - sp)) * 300.0)) * (1.0 - ph)) * 1.5);
    }
    return col;
}

vec3 sFireworks(vec2 q, float T)
{
    vec3 col = mix(_352.c1.xyz * 0.60000002384185791015625, _352.c0.xyz, vec3(smoothstep(-0.5, 0.4000000059604644775390625, q.y)));
    vec2 param = q;
    float param_1 = 50.0;
    float param_2 = T;
    col += ((vec3(1.0) * stars(param, param_1, param_2)) * 0.4000000059604644775390625);
    for (int k = 0; k < 5; k++)
    {
        float fk = float(k);
        float P = 3.599999904632568359375 + (fk * 0.37000000476837158203125);
        float tt = T + (fk * 1.2999999523162841796875);
        float seed = floor(tt / P) + (fk * 17.0);
        float tl = tt - (floor(tt / P) * P);
        vec2 param_3 = vec2(seed, 1.0);
        vec2 h = hash2(param_3);
        vec2 c = vec2((h.x - 0.5) * 1.2999999523162841796875, 0.0500000007450580596923828125 + (0.300000011920928955078125 * h.y));
        float param_4 = fract((h.x * 3.7000000476837158203125) + h.y);
        vec3 fc = pal(param_4);
        fc = mix(fc, vec3(1.0, 0.949999988079071044921875, 0.85000002384185791015625), vec3(0.25));
        if (tl < 0.699999988079071044921875)
        {
            float k2 = tl / 0.699999988079071044921875;
            vec2 rp = vec2(c.x, (-0.5) + ((c.y + 0.5) * (1.0 - ((1.0 - k2) * (1.0 - k2)))));
            vec2 param_5 = q;
            vec2 param_6 = rp;
            vec2 param_7 = rp - vec2(0.0, 0.039999999105930328369140625);
            float d = sdSeg(param_5, param_6, param_7);
            col += ((vec3(1.0, 0.800000011920928955078125, 0.5) * exp((-d) * 250.0)) * 0.60000002384185791015625);
        }
        else
        {
            float tb = tl - 0.699999988079071044921875;
            float fade = exp((-tb) * 1.2999999523162841796875);
            vec2 rel = (q - c) + vec2(0.0, (0.02999999932944774627685546875 * tb) * tb);
            if (length(rel) > 0.300000011920928955078125)
            {
                continue;
            }
            float ai = floor(((mod(atan(rel.y, rel.x), 6.283185482025146484375) / 6.283185482025146484375) * 36.0) + 0.5);
            for (int j = -1; j <= 1; j++)
            {
                float fi = mod(ai + float(j), 36.0);
                int i = int(fi);
                vec2 param_8 = vec2(seed, fi);
                float ang = ((fi / 36.0) * 6.283185482025146484375) + (hash(param_8) * 0.1500000059604644775390625);
                vec2 param_9 = vec2(fi, seed);
                float spd = 0.1599999964237213134765625 + (0.0500000007450580596923828125 * hash(param_9));
                vec2 pp = (c + ((vec2(cos(ang), sin(ang)) * spd) * (1.0 - exp((-tb) * 2.2000000476837158203125)))) - vec2(0.0, (0.02999999932944774627685546875 * tb) * tb);
                float d_1 = length(q - pp);
                float tw = 0.699999988079071044921875 + (0.300000011920928955078125 * sin((T * 20.0) + fi));
                col += (((fc * fade) * tw) * ((5.9999998484272509813308715820312e-05 / ((d_1 * d_1) + 3.9999998989515006542205810546875e-05)) + (exp((-d_1) * 120.0) * 0.1500000059604644775390625)));
            }
            col += ((((fc * fade) * 0.100000001490116119384765625) * exp((-length(q - c)) * 6.0)) * exp((-tb) * 2.0));
        }
    }
    float bx = floor(q.x * 18.0);
    vec2 param_10 = vec2(bx, 2.0);
    if (q.y < ((-0.36000001430511474609375) + (0.0900000035762786865234375 * hash(param_10))))
    {
        vec2 param_11 = floor(q * vec2(90.0, 70.0));
        col = (_352.c0.xyz * 0.5) + (vec3(0.25, 0.1875, 0.100000001490116119384765625) * step(0.85000002384185791015625, hash(param_11)));
    }
    return col;
}

vec3 sGalaxy(vec2 q, float T)
{
    vec2 param = q + vec2(5.0);
    float param_1 = 60.0;
    float param_2 = T;
    vec3 col = _352.c0.xyz + ((vec3(1.0) * stars(param, param_1, param_2)) * 0.5);
    vec2 param_3 = q;
    float param_4 = 0.4000000059604644775390625;
    vec2 g = rot(param_3, param_4);
    g.y *= 2.0;
    float r = length(g);
    float a = atan(g.y, g.x);
    float sw = (a + (T * 0.039999999105930328369140625)) - (log(r + 0.0199999995529651641845703125) * 2.400000095367431640625);
    float arms = pow(0.5 + (0.5 * cos(sw * 2.0)), 3.0);
    vec2 param_5 = g;
    float param_6 = (T * 0.039999999105930328369140625) - (r * 3.0);
    vec2 param_7 = rot(param_5, param_6) * 6.0;
    float _1961 = fbm(param_7);
    float dust = _1961;
    float dens = (exp((-r) * 3.2000000476837158203125) * (0.25 + (0.949999988079071044921875 * arms))) * (0.4000000059604644775390625 + (0.89999997615814208984375 * dust));
    col += ((mix(_352.c1.xyz, _352.c2.xyz, vec3(arms)) * dens) * 1.39999997615814208984375);
    col += ((_352.c3.xyz * exp((-r) * 18.0)) * 1.10000002384185791015625);
    vec2 param_8 = g * 40.0;
    float param_9 = (T * 0.039999999105930328369140625) * (1.0 + (2.0 / (r + 0.300000011920928955078125)));
    vec2 sp = rot(param_8, param_9);
    vec2 param_10 = floor(sp);
    col += (((vec3(0.89999997615814208984375, 0.89999997615814208984375, 1.0) * step(0.9700000286102294921875, hash(param_10))) * dens) * 1.5);
    return col;
}

vec3 sHole(vec2 q, float T)
{
    float r = length(q);
    vec2 lq = q * (1.0 + (0.01200000010430812835693359375 / max(r * r, 0.0040000001899898052215576171875)));
    vec2 param = lq;
    float param_1 = 60.0;
    float param_2 = T;
    vec3 col = _352.c0.xyz + ((vec3(1.0) * stars(param, param_1, param_2)) * 0.800000011920928955078125);
    vec2 param_3 = lq * 2.0;
    float _2064 = fbm(param_3);
    col += ((_352.c1.xyz * 0.1500000059604644775390625) * _2064);
    vec2 v = vec2(q.x, q.y * 3.599999904632568359375);
    float rr = length(v);
    float ang = atan(v.y, v.x);
    vec2 param_4 = vec2((ang * 3.0) + ((T * 0.5) / (rr + 0.100000001490116119384765625)), rr * 14.0);
    float _2097 = fbm(param_4);
    float swirl = _2097;
    float disk = smoothstep(0.100000001490116119384765625, 0.14000000059604644775390625, rr) * smoothstep(0.550000011920928955078125, 0.20000000298023223876953125, rr);
    float dop = 1.0 + (0.699999988079071044921875 * ((-q.x) / max(rr, 0.0500000007450580596923828125)));
    vec3 hot = mix(_352.c2.xyz, _352.c3.xyz, vec3(smoothstep(0.3499999940395355224609375, 0.119999997317790985107421875, rr)));
    vec3 dc = ((hot * disk) * (0.4000000059604644775390625 + (0.89999997615814208984375 * swirl))) * dop;
    float arc = abs(length(q - vec2(0.0)) - 0.104999996721744537353515625);
    vec2 param_5 = vec2((atan(q.y, q.x) * 4.0) - (T * 0.60000002384185791015625), 1.0);
    float _2160 = fbm(param_5);
    vec3 halo = ((hot * smoothstep(0.02999999932944774627685546875, 0.0, arc)) * 0.800000011920928955078125) * (0.60000002384185791015625 + (0.4000000059604644775390625 * _2160));
    if (q.y > 0.0)
    {
        col += (dc * 0.800000011920928955078125);
    }
    col += halo;
    col = mix(col, vec3(0.0), vec3(smoothstep(0.087999999523162841796875, 0.08200000226497650146484375, r)));
    if (q.y <= 0.0)
    {
        col += dc;
    }
    col += ((hot * 0.60000002384185791015625) * smoothstep(0.0040000001899898052215576171875, 0.0, abs(r - 0.0900000035762786865234375)));
    return col;
}

vec3 sOcean(vec2 q, float T)
{
    float hy = 0.07999999821186065673828125;
    vec3 col = mix(_352.c1.xyz, _352.c0.xyz, vec3(smoothstep(hy, 0.5, q.y)));
    vec2 mc = vec2(0.3499999940395355224609375, 0.2800000011920928955078125);
    col += ((_352.c3.xyz * 0.25) * exp((-length(q - mc)) * 5.0));
    col = mix(col, mix(_352.c3.xyz, vec3(1.0), vec3(0.5)), vec3(smoothstep(0.05200000107288360595703125, 0.048000000417232513427734375, length(q - mc))));
    for (int i = 0; i < 7; i++)
    {
        float fi = float(i);
        float k = fi / 6.0;
        float y0 = (hy - 0.0199999995529651641845703125) - ((k * k) * 0.550000011920928955078125);
        float amp = 0.006000000052154064178466796875 + ((0.0500000007450580596923828125 * k) * k);
        float fr = 14.0 - (10.0 * k);
        float sp = 0.4000000059604644775390625 + (0.60000002384185791015625 * k);
        float x = ((q.x * fr) + (T * sp)) + (fi * 1.7000000476837158203125);
        float yw = y0 + (amp * (sin(x) + (0.3499999940395355224609375 * sin((x * 2.2999999523162841796875) + (T * 0.699999988079071044921875)))));
        if (q.y < yw)
        {
            float depth = yw - q.y;
            vec3 wc = mix(_352.c2.xyz, _352.c0.xyz, vec3(0.3499999940395355224609375 + (0.5 * k))) * (0.85000002384185791015625 + (0.25 * smoothstep(0.02999999932944774627685546875, 0.0, depth)));
            float foam = smoothstep(0.004999999888241291046142578125 + (0.00999999977648258209228515625 * k), 0.0, depth) * smoothstep(0.300000011920928955078125, 1.0, sin(x - 0.60000002384185791015625));
            col = mix(wc, vec3(0.89999997615814208984375, 0.949999988079071044921875, 1.0), vec3(foam * 0.800000011920928955078125));
            col += ((((_352.c3.xyz * 0.25) * exp(-pow((q.x - mc.x) * (3.0 + (6.0 * k)), 2.0))) * smoothstep(0.5, 1.0, sin((x * 3.0) + (q.y * 90.0)))) * (1.0 - k));
        }
    }
    return col;
}

vec3 sKaleido(inout vec2 q, float T)
{
    vec2 param = q;
    float param_1 = T * 0.0500000007450580596923828125;
    q = rot(param, param_1);
    float r = length(q);
    float a = atan(q.y, q.x);
    float n = 8.0;
    float seg = 6.283185482025146484375 / n;
    a = mod(a, seg);
    a = abs(a - (seg * 0.5));
    vec2 p = vec2(cos(a), sin(a)) * r;
    vec2 param_2 = (p * 3.0) + vec2(T * 0.119999997317790985107421875);
    float _2441 = fbm(param_2);
    vec2 param_3 = ((p * 3.0) - vec2(T * 0.100000001490116119384765625)) + vec2(4.0);
    float _2451 = fbm(param_3);
    vec2 w = vec2(_2441, _2451);
    vec2 param_4 = ((p * 4.0) + (w * 2.5)) - vec2(T * 0.07999999821186065673828125, 0.0);
    float _2464 = fbm(param_4);
    float f = _2464;
    float param_5 = fract(((f * 1.60000002384185791015625) + (r * 0.60000002384185791015625)) - (T * 0.02999999932944774627685546875));
    vec3 col = pal(param_5);
    col *= (0.3499999940395355224609375 + (0.89999997615814208984375 * smoothstep(0.3499999940395355224609375, 0.800000011920928955078125, f)));
    col += ((_352.c3.xyz * smoothstep(0.0199999995529651641845703125, 0.0, abs(fract(f * 6.0) - 0.5) - 0.4600000083446502685546875)) * 0.25);
    return mix(_352.c0.xyz, col, vec3(((smoothstep(0.75, 0.0500000007450580596923828125, r) * 0.60000002384185791015625) + 0.4000000059604644775390625) * 0.699999988079071044921875));
}

vec2 plexPt(vec2 id, float T)
{
    vec2 param = id;
    vec2 h = hash2(param);
    return (id + vec2(0.5)) + (vec2(sin((T * (0.300000011920928955078125 + (0.4000000059604644775390625 * h.x))) + (h.y * 6.283185482025146484375)), cos((T * (0.25 + (0.4000000059604644775390625 * h.y))) + (h.x * 6.283185482025146484375))) * 0.4000000059604644775390625);
}

vec3 sPlexus(vec2 q, float T)
{
    vec3 col = mix(_352.c0.xyz, _352.c1.xyz * 0.5, vec3(smoothstep(0.60000002384185791015625, -0.5, q.y)));
    vec2 g = (q * 6.0) + vec2(T * 0.0500000007450580596923828125, 0.0);
    vec2 id = floor(g);
    int n = 0;
    vec2 pts[9];
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            vec2 param = id + vec2(float(i), float(j));
            float param_1 = T;
            pts[n] = plexPt(param, param_1);
            n++;
        }
    }
    float lines = 0.0;
    int _2621;
    int _2627;
    int _2634;
    int _2647;
    int _2652;
    int _2663;
    int _2669;
    for (int k = 0; k < 12; k++)
    {
        if (k < 8)
        {
            _2621 = 4;
        }
        else
        {
            if (k == 8)
            {
                _2627 = 1;
            }
            else
            {
                if (k == 9)
                {
                    _2634 = 1;
                }
                else
                {
                    _2634 = (k == 10) ? 7 : 7;
                }
                _2627 = _2634;
            }
            _2621 = _2627;
        }
        int a = _2621;
        if (k < 8)
        {
            if (k < 4)
            {
                _2652 = k;
            }
            else
            {
                _2652 = k + 1;
            }
            _2647 = _2652;
        }
        else
        {
            if (k == 8)
            {
                _2663 = 3;
            }
            else
            {
                if (k == 9)
                {
                    _2669 = 5;
                }
                else
                {
                    _2669 = (k == 10) ? 3 : 5;
                }
                _2663 = _2669;
            }
            _2647 = _2663;
        }
        int b = _2647;
        float len = length(pts[a] - pts[b]);
        if (len > 1.35000002384185791015625)
        {
            continue;
        }
        vec2 param_2 = g;
        vec2 param_3 = pts[a];
        vec2 param_4 = pts[b];
        lines += (smoothstep(0.02199999988079071044921875, 0.0, sdSeg(param_2, param_3, param_4)) * smoothstep(1.35000002384185791015625, 0.60000002384185791015625, len));
    }
    for (int i_1 = 0; i_1 < 9; i_1++)
    {
        float pd = length(g - pts[i_1]);
        col += (_352.c3.xyz * ((smoothstep(0.070000000298023223876953125, 0.0, pd) * 0.89999997615814208984375) + (exp((-pd) * 12.0) * 0.119999997317790985107421875)));
    }
    col += ((mix(_352.c2.xyz, _352.c3.xyz, vec3(0.5 + (0.5 * sin((q.x * 2.0) + (T * 0.20000000298023223876953125))))) * min(lines, 1.5)) * 0.449999988079071044921875);
    return col;
}

vec3 sEq(vec2 q, float T, float e, float asp)
{
    vec3 col = mix(_352.c0.xyz, _352.c1.xyz * 0.5, vec3(smoothstep(0.5, -0.5, q.y)));
    float n = 56.0;
    float x = ((q.x / asp) + 0.5) * n;
    float i = floor(x);
    float fx = fract(x);
    vec2 param = vec2(i * 0.3499999940395355224609375, T * 2.2000000476837158203125);
    vec2 param_1 = vec2(i * 0.89999997615814208984375, T * 5.0);
    float lvl = (pow(_noise(param), 1.39999997615814208984375) * 0.800000011920928955078125) + (0.20000000298023223876953125 * _noise(param_1));
    lvl *= (0.550000011920928955078125 + (0.699999988079071044921875 * e));
    float hgt = 0.039999999105930328369140625 + (0.4199999868869781494140625 * lvl);
    float base = -0.300000011920928955078125;
    float bar = step(0.119999997317790985107421875, fx) * step(fx, 0.87999999523162841796875);
    float y = q.y - base;
    float param_2 = clamp(y / 0.5, 0.0, 1.0);
    vec3 bc = pal(param_2);
    if ((y > 0.0) && (y < hgt))
    {
        col = mix(col, bc, vec3(bar * (0.60000002384185791015625 + (0.4000000059604644775390625 * step(0.25, fract(y * 60.0))))));
    }
    vec2 param_3 = vec2(i * 0.3499999940395355224609375, (T - 0.300000011920928955078125) * 2.2000000476837158203125);
    float cap = (hgt + 0.0199999995529651641845703125) + (0.02999999932944774627685546875 * pow(_noise(param_3), 1.39999997615814208984375));
    col = mix(col, _352.c3.xyz, vec3(bar * smoothstep(0.0040000001899898052215576171875, 0.0, abs(y - cap))));
    bool _2897 = y < 0.0;
    bool _2905;
    if (_2897)
    {
        _2905 = (-y) < (hgt * 0.60000002384185791015625);
    }
    else
    {
        _2905 = _2897;
    }
    if (_2905)
    {
        col = mix(col, bc, vec3((bar * 0.180000007152557373046875) * (1.0 + (y / (hgt * 0.60000002384185791015625)))));
    }
    col += ((_352.c2.xyz * 0.07999999821186065673828125) * exp((-abs(y)) * 6.0));
    return col;
}

vec3 sOrbits(vec2 q, float T)
{
    vec2 param = q;
    float param_1 = 60.0;
    float param_2 = T;
    vec3 col = _352.c0.xyz + ((vec3(1.0) * stars(param, param_1, param_2)) * 0.5);
    vec2 p = vec2(q.x, q.y * 2.400000095367431640625);
    col += ((_352.c3.xyz * 0.5) * exp((-length(q)) * 9.0));
    col = mix(col, mix(_352.c3.xyz, vec3(1.0), vec3(0.5)), vec3(smoothstep(0.0419999994337558746337890625, 0.0379999987781047821044921875, length(q))));
    for (int i = 0; i < 6; i++)
    {
        float fi = float(i);
        float R = 0.119999997317790985107421875 + (fi * 0.085000000894069671630859375);
        float ring = abs(length(p) - R);
        col += ((_352.c2.xyz * smoothstep(0.0040000001899898052215576171875, 0.0, ring)) * 0.25);
        float a = ((T * 0.60000002384185791015625) / pow(R / 0.119999997317790985107421875, 1.5)) + (fi * 2.099999904632568359375);
        vec2 pp = vec2(cos(a), sin(a) / 2.400000095367431640625) * R;
        vec2 param_3 = vec2(fi, 3.0);
        float pr = 0.008000000379979610443115234375 + (0.01200000010430812835693359375 * hash(param_3));
        float param_4 = fract(fi * 0.37000000476837158203125);
        vec3 pc = pal(param_4);
        float lit = 0.5 + (0.5 * dot(normalize((q - pp) + vec2(9.9999997473787516355514526367188e-05)), normalize((-pp) + vec2(9.9999997473787516355514526367188e-05))));
        col = mix(col, pc * (0.4000000059604644775390625 + (0.800000011920928955078125 * lit)), vec3(smoothstep(pr, pr - 0.00200000009499490261077880859375, length(q - pp))));
        col += ((pc * 0.20000000298023223876953125) * exp((-length(q - pp)) * 40.0));
        float tr = abs(length(p) - R) + 0.0;
        float ta = mod((atan(p.y, p.x) - a) + 6.283185482025146484375, 6.283185482025146484375);
        col += (((pc * smoothstep(0.006000000052154064178466796875, 0.0, tr)) * exp((-ta) * 2.5)) * 0.4000000059604644775390625);
    }
    return col;
}

vec3 sBubbles(vec2 q, float T)
{
    vec3 col = mix(_352.c0.xyz, _352.c1.xyz, vec3(smoothstep(-0.5, 0.60000002384185791015625, q.y) * 0.60000002384185791015625));
    for (int k = 0; k < 16; k++)
    {
        float fk = float(k);
        vec2 param = vec2(fk, 2.0);
        vec2 h = hash2(param);
        float R = 0.02999999932944774627685546875 + (0.070000000298023223876953125 * h.y);
        float y = mod(((T * 0.02999999932944774627685546875) * (0.60000002384185791015625 + h.x)) + (h.y * 1.60000002384185791015625), 1.39999997615814208984375) - 0.699999988079071044921875;
        vec2 c = vec2(((h.x - 0.5) * 1.7000000476837158203125) + (0.0500000007450580596923828125 * sin((T * 0.5) + fk)), y);
        vec2 d = q - c;
        d.x *= (1.0 + (0.0500000007450580596923828125 * sin((T * 3.0) + fk)));
        float r = length(d);
        if (r < (R + 0.00999999977648258209228515625))
        {
            float k2 = r / R;
            vec2 param_1 = (d * 30.0) + vec2(T);
            float th = (((k2 * k2) * 2.0) + (0.300000011920928955078125 * _noise(param_1))) + (T * 0.100000001490116119384765625);
            vec3 film = vec3(0.5) + (cos((vec3(th) + vec3(0.0, 0.3300000131130218505859375, 0.670000016689300537109375)) * 6.283185482025146484375) * 0.5);
            float rim = smoothstep(0.60000002384185791015625, 1.0, k2) * smoothstep(1.019999980926513671875, 0.980000019073486328125, k2);
            col = mix(col, film, vec3(rim * 0.550000011920928955078125));
            col += ((film * 0.0500000007450580596923828125) * step(k2, 1.0));
            col += ((vec3(1.0) * smoothstep(0.25, 0.0, length((d / vec2(R)) - vec2(-0.4000000059604644775390625, 0.4000000059604644775390625)))) * 0.5);
        }
    }
    return col;
}

vec3 sMeteors(vec2 q, float T)
{
    vec3 col = mix(_352.c1.xyz, _352.c0.xyz, vec3(smoothstep(-0.5, 0.5, q.y)));
    vec2 param = q;
    float param_1 = 70.0;
    float param_2 = T;
    vec2 param_3 = q + vec2(2.0);
    float param_4 = 30.0;
    float param_5 = T;
    col += (((vec3(1.0) * stars(param, param_1, param_2)) * 0.800000011920928955078125) + ((vec3(0.800000011920928955078125, 0.89999997615814208984375, 1.0) * stars(param_3, param_4, param_5)) * 0.4000000059604644775390625));
    vec2 dir = vec2(-0.87621593475341796875, -0.481918752193450927734375);
    for (int k = 0; k < 8; k++)
    {
        float fk = float(k);
        float P = 2.2000000476837158203125 + (fk * 0.310000002384185791015625);
        float tt = T + (fk * 0.89999997615814208984375);
        float seed = floor(tt / P) + (fk * 11.0);
        float ph = (tt - (floor(tt / P) * P)) / 0.89999997615814208984375;
        if (ph > 1.0)
        {
            continue;
        }
        vec2 param_6 = vec2(seed, 3.0);
        vec2 h = hash2(param_6);
        vec2 head = vec2((-0.20000000298023223876953125) + (h.x * 1.2000000476837158203125), 0.100000001490116119384765625 + (h.y * 0.4000000059604644775390625)) + ((dir * ph) * 0.699999988079071044921875);
        vec2 rel = q - head;
        float along = dot(rel, -dir);
        float across = abs(dot(rel, vec2(-dir.y, dir.x)));
        float tail = (step(0.0, along) * smoothstep(0.2199999988079071044921875, 0.0, along)) * smoothstep(0.0030000000260770320892333984375, 0.0, across);
        col += (((mix(_352.c3.xyz, vec3(1.0), vec3(0.60000002384185791015625)) * (tail + exp((-length(rel)) * 200.0))) * sin(ph * 3.1415927410125732421875)) * 0.89999997615814208984375);
    }
    vec2 param_7 = vec2(q.x * 20.0, 1.0);
    float g = ((-0.37999999523162841796875) + (0.02999999932944774627685546875 * sin(q.x * 4.0))) + (0.0199999995529651641845703125 * _noise(param_7));
    if (q.y < g)
    {
        col = _352.c0.xyz * 0.4000000059604644775390625;
    }
    return col;
}

vec3 sMeadow(vec2 q, float T)
{
    vec3 col = mix(_352.c3.xyz, _352.c1.xyz, vec3(smoothstep(-0.20000000298023223876953125, 0.5, q.y)));
    vec2 sc = vec2(-0.300000011920928955078125, -0.119999997317790985107421875);
    col += ((_352.c3.xyz * 0.5) * exp((-length(q - sc)) * 4.0));
    col = mix(col, vec3(1.0, 0.89999997615814208984375, 0.699999988079071044921875), vec3(smoothstep(0.061999998986721038818359375, 0.0579999983310699462890625, length(q - sc))));
    for (int b = 0; b < 4; b++)
    {
        float fb = float(b);
        float ph = fract((T * 0.0199999995529651641845703125) + (fb * 0.25));
        vec2 bp = vec2((-1.0) + (ph * 2.2000000476837158203125), (0.1500000059604644775390625 + (0.07999999821186065673828125 * fb)) + (0.0199999995529651641845703125 * sin(T + fb)));
        vec2 r = q - bp;
        float wing = sin((T * 7.0) + fb) * 0.008000000379979610443115234375;
        vec2 param = r;
        vec2 param_1 = vec2(-0.014999999664723873138427734375, wing);
        vec2 param_2 = vec2(0.0);
        vec2 param_3 = r;
        vec2 param_4 = vec2(0.0);
        vec2 param_5 = vec2(0.014999999664723873138427734375, wing);
        float bird = min(sdSeg(param, param_1, param_2), sdSeg(param_3, param_4, param_5));
        col = mix(col, _352.c0.xyz, vec3(smoothstep(0.0024999999441206455230712890625, 0.0, bird)));
    }
    for (int l = 0; l < 4; l++)
    {
        float L = float(l);
        float k = L / 3.0;
        float y0 = (-0.07999999821186065673828125) - (0.119999997317790985107421875 * L);
        float dens = 120.0 + (80.0 * L);
        float xi = floor(q.x * dens);
        for (int j = -1; j <= 1; j++)
        {
            float id = xi + float(j);
            vec2 param_6 = vec2(id, L);
            float h = hash(param_6);
            float bx = (id + 0.5) / dens;
            float hgt = (0.0599999986588954925537109375 + (0.119999997317790985107421875 * h)) * (0.60000002384185791015625 + (0.60000002384185791015625 * k));
            float t = clamp((q.y - y0) / hgt, 0.0, 1.0);
            float gust = 0.5 + (0.5 * sin(((bx * 3.0) - (T * 1.39999997615814208984375)) + L));
            float bend = (((0.00999999977648258209228515625 + (0.0500000007450580596923828125 * gust)) * t) * t) * (0.5 + k);
            float d = abs((q.x - bx) - bend) - ((0.0040000001899898052215576171875 * (1.0 - t)) * (0.699999988079071044921875 + k));
            bool _3687 = q.y > (y0 - 0.20000000298023223876953125);
            bool _3696;
            if (_3687)
            {
                _3696 = q.y < (y0 + hgt);
            }
            else
            {
                _3696 = _3687;
            }
            if (_3696 && (d < 0.0))
            {
                col = mix(_352.c2.xyz * (0.4000000059604644775390625 + (0.5 * t)), _352.c0.xyz, vec3(0.60000002384185791015625 * k)) * (0.5 + (0.5 * (1.0 - k)));
            }
        }
        if (q.y < y0)
        {
            col = mix(_352.c2.xyz * 0.3499999940395355224609375, _352.c0.xyz, vec3(0.5 + (0.4000000059604644775390625 * k)));
        }
    }
    return col;
}

vec3 sHighway(vec2 q, float T)
{
    float hy = 0.0199999995529651641845703125;
    vec3 col = mix(_352.c1.xyz, _352.c0.xyz, vec3(smoothstep(hy, 0.5, q.y)));
    float bx = floor(q.x * 22.0);
    vec2 param = vec2(bx, 1.0);
    float bh = (hy + 0.0500000007450580596923828125) + ((0.180000007152557373046875 * hash(param)) * smoothstep(0.100000001490116119384765625, 0.89999997615814208984375, abs(q.x)));
    bool _3783 = q.y > hy;
    bool _3790;
    if (_3783)
    {
        _3790 = q.y < bh;
    }
    else
    {
        _3790 = _3783;
    }
    if (_3790)
    {
        col = _352.c0.xyz * 0.800000011920928955078125;
        vec2 w = floor(vec2(q.x * 150.0, q.y * 120.0));
        vec2 param_1 = w;
        col += (((vec3(0.4000000059604644775390625, 0.319999992847442626953125, 0.20000000298023223876953125) * step(0.800000011920928955078125, hash(param_1))) * step(0.5, fract(q.x * 150.0))) * step(0.4000000059604644775390625, fract(q.y * 120.0)));
    }
    if (q.y < hy)
    {
        float z = 0.25 / ((hy - q.y) + 0.0040000001899898052215576171875);
        float cx = (0.4000000059604644775390625 * sin((z * 0.07999999821186065673828125) + 1.0)) - 0.336588382720947265625;
        float lat = (q.x - ((cx / z) * 3.0)) * z;
        col = _352.c0.xyz * 0.699999988079071044921875;
        float fog = exp((-(hy - q.y)) * 3.0);
        float _3906;
        for (int i = 0; i < 6; i++)
        {
            float fi = float(i);
            float lane = (fi - 2.5) * 0.3499999940395355224609375;
            float d = abs(lat - lane);
            bool away = i < 3;
            bvec3 _3900 = bvec3(away);
            vec3 lc = vec3(_3900.x ? vec3(1.0, 0.180000007152557373046875, 0.119999997317790985107421875).x : vec3(1.0, 0.920000016689300537109375, 0.75).x, _3900.y ? vec3(1.0, 0.180000007152557373046875, 0.119999997317790985107421875).y : vec3(1.0, 0.920000016689300537109375, 0.75).y, _3900.z ? vec3(1.0, 0.180000007152557373046875, 0.119999997317790985107421875).z : vec3(1.0, 0.920000016689300537109375, 0.75).z);
            if (away)
            {
                _3906 = (-T) * 3.0;
            }
            else
            {
                _3906 = T * 4.0;
            }
            float flow = 0.60000002384185791015625 + (0.4000000059604644775390625 * sin(((z * 0.60000002384185791015625) + _3906) + (fi * 2.0)));
            col += ((((lc * smoothstep(0.0599999986588954925537109375, 0.0, d)) * flow) * 0.699999988079071044921875) * (0.300000011920928955078125 + (0.699999988079071044921875 * fog)));
            col += ((lc * exp((-d) * 5.0)) * 0.0500000007450580596923828125);
        }
        float dash = step(0.5, fract((z * 0.4000000059604644775390625) - (T * 2.0))) * smoothstep(0.02999999932944774627685546875, 0.0, abs(lat));
        col += ((vec3(0.60000002384185791015625) * dash) * 0.25);
    }
    col += ((_352.c3.xyz * 0.25) * exp((-abs(q.y - hy)) * 30.0));
    return col;
}

vec3 sVortex(vec2 q, float T)
{
    float r = length(q);
    float a = atan(q.y, q.x);
    a += ((1.2000000476837158203125 / (r + 0.1500000059604644775390625)) + (T * 0.25));
    vec2 p = vec2(cos(a), sin(a)) * r;
    vec2 param = (p * 4.0) - vec2(T * 0.100000001490116119384765625);
    float _4027 = fbm(param);
    vec2 param_1 = ((p * 3.0) + vec2((log(r + 0.00999999977648258209228515625) * 2.0) - (T * 0.4000000059604644775390625), 0.0)) + vec2(_4027);
    float _4031 = fbm(param_1);
    float f = _4031;
    float param_2 = fract(f + (r * 0.5));
    vec3 col = pal(param_2);
    col *= (smoothstep(0.20000000298023223876953125, 0.89999997615814208984375, f) * 1.2000000476837158203125);
    col *= smoothstep(0.0199999995529651641845703125, 0.20000000298023223876953125, r);
    return _352.c0.xyz + (col * 0.85000002384185791015625);
}

vec3 sRibbons(vec2 q, float T)
{
    vec3 col = _352.c0.xyz;
    for (int i = 0; i < 4; i++)
    {
        float fi = float(i);
        float y = ((0.180000007152557373046875 * sin(((q.x * (1.60000002384185791015625 + (0.300000011920928955078125 * fi))) + (T * (0.5 + (0.1500000059604644775390625 * fi)))) + (fi * 1.89999997615814208984375))) + (0.0599999986588954925537109375 * sin(((q.x * 4.0) - (T * 0.699999988079071044921875)) + fi))) + ((fi - 1.5) * 0.0599999986588954925537109375);
        float tw = cos(((q.x * (3.0 + fi)) + (T * (0.800000011920928955078125 + (0.20000000298023223876953125 * fi)))) + fi);
        float th = 0.0040000001899898052215576171875 + (0.02999999932944774627685546875 * abs(tw));
        float d = abs(q.y - y);
        float param = fract(((fi * 0.300000011920928955078125) + (q.x * 0.1500000059604644775390625)) + (T * 0.0199999995529651641845703125));
        vec3 rc = pal(param);
        float body = smoothstep(th, th - 0.0030000000260770320892333984375, d);
        col = mix(col, (rc * (0.449999988079071044921875 + (0.550000011920928955078125 * abs(tw)))) + (vec3(0.20000000298023223876953125) * pow(abs(tw), 8.0)), vec3(body * 0.85000002384185791015625));
        col += (((rc * 0.0035000001080334186553955078125) / vec3(d + 0.00999999977648258209228515625)) * 0.25);
    }
    return col;
}

vec3 sGlobe(vec2 q, float T)
{
    vec3 col = _352.c0.xyz + ((_352.c1.xyz * 0.25) * exp((-length(q)) * 3.0));
    float R = 0.3300000131130218505859375;
    vec2 c = q - vec2(0.0);
    float r = length(c);
    col += (((_352.c2.xyz * 0.3499999940395355224609375) * smoothstep(0.0199999995529651641845703125, 0.0, abs(r - R))) + ((_352.c2.xyz * 0.1500000059604644775390625) * exp((-abs(r - R)) * 30.0)));
    if (r < R)
    {
        for (int side = 0; side < 2; side++)
        {
            float z = sqrt((R * R) - (r * r)) * ((side == 0) ? 1.0 : (-1.0));
            vec3 n = vec3(c.x, c.y, z) / vec3(R);
            float tilt = 0.4000000059604644775390625;
            n = vec3(n.x, (n.y * cos(tilt)) - (n.z * sin(tilt)), (n.y * sin(tilt)) + (n.z * cos(tilt)));
            float ang = T * 0.25;
            n = vec3((n.x * cos(ang)) + (n.z * sin(ang)), n.y, ((-n.x) * sin(ang)) + (n.z * cos(ang)));
            float lat = asin(clamp(n.y, -1.0, 1.0));
            float lon = atan(n.x, n.z);
            vec2 g = vec2(lon, lat) / vec2(0.261799395084381103515625);
            vec2 w = fwidth(g);
            vec2 gl = vec2(1.0) - smoothstep(vec2(0.0), w * 1.5, abs(fract(g) - vec2(0.5)) - vec2(0.449999988079071044921875));
            float line = max(gl.x, gl.y);
            vec2 param = vec2(lon * 1.5, lat * 2.0) + vec2(3.0);
            vec2 param_1 = vec2(lon * 4.0, lat * 5.0);
            float land = smoothstep(0.5, 0.560000002384185791015625, (_noise(param) * 0.64999997615814208984375) + (_noise(param_1) * 0.3499999940395355224609375));
            float k = (side == 0) ? 1.0 : 0.300000011920928955078125;
            col += ((((_352.c3.xyz * line) * 0.449999988079071044921875) * k) + (((_352.c2.xyz * land) * 0.300000011920928955078125) * k));
            if (side == 0)
            {
                col += (_352.c2.xyz * 0.07999999821186065673828125);
            }
        }
        col *= (0.85000002384185791015625 + (0.1500000059604644775390625 * step(0.5, fract(((q.y * _352.size.y) / 4.0) + (T * 2.0)))));
    }
    return col;
}

vec3 sDNA(vec2 q, float T)
{
    vec3 col = _352.c0.xyz + ((_352.c1.xyz * 0.20000000298023223876953125) * exp((-abs(q.y)) * 4.0));
    vec2 param = q;
    float param_1 = -0.25;
    vec2 p = rot(param, param_1);
    float ph = (p.x * 9.0) + (T * 0.89999997615814208984375);
    float A = 0.10999999940395355224609375;
    vec3 _4505;
    for (int s = 0; s < 2; s++)
    {
        float o = float(s) * 3.1415927410125732421875;
        float y = A * sin(ph + o);
        float z = cos(ph + o);
        float d = abs(p.y - y);
        float w = 0.008000000379979610443115234375 + (0.006000000052154064178466796875 * ((z * 0.5) + 0.5));
        if (s == 0)
        {
            _4505 = _352.c2.xyz;
        }
        else
        {
            _4505 = _352.c3.xyz;
        }
        vec3 sc = _4505;
        col = mix(col, sc * (0.449999988079071044921875 + (0.550000011920928955078125 * ((z * 0.5) + 0.5))), vec3(smoothstep(w, w - 0.0030000000260770320892333984375, d) * (0.5 + (0.5 * step(0.0, z)))));
        col += (((sc * 0.0040000001899898052215576171875) / vec3(d + 0.008000000379979610443115234375)) * 0.300000011920928955078125);
    }
    float rung = fract(ph / 1.0471975803375244140625);
    float rx = (floor(ph / 1.0471975803375244140625) + 0.5) * 1.0471975803375244140625;
    float xr = (rx - (T * 0.89999997615814208984375)) / 9.0;
    float y1 = A * sin(rx);
    float y2 = A * sin(rx + 3.1415927410125732421875);
    vec2 param_2 = p;
    vec2 param_3 = vec2(xr, y1);
    vec2 param_4 = vec2(xr, y2);
    float dr = sdSeg(param_2, param_3, param_4);
    vec3 rc = mix(_352.c2.xyz, _352.c3.xyz, vec3(smoothstep(y1, y2, p.y)));
    col = mix(col, rc * 0.699999988079071044921875, vec3(smoothstep(0.0040000001899898052215576171875, 0.001000000047497451305389404296875, dr) * 0.800000011920928955078125));
    vec2 param_5 = q + vec2(T * 0.00999999977648258209228515625, 0.0);
    float param_6 = 40.0;
    float param_7 = T;
    col += ((vec3(1.0) * stars(param_5, param_6, param_7)) * 0.300000011920928955078125);
    return col;
}

void main()
{
    AA = 0.001000000047497451305389404296875;
    vec2 uv = qt_TexCoord0;
    float asp = _352.size.x / max(_352.size.y, 1.0);
    vec2 q = vec2((uv.x - 0.5) * asp, 0.5 - uv.y);
    AA = 1.0 / max(_352.size.y, 1.0);
    float T = _352.time;
    float e = _352.opt.w;
    int m = int(_352.mode + 0.5);
    vec3 col;
    if (m == 0)
    {
        vec2 param = q;
        float param_1 = T;
        col = sWarp(param, param_1);
    }
    else
    {
        if (m == 1)
        {
            vec2 param_2 = q;
            float param_3 = T;
            vec3 _4682 = sTunnel(param_2, param_3);
            col = _4682;
        }
        else
        {
            if (m == 2)
            {
                vec2 param_4 = uv;
                float param_5 = T;
                col = sCode(param_4, param_5);
            }
            else
            {
                if (m == 3)
                {
                    vec2 param_6 = q;
                    float param_7 = T;
                    col = sPlasma(param_6, param_7);
                }
                else
                {
                    if (m == 4)
                    {
                        vec2 param_8 = q;
                        float param_9 = T;
                        col = sStorm(param_8, param_9);
                    }
                    else
                    {
                        if (m == 5)
                        {
                            vec2 param_10 = q;
                            float param_11 = T;
                            col = sFire(param_10, param_11);
                        }
                        else
                        {
                            if (m == 6)
                            {
                                vec2 param_12 = q;
                                float param_13 = T;
                                col = sFireworks(param_12, param_13);
                            }
                            else
                            {
                                if (m == 7)
                                {
                                    vec2 param_14 = q;
                                    float param_15 = T;
                                    col = sGalaxy(param_14, param_15);
                                }
                                else
                                {
                                    if (m == 8)
                                    {
                                        vec2 param_16 = q;
                                        float param_17 = T;
                                        col = sHole(param_16, param_17);
                                    }
                                    else
                                    {
                                        if (m == 9)
                                        {
                                            vec2 param_18 = q;
                                            float param_19 = T;
                                            col = sOcean(param_18, param_19);
                                        }
                                        else
                                        {
                                            if (m == 10)
                                            {
                                                vec2 param_20 = q;
                                                float param_21 = T;
                                                vec3 _4772 = sKaleido(param_20, param_21);
                                                col = _4772;
                                            }
                                            else
                                            {
                                                if (m == 11)
                                                {
                                                    vec2 param_22 = q;
                                                    float param_23 = T;
                                                    col = sPlexus(param_22, param_23);
                                                }
                                                else
                                                {
                                                    if (m == 12)
                                                    {
                                                        vec2 param_24 = q;
                                                        float param_25 = T;
                                                        float param_26 = e;
                                                        float param_27 = asp;
                                                        col = sEq(param_24, param_25, param_26, param_27);
                                                    }
                                                    else
                                                    {
                                                        if (m == 13)
                                                        {
                                                            vec2 param_28 = q;
                                                            float param_29 = T;
                                                            col = sOrbits(param_28, param_29);
                                                        }
                                                        else
                                                        {
                                                            if (m == 14)
                                                            {
                                                                vec2 param_30 = q;
                                                                float param_31 = T;
                                                                col = sBubbles(param_30, param_31);
                                                            }
                                                            else
                                                            {
                                                                if (m == 15)
                                                                {
                                                                    vec2 param_32 = q;
                                                                    float param_33 = T;
                                                                    col = sMeteors(param_32, param_33);
                                                                }
                                                                else
                                                                {
                                                                    if (m == 16)
                                                                    {
                                                                        vec2 param_34 = q;
                                                                        float param_35 = T;
                                                                        col = sMeadow(param_34, param_35);
                                                                    }
                                                                    else
                                                                    {
                                                                        if (m == 17)
                                                                        {
                                                                            vec2 param_36 = q;
                                                                            float param_37 = T;
                                                                            col = sHighway(param_36, param_37);
                                                                        }
                                                                        else
                                                                        {
                                                                            if (m == 18)
                                                                            {
                                                                                vec2 param_38 = q;
                                                                                float param_39 = T;
                                                                                col = sVortex(param_38, param_39);
                                                                            }
                                                                            else
                                                                            {
                                                                                if (m == 19)
                                                                                {
                                                                                    vec2 param_40 = q;
                                                                                    float param_41 = T;
                                                                                    col = sRibbons(param_40, param_41);
                                                                                }
                                                                                else
                                                                                {
                                                                                    if (m == 20)
                                                                                    {
                                                                                        vec2 param_42 = q;
                                                                                        float param_43 = T;
                                                                                        col = sGlobe(param_42, param_43);
                                                                                    }
                                                                                    else
                                                                                    {
                                                                                        vec2 param_44 = q;
                                                                                        float param_45 = T;
                                                                                        col = sDNA(param_44, param_45);
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    col *= (1.0 + (0.100000001490116119384765625 * e));
    vec2 p = vec2(q.x / asp, q.y);
    col *= mix(1.0, 1.0 - (1.2000000476837158203125 * dot(p, p)), _352.opt.y);
    col *= _352.opt.z;
    vec2 param_46 = gl_FragCoord.xy;
    vec2 param_47 = gl_FragCoord.xy + vec2(17.299999237060546875);
    col += vec3(((hash(param_46) + hash(param_47)) - 1.0) / 255.0);
    fragColor = vec4(max(col, vec3(0.0)), 1.0) * _352.qt_Opacity;
}

