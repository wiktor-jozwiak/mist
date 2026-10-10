#version 440
// Live wallpaper (components/LiveWall.qml), rendered at half resolution: soft "glow" scenes in the
// style of ~/Obrazy/Tapety/glow-*.png, dark enough for the white desktop widgets on top.
// mode 0 glow (domain-warped colour clouds), 1 aurora (diagonal ribbon with curtains), 2 nebula (glow +
// stars), 3 silk (folded light bands), 4 lava (rising metaballs), 5 horizon (sky + sea, sun / moon at
// `sun`), 6 bokeh (drifting out-of-focus lights), 7 caustics (light on a pool floor), 8 rain on a window
// over city lights, 9 topographic contours, 10 synthwave grid, 11 drifting stars + shooting stars, 12 dunes,
// 13 mesh gradient, 14 mountains with valley fog (sun / moon at `sun`), 15 light waves (music lifts them),
// 16 snowfall, 17 fireflies over a meadow, 18 dot matrix swell, 19 crystal cells, 20 smoke, 21 pond ripples.
// c0 = base / shadow, c1..c3 = lights. opt = stars, vignette, brightness, music energy (0..1).
// sun = x, y (0..1 of the frame), disc size, strength (horizon mode; < 0 = moon).
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

vec3 glow(vec2 p, float t) {
    vec2 q = vec2(fbm(p * 1.1 + vec2(0.0, t)), fbm(p * 1.1 + vec2(5.2, 1.3) - t));
    vec2 r = vec2(fbm(p * 1.4 + 2.2 * q + vec2(1.7, 9.2) + 0.6 * t), fbm(p * 1.4 + 2.2 * q + vec2(8.3, 2.8) - 0.4 * t));
    float f = fbm(p * 1.2 + 2.0 * r);
    vec3 col = mix(c0.rgb, c1.rgb, smoothstep(0.25, 0.85, f));
    col = mix(col, c2.rgb, smoothstep(0.35, 1.0, length(q)) * 0.85);
    col = mix(col, c3.rgb, smoothstep(0.55, 0.95, r.x) * smoothstep(0.3, 0.8, f) * 0.8);
    return col;
}

vec3 aurora(vec2 p, vec2 uv, float t) {
    vec3 col = c0.rgb;
    // two soft colour pools in the corners, like the static glow-aurora wallpaper
    col += c3.rgb * 0.9 * exp(-dot(p - vec2(-0.55 + 0.05 * sin(t), -0.3), p - vec2(-0.55 + 0.05 * sin(t), -0.3)) * 3.0);
    col += c2.rgb * 0.7 * exp(-dot(p - vec2(0.5, -0.15), p - vec2(0.5, -0.15)) * 2.5);
    // the ribbon: a diagonal band bent by slow noise, brighter in moving curtains
    for (int k = 0; k < 2; k++) {
        float o = float(k);
        float w = fbm(vec2(p.x * 0.9 + t * (0.5 + 0.2 * o) + o * 7.0, t * 0.25)) - 0.5;
        float line = p.y + 0.42 * p.x - 0.08 + 0.32 * w + o * 0.22;
        float width = 0.11 + 0.05 * o;
        float band = exp(-line * line / (width * width));
        float curt = 0.65 + 0.35 * noise(vec2(p.x * 7.0 - line * 3.0, t * 2.5 + o * 3.0));
        vec3 rib = mix(c1.rgb, c2.rgb, clamp(uv.x * 1.1 - 0.05 + 0.15 * sin(t * 1.3 + p.x * 2.0), 0.0, 1.0));
        col += rib * band * curt * (1.0 - 0.55 * o);
    }
    return col;
}

vec3 silk(vec2 p, float t) {
    vec3 col = c0.rgb;
    float n = fbm(p * 0.8 + vec2(t * 0.6, -t * 0.4));
    for (int k = 0; k < 3; k++) {
        float o = float(k);
        float y = p.y + 0.25 * sin(p.x * (1.4 + 0.3 * o) + t * (1.2 - 0.25 * o) + o * 2.1) + 0.35 * (n - 0.5) - 0.18 * (o - 1.0);
        float fold = exp(-y * y * (18.0 + 10.0 * o));
        float sheen = pow(fold, 6.0);
        vec3 lc = k == 0 ? c1.rgb : k == 1 ? c2.rgb : c3.rgb;
        col += lc * (fold * 0.75 + sheen * 0.5);
    }
    return col;
}

