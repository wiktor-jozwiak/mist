#version 440
// Lively live wallpapers (components/LiveWall.qml when a preset has shader: "fx"). Same uniform block as
// live.frag. Coordinates: q with y UP, height = 1, x = +-aspect / 2; T = seconds (tempo-scaled).
// mode 0 warp speed, 1 neon tunnel, 2 code rain, 3 plasma, 4 thunderstorm (soft, rare flashes),
// 5 campfire, 6 fireworks, 7 spiral galaxy, 8 black hole, 9 ocean swell, 10 kaleidoscope, 11 plexus,
// 12 equalizer (music lifts it), 13 orbits, 14 soap bubbles, 15 meteor shower, 16 meadow in the wind,
// 17 highway light trails, 18 vortex, 19 neon ribbons, 20 hologram globe, 21 DNA helix, 22 aurora over a
// mountain lake, 23 liquid chrome, 24 neon city in the rain, 25 ringed planet, 26 layered waves.
// c0 = base / shadow, c1..c3 = lights. opt = stars, vignette, brightness, music energy.
// Nothing strobes: flashes are rare, soft and local.
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
const float PI = 3.14159265;
const float TAU = 6.2831853;

float hash(vec2 p) { vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
vec2 hash2(vec2 p) { return vec2(hash(p), hash(p + 19.19)); }
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
float fbm(vec2 p) {
    float v = 0.0, a = 0.5;
    mat2 m = mat2(1.6, 1.2, -1.2, 1.6);
    for (int i = 0; i < 5; i++) { v += a * noise(p); p = m * p; a *= 0.5; }
    return v;
}
vec2 rot(vec2 p, float a) { float c = cos(a), s = sin(a); return vec2(c * p.x - s * p.y, s * p.x + c * p.y); }
float sdSeg(vec2 p, vec2 a, vec2 b) { vec2 pa = p - a, ba = b - a; float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0); return length(pa - ba * h); }
float fill(float d) { return smoothstep(AA, -AA, d); }
vec3 pal(float t) { return t < 0.5 ? mix(c1.rgb, c2.rgb, t * 2.0) : mix(c2.rgb, c3.rgb, t * 2.0 - 1.0); }
float stars(vec2 q, float sc, float t) {
    vec2 g = q * sc;
    vec2 id = floor(g), f = fract(g) - 0.5;
    float h = hash(id);
    if (h < 0.92) return 0.0;
    vec2 o = (hash2(id) - 0.5) * 0.6;
    float tw = 0.55 + 0.45 * sin(t * (0.8 + 2.5 * fract(h * 91.0)) + h * 40.0);
    return smoothstep(0.08 + 0.06 * fract(h * 13.0), 0.0, length(f - o)) * tw;
}

// ---------------------------------------------------------------- 0: warp speed
vec3 sWarp(vec2 q, float T) {
    float r = length(q), a = atan(q.y, q.x);
    vec3 col = c0.rgb + c1.rgb * 0.35 * exp(-r * 5.0);
    for (int l = 0; l < 3; l++) {
        float L = float(l);
        float n = 120.0 + 70.0 * L;
        float ai = floor((a / TAU + 0.5) * n);
        float h = hash(vec2(ai, L + 1.0));
        if (h < 0.35) continue;
        float ac = (ai + 0.5) / n * TAU - PI;
        float da = abs(a - ac) * r;
        float z = fract(h * 7.0 + T * (0.18 + 0.1 * L) * (0.6 + h));
        float rs = z * z * 1.3, len = 0.02 + 0.3 * z * z;
        float on = smoothstep(rs - len, rs - len * 0.6, r) * smoothstep(rs, rs - 0.01, r);
        col += mix(c2.rgb, c3.rgb, fract(h * 5.0)) * on * smoothstep(0.0012 + 0.003 * z, 0.0, da) * (0.25 + 0.9 * z);
    }
    return col;
}

// ---------------------------------------------------------------- 1: neon tunnel
vec3 sTunnel(vec2 q, float T) {
    q = rot(q, 0.15 * sin(T * 0.2));
    float r = length(q), a = atan(q.y, q.x);
    float z = 0.6 / (r + 0.015) + T * 1.6;
    vec2 uv = vec2(a / TAU * 16.0, z);
    vec2 w = fwidth(uv);
    vec2 dl = abs(fract(uv + 0.5) - 0.5);
    float lx = 1.0 - smoothstep(0.02 + 0.5 * w.x, 0.02 + 1.5 * w.x, dl.x);
    float lz = 1.0 - smoothstep(0.04 + 0.5 * w.y, 0.04 + 1.5 * w.y, dl.y);
    float line = max(lx * 0.6, lz);
    vec3 lc = mix(c2.rgb, c3.rgb, 0.5 + 0.5 * sin(z * 0.4 + T * 0.3));
    vec3 col = c0.rgb + lc * line * smoothstep(0.01, 0.35, r) * 0.9;
    col += c1.rgb * 0.4 * exp(-r * 9.0);
    return col;
}

// ---------------------------------------------------------------- 2: code rain
vec3 sCode(vec2 uv, float T) {
    vec2 px = uv * size;
    vec2 cell = vec2(14.0, 18.0);
    vec2 id = floor(px / cell), f = fract(px / cell);
    float rows = size.y / cell.y;
    vec3 col = c0.rgb;
    for (int l = 0; l < 2; l++) {
        float L = float(l);
        vec2 lid = id + vec2(L * 0.5, 0.0);
        float h = hash(vec2(lid.x, 3.0 + L));
        float sp = (5.0 + 9.0 * h) * (1.0 - 0.4 * L);
        float head = mod(T * sp + h * 200.0, rows + 40.0);
        float d = head - id.y;
        float b = d >= 0.0 && d < 30.0 ? exp(-d * 0.11) : 0.0;
        vec2 gp = floor(f * vec2(5.0, 7.0));
        float glyph = step(0.45, hash(gp + lid * 7.0 + floor(T * 1.5 + hash(lid) * 10.0) * 13.0));
        glyph *= step(0.5, gp.x) * step(gp.x, 3.5) * step(0.5, gp.y) * step(gp.y, 5.5);
        float k = L > 0.5 ? 0.35 : 1.0;
        col += c2.rgb * glyph * b * 0.85 * k;
        col += c3.rgb * glyph * step(0.0, d) * step(d, 1.0) * 0.9 * k;
        if (L < 0.5) break;
    }
    return col;
}

// ---------------------------------------------------------------- 3: plasma
vec3 sPlasma(vec2 q, float T) {
    float t = T * 0.35;
    float v = sin(q.x * 5.0 + t) + sin(q.y * 6.0 - t * 0.8) + sin((q.x + q.y) * 4.0 + t * 0.6)
            + sin(length(q - vec2(0.35 * sin(t * 0.5), 0.2 * cos(t * 0.7))) * 10.0 - t * 1.4);
    v = 0.5 + 0.125 * v;
    vec3 col = pal(fract(v + t * 0.05));
    col *= 0.55 + 0.45 * smoothstep(0.2, 0.9, 0.5 + 0.5 * sin(v * TAU * 2.0));
    return mix(c0.rgb, col, 0.55);
}

