// Mist Files, the smart parts: the search field's query language, file kinds, the view each folder remembers
// (~/.local/state/mist/files-folders.json), the search through subfolders and the archive helpers.
use super::fs::{self, Entry, MimeDb};
use super::SortKey;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------- kinds
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Kind {
    #[default]
    All,
    Folders,
    Images,
    Videos,
    Audio,
    Docs,
    Archives,
    Code,
}
impl Kind {
    pub const LIST: [Kind; 8] = [Kind::All, Kind::Folders, Kind::Images, Kind::Videos, Kind::Audio, Kind::Docs, Kind::Archives, Kind::Code];
    pub fn label(self) -> &'static str {
        match self {
            Kind::All => "Wszystko",
            Kind::Folders => "Foldery",
            Kind::Images => "Obrazy",
            Kind::Videos => "Wideo",
            Kind::Audio => "Muzyka",
            Kind::Docs => "Dokumenty",
            Kind::Archives => "Archiwa",
            Kind::Code => "Kod",
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Kind::All => "filter",
            Kind::Folders => "folder",
            Kind::Images => "photo",
            Kind::Videos => "movie",
            Kind::Audio => "music",
            Kind::Docs => "doc",
            Kind::Archives => "archive",
            Kind::Code => "code",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Kind::All => "all",
            Kind::Folders => "dirs",
            Kind::Images => "images",
            Kind::Videos => "videos",
            Kind::Audio => "audio",
            Kind::Docs => "docs",
            Kind::Archives => "archives",
            Kind::Code => "code",
        }
    }
    fn from_key(s: &str) -> Kind {
        Kind::LIST.into_iter().find(|k| k.key() == s).unwrap_or_default()
    }
    // `typ:` words of the search field (folded: no diacritics)
    pub fn from_word(w: &str) -> Option<Kind> {
        Some(match w {
            "folder" | "foldery" | "katalog" | "katalogi" | "dir" | "dirs" => Kind::Folders,
            "obraz" | "obrazy" | "zdjecie" | "zdjecia" | "foto" | "img" | "image" | "images" => Kind::Images,
            "wideo" | "film" | "filmy" | "video" | "videos" => Kind::Videos,
            "muzyka" | "audio" | "dzwiek" | "music" => Kind::Audio,
            "dokument" | "dokumenty" | "doc" | "docs" | "tekst" => Kind::Docs,
            "archiwum" | "archiwa" | "zip" | "archive" => Kind::Archives,
            "kod" | "code" | "zrodla" | "skrypt" => Kind::Code,
            _ => return None,
        })
    }
    /// The kind of a file (All: none of them).
    pub fn of(e: &Entry) -> Kind {
        if e.dir {
            return Kind::Folders;
        }
        let m = e.mime.as_str();
        let x = fs::ext(&e.name).to_ascii_lowercase();
        if m.starts_with("image/") {
            return Kind::Images;
        }
        if m.starts_with("video/") {
            return Kind::Videos;
        }
        if m.starts_with("audio/") {
            return Kind::Audio;
        }
        if is_archive(&e.name, m) {
            return Kind::Archives;
        }
        const DOCS: [&str; 18] = ["pdf", "txt", "md", "doc", "docx", "odt", "rtf", "ods", "xls", "xlsx", "csv", "odp", "ppt", "pptx", "epub", "djvu", "tex", "org"];
        if DOCS.contains(&x.as_str()) || m == "application/pdf" || m.contains("opendocument") || m.contains("officedocument") || m == "application/msword" {
            return Kind::Docs;
        }
        const CODE: [&str; 30] = ["rs", "c", "h", "cpp", "hpp", "cc", "py", "js", "ts", "tsx", "jsx", "go", "java", "kt", "lua", "sh", "fish", "zsh", "bash", "qml", "json", "toml", "yaml", "yml", "xml", "html", "css", "scss", "sql", "nix"];
        if CODE.contains(&x.as_str()) || m.starts_with("text/x-") || m == "application/x-shellscript" || m == "application/json" || m == "application/javascript" {
            return Kind::Code;
        }
        if m.starts_with("text/") {
            return Kind::Docs;
        }
        Kind::All
    }
    pub fn shows(self, e: &Entry) -> bool {
        self == Kind::All || Kind::of(e) == self
    }
}