// lava lamp: blobs rise, swell and merge; field rendered as soft glowing wax
vec3 lava(vec2 p, float asp, float t) {
    float f = 0.0;
    float hot = 0.0;
    for (int i = 0; i < 7; i++) {
        float fi = float(i);
        float sp = 0.6 + 0.35 * fract(fi * 0.618);
        float y = 0.75 - mod(t * sp * 1.6 + fi * 0.37, 1.5);
        vec2 c = vec2((fract(fi * 0.413) - 0.5) * asp * 0.8 + 0.18 * sin(t * 2.0 * sp + fi * 1.7), y);
        float r = 0.10 + 0.06 * fract(fi * 0.271) + 0.025 * sin(t * 3.0 + fi);
        float d = dot(p - c, p - c);
        f += r * r / (d + 1e-4);
        hot += r * r / (d + 1e-4) * (0.5 + 0.5 * sin(fi * 2.3));
    }
    vec3 col = mix(c0.rgb, c1.rgb, 0.35 + 0.3 * (0.5 - p.y));
    float body = smoothstep(0.6, 1.4, f);
    float core = smoothstep(1.6, 4.0, f);
    col = mix(col, mix(c2.rgb, c3.rgb, clamp(hot / max(f, 1e-3), 0.0, 1.0)), body * 0.9);
    col += c3.rgb * core * 0.35;
    col += c2.rgb * smoothstep(0.25, 0.6, f) * (1.0 - body) * 0.25;   // halo around the wax
    return col;
}

// sky + sea: gradient sky (c1 zenith -> c3 horizon), sun / moon glow, layered waves with a glitter road
vec3 horizon(vec2 uv, vec2 p, float asp, float t) {
    float hy = 0.64;
    vec2 sp = vec2((sun.x - 0.5) * asp, sun.y - 0.5);
    float moon = sun.w < 0.0 ? 1.0 : 0.0;
    float sw = abs(sun.w);
    vec3 col;
    if (uv.y < hy) {
        float k = pow(uv.y / hy, 1.6);
        col = mix(c1.rgb, c3.rgb, k) * 1.25;
        // slow high clouds
        float cl = fbm(vec2(p.x * 1.5 + t * 0.4, p.y * 5.0 - t * 0.1));
        col = mix(col, mix(c2.rgb, c3.rgb, k), smoothstep(0.5, 0.85, cl) * 0.35 * (0.4 + k));
        float d = length(p - sp);
        col += mix(c3.rgb, vec3(1.0, 0.95, 0.85), 0.4) * sw * (0.35 * exp(-d * d * 6.0) + 0.6 * exp(-d * d * 60.0));
        col += mix(vec3(1.0, 0.93, 0.8), vec3(0.85, 0.9, 1.0), moon) * sw * smoothstep(sun.z, sun.z * 0.8, d);
    } else {
        float depth = (uv.y - hy) / (1.0 - hy);
        col = mix(mix(c3.rgb, c2.rgb, 0.5), c0.rgb * 1.3, pow(depth, 0.7));
        // waves: perspective-squashed noise lines moving toward the viewer
        float z = 1.0 / (depth + 0.04);
        vec2 wp = vec2(p.x * z * 0.6, z * 1.2 - t * 6.0);
        float w = fbm(wp * vec2(1.0, 0.6) + vec2(t * 0.8, 0.0));
        float crest = smoothstep(0.55, 0.8, w);
        col += c2.rgb * crest * 0.25 * (1.0 - depth * 0.6);
        // glitter road below the sun / moon
        float road = exp(-pow((p.x - sp.x) * (2.0 + depth * 6.0), 2.0));
        float gl = smoothstep(0.62, 0.9, noise(vec2(p.x * 90.0 * (0.3 + depth), z * 6.0 - t * 14.0)));
        col += mix(c3.rgb, vec3(1.0), 0.5) * sw * road * (0.25 + gl * 0.9) * (1.0 - depth * 0.5) * step(sp.y, hy - 0.5 + 0.02);
    }
    // haze on the horizon line
    col += c3.rgb * 0.4 * exp(-pow((uv.y - hy) * 22.0, 2.0));
    return col;
}

