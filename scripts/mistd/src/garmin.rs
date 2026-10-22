// Native Garmin Connect sync & workout recommendation engine for Mist.
// Replaces the heavy garmin-fetch-data and garmin-coach Python containers.
use crate::common::*;
use crate::garmin_db as db;
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value as J, json};
use std::fs;
use std::time::Duration;

type R<T> = Result<T, String>;

#[derive(Serialize, Deserialize, Debug)]
struct TokenFile {
    di_token: String,
    di_refresh_token: String,
    di_client_id: String,
}

// written by garmin-fetch-data's first interactive login (docs/guides/garmin.md)
fn tokens_path() -> String {
    let h = home();
    [format!("{h}/garmin-mist/garminconnect-tokens/garmin_tokens.json"), format!("{h}/garmin-grafana/garminconnect-tokens/garmin_tokens.json")]
        .into_iter()
        .find(|p| fs::metadata(p).is_ok())
        .unwrap_or_else(|| format!("{h}/.config/mist/garmin_tokens.json"))
}

fn load_tokens() -> R<(TokenFile, String)> {
    let path = tokens_path();
    let content = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    let tokens: TokenFile = serde_json::from_str(&content).map_err(|e| format!("invalid tokens json: {}", e))?;
    Ok((tokens, path))
}

fn jwt_exp(jwt: &str) -> Option<i64> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    use base64::Engine;
    let b64 = parts[1];
    let decoded = base64::prelude::BASE64_URL_SAFE_NO_PAD
        .decode(b64)
        .or_else(|_| {
            let padded = match b64.len() % 4 {
                2 => format!("{}==", b64),
                3 => format!("{}=", b64),
                _ => b64.to_string(),
            };
            base64::prelude::BASE64_STANDARD.decode(&padded)
        })
        .ok()?;
    let val: J = serde_json::from_slice(&decoded).ok()?;
    val["exp"].as_i64()
}

