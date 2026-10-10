#version 440
// Pass B of the weather sky (full resolution, see components/Sky.qml). Takes pass A (shaders/clouds.frag:
// sky + volumetric clouds, alpha = cloud opacity) and adds everything that must stay sharp: stars, meteors,
// the Moon (true phase, earthshine) and the Sun disc hidden by the clouds in front, fog banks, rain
// streaks in depth, drops on the glass (some sliding), bokeh snow, hail, the lightning bolt, wind streaks,
// then the text scrim, the bottom shade and the rounded corners. Output premultiplied.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;          // seconds, wraps every hour
    float radius;        // corner radius, px
    vec2 size;           // px
    vec4 sun;            // x, y (0..1), altitude deg, -
    vec4 moon;           // x, y (0..1), altitude deg, lit fraction
    vec4 moonLit;        // sun direction seen from the Moon (screen x, y down, z to viewer), parallactic angle
    vec4 wx;             // cloud cover, rain, snow, fog        (0..1)
    vec4 wx2;            // storm, hail, drizzle, wind streaks  (0..1)
    vec4 fx;             // rain slant (tan), -, meteor chance per slot, haze
    vec4 bolt;           // flash, bolt x (0..1), seconds since the strike, strike id
    vec4 life;           // visible box height (0..1 of size.y), wind-blown leaves, birds, god rays (0..1)
    vec4 env;            // temperature °C, humidity 0..1, frost on the glass, heat shimmer
    vec4 env2;           // snow on the ground, fireflies, floating motes, horizon y (0..1)
    vec4 env3;           // halo (sun / moon), misted glass, rain curtains, snowflakes on the glass
};
layout(binding = 1) uniform sampler2D src;

float HOR = 0.86;

float hash(vec2 p) { vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
vec2 hash2(vec2 p) { vec3 q = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973)); q += dot(q, q.yzx + 33.33); return fract((q.xx + q.yz) * q.zy); }
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
const mat2 ROT = mat2(0.8, -0.6, 0.6, 0.8);
float fbm(vec2 p, int oct) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { if (i >= oct) break; v += a * noise(p); p = ROT * p * 2.02 + 17.1; a *= 0.5; }
    return v;
}
void over(inout vec4 acc, vec3 c, float a) { acc = acc * (1.0 - a) + vec4(c * a, a); }
void glow(inout vec4 acc, vec3 c) { acc += vec4(c, max(c.r, max(c.g, c.b))); }
vec3 capLum(vec3 c, float cap) { float l = dot(c, vec3(0.2126, 0.7152, 0.0722)); return l > cap * 0.75 ? c * (cap * 0.75 + (l - cap * 0.75) * 0.3) / l : c; }