// ---------------------------------------------------------------- 4: thunderstorm
vec3 sStorm(vec2 q, float T) {
    vec3 col = mix(c0.rgb, c1.rgb, smoothstep(-0.5, 0.5, q.y) * 0.6);
    float cl = fbm(vec2(q.x * 2.0 + T * 0.06, q.y * 3.0 - T * 0.02) + fbm(q * 3.0 - T * 0.04));
    col = mix(col, c1.rgb * 1.2, smoothstep(0.35, 0.85, cl) * smoothstep(-0.3, 0.3, q.y));
    // a strike every ~7 s somewhere, soft envelope, local light only
    float per = floor(T / 7.0);
    float ph = T - per * 7.0;
    vec2 h = hash2(vec2(per, 4.0));
    float env = smoothstep(0.0, 0.08, ph) * exp(-ph * 3.0) * (0.7 + 0.3 * sin(ph * 40.0));
    vec2 sp = vec2((h.x - 0.5) * 1.4, 0.25);
    col += c3.rgb * env * 0.5 * exp(-length(q - sp) * 3.0) * (0.4 + cl);
    if (ph < 0.6) {
        float x = sp.x;
        float dmin = 1.0;
        vec2 prev = sp;
        for (int i = 1; i < 10; i++) {
            float fi = float(i);
            vec2 nxt = vec2(x + (hash(vec2(per, fi)) - 0.5) * 0.08 * fi * 0.4, sp.y - fi * 0.07);
            dmin = min(dmin, sdSeg(q, prev, nxt));
            prev = nxt;
            x = nxt.x;
        }
        col += vec3(0.85, 0.9, 1.0) * env * (smoothstep(0.004, 0.0, dmin) * 0.9 + exp(-dmin * 60.0) * 0.25);
    }
    // rain
    vec2 rq = vec2(q.x * 60.0 + q.y * 12.0, q.y * 2.0 + T * 3.0);
    float rr = step(0.92, hash(vec2(floor(rq.x), 1.0))) * smoothstep(0.7, 1.0, fract(rq.y + hash(vec2(floor(rq.x), 2.0))));
    col += vec3(0.5, 0.6, 0.75) * rr * 0.12;
    return col;
}

// ---------------------------------------------------------------- 5: campfire
vec3 sFire(vec2 q, float T) {
    vec3 col = mix(c0.rgb, c1.rgb, smoothstep(0.5, -0.4, q.y) * 0.6);
    col += vec3(1.0) * stars(q, 55.0, T) * 0.6 * smoothstep(0.0, 0.3, q.y);
    vec2 base = vec2(0.0, -0.3);
    col += c3.rgb * 0.35 * exp(-length((q - base) * vec2(1.0, 2.0)) * 3.0);
    // trees
    for (int i = 0; i < 10; i++) {
        float fi = float(i);
        float x0 = -0.95 + fi * 0.2 + 0.06 * hash(vec2(fi, 1.0));
        if (abs(x0) < 0.25) continue;
        float h = 0.25 + 0.2 * hash(vec2(fi, 2.0));
        float yb = -0.35;
        float w = (h - (q.y - yb)) * 0.28;
        if (q.y > yb && q.y < yb + h && abs(q.x - x0) < w * (0.8 + 0.2 * sin((q.y - yb) * 60.0))) col = c0.rgb * 0.5;
    }
    if (q.y < -0.33) col = mix(c0.rgb * 0.6, c3.rgb * 0.3, exp(-length((q - base) * vec2(1.0, 4.0)) * 4.0));
    // logs
    float logs = min(sdSeg(q, base + vec2(-0.09, -0.02), base + vec2(0.08, 0.01)), sdSeg(q, base + vec2(-0.07, 0.015), base + vec2(0.09, -0.02))) - 0.014;
    col = mix(col, vec3(0.2, 0.1, 0.05), fill(logs));
    // flames
    vec2 fq = (q - base - vec2(0.0, 0.01)) * vec2(1.0, 0.75);
    float n = fbm(vec2(fq.x * 9.0, fq.y * 6.0 - T * 2.2));
    float shape = 1.0 - length(fq * vec2(5.5, 3.0) - vec2(0.0, 0.35)) + n * 0.8 - fq.y * 1.5;
    float fl = smoothstep(0.15, 0.8, shape) * step(-0.02, fq.y);
    vec3 fc = mix(vec3(0.8, 0.15, 0.05), vec3(1.0, 0.55, 0.1), smoothstep(0.2, 0.6, shape));
    fc = mix(fc, vec3(1.0, 0.92, 0.6), smoothstep(0.65, 1.0, shape));
    col = mix(col, fc, fl);
    col += c3.rgb * 0.15 * smoothstep(-0.2, 0.6, shape) * (1.0 - fl);
    // sparks
    for (int i = 0; i < 18; i++) {
        float fi = float(i);
        float ph = fract(T * (0.25 + 0.2 * hash(vec2(fi, 5.0))) + hash(vec2(fi, 6.0)));
        vec2 sp = base + vec2((hash(vec2(fi, 7.0)) - 0.5) * 0.08 + 0.12 * sin(ph * 4.0 + fi) * ph, 0.05 + ph * 0.55);
        col += vec3(1.0, 0.6, 0.2) * exp(-length(q - sp) * 300.0) * (1.0 - ph) * 1.5;
    }
    return col;
}