// drifting out-of-focus lights over a dim glow
vec3 bokeh(vec2 p, float t) {
    vec3 col = glow(p * 0.7, t * 0.5) * 0.55 + c0.rgb * 0.45;
    for (int l = 0; l < 3; l++) {
        float L = float(l);
        float sc = 3.0 + L * 2.5;
        vec2 q = p * sc + vec2(t * (0.8 + L * 0.5), -t * (1.4 + L * 0.7)) + L * 13.7;
        vec2 id = floor(q), f = fract(q);
        for (int j = -1; j <= 1; j++) for (int i = -1; i <= 1; i++) {
            vec2 n = vec2(i, j), cid = id + n;
            vec2 h = hash2(cid);
            if (h.x < 0.55) continue;
            vec2 o = n + 0.5 + 0.35 * sin(6.2831 * h + t * (0.6 + h.y));
            float r = (0.22 + 0.25 * h.y) * (1.0 - L * 0.18);
            float d = length(f - o);
            float disc = smoothstep(r, r * 0.82, d);
            float rim = smoothstep(r, r * 0.92, d) - smoothstep(r * 0.92, r * 0.8, d);
            vec3 lc = h.y < 0.33 ? c1.rgb : h.y < 0.66 ? c2.rgb : c3.rgb;
            float a = (0.10 + 0.12 * h.x) * (0.6 + 0.4 * sin(t * 3.0 + h.x * 40.0)) / (1.0 + L * 0.6);
            col += lc * a * (disc + rim * 0.8);
        }
    }
    return col;
}

// light caught by a rippled water surface, as seen on the floor of a pool
vec3 caustics(vec2 p, float t) {
    vec2 q = mod(p * 4.0, 6.2831) - 250.0;
    vec2 i = q;
    float c = 1.0;
    float inten = 0.005;
    for (int n = 0; n < 5; n++) {
        float tt = t * 4.0 * (1.0 - 3.5 / float(n + 1));
        i = q + vec2(cos(tt - i.x) + sin(tt + i.y), sin(tt - i.y) + cos(tt + i.x));
        c += 1.0 / length(vec2(q.x / (sin(i.x + tt) / inten), q.y / (cos(i.y + tt) / inten)));
    }
    c /= 5.0;
    c = 1.17 - pow(c, 1.4);
    float v = pow(abs(c), 8.0);
    vec3 base = mix(c0.rgb, c1.rgb, 0.5 + 0.5 * fbm(p * 1.2 + t * 0.3));
    return base + mix(c2.rgb, c3.rgb, fbm(p * 2.0 - t * 0.2)) * clamp(v, 0.0, 1.5) * 0.55;
}

// ---------------------------------------------------------------- rain on a window
vec3 rainBg(vec2 p, float t) {
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(-0.5, 0.5, p.y));
    for (int l = 0; l < 2; l++) {
        float L = float(l);
        vec2 q = p * (3.0 + 2.5 * L) + vec2(t * (0.15 + 0.1 * L), 0.0) + L * 7.3;
        vec2 id = floor(q), f = fract(q);
        for (int j = -1; j <= 1; j++) for (int i = -1; i <= 1; i++) {
            vec2 n = vec2(i, j);
            vec2 h = hash2(id + n);
            if (h.x < 0.45) continue;
            vec2 o = n + h;
            float r = 0.45 + 0.35 * h.y;
            float d = length(f - o);
            vec3 lc = h.y < 0.4 ? c2.rgb : h.y < 0.75 ? c3.rgb : c1.rgb * 1.6;
            col += lc * smoothstep(r, 0.0, d) * (0.34 - 0.1 * L) * (0.75 + 0.25 * sin(t * 4.0 + h.x * 40.0));
        }
    }
    return col;
}
vec3 rainglass(vec2 p, float t) {
    vec2 nrm = vec2(0.0);
    float m = 0.0;
    // beads that appear and dry up
    vec2 g = p * 16.0;
    vec2 id = floor(g), f = fract(g);
    vec2 h = hash2(id);
    if (h.y > 0.45) {
        float life = fract(t * 0.6 + h.x * 7.0);
        vec2 o = 0.25 + 0.5 * hash2(id + 3.1);
        float r = (0.12 + 0.18 * h.x) * smoothstep(1.0, 0.7, life) * smoothstep(0.0, 0.05, life);
        vec2 d = f - o;
        float k = smoothstep(r, r * 0.7, length(d));
        if (k > m) { m = k; nrm = d / max(r, 1e-3); }
    }
    // drops sliding down with a trail of small beads
    for (int c = 0; c < 2; c++) {
        float cols = 6.0 + 3.0 * float(c);
        float cx = p.x * cols + float(c) * 0.37;
        float cid = floor(cx);
        float hc = hash(vec2(cid, 3.7 + float(c)));
        if (hc < 0.35) continue;
        float sp = 0.8 + 1.6 * hc;
        float wig = 0.18 * sin(p.y * 9.0 + hc * 20.0) * sin(p.y * 3.1 + hc * 4.0);
        float fx = fract(cx) - 0.5 + wig;
        float yy = fract(p.y * 0.9 - t * sp - hc * 3.0);
        vec2 dd = vec2(fx / cols, (yy - 0.5) / 0.9);
        dd.y *= dd.y < 0.0 ? 0.7 : 1.1;
        float r = 0.016 + 0.01 * hc - 0.004 * float(c);
        float k = smoothstep(r, r * 0.7, length(dd));
        if (k > m) { m = k; nrm = dd / r; }
        if (dd.y < 0.0 && dd.y > -0.3) {
            float ty = fract(yy * 14.0) - 0.5;
            vec2 td = vec2(fx / cols, ty / (14.0 * 0.9));
            float tr = r * 0.35 * (1.0 + dd.y * 3.0);
            float kt = smoothstep(tr, tr * 0.6, length(td)) * step(abs(fx), 0.1) * step(0.5, hash(vec2(cid, floor(yy * 14.0))));
            if (kt > m) { m = kt; nrm = td / max(tr, 1e-3); }
        }
    }
    vec3 fog = rainBg(p, t) * 0.8;
    vec3 thru = rainBg(p - nrm * 0.06, t) * 1.2;
    vec3 col = mix(fog, thru, m);
    col += vec3(1.0) * pow(max(0.0, dot(normalize(vec3(nrm, 1.0)), normalize(vec3(-0.5, -0.6, 0.6)))), 30.0) * m * 0.25;
    col *= 1.0 - 0.35 * m * smoothstep(0.6, 1.0, length(nrm));
    return col;
}

