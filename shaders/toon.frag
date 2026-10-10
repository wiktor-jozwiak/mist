#version 440
// Cartoon live wallpapers (components/LiveWall.qml when a preset has shader: "toon"): small illustrated
// scenes with characters, drawn from 2D signed distance fields, rendered at full resolution (crisp edges).
// Same uniform block as live.frag. Coordinates: q = (x, y) with y UP, height = 1, x = +-aspect / 2.
// mode 0 cat on a roof, 1 whales, 2 jellyfish, 3 koi pond, 4 astronaut, 5 lighthouse, 6 fox under the
// aurora, 7 hot-air balloons, 8 ghosts (Halloween), 9 penguins, 10 rocket, 11 owl, 12 8-bit city,
// 13 paper boats + sky lanterns, 14 aquarium, 15 bunny fishing for stars.
// c0..c3 tint the sky of most scenes; opt = -, vignette, brightness, music energy.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
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

float AA = 0.001;

float hash(vec2 p) { vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
vec2 hash2(vec2 p) { return vec2(hash(p), hash(p + 19.19)); }
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
float fbm(vec2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * noise(p); p = p * 2.03 + 17.0; a *= 0.5; }
    return v;
}

// ---------------------------------------------------------------- shapes
float sdCircle(vec2 p, float r) { return length(p) - r; }
float sdBox(vec2 p, vec2 b) { vec2 d = abs(p) - b; return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0); }
float sdRBox(vec2 p, vec2 b, float r) { return sdBox(p, b - r) - r; }
float sdEll(vec2 p, vec2 r) { float k0 = length(p / r); float k1 = length(p / (r * r)); return k0 * (k0 - 1.0) / max(k1, 1e-5); }
float sdSeg(vec2 p, vec2 a, vec2 b) { vec2 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h); }
float sdTri(vec2 p, vec2 p0, vec2 p1, vec2 p2) {
    vec2 e0 = p1 - p0, e1 = p2 - p1, e2 = p0 - p2;
    vec2 v0 = p - p0, v1 = p - p1, v2 = p - p2;
    vec2 pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    vec2 pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    vec2 pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    float s = sign(e0.x * e2.y - e0.y * e2.x);
    vec2 d = min(min(vec2(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
                     vec2(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x))),
                     vec2(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)));
    return -sqrt(d.x) * sign(d.y);
}
float sdStar5(vec2 p, float r, float rf) {
    const vec2 k1 = vec2(0.809016994375, -0.587785252292);
    const vec2 k2 = vec2(-k1.x, k1.y);
    p.x = abs(p.x);
    p -= 2.0 * max(dot(k1, p), 0.0) * k1;
    p -= 2.0 * max(dot(k2, p), 0.0) * k2;
    p.x = abs(p.x);
    p.y -= r;
    vec2 ba = rf * vec2(-k1.y, k1.x) - vec2(0, 1);
    float h = clamp(dot(p, ba) / dot(ba, ba), 0.0, r);
    return length(p - ba * h) * sign(p.y * ba.x - p.x * ba.y);
}
float smin(float a, float b, float k) { float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0); return mix(b, a, h) - k * h * (1.0 - h); }
vec2 rot(vec2 p, float a) { float c = cos(a), s = sin(a); return vec2(c * p.x - s * p.y, s * p.x + c * p.y); }
float fill(float d) { return smoothstep(AA, -AA, d); }
void lay(inout vec3 col, vec3 c, float d) { col = mix(col, c, fill(d)); }
void layA(inout vec3 col, vec3 c, float d, float a) { col = mix(col, c, fill(d) * a); }

float stars(vec2 q, float sc, float t) {
    vec2 g = q * sc;
    vec2 id = floor(g), f = fract(g) - 0.5;
    float h = hash(id);
    if (h < 0.92) return 0.0;
    vec2 o = (hash2(id) - 0.5) * 0.6;
    float tw = 0.55 + 0.45 * sin(t * (0.8 + 2.5 * fract(h * 91.0)) + h * 40.0);
    return smoothstep(0.07 + 0.07 * fract(h * 13.0), 0.0, length(f - o)) * tw;
}
vec3 skyGrad(vec2 q) { return mix(c1.rgb, c0.rgb, smoothstep(-0.5, 0.5, q.y)); }

// ================================================================ 0: cat on a roof under the moon
vec3 sCat(vec2 q, float T) {
    vec3 col = skyGrad(q);
    col += vec3(1.0) * stars(q, 55.0, T) * 0.85 * smoothstep(-0.15, 0.3, q.y);
    vec2 mc = vec2(0.5, 0.27);
    float md = length(q - mc);
    col += vec3(1.0, 0.92, 0.7) * 0.22 * exp(-md * 6.0);
    lay(col, vec3(0.98, 0.94, 0.8), md - 0.085);
    lay(col, vec3(0.88, 0.83, 0.68), length(q - mc - vec2(-0.025, 0.02)) - 0.018);
    lay(col, vec3(0.88, 0.83, 0.68), length(q - mc - vec2(0.03, -0.028)) - 0.012);
    // far skyline with windows
    float bx = floor(q.x * 14.0);
    float bh = -0.22 + 0.17 * hash(vec2(bx, 1.0));
    if (q.y < bh) {
        col = vec3(0.07, 0.065, 0.13);
        vec2 w = vec2(q.x * 70.0, q.y * 60.0);
        vec2 wi = floor(w), wf = fract(w);
        float lit = step(0.62, hash(wi + bx)) * step(0.25, wf.x) * step(wf.x, 0.75) * step(0.3, wf.y) * step(wf.y, 0.75);
        float on = step(0.12, fract(hash(wi * 1.7) + T * 0.015));
        col += vec3(1.0, 0.75, 0.38) * lit * on * 0.55;
    }
    // chimney smoke
    float roofY = -0.18 - 0.35 * abs(q.x - 0.2);
    vec2 ch = vec2(-0.12, -0.18 - 0.35 * 0.32 + 0.1);
    for (int i = 0; i < 6; i++) {
        float ph = fract(T * 0.07 + float(i) / 6.0);
        vec2 sp = ch + vec2(0.06 * ph + 0.015 * sin(T * 0.8 + float(i)), 0.02 + ph * 0.32);
        layA(col, vec3(0.55, 0.55, 0.68), sdCircle(q - sp, 0.012 + ph * 0.045), (1.0 - ph) * 0.28);
    }
    // the roof and the chimney
    lay(col, vec3(0.13, 0.1, 0.18), sdBox(q - vec2(-0.12, -0.24), vec2(0.03, 0.14)));
    lay(col, vec3(0.17, 0.13, 0.22), sdBox(q - vec2(-0.12, -0.105), vec2(0.036, 0.008)));
    if (q.y < roofY) {
        col = vec3(0.1, 0.075, 0.13);
        col += vec3(0.05, 0.03, 0.06) * step(0.82, fract((q.y - roofY) * 40.0));
    }
    // the cat on the ridge, moon rim light on its right
    vec2 cp = vec2(0.2, -0.18);
    vec2 lp = q - cp;
    float body = sdEll(lp - vec2(0.0, 0.07), vec2(0.05, 0.075));
    float head = sdCircle(lp - vec2(0.004, 0.165), 0.042);
    float earL = sdTri(lp, vec2(-0.038, 0.18), vec2(-0.03, 0.235), vec2(-0.004, 0.2));
    float earR = sdTri(lp, vec2(0.046, 0.18), vec2(0.038, 0.235), vec2(0.012, 0.2));
    float tail = 1.0;
    for (int i = 0; i < 9; i++) {
        float s = float(i) / 8.0;
        vec2 a = vec2(0.035 + 0.13 * s, 0.01 + 0.06 * s * s + 0.035 * sin(s * 3.0 + T * 1.1) * s);
        float s2 = float(i + 1) / 8.0;
        vec2 b = vec2(0.035 + 0.13 * s2, 0.01 + 0.06 * s2 * s2 + 0.035 * sin(s2 * 3.0 + T * 1.1) * s2);
        tail = min(tail, sdSeg(lp, a, b) - 0.013 * (1.0 - 0.45 * s));
    }
    float cat = min(min(smin(body, head, 0.02), min(earL, earR)), tail);
    lay(col, vec3(0.03, 0.025, 0.05), cat);
    col += vec3(0.55, 0.55, 0.75) * 0.5 * fill(cat) * smoothstep(-0.006, 0.0, cat) * step(0.0, lp.x - 0.005);
    float blink = step(0.035, fract(T / 5.0));
    vec2 ey = vec2(0.016, 0.168);
    float eyes = min(sdEll(lp - vec2(-ey.x + 0.004, ey.y), vec2(0.008, 0.0095 * blink + 0.0008)), sdEll(lp - vec2(ey.x + 0.004, ey.y), vec2(0.008, 0.0095 * blink + 0.0008)));
    lay(col, vec3(0.75, 0.95, 0.35), eyes);
    col += vec3(0.6, 0.9, 0.3) * 0.2 * exp(-max(eyes, 0.0) / 0.008) * blink;
    return col;
}