// ---------------------------------------------------------------- 6: fireworks
vec3 sFireworks(vec2 q, float T) {
    vec3 col = mix(c1.rgb * 0.6, c0.rgb, smoothstep(-0.5, 0.4, q.y));
    col += vec3(1.0) * stars(q, 50.0, T) * 0.4;
    for (int k = 0; k < 5; k++) {
        float fk = float(k);
        float P = 3.6 + fk * 0.37;
        float tt = T + fk * 1.3;
        float seed = floor(tt / P) + fk * 17.0;
        float tl = tt - floor(tt / P) * P;
        vec2 h = hash2(vec2(seed, 1.0));
        vec2 c = vec2((h.x - 0.5) * 1.3, 0.05 + 0.3 * h.y);
        vec3 fc = pal(fract(h.x * 3.7 + h.y));
        fc = mix(fc, vec3(1.0, 0.95, 0.85), 0.25);
        if (tl < 0.7) {
            float k2 = tl / 0.7;
            vec2 rp = vec2(c.x, -0.5 + (c.y + 0.5) * (1.0 - (1.0 - k2) * (1.0 - k2)));
            float d = sdSeg(q, rp, rp - vec2(0.0, 0.04));
            col += vec3(1.0, 0.8, 0.5) * exp(-d * 250.0) * 0.6;
        } else {
            float tb = tl - 0.7;
            float fade = exp(-tb * 1.3);
            // only pixels near the burst, and only the 3 particles nearest in angle (36 per burst)
            vec2 rel = q - c + vec2(0.0, 0.03 * tb * tb);
            if (length(rel) > 0.3) continue;
            float ai = floor(mod(atan(rel.y, rel.x), TAU) / TAU * 36.0 + 0.5);
            for (int j = -1; j <= 1; j++) {
                float fi = mod(ai + float(j), 36.0);
                int i = int(fi);
                float ang = fi / 36.0 * TAU + hash(vec2(seed, fi)) * 0.15;
                float spd = 0.16 + 0.05 * hash(vec2(fi, seed));
                vec2 pp = c + vec2(cos(ang), sin(ang)) * spd * (1.0 - exp(-tb * 2.2)) - vec2(0.0, 0.03 * tb * tb);
                float d = length(q - pp);
                float tw = 0.7 + 0.3 * sin(T * 20.0 + fi);
                col += fc * fade * tw * (0.00006 / (d * d + 0.00004) + exp(-d * 120.0) * 0.15);
            }
            col += fc * fade * 0.1 * exp(-length(q - c) * 6.0) * exp(-tb * 2.0);
        }
    }
    // city silhouette
    float bx = floor(q.x * 18.0);
    if (q.y < -0.36 + 0.09 * hash(vec2(bx, 2.0))) col = c0.rgb * 0.5 + vec3(1.0, 0.75, 0.4) * 0.25 * step(0.85, hash(floor(q * vec2(90.0, 70.0))));
    return col;
}

// ---------------------------------------------------------------- 7: spiral galaxy
vec3 sGalaxy(vec2 q, float T) {
    vec3 col = c0.rgb + vec3(1.0) * stars(q + 5.0, 60.0, T) * 0.5;
    vec2 g = rot(q, 0.4);
    g.y *= 2.0;
    float r = length(g), a = atan(g.y, g.x);
    float sw = a + T * 0.04 - log(r + 0.02) * 2.4;
    float arms = pow(0.5 + 0.5 * cos(sw * 2.0), 3.0);
    float dust = fbm(rot(g, T * 0.04 - r * 3.0) * 6.0);
    float dens = exp(-r * 3.2) * (0.25 + 0.95 * arms) * (0.4 + 0.9 * dust);
    col += mix(c1.rgb, c2.rgb, arms) * dens * 1.4;
    col += c3.rgb * exp(-r * 18.0) * 1.1;
    vec2 sp = rot(g * 40.0, T * 0.04 * (1.0 + 2.0 / (r + 0.3)));
    col += vec3(0.9, 0.9, 1.0) * step(0.97, hash(floor(sp))) * dens * 1.5;
    return col;
}

// ---------------------------------------------------------------- 8: black hole
vec3 sHole(vec2 q, float T) {
    float r = length(q);
    vec2 lq = q * (1.0 + 0.012 / max(r * r, 0.004));
    vec3 col = c0.rgb + vec3(1.0) * stars(lq, 60.0, T) * 0.8;
    col += c1.rgb * 0.15 * fbm(lq * 2.0);
    vec2 v = vec2(q.x, q.y * 3.6);
    float rr = length(v);
    float ang = atan(v.y, v.x);
    float swirl = fbm(vec2(ang * 3.0 + T * 0.5 / (rr + 0.1), rr * 14.0));
    float disk = smoothstep(0.1, 0.14, rr) * smoothstep(0.55, 0.2, rr);
    float dop = 1.0 + 0.7 * (-q.x / max(rr, 0.05));
    vec3 hot = mix(c2.rgb, c3.rgb, smoothstep(0.35, 0.12, rr));
    vec3 dc = hot * disk * (0.4 + 0.9 * swirl) * dop;
    // the far half of the disk bent over the top of the hole
    float arc = abs(length(q - vec2(0.0, 0.0)) - 0.105);
    vec3 halo = hot * smoothstep(0.03, 0.0, arc) * 0.8 * (0.6 + 0.4 * fbm(vec2(atan(q.y, q.x) * 4.0 - T * 0.6, 1.0)));
    if (q.y > 0.0) col += dc * 0.8;
    col += halo;
    col = mix(col, vec3(0.0), smoothstep(0.088, 0.082, r));
    if (q.y <= 0.0) col += dc;
    col += hot * 0.6 * smoothstep(0.004, 0.0, abs(r - 0.09));
    return col;
}

// ---------------------------------------------------------------- 9: ocean swell
vec3 sOcean(vec2 q, float T) {
    float hy = 0.08;
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(hy, 0.5, q.y));
    vec2 mc = vec2(0.35, 0.28);
    col += c3.rgb * 0.25 * exp(-length(q - mc) * 5.0);
    col = mix(col, mix(c3.rgb, vec3(1.0), 0.5), smoothstep(0.052, 0.048, length(q - mc)));
    for (int i = 0; i < 7; i++) {
        float fi = float(i);
        float k = fi / 6.0;
        float y0 = hy - 0.02 - k * k * 0.55;
        float amp = 0.006 + 0.05 * k * k;
        float fr = 14.0 - 10.0 * k;
        float sp = 0.4 + 0.6 * k;
        float x = q.x * fr + T * sp + fi * 1.7;
        float yw = y0 + amp * (sin(x) + 0.35 * sin(x * 2.3 + T * 0.7));
        if (q.y < yw) {
            float depth = yw - q.y;
            vec3 wc = mix(c2.rgb, c0.rgb, 0.35 + 0.5 * k) * (0.85 + 0.25 * smoothstep(0.03, 0.0, depth));
            float foam = smoothstep(0.005 + 0.01 * k, 0.0, depth) * smoothstep(0.3, 1.0, sin(x - 0.6));
            col = mix(wc, vec3(0.9, 0.95, 1.0), foam * 0.8);
            col += c3.rgb * 0.25 * exp(-pow((q.x - mc.x) * (3.0 + 6.0 * k), 2.0)) * smoothstep(0.5, 1.0, sin(x * 3.0 + q.y * 90.0)) * (1.0 - k);
        }
    }
    return col;
}

// ---------------------------------------------------------------- 10: kaleidoscope
vec3 sKaleido(vec2 q, float T) {
    q = rot(q, T * 0.05);
    float r = length(q), a = atan(q.y, q.x);
    float n = 8.0;
    float seg = TAU / n;
    a = mod(a, seg);
    a = abs(a - seg * 0.5);
    vec2 p = r * vec2(cos(a), sin(a));
    vec2 w = vec2(fbm(p * 3.0 + T * 0.12), fbm(p * 3.0 - T * 0.1 + 4.0));
    float f = fbm(p * 4.0 + w * 2.5 - vec2(T * 0.08, 0.0));
    vec3 col = pal(fract(f * 1.6 + r * 0.6 - T * 0.03));
    col *= 0.35 + 0.9 * smoothstep(0.35, 0.8, f);
    col += c3.rgb * smoothstep(0.02, 0.0, abs(fract(f * 6.0) - 0.5) - 0.46) * 0.25;
    return mix(c0.rgb, col, (smoothstep(0.75, 0.05, r) * 0.6 + 0.4) * 0.7);
}

