// The weather sky of the desktop weather card (components/Sky.qml 1:1): what the reading means for the
// scene (rain / snow / fog intensities, wind slant, life), where the Sun and the Moon stand on the card
// (camera facing what is worth seeing) and the lightning schedule. Shared: mist-wall draws the sky with it,
// mist-ui places the hoverable sun / moon spots and shakes the card on a strike.
#![allow(dead_code)]
use crate::astro;
use serde_json::Value;

pub const HOR: f64 = 0.6; // horizon line of the weather card
pub const FOV: f64 = 1.8;

#[derive(Clone, Copy, Default, Debug)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Default, Debug)]
pub struct Model {
    pub code: i64,
    pub wind: f64,
    pub gust: f64,
    pub temp: f64,
    pub hum: f64,
    pub cover_pct: f64,
    pub rate: f64,
    pub rain_k: f64,
    pub snow_k: f64,
    pub fog_k: f64,
    pub haze_k: f64,
    pub storm_k: f64,
    pub hail_k: f64,
    pub wind_k: f64,
    pub slant: f64,
    pub cloud_speed: f64,
    pub sun: astro::Body,
    pub moon: astro::Body,
    pub yaw: f64,
    pub pitch: f64,
    pub moon_light: f64,
    pub aurora_k: f64,
    pub meteor_k: f64,
    pub bow_k: f64,
    pub leaf_k: f64,
    pub bird_k: f64,
    pub ray_k: f64,
    pub frost_k: f64,
    pub mist_k: f64,
    pub heat_k: f64,
    pub gsnow_k: f64,
    pub curtain_k: f64,
    pub glass_snow_k: f64,
    pub halo_k: f64,
    pub firefly_k: f64,
    pub mote_k: f64,
}

fn f(w: &Value, k: &str, d: f64) -> f64 {
    w[k].as_f64().unwrap_or(d)
}

// local month (0..11) and year of a unix ms
fn local(ms: f64) -> (i32, i32) {
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let s = (ms / 1000.0) as libc::time_t;
        libc::localtime_r(&s, &mut tm);
        (tm.tm_mon, tm.tm_year + 1900)
    }
}