// ================================================================ 1: whales in deep water
vec3 sWhales(vec2 q, float T) {
    vec3 col = mix(vec3(0.01, 0.035, 0.08), vec3(0.03, 0.2, 0.3), smoothstep(-0.5, 0.55, q.y));
    float ray = 0.0;
    for (int i = 0; i < 3; i++) ray += pow(0.5 + 0.5 * sin(q.x * (5.0 + float(i)) + q.y * 1.5 + T * 0.15 * float(i + 1) + float(i) * 2.0), 10.0);
    col += vec3(0.3, 0.6, 0.7) * ray * 0.07 * smoothstep(-0.4, 0.5, q.y);
    col += vec3(0.6, 0.8, 0.9) * stars(q + vec2(T * 0.004, T * 0.006), 45.0, T) * 0.35;
    for (int k = 0; k < 3; k++) {
        float i = float(2 - k);
        float sc = 1.7 - 0.45 * i;
        float x = mod(T * (0.012 + 0.004 * i) + i * 1.1 + 0.6, 3.0) - 1.5;
        float y = -0.05 + 0.22 * i - 0.06 * i * i + 0.03 * sin(T * 0.25 + i);
        vec2 lp = (q - vec2(x, y)) / sc;
        lp = rot(lp, -0.08 * cos(T * 0.25 + i));
        float flap = 0.025 * sin(T * 1.1 + i);
        float body = sdEll(lp, vec2(0.16, 0.052));
        float head = sdEll(lp - vec2(0.09, -0.004), vec2(0.09, 0.058));
        float stock = sdEll(lp - vec2(-0.17, 0.006 + flap * 0.4), vec2(0.07, 0.022));
        float fl1 = sdEll(rot(lp - vec2(-0.245, 0.025 + flap), 0.6), vec2(0.045, 0.013));
        float fl2 = sdEll(rot(lp - vec2(-0.245, -0.008 + flap), -0.6), vec2(0.045, 0.013));
        float fin = sdEll(rot(lp - vec2(0.03, -0.055), -0.55 + 0.15 * sin(T * 0.8 + i)), vec2(0.055, 0.012));
        float wd = smin(smin(body, head, 0.04), stock, 0.04);
        wd = min(wd, smin(fl1, fl2, 0.01));
        wd = smin(wd, fin, 0.012) * sc;
        vec3 wc = mix(vec3(0.08, 0.18, 0.3), vec3(0.42, 0.6, 0.68), smoothstep(0.0, -0.045, lp.y));
        wc += vec3(0.04) * step(0.6, fract(lp.x * 60.0)) * smoothstep(-0.02, -0.045, lp.y) * step(0.0, lp.x);
        float fade = 1.0 - 0.25 * i;
        col = mix(col, mix(col, wc, fade), fill(wd));
        lay(col, vec3(0.02, 0.03, 0.05), (length(lp - vec2(0.13, 0.008)) - 0.005) * sc);
        // bubbles from the blowhole
        for (int b = 0; b < 4; b++) {
            float ph = fract(T * 0.12 + float(b) * 0.25 + i * 0.3);
            vec2 bp = vec2(0.1 + 0.01 * sin(ph * 12.0), 0.06 + ph * 0.35);
            float bd = abs(length(lp - bp) - 0.006 * (1.0 + ph)) - 0.0015;
            layA(col, vec3(0.7, 0.9, 1.0), bd * sc, (1.0 - ph) * 0.6 * fade);
        }
    }
    return col;
}

// ================================================================ 2: jellyfish
vec3 sJelly(vec2 q, float T) {
    vec3 col = mix(vec3(0.0, 0.008, 0.03), vec3(0.03, 0.05, 0.15), smoothstep(-0.5, 0.5, q.y));
    col += vec3(0.5, 0.6, 0.9) * stars(q + vec2(0.0, T * 0.005), 50.0, T) * 0.25;
    for (int k = 0; k < 10; k++) {
        float i = float(k);
        vec2 h = hash2(vec2(i, 7.0));
        float sc = 0.9 + 0.9 * h.y;
        float pulse = 0.5 + 0.5 * sin(T * 1.5 * (0.7 + 0.3 * h.x) + i * 2.0);
        float y = mod(T * 0.02 * (0.5 + h.x) + h.y * 1.7 + i * 0.37, 1.7) - 0.85;
        float x = (h.x - 0.5) * 1.6 + 0.04 * sin(T * 0.2 + i);
        vec2 lp = (q - vec2(x, y)) / sc;
        vec3 jc = mix(vec3(0.95, 0.45, 0.85), vec3(0.4, 0.85, 1.0), fract(h.x * 3.7));
        vec2 br = vec2(0.06 * (1.0 + 0.12 * pulse), 0.05 * (1.0 - 0.1 * pulse));
        float bell = max(sdEll(lp, br), -lp.y - 0.008) * sc;
        for (int j = 0; j < 5; j++) {
            float tx = (float(j) - 2.0) * 0.021 * (1.0 + 0.15 * pulse);
            float wave = tx + 0.012 * sin(lp.y * 28.0 - T * 2.5 + float(j)) * (-lp.y * 5.0);
            float a = smoothstep(-0.24, 0.0, lp.y) * step(lp.y, 0.0);
            col += jc * smoothstep(0.004, 0.0, abs(lp.x - wave) * sc) * a * 0.45;
        }
        col += jc * 0.22 * exp(-max(bell, 0.0) / 0.03);
        float inner = 0.55 + 0.45 * smoothstep(0.0, br.y, lp.y);
        col = mix(col, jc * inner + vec3(0.15), fill(bell) * 0.6);
        col += vec3(1.0) * fill(sdEll(lp - vec2(-0.02, 0.025), vec2(0.015, 0.008)) * sc) * 0.25;
    }
    return col;
}

// ================================================================ 3: koi pond from above
float koiD(vec2 q, float T, float k, out float seg) {
    float w = (mod(k, 2.0) < 0.5 ? 1.0 : -1.0) * (0.13 + 0.03 * k);
    vec2 c = vec2((k - 1.5) * 0.32, 0.04 * sin(k * 2.0));
    float R = 0.2 + 0.05 * k;
    float d = 1.0;
    seg = 0.0;
    for (int s = 0; s < 11; s++) {
        float fs = float(s);
        float a = T * w + k * 1.7 - fs * 0.075 * sign(w);
        vec2 pos = c + R * vec2(cos(a), sin(a) * 0.75) + 0.008 * sin(T * 3.0 - fs * 0.8) * vec2(-sin(a), cos(a));
        float r = 0.04 * (1.0 - fs / 13.0) * (fs < 1.5 ? 0.85 + 0.15 * fs : 1.0);
        float dd = length(q - pos) - r;
        if (dd < d) seg = fs;
        d = smin(d, dd, 0.012);
    }
    return d;
}
vec3 sKoi(vec2 q, float T) {
    vec3 col = mix(vec3(0.02, 0.08, 0.1), vec3(0.05, 0.18, 0.19), fbm(q * 1.5 + T * 0.02));
    vec2 w = q * 9.0;
    float cs = sin(w.x + sin(w.y * 1.3 + T * 0.4) * 1.5) * sin(w.y + sin(w.x * 1.1 - T * 0.35) * 1.5);
    col += vec3(0.2, 0.4, 0.4) * smoothstep(0.75, 1.0, cs) * 0.25;
    for (int k = 0; k < 4; k++) {
        float fk = float(k);
        float seg;
        float sh = koiD(q + vec2(-0.018, 0.018), T, fk, seg);
        col *= 1.0 - 0.35 * fill(sh - 0.004);
    }
    for (int k = 0; k < 4; k++) {
        float fk = float(k);
        float seg;
        float d = koiD(q, T, fk, seg);
        float pat = noise(q * 22.0 + fk * 10.0);
        vec3 kc = fk == 1.0 ? vec3(0.95, 0.85, 0.2) : vec3(0.97, 0.94, 0.9);
        vec3 spot = fk == 3.0 ? vec3(0.1, 0.1, 0.12) : vec3(0.92, 0.32, 0.12);
        kc = mix(kc, spot, smoothstep(0.45, 0.55, pat) * step(1.0, seg));
        lay(col, kc * (0.8 + 0.2 * smoothstep(0.0, -0.02, d)), d);
    }
    // lily pads float above the fish
    for (int i = 0; i < 7; i++) {
        vec2 h = hash2(vec2(float(i), 3.0));
        vec2 pc = vec2((h.x - 0.5) * 1.7, (h.y - 0.5) * 0.9) + 0.006 * vec2(sin(T * 0.4 + float(i)), cos(T * 0.3 + float(i)));
        float r = 0.05 + 0.04 * fract(h.x * 7.0);
        vec2 lp = rot(q - pc, h.y * 6.28);
        float pad = max(sdCircle(lp, r), -sdTri(lp, vec2(0.0), vec2(r * 1.3, r * 0.3), vec2(r * 1.3, -r * 0.3)));
        col *= 1.0 - 0.3 * fill(sdCircle(q - pc + vec2(-0.012, 0.012), r));
        lay(col, mix(vec3(0.12, 0.38, 0.2), vec3(0.2, 0.5, 0.26), noise(lp * 40.0)), pad);
        col -= vec3(0.05) * fill(pad) * smoothstep(0.003, 0.0, abs(sin(atan(lp.y, lp.x) * 7.0)) * length(lp));
        if (fract(h.y * 5.0) > 0.55) {
            for (int p = 0; p < 6; p++) {
                vec2 pp = rot(vec2(0.018, 0.0), float(p) * 1.047 + T * 0.05);
                lay(col, vec3(0.98, 0.7, 0.82), sdEll(rot(lp - pp - vec2(0.0, 0.0) + vec2(r * 0.2, 0.0), -float(p) * 1.047), vec2(0.016, 0.008)));
            }
            lay(col, vec3(1.0, 0.85, 0.3), sdCircle(lp + vec2(r * 0.2, 0.0), 0.007));
        }
    }
    return col;
}