// ---------------------------------------------------------------- 11: plexus
vec2 plexPt(vec2 id, float T) {
    vec2 h = hash2(id);
    return id + 0.5 + 0.4 * vec2(sin(T * (0.3 + 0.4 * h.x) + h.y * TAU), cos(T * (0.25 + 0.4 * h.y) + h.x * TAU));
}
vec3 sPlexus(vec2 q, float T) {
    vec3 col = mix(c0.rgb, c1.rgb * 0.5, smoothstep(0.6, -0.5, q.y));
    vec2 g = q * 6.0 + vec2(T * 0.05, 0.0);
    vec2 id = floor(g);
    vec2 pts[9];
    int n = 0;
    for (int j = -1; j <= 1; j++) for (int i = -1; i <= 1; i++) { pts[n] = plexPt(id + vec2(i, j), T); n++; }
    // 12 links per pixel: the centre point to its 8 neighbours + the 4 links around it
    float lines = 0.0;
    for (int k = 0; k < 12; k++) {
        int a = k < 8 ? 4 : (k == 8 ? 1 : k == 9 ? 1 : k == 10 ? 7 : 7);
        int b = k < 8 ? (k < 4 ? k : k + 1) : (k == 8 ? 3 : k == 9 ? 5 : k == 10 ? 3 : 5);
        float len = length(pts[a] - pts[b]);
        if (len > 1.35) continue;
        lines += smoothstep(0.022, 0.0, sdSeg(g, pts[a], pts[b])) * smoothstep(1.35, 0.6, len);
    }
    for (int i = 0; i < 9; i++) {
        float pd = length(g - pts[i]);
        col += c3.rgb * (smoothstep(0.07, 0.0, pd) * 0.9 + exp(-pd * 12.0) * 0.12);
    }
    col += mix(c2.rgb, c3.rgb, 0.5 + 0.5 * sin(q.x * 2.0 + T * 0.2)) * min(lines, 1.5) * 0.45;
    return col;
}

// ---------------------------------------------------------------- 12: equalizer
vec3 sEq(vec2 q, float T, float e, float asp) {
    vec3 col = mix(c0.rgb, c1.rgb * 0.5, smoothstep(0.5, -0.5, q.y));
    float n = 56.0;
    float x = (q.x / asp + 0.5) * n;
    float i = floor(x);
    float fx = fract(x);
    float lvl = pow(noise(vec2(i * 0.35, T * 2.2)), 1.4) * 0.8 + 0.2 * noise(vec2(i * 0.9, T * 5.0));
    lvl *= 0.55 + 0.7 * e;
    float hgt = 0.04 + 0.42 * lvl;
    float base = -0.3;
    float bar = step(0.12, fx) * step(fx, 0.88);
    float y = q.y - base;
    vec3 bc = pal(clamp(y / 0.5, 0.0, 1.0));
    if (y > 0.0 && y < hgt) col = mix(col, bc, bar * (0.6 + 0.4 * step(0.25, fract(y * 60.0))));
    float cap = hgt + 0.02 + 0.03 * pow(noise(vec2(i * 0.35, (T - 0.3) * 2.2)), 1.4);
    col = mix(col, c3.rgb, bar * smoothstep(0.004, 0.0, abs(y - cap)));
    if (y < 0.0 && -y < hgt * 0.6) col = mix(col, bc, bar * 0.18 * (1.0 + y / (hgt * 0.6)));
    col += c2.rgb * 0.08 * exp(-abs(y) * 6.0);
    return col;
}

// ---------------------------------------------------------------- 13: orbits
vec3 sOrbits(vec2 q, float T) {
    vec3 col = c0.rgb + vec3(1.0) * stars(q, 60.0, T) * 0.5;
    vec2 p = vec2(q.x, q.y * 2.4);
    col += c3.rgb * 0.5 * exp(-length(q) * 9.0);
    col = mix(col, mix(c3.rgb, vec3(1.0), 0.5), smoothstep(0.042, 0.038, length(q)));
    for (int i = 0; i < 6; i++) {
        float fi = float(i);
        float R = 0.12 + fi * 0.085;
        float ring = abs(length(p) - R);
        col += c2.rgb * smoothstep(0.004, 0.0, ring) * 0.25;
        float a = T * 0.6 / pow(R / 0.12, 1.5) + fi * 2.1;
        vec2 pp = vec2(cos(a), sin(a) / 2.4) * R;
        float pr = 0.008 + 0.012 * hash(vec2(fi, 3.0));
        vec3 pc = pal(fract(fi * 0.37));
        float lit = 0.5 + 0.5 * dot(normalize(q - pp + 1e-4), normalize(-pp + 1e-4));
        col = mix(col, pc * (0.4 + 0.8 * lit), smoothstep(pr, pr - 0.002, length(q - pp)));
        col += pc * 0.2 * exp(-length(q - pp) * 40.0);
        float tr = abs(length(p) - R) + 0.0;
        float ta = mod(atan(p.y, p.x) - a + TAU, TAU);
        col += pc * smoothstep(0.006, 0.0, tr) * exp(-ta * 2.5) * 0.4;
    }
    return col;
}

// ---------------------------------------------------------------- 14: soap bubbles
vec3 sBubbles(vec2 q, float T) {
    vec3 col = mix(c0.rgb, c1.rgb, smoothstep(-0.5, 0.6, q.y) * 0.6);
    for (int k = 0; k < 16; k++) {
        float fk = float(k);
        vec2 h = hash2(vec2(fk, 2.0));
        float R = 0.03 + 0.07 * h.y;
        float y = mod(T * 0.03 * (0.6 + h.x) + h.y * 1.6, 1.4) - 0.7;
        vec2 c = vec2((h.x - 0.5) * 1.7 + 0.05 * sin(T * 0.5 + fk), y);
        vec2 d = q - c;
        d.x *= 1.0 + 0.05 * sin(T * 3.0 + fk);
        float r = length(d);
        if (r < R + 0.01) {
            float k2 = r / R;
            float th = k2 * k2 * 2.0 + 0.3 * noise(d * 30.0 + T) + T * 0.1;
            vec3 film = 0.5 + 0.5 * cos(TAU * (th + vec3(0.0, 0.33, 0.67)));
            float rim = smoothstep(0.6, 1.0, k2) * smoothstep(1.02, 0.98, k2);
            col = mix(col, film, rim * 0.55);
            col += film * 0.05 * step(k2, 1.0);
            col += vec3(1.0) * smoothstep(0.25, 0.0, length(d / R - vec2(-0.4, 0.4))) * 0.5;
        }
    }
    return col;
}

