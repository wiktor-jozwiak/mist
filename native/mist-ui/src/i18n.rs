// UI language. Strings are written in Polish in the source (like gettext msgids); Text::shape translates what
// it draws, so every surface follows without touching its code. Table: native/assets/i18n.tsv, one row per
// source string, one column per language; `{}` / `{name}` in a key matches any text (numbers, names) and is
// put back in order. Lookup, on a shape-cache miss only: whole string, template, " · " segments, dates.
// Language: ~/.config/mist/settings.json {"lang": "auto"|"pl"|"en"|"de"|"es"}, MIST_LANG overrides; auto
// follows $LANG and falls back to English.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};

pub const LANGS: [(&str, &str); 4] = [("pl", "Polski"), ("en", "English"), ("de", "Deutsch"), ("es", "Español")];
const TSV: &str = include_str!("../../assets/i18n.tsv");

struct Table {
    exact: HashMap<&'static str, &'static str>,
    // (literal parts between the wildcards, translation template); longest keys first
    tpl: Vec<(Vec<&'static str>, &'static str)>,
}

static CUR: RwLock<Option<(&'static str, &'static Table)>> = RwLock::new(None);
static GEN: AtomicU32 = AtomicU32::new(0);

fn settings_path() -> String {
    format!("{}/.config/mist/settings.json", std::env::var("HOME").unwrap_or_default())
}

// the setting as saved ("auto" when unset)
pub fn setting() -> String {
    std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v["lang"].as_str().map(String::from)).unwrap_or_else(|| "auto".into())
}

fn resolve(want: &str) -> &'static str {
    let want = if want == "auto" || want.is_empty() {
        std::env::var("LANG").unwrap_or_default().chars().take(2).collect::<String>()
    } else {
        want.to_string()
    };
    LANGS.iter().find(|l| l.0 == want).map(|l| l.0).unwrap_or("en")
}

fn wild(s: &str) -> Vec<&str> {
    // "{} min temu" -> ["", " min temu"]; "{d} d {h} godz." -> ["", " d ", " godz."]
    let mut parts = Vec::new();
    let mut rest = s;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        parts.push(&rest[..a]);
        rest = &rest[a + b + 1..];
    }
    parts.push(rest);
    parts
}

fn load(lang: &'static str) -> &'static Table {
    static TABLES: OnceLock<RwLock<HashMap<&'static str, &'static Table>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(t) = tables.read().unwrap().get(lang) {
        return t;
    }
    let mut lines = TSV.lines();
    let head: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let col = head.iter().position(|h| *h == lang).unwrap_or(0);
    let (mut exact, mut tpl) = (HashMap::new(), Vec::new());
    for l in lines {
        let c: Vec<&'static str> = l.split('\t').collect();
        let (Some(k), Some(v)) = (c.first(), c.get(col)) else { continue };
        if k.is_empty() || k.starts_with('#') || v.is_empty() || col == 0 {
            continue;
        }
        if k.contains('{') {
            tpl.push((wild(k), *v));
        } else {
            exact.insert(*k, *v);
        }
    }
    tpl.sort_by_key(|t| std::cmp::Reverse(t.0.iter().map(|p| p.len()).sum::<usize>()));
    let t: &'static Table = Box::leak(Box::new(Table { exact, tpl }));
    tables.write().unwrap().insert(lang, t);
    t
}

// the UI language's code ("pl", "en", ...)
pub fn lang() -> &'static str {
    current().0
}

fn current() -> (&'static str, &'static Table) {
    if let Some(c) = *CUR.read().unwrap() {
        return c;
    }
    let lang = resolve(&std::env::var("MIST_LANG").unwrap_or_else(|_| setting()));
    let c = (lang, load(lang));
    *CUR.write().unwrap() = Some(c);
    c
}

// bumped on a language change: Text drops its shaped cache
pub fn generation() -> u32 {
    GEN.load(Ordering::Relaxed)
}

// save and apply ("auto" or a code from LANGS)
pub fn set(code: &str) {
    let path = settings_path();
    let mut v: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::json!({}));
    v["lang"] = serde_json::json!(code);
    let tmp = format!("{path}.tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
    let lang = resolve(code);
    *CUR.write().unwrap() = Some((lang, load(lang)));
    GEN.fetch_add(1, Ordering::Relaxed);
}