impl Model {
    // `now`: unix ms of the minute (Data.now)
    pub fn new(wx: &Value, now: f64) -> Model {
        let code = wx["code"].as_i64().unwrap_or(3);
        let (wind, gust, temp, hum) = (f(wx, "wind", 0.0), f(wx, "gust", 0.0), f(wx, "temp", 10.0), f(wx, "hum", 70.0));
        let precip = f(wx, "precip", 0.0);
        let snow_now = f(wx, "snow", 0.0);
        let cover = f(wx, "cloud", -1.0);
        let vis = f(wx, "vis", 50000.0);
        let wind_dir = f(wx, "windDir", 270.0);
        let kp = f(wx, "kp", 0.0);
        let snow_depth = wx["snowDepth"].as_f64().unwrap_or(0.0);
        let (lat, lon) = (f(wx, "lat", 52.2297), f(wx, "lon", 21.0122));

        let code_snow = (71..=77).contains(&code) || code == 85 || code == 86;
        let code_wet = (51..=67).contains(&code) || (80..=82).contains(&code) || code >= 95;
        let code_fog = code == 45 || code == 48;
        let drizzle = (51..=57).contains(&code);
        let heavy = code == 65 || code == 67 || code == 82 || code >= 95;
        let freezing = matches!(code, 56 | 57 | 66 | 67);
        let rime = code == 48;
        let sleet = (precip > 0.0 || code_wet || code_snow) && temp > -1.0 && temp < 2.5 && !freezing && code < 95;
        let snowing = snow_now > 0.0 || (code_snow && precip <= 0.0) || sleet;
        let raining = (!snowing || sleet) && (precip > 0.0 || code_wet || sleet);
        let rate = if precip > 0.0 { precip * 4.0 } else if raining { if drizzle { 0.3 } else if heavy { 6.0 } else { 1.2 } } else { 0.0 };
        let cover_pct = if cover >= 0.0 { cover } else if code <= 1 { 5.0 } else if code == 2 { 40.0 } else { 90.0 };
        let rain_k = if raining { (0.3 + 0.28 * (1.0 + rate).log2()).min(1.0) * if sleet { 0.6 } else { 1.0 } } else { 0.0 };
        let snow_k = if snowing { (if sleet { 0.45 } else { 0.0f64 }).max(((if code == 77 { 0.25 } else { 0.35 }) + 1.2 * snow_now).min(1.0)) } else { 0.0 };
        let fog_k = (if code_fog { 0.75 } else { 0.0f64 }).max(((1200.0 - vis) / 900.0).clamp(0.0, 1.0));
        let haze_k = if fog_k > 0.0 { 0.0 } else { ((5000.0 - vis) / 4000.0).clamp(0.0, 1.0) };
        let storm_k: f64 = if code >= 95 { 1.0 } else { 0.0 };
        let hail_k: f64 = if code == 96 || code == 99 { 1.0 } else { 0.0 };
        let wind_k = if gust >= 35.0 || wind >= 25.0 { ((gust.max(wind) - 25.0) / 35.0).clamp(0.25, 1.0) } else { 0.0 };
        let to_rad = (wind_dir + 180.0).to_radians();
        let vx = -to_rad.sin();
        let hwind = wind * vx.abs().max(0.2);
        let sgn = if vx >= 0.0 { 1.0 } else { -1.0 };
        let slant = (sgn * hwind * 0.9).clamp(-32.0, 32.0).to_radians().tan();
        let cloud_speed = sgn * (0.035 + wind * 0.0045);

        let sun = astro::sun(now, lat, lon);
        let moon = astro::moon(now, lat, lon);
        let bow_k = if rain_k > 0.0 && sun.alt > 0.0 && sun.alt < 42.0 && cover_pct < 85.0 { (rain_k * 2.0).min(1.0) * (1.0 - cover_pct / 100.0) * ((42.0 - sun.alt) / 8.0).min(1.0) } else { 0.0 };
        let yaw = if bow_k > 0.0 { sun.az + 180.0 } else if sun.alt > -4.0 { sun.az } else if moon.alt > 0.0 { moon.az } else { 180.0 };
        let focus = if bow_k > 0.0 { 0.0 } else if sun.alt > -4.0 { sun.alt } else if moon.alt > 0.0 { moon.alt } else { 0.0 };
        let pitch = (focus - 36.0).max(0.0).to_radians();
        let moon_light = moon.fraction * ((moon.alt + 1.0) / 9.0).clamp(0.0, 1.0) * if sun.alt < -5.0 { 1.0 } else { 0.0 };
        let aurora_k = if kp >= 5.5 && sun.alt < -10.0 { ((kp - 5.5) / 2.5).min(1.0) * (1.0 - cover_pct / 100.0) } else { 0.0 };
        let (month, year) = local(now);
        let meteor_k = if sun.alt < -10.0 && cover_pct < 60.0 { (0.015 + astro::meteor_zhr(now, year) * 0.0035).min(0.55) } else { 0.0 };
        let autumn = (8..=10).contains(&month);
        let wl = wind.max(gust * 0.7);
        let leaf_k = if autumn && !snowing && wl >= 12.0 { ((wl - 8.0) / 30.0).min(1.0) * if raining { 0.5 } else { 1.0 } } else { 0.0 };
        let bird_k = if sun.alt > 2.0 && rain_k < 0.5 && snow_k <= 0.0 && storm_k <= 0.0 && fog_k < 0.5 { if wind > 35.0 { 0.25 } else { 0.6 } } else { 0.0 };
        let ray_k = if sun.alt > -1.0 && cover_pct > 15.0 && cover_pct < 97.0 && fog_k < 0.5 { ((cover_pct - 15.0) / 25.0).min(1.0) * ((97.0 - cover_pct) / 12.0).min(1.0) } else { 0.0 };
        let frost_k = if freezing || rime { 0.85 } else if temp <= 0.5 { ((1.0 - temp) / 7.0).min(1.0) } else { 0.0 };
        let mist_k = if hum >= 92.0 && temp <= 14.0 && !raining { ((hum - 91.0) / 6.0).min(1.0) * 0.8 } else { 0.0 };
        let heat_k = if temp >= 25.0 && sun.alt > 12.0 && cover_pct < 60.0 && !raining { ((temp - 24.0) / 8.0).min(1.0) } else { 0.0 };
        let gsnow_k = if snow_depth > 0.003 { (0.45 + snow_depth * 12.0).min(1.0) } else if snowing && temp <= 1.0 { 0.35 } else { 0.0 };
        let curtain_k = if rain_k > 0.55 || storm_k > 0.0 { (storm_k * 0.8).max((rain_k - 0.5) * 2.2).min(1.0) } else { 0.0 };
        let glass_snow_k = if snow_k > 0.0 && temp <= 1.5 { (snow_k * 1.4).min(1.0) } else { 0.0 };
        let halo_k = if !raining && !snowing && fog_k < 0.3 && (30.0..=85.0).contains(&cover_pct) && ((sun.alt > 4.0 && sun.alt < 50.0) || (sun.alt < -6.0 && moon.alt > 5.0 && moon.fraction > 0.5)) { 0.8 } else { 0.0 };
        let firefly_k = if (5..=7).contains(&month) && sun.alt < -4.0 && temp >= 14.0 && !raining && wind < 18.0 { 1.0 } else { 0.0 };
        let mote_k = if (2..=8).contains(&month) && sun.alt > 8.0 && temp >= 12.0 && !raining && !snowing && cover_pct < 70.0 && wind < 25.0 { 0.8 } else { 0.0 };
        Model {
            code, wind, gust, temp, hum, cover_pct, rate, rain_k, snow_k, fog_k, haze_k, storm_k, hail_k, wind_k, slant, cloud_speed, sun, moon, yaw, pitch, moon_light, aurora_k, meteor_k, bow_k,
            leaf_k, bird_k, ray_k, frost_k, mist_k, heat_k, gsnow_k, curtain_k, glass_snow_k, halo_k, firefly_k, mote_k,
        }
    }

