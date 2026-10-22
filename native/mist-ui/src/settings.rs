// Ustawienia Mist: one window in the middle of the monitor (System Settings style). A sidebar with a search
// field and the sections, the selected section on the right as grouped rows (label, a line of help, the
// control on the right: switch, segmented control, pop-up menu, chips, colour swatches, buttons). Everything
// is applied at once and saved where Mist already reads it:
//   Ogólne       settings.json lang, location.json (searched with `mistd geo`), restart
//   Tapeta       wallpaper.json (live / image / tempo / music), thumbnails from ~/.cache/mist/thumbs
//   Pulpit       widgets.json desk.* + the bento places (a live mock of the desktop: click a card to swap it)
//   Widżety      widgets.json per widget (desktop::groups), shown / hidden, its place
//   Ekran        night light (~/.local/state/mist/nightlight + the Hyprland screen shader), high contrast
//   Blokada      idle.json lock / off (idle.rs follows the file)
//   Powiadomienia  do not disturb (`msg notif dnd`), the history
//   O Mist       version, what the Mist processes use now
// Opened by `msg mist toggleOn settings|desk [page]`, a right click on the desktop (its card's page), Start,
// Spotlight and the control centre. It opens by itself on the first start (settings.json "welcomed").
use crate::anim::{Anim, Ease};
use crate::desktop::{border, circle, groups, line_h, rect, st, stroke, text, width, Cfg, Fx, ACC, FAINT, FG, FG2, HAIR, MUTED, WHITE, WIDGETS};
use crate::gfx::{Canvas, Rgba};
use crate::pane::*;
use serde_json::{json, Value};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use tiny_skia::{PathBuilder, Pixmap, PixmapPaint, Transform};

const WIN_W: f32 = 1000.0;
const WIN_H: f32 = 700.0;
const SIDE: f32 = 240.0;
const RAD: f32 = 20.0;
const M: f32 = 48.0; // the layer's room for the shadow
const HEAD: f32 = 66.0; // the content header
const PADX: f32 = 32.0;
const ROW_H: f32 = 46.0;
const BG: Rgba = Rgba(20.0 / 255.0, 22.0 / 255.0, 29.0 / 255.0, 0.985);
const SIDE_BG: Rgba = Rgba(1.0, 1.0, 1.0, 0.035);
const CARD: Rgba = Rgba(1.0, 1.0, 1.0, 0.045);
const CARD_RIM: Rgba = Rgba(1.0, 1.0, 1.0, 0.07);
const ON: Rgba = Rgba::rgb8(0x34, 0xc7, 0x8f, 1.0);
const DARK: Rgba = Rgba(0.05, 0.06, 0.09, 1.0);

// key, name, icon, colour of its square
const PAGES: [(&str, &str, &str, (u8, u8, u8)); 8] = [
    ("general", "Ogólne", "settings", (0x8e, 0x8e, 0x93)),
    ("wallpaper", "Tapeta", "photo", (0x32, 0xad, 0xe6)),
    ("desktop", "Pulpit", "desktop", (0x0a, 0x84, 0xff)),
    ("widgets", "Widżety", "layoutGrid", (0xbf, 0x5a, 0xf2)),
    ("display", "Ekran", "bright", (0xff, 0x9f, 0x0a)),
    ("lock", "Blokada i uśpienie", "lock", (0x5e, 0x5c, 0xe6)),
    ("notif", "Powiadomienia", "bell", (0xff, 0x45, 0x3a)),
    ("about", "O Mist", "sparkles", (0x30, 0xb0, 0xa8)),
];
// a little space in the sidebar before these
const GAP_BEFORE: [usize; 2] = [4, 7];
const SLOTS: [&str; 3] = ["0", "1", "2"];
const WCATS: [(&str, &str); 6] = [("all", "Wszystkie"), ("auto", "Auto"), ("calm", "Spokojne"), ("live", "Żywe"), ("toon", "Bajki"), ("photos", "Zdjęcia")];
const TEMPOS: [(&str, &str); 3] = [("calm", "Spokojne"), ("normal", "Normalne"), ("fast", "Szybkie")];