// ---------------------------------------------------------------- topographic contours
vec3 topo(vec2 p, float t) {
    vec2 q = p * 1.3 + vec2(t * 0.25, -t * 0.15);
    float f = fbm(q + 0.35 * vec2(fbm(q * 0.7 + t * 0.2), fbm(q * 0.7 - t * 0.2 + 3.0)));
    vec3 col = mix(c0.rgb, c1.rgb, smoothstep(0.2, 0.8, f) * 0.8);
    float v = f * 16.0;
    float w = fwidth(v);
    float line = 1.0 - smoothstep(0.5 * w, 1.5 * w, abs(fract(v + 0.5) - 0.5));
    float major = 1.0 - smoothstep(0.5 * w, 1.8 * w, abs(fract(v / 4.0 + 0.5) - 0.5) * 4.0);
    col += mix(c2.rgb, c3.rgb, smoothstep(0.3, 0.8, f)) * (line * 0.2 + major * 0.32);
    return col;
}

// ---------------------------------------------------------------- synthwave: striped sun over a neon grid
vec3 synth(vec2 p, float t, inout float sm) {
    float hy = 0.08;
    vec3 col;
    if (p.y < hy) {
        float k = (p.y + 0.5) / (hy + 0.5);
        col = mix(c0.rgb, c1.rgb, k * k);
        vec2 sc = vec2(0.0, hy - 0.15);
        float R = 0.21;
        float d = length(p - sc);
        float yb = (p.y - sc.y) / R;
        float band = 0.5 + 0.5 * sin(p.y * 110.0 - t * 3.0);
        float on = yb < 0.0 ? 1.0 : step(yb * 0.9, band);
        vec3 sunCol = mix(c3.rgb, c2.rgb, clamp((p.y - sc.y + R) / (2.0 * R), 0.0, 1.0));
        col = mix(col, sunCol, smoothstep(R, R - 0.004, d) * on);
        col += c2.rgb * 0.22 * exp(-d * d * 10.0);
        sm = 1.0 - smoothstep(R * 1.6, R, d);
    } else {
        float z = 0.12 / (p.y - hy + 0.002);
        vec2 g = vec2(p.x * z * 2.0, z * 2.0 + t * 8.0);
        vec2 w = fwidth(g);
        float lx = 1.0 - smoothstep(0.0, 1.5 * w.x, abs(fract(g.x + 0.5) - 0.5));
        float lz = 1.0 - smoothstep(0.0, 1.5 * w.y, abs(fract(g.y + 0.5) - 0.5));
        col = c0.rgb * 0.7 + c2.rgb * max(lx, lz) * 0.55 * smoothstep(40.0, 2.0, z);
        sm = 0.0;
    }
    col += c3.rgb * 0.3 * exp(-pow((p.y - hy) * 25.0, 2.0));
    return col;
}