// ================================================================ 4: astronaut drifting by a ringed planet
vec3 sAstro(vec2 q, float T) {
    vec3 col = mix(vec3(0.01, 0.01, 0.03), vec3(0.04, 0.03, 0.09), smoothstep(-0.5, 0.5, q.y));
    col += vec3(0.28, 0.1, 0.38) * smoothstep(0.35, 0.9, fbm(q * 1.6 + T * 0.006)) * 0.5;
    col += vec3(0.05, 0.22, 0.32) * smoothstep(0.4, 0.9, fbm(q * 2.1 - 7.0 + T * 0.004)) * 0.4;
    col += vec3(1.0) * stars(q, 60.0, T) + vec3(0.8, 0.9, 1.0) * stars(q + 3.0, 25.0, T * 0.7) * 0.6;
    // ringed planet
    vec2 pc = vec2(-0.62, -0.5);
    vec2 rq = rot(q - pc, 0.28);
    float ringO = abs(sdEll(rq, vec2(0.62, 0.11))) - 0.016;
    if (rq.y > 0.0) layA(col, vec3(0.75, 0.65, 0.55), ringO, 0.8);
    float pd = sdCircle(q - pc, 0.34);
    vec2 n = (q - pc) / 0.34;
    float lit = clamp(dot(n, normalize(vec2(0.7, 0.7))) * 0.8 + 0.35, 0.0, 1.0);
    vec3 band = mix(vec3(0.75, 0.42, 0.25), vec3(0.95, 0.7, 0.45), 0.5 + 0.5 * sin((q.y - pc.y) * 45.0 + noise(q * 6.0) * 3.0));
    lay(col, band * lit, pd);
    if (rq.y <= 0.0) layA(col, vec3(0.8, 0.7, 0.6), ringO, 0.85);
    // the astronaut
    vec2 ac = vec2(0.28 + 0.03 * sin(T * 0.17), 0.05 + 0.03 * sin(T * 0.29));
    if (length(q - ac) > 0.26) return col;
    vec2 lp = rot(q - ac, -0.35 * sin(T * 0.13));
    vec3 white = vec3(0.93, 0.94, 0.97), grey = vec3(0.68, 0.7, 0.76);
    float shade = 0.82 + 0.18 * smoothstep(-0.05, 0.05, lp.x + lp.y);
    lay(col, grey * 0.9, sdRBox(lp - vec2(-0.035, -0.005), vec2(0.03, 0.052), 0.012));
    float legL = sdSeg(lp, vec2(-0.018, -0.07), vec2(-0.035 + 0.01 * sin(T * 0.5), -0.145)) - 0.019;
    float legR = sdSeg(lp, vec2(0.02, -0.07), vec2(0.038, -0.14 + 0.008 * sin(T * 0.6))) - 0.019;
    float wave = 0.5 + 0.5 * sin(T * 1.6);
    vec2 handR = vec2(0.075, 0.03) + rot(vec2(0.04, 0.0), 0.6 + 0.6 * wave);
    float armL = sdSeg(lp, vec2(-0.04, 0.005), vec2(-0.1, -0.03 + 0.02 * sin(T * 0.4))) - 0.016;
    float armR = sdSeg(lp, vec2(0.04, 0.01), vec2(0.075, 0.03)) - 0.016;
    float armR2 = sdSeg(lp, vec2(0.075, 0.03), handR) - 0.015;
    float suit = min(min(legL, legR), min(armL, min(armR, armR2)));
    suit = smin(suit, sdRBox(lp - vec2(0.0, -0.02), vec2(0.048, 0.058), 0.026), 0.01);
    lay(col, white * shade, suit);
    lay(col, grey, min(sdCircle(lp - vec2(-0.1, -0.03 + 0.02 * sin(T * 0.4)), 0.018), sdCircle(lp - handR, 0.017)));
    lay(col, grey * 0.85, min(sdEll(lp - vec2(-0.036 + 0.01 * sin(T * 0.5), -0.15), vec2(0.024, 0.016)), sdEll(lp - vec2(0.04, -0.145 + 0.008 * sin(T * 0.6)), vec2(0.024, 0.016))));
    lay(col, vec3(0.95, 0.45, 0.2), sdRBox(lp - vec2(0.0, -0.01), vec2(0.022, 0.016), 0.004));
    lay(col, vec3(0.3, 0.8, 0.9), sdCircle(lp - vec2(-0.008, -0.008), 0.004));
    lay(col, vec3(0.95, 0.85, 0.3), sdCircle(lp - vec2(0.008, -0.008), 0.004));
    lay(col, white * shade, sdCircle(lp - vec2(0.0, 0.075), 0.056));
    float visor = sdEll(lp - vec2(0.012, 0.078), vec2(0.04, 0.03));
    vec3 vc = mix(vec3(0.05, 0.08, 0.2), vec3(0.25, 0.35, 0.6), smoothstep(-0.03, 0.03, lp.y - 0.078 + lp.x - 0.012));
    lay(col, vc, visor);
    lay(col, vec3(0.85, 0.9, 1.0), sdEll(rot(lp - vec2(-0.004, 0.092), 0.5), vec2(0.014, 0.004)));
    col += vec3(0.95, 0.6, 0.85) * 0.25 * fill(visor) * exp(-length(lp - vec2(0.03, 0.065)) * 60.0);
    return col;
}

// ================================================================ 5: lighthouse at night
vec3 sLighthouse(vec2 q, float T) {
    float hy = -0.12;
    vec3 col = skyGrad(q);
    col += vec3(1.0) * stars(q, 55.0, T) * 0.8 * smoothstep(-0.05, 0.3, q.y);
    vec2 mc = vec2(-0.1, 0.3);
    col += vec3(0.9, 0.9, 1.0) * 0.15 * exp(-length(q - mc) * 7.0);
    lay(col, vec3(0.92, 0.93, 0.98), max(sdCircle(q - mc, 0.05), -sdCircle(q - mc - vec2(0.022, 0.012), 0.045)));
    float cl = smoothstep(0.55, 0.8, fbm(vec2(q.x * 2.2 + T * 0.015, q.y * 6.0))) * smoothstep(-0.05, 0.15, q.y);
    col = mix(col, vec3(0.16, 0.18, 0.3), cl * 0.6);
    if (q.y < hy) {
        float depth = hy - q.y;
        col = mix(vec3(0.05, 0.08, 0.18), vec3(0.01, 0.02, 0.06), smoothstep(0.0, 0.35, depth));
        float z = 1.0 / (depth + 0.02);
        float wv = sin(z * 5.0 - T * 1.2 + 3.0 * noise(vec2(q.x * z * 1.2, z * 0.6)));
        col += vec3(0.25, 0.35, 0.55) * smoothstep(0.85, 1.0, wv) * 0.12 * smoothstep(0.0, 0.05, depth);
        float road = exp(-pow((q.x - mc.x) * (3.0 + depth * 10.0), 2.0));
        col += vec3(0.8, 0.85, 1.0) * road * smoothstep(0.6, 1.0, wv) * 0.35;
    }
    // island + lighthouse
    float rock = smin(sdEll(q - vec2(-0.52, hy - 0.035), vec2(0.26, 0.055)), sdEll(q - vec2(-0.44, hy - 0.005), vec2(0.12, 0.05)), 0.04);
    lay(col, vec3(0.06, 0.05, 0.08), rock);
    float lx = -0.47;
    float k = clamp((q.y - hy) / 0.33, 0.0, 1.0);
    float tower = max(abs(q.x - lx) - mix(0.042, 0.028, k), abs(q.y - (hy + 0.165)) - 0.165);
    vec3 tc = mod(floor((q.y - hy) / 0.055), 2.0) < 0.5 ? vec3(0.85, 0.85, 0.88) : vec3(0.75, 0.2, 0.2);
    tc *= 0.55 + 0.45 * smoothstep(0.02, -0.02, q.x - lx);
    lay(col, tc, tower);
    lay(col, vec3(0.12, 0.1, 0.12), sdBox(q - vec2(lx, hy + 0.335), vec2(0.04, 0.006)));
    vec2 L = vec2(lx, hy + 0.36);
    lay(col, vec3(1.0, 0.85, 0.45), sdBox(q - L, vec2(0.022, 0.02)));
    lay(col, vec3(0.5, 0.12, 0.12), sdTri(q, L + vec2(-0.032, 0.02), L + vec2(0.032, 0.02), L + vec2(0.0, 0.055)));
    // the sweeping beam
    float ang = T * 0.45;
    float bx = cos(ang);
    vec2 rel = q - L;
    if (rel.x * bx > 0.0) {
        float along = abs(rel.x);
        float spread = 0.008 + along * 0.13;
        col += vec3(1.0, 0.92, 0.7) * exp(-pow(rel.y / spread, 2.0)) * exp(-along * 1.1) * pow(abs(bx), 0.6) * 0.5;
    }
    col += vec3(1.0, 0.9, 0.6) * exp(-length(rel) * 22.0) * (0.5 + 0.6 * smoothstep(0.7, 1.0, sin(ang)));
    return col;
}

