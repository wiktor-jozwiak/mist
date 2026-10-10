// Low-precision ephemeris (components/Astro.js 1:1, after SunCalc / Meeus: sun ~0.01°, moon ~0.5°).
// Times are unix milliseconds (f64), like JS Date values.
#![allow(dead_code)]
use std::f64::consts::PI;

const RAD: f64 = PI / 180.0;
const E: f64 = RAD * 23.4397;

fn to_days(ms: f64) -> f64 { ms / 86400000.0 - 0.5 + 2440588.0 - 2451545.0 }
fn ra(l: f64, b: f64) -> f64 { (l.sin() * E.cos() - b.tan() * E.sin()).atan2(l.cos()) }
fn dec(l: f64, b: f64) -> f64 { (b.sin() * E.cos() + b.cos() * E.sin() * l.sin()).asin() }
fn azim(h: f64, phi: f64, d: f64) -> f64 { h.sin().atan2(h.cos() * phi.sin() - d.tan() * phi.cos()) }
fn alti(h: f64, phi: f64, d: f64) -> f64 { (phi.sin() * d.sin() + phi.cos() * d.cos() * h.cos()).asin() }
fn sidereal(d: f64, lw: f64) -> f64 { RAD * (280.16 + 360.9856235 * d) - lw }
fn refraction(h: f64) -> f64 { let h = h.max(0.0); 0.0002967 / (h + 0.00312536 / (h + 0.08901179)).tan() }
fn sun_coords(d: f64) -> (f64, f64) {
    let m = RAD * (357.5291 + 0.98560028 * d);
    let c = RAD * (1.9148 * m.sin() + 0.02 * (2.0 * m).sin() + 0.0003 * (3.0 * m).sin());
    let l = m + c + RAD * 102.9372 + PI;
    (dec(l, 0.0), ra(l, 0.0))
}
fn moon_coords(d: f64) -> (f64, f64, f64) {
    let l0 = RAD * (218.316 + 13.176396 * d);
    let m = RAD * (134.963 + 13.064993 * d);
    let f = RAD * (93.272 + 13.229350 * d);
    let l = l0 + RAD * 6.289 * m.sin();
    let b = RAD * 5.128 * f.sin();
    (ra(l, b), dec(l, b), 385001.0 - 20905.0 * m.cos())
}
fn compass(a: f64) -> f64 { ((a / RAD + 180.0) % 360.0 + 360.0) % 360.0 }

#[derive(Clone, Copy, Debug, Default)]
pub struct Body { pub alt: f64, pub az: f64, pub fraction: f64, pub phase: f64, pub inc: f64, pub limb_zenith: f64, pub pa: f64 }

pub fn sun(ms: f64, lat: f64, lon: f64) -> Body {
    let (lw, phi, d) = (-lon * RAD, lat * RAD, to_days(ms));
    let (cd, cr) = sun_coords(d);
    let h = sidereal(d, lw) - cr;
    Body { alt: alti(h, phi, cd) / RAD, az: compass(azim(h, phi, cd)), ..Default::default() }
}

pub fn moon(ms: f64, lat: f64, lon: f64) -> Body {
    let (lw, phi, d) = (-lon * RAD, lat * RAD, to_days(ms));
    let (cra, cdec, dist) = moon_coords(d);
    let hh = sidereal(d, lw) - cra;
    let mut h = alti(hh, phi, cdec);
    h += refraction(h);
    let pa = hh.sin().atan2(phi.tan() * cdec.cos() - cdec.sin() * hh.cos());
    let (sdec, sra) = sun_coords(d);
    let sd = 149598000.0;
    let psi = (sdec.sin() * cdec.sin() + sdec.cos() * cdec.cos() * (sra - cra).cos()).acos();
    let inc = (sd * psi.sin()).atan2(dist - sd * psi.cos());
    let limb = (sdec.cos() * (sra - cra).sin()).atan2(sdec.sin() * cdec.cos() - sdec.cos() * cdec.sin() * (sra - cra).cos());
    Body {
        alt: h / RAD,
        az: compass(azim(hh, phi, cdec)),
        fraction: (1.0 + inc.cos()) / 2.0,
        phase: 0.5 + 0.5 * inc * if limb < 0.0 { -1.0 } else { 1.0 } / PI,
        inc,
        limb_zenith: limb - pa,
        pa,
    }
}

pub fn moon_distance(ms: f64) -> f64 { moon_coords(to_days(ms)).2 }

// meteor rate (ZHR) now: showers + sporadic background
pub fn meteor_zhr(ms: f64, year: i32) -> f64 {
    const SH: [(u32, u32, f64, f64); 10] = [(1, 3, 110.0, 0.6), (4, 22, 18.0, 1.5), (5, 6, 12.0, 3.0), (7, 30, 20.0, 3.0), (8, 12, 100.0, 2.5), (10, 8, 10.0, 0.6), (10, 21, 20.0, 3.0), (11, 17, 15.0, 1.5), (12, 14, 150.0, 1.2), (12, 22, 10.0, 0.7)];
    let mut z = 6.0;
    for (m, dday, zhr, w) in SH {
        let peak = local_ms(year, m, dday, 12);
        let mut dd = (ms - peak) / 86400000.0;
        if dd > 182.0 { dd -= 365.0 } else if dd < -182.0 { dd += 365.0 }
        z += zhr * (-(dd * dd) / (2.0 * w * w)).exp();
    }
    z
}

// local time -> unix ms (mktime)
pub fn local_ms(year: i32, month: u32, day: u32, hour: u32) -> f64 {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = year - 1900;
        tm.tm_mon = month as i32 - 1;
        tm.tm_mday = day as i32;
        tm.tm_hour = hour as i32;
        tm.tm_isdst = -1;
        libc::mktime(&mut tm) as f64 * 1000.0
    }
}

// moonrise / moonset on the local day of `ms` (10-minute scan + linear refinement)
pub fn moon_times(ms: f64, lat: f64, lon: f64) -> (Option<f64>, Option<f64>) {
    let t = unsafe { let mut tm: libc::tm = std::mem::zeroed(); let s = (ms / 1000.0) as libc::time_t; libc::localtime_r(&s, &mut tm); tm };
    let d0 = local_ms(t.tm_year + 1900, (t.tm_mon + 1) as u32, t.tm_mday as u32, 0);
    let h0 = 0.133;
    let (mut rise, mut set) = (None, None);
    let mut prev = moon(d0, lat, lon).alt - h0;
    for i in 1..=144 {
        let tt = d0 + i as f64 * 600000.0;
        let a = moon(tt, lat, lon).alt - h0;
        if (prev < 0.0) != (a < 0.0) {
            let x = tt - 600000.0 * a / (a - prev);
            if prev < 0.0 { if rise.is_none() { rise = Some(x) } } else if set.is_none() { set = Some(x) }
        }
        prev = a;
    }
    (rise, set)
}

// the next moment the phase reaches `target` (0 new, .25 first quarter, .5 full, .75 last quarter)
pub fn next_phase(ms: f64, target: f64) -> Option<f64> {
    let at = |t: f64| { let p = moon(t, 0.0, 0.0).phase - target; p - p.round() };
    let mut t = ms + 3600000.0;
    let mut prev = at(t);
    for _ in 0..31 * 24 {
        let n = t + 3600000.0;
        let v = at(n);
        if prev < 0.0 && v >= 0.0 && v - prev < 0.5 {
            return Some(t + 3600000.0 * (-prev) / (v - prev));
        }
        t = n;
        prev = v;
    }
    None
}