const ARCHIVES: [&str; 14] = ["zip", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "7z", "rar", "lz", "lzma", "cab"];
pub fn is_archive(name: &str, mime: &str) -> bool {
    let x = fs::ext(name).to_ascii_lowercase();
    ARCHIVES.contains(&x.as_str()) || mime == "application/zip" || mime.contains("compressed") || mime.ends_with("-tar") || mime == "application/x-7z-compressed" || mime == "application/vnd.rar"
}

/// Lower case without Polish diacritics: "Zdjęcia" and "zdjecia" find the same file.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            c => c,
        })
        .collect()
}

// ---------------------------------------------------------------- the search field
/// "faktura .pdf >100k data:tydzień typ:dokument": every word must hold. Words are parts of the name (case and
/// diacritics ignored), `*`/`?` globs, `.ext` / `ext:a,b`, `>size` / `<size` (k, m, g), `data:` (dziś, wczoraj,
/// tydzień, miesiąc, rok, 7d) and `typ:` (obrazy, wideo, muzyka, dokumenty, archiwa, kod, foldery).
#[derive(Default, Debug, PartialEq)]
pub struct Query {
    words: Vec<String>,
    globs: Vec<String>,
    exts: Vec<String>,
    kind: Option<Kind>,
    min: Option<u64>,
    max: Option<u64>,
    since: Option<i64>,
    until: Option<i64>,
}
impl Query {
    pub fn parse(s: &str) -> Query {
        let mut q = Query::default();
        for tok in s.split_whitespace() {
            let t = fold(tok);
            let pre = |p: &[&str]| p.iter().find_map(|p| t.strip_prefix(p)).map(|x| x.to_string());
            if let Some(x) = pre(&["ext:", "rozszerzenie:"]) {
                q.exts.extend(x.split(',').map(|e| e.trim_start_matches('.').to_string()).filter(|e| !e.is_empty()));
            } else if let Some(x) = pre(&["typ:", "rodzaj:", "kind:"]) {
                match Kind::from_word(&x) {
                    Some(k) => q.kind = Some(k),
                    None => q.words.push(t.clone()),
                }
            } else if let Some(x) = pre(&["data:", "zmieniono:", "date:"]) {
                match since(&x) {
                    Some((a, b)) => {
                        q.since = Some(a);
                        q.until = b;
                    }
                    None => q.words.push(t.clone()),
                }
            } else if let (Some(op), Some(n)) = (t.chars().next().filter(|c| *c == '>' || *c == '<'), parse_size(&t[1..])) {
                if op == '>' {
                    q.min = Some(n);
                } else {
                    q.max = Some(n);
                }
            } else if t.contains('*') || t.contains('?') {
                q.globs.push(t);
            } else if t.len() > 1 && t.starts_with('.') && t[1..].chars().all(|c| c.is_alphanumeric()) {
                q.exts.push(t[1..].to_string());
            } else {
                q.words.push(t);
            }
        }
        q
    }
    pub fn matches(&self, e: &Entry) -> bool {
        let need_name = !self.words.is_empty() || !self.globs.is_empty();
        let name = if need_name { fold(&e.name) } else { String::new() };
        if !self.words.iter().all(|w| name.contains(w.as_str())) || !self.globs.iter().all(|g| super::glob(g, &name)) {
            return false;
        }
        if !self.exts.is_empty() && (e.dir || !self.exts.iter().any(|x| fs::ext(&e.name).eq_ignore_ascii_case(x))) {
            return false;
        }
        if let Some(k) = self.kind {
            if !k.shows(e) {
                return false;
            }
        }
        if self.min.is_some() || self.max.is_some() {
            if e.dir || self.min.is_some_and(|m| e.size < m) || self.max.is_some_and(|m| e.size > m) {
                return false;
            }
        }
        self.since.is_none_or(|s| e.mtime >= s) && self.until.is_none_or(|u| e.mtime < u)
    }
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim_end_matches('b').trim_end_matches('i');
    let (num, mul) = match s.chars().last()? {
        'k' => (&s[..s.len() - 1], 1u64 << 10),
        'm' => (&s[..s.len() - 1], 1 << 20),
        'g' => (&s[..s.len() - 1], 1 << 30),
        't' => (&s[..s.len() - 1], 1 << 40),
        _ => (s, 1),
    };
    let v: f64 = num.replace(',', ".").parse().ok()?;
    (v >= 0.0).then(|| (v * mul as f64) as u64)
}

