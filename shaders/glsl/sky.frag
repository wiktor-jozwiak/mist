#version 150

struct buf
{
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float radius;
    vec2 size;
    vec4 sun;
    vec4 moon;
    vec4 moonLit;
    vec4 wx;
    vec4 wx2;
    vec4 fx;
    vec4 bolt;
    vec4 life;
    vec4 env;
    vec4 env2;
    vec4 env3;
};

uniform buf _278;

uniform sampler2D src;

in vec2 qt_TexCoord0;
out vec4 fragColor;
float HOR;

float hash(vec2 p)
{
    vec3 q = fract(p.xyx * 0.103100001811981201171875);
    q += vec3(dot(q, q.yzx + vec3(33.3300018310546875)));
    return fract((q.x + q.y) * q.z);
}

vec2 hash2(vec2 p)
{
    vec3 q = fract(p.xyx * vec3(0.103100001811981201171875, 0.10300000011920928955078125, 0.097300000488758087158203125));
    q += vec3(dot(q, q.yzx + vec3(33.3300018310546875)));
    return fract((q.xx + q.yz) * q.zy);
}

void glow(inout vec4 acc, vec3 c)
{
    acc += vec4(c, max(c.x, max(c.y, c.z)));
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

void over(inout vec4 acc, vec3 c, float a)
{
    acc = (acc * (1.0 - a)) + vec4(c * a, a);
}

float fbm(inout vec2 p, int oct)
{
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++)
    {
        if (i >= oct)
        {
            break;
        }
        vec2 param = p;
        v += (a * _noise(param));
        p = ((mat2(vec2(0.800000011920928955078125, -0.60000002384185791015625), vec2(0.60000002384185791015625, 0.800000011920928955078125)) * p) * 2.019999980926513671875) + vec2(17.1000003814697265625);
        a *= 0.5;
    }
    return v;
}

vec3 capLum(vec3 c, float cap)
{
    float l = dot(c, vec3(0.2125999927520751953125, 0.715200006961822509765625, 0.072200000286102294921875));
    vec3 _247;
    if (l > (cap * 0.75))
    {
        _247 = (c * ((cap * 0.75) + ((l - (cap * 0.75)) * 0.300000011920928955078125))) / vec3(l);
    }
    else
    {
        _247 = c;
    }
    return _247;
}