// ================================================================ 6: a fox watching the aurora
float hill(float x, float base, float a, float f, float ph) { return base + a * sin(x * f + ph) + a * 0.5 * sin(x * f * 2.3 + ph * 1.7); }
float pine(vec2 p, float h) {
    float d = sdBox(p - vec2(0.0, h * 0.08), vec2(h * 0.04, h * 0.1));
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float y0 = h * (0.15 + 0.28 * fi);
        float w = h * (0.32 - 0.08 * fi);
        d = min(d, sdTri(p, vec2(-w, y0), vec2(w, y0), vec2(0.0, y0 + h * 0.45)));
    }
    return d;
}
vec3 sFox(vec2 q, float T) {
    vec3 col = skyGrad(q);
    col += vec3(1.0) * stars(q, 55.0, T) * 0.8;
    float line = q.y - 0.15 - 0.08 * sin(q.x * 2.0 + T * 0.08) - 0.06 * (fbm(vec2(q.x * 3.0 + T * 0.04, T * 0.02)) - 0.5);
    float curt = 0.6 + 0.4 * noise(vec2(q.x * 14.0 - line * 4.0, T * 0.25));
    vec3 ac = mix(vec3(0.2, 0.95, 0.6), vec3(0.55, 0.35, 0.95), smoothstep(-0.6, 0.8, q.x));
    col += ac * exp(-line * line / 0.003) * curt * 0.55;
    col += ac * exp(-max(line, 0.0) / 0.14) * step(0.0, line) * curt * 0.12;
    float h1 = hill(q.x, -0.14, 0.035, 2.0, 0.5);
    for (int i = 0; i < 9; i++) {
        float px = -0.85 + float(i) * 0.21 + 0.05 * hash(vec2(float(i), 2.0));
        float ph = 0.07 + 0.05 * hash(vec2(float(i), 5.0));
        lay(col, vec3(0.04, 0.07, 0.1), pine(q - vec2(px, hill(px, -0.14, 0.035, 2.0, 0.5) - 0.01), ph));
    }
    if (q.y < h1) col = mix(vec3(0.2, 0.28, 0.42), vec3(0.32, 0.42, 0.58), smoothstep(-0.3, -0.1, q.y)) * 0.75 + ac * 0.05;
    float h2 = hill(q.x, -0.29, 0.05, 1.3, -0.4);
    if (q.y < h2) col = mix(vec3(0.42, 0.5, 0.68), vec3(0.6, 0.68, 0.84), smoothstep(-0.5, h2, q.y)) * 0.7 + ac * 0.06;
    // the fox, seen from behind
    vec2 fp = vec2(0.2, hill(0.2, -0.29, 0.05, 1.3, -0.4) - 0.005);
    vec2 lp = q - fp;
    vec3 fur = vec3(0.85, 0.42, 0.16), dark = vec3(0.35, 0.14, 0.06);
    float tail = 1.0;
    float tip = 1.0;
    for (int i = 0; i < 8; i++) {
        float s = float(i) / 7.0, s2 = float(i + 1) / 7.0;
        float sw = 0.02 * sin(T * 0.7);
        vec2 a = vec2(0.03 + 0.09 * s + sw * s, 0.01 + 0.03 * sin(s * 3.0) - 0.01 * s);
        vec2 b = vec2(0.03 + 0.09 * s2 + sw * s2, 0.01 + 0.03 * sin(s2 * 3.0) - 0.01 * s2);
        float d = sdSeg(lp, a, b) - 0.024 * (1.0 - 0.5 * s * s);
        tail = min(tail, d);
        if (i >= 6) tip = min(tip, d);
    }
    lay(col, fur * 0.85, tail);
    lay(col, vec3(0.95, 0.93, 0.9), tip);
    float body = sdEll(lp - vec2(0.0, 0.055), vec2(0.045, 0.06));
    float head = sdCircle(lp - vec2(0.0, 0.13), 0.034);
    float ears = min(sdTri(lp, vec2(-0.032, 0.14), vec2(-0.03, 0.2), vec2(-0.006, 0.158)), sdTri(lp, vec2(0.032, 0.14), vec2(0.03, 0.2), vec2(0.006, 0.158)));
    float fox = min(smin(body, head, 0.018), ears);
    lay(col, mix(dark, fur, smoothstep(-0.02, 0.1, lp.y)), fox);
    lay(col, dark * 0.7, min(sdTri(lp, vec2(-0.026, 0.15), vec2(-0.026, 0.188), vec2(-0.012, 0.16)), sdTri(lp, vec2(0.026, 0.15), vec2(0.026, 0.188), vec2(0.012, 0.16))));
    col += ac * 0.45 * fill(fox) * smoothstep(-0.007, 0.0, fox) * step(0.08, lp.y);
    // light snow
    col += vec3(0.9, 0.95, 1.0) * stars(q + vec2(T * 0.01, T * 0.03), 30.0, 0.0) * 0.35;
    return col;
}

// ================================================================ 7: hot-air balloons at sunset
float cloudD(vec2 p, float seed) {
    float d = 1.0;
    for (int i = 0; i < 5; i++) {
        float fi = float(i);
        float r = 0.03 + 0.03 * hash(vec2(fi, seed));
        d = smin(d, sdCircle(p - vec2((fi - 2.0) * 0.04, r * 0.5), r), 0.02);
    }
    return max(d, -p.y);
}
vec3 sBalloons(vec2 q, float T) {
    vec3 col = mix(vec3(0.95, 0.6, 0.42), vec3(0.16, 0.14, 0.34), smoothstep(-0.35, 0.45, q.y));
    col = mix(col, vec3(0.78, 0.4, 0.5), exp(-pow((q.y + 0.05) * 4.0, 2.0)) * 0.4);
    vec2 sc = vec2(-0.35, -0.3);
    col += vec3(1.0, 0.7, 0.4) * 0.35 * exp(-length(q - sc) * 5.0);
    lay(col, vec3(1.0, 0.82, 0.55), sdCircle(q - sc, 0.07));
    for (int i = 0; i < 6; i++) {
        float fi = float(i);
        vec2 h = hash2(vec2(fi, 9.0));
        vec2 cp = vec2(mod(h.x * 2.0 + T * 0.006 * (1.0 + h.y), 2.2) - 1.1, -0.1 + 0.45 * h.y);
        if (abs(q.y - cp.y - 0.05) > 0.12 || abs(q.x - cp.x) > 0.3) continue;
        float cd = cloudD((q - cp) / (0.8 + h.y), fi);
        layA(col, mix(vec3(0.95, 0.65, 0.6), vec3(1.0, 0.85, 0.75), smoothstep(0.0, 0.05, q.y - cp.y)), cd, 0.75);
    }
    float sea = -0.33 + 0.015 * sin(q.x * 9.0 + T * 0.2) + 0.012 * noise(vec2(q.x * 20.0, T * 0.1));
    if (q.y < sea) col = mix(vec3(0.95, 0.7, 0.65), vec3(0.7, 0.5, 0.6), smoothstep(sea, sea - 0.15, q.y));
    for (int k = 0; k < 7; k++) {
        float i = float(k);
        vec2 h = hash2(vec2(i, 4.0));
        float s = 0.4 + 0.1 * i;
        vec2 pos = vec2(mod(h.x * 2.4 + T * 0.006 * (0.6 + h.y), 2.4) - 1.2, -0.15 + 0.5 * h.y + 0.02 * sin(T * 0.3 + i));
        if (length(q - pos) > 0.16 * s) continue;
        vec2 lp = (q - pos) / s;
        float wy = 0.07 * (0.72 + 0.28 * smoothstep(-0.08, 0.03, lp.y));
        float env = (length(vec2(lp.x / wy, (lp.y - 0.01) / 0.08)) - 1.0) * 0.07 * s;
        float stripe = mod(floor(lp.x / wy * 3.5 + 3.5), 3.0);
        vec3 b0 = mix(vec3(0.9, 0.3, 0.3), vec3(0.3, 0.55, 0.9), fract(h.y * 5.0));
        vec3 b1 = mix(vec3(0.98, 0.85, 0.35), vec3(0.95, 0.95, 0.95), fract(h.x * 7.0));
        vec3 bc = stripe < 0.5 ? b0 : stripe < 1.5 ? b1 : b0 * 0.85;
        bc *= 0.7 + 0.35 * smoothstep(0.07, -0.07, lp.x + lp.y * 0.3);
        float haze = 1.0 - 0.5 * (1.0 - (s - 0.4) / 0.6);
        vec3 cc = mix(col, bc, haze);
        lay(col, cc, env);
        float ropes = min(sdSeg(lp, vec2(-0.03, -0.06), vec2(-0.013, -0.102)), sdSeg(lp, vec2(0.03, -0.06), vec2(0.013, -0.102))) * s - 0.0012;
        layA(col, vec3(0.25, 0.18, 0.15), ropes, haze);
        lay(col, mix(col, vec3(0.45, 0.28, 0.16), haze), sdRBox(lp - vec2(0.0, -0.112), vec2(0.016, 0.012), 0.003) * s);
        float burn = smoothstep(0.85, 0.95, sin(T * 0.4 + i * 3.0));
        col += vec3(1.0, 0.6, 0.2) * burn * exp(-length(lp - vec2(0.0, -0.085)) / 0.012) * 0.6;
    }
    return col;
}

