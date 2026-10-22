// Mist Settings pages as data: a page is a list of blocks (section captions, cards of rows, the wallpaper
// grid, colour swatches...), each row a title, a subtitle and one control. The pages are built from the
// current state on every paint (cheap: a few hundred small strings), so a change shows at once and the
// search can read every row of every page.
use super::backend::{self, MIME_CATS};
use super::{Fid, Settings, Sl};
use crate::gfx::Rgba;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Page {
    Home,
    System,
    Display,
    Sound,
    Notif,
    Power,
    Storage,
    Multitask,
    About,
    Devices,
    Bluetooth,
    Mouse,
    Network,
    Personal,
    Wallpaper,
    Colors,
    LockScreen,
    Apps,
    Defaults,
    Installed,
    Accounts,
    Time,
    Lang,
    Access,
    Keys,
    Updates,
}
use Page::*;

pub const SIDEBAR: [Page; 11] = [Home, System, Devices, Network, Personal, Apps, Accounts, Time, Access, Keys, Updates];
pub const ALL: [Page; 26] = [Home, System, Display, Sound, Notif, Power, Storage, Multitask, About, Devices, Bluetooth, Mouse, Network, Personal, Wallpaper, Colors, LockScreen, Apps, Defaults, Installed, Accounts, Time, Lang, Access, Keys, Updates];

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Home => "Strona główna",
            System => "System",
            Display => "Ekran",
            Sound => "Dźwięk",
            Notif => "Powiadomienia",
            Power => "Zasilanie i blokada",
            Storage => "Pamięć",
            Multitask => "Okna i wielozadaniowość",
            About => "Informacje",
            Devices => "Bluetooth i urządzenia",
            Bluetooth => "Bluetooth",
            Mouse => "Mysz i touchpad",
            Network => "Sieć i Internet",
            Personal => "Personalizacja",
            Wallpaper => "Tapeta",
            Colors => "Kolory",
            LockScreen => "Ekran blokady",
            Apps => "Aplikacje",
            Defaults => "Domyślne aplikacje",
            Installed => "Zainstalowane aplikacje",
            Accounts => "Konta",
            Time => "Data i godzina",
            Lang => "Język i klawiatura",
            Access => "Dostępność",
            Keys => "Skróty klawiszowe",
            Updates => "Aktualizacje",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Home => "home",
            System => "system",
            Display => "display",
            Sound => "sound",
            Notif => "notifications",
            Power => "power",
            Storage => "storage",
            Multitask => "multitasking",
            About => "about",
            Devices => "devices",
            Bluetooth => "bluetooth",
            Mouse => "mouse",
            Network => "network",
            Personal => "personalization",
            Wallpaper => "wallpaper",
            Colors => "colors",
            LockScreen => "lockscreen",
            Apps => "apps",
            Defaults => "default-apps",
            Installed => "installed",
            Accounts => "accounts",
            Time => "time",
            Lang => "language",
            Access => "accessibility",
            Keys => "shortcuts",
            Updates => "updates",
        }
    }
    pub fn from_id(s: &str) -> Option<Page> {
        ALL.iter().copied().find(|p| p.id() == s)
    }
    pub fn icon(self) -> &'static str {
        match self {
            Home => "home",
            System => "device-desktop",
            Display => "device-desktop",
            Sound => "volume",
            Notif => "bell",
            Power => "power",
            Storage => "database",
            Multitask => "app-window",
            About => "info-circle",
            Devices | Bluetooth => "bluetooth",
            Mouse => "mouse",
            Network => "world",
            Personal => "brush",
            Wallpaper => "photo",
            Colors => "palette",
            LockScreen => "lock",
            Apps | Installed => "apps",
            Defaults => "circle-check",
            Accounts => "user-circle",
            Time => "clock",
            Lang => "language",
            Access => "accessible",
            Keys => "keyboard",
            Updates => "refresh",
        }
    }
    // the sidebar tile colour
    pub fn tint(self) -> Rgba {
        let h = match self.top() {
            Home => "#7d8ba3",
            System => "#0a84ff",
            Devices => "#3d7bf7",
            Network => "#32ade6",
            Personal => "#bf5af2",
            Apps => "#ff9f0a",
            Accounts => "#30d158",
            Time | Lang => "#40c8b8",
            Access => "#0a84ff",
            Keys => "#8e8e93",
            Updates => "#5e5ce6",
            _ => "#0a84ff",
        };
        Rgba::hex(h)
    }
    pub fn parent(self) -> Option<Page> {
        match self {
            Display | Sound | Notif | Power | Storage | Multitask | About => Some(System),
            Bluetooth | Mouse => Some(Devices),
            Wallpaper | Colors | LockScreen => Some(Personal),
            Defaults | Installed => Some(Apps),
            Lang => Some(Time),
            _ => None,
        }
    }
    // the sidebar entry it belongs to
    pub fn top(self) -> Page {
        self.parent().unwrap_or(self)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    Nav(Page),
    Back,
    Lang(&'static str),
    Accent(u32),
    Contrast(bool),
    Night(bool),
    Dnd(bool),
    NotifClear,
    WallLive(String),
    WallPhoto(String),
    WallMusic(bool),
    WallTempo(&'static str),
    Gallery,
    DeskSheet,
    Sink(String),
    Source(String),
    SinkMute,
    SourceMute,
    AppMute(usize),
    BtPower(bool),
    BtScan(bool),
    BtConn(String, bool),
    BtPair(String),
    Wifi(String),
    WifiOn(bool),
    NetEditor,
    Idle(&'static str, u32),
    Profile(String),
    SessionMenu,
    Lock,
    LockPreview,
    Scale(String, f64),
    Comp(&'static str, String),
    DefApp(usize, String),
    Launch(String),
    Ntp(bool),
    Tz(String),
    Passwd,
    Update,
    CheckUpdates,
    Open(String),
    EditComp,
    CopyInfo,
    Zoom(&'static str),
    Url(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Btn {
    Plain,
    Primary,
}

pub enum Ctl {
    None,
    Text(String),
    Toggle(bool, Act),
    Seg(Vec<(String, bool, Act)>),
    // chips under the title (they wrap)
    Chips(Vec<(String, bool, Act)>),
    Slider(f32, Sl, bool, Act),
    Button(String, Act, Btn),
    Buttons(Vec<(String, Act, Btn)>),
    Chev,
    Field(Fid, &'static str),
    // a usage bar: fraction, the text over it
    Bar(f32, String),
    Keys(Vec<String>),
}

pub struct Row {
    pub icon: &'static str,
    // an app icon (theme name) instead of the glyph
    pub app_icon: String,
    pub title: String,
    pub sub: String,
    pub ctl: Ctl,
    // the whole row is a button
    pub act: Option<Act>,
}
impl Row {
    pub fn new(icon: &'static str, title: &str, sub: &str, ctl: Ctl) -> Row {
        Row { icon, app_icon: String::new(), title: title.into(), sub: sub.into(), ctl, act: None }
    }
    pub fn nav(icon: &'static str, title: &str, sub: &str, to: Page) -> Row {
        Row { icon, app_icon: String::new(), title: title.into(), sub: sub.into(), ctl: Ctl::Chev, act: Some(Act::Nav(to)) }
    }
    fn on(mut self, a: Act) -> Row {
        self.act = Some(a);
        self
    }
    fn app(mut self, icon: &str) -> Row {
        self.app_icon = icon.into();
        self
    }
}

pub struct Thumb {
    pub key: String,
    pub path: String,
    pub name: String,
    pub on: bool,
    pub act: Act,
}

pub struct Tile {
    pub icon: &'static str,
    pub tint: Rgba,
    pub title: String,
    pub sub: String,
    pub act: Act,
}

pub enum Block {
    Caption(String),
    Card(Vec<Row>),
    // the page's hero: picture (a thumbnail path), title, lines, a button
    Hero { pic: String, initials: String, title: String, lines: Vec<String>, act: Option<(String, Act)> },
    Thumbs(Vec<Thumb>),
    Swatches(Vec<(u32, &'static str, bool)>),
    Tiles(Vec<Tile>),
    Note(String),
}

fn chips<T: Clone>(items: &[(T, &str)], cur: &T, act: impl Fn(T) -> Act) -> Vec<(String, bool, Act)>
where
    T: PartialEq,
{
    items.iter().map(|(v, l)| (l.to_string(), v == cur, act(v.clone()))).collect()
}

fn mins(s: u32) -> String {
    match s {
        0 => "Nigdy".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} godz.", s / 3600),
    }
}

pub const ACCENTS: [(u32, &str); 10] = [
    (0x0a84ff, "Niebieski"),
    (0x5e5ce6, "Indygo"),
    (0xbf5af2, "Fioletowy"),
    (0xff375f, "Różowy"),
    (0xff453a, "Czerwony"),
    (0xff9f0a, "Pomarańczowy"),
    (0xffd60a, "Żółty"),
    (0x30d158, "Zielony"),
    (0x40c8b8, "Morski"),
    (0x8e8e93, "Grafitowy"),
];

const KB_LAYOUTS: [(&str, &str); 10] = [("pl", "Polski"), ("us", "Angielski (USA)"), ("gb", "Angielski (UK)"), ("de", "Niemiecki"), ("fr", "Francuski"), ("es", "Hiszpański"), ("it", "Włoski"), ("ua", "Ukraiński"), ("cz", "Czeski"), ("ru", "Rosyjski")];

impl Settings {
    pub fn blocks(&self, p: Page) -> Vec<Block> {
        let d = &self.d;
        let mut b: Vec<Block> = Vec::new();
        match p {
            Home => {
                let host = d.info.iter().find(|i| i.0 == "Nazwa urządzenia").map(|i| i.1.clone()).unwrap_or_default();
                let os = d.info.iter().find(|i| i.0 == "System").map(|i| i.1.clone()).unwrap_or_default();
                let (pic, _) = self.wall_now();
                b.push(Block::Hero { pic, initials: String::new(), title: if host.is_empty() { "Mist".into() } else { host }, lines: vec![os, format!("{} · {}", d.user.1, d.user.0)], act: Some(("Zmień tapetę".into(), Act::Nav(Wallpaper))) });
                let net = d.net.as_ref().map(|n| if n.up { n.conn.clone() } else { "Brak połączenia".into() }).unwrap_or_else(|| "…".into());
                let bt = d.bt.as_ref().map(|b| if !b.on { "Wyłączony".to_string() } else { match b.devs.iter().filter(|x| x.connected).count() { 0 => "Włączony".into(), n => format!("Połączone: {n}") } }).unwrap_or_else(|| "…".into());
                let upd = match &d.updates {
                    Some(Ok(v)) if v.is_empty() => "System jest aktualny".to_string(),
                    Some(Ok(v)) => format!("Dostępne: {}", v.len()),
                    Some(Err(_)) => "Nie sprawdzono".into(),
                    None => "Sprawdzam…".into(),
                };
                let snd = d.snd.as_ref().and_then(|s| s.sinks.iter().find(|x| x.name == s.sink)).map(|x| format!("{} · {}%", x.desc, (x.vol * 100.0).round())).unwrap_or_else(|| "…".into());
                b.push(Block::Caption("Na skróty".into()));
                b.push(Block::Tiles(vec![
                    Tile { icon: "world", tint: Network.tint(), title: "Internet".into(), sub: net, act: Act::Nav(Network) },
                    Tile { icon: "bluetooth", tint: Devices.tint(), title: "Bluetooth".into(), sub: bt, act: Act::Nav(Bluetooth) },
                    Tile { icon: "volume", tint: System.tint(), title: "Dźwięk".into(), sub: snd, act: Act::Nav(Sound) },
                    Tile { icon: "refresh", tint: Updates.tint(), title: "Aktualizacje".into(), sub: upd, act: Act::Nav(Updates) },
                ]));
                b.push(Block::Caption("Wygląd".into()));
                b.push(Block::Swatches(ACCENTS.iter().map(|(c, n)| (*c, *n, *c == self.accent_now())).collect()));
                b.push(Block::Card(vec![
                    Row::nav("photo", "Tapeta", "Zdjęcie lub żywa scena, rytm muzyki", Wallpaper),
                    Row::new("layout-dashboard", "Pulpit i widżety", "Zegar, karty, widżety pulpitu", Ctl::Button("Dostosuj".into(), Act::DeskSheet, Btn::Plain)),
                    Row::new("moon", "Nocne światło", "Cieplejsze barwy ekranu wieczorem", Ctl::Toggle(d.night, Act::Night(!d.night))),
                ]));
            }
            System => {
                let mon = d.mons.iter().map(|m| format!("{}×{}", m.w, m.h)).collect::<Vec<_>>().join(", ");
                b.push(Block::Card(vec![
                    Row::nav("device-desktop", "Ekran", &if mon.is_empty() { "Monitory, skalowanie, nocne światło".to_string() } else { format!("{mon} · skalowanie, nocne światło") }, Display),
                    Row::nav("volume", "Dźwięk", "Głośność, urządzenia wyjścia i wejścia, aplikacje", Sound),
                    Row::nav("bell", "Powiadomienia", "Nie przeszkadzać, historia", Notif),
                    Row::nav("power", "Zasilanie i blokada", "Blokada i wygaszanie, profil zasilania", Power),
                    Row::nav("database", "Pamięć", "Dyski i wolne miejsce", Storage),
                    Row::nav("app-window", "Okna i wielozadaniowość", "Gesty, przegląd okien, przyciąganie", Multitask),
                    Row::nav("info-circle", "Informacje", "Specyfikacja urządzenia, wersja Mist", About),
                ]));
            }
            Display => {
                if d.mons.is_empty() {
                    b.push(Block::Note("Brak informacji o monitorach (kompozytor nie odpowiada).".into()));
                }
                let comp = backend::under_mist_comp();
                for m in &d.mons {
                    b.push(Block::Caption(if m.desc.is_empty() { m.name.clone() } else { format!("{} · {}", m.name, m.desc) }));
                    let scales: Vec<(String, bool, Act)> = [1.0, 1.25, 1.5, 1.75, 2.0].iter().map(|s| (format!("{}%", (s * 100.0) as i32), (m.scale - s).abs() < 0.01, Act::Scale(m.name.clone(), *s))).collect();
                    b.push(Block::Card(vec![
                        Row::new("arrows-maximize", "Rozdzielczość", "Tryb ekranu ustawiony przez kompozytor", Ctl::Text(format!("{} × {} · {:.0} Hz", m.w, m.h, m.hz))),
                        Row::new("zoom-in", "Skalowanie", if comp { "Rozmiar tekstu, aplikacji i elementów" } else { "Ustawiane w konfiguracji Hyprlanda" }, if comp { Ctl::Chips(scales) } else { Ctl::Text(format!("{}%", (m.scale * 100.0).round())) }),
                        Row::new("layout-grid", "Położenie", "Od lewej, w pikselach logicznych", Ctl::Text(format!("{}, {}", m.x, m.y))),
                    ]));
                }
                b.push(Block::Caption("Jasność i kolor".into()));
                let vrr = backend::comp_get("vrr");
                let mut rows = vec![Row::new("moon", "Nocne światło", "Cieplejsze barwy (ok. 4000 K) mniej męczą oczy", Ctl::Toggle(d.night, Act::Night(!d.night)))];
                if comp {
                    rows.push(Row::new("bolt", "Zmienna częstotliwość (VRR)", "Adaptive Sync dla gier i filmów na pełnym ekranie", Ctl::Toggle(vrr == "fullscreen", Act::Comp("vrr", if vrr == "fullscreen" { "off".into() } else { "fullscreen".into() }))));
                }
                b.push(Block::Card(rows));
            }
            Sound => {
                if let Some(s) = &d.snd {
                    let sink = s.sinks.iter().find(|x| x.name == s.sink).cloned().unwrap_or_default();
                    let src = s.sources.iter().find(|x| x.name == s.source).cloned().unwrap_or_default();
                    b.push(Block::Caption("Wyjście".into()));
                    b.push(Block::Card(vec![
                        Row::new(if sink.mute { "volume-off" } else { "volume" }, "Głośność", &sink.desc, Ctl::Slider(sink.vol, Sl::Sink, sink.mute, Act::SinkMute)),
                        Row::new("speakerphone", "Urządzenie wyjściowe", "Gdzie gra dźwięk", Ctl::Chips(s.sinks.iter().map(|x| (x.desc.clone(), x.name == s.sink, Act::Sink(x.name.clone()))).collect())),
                    ]));
                    b.push(Block::Caption("Wejście".into()));
                    b.push(Block::Card(vec![
                        Row::new("microphone", "Mikrofon", &src.desc, Ctl::Slider(src.vol, Sl::Source, src.mute, Act::SourceMute)),
                        Row::new("microphone", "Urządzenie wejściowe", "Czym nagrywasz i rozmawiasz", Ctl::Chips(s.sources.iter().map(|x| (x.desc.clone(), x.name == s.source, Act::Source(x.name.clone()))).collect())),
                    ]));
                    if !s.apps.is_empty() {
                        b.push(Block::Caption("Aplikacje".into()));
                        b.push(Block::Card(s.apps.iter().enumerate().map(|(i, a)| Row::new("music", &a.name, &a.media, Ctl::Slider(a.vol, Sl::App(i), a.mute, Act::AppMute(i))).app(&a.icon)).collect()));
                    }
                } else {
                    b.push(Block::Note("Wczytuję urządzenia dźwięku…".into()));
                }
            }
            Notif => {
                b.push(Block::Card(vec![
                    Row::new("bell-off", "Nie przeszkadzać", "Powiadomienia trafiają tylko do centrum, bez dźwięku i dymków", Ctl::Toggle(d.dnd, Act::Dnd(!d.dnd))),
                    Row::new("history", "Historia powiadomień", &format!("W centrum powiadomień: {}", d.notif_count), Ctl::Button("Wyczyść".into(), Act::NotifClear, Btn::Plain)),
                ]));
                b.push(Block::Note("Centrum powiadomień otworzysz klawiszami Super+A.".into()));
            }
            Power => {
                let (lock, off) = (d.idle.0, d.idle.1);
                let opts = [60, 120, 300, 600, 900, 1800, 3600, 0];
                b.push(Block::Caption("Ekran i blokada".into()));
                b.push(Block::Card(vec![
                    Row::new("lock", "Zablokuj po bezczynności", &mins(lock), Ctl::Chips(opts.iter().map(|s| (mins(*s), *s == lock, Act::Idle("lock", *s))).collect())),
                    Row::new("device-desktop", "Wyłącz ekran po", &mins(off), Ctl::Chips(opts.iter().map(|s| (mins(*s), *s == off, Act::Idle("off", *s))).collect())),
                ]));
                if !d.profiles.is_empty() {
                    let name = |p: &str| match p {
                        "performance" => "Wydajność",
                        "balanced" => "Zrównoważony",
                        "power-saver" => "Oszczędzanie",
                        p => p,
                    }
                    .to_string();
                    b.push(Block::Caption("Zasilanie".into()));
                    b.push(Block::Card(vec![Row::new("gauge", "Profil zasilania", "power-profiles-daemon", Ctl::Seg(d.profiles.iter().map(|p| (name(p), *p == d.profile, Act::Profile(p.clone()))).collect()))]));
                }
                b.push(Block::Card(vec![Row::new("power", "Wyłącz, uruchom ponownie, uśpij", "Menu sesji (Super+Alt+C)", Ctl::Button("Otwórz".into(), Act::SessionMenu, Btn::Plain))]));
            }
            Storage => {
                let rows: Vec<Row> = d
                    .drives
                    .iter()
                    .map(|x| {
                        let used = if x.size > 0 { 1.0 - x.free as f32 / x.size as f32 } else { 0.0 };
                        Row::new(if x.removable { "usb" } else { "database" }, &x.name, &format!("{} · wolne {} z {}", x.path, crate::files::fs::size(x.free), crate::files::fs::size(x.size)), Ctl::Bar(used, format!("{}%", (used * 100.0).round()))).on(Act::Open(x.path.clone()))
                    })
                    .collect();
                b.push(Block::Card(rows));
                b.push(Block::Card(vec![Row::new("trash", "Pamięć podręczna pakietów", "Zostawia trzy ostatnie wersje każdego pakietu (paccache)", Ctl::Button("Wyczyść".into(), Act::Comp("__paccache", String::new()), Btn::Plain))]));
            }
            Multitask => {
                let comp = backend::under_mist_comp();
                let swipe = backend::comp_get("swipe");
                let swipe = if swipe.is_empty() { "3".to_string() } else { swipe };
                let sw = backend::comp_get("swallow") == "on";
                if comp {
                    b.push(Block::Card(vec![
                        Row::new("hand-finger", "Gest przełączania pulpitów", "Przesunięcie w poziomie na touchpadzie", Ctl::Seg(chips(&[("3".to_string(), "3 palce"), ("4".to_string(), "4 palce"), ("off".to_string(), "Wyłączony")], &swipe, |v| Act::Comp("swipe", v)))),
                        Row::new("terminal", "Ukrywaj terminal", "Okno uruchomione z terminala zastępuje go, aż się zamknie", Ctl::Toggle(sw, Act::Comp("swallow", if sw { "off".into() } else { "on".into() }))),
                        Row::new("layout-grid", "Przegląd okien", "Wszystkie okna pulpitu obok siebie (Super+G)", Ctl::Button("Pokaż".into(), Act::Comp("__overview", String::new()), Btn::Plain)),
                    ]));
                }
                b.push(Block::Caption("Przyciąganie okien".into()));
                b.push(Block::Card(vec![
                    Row::new("layout-dashboard", "Układy przyciągania", "Super+Z lub najedź na przycisk maksymalizacji", Ctl::Keys(vec!["Super".into(), "Z".into()])),
                    Row::new("arrows-maximize", "Przyciągnij do krawędzi", "Przeciągnij okno do krawędzi lub rogu ekranu", Ctl::Keys(vec!["Super".into(), "←→↑↓".into()])),
                ]));
            }
            About => {
                let icon = |k: &str| match k {
                    "Nazwa urządzenia" => "device-desktop",
                    "System" => "package",
                    "Jądro" => "terminal",
                    "Procesor" => "cpu",
                    "Karta graficzna" => "gauge",
                    "Pamięć RAM" => "database",
                    "Kompozytor" => "app-window",
                    _ => "info-circle",
                };
                b.push(Block::Card(d.info.iter().map(|(k, v)| Row::new(icon(k), k, "", Ctl::Text(v.clone()))).collect()));
                b.push(Block::Card(vec![
                    Row::new("heart", "Wesprzyj projekt", "github.com/sponsors/wiktor-jozwiak", Ctl::Button("Sponsoruj".into(), Act::Url("https://github.com/sponsors/wiktor-jozwiak".into()), Btn::Plain)),
                    Row::new("copy", "Specyfikacja", "Kopiuj wszystkie dane do schowka", Ctl::Button("Kopiuj".into(), Act::CopyInfo, Btn::Plain)),
                ]));
            }
            Devices => {
                let bt_on = d.bt.as_ref().map(|b| b.on).unwrap_or(false);
                b.push(Block::Card(vec![
                    Row::new("bluetooth", "Bluetooth", if bt_on { "Wykrywalny jako to urządzenie" } else { "Wyłączony" }, Ctl::Toggle(bt_on, Act::BtPower(!bt_on))),
                    Row::nav("headphones", "Urządzenia", "Sparuj słuchawki, mysz, klawiaturę, telefon", Bluetooth),
                    Row::nav("mouse", "Mysz i touchpad", "Przyspieszenie, gesty, wklejanie środkowym", Mouse),
                ]));
                if let Some(bt) = &d.bt {
                    let con: Vec<Row> = bt.devs.iter().filter(|x| x.connected).map(|x| bt_row(x)).collect();
                    if !con.is_empty() {
                        b.push(Block::Caption("Połączone".into()));
                        b.push(Block::Card(con));
                    }
                }
            }
            Bluetooth => match &d.bt {
                Some(bt) => {
                    b.push(Block::Card(vec![
                        Row::new("bluetooth", "Bluetooth", if bt.on { "Włączony" } else { "Wyłączony" }, Ctl::Toggle(bt.on, Act::BtPower(!bt.on))),
                        Row::new("search", "Dodaj urządzenie", if bt.discovering { "Szukam urządzeń w pobliżu…" } else { "Ustaw urządzenie w tryb parowania" }, Ctl::Button(if bt.discovering { "Zatrzymaj".into() } else { "Szukaj".into() }, Act::BtScan(!bt.discovering), if bt.discovering { Btn::Plain } else { Btn::Primary })),
                    ]));
                    let paired: Vec<Row> = bt.devs.iter().filter(|x| x.paired).map(|x| bt_row(x)).collect();
                    if !paired.is_empty() {
                        b.push(Block::Caption("Sparowane".into()));
                        b.push(Block::Card(paired));
                    }
                    let near: Vec<Row> = bt.devs.iter().filter(|x| !x.paired && !x.name.is_empty()).map(|x| bt_row(x)).collect();
                    if !near.is_empty() {
                        b.push(Block::Caption("W pobliżu".into()));
                        b.push(Block::Card(near));
                    }
                }
                None => b.push(Block::Note("Wczytuję Bluetooth…".into())),
            },
            Mouse => {
                if backend::under_mist_comp() {
                    let flat = backend::comp_get("pointer") == "flat";
                    let mcp = backend::comp_get("middle_click_paste") != "off";
                    b.push(Block::Card(vec![
                        Row::new("pointer", "Przyspieszenie wskaźnika", "Płaskie: ruch wskaźnika zawsze taki sam jak ruch myszy", Ctl::Seg(vec![("Płaskie".into(), flat, Act::Comp("pointer", "flat".into())), ("Adaptacyjne".into(), !flat, Act::Comp("pointer", "adaptive".into()))])),
                        Row::new("mouse", "Wklejanie środkowym przyciskiem", "Zaznaczony tekst wkleja się kółkiem (po ponownym zalogowaniu)", Ctl::Toggle(mcp, Act::Comp("middle_click_paste", if mcp { "off".into() } else { "on".into() }))),
                    ]));
                    b.push(Block::Note("Przyspieszenie działa od następnego podłączenia urządzenia lub logowania.".into()));
                } else {
                    b.push(Block::Note("Ustawienia myszy dla Hyprlanda są w ~/.config/hypr/config/inputs.lua.".into()));
                }
            }
            Network => {
                match &d.net {
                    Some(n) => {
                        let kind = if n.kind == "wifi" { "Wi‑Fi" } else { "Ethernet" };
                        b.push(Block::Hero {
                            pic: String::new(),
                            initials: String::new(),
                            title: if n.up { n.conn.clone() } else { "Brak połączenia".into() },
                            lines: vec![if n.up { format!("{kind} · {} · połączono z Internetem", n.dev) } else { kind.to_string() }, if n.speed > 0 { format!("{} Mb/s", n.speed) } else { String::new() }],
                            act: Some(("Zaawansowane".into(), Act::NetEditor)),
                        });
                        if n.up {
                            b.push(Block::Card(vec![
                                Row::new("router", "Adres IPv4", "", Ctl::Text(n.ip.clone())),
                                Row::new("world", "Brama", "", Ctl::Text(n.gw.clone())),
                                Row::new("server", "DNS", "", Ctl::Text(n.dns.clone())),
                            ]));
                        }
                        if n.has_wifi {
                            b.push(Block::Caption("Wi‑Fi".into()));
                            let mut rows = vec![Row::new("wifi", "Wi‑Fi", "", Ctl::Toggle(d.wifi_on, Act::WifiOn(!d.wifi_on)))];
                            for w in &d.wifi {
                                let sub = format!("{}{}{}", if w.on { "Połączono · " } else { "" }, if w.secure { "Zabezpieczona" } else { "Otwarta" }, if w.known && !w.on { " · zapisana" } else { "" });
                                if self.wifi_ask.as_deref() == Some(w.ssid.as_str()) {
                                    rows.push(Row::new("lock", &w.ssid, "Wpisz hasło sieci i naciśnij Enter", Ctl::Field(Fid::WifiPwd, "Hasło")));
                                } else {
                                    rows.push(Row::new("wifi", &w.ssid, &sub, if w.on { Ctl::Text(format!("{}%", w.signal)) } else { Ctl::Button("Połącz".into(), Act::Wifi(w.ssid.clone()), Btn::Plain) }));
                                }
                            }
                            b.push(Block::Card(rows));
                        }
                        if !d.wifi_err.is_empty() {
                            b.push(Block::Note(d.wifi_err.clone()));
                        }
                    }
                    None => b.push(Block::Note("Wczytuję sieć…".into())),
                }
            }
            Personal => {
                let (pic, name) = self.wall_now();
                b.push(Block::Hero { pic, initials: String::new(), title: name, lines: vec!["Tapeta pulpitu".into()], act: Some(("Galeria tapet".into(), Act::Gallery)) });
                b.push(Block::Card(vec![
                    Row::nav("photo", "Tapeta", "Zdjęcia i żywe sceny, rytm muzyki, tempo", Wallpaper),
                    Row::nav("palette", "Kolory", "Kolor akcentu, wysoki kontrast", Colors),
                    Row::nav("lock", "Ekran blokady", "Podgląd, blokada po bezczynności", LockScreen),
                    Row::new("layout-dashboard", "Pulpit i widżety", "Zegar, karty pogody i treningów, muzyka, kalendarz", Ctl::Button("Dostosuj".into(), Act::DeskSheet, Btn::Plain)),
                ]));
            }
            Wallpaper => {
                let w = &d.wall;
                let live = w["live"].as_str().unwrap_or("").to_string();
                let music = w["music"].as_bool().unwrap_or(false);
                let tempo = w["tempo"].as_str().unwrap_or("normal").to_string();
                b.push(Block::Card(vec![
                    Row::new("music", "Rytm muzyki", "Żywa scena reaguje na odtwarzaną muzykę", Ctl::Toggle(music, Act::WallMusic(!music))),
                    Row::new("gauge", "Tempo sceny", "", Ctl::Seg(vec![("Spokojnie".into(), tempo == "calm", Act::WallTempo("calm")), ("Normalnie".into(), tempo == "normal", Act::WallTempo("normal")), ("Dynamicznie".into(), tempo == "fast", Act::WallTempo("fast"))])),
                    Row::new("layout-grid", "Galeria tapet", "Pełny ekran z podglądem na żywo i filtrami", Ctl::Button("Otwórz".into(), Act::Gallery, Btn::Plain)),
                ]));
                let image = w["image"].as_str().unwrap_or("").to_string();
                let photos = crate::presets::photos();
                if !photos.is_empty() {
                    b.push(Block::Caption("Zdjęcia".into()));
                    b.push(Block::Thumbs(
                        photos
                            .iter()
                            .map(|p| Thumb { key: format!("photo:{p}"), path: crate::presets::thumb_path(&format!("photo:{p}")), name: p.rsplit('/').next().unwrap_or(p).rsplit_once('.').map(|x| x.0).unwrap_or(p).replace(['-', '_'], " "), on: live.is_empty() && *p == image, act: Act::WallPhoto(p.clone()) })
                            .collect(),
                    ));
                }
                b.push(Block::Caption("Żywe sceny".into()));
                b.push(Block::Thumbs(
                    self.presets
                        .iter()
                        .filter(|p| p.get("mode").is_some())
                        .filter_map(|p| {
                            let id = p["id"].as_str()?.to_string();
                            Some(Thumb { key: format!("live:{id}"), path: crate::presets::thumb_path(&format!("live:{id}")), name: p["name"].as_str().unwrap_or(&id).to_string(), on: id == live, act: Act::WallLive(id) })
                        })
                        .collect(),
                ));
            }
            Colors => {
                b.push(Block::Caption("Kolor akcentu".into()));
                b.push(Block::Swatches(ACCENTS.iter().map(|(c, n)| (*c, *n, *c == self.accent_now())).collect()));
                b.push(Block::Note("Akcent barwi przełączniki, zaznaczenia, suwaki i aktywne elementy całej powłoki.".into()));
                b.push(Block::Card(vec![Row::new("contrast", "Wysoki kontrast", "Nieprzezroczyste panele i wyraźniejszy tekst", Ctl::Toggle(d.contrast, Act::Contrast(!d.contrast)))]));
            }
            LockScreen => {
                let opts = [60, 120, 300, 600, 900, 1800, 0];
                b.push(Block::Card(vec![
                    Row::new("eye", "Podgląd ekranu blokady", "Pokazuje go na kilka sekund, bez blokowania", Ctl::Button("Podgląd".into(), Act::LockPreview, Btn::Plain)),
                    Row::new("lock", "Zablokuj teraz", "Super+L", Ctl::Button("Zablokuj".into(), Act::Lock, Btn::Plain)),
                    Row::new("clock", "Zablokuj po bezczynności", &mins(d.idle.0), Ctl::Chips(opts.iter().map(|s| (mins(*s), *s == d.idle.0, Act::Idle("lock", *s))).collect())),
                ]));
            }
            Apps => {
                b.push(Block::Card(vec![
                    Row::nav("circle-check", "Domyślne aplikacje", "Przeglądarka, pliki, muzyka, wideo, zdjęcia", Defaults),
                    Row::nav("apps", "Zainstalowane aplikacje", &format!("{} aplikacji", self.app_list().len()), Installed),
                ]));
            }
            Defaults => {
                if d.mime.is_empty() {
                    b.push(Block::Note("Sprawdzam skojarzenia plików…".into()));
                }
                let mut rows = Vec::new();
                for (i, (label, icon, _)) in MIME_CATS.iter().enumerate() {
                    let Some((cur, cands)) = d.mime.get(i) else { continue };
                    let name = |id: &str| self.app_name(id);
                    let shown: Vec<&String> = cands.iter().take(8).collect();
                    // two apps with one name (Files from GNOME and from Mist): the desktop id tells them apart
                    let chip = |id: &str| {
                        let n = name(id);
                        if shown.iter().filter(|o| name(o) == n).count() > 1 { format!("{n} ({id})") } else { n }
                    };
                    let items: Vec<(String, bool, Act)> = shown.iter().map(|id| (chip(id), *id == cur, Act::DefApp(i, (*id).clone()))).collect();
                    let sub = if cur.is_empty() { "Nie ustawiono".to_string() } else { name(cur) };
                    rows.push(Row::new(icon, label, &sub, Ctl::Chips(items)));
                }
                if !rows.is_empty() {
                    b.push(Block::Card(rows));
                }
            }
            Installed => {
                let list = self.app_list();
                b.push(Block::Card(list.iter().map(|(id, name, icon, comment)| Row::new("apps", name, comment, Ctl::Button("Otwórz".into(), Act::Launch(id.clone()), Btn::Plain)).app(icon)).collect()));
            }
            Accounts => {
                b.push(Block::Hero { pic: d.face.clone(), initials: initials(&d.user.1), title: d.user.1.clone(), lines: vec![format!("{} · konto lokalne", d.user.0), if d.user.2.contains(&"wheel".to_string()) { "Administrator".into() } else { "Użytkownik standardowy".into() }], act: None });
                b.push(Block::Card(vec![
                    Row::new("key", "Hasło", "Zmień hasło logowania (w terminalu)", Ctl::Button("Zmień".into(), Act::Passwd, Btn::Plain)),
                    Row::new("lock", "Zablokuj ekran", "Super+L", Ctl::Button("Zablokuj".into(), Act::Lock, Btn::Plain)),
                    Row::new("user-shield", "Grupy", &d.user.2.join(", "), Ctl::None),
                ]));
            }
            Time => {
                b.push(Block::Card(vec![
                    Row::new("clock", "Ustaw czas automatycznie", "Synchronizacja z serwerami czasu (NTP)", Ctl::Toggle(d.ntp, Act::Ntp(!d.ntp))),
                    Row::new("world", "Strefa czasowa", &d.tz, Ctl::Field(Fid::Tz, "Szukaj strefy…")),
                ]));
                let q = self.tz_q.text.to_lowercase();
                if !q.is_empty() {
                    let m: Vec<(String, bool, Act)> = d.tzs.iter().filter(|z| z.to_lowercase().contains(&q)).take(12).map(|z| (z.clone(), *z == d.tz, Act::Tz(z.clone()))).collect();
                    b.push(Block::Card(vec![Row::new("map-pin", "Wyniki", if m.is_empty() { "Nic nie znaleziono" } else { "Kliknij, aby ustawić" }, Ctl::Chips(m))]));
                }
                b.push(Block::Card(vec![
                    Row::nav("language", "Język i klawiatura", "Język interfejsu, układ klawiatury", Lang),
                    Row::new("map-pin", "Miejsce pogody", &d.place, Ctl::Button("Zmień".into(), Act::DeskSheet, Btn::Plain)),
                ]));
            }
            Lang => {
                let cur = crate::i18n::setting();
                let mut langs: Vec<(String, bool, Act)> = vec![("Automatycznie".into(), cur == "auto", Act::Lang("auto"))];
                langs.extend(crate::i18n::LANGS.iter().map(|(c, n)| (n.to_string(), cur == *c, Act::Lang(c))));
                b.push(Block::Card(vec![Row::new("language", "Język interfejsu", "Mist i jego aplikacje", Ctl::Chips(langs))]));
                if backend::under_mist_comp() {
                    let kb = backend::comp_get("keyboard");
                    let lay = kb.split_whitespace().next().unwrap_or("pl").to_string();
                    let nl = backend::comp_get("numlock") == "on";
                    b.push(Block::Caption("Klawiatura".into()));
                    b.push(Block::Card(vec![
                        Row::new("keyboard", "Układ klawiatury", &lay, Ctl::Chips(KB_LAYOUTS.iter().map(|(c, n)| (n.to_string(), *c == lay, Act::Comp("keyboard", c.to_string()))).collect())),
                        Row::new("numbers", "Num Lock po zalogowaniu", "Klawiatura numeryczna włączona od startu", Ctl::Toggle(nl, Act::Comp("numlock", if nl { "off".into() } else { "on".into() }))),
                    ]));
                }
            }
            Access => {
                b.push(Block::Caption("Widzenie".into()));
                let mut rows = vec![Row::new("contrast", "Wysoki kontrast", "Nieprzezroczyste panele i wyraźniejszy tekst", Ctl::Toggle(d.contrast, Act::Contrast(!d.contrast)))];
                if backend::under_mist_comp() {
                    rows.push(Row::new("zoom-in", "Lupa", "Super + =  /  Super + −", Ctl::Buttons(vec![("−".into(), Act::Zoom("out"), Btn::Plain), ("Wyłącz".into(), Act::Zoom("off"), Btn::Plain), ("+".into(), Act::Zoom("in"), Btn::Plain)])));
                }
                rows.push(Row::new("moon", "Nocne światło", "Mniej niebieskiego światła", Ctl::Toggle(d.night, Act::Night(!d.night))));
                b.push(Block::Card(rows));
                b.push(Block::Caption("Słuch".into()));
                b.push(Block::Card(vec![Row::new("bell-off", "Nie przeszkadzać", "Bez dźwięków powiadomień", Ctl::Toggle(d.dnd, Act::Dnd(!d.dnd)))]));
            }
            Keys => {
                b.push(Block::Card(vec![Row::new("search", "Szukaj skrótu", "", Ctl::Field(Fid::Keys, "Np. zrzut, pulpit, terminal…"))]));
                let q = self.keys_q.text.to_lowercase();
                let mut groups: Vec<(&'static str, Vec<Row>)> = Vec::new();
                for (keys, act) in backend::binds() {
                    let (group, label) = describe(&act);
                    if !q.is_empty() && !label.to_lowercase().contains(&q) && !crate::i18n::tr(&label).to_lowercase().contains(&q) && !keys.to_lowercase().contains(&q) {
                        continue;
                    }
                    let caps: Vec<String> = keys.split('+').map(key_name).collect();
                    let row = Row::new("keyboard", &label, "", Ctl::Keys(caps));
                    match groups.iter_mut().find(|g| g.0 == group) {
                        Some(g) => g.1.push(row),
                        None => groups.push((group, vec![row])),
                    }
                }
                let order = ["Mist", "Okna", "Pulpity", "Aplikacje", "Multimedia", "Dostępność", "System", "Inne"];
                groups.sort_by_key(|g| order.iter().position(|o| *o == g.0).unwrap_or(99));
                for (g, rows) in groups {
                    b.push(Block::Caption(g.into()));
                    b.push(Block::Card(rows));
                }
                if backend::under_mist_comp() {
                    b.push(Block::Card(vec![Row::new("terminal", "Własne skróty", "~/.config/mist/comp.conf: linie bind KLAWISZE akcja", Ctl::Button("Edytuj plik".into(), Act::EditComp, Btn::Plain))]));
                }
            }
            Updates => {
                let (title, sub) = match &d.updates {
                    _ if d.checking => ("Sprawdzam aktualizacje…".to_string(), String::new()),
                    Some(Ok(v)) if v.is_empty() => ("System jest aktualny".into(), String::new()),
                    Some(Ok(v)) => (format!("Dostępne aktualizacje: {}", v.len()), String::new()),
                    Some(Err(e)) => ("Nie udało się sprawdzić".into(), e.clone()),
                    None => ("Aktualizacje".into(), String::new()),
                };
                let last = if d.last_upgrade.is_empty() { String::new() } else { format!("Ostatnia aktualizacja: {}", d.last_upgrade) };
                b.push(Block::Hero { pic: String::new(), initials: String::new(), title, lines: vec![last, sub], act: Some(("Sprawdź".into(), Act::CheckUpdates)) });
                if let Some(Ok(v)) = &d.updates {
                    if !v.is_empty() {
                        b.push(Block::Card(vec![Row::new("download", "Zainstaluj aktualizacje", "Otwiera terminal (pacman / yay pyta o hasło)", Ctl::Button("Aktualizuj teraz".into(), Act::Update, Btn::Primary))]));
                        b.push(Block::Caption("Pakiety".into()));
                        b.push(Block::Card(
                            v.iter()
                                .take(60)
                                .map(|l| {
                                    let mut it = l.split_whitespace();
                                    let name = it.next().unwrap_or("").to_string();
                                    let rest: Vec<&str> = it.collect();
                                    Row::new("package", &name, "", Ctl::Text(rest.join(" ")))
                                })
                                .collect(),
                        ));
                    }
                }
            }
        }
        b
    }
}

fn bt_row(x: &crate::control::BtD) -> Row {
    let sub = match (x.connected, x.battery) {
        (true, Some(b)) => format!("Połączono · bateria {}%", (b * 100.0).round()),
        (true, None) => "Połączono".to_string(),
        (false, _) if x.paired => "Sparowane".to_string(),
        _ => x.address.clone(),
    };
    let ctl = if !x.paired {
        Ctl::Button("Paruj".into(), Act::BtPair(x.path.clone()), Btn::Primary)
    } else if x.connected {
        Ctl::Button("Rozłącz".into(), Act::BtConn(x.path.clone(), false), Btn::Plain)
    } else {
        Ctl::Button("Połącz".into(), Act::BtConn(x.path.clone(), true), Btn::Plain)
    };
    let mut r = Row::new("bluetooth", if x.name.is_empty() { &x.address } else { &x.name }, &sub, ctl);
    r.icon = match crate::control::bt_icon(x) {
        "headphones" => "headphones",
        "keyboard" => "keyboard",
        "mouse" => "mouse",
        "gamepad" => "device-gamepad-2",
        "mobile" => "mobile",
        "speaker" => "speakerphone",
        _ => "bluetooth",
    };
    r
}

pub fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

fn key_name(k: &str) -> String {
    match k {
        "SUPER" => "Super".into(),
        "SHIFT" => "Shift".into(),
        "CTRL" => "Ctrl".into(),
        "ALT" => "Alt".into(),
        "Return" => "Enter".into(),
        "Escape" => "Esc".into(),
        "period" => ".".into(),
        "equal" => "=".into(),
        "minus" => "−".into(),
        "grave" => "`".into(),
        "Space" | "space" => "Spacja".into(),
        "Left" => "←".into(),
        "Right" => "→".into(),
        "Up" => "↑".into(),
        "Down" => "↓".into(),
        "mouse_up" => "Kółko ↑".into(),
        "mouse_down" => "Kółko ↓".into(),
        "Super_L" => "Super".into(),
        "BackSpace" => "Backspace".into(),
        k if k.starts_with("XF86") => k[4..].into(),
        k => k.into(),
    }
}

// a bind's action in words, and its group
pub fn describe(act: &str) -> (&'static str, String) {
    let a = act.trim();
    let msg = |s: &str| a.contains(&format!("toggle {s}"));
    let s: (&str, &str) = if msg("spotlight") {
        ("Mist", "Wyszukiwanie (Spotlight)")
    } else if a.ends_with("toggle start") {
        ("Mist", "Menu Start")
    } else if msg("control") {
        ("Mist", "Centrum sterowania")
    } else if msg("phone") {
        ("Mist", "Telefon")
    } else if msg("emoji") {
        ("Mist", "Emoji")
    } else if msg("clipboard") {
        ("Mist", "Historia schowka")
    } else if msg("notifications") {
        ("Mist", "Centrum powiadomień")
    } else if msg("wallpaper") {
        ("Mist", "Galeria tapet")
    } else if msg("session") {
        ("Mist", "Menu sesji")
    } else if a.contains("mist-ui-bin settings") {
        ("Mist", "Ustawienia")
    } else if a.contains("mist-ui-bin lock") {
        ("Mist", "Zablokuj ekran")
    } else if a.contains("switcher next") {
        ("Okna", "Przełącznik okien")
    } else if a.contains("switcher prev") {
        ("Okna", "Przełącznik okien wstecz")
    } else if a.contains("screenshot.sh region") {
        ("Aplikacje", "Zrzut zaznaczonego obszaru")
    } else if a.contains("screenshot.sh full") {
        ("Aplikacje", "Zrzut całego ekranu")
    } else if a.contains("media toggle") {
        ("Multimedia", "Odtwarzaj / pauza")
    } else if a.contains("media next") {
        ("Multimedia", "Następny utwór")
    } else if a.contains("media prev") {
        ("Multimedia", "Poprzedni utwór")
    } else if a.contains("set-volume") && a.ends_with('+') {
        ("Multimedia", "Głośniej")
    } else if a.contains("set-volume") {
        ("Multimedia", "Ciszej")
    } else if a.contains("SINK@ toggle") {
        ("Multimedia", "Wycisz dźwięk")
    } else if a.contains("SOURCE@ toggle") {
        ("Multimedia", "Wycisz mikrofon")
    } else if a.contains("bright.sh up") {
        ("Multimedia", "Jaśniej")
    } else if a.contains("bright.sh down") {
        ("Multimedia", "Ciemniej")
    } else {
        let mut it = a.splitn(2, ' ');
        let (k, rest) = (it.next().unwrap_or(""), it.next().unwrap_or("").trim());
        return match k {
            "exec" => ("Aplikacje", format!("Uruchom {}", rest.split_whitespace().next().unwrap_or(rest).rsplit('/').next().unwrap_or(rest))),
            "close" => ("Okna", "Zamknij okno".into()),
            "kill" => ("Okna", "Wymuś zamknięcie".into()),
            "quit" => ("System", "Wyloguj (zakończ sesję)".into()),
            "reload" => ("System", "Przeładuj konfigurację".into()),
            "fullscreen" => ("Okna", "Pełny ekran".into()),
            "maximize" => ("Okna", "Maksymalizuj".into()),
            "float" => ("Okna", "Okno pływające".into()),
            "overview" => ("Okna", "Przegląd okien".into()),
            "cycle" => ("Okna", "Następne okno".into()),
            "togglespecial" => ("Pulpity", "Notatnik (pulpit specjalny)".into()),
            "focus" => ("Okna", format!("Fokus: {}", dir(rest))),
            "snap" => ("Okna", match rest { "layouts" => "Układy przyciągania".into(), "stretch" => "Rozciągnij w pionie".into(), "unsnap" => "Odczep okno".into(), r => format!("Przyciągnij: {}", dir(r)) }),
            "zoom" => ("Dostępność", match rest { "in" => "Lupa: przybliż".into(), "out" => "Lupa: oddal".into(), _ => "Lupa: wyłącz".into() }),
            "workspace" => ("Pulpity", ws(rest, "Pulpit")),
            "movetoworkspace" => ("Pulpity", ws(rest, "Przenieś na pulpit")),
            "movetoworkspacesilent" => ("Pulpity", ws(rest, "Przenieś po cichu na pulpit")),
            "movetomonitor" => ("Okna", if rest.starts_with('-') { "Przenieś na poprzedni monitor".into() } else { "Przenieś na następny monitor".into() }),
            "vt" => ("System", format!("Konsola {rest}")),
            _ => ("Inne", a.to_string()),
        };
    };
    (s.0, s.1.to_string())
}

fn dir(d: &str) -> &'static str {
    match d {
        "left" => "w lewo",
        "right" => "w prawo",
        "up" => "w górę",
        "down" => "w dół",
        _ => "",
    }
}
fn ws(a: &str, what: &str) -> String {
    match a {
        "+1" | "m+1" | "r+1" => format!("{what}: następny"),
        "-1" | "m-1" | "r-1" => format!("{what}: poprzedni"),
        "empty" => "Nowy pusty pulpit".into(),
        "special" => format!("{what}: notatnik"),
        a => format!("{what} {}", a.trim_start_matches("m~")),
    }
}
