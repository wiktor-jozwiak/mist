#version 440
// Pass A of the weather sky (rendered at half resolution, see components/Sky.qml): the sky dome seen by a
// camera looking south, and volumetric clouds ray-marched through a slab (3D value-noise fbm, coverage
// remap, flat base / rounded top, edge erosion). Each step: Beer transmittance toward the real sun
// direction, two-lobe Henyey-Greenstein phase (silver lining), a multiple-scattering term, sky / ground
// ambient. Far clouds dissolve in haze; aurora behind, rainbow (42° around the antisolar point) in front.
// Output: rgb = finished sky + clouds, a = cloud opacity (pass B hides stars / discs behind it).
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float time;
    float fov;           // tan scale of the view: ray = ((x-.5)*asp*fov, (HOR-y)*fov, 1), then pitched up
    float pitch;         // radians the camera tilts up (a high sun / moon stays in frame)
    vec2 size;
    vec4 sunDir;         // world direction to the sun (x west, y up, z south), altitude deg
    vec4 scene;          // -, -, cloud drift (world units, + = west), evolution
    vec4 wx;             // cover, rain, snow, fog
    vec4 wx2;            // storm, moon light, aurora, haze
    vec4 bolt;           // flash, bolt x (0..1), -, -
    vec4 fx;             // rainbow strength, horizon y (0..1 of the height), snow on the ground, -
};

float HOR = 0.86;

float hash(vec2 p) { vec3 q = fract(vec3(p.xyx) * 0.1031); q += dot(q, q.yzx + 33.33); return fract((q.x + q.y) * q.z); }
float hash3(vec3 p) { p = fract(p * 0.1031); p += dot(p, p.zyx + 31.32); return fract((p.x + p.y) * p.z); }
float noise3(vec3 p) {
    vec3 i = floor(p), f = fract(p);
    vec3 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash3(i), hash3(i + vec3(1, 0, 0)), u.x), mix(hash3(i + vec3(0, 1, 0)), hash3(i + vec3(1, 1, 0)), u.x), u.y),
               mix(mix(hash3(i + vec3(0, 0, 1)), hash3(i + vec3(1, 0, 1)), u.x), mix(hash3(i + vec3(0, 1, 1)), hash3(i + vec3(1, 1, 1)), u.x), u.y), u.z);
}
float fbm3(vec3 p, int oct) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { if (i >= oct) break; v += a * noise3(p); p = p * 2.03 + vec3(17.1, 3.7, 9.3); a *= 0.5; }
    return v;
}
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
float hg(float c, float g) { float g2 = g * g; return (1.0 - g2) / pow(1.0 + g2 - 2.0 * g * c, 1.5); }   // x4pi: isotropic = 1

float cov, base0, thick;
// cloud density at a world point (0..1)
float density(vec3 pos, bool detail) {
    float h = (pos.y - base0) / thick;
    if (h < 0.0 || h > 1.0) return 0.0;
    vec3 q = pos * 0.62 + vec3(-scene.z, 0.0, scene.w * 0.35);
    float b = fbm3(q, 3);
    float thr = mix(0.58, 0.28, cov) + h * 0.16;              // only strong cells grow tall: rounded tops
    float prof = smoothstep(0.0, 0.10, h) * smoothstep(1.0, 0.6, h);
    float d = (b - thr) * 3.2 * prof;
    if (d <= 0.0) return 0.0;
    if (detail) {
        float det = fbm3(q * 3.3 + vec3(0.0, scene.w * 0.8, 0.0), 2);
        d -= (1.0 - clamp(d, 0.0, 1.0)) * det * 0.55;          // eroded, wispy edges
    }
    return clamp(d, 0.0, 1.0);
}