fn ensure_token(tokens: &mut TokenFile, path: &str) -> R<String> {
    let now = Utc::now().timestamp();
    let exp = jwt_exp(&tokens.di_token).unwrap_or(0);
    // If token has at least 5 minutes left, reuse
    if exp > now + 300 {
        return Ok(tokens.di_token.clone());
    }

    eprintln!("garmin: refreshing DI token...");
    use base64::Engine;
    let auth = format!(
        "Basic {}",
        base64::prelude::BASE64_STANDARD.encode(format!("{}:", tokens.di_client_id))
    );
    let form = format!(
        "grant_type=refresh_token&client_id={}&refresh_token={}",
        urlencoding(&tokens.di_client_id),
        urlencoding(&tokens.di_refresh_token)
    );

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();

    let mut res = agent
        .post("https://diauth.garmin.com/di-oauth2-service/oauth/token")
        .header("Authorization", &auth)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .header("User-Agent", "com.garmin.android.apps.connectmobile")
        .send(form.as_bytes())
        .map_err(|e| format!("refresh token request failed: {}", e))?;

    let json_res: J = res.body_mut().read_json().map_err(|e| format!("token json error: {}", e))?;
    let new_access = json_res["access_token"]
        .as_str()
        .ok_or("brak access_token w odpowiedzi odświeżania")?
        .to_string();

    tokens.di_token = new_access.clone();
    if let Some(r) = json_res["refresh_token"].as_str() {
        tokens.di_refresh_token = r.to_string();
    }

    if let Ok(new_content) = serde_json::to_string_pretty(&tokens) {
        let _ = fs::write(path, new_content);
    }

    Ok(new_access)
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn garmin_get(agent: &ureq::Agent, token: &str, url: &str) -> R<J> {
    let mut res = agent
        .get(url)
        .header("Authorization", &format!("Bearer {}", token))
        .header("User-Agent", "com.garmin.android.apps.connectmobile")
        .header("Accept", "application/json")
        .call()
        .map_err(|e| format!("GET {}: {}", url, e))?;
    res.body_mut().read_json::<J>().map_err(|e| format!("JSON {}: {}", url, e))
}

pub fn sync() -> R<J> {
    let (mut tokens, token_path) = load_tokens()?;
    let token = ensure_token(&mut tokens, &token_path)?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
        .into();

    let today = Local::now().format("%Y-%m-%d").to_string();

    // 1. Social Profile -> displayName
    let prof = garmin_get(&agent, &token, "https://connectapi.garmin.com/userprofile-service/socialProfile")?;
    let display_name = prof["displayName"]
        .as_str()
        .ok_or("brak displayName w profilu Garmin")?;

    // 2. Devices -> device name
    let dev_json = garmin_get(&agent, &token, "https://connectapi.garmin.com/device-service/deviceregistration/devices").unwrap_or(json!([]));
    let device_name = dev_json
        .as_array()
        .and_then(|a| a.first())
        .and_then(|d| d["productDisplayName"].as_str())
        .unwrap_or("Garmin Watch");

    let now_ts = Utc::now().timestamp();

    // 3. Training Readiness
    let tr_url = format!("https://connectapi.garmin.com/metrics-service/metrics/trainingreadiness/{}", today);
    let mut snap = db::Snapshot { device: Some((now_ts, device_name.to_string())), ..Default::default() };
    let mut tr_score: Option<i64> = None;
    let mut rec_hours: i64 = 0;
    if let Ok(tr_val) = garmin_get(&agent, &token, &tr_url) {
        if let Some(entry) = tr_val.as_array().and_then(|a| a.first()) {
            if let Some(score) = entry["score"].as_i64() {
                tr_score = Some(score);
                let tr_level = entry["level"].as_str().unwrap_or("MODERATE");
                rec_hours = entry["recoveryTime"].as_i64().unwrap_or(0);
                let ts = DateTime::parse_from_rfc3339(entry["timestamp"].as_str().unwrap_or(""))
                    .map(|dt| dt.timestamp())
                    .unwrap_or(now_ts);
                let n = |k: &str| entry[k].as_i64().unwrap_or(0);
                snap.readiness = Some(db::Readiness {
                    ts,
                    score,
                    level: tr_level.to_string(),
                    recovery_time: rec_hours,
                    acute_load: n("acuteLoad"),
                    acwr_factor: n("acwrFactorPercent"),
                    hrv_factor: n("hrvFactorPercent"),
                    sleep_score: n("sleepScore"),
                    sleep_factor: n("sleepScoreFactorPercent"),
                    stress_factor: n("stressHistoryFactorPercent"),
                    recovery_factor: n("recoveryTimeFactorPercent"),
                });
            }
        }
    }

    // 4. Daily Stress & Body Battery
    let stress_url = format!("https://connectapi.garmin.com/wellness-service/wellness/dailyStress/{}", today);
    let mut last_bb: Option<i64> = None;
    if let Ok(st_val) = garmin_get(&agent, &token, &stress_url) {
        if let Some(bbs) = st_val["bodyBatteryValuesArray"].as_array() {
            for item in bbs {
                if let Some(arr) = item.as_array() {
                    if arr.len() >= 3 {
                        if let (Some(ts_ms), Some(level)) = (arr[0].as_i64(), arr[2].as_i64()) {
                            last_bb = Some(level);
                            snap.body_battery.push((ts_ms / 1000, level));
                        }
                    }
                }
            }
        }
        if let Some(st_arr) = st_val["stressValuesArray"].as_array() {
            for item in st_arr {
                if let Some(arr) = item.as_array() {
                    if arr.len() >= 2 {
                        if let (Some(ts_ms), Some(level)) = (arr[0].as_i64(), arr[1].as_i64()) {
                            snap.stress.push((ts_ms / 1000, level));
                        }
                    }
                }
            }
        }
    }

    // 5. Daily Sleep & HRV
    let sleep_url = format!(
        "https://connectapi.garmin.com/wellness-service/wellness/dailySleepData/{}?date={}&nonSleepBufferMinutes=60",
        display_name, today
    );
    if let Ok(sl_val) = garmin_get(&agent, &token, &sleep_url) {
        let dto = &sl_val["dailySleepDTO"];
        if let Some(end_ms) = dto["sleepEndTimestampGMT"].as_i64() {
            let sleep_sec = dto["sleepTimeSeconds"].as_i64().unwrap_or(0);
            let score = dto["sleepScores"]["overall"]["value"].as_i64()
                .or_else(|| sl_val["sleepScore"].as_i64())
                .unwrap_or(0);
            let hrv = sl_val["avgOvernightHrv"].as_f64().unwrap_or(0.0);
            let rhr = sl_val["restingHeartRate"].as_i64().unwrap_or(0);
            let avg_stress = dto["avgSleepStress"].as_f64().unwrap_or(0.0);
            snap.sleep = Some(db::Sleep {
                ts: end_ms / 1000,
                seconds: sleep_sec,
                score,
                hrv,
                rhr,
                deep: dto["deepSleepSeconds"].as_i64().unwrap_or(0),
                light: dto["lightSleepSeconds"].as_i64().unwrap_or(0),
                rem: dto["remSleepSeconds"].as_i64().unwrap_or(0),
                awake: dto["awakeSleepSeconds"].as_i64().unwrap_or(0),
                avg_stress,
                restless: sl_val["restlessMomentsCount"].as_i64().unwrap_or(0),
            });
        }
    }

    // 6. Steps
    let steps_url = format!(
        "https://connectapi.garmin.com/wellness-service/wellness/dailySummaryChart/{}?date={}",
        display_name, today
    );
    if let Ok(steps_val) = garmin_get(&agent, &token, &steps_url) {
        if let Some(steps_arr) = steps_val.as_array() {
            for entry in steps_arr {
                if let (Some(start_str), Some(cnt)) = (entry["startGMT"].as_str(), entry["steps"].as_i64()) {
                    // "2026-10-22T05:00:00.0": UTC without a zone (one row per 15 min)
                    let Ok(t) = chrono::NaiveDateTime::parse_from_str(start_str, "%Y-%m-%dT%H:%M:%S%.f") else { continue };
                    let ts = t.and_utc().timestamp();
                    snap.steps.push((ts, cnt));
                }
            }
        }
    }

    // 7. Race Predictions
    let race_url = format!(
        "https://connectapi.garmin.com/metrics-service/metrics/racepredictions/latest/{}",
        display_name
    );
    if let Ok(rp_val) = garmin_get(&agent, &token, &race_url) {
        if let Some(t5) = rp_val["time5K"].as_i64() {
            snap.race = Some(db::Race {
                ts: now_ts,
                t5k: t5,
                t10k: rp_val["time10K"].as_i64().unwrap_or(0),
                half: rp_val["timeHalfMarathon"].as_i64().unwrap_or(0),
                marathon: rp_val["timeMarathon"].as_i64().unwrap_or(0),
            });
        }
    }

    // 8. Recent Activities (running)
    let acts_url = "https://connectapi.garmin.com/activitylist-service/activities/search/activities?limit=10";
    let mut acts_data: Vec<J> = Vec::new();
    let mut hr_zones = (104, 124, 145, 166, 186);

    if let Ok(acts_val) = garmin_get(&agent, &token, acts_url) {
        if let Some(acts) = acts_val.as_array() {
            for act in acts {
                acts_data.push(act.clone());
                let act_id = act["activityId"].as_i64().unwrap_or(0);
                let act_type = act["activityType"]["typeKey"].as_str().unwrap_or("other");
                let act_name = act["activityName"].as_str().unwrap_or("Aktywność");
                let dist = act["distance"].as_f64().unwrap_or(0.0);
                let dur = act["duration"].as_f64().unwrap_or(0.0);
                let moving = act["movingDuration"].as_f64().unwrap_or(dur);
                let speed = act["averageSpeed"].as_f64().unwrap_or(0.0);
                let hr = act["averageHR"].as_f64().unwrap_or(0.0);
                let load = act["activityTrainingLoad"].as_f64().unwrap_or(0.0);
                let ae = act["aerobicTrainingEffect"].as_f64().unwrap_or(0.0);
                let an = act["anaerobicTrainingEffect"].as_f64().unwrap_or(0.0);

                let start_gmt = act["startTimeGMT"].as_str().unwrap_or("");
                let act_ts = DateTime::parse_from_rfc3339(start_gmt)
                    .map(|dt| dt.timestamp())
                    .ok()
                    .or_else(|| {
                        chrono::NaiveDateTime::parse_from_str(start_gmt, "%Y-%m-%d %H:%M:%S")
                            .ok()
                            .map(|ndt| ndt.and_utc().timestamp())
                    })
                    .unwrap_or(now_ts);

                let mut z = (104, 124, 145, 166, 186);
                if act_type == "running" && act_id > 0 {
                    let z_url = format!("https://connectapi.garmin.com/activity-service/activity/{}/hrTimeInZones", act_id);
                    if let Ok(z_val) = garmin_get(&agent, &token, &z_url) {
                        if let Some(za) = z_val.as_array() {
                            let mut bounds = [104, 124, 145, 166, 186];
                            for item in za {
                                if let (Some(num), Some(b)) = (item["zoneNumber"].as_i64(), item["zoneLowBoundary"].as_i64()) {
                                    if num >= 1 && num <= 5 {
                                        bounds[(num - 1) as usize] = b;
                                    }
                                }
                            }
                            z = (bounds[0], bounds[1], bounds[2], bounds[3], bounds[4]);
                            hr_zones = z;
                        }
                    }
                }

                snap.activities.push(db::Activity {
                    id: act_id.to_string(),
                    ts: act_ts,
                    kind: act_type.to_string(),
                    name: act_name.to_string(),
                    dist,
                    moving,
                    speed,
                    hr,
                    load,
                    ae,
                    an,
                    zones: z,
                });
            }
        }
    }

    // 9. Coach suggestion (computed in Rust directly)
    let plan = compute_coach(tr_score, rec_hours, last_bb, &acts_data, hr_zones);
    snap.workout = Some(db::Workout {
        ts: now_ts,
        title: plan.title.clone(),
        detail: plan.detail.clone(),
        reason: plan.reason.clone(),
        kind: plan.kind.clone(),
        readiness: plan.readiness.unwrap_or(0) as f64,
        race_days: plan.race_days,
    });
    let rows = db::save(&snap).map_err(|e| format!("garmin.db: {e}"))?;

    Ok(json!({
        "status": "ok",
        "rows": rows,
        "device": device_name,
        "coach": {
            "title": plan.title,
            "detail": plan.detail,
            "readiness": plan.readiness,
        }
    }))
}

struct CoachPlan {
    kind: String,
    title: String,
    detail: String,
    reason: String,
    readiness: Option<i64>,
    race_days: i64,
}

fn compute_coach(
    score: Option<i64>,
    rec_h: i64,
    body: Option<i64>,
    acts: &[J],
    z: (i64, i64, i64, i64, i64),
) -> CoachPlan {
    let now = Utc::now().timestamp();
    let hr = [
        format!("{}–{} bpm", z.0, z.1 - 1),
        format!("{}–{} bpm", z.1, z.2 - 1),
        format!("{}–{} bpm", z.2, z.3 - 1),
        format!("{}–{} bpm", z.3, z.4 - 1),
        format!("{}–{} bpm", z.4, z.4 + 10),
    ];

    let mut low = 0.0;
    let mut high = 0.0;
    let mut anaer = 0.0;
    let mut last_hard_ts: Option<i64> = None;
    let mut last_run_ts: Option<i64> = None;

    for a in acts {
        let load = a["activityTrainingLoad"].as_f64().unwrap_or(0.0);
        let ae = a["aerobicTrainingEffect"].as_f64().unwrap_or(0.0);
        let an = a["anaerobicTrainingEffect"].as_f64().unwrap_or(0.0);
        let is_running = a["activityType"]["typeKey"].as_str().unwrap_or("") == "running";

        let start_gmt = a["startTimeGMT"].as_str().unwrap_or("");
        let ts = chrono::NaiveDateTime::parse_from_str(start_gmt, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|ndt| ndt.and_utc().timestamp());

        if an >= 2.0 {
            anaer += load;
        } else if ae >= 3.5 {
            high += load;
        } else {
            low += load;
        }

        if an >= 2.0 || ae >= 4.0 {
            if last_hard_ts.is_none() || ts > last_hard_ts {
                last_hard_ts = ts;
            }
        }
        if is_running {
            if last_run_ts.is_none() || ts > last_run_ts {
                last_run_ts = ts;
            }
        }
    }

    let total = low + high + anaer;
    let days_hard = last_hard_ts.map(|t| (now - t) as f64 / 86400.0).unwrap_or(99.0);
    let days_run = last_run_ts.map(|t| (now - t) as f64 / 86400.0).unwrap_or(99.0);

    // Optional race date from config or fallback
    let coach_cfg_path = format!("{}/.config/mist/coach.json", home());
    let coach_cfg: J = fs::read_to_string(&coach_cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!({}));
    let race_str = coach_cfg["race_date"].as_str().unwrap_or("");
    let race_name = coach_cfg["race_name"].as_str().unwrap_or("zawody");
    let race_days: Option<i64> = NaiveDate::parse_from_str(race_str, "%Y-%m-%d")
        .ok()
        .map(|d| (d - Local::now().date_naive()).num_days());

    let out = |kind: &str, title: &str, detail: &str, reason: &str| CoachPlan {
        kind: kind.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        reason: reason.to_string(),
        readiness: score,
        race_days: race_days.unwrap_or(-1),
    };

    let tired = score.map(|s| s < 25).unwrap_or(false) || body.map(|b| b < 20).unwrap_or(false) || rec_h >= 36;
    if race_days == Some(0) {
        return out("race", &format!("Dzień startu – {}!", race_name), "Rozgrzewka 15 min + 3 przebieżki, potem na start", "Powodzenia!");
    }
    if tired {
        return out("rest", "Odpoczynek", "Dzień wolny albo 20–30 min spaceru", &format!("Gotowość {:?}, Body Battery {:?}, regeneracja {} h", score, body, rec_h));
    }
    if let Some(rd) = race_days {
        if rd >= 1 && rd <= 2 {
            return out("recovery", "Rozruch przedstartowy", &format!("15–20 min truchtu ({}) + 4×20 s przebieżki", hr[1]), &format!("{} za {} {} – świeże nogi", race_name, rd, if rd == 1 { "dzień" } else { "dni" }));
        }
    }
    if score.map(|s| s < 50).unwrap_or(false) || days_hard < 2.0 {
        let why = if days_hard >= 2.0 { format!("Gotowość {:?}", score) } else { "Wczoraj/przedwczoraj był mocny trening".to_string() };
        return out("recovery", "Bieg regeneracyjny", &format!("30–40 min spokojnie, tętno {}", hr[0]), &why);
    }
    if let Some(rd) = race_days {
        if rd >= 3 && rd <= 7 {
            return out("tempo", "Akcent pod start", &format!("Rozgrzewka 10 min, 3×1 km w tempie startowym ({}), przerwy 2 min truchtem, 10 min schłodzenia", hr[3]), &format!("{} za {} dni – krótko i szybko", race_name, rd));
        }
    }
    let low_share = if total > 0.0 { low / total } else { 0.0 };
    let anaer_share = if total > 0.0 { anaer / total } else { 0.0 };
    if total > 0.0 && low_share < 0.3 && days_run < 10.0 {
        return out("base", "Bieg bazowy", &format!("45–60 min w strefie 2 ({})", hr[1]), &format!("Za mało spokojnej pracy tlenowej ({:.0}% obciążenia z 4 tyg.)", low_share * 100.0));
    }
    if score.map(|s| s >= 70).unwrap_or(false) && days_hard >= 3.0 {
        if let Some(rd) = race_days {
            if rd <= 21 {
                return out("intervals", "Interwały pod 5 km", &format!("Rozgrzewka 15 min, 5×800 m ({}), przerwy 400 m truchtem, 10 min schłodzenia", hr[4]), &format!("Gotowość {:?}, {} za {} dni", score, race_name, rd));
            }
        }
        if anaer_share > 0.35 {
            return out("tempo", "Bieg progowy", &format!("Rozgrzewka 15 min, 3×8 min ({}), przerwy 2 min, 10 min schłodzenia", hr[3]), &format!("Gotowość {:?} – dobry dzień na akcent", score));
        }
        return out("intervals", "Interwały VO2max", &format!("Rozgrzewka 15 min, 5×3 min ({}), przerwy 2 min, 10 min schłodzenia", hr[4]), &format!("Gotowość {:?} – dobry dzień na akcent", score));
    }
    out("base", "Bieg bazowy", &format!("40–50 min w strefie 2 ({})", hr[1]), &format!("Gotowość {:?}", score))
}

pub fn main(args: &[String]) {
    let mode = args.first().map(|s| s.as_str()).unwrap_or("sync");
    match mode {
        "sync" => {
            let tp = tokens_path();
            if !std::path::Path::new(&tp).exists() {
                out(json!({"status": "disabled", "reason": "brak konfiguracji Garmin (karty Garmin są opcjonalne)"}));
                return;
            }
            match sync() {
                Ok(v) => out(v),
                Err(e) => {
                    eprintln!("garmin sync: {}", e);
                    out(json!({"status": "error", "error": e}));
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("usage: mistd garmin sync");
            std::process::exit(2);
        }
    }
}