// ---------------------------------------------------------------- 15: meteor shower
vec3 sMeteors(vec2 q, float T) {
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(-0.5, 0.5, q.y));
    col += vec3(1.0) * stars(q, 70.0, T) * 0.8 + vec3(0.8, 0.9, 1.0) * stars(q + 2.0, 30.0, T) * 0.4;
    vec2 dir = normalize(vec2(-1.0, -0.55));
    for (int k = 0; k < 8; k++) {
        float fk = float(k);
        float P = 2.2 + fk * 0.31;
        float tt = T + fk * 0.9;
        float seed = floor(tt / P) + fk * 11.0;
        float ph = (tt - floor(tt / P) * P) / 0.9;
        if (ph > 1.0) continue;
        vec2 h = hash2(vec2(seed, 3.0));
        vec2 head = vec2(-0.2 + h.x * 1.2, 0.1 + h.y * 0.4) + dir * ph * 0.7;
        vec2 rel = q - head;
        float along = dot(rel, -dir);
        float across = abs(dot(rel, vec2(-dir.y, dir.x)));
        float tail = step(0.0, along) * smoothstep(0.22, 0.0, along) * smoothstep(0.003, 0.0, across);
        col += mix(c3.rgb, vec3(1.0), 0.6) * (tail + exp(-length(rel) * 200.0)) * sin(ph * PI) * 0.9;
    }
    float g = -0.38 + 0.03 * sin(q.x * 4.0) + 0.02 * noise(vec2(q.x * 20.0, 1.0));
    if (q.y < g) col = c0.rgb * 0.4;
    return col;
}

// ---------------------------------------------------------------- 16: meadow in the wind
vec3 sMeadow(vec2 q, float T) {
    vec3 col = mix(c3.rgb, c1.rgb, smoothstep(-0.2, 0.5, q.y));
    vec2 sc = vec2(-0.3, -0.12);
    col += c3.rgb * 0.5 * exp(-length(q - sc) * 4.0);
    col = mix(col, vec3(1.0, 0.9, 0.7), smoothstep(0.062, 0.058, length(q - sc)));
    for (int b = 0; b < 4; b++) {
        float fb = float(b);
        float ph = fract(T * 0.02 + fb * 0.25);
        vec2 bp = vec2(-1.0 + ph * 2.2, 0.15 + 0.08 * fb + 0.02 * sin(T + fb));
        vec2 r = q - bp;
        float wing = sin(T * 7.0 + fb) * 0.008;
        float bird = min(sdSeg(r, vec2(-0.015, wing), vec2(0.0, 0.0)), sdSeg(r, vec2(0.0, 0.0), vec2(0.015, wing)));
        col = mix(col, c0.rgb, smoothstep(0.0025, 0.0, bird));
    }
    for (int l = 0; l < 4; l++) {
        float L = float(l);
        float k = L / 3.0;
        float y0 = -0.08 - 0.12 * L;
        float dens = 120.0 + 80.0 * L;
        float xi = floor(q.x * dens);
        for (int j = -1; j <= 1; j++) {
            float id = xi + float(j);
            float h = hash(vec2(id, L));
            float bx = (id + 0.5) / dens;
            float hgt = (0.06 + 0.12 * h) * (0.6 + 0.6 * k);
            float t = clamp((q.y - y0) / hgt, 0.0, 1.0);
            float gust = 0.5 + 0.5 * sin(bx * 3.0 - T * 1.4 + L);
            float bend = (0.01 + 0.05 * gust) * t * t * (0.5 + k);
            float d = abs(q.x - bx - bend) - 0.004 * (1.0 - t) * (0.7 + k);
            if (q.y > y0 - 0.2 && q.y < y0 + hgt && d < 0.0) col = mix(c2.rgb * (0.4 + 0.5 * t), c0.rgb, 0.6 * k) * (0.5 + 0.5 * (1.0 - k));
        }
        if (q.y < y0) col = mix(c2.rgb * 0.35, c0.rgb, 0.5 + 0.4 * k);
    }
    return col;
}

// ---------------------------------------------------------------- 17: highway light trails
vec3 sHighway(vec2 q, float T) {
    float hy = 0.02;
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(hy, 0.5, q.y));
    float bx = floor(q.x * 22.0);
    float bh = hy + 0.05 + 0.18 * hash(vec2(bx, 1.0)) * smoothstep(0.1, 0.9, abs(q.x));
    if (q.y > hy && q.y < bh) {
        col = c0.rgb * 0.8;
        vec2 w = floor(vec2(q.x * 150.0, q.y * 120.0));
        col += vec3(1.0, 0.8, 0.5) * 0.4 * step(0.8, hash(w)) * step(0.5, fract(q.x * 150.0)) * step(0.4, fract(q.y * 120.0));
    }
    if (q.y < hy) {
        float z = 0.25 / (hy - q.y + 0.004);
        float cx = 0.4 * sin(z * 0.08 + 1.0) - 0.4 * sin(1.0);
        float lat = (q.x - cx / z * 3.0) * z;
        col = c0.rgb * 0.7;
        float fog = exp(-(hy - q.y) * 3.0);
        for (int i = 0; i < 6; i++) {
            float fi = float(i);
            float lane = (fi - 2.5) * 0.35;
            float d = abs(lat - lane);
            bool away = i < 3;
            vec3 lc = away ? vec3(1.0, 0.18, 0.12) : vec3(1.0, 0.92, 0.75);
            float flow = 0.6 + 0.4 * sin(z * 0.6 + (away ? -T * 3.0 : T * 4.0) + fi * 2.0);
            col += lc * smoothstep(0.06, 0.0, d) * flow * 0.7 * (0.3 + 0.7 * fog);
            col += lc * exp(-d * 5.0) * 0.05;
        }
        float dash = step(0.5, fract(z * 0.4 - T * 2.0)) * smoothstep(0.03, 0.0, abs(lat));
        col += vec3(0.6) * dash * 0.25;
    }
    col += c3.rgb * 0.25 * exp(-abs(q.y - hy) * 30.0);
    return col;
}

// ---------------------------------------------------------------- 18: vortex
vec3 sVortex(vec2 q, float T) {
    float r = length(q) + 1e-4, a = atan(q.y, q.x);
    // logarithmic spiral arms, soft gaussian bands, a slow breathing drift; no noise blotches
    float u = a / TAU * 3.0 + log(r) * 1.35 - T * 0.06;
    float band = 0.5 + 0.5 * cos(u * TAU);
    band = pow(band, 2.5);
    float u2 = a / TAU * 5.0 - log(r) * 0.9 + T * 0.035;
    float fine = pow(0.5 + 0.5 * cos(u2 * TAU), 6.0);
    vec3 tint = pal(0.5 + 0.5 * sin(log(r) * 1.6 + 0.6 * sin(a + T * 0.05) - T * 0.03));
    float fade = smoothstep(0.015, 0.25, r) * exp(-r * 1.6);
    vec3 col = c0.rgb;
    col += tint * band * fade * 1.05;
    col += mix(c3.rgb, vec3(1.0), 0.3) * fine * fade * 0.35;
    col += c3.rgb * 0.5 * exp(-r * 14.0);
    col += vec3(1.0) * stars(q, 55.0, T) * 0.35 * (1.0 - fade);
    return col;
}