// ---------------------------------------------------------------- drifting stars, a shooting star now and then
vec3 starfield(vec2 p, float t) {
    vec3 col = glow(p * 0.6, t * 0.3) * 0.35 + c0.rgb * 0.65;
    for (int l = 0; l < 4; l++) {
        float L = float(l);
        float sc = 8.0 + L * 7.0;
        vec2 q = p * sc + vec2(t * (0.6 + L * 0.8), t * (0.2 + L * 0.3)) + L * 31.0;
        vec2 id = floor(q), f = fract(q) - 0.5;
        vec2 h = hash2(id);
        if (h.x > 0.72) {
            vec2 o = (h - 0.5) * 0.6;
            float d = length(f - o);
            float s = smoothstep(0.12 + 0.04 * (3.0 - L), 0.0, d) * (0.6 + 0.4 * sin(t * 20.0 * h.y + h.x * 50.0));
            col += mix(vec3(0.8, 0.88, 1.0), c3.rgb * 2.0, h.y * 0.5) * s * (0.25 + L * 0.2);
        }
    }
    float per = floor(time / 25.0);
    float ph = fract(time / 25.0) * 25.0;
    if (ph < 1.2) {
        vec2 hs = hash2(vec2(per, 1.3));
        vec2 dir = normalize(vec2(1.0, 0.45));
        float k = ph / 1.2;
        vec2 head = vec2((hs.x - 0.7) * 1.4, -0.45 + hs.y * 0.3) + dir * k * 0.8;
        vec2 rel = p - head;
        float along = dot(rel, -dir);
        float across = abs(dot(rel, vec2(-dir.y, dir.x)));
        float tail = smoothstep(0.25, 0.0, along) * step(0.0, along) * smoothstep(0.004, 0.0, across);
        col += vec3(0.9, 0.95, 1.0) * tail * sin(k * 3.14159) * 0.8;
    }
    return col;
}

// ---------------------------------------------------------------- dunes at dusk
vec3 dunes(vec2 p, float t, inout float sm) {
    float k = clamp(p.y + 0.5, 0.0, 1.0);
    vec3 col = mix(c1.rgb, c3.rgb, smoothstep(0.0, 0.6, k));
    vec2 sp2 = vec2(0.35, -0.02);
    col += c3.rgb * 0.4 * exp(-dot(p - sp2, p - sp2) * 8.0);
    sm = 1.0 - smoothstep(-0.25, 0.0, p.y);
    for (int i = 0; i < 4; i++) {
        float L = float(i);
        float x = p.x * (1.2 + L * 0.5) + L * 3.7 + t * (0.05 + 0.08 * L);
        float y0 = -0.02 + L * 0.1 + 0.06 * sin(x * 1.3) + 0.04 * sin(x * 2.7 + 1.0) + 0.02 * fbm(vec2(x * 2.0, L)) - 0.05 * abs(sin(x * 1.1 + 0.5));
        if (p.y > y0) {
            float slope = 0.078 * cos(x * 1.3) + 0.108 * cos(x * 2.7 + 1.0);
            float lit = clamp(0.5 + slope * 4.0, 0.0, 1.0);
            vec3 sand = mix(c2.rgb, c3.rgb, lit * 0.6) * (1.0 - 0.15 * L);
            float rip = 0.5 + 0.5 * sin((p.x * 60.0 + (p.y - y0) * 120.0 + noise(p * 8.0) * 6.0) * (1.0 + L * 0.3));
            sand *= 0.92 + 0.08 * rip;
            float haze = exp(-(p.y - y0) * 40.0) * noise(vec2(p.x * 10.0 - t * 12.0, L));
            col = mix(c0.rgb, sand, 0.85 - 0.15 * L) + c3.rgb * haze * 0.15;
        }
    }
    return col;
}

// ---------------------------------------------------------------- mesh gradient (four drifting colour points)
vec3 mesh(vec2 p, float t) {
    vec2 a = vec2(-0.6 + 0.25 * sin(t * 1.1), -0.25 + 0.15 * cos(t * 0.9));
    vec2 b = vec2(0.6 + 0.2 * cos(t * 0.8), -0.3 + 0.2 * sin(t * 1.3));
    vec2 c = vec2(-0.4 + 0.3 * cos(t * 0.7 + 2.0), 0.35 + 0.1 * sin(t * 1.2));
    vec2 d = vec2(0.5 + 0.25 * sin(t + 1.0), 0.3 + 0.15 * cos(t * 0.6));
    vec2 q = p + 0.12 * vec2(fbm(p * 1.5 + t * 0.4) - 0.5, fbm(p * 1.5 - t * 0.4 + 5.0) - 0.5);
    float wa = 1.0 / (dot(q - a, q - a) * 6.0 + 0.05);
    float wb = 1.0 / (dot(q - b, q - b) * 6.0 + 0.05);
    float wc = 1.0 / (dot(q - c, q - c) * 6.0 + 0.05);
    float wd = 1.0 / (dot(q - d, q - d) * 6.0 + 0.05);
    return (c0.rgb * wa + c1.rgb * wb + c2.rgb * wc + c3.rgb * wd) / (wa + wb + wc + wd);
}