    // camera frame: x right, y up, z forward
    pub fn dir(&self, az: f64, alt: f64) -> V3 {
        let a = (az - self.yaw).to_radians();
        let e = alt.to_radians();
        V3 { x: a.sin() * e.cos(), y: e.sin(), z: a.cos() * e.cos() }
    }
    // position on the card 0..1 (far outside when behind the camera)
    pub fn proj(&self, az: f64, alt: f64, w: f64, h: f64) -> (f64, f64) {
        let v = self.dir(az, alt);
        let asp = w.max(1.0) / h.max(1.0);
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        let d = V3 { x: v.x, y: v.y * cp - v.z * sp, z: v.y * sp + v.z * cp };
        if d.z < 0.05 {
            return (if d.x > 0.0 { 3.0 } else { -2.0 }, HOR);
        }
        (0.5 + d.x / d.z / (asp * FOV), HOR - d.y / d.z / FOV)
    }
    pub fn sun_s(&self, w: f64, h: f64) -> (f64, f64) {
        self.proj(self.sun.az, self.sun.alt, w, h)
    }
    pub fn moon_s(&self, w: f64, h: f64) -> (f64, f64) {
        self.proj(self.moon.az, self.moon.alt, w, h)
    }
}

fn h1(a: f64, b: f64) -> f64 {
    let x = (a * 127.1 + b * 311.7).sin() * 43758.5453;
    x - x.floor()
}

// lightning at sky time t (s, wraps at 3600): (flash, x, local t, slot); a strike may fall in each 3.2 s slot
pub fn bolt(t: f64, storm_k: f64) -> [f64; 4] {
    if storm_k <= 0.0 {
        return [0.0, 0.0, 9.0, 0.0];
    }
    let slot = (t / 3.2).floor();
    if h1(slot, 9.1) >= 0.55 * storm_k {
        return [0.0, 0.0, 9.0, slot];
    }
    let lt = t - slot * 3.2 - h1(slot, 8.3) * 2.2;
    let fl = if lt <= 0.0 { 0.0 } else { (-lt * 7.0).exp() + if lt >= 0.18 { 0.7 * (-(lt - 0.18) * 9.0).exp() } else { 0.0 } };
    [if fl < 0.002 { 0.0 } else { fl }, 0.15 + 0.7 * h1(slot, 7.7), lt, slot]
}

// the sky's clock: wall time, so mist-ui knows when a strike falls without asking mist-wall
pub fn sky_time() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0) % 3600.0
}
