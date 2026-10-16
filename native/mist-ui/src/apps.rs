// Applications and icons for mist-ui:
// - .desktop entries (XDG data dirs): the app name for a window class, like Quickshell's
//   DesktopEntries.byId / heuristicLookup (Name[xx] of the UI language first)
// - icon theme lookup (freedesktop spec: index.theme directories, inherited themes, hicolor, pixmaps),
//   the theme from qt6ct (MistIcons), decoded to premultiplied RGBA at the asked size (PNG / SVG)
use std::collections::HashMap;
use std::rc::Rc;

pub struct Entry {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub wm_class: String,
    pub exec: String,
    pub generic: String,
    pub comment: String,
    pub keywords: String,
    pub no_display: bool,
    pub terminal: bool,
}

fn data_dirs() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut v = vec![std::env::var("XDG_DATA_HOME").unwrap_or(format!("{home}/.local/share"))];
    for d in std::env::var("XDG_DATA_DIRS").unwrap_or("/usr/local/share:/usr/share".into()).split(':') {
        if !d.is_empty() && !v.iter().any(|x| x == d) {
            v.push(d.into());
        }
    }
    v
}

pub struct Apps {
    pub entries: Vec<Entry>,
}

impl Apps {
    pub fn load() -> Apps {
        let mut seen = std::collections::HashSet::new();
        let mut entries = Vec::new();
        // Name[xx] / Comment[xx] ... in the UI language (Name[pl] or Name[pl_PL] for "pl"), else the plain key
        let lang = crate::i18n::lang();
        let local = |k: &str, base: &str| -> bool {
            k.strip_prefix(base).and_then(|r| r.strip_prefix('[')).and_then(|r| r.strip_suffix(']')).map(|l| l == lang || l.split('_').next() == Some(lang)).unwrap_or(false)
        };
        for d in data_dirs() {
            let dir = format!("{d}/applications");
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x == "desktop").unwrap_or(false)).collect();
            files.sort();
            for p in files {
                let id = p.file_stem().unwrap().to_string_lossy().to_string();
                if !seen.insert(id.clone()) {
                    continue; // the first data dir wins (user overrides)
                }
                let Ok(t) = std::fs::read_to_string(&p) else { continue };
                let (mut name, mut name_pl, mut icon, mut wm, mut exec, mut hidden, mut in_main) = (String::new(), String::new(), String::new(), String::new(), String::new(), false, false);
                let (mut generic, mut generic_pl, mut comment, mut comment_pl, mut kw, mut kw_pl, mut terminal) = (String::new(), String::new(), String::new(), String::new(), String::new(), String::new(), false);
                for l in t.lines() {
                    let l = l.trim();
                    if l.starts_with('[') {
                        in_main = l == "[Desktop Entry]";
                        continue;
                    }
                    if !in_main {
                        continue;
                    }
                    if let Some((k, v)) = l.split_once('=') {
                        let k = k.trim();
                        match k {
                            "Name" => name = v.into(),
                            _ if local(k, "Name") => name_pl = v.into(),
                            "Icon" => icon = v.into(),
                            "StartupWMClass" => wm = v.into(),
                            "Exec" => exec = v.into(),
                            "NoDisplay" | "Hidden" => hidden |= v.trim() == "true",
                            "GenericName" => generic = v.into(),
                            _ if local(k, "GenericName") => generic_pl = v.into(),
                            "Comment" => comment = v.into(),
                            _ if local(k, "Comment") => comment_pl = v.into(),
                            "Keywords" => kw = v.into(),
                            _ if local(k, "Keywords") => kw_pl = v.into(),
                            "Terminal" => terminal = v.trim() == "true",
                            _ => {}
                        }
                    }
                }
                let pick = |pl: String, en: String| if pl.is_empty() { en } else { pl };
                entries.push(Entry { id, name: pick(name_pl, name), icon, wm_class: wm, exec, generic: pick(generic_pl, generic), comment: pick(comment_pl, comment), keywords: format!("{kw};{kw_pl}").replace(';', " "), no_display: hidden, terminal });
            }
        }
        Apps { entries }
    }

    // DesktopEntries.byId(appId) || heuristicLookup(appId)
    pub fn lookup(&self, class: &str) -> Option<&Entry> {
        if class.is_empty() {
            return None;
        }
        let c = class.to_lowercase();
        if let Some(e) = self.entries.iter().find(|e| e.id == class || e.id.to_lowercase() == c) {
            return Some(e);
        }
        if let Some(e) = self.entries.iter().find(|e| !e.wm_class.is_empty() && e.wm_class.to_lowercase() == c) {
            return Some(e);
        }
        let last = |s: &str| s.rsplit('.').next().unwrap_or(s).to_lowercase();
        self.entries.iter().find(|e| last(&e.id) == last(&c) || e.name.to_lowercase() == c)
    }
}