// ---------------------------------------------------------------- mountains, valley fog, sun / moon
float ridged(float x, float L) {
    float r = 0.0, a = 0.5, f = 1.0;
    for (int i = 0; i < 4; i++) { r += a * (1.0 - abs(noise(vec2(x * f, L * 7.0)) * 2.0 - 1.0)); f *= 2.1; a *= 0.5; }
    return r;
}
vec3 mountains(vec2 uv, vec2 p, float asp, float t, inout float sm) {
    float k = clamp(uv.y / 0.7, 0.0, 1.0);
    vec3 col = mix(c1.rgb, c3.rgb, pow(k, 1.5)) * 1.15;
    vec2 sp = vec2((sun.x - 0.5) * asp, sun.y - 0.5);
    float sw = abs(sun.w);
    float moon = sun.w < 0.0 ? 1.0 : 0.0;
    float d = length(p - sp);
    col += mix(c3.rgb, vec3(1.0, 0.95, 0.85), 0.4) * sw * (0.3 * exp(-d * d * 6.0) + 0.5 * exp(-d * d * 60.0));
    col += mix(vec3(1.0, 0.93, 0.8), vec3(0.85, 0.9, 1.0), moon) * sw * smoothstep(sun.z, sun.z * 0.8, d);
    sm = 1.0 - k;
    for (int i = 0; i < 5; i++) {
        float L = float(i);
        float x = p.x * (1.0 + L * 0.6) + L * 5.3 + t * 0.04 * (L + 1.0);
        float y0 = 0.02 + L * 0.075 - 0.28 * ridged(x, L) * (1.0 - L * 0.12);
        if (p.y > y0) {
            vec3 lc = mix(mix(c3.rgb, c1.rgb, 0.5) * 0.85, c0.rgb, L / 4.0);
            float fogA = smoothstep(y0, y0 + 0.18, p.y) * fbm(vec2(p.x * 2.0 - t * (0.5 + 0.2 * L), p.y * 4.0 + L)) * (0.55 - 0.08 * L);
            col = mix(lc, mix(c3.rgb, c1.rgb, 0.4) * 0.9, fogA);
            sm = 0.0;
        }
    }
    return col;
}

// ---------------------------------------------------------------- waves of light (music lifts them)
vec3 lightwaves(vec2 p, float t, float e) {
    vec3 col = glow(p * 0.6, t * 0.3) * 0.35 + c0.rgb * 0.65;
    for (int i = 0; i < 5; i++) {
        float L = float(i);
        float amp = (0.05 + 0.03 * sin(t * 2.0 + L)) * (1.0 + 0.5 * e);
        float y = 0.22 + amp * sin(p.x * (2.0 + L * 0.35) + t * (3.0 + L * 0.7) + L * 1.3) * (0.6 + 0.4 * sin(p.x * 0.8 - t + L));
        float d = abs(p.y - y);
        vec3 lc = L < 1.5 ? c1.rgb : L < 3.5 ? c2.rgb : c3.rgb;
        col += lc * (0.0025 / (d + 0.004)) * 0.3 * smoothstep(1.0, 0.3, abs(p.x));
    }
    return col;
}

// ---------------------------------------------------------------- snowfall
vec3 snowfall(vec2 p, float t) {
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(-0.5, 0.5, p.y));
    col += c3.rgb * 0.18 * exp(-pow((p.y - 0.35) * 4.0, 2.0));
    for (int l = 0; l < 4; l++) {
        float L = float(l);
        float sc = 26.0 - L * 4.5;
        vec2 q = p * sc;
        q.y -= t * (3.0 + L * 1.5) * 1.6;
        q.x += sin(q.y * 0.4 + L) * 0.6 + t * 0.6;
        vec2 id = floor(q), f = fract(q) - 0.5;
        vec2 h = hash2(id + L * 17.0);
        if (h.x > 0.6) {
            vec2 o = (h - 0.5) * 0.5 + vec2(0.15 * sin(t * 6.0 * (h.y + 0.3) + h.x * 30.0), 0.0);
            float r = 0.05 + 0.05 * h.y + L * 0.02;
            col += vec3(0.9, 0.94, 1.0) * smoothstep(r, r * (0.1 + 0.2 * L), length(f - o)) * (0.14 + 0.1 * L);
        }
    }
    return col;
}