// ---------------------------------------------------------------- 19: neon ribbons
vec3 sRibbons(vec2 q, float T) {
    vec3 col = c0.rgb;
    for (int i = 0; i < 4; i++) {
        float fi = float(i);
        float y = 0.18 * sin(q.x * (1.6 + 0.3 * fi) + T * (0.5 + 0.15 * fi) + fi * 1.9) + 0.06 * sin(q.x * 4.0 - T * 0.7 + fi) + (fi - 1.5) * 0.06;
        float tw = cos(q.x * (3.0 + fi) + T * (0.8 + 0.2 * fi) + fi);
        float th = 0.004 + 0.03 * abs(tw);
        float d = abs(q.y - y);
        vec3 rc = pal(fract(fi * 0.3 + q.x * 0.15 + T * 0.02));
        float body = smoothstep(th, th - 0.003, d);
        col = mix(col, rc * (0.45 + 0.55 * abs(tw)) + vec3(0.2) * pow(abs(tw), 8.0), body * 0.85);
        col += rc * 0.0035 / (d + 0.01) * 0.25;
    }
    return col;
}

// ---------------------------------------------------------------- 20: hologram globe
vec3 sGlobe(vec2 q, float T) {
    vec3 col = c0.rgb + c1.rgb * 0.25 * exp(-length(q) * 3.0);
    float R = 0.33;
    vec2 c = q - vec2(0.0, 0.0);
    float r = length(c);
    col += c2.rgb * 0.35 * smoothstep(0.02, 0.0, abs(r - R)) + c2.rgb * 0.15 * exp(-abs(r - R) * 30.0);
    if (r < R) {
        for (int side = 0; side < 2; side++) {
            float z = sqrt(R * R - r * r) * (side == 0 ? 1.0 : -1.0);
            vec3 n = vec3(c.x, c.y, z) / R;
            float tilt = 0.4;
            n = vec3(n.x, n.y * cos(tilt) - n.z * sin(tilt), n.y * sin(tilt) + n.z * cos(tilt));
            float ang = T * 0.25;
            n = vec3(n.x * cos(ang) + n.z * sin(ang), n.y, -n.x * sin(ang) + n.z * cos(ang));
            float lat = asin(clamp(n.y, -1.0, 1.0)), lon = atan(n.x, n.z);
            vec2 g = vec2(lon, lat) / (PI / 12.0);
            vec2 w = fwidth(g);
            vec2 gl = 1.0 - smoothstep(vec2(0.0), 1.5 * w, abs(fract(g) - 0.5) - 0.45);
            float line = max(gl.x, gl.y);
            float land = smoothstep(0.5, 0.56, noise(vec2(lon * 1.5, lat * 2.0) + 3.0) * 0.65 + noise(vec2(lon * 4.0, lat * 5.0)) * 0.35);
            float k = side == 0 ? 1.0 : 0.3;
            col += c3.rgb * line * 0.45 * k + c2.rgb * land * 0.3 * k;
            if (side == 0) col += c2.rgb * 0.08;
        }
        col *= 0.85 + 0.15 * step(0.5, fract(q.y * size.y / 4.0 + T * 2.0));
    }
    return col;
}

// ---------------------------------------------------------------- 21: DNA helix
vec3 sDNA(vec2 q, float T) {
    vec3 col = c0.rgb + c1.rgb * 0.2 * exp(-abs(q.y) * 4.0);
    vec2 p = rot(q, -0.25);
    float ph = p.x * 9.0 + T * 0.9;
    float A = 0.11;
    for (int s = 0; s < 2; s++) {
        float o = float(s) * PI;
        float y = A * sin(ph + o);
        float z = cos(ph + o);
        float d = abs(p.y - y);
        float w = 0.008 + 0.006 * (z * 0.5 + 0.5);
        vec3 sc = s == 0 ? c2.rgb : c3.rgb;
        col = mix(col, sc * (0.45 + 0.55 * (z * 0.5 + 0.5)), smoothstep(w, w - 0.003, d) * (0.5 + 0.5 * step(0.0, z)));
        col += sc * 0.004 / (d + 0.008) * 0.3;
    }
    float rung = fract(ph / (PI / 3.0));
    float rx = (floor(ph / (PI / 3.0)) + 0.5) * (PI / 3.0);
    float xr = (rx - T * 0.9) / 9.0;
    float y1 = A * sin(rx), y2 = A * sin(rx + PI);
    float dr = sdSeg(p, vec2(xr, y1), vec2(xr, y2));
    vec3 rc = mix(c2.rgb, c3.rgb, smoothstep(y1, y2, p.y));
    col = mix(col, rc * 0.7, smoothstep(0.004, 0.001, dr) * 0.8);
    col += vec3(1.0) * stars(q + vec2(T * 0.01, 0.0), 40.0, T) * 0.3;
    return col;
}

// ---------------------------------------------------------------- 22: aurora over a mountain lake
// a mountain ridge: sharp peaks (folded noise), height h, amplitude a
float ridge(float x, float s, float h, float a) {
    float v = 0.0, amp = 0.55, f = 1.6;
    for (int i = 0; i < 4; i++) { v += amp * (1.0 - abs(noise(vec2(x * f + s, s * 1.7)) * 2.0 - 1.0)); f *= 2.1; amp *= 0.45; }
    return h + a * (v - 0.55);
}
vec3 auroraSky(vec2 q, float T) {
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(-0.1, 0.5, q.y));
    col += vec3(1.0) * stars(q + 3.0, 70.0, T) * 0.55 * smoothstep(-0.05, 0.25, q.y);
    // curtains: bands along a wavy line, vertical rays, colour from green (low) to violet (high)
    float acc = 0.0;
    vec3 ac = vec3(0.0);
    for (int i = 0; i < 3; i++) {
        float fi = float(i);
        float base = 0.04 + 0.07 * fi + 0.05 * sin(q.x * (1.6 + fi * 0.7) + T * (0.07 + 0.03 * fi) + fi * 2.0);
        base += 0.03 * (fbm(vec2(q.x * 2.0 + fi * 7.0, T * 0.05)) - 0.5);
        float d = q.y - base;
        float rays = 0.55 + 0.45 * fbm(vec2(q.x * 24.0 + fi * 13.0, T * 0.25 + fi));
        // a soft lower hem (no hard line that reads as a hill), rays fading upwards
        float curtain = smoothstep(-0.06, 0.012, d) * smoothstep(-0.06, 0.0, d) * exp(-max(d, 0.0) * (7.0 - fi * 1.5)) * rays * rays;
        vec3 cc = mix(c2.rgb, c3.rgb, clamp(d * 5.0 + fi * 0.3, 0.0, 1.0));
        ac += cc * curtain * (0.85 - 0.2 * fi);
        acc += curtain;
    }
    col += ac * 0.9;
    return col;
}
vec3 sAuroraLake(vec2 q, float T) {
    float hz = -0.24;
    vec3 col;
    vec3 farc = mix(c0.rgb, c1.rgb, 0.55);
    if (q.y > hz) {
        col = auroraSky(q, T);
        // two mountain ranges, the far one hazy (lit by the aurora), the near one dark
        float far = ridge(q.x, 1.3, -0.08, 0.2);
        float near = ridge(q.x * 1.3 + 4.0, 7.1, -0.17, 0.12);
        col = mix(col, farc + c2.rgb * 0.05 * smoothstep(far - 0.12, far, q.y), smoothstep(far + AA, far - AA, q.y));
        col = mix(col, c0.rgb * 0.55, smoothstep(near + AA, near - AA, q.y));
    } else {
        // the lake mirrors the sky, rippled, darker with depth
        float d = hz - q.y;
        vec2 m = vec2(q.x + 0.004 * sin(q.y * 180.0 + T * 1.2) * (0.3 + d * 3.0), 2.0 * hz - q.y);
        vec3 refl = auroraSky(m, T);
        float far = ridge(m.x, 1.3, -0.08, 0.2);
        float near = ridge(m.x * 1.3 + 4.0, 7.1, -0.17, 0.12);
        if (m.y < far) refl = farc;
        if (m.y < near) refl = c0.rgb * 0.55;
        col = refl * (0.55 - 0.5 * d) + c0.rgb * 0.3;
        col += vec3(0.8, 0.9, 1.0) * 0.06 * smoothstep(0.004, 0.0, abs(q.y - hz + 0.002));
    }
    return col;
}