void main()
{
    HOR = 0.86000001430511474609375;
    vec2 uv = qt_TexCoord0;
    vec2 px = uv * _278.size;
    float H = _278.size.y;
    float t = _278.time;
    float cov = _278.wx.x;
    float rain = _278.wx.y;
    float snow = _278.wx.z;
    float fog = _278.wx.w;
    float storm = _278.wx2.x;
    float hail = _278.wx2.y;
    float drizzle = _278.wx2.z;
    float windS = _278.wx2.w;
    float s = _278.sun.z;
    float slant = _278.fx.x;
    float dayK = smoothstep(-6.0, 7.0, s);
    float nightK = 1.0 - smoothstep(-16.0, -5.0, s);
    float moonLight = (_278.moon.w * smoothstep(-1.0, 8.0, _278.moon.z)) * nightK;
    float gloom = smoothstep(0.60000002384185791015625, 1.0, cov);
    if (_278.env2.w > 0.0)
    {
        HOR = _278.env2.w;
    }
    vec2 suv = uv;
    if (_278.env.w > 0.0)
    {
        float band = smoothstep(HOR - 0.300000011920928955078125, HOR - 0.0199999995529651641845703125, uv.y) * (1.0 - smoothstep(HOR + 0.0199999995529651641845703125, HOR + 0.119999997317790985107421875, uv.y));
        suv += (((vec2(sin((uv.y * 150.0) + (t * 7.0)), sin(((uv.x * 90.0) - (t * 5.0)) + (uv.y * 70.0))) * vec2(0.00179999996908009052276611328125, 0.00260000000707805156707763671875)) * _278.env.w) * band);
    }
    vec4 A = texture(src, suv);
    vec3 skyC = A.xyz;
    float occ = A.w;
    vec4 acc = vec4(skyC * 0.949999988079071044921875, 0.949999988079071044921875);
    float open = 1.0 - occ;
    float starVis = (((nightK * (1.0 - (0.60000002384185791015625 * moonLight))) * (1.0 - _278.fx.w)) * (1.0 - fog)) * open;
    if (starVis > 0.00999999977648258209228515625)
    {
        vec2 cell = floor(px / vec2(9.0));
        vec2 param = cell;
        float h = hash(param);
        if (h > 0.800000011920928955078125)
        {
            vec2 param_1 = cell;
            vec2 l = px - (((cell + vec2(0.20000000298023223876953125)) + (hash2(param_1) * 0.60000002384185791015625)) * 9.0);
            vec2 param_2 = cell + vec2(7.30000019073486328125);
            float mag = pow(hash(param_2), 3.0);
            float tw = 0.699999988079071044921875 + ((0.300000011920928955078125 * sin((t * (2.0 + (5.0 * h))) + (h * 50.0))) * (0.4000000059604644775390625 + (0.60000002384185791015625 * uv.y)));
            float vis = smoothstep(0.550000011920928955078125 * moonLight, (0.550000011920928955078125 * moonLight) + 0.20000000298023223876953125, mag + 0.1500000059604644775390625);
            float b = ((((0.300000011920928955078125 + (0.699999988079071044921875 * mag)) * tw) * vis) * smoothstep(0.75 + mag, 0.0, length(l))) * (1.0 - smoothstep(HOR - 0.23999999463558197021484375, HOR, uv.y));
            vec2 param_3 = cell + vec2(3.099999904632568359375);
            vec4 param_4 = acc;
            vec3 param_5 = (mix(vec3(0.7799999713897705078125, 0.85000002384185791015625, 1.0), vec3(1.0, 0.89999997615814208984375, 0.7799999713897705078125), vec3(hash(param_3))) * b) * starVis;
            glow(param_4, param_5);
            acc = param_4;
        }
    }
    if ((_278.fx.z > 0.0) && (starVis > 0.0500000007450580596923828125))
    {
        float slot = floor(t / 6.0);
        vec2 param_6 = vec2(slot, 1.2999999523162841796875);
        if (hash(param_6) < _278.fx.z)
        {
            vec2 param_7 = vec2(slot, 2.099999904632568359375);
            float lt = (t - (slot * 6.0)) - (hash(param_7) * 4.5);
            if ((lt > 0.0) && (lt < 0.89999997615814208984375))
            {
                vec2 param_8 = vec2(slot, 3.7000000476837158203125);
                vec2 param_9 = vec2(slot, 4.900000095367431640625);
                vec2 a0 = vec2(0.100000001490116119384765625 + (0.800000011920928955078125 * hash(param_8)), 0.0500000007450580596923828125 + (0.300000011920928955078125 * hash(param_9))) * _278.size;
                vec2 param_10 = vec2(slot, 5.30000019073486328125);
                vec2 param_11 = vec2(slot, 6.099999904632568359375);
                float ang = mix(0.3499999940395355224609375, 1.10000002384185791015625, hash(param_10)) * ((hash(param_11) < 0.5) ? (-1.0) : 1.0);
                vec2 dir = vec2(sin(ang), cos(ang) * 0.60000002384185791015625);
                vec2 head = a0 + ((dir * lt) * 140.0);
                float along = -dot(px - head, dir);
                float perp = length((px - head) + (dir * along));
                float tail = min(lt, 0.300000011920928955078125) * 140.0;
                float _700;
                if ((along > 0.0) && (along < tail))
                {
                    _700 = 1.0 - (along / tail);
                }
                else
                {
                    _700 = 0.0;
                }
                float k = _700;
                vec4 param_12 = acc;
                vec3 param_13 = ((((vec3(0.89999997615814208984375, 0.949999988079071044921875, 1.0) * k) * k) * smoothstep(1.2999999523162841796875, 0.0, perp)) * sin((3.1415998935699462890625 * lt) / 0.89999997615814208984375)) * starVis;
                glow(param_12, param_13);
                acc = param_12;
            }
        }
    }
    if (_278.moon.z > (-1.5))
    {
        float R = 13.0;
        vec2 q = (px - (_278.moon.xy * _278.size)) / vec2(R);
        float r = length(q);
        float above = smoothstep(-1.5, 0.5, _278.moon.z);
        if (r > 1.0)
        {
            vec4 param_14 = acc;
            vec3 param_15 = (((((vec3(0.7200000286102294921875, 0.800000011920928955078125, 1.0) * ((exp((-(r - 1.0)) * 0.60000002384185791015625) * 0.100000001490116119384765625) + (exp((-(r - 1.0)) * 2.5) * 0.100000001490116119384765625))) * _278.moon.w) * nightK) * above) * (1.0 - (0.60000002384185791015625 * occ))) * (1.0 - gloom);
            glow(param_14, param_15);
            acc = param_14;
        }
        else
        {
            vec3 n = vec3(q, sqrt(1.0 - (r * r)));
            float lit = smoothstep(-0.0599999986588954925537109375, 0.0599999986588954925537109375, dot(n, _278.moonLit.xyz));
            vec2 nd = vec2(sin(_278.moonLit.w), -cos(_278.moonLit.w));
            vec2 tq = vec2(dot(q, vec2(-nd.y, nd.x)), -dot(q, nd));
            float mare = (((((smoothstep(0.4199999868869781494140625, 0.0, length((tq - vec2(-0.300000011920928955078125, -0.36000001430511474609375)) * vec2(1.0, 1.2000000476837158203125))) + smoothstep(0.2599999904632568359375, 0.0, length(tq - vec2(0.20000000298023223876953125, -0.3400000035762786865234375)))) + smoothstep(0.300000011920928955078125, 0.0, length((tq - vec2(0.3300000131130218505859375, -0.039999999105930328369140625)) * vec2(1.0, 1.2999999523162841796875)))) + smoothstep(0.180000007152557373046875, 0.0, length(tq - vec2(0.680000007152557373046875, -0.2599999904632568359375)))) + smoothstep(0.2199999988079071044921875, 0.0, length(tq - vec2(0.540000021457672119140625, 0.180000007152557373046875)))) + smoothstep(0.550000011920928955078125, 0.0, length((tq - vec2(-0.62000000476837158203125, 0.0)) * vec2(1.39999997615814208984375, 0.800000011920928955078125)))) + smoothstep(0.2599999904632568359375, 0.0, length(tq - vec2(-0.14000000059604644775390625, 0.36000001430511474609375)));
            vec2 param_16 = tq * 5.0;
            mare = min(mare, 1.0) * (0.75 + (0.5 * _noise(param_16)));
            vec2 param_17 = tq * 11.0;
            float alb = (((0.930000007152557373046875 - (0.2199999988079071044921875 * mare)) + (0.0500000007450580596923828125 * (_noise(param_17) - 0.5))) + (0.25 * smoothstep(0.070000000298023223876953125, 0.0, length(tq - vec2(-0.119999997317790985107421875, 0.800000011920928955078125))))) * (0.85000002384185791015625 + (0.1500000059604644775390625 * n.z));
            vec3 litC = (mix(vec3(0.980000019073486328125, 0.959999978542327880859375, 0.89999997615814208984375), vec3(0.930000007152557373046875, 0.949999988079071044921875, 1.0), vec3(dayK)) * alb) * mix(vec3(1.0, 0.7200000286102294921875, 0.4799999892711639404296875), vec3(1.0), vec3(smoothstep(2.0, 16.0, _278.moon.z) + dayK));
            float earth = 0.25 + (0.75 * pow(1.0 - _278.moon.w, 3.0));
            vec3 darkC = mix(skyC, vec3(0.2800000011920928955078125, 0.319999992847442626953125, 0.4199999868869781494140625) * alb, vec3(earth * 0.5));
            float aLit = mix(1.0, 0.550000011920928955078125 * smoothstep(0.07999999821186065673828125, 0.3499999940395355224609375, _278.moon.w), dayK);
            float aDark = 0.75 * (1.0 - dayK);
            float a = ((((clamp(((1.0 - r) * R) + 0.5, 0.0, 1.0) * mix(aDark, aLit, lit)) * above) * (1.0 - occ)) * (1.0 - occ)) * (1.0 - gloom);
            vec4 param_18 = acc;
            vec3 param_19 = mix(darkC, litC, vec3(lit));
            float param_20 = a;
            over(param_18, param_19, param_20);
            acc = param_18;
        }
    }
    float gx = px.x / H;
    vec2 param_21 = vec2(gx * 1.60000002384185791015625, 3.0);
    vec2 param_22 = vec2(gx * 6.0, 1.0);
    float ridge = ((HOR - 0.00999999977648258209228515625) - (0.02999999932944774627685546875 * _noise(param_21))) - (0.01200000010430812835693359375 * _noise(param_22));
    float sway = (0.1500000059604644775390625 + windS) * 0.0500000007450580596923828125;
    vec2 param_23 = vec2((gx * 26.0) + (sin((t * 1.60000002384185791015625) + (gx * 5.0)) * sway), 5.0);
    vec2 param_24 = vec2((gx * 61.0) + ((sin((t * 2.2999999523162841796875) + (gx * 9.0)) * sway) * 1.39999997615814208984375), 9.0);
    float crowns = (pow(_noise(param_23), 1.60000002384185791015625) * 0.02999999932944774627685546875) + (pow(_noise(param_24), 2.0) * 0.01200000010430812835693359375);
    float treeTop = ridge - crowns;
    float treeMask = smoothstep(treeTop - (0.699999988079071044921875 / H), treeTop + (0.699999988079071044921875 / H), uv.y);
    float gSnow = _278.env2.x;
    vec3 treeC = mix(vec3(0.014999999664723873138427734375, 0.0199999995529651641845703125, 0.02999999932944774627685546875), mix(skyC * 0.4199999868869781494140625, vec3(0.100000001490116119384765625, 0.12999999523162841796875, 0.119999997317790985107421875), vec3(0.5)), vec3(dayK));
    vec2 param_25 = px * 0.4000000059604644775390625;
    treeC = mix(treeC, mix(vec3(0.20000000298023223876953125, 0.2199999988079071044921875, 0.2599999904632568359375), vec3(0.699999988079071044921875, 0.7400000095367431640625, 0.800000011920928955078125), vec3(dayK)), vec3((gSnow * 0.449999988079071044921875) * smoothstep(0.3499999940395355224609375, 0.699999988079071044921875, _noise(param_25))));
    treeC = mix(treeC, skyC, vec3(0.25 + (0.5 * _278.fx.w)));
    vec4 param_26 = acc;
    vec3 param_27 = treeC;
    float param_28 = treeMask * (1.0 - (0.75 * fog));
    over(param_26, param_27, param_28);
    acc = param_26;
    if (uv.y > ridge)
    {
        float k_1 = clamp((uv.y - ridge) / max(0.0500000007450580596923828125, 1.0 - ridge), 0.0, 1.0);
        vec2 param_29 = vec2(gx * 40.0, (uv.y * 120.0) / (0.300000011920928955078125 + k_1));
        vec3 field = mix(vec3(0.02999999932944774627685546875, 0.039999999105930328369140625, 0.0350000001490116119384765625), vec3(0.17000000178813934326171875, 0.2199999988079071044921875, 0.1599999964237213134765625), vec3(dayK)) * (0.800000011920928955078125 + (0.4000000059604644775390625 * _noise(param_29)));
        vec2 param_30 = px * 0.300000011920928955078125;
        vec3 snowF = mix(vec3(0.17000000178813934326171875, 0.189999997615814208984375, 0.23999999463558197021484375), vec3(0.86000001430511474609375, 0.88999998569488525390625, 0.939999997615814208984375), vec3(dayK)) * (0.920000016689300537109375 + (0.07999999821186065673828125 * _noise(param_30)));
        field = mix(field, snowF, vec3(gSnow));
        field += (((skyC * 0.300000011920928955078125) * rain) * (1.0 - gSnow));
        vec4 param_31 = acc;
        vec3 param_32 = mix(skyC, field, vec3(smoothstep(0.0, 0.25, k_1)));
        float param_33 = (smoothstep(ridge, ridge + 0.014999999664723873138427734375, uv.y) * 0.89999997615814208984375) * (1.0 - (0.60000002384185791015625 * fog));
        over(param_31, param_32, param_33);
        acc = param_31;
    }
    if (fog > 0.0)
    {
        vec2 p = vec2((uv.x * _278.size.x) / H, uv.y);
        vec2 param_34 = vec2((p.x * 2.2000000476837158203125) - (t * 0.02500000037252902984619140625), (uv.y * 6.0) + (t * 0.00999999977648258209228515625));
        int param_35 = 4;
        float _1296 = fbm(param_34, param_35);
        float f = _1296;
        vec2 param_36 = vec2((p.x * 1.2999999523162841796875) + (t * 0.014999999664723873138427734375), (uv.y * 3.0) + 5.0);
        int param_37 = 3;
        float _1312 = fbm(param_36, param_37);
        float f2 = _1312;
        float dens = (fog * (0.25 + (0.75 * smoothstep(0.1500000059604644775390625, 0.89999997615814208984375, uv.y)))) * (0.449999988079071044921875 + ((1.39999997615814208984375 * f) * f2));
        vec4 param_38 = acc;
        vec3 param_39 = mix(vec3(0.20000000298023223876953125, 0.20999999344348907470703125, 0.23999999463558197021484375), vec3(0.62000000476837158203125, 0.64999997615814208984375, 0.699999988079071044921875), vec3(max(dayK, 0.100000001490116119384765625)));
        float param_40 = clamp(dens * 0.75, 0.0, 0.800000011920928955078125);
        over(param_38, param_39, param_40);
        acc = param_38;
    }
    if (_278.fx.w > 0.0)
    {
        vec4 param_41 = acc;
        vec3 param_42 = mix(vec3(0.25, 0.2700000107288360595703125, 0.300000011920928955078125), vec3(0.579999983310699462890625, 0.61000001430511474609375, 0.660000026226043701171875), vec3(dayK));
        float param_43 = _278.fx.w * 0.300000011920928955078125;
        over(param_41, param_42, param_43);
        acc = param_41;
    }
    if (_278.env3.z > 0.0)
    {
        float dirc = (slant >= 0.0) ? 1.0 : (-1.0);
        vec2 param_44 = vec2((((px.x - ((px.y * slant) * 1.2000000476837158203125)) / H) * 1.39999997615814208984375) - ((dirc * t) * 0.300000011920928955078125), t * 0.039999999105930328369140625);
        int param_45 = 3;
        float _1400 = fbm(param_44, param_45);
        float band_1 = _1400;
        vec4 param_46 = acc;
        vec3 param_47 = mix(vec3(0.300000011920928955078125, 0.3300000131130218505859375, 0.38999998569488525390625), vec3(0.62000000476837158203125, 0.660000026226043701171875, 0.730000019073486328125), vec3(dayK));
        float param_48 = ((smoothstep(0.4199999868869781494140625, 0.7799999713897705078125, band_1) * 0.23999999463558197021484375) * _278.env3.z) * smoothstep(0.0500000007450580596923828125, 0.449999988079071044921875, uv.y);
        over(param_46, param_47, param_48);
        acc = param_46;
    }
    vec3 dropC = mix(vec3(0.519999980926513671875, 0.579999983310699462890625, 0.699999988079071044921875), vec3(0.800000011920928955078125, 0.86000001430511474609375, 0.959999978542327880859375), vec3(max(dayK, 0.25)));
    if (rain > 0.0)
    {
        for (int i = 0; i < 3; i++)
        {
            float fi = float(i) / 2.0;
            float cw = mix(5.0, 11.0, fi);
            float speed = mix(1.5, 2.5, fi) * mix(1.0, 0.449999988079071044921875, drizzle);
            float len = (mix(0.0599999986588954925537109375, 0.12999999523162841796875, fi) * mix(1.0, 0.449999988079071044921875, drizzle)) * (0.800000011920928955078125 + (0.4000000059604644775390625 * rain));
            vec2 rp = vec2(px.x - (px.y * slant), uv.y);
            float col = floor(rp.x / cw);
            vec2 param_49 = vec2(col, fi * 13.0);
            float rnd = hash(param_49);
            float y = (rp.y - ((t * speed) * (0.85000002384185791015625 + (0.300000011920928955078125 * rnd)))) + (rnd * 7.0);
            float rows = 1.60000002384185791015625 + fi;
            float row = floor(y * rows);
            vec2 param_50 = vec2(col, row + (fi * 31.0));
            float on = step(hash(param_50), rain * (0.300000011920928955078125 + (0.25 * fi)));
            float ly = fract(y * rows) / rows;
            vec2 param_51 = vec2(row, col + fi);
            float dx = abs(mod(rp.x, cw) - ((0.20000000298023223876953125 + (0.60000002384185791015625 * hash(param_51))) * cw));
            float w = mix(0.449999988079071044921875, 0.85000002384185791015625, fi) * mix(1.0, 0.699999988079071044921875, drizzle);
            float body = smoothstep(0.0, len, ly) * float(ly < len);
            vec4 param_52 = acc;
            vec3 param_53 = dropC;
            float param_54 = (((on * smoothstep(w + 0.60000002384185791015625, w - 0.4000000059604644775390625, dx)) * body) * mix(0.100000001490116119384765625, 0.2599999904632568359375, fi)) * (1.0 - (0.60000002384185791015625 * fog));
            over(param_52, param_53, param_54);
            acc = param_52;
        }
    }
    if (hail > 0.0)
    {
        vec2 hp = vec2(px.x - ((px.y * slant) * 0.60000002384185791015625), px.y - (t * 300.0));
        vec2 cell_1 = floor(hp / vec2(22.0));
        vec2 param_55 = cell_1 + vec2(4.19999980926513671875);
        if (hash(param_55) < (0.3499999940395355224609375 * hail))
        {
            vec2 param_56 = cell_1;
            vec4 param_57 = acc;
            vec3 param_58 = vec3(0.920000016689300537109375, 0.949999988079071044921875, 1.0);
            float param_59 = smoothstep(2.0, 0.800000011920928955078125, length(hp - (((cell_1 + vec2(0.20000000298023223876953125)) + (hash2(param_56) * 0.60000002384185791015625)) * 22.0))) * 0.85000002384185791015625;
            over(param_57, param_58, param_59);
            acc = param_57;
        }
    }
    bool _1655 = rain > 0.0;
    bool _1663;
    if (_1655)
    {
        _1663 = uv.y > (ridge + 0.0199999995529651641845703125);
    }
    else
    {
        _1663 = _1655;
    }
    if (_1663)
    {
        float k_2 = clamp((uv.y - ridge) / max(0.0500000007450580596923828125, 1.0 - ridge), 0.0, 1.0);
        float cs = mix(10.0, 34.0, k_2);
        vec2 cell_2 = floor(vec2(px.x / cs, ((uv.y - ridge) * H) / (cs * 0.449999988079071044921875)));
        vec2 param_60 = cell_2 + vec2(61.0);
        float h_1 = hash(param_60);
        if (h_1 < (0.300000011920928955078125 * rain))
        {
            vec2 param_61 = cell_2 + vec2(3.0);
            float life = fract((t * (1.2000000476837158203125 + (0.800000011920928955078125 * hash(param_61)))) + (h_1 * 7.0));
            vec2 param_62 = cell_2 + vec2(5.0);
            vec2 c0 = vec2(((cell_2.x + 0.20000000298023223876953125) + (0.60000002384185791015625 * hash(param_62))) * cs, (ridge * H) + (((cell_2.y + 0.5) * cs) * 0.449999988079071044921875));
            vec2 d = (px - c0) * vec2(1.0, 3.2000000476837158203125);
            float r_1 = life * mix(2.5, 9.0, k_2);
            float ring = (smoothstep(1.2999999523162841796875, 0.0, abs(length(d) - r_1)) * (1.0 - life)) * (1.0 - life);
            vec4 param_63 = acc;
            vec3 param_64 = dropC;
            float param_65 = ((ring * 0.3499999940395355224609375) * min(1.0, rain * 1.5)) * (0.3499999940395355224609375 + (0.64999997615814208984375 * k_2));
            over(param_63, param_64, param_65);
            acc = param_63;
        }
    }
    if (snow > 0.0)
    {
        for (int i_1 = 0; i_1 < 3; i_1++)
        {
            float fi_1 = float(i_1) / 2.0;
            float cs_1 = mix(16.0, 46.0, fi_1);
            float fall = mix(14.0, 38.0, fi_1);
            vec2 sp = vec2(px.x - ((px.y * slant) * 0.800000011920928955078125), px.y - (t * fall));
            sp.x += (sin(((sp.y * 0.02999999932944774627685546875) + (t * 0.89999997615814208984375)) + (fi_1 * 4.0)) * (3.0 + (6.0 * fi_1)));
            vec2 cell_3 = floor(sp / vec2(cs_1));
            vec2 param_66 = cell_3 + vec2(fi_1 * 17.0);
            if (hash(param_66) < (snow * (0.550000011920928955078125 + (0.25 * fi_1))))
            {
                vec2 param_67 = cell_3 + vec2(fi_1 * 5.0);
                float d_1 = length(sp - (((cell_3 + vec2(0.20000000298023223876953125)) + (hash2(param_67) * 0.60000002384185791015625)) * cs_1));
                vec2 param_68 = cell_3 + vec2(9.0);
                float rr = mix(0.89999997615814208984375, 3.599999904632568359375, fi_1) * (0.699999988079071044921875 + (0.60000002384185791015625 * hash(param_68)));
                float soft = mix(0.60000002384185791015625, 3.0, fi_1);
                vec4 param_69 = acc;
                vec3 param_70 = mix(vec3(0.680000007152557373046875, 0.7200000286102294921875, 0.800000011920928955078125), vec3(0.9700000286102294921875, 0.980000019073486328125, 1.0), vec3(max(dayK, 0.3499999940395355224609375)));
                float param_71 = smoothstep(rr + soft, rr - (soft * 0.300000011920928955078125), d_1) * mix(0.75, 0.4199999868869781494140625, fi_1);
                over(param_69, param_70, param_71);
                acc = param_69;
            }
        }
    }
    bool _1935 = _278.bolt.x > 0.001000000047497451305389404296875;
    bool _1941;
    if (_1935)
    {
        _1941 = _278.bolt.z < 0.3499999940395355224609375;
    }
    else
    {
        _1941 = _1935;
    }
    if (_1941)
    {
        float y0 = 0.2199999988079071044921875;
        float y1 = HOR - 0.02999999932944774627685546875;
        bool _1951 = uv.y > y0;
        bool _1958;
        if (_1951)
        {
            _1958 = uv.y < y1;
        }
        else
        {
            _1958 = _1951;
        }
        if (_1958)
        {
            vec2 param_72 = vec2(uv.y * 7.0, _278.bolt.w);
            vec2 param_73 = vec2(uv.y * 30.0, _278.bolt.w + 3.0);
            float bx = (_278.bolt.y * _278.size.x) + ((((_noise(param_72) - 0.5) * 0.2199999988079071044921875) + ((_noise(param_73) - 0.5) * 0.0500000007450580596923828125)) * H);
            float d_2 = abs(px.x - bx);
            vec4 param_74 = acc;
            vec3 param_75 = ((vec3(0.920000016689300537109375, 0.939999997615814208984375, 1.0) * (smoothstep(1.5, 0.300000011920928955078125, d_2) + (exp((-d_2) / 7.0) * 0.3499999940395355224609375))) * min(_278.bolt.x, 1.0)) * smoothstep(y0, y0 + 0.0599999986588954925537109375, uv.y);
            glow(param_74, param_75);
            acc = param_74;
        }
    }
    if (_278.bolt.x > 0.001000000047497451305389404296875)
    {
        vec4 param_76 = acc;
        vec3 param_77 = (vec3(0.75, 0.7799999713897705078125, 0.949999988079071044921875) * _278.bolt.x) * 0.119999997317790985107421875;
        glow(param_76, param_77);
        acc = param_76;
    }
    if (windS > 0.0)
    {
        float dirx = (slant >= 0.0) ? 1.0 : (-1.0);
        vec2 wp = vec2(px.x - ((dirx * t) * 420.0), px.y);
        float lane = floor(wp.y / 9.0);
        vec2 param_78 = vec2(lane, 3.2999999523162841796875);
        float rnd_1 = hash(param_78);
        bool _2074 = rnd_1 < (windS * 0.3499999940395355224609375);
        bool _2080;
        if (_2074)
        {
            _2080 = uv.y > 0.25;
        }
        else
        {
            _2080 = _2074;
        }
        bool _2086;
        if (_2080)
        {
            _2086 = uv.y < 0.800000011920928955078125;
        }
        else
        {
            _2086 = _2080;
        }
        if (_2086)
        {
            float lx = fract((wp.x + (rnd_1 * 900.0)) / 700.0);
            vec2 param_79 = vec2(lane, floor((wp.x + (rnd_1 * 900.0)) / 700.0));
            float on_1 = step(hash(param_79), 0.5);
            vec4 param_80 = acc;
            vec3 param_81 = vec3(1.0);
            float param_82 = (((on_1 * smoothstep(0.0, 0.0599999986588954925537109375, lx)) * smoothstep(0.14000000059604644775390625, 0.0599999986588954925537109375, lx)) * smoothstep(1.2000000476837158203125, 0.0, abs(mod(wp.y, 9.0) - 4.5))) * 0.20000000298023223876953125;
            over(param_80, param_81, param_82);
            acc = param_80;
        }
    }
    bool _2136 = _278.env2.y > 0.0;
    bool _2144;
    if (_2136)
    {
        _2144 = uv.y > (HOR - 0.07999999821186065673828125);
    }
    else
    {
        _2144 = _2136;
    }
    if (_2144)
    {
        vec2 cell_4 = floor(px / vec2(38.0));
        vec2 param_83 = cell_4 + vec2(91.0);
        float h_2 = hash(param_83);
        if (h_2 < (0.3499999940395355224609375 * _278.env2.y))
        {
            vec2 c0_1 = ((cell_4 + vec2(0.5)) * 38.0) + (vec2(sin((t * 0.5) + (h_2 * 30.0)), cos((t * 0.430000007152557373046875) + (h_2 * 17.0))) * 12.0);
            float d_3 = length(px - c0_1);
            float blink = pow(max(0.0, sin((t * (0.89999997615814208984375 + (h_2 * 1.5))) + (h_2 * 40.0))), 5.0);
            vec4 param_84 = acc;
            vec3 param_85 = ((vec3(0.7200000286102294921875, 1.0, 0.37999999523162841796875) * ((exp(((-d_3) * d_3) / 2.5) * 0.89999997615814208984375) + (exp((-d_3) / 6.0) * 0.14000000059604644775390625))) * blink) * _278.env2.y;
            glow(param_84, param_85);
            acc = param_84;
        }
    }
    if (_278.env2.z > 0.0)
    {
        for (int i_2 = 0; i_2 < 2; i_2++)
        {
            float fi_2 = float(i_2);
            float cs_2 = mix(26.0, 44.0, fi_2);
            vec2 mp = px + vec2(((-t) * mix(5.0, 9.0, fi_2)) * ((slant >= 0.0) ? 1.0 : (-1.0)), t * mix(3.0, 6.0, fi_2));
            vec2 cell_5 = floor(mp / vec2(cs_2));
            vec2 param_86 = (cell_5 + vec2(fi_2 * 23.0)) + vec2(101.0);
            float h_3 = hash(param_86);
            if (h_3 < (0.300000011920928955078125 * _278.env2.z))
            {
                vec2 param_87 = cell_5 + vec2(fi_2);
                vec2 c0_2 = (((cell_5 + vec2(0.20000000298023223876953125)) + (hash2(param_87) * 0.60000002384185791015625)) * cs_2) + (vec2(sin((t * 0.699999988079071044921875) + (h_3 * 50.0)), cos((t * 0.60000002384185791015625) + (h_3 * 20.0))) * 4.0);
                float d_4 = length(mp - c0_2);
                float sunNear = exp((-length((px - (_278.sun.xy * _278.size)) / vec2(H))) * 2.2000000476837158203125);
                float tw_1 = 0.5 + (0.5 * sin((t * 3.0) + (h_3 * 60.0)));
                vec4 param_88 = acc;
                vec3 param_89 = ((((vec3(1.0, 0.939999997615814208984375, 0.7799999713897705078125) * exp(((-d_4) * d_4) / mix(0.60000002384185791015625, 1.39999997615814208984375, fi_2))) * (0.25 + (1.10000002384185791015625 * sunNear))) * tw_1) * _278.env2.z) * dayK;
                glow(param_88, param_89);
                acc = param_88;
            }
        }
    }
    if (_278.life.z > 0.0)
    {
        float slot_1 = floor(t / 11.0);
        vec2 param_90 = vec2(slot_1, 21.700000762939453125);
        if (hash(param_90) < _278.life.z)
        {
            float lt_1 = t - (slot_1 * 11.0);
            vec2 param_91 = vec2(slot_1, 22.299999237060546875);
            float dirb = (hash(param_91) < 0.5) ? (-1.0) : 1.0;
            vec2 param_92 = vec2(slot_1, 23.8999996185302734375);
            float spd = 34.0 + (22.0 * hash(param_92));
            vec2 param_93 = vec2(slot_1, 24.1000003814697265625);
            float gy = (0.14000000059604644775390625 + (0.300000011920928955078125 * hash(param_93))) * H;
            float _2436;
            if (dirb > 0.0)
            {
                _2436 = (-60.0) + (lt_1 * spd);
            }
            else
            {
                _2436 = (_278.size.x + 60.0) - (lt_1 * spd);
            }
            float cx = _2436;
            vec2 param_94 = vec2(slot_1, 25.299999237060546875);
            float sc = 0.800000011920928955078125 + (0.5 * hash(param_94));
            vec3 birdC = mix(vec3(0.02999999932944774627685546875, 0.039999999105930328369140625, 0.070000000298023223876953125), vec3(0.100000001490116119384765625, 0.10999999940395355224609375, 0.1500000059604644775390625), vec3(dayK));
            for (int i_3 = 0; i_3 < 5; i_3++)
            {
                float fi_3 = float(i_3);
                vec2 param_95 = vec2(slot_1, 30.0 + fi_3);
                if ((hash(param_95) > 0.75) && (i_3 > 1))
                {
                    continue;
                }
                float side = (mod(fi_3, 2.0) * 2.0) - 1.0;
                float rank = floor((fi_3 + 1.0) / 2.0);
                vec2 bc = vec2(cx - (((dirb * rank) * 13.0) * sc), (gy + (((side * rank) * 7.0) * sc)) + (3.0 * sin((t * 0.800000011920928955078125) + (fi_3 * 1.7000000476837158203125))));
                vec2 q_1 = (px - bc) / vec2(5.5 * sc);
                bool _2541 = abs(q_1.x) < 1.2000000476837158203125;
                bool _2548;
                if (_2541)
                {
                    _2548 = abs(q_1.y) < 1.2000000476837158203125;
                }
                else
                {
                    _2548 = _2541;
                }
                if (_2548)
                {
                    float flap = sin((t * (9.0 + fi_3)) + (fi_3 * 2.099999904632568359375));
                    float ax = abs(q_1.x);
                    float wy = (((-flap) * 0.550000011920928955078125) * ax) + ((0.3499999940395355224609375 * ax) * ax);
                    float d_5 = (abs(q_1.y - wy) * 5.5) * sc;
                    float a_1 = smoothstep(1.10000002384185791015625, 0.25, d_5) * smoothstep(1.0499999523162841796875, 0.85000002384185791015625, ax);
                    vec4 param_96 = acc;
                    vec3 param_97 = birdC;
                    float param_98 = (a_1 * 0.75) * (1.0 - fog);
                    over(param_96, param_97, param_98);
                    acc = param_96;
                }
            }
        }
    }
    if (_278.life.y > 0.0)
    {
        float dirx_1 = (slant >= 0.0) ? 1.0 : (-1.0);
        for (int i_4 = 0; i_4 < 2; i_4++)
        {
            float fi_4 = float(i_4);
            float cs_3 = mix(44.0, 70.0, fi_4);
            float spd_1 = mix(70.0, 150.0, fi_4) * (0.60000002384185791015625 + (0.60000002384185791015625 * _278.life.y));
            vec2 lp = vec2(px.x - ((dirx_1 * t) * spd_1), px.y - (t * mix(10.0, 22.0, fi_4)));
            vec2 cell_6 = floor(lp / vec2(cs_3));
            vec2 param_99 = cell_6 + vec2(fi_4 * 11.0);
            float h_4 = hash(param_99);
            if (h_4 < (_278.life.y * mix(0.2199999988079071044921875, 0.119999997317790985107421875, fi_4)))
            {
                vec2 param_100 = cell_6 + vec2(fi_4 * 3.0);
                vec2 c0_3 = ((cell_6 + vec2(0.25)) + (hash2(param_100) * 0.5)) * cs_3;
                c0_3.y += ((sin((t * (1.5 + (h_4 * 2.0))) + (h_4 * 40.0)) * cs_3) * 0.180000007152557373046875);
                vec2 param_101 = cell_6 + vec2(6.0);
                float ang_1 = ((t * (2.0 + (4.0 * hash(param_101)))) * ((h_4 < 0.0500000007450580596923828125) ? (-1.0) : 1.0)) + (h_4 * 30.0);
                vec2 q_2 = lp - c0_3;
                q_2 = mat2(vec2(cos(ang_1), -sin(ang_1)), vec2(sin(ang_1), cos(ang_1))) * q_2;
                vec2 param_102 = cell_6 + vec2(9.0);
                float rr_1 = mix(3.2000000476837158203125, 5.5, fi_4) * (0.75 + (0.5 * hash(param_102)));
                q_2.y /= (0.25 + (0.75 * abs(cos(ang_1 * 0.699999988079071044921875))));
                float u = q_2.x / rr_1;
                float wdt = (rr_1 * 0.4799999892711639404296875) * max(0.0, 1.0 - (u * u));
                float d_6 = abs(q_2.y) - wdt;
                float a_2 = smoothstep(0.89999997615814208984375, -0.300000011920928955078125, d_6) * step(abs(u), 1.0);
                vec2 param_103 = cell_6 + vec2(13.0);
                vec3 lc = mix(vec3(0.86000001430511474609375, 0.4199999868869781494140625, 0.119999997317790985107421875), vec3(0.7200000286102294921875, 0.180000007152557373046875, 0.100000001490116119384765625), vec3(hash(param_103)));
                vec2 param_104 = cell_6 + vec2(15.0);
                lc = mix(lc, vec3(0.87999999523162841796875, 0.699999988079071044921875, 0.2199999988079071044921875), vec3(step(0.660000026226043701171875, hash(param_104))));
                lc *= (mix(0.449999988079071044921875, 1.0, dayK) * (0.75 + (0.3499999940395355224609375 * smoothstep(-rr_1, rr_1, q_2.y))));
                vec4 param_105 = acc;
                vec3 param_106 = lc;
                float param_107 = (a_2 * mix(0.89999997615814208984375, 0.800000011920928955078125, fi_4)) * (1.0 - (0.5 * fog));
                over(param_105, param_106, param_107);
                acc = param_105;
            }
        }
    }
    if ((rain > 0.0) || (storm > 0.0))
    {
        float wetK = max(rain, storm * 0.800000011920928955078125) * (1.0 - (drizzle * 0.5));
        vec2 cell_7 = floor(px / vec2(24.0));
        vec2 param_108 = cell_7 + vec2(41.0);
        float h_5 = hash(param_108);
        vec2 param_109 = cell_7 + vec2(2.0);
        float life_1 = fract((t * (0.039999999105930328369140625 + (0.0500000007450580596923828125 * hash(param_109)))) + h_5);
        if (h_5 < (0.1599999964237213134765625 * wetK))
        {
            vec2 param_110 = cell_7 + vec2(5.0);
            vec2 c0_4 = ((cell_7 + vec2(0.25)) + (hash2(param_110) * 0.5)) * 24.0;
            vec2 param_111 = cell_7 + vec2(8.0);
            float rr_2 = 2.400000095367431640625 + (3.400000095367431640625 * hash(param_111));
            vec2 l_1 = (px - c0_4) / vec2(rr_2, rr_2 * 1.12000000476837158203125);
            float r_2 = length(l_1);
            float a_3 = (smoothstep(1.0, 0.85000002384185791015625, r_2) * smoothstep(0.0, 0.100000001490116119384765625, life_1)) * smoothstep(1.0, 0.699999988079071044921875, life_1);
            if (a_3 > 0.0)
            {
                vec3 dc = mix((skyC * 1.0499999523162841796875) + vec3(0.02999999932944774627685546875), (skyC * 1.4500000476837158203125) + vec3(0.100000001490116119384765625), vec3(smoothstep(-0.60000002384185791015625, 0.800000011920928955078125, l_1.y)));
                dc += (vec3(0.85000002384185791015625) * smoothstep(0.300000011920928955078125, 0.0, length(l_1 - vec2(-0.3499999940395355224609375, -0.4199999868869781494140625))));
                vec3 param_112 = dc;
                float param_113 = 0.75;
                vec4 param_114 = acc;
                vec3 param_115 = capLum(param_112, param_113);
                float param_116 = a_3 * 0.550000011920928955078125;
                over(param_114, param_115, param_116);
                acc = param_114;
                vec4 param_117 = acc;
                vec3 param_118 = skyC * 0.3499999940395355224609375;
                float param_119 = (smoothstep(0.7200000286102294921875, 0.980000019073486328125, r_2) * a_3) * 0.449999988079071044921875;
                over(param_117, param_118, param_119);
                acc = param_117;
            }
        }
        float colx = floor(px.x / 46.0);
        vec2 param_120 = vec2(colx, 77.0);
        float hc = hash(param_120);
        if (hc < (0.2199999988079071044921875 * wetK))
        {
            vec2 param_121 = vec2(colx, 3.0);
            float sx = (((colx + 0.300000011920928955078125) + (0.4000000059604644775390625 * hash(param_121))) * 46.0) + (sin((px.y * 0.070000000298023223876953125) + colx) * 1.5);
            float sy = mod((t * (14.0 + (20.0 * hc))) + (hc * 500.0), H * 1.60000002384185791015625) - (H * 0.300000011920928955078125);
            vec2 l_2 = (px - vec2(sx, sy)) / vec2(2.7999999523162841796875, 3.400000095367431640625);
            float r_3 = length(l_2);
            if (r_3 < 1.0)
            {
                vec3 dc_1 = mix((skyC * 1.0499999523162841796875) + vec3(0.02999999932944774627685546875), (skyC * 1.4500000476837158203125) + vec3(0.100000001490116119384765625), vec3(smoothstep(-0.60000002384185791015625, 0.800000011920928955078125, l_2.y))) + (vec3(0.85000002384185791015625) * smoothstep(0.300000011920928955078125, 0.0, length(l_2 - vec2(-0.3499999940395355224609375, -0.4199999868869781494140625))));
                vec3 param_122 = dc_1;
                float param_123 = 0.75;
                vec4 param_124 = acc;
                vec3 param_125 = capLum(param_122, param_123);
                float param_126 = smoothstep(1.0, 0.85000002384185791015625, r_3) * 0.60000002384185791015625;
                over(param_124, param_125, param_126);
                acc = param_124;
                vec4 param_127 = acc;
                vec3 param_128 = skyC * 0.3499999940395355224609375;
                float param_129 = smoothstep(0.7200000286102294921875, 0.980000019073486328125, r_3) * 0.4000000059604644775390625;
                over(param_127, param_128, param_129);
                acc = param_127;
            }
            bool _3104 = px.y < sy;
            bool _3112;
            if (_3104)
            {
                _3112 = px.y > (sy - 46.0);
            }
            else
            {
                _3112 = _3104;
            }
            float _3113;
            if (_3112)
            {
                _3113 = 1.0 - ((sy - px.y) / 46.0);
            }
            else
            {
                _3113 = 0.0;
            }
            float trail = _3113;
            vec4 param_130 = acc;
            vec3 param_131 = (skyC * 1.2000000476837158203125) + vec3(0.0500000007450580596923828125);
            float param_132 = (smoothstep(1.2000000476837158203125, 0.20000000298023223876953125, abs(px.x - sx)) * trail) * 0.2199999988079071044921875;
            over(param_130, param_131, param_132);
            acc = param_130;
        }
    }
    if (_278.env3.w > 0.0)
    {
        vec2 cell_8 = floor(px / vec2(30.0));
        vec2 param_133 = cell_8 + vec2(131.0);
        float h_6 = hash(param_133);
        vec2 param_134 = cell_8 + vec2(4.0);
        float life_2 = fract((t * (0.0500000007450580596923828125 + (0.0500000007450580596923828125 * hash(param_134)))) + h_6);
        if (h_6 < (0.2199999988079071044921875 * _278.env3.w))
        {
            vec2 param_135 = cell_8 + vec2(7.0);
            vec2 c0_5 = ((cell_8 + vec2(0.25)) + (hash2(param_135) * 0.5)) * 30.0;
            vec2 q_3 = px - c0_5;
            vec2 param_136 = cell_8 + vec2(9.0);
            float rr_3 = (2.5 + (2.5 * hash(param_136))) * (1.0 - (0.5 * smoothstep(0.60000002384185791015625, 1.0, life_2)));
            float a_4 = atan(q_3.y, q_3.x) + (h_6 * 6.0);
            float arms = pow(abs(cos(3.0 * a_4)), 10.0);
            float r_4 = length(q_3);
            float f_1 = (smoothstep((rr_3 * (0.300000011920928955078125 + (0.699999988079071044921875 * arms))) + 0.60000002384185791015625, (rr_3 * (0.300000011920928955078125 + (0.699999988079071044921875 * arms))) - 0.4000000059604644775390625, r_4) * smoothstep(0.0, 0.07999999821186065673828125, life_2)) * smoothstep(1.0, 0.64999997615814208984375, life_2);
            vec4 param_137 = acc;
            vec3 param_138 = vec3(0.939999997615814208984375, 0.9700000286102294921875, 1.0);
            float param_139 = f_1 * 0.800000011920928955078125;
            over(param_137, param_138, param_139);
            acc = param_137;
        }
    }
    float zone = sin(clamp((uv.y - 0.0199999995529651641845703125) / 0.680000007152557373046875, 0.0, 1.0) * 3.1415998935699462890625) * (0.75 + ((0.25 * abs(uv.x - 0.5)) * 2.0));
    vec4 param_140 = acc;
    vec3 param_141 = vec3(0.0199999995529651641845703125, 0.02999999932944774627685546875, 0.070000000298023223876953125);
    float param_142 = (0.02999999932944774627685546875 + (0.039999999105930328369140625 * dayK)) + ((0.1599999964237213134765625 * zone) * dayK);
    over(param_140, param_141, param_142);
    acc = param_140;
    float sunUp = smoothstep(-1.2000000476837158203125, 0.5, s);
    if (sunUp > 0.0)
    {
        vec3 sunCol = mix(vec3(1.0, 0.4799999892711639404296875, 0.2199999988079071044921875), vec3(1.0, 0.949999988079071044921875, 0.87000000476837158203125), vec3(smoothstep(0.0, 20.0, s)));
        vec2 d_7 = px - (_278.sun.xy * _278.size);
        d_7.y /= mix(0.839999973773956298828125, 1.0, smoothstep(0.0, 4.0, s));
        float r_5 = length(d_7);
        float clear = ((1.0 - (0.949999988079071044921875 * occ)) * (1.0 - (0.800000011920928955078125 * gloom))) * (1.0 - treeMask);
        vec4 param_143 = acc;
        vec3 param_144 = ((sunCol * ((exp((-r_5) / 6.0) * 0.449999988079071044921875) + ((exp((-abs(d_7.y)) / 1.39999997615814208984375) * exp((-abs(d_7.x)) / 55.0)) * 0.14000000059604644775390625))) * clear) * sunUp;
        glow(param_143, param_144);
        acc = param_143;
        vec4 param_145 = acc;
        vec3 param_146 = mix(vec3(1.0, 0.9900000095367431640625, 0.959999978542327880859375), sunCol, vec3(smoothstep(3.0, 10.0, r_5)));
        float param_147 = (clamp((10.0 - r_5) + 0.5, 0.0, 1.0) * sunUp) * clear;
        over(param_145, param_146, param_147);
        acc = param_145;
    }
    if ((_278.life.w > 0.0) && (sunUp > 0.0))
    {
        vec2 toS = _278.sun.xy - uv;
        float light = 0.0;
        float w_1 = 1.0;
        float wsum = 0.0;
        vec2 param_148 = px + vec2(fract(t));
        float j = hash(param_148);
        for (int i_5 = 0; i_5 < 14; i_5++)
        {
            vec2 sp_1 = uv + (toS * ((float(i_5) + j) / 14.0));
            float o = texture(src, clamp(sp_1, vec2(0.0), vec2(1.0))).w;
            light += ((1.0 - o) * w_1);
            wsum += w_1;
            w_1 *= 0.930000007152557373046875;
        }
        light /= wsum;
        float shaft = max(0.0, light - ((1.0 - occ) * 0.550000011920928955078125)) * occ;
        float fall_1 = exp((-length(toS * vec2(_278.size.x / H, 1.0))) * 1.60000002384185791015625);
        float flick = 0.85000002384185791015625 + (0.1500000059604644775390625 * sin((t * 0.60000002384185791015625) + (uv.x * 3.0)));
        vec3 sunCol_1 = mix(vec3(1.0, 0.62000000476837158203125, 0.319999992847442626953125), vec3(1.0, 0.949999988079071044921875, 0.85000002384185791015625), vec3(smoothstep(0.0, 20.0, s)));
        vec4 param_149 = acc;
        vec3 param_150 = ((((sunCol_1 * ((shaft * 0.550000011920928955078125) + (light * 0.100000001490116119384765625))) * fall_1) * flick) * _278.life.w) * sunUp;
        glow(param_149, param_150);
        acc = param_149;
    }
    if (_278.env3.x > 0.0)
    {
        bool bySun = s > 0.0;
        vec2 _3524;
        if (bySun)
        {
            _3524 = _278.sun.xy;
        }
        else
        {
            _3524 = _278.moon.xy;
        }
        vec2 c0_6 = _3524 * _278.size;
        float R_1 = (H * 0.4040000140666961669921875) / 1.7999999523162841796875;
        float d_8 = length(px - c0_6);
        float ring_1 = exp(-pow((d_8 - R_1) / 2.599999904632568359375, 2.0));
        float inner = exp(-pow(((d_8 - R_1) + 2.5) / 1.39999997615814208984375, 2.0));
        float _3571;
        if (bySun)
        {
            _3571 = 1.0;
        }
        else
        {
            _3571 = 0.60000002384185791015625 * _278.moon.w;
        }
        float k_3 = _278.env3.x * _3571;
        vec4 param_151 = acc;
        vec3 param_152 = (((vec3(0.949999988079071044921875, 0.959999978542327880859375, 1.0) * ring_1) * 0.14000000059604644775390625) + ((vec3(0.300000011920928955078125, 0.119999997317790985107421875, 0.0199999995529651641845703125) * inner) * 0.119999997317790985107421875)) * k_3;
        glow(param_151, param_152);
        acc = param_151;
    }
    vec2 bs = vec2(_278.size.x, H * _278.life.x);
    vec2 hs0 = bs * 0.5;
    vec2 qd0 = (abs(px - hs0) - hs0) + vec2(_278.radius);
    float edge = -((length(max(qd0, vec2(0.0))) + min(max(qd0.x, qd0.y), 0.0)) - _278.radius);
    if (_278.env.z > 0.0)
    {
        vec2 param_153 = px * 0.0599999986588954925537109375;
        int param_154 = 4;
        float _3644 = fbm(param_153, param_154);
        float fr = _3644;
        vec2 param_155 = (px * 0.2199999988079071044921875) + vec2(3.0);
        int param_156 = 3;
        float _3652 = fbm(param_155, param_156);
        float ridges = 1.0 - (abs(_3652 - 0.5) * 2.0);
        float reach = (14.0 + (46.0 * fr)) * _278.env.z;
        float m = smoothstep(reach, reach * 0.25, edge);
        vec3 frostC = mix(vec3(0.550000011920928955078125, 0.62000000476837158203125, 0.75), vec3(0.89999997615814208984375, 0.949999988079071044921875, 1.0), vec3(max(dayK, 0.300000011920928955078125)));
        vec4 param_157 = acc;
        vec3 param_158 = frostC;
        float param_159 = (m * (0.25 + (0.449999988079071044921875 * pow(ridges, 3.0)))) * 0.85000002384185791015625;
        over(param_157, param_158, param_159);
        acc = param_157;
        vec2 sc_1 = floor(px / vec2(3.0));
        vec2 param_160 = sc_1 + vec2(7.0);
        if (hash(param_160) > 0.98500001430511474609375)
        {
            vec2 param_161 = sc_1;
            vec4 param_162 = acc;
            vec3 param_163 = ((vec3(1.0) * m) * 0.5) * (0.5 + (0.5 * sin((t * 4.0) + (hash(param_161) * 30.0))));
            glow(param_162, param_163);
            acc = param_162;
        }
    }
    if (_278.env3.y > 0.0)
    {
        vec2 param_164 = (px * 0.02500000037252902984619140625) + vec2(t * 0.00999999977648258209228515625, 0.0);
        int param_165 = 3;
        float _3737 = fbm(param_164, param_165);
        float reach_1 = (22.0 + (50.0 * _3737)) * _278.env3.y;
        float m_1 = smoothstep(reach_1, reach_1 * 0.100000001490116119384765625, edge);
        vec4 param_166 = acc;
        vec3 param_167 = mix((skyC * 1.2000000476837158203125) + vec3(0.100000001490116119384765625), vec3(0.62000000476837158203125, 0.660000026226043701171875, 0.7200000286102294921875), vec3(0.449999988079071044921875));
        float param_168 = m_1 * 0.4199999868869781494140625;
        over(param_166, param_167, param_168);
        acc = param_166;
    }
    vec4 param_169 = acc;
    vec3 param_170 = vec3(0.0199999995529651641845703125, 0.0350000001490116119384765625, 0.070000000298023223876953125);
    float param_171 = smoothstep(0.550000011920928955078125, 1.0, px.y / bs.y) * 0.2199999988079071044921875;
    over(param_169, param_170, param_171);
    acc = param_169;
    acc *= clamp(0.5 + edge, 0.0, 1.0);
    fragColor = clamp(acc, vec4(0.0), vec4(1.0)) * _278.qt_Opacity;
}