// ---------------------------------------------------------------- fireflies over a meadow
vec3 fireflies(vec2 p, float t, inout float sm) {
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(-0.5, 0.3, p.y));
    float tl = 0.12 - 0.12 * fbm(vec2(p.x * 3.0, 1.0)) - 0.05 * pow(noise(vec2(p.x * 14.0, 2.0)), 2.0);
    sm = 1.0 - smoothstep(tl - 0.08, tl, p.y);
    if (p.y > tl) col = mix(c0.rgb * 0.6, c2.rgb * 0.25, smoothstep(tl + 0.1, tl, p.y));
    float gx = p.x * 90.0;
    float gh = 0.36 - 0.06 * hash(vec2(floor(gx), 4.0)) + 0.02 * sin(p.x * 3.0 + t * 2.0);
    if (p.y > gh + abs(fract(gx) - 0.5) * 0.1) col = c0.rgb * 0.4;
    for (int l = 0; l < 2; l++) {
        float L = float(l);
        vec2 q = p * (4.0 + L * 3.0) + L * 11.0;
        vec2 id = floor(q), f = fract(q);
        for (int j = -1; j <= 1; j++) for (int i = -1; i <= 1; i++) {
            vec2 n = vec2(i, j);
            vec2 h = hash2(id + n);
            if (h.x < 0.5) continue;
            vec2 o = n + 0.5 + 0.4 * vec2(sin(t * (3.0 + h.y * 4.0) + h.x * 20.0), cos(t * (2.5 + h.x * 3.0) + h.y * 17.0));
            vec2 r = f - o;
            float d2 = dot(r, r) / (1.0 + L);
            float blink = smoothstep(0.2, 1.0, sin(t * (25.0 + 15.0 * h.y) + h.x * 50.0));
            col += c3.rgb * blink * (exp(-d2 * 90.0) * 0.8 + exp(-d2 * 8.0) * 0.12) * smoothstep(-0.35, 0.15, p.y);
        }
    }
    return col;
}

// ---------------------------------------------------------------- dot matrix swell
vec3 dotgrid(vec2 uv, float t) {
    vec2 g = uv * size / 32.0;
    vec2 id = floor(g), f = fract(g) - 0.5;
    vec2 cp = (id + 0.5) * 32.0 / size.y;
    float w = fbm(cp * 1.4 + vec2(t * 0.8, -t * 0.5));
    float wave = 0.5 + 0.5 * sin(cp.x * 3.0 - t * 6.0 + w * 5.0);
    float v = smoothstep(0.35, 0.8, w) * (0.4 + 0.6 * wave);
    float r = 0.12 + 0.2 * v;
    vec3 dc = mix(c1.rgb * 1.6, mix(c2.rgb, c3.rgb, wave), v);
    return c0.rgb + c1.rgb * 0.3 * w + dc * smoothstep(r, r - 0.08, length(f)) * (0.35 + 0.9 * v);
}

// ---------------------------------------------------------------- crystal cells
vec3 cells(vec2 p, float t) {
    vec2 q = p * 4.0;
    vec2 id = floor(q), f = fract(q);
    float d1 = 8.0, d2 = 8.0;
    vec2 cid = id;
    for (int j = -1; j <= 1; j++) for (int i = -1; i <= 1; i++) {
        vec2 n = vec2(i, j);
        vec2 h = hash2(id + n);
        vec2 o = n + 0.5 + 0.4 * sin(t * 2.0 + 6.2831 * h);
        float d = length(f - o);
        if (d < d1) { d2 = d1; d1 = d; cid = id + n; } else if (d < d2) d2 = d;
    }
    float edge = d2 - d1;
    float hh = hash(cid);
    vec3 fill = mix(c0.rgb, mix(c1.rgb, c2.rgb, hh), 0.35 + 0.25 * sin(t * 3.0 + hh * 20.0)) * (0.7 + 0.3 * (1.0 - d1));
    return fill + mix(c2.rgb, c3.rgb, hh) * (smoothstep(0.08, 0.0, edge) * 0.45 + smoothstep(0.3, 0.0, edge) * 0.1);
}

// ---------------------------------------------------------------- smoke rising
vec3 smoke(vec2 p, float t) {
    vec2 q = p * vec2(1.6, 1.0) + vec2(0.0, t * 1.2);
    float a = fbm(q + vec2(t * 0.3, 0.0));
    float b = fbm(q * 1.7 + 3.0 * vec2(a, fbm(q * 0.8 - t * 0.4)) - vec2(0.0, t * 0.8));
    float wisp = pow(smoothstep(0.35, 0.85, b), 2.0);
    vec3 col = c0.rgb + mix(c1.rgb, c2.rgb, a) * 0.5 * smoothstep(0.2, 0.8, a);
    return col + mix(c2.rgb, c3.rgb, b) * wisp * 0.7;
}