// ------------------------------------------------------------------ icon themes
struct Dir {
    path: String,
    size: u32,
    min: u32,
    max: u32,
    scalable: bool,
}
struct Theme {
    bases: Vec<String>,
    dirs: Vec<Dir>,
    inherits: Vec<String>,
}

pub struct Icons {
    themes: HashMap<String, Option<Theme>>,
    order: Vec<String>,
    cache: HashMap<(String, u32), Option<Rc<Image>>>,
}

pub struct Image {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>, // premultiplied
}

fn icon_bases() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut v = vec![format!("{home}/.icons")];
    for d in data_dirs() {
        v.push(format!("{d}/icons"));
    }
    v
}

impl Icons {
    pub fn new() -> Icons {
        let home = std::env::var("HOME").unwrap_or_default();
        let theme = std::fs::read_to_string(format!("{home}/.config/qt6ct/qt6ct.conf"))
            .ok()
            .and_then(|t| t.lines().find_map(|l| l.strip_prefix("icon_theme=").map(|s| s.trim().to_string())))
            .unwrap_or("hicolor".into());
        let mut ic = Icons { themes: HashMap::new(), order: Vec::new(), cache: HashMap::new() };
        // breadth-first inheritance order, hicolor last
        let mut queue = vec![theme];
        while let Some(t) = queue.first().cloned() {
            queue.remove(0);
            if ic.order.contains(&t) || t == "hicolor" {
                continue;
            }
            ic.order.push(t.clone());
            if let Some(Some(th)) = ic.load(&t).map(Some) {
                queue.extend(th.inherits.iter().cloned());
            }
        }
        ic.order.push("hicolor".into());
        ic.load("hicolor");
        ic
    }
    fn load(&mut self, name: &str) -> Option<&Theme> {
        if !self.themes.contains_key(name) {
            let bases: Vec<String> = icon_bases().into_iter().map(|b| format!("{b}/{name}")).filter(|p| std::path::Path::new(p).is_dir()).collect();
            let idx = bases.iter().find_map(|b| std::fs::read_to_string(format!("{b}/index.theme")).ok());
            let th = idx.map(|t| {
                let mut dirs = Vec::new();
                let mut inherits = Vec::new();
                let mut sec = String::new();
                let mut cur: Option<Dir> = None;
                let mut ty = String::new();
                let mut thr = 2u32;
                for l in t.lines() {
                    let l = l.trim();
                    if l.starts_with('[') {
                        if let Some(mut d) = cur.take() {
                            fix(&mut d, &ty, thr);
                            dirs.push(d);
                        }
                        sec = l.trim_matches(|c| c == '[' || c == ']').to_string();
                        if sec != "Icon Theme" {
                            cur = Some(Dir { path: sec.clone(), size: 0, min: 0, max: 0, scalable: false });
                            ty = "Threshold".into();
                            thr = 2;
                        }
                        continue;
                    }
                    let Some((k, v)) = l.split_once('=') else { continue };
                    if sec == "Icon Theme" && k == "Inherits" {
                        inherits = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    }
                    if let Some(d) = cur.as_mut() {
                        match k {
                            "Size" => d.size = v.trim().parse().unwrap_or(0),
                            "MinSize" => d.min = v.trim().parse().unwrap_or(0),
                            "MaxSize" => d.max = v.trim().parse().unwrap_or(0),
                            "Type" => ty = v.trim().into(),
                            "Threshold" => thr = v.trim().parse().unwrap_or(2),
                            _ => {}
                        }
                    }
                }
                if let Some(mut d) = cur.take() {
                    fix(&mut d, &ty, thr);
                    dirs.push(d);
                }
                Theme { bases: bases.clone(), dirs, inherits }
            });
            self.themes.insert(name.into(), th);
        }
        self.themes.get(name).and_then(|t| t.as_ref())
    }