void main() {
    vec2 uv = qt_TexCoord0;
    vec2 px = uv * size;
    float H = size.y;
    float t = time;
    float cov = wx.x, rain = wx.y, snow = wx.z, fog = wx.w;
    float storm = wx2.x, hail = wx2.y, drizzle = wx2.z, windS = wx2.w;
    float s = sun.z;
    float slant = fx.x;

    float dayK = smoothstep(-6.0, 7.0, s);
    float nightK = 1.0 - smoothstep(-16.0, -5.0, s);
    float moonLight = moon.w * smoothstep(-1.0, 8.0, moon.z) * nightK;
    float gloom = smoothstep(0.6, 1.0, cov);

    if (env2.w > 0.0) HOR = env2.w;
    // heat shimmer: the air above the warm ground wobbles
    vec2 suv = uv;
    if (env.w > 0.0) {
        float band = smoothstep(HOR - 0.30, HOR - 0.02, uv.y) * (1.0 - smoothstep(HOR + 0.02, HOR + 0.12, uv.y));
        suv += vec2(sin(uv.y * 150.0 + t * 7.0), sin(uv.x * 90.0 - t * 5.0 + uv.y * 70.0)) * vec2(0.0018, 0.0026) * env.w * band;
    }
    vec4 A = texture(src, suv);
    vec3 skyC = A.rgb;
    float occ = A.a;
    vec4 acc = vec4(skyC * 0.95, 0.95);
    float open = 1.0 - occ;

    // ------------------------------------------------------------ stars (behind the clouds)
    float starVis = nightK * (1.0 - 0.6 * moonLight) * (1.0 - fx.w) * (1.0 - fog) * open;
    if (starVis > 0.01) {
        vec2 cell = floor(px / 9.0);
        float h = hash(cell);
        if (h > 0.80) {
            vec2 l = px - (cell + 0.2 + 0.6 * hash2(cell)) * 9.0;
            float mag = pow(hash(cell + 7.3), 3.0);
            float tw = 0.7 + 0.3 * sin(t * (2.0 + 5.0 * h) + h * 50.0) * (0.4 + 0.6 * uv.y);
            float vis = smoothstep(0.55 * moonLight, 0.55 * moonLight + 0.2, mag + 0.15);
            float b = (0.3 + 0.7 * mag) * tw * vis * smoothstep(0.75 + mag, 0.0, length(l)) * (1.0 - smoothstep(HOR - 0.24, HOR, uv.y));
            glow(acc, mix(vec3(0.78, 0.85, 1.0), vec3(1.0, 0.9, 0.78), hash(cell + 3.1)) * b * starVis);
        }
    }

    // ------------------------------------------------------------ meteors: chance per 6 s slot
    if (fx.z > 0.0 && starVis > 0.05) {
        float slot = floor(t / 6.0);
        if (hash(vec2(slot, 1.3)) < fx.z) {
            float lt = t - slot * 6.0 - hash(vec2(slot, 2.1)) * 4.5;
            if (lt > 0.0 && lt < 0.9) {
                vec2 a0 = vec2(0.1 + 0.8 * hash(vec2(slot, 3.7)), 0.05 + 0.3 * hash(vec2(slot, 4.9))) * size;
                float ang = mix(0.35, 1.1, hash(vec2(slot, 5.3))) * (hash(vec2(slot, 6.1)) < 0.5 ? -1.0 : 1.0);
                vec2 dir = vec2(sin(ang), cos(ang) * 0.6);
                vec2 head = a0 + dir * lt * 140.0;
                float along = -dot(px - head, dir);
                float perp = length(px - head + dir * along);
                float tail = min(lt, 0.3) * 140.0;
                float k = along > 0.0 && along < tail ? 1.0 - along / tail : 0.0;
                glow(acc, vec3(0.9, 0.95, 1.0) * k * k * smoothstep(1.3, 0.0, perp) * sin(3.1416 * lt / 0.9) * starVis);
            }
        }
    }

    // ------------------------------------------------------------ the Moon (true phase), veiled by clouds
    if (moon.z > -1.5) {
        float R = 13.0;
        vec2 q = (px - moon.xy * size) / R;
        float r = length(q);
        float above = smoothstep(-1.5, 0.5, moon.z);
        if (r > 1.0) glow(acc, vec3(0.72, 0.80, 1.0) * (exp(-(r - 1.0) * 0.6) * 0.10 + exp(-(r - 1.0) * 2.5) * 0.10) * moon.w * nightK * above * (1.0 - 0.6 * occ) * (1.0 - gloom));
        else {
            vec3 n = vec3(q, sqrt(1.0 - r * r));
            float lit = smoothstep(-0.06, 0.06, dot(n, moonLit.xyz));
            vec2 nd = vec2(sin(moonLit.w), -cos(moonLit.w));
            vec2 tq = vec2(dot(q, vec2(-nd.y, nd.x)), -dot(q, nd));
            float mare = smoothstep(0.42, 0.0, length((tq - vec2(-0.30, -0.36)) * vec2(1.0, 1.2)))
                       + smoothstep(0.26, 0.0, length(tq - vec2(0.20, -0.34))) + smoothstep(0.30, 0.0, length((tq - vec2(0.33, -0.04)) * vec2(1.0, 1.3)))
                       + smoothstep(0.18, 0.0, length(tq - vec2(0.68, -0.26))) + smoothstep(0.22, 0.0, length(tq - vec2(0.54, 0.18)))
                       + smoothstep(0.55, 0.0, length((tq - vec2(-0.62, 0.0)) * vec2(1.4, 0.8))) + smoothstep(0.26, 0.0, length(tq - vec2(-0.14, 0.36)));
            mare = min(mare, 1.0) * (0.75 + 0.5 * noise(tq * 5.0));
            float alb = (0.93 - 0.22 * mare + 0.05 * (noise(tq * 11.0) - 0.5) + 0.25 * smoothstep(0.07, 0.0, length(tq - vec2(-0.12, 0.80)))) * (0.85 + 0.15 * n.z);
            vec3 litC = mix(vec3(0.98, 0.96, 0.90), vec3(0.93, 0.95, 1.0), dayK) * alb
                      * mix(vec3(1.0, 0.72, 0.48), vec3(1.0), smoothstep(2.0, 16.0, moon.z) + dayK);
            float earth = 0.25 + 0.75 * pow(1.0 - moon.w, 3.0);
            vec3 darkC = mix(skyC, vec3(0.28, 0.32, 0.42) * alb, earth * 0.5);
            float aLit = mix(1.0, 0.55 * smoothstep(0.08, 0.35, moon.w), dayK);
            float aDark = 0.75 * (1.0 - dayK);
            float a = clamp((1.0 - r) * R + 0.5, 0.0, 1.0) * mix(aDark, aLit, lit) * above * (1.0 - occ) * (1.0 - occ) * (1.0 - gloom);
            over(acc, mix(darkC, litC, lit), a);
        }
    }

    // ------------------------------------------------------------ the land: a far treeline swaying in the wind, fields
    float gx = px.x / H;
    float ridge = HOR - 0.010 - 0.030 * noise(vec2(gx * 1.6, 3.0)) - 0.012 * noise(vec2(gx * 6.0, 1.0));
    float sway = (0.15 + windS) * 0.05;
    float crowns = pow(noise(vec2(gx * 26.0 + sin(t * 1.6 + gx * 5.0) * sway, 5.0)), 1.6) * 0.030
                 + pow(noise(vec2(gx * 61.0 + sin(t * 2.3 + gx * 9.0) * sway * 1.4, 9.0)), 2.0) * 0.012;
    float treeTop = ridge - crowns;
    float treeMask = smoothstep(treeTop - 0.7 / H, treeTop + 0.7 / H, uv.y);
    float gSnow = env2.x;
    vec3 treeC = mix(vec3(0.015, 0.02, 0.03), mix(skyC * 0.42, vec3(0.10, 0.13, 0.12), 0.5), dayK);
    treeC = mix(treeC, mix(vec3(0.20, 0.22, 0.26), vec3(0.70, 0.74, 0.80), dayK), gSnow * 0.45 * smoothstep(0.35, 0.7, noise(px * 0.4)));
    treeC = mix(treeC, skyC, 0.25 + 0.5 * fx.w);                         // aerial perspective
    over(acc, treeC, treeMask * (1.0 - 0.75 * fog));
    if (uv.y > ridge) {
        float k = clamp((uv.y - ridge) / max(0.05, 1.0 - ridge), 0.0, 1.0);   // 0 far .. 1 near
        vec3 field = mix(vec3(0.03, 0.04, 0.035), vec3(0.17, 0.22, 0.16), dayK) * (0.8 + 0.4 * noise(vec2(gx * 40.0, uv.y * 120.0 / (0.3 + k))));
        vec3 snowF = mix(vec3(0.17, 0.19, 0.24), vec3(0.86, 0.89, 0.94), dayK) * (0.92 + 0.08 * noise(px * 0.3));
        field = mix(field, snowF, gSnow);
        field += skyC * 0.30 * rain * (1.0 - gSnow);                      // wet ground shines
        over(acc, mix(skyC, field, smoothstep(0.0, 0.25, k)), smoothstep(ridge, ridge + 0.015, uv.y) * 0.9 * (1.0 - 0.6 * fog));
    }

    // ------------------------------------------------------------ fog banks
    if (fog > 0.0) {
        vec2 p = vec2(uv.x * size.x / H, uv.y);
        float f = fbm(vec2(p.x * 2.2 - t * 0.025, uv.y * 6.0 + t * 0.01), 4);
        float f2 = fbm(vec2(p.x * 1.3 + t * 0.015, uv.y * 3.0 + 5.0), 3);
        float dens = fog * (0.25 + 0.75 * smoothstep(0.15, 0.9, uv.y)) * (0.45 + 1.4 * f * f2);
        over(acc, mix(vec3(0.20, 0.21, 0.24), vec3(0.62, 0.65, 0.70), max(dayK, 0.1)), clamp(dens * 0.75, 0.0, 0.8));
    }
    if (fx.w > 0.0) over(acc, mix(vec3(0.25, 0.27, 0.30), vec3(0.58, 0.61, 0.66), dayK), fx.w * 0.30);

    // ------------------------------------------------------------ rain curtains (heavy rain sweeping across)
    if (env3.z > 0.0) {
        float dirc = slant >= 0.0 ? 1.0 : -1.0;
        float band = fbm(vec2((px.x - px.y * slant * 1.2) / H * 1.4 - dirc * t * 0.30, t * 0.04), 3);
        over(acc, mix(vec3(0.30, 0.33, 0.39), vec3(0.62, 0.66, 0.73), dayK), smoothstep(0.42, 0.78, band) * 0.24 * env3.z * smoothstep(0.05, 0.45, uv.y));
    }

    // ------------------------------------------------------------ rain: motion-blurred streaks in depth
    vec3 dropC = mix(vec3(0.52, 0.58, 0.70), vec3(0.80, 0.86, 0.96), max(dayK, 0.25));
    if (rain > 0.0) {
        for (int i = 0; i < 3; i++) {
            float fi = float(i) / 2.0;
            float cw = mix(5.0, 11.0, fi);
            float speed = mix(1.5, 2.5, fi) * mix(1.0, 0.45, drizzle);
            float len = mix(0.06, 0.13, fi) * mix(1.0, 0.45, drizzle) * (0.8 + 0.4 * rain);
            vec2 rp = vec2(px.x - px.y * slant, uv.y);
            float col = floor(rp.x / cw);
            float rnd = hash(vec2(col, fi * 13.0));
            float y = rp.y - t * speed * (0.85 + 0.3 * rnd) + rnd * 7.0;
            float rows = 1.6 + fi;
            float row = floor(y * rows);
            float on = step(hash(vec2(col, row + fi * 31.0)), rain * (0.30 + 0.25 * fi));
            float ly = fract(y * rows) / rows;
            float dx = abs(mod(rp.x, cw) - (0.2 + 0.6 * hash(vec2(row, col + fi))) * cw);
            float w = mix(0.45, 0.85, fi) * mix(1.0, 0.7, drizzle);
            float body = smoothstep(0.0, len, ly) * (ly < len ? 1.0 : 0.0);
            over(acc, dropC, on * smoothstep(w + 0.6, w - 0.4, dx) * body * mix(0.10, 0.26, fi) * (1.0 - 0.6 * fog));
        }
    }
    if (hail > 0.0) {
        vec2 hp = vec2(px.x - px.y * slant * 0.6, px.y - t * 300.0);
        vec2 cell = floor(hp / 22.0);
        if (hash(cell + 4.2) < 0.35 * hail)
            over(acc, vec3(0.92, 0.95, 1.0), smoothstep(2.0, 0.8, length(hp - (cell + 0.2 + 0.6 * hash2(cell)) * 22.0)) * 0.85);
    }

    // ------------------------------------------------------------ splashes on the ground (rings in perspective)
    if (rain > 0.0 && uv.y > ridge + 0.02) {
        float k = clamp((uv.y - ridge) / max(0.05, 1.0 - ridge), 0.0, 1.0);
        float cs = mix(10.0, 34.0, k);
        vec2 cell = floor(vec2(px.x / cs, (uv.y - ridge) * H / (cs * 0.45)));
        float h = hash(cell + 61.0);
        if (h < 0.30 * rain) {
            float life = fract(t * (1.2 + 0.8 * hash(cell + 3.0)) + h * 7.0);
            vec2 c0 = vec2((cell.x + 0.2 + 0.6 * hash(cell + 5.0)) * cs, ridge * H + (cell.y + 0.5) * cs * 0.45);
            vec2 d = (px - c0) * vec2(1.0, 3.2);
            float r = life * mix(2.5, 9.0, k);
            float ring = smoothstep(1.3, 0.0, abs(length(d) - r)) * (1.0 - life) * (1.0 - life);
            over(acc, dropC, ring * 0.35 * min(1.0, rain * 1.5) * (0.35 + 0.65 * k));
        }
    }

    // ------------------------------------------------------------ snow: sharp far flakes, soft bokeh near
    if (snow > 0.0) {
        for (int i = 0; i < 3; i++) {
            float fi = float(i) / 2.0;
            float cs = mix(16.0, 46.0, fi);
            float fall = mix(14.0, 38.0, fi);
            vec2 sp = vec2(px.x - px.y * slant * 0.8, px.y - t * fall);
            sp.x += sin(sp.y * 0.03 + t * 0.9 + fi * 4.0) * (3.0 + 6.0 * fi);
            vec2 cell = floor(sp / cs);
            if (hash(cell + fi * 17.0) < snow * (0.55 + 0.25 * fi)) {
                float d = length(sp - (cell + 0.2 + 0.6 * hash2(cell + fi * 5.0)) * cs);
                float rr = mix(0.9, 3.6, fi) * (0.7 + 0.6 * hash(cell + 9.0));
                float soft = mix(0.6, 3.0, fi);
                over(acc, mix(vec3(0.68, 0.72, 0.80), vec3(0.97, 0.98, 1.0), max(dayK, 0.35)), smoothstep(rr + soft, rr - soft * 0.3, d) * mix(0.75, 0.42, fi));
            }
        }
    }

    // ------------------------------------------------------------ lightning: jagged bolt from the deck
    if (bolt.x > 0.001 && bolt.z < 0.35) {
        float y0 = 0.22, y1 = HOR - 0.03;
        if (uv.y > y0 && uv.y < y1) {
            float bx = bolt.y * size.x + ((noise(vec2(uv.y * 7.0, bolt.w)) - 0.5) * 0.22 + (noise(vec2(uv.y * 30.0, bolt.w + 3.0)) - 0.5) * 0.05) * H;
            float d = abs(px.x - bx);
            glow(acc, vec3(0.92, 0.94, 1.0) * (smoothstep(1.5, 0.3, d) + exp(-d / 7.0) * 0.35) * min(bolt.x, 1.0) * smoothstep(y0, y0 + 0.06, uv.y));
        }
    }
    if (bolt.x > 0.001) glow(acc, vec3(0.75, 0.78, 0.95) * bolt.x * 0.12);

    // ------------------------------------------------------------ wind streaks
    if (windS > 0.0) {
        float dirx = slant >= 0.0 ? 1.0 : -1.0;
        vec2 wp = vec2(px.x - dirx * t * 420.0, px.y);
        float lane = floor(wp.y / 9.0);
        float rnd = hash(vec2(lane, 3.3));
        if (rnd < windS * 0.35 && uv.y > 0.25 && uv.y < 0.8) {
            float lx = fract((wp.x + rnd * 900.0) / 700.0);
            float on = step(hash(vec2(lane, floor((wp.x + rnd * 900.0) / 700.0))), 0.5);
            over(acc, vec3(1.0), on * smoothstep(0.0, 0.06, lx) * smoothstep(0.14, 0.06, lx) * smoothstep(1.2, 0.0, abs(mod(wp.y, 9.0) - 4.5)) * 0.2);
        }
    }

    // ------------------------------------------------------------ fireflies over the fields on warm summer nights
    if (env2.y > 0.0 && uv.y > HOR - 0.08) {
        vec2 cell = floor(px / 38.0);
        float h = hash(cell + 91.0);
        if (h < 0.35 * env2.y) {
            vec2 c0 = (cell + 0.5) * 38.0 + vec2(sin(t * 0.5 + h * 30.0), cos(t * 0.43 + h * 17.0)) * 12.0;
            float d = length(px - c0);
            float blink = pow(max(0.0, sin(t * (0.9 + h * 1.5) + h * 40.0)), 5.0);
            glow(acc, vec3(0.72, 1.0, 0.38) * (exp(-d * d / 2.5) * 0.9 + exp(-d / 6.0) * 0.14) * blink * env2.y);
        }
    }
    // ------------------------------------------------------------ pollen / dust motes glinting in the sun
    if (env2.z > 0.0) {
        for (int i = 0; i < 2; i++) {
            float fi = float(i);
            float cs = mix(26.0, 44.0, fi);
            vec2 mp = px + vec2(-t * mix(5.0, 9.0, fi) * (slant >= 0.0 ? 1.0 : -1.0), t * mix(3.0, 6.0, fi));
            vec2 cell = floor(mp / cs);
            float h = hash(cell + fi * 23.0 + 101.0);
            if (h < 0.30 * env2.z) {
                vec2 c0 = (cell + 0.2 + 0.6 * hash2(cell + fi)) * cs + vec2(sin(t * 0.7 + h * 50.0), cos(t * 0.6 + h * 20.0)) * 4.0;
                float d = length(mp - c0);
                float sunNear = exp(-length((px - sun.xy * size) / H) * 2.2);
                float tw = 0.5 + 0.5 * sin(t * 3.0 + h * 60.0);
                glow(acc, vec3(1.0, 0.94, 0.78) * exp(-d * d / mix(0.6, 1.4, fi)) * (0.25 + 1.1 * sunNear) * tw * env2.z * dayK);
            }
        }
    }

    // ------------------------------------------------------------ birds: a small flock crosses now and then
    if (life.z > 0.0) {
        float slot = floor(t / 11.0);
        if (hash(vec2(slot, 21.7)) < life.z) {
            float lt = t - slot * 11.0;
            float dirb = hash(vec2(slot, 22.3)) < 0.5 ? -1.0 : 1.0;
            float spd = 34.0 + 22.0 * hash(vec2(slot, 23.9));
            float gy = (0.14 + 0.30 * hash(vec2(slot, 24.1))) * H;
            float cx = dirb > 0.0 ? -60.0 + lt * spd : size.x + 60.0 - lt * spd;
            float sc = 0.8 + 0.5 * hash(vec2(slot, 25.3));
            vec3 birdC = mix(vec3(0.03, 0.04, 0.07), vec3(0.10, 0.11, 0.15), dayK);
            for (int i = 0; i < 5; i++) {
                float fi = float(i);
                if (hash(vec2(slot, 30.0 + fi)) > 0.75 && i > 1) continue;
                float side = mod(fi, 2.0) * 2.0 - 1.0, rank = floor((fi + 1.0) / 2.0);
                vec2 bc = vec2(cx - dirb * rank * 13.0 * sc, gy + side * rank * 7.0 * sc + 3.0 * sin(t * 0.8 + fi * 1.7));
                vec2 q = (px - bc) / (5.5 * sc);
                if (abs(q.x) < 1.2 && abs(q.y) < 1.2) {
                    float flap = sin(t * (9.0 + fi) + fi * 2.1);
                    float ax = abs(q.x);
                    float wy = -flap * 0.55 * ax + 0.35 * ax * ax;          // wing curve (y down)
                    float d = abs(q.y - wy) * 5.5 * sc;
                    float a = smoothstep(1.1, 0.25, d) * smoothstep(1.05, 0.85, ax);
                    over(acc, birdC, a * 0.75 * (1.0 - fog));
                }
            }
        }
    }

    // ------------------------------------------------------------ autumn leaves blown by the wind
    if (life.y > 0.0) {
        float dirx = slant >= 0.0 ? 1.0 : -1.0;
        for (int i = 0; i < 2; i++) {
            float fi = float(i);
            float cs = mix(44.0, 70.0, fi);
            float spd = mix(70.0, 150.0, fi) * (0.6 + 0.6 * life.y);
            vec2 lp = vec2(px.x - dirx * t * spd, px.y - t * mix(10.0, 22.0, fi));
            vec2 cell = floor(lp / cs);
            float h = hash(cell + fi * 11.0);
            if (h < life.y * mix(0.22, 0.12, fi)) {
                vec2 c0 = (cell + 0.25 + 0.5 * hash2(cell + fi * 3.0)) * cs;
                c0.y += sin(t * (1.5 + h * 2.0) + h * 40.0) * cs * 0.18;
                float ang = t * (2.0 + 4.0 * hash(cell + 6.0)) * (h < 0.05 ? -1.0 : 1.0) + h * 30.0;
                vec2 q = lp - c0;
                q = mat2(cos(ang), -sin(ang), sin(ang), cos(ang)) * q;
                float rr = mix(3.2, 5.5, fi) * (0.75 + 0.5 * hash(cell + 9.0));
                q.y /= 0.25 + 0.75 * abs(cos(ang * 0.7));                 // tumbling: the leaf turns edge-on
                float u = q.x / rr;
                float wdt = rr * 0.48 * max(0.0, 1.0 - u * u);
                float d = abs(q.y) - wdt;
                float a = smoothstep(0.9, -0.3, d) * step(abs(u), 1.0);
                vec3 lc = mix(vec3(0.86, 0.42, 0.12), vec3(0.72, 0.18, 0.10), hash(cell + 13.0));
                lc = mix(lc, vec3(0.88, 0.70, 0.22), step(0.66, hash(cell + 15.0)));
                lc *= mix(0.45, 1.0, dayK) * (0.75 + 0.35 * smoothstep(-rr, rr, q.y));
                over(acc, lc, a * mix(0.9, 0.8, fi) * (1.0 - 0.5 * fog));
            }
        }
    }

    // ------------------------------------------------------------ drops on the glass: lenses, some slide down
    if (rain > 0.0 || storm > 0.0) {
        float wetK = max(rain, storm * 0.8) * (1.0 - drizzle * 0.5);
        vec2 cell = floor(px / 24.0);
        float h = hash(cell + 41.0);
        float life = fract(t * (0.04 + 0.05 * hash(cell + 2.0)) + h);
        if (h < 0.16 * wetK) {
            vec2 c0 = (cell + 0.25 + 0.5 * hash2(cell + 5.0)) * 24.0;
            float rr = 2.4 + 3.4 * hash(cell + 8.0);
            vec2 l = (px - c0) / vec2(rr, rr * 1.12);
            float r = length(l);
            float a = smoothstep(1.0, 0.85, r) * smoothstep(0.0, 0.1, life) * smoothstep(1.0, 0.7, life);
            if (a > 0.0) {
                vec3 dc = mix(skyC * 1.05 + 0.03, skyC * 1.45 + 0.10, smoothstep(-0.6, 0.8, l.y));
                dc += vec3(0.85) * smoothstep(0.30, 0.0, length(l - vec2(-0.35, -0.42)));
                over(acc, capLum(dc, 0.75), a * 0.55);
                over(acc, skyC * 0.35, smoothstep(0.72, 0.98, r) * a * 0.45);
            }
        }
        float colx = floor(px.x / 46.0);
        float hc = hash(vec2(colx, 77.0));
        if (hc < 0.22 * wetK) {
            float sx = (colx + 0.3 + 0.4 * hash(vec2(colx, 3.0))) * 46.0 + sin(px.y * 0.07 + colx) * 1.5;
            float sy = mod(t * (14.0 + 20.0 * hc) + hc * 500.0, H * 1.6) - H * 0.3;
            vec2 l = (px - vec2(sx, sy)) / vec2(2.8, 3.4);
            float r = length(l);
            if (r < 1.0) {
                vec3 dc = mix(skyC * 1.05 + 0.03, skyC * 1.45 + 0.10, smoothstep(-0.6, 0.8, l.y)) + vec3(0.85) * smoothstep(0.30, 0.0, length(l - vec2(-0.35, -0.42)));
                over(acc, capLum(dc, 0.75), smoothstep(1.0, 0.85, r) * 0.6);
                over(acc, skyC * 0.35, smoothstep(0.72, 0.98, r) * 0.4);
            }
            float trail = px.y < sy && px.y > sy - 46.0 ? (1.0 - (sy - px.y) / 46.0) : 0.0;
            over(acc, skyC * 1.2 + 0.05, smoothstep(1.2, 0.2, abs(px.x - sx)) * trail * 0.22);
        }
    }

    // ------------------------------------------------------------ snowflakes settling on the glass and melting
    if (env3.w > 0.0) {
        vec2 cell = floor(px / 30.0);
        float h = hash(cell + 131.0);
        float life = fract(t * (0.05 + 0.05 * hash(cell + 4.0)) + h);
        if (h < 0.22 * env3.w) {
            vec2 c0 = (cell + 0.25 + 0.5 * hash2(cell + 7.0)) * 30.0;
            vec2 q = px - c0;
            float rr = (2.5 + 2.5 * hash(cell + 9.0)) * (1.0 - 0.5 * smoothstep(0.6, 1.0, life));
            float a = atan(q.y, q.x) + h * 6.0;
            float arms = pow(abs(cos(3.0 * a)), 10.0);
            float r = length(q);
            float f = smoothstep(rr * (0.30 + 0.70 * arms) + 0.6, rr * (0.30 + 0.70 * arms) - 0.4, r) * smoothstep(0.0, 0.08, life) * smoothstep(1.0, 0.65, life);
            over(acc, vec3(0.94, 0.97, 1.0), f * 0.8);
        }
    }

    // ------------------------------------------------------------ text scrim, bottom shade, rounded corners
    float zone = sin(clamp((uv.y - 0.02) / 0.68, 0.0, 1.0) * 3.1416) * (0.75 + 0.25 * abs(uv.x - 0.5) * 2.0);   // no visible edge
    over(acc, vec3(0.02, 0.03, 0.07), 0.03 + 0.04 * dayK + 0.16 * zone * dayK);
    // ------------------------------------------------------------ the Sun disc (after the scrim: stays bright; wide glow is in pass A)
    float sunUp = smoothstep(-1.2, 0.5, s);
    if (sunUp > 0.0) {
        vec3 sunCol = mix(vec3(1.0, 0.48, 0.22), vec3(1.0, 0.95, 0.87), smoothstep(0.0, 20.0, s));
        vec2 d = px - sun.xy * size;
        d.y /= mix(0.84, 1.0, smoothstep(0.0, 4.0, s));
        float r = length(d);
        float clear = (1.0 - 0.95 * occ) * (1.0 - 0.8 * gloom) * (1.0 - treeMask);
        glow(acc, sunCol * (exp(-r / 6.0) * 0.45 + exp(-abs(d.y) / 1.4) * exp(-abs(d.x) / 55.0) * 0.14) * clear * sunUp);
        over(acc, mix(vec3(1.0, 0.99, 0.96), sunCol, smoothstep(3.0, 10.0, r)), clamp(10.0 - r + 0.5, 0.0, 1.0) * sunUp * clear);
    }

    // ------------------------------------------------------------ god rays: sunlight through gaps in the clouds
    if (life.w > 0.0 && sunUp > 0.0) {
        vec2 toS = sun.xy - uv;
        float light = 0.0, w = 1.0, wsum = 0.0;
        float j = hash(px + fract(t));
        for (int i = 0; i < 14; i++) {
            vec2 sp = uv + toS * ((float(i) + j) / 14.0);
            float o = texture(src, clamp(sp, vec2(0.0), vec2(1.0))).a;
            light += (1.0 - o) * w; wsum += w; w *= 0.93;
        }
        light /= wsum;
        float shaft = max(0.0, light - (1.0 - occ) * 0.55) * occ;          // lit line of sight across a cloudy spot
        float fall = exp(-length(toS * vec2(size.x / H, 1.0)) * 1.6);
        float flick = 0.85 + 0.15 * sin(t * 0.6 + uv.x * 3.0);
        vec3 sunCol = mix(vec3(1.0, 0.62, 0.32), vec3(1.0, 0.95, 0.85), smoothstep(0.0, 20.0, s));
        glow(acc, sunCol * (shaft * 0.55 + light * 0.10) * fall * flick * life.w * sunUp);
    }

    // ------------------------------------------------------------ 22° halo around the sun (or a bright moon) in thin high cloud
    if (env3.x > 0.0) {
        bool bySun = s > 0.0;
        vec2 c0 = (bySun ? sun.xy : moon.xy) * size;
        float R = H * 0.404 / 1.8;
        float d = length(px - c0);
        float ring = exp(-pow((d - R) / 2.6, 2.0));
        float inner = exp(-pow((d - R + 2.5) / 1.4, 2.0));
        float k = env3.x * (bySun ? 1.0 : 0.6 * moon.w);
        glow(acc, (vec3(0.95, 0.96, 1.0) * ring * 0.14 + vec3(0.30, 0.12, 0.02) * inner * 0.12) * k);
    }

    vec2 bs = vec2(size.x, H * life.x);
    vec2 hs0 = bs * 0.5;
    vec2 qd0 = abs(px - hs0) - hs0 + radius;
    float edge = -(length(max(qd0, 0.0)) + min(max(qd0.x, qd0.y), 0.0) - radius);   // px inward from the rim
    // ------------------------------------------------------------ frost creeping in from the rim (below 0 °C)
    if (env.z > 0.0) {
        float fr = fbm(px * 0.06, 4);
        float ridges = 1.0 - abs(fbm(px * 0.22 + 3.0, 3) - 0.5) * 2.0;
        float reach = (14.0 + 46.0 * fr) * env.z;
        float m = smoothstep(reach, reach * 0.25, edge);
        vec3 frostC = mix(vec3(0.55, 0.62, 0.75), vec3(0.90, 0.95, 1.0), max(dayK, 0.3));
        over(acc, frostC, m * (0.25 + 0.45 * pow(ridges, 3.0)) * 0.85);
        vec2 sc = floor(px / 3.0);
        if (hash(sc + 7.0) > 0.985) glow(acc, vec3(1.0) * m * 0.5 * (0.5 + 0.5 * sin(t * 4.0 + hash(sc) * 30.0)));
    }
    // ------------------------------------------------------------ misted glass (damp, cool air)
    if (env3.y > 0.0) {
        float reach = (22.0 + 50.0 * fbm(px * 0.025 + vec2(t * 0.01, 0.0), 3)) * env3.y;
        float m = smoothstep(reach, reach * 0.1, edge);
        over(acc, mix(skyC * 1.2 + 0.10, vec3(0.62, 0.66, 0.72), 0.45), m * 0.42);
    }

    over(acc, vec3(0.02, 0.035, 0.07), smoothstep(0.55, 1.0, px.y / bs.y) * 0.22);
    acc *= clamp(0.5 + edge, 0.0, 1.0);
    fragColor = clamp(acc, 0.0, 1.0) * qt_Opacity;
}