// the start of the local day `days_ago` days back (unix time)
fn day_start(days_ago: i64) -> i64 {
    let now = unsafe { libc::time(std::ptr::null_mut()) } - days_ago * 86400;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    tm.tm_hour = 0;
    tm.tm_min = 0;
    tm.tm_sec = 0;
    tm.tm_isdst = -1;
    unsafe { libc::mktime(&mut tm) }
}
// a `data:` word: (since, until)
fn since(w: &str) -> Option<(i64, Option<i64>)> {
    Some(match w {
        "dzis" | "dzisiaj" | "today" => (day_start(0), None),
        "wczoraj" | "yesterday" => (day_start(1), Some(day_start(0))),
        "tydzien" | "week" => (day_start(7), None),
        "miesiac" | "month" => (day_start(30), None),
        "rok" | "year" => (day_start(365), None),
        _ => {
            let n: i64 = w.strip_suffix('d').or_else(|| w.strip_suffix("dni"))?.parse().ok()?;
            (day_start(n), None)
        }
    })
}

// ---------------------------------------------------------------- the view each folder remembers
#[derive(Clone, PartialEq, Debug)]
pub struct ViewState {
    pub sort: SortKey,
    pub desc: bool,
    pub hidden: bool,
    pub icons: bool,
    pub kind: Kind,
    pub filter: String,
}
fn sort_key_name(k: SortKey) -> &'static str {
    match k {
        SortKey::Name => "name",
        SortKey::Mtime => "mtime",
        SortKey::Type => "type",
        SortKey::Size => "size",
    }
}
fn sort_key_from(s: &str) -> SortKey {
    match s {
        "mtime" => SortKey::Mtime,
        "type" => SortKey::Type,
        "size" => SortKey::Size,
        _ => SortKey::Name,
    }
}

// folders kept (the least recently used go first)
const KEEP: usize = 600;