// ================================================================ 8: ghosts, pumpkins and bats
vec3 sGhosts(vec2 q, float T) {
    vec3 col = mix(vec3(0.28, 0.12, 0.32), vec3(0.06, 0.03, 0.13), smoothstep(-0.35, 0.5, q.y));
    col += vec3(1.0) * stars(q, 55.0, T) * 0.7;
    vec2 mc = vec2(0.35, 0.18);
    col += vec3(1.0, 0.55, 0.2) * 0.3 * exp(-length(q - mc) * 4.0);
    lay(col, vec3(1.0, 0.68, 0.3), sdCircle(q - mc, 0.14));
    col -= vec3(0.12, 0.08, 0.02) * fill(sdCircle(q - mc - vec2(-0.04, 0.03), 0.03)) + vec3(0.1, 0.07, 0.02) * fill(sdCircle(q - mc - vec2(0.05, -0.04), 0.02));
    // bats
    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        float ph = fract(T * 0.03 + fi * 0.27);
        vec2 bp = vec2(-1.0 + ph * 2.2, 0.1 + 0.25 * hash(vec2(fi, 1.0)) + 0.04 * sin(ph * 20.0));
        if (length(q - bp) > 0.05) continue;
        vec2 lp = (q - bp) / (0.7 + 0.3 * hash(vec2(fi, 2.0)));
        float fl = sin(T * 10.0 + fi) * 0.018;
        float bat = min(sdEll(lp, vec2(0.008, 0.006)), min(sdTri(lp, vec2(0.0, 0.0), vec2(-0.04, fl + 0.005), vec2(-0.018, -0.006)), sdTri(lp, vec2(0.0, 0.0), vec2(0.04, fl + 0.005), vec2(0.018, -0.006))));
        lay(col, vec3(0.03, 0.02, 0.05), bat);
    }
    // ground, tree, tombstone
    float g = -0.3 + 0.035 * sin(q.x * 3.0) + 0.015 * sin(q.x * 9.0);
    float tree = 1.0;
    if (q.x < -0.3 && q.y < 0.3) {
    tree = sdSeg(q, vec2(-0.62, -0.32), vec2(-0.58, 0.02)) - 0.022;
    tree = min(tree, sdSeg(q, vec2(-0.585, 0.0), vec2(-0.46, 0.13)) - 0.01);
    tree = min(tree, sdSeg(q, vec2(-0.585, 0.02), vec2(-0.72, 0.16)) - 0.009);
    tree = min(tree, sdSeg(q, vec2(-0.5, 0.08), vec2(-0.43, 0.22)) - 0.006);
    tree = min(tree, sdSeg(q, vec2(-0.68, 0.12), vec2(-0.75, 0.24)) - 0.005);
    tree = min(tree, sdSeg(q, vec2(-0.46, 0.13), vec2(-0.36, 0.14)) - 0.005);
    tree = min(tree, sdSeg(q, vec2(-0.59, -0.05), vec2(-0.66, -0.0)) - 0.007);
    }
    lay(col, vec3(0.04, 0.025, 0.06), tree);
    float tomb = sdRBox(q - vec2(0.62, g - 0.0), vec2(0.04, 0.07), 0.035);
    lay(col, vec3(0.3, 0.28, 0.36), tomb);
    lay(col, vec3(0.2, 0.18, 0.25), min(sdBox(q - vec2(0.62, g + 0.03), vec2(0.004, 0.022)), sdBox(q - vec2(0.62, g + 0.035), vec2(0.014, 0.004))));
    if (q.y < g) col = vec3(0.05, 0.03, 0.07);
    // pumpkins with flickering faces
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float px = -0.28 + fi * 0.34 + 0.05 * fi * fi;
        float s = 0.8 + 0.25 * hash(vec2(fi, 6.0));
        vec2 pc = vec2(px, -0.3 + 0.035 * sin(px * 3.0) + 0.015 * sin(px * 9.0) + 0.02 * s);
        if (length(q - pc) > 0.2) continue;
        vec2 lp = (q - pc) / s;
        float pk = smin(sdEll(lp, vec2(0.045, 0.034)), min(sdEll(lp - vec2(-0.022, 0.0), vec2(0.03, 0.032)), sdEll(lp - vec2(0.022, 0.0), vec2(0.03, 0.032))), 0.01);
        float fl = 0.75 + 0.25 * noise(vec2(T * 6.0, fi * 5.0));
        col += vec3(1.0, 0.5, 0.1) * 0.25 * fl * exp(-max(pk, 0.0) * s / 0.04);
        lay(col, vec3(0.88, 0.42, 0.1) * (0.8 + 0.2 * smoothstep(0.03, -0.03, lp.x)), pk * s);
        col -= vec3(0.15, 0.08, 0.0) * fill(abs(lp.x) * s - 0.002) * fill(pk * s) * 0.6;
        lay(col, vec3(0.2, 0.35, 0.1), sdBox(lp - vec2(0.003, 0.038), vec2(0.004, 0.008)) * s);
        float face = min(sdTri(lp, vec2(-0.025, 0.004), vec2(-0.008, 0.004), vec2(-0.016, 0.018)), sdTri(lp, vec2(0.025, 0.004), vec2(0.008, 0.004), vec2(0.016, 0.018)));
        face = min(face, max(sdBox(lp - vec2(0.0, -0.014), vec2(0.024, 0.007)), -sdTri(lp, vec2(-0.008, -0.007), vec2(0.008, -0.007), vec2(0.0, -0.016))));
        lay(col, vec3(1.0, 0.85, 0.35) * fl, face * s);
    }
    // the ghosts
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        vec2 gp = vec2(-0.2 + fi * 0.33 + 0.08 * sin(T * 0.18 + fi * 2.0), 0.02 + 0.08 * fi * (1.0 - fi * 0.4) + 0.025 * sin(T * 0.8 + fi * 2.0));
        float s = 0.85 + 0.2 * fi * (2.0 - fi);
        if (length(q - gp) > 0.2) continue;
        vec2 lp = rot((q - gp) / s, 0.12 * sin(T * 0.6 + fi));
        float hem = -0.06 + 0.01 * sin(lp.x * 70.0 + T * 4.0 + fi);
        float gd = smin(sdCircle(lp - vec2(0.0, 0.02), 0.05), sdBox(lp - vec2(0.0, -0.02), vec2(0.05, 0.045)), 0.01);
        gd = max(gd, hem - lp.y) * s;
        col += vec3(0.6, 0.7, 1.0) * 0.12 * exp(-max(gd, 0.0) / 0.02);
        layA(col, vec3(0.95, 0.96, 1.0) * (0.85 + 0.15 * smoothstep(-0.05, 0.05, lp.y)), gd, 0.9);
        float blink = step(0.04, fract(T / 4.0 + fi * 0.3));
        lay(col, vec3(0.08, 0.06, 0.12), min(sdEll(lp - vec2(-0.017, 0.028), vec2(0.008, 0.012 * blink + 0.001)), sdEll(lp - vec2(0.017, 0.028), vec2(0.008, 0.012 * blink + 0.001))) * s);
        lay(col, vec3(0.08, 0.06, 0.12), sdEll(lp - vec2(0.0, 0.004), vec2(0.007, 0.006 + 0.004 * sin(T * 1.5 + fi))) * s);
        layA(col, vec3(1.0, 0.6, 0.7), min(sdCircle(lp - vec2(-0.03, 0.012), 0.007), sdCircle(lp - vec2(0.03, 0.012), 0.007)) * s, 0.6);
    }
    return col;
}

// ================================================================ 9: penguins on the ice
float penguin(vec2 lp, float T, float flap, out float belly, out float beak, out float feet, out float eye) {
    float body = sdEll(lp - vec2(0.0, 0.07), vec2(0.036, 0.07));
    float head = sdCircle(lp - vec2(0.004, 0.135), 0.03);
    float wing = sdEll(rot(lp - vec2(-0.03, 0.075), 0.25 + flap), vec2(0.012, 0.045));
    belly = sdEll(lp - vec2(0.008, 0.062), vec2(0.024, 0.056));
    beak = sdTri(lp, vec2(0.028, 0.14), vec2(0.056, 0.132), vec2(0.028, 0.126));
    feet = min(sdEll(lp - vec2(-0.01, 0.002), vec2(0.014, 0.005)), sdEll(lp - vec2(0.016, 0.002), vec2(0.014, 0.005)));
    eye = sdCircle(lp - vec2(0.016, 0.142), 0.0055);
    return min(smin(body, head, 0.015), wing);
}
vec3 sPenguins(vec2 q, float T) {
    float hy = -0.06;
    vec3 col = mix(vec3(0.35, 0.42, 0.62), vec3(0.06, 0.08, 0.2), smoothstep(-0.1, 0.5, q.y));
    col += vec3(1.0) * stars(q, 55.0, T) * 0.6 * smoothstep(0.0, 0.3, q.y);
    col += vec3(0.25, 0.9, 0.6) * 0.18 * exp(-pow((q.y - 0.25 - 0.05 * sin(q.x * 2.5 + T * 0.1)) * 14.0, 2.0));
    float berg = min(sdTri(q, vec2(-0.85, hy), vec2(-0.45, hy), vec2(-0.66, hy + 0.12)), sdTri(q, vec2(0.4, hy), vec2(0.85, hy), vec2(0.62, hy + 0.08)));
    lay(col, vec3(0.62, 0.72, 0.88), berg);
    if (q.y < hy) {
        col = mix(vec3(0.12, 0.2, 0.36), vec3(0.04, 0.07, 0.16), smoothstep(hy, hy - 0.2, q.y));
        col += vec3(0.3, 0.4, 0.6) * smoothstep(0.85, 1.0, sin(q.x * 40.0 / (hy - q.y + 0.05) - T)) * 0.12;
    }
    float ice = -0.22 + 0.006 * sin(q.x * 12.0);
    if (q.y < ice) col = mix(vec3(0.82, 0.88, 0.96), vec3(0.55, 0.65, 0.8), smoothstep(ice, ice - 0.28, q.y)) * 0.8;
    lay(col, vec3(0.45, 0.58, 0.75) * 0.8, sdBox(q - vec2(0.0, ice - 0.006), vec2(2.0, 0.004)));
    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        float walk = i == 3 ? 1.0 : 0.0;
        float x = i == 3 ? mod(T * 0.035, 2.2) - 1.1 : -0.45 + fi * 0.22;
        float s = i == 2 ? 0.6 : 1.0 - 0.05 * fi;
        float w = walk * 0.12 * sin(T * 6.0) + (1.0 - walk) * 0.03 * sin(T * 0.5 + fi);
        vec2 lp = rot((q - vec2(x, ice + 0.004)) / s, -w);
        if (i == 1) lp.x = -lp.x;
        float belly, beak, feet, eye;
        float d = penguin(lp, T, walk * 0.25 * sin(T * 6.0) + 0.05 * sin(T + fi), belly, beak, feet, eye);
        lay(col, vec3(0.95, 0.55, 0.15), feet * s);
        lay(col, vec3(0.08, 0.09, 0.13), d * s);
        lay(col, vec3(0.95, 0.96, 0.98), belly * s);
        lay(col, vec3(0.95, 0.6, 0.15), beak * s);
        lay(col, vec3(1.0), eye * s);
        lay(col, vec3(0.0), sdCircle(lp - vec2(0.018, 0.142), 0.003) * s);
    }
    col += vec3(0.95) * stars(q + vec2(T * 0.008, T * 0.025), 35.0, 0.0) * 0.4;
    return col;
}