// ---------------------------------------------------------------- 23: liquid chrome (glossy flowing folds)
float chromeH(vec2 p, float T) {
    vec2 w = p + 0.35 * vec2(sin(p.y * 2.3 + T * 0.21), cos(p.x * 1.9 - T * 0.17));
    w += 0.18 * vec2(sin(w.y * 4.1 - T * 0.13), cos(w.x * 3.7 + T * 0.19));
    return sin(w.x * 3.0 + sin(w.y * 2.0)) * 0.5 + sin(w.y * 2.6 - w.x * 1.3 + T * 0.1) * 0.5;
}
vec3 sChrome(vec2 q, float T) {
    vec2 p = q * 2.2;
    float e = 0.004;
    float h = chromeH(p, T);
    vec2 g = vec2(chromeH(p + vec2(e, 0.0), T) - h, chromeH(p + vec2(0.0, e), T) - h) / e;
    vec3 n = normalize(vec3(-g * 0.35, 1.0));
    vec3 l = normalize(vec3(-0.4, 0.6, 0.7));
    float dif = clamp(dot(n, l), 0.0, 1.0);
    float spec = pow(clamp(dot(reflect(-l, n), vec3(0.0, 0.0, 1.0)), 0.0, 1.0), 24.0);
    float fres = pow(1.0 - n.z, 2.0);
    vec3 base = pal(clamp(0.5 + 0.5 * h, 0.0, 1.0));
    vec3 col = mix(c0.rgb, base, 0.25 + 0.6 * dif);
    col += mix(c3.rgb, vec3(1.0), 0.6) * spec * 0.55;
    col += c2.rgb * fres * 0.35;
    return col;
}

// ---------------------------------------------------------------- 24: neon city in the rain
float bldg(float x, float layer, out float id) {
    float w = 0.05 + 0.03 * layer;
    id = floor(x / w);
    float hgt = 0.1 + 0.32 * hash(vec2(id, layer * 7.0)) * (1.0 - 0.3 * layer);
    return hgt - 0.12 - 0.1 * layer;
}
vec3 cityLayer(vec2 q, float T, float layer, vec3 col) {
    float id;
    float x = q.x * (1.0 + 0.3 * layer) + layer * 3.7 + T * 0.003 * (2.0 - layer);
    float top = bldg(x, layer, id);
    if (q.y < top) {
        float k = layer / 2.0;
        vec3 wall = mix(c0.rgb * 1.4, c1.rgb * 0.6, 1.0 - k) * (0.6 + 0.4 * k);
        col = wall;
        // windows: a grid, some lit (warm or the neon tint), some flicker very slowly
        float w = 0.05 + 0.03 * layer;
        vec2 cell = vec2(fract(x / w) * 6.0, (top - q.y) * (70.0 - 15.0 * layer));
        vec2 ci = floor(cell);
        float lit = step(0.62, hash(ci + id * 13.0 + layer * 31.0 + floor(T * 0.02 + hash(ci + id) * 9.0)));
        float inwin = step(0.25, fract(cell.x)) * step(fract(cell.x), 0.8) * step(0.3, fract(cell.y)) * step(fract(cell.y), 0.8) * step(1.0, ci.y) * step(ci.x, 4.5) * step(0.5, ci.x);
        vec3 wc = hash(ci + id) > 0.7 ? c3.rgb : vec3(1.0, 0.82, 0.55);
        col += wc * lit * inwin * (0.55 - 0.15 * k);
        // a neon sign on some roofs
        float sign = step(0.8, hash(vec2(id, layer + 4.0)));
        float sy = top + 0.012;
        col += mix(c2.rgb, c3.rgb, hash(vec2(id, 2.0))) * sign * exp(-abs(q.y - top + 0.03) * 60.0) * 0.25;
    } else if (layer > 1.5) {
        // glow of the signs into the haze above the near layer
        col += mix(c2.rgb, c3.rgb, 0.5) * 0.04 * exp(-(q.y - top) * 8.0);
    }
    return col;
}
vec3 citySky(vec2 q, float T) {
    vec3 col = mix(c1.rgb * 0.9, c0.rgb, smoothstep(-0.2, 0.5, q.y));
    col += c2.rgb * 0.18 * exp(-(q.y + 0.1) * 4.0);
    for (int i = 0; i < 3; i++) col = cityLayer(q, T, float(i), col);
    return col;
}
vec3 sNeonCity(vec2 q, float T) {
    float st = -0.27;
    vec3 col;
    if (q.y > st) {
        col = citySky(q, T);
    } else {
        // the wet street: the city mirrored, smeared down, broken by ripples
        float d = st - q.y;
        vec2 m = vec2(q.x + 0.006 * sin(q.y * 140.0 + T * 2.0) + 0.004 * noise(vec2(q.x * 40.0, q.y * 400.0 - T)), 2.0 * st - q.y + d * d * 0.6);
        col = citySky(m, T) * (0.45 - d * 0.6) + c0.rgb * 0.4;
    }
    // rain: thin slanted streaks, a few brighter ones
    vec2 rp = vec2(q.x * 90.0 + q.y * 12.0, q.y * 6.0 + T * 3.5);
    float rid = hash(vec2(floor(rp.x), 3.0));
    float streak = smoothstep(0.08, 0.0, abs(fract(rp.x) - 0.5)) * step(0.85, fract(rp.y + rid * 7.0)) * step(0.55, rid);
    col += vec3(0.75, 0.85, 1.0) * streak * 0.12;
    return col;
}