#[derive(Default)]
pub struct Memory {
    map: HashMap<String, (ViewState, i64)>,
    dirty: bool,
}
fn mem_path() -> String {
    format!("{}/.local/state/mist/files-folders.json", fs::home())
}
fn now() -> i64 {
    unsafe { libc::time(std::ptr::null_mut()) }
}
impl Memory {
    pub fn load() -> Memory {
        let mut m = Memory::default();
        let Ok(s) = std::fs::read_to_string(mem_path()) else { return m };
        let Ok(serde_json::Value::Object(o)) = serde_json::from_str::<serde_json::Value>(&s) else { return m };
        for (k, v) in o {
            let vs = ViewState {
                sort: sort_key_from(v["sort"].as_str().unwrap_or("")),
                desc: v["desc"].as_bool().unwrap_or(false),
                hidden: v["hidden"].as_bool().unwrap_or(false),
                icons: v["icons"].as_bool().unwrap_or(false),
                kind: Kind::from_key(v["kind"].as_str().unwrap_or("")),
                filter: v["filter"].as_str().unwrap_or("").to_string(),
            };
            m.map.insert(k, (vs, v["t"].as_i64().unwrap_or(0)));
        }
        m
    }
    pub fn get(&self, path: &str) -> Option<&ViewState> {
        self.map.get(path).map(|v| &v.0)
    }
    pub fn set(&mut self, path: &str, vs: ViewState) {
        if self.map.get(path).map(|v| v.0 == vs).unwrap_or(false) {
            return;
        }
        self.map.insert(path.to_string(), (vs, now()));
        self.dirty = true;
    }
    pub fn clear(&mut self) {
        self.map.clear();
        self.dirty = true;
    }
    /// Writes the file when something changed since the last save.
    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.map.len() > KEEP {
            let mut v: Vec<(String, i64)> = self.map.iter().map(|(k, v)| (k.clone(), v.1)).collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.1));
            for (k, _) in v.into_iter().skip(KEEP) {
                self.map.remove(&k);
            }
        }
        let mut o = serde_json::Map::new();
        for (k, (v, t)) in &self.map {
            o.insert(
                k.clone(),
                serde_json::json!({ "sort": sort_key_name(v.sort), "desc": v.desc, "hidden": v.hidden, "icons": v.icons, "kind": v.kind.key(), "filter": v.filter, "t": t }),
            );
        }
        let p = mem_path();
        let _ = std::fs::create_dir_all(fs::parent(&p));
        let tmp = format!("{p}.tmp");
        if std::fs::write(&tmp, serde_json::Value::Object(o).to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

/// A folder seen for the first time: Downloads newest first, picture and video folders as icons.
pub fn guess_view(path: &str, xdg: &HashMap<String, String>, mut base: ViewState) -> ViewState {
    let under = |k: &str| xdg.get(k).map(|d| path == d || path.starts_with(&format!("{d}/"))).unwrap_or(false);
    let name = fold(&fs::base(path));
    if xdg.get("DOWNLOAD").map(|d| d == path).unwrap_or(false) {
        base.sort = SortKey::Mtime;
        base.desc = true;
    }
    if under("PICTURES") || under("VIDEOS") || ["screenshots", "zrzuty ekranu", "wallpapers", "tapety", "camera", "dcim"].contains(&name.as_str()) {
        base.icons = true;
    }
    if ["screenshots", "zrzuty ekranu", "camera", "dcim"].contains(&name.as_str()) {
        base.sort = SortKey::Mtime;
        base.desc = true;
    }
    base
}
/// Mostly pictures and videos (a folder seen for the first time shows them as icons).
pub fn mostly_media(v: &[Entry]) -> bool {
    let files: Vec<&Entry> = v.iter().filter(|e| !e.dir && !e.hidden).collect();
    let media = files.iter().filter(|e| matches!(Kind::of(e), Kind::Images | Kind::Videos)).count();
    files.len() >= 6 && media * 10 >= files.len() * 6
}

// ---------------------------------------------------------------- the search through subfolders
pub const DEEP_MAX: usize = 3000;
const DEEP_SECS: u64 = 6;

/// Entries under `root` (not root's own) matching `q`, breadth first: the nearest come first. Stops at DEEP_MAX
/// results, after DEEP_SECS or when `stop` is set; the bool says it stopped early.
pub fn deep(root: &str, q: &Query, hidden: bool, mime: &MimeDb, stop: &AtomicBool) -> (Vec<Entry>, bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(DEEP_SECS);
    let skip = ["/proc", "/sys", "/dev", "/run", "/tmp/.X11-unix"];
    let mut out = vec![];
    let mut queue = std::collections::VecDeque::from([root.to_string()]);
    let mut n = 0usize;
    while let Some(d) = queue.pop_front() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for de in rd.flatten() {
            n += 1;
            if n % 512 == 0 && (stop.load(Ordering::Relaxed) || std::time::Instant::now() >= deadline) {
                return (out, true);
            }
            let name = de.file_name().to_string_lossy().into_owned();
            if !hidden && name.starts_with('.') {
                continue;
            }
            let p = fs::join(&d, &name);
            let Ok(lm) = std::fs::symlink_metadata(&p) else { continue };
            let link = lm.file_type().is_symlink();
            let m = if link { std::fs::metadata(&p).unwrap_or(lm) } else { lm };
            let dir = m.is_dir();
            // links are shown, not entered (no loops)
            if dir && !link && !skip.contains(&p.as_str()) {
                queue.push_back(p.clone());
            }
            let mt = if dir { "inode/directory".to_string() } else { mime.guess(&name).unwrap_or("application/octet-stream").to_string() };
            let e = Entry { hidden: name.starts_with('.'), size: if dir { 0 } else { m.len() }, mtime: m.mtime(), name, path: p, dir, mime: mt, link, ..Default::default() };
            if q.matches(&e) {
                out.push(e);
                if out.len() >= DEEP_MAX {
                    return (out, true);
                }
            }
        }
    }
    (out, false)
}