// ================================================================ 10: rocket climbing through space
vec3 sRocket(vec2 q, float T) {
    vec3 col = mix(vec3(0.01, 0.01, 0.04), vec3(0.05, 0.03, 0.12), smoothstep(-0.5, 0.5, q.y));
    col += vec3(0.25, 0.08, 0.35) * smoothstep(0.4, 0.9, fbm(q * 1.4 + vec2(0.0, T * 0.02))) * 0.4;
    for (int l = 0; l < 3; l++) {
        float fl = float(l);
        col += vec3(0.9, 0.93, 1.0) * stars(q + vec2(fl * 5.0, T * 0.03 * (fl + 1.0)), 30.0 + fl * 20.0, T) * (0.4 + 0.25 * fl);
    }
    float py = 0.9 - mod(T * 0.018, 2.0);
    vec2 pc = vec2(-0.55, py);
    float pd = sdCircle(q - pc, 0.11);
    lay(col, mix(vec3(0.3, 0.5, 0.75), vec3(0.6, 0.8, 0.95), 0.5 + 0.5 * sin((q.y - pc.y) * 50.0 + noise(q * 8.0) * 2.0)) * (0.5 + 0.5 * smoothstep(-0.1, 0.1, q.x - pc.x)), pd);
    vec2 pc2 = vec2(0.62, 0.9 - mod(T * 0.012 + 1.1, 2.0));
    lay(col, vec3(0.75, 0.45, 0.35) * (0.5 + 0.5 * smoothstep(-0.05, 0.05, q.x - pc2.x)), sdCircle(q - pc2, 0.05));
    vec2 rc = vec2(0.05 + 0.015 * sin(T * 0.7), -0.02 + 0.012 * sin(T * 1.1));
    vec2 lp = rot(q - rc, 0.06 * sin(T * 0.5));
    // flame
    float n1 = noise(vec2(T * 18.0, 0.0)), n2 = noise(vec2(T * 14.0, 3.0));
    float fo = sdEll(lp - vec2(0.0, -0.17), vec2(0.024 * (1.0 + 0.15 * n1), 0.065 * (1.0 + 0.25 * n2)));
    float fi = sdEll(lp - vec2(0.0, -0.15), vec2(0.013, 0.04 * (1.0 + 0.2 * n1)));
    col += vec3(1.0, 0.5, 0.15) * 0.4 * exp(-max(fo, 0.0) / 0.03);
    lay(col, vec3(1.0, 0.55, 0.15), fo);
    lay(col, vec3(1.0, 0.92, 0.6), fi);
    lay(col, vec3(0.45, 0.47, 0.52), sdRBox(lp - vec2(0.0, -0.112), vec2(0.022, 0.012), 0.004));
    vec3 red = vec3(0.85, 0.2, 0.22);
    lay(col, red * 0.85, min(sdTri(lp, vec2(-0.035, -0.04), vec2(-0.085, -0.115), vec2(-0.03, -0.1)), sdTri(lp, vec2(0.035, -0.04), vec2(0.085, -0.115), vec2(0.03, -0.1))));
    float body = sdEll(lp, vec2(0.045, 0.11));
    vec3 bc = mix(vec3(0.95, 0.96, 0.98), red, step(0.055, lp.y));
    bc *= 0.7 + 0.3 * smoothstep(0.04, -0.04, lp.x);
    lay(col, bc, body);
    lay(col, vec3(0.55, 0.58, 0.64), sdCircle(lp - vec2(0.0, 0.02), 0.026));
    lay(col, mix(vec3(0.15, 0.35, 0.6), vec3(0.45, 0.75, 0.95), smoothstep(-0.02, 0.02, lp.y - 0.02 - lp.x)), sdCircle(lp - vec2(0.0, 0.02), 0.019));
    lay(col, vec3(0.9, 0.95, 1.0), sdEll(rot(lp - vec2(-0.006, 0.028), 0.7), vec2(0.008, 0.003)));
    return col;
}

// ================================================================ 11: owl on a branch before the moon
vec3 sOwl(vec2 q, float T) {
    vec3 col = skyGrad(q);
    col += vec3(1.0) * stars(q, 55.0, T) * 0.7;
    vec2 mc = vec2(0.28, 0.08);
    col += vec3(1.0, 0.95, 0.75) * 0.25 * exp(-length(q - mc) * 4.5);
    lay(col, vec3(0.98, 0.95, 0.82), sdCircle(q - mc, 0.2));
    col -= vec3(0.06, 0.06, 0.04) * fill(sdCircle(q - mc - vec2(-0.07, 0.06), 0.04)) + vec3(0.05) * fill(sdCircle(q - mc - vec2(0.08, -0.05), 0.03));
    for (int i = 0; i < 8; i++) {
        if (q.y > 0.0) break;
        float px = -0.95 + float(i) * 0.26 + 0.06 * hash(vec2(float(i), 8.0));
        if (abs(q.x - px) > 0.12) continue;
        lay(col, vec3(0.02, 0.05, 0.06), pine(q - vec2(px, -0.5), 0.22 + 0.12 * hash(vec2(float(i), 9.0))));
    }
    float by = -0.12;
    float br = sdSeg(q, vec2(-1.0, by - 0.03), vec2(0.5, by + 0.01)) - 0.016 * (1.0 - 0.4 * smoothstep(-1.0, 0.5, q.x));
    br = min(br, sdSeg(q, vec2(-0.2, by - 0.008), vec2(-0.1, by + 0.07)) - 0.006);
    br = min(br, sdSeg(q, vec2(0.38, by + 0.005), vec2(0.48, by + 0.06)) - 0.004);
    lay(col, vec3(0.16, 0.1, 0.07), br);
    for (int i = 0; i < 6; i++) {
        vec2 lc = vec2(-0.6 + float(i) * 0.2, by + 0.015 + 0.01 * sin(float(i) * 3.0));
        lay(col, vec3(0.12, 0.3, 0.16), sdEll(rot(q - lc, 0.6 + 0.08 * sin(T + float(i))), vec2(0.025, 0.01)));
    }
    // the owl
    vec2 op = vec2(0.24, by + 0.012);
    vec2 lp = q - op;
    if (length(lp - vec2(0.0, 0.12)) < 0.2) {
    float tilt = 0.18 * sin(T * 0.25) * smoothstep(0.3, 0.8, sin(T * 0.12));
    vec2 hp = rot(lp - vec2(0.0, 0.14), tilt) + vec2(0.0, 0.14);
    vec3 brown = vec3(0.42, 0.29, 0.19), light = vec3(0.72, 0.56, 0.38), disc = vec3(0.82, 0.68, 0.5);
    float body = sdEll(lp - vec2(0.0, 0.075), vec2(0.068, 0.088));
    float head = sdEll(hp - vec2(0.0, 0.165), vec2(0.07, 0.058));
    float tufts = min(sdTri(hp, vec2(-0.06, 0.19), vec2(-0.062, 0.245), vec2(-0.03, 0.212)), sdTri(hp, vec2(0.06, 0.19), vec2(0.062, 0.245), vec2(0.03, 0.212)));
    lay(col, brown * 0.75, min(sdEll(rot(lp - vec2(-0.06, 0.07), 0.15), vec2(0.022, 0.07)), sdEll(rot(lp - vec2(0.06, 0.07), -0.15), vec2(0.022, 0.07))));
    lay(col, brown, smin(body, min(head, tufts), 0.02));
    float bel = sdEll(lp - vec2(0.0, 0.06), vec2(0.045, 0.065));
    lay(col, light, bel);
    float chev = abs(fract(lp.y * 45.0 + abs(lp.x) * 25.0) - 0.5);
    col -= vec3(0.12, 0.09, 0.05) * fill(bel) * smoothstep(0.12, 0.05, chev) * step(lp.y, 0.11);
    lay(col, disc, min(sdCircle(hp - vec2(-0.028, 0.165), 0.033), sdCircle(hp - vec2(0.028, 0.165), 0.033)));
    vec2 look = vec2(0.008 * sin(T * 0.3), 0.004 * sin(T * 0.23));
    float blink = step(0.97, fract(T / 6.0));
    float eyeL = sdCircle(hp - vec2(-0.028, 0.167), 0.021), eyeR = sdCircle(hp - vec2(0.028, 0.167), 0.021);
    lay(col, vec3(1.0, 0.78, 0.2), min(eyeL, eyeR));
    lay(col, vec3(0.05, 0.03, 0.02), min(sdCircle(hp - vec2(-0.028, 0.167) - look, 0.011), sdCircle(hp - vec2(0.028, 0.167) - look, 0.011)));
    lay(col, vec3(1.0), min(sdCircle(hp - vec2(-0.032, 0.172) - look, 0.0035), sdCircle(hp - vec2(0.024, 0.172) - look, 0.0035)));
    lay(col, brown * 0.9, max(min(eyeL, eyeR), blink > 0.5 ? -1.0 : 1.0));
    lay(col, vec3(0.85, 0.55, 0.2), sdTri(hp, vec2(-0.008, 0.155), vec2(0.008, 0.155), vec2(0.0, 0.132)));
    lay(col, vec3(0.75, 0.6, 0.3), min(sdEll(lp - vec2(-0.02, 0.0), vec2(0.012, 0.006)), sdEll(lp - vec2(0.02, 0.0), vec2(0.012, 0.006))));
    }
    for (int i = 0; i < 6; i++) {
        vec2 fp = vec2(-0.7 + 1.4 * hash(vec2(float(i), 1.0)) + 0.05 * sin(T * 0.5 + float(i)), -0.4 + 0.25 * hash(vec2(float(i), 2.0)) + 0.03 * cos(T * 0.4 + float(i)));
        col += vec3(0.85, 1.0, 0.4) * exp(-length(q - fp) * 120.0) * (0.5 + 0.5 * sin(T * 1.5 + float(i) * 2.0));
    }
    return col;
}