void main() {
    vec2 uv = qt_TexCoord0;
    if (fx.y > 0.0) HOR = fx.y;
    float asp = size.x / size.y;
    vec3 rc = normalize(vec3((uv.x - 0.5) * asp * fov, (HOR - uv.y) * fov, 1.0));
    float cp = cos(pitch), sp = sin(pitch);
    vec3 rd = vec3(rc.x, rc.y * cp + rc.z * sp, -rc.y * sp + rc.z * cp);
    vec3 L = sunDir.xyz;
    float s = sunDir.w;
    cov = wx.x;
    float rain = wx.y, fog = wx.w, storm = wx2.x, moonLight = wx2.y;

    float dayK = smoothstep(-6.0, 7.0, s);
    float nightK = 1.0 - smoothstep(-16.0, -5.0, s);
    float warm = exp(-pow((s - 2.0) / 6.0, 2.0));
    float gloom = smoothstep(0.6, 1.0, cov);
    vec3 sunCol = mix(vec3(1.0, 0.48, 0.22), vec3(1.0, 0.95, 0.87), smoothstep(0.0, 20.0, s));

    // ------------------------------------------------------------ sky dome
    float e = rd.y;
    vec3 zen = mix(vec3(0.025, 0.045, 0.11), vec3(0.05, 0.08, 0.20), smoothstep(-18.0, -9.0, s));
    zen = mix(zen, vec3(0.11, 0.18, 0.40), smoothstep(-9.0, -2.0, s));
    zen = mix(zen, vec3(0.12, 0.32, 0.68), smoothstep(-2.0, 12.0, s));
    vec3 hor = mix(vec3(0.07, 0.09, 0.18), vec3(0.17, 0.18, 0.34), smoothstep(-18.0, -7.0, s));
    hor = mix(hor, vec3(0.56, 0.70, 0.88), smoothstep(-1.0, 14.0, s));
    zen += vec3(0.02, 0.04, 0.08) * moonLight;
    hor += vec3(0.04, 0.05, 0.08) * moonLight;
    float toSun = max(dot(normalize(rd.xz + 1e-4), normalize(L.xz + 1e-4)), 0.0);
    hor = mix(hor, vec3(0.98, 0.50, 0.26), warm * (0.35 + 0.65 * pow(toSun, 3.0)));
    zen = mix(zen, vec3(0.30, 0.24, 0.46), warm * 0.40);
    hor += vec3(0.09, 0.06, 0.04) * (1.0 - dayK) * (0.4 + 0.6 * cov);                  // town glow
    vec3 gz = mix(vec3(0.09, 0.10, 0.14), vec3(0.38, 0.42, 0.49), dayK) * (1.0 - 0.30 * rain - 0.35 * storm);
    vec3 gh = mix(vec3(0.17, 0.16, 0.19), vec3(0.52, 0.56, 0.62), dayK) * (1.0 - 0.25 * rain - 0.30 * storm);
    zen = mix(zen, gz, gloom * 0.85);
    hor = mix(hor, gh, gloom * 0.85);
    vec3 sky = mix(hor, zen, smoothstep(0.0, 0.75, e));
    float mu = dot(rd, L);
    sky += sunCol * (pow(max(mu, 0.0), 24.0) * 0.16 + pow(max(mu, 0.0), 4.0) * 0.05)                 // wide glow; disc in pass B
         * smoothstep(-2.0, 1.0, s) * (1.0 - 0.7 * gloom);
    if (e < 0.0) {                                                                    // below the horizon: fields fading into haze
        vec3 field = mix(vec3(0.035, 0.045, 0.04), vec3(0.20, 0.25, 0.20), dayK) * (1.0 - 0.3 * rain);
        field = mix(field, mix(vec3(0.16, 0.18, 0.22), vec3(0.78, 0.82, 0.88), dayK), fx.z);
        sky = mix(hor * 0.9, field, smoothstep(0.0, -0.12, e));
    }

    // aurora, behind everything
    if (wx2.z > 0.0 && e > 0.0) {
        float base = 0.18 + 0.03 * sin(rd.x * 6.0 + time * 0.12) + 0.02 * sin(rd.x * 17.0 - time * 0.25);
        float above = e - base * 0.4;
        float rays = (0.5 + 0.5 * noise(vec2(rd.x * 60.0 + time * 0.4, time * 0.1))) * (0.5 + 0.5 * noise(vec2(rd.x * 9.0 - time * 0.1, 2.0)));
        float a = above > 0.0 ? exp(-above * 7.0) * smoothstep(0.0, 0.02, above) : 0.0;
        sky += mix(vec3(0.20, 0.95, 0.55), vec3(0.75, 0.30, 0.85), smoothstep(0.03, 0.25, above)) * a * rays * wx2.z * nightK * 0.5;
    }

    // ------------------------------------------------------------ volumetric clouds
    float T = 1.0;
    vec3 scat = vec3(0.0);
    float tHit = 0.0;
    if (cov > 0.01 && e > 0.015) {
        base0 = 1.0;
        thick = mix(0.45, 0.95, cov);
        float t0 = base0 / e, t1 = (base0 + thick) / e;
        if (t0 < 30.0) {
            tHit = t0;
            t1 = min(t1, t0 + 5.0);
            const int N = 12;
            float dt = (t1 - t0) / float(N);
            float tt = t0 + dt * hash(gl_FragCoord.xy);
            vec3 Ll = normalize(mix(vec3(0.0, 1.0, 0.0), L, smoothstep(-3.0, 3.0, s) * 0.85));
            float cosT = dot(rd, Ll);
            float phase = 0.7 * hg(cosT, 0.6) + 0.3 * hg(cosT, -0.25);
            vec3 sunL = sunCol * dayK * (1.0 - 0.65 * gloom) * 1.15 + vec3(0.32, 0.37, 0.50) * moonLight * 0.35;
            vec3 ambTop = mix(vec3(0.14, 0.15, 0.19), mix(vec3(0.50, 0.58, 0.72), vec3(0.50, 0.53, 0.58), gloom), dayK);
            vec3 ambBot = mix(vec3(0.17, 0.165, 0.19), mix(vec3(0.30, 0.32, 0.37), vec3(0.24, 0.26, 0.30), gloom), dayK);
            float sigma = 7.0 + 5.0 * cov;
            for (int i = 0; i < N; i++) {
                vec3 pos = rd * tt;
                float d = density(pos, true);
                if (d > 0.004) {
                    float dl = density(pos + Ll * 0.12, false) + density(pos + Ll * 0.32, false) * 0.8;
                    float Tl = exp(-dl * 2.6);
                    float h = clamp((pos.y - base0) / thick, 0.0, 1.0);
                    float powder = 1.0 - exp(-d * 5.0);
                    vec3 lum = sunL * (Tl * phase * 0.55 + pow(Tl, 0.3) * 0.45 * (0.5 + 0.5 * powder)) + mix(ambBot, ambTop, h);
                    lum += vec3(0.85, 0.85, 1.0) * bolt.x * exp(-abs(uv.x - bolt.y) * 4.0) * 1.6;
                    lum *= 1.0 - 0.30 * rain - 0.35 * storm;
                    float Ts = exp(-d * sigma * dt);
                    scat += T * (1.0 - Ts) * lum;
                    T *= Ts;
                    if (T < 0.03) break;
                }
                tt += dt;
            }
        }
    }
    // distance haze: far clouds melt into the horizon
    float fade = exp(-tHit * 0.075);
    vec3 col = sky + (scat - sky * (1.0 - T)) * fade;
    float occ = (1.0 - T) * fade;

    // rainbow: in the rain between us and the clouds
    if (fx.x > 0.0 && e > 0.0) {
        float ang = degrees(acos(clamp(dot(rd, -L), -1.0, 1.0)));
        float k = (ang - 40.4) / 2.2;
        if (k > 0.0 && k < 1.0) {
            vec3 rb = clamp(abs(mod((1.0 - k) * 4.5 + vec3(0.0, 4.0, 2.0), 6.0) - 3.0) - 1.0, 0.0, 1.0);
            col += rb * sin(k * 3.1416) * fx.x * 0.28 * smoothstep(0.0, 0.08, e);
        }
    }
    col = col / (1.0 + max(max(col.r, col.g), col.b) * 0.18);                 // gentle shoulder: no clipped channels
    fragColor = vec4(col * 1.12, occ);
}