    // path of the best icon file for `name` at `size` px
    pub fn find(&mut self, name: &str, size: u32) -> Option<String> {
        if name.starts_with('/') {
            return std::path::Path::new(name).exists().then(|| name.to_string());
        }
        for t in self.order.clone() {
            let Some(th) = self.load(&t) else { continue };
            let mut best: Option<(u32, String)> = None;
            for d in &th.dirs {
                for b in &th.bases {
                    for ext in ["svg", "png"] {
                        let p = format!("{b}/{}/{name}.{ext}", d.path);
                        if !std::path::Path::new(&p).exists() {
                            continue;
                        }
                        let dist = if d.scalable || (size >= d.min && size <= d.max) { 0 } else if size < d.min { d.min - size } else { size - d.max };
                        // prefer exact, then bigger, then scalable
                        let score = dist * 2 + if d.size < size && !d.scalable { 1 } else { 0 };
                        if best.as_ref().map(|(s, _)| score < *s).unwrap_or(true) {
                            best = Some((score, p));
                        }
                    }
                }
            }
            if let Some((_, p)) = best {
                return Some(p);
            }
        }
        for ext in ["svg", "png", "xpm"] {
            let p = format!("/usr/share/pixmaps/{name}.{ext}");
            if std::path::Path::new(&p).exists() {
                return Some(p);
            }
        }
        None
    }

    pub fn image(&mut self, name: &str, size: u32) -> Option<Rc<Image>> {
        let key = (name.to_string(), size);
        if let Some(c) = self.cache.get(&key) {
            return c.clone();
        }
        let img = self.find(name, size).and_then(|p| decode(&p, size)).map(Rc::new);
        if self.cache.len() > 256 {
            self.cache.clear();
        }
        self.cache.insert(key, img.clone());
        img
    }
}

fn fix(d: &mut Dir, ty: &str, thr: u32) {
    match ty {
        "Scalable" => {
            d.scalable = true;
            if d.min == 0 { d.min = d.size }
            if d.max == 0 { d.max = d.size.max(512) }
        }
        "Fixed" => {
            d.min = d.size;
            d.max = d.size;
        }
        _ => {
            d.min = d.size.saturating_sub(thr);
            d.max = d.size + thr;
        }
    }
}

// PNG or SVG -> premultiplied RGBA, size x size (aspect kept, centred)
pub fn decode(path: &str, size: u32) -> Option<Image> {
    let mut pm = tiny_skia::Pixmap::new(size, size)?;
    if path.ends_with(".svg") || path.ends_with(".svgz") {
        let data = std::fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let s = tree.size();
        let k = size as f32 / s.width().max(s.height());
        let tx = (size as f32 - s.width() * k) / 2.0;
        let ty = (size as f32 - s.height() * k) / 2.0;
        resvg::render(&tree, tiny_skia::Transform::from_row(k, 0.0, 0.0, k, tx, ty), &mut pm.as_mut());
    } else {
        let src = tiny_skia::Pixmap::load_png(path).ok()?;
        let k = size as f32 / src.width().max(src.height()) as f32;
        let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
        let tx = (size as f32 - src.width() as f32 * k) / 2.0;
        let ty = (size as f32 - src.height() as f32 * k) / 2.0;
        pm.draw_pixmap(0, 0, src.as_ref(), &paint, tiny_skia::Transform::from_row(k, 0.0, 0.0, k, tx, ty), None);
    }
    Some(Image { w: size, h: size, rgba: pm.take() })
}