// ---------------------------------------------------------------- archives (bsdtar)
/// Packs `names` of `dir` into `dest` (.zip from its name).
pub fn compress(dir: &str, names: &[String], dest: &str) -> Result<(), String> {
    let mut c = std::process::Command::new("bsdtar");
    c.args(["-a", "-cf", dest, "-C", dir, "--"]).args(names);
    run(c)
}
/// Unpacks `file` into `dir`: one folder at the archive's top goes in as it is, anything else into a new folder
/// named after the archive. Returns what was made.
pub fn extract(file: &str, dir: &str) -> Result<String, String> {
    let names: Vec<String> = std::fs::read_dir(dir).map(|r| r.flatten().map(|d| d.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    let base = fs::base(file);
    let stem = base.strip_suffix(".tar.gz").or_else(|| base.strip_suffix(".tar.xz")).or_else(|| base.strip_suffix(".tar.zst")).or_else(|| base.strip_suffix(".tar.bz2")).unwrap_or_else(|| base.rsplit_once('.').map(|x| x.0).unwrap_or(&base)).to_string();
    let tmp = fs::join(dir, &format!(".mist-extract-{}", std::process::id()));
    std::fs::create_dir(&tmp).map_err(fs::err_pl)?;
    let mut c = std::process::Command::new("bsdtar");
    c.args(["-xf", file, "-C", &tmp]);
    if let Err(e) = run(c) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let top: Vec<String> = std::fs::read_dir(&tmp).map(|r| r.flatten().map(|d| d.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    let one_dir = top.len() == 1 && std::fs::metadata(fs::join(&tmp, &top[0])).map(|m| m.is_dir()).unwrap_or(false);
    let made = if one_dir {
        let n = fs::free_name(&names, &top[0], false, true);
        let dst = fs::join(dir, &n);
        std::fs::rename(fs::join(&tmp, &top[0]), &dst).map_err(fs::err_pl)?;
        let _ = std::fs::remove_dir(&tmp);
        dst
    } else {
        let n = fs::free_name(&names, &stem, false, true);
        let dst = fs::join(dir, &n);
        std::fs::rename(&tmp, &dst).map_err(fs::err_pl)?;
        dst
    };
    Ok(made)
}
fn run(mut c: std::process::Command) -> Result<(), String> {
    let o = c.stdin(std::process::Stdio::null()).output().map_err(|e| format!("bsdtar: {e}"))?;
    if o.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&o.stderr);
    Err(err.lines().next().unwrap_or("bsdtar: błąd").trim_start_matches("bsdtar: ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(name: &str, size: u64, mtime: i64, mime: &str) -> Entry {
        Entry { name: name.into(), path: format!("/x/{name}"), size, mtime, mime: mime.into(), ..Default::default() }
    }
    #[test]
    fn query() {
        let f = e("Zdjęcie z wakacji.JPG", 3 << 20, day_start(0) + 10, "image/jpeg");
        assert!(Query::parse("zdjecie").matches(&f));
        assert!(Query::parse("WAKACJI zdj").matches(&f));
        assert!(Query::parse(".jpg").matches(&f));
        assert!(Query::parse("ext:png,jpg").matches(&f));
        assert!(!Query::parse(".png").matches(&f));
        assert!(Query::parse(">1m <5mb").matches(&f));
        assert!(!Query::parse(">10m").matches(&f));
        assert!(Query::parse("typ:obrazy data:dziś").matches(&f));
        assert!(!Query::parse("typ:wideo").matches(&f));
        assert!(!Query::parse("data:wczoraj").matches(&f));
        assert!(Query::parse("zdj*.jpg").matches(&f));
        assert!(Query::parse("typ:nieznany").words == vec!["typ:nieznany".to_string()]);
        assert_eq!(Query::parse("  "), Query::default());
    }
    #[test]
    fn kinds() {
        assert_eq!(Kind::of(&e("a.tar.gz", 1, 0, "application/x-compressed-tar")), Kind::Archives);
        assert_eq!(Kind::of(&e("main.rs", 1, 0, "text/rust")), Kind::Code);
        assert_eq!(Kind::of(&e("x.pdf", 1, 0, "application/pdf")), Kind::Docs);
        assert_eq!(Kind::of(&e("notes", 1, 0, "text/plain")), Kind::Docs);
        assert_eq!(parse_size("1,5k"), Some(1536));
    }
    #[test]
    fn extract_zip() {
        let d = std::env::temp_dir().join(format!("mist-smart-{}", std::process::id()));
        let d = d.to_string_lossy().into_owned();
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(format!("{d}/src/inner")).unwrap();
        std::fs::write(format!("{d}/src/inner/a.txt"), "a").unwrap();
        std::fs::write(format!("{d}/src/b.txt"), "b").unwrap();
        compress(&format!("{d}/src"), &["inner".into()], &format!("{d}/one.zip")).unwrap();
        compress(&format!("{d}/src"), &["inner".into(), "b.txt".into()], &format!("{d}/two.zip")).unwrap();
        std::fs::create_dir(format!("{d}/out")).unwrap();
        // one folder at the top: as it is
        assert_eq!(extract(&format!("{d}/one.zip"), &format!("{d}/out")).unwrap(), format!("{d}/out/inner"));
        // more: into a folder named after the archive
        assert_eq!(extract(&format!("{d}/two.zip"), &format!("{d}/out")).unwrap(), format!("{d}/out/two"));
        assert!(std::path::Path::new(&format!("{d}/out/two/b.txt")).exists());
        // a clash gets a free name
        assert_eq!(extract(&format!("{d}/one.zip"), &format!("{d}/out")).unwrap(), format!("{d}/out/inner (2)"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