// the captured texts when `s` fits the key's literal parts (the first part a prefix, the last a suffix)
fn capture<'a>(parts: &[&str], s: &'a str) -> Option<Vec<&'a str>> {
    let rest = s.strip_prefix(parts[0])?;
    let mut caps = Vec::new();
    let mut rest = rest;
    for (i, p) in parts.iter().enumerate().skip(1) {
        let last = i == parts.len() - 1;
        let at = if last {
            if !rest.ends_with(p) || rest.len() < p.len() { return None; }
            rest.len() - p.len()
        } else if p.is_empty() {
            return None; // two wildcards in a row: ambiguous
        } else {
            rest.find(p)?
        };
        if at == 0 {
            return None; // a wildcard matches at least one character
        }
        caps.push(&rest[..at]);
        rest = &rest[at + p.len()..];
    }
    if !parts.iter().any(|p| p.contains(" · ")) && caps.iter().any(|c| c.contains(" · ")) {
        return None;
    }
    Some(caps)
}

fn fill(tpl: &str, caps: &[&str]) -> String {
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl;
    let mut i = 0;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        out.push_str(&rest[..a]);
        out.push_str(caps.get(i).copied().unwrap_or(""));
        i += 1;
        rest = &rest[a + b + 1..];
    }
    out.push_str(rest);
    out
}

fn one(t: &Table, s: &str) -> Option<String> {
    one_depth(t, s, 0)
}

fn one_depth(t: &Table, s: &str, depth: usize) -> Option<String> {
    if let Some(v) = t.exact.get(s) {
        return Some(v.to_string());
    }
    // headings drawn in capitals ("GOTOWOŚĆ", "TYDZIEŃ 42"): the key is the capitalised or lowercase form
    if s.chars().any(|c| c.is_lowercase()) == false && s.chars().any(|c| c.is_uppercase()) {
        let low = s.to_lowercase();
        let mut cs = low.chars();
        let cap: String = cs.next().map(|f| f.to_uppercase().chain(cs).collect()).unwrap_or_default();
        if let Some(v) = t.exact.get(cap.as_str()).or_else(|| t.exact.get(low.as_str())) {
            return Some(v.to_uppercase());
        }
        for (parts, v) in &t.tpl {
            if parts.iter().all(|p| p.is_empty()) {
                continue;
            }
            if let Some(caps) = capture(parts, &cap).or_else(|| capture(parts, &low)) {
                let caps: Vec<String> = if depth < 2 {
                    caps.iter().map(|c| one_depth(t, c, depth + 1).unwrap_or_else(|| c.to_string())).collect()
                } else {
                    caps.iter().map(|c| t.exact.get(c).map(|x| x.to_string()).unwrap_or_else(|| c.to_string())).collect()
                };
                return Some(fill(v, &caps.iter().map(|c| c.as_str()).collect::<Vec<_>>()).to_uppercase());
            }
        }
    }
    // lowercase words whose table key is capitalised ("pochmurno" -> "Pochmurno" -> "cloudy")
    let mut cs = s.chars();
    if let Some(first) = cs.next().filter(|c| c.is_lowercase()) {
        let cap: String = first.to_uppercase().chain(cs).collect();
        if let Some(v) = t.exact.get(cap.as_str()) {
            let mut v_cs = v.chars();
            let v_low: String = v_cs.next().map(|f| f.to_lowercase().chain(v_cs).collect()).unwrap_or_default();
            return Some(v_low);
        }
    }
    for (parts, v) in &t.tpl {
        if parts.iter().all(|p| p.is_empty()) {
            continue;
        }
        if let Some(caps) = capture(parts, s) {
            // captured texts may be words of their own ("{} dni temu" with "pon")
            let caps: Vec<String> = if depth < 2 {
                caps.iter().map(|c| one_depth(t, c, depth + 1).unwrap_or_else(|| c.to_string())).collect()
            } else {
                caps.iter().map(|c| t.exact.get(c).map(|x| x.to_string()).unwrap_or_else(|| c.to_string())).collect()
            };
            return Some(fill(v, &caps.iter().map(|c| c.as_str()).collect::<Vec<_>>()));
        }
    }
    None
}