// ================================================================ 12: 8-bit city with a night train
vec3 sPixel(vec2 q, float T) {
    float px = 6.0 / size.y;
    vec2 p = (floor(q / px) + 0.5) * px;
    float k = floor((p.y + 0.5) * 9.0) / 9.0;
    vec3 col = mix(vec3(0.85, 0.42, 0.5), vec3(0.1, 0.06, 0.22), smoothstep(0.1, 0.9, k));
    vec2 pi = floor(q / px);
    float st = hash(pi);
    col += vec3(1.0) * step(0.992, st) * step(0.5, fract(T * 0.3 + st * 10.0)) * step(0.05, p.y) * 0.8;
    vec2 mc = vec2(-0.5, 0.3);
    lay(col, vec3(1.0, 0.95, 0.8), length(p - mc) - 0.06);
    float bx = floor(p.x / 0.07);
    float bh = -0.08 + 0.2 * hash(vec2(bx, 3.0));
    if (p.y < bh) {
        col = vec3(0.2, 0.12, 0.3);
        vec2 w = floor(vec2(p.x / (px * 3.0), p.y / (px * 4.0)));
        float on = step(0.6, hash(w + floor(T * 0.05 + hash(w) * 3.0)));
        col = mix(col, vec3(1.0, 0.8, 0.45), on * step(1.0, mod(w.x, 2.0)) * step(1.0, mod(w.y, 2.0)) * 0.7);
    }
    float bx2 = floor((p.x + 0.03) / 0.11);
    float bh2 = -0.2 + 0.16 * hash(vec2(bx2, 5.0));
    if (p.y < bh2) {
        col = vec3(0.08, 0.05, 0.14);
        vec2 w = floor(vec2(p.x / (px * 2.0), p.y / (px * 3.0)));
        float on = step(0.7, hash(w * 1.3 + floor(T * 0.04 + hash(w) * 5.0)));
        col = mix(col, vec3(0.45, 0.9, 1.0), on * step(1.0, mod(w.x, 2.0)) * step(1.0, mod(w.y, 2.0)) * 0.6);
    }
    float ty = -0.3;
    if (abs(p.y - ty) < px * 1.0) col = vec3(0.3, 0.25, 0.35);
    if (p.y < ty - px && mod(floor(p.x / 0.15), 1.0) < 1.0 && abs(mod(p.x, 0.15) - 0.075) < px * 1.5) col = vec3(0.15, 0.12, 0.2);
    float tx = mod(T * 0.12, 3.2) - 1.6;
    float rel = p.x - tx;
    if (rel > 0.0 && rel < 0.7 && p.y > ty + px && p.y < ty + px * 8.0) {
        float car = mod(rel, 0.14);
        if (car > px) {
            col = vec3(0.2, 0.6, 0.6);
            if (p.y > ty + px * 4.0 && p.y < ty + px * 7.0 && mod(floor(car / (px * 3.0)), 2.0) < 1.0) col = vec3(1.0, 0.9, 0.55);
        }
    }
    if (p.y < ty - px * 6.0) col = vec3(0.05, 0.03, 0.08);
    return col;
}

// ================================================================ 13: paper boats under sky lanterns
vec3 lanterns(vec2 q, float T, bool refl) {
    vec3 col = vec3(0.0);
    for (int i = 0; i < 16; i++) {
        float fi = float(i);
        vec2 h = hash2(vec2(fi, 11.0));
        float s = 0.45 + 0.6 * h.y;
        vec2 lp = vec2(q.x - ((h.x - 0.5) * 1.8 + 0.04 * sin(T * 0.25 + fi)), q.y - (mod(T * 0.008 * (0.5 + h.y) + h.x * 1.3, 1.3) - 0.55));
        lp /= s;
        float fl = 0.8 + 0.2 * noise(vec2(T * 4.0, fi));
        col += vec3(1.0, 0.55, 0.2) * exp(-length(lp) / 0.03) * 0.18 * fl;
        float d = sdRBox(vec2(lp.x * (1.0 + lp.y * 12.0), lp.y), vec2(0.011, 0.016), 0.004) * s;
        col += mix(vec3(0.0), mix(vec3(1.0, 0.75, 0.35), vec3(1.0, 0.45, 0.15), smoothstep(-0.016, 0.016, lp.y)) * fl, fill(d));
    }
    return col;
}
vec3 sBoats(vec2 q, float T) {
    float hy = -0.1;
    vec3 col = mix(vec3(0.55, 0.28, 0.45), vec3(0.06, 0.06, 0.18), smoothstep(-0.1, 0.5, q.y));
    col += vec3(1.0) * stars(q, 55.0, T) * 0.6 * smoothstep(0.1, 0.4, q.y);
    if (q.y > hy) col += lanterns(q, T, false);
    else {
        float depth = hy - q.y;
        vec2 rq = vec2(q.x + 0.01 * sin(q.y * 80.0 + T * 1.5), 2.0 * hy - q.y);
        col = mix(vec3(0.35, 0.18, 0.35), vec3(0.03, 0.04, 0.12), smoothstep(0.0, 0.3, depth));
        col += lanterns(rq, T, true) * 0.45 * (1.0 - smoothstep(0.0, 0.4, depth));
        float z = 1.0 / (depth + 0.02);
        col += vec3(0.5, 0.35, 0.5) * smoothstep(0.88, 1.0, sin(z * 4.0 - T * 0.9 + 3.0 * noise(vec2(q.x * z * 1.2, z * 0.6)))) * 0.1;
    }
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float s = 0.7 + 0.25 * fi;
        vec2 bp = vec2(-0.45 + fi * 0.42 + 0.03 * sin(T * 0.1 + fi), hy - 0.06 - 0.09 * fi + 0.006 * sin(T * 1.3 + fi));
        vec2 lp = rot((q - bp) / s, 0.08 * sin(T * 1.1 + fi * 2.0));
        float hull = min(sdTri(lp, vec2(-0.07, 0.02), vec2(0.07, 0.02), vec2(0.04, -0.016)), sdTri(lp, vec2(-0.07, 0.02), vec2(0.04, -0.016), vec2(-0.04, -0.016)));
        float sailL = sdTri(lp, vec2(0.0, 0.075), vec2(-0.036, 0.02), vec2(0.0, 0.02));
        float sailR = sdTri(lp, vec2(0.0, 0.075), vec2(0.0, 0.02), vec2(0.036, 0.02));
        col *= 1.0 - 0.3 * fill(sdEll(lp - vec2(0.0, -0.02), vec2(0.08, 0.008)) * s);
        lay(col, vec3(0.88, 0.86, 0.82), hull * s);
        lay(col, vec3(0.95, 0.94, 0.9), sailL * s);
        lay(col, vec3(0.74, 0.72, 0.7), sailR * s);
        col += vec3(1.0, 0.6, 0.3) * 0.08 * fill(min(hull, min(sailL, sailR)) * s);
    }
    return col;
}