// ---------------------------------------------------------------- pond: rings from falling drops
vec3 pond(vec2 p, float t) {
    vec2 grad = vec2(0.0);
    for (int l = 0; l < 2; l++) {
        float L = float(l);
        vec2 q = p * (2.5 + L * 1.5) + L * 9.1;
        vec2 id = floor(q), f = fract(q);
        for (int j = -1; j <= 1; j++) for (int i = -1; i <= 1; i++) {
            vec2 n = vec2(i, j);
            vec2 h = hash2(id + n + L * 31.0);
            vec2 o = n + 0.2 + 0.6 * h;
            float ph = fract(t * 2.7 / (0.6 + h.y) + h.x);
            vec2 rel = f - o;
            float d = length(rel);
            float x = (d - ph * 0.9) * 14.0;
            float amp = (1.0 - ph) * (1.0 - ph) * exp(-x * x);
            grad += rel / max(d, 1e-3) * cos(x * 3.0) * amp;
        }
    }
    vec3 col = mix(c1.rgb, c0.rgb, smoothstep(-0.5, 0.5, p.y));
    col += c1.rgb * 0.3 * fbm(p * 2.0 + t * 0.2);
    float g = length(grad);
    float lit = max(0.0, dot(grad / max(g, 1e-3), normalize(vec2(-0.6, -0.8))));
    return col + c3.rgb * lit * min(g, 1.5) * 0.3 + c2.rgb * min(g, 1.5) * 0.08;
}

void main() {
    vec2 uv = qt_TexCoord0;
    float asp = size.x / max(size.y, 1.0);
    vec2 p = vec2((uv.x - 0.5) * asp, uv.y - 0.5);
    float t = time * 0.045;
    float e = opt.w;
    int m = int(mode + 0.5);
    float sm = 1.0;                     // where the generic stars may show (sky)
    vec3 col;
    if (m == 0) col = glow(p, t);
    else if (m == 1) col = aurora(p, uv, t);
    else if (m == 2) col = glow(p * 0.8, t * 0.7);
    else if (m == 3) col = silk(p, t);
    else if (m == 4) col = lava(p, asp, t);
    else if (m == 5) { col = horizon(uv, p, asp, t); sm = max(0.0, 1.0 - uv.y / 0.64); }
    else if (m == 6) col = bokeh(p, t);
    else if (m == 7) col = caustics(p, t);
    else if (m == 8) { col = rainglass(p, t); sm = 0.0; }
    else if (m == 9) col = topo(p, t);
    else if (m == 10) col = synth(p, t, sm);
    else if (m == 11) { col = starfield(p, t); sm = 0.0; }
    else if (m == 12) col = dunes(p, t, sm);
    else if (m == 13) col = mesh(p, t);
    else if (m == 14) col = mountains(uv, p, asp, t, sm);
    else if (m == 15) col = lightwaves(p, t, e);
    else if (m == 16) { col = snowfall(p, t); sm = 0.0; }
    else if (m == 17) col = fireflies(p, t, sm);
    else if (m == 18) col = dotgrid(uv, t);
    else if (m == 19) col = cells(p, t);
    else if (m == 20) col = smoke(p, t);
    else col = pond(p, t);

    // stars: sparse cells, each with its own twinkle
    if (opt.x > 0.0 && sm > 0.0) {
        vec2 g = uv * size / 3.0;
        vec2 id = floor(g), f = fract(g) - 0.5;
        float h = hash(id);
        float s = step(0.985, h) * smoothstep(0.35, 0.0, length(f));
        s *= 0.55 + 0.45 * sin(time * (0.6 + 2.0 * fract(h * 37.0)) + h * 60.0);
        col += vec3(0.85, 0.9, 1.0) * s * opt.x * sm;
    }
    // music: a gentle, slow lift of the scene (the level is already a smoothed loudness)
    col *= 1.0 + 0.12 * e;
    col += c3.rgb * e * 0.04 * exp(-dot(p, p) * 2.5);
    // vignette keeps the edges calm and the centre (the widgets) in a soft light
    col *= mix(1.0, 1.0 - 0.55 * dot(p / vec2(asp, 1.0), p / vec2(asp, 1.0)) * 2.2, opt.y);
    col *= opt.z;
    // fixed sub-LSB dither so the dark gradients don't band (drawn straight at full resolution)
    col += (hash(gl_FragCoord.xy) + hash(gl_FragCoord.xy + 17.3) - 1.0) / 255.0;
    fragColor = vec4(col, 1.0) * qt_Opacity;
}