// "Piątek, 16 października" and the like: day / month words, numbers and punctuation only
fn date(t: &Table, lang: &str, s: &str) -> Option<String> {
    if !s.chars().any(|c| c.is_ascii_digit()) || s.len() > 48 {
        return None;
    }
    let mut out = String::new();
    let mut hit = false;
    for tok in s.split_inclusive(|c: char| !c.is_alphanumeric()) {
        let w = tok.trim_end_matches(|c: char| !c.is_alphanumeric());
        let tail = &tok[w.len()..];
        if w.chars().all(|c| c.is_ascii_digit()) {
            out.push_str(tok);
            continue;
        }
        let v = t.exact.get(w)?;
        hit = true;
        out.push_str(v);
        out.push_str(tail);
    }
    if !hit {
        return None;
    }
    // "16 Oktober" -> "16. Oktober" (de); es keeps "16 de octubre" from the table
    if lang == "de" {
        let mut r = String::new();
        let ch: Vec<char> = out.chars().collect();
        for (i, c) in ch.iter().enumerate() {
            r.push(*c);
            if c.is_ascii_digit() && ch.get(i + 1) == Some(&' ') && ch.get(i + 2).map(|n| n.is_uppercase()).unwrap_or(false) {
                r.push('.');
            }
        }
        out = r;
    }
    Some(out)
}

// what to draw for `s`
pub fn tr(s: &str) -> std::borrow::Cow<'_, str> {
    let (lang, t) = current();
    if lang == "pl" || s.len() < 2 || !s.chars().any(|c| c.is_alphabetic()) {
        return s.into();
    }
    if let Some(v) = one(t, s) {
        return v.into();
    }
    if s.contains(" · ") {
        let segs: Vec<String> = s.split(" · ").map(|x| one(t, x).or_else(|| date(t, lang, x)).unwrap_or_else(|| x.to_string())).collect();
        return segs.join(" · ").into();
    }
    if let Some(v) = date(t, lang, s) {
        return v.into();
    }
    miss(s);
    s.into()
}

// MIST_I18N_MISS=file: every string drawn without a translation is appended there (finding Polish left over)
fn miss(s: &str) {
    static OUT: std::sync::OnceLock<Option<Mutex<std::fs::File>>> = std::sync::OnceLock::new();
    let out = OUT.get_or_init(|| {
        let p = std::env::var("MIST_I18N_MISS").ok().filter(|p| !p.is_empty())?;
        std::fs::OpenOptions::new().create(true).append(true).open(p).ok().map(Mutex::new)
    });
    if let Some(f) = out {
        use std::io::Write;
        let _ = writeln!(f.lock().unwrap(), "{s}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn with(lang: &'static str, s: &str) -> String {
        *CUR.write().unwrap() = Some((lang, load(lang)));
        tr(s).into_owned()
    }
    #[test]
    fn translates() {
        assert_eq!(with("en", "Nie przeszkadzać"), "Do not disturb");
        assert_eq!(with("en", "5 min temu"), "5 min ago");
        assert_eq!(with("en", "3 d 4 godz."), "3 d 4 h");
        assert_eq!(with("en", "Połączono · 80%"), "Connected · 80%");
        assert_eq!(with("en", "Piątek, 16 października"), "Friday, 16 October");
        assert_eq!(with("de", "Piątek, 16 października"), "Freitag, 16. Oktober");
        assert_eq!(with("es", "Piątek, 16 października"), "Viernes, 16 de octubre");
        assert_eq!(with("en", "Firefox"), "Firefox");
        assert_eq!(with("en", "PON"), "MON");
        assert_eq!(with("de", "Wydmy"), "Dünen");
        assert_eq!(with("pl", "Nie przeszkadzać"), "Nie przeszkadzać");
        assert_eq!(with("en", "TYDZIEŃ 42"), "WEEK 42");
        assert_eq!(with("de", "TYDZIEŃ 42"), "WOCHE 42");
        assert_eq!(with("en", "poprzedni 8,1 km"), "previous 8,1 km");
        assert_eq!(with("en", "umiarkowana"), "moderate");
        assert_eq!(with("en", "Odpoczynek"), "Rest");
        assert_eq!(with("en", "Ostatni bieg 0,8 km · 2 dni temu"), "Last run 0,8 km · 2 days ago");
        assert_eq!(with("en", "Pochmurno"), "Cloudy");
        assert_eq!(with("en", "pochmurno"), "cloudy");
        assert_eq!(with("en", "Jutro 9°/3° · pochmurno, 5° chłodniej"), "Tomorrow 9°/3° · cloudy, 5° cooler");
    }
    #[test]
    fn table_is_well_formed() {
        let mut lines = TSV.lines();
        let n = lines.next().unwrap().split('\t').count();
        assert_eq!(n, LANGS.len());
        for (i, l) in lines.enumerate() {
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let c: Vec<&str> = l.split('\t').collect();
            assert_eq!(c.len(), n, "line {}: {l}", i + 2);
            let holes = wild(c[0]).len();
            for v in &c[1..] {
                assert!(v.is_empty() || wild(v).len() == holes, "line {}: placeholders differ: {l}", i + 2);
            }
        }
    }
}
