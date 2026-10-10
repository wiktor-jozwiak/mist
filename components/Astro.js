.pragma library

// Low-precision ephemeris for the sky scene (formulas after SunCalc / Meeus: sun ~0.01°, moon ~0.5°).
// Everything the card needs: where the Sun and the Moon stand (altitude / compass azimuth), the Moon's
// lit fraction, the phase angle, where its bright limb points as seen from the ground, and its tilt.

const rad = Math.PI / 180
const e = rad * 23.4397                       // obliquity of the ecliptic

function toDays(date) { return date.valueOf() / 86400000 - 0.5 + 2440588 - 2451545 }
function ra(l, b) { return Math.atan2(Math.sin(l) * Math.cos(e) - Math.tan(b) * Math.sin(e), Math.cos(l)) }
function dec(l, b) { return Math.asin(Math.sin(b) * Math.cos(e) + Math.cos(b) * Math.sin(e) * Math.sin(l)) }
// azimuth from the south, positive westward (SunCalc convention); altitude
function azim(H, phi, d) { return Math.atan2(Math.sin(H), Math.cos(H) * Math.sin(phi) - Math.tan(d) * Math.cos(phi)) }
function alti(H, phi, d) { return Math.asin(Math.sin(phi) * Math.sin(d) + Math.cos(phi) * Math.cos(d) * Math.cos(H)) }
function sidereal(d, lw) { return rad * (280.16 + 360.9856235 * d) - lw }
// atmospheric refraction lifts a body near the horizon by ~0.5°
function refraction(h) { h = Math.max(h, 0); return 0.0002967 / Math.tan(h + 0.00312536 / (h + 0.08901179)) }

function sunCoords(d) {
    const M = rad * (357.5291 + 0.98560028 * d)
    const C = rad * (1.9148 * Math.sin(M) + 0.02 * Math.sin(2 * M) + 0.0003 * Math.sin(3 * M))
    const L = M + C + rad * 102.9372 + Math.PI
    return { dec: dec(L, 0), ra: ra(L, 0) }
}
function moonCoords(d) {
    const L = rad * (218.316 + 13.176396 * d), M = rad * (134.963 + 13.064993 * d), F = rad * (93.272 + 13.229350 * d)
    const l = L + rad * 6.289 * Math.sin(M), b = rad * 5.128 * Math.sin(F)
    return { ra: ra(l, b), dec: dec(l, b), dist: 385001 - 20905 * Math.cos(M) }
}

// compass azimuth in degrees (0 = N, 90 = E, 180 = S, 270 = W)
function compass(a) { return ((a / rad + 180) % 360 + 360) % 360 }

function sun(date, lat, lon) {
    const lw = -lon * rad, phi = lat * rad, d = toDays(date)
    const c = sunCoords(d), H = sidereal(d, lw) - c.ra
    return { alt: alti(H, phi, c.dec) / rad, az: compass(azim(H, phi, c.dec)) }
}

function moon(date, lat, lon) {
    const lw = -lon * rad, phi = lat * rad, d = toDays(date)
    const c = moonCoords(d), H = sidereal(d, lw) - c.ra
    let h = alti(H, phi, c.dec)
    h += refraction(h)
    // parallactic angle: how far the Moon's north is turned away from the zenith
    const pa = Math.atan2(Math.sin(H), Math.tan(phi) * Math.cos(c.dec) - Math.sin(c.dec) * Math.cos(H))
    const s = sunCoords(d), sd = 149598000
    const psi = Math.acos(Math.sin(s.dec) * Math.sin(c.dec) + Math.cos(s.dec) * Math.cos(c.dec) * Math.cos(s.ra - c.ra))
    const inc = Math.atan2(sd * Math.sin(psi), c.dist - sd * Math.cos(psi))          // phase angle: 0 full, pi new
    // bright limb, eastward from north; minus the parallactic angle = measured from the zenith
    const limb = Math.atan2(Math.cos(s.dec) * Math.sin(s.ra - c.ra),
                            Math.sin(s.dec) * Math.cos(c.dec) - Math.cos(s.dec) * Math.sin(c.dec) * Math.cos(s.ra - c.ra))
    return {
        alt: h / rad, az: compass(azim(H, phi, c.dec)),
        fraction: (1 + Math.cos(inc)) / 2,
        phase: 0.5 + 0.5 * inc * (limb < 0 ? -1 : 1) / Math.PI,      // 0 new, .25 first quarter, .5 full, .75 last
        inc: inc,
        limbZenith: limb - pa,
        pa: pa
    }
}

// meteor showers (peak month, day, ZHR, half-width in days) + sporadic background:
// how many meteors per hour an observer under a dark sky would see right now
const showers = [[1, 3, 110, 0.6], [4, 22, 18, 1.5], [5, 6, 12, 3], [7, 30, 20, 3], [8, 12, 100, 2.5],
                 [10, 8, 10, 0.6], [10, 21, 20, 3], [11, 17, 15, 1.5], [12, 14, 150, 1.2], [12, 22, 10, 0.7]]
function meteorZhr(date) {
    let z = 6
    for (const s of showers) {
        const peak = new Date(date.getFullYear(), s[0] - 1, s[1], 12)
        let dd = (date - peak) / 86400000
        if (dd > 182) dd -= 365; else if (dd < -182) dd += 365
        z += s[2] * Math.exp(-(dd * dd) / (2 * s[3] * s[3]))
    }
    return z
}

// moonrise / moonset on the local day of `date` (10-minute scan + linear refinement); null = none that day
function moonTimes(date, lat, lon) {
    const d0 = new Date(date.getFullYear(), date.getMonth(), date.getDate())
    const h0 = 0.133                                               // centre at the horizon incl. refraction
    let rise = null, set = null
    let prev = moon(d0, lat, lon).alt - h0
    for (let i = 1; i <= 144; i++) {
        const t = new Date(d0.valueOf() + i * 600000)
        const a = moon(t, lat, lon).alt - h0
        if ((prev < 0) !== (a < 0)) {
            const x = new Date(t.valueOf() - 600000 * a / (a - prev))
            if (prev < 0) { if (!rise) rise = x } else { if (!set) set = x }
        }
        prev = a
    }
    return { rise: rise, set: set }
}

// next moment the phase (0 new, .25 first quarter, .5 full, .75 last quarter) reaches `target`
function nextPhase(date, target) {
    const at = t => { const p = moon(t, 0, 0).phase - target; return p - Math.round(p) }   // signed distance, -.5...5
    let t = new Date(date.valueOf() + 3600000), prev = at(t)
    for (let i = 0; i < 31 * 24; i++) {
        const n = new Date(t.valueOf() + 3600000), v = at(n)
        if (prev < 0 && v >= 0 && v - prev < 0.5) return new Date(t.valueOf() + 3600000 * (-prev) / (v - prev))
        t = n; prev = v
    }
    return null
}

// Moon-Earth distance, km
function moonDistance(date) { return moonCoords(toDays(date)).dist }