// ---------------------------------------------------------------- 25: ringed planet
vec3 sPlanet(vec2 q, float T) {
    vec3 col = c0.rgb + vec3(1.0) * stars(q, 80.0, T) * 0.6 + c1.rgb * 0.15 * fbm(q * 2.0 + 3.0);
    vec2 pc = vec2(0.12, -0.04);
    float R = 0.24;
    vec2 p = q - pc;
    vec3 lightDir = normalize(vec3(-0.75, 0.45, 0.5));
    // the rings: a tilted flat ellipse; behind the planet first, in front after
    vec2 rp = rot(p, 0.32);
    float rr = length(vec2(rp.x, rp.y / 0.22));
    float ringBand = smoothstep(R * 1.32, R * 1.36, rr) * smoothstep(R * 2.25, R * 2.2, rr);
    float gaps = 0.55 + 0.45 * sin(rr * 150.0) * sin(rr * 61.0 + 1.0);
    gaps *= 1.0 - 0.85 * smoothstep(R * 1.83, R * 1.86, rr) * smoothstep(R * 1.93, R * 1.9, rr);
    vec3 ringc = mix(c2.rgb, c3.rgb, smoothstep(R * 1.4, R * 2.2, rr)) * gaps * ringBand;
    bool front = rp.y < 0.0;
    float dp = length(p);
    // rings behind
    // the planet's shadow on the rings: behind it, seen from the light
    vec2 L2 = normalize(lightDir.xy);
    float along = dot(p, L2);
    float perp = abs(p.x * L2.y - p.y * L2.x);
    float pshadow = 1.0 - 0.92 * smoothstep(0.02, -0.02, along) * smoothstep(R * 1.02, R * 0.9, perp);
    ringc *= pshadow;
    if (!front) col = mix(col, ringc * 0.9, ringBand * 0.85);
    // the planet: lit sphere with soft bands
    if (dp < R) {
        vec3 n = vec3(p / R, sqrt(max(0.0, 1.0 - dot(p, p) / (R * R))));
        float lat = rot(p, 0.32).y / R;
        float bands = fbm(vec2(lat * 7.0, T * 0.01)) * 0.6 + 0.4 * sin(lat * 18.0 + fbm(vec2(n.x * 3.0 + T * 0.02, lat * 4.0)) * 3.0);
        vec3 surf = mix(c2.rgb, c3.rgb, 0.5 + 0.5 * bands);
        float dif = clamp(dot(n, lightDir), 0.0, 1.0);
        // the rings' shadow on the planet
        float sh = 1.0 - 0.5 * smoothstep(0.022, 0.0, abs(rot(p, 0.32).y + 0.035)) * smoothstep(-0.2, 0.2, dif);
        vec3 pcol = surf * (0.04 + 1.05 * pow(dif, 0.8)) * sh;
        pcol += c3.rgb * 0.25 * pow(1.0 - n.z, 3.0) * dif;
        col = mix(col, pcol, smoothstep(R, R - AA * 1.5, dp));
    }
    col += c3.rgb * 0.12 * exp(-max(dp - R, 0.0) * 30.0) * step(R, dp) * clamp(dot(normalize(vec3(p, 0.0)), lightDir) + 0.3, 0.0, 1.0);
    // rings in front, the planet's shadow falls on them
    if (front && ringBand > 0.0) {
        col = mix(col, ringc, ringBand * 0.9);
    }
    return col;
}

// ---------------------------------------------------------------- 26: layered waves (soft hills of light)
vec3 sLayers(vec2 q, float T) {
    vec3 col = mix(c0.rgb, mix(c1.rgb, c0.rgb, 0.4), smoothstep(0.5, -0.5, q.y));
    col += c3.rgb * 0.12 * exp(-length(q - vec2(-0.3, 0.35)) * 2.5);
    for (int i = 0; i < 7; i++) {
        float fi = float(i);
        float k = fi / 6.0;
        float y0 = 0.28 - k * 0.62;
        float wv = y0 + 0.07 * sin(q.x * (1.6 + 0.4 * fi) + T * (0.08 + 0.02 * fi) + fi * 1.9)
                      + 0.035 * sin(q.x * (3.3 - 0.2 * fi) - T * (0.06 + 0.015 * fi) + fi * 0.7);
        float d = q.y - wv;
        if (d < 0.0) {
            vec3 lc = pal(clamp(0.15 + k * 0.85, 0.0, 1.0));
            lc = mix(lc, c0.rgb, 0.15 + 0.55 * (1.0 - k) * 0.6);
            // lit rim along the crest, shade going down
            float rim = exp(d * 90.0);
            float shade = 0.75 + 0.25 * exp(d * 6.0);
            col = lc * shade + mix(c3.rgb, vec3(1.0), 0.4) * rim * (0.18 + 0.12 * k);
        } else {
            // a soft shadow the layer casts on the one behind
            col *= 1.0 - 0.25 * exp(-d * 30.0) * step(0.5, fi);
        }
    }
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
    if (m == 0) col = sWarp(q, T);
    else if (m == 1) col = sTunnel(q, T);
    else if (m == 2) col = sCode(uv, T);
    else if (m == 3) col = sPlasma(q, T);
    else if (m == 4) col = sStorm(q, T);
    else if (m == 5) col = sFire(q, T);
    else if (m == 6) col = sFireworks(q, T);
    else if (m == 7) col = sGalaxy(q, T);
    else if (m == 8) col = sHole(q, T);
    else if (m == 9) col = sOcean(q, T);
    else if (m == 10) col = sKaleido(q, T);
    else if (m == 11) col = sPlexus(q, T);
    else if (m == 12) col = sEq(q, T, e, asp);
    else if (m == 13) col = sOrbits(q, T);
    else if (m == 14) col = sBubbles(q, T);
    else if (m == 15) col = sMeteors(q, T);
    else if (m == 16) col = sMeadow(q, T);
    else if (m == 17) col = sHighway(q, T);
    else if (m == 18) col = sVortex(q, T);
    else if (m == 19) col = sRibbons(q, T);
    else if (m == 20) col = sGlobe(q, T);
    else if (m == 21) col = sDNA(q, T);
    else if (m == 22) col = sAuroraLake(q, T);
    else if (m == 23) col = sChrome(q, T);
    else if (m == 24) col = sNeonCity(q, T);
    else if (m == 25) col = sPlanet(q, T);
    else col = sLayers(q, T);

    col *= 1.0 + 0.1 * e;
    vec2 p = vec2(q.x / asp, q.y);
    col *= mix(1.0, 1.0 - 1.2 * dot(p, p), opt.y);
    col *= opt.z;
    // fixed sub-LSB dither: dark gradients don't band
    col += (hash(gl_FragCoord.xy) + hash(gl_FragCoord.xy + 17.3) - 1.0) / 255.0;
    fragColor = vec4(max(col, 0.0), 1.0) * qt_Opacity;
}