// ================================================================ 14: aquarium
vec3 sAquarium(vec2 q, float T) {
    vec3 col = mix(vec3(0.02, 0.12, 0.2), vec3(0.08, 0.35, 0.45), smoothstep(-0.5, 0.5, q.y));
    float ray = 0.0;
    for (int i = 0; i < 3; i++) ray += pow(0.5 + 0.5 * sin(q.x * (6.0 + float(i)) - q.y * 2.0 + T * 0.2 * float(i + 1)), 12.0);
    col += vec3(0.5, 0.8, 0.8) * ray * 0.06 * smoothstep(-0.3, 0.5, q.y);
    float sand = -0.38 + 0.02 * sin(q.x * 5.0) + 0.01 * sin(q.x * 13.0);
    for (int i = 0; i < 9; i++) {
        float fi = float(i);
        float x0 = -0.85 + fi * 0.21 + 0.05 * hash(vec2(fi, 4.0));
        float top = -0.05 + 0.2 * hash(vec2(fi, 6.0));
        float t = clamp((q.y - sand) / (top - sand), 0.0, 1.0);
        float off = 0.035 * sin(q.y * 9.0 + T * 1.0 + fi) * t;
        float d = abs(q.x - x0 - off) - 0.008 * (1.0 - 0.7 * t);
        d = max(d, max(sand - q.y, q.y - top));
        lay(col, mix(vec3(0.1, 0.35, 0.2), vec3(0.25, 0.6, 0.3), t), d);
    }
    for (int i = 0; i < 14; i++) {
        float fi = float(i);
        vec2 h = hash2(vec2(fi, 2.0));
        float a = T * (0.12 + 0.08 * h.y) + fi * 0.45;
        vec2 fp = vec2(0.6 * sin(a) + 0.15 * sin(a * 2.3 + fi), -0.05 + 0.25 * (h.y - 0.5) + 0.05 * sin(a * 1.7));
        float dir = cos(a) >= 0.0 ? 1.0 : -1.0;
        float s = 0.6 + 0.6 * h.x;
        vec2 lp = (q - fp) / s;
        lp.x *= dir;
        float body = sdEll(lp, vec2(0.026, 0.013));
        float tail = sdTri(lp, vec2(-0.02, 0.0), vec2(-0.042, 0.013 + 0.004 * sin(T * 8.0 + fi)), vec2(-0.042, -0.013 + 0.004 * sin(T * 8.0 + fi)));
        float fd = min(body, tail) * s;
        float pal = fract(h.x * 3.3);
        vec3 fc = pal < 0.33 ? vec3(1.0, 0.55, 0.15) : pal < 0.66 ? vec3(0.25, 0.6, 1.0) : vec3(1.0, 0.85, 0.2);
        fc = mix(fc, vec3(0.95), step(0.6, fract(lp.x * 40.0)) * step(0.0, lp.x) * 0.6);
        lay(col, fc, fd);
        lay(col, vec3(0.02), sdCircle(lp - vec2(0.014, 0.003), 0.003) * s);
    }
    if (q.y < sand) {
        col = mix(vec3(0.75, 0.65, 0.45), vec3(0.55, 0.45, 0.3), noise(q * 60.0)) * 0.7;
        col *= 0.8 + 0.2 * smoothstep(sand - 0.1, sand, q.y);
    }
    for (int b = 0; b < 8; b++) {
        float ph = fract(T * 0.08 + float(b) * 0.125);
        vec2 bp = vec2(0.55 + 0.012 * sin(ph * 25.0 + float(b)), sand + ph * 0.9);
        float bd = abs(length(q - bp) - 0.006 - 0.004 * hash(vec2(float(b), 3.0))) - 0.0015;
        lay(col, vec3(0.8, 0.95, 1.0), bd);
    }
    return col;
}

// ================================================================ 15: bunny on a cloud fishing for a star
vec3 sBunny(vec2 q, float T) {
    vec3 col = skyGrad(q);
    col += vec3(1.0) * stars(q, 50.0, T) * 0.9;
    vec2 mc = vec2(-0.45, 0.12);
    col += vec3(1.0, 0.85, 0.5) * 0.18 * exp(-length(q - mc) * 4.0);
    lay(col, vec3(1.0, 0.86, 0.5), max(sdCircle(q - mc, 0.16), -sdCircle(q - mc - vec2(0.07, 0.045), 0.15)));
    // cloud
    vec2 cc = vec2(0.12, -0.2 + 0.01 * sin(T * 0.4));
    float cd = 1.0;
    for (int i = 0; i < 7; i++) {
        float fi = float(i);
        cd = smin(cd, sdCircle(q - cc - vec2((fi - 3.0) * 0.05, 0.02 * sin(fi * 2.1)), 0.045 + 0.02 * hash(vec2(fi, 1.0))), 0.03);
    }
    cd = max(cd, -(q.y - cc.y + 0.035));
    col += vec3(0.6, 0.6, 0.9) * 0.1 * exp(-max(cd, 0.0) / 0.03);
    lay(col, mix(vec3(0.62, 0.62, 0.82), vec3(0.88, 0.88, 1.0), smoothstep(-0.03, 0.06, q.y - cc.y)), cd);
    // rod + line + star
    vec2 bp = cc + vec2(0.0, 0.035);
    vec2 hand = bp + vec2(0.035, 0.075);
    vec2 tipP = vec2(0.45, 0.28);
    float sway = 0.04 * sin(T * 0.6);
    vec2 star = vec2(0.45 + sway, 0.02 + 0.02 * sin(T * 0.8));
    lay(col, vec3(0.45, 0.3, 0.2), sdSeg(q, hand, tipP) - 0.004);
    lay(col, vec3(0.85, 0.85, 0.95), sdSeg(q, tipP, star) - 0.0012);
    col += vec3(1.0, 0.9, 0.5) * 0.35 * exp(-length(q - star) * 18.0);
    lay(col, vec3(1.0, 0.9, 0.45), sdStar5(rot(q - star, 0.25 * sin(T * 0.6)), 0.035, 0.45));
    // the bunny
    vec2 lp = q - bp;
    vec3 fur = vec3(0.93, 0.9, 0.95), pink = vec3(0.95, 0.65, 0.75);
    float body = sdEll(lp - vec2(0.0, 0.04), vec2(0.04, 0.045));
    float head = sdCircle(lp - vec2(0.008, 0.105), 0.034);
    float earL = sdEll(rot(lp - vec2(-0.012, 0.16), 0.2), vec2(0.011, 0.045));
    float earR = sdEll(rot(lp - vec2(0.03, 0.15), -0.7 - 0.1 * sin(T * 0.9)), vec2(0.011, 0.042));
    float bun = min(smin(body, head, 0.015), min(earL, earR));
    lay(col, fur * 0.9, sdCircle(lp - vec2(-0.04, 0.02), 0.016));
    lay(col, fur * (0.85 + 0.15 * smoothstep(-0.04, 0.04, lp.x)), bun);
    lay(col, pink, min(sdEll(rot(lp - vec2(-0.012, 0.16), 0.2), vec2(0.005, 0.034)), sdEll(rot(lp - vec2(0.03, 0.15), -0.7 - 0.1 * sin(T * 0.9)), vec2(0.005, 0.031))));
    float blink = step(0.04, fract(T / 4.5));
    lay(col, vec3(0.1, 0.08, 0.12), sdEll(lp - vec2(0.022, 0.112), vec2(0.005, 0.006 * blink + 0.001)));
    lay(col, pink, sdCircle(lp - vec2(0.04, 0.1), 0.005));
    layA(col, pink, sdCircle(lp - vec2(0.022, 0.092), 0.008), 0.5);
    lay(col, fur, sdCircle(lp - (hand - bp), 0.012));
    return col;
}

void main() {
    vec2 uv = qt_TexCoord0;
    float asp = size.x / max(size.y, 1.0);
    vec2 q = vec2((uv.x - 0.5) * asp, 0.5 - uv.y);
    AA = 1.0 / max(size.y, 1.0);
    float T = time;
    float e = opt.w;
    int m = int(mode + 0.5);
    vec3 col;
    if (m == 0) col = sCat(q, T);
    else if (m == 1) col = sWhales(q, T);
    else if (m == 2) col = sJelly(q, T);
    else if (m == 3) col = sKoi(q, T);
    else if (m == 4) col = sAstro(q, T);
    else if (m == 5) col = sLighthouse(q, T);
    else if (m == 6) col = sFox(q, T);
    else if (m == 7) col = sBalloons(q, T);
    else if (m == 8) col = sGhosts(q, T);
    else if (m == 9) col = sPenguins(q, T);
    else if (m == 10) col = sRocket(q, T);
    else if (m == 11) col = sOwl(q, T);
    else if (m == 12) col = sPixel(q, T);
    else if (m == 13) col = sBoats(q, T);
    else if (m == 14) col = sAquarium(q, T);
    else col = sBunny(q, T);

    col *= 1.0 + 0.1 * e;
    vec2 p = vec2(q.x / asp, q.y);
    col *= mix(1.0, 1.0 - 1.2 * dot(p, p), opt.y);
    col *= opt.z;
    // fixed sub-LSB dither: dark gradients don't band
    col += (hash(gl_FragCoord.xy) + hash(gl_FragCoord.xy + 17.3) - 1.0) / 255.0;
    fragColor = vec4(max(col, 0.0), 1.0) * qt_Opacity;
}