pub fn widget_icon(k: &str) -> &'static str {
    match k {
        "weather" => "cloudSun",
        "garmin" => "watch",
        "running" => "run",
        "music" => "music",
        "calendar" => "calendar",
        "system" => "cpu",
        _ => "clock",
    }
}
fn widget_color(k: &str) -> Rgba {
    match k {
        "weather" => Rgba::rgb8(0x32, 0xad, 0xe6, 1.0),
        "garmin" => Rgba::rgb8(0x30, 0xd1, 0x58, 1.0),
        "running" => Rgba::rgb8(0xff, 0x9f, 0x0a, 1.0),
        "music" => Rgba::rgb8(0xff, 0x37, 0x5f, 1.0),
        "calendar" => Rgba::rgb8(0xff, 0x45, 0x3a, 1.0),
        "system" => Rgba::rgb8(0x8e, 0x8e, 0x93, 1.0),
        _ => Rgba::rgb8(0xff, 0xcc, 0x00, 1.0),
    }
}
fn wname(k: &str) -> &'static str {
    WIDGETS.iter().find(|w| w.0 == k).map(|w| w.1).unwrap_or("")
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Page {
    Nav(usize),
    Widget(&'static str),
}

type Id = (&'static str, &'static str);

#[derive(Clone, PartialEq, Debug)]
enum H {
    Nav(usize),
    Search,
    Result(usize),
    Back,
    Close,
    Set(Id, Value),
    Menu(Id),
    MenuItem(usize),
    Chip(Id, Value),
    Slot(usize),
    Go(&'static str),
    Wall(String),
    WallCat(usize),
    Btn(&'static str),
    Field,
    Place(usize),
    Win,
}

enum Ctl {
    Toggle(Id, bool, Value, Value),
    Seg(Id, Vec<(Value, String)>, Value),
    Menu(Id, Vec<(Value, String)>, Value),
    Chips(Id, Vec<(Value, String)>, Vec<Value>),
    Swatch(Id, Vec<(Value, Rgba, String)>, Value),
    Button(&'static str, String, bool),
    Value(String),
    Go(&'static str, String),
    Place,
}
struct Row {
    label: String,
    sub: String,
    icon: Option<(&'static str, Rgba)>,
    ctl: Ctl,
}
fn row(label: &str, sub: &str, ctl: Ctl) -> Row {
    Row { label: label.into(), sub: sub.into(), icon: None, ctl }
}
enum Block {
    Group(String, Vec<Row>, String),
    Welcome,
    WallHero,
    WallGrid,
    DeskPreview,
    AboutHero,
}

struct Menu {
    id: Id,
    rect: (f32, f32, f32, f32), // its button, layer coordinates
    items: Vec<(Value, String)>,
    cur: Value,
    sel: usize,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
fn read(p: &str) -> Value {
    std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({}))
}
fn write_atomic(p: &str, v: &Value) {
    let _ = std::fs::create_dir_all(std::path::Path::new(p).parent().unwrap());
    let tmp = format!("{p}.tmp");
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, p);
    }
}
fn patch(p: &str, k: &str, v: Value) -> Value {
    let mut c = read(p);
    if !c.is_object() {
        c = json!({});
    }
    c[k] = v;
    write_atomic(p, &c);
    c
}
fn settings_path() -> String {
    format!("{}/.config/mist/settings.json", home())
}
fn location_path() -> String {
    format!("{}/.config/mist/location.json", home())
}
fn idle_path() -> String {
    format!("{}/.config/mist/idle.json", home())
}
fn wall_path() -> String {
    format!("{}/.config/mist/wallpaper.json", home())
}
// first start: nothing saved yet about the welcome
pub fn first_run() -> bool {
    !read(&settings_path())["welcomed"].as_bool().unwrap_or(false)
}
fn current_place() -> String {
    read(&location_path())["city"].as_str().unwrap_or("").to_string()
}
fn place_label(v: &Value) -> String {
    let mut s = v["city"].as_str().unwrap_or("").to_string();
    for k in ["region", "country"] {
        if let Some(x) = v[k].as_str().filter(|x| !x.is_empty() && *x != s) {
            s.push_str(", ");
            s.push_str(x);
        }
    }
    s
}
fn dur(s: u64) -> String {
    match s {
        0 => "Nigdy".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h", s / 3600),
    }
}
// RSS of the processes named so (MB), from /proc
fn rss(names: &[&str]) -> Option<f64> {
    let mut kb = 0u64;
    let mut any = false;
    for e in std::fs::read_dir("/proc").ok()?.flatten() {
        let n = e.file_name();
        if !n.to_string_lossy().chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let comm = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
        if !names.contains(&comm.trim()) {
            continue;
        }
        let status = std::fs::read_to_string(e.path().join("status")).unwrap_or_default();
        if let Some(l) = status.lines().find(|l| l.starts_with("VmRSS:")) {
            kb += l.split_whitespace().nth(1).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            any = true;
        }
    }
    any.then_some(kb as f64 / 1024.0)
}

pub struct Settings {
    page: Page,
    cfg: Cfg,
    fx: Fx,
    o: Anim,
    po: Anim,
    ptr: (f32, f32),
    hits: Vec<(f32, f32, f32, f32, H)>,
    hot: Option<H>,
    scroll: f32,
    max_scroll: f32,
    lo: (f32, f32), // the layer's origin on the monitor
    menu: Option<Menu>,
    search: crate::edit::LineEdit,
    searching: bool,
    results: Vec<(Page, String)>,
    place: crate::edit::LineEdit,
    editing: bool,
    found: Vec<Value>,
    asked: String,
    geo_rx: Option<std::sync::mpsc::Receiver<Vec<Value>>>,
    welcome: bool,
    wall: Value,
    presets: Vec<Value>,
    photos: Vec<String>,
    wcat: usize,
    idle: Value,
    settings: Value,
    night: bool,
    contrast0: bool,
    frame: Option<((u32, u32), Pixmap)>,
    about: Vec<(String, String)>,
    version: String,
    flash: Option<(String, std::time::Instant)>,
}

impl Settings {
    pub fn new(page: &str) -> Settings {
        let mut o = Anim::new(0.0);
        o.set(1.0, 160.0, Ease::OutCubic);
        let welcome = first_run();
        if welcome {
            patch(&settings_path(), "welcomed", json!(true));
        }
        let settings = read(&settings_path());
        let mut s = Settings {
            page: Page::Nav(2),
            cfg: Cfg::load(),
            fx: Fx::new(),
            o,
            po: Anim::new(1.0),
            ptr: (-1.0, -1.0),
            hits: Vec::new(),
            hot: None,
            scroll: 0.0,
            max_scroll: 0.0,
            lo: (0.0, 0.0),
            menu: None,
            search: Default::default(),
            searching: false,
            results: Vec::new(),
            place: Default::default(),
            editing: welcome && current_place().is_empty(),
            found: Vec::new(),
            asked: String::new(),
            geo_rx: None,
            welcome,
            wall: read(&wall_path()),
            presets: crate::presets::load().into_iter().filter(|p| p.get("mode").is_some()).collect(),
            photos: crate::presets::photos(),
            wcat: 0,
            idle: read(&idle_path()),
            contrast0: settings["contrast"] == "high",
            settings,
            night: crate::control::night_on(),
            frame: None,
            about: Vec::new(),
            version: String::new(),
            flash: None,
        };
        s.page = if welcome { Page::Nav(0) } else { Self::page_of(page) };
        if s.page == Page::Nav(7) {
            s.load_about();
        }
        s
    }
    // "desk" / "" = the desktop, a widget's key = its page, else a section key
    fn page_of(p: &str) -> Page {
        if let Some(k) = WIDGETS.iter().map(|w| w.0).find(|k| *k == p) {
            return Page::Widget(k);
        }
        match p {
            "" | "desk" => Page::Nav(2),
            _ => Page::Nav(PAGES.iter().position(|x| x.0 == p).unwrap_or(2)),
        }
    }
    fn nav_index(&self) -> usize {
        match self.page {
            Page::Nav(i) => i,
            Page::Widget(_) => 3,
        }
    }
    fn go(&mut self, p: Page) {
        if p != self.page {
            self.page = p;
            self.scroll = 0.0;
            self.menu = None;
            self.po = Anim::new(0.0);
            self.po.set(1.0, 180.0, Ease::OutCubic);
            if p == Page::Nav(7) {
                self.load_about();
            }
        }
    }
    fn load_about(&mut self) {
        let mb = |v: Option<f64>| v.map(|x| format!("{x:.0} MB")).unwrap_or("nie działa".into());
        let comp = if rss(&["mist-comp"]).is_some() { ("Kompozytor (mist-comp)", rss(&["mist-comp"])) } else { ("Kompozytor (Hyprland)", rss(&["Hyprland"])) };
        self.about = vec![
            ("Powłoka (mist-ui)".into(), mb(rss(&["mist-ui-bin"]))),
            ("Tapeta (mist-wall)".into(), mb(rss(&["mist-wall-bin"]))),
            ("Usługi (mistd)".into(), mb(rss(&["mistd"]))),
            (comp.0.into(), mb(comp.1)),
        ];
        if self.version.is_empty() {
            let out = std::process::Command::new("git").args(["-C", &crate::paths::mist_path(""), "describe", "--tags", "--always", "--dirty"]).stderr(std::process::Stdio::null()).output();
            self.version = out.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty()).unwrap_or("dev".into());
        }
    }

    // ---------------------------------------------------------------- what each page shows
    fn cfg_rows(&self, card: &'static str) -> Vec<Row> {
        let mut out = Vec::new();
        for gr in groups(card) {
            let id: Id = (card, gr.key);
            let cur = self.cfg.opt(card, gr.key);
            let opts: Vec<(Value, String)> = gr.opts.iter().map(|(v, l)| (v.clone(), l.to_string())).collect();
            let label = gr.label.to_string();
            let ctl = if gr.multi {
                Ctl::Chips(id, opts, cur.as_array().cloned().unwrap_or_default())
            } else if opts.len() == 2 && opts.iter().any(|o| o.0 == "show") && opts.iter().any(|o| o.0 == "hide") {
                Ctl::Toggle(id, cur == "show", json!("show"), json!("hide"))
            } else if opts.len() <= 3 && opts.iter().map(|o| o.1.chars().count()).sum::<usize>() <= 30 {
                Ctl::Seg(id, opts, cur)
            } else {
                Ctl::Menu(id, opts, cur)
            };
            let sub = if gr.multi && gr.max > 0 { format!("Najwyżej {}", gr.max) } else { String::new() };
            out.push(Row { label, sub, icon: None, ctl });
        }
        out
    }
    fn desk_row(&self, key: &'static str, label: &str) -> Row {
        let mut r = self.cfg_rows("desk").into_iter().find(|r| match &r.ctl {
            Ctl::Toggle(id, ..) | Ctl::Seg(id, ..) | Ctl::Menu(id, ..) => id.1 == key,
            _ => false,
        }).unwrap_or_else(|| row(label, "", Ctl::Value(String::new())));
        r.label = label.into();
        r
    }
    fn blocks(&self, page: Page, dnd: bool) -> Vec<Block> {
        let g = |t: &str, rows: Vec<Row>, foot: &str| Block::Group(t.into(), rows, foot.into());
        match page {
            Page::Nav(0) => {
                let lang = crate::i18n::setting();
                let mut langs = vec![(json!("auto"), "Automatycznie".to_string())];
                langs.extend(crate::i18n::LANGS.iter().map(|(c, n)| (json!(c), n.to_string())));
                let mut v = Vec::new();
                if self.welcome {
                    v.push(Block::Welcome);
                }
                v.push(g("Język i region", vec![row("Język", "Napisy w całym Mist", Ctl::Menu(("app", "lang"), langs, json!(lang))), row("Miejsce pogody", "Prognoza, wschód i zachód słońca", Ctl::Place)], ""));
                v.push(g("Mist", vec![row("Uruchom ponownie Mist", "Pasek, dok i pulpit wstaną od nowa po sekundzie", Ctl::Button("restart", "Uruchom ponownie".into(), false))], ""));
                v
            }
            Page::Nav(1) => {
                let tempo = self.wall["tempo"].as_str().unwrap_or("normal").to_string();
                vec![
                    Block::WallHero,
                    g("Animacja", vec![
                        row("Tempo animacji", "Jak szybko płynie żywa tapeta", Ctl::Seg(("wall", "tempo"), TEMPOS.iter().map(|(k, l)| (json!(k), l.to_string())).collect(), json!(tempo))),
                        row("Reaguje na muzykę", "Sceny pulsują w rytm tego, co gra", Ctl::Toggle(("wall", "music"), self.wall["music"].as_bool().unwrap_or(true), json!(true), json!(false))),
                    ], ""),
                    Block::WallGrid,
                ]
            }
            Page::Nav(2) => {
                let swatches = vec![(json!("white"), WHITE, "Biały".to_string()), (json!("accent"), ACC, "Akcent".to_string()), (json!("warm"), Rgba::rgb8(0xf6, 0xd9, 0xb0, 1.0), "Ciepły".to_string())];
                vec![
                    Block::DeskPreview,
                    g("Karty", vec![
                        row("Liczba kart", "Ile widżetów stoi pod zegarem", Ctl::Seg(("desk", "cards"), vec![(json!(0), "Brak".into()), (json!(1), "1".into()), (json!(2), "2".into()), (json!(3), "3".into())], json!(self.cfg.cards()))),
                        { let mut r = self.desk_row("glass", "Tło kart"); r.sub = "Jak mocno karty przyciemniają tapetę".into(); r },
                        row("Widżety", "Co pokazują i w jakiej kolejności", Ctl::Go("widgets", String::new())),
                    ], ""),
                    g("Zegar", vec![
                        self.desk_row("clock", "Rozmiar"),
                        self.desk_row("clockw", "Grubość"),
                        row("Kolor", "", Ctl::Swatch(("desk", "clockc"), swatches, self.cfg.opt("desk", "clockc"))),
                        self.desk_row("date", "Data nad zegarem"),
                        { let mut r = self.desk_row("sun", "Wschód i zachód słońca"); r.sub = "Linia dnia pod zegarem".into(); r },
                    ], ""),
                ]
            }
            Page::Nav(3) => {
                let shown = self.cfg.shown();
                let mk = |k: &'static str, status: String| Row { label: wname(k).into(), sub: String::new(), icon: Some((widget_icon(k), widget_color(k))), ctl: Ctl::Go(k, status) };
                let on: Vec<Row> = shown.iter().enumerate().filter_map(|(i, k)| WIDGETS.iter().find(|w| w.0 == k).map(|w| mk(w.0, format!("Miejsce {}", i + 1)))).collect();
                let off: Vec<Row> = WIDGETS.iter().filter(|w| crate::data::available(w.0) && !shown.iter().any(|k| k == w.0)).map(|w| mk(w.0, "Ukryty".into())).collect();
                let mut v = Vec::new();
                if !on.is_empty() {
                    v.push(g("Na pulpicie", on, ""));
                }
                let foot = if WIDGETS.iter().any(|w| !crate::data::available(w.0)) { "Garmin i Bieganie pojawią się po podłączeniu danych (docs/guides/garmin.md)." } else { "" };
                v.push(g("Dostępne", off, foot));
                v
            }
            Page::Widget(k) => {
                let shown = self.cfg.shown();
                let pos = shown.iter().position(|x| x == k);
                let mut top = vec![row("Pokaż na pulpicie", "", Ctl::Toggle(("show", k), pos.is_some(), json!(true), json!(false)))];
                if let Some(p) = pos {
                    let n = self.cfg.cards();
                    if n > 1 {
                        top.push(row("Miejsce", "Od lewej", Ctl::Seg(("slotof", k), (0..n).map(|i| (json!(i), format!("{}", i + 1))).collect(), json!(p))));
                    }
                }
                if k == "weather" {
                    let cur = current_place();
                    top.push(row("Miejsce pogody", "", Ctl::Go("general", if cur.is_empty() { "Ustaw".into() } else { cur })));
                }
                vec![g("", top, ""), g("Co pokazuje", self.cfg_rows(k), "Zmiany widać od razu na pulpicie.")]
            }
            Page::Nav(4) => vec![
                g("Kolory", vec![row("Nocne światło", "Cieplejsze barwy ekranu (~4000 K)", Ctl::Toggle(("sys", "night"), self.night, json!(true), json!(false)))], ""),
                g("Dostępność", vec![row("Wysoki kontrast", "Nieprzezroczyste panele i wyraźniejszy tekst", Ctl::Toggle(("app", "contrast"), self.settings["contrast"] == "high", json!("high"), json!("normal")))],
                    if (self.settings["contrast"] == "high") != self.contrast0 { "Zmiana zadziała po ponownym uruchomieniu Mist (Ogólne)." } else { "" }),
                g("Monitory", vec![row("Monitor główny", "Pulpit, dok i powiadomienia", Ctl::Value(self.settings["primary"].as_str().map(String::from).unwrap_or_else(|| std::env::var("MIST_PRIMARY").unwrap_or("auto".into()))))], ""),
            ],
            Page::Nav(5) => {
                let opts = |xs: &[u64]| xs.iter().map(|s| (json!(s), dur(*s))).collect::<Vec<_>>();
                let lock = self.idle["lock"].as_u64().unwrap_or(600);
                let off = self.idle["off"].as_u64().unwrap_or(900);
                vec![
                    g("Gdy nic nie robisz", vec![
                        row("Zablokuj ekran po", "", Ctl::Menu(("idle", "lock"), opts(&[60, 120, 300, 600, 900, 1800, 3600, 0]), json!(lock))),
                        row("Wyłącz ekran po", "", Ctl::Menu(("idle", "off"), opts(&[60, 120, 300, 600, 900, 1800, 3600, 0]), json!(off))),
                    ], "Filmy i muzyka w przeglądarce wstrzymują blokadę."),
                    g("", vec![row("Zablokuj teraz", "", Ctl::Button("lock", "Zablokuj".into(), false))], ""),
                ]
            }
            Page::Nav(6) => vec![
                g("", vec![
                    row("Nie przeszkadzać", "Dymki się nie pokazują, trafiają do historii", Ctl::Toggle(("sys", "dnd"), dnd, json!(true), json!(false))),
                    row("Historia powiadomień", "", Ctl::Button("history", "Otwórz".into(), false)),
                ], ""),
            ],
            _ => {
                let mut rows: Vec<Row> = self.about.iter().map(|(a, b)| row(a, "", Ctl::Value(b.clone()))).collect();
                rows.push(row("Zmierz jeszcze raz", "", Ctl::Button("about", "Odśwież".into(), false)));
                let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim().to_string();
                vec![
                    Block::AboutHero,
                    g("Pamięć teraz", rows, ""),
                    g("System", vec![
                        row("Jądro", "", Ctl::Value(kernel)),
                        row("Kod źródłowy", "github.com/wiktor-jozwiak/mist", Ctl::Button("source", "Otwórz".into(), false)),
                        row("Sponsoruj na GitHubie", "github.com/sponsors/wiktor-jozwiak", Ctl::Button("sponsor", "Wesprzyj".into(), false)),
                    ], ""),
                ]
            }
        }
    }
    fn title(&self) -> String {
        match self.page {
            Page::Widget(k) => wname(k).to_string(),
            Page::Nav(i) => PAGES[i].1.to_string(),
        }
    }
    // the search: every row label of every page (+ the page names)
    fn run_search(&mut self, dnd: bool) {
        let q = self.search.text.trim().to_lowercase();
        self.results.clear();
        if q.is_empty() {
            return;
        }
        let mut pages: Vec<Page> = (0..PAGES.len()).map(Page::Nav).collect();
        pages.extend(WIDGETS.iter().map(|w| Page::Widget(w.0)));
        for p in pages {
            let pname = match p { Page::Nav(i) => PAGES[i].1.to_string(), Page::Widget(k) => wname(k).to_string() };
            if pname.to_lowercase().contains(&q) {
                self.results.push((p, String::new()));
            }
            for b in self.blocks(p, dnd) {
                if let Block::Group(_, rows, _) = b {
                    for r in rows {
                        if (r.label.to_lowercase().contains(&q) || r.sub.to_lowercase().contains(&q)) && !self.results.iter().any(|x| x.1 == r.label && x.0 == p) {
                            self.results.push((p, r.label));
                        }
                    }
                }
            }
        }
        self.results.truncate(40);
    }

    // ---------------------------------------------------------------- applying
    fn set(&mut self, id: Id, v: Value) -> Act {
        match id.0 {
            "app" if id.1 == "lang" => {
                crate::i18n::set(v.as_str().unwrap_or("auto"));
                Act::DeskCfg
            }
            "app" => {
                self.settings = patch(&settings_path(), id.1, v);
                Act::Redraw
            }
            "idle" => {
                self.idle = patch(&idle_path(), id.1, v);
                Act::Redraw
            }
            "wall" => {
                self.wall = patch(&wall_path(), id.1, v);
                Act::Redraw
            }
            "sys" if id.1 == "dnd" => Act::Exec(vec![crate::paths::self_bin(), "msg".into(), "notif".into(), "dnd".into(), v.as_bool().unwrap_or(false).to_string()]),
            "sys" => {
                self.night = v.as_bool().unwrap_or(false);
                let _ = std::fs::create_dir_all(format!("{}/.local/state/mist", home()));
                let _ = std::fs::write(format!("{}/.local/state/mist/nightlight", home()), if self.night { "1" } else { "0" });
                Act::Exec(crate::control::night_cmd(self.night))
            }
            "slot" => {
                let i = id.1.parse::<usize>().unwrap_or(0);
                if let Some(k) = v.as_str() {
                    self.cfg.put_slot(i, k);
                }
                Act::DeskCfg
            }
            "slotof" => {
                self.cfg.put_slot(v.as_u64().unwrap_or(0) as usize, id.1);
                Act::DeskCfg
            }
            "show" => {
                let k = id.1;
                let n = self.cfg.cards();
                if v.as_bool().unwrap_or(false) {
                    if n < 3 {
                        self.cfg.set("desk", "cards", json!(n + 1));
                    }
                    let n = self.cfg.cards();
                    if n > 0 {
                        self.cfg.put_slot(n - 1, k);
                    }
                } else {
                    let mut o = self.cfg.order();
                    if let Some(p) = o.iter().position(|x| x == k).filter(|p| *p < n) {
                        let x = o.remove(p);
                        o.push(x);
                        self.cfg.set("order", "", json!(o));
                        self.cfg.set("desk", "cards", json!(n.saturating_sub(1)));
                    }
                }
                Act::DeskCfg
            }
            card => {
                self.cfg.set(card, id.1, v);
                Act::DeskCfg
            }
        }
    }
    fn chip(&mut self, id: Id, v: &Value) -> Act {
        if let Some(gr) = groups(id.0).into_iter().find(|g| g.key == id.1) {
            self.cfg.click(id.0, &gr, v);
        }
        Act::DeskCfg
    }
    fn set_wall(&mut self, key: &str) {
        if let Some(id) = key.strip_prefix("live:") {
            self.wall = patch(&wall_path(), "live", json!(id));
        } else if let Some(f) = key.strip_prefix("photo:") {
            patch(&wall_path(), "live", json!(""));
            self.wall = patch(&wall_path(), "image", json!(f));
        }
    }
    fn wall_key(&self) -> String {
        let live = self.wall["live"].as_str().unwrap_or("");
        if !live.is_empty() { format!("live:{live}") } else { format!("photo:{}", self.wall["image"].as_str().unwrap_or("")) }
    }
    // (key, name, desc, category, colours) of the gallery items in the chosen category
    fn wall_items(&self, cat: &str) -> Vec<(String, String, String, String, Vec<String>)> {
        let mut v = Vec::new();
        if cat != "photos" {
            for p in &self.presets {
                let c = p["cat"].as_str().unwrap_or("");
                if cat == "all" || c == cat {
                    let s = |k: &str| p[k].as_str().unwrap_or("").to_string();
                    let cols = p["c"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                    v.push((format!("live:{}", s("id")), s("name"), s("desc"), c.to_string(), cols));
                }
            }
        }
        if cat == "photos" || cat == "all" {
            for ph in &self.photos {
                let name = std::path::Path::new(ph).file_stem().map(|s| s.to_string_lossy().replace(['-', '_'], " ")).unwrap_or_default();
                v.push((format!("photo:{ph}"), name, "Zdjęcie z folderu tapet".into(), "photos".into(), Vec::new()));
            }
        }
        v
    }
    fn search_geo(&mut self) {
        let q = self.place.text.trim().to_string();
        if q.chars().count() < 2 || q == self.asked {
            return;
        }
        self.asked = q.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.geo_rx = Some(rx);
        let lang = if crate::i18n::setting() == "pl" || std::env::var("LANG").unwrap_or_default().starts_with("pl") { "pl" } else { "en" };
        let _ = std::thread::Builder::new().name("geo".into()).spawn(move || {
            let out = std::process::Command::new(crate::paths::mistd()).args(["geo", &q, lang]).stderr(std::process::Stdio::null()).output();
            let v: Vec<Value> = out.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or_default();
            let _ = tx.send(v);
        });
    }
    fn choose_place(&mut self, i: usize) {
        let Some(v) = self.found.get(i) else { return };
        write_atomic(&location_path(), &json!({"city": v["city"], "lat": v["lat"], "lon": v["lon"]}));
        self.found.clear();
        self.editing = false;
        self.place.set("");
        self.asked.clear();
        crate::data::refresh("weather");
    }
    fn hit(&self) -> Option<H> {
        let p = self.ptr;
        self.hits.iter().rev().find(|r| p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3).map(|r| r.4.clone())
    }
    fn is_hot(&self, h: &H) -> bool {
        self.hot.as_ref() == Some(h)
    }

    // ---------------------------------------------------------------- controls
    fn toggle(&mut self, c: &mut Canvas, x: f32, cy: f32, on: bool, key: &str) {
        let k = self.fx.v(&format!("tg.{key}"), if on { 1.0 } else { 0.0 }, 180.0, Ease::OutCubic);
        let (w, h) = (44.0, 26.0);
        let y = cy - h / 2.0;
        rect(c, x, y, w, h, h / 2.0, WHITE.alpha(0.16));
        if k > 0.0 {
            rect(c, x, y, w, h, h / 2.0, ON.alpha(k));
        }
        let kx = x + 13.0 + k * (w - 26.0);
        circle(c, kx, cy + 1.0, 11.5, Rgba(0.0, 0.0, 0.0, 0.18));
        circle(c, kx, cy, 11.0, WHITE);
    }
    fn chevrons(c: &mut Canvas, cx: f32, cy: f32, col: Rgba) {
        for d in [-1.0f32, 1.0] {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 3.5, cy + d * 1.8);
            pb.line_to(cx, cy + d * 5.0);
            pb.line_to(cx + 3.5, cy + d * 1.8);
            if let Some(p) = pb.finish() {
                stroke(c, &p, 1.5, col, true);
            }
        }
    }
    fn chev_right(c: &mut Canvas, cx: f32, cy: f32, col: Rgba) {
        let mut pb = PathBuilder::new();
        pb.move_to(cx - 2.5, cy - 5.0);
        pb.line_to(cx + 2.5, cy);
        pb.line_to(cx - 2.5, cy + 5.0);
        if let Some(p) = pb.finish() {
            stroke(c, &p, 1.8, col, true);
        }
    }
    fn button(c: &mut Canvas, t: &mut crate::text::Text, x: f32, cy: f32, label: &str, primary: bool, hot: bool) -> f32 {
        let s = st(13.0, 600.0);
        let w = width(t, label, &s) + 28.0;
        let fill = if primary { if hot { ACC } else { ACC.alpha(0.9) } } else { WHITE.alpha(if hot { 0.16 } else { 0.10 }) };
        rect(c, x, cy - 15.0, w, 30.0, 9.0, fill);
        let lh = line_h(t, &s);
        text(c, t, x + 14.0, cy - lh / 2.0, label, &s, if primary { DARK } else { FG }, false, None);
        w
    }

    // the width a control takes on the right of its row (to give the label the rest)
    fn ctl_w(t: &mut crate::text::Text, ctl: &Ctl) -> f32 {
        let s = st(13.0, 550.0);
        match ctl {
            Ctl::Toggle(..) => 44.0,
            Ctl::Seg(_, o, _) => o.iter().map(|x| width(t, &x.1, &s) + 26.0).sum::<f32>() + 4.0,
            Ctl::Menu(_, o, cur) => width(t, &o.iter().find(|x| &x.0 == cur).map(|x| x.1.clone()).unwrap_or_default(), &s) + 44.0,
            Ctl::Swatch(_, o, _) => o.len() as f32 * 32.0 + 80.0,
            Ctl::Button(_, l, _) => width(t, l, &st(13.0, 600.0)) + 28.0,
            Ctl::Value(v) => width(t, v, &s).min(320.0),
            Ctl::Go(_, v) => width(t, v, &s) + 22.0,
            Ctl::Chips(..) | Ctl::Place => 0.0,
        }
    }
    // the row's height (chips and the place search wrap under the label)
    fn row_h(&self, t: &mut crate::text::Text, r: &Row, iw: f32) -> f32 {
        let base = if r.sub.is_empty() { ROW_H } else { 56.0 };
        match &r.ctl {
            Ctl::Chips(_, opts, _) => {
                let (mut x, mut lines) = (16.0, 1);
                for (_, l) in opts {
                    let w = width(t, l, &st(13.0, 550.0)) + 40.0;
                    if x > 16.0 && x + w > iw - 16.0 {
                        x = 16.0;
                        lines += 1;
                    }
                    x += w + 8.0;
                }
                base - 4.0 + lines as f32 * 36.0
            }
            Ctl::Place if self.editing => base + 44.0 + if self.found.is_empty() { if self.asked.is_empty() || self.geo_rx.is_some() { 0.0 } else { 26.0 } } else { self.found.len().min(5) as f32 * 34.0 },
            _ => base,
        }
    }

    // ---------------------------------------------------------------- painting the content (body canvas)
    #[allow(clippy::too_many_arguments)]
    fn paint_row(&mut self, b: &mut Canvas, cx: &mut Ctx, bh: &mut Vec<(f32, f32, f32, f32, H)>, r: &Row, x0: f32, y: f32, iw: f32, h: f32) {
        let t = &mut *cx.t;
        let ls = st(13.5, 550.0);
        let ss = st(11.5, 500.0);
        let top_h = if r.sub.is_empty() { ROW_H } else { 56.0 };
        let mut lx = x0 + 16.0;
        if let Some((icon, col)) = r.icon {
            rect(b, lx, y + top_h / 2.0 - 14.0, 28.0, 28.0, 8.0, col);
            b.icon(t, lx + 14.0, y + top_h / 2.0, icon, 16.0, WHITE);
            lx += 40.0;
        }
        let cw = Self::ctl_w(t, &r.ctl);
        let lmax = (iw - (lx - x0) - cw - 32.0).max(80.0);
        let lh = line_h(t, &ls);
        if r.sub.is_empty() {
            text(b, t, lx, y + (top_h - lh) / 2.0, &r.label, &ls, FG, false, Some(lmax));
        } else {
            let sh = line_h(t, &ss);
            let y0 = y + (top_h - lh - sh - 1.0) / 2.0;
            text(b, t, lx, y0, &r.label, &ls, FG, false, Some(lmax));
            text(b, t, lx, y0 + lh + 1.0, &r.sub, &ss, MUTED, false, Some(lmax));
        }
        let cy = y + top_h / 2.0;
        let right = x0 + iw - 16.0;
        let vs = st(13.0, 550.0);
        match &r.ctl {
            Ctl::Toggle(id, on, onv, offv) => {
                let key = format!("{}.{}", id.0, id.1);
                self.toggle(b, right - 44.0, cy, *on, &key);
                bh.push((x0, y, iw, h, H::Set(*id, if *on { offv.clone() } else { onv.clone() })));
            }
            Ctl::Seg(id, opts, cur) => {
                let ws: Vec<f32> = opts.iter().map(|o| width(t, &o.1, &vs) + 26.0).collect();
                let tw: f32 = ws.iter().sum::<f32>() + 4.0;
                let mut sx = right - tw;
                rect(b, sx, cy - 15.0, tw, 30.0, 9.0, WHITE.alpha(0.07));
                sx += 2.0;
                for (i, (v, l)) in opts.iter().enumerate() {
                    let on = v == cur;
                    let hh = H::Set(*id, v.clone());
                    if on {
                        rect(b, sx, cy - 13.0, ws[i], 26.0, 7.0, WHITE.alpha(0.2));
                    } else if self.is_hot(&hh) {
                        rect(b, sx, cy - 13.0, ws[i], 26.0, 7.0, WHITE.alpha(0.07));
                    }
                    let lh = line_h(t, &vs);
                    text(b, t, sx + 13.0, cy - lh / 2.0, l, &vs, if on { FG } else { FG2 }, false, None);
                    bh.push((sx, cy - 15.0, ws[i], 30.0, hh));
                    sx += ws[i];
                }
            }
            Ctl::Menu(id, opts, cur) => {
                let lab = opts.iter().find(|o| &o.0 == cur).map(|o| o.1.clone()).unwrap_or_else(|| cur.as_str().unwrap_or("").to_string());
                let w = width(t, &lab, &vs) + 44.0;
                let bx = right - w;
                let hh = H::Menu(*id);
                let open = self.menu.as_ref().map(|m| m.id == *id).unwrap_or(false);
                rect(b, bx, cy - 15.0, w, 30.0, 9.0, WHITE.alpha(if open || self.is_hot(&hh) { 0.15 } else { 0.09 }));
                let lh = line_h(t, &vs);
                text(b, t, bx + 12.0, cy - lh / 2.0, &lab, &vs, FG, false, None);
                Self::chevrons(b, right - 16.0, cy, FG2);
                bh.push((bx, cy - 15.0, w, 30.0, hh));
            }
            Ctl::Chips(id, opts, on) => {
                let (mut x, mut yy) = (x0 + 16.0, y + top_h - 4.0);
                for (v, l) in opts {
                    let sel = on.contains(v);
                    let w = width(t, l, &vs) + 40.0;
                    if x > x0 + 16.0 && x + w > x0 + iw - 16.0 {
                        x = x0 + 16.0;
                        yy += 36.0;
                    }
                    let hh = H::Chip(*id, v.clone());
                    let hot = self.is_hot(&hh);
                    if sel {
                        rect(b, x, yy, w, 28.0, 14.0, ACC.alpha(if hot { 0.3 } else { 0.22 }));
                        border(b, x, yy, w, 28.0, 14.0, 1.0, ACC.alpha(0.55));
                        b.icon(t, x + 15.0, yy + 14.0, "check", 13.0, ACC);
                    } else {
                        rect(b, x, yy, w, 28.0, 14.0, WHITE.alpha(if hot { 0.11 } else { 0.06 }));
                        b.icon(t, x + 15.0, yy + 14.0, "plus", 12.0, MUTED);
                    }
                    let lh = line_h(t, &vs);
                    text(b, t, x + 26.0, yy + (28.0 - lh) / 2.0, l, &vs, if sel { FG } else { FG2 }, false, None);
                    bh.push((x, yy, w, 28.0, hh));
                    x += w + 8.0;
                }
            }
            Ctl::Swatch(id, opts, cur) => {
                let mut sx = right - opts.len() as f32 * 32.0 + 4.0;
                if let Some(o) = opts.iter().find(|o| &o.0 == cur) {
                    let nw = width(t, &o.2, &vs);
                    let lh = line_h(t, &vs);
                    text(b, t, sx - nw - 12.0, cy - lh / 2.0, &o.2, &vs, MUTED, false, None);
                }
                for (v, col, _) in opts {
                    let ccx = sx + 12.0;
                    if v == cur {
                        circle(b, ccx, cy, 14.5, WHITE.alpha(0.9));
                        circle(b, ccx, cy, 12.5, BG.alpha(1.0));
                    }
                    circle(b, ccx, cy, 11.0, *col);
                    bh.push((sx, cy - 14.0, 28.0, 28.0, H::Set(*id, v.clone())));
                    sx += 32.0;
                }
            }
            Ctl::Button(k, l, primary) => {
                let hh = H::Btn(k);
                let hot = self.is_hot(&hh);
                let w = Self::button(b, t, right - cw, cy, l, *primary, hot);
                bh.push((right - w, cy - 15.0, w, 30.0, hh));
            }
            Ctl::Value(v) => {
                let w = width(t, v, &vs).min(320.0);
                let lh = line_h(t, &vs);
                text(b, t, right - w, cy - lh / 2.0, v, &vs, MUTED, false, Some(320.0));
            }
            Ctl::Go(k, v) => {
                let hh = H::Go(k);
                if self.is_hot(&hh) {
                    rect(b, x0 + 2.0, y + 2.0, iw - 4.0, h - 4.0, 10.0, WHITE.alpha(0.04));
                }
                Self::chev_right(b, right - 4.0, cy, FAINT);
                let w = width(t, v, &vs);
                let lh = line_h(t, &vs);
                text(b, t, right - 18.0 - w, cy - lh / 2.0, v, &vs, MUTED, false, None);
                bh.push((x0, y, iw, h, hh));
            }
            Ctl::Place => {
                let cur = current_place();
                if !self.editing {
                    let hot = self.is_hot(&H::Field);
                    let w = Self::button(b, t, right - 74.0, cy, "Zmień", false, hot);
                    bh.push((right - w, cy - 15.0, w, 30.0, H::Field));
                    let v = if cur.is_empty() { "nie ustawione".to_string() } else { cur };
                    let vw = width(t, &v, &vs);
                    let lh = line_h(t, &vs);
                    text(b, t, right - 74.0 - 12.0 - vw, cy - lh / 2.0, &v, &vs, MUTED, false, None);
                } else {
                    let fy = y + top_h - 2.0;
                    let fw = iw - 32.0;
                    rect(b, x0 + 16.0, fy, fw, 34.0, 10.0, WHITE.alpha(0.08));
                    border(b, x0 + 16.0, fy, fw, 34.0, 10.0, 1.5, ACC.alpha(0.7));
                    b.icon(t, x0 + 32.0, fy + 17.0, "search", 14.0, MUTED);
                    let hint = if self.geo_rx.is_some() { "Szukam…" } else { "Wpisz miasto i naciśnij Enter" };
                    self.place.paint(b, t, x0 + 48.0, fy, fw - 44.0, 34.0, &st(13.5, 500.0), FG, hint, true, false);
                    bh.push((x0 + 16.0, fy, fw, 34.0, H::Field));
                    let mut ry = fy + 42.0;
                    if self.found.is_empty() && !self.asked.is_empty() && self.geo_rx.is_none() {
                        text(b, t, x0 + 20.0, ry, "Nic nie znaleziono", &st(12.5, 500.0), MUTED, false, None);
                    }
                    for (i, v) in self.found.iter().take(5).enumerate() {
                        let hh = H::Place(i);
                        if self.is_hot(&hh) {
                            rect(b, x0 + 12.0, ry, iw - 24.0, 30.0, 8.0, WHITE.alpha(0.08));
                        }
                        b.icon(t, x0 + 30.0, ry + 15.0, "pin", 13.0, ACC);
                        let lh = line_h(t, &vs);
                        text(b, t, x0 + 46.0, ry + (30.0 - lh) / 2.0, &place_label(v), &vs, FG, false, Some(iw - 80.0));
                        bh.push((x0 + 12.0, ry, iw - 24.0, 30.0, hh));
                        ry += 34.0;
                    }
                }
            }
        }
    }

    fn paint_group(&mut self, b: &mut Canvas, cx: &mut Ctx, bh: &mut Vec<(f32, f32, f32, f32, H)>, title: &str, rows: &[Row], foot: &str, x0: f32, mut y: f32, iw: f32) -> f32 {
        if !title.is_empty() {
            let cs = st(12.5, 650.0);
            text(b, cx.t, x0 + 4.0, y, title, &cs, MUTED, false, None);
            y += line_h(cx.t, &cs) + 8.0;
        }
        let hs: Vec<f32> = rows.iter().map(|r| self.row_h(cx.t, r, iw)).collect();
        let total: f32 = hs.iter().sum();
        if !rows.is_empty() {
            rect(b, x0, y, iw, total, 12.0, CARD);
            border(b, x0, y, iw, total, 12.0, 1.0, CARD_RIM);
        }
        let flash = self.flash.as_ref().filter(|f| f.1.elapsed().as_millis() < 1600).map(|f| f.0.clone());
        let mut ry = y;
        for (i, r) in rows.iter().enumerate() {
            if i > 0 {
                let inset = if r.icon.is_some() { 56.0 } else { 16.0 };
                rect(b, x0 + inset, ry, iw - inset, 1.0, 0.0, HAIR);
            }
            if flash.as_deref() == Some(r.label.as_str()) {
                let k = 1.0 - self.flash.as_ref().unwrap().1.elapsed().as_millis() as f32 / 1600.0;
                rect(b, x0 + 2.0, ry + 2.0, iw - 4.0, hs[i] - 4.0, 10.0, ACC.alpha(0.18 * k));
            }
            self.paint_row(b, cx, bh, r, x0, ry, iw, hs[i]);
            ry += hs[i];
        }
        y = ry;
        if !foot.is_empty() {
            let fs = st(11.5, 500.0);
            let lines = crate::gfx::wrap(cx.t, foot, &fs, iw - 8.0, 3);
            let lh = line_h(cx.t, &fs);
            y += 8.0;
            for l in lines {
                text(b, cx.t, x0 + 4.0, y, &l, &fs, FAINT, false, None);
                y += lh;
            }
        }
        y + 26.0
    }

    // a wallpaper's picture: its thumbnail, else its colours as a soft gradient
    fn wall_pic(b: &mut Canvas, cx: &mut Ctx, key: &str, cols: &[String], x: f32, y: f32, w: f32, h: f32, r: f32) {
        let tp = crate::presets::thumb_path(key);
        let photo = key.strip_prefix("photo:").map(String::from);
        let src = if std::path::Path::new(&tp).exists() { Some(tp) } else { photo };
        if let Some(pm) = src.and_then(|s| cx.imgs.get(&s, 360, cx.icons)) {
            crate::img::draw(b, &pm, x, y, w, h, r, true, 1.0);
            return;
        }
        let c = |i: usize| cols.get(i).map(|s| { let v = crate::presets::hex(s); Rgba(v[0], v[1], v[2], 1.0) }).unwrap_or(Rgba(0.1, 0.12, 0.18, 1.0));
        if let Some(p) = crate::gfx::rrect_path(x, y, w, h, r) {
            crate::desktop::fill_lin(b, &p, (x, y), (x + w * 0.6, y + h), &[(0.0, c(1)), (0.55, c(0)), (1.0, c(0))]);
            crate::desktop::fill_rad(b, &p, x + w * 0.7, y + h * 0.35, w * 0.55, &[(0.0, c(3).alpha(0.55)), (0.5, c(2).alpha(0.25)), (1.0, c(2).alpha(0.0))]);
        }
    }

    fn paint_block(&mut self, b: &mut Canvas, cx: &mut Ctx, bh: &mut Vec<(f32, f32, f32, f32, H)>, blk: &Block, x0: f32, mut y: f32, iw: f32, view: (f32, f32)) -> f32 {
        match blk {
            Block::Group(title, rows, foot) => self.paint_group(b, cx, bh, title, rows, foot, x0, y, iw),
            Block::Welcome => {
                let h = 92.0;
                if let Some(p) = crate::gfx::rrect_path(x0, y, iw, h, 14.0) {
                    crate::desktop::fill_lin(b, &p, (x0, y), (x0 + iw, y + h), &[(0.0, ACC.alpha(0.28)), (1.0, Rgba::rgb8(0x5e, 0x5c, 0xe6, 0.22))]);
                }
                border(b, x0, y, iw, h, 14.0, 1.0, WHITE.alpha(0.1));
                b.icon(cx.t, x0 + 40.0, y + h / 2.0, "sparkles", 26.0, WHITE);
                text(b, cx.t, x0 + 72.0, y + 22.0, "Witaj w Mist", &st(18.0, 700.0), FG, false, None);
                text(b, cx.t, x0 + 72.0, y + 48.0, "Wybierz język i miejsce pogody. Ustawienia otworzysz prawym klikiem na pulpicie.", &st(12.5, 500.0), FG2, false, Some(iw - 90.0));
                y + h + 26.0
            }
            Block::WallHero => {
                let key = self.wall_key();
                let items = self.wall_items("all");
                let it = items.iter().find(|i| i.0 == key).cloned().unwrap_or((key.clone(), "Tapeta".into(), String::new(), String::new(), Vec::new()));
                let (pw, ph) = (300.0, 169.0);
                let mut sh = Canvas::new((pw + 60.0) as u32, (ph + 60.0) as u32);
                rect(&mut sh, 30.0, 30.0, pw, ph, 14.0, Rgba(0.0, 0.0, 0.0, 1.0));
                let s = crate::desktop::drop_shadow(&sh.pm, 12.0, Rgba(0.0, 0.0, 0.0, 1.0), 0.45, 0, 6);
                b.pm.draw_pixmap((x0 - 30.0) as i32, (y - 30.0) as i32, s.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
                Self::wall_pic(b, cx, &it.0, &it.4, x0, y, pw, ph, 14.0);
                border(b, x0, y, pw, ph, 14.0, 1.0, WHITE.alpha(0.12));
                let tx = x0 + pw + 24.0;
                let tw = iw - pw - 24.0;
                let catn = WCATS.iter().find(|c| c.0 == it.3).map(|c| c.1).unwrap_or("Zdjęcie");
                text(b, cx.t, tx, y + 6.0, &catn.to_uppercase(), &st(11.0, 700.0).spaced(1.0), ACC, false, None);
                text(b, cx.t, tx, y + 24.0, &it.1, &st(20.0, 700.0), FG, false, Some(tw));
                let ds = st(12.5, 500.0);
                let lh = line_h(cx.t, &ds);
                for (i, l) in crate::gfx::wrap(cx.t, &it.2, &ds, tw, 3).iter().enumerate() {
                    text(b, cx.t, tx, y + 56.0 + i as f32 * lh, l, &ds, FG2, false, None);
                }
                let hot = self.is_hot(&H::Btn("gallery"));
                let w = Self::button(b, cx.t, tx, y + ph - 16.0, "Galeria z podglądem", true, hot);
                bh.push((tx, y + ph - 31.0, w, 30.0, H::Btn("gallery")));
                y + ph + 30.0
            }
            Block::WallGrid => {
                // categories (a segmented row), then the thumbnails, 4 per row
                let vs = st(13.0, 550.0);
                let ws: Vec<f32> = WCATS.iter().map(|c| width(cx.t, c.1, &vs) + 26.0).collect();
                let tw: f32 = ws.iter().sum::<f32>() + 4.0;
                let mut sx = x0;
                text(b, cx.t, x0 + 4.0, y, "Wszystkie tapety", &st(12.5, 650.0), MUTED, false, None);
                y += 26.0;
                rect(b, sx, y, tw, 30.0, 9.0, WHITE.alpha(0.07));
                sx += 2.0;
                for (i, (_, l)) in WCATS.iter().enumerate() {
                    let on = i == self.wcat;
                    let hh = H::WallCat(i);
                    if on {
                        rect(b, sx, y + 2.0, ws[i], 26.0, 7.0, WHITE.alpha(0.2));
                    } else if self.is_hot(&hh) {
                        rect(b, sx, y + 2.0, ws[i], 26.0, 7.0, WHITE.alpha(0.07));
                    }
                    let lh = line_h(cx.t, &vs);
                    text(b, cx.t, sx + 13.0, y + 15.0 - lh / 2.0, l, &vs, if on { FG } else { FG2 }, false, None);
                    bh.push((sx, y, ws[i], 30.0, hh));
                    sx += ws[i];
                }
                y += 46.0;
                let items = self.wall_items(WCATS[self.wcat].0);
                let cur = self.wall_key();
                let cols = 4;
                let gap = 14.0;
                let w = ((iw - gap * (cols as f32 - 1.0)) / cols as f32).floor();
                let h = (w * 9.0 / 16.0).round();
                let ns = st(12.0, 550.0);
                let cell_h = h + 30.0;
                for (i, it) in items.iter().enumerate() {
                    let (col, rw) = (i % cols, i / cols);
                    let (tx, ty) = (x0 + col as f32 * (w + gap), y + rw as f32 * cell_h);
                    if ty + cell_h < view.0 || ty > view.1 {
                        continue; // not decoded while off screen
                    }
                    let hh = H::Wall(it.0.clone());
                    Self::wall_pic(b, cx, &it.0, &it.4, tx, ty, w, h, 10.0);
                    if it.0 == cur {
                        border(b, tx - 3.0, ty - 3.0, w + 6.0, h + 6.0, 13.0, 2.5, ACC);
                    } else if self.is_hot(&hh) {
                        border(b, tx - 2.0, ty - 2.0, w + 4.0, h + 4.0, 12.0, 2.0, WHITE.alpha(0.5));
                    } else {
                        border(b, tx, ty, w, h, 10.0, 1.0, WHITE.alpha(0.1));
                    }
                    text(b, cx.t, tx + 2.0, ty + h + 7.0, &it.1, &ns, if it.0 == cur { FG } else { FG2 }, false, Some(w - 4.0));
                    bh.push((tx, ty, w, h + 26.0, hh));
                }
                y + items.len().div_ceil(cols) as f32 * cell_h + 20.0
            }
            Block::DeskPreview => {
                // the desktop in small: the wallpaper, the date + clock as set, the cards in their places
                let pw = iw;
                let ph = (pw * 0.46).round();
                let key = self.wall_key();
                let items = self.wall_items("all");
                let cols = items.iter().find(|i| i.0 == key).map(|i| i.4.clone()).unwrap_or_default();
                Self::wall_pic(b, cx, &key, &cols, x0, y, pw, ph, 14.0);
                if let Some(p) = crate::gfx::rrect_path(x0, y, pw, ph, 14.0) {
                    crate::desktop::fill_lin(b, &p, (x0, y), (x0, y + ph), &[(0.0, Rgba(0.0, 0.0, 0.0, 0.1)), (1.0, Rgba(0.0, 0.0, 0.0, 0.35))]);
                }
                border(b, x0, y, pw, ph, 14.0, 1.0, WHITE.alpha(0.12));
                // one scale for all: the date, the clock and the cards fit the preview's height
                let k = (pw / 1920.0 * 1.6).min((ph - 52.0) / 600.0);
                let mid = x0 + pw / 2.0;
                let mut ty = y + 26.0;
                if self.cfg.s("desk", "date") != "hide" {
                    let ds = st((22.0 * k).max(9.0), 600.0);
                    const DAYS: [&str; 7] = ["Niedziela", "Poniedziałek", "Wtorek", "Środa", "Czwartek", "Piątek", "Sobota"];
                    let tm = crate::data::local(crate::data::now());
                    let s = format!("{}, {} {}", DAYS[tm.tm_wday as usize], tm.tm_mday, crate::data::MONTHS_GEN[tm.tm_mon as usize]);
                    let s = s.as_str();
                    let w = width(cx.t, s, &ds);
                    text(b, cx.t, mid - w / 2.0, ty, s, &ds, WHITE.alpha(0.8), true, None);
                    ty += line_h(cx.t, &ds) - 2.0;
                }
                let csz = match self.cfg.s("desk", "clock").as_str() { "small" => 96.0, "hide" => 0.0, _ => 150.0 } * k;
                if csz > 0.0 {
                    let wght = match self.cfg.s("desk", "clockw").as_str() { "thin" => 250.0, "bold" => 800.0, _ => 620.0 };
                    let col = match self.cfg.s("desk", "clockc").as_str() { "accent" => ACC, "warm" => Rgba::rgb8(0xf6, 0xd9, 0xb0, 1.0), _ => WHITE.alpha(0.9) };
                    let cs = st(csz, wght).spaced(-4.0 * k).tnum();
                    let s = crate::data::hm(crate::data::now());
                    let w = width(cx.t, &s, &cs);
                    text(b, cx.t, mid - w / 2.0, ty - csz * 0.12, &s, &cs, col, true, None);
                    ty += line_h(cx.t, &cs) - csz * 0.25;
                }
                if self.cfg.s("desk", "sun") != "hide" {
                    rect(b, mid - 40.0, ty + 4.0, 80.0, 1.0, 0.0, WHITE.alpha(0.3));
                    circle(b, mid - 10.0, ty + 4.5, 2.5, WHITE);
                }
                // the cards
                let shown = self.cfg.shown();
                let ch = (312.0 * k).round();
                let unit = 308.0 * k;
                let gap = 16.0 * k;
                let wfor = |s: &str| if s == "weather" { 548.0 * k } else { unit };
                let row_w: f32 = shown.iter().map(|s| wfor(s)).sum::<f32>() + gap * (shown.len() as f32 - 1.0).max(0.0);
                let mut sx = mid - row_w / 2.0;
                let sy = (ty + 22.0 * k + 10.0).max(y + ph - ch - 22.0).min(y + ph - ch - 14.0);
                if shown.is_empty() {
                    let s = "Bez kart";
                    let w = width(cx.t, s, &st(12.0, 550.0));
                    text(b, cx.t, mid - w / 2.0, y + ph - 46.0, s, &st(12.0, 550.0), WHITE.alpha(0.6), true, None);
                }
                for (i, s) in shown.iter().enumerate() {
                    let w = wfor(s);
                    let hh = H::Slot(i);
                    let open = self.menu.as_ref().map(|m| m.id == ("slot", SLOTS[i])).unwrap_or(false);
                    let hot = self.is_hot(&hh) || open;
                    rect(b, sx, sy, w, ch, 12.0, Rgba(0.04, 0.06, 0.1, if hot { 0.55 } else { 0.42 }));
                    border(b, sx, sy, w, ch, 12.0, if hot { 2.0 } else { 1.0 }, if hot { ACC } else { WHITE.alpha(0.22) });
                    let icx = sx + w / 2.0;
                    circle(b, icx, sy + ch / 2.0 - 12.0, 18.0, widget_color(s).alpha(0.9));
                    b.icon(cx.t, icx, sy + ch / 2.0 - 12.0, widget_icon(s), 18.0, WHITE);
                    let ns = st(12.5, 650.0);
                    let nw = width(cx.t, wname(s), &ns);
                    text(b, cx.t, icx - nw / 2.0, sy + ch / 2.0 + 14.0, wname(s), &ns, FG, true, None);
                    let hs = st(10.5, 550.0);
                    let hint = if hot { "Zmień" } else { "" };
                    let hw = width(cx.t, hint, &hs);
                    text(b, cx.t, icx - hw / 2.0, sy + ch / 2.0 + 32.0, hint, &hs, ACC, true, None);
                    bh.push((sx, sy, w, ch, hh));
                    sx += w + gap;
                }
                let ny = y + ph + 10.0;
                text(b, cx.t, x0 + 4.0, ny, "Kliknij kartę na podglądzie, żeby postawić tam inny widżet.", &st(11.5, 500.0), FAINT, false, None);
                ny + 40.0
            }
            Block::AboutHero => {
                let h = 120.0;
                if let Some(p) = crate::gfx::rrect_path(x0, y, iw, h, 14.0) {
                    crate::desktop::fill_lin(b, &p, (x0, y), (x0 + iw, y + h), &[(0.0, Rgba::rgb8(0x10, 0x3a, 0x48, 1.0)), (0.6, Rgba::rgb8(0x1a, 0x24, 0x48, 1.0)), (1.0, Rgba::rgb8(0x2c, 0x1c, 0x48, 1.0))]);
                    crate::desktop::fill_rad(b, &p, x0 + iw * 0.15, y + h * 0.2, iw * 0.4, &[(0.0, ACC.alpha(0.35)), (1.0, ACC.alpha(0.0))]);
                }
                border(b, x0, y, iw, h, 14.0, 1.0, WHITE.alpha(0.1));
                text(b, cx.t, x0 + 28.0, y + 22.0, "Mist", &st(40.0, 700.0).spaced(-1.0), WHITE, false, None);
                text(b, cx.t, x0 + 30.0, y + 76.0, "Natywna powłoka pulpitu w Rust", &st(13.0, 550.0), FG2, false, None);
                let v = format!("wersja {}", self.version);
                let vs = st(12.0, 600.0);
                let vw = width(cx.t, &v, &vs);
                rect(b, x0 + iw - vw - 52.0, y + 24.0, vw + 24.0, 26.0, 13.0, WHITE.alpha(0.12));
                let vh = line_h(cx.t, &vs);
                text(b, cx.t, x0 + iw - vw - 40.0, y + 37.0 - vh / 2.0, &v, &vs, FG, false, None);
                y + h + 26.0
            }
        }
    }

    // ---------------------------------------------------------------- the window frame (cached: its shadow is a blur)
    fn frame(&mut self, ww: f32, wh: f32) -> &Pixmap {
        let key = (ww as u32, wh as u32);
        if self.frame.as_ref().map(|f| f.0 != key).unwrap_or(true) {
            let (lw, lh) = ((ww + 2.0 * M) as u32, (wh + 2.0 * M) as u32);
            let mut l = Canvas::new(lw, lh);
            let mut s = Canvas::new(lw, lh);
            rect(&mut s, M, M, ww, wh, RAD, Rgba(0.0, 0.0, 0.0, 1.0));
            let sh = crate::desktop::drop_shadow(&s.pm, 22.0, Rgba(0.0, 0.0, 0.0, 1.0), 0.55, 0, 14);
            l.pm.draw_pixmap(0, 0, sh.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            rect(&mut l, M, M, ww, wh, RAD, BG);
            // the sidebar: a lighter panel with its own rounded inset
            if let Some(p) = crate::gfx::rrect_path(M, M, SIDE, wh, RAD) {
                let mut mask = tiny_skia::Mask::new(lw, lh).unwrap();
                mask.fill_path(&p, tiny_skia::FillRule::Winding, true, Transform::identity());
                let paint = tiny_skia::Paint { shader: tiny_skia::Shader::SolidColor(crate::desktop::skia(SIDE_BG)), anti_alias: true, ..Default::default() };
                l.pm.fill_rect(tiny_skia::Rect::from_xywh(M, M, SIDE, wh).unwrap(), &paint, Transform::identity(), Some(&mask));
            }
            rect(&mut l, M + SIDE, M, 1.0, wh, 0.0, WHITE.alpha(0.07));
            border(&mut l, M, M, ww, wh, RAD, 1.0, WHITE.alpha(0.11));
            self.frame = Some((key, l.pm));
        }
        &self.frame.as_ref().unwrap().1
    }
}

impl Pane for Settings {
    fn name(&self) -> &'static str {
        "settings"
    }
    fn animating(&self) -> bool {
        self.o.running() || self.po.running() || self.fx.running() || self.flash.as_ref().map(|f| f.1.elapsed().as_millis() < 1600).unwrap_or(false)
    }
    fn data(&mut self, what: &str) -> bool {
        what == "dnd"
    }
    fn slow(&mut self) -> bool {
        let Some(rx) = &self.geo_rx else { return false };
        match rx.try_recv() {
            Ok(v) => {
                self.found = v;
                self.geo_rx = None;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.geo_rx = None;
                true
            }
            Err(_) => false,
        }
    }
    // `--pane-png settings:<page> … <query>`: the search typed
    fn seed(&mut self, q: &str) {
        self.search.set(q);
        self.searching = true;
        self.run_search(false);
    }
    fn set_page(&mut self, p: &str) -> bool {
        let to = Self::page_of(p);
        if to == self.page {
            return false;
        }
        self.go(to);
        true
    }
    fn hand(&self) -> bool {
        !matches!(self.hit(), None | Some(H::Win))
    }
    fn paint(&mut self, c: &mut Canvas, cx: &mut Ctx) {
        c.clear();
        self.hot = self.hit();
        self.hits.clear();
        let o = self.o.value();
        rect(c, 0.0, 0.0, c.w(), c.h(), 0.0, Rgba(4.0 / 255.0, 7.0 / 255.0, 14.0 / 255.0, 0.45 * o));
        let ww = WIN_W.min(c.w() - 32.0);
        let wh = WIN_H.min(c.h() - 32.0);
        let (wx, wy) = (((c.w() - ww) / 2.0).round(), ((c.h() - wh) / 2.0).round());
        self.lo = (wx - M, wy - M);
        let (lox, loy) = self.lo;
        let mut l = Canvas::new((ww + 2.0 * M) as u32, (wh + 2.0 * M) as u32);
        {
            let f = self.frame(ww, wh).clone();
            l.pm.draw_pixmap(0, 0, f.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        let mut hits: Vec<(f32, f32, f32, f32, H)> = Vec::new();
        hits.push((wx, wy, ww, wh, H::Win));
        let t = &mut *cx.t;

        // ---- sidebar: search, the sections
        let (sx, sy) = (M + 14.0, M + 18.0);
        let sw = SIDE - 28.0;
        rect(&mut l, sx, sy, sw, 32.0, 9.0, WHITE.alpha(if self.searching { 0.1 } else { 0.065 }));
        if self.searching {
            border(&mut l, sx, sy, sw, 32.0, 9.0, 1.5, ACC.alpha(0.6));
        }
        l.icon(t, sx + 16.0, sy + 16.0, "search", 14.0, MUTED);
        self.search.paint(&mut l, t, sx + 30.0, sy, sw - 38.0, 32.0, &st(13.0, 500.0), FG, "Szukaj", self.searching, false);
        hits.push((lox + sx, loy + sy, sw, 32.0, H::Search));
        let mut iy = sy + 48.0;
        let ls = st(13.5, 550.0);
        let lh = line_h(t, &ls);
        let cur = if self.search.text.trim().is_empty() { Some(self.nav_index()) } else { None };
        for (i, (_, name, icon, col)) in PAGES.iter().enumerate() {
            if GAP_BEFORE.contains(&i) {
                iy += 12.0;
            }
            let hh = H::Nav(i);
            let (ix, iw) = (M + 10.0, SIDE - 20.0);
            if cur == Some(i) {
                rect(&mut l, ix, iy, iw, 34.0, 9.0, WHITE.alpha(0.12));
            } else if self.is_hot(&hh) {
                rect(&mut l, ix, iy, iw, 34.0, 9.0, WHITE.alpha(0.05));
            }
            rect(&mut l, ix + 8.0, iy + 5.0, 24.0, 24.0, 7.0, Rgba::rgb8(col.0, col.1, col.2, 1.0));
            l.icon(t, ix + 20.0, iy + 17.0, icon, 14.0, WHITE);
            text(&mut l, t, ix + 42.0, iy + (34.0 - lh) / 2.0, name, &ls, if cur == Some(i) { FG } else { FG2 }, false, Some(iw - 50.0));
            hits.push((lox + ix, loy + iy, iw, 34.0, hh));
            iy += 36.0;
        }

        // ---- content header: (back) title, close
        let cx0 = M + SIDE;
        let cw = ww - SIDE;
        let mut tx = cx0 + PADX;
        let searching = !self.search.text.trim().is_empty();
        if matches!(self.page, Page::Widget(_)) && !searching {
            let hot = self.is_hot(&H::Back);
            rect(&mut l, cx0 + 20.0, M + 19.0, 30.0, 30.0, 9.0, WHITE.alpha(if hot { 0.14 } else { 0.07 }));
            l.icon(t, cx0 + 35.0, M + 34.0, "chevLeft", 16.0, FG);
            hits.push((lox + cx0 + 20.0, loy + M + 19.0, 30.0, 30.0, H::Back));
            tx = cx0 + 62.0;
        }
        let title = if searching { "Wyniki wyszukiwania".to_string() } else { self.title() };
        let ts = st(21.0, 700.0);
        let th = line_h(t, &ts);
        text(&mut l, t, tx, M + 34.0 - th / 2.0, &title, &ts, FG, false, None);
        {
            let hot = self.is_hot(&H::Close);
            let (bx, by) = (M + ww - 48.0, M + 19.0);
            circle(&mut l, bx + 15.0, by + 15.0, 15.0, WHITE.alpha(if hot { 0.16 } else { 0.07 }));
            l.icon(t, bx + 15.0, by + 15.0, "x", 15.0, if hot { FG } else { FG2 });
            hits.push((lox + bx, loy + by, 30.0, 30.0, H::Close));
        }

        // ---- the body: blocks, scrolled under the header
        let view_h = wh - HEAD;
        let iw = cw - 2.0 * PADX;
        let mut b = Canvas::new(cw as u32, 4200);
        let mut bh: Vec<(f32, f32, f32, f32, H)> = Vec::new();
        let mut y = 8.0;
        let dnd = cx.st.dnd;
        if searching {
            if self.results.is_empty() {
                text(&mut b, cx.t, PADX, y + 20.0, "Nic nie pasuje", &st(15.0, 600.0), FG2, false, None);
                y += 60.0;
            } else {
                let rows: Vec<Row> = self.results.iter().map(|(p, lab)| {
                    let (pn, icon, col) = match p {
                        Page::Nav(i) => (PAGES[*i].1, PAGES[*i].2, Rgba::rgb8(PAGES[*i].3 .0, PAGES[*i].3 .1, PAGES[*i].3 .2, 1.0)),
                        Page::Widget(k) => (wname(k), widget_icon(k), widget_color(k)),
                    };
                    Row { label: if lab.is_empty() { pn.to_string() } else { lab.clone() }, sub: if lab.is_empty() { String::new() } else { pn.to_string() }, icon: Some((icon, col)), ctl: Ctl::Value(String::new()) }
                }).collect();
                let y0 = y;
                y = self.paint_group(&mut b, cx, &mut bh, "", &rows, "", PADX, y, iw);
                // every result row opens its page
                let mut ry = y0;
                for (i, r) in rows.iter().enumerate() {
                    let h = self.row_h(cx.t, r, iw);
                    bh.push((PADX, ry, iw, h, H::Result(i)));
                    if self.is_hot(&H::Result(i)) {
                        rect(&mut b, PADX + 2.0, ry + 2.0, iw - 4.0, h - 4.0, 10.0, WHITE.alpha(0.05));
                    }
                    Self::chev_right(&mut b, PADX + iw - 20.0, ry + h / 2.0, FAINT);
                    ry += h;
                }
            }
        } else {
            let view = (self.scroll - 40.0, self.scroll + view_h + 40.0);
            for blk in self.blocks(self.page, dnd) {
                y = self.paint_block(&mut b, cx, &mut bh, &blk, PADX, y, iw, view);
            }
        }
        let content_h = y + 8.0;
        self.max_scroll = (content_h - view_h).max(0.0);
        self.scroll = self.scroll.clamp(0.0, self.max_scroll);
        let po = self.po.value();
        if let Some(p) = crate::gfx::rrect_path(M, M, ww, wh, RAD) {
            let (lw, lhh) = (l.pm.width() as usize, l.pm.height() as usize);
            let mut mask = tiny_skia::Mask::new(lw as u32, lhh as u32).unwrap();
            mask.fill_path(&p, tiny_skia::FillRule::Winding, true, Transform::identity());
            let top = (M + HEAD) as usize;
            let left = (M + SIDE + 1.0) as usize;
            let d = mask.data_mut();
            for r in 0..lhh {
                if r < top {
                    d[r * lw..(r + 1) * lw].fill(0);
                } else {
                    d[r * lw..r * lw + left.min(lw)].fill(0);
                }
            }
            let paint = PixmapPaint { opacity: po, ..Default::default() };
            l.pm.draw_pixmap(cx0 as i32, (M + HEAD - self.scroll + (1.0 - po) * 8.0) as i32, b.pm.as_ref(), &paint, Transform::identity(), Some(&mask));
        }
        rect(&mut l, cx0, M + HEAD - 1.0, cw, 1.0, 0.0, if self.scroll > 1.0 { WHITE.alpha(0.1) } else { Rgba(0.0, 0.0, 0.0, 0.0) });
        if self.max_scroll > 0.0 {
            let track = view_h - 20.0;
            let bar = (track * view_h / content_h).max(32.0);
            let by = M + HEAD + 10.0 + (track - bar) * (self.scroll / self.max_scroll);
            rect(&mut l, M + ww - 9.0, by, 4.0, bar, 2.0, WHITE.alpha(0.22));
        }
        for (x, y, w, h, hh) in bh {
            let ry = M + HEAD + y - self.scroll;
            if ry + h > M + HEAD && ry < M + wh {
                hits.push((lox + cx0 + x, loy + ry, w, h, hh));
            }
        }

        // ---- an open pop-up menu, over everything
        if let Some(m) = &mut self.menu {
            let t = &mut *cx.t;
            let ms = st(13.0, 550.0);
            let mw = m.items.iter().map(|i| width(t, &i.1, &ms)).fold(0.0f32, f32::max).max(m.rect.2 - 34.0) + 54.0;
            let ih = 30.0;
            let mh = m.items.len() as f32 * ih + 10.0;
            let mut my = m.rect.1 + m.rect.3 + 6.0;
            if my + mh > M + wh - 8.0 {
                my = (m.rect.1 - mh - 6.0).max(M + 8.0);
            }
            let mx = (m.rect.0 + m.rect.2 - mw).max(M + 8.0);
            let mut s = Canvas::new((mw + 60.0) as u32, (mh + 60.0) as u32);
            rect(&mut s, 30.0, 30.0, mw, mh, 11.0, Rgba(0.0, 0.0, 0.0, 1.0));
            let sh = crate::desktop::drop_shadow(&s.pm, 14.0, Rgba(0.0, 0.0, 0.0, 1.0), 0.5, 0, 8);
            l.pm.draw_pixmap((mx - 30.0) as i32, (my - 30.0) as i32, sh.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            rect(&mut l, mx, my, mw, mh, 11.0, Rgba(36.0 / 255.0, 39.0 / 255.0, 48.0 / 255.0, 0.99));
            border(&mut l, mx, my, mw, mh, 11.0, 1.0, WHITE.alpha(0.12));
            hits.push((lox + mx, loy + my, mw, mh, H::Win));
            let p = self.ptr;
            for (i, (v, lab)) in m.items.iter().enumerate() {
                let iy = my + 5.0 + i as f32 * ih;
                let over = p.0 >= lox + mx && p.0 < lox + mx + mw && p.1 >= loy + iy && p.1 < loy + iy + ih;
                if over {
                    m.sel = i;
                }
                if i == m.sel {
                    rect(&mut l, mx + 5.0, iy, mw - 10.0, ih, 7.0, ACC.alpha(0.28));
                }
                if *v == m.cur {
                    l.icon(t, mx + 19.0, iy + ih / 2.0, "check", 13.0, FG);
                }
                let mh2 = line_h(t, &ms);
                text(&mut l, t, mx + 34.0, iy + (ih - mh2) / 2.0, lab, &ms, FG, false, None);
                hits.push((lox + mx, loy + iy, mw, ih, H::MenuItem(i)));
            }
        }
        self.hits = hits;
        let paint = PixmapPaint { opacity: o, ..Default::default() };
        c.pm.draw_pixmap(lox as i32, (loy + (1.0 - o) * 14.0) as i32, l.pm.as_ref(), &paint, Transform::identity(), None);
    }

    fn pointer(&mut self, ev: Ptr, x: f32, y: f32, cx: &mut Ctx) -> Act {
        let before = self.hit();
        self.ptr = (x, y);
        match ev {
            Ptr::Wheel(d) => {
                if self.menu.is_some() {
                    return Act::None;
                }
                let s = (self.scroll + d * 52.0).clamp(0.0, self.max_scroll);
                if s != self.scroll {
                    self.scroll = s;
                    return Act::Redraw;
                }
                Act::None
            }
            Ptr::Release(0x110) => {
                let h = self.hit();
                // a click anywhere closes an open menu (an item picks first)
                if let Some(m) = self.menu.take() {
                    if let Some(H::MenuItem(i)) = h {
                        if let Some((v, _)) = m.items.get(i).cloned() {
                            return self.set(m.id, v);
                        }
                    }
                    return Act::Redraw;
                }
                if !matches!(h, Some(H::Search)) {
                    self.searching = false;
                }
                match h {
                    None | Some(H::Close) => Act::Close,
                    Some(H::Win) | Some(H::MenuItem(_)) => Act::Redraw,
                    Some(H::Nav(i)) => {
                        self.search.set("");
                        self.results.clear();
                        self.go(Page::Nav(i));
                        Act::Redraw
                    }
                    Some(H::Search) => {
                        self.searching = true;
                        self.editing = false;
                        Act::Redraw
                    }
                    Some(H::Result(i)) => {
                        if let Some((p, lab)) = self.results.get(i).cloned() {
                            self.search.set("");
                            self.results.clear();
                            self.go(p);
                            if !lab.is_empty() {
                                self.flash = Some((lab, std::time::Instant::now()));
                            }
                        }
                        Act::Redraw
                    }
                    Some(H::Back) => {
                        self.go(Page::Nav(3));
                        Act::Redraw
                    }
                    Some(H::Set(id, v)) => self.set(id, v),
                    Some(H::Chip(id, v)) => self.chip(id, &v),
                    Some(H::Menu(id)) => {
                        // its items again from the page, its button from the hits
                        let r = self.hits.iter().rev().find(|r| r.4 == H::Menu(id)).map(|r| (r.0 - self.lo.0, r.1 - self.lo.1, r.2, r.3));
                        for blk in self.blocks(self.page, cx.st.dnd) {
                            if let Block::Group(_, rows, _) = blk {
                                for rw in rows {
                                    if let Ctl::Menu(mid, items, cur) = rw.ctl {
                                        if mid == id {
                                            let sel = items.iter().position(|i| i.0 == cur).unwrap_or(0);
                                            self.menu = Some(Menu { id, rect: r.unwrap_or_default(), items, cur, sel });
                                        }
                                    }
                                }
                            }
                        }
                        Act::Redraw
                    }
                    Some(H::Slot(i)) => {
                        let r = self.hits.iter().rev().find(|r| r.4 == H::Slot(i)).map(|r| (r.0 - self.lo.0, r.1 - self.lo.1, r.2, r.3)).unwrap_or_default();
                        let items: Vec<(Value, String)> = WIDGETS.iter().filter(|w| crate::data::available(w.0)).map(|w| (json!(w.0), w.1.to_string())).collect();
                        let cur = json!(self.cfg.shown().get(i).cloned().unwrap_or_default());
                        let sel = items.iter().position(|x| x.0 == cur).unwrap_or(0);
                        self.menu = Some(Menu { id: ("slot", SLOTS[i.min(2)]), rect: r, items, cur, sel });
                        Act::Redraw
                    }
                    Some(H::Go(k)) => {
                        self.go(Self::page_of(k));
                        Act::Redraw
                    }
                    Some(H::Wall(k)) => {
                        self.set_wall(&k);
                        Act::Redraw
                    }
                    Some(H::WallCat(i)) => {
                        self.wcat = i;
                        Act::Redraw
                    }
                    Some(H::Btn(k)) => match k {
                        "gallery" => Act::Toggle("wallpaper"),
                        "history" => Act::Toggle("notifications"),
                        "lock" => Act::Run(vec![crate::paths::self_bin(), "lock".into()]),
                        "restart" => Act::Run(vec!["pkill".into(), "-x".into(), "mist-ui-bin".into()]),
                        "source" => Act::Run(vec!["xdg-open".into(), "https://github.com/wiktor-jozwiak/mist".into()]),
                        "sponsor" => Act::Run(vec!["xdg-open".into(), "https://github.com/sponsors/wiktor-jozwiak".into()]),
                        "about" => {
                            self.load_about();
                            Act::Redraw
                        }
                        _ => Act::None,
                    },
                    Some(H::Field) => {
                        self.editing = true;
                        self.searching = false;
                        Act::Redraw
                    }
                    Some(H::Place(i)) => {
                        self.choose_place(i);
                        Act::DeskCfg
                    }
                }
            }
            Ptr::Motion | Ptr::Enter | Ptr::Leave => {
                if self.hit() != before || self.menu.is_some() { Act::Redraw } else { Act::None }
            }
            _ => Act::None,
        }
    }

    fn key(&mut self, ev: &KeyEvent, cx: &mut Ctx) -> Act {
        // an open menu: ↑↓ Enter Esc
        if let Some(m) = &mut self.menu {
            match ev.keysym {
                Keysym::Escape => {
                    self.menu = None;
                    return Act::Redraw;
                }
                Keysym::Down => {
                    m.sel = (m.sel + 1).min(m.items.len().saturating_sub(1));
                    return Act::Redraw;
                }
                Keysym::Up => {
                    m.sel = m.sel.saturating_sub(1);
                    return Act::Redraw;
                }
                Keysym::Return | Keysym::KP_Enter | Keysym::space => {
                    let m = self.menu.take().unwrap();
                    if let Some((v, _)) = m.items.get(m.sel).cloned() {
                        return self.set(m.id, v);
                    }
                    return Act::Redraw;
                }
                _ => return Act::None,
            }
        }
        // typing a weather place
        if self.editing {
            return match self.place.key(ev.keysym, ev.utf8.as_deref(), cx.ctrl, false) {
                crate::edit::EditResult::Accept => {
                    if self.found.len() == 1 && self.asked == self.place.text.trim() {
                        self.choose_place(0);
                        Act::DeskCfg
                    } else {
                        self.search_geo();
                        Act::Redraw
                    }
                }
                crate::edit::EditResult::Cancel => {
                    self.editing = false;
                    self.found.clear();
                    Act::Redraw
                }
                crate::edit::EditResult::Used => Act::Redraw,
                crate::edit::EditResult::Ignored => Act::None,
            };
        }
        let printable = ev.utf8.as_deref().map(|s| !s.is_empty() && !s.chars().any(|c| c.is_control())).unwrap_or(false);
        if ev.keysym == Keysym::f && cx.ctrl {
            self.searching = true;
            return Act::Redraw;
        }
        // the search: focused, or any letter starts it
        if self.searching || (printable && !cx.ctrl) {
            self.searching = true;
            let before = self.search.text.clone();
            let r = self.search.key(ev.keysym, ev.utf8.as_deref(), cx.ctrl, false);
            match r {
                crate::edit::EditResult::Cancel => {
                    self.searching = false;
                    self.search.set("");
                    self.results.clear();
                    return Act::Redraw;
                }
                crate::edit::EditResult::Accept => {
                    if let Some((p, lab)) = self.results.first().cloned() {
                        self.search.set("");
                        self.results.clear();
                        self.searching = false;
                        self.go(p);
                        if !lab.is_empty() {
                            self.flash = Some((lab, std::time::Instant::now()));
                        }
                    }
                    return Act::Redraw;
                }
                crate::edit::EditResult::Used => {
                    if self.search.text != before {
                        self.run_search(cx.st.dnd);
                        self.scroll = 0.0;
                    }
                    return Act::Redraw;
                }
                crate::edit::EditResult::Ignored => {}
            }
        }
        match ev.keysym {
            Keysym::Escape => {
                if matches!(self.page, Page::Widget(_)) {
                    self.go(Page::Nav(3));
                    Act::Redraw
                } else {
                    Act::Close
                }
            }
            Keysym::Down | Keysym::Up if cx.ctrl || !self.searching => {
                let n = PAGES.len();
                let i = self.nav_index();
                let j = if ev.keysym == Keysym::Down { (i + 1) % n } else { (i + n - 1) % n };
                self.go(Page::Nav(j));
                Act::Redraw
            }
            Keysym::Page_Down => {
                self.scroll = (self.scroll + 300.0).min(self.max_scroll);
                Act::Redraw
            }
            Keysym::Page_Up => {
                self.scroll = (self.scroll - 300.0).max(0.0);
                Act::Redraw
            }
            _ => Act::None,
        }
    }
}
