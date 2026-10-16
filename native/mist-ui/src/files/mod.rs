// Mist Files, native (was the Quickshell app ~/.config/quickshell/mist-files): Finder look (glass sidebar card,
// traffic lights, capsules), Windows Explorer behaviour (tabs, address bar, command bar, details / icons view,
// F2 / Del / Ctrl+C/X/V / Ctrl+Z, type-to-select, rubber band, drag and drop, preview pane).
// `mist-ui-bin files [path | file:// uri | --select file | --portal]`; one process for every window, it also
// serves the FileChooser portal backend (portal.rs) so "Open…" / "Save…" dialogs of other apps are this window.
pub mod fs;
pub mod portal;
mod view;

use crate::appwin::{self, Cmd, Cx, Ev, Logic, WinId, LEFT, MIDDLE, RIGHT};
use crate::edit::{EditResult, LineEdit};
use crate::text::{Style, Text};
use fs::{Entry, MimeDb};
use smithay_client_toolkit::seat::keyboard::Keysym;
use smithay_client_toolkit::seat::pointer::CursorIcon;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortKey {
    Name,
    Mtime,
    Type,
    Size,
}

pub struct Tab {
    pub uid: u64,
    pub path: String,
    pub back: Vec<String>,
    pub fwd: Vec<String>,
    pub show_hidden: bool,
    pub sort_key: SortKey,
    pub sort_desc: bool,
    pub filter: String,
    pub loading: bool,
    pub error: String,
    pub all: Vec<Entry>,
    pub items: Vec<usize>, // visible, sorted: indices into all
    pub sel: HashSet<String>,
    pub anchor: i32,
    pub cur: i32,
    pub pending_select: Vec<String>,
    pub pending_rename: String,
    pub scroll: f32,
    pub serial: u64,
    pub reload_at: Option<Instant>,
}
impl Tab {
    fn new(uid: u64, path: &str) -> Tab {
        Tab { uid, path: path.into(), back: vec![], fwd: vec![], show_hidden: false, sort_key: SortKey::Name, sort_desc: false, filter: String::new(), loading: false, error: String::new(), all: vec![], items: vec![], sel: HashSet::new(), anchor: -1, cur: -1, pending_select: vec![], pending_rename: String::new(), scroll: 0.0, serial: 0, reload_at: None }
    }
    pub fn is_trash(&self) -> bool {
        self.path.starts_with("trash:")
    }
    pub fn title(&self) -> String {
        fs::base(&self.path)
    }
    pub fn item(&self, i: usize) -> Option<&Entry> {
        self.items.get(i).map(|&k| &self.all[k])
    }
    pub fn selected(&self) -> Vec<&Entry> {
        self.items.iter().map(|&k| &self.all[k]).filter(|e| self.sel.contains(&e.path)).collect()
    }
    pub fn names(&self) -> Vec<String> {
        self.all.iter().map(|e| e.name.clone()).collect()
    }
    pub fn sel_size(&self) -> u64 {
        self.selected().iter().filter(|e| !e.dir).map(|e| e.size).sum()
    }
    pub fn clear_sel(&mut self) {
        self.sel.clear();
        self.anchor = -1;
        self.cur = -1;
    }
    pub fn select_index(&mut self, i: i32, ctrl: bool, shift: bool) {
        if i < 0 || i as usize >= self.items.len() {
            return;
        }
        let p = self.item(i as usize).unwrap().path.clone();
        if shift && self.anchor >= 0 {
            if !ctrl {
                self.sel.clear();
            }
            let (a, b) = (self.anchor.min(i), self.anchor.max(i));
            for j in a..=b {
                let q = self.item(j as usize).unwrap().path.clone();
                self.sel.insert(q);
            }
        } else if ctrl {
            if !self.sel.remove(&p) {
                self.sel.insert(p);
            }
            self.anchor = i;
        } else {
            self.sel.clear();
            self.sel.insert(p);
            self.anchor = i;
        }
        self.cur = i;
    }
    pub fn select_path(&mut self, p: &str) {
        if let Some(i) = self.items.iter().position(|&k| self.all[k].path == p) {
            self.select_index(i as i32, false, false);
        }
    }
    pub fn set_sel(&mut self, paths: &[String]) {
        self.sel = paths.iter().filter(|p| self.items.iter().any(|&k| &self.all[k].path == *p)).cloned().collect();
        if let Some(i) = paths.first().and_then(|p| self.items.iter().position(|&k| &self.all[k].path == p)) {
            self.cur = i as i32;
            self.anchor = i as i32;
        }
    }
    pub fn resort(&mut self, mime: &MimeDb, pick: Option<&Pick>) {
        let f = self.filter.to_lowercase();
        let mut v: Vec<usize> = (0..self.all.len())
            .filter(|&k| {
                let e = &self.all[k];
                (self.show_hidden || !e.hidden) && (f.is_empty() || e.name.to_lowercase().contains(&f)) && pick.map(|p| p.shows(e)).unwrap_or(true)
            })
            .collect();
        let (k, all) = (self.sort_key, &self.all);
        let desc = self.sort_desc;
        v.sort_by(|&a, &b| {
            let (x, y) = (&all[a], &all[b]);
            if x.dir != y.dir {
                return y.dir.cmp(&x.dir);
            }
            let r = match k {
                SortKey::Mtime => x.mtime.cmp(&y.mtime),
                SortKey::Size => x.size.cmp(&y.size),
                SortKey::Type => mime.type_name(x).cmp(&mime.type_name(y)),
                SortKey::Name => std::cmp::Ordering::Equal,
            };
            let r = r.then_with(|| fs::natural_cmp(&x.name, &y.name));
            if desc { r.reverse() } else { r }
        });
        self.items = v;
        let keep: HashSet<String> = self.items.iter().map(|&k| self.all[k].path.clone()).filter(|p| self.sel.contains(p)).collect();
        self.sel = keep;
        if self.cur >= self.items.len() as i32 {
            self.cur = self.items.len() as i32 - 1;
        }
    }
}

// ---------------------------------------------------------------- actions (menus, buttons, keys)
#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    OpenSel,
    OpenTab(String),
    OpenWindow(String),
    Terminal(Option<String>),
    Cut,
    Copy,
    Paste,
    PasteInto(String),
    Rename,
    CopyPath,
    Trash,
    Delete,
    DeleteNow,
    Restore,
    EmptyTrash,
    EmptyTrashNow,
    View(bool), // icons?
    Reload,
    Undo,
    NewFolder,
    NewFile,
    ToggleHidden,
    Sort(SortKey),
    SortDesc(bool),
    ToggleSidebar,
    TogglePreview,
    SelectAll,
    InvertSel,
    ClearSel,
    NewTab,
    Overwrite, // pick: save over an existing file
    MenuSort,
    MenuView,
    MenuNew,
    MenuMore,
    MenuFilter,
    SetFilter(usize),
}

pub struct MenuEntry {
    pub label: String,
    pub glyph: &'static str,
    pub keys: &'static str,
    pub act: Option<Act>,
    pub danger: bool,
    pub enabled: bool,
    pub checked: bool,
    pub sep: bool,
    pub head: bool,
}
fn me(label: &str, glyph: &'static str, keys: &'static str, act: Act) -> MenuEntry {
    MenuEntry { label: label.into(), glyph, keys, act: Some(act), danger: false, enabled: true, checked: false, sep: false, head: false }
}
fn sep() -> MenuEntry {
    MenuEntry { label: String::new(), glyph: "", keys: "", act: None, danger: false, enabled: false, checked: false, sep: true, head: false }
}
fn head(s: &str) -> MenuEntry {
    MenuEntry { label: s.into(), glyph: "", keys: "", act: None, danger: false, enabled: false, checked: false, sep: false, head: true }
}
impl MenuEntry {
    fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
    fn on(mut self, b: bool) -> Self {
        self.checked = b;
        self
    }
    fn enabled(mut self, b: bool) -> Self {
        self.enabled = b;
        self
    }
}

pub struct Menu {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub entries: Vec<MenuEntry>,
    pub hi: i32,
}
pub struct Ask {
    pub title: String,
    pub text: String,
    pub ok: String,
    pub act: Act,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    Light(u8),
    Drag,
    TabStrip,
    Place(String),
    Trash,
    Tab(usize),
    TabClose(usize),
    NewTab,
    Back,
    Fwd,
    Up,
    Reload,
    Addr,
    Crumb(String),
    Search,
    SearchClear,
    Btn(Act),
    Header(SortKey),
    View,
    ScrollBar,
    OpsCancel,
    MenuItem(usize),
    MenuOutside,
    Sheet,
    AskOk,
    AskCancel,
    PickOk,
    PickCancel,
    PickName,
    Preview,
    Side,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    View,
    Addr,
    Search,
    Rename,
    Name,
}

enum Press {
    Band { ax: f32, ay: f32, base: HashSet<String>, x: f32, y: f32 },
    Item { idx: usize, x0: f32, y0: f32, narrow: bool },
    Scroll { grab: f32 },
    Field,
}

// FileChooser request (portal.rs) shown in a window
pub struct Pick {
    pub save: bool,
    pub multiple: bool,
    pub directory: bool,
    pub title: String,
    pub accept: String,
    pub filters: Vec<(String, Vec<String>)>, // name, patterns ("*.png" or "mime:image/png")
    pub cur_filter: usize,
    pub reply: Option<std::sync::mpsc::Sender<Option<(Vec<String>, Option<usize>)>>>,
}
impl Pick {
    pub fn shows(&self, e: &Entry) -> bool {
        if e.dir {
            return true;
        }
        if self.directory {
            return false;
        }
        let Some((_, pats)) = self.filters.get(self.cur_filter) else { return true };
        pats.iter().any(|p| match p.strip_prefix("mime:") {
            Some(m) => m == e.mime || (m.ends_with("/*") && e.mime.starts_with(&m[..m.len() - 1])) || m == "application/octet-stream",
            None => glob(&p.to_lowercase(), &e.name.to_lowercase()),
        })
    }
}
fn glob(p: &str, s: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (p.chars().collect(), s.chars().collect());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < s.len() {
        if i < p.len() && (p[i] == '?' || p[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(st) = star {
            i = st + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

pub struct Win {
    pub id: WinId,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub w: f32,
    pub h: f32,
    pub hits: Vec<(f32, f32, f32, f32, Hit, &'static str)>,
    pub hover: Option<Hit>,
    pub mouse: (f32, f32),
    pub focus: Focus,
    pub addr: Option<LineEdit>,
    pub search: LineEdit,
    pub rename: Option<(String, LineEdit)>,
    pub menu: Option<Menu>,
    pub ask: Option<Ask>,
    pub toast: String,
    pub toast_until: Instant,
    typed: String,
    typed_at: Instant,
    press: Option<Press>,
    pub band: Option<(f32, f32, f32, f32)>,
    pub drop: Option<String>, // drop target path ("" = the view's folder, "trash:" = trash)
    pub tip: Option<(String, f32, f32)>,
    hover_at: Instant,
    pub side_scroll: f32,
    pub pick: Option<Pick>,
    pub name: LineEdit,
    pub focused: bool,
}
impl Win {
    pub fn cur(&self) -> &Tab {
        &self.tabs[self.active]
    }
    pub fn cur_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }
}

// a copy / move job (one at a time, the rest queued)
struct Job {
    pairs: Vec<(String, String)>,
    mv: bool,
    title: String,
    win: WinId,
    dest: String,
    after_cut: bool,
}
pub struct Running {
    pub title: String,
    pub total: u64,
    pub pr: fs::Progress,
    win: WinId,
    dest: String,
    after_cut: bool,
    mv: bool,
}
#[derive(Clone)]
enum Undo {
    Mv(Vec<(String, String)>), // from, to
    Trash(Vec<String>),
    Restore(Vec<String>), // original paths
}

// messages from worker threads
enum Bg {
    Listed { uid: u64, serial: u64, res: Result<Vec<Entry>, String> },
    Total(u64),
    JobDone { made: Vec<(String, String)>, cancelled: bool, err: String },
    Thumb { key: (String, u32), pm: Option<Pixmap> },
    FolderInfo { path: String, n: usize, size: u64 },
    Done { win: WinId, msg: String, reload: bool, select: Vec<String> },
    Watch,
    Pick(portal::Request),
}

pub struct Files {
    pub wins: Vec<Win>,
    pub icons_view: bool,
    pub sidebar: bool,
    pub preview: bool,
    pub clip: Vec<String>,
    pub clip_cut: bool,
    pub mime: MimeDb,
    pub text: Text,
    pub icons: crate::apps::Icons,
    icon_cache: HashMap<(String, u32), Option<Rc<Pixmap>>>,
    mime_icon: HashMap<String, String>,
    // thumbnails with the tick they were last asked for; at most THUMB_BYTES of pixels, least recent go first
    pub thumbs: HashMap<(String, u32), (Option<Rc<Pixmap>>, u64)>,
    thumb_bytes: usize,
    thumb_tick: u64,
    thumb_tx: std::sync::mpsc::Sender<(String, i64, u32, String)>,
    pub drives: Vec<fs::Drive>,
    drives_at: Instant,
    pub xdg: HashMap<String, String>,
    pub places: Vec<(String, String, &'static str)>, // name, path, glyph
    queue: Vec<Job>,
    pub job: Option<Running>,
    undo: Vec<(String, Undo)>,
    next_uid: u64,
    pending: Vec<(u64, PendingWin)>,
    tx: Option<calloop::channel::Sender<Box<dyn Any + Send>>>,
    watch: Option<Watch>,
    pub folder_info: HashMap<String, (usize, u64)>,
    portal_only: bool,
    idle_since: Option<Instant>,
    pending_picks: Vec<portal::Request>,
}
struct PendingWin {
    path: String,
    select: String,
    pick: Option<Pick>,
    name: String,
}

// inotify on the folders shown in the windows (one fd, a watch per folder)
struct Watch {
    fd: i32,
    wds: HashMap<i32, String>,
}
impl Watch {
    fn sync(&mut self, want: &HashSet<String>) {
        let have: Vec<(i32, String)> = self.wds.iter().map(|(k, v)| (*k, v.clone())).collect();
        for (wd, p) in have {
            if !want.contains(&p) {
                unsafe { libc::inotify_rm_watch(self.fd, wd) };
                self.wds.remove(&wd);
            }
        }
        for p in want {
            if self.wds.values().any(|v| v == p) {
                continue;
            }
            let c = std::ffi::CString::new(p.as_str()).unwrap_or_default();
            let m = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_CLOSE_WRITE | libc::IN_ATTRIB | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF;
            let wd = unsafe { libc::inotify_add_watch(self.fd, c.as_ptr(), m) };
            if wd >= 0 {
                self.wds.insert(wd, p.clone());
            }
        }
    }
}

pub const W0: u32 = 1180;
pub const H0: u32 = 740;
// thumbnail pixels kept in memory (~190 at 256 px, ~760 at 128 px)
const THUMB_BYTES: usize = 48 << 20;

impl Files {
    pub fn new() -> Files {
        let (thumb_tx, thumb_rx) = std::sync::mpsc::channel::<(String, i64, u32, String)>();
        let mut f = Files {
            wins: vec![],
            icons_view: false,
            sidebar: true,
            preview: false,
            clip: vec![],
            clip_cut: false,
            mime: MimeDb::load(),
            text: Text::new(),
            icons: crate::apps::Icons::new(),
            icon_cache: HashMap::new(),
            mime_icon: HashMap::new(),
            thumbs: HashMap::new(),
            thumb_bytes: 0,
            thumb_tick: 0,
            thumb_tx,
            drives: fs::drives(),
            drives_at: Instant::now(),
            xdg: fs::xdg_dirs(),
            places: vec![],
            queue: vec![],
            job: None,
            undo: vec![],
            next_uid: 1,
            pending: vec![],
            tx: None,
            watch: None,
            folder_info: HashMap::new(),
            portal_only: false,
            idle_since: None,
            pending_picks: vec![],
        };
        let h = fs::home();
        let x = |k: &str, d: &str| f.xdg.get(k).cloned().unwrap_or(format!("{h}/{d}"));
        f.places = vec![
            ("Katalog domowy".into(), h.clone(), "home"),
            ("Pulpit".into(), x("DESKTOP", "Pulpit"), "desktop"),
            ("Dokumenty".into(), x("DOCUMENTS", "Dokumenty"), "doc"),
            ("Pobrane".into(), x("DOWNLOAD", "Pobrane"), "download"),
            ("Obrazy".into(), x("PICTURES", "Obrazy"), "photo"),
            ("Muzyka".into(), x("MUSIC", "Muzyka"), "music"),
            ("Wideo".into(), x("VIDEOS", "Wideo"), "movie"),
        ];
        // view settings shared by the windows, kept across runs
        if let Ok(s) = std::fs::read_to_string(cfg_path()) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                f.icons_view = v["view"].as_str() == Some("icons");
                f.sidebar = v["sidebar"].as_bool().unwrap_or(true);
                f.preview = v["preview"].as_bool().unwrap_or(false);
            }
        }
        // thumbnails: one worker (decode, cache lookup, ffmpegthumbnailer); results come back through tx
        f.spawn_thumbs(thumb_rx);
        f
    }
    fn save_cfg(&self) {
        let v = serde_json::json!({ "view": if self.icons_view { "icons" } else { "details" }, "sidebar": self.sidebar, "preview": self.preview });
        let _ = std::fs::create_dir_all(std::path::Path::new(&cfg_path()).parent().unwrap());
        let _ = std::fs::write(cfg_path(), v.to_string());
    }
    fn spawn_thumbs(&mut self, rx: std::sync::mpsc::Receiver<(String, i64, u32, String)>) {
        let slot: std::sync::Arc<std::sync::Mutex<Option<calloop::channel::Sender<Box<dyn Any + Send>>>>> = Default::default();
        THUMB_TX.with(|t| *t.borrow_mut() = Some(slot.clone()));
        std::thread::Builder::new()
            .name("thumbs".into())
            .spawn(move || {
                let mut q = std::collections::VecDeque::new();
                loop {
                    if q.is_empty() {
                        // decoded pictures leave big free blocks: give them back before waiting
                        unsafe { libc::malloc_trim(0) };
                        match rx.recv() {
                            Ok(j) => q.push_back(j),
                            Err(_) => break,
                        }
                    }
                    q.extend(rx.try_iter());
                    let Some((path, mtime, size, mime)) = q.pop_front() else { continue };
                    let pm = make_thumb(&path, mtime, size, &mime);
                    if let Some(tx) = slot.lock().unwrap().as_ref() {
                        let _ = tx.send(Box::new(Bg::Thumb { key: (path, size), pm }));
                    }
                }
            })
            .ok();
    }
    fn bind_tx(&mut self, cx: &Cx) {
        if self.tx.is_none() {
            self.tx = Some(cx.tx.clone());
            THUMB_TX.with(|t| {
                if let Some(s) = t.borrow().as_ref() {
                    *s.lock().unwrap() = Some(cx.tx.clone());
                }
            });
            // inotify fd: a thread blocks on it and pings the loop (the folder reloads after 250 ms of quiet)
            let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if fd >= 0 {
                self.watch = Some(Watch { fd, wds: HashMap::new() });
                let tx = cx.tx.clone();
                let rfd = unsafe { libc::dup(fd) };
                std::thread::Builder::new()
                    .name("inotify".into())
                    .spawn(move || {
                        let mut buf = [0u8; 4096];
                        loop {
                            let n = unsafe { libc::read(rfd, buf.as_mut_ptr() as *mut _, buf.len()) };
                            if n <= 0 {
                                break;
                            }
                            // which watches fired
                            let mut wds = Vec::new();
                            let mut o = 0usize;
                            while o + 16 <= n as usize {
                                let wd = i32::from_ne_bytes(buf[o..o + 4].try_into().unwrap());
                                let len = u32::from_ne_bytes(buf[o + 12..o + 16].try_into().unwrap()) as usize;
                                wds.push(wd);
                                o += 16 + len;
                            }
                            WATCH_HITS.lock().unwrap().extend(wds);
                            if tx.send(Box::new(Bg::Watch)).is_err() {
                                break;
                            }
                        }
                    })
                    .ok();
            }
        }
    }
    fn send(&self, b: Bg) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Box::new(b));
        }
    }
    fn win_mut(&mut self, id: WinId) -> Option<&mut Win> {
        self.wins.iter_mut().find(|w| w.id == id)
    }
    fn wi(&self, id: WinId) -> Option<usize> {
        self.wins.iter().position(|w| w.id == id)
    }

    // ------------------------------------------------------------ windows
    pub fn open_window(&mut self, path: &str, select: &str, pick: Option<Pick>, name: &str, cx: &mut Cx) {
        let (w, h) = if pick.is_some() { (980, 640) } else { (W0, H0) };
        let title = pick.as_ref().map(|p| p.title.clone()).unwrap_or("Pliki".into());
        let token = cx.new_window(&title, w, h, (760, 480));
        self.pending.push((token, PendingWin { path: path.into(), select: select.into(), pick, name: name.into() }));
        self.idle_since = None;
    }
    fn close_window(&mut self, id: WinId, cx: &mut Cx) {
        if let Some(i) = self.wi(id) {
            let w = self.wins.remove(i);
            // an unanswered dialog is a cancel
            if let Some(p) = w.pick {
                if let Some(r) = p.reply {
                    let _ = r.send(None);
                }
            }
        }
        cx.cmds.push(Cmd::Close(id));
        self.sync_watch();
        if self.wins.is_empty() && self.job.is_none() {
            // the portal keeps the process for a while (the next dialog opens instantly)
            self.idle_since = Some(Instant::now());
            cx.wake_at(Instant::now() + Duration::from_secs(if portal::OWNED.load(Ordering::Relaxed) { 120 } else { 0 }));
        }
    }
    pub fn new_tab(&mut self, wid: WinId, path: &str, focus: bool, cx: &mut Cx) {
        let uid = self.next_uid;
        self.next_uid += 1;
        let Some(w) = self.win_mut(wid) else { return };
        w.tabs.push(Tab::new(uid, path));
        if focus {
            w.active = w.tabs.len() - 1;
            w.search.set("");
        }
        let n = w.tabs.len() - 1;
        self.reload(wid, n);
        self.sync_watch();
        self.title(wid, cx);
        cx.redraw(wid);
    }
    fn close_tab(&mut self, wid: WinId, i: usize, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        if w.tabs.len() <= 1 {
            self.close_window(wid, cx);
            return;
        }
        w.tabs.remove(i);
        if w.active >= w.tabs.len() {
            w.active = w.tabs.len() - 1;
        } else if i < w.active {
            w.active -= 1;
        }
        let f = w.cur().filter.clone();
        w.search.set(&f);
        self.sync_watch();
        self.title(wid, cx);
        cx.redraw(wid);
    }
    fn set_active(&mut self, wid: WinId, i: usize, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        if i >= w.tabs.len() {
            return;
        }
        w.active = i;
        w.rename = None;
        w.focus = Focus::View;
        let f = w.cur().filter.clone();
        w.search.set(&f);
        self.sync_watch();
        self.title(wid, cx);
        cx.redraw(wid);
    }
    fn title(&mut self, wid: WinId, cx: &mut Cx) {
        if let Some(w) = self.wins.iter().find(|w| w.id == wid) {
            let t = match &w.pick {
                Some(p) => p.title.clone(),
                None => format!("Pliki — {}", w.cur().title()),
            };
            cx.cmds.push(Cmd::Title(wid, t));
        }
    }
    fn sync_watch(&mut self) {
        let mut want = HashSet::new();
        for w in &self.wins {
            let p = &w.cur().path;
            want.insert(if p.starts_with("trash:") { format!("{}/files", fs::trash_dir()) } else { p.clone() });
        }
        if let Some(wt) = self.watch.as_mut() {
            wt.sync(&want);
        }
    }

    // ------------------------------------------------------------ navigation + listing
    pub fn go(&mut self, wid: WinId, p: &str, mode: u8, cx: &mut Cx) {
        // mode: 0 push history, 1 back, 2 fwd
        let Some(w) = self.win_mut(wid) else { return };
        let mut p = p.to_string();
        if p.len() > 1 && !p.starts_with("trash:") {
            p = p.trim_end_matches('/').to_string();
        }
        w.rename = None;
        w.addr = None;
        w.search.set("");
        if w.focus != Focus::Name {
            w.focus = Focus::View;
        }
        let t = w.cur_mut();
        if p == t.path {
            let a = w.active;
            self.reload(wid, a);
            return;
        }
        if mode == 0 {
            let old = t.path.clone();
            t.back.push(old);
            t.fwd.clear();
        }
        t.filter.clear();
        t.clear_sel();
        t.path = p;
        t.all.clear();
        t.items.clear();
        t.scroll = 0.0;
        let a = w.active;
        self.reload(wid, a);
        self.sync_watch();
        self.title(wid, cx);
        cx.redraw(wid);
    }
    fn go_back(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        let t = w.cur_mut();
        let Some(p) = t.back.pop() else { return };
        let cur = t.path.clone();
        t.fwd.push(cur.clone());
        t.pending_select = vec![cur];
        self.go(wid, &p, 1, cx);
    }
    fn go_fwd(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        let t = w.cur_mut();
        let Some(p) = t.fwd.pop() else { return };
        let cur = t.path.clone();
        t.back.push(cur);
        self.go(wid, &p, 2, cx);
    }
    fn go_up(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        let p = fs::parent(&w.cur().path);
        if p.is_empty() {
            return;
        }
        let cur = w.cur().path.clone();
        w.cur_mut().pending_select = vec![cur];
        self.go(wid, &p, 0, cx);
    }
    pub fn reload(&mut self, wid: WinId, tab: usize) {
        let tx = self.tx.clone();
        let Some(w) = self.win_mut(wid) else { return };
        let Some(t) = w.tabs.get_mut(tab) else { return };
        t.serial += 1;
        t.loading = true;
        t.reload_at = None;
        let (uid, serial, path) = (t.uid, t.serial, t.path.clone());
        let Some(tx) = tx else { return };
        std::thread::Builder::new()
            .name("list".into())
            .spawn(move || {
                let res = MIME.with(|m| fs::list(&path, m));
                let _ = tx.send(Box::new(Bg::Listed { uid, serial, res }));
            })
            .ok();
    }
    fn reload_path(&mut self, path: &str) {
        let mut todo = vec![];
        for w in &self.wins {
            for (i, t) in w.tabs.iter().enumerate() {
                if t.path == path {
                    todo.push((w.id, i));
                }
            }
        }
        for (w, i) in todo {
            self.reload(w, i);
        }
    }
    fn listed(&mut self, uid: u64, serial: u64, res: Result<Vec<Entry>, String>, cx: &mut Cx) {
        let mime = &self.mime;
        for w in self.wins.iter_mut() {
            let wid = w.id;
            let pick = w.pick.as_ref();
            let Some(t) = w.tabs.iter_mut().find(|t| t.uid == uid) else { continue };
            if t.serial != serial {
                return;
            }
            t.loading = false;
            match res {
                Ok(v) => {
                    t.error.clear();
                    t.all = v;
                }
                Err(e) => {
                    t.error = e;
                    t.all.clear();
                }
            }
            t.resort(mime, pick);
            if !t.pending_select.is_empty() {
                let ps = std::mem::take(&mut t.pending_select);
                t.set_sel(&ps);
                let cur = t.cur;
                if cur >= 0 {
                    self.show_index(wid, cur as usize);
                }
            }
            if let Some(w) = self.win_mut(wid) {
                let t = w.tabs.iter_mut().find(|t| t.uid == uid).unwrap();
                if !t.pending_rename.is_empty() {
                    let p = std::mem::take(&mut t.pending_rename);
                    t.select_path(&p);
                    let name = t.all.iter().find(|e| e.path == p).map(|e| (e.name.clone(), e.dir));
                    if let Some((n, dir)) = name {
                        let mut le = LineEdit::new(&n);
                        let stem = if dir { n.len() } else { n.rfind('.').filter(|&i| i > 0).unwrap_or(n.len()) };
                        le.select(0, stem);
                        w.rename = Some((p, le));
                        w.focus = Focus::Rename;
                    }
                }
            }
            cx.redraw(wid);
            return;
        }
    }
    // keep item i in view
    fn show_index(&mut self, wid: WinId, i: usize) {
        let icons = self.icons_view;
        let preview = self.preview;
        let sidebar = self.sidebar;
        let Some(w) = self.win_mut(wid) else { return };
        let (_, _, pw, ph) = view::panel_rect(w, sidebar, preview);
        let t = w.cur_mut();
        let (top, h) = if icons {
            let cols = ((pw - 20.0) / view::CELL_W).floor().max(1.0) as usize;
            (10.0 + (i / cols) as f32 * view::CELL_H, view::CELL_H)
        } else {
            (view::HEAD_H + i as f32 * view::ROW_H, view::ROW_H)
        };
        let vis_top = if icons { 0.0 } else { view::HEAD_H };
        if top - t.scroll < vis_top {
            t.scroll = (top - vis_top).max(0.0);
        } else if top + h - t.scroll > ph - 6.0 {
            t.scroll = top + h - ph + 6.0;
        }
    }

    // ------------------------------------------------------------ file operations
    fn say(&mut self, wid: WinId, s: &str, cx: &mut Cx) {
        if let Some(w) = self.win_mut(wid) {
            w.toast = s.into();
            w.toast_until = Instant::now() + Duration::from_millis(3500);
            cx.wake_at(w.toast_until);
            cx.redraw(wid);
        }
    }
    fn push_undo(&mut self, label: String, u: Undo) {
        self.undo.push((label, u));
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
    }
    fn open_entry(&mut self, wid: WinId, e: &Entry, new_tab: bool, cx: &mut Cx) {
        if e.dir {
            if new_tab {
                self.new_tab(wid, &e.path, false, cx);
            } else {
                self.go(wid, &e.path, 0, cx);
            }
        } else if !e.path.starts_with("trash:") {
            open_file(&e.path);
        }
    }
    fn open_sel(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let s: Vec<Entry> = w.cur().selected().into_iter().cloned().collect();
        if w.pick.is_some() {
            self.pick_accept(wid, cx);
            return;
        }
        if s.len() == 1 && s[0].dir {
            self.go(wid, &s[0].path, 0, cx);
            return;
        }
        let many = s.len() > 1;
        for e in &s {
            self.open_entry(wid, e, many, cx);
        }
    }
    fn new_item(&mut self, wid: WinId, dir: bool, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let t = w.cur();
        if t.is_trash() {
            return;
        }
        let n = fs::free_name(&t.names(), if dir { "Nowy folder" } else { "Nowy dokument tekstowy.txt" }, false, dir);
        let p = fs::join(&t.path, &n);
        let ok = if dir { std::fs::create_dir(&p).is_ok() } else { std::fs::OpenOptions::new().write(true).create_new(true).open(&p).is_ok() };
        if ok {
            self.push_undo(if dir { "nowy folder".into() } else { "nowy plik".into() }, Undo::Trash(vec![p.clone()]));
            let w = self.win_mut(wid).unwrap();
            w.cur_mut().pending_rename = p;
            let a = w.active;
            self.reload(wid, a);
        } else {
            self.say(wid, if dir { "Nie można utworzyć folderu" } else { "Nie można utworzyć pliku" }, cx);
        }
    }
    fn start_rename(&mut self, wid: WinId) {
        let Some(w) = self.win_mut(wid) else { return };
        if w.cur().is_trash() {
            return;
        }
        let s: Vec<(String, String, bool)> = w.cur().selected().iter().map(|e| (e.path.clone(), e.name.clone(), e.dir)).collect();
        if s.len() != 1 {
            return;
        }
        let (p, n, dir) = s[0].clone();
        let mut le = LineEdit::new(&n);
        let stem = if dir { n.len() } else { n.rfind('.').filter(|&i| i > 0).unwrap_or(n.len()) };
        le.select(0, stem);
        w.rename = Some((p, le));
        w.focus = Focus::Rename;
    }
    fn finish_rename(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        let Some((p, le)) = w.rename.take() else { return };
        w.focus = Focus::View;
        let name = le.text.trim().to_string();
        let old = fs::base(&p);
        let old = if p.rsplit('/').next() == Some(old.as_str()) { old } else { p.rsplit('/').next().unwrap_or("").to_string() };
        cx.redraw(wid);
        if name.is_empty() || name == old {
            return;
        }
        if name.contains('/') {
            self.say(wid, "Nazwa nie może zawierać znaku /", cx);
            return;
        }
        let w = self.win_mut(wid).unwrap();
        if w.cur().names().iter().any(|n| n == &name && n.to_lowercase() != old.to_lowercase()) {
            let m = format!("Element o nazwie „{name}” już istnieje");
            self.say(wid, &m, cx);
            return;
        }
        let np = fs::join(&fs::parent(&p), &name);
        // no clobbering (mv -n): a different-case rename of the same file is allowed
        let exists = std::fs::symlink_metadata(&np).is_ok() && name.to_lowercase() != old.to_lowercase();
        if !exists && std::fs::rename(&p, &np).is_ok() {
            self.push_undo(format!("zmiana nazwy „{old}”"), Undo::Mv(vec![(np.clone(), p)]));
            let w = self.win_mut(wid).unwrap();
            w.cur_mut().pending_select = vec![np];
            let a = w.active;
            self.reload(wid, a);
        } else {
            self.say(wid, "Nie można zmienić nazwy", cx);
        }
    }
    fn trash_sel(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        if w.cur().is_trash() {
            self.ask_delete(wid);
            cx.redraw(wid);
            return;
        }
        let s: Vec<(String, String)> = w.cur().selected().iter().map(|e| (e.path.clone(), e.name.clone())).collect();
        if s.is_empty() {
            return;
        }
        let paths: Vec<String> = s.iter().map(|x| x.0.clone()).collect();
        self.trash_paths(wid, paths, if s.len() == 1 { Some(s[0].1.clone()) } else { None }, cx);
    }
    fn trash_paths(&mut self, wid: WinId, paths: Vec<String>, one: Option<String>, cx: &mut Cx) {
        let tx = self.tx.clone();
        let n = paths.len();
        let one2 = one.clone();
        std::thread::spawn(move || {
            let one = one2;
            let r = fs::trash(&paths);
            if let Some(tx) = tx {
                let msg = match (&r, &one) {
                    (Ok(()), Some(o)) => format!("Przeniesiono do kosza: {o}"),
                    (Ok(()), None) => format!("Przeniesiono do kosza: {n} elem."),
                    (Err(e), _) => e.clone(),
                };
                let _ = tx.send(Box::new(Bg::Done { win: wid, msg, reload: true, select: vec![] }));
            }
        });
        let label = match &one {
            Some(o) => format!("usunięcie „{o}”"),
            None => format!("usunięcie {n} elem."),
        };
        UNDO_PENDING.with(|u| *u.borrow_mut() = Some((label, Undo::Restore(vec![]))));
        let _ = cx;
    }
    fn ask_delete(&mut self, wid: WinId) {
        let Some(w) = self.win_mut(wid) else { return };
        let s = w.cur().selected();
        if s.is_empty() {
            return;
        }
        let text = if s.len() == 1 { format!("„{}” zostanie usunięty na zawsze. Tej operacji nie można cofnąć.", s[0].name) } else { format!("{} elementów zostanie usuniętych na zawsze. Tej operacji nie można cofnąć.", s.len()) };
        w.ask = Some(Ask { title: "Usunąć trwale?".into(), text, ok: "Usuń".into(), act: Act::DeleteNow });
    }
    fn delete_now(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let tr = w.cur().is_trash();
        let paths: Vec<String> = w.cur().selected().iter().map(|e| e.path.clone()).collect();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut ok = true;
            if tr {
                fs::trash_remove(&paths.iter().map(|p| p.trim_start_matches("trash:///").to_string()).collect::<Vec<_>>());
            } else {
                for p in &paths {
                    ok &= fs::remove_tree(p).is_ok();
                }
            }
            if let Some(tx) = tx {
                let _ = tx.send(Box::new(Bg::Done { win: wid, msg: if ok { "Usunięto".into() } else { "Nie udało się usunąć".into() }, reload: true, select: vec![] }));
            }
        });
        cx.redraw(wid);
    }
    fn restore_sel(&mut self, wid: WinId) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let names: Vec<String> = w.cur().selected().iter().map(|e| e.path.trim_start_matches("trash:///").to_string()).collect();
        if names.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ok = fs::trash_restore(&names);
            if let Some(tx) = tx {
                let _ = tx.send(Box::new(Bg::Done { win: wid, msg: if ok { "Przywrócono".into() } else { "Nie udało się przywrócić".into() }, reload: true, select: vec![] }));
            }
        });
    }
    fn empty_trash(&mut self, wid: WinId) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            fs::trash_empty();
            if let Some(tx) = tx {
                let _ = tx.send(Box::new(Bg::Done { win: wid, msg: "Kosz opróżniony".into(), reload: true, select: vec![] }));
            }
        });
    }
    fn copy_sel(&mut self, wid: WinId, cut: bool, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        if w.cur().is_trash() {
            return;
        }
        let s: Vec<(String, String)> = w.cur().selected().iter().map(|e| (e.path.clone(), e.name.clone())).collect();
        if s.is_empty() {
            return;
        }
        self.clip = s.iter().map(|x| x.0.clone()).collect();
        self.clip_cut = cut;
        let uris = self.clip.iter().map(|p| appwin::path_to_uri(p)).collect::<Vec<_>>().join("\r\n");
        set_clipboard_uris(&uris);
        let m = format!("{}{}", if cut { "Wycięto: " } else { "Skopiowano: " }, if s.len() == 1 { s[0].1.clone() } else { format!("{} elem.", s.len()) });
        self.say(wid, &m, cx);
        for w in &self.wins {
            cx.redraw(w.id);
        }
    }
    fn paste(&mut self, wid: WinId, dest: Option<String>, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        if w.cur().is_trash() {
            return;
        }
        let dest = dest.unwrap_or(w.cur().path.clone());
        let (srcs, cut) = if !self.clip.is_empty() {
            (self.clip.clone(), self.clip_cut)
        } else {
            let out = std::process::Command::new("wl-paste").args(["-n", "-t", "text/uri-list"]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            (out.lines().filter(|u| u.starts_with("file://")).map(|u| appwin::uri_to_path(u.trim())).collect(), false)
        };
        self.transfer(wid, srcs, &dest, cut, cut, cx);
    }
    // copy / move paths into dest (paste, drop): Windows names for clashes, progress and undo
    fn transfer(&mut self, wid: WinId, srcs: Vec<String>, dest: &str, mv: bool, after_cut: bool, cx: &mut Cx) {
        if srcs.is_empty() || dest.starts_with("trash:") {
            return;
        }
        let mut names: Vec<String> = std::fs::read_dir(dest).map(|r| r.flatten().map(|d| d.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        let mut pairs = vec![];
        for src in &srcs {
            let nm = src.rsplit('/').next().unwrap_or("").to_string();
            if mv && fs::parent(src) == dest {
                continue;
            }
            let is_dir = std::fs::metadata(src).map(|m| m.is_dir()).unwrap_or(false);
            let n = if mv && !names.contains(&nm) { nm } else { fs::free_name(&names, &nm, !mv, is_dir) };
            names.push(n.clone());
            let dst = fs::join(dest, &n);
            if &dst == src || dst.starts_with(&format!("{src}/")) {
                continue;
            }
            pairs.push((src.clone(), dst));
        }
        if pairs.is_empty() {
            return;
        }
        let n = pairs.len();
        let title = format!("{}{}", if mv { "Przenoszenie " } else { "Kopiowanie " }, if n == 1 { format!("„{}”", pairs[0].0.rsplit('/').next().unwrap_or("")) } else { format!("{n} elem.") });
        self.queue.push(Job { pairs, mv, title, win: wid, dest: dest.into(), after_cut });
        if self.job.is_none() {
            self.next_job(cx);
        }
    }
    fn next_job(&mut self, cx: &mut Cx) {
        if self.queue.is_empty() {
            self.job = None;
            return;
        }
        let j = self.queue.remove(0);
        let pr = fs::Progress::default();
        self.job = Some(Running { title: j.title.clone(), total: 0, pr: pr.clone(), win: j.win, dest: j.dest.clone(), after_cut: j.after_cut, mv: j.mv });
        let tx = self.tx.clone();
        let (pairs, mv) = (j.pairs, j.mv);
        std::thread::Builder::new()
            .name("copy".into())
            .spawn(move || {
                let total: u64 = pairs.iter().map(|(s, _)| fs::du(s, None)).sum();
                if let Some(tx) = &tx {
                    let _ = tx.send(Box::new(Bg::Total(total)));
                }
                let mut made = vec![];
                let mut err = String::new();
                for (s, d) in &pairs {
                    if pr.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let r = if mv { fs::move_tree(s, d, &pr) } else { fs::copy_tree(s, d, &pr) };
                    match r {
                        Ok(()) => made.push((s.clone(), d.clone())),
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            let _ = fs::remove_tree(d);
                        }
                        Err(_) => err = format!("Błąd przy: {}", s.rsplit('/').next().unwrap_or("")),
                    }
                }
                let cancelled = pr.cancel.load(Ordering::Relaxed);
                if let Some(tx) = tx {
                    let _ = tx.send(Box::new(Bg::JobDone { made, cancelled, err }));
                }
            })
            .ok();
        cx.wake_at(Instant::now() + Duration::from_millis(250));
        for w in &self.wins {
            cx.redraw(w.id);
        }
    }
    fn undo_last(&mut self, wid: WinId, cx: &mut Cx) {
        if self.undo.is_empty() || self.job.is_some() {
            self.say(wid, "Nie ma czego cofnąć", cx);
            return;
        }
        let (label, u) = self.undo.pop().unwrap();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let msg = format!("Cofnięto: {label}");
            let msg = match u {
                Undo::Mv(pairs) => {
                    for (a, b) in pairs {
                        let _ = fs::move_tree(&a, &b, &fs::Progress::default());
                    }
                    msg
                }
                Undo::Trash(paths) => {
                    let _ = fs::trash(&paths);
                    msg
                }
                Undo::Restore(origs) => {
                    let names = fs::trash_find(&origs);
                    if names.is_empty() {
                        "Nie znaleziono elementów w koszu".into()
                    } else {
                        fs::trash_restore(&names);
                        msg
                    }
                }
            };
            if let Some(tx) = tx {
                let _ = tx.send(Box::new(Bg::Done { win: wid, msg, reload: true, select: vec![] }));
            }
        });
    }
    fn copy_path(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let s = w.cur().selected();
        let t = if s.is_empty() { w.cur().path.clone() } else { s.iter().map(|e| e.path.clone()).collect::<Vec<_>>().join("\n") };
        crate::edit::copy_text(&t);
        self.say(wid, "Skopiowano ścieżkę", cx);
    }
    fn resort_all(&mut self, wid: WinId) {
        let mime = &self.mime;
        if let Some(w) = self.wins.iter_mut().find(|w| w.id == wid) {
            let pick = w.pick.as_ref();
            for t in w.tabs.iter_mut() {
                t.resort(mime, pick);
            }
        }
    }

    // ------------------------------------------------------------ drag and drop
    fn drop_into(&mut self, wid: WinId, uris: Vec<String>, dest: &str, cx: &mut Cx) {
        let srcs: Vec<String> = uris.iter().filter(|u| u.starts_with("file://")).map(|u| appwin::uri_to_path(u)).filter(|p| p != dest && !dest.starts_with(&format!("{p}/"))).collect();
        if srcs.is_empty() {
            return;
        }
        if dest.starts_with("trash:") {
            let n = srcs.len();
            self.trash_paths(wid, srcs, None, cx);
            let _ = n;
            return;
        }
        // Explorer: move inside one filesystem, copy to another; Ctrl forces a copy
        let mv = !cx.mods.ctrl && fs::same_fs(&srcs[0], dest);
        self.transfer(wid, srcs, dest, mv, false, cx);
    }

    // ------------------------------------------------------------ menus
    fn open_menu(&mut self, wid: WinId, x: f32, y: f32, entries: Vec<MenuEntry>, cx: &mut Cx) {
        let t = &mut self.text;
        let st = Style::ui(13.0, 450.0);
        let ks = Style::ui(12.0, 450.0);
        let mut mw: f32 = 220.0;
        for e in &entries {
            mw = mw.max(t.width(&e.label, &st) + t.width(e.keys, &ks) + 44.0 + 28.0 + 12.0);
        }
        let h: f32 = entries.iter().map(|e| if e.sep { 9.0 } else if e.head { 24.0 } else { 30.0 }).sum::<f32>() + 12.0;
        let Some(w) = self.win_mut(wid) else { return };
        let x = x.min(w.w - mw - 8.0).max(8.0);
        let y = y.min(w.h - h - 8.0).max(8.0);
        w.menu = Some(Menu { x, y, w: mw, entries, hi: -1 });
        w.tip = None;
        cx.redraw(wid);
    }
    fn item_menu(&mut self, wid: WinId, x: f32, y: f32, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let s = w.cur().selected();
        let one = if s.len() == 1 { Some(s[0].clone()) } else { None };
        let entries = if w.cur().is_trash() {
            vec![me("Przywróć", "undo", "", Act::Restore), me("Usuń trwale", "trash", "Del", Act::Delete).danger()]
        } else if w.pick.is_some() {
            vec![me("Otwórz", "open", "Enter", Act::OpenSel), sep(), me("Zmień nazwę", "rename", "F2", Act::Rename).enabled(one.is_some()), me("Kopiuj ścieżkę", "link", "Ctrl+Shift+C", Act::CopyPath)]
        } else {
            let dir = one.as_ref().filter(|e| e.dir).map(|e| e.path.clone());
            let mut v = vec![me("Otwórz", "open", "Enter", Act::OpenSel)];
            if let Some(p) = &dir {
                v.push(me("Otwórz w nowej karcie", "plus", "", Act::OpenTab(p.clone())));
                v.push(me("Otwórz w nowym oknie", "app", "", Act::OpenWindow(p.clone())));
                v.push(me("Otwórz w terminalu", "terminal", "", Act::Terminal(Some(p.clone()))));
            }
            v.push(sep());
            v.push(me("Wytnij", "cut", "Ctrl+X", Act::Cut));
            v.push(me("Kopiuj", "copy", "Ctrl+C", Act::Copy));
            if let Some(p) = &dir {
                v.push(me("Wklej do folderu", "paste", "", Act::PasteInto(p.clone())).enabled(!self.clip.is_empty()));
            }
            v.push(sep());
            v.push(me("Zmień nazwę", "rename", "F2", Act::Rename).enabled(one.is_some()));
            v.push(me("Kopiuj ścieżkę", "link", "Ctrl+Shift+C", Act::CopyPath));
            v.push(sep());
            v.push(me("Przenieś do kosza", "trash", "Del", Act::Trash).danger());
            v
        };
        self.open_menu(wid, x, y, entries, cx);
    }
    fn bg_menu(&mut self, wid: WinId, x: f32, y: f32, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let t = w.cur();
        let entries = if t.is_trash() {
            vec![me("Opróżnij kosz", "trash", "", Act::EmptyTrash).danger(), me("Odśwież", "refresh", "F5", Act::Reload)]
        } else {
            let u = self.undo_label();
            vec![
                head("Widok"),
                me("Szczegóły", "list", "", Act::View(false)).on(!self.icons_view),
                me("Ikony", "grid", "", Act::View(true)).on(self.icons_view),
                me("Sortuj według", "sort", "", Act::MenuSort),
                me("Odśwież", "refresh", "F5", Act::Reload),
                sep(),
                me("Wklej", "paste", "Ctrl+V", Act::Paste),
                me(&u, "undo", "Ctrl+Z", Act::Undo).enabled(!self.undo.is_empty() && self.job.is_none()),
                sep(),
                me("Nowy folder", "folderPlus", "Ctrl+Shift+N", Act::NewFolder),
                me("Nowy dokument tekstowy", "filePlus", "", Act::NewFile),
                sep(),
                me("Otwórz w terminalu", "terminal", "", Act::Terminal(None)),
                me("Kopiuj ścieżkę", "link", "", Act::CopyPath),
                me("Pokaż ukryte", "eye", "Ctrl+H", Act::ToggleHidden).on(t.show_hidden),
            ]
        };
        self.open_menu(wid, x, y, entries, cx);
    }
    fn undo_label(&self) -> String {
        match self.undo.last() {
            Some((l, _)) if self.job.is_none() => format!("Cofnij {l}"),
            _ => "Cofnij".into(),
        }
    }
    fn sort_menu(&mut self, wid: WinId, x: f32, y: f32, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let (k, d) = (w.cur().sort_key, w.cur().sort_desc);
        let e = vec![
            me("Nazwa", "", "", Act::Sort(SortKey::Name)).on(k == SortKey::Name),
            me("Data modyfikacji", "", "", Act::Sort(SortKey::Mtime)).on(k == SortKey::Mtime),
            me("Typ", "", "", Act::Sort(SortKey::Type)).on(k == SortKey::Type),
            me("Rozmiar", "", "", Act::Sort(SortKey::Size)).on(k == SortKey::Size),
            sep(),
            me("Rosnąco", "asc", "", Act::SortDesc(false)).on(!d),
            me("Malejąco", "desc", "", Act::SortDesc(true)).on(d),
        ];
        self.open_menu(wid, x, y, e, cx);
    }
    fn view_menu(&mut self, wid: WinId, x: f32, y: f32, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let h = w.cur().show_hidden;
        let e = vec![
            me("Szczegóły", "list", "Ctrl+Shift+6", Act::View(false)).on(!self.icons_view),
            me("Ikony", "grid", "Ctrl+Shift+2", Act::View(true)).on(self.icons_view),
            sep(),
            me("Pokaż ukryte elementy", "eye", "Ctrl+H", Act::ToggleHidden).on(h),
            me("Okienko nawigacji", "sidebar", "", Act::ToggleSidebar).on(self.sidebar),
        ];
        self.open_menu(wid, x, y, e, cx);
    }
    fn more_menu(&mut self, wid: WinId, x: f32, y: f32, cx: &mut Cx) {
        let u = self.undo_label();
        let e = vec![
            me(&u, "undo", "Ctrl+Z", Act::Undo).enabled(!self.undo.is_empty() && self.job.is_none()),
            me("Okienko podglądu", "preview", "Alt+P", Act::TogglePreview).on(self.preview),
            sep(),
            me("Zaznacz wszystko", "check", "Ctrl+A", Act::SelectAll),
            me("Odwróć zaznaczenie", "refresh", "", Act::InvertSel),
            me("Wyczyść zaznaczenie", "x", "", Act::ClearSel),
            sep(),
            me("Otwórz w terminalu", "terminal", "", Act::Terminal(None)),
            me("Kopiuj ścieżkę", "link", "Ctrl+Shift+C", Act::CopyPath),
            me("Nowe okno", "app", "Ctrl+N", Act::OpenWindow(String::new())),
        ];
        self.open_menu(wid, x, y, e, cx);
    }

    pub fn act(&mut self, wid: WinId, a: Act, at: (f32, f32), cx: &mut Cx) {
        cx.redraw(wid);
        match a {
            Act::OpenSel => self.open_sel(wid, cx),
            Act::OpenTab(p) => self.new_tab(wid, &p, false, cx),
            Act::OpenWindow(p) => {
                let p = if p.is_empty() { self.wins.iter().find(|w| w.id == wid).map(|w| w.cur().path.clone()).unwrap_or_default() } else { p };
                self.open_window(&p, "", None, "", cx);
            }
            Act::Terminal(p) => {
                let p = p.unwrap_or_else(|| self.wins.iter().find(|w| w.id == wid).map(|w| w.cur().path.clone()).unwrap_or_default());
                appwin::spawn(&["kitty", "--directory", &p]);
            }
            Act::Cut => self.copy_sel(wid, true, cx),
            Act::Copy => self.copy_sel(wid, false, cx),
            Act::Paste => self.paste(wid, None, cx),
            Act::PasteInto(p) => self.paste(wid, Some(p), cx),
            Act::Rename => self.start_rename(wid),
            Act::CopyPath => self.copy_path(wid, cx),
            Act::Trash => self.trash_sel(wid, cx),
            Act::Delete => self.ask_delete(wid),
            Act::DeleteNow => self.delete_now(wid, cx),
            Act::Restore => self.restore_sel(wid),
            Act::EmptyTrash => {
                if let Some(w) = self.win_mut(wid) {
                    w.ask = Some(Ask { title: "Opróżnić kosz?".into(), text: "Wszystkie elementy w koszu zostaną usunięte na zawsze.".into(), ok: "Opróżnij".into(), act: Act::EmptyTrashNow });
                }
            }
            Act::EmptyTrashNow => self.empty_trash(wid),
            Act::View(icons) => {
                self.icons_view = icons;
                self.save_cfg();
                for w in &self.wins {
                    cx.redraw(w.id);
                }
            }
            Act::Reload => {
                if let Some(w) = self.wins.iter().find(|w| w.id == wid) {
                    let a = w.active;
                    self.reload(wid, a);
                }
                self.drives = fs::drives();
            }
            Act::Undo => self.undo_last(wid, cx),
            Act::NewFolder => self.new_item(wid, true, cx),
            Act::NewFile => self.new_item(wid, false, cx),
            Act::ToggleHidden => {
                let mime = &self.mime;
                if let Some(w) = self.wins.iter_mut().find(|w| w.id == wid) {
                    let pick = w.pick.as_ref();
                    let t = &mut w.tabs[w.active];
                    t.show_hidden = !t.show_hidden;
                    t.resort(mime, pick);
                }
            }
            Act::Sort(k) => {
                if let Some(w) = self.win_mut(wid) {
                    w.cur_mut().sort_key = k;
                }
                self.resort_all(wid);
            }
            Act::SortDesc(d) => {
                if let Some(w) = self.win_mut(wid) {
                    w.cur_mut().sort_desc = d;
                }
                self.resort_all(wid);
            }
            Act::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                self.save_cfg();
                for w in &self.wins {
                    cx.redraw(w.id);
                }
            }
            Act::TogglePreview => {
                self.preview = !self.preview;
                self.save_cfg();
                for w in &self.wins {
                    cx.redraw(w.id);
                }
            }
            Act::SelectAll => {
                if let Some(w) = self.win_mut(wid) {
                    let t = w.cur_mut();
                    t.sel = t.items.iter().map(|&k| t.all[k].path.clone()).collect();
                }
            }
            Act::InvertSel => {
                if let Some(w) = self.win_mut(wid) {
                    let t = w.cur_mut();
                    t.sel = t.items.iter().map(|&k| t.all[k].path.clone()).filter(|p| !t.sel.contains(p)).collect();
                }
            }
            Act::ClearSel => {
                if let Some(w) = self.win_mut(wid) {
                    w.cur_mut().clear_sel();
                }
            }
            Act::NewTab => {
                let p = self.wins.iter().find(|w| w.id == wid).map(|w| w.cur().path.clone()).unwrap_or(fs::home());
                self.new_tab(wid, &p, true, cx);
            }
            Act::Overwrite => self.pick_finish(wid, true, cx),
            Act::MenuSort => self.sort_menu(wid, at.0, at.1, cx),
            Act::MenuView => self.view_menu(wid, at.0, at.1, cx),
            Act::MenuMore => self.more_menu(wid, at.0, at.1, cx),
            Act::MenuNew => {
                let e = vec![me("Folder", "folderPlus", "Ctrl+Shift+N", Act::NewFolder), me("Dokument tekstowy", "filePlus", "", Act::NewFile)];
                self.open_menu(wid, at.0, at.1, e, cx);
            }
            Act::MenuFilter => {
                let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
                let Some(p) = &w.pick else { return };
                let e: Vec<MenuEntry> = p.filters.iter().enumerate().map(|(i, (n, _))| me(n, "", "", Act::SetFilter(i)).on(i == p.cur_filter)).collect();
                let h: f32 = e.len() as f32 * 30.0 + 12.0;
                self.open_menu(wid, at.0, at.1 - h - 8.0, e, cx);
            }
            Act::SetFilter(i) => {
                if let Some(w) = self.win_mut(wid) {
                    if let Some(p) = w.pick.as_mut() {
                        p.cur_filter = i;
                    }
                }
                self.resort_all(wid);
            }
        }
    }

    // ------------------------------------------------------------ the FileChooser dialog
    fn pick_accept(&mut self, wid: WinId, cx: &mut Cx) {
        let Some(w) = self.wins.iter().find(|w| w.id == wid) else { return };
        let Some(p) = &w.pick else { return };
        let t = w.cur();
        let sel: Vec<Entry> = t.selected().into_iter().cloned().collect();
        if p.save {
            // a folder selected: go into it
            if sel.len() == 1 && sel[0].dir && w.focus != Focus::Name {
                let path = sel[0].path.clone();
                self.go(wid, &path, 0, cx);
                return;
            }
            let name = w.name.text.trim().to_string();
            if name.is_empty() || t.is_trash() {
                return;
            }
            let target = fs::join(&t.path, &name);
            if std::fs::metadata(&target).map(|m| m.is_dir()).unwrap_or(false) {
                self.go(wid, &target, 0, cx);
                if let Some(w) = self.win_mut(wid) {
                    w.name.set("");
                }
                return;
            }
            if std::fs::symlink_metadata(&target).is_ok() {
                let w = self.win_mut(wid).unwrap();
                w.ask = Some(Ask { title: format!("Zastąpić „{name}”?"), text: "Plik o tej nazwie już istnieje w tym folderze. Zastąpienie nadpisze jego zawartość.".into(), ok: "Zastąp".into(), act: Act::Overwrite });
                cx.redraw(wid);
                return;
            }
            self.pick_finish(wid, true, cx);
            return;
        }
        if p.directory {
            self.pick_finish(wid, true, cx);
            return;
        }
        if sel.len() == 1 && sel[0].dir {
            let path = sel[0].path.clone();
            self.go(wid, &path, 0, cx);
            return;
        }
        if sel.iter().any(|e| !e.dir) {
            self.pick_finish(wid, true, cx);
        }
    }
    fn pick_finish(&mut self, wid: WinId, ok: bool, cx: &mut Cx) {
        let Some(i) = self.wi(wid) else { return };
        let w = &mut self.wins[i];
        let Some(mut p) = w.pick.take() else { return };
        let t = &w.tabs[w.active];
        let uris: Vec<String> = if p.save {
            vec![appwin::path_to_uri(&fs::join(&t.path, w.name.text.trim()))]
        } else if p.directory {
            let s = t.selected();
            let dirs: Vec<String> = s.iter().filter(|e| e.dir).map(|e| e.path.clone()).collect();
            if dirs.is_empty() { vec![appwin::path_to_uri(&t.path)] } else { dirs.iter().take(if p.multiple { usize::MAX } else { 1 }).map(|d| appwin::path_to_uri(d)).collect() }
        } else {
            t.selected().iter().filter(|e| !e.dir).take(if p.multiple { usize::MAX } else { 1 }).map(|e| appwin::path_to_uri(&e.path)).collect()
        };
        if let Some(r) = p.reply.take() {
            let _ = r.send(if ok { Some((uris, (!p.filters.is_empty()).then_some(p.cur_filter))) } else { None });
        }
        // remember the folder for the next dialog
        let _ = std::fs::write(last_dir_path(), &t.path);
        self.close_window(wid, cx);
    }
}

thread_local! {
    static THUMB_TX: std::cell::RefCell<Option<std::sync::Arc<std::sync::Mutex<Option<calloop::channel::Sender<Box<dyn Any + Send>>>>>>> = const { std::cell::RefCell::new(None) };
    static UNDO_PENDING: std::cell::RefCell<Option<(String, Undo)>> = const { std::cell::RefCell::new(None) };
    // per listing thread: its own MIME database (globs parse once per thread, ~1 ms)
    static MIME: MimeDb = MimeDb::load();
}
static WATCH_HITS: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

fn cfg_path() -> String {
    format!("{}/.config/mist/files.json", fs::home())
}
fn last_dir_path() -> String {
    format!("{}/.local/state/mist/files-last-dir", fs::home())
}
pub fn open_file(p: &str) {
    appwin::spawn(&["gio", "open", p]);
}
fn set_clipboard_uris(uris: &str) {
    use std::io::Write;
    if let Ok(mut ch) = std::process::Command::new("wl-copy").args(["-t", "text/uri-list"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        if let Some(mut i) = ch.stdin.take() {
            let _ = i.write_all(uris.as_bytes());
        }
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
    }
}

// a thumbnail (premultiplied pixmap fitting size x size): pictures decoded here, others from the thumbnail cache
fn make_thumb(path: &str, mtime: i64, size: u32, mime: &str) -> Option<Pixmap> {
    let src = if mime.starts_with("image/") && !mime.contains("svg") {
        path.to_string()
    } else if let Some(t) = fs::cached_thumb(path, mtime) {
        t
    } else if mime.starts_with("video/") {
        fs::make_video_thumb(path)?
    } else if mime == "image/svg+xml" {
        let im = crate::apps::decode(path, size)?;
        return Pixmap::from_vec(im.rgba, tiny_skia::IntSize::from_wh(im.w, im.h)?);
    } else {
        return None;
    };
    let im = image::ImageReader::open(&src).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let im = im.thumbnail(size, size).to_rgba8();
    let (w, h) = im.dimensions();
    let mut data = im.into_raw();
    for p in data.chunks_exact_mut(4) {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = (p[c] as u32 * a / 255) as u8;
        }
    }
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)
}

impl Files {
    // icon pixmap for a freedesktop icon name list (first found), cached
    pub fn icon(&mut self, names: &[String], size: u32) -> Option<Rc<Pixmap>> {
        let key = (names.join("|"), size);
        if let Some(c) = self.icon_cache.get(&key) {
            return c.clone();
        }
        let mut out = None;
        for n in names {
            if let Some(im) = self.icons.image(n, size) {
                out = Pixmap::from_vec(im.rgba.clone(), tiny_skia::IntSize::from_wh(im.w, im.h).unwrap()).map(Rc::new);
                break;
            }
        }
        self.icon_cache.insert(key, out.clone());
        out
    }
    pub fn folder_icon_names(&self, path: &str) -> Vec<String> {
        let h = fs::home();
        let x = |k: &str| self.xdg.get(k).cloned().unwrap_or_default();
        let k = if path == h {
            "user-home"
        } else if path == x("DESKTOP") {
            "user-desktop"
        } else if path == x("DOCUMENTS") {
            "folder-documents"
        } else if path == x("DOWNLOAD") {
            "folder-download"
        } else if path == x("PICTURES") {
            "folder-pictures"
        } else if path == x("MUSIC") {
            "folder-music"
        } else if path == x("VIDEOS") {
            "folder-videos"
        } else if path == x("PUBLICSHARE") {
            "folder-publicshare"
        } else if path == x("TEMPLATES") {
            "folder-templates"
        } else if path.starts_with("trash:") {
            "user-trash"
        } else {
            "folder"
        };
        vec![k.into(), "folder".into()]
    }
    pub fn file_icon(&mut self, e: &Entry, size: u32) -> Option<Rc<Pixmap>> {
        let names = if e.dir {
            self.folder_icon_names(&e.path)
        } else {
            if !self.mime_icon.contains_key(&e.mime) {
                let n = self.mime.icon_names(&e.mime).join("|");
                self.mime_icon.insert(e.mime.clone(), n);
            }
            self.mime_icon[&e.mime].split('|').map(String::from).collect()
        };
        self.icon(&names, size)
    }
    pub fn has_thumb(&self, e: &Entry) -> bool {
        !e.dir && ((e.mime.starts_with("image/") && e.size < 40_000_000) || e.mime.starts_with("video/") || e.mime == "application/pdf")
    }
    // drop the least recently asked thumbnails over THUMB_BYTES (was: all 600 at once, up to ~150 MB at 256 px)
    fn trim_thumbs(&mut self) {
        while self.thumb_bytes > THUMB_BYTES || self.thumbs.len() > 1500 {
            let Some(k) = self.thumbs.iter().min_by_key(|(_, t)| t.1).map(|(k, _)| k.clone()) else { break };
            if let Some((Some(p), _)) = self.thumbs.remove(&k) {
                self.thumb_bytes -= p.data().len();
            }
        }
    }
    // the thumbnail when ready; asks the worker for it the first time
    pub fn thumb(&mut self, e: &Entry, size: u32) -> Option<Rc<Pixmap>> {
        let key = (e.path.clone(), size);
        self.thumb_tick += 1;
        let tick = self.thumb_tick;
        if let Some(t) = self.thumbs.get_mut(&key) {
            t.1 = tick;
            return t.0.clone();
        }
        self.thumbs.insert(key, (None, tick));
        self.trim_thumbs();
        let _ = self.thumb_tx.send((e.path.clone(), e.mtime, size, e.mime.clone()));
        None
    }
    fn folder_info(&mut self, path: &str) -> Option<(usize, u64)> {
        if let Some(v) = self.folder_info.get(path) {
            return Some(*v);
        }
        self.folder_info.insert(path.into(), (usize::MAX, 0));
        let tx = self.tx.clone();
        let p = path.to_string();
        std::thread::spawn(move || {
            let n = std::fs::read_dir(&p).map(|r| r.count()).unwrap_or(0);
            let size = fs::du(&p, Some(Instant::now() + Duration::from_secs(2)));
            if let Some(tx) = tx {
                let _ = tx.send(Box::new(Bg::FolderInfo { path: p, n, size }));
            }
        });
        None
    }
}

// ---------------------------------------------------------------- input
impl Files {
    fn hit_at(&self, wid: WinId, x: f32, y: f32) -> Option<(Hit, &'static str)> {
        let w = self.wins.iter().find(|w| w.id == wid)?;
        w.hits.iter().rev().find(|(hx, hy, hw, hh, _, _)| x >= *hx && x < hx + hw && y >= *hy && y < hy + hh).map(|h| (h.4.clone(), h.5))
    }
    // item index under (x, y) in the view, by geometry
    fn index_at(&self, wid: WinId, x: f32, y: f32) -> Option<usize> {
        let w = self.wins.iter().find(|w| w.id == wid)?;
        let (px, py, pw, ph) = view::panel_rect(w, self.sidebar, self.preview);
        if x < px || x >= px + pw || y < py || y >= py + ph {
            return None;
        }
        let t = w.cur();
        if self.icons_view {
            let cols = ((pw - 20.0) / view::CELL_W).floor().max(1.0) as usize;
            let (lx, ly) = (x - px - 10.0, y - py - 10.0 + t.scroll);
            if lx < 0.0 || ly < 0.0 {
                return None;
            }
            let (c, r) = ((lx / view::CELL_W) as usize, (ly / view::CELL_H) as usize);
            let (ix, iy) = (lx - c as f32 * view::CELL_W, ly - r as f32 * view::CELL_H);
            if c >= cols || ix < 6.0 || ix > view::CELL_W - 6.0 || iy < 4.0 || iy > view::CELL_H - 4.0 {
                return None;
            }
            let i = r * cols + c;
            (i < t.items.len()).then_some(i)
        } else {
            if y < py + view::HEAD_H {
                return None;
            }
            let i = ((y - py - view::HEAD_H + t.scroll) / view::ROW_H).floor();
            (i >= 0.0 && (i as usize) < t.items.len()).then_some(i as usize)
        }
    }
    fn band_update(&mut self, wid: WinId, x: f32, y: f32) {
        let icons = self.icons_view;
        let (sidebar, preview) = (self.sidebar, self.preview);
        let Some(w) = self.win_mut(wid) else { return };
        let (px, py, pw, _) = view::panel_rect(w, sidebar, preview);
        let scroll = w.cur().scroll;
        let Some(Press::Band { ax, ay, base, x: lx, y: ly }) = w.press.as_mut() else { return };
        *lx = x;
        *ly = y;
        let (bx, by) = (x, y + scroll);
        let (rx, ry, rw, rh) = (ax.min(bx), ay.min(by), (*ax - bx).abs(), (*ay - by).abs());
        w.band = Some((rx, ry - scroll, rw, rh));
        let mut s = base.clone();
        let t = &w.tabs[w.active];
        if icons {
            let cols = ((pw - 20.0) / view::CELL_W).floor().max(1.0) as usize;
            for (i, &k) in t.items.iter().enumerate() {
                let ix = px + 10.0 + (i % cols) as f32 * view::CELL_W + 10.0;
                let iy = py + 10.0 + (i / cols) as f32 * view::CELL_H + 6.0;
                if ix < rx + rw && ix + view::CELL_W - 20.0 > rx && iy < ry + rh && iy + view::CELL_H - 12.0 > ry {
                    s.insert(t.all[k].path.clone());
                }
            }
        } else {
            let top = py + view::HEAD_H;
            if rx < px + pw * 0.55 {
                let i0 = ((ry - top) / view::ROW_H).floor().max(0.0) as usize;
                let i1 = ((ry + rh - top) / view::ROW_H).floor();
                if i1 >= 0.0 {
                    for i in i0..=(i1 as usize).min(t.items.len().saturating_sub(1)) {
                        if let Some(&k) = t.items.get(i) {
                            s.insert(t.all[k].path.clone());
                        }
                    }
                }
            }
        }
        w.tabs[w.active].sel = s;
    }
    fn content_h(&self, w: &Win) -> f32 {
        let (_, _, pw, _) = view::panel_rect(w, self.sidebar, self.preview);
        let n = w.cur().items.len();
        if self.icons_view {
            let cols = ((pw - 20.0) / view::CELL_W).floor().max(1.0) as usize;
            20.0 + n.div_ceil(cols) as f32 * view::CELL_H
        } else {
            view::HEAD_H + n as f32 * view::ROW_H + 10.0
        }
    }
    fn scroll_by(&mut self, wid: WinId, dy: f32) {
        let Some(i) = self.wi(wid) else { return };
        let ch = self.content_h(&self.wins[i]);
        let (_, _, _, ph) = view::panel_rect(&self.wins[i], self.sidebar, self.preview);
        let t = self.wins[i].cur_mut();
        t.scroll = (t.scroll + dy).clamp(0.0, (ch - ph).max(0.0));
    }

    fn key(&mut self, wid: WinId, k: Keysym, utf8: Option<String>, cx: &mut Cx) {
        let m = cx.mods;
        let (c, s, a) = (m.ctrl, m.shift, m.alt);
        let Some(i) = self.wi(wid) else { return };
        cx.redraw(wid);
        // overlays first
        if self.wins[i].ask.is_some() {
            match k {
                Keysym::Escape => self.wins[i].ask = None,
                Keysym::Return | Keysym::KP_Enter => {
                    let act = self.wins[i].ask.take().unwrap().act;
                    self.act(wid, act, (0.0, 0.0), cx);
                }
                _ => {}
            }
            return;
        }
        if let Some(menu) = self.wins[i].menu.as_mut() {
            let n = menu.entries.len() as i32;
            let ok = |m: &Menu, j: i32| m.entries.get(j as usize).map(|e| !e.sep && !e.head && e.enabled).unwrap_or(false);
            match k {
                Keysym::Escape => self.wins[i].menu = None,
                Keysym::Down => {
                    let mut j = menu.hi;
                    for _ in 0..n {
                        j = (j + 1).rem_euclid(n);
                        if ok(menu, j) {
                            break;
                        }
                    }
                    menu.hi = j;
                }
                Keysym::Up => {
                    let mut j = if menu.hi < 0 { n } else { menu.hi };
                    for _ in 0..n {
                        j = (j - 1).rem_euclid(n);
                        if ok(menu, j) {
                            break;
                        }
                    }
                    menu.hi = j;
                }
                Keysym::Return | Keysym::KP_Enter => {
                    let hi = menu.hi;
                    self.fire_menu(wid, hi, cx);
                }
                _ => {}
            }
            return;
        }
        // text fields
        let focus = self.wins[i].focus;
        if matches!(focus, Focus::Addr | Focus::Search | Focus::Rename | Focus::Name) {
            let w = &mut self.wins[i];
            let le = match focus {
                Focus::Addr => w.addr.as_mut(),
                Focus::Search => Some(&mut w.search),
                Focus::Rename => w.rename.as_mut().map(|r| &mut r.1),
                _ => Some(&mut w.name),
            };
            let Some(le) = le else {
                w.focus = Focus::View;
                return;
            };
            // the search field hands Down to the list
            if focus == Focus::Search && k == Keysym::Down {
                w.focus = Focus::View;
                if !w.cur().items.is_empty() {
                    w.cur_mut().select_index(0, false, false);
                }
                return;
            }
            let r = le.key(k, utf8.as_deref(), c, s);
            match (focus, r) {
                (Focus::Search, EditResult::Used) => {
                    let f = w.search.text.clone();
                    w.cur_mut().filter = f;
                    let mime = &self.mime;
                    let pick = w.pick.as_ref();
                    w.tabs[w.active].resort(mime, pick);
                    w.tabs[w.active].scroll = 0.0;
                }
                (Focus::Search, EditResult::Cancel) => {
                    w.search.set("");
                    w.cur_mut().filter.clear();
                    w.focus = Focus::View;
                    let mime = &self.mime;
                    let pick = w.pick.as_ref();
                    w.tabs[w.active].resort(mime, pick);
                }
                (Focus::Search, EditResult::Accept) => {
                    w.focus = Focus::View;
                    if !w.cur().items.is_empty() && w.cur().cur < 0 {
                        w.cur_mut().select_index(0, false, false);
                    }
                }
                (Focus::Addr, EditResult::Accept) => {
                    let mut p = w.addr.as_ref().unwrap().text.trim().to_string();
                    if p == "~" || p.starts_with("~/") {
                        p = format!("{}{}", fs::home(), &p[1..]);
                    }
                    if p.starts_with("file://") {
                        p = appwin::uri_to_path(&p);
                    }
                    w.addr = None;
                    w.focus = Focus::View;
                    if !p.is_empty() {
                        if std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false) {
                            open_file(&p);
                        } else {
                            self.go(wid, &p, 0, cx);
                        }
                    }
                }
                (Focus::Addr, EditResult::Cancel) => {
                    w.addr = None;
                    w.focus = Focus::View;
                }
                (Focus::Rename, EditResult::Accept) => self.finish_rename(wid, cx),
                (Focus::Rename, EditResult::Cancel) => {
                    w.rename = None;
                    w.focus = Focus::View;
                }
                (Focus::Name, EditResult::Accept) => self.pick_accept(wid, cx),
                (Focus::Name, EditResult::Cancel) => self.pick_finish(wid, false, cx),
                (_, EditResult::Ignored) if focus == Focus::Name || focus == Focus::Search => {
                    // keys the field does not use go to the view (Tab, F-keys)
                    if k == Keysym::Tab {
                        w.focus = Focus::View;
                    }
                }
                _ => {}
            }
            return;
        }
        // the view
        let icons = self.icons_view;
        let cols = {
            let w = &self.wins[i];
            let (_, _, pw, _) = view::panel_rect(w, self.sidebar, self.preview);
            if icons { ((pw - 20.0) / view::CELL_W).floor().max(1.0) as i32 } else { 1 }
        };
        let pick = self.wins[i].pick.is_some();
        let mv = |f: &mut Files, d: i32| {
            let w = &mut f.wins[i];
            let t = w.cur_mut();
            let n = t.items.len() as i32;
            if n == 0 {
                return;
            }
            let j = if t.cur < 0 { if d > 0 { 0 } else { n - 1 } } else { (t.cur + d).clamp(0, n - 1) };
            if t.cur < 0 {
                t.select_index(j, false, false);
            } else {
                t.select_index(j, c && s, s);
            }
            f.show_index(wid, j as usize);
        };
        match k {
            Keysym::Left if a => self.go_back(wid, cx),
            Keysym::Right if a => self.go_fwd(wid, cx),
            Keysym::Up if a => self.go_up(wid, cx),
            Keysym::d | Keysym::D if a => self.edit_addr(wid),
            Keysym::l | Keysym::L if c => self.edit_addr(wid),
            Keysym::BackSpace => self.go_back(wid, cx),
            Keysym::F5 => self.act(wid, Act::Reload, (0.0, 0.0), cx),
            Keysym::r | Keysym::R if c => self.act(wid, Act::Reload, (0.0, 0.0), cx),
            Keysym::F2 => self.start_rename(wid),
            Keysym::F3 => self.wins[i].focus = Focus::Search,
            Keysym::f | Keysym::F | Keysym::e | Keysym::E if c => self.wins[i].focus = Focus::Search,
            Keysym::Delete | Keysym::KP_Delete if !pick => {
                if s {
                    self.ask_delete(wid)
                } else {
                    self.trash_sel(wid, cx)
                }
            }
            Keysym::n | Keysym::N if c && s => self.new_item(wid, true, cx),
            Keysym::c | Keysym::C if c && s => self.copy_path(wid, cx),
            Keysym::_2 | Keysym::at if c && s => self.act(wid, Act::View(true), (0.0, 0.0), cx),
            Keysym::_6 | Keysym::asciicircum if c && s => self.act(wid, Act::View(false), (0.0, 0.0), cx),
            Keysym::z | Keysym::Z if c && !pick => self.undo_last(wid, cx),
            Keysym::p | Keysym::P if a => self.act(wid, Act::TogglePreview, (0.0, 0.0), cx),
            Keysym::c | Keysym::C if c && !pick => self.copy_sel(wid, false, cx),
            Keysym::x | Keysym::X if c && !pick => self.copy_sel(wid, true, cx),
            Keysym::v | Keysym::V if c && !pick => self.paste(wid, None, cx),
            Keysym::a | Keysym::A if c => self.act(wid, Act::SelectAll, (0.0, 0.0), cx),
            Keysym::h | Keysym::H if c => self.act(wid, Act::ToggleHidden, (0.0, 0.0), cx),
            Keysym::t | Keysym::T if c && !pick => self.act(wid, Act::NewTab, (0.0, 0.0), cx),
            Keysym::w | Keysym::W if c => {
                if pick {
                    self.pick_finish(wid, false, cx)
                } else {
                    let a = self.wins[i].active;
                    self.close_tab(wid, a, cx)
                }
            }
            Keysym::n | Keysym::N if c && !pick => self.act(wid, Act::OpenWindow(String::new()), (0.0, 0.0), cx),
            Keysym::Tab | Keysym::ISO_Left_Tab if c => {
                let n = self.wins[i].tabs.len();
                let a = self.wins[i].active;
                let j = if s || k == Keysym::ISO_Left_Tab { (a + n - 1) % n } else { (a + 1) % n };
                self.set_active(wid, j, cx);
            }
            Keysym::Tab if pick && self.wins[i].pick.as_ref().map(|p| p.save).unwrap_or(false) => self.wins[i].focus = Focus::Name,
            Keysym::Return | Keysym::KP_Enter => self.open_sel(wid, cx),
            Keysym::Escape => {
                let w = &mut self.wins[i];
                if !w.cur().filter.is_empty() {
                    w.search.set("");
                    w.cur_mut().filter.clear();
                    let mime = &self.mime;
                    let pick = w.pick.as_ref();
                    w.tabs[w.active].resort(mime, pick);
                } else if pick {
                    self.pick_finish(wid, false, cx);
                } else {
                    w.cur_mut().clear_sel();
                }
            }
            Keysym::Down => mv(self, cols),
            Keysym::Up => mv(self, -cols),
            Keysym::Right if icons => mv(self, 1),
            Keysym::Left if icons => mv(self, -1),
            Keysym::Page_Down => mv(self, cols * 12),
            Keysym::Page_Up => mv(self, -cols * 12),
            Keysym::Home => {
                self.wins[i].cur_mut().select_index(0, false, s);
                self.show_index(wid, 0);
            }
            Keysym::End => {
                let n = self.wins[i].cur().items.len();
                if n > 0 {
                    self.wins[i].cur_mut().select_index(n as i32 - 1, false, s);
                    self.show_index(wid, n - 1);
                }
            }
            _ => {
                // type-to-select (Explorer)
                let Some(ch) = utf8.filter(|t| !c && !a && t.chars().count() == 1 && !t.trim().is_empty() && !t.chars().any(|x| x.is_control())) else { return };
                let w = &mut self.wins[i];
                if w.typed_at.elapsed() > Duration::from_millis(900) {
                    w.typed.clear();
                }
                w.typed.push_str(&ch.to_lowercase());
                w.typed_at = Instant::now();
                let typed = w.typed.clone();
                let t = w.cur_mut();
                let n = t.items.len() as i32;
                let start = if typed.chars().count() == 1 { t.cur + 1 } else { t.cur.max(0) };
                for j in 0..n {
                    let k = (start + j).rem_euclid(n.max(1));
                    if t.item(k as usize).map(|e| e.name.to_lowercase().starts_with(&typed)).unwrap_or(false) {
                        t.select_index(k, false, false);
                        self.show_index(wid, k as usize);
                        break;
                    }
                }
            }
        }
    }
    fn edit_addr(&mut self, wid: WinId) {
        if let Some(w) = self.win_mut(wid) {
            let p = w.cur().path.clone();
            let mut le = LineEdit::new(&p);
            le.select_all();
            w.addr = Some(le);
            w.focus = Focus::Addr;
        }
    }
    fn fire_menu(&mut self, wid: WinId, i: i32, cx: &mut Cx) {
        let Some(w) = self.win_mut(wid) else { return };
        let Some(m) = w.menu.take() else { return };
        let Some(e) = m.entries.get(i.max(0) as usize) else { return };
        if !e.enabled || e.sep || e.head {
            w.menu = Some(m);
            return;
        }
        let at = (m.x, m.y);
        if let Some(a) = e.act.clone() {
            self.act(wid, a, at, cx);
        }
    }

    fn pointer(&mut self, wid: WinId, ev: Ev, cx: &mut Cx) {
        let Some(i) = self.wi(wid) else { return };
        match ev {
            Ev::Motion(x, y) => {
                self.wins[i].mouse = (x, y);
                // dragging
                match self.wins[i].press.as_ref() {
                    Some(Press::Band { .. }) => {
                        self.band_update(wid, x, y);
                        let (_, py, _, ph) = view::panel_rect(&self.wins[i], self.sidebar, self.preview);
                        if y < py + 30.0 || y > py + ph - 30.0 {
                            cx.wake_at(Instant::now() + Duration::from_millis(16));
                        }
                        cx.redraw(wid);
                        return;
                    }
                    Some(Press::Item { idx, x0, y0, .. }) => {
                        let (idx, x0, y0) = (*idx, *x0, *y0);
                        if (x - x0).abs() + (y - y0).abs() > 10.0 && !self.wins[i].cur().is_trash() {
                            self.wins[i].press = None;
                            let t = self.wins[i].cur();
                            let uris: Vec<String> = t.selected().iter().map(|e| appwin::path_to_uri(&e.path)).collect();
                            let e = t.item(idx).cloned();
                            let icon = e.and_then(|e| self.file_icon(&e, 48)).map(|pm| {
                                let mut c = crate::gfx::Canvas::new(48, 48);
                                c.pm.draw_pixmap(0, 0, pm.as_ref().as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::from_scale(48.0 / pm.width() as f32, 48.0 / pm.height() as f32), None);
                                let mut b = vec![0u8; 48 * 48 * 4];
                                c.to_bgra(&mut b);
                                (48, 48, b)
                            });
                            if !uris.is_empty() {
                                cx.cmds.push(Cmd::Drag(wid, uris, icon));
                            }
                        }
                        return;
                    }
                    Some(Press::Scroll { grab }) => {
                        let grab = *grab;
                        let w = &self.wins[i];
                        let (_, py, _, ph) = view::panel_rect(w, self.sidebar, self.preview);
                        let ch = self.content_h(w);
                        let k = ch / ph;
                        let t = self.wins[i].cur_mut();
                        t.scroll = ((y - py - grab) * k).clamp(0.0, (ch - ph).max(0.0));
                        cx.redraw(wid);
                        return;
                    }
                    Some(Press::Field) => {
                        let st = Style::ui(13.0, 450.0);
                        let fx = view::field_x(&self.wins[i]);
                        let w = &mut self.wins[i];
                        let le = match w.focus {
                            Focus::Addr => w.addr.as_mut(),
                            Focus::Search => Some(&mut w.search),
                            Focus::Rename => w.rename.as_mut().map(|r| &mut r.1),
                            _ => Some(&mut w.name),
                        };
                        if let Some(le) = le {
                            le.click(&mut self.text, &st, x - fx, true);
                            cx.redraw(wid);
                        }
                        return;
                    }
                    None => {}
                }
                let h = self.hit_at(wid, x, y);
                let w = &mut self.wins[i];
                // menu highlight follows the pointer
                if let (Some(m), Some((Hit::MenuItem(k), _))) = (w.menu.as_mut(), &h) {
                    if m.hi != *k as i32 {
                        m.hi = *k as i32;
                        cx.redraw(wid);
                    }
                }
                let newh = h.as_ref().map(|x| x.0.clone());
                // rows hover by index
                let row = if newh == Some(Hit::View) { self.index_at(wid, x, y) } else { None };
                let w = &mut self.wins[i];
                let key_changed = newh != w.hover || (newh == Some(Hit::View));
                if key_changed {
                    if newh != w.hover {
                        w.tip = None;
                        w.hover_at = Instant::now();
                        if let Some((_, tip)) = &h {
                            if !tip.is_empty() {
                                cx.wake_at(Instant::now() + Duration::from_millis(650));
                            }
                        }
                    }
                    w.hover = newh.clone();
                    if VIEW_ROW.with(|r| r.replace(row)) != row || newh != Some(Hit::View) {
                        cx.redraw(wid);
                    }
                }
                let cur = match &newh {
                    Some(Hit::Addr) | Some(Hit::Search) | Some(Hit::PickName) => CursorIcon::Text,
                    Some(Hit::Place(_)) | Some(Hit::Trash) | Some(Hit::Crumb(_)) => CursorIcon::Pointer,
                    _ => CursorIcon::Default,
                };
                cx.cursor(cur);
            }
            Ev::Leave => {
                let w = &mut self.wins[i];
                if w.hover.is_some() {
                    w.hover = None;
                    w.tip = None;
                    VIEW_ROW.with(|r| r.set(None));
                    cx.redraw(wid);
                }
            }
            Ev::Press(x, y, b, clicks) => self.press(wid, x, y, b, clicks, cx),
            Ev::Release(x, y, b) => {
                let w = &mut self.wins[i];
                let p = w.press.take();
                w.band = None;
                if let Some(Press::Item { idx, narrow: true, .. }) = p {
                    if b == LEFT && self.index_at(wid, x, y) == Some(idx) {
                        self.wins[i].cur_mut().select_index(idx as i32, false, false);
                    }
                }
                cx.redraw(wid);
            }
            Ev::Scroll(x, y, dy) => {
                let w = &mut self.wins[i];
                if w.menu.is_some() {
                    w.menu = None;
                    cx.redraw(wid);
                    return;
                }
                if x < view::SIDE_X + view::SIDE_W && self.sidebar {
                    w.side_scroll = (w.side_scroll + dy * 2.0).max(0.0);
                } else {
                    self.scroll_by(wid, dy * 3.0);
                }
                let _ = y;
                cx.redraw(wid);
            }
            _ => {}
        }
    }

    fn press(&mut self, wid: WinId, x: f32, y: f32, b: u32, clicks: u8, cx: &mut Cx) {
        let Some(i) = self.wi(wid) else { return };
        cx.redraw(wid);
        let h = self.hit_at(wid, x, y).map(|h| h.0);
        let w = &mut self.wins[i];
        w.tip = None;
        // a click outside the menu closes it
        if w.menu.is_some() {
            if let Some(Hit::MenuItem(k)) = h {
                if b == LEFT {
                    self.fire_menu(wid, k as i32, cx);
                }
            } else {
                w.menu = None;
            }
            return;
        }
        if w.ask.is_some() {
            match h {
                Some(Hit::AskOk) => {
                    let act = w.ask.take().unwrap().act;
                    self.act(wid, act, (x, y), cx);
                }
                Some(Hit::AskCancel) => w.ask = None,
                Some(Hit::Sheet) => {}
                _ => w.ask = None,
            }
            return;
        }
        // leaving a text field
        let field = matches!(h, Some(Hit::Addr) | Some(Hit::Search) | Some(Hit::PickName));
        if w.focus == Focus::Rename && !field {
            self.finish_rename(wid, cx);
        }
        let w = &mut self.wins[i];
        if w.focus == Focus::Addr && h != Some(Hit::Addr) {
            w.addr = None;
            w.focus = Focus::View;
        }
        if w.focus == Focus::Search && h != Some(Hit::Search) {
            w.focus = Focus::View;
        }
        if w.focus == Focus::Name && h != Some(Hit::PickName) {
            w.focus = Focus::View;
        }
        let Some(h) = h else { return };
        let st = Style::ui(13.0, 450.0);
        match h {
            Hit::Light(k) if b == LEFT => match k {
                0 => {
                    if self.wins[i].pick.is_some() {
                        self.pick_finish(wid, false, cx)
                    } else {
                        self.close_window(wid, cx)
                    }
                }
                1 => appwin::hypr("MistMinimize()"),
                _ => appwin::hypr("MistMaximize()"),
            },
            Hit::Drag | Hit::TabStrip | Hit::Side if b == LEFT => {
                if clicks == 2 && h == Hit::TabStrip {
                    appwin::hypr("MistMaximize()");
                } else {
                    cx.cmds.push(Cmd::Move(wid));
                }
            }
            Hit::Place(p) => {
                if b == MIDDLE {
                    self.new_tab(wid, &p, false, cx);
                } else if b == RIGHT {
                    let e = vec![me("Otwórz w nowej karcie", "plus", "", Act::OpenTab(p.clone())), me("Otwórz w nowym oknie", "app", "", Act::OpenWindow(p.clone())), me("Otwórz w terminalu", "terminal", "", Act::Terminal(Some(p.clone())))];
                    self.open_menu(wid, x, y, e, cx);
                } else {
                    self.go(wid, &p, 0, cx);
                    self.drives = fs::drives();
                }
            }
            Hit::Trash => {
                if b == RIGHT {
                    self.open_menu(wid, x, y, vec![me("Opróżnij kosz", "trash", "", Act::EmptyTrash).danger()], cx);
                } else {
                    self.go(wid, "trash:///", 0, cx);
                }
            }
            Hit::Tab(k) => {
                if b == MIDDLE {
                    self.close_tab(wid, k, cx);
                } else if b == LEFT {
                    self.set_active(wid, k, cx);
                }
            }
            Hit::TabClose(k) if b == LEFT => self.close_tab(wid, k, cx),
            Hit::NewTab if b == LEFT => self.act(wid, Act::NewTab, (x, y), cx),
            Hit::Back if b == LEFT => self.go_back(wid, cx),
            Hit::Fwd if b == LEFT => self.go_fwd(wid, cx),
            Hit::Up if b == LEFT => self.go_up(wid, cx),
            Hit::Reload if b == LEFT => self.act(wid, Act::Reload, (x, y), cx),
            Hit::Crumb(p) => {
                if b == MIDDLE {
                    self.new_tab(wid, &p, false, cx);
                } else if b == LEFT {
                    self.go(wid, &p, 0, cx);
                }
            }
            Hit::Addr if b == LEFT => {
                let fx = view::field_x(&self.wins[i]);
                let w = &mut self.wins[i];
                if w.addr.is_none() {
                    self.edit_addr(wid);
                } else {
                    if let Some(le) = w.addr.as_mut() {
                        le.click(&mut self.text, &st, x - fx, cx.mods.shift);
                    }
                    w.press = Some(Press::Field);
                }
            }
            Hit::Search if b == LEFT => {
                let w = &mut self.wins[i];
                w.focus = Focus::Search;
                let fx = view::field_x(w);
                if clicks == 2 {
                    w.search.select_word_at(&mut self.text, &st, x - fx);
                } else {
                    w.search.click(&mut self.text, &st, x - fx, cx.mods.shift);
                }
                w.press = Some(Press::Field);
            }
            Hit::PickName if b == LEFT => {
                let w = &mut self.wins[i];
                w.focus = Focus::Name;
                let fx = view::field_x(w);
                w.name.click(&mut self.text, &st, x - fx, cx.mods.shift);
                w.press = Some(Press::Field);
            }
            Hit::SearchClear if b == LEFT => {
                let w = &mut self.wins[i];
                w.search.set("");
                w.cur_mut().filter.clear();
                let mime = &self.mime;
                let pick = w.pick.as_ref();
                w.tabs[w.active].resort(mime, pick);
            }
            Hit::Btn(a) if b == LEFT => {
                // dropdowns open under their button
                let (bx, by) = self.wins[i].hits.iter().rev().find(|r| r.4 == Hit::Btn(a.clone())).map(|r| (r.0, r.1 + r.3 + 4.0)).unwrap_or((x, y));
                self.act(wid, a, (bx, by), cx);
            }
            Hit::Header(k) if b == LEFT => {
                let w = &mut self.wins[i];
                let t = w.cur_mut();
                if t.sort_key == k {
                    t.sort_desc = !t.sort_desc;
                } else {
                    t.sort_key = k;
                    t.sort_desc = !matches!(k, SortKey::Name | SortKey::Type);
                }
                self.resort_all(wid);
            }
            Hit::ScrollBar if b == LEFT => {
                let w = &self.wins[i];
                let (_, py, _, ph) = view::panel_rect(w, self.sidebar, self.preview);
                let ch = self.content_h(w);
                let thumb_y = py + w.cur().scroll / ch * ph;
                let grab = if y >= thumb_y && y <= thumb_y + ph * ph / ch { y - thumb_y } else { ph * ph / ch / 2.0 };
                self.wins[i].press = Some(Press::Scroll { grab });
                let ev = Ev::Motion(x, y);
                self.pointer(wid, ev, cx);
            }
            Hit::OpsCancel if b == LEFT => {
                if let Some(j) = &self.job {
                    j.pr.cancel.store(true, Ordering::Relaxed);
                }
            }
            Hit::PickOk if b == LEFT => self.pick_accept(wid, cx),
            Hit::PickCancel if b == LEFT => self.pick_finish(wid, false, cx),
            Hit::View => self.press_view(wid, x, y, b, clicks, cx),
            _ => {}
        }
    }

    fn press_view(&mut self, wid: WinId, x: f32, y: f32, b: u32, clicks: u8, cx: &mut Cx) {
        let Some(i) = self.wi(wid) else { return };
        let idx = self.index_at(wid, x, y);
        let m = cx.mods;
        let w = &mut self.wins[i];
        w.focus = Focus::View;
        match idx {
            Some(k) => {
                let e = w.cur().item(k).cloned().unwrap();
                if b == MIDDLE {
                    if e.dir {
                        self.new_tab(wid, &e.path, false, cx);
                    }
                    return;
                }
                if b == RIGHT {
                    if !w.cur().sel.contains(&e.path) {
                        w.cur_mut().select_index(k as i32, false, false);
                    }
                    self.item_menu(wid, x, y, cx);
                    return;
                }
                if clicks == 2 {
                    w.press = None;
                    if w.pick.is_some() {
                        if e.dir {
                            self.go(wid, &e.path, 0, cx);
                        } else {
                            w.cur_mut().select_index(k as i32, false, false);
                            self.pick_accept(wid, cx);
                        }
                    } else {
                        self.open_entry(wid, &e, false, cx);
                    }
                    return;
                }
                // Explorer: press on a selected item keeps the selection (drag), release narrows it
                let selected = w.cur().sel.contains(&e.path);
                if selected && !m.ctrl && !m.shift {
                    w.cur_mut().cur = k as i32;
                    w.press = Some(Press::Item { idx: k, x0: x, y0: y, narrow: true });
                } else {
                    w.cur_mut().select_index(k as i32, m.ctrl, m.shift);
                    w.press = Some(Press::Item { idx: k, x0: x, y0: y, narrow: false });
                }
                // the save dialog takes the clicked file's name
                if let Some(p) = &w.pick {
                    if p.save && !e.dir {
                        w.name.set(&e.name);
                    }
                }
            }
            None => {
                if b == RIGHT {
                    w.cur_mut().clear_sel();
                    self.bg_menu(wid, x, y, cx);
                    return;
                }
                if b != LEFT {
                    return;
                }
                let scroll = w.cur().scroll;
                let base = if m.ctrl { w.cur().sel.clone() } else { HashSet::new() };
                if !m.ctrl {
                    w.cur_mut().clear_sel();
                }
                w.press = Some(Press::Band { ax: x, ay: y + scroll, base, x, y });
            }
        }
    }
}

thread_local! {
    static VIEW_ROW: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

// ---------------------------------------------------------------- the app
impl Logic for Files {
    fn app_id(&self) -> &str {
        "mist-files"
    }
    fn opened(&mut self, id: WinId, token: u64, cx: &mut Cx) {
        self.bind_tx(cx);
        let Some(k) = self.pending.iter().position(|(t, _)| *t == token) else { return };
        let (_, p) = self.pending.remove(k);
        let pick_save = p.pick.as_ref().map(|x| x.save).unwrap_or(false);
        self.wins.push(new_win(id, p.pick, &p.name, pick_save));
        self.new_tab(id, &p.path, true, cx);
        if !p.select.is_empty() {
            if let Some(w) = self.win_mut(id) {
                w.cur_mut().pending_select = vec![p.select];
            }
        }
    }
    fn paint(&mut self, id: WinId, c: &mut crate::gfx::Canvas, _cx: &mut Cx) {
        self.paint_win(id, c);
    }
    fn event(&mut self, id: WinId, ev: Ev, cx: &mut Cx) {
        match ev {
            Ev::Key(k, u) => self.key(id, k, u, cx),
            Ev::Focus(f) => {
                if let Some(w) = self.win_mut(id) {
                    w.focused = f;
                }
                if f && self.drives_at.elapsed() > Duration::from_secs(10) {
                    self.drives = fs::drives();
                    self.drives_at = Instant::now();
                }
                cx.redraw(id);
            }
            Ev::Resized => {
                if let Some(i) = self.wi(id) {
                    self.scroll_by(id, 0.0);
                    let _ = i;
                }
            }
            Ev::DragMotion(x, y) => {
                let t = self.drop_target(id, x, y);
                if let Some(w) = self.win_mut(id) {
                    if w.drop != t {
                        w.drop = t;
                        cx.redraw(id);
                    }
                }
            }
            Ev::DragLeave => {
                if let Some(w) = self.win_mut(id) {
                    w.drop = None;
                    w.press = None;
                    cx.redraw(id);
                }
            }
            Ev::Drop(x, y, uris) => {
                let t = self.drop_target(id, x, y);
                if let Some(w) = self.win_mut(id) {
                    w.drop = None;
                    w.press = None;
                }
                if let Some(dest) = t {
                    self.drop_into(id, uris, &dest, cx);
                }
                cx.redraw(id);
            }
            e => self.pointer(id, e, cx),
        }
    }
    fn close_requested(&mut self, id: WinId, cx: &mut Cx) {
        if self.wins.iter().any(|w| w.id == id && w.pick.is_some()) {
            self.pick_finish(id, false, cx);
        } else {
            self.close_window(id, cx);
        }
    }
    fn tick(&mut self, cx: &mut Cx) {
        let now = Instant::now();
        // the process ends when nothing is open (the portal waits a little longer)
        if let Some(t) = self.idle_since {
            let keep = if portal::OWNED.load(Ordering::Relaxed) { Duration::from_secs(120) } else { Duration::ZERO };
            if self.wins.is_empty() && self.job.is_none() && self.pending.is_empty() {
                if now.duration_since(t) >= keep {
                    cx.cmds.push(Cmd::Exit);
                    return;
                }
                cx.wake_at(t + keep);
            } else {
                self.idle_since = None;
            }
        }
        let mut reload = vec![];
        for w in self.wins.iter_mut() {
            if !w.toast.is_empty() && now >= w.toast_until {
                w.toast.clear();
                cx.redraw(w.id);
            }
            // tooltip after resting on a button
            if w.tip.is_none() && w.press.is_none() && w.menu.is_none() && now.duration_since(w.hover_at) >= Duration::from_millis(640) {
                if let Some(h) = &w.hover {
                    if let Some(r) = w.hits.iter().rev().find(|r| &r.4 == h && !r.5.is_empty()) {
                        w.tip = Some((r.5.to_string(), r.0 + r.2 / 2.0, r.1 + r.3 + 6.0));
                        cx.redraw(w.id);
                    }
                }
            }
            for (k, t) in w.tabs.iter().enumerate() {
                if let Some(at) = t.reload_at {
                    if now >= at {
                        reload.push((w.id, k));
                    } else {
                        cx.wake_at(at);
                    }
                }
            }
        }
        for (w, k) in reload {
            self.reload(w, k);
        }
        // rubber band autoscroll near the edges
        let ids: Vec<WinId> = self.wins.iter().filter(|w| matches!(w.press, Some(Press::Band { .. }))).map(|w| w.id).collect();
        for id in ids {
            let i = self.wi(id).unwrap();
            let (_, py, _, ph) = view::panel_rect(&self.wins[i], self.sidebar, self.preview);
            let (x, y) = match &self.wins[i].press {
                Some(Press::Band { x, y, .. }) => (*x, *y),
                _ => continue,
            };
            let dy = if y < py + 30.0 { -14.0 } else if y > py + ph - 30.0 { 14.0 } else { 0.0 };
            if dy != 0.0 {
                self.scroll_by(id, dy);
                self.band_update(id, x, y);
                cx.redraw(id);
                cx.wake_at(now + Duration::from_millis(16));
            }
        }
        if self.job.is_some() {
            for w in &self.wins {
                cx.redraw(w.id);
            }
            cx.wake_at(now + Duration::from_millis(250));
        }
    }
    fn message(&mut self, args: Vec<String>, cx: &mut Cx) {
        self.bind_tx(cx);
        if args.first().map(|s| s.as_str()) == Some("--test") {
            self.test(&args[1..], cx);
            return;
        }
        let mut select = false;
        let mut opened = false;
        for a in &args {
            match a.as_str() {
                "--select" => select = true,
                "--portal" => {
                    self.portal_only = true;
                    portal::spawn(cx.tx.clone());
                    // nothing open yet: stay for the dialogs
                    self.idle_since = Some(Instant::now());
                    cx.wake_at(Instant::now() + Duration::from_secs(120));
                    opened = true;
                }
                "" => {}
                p => {
                    let mut p = if p.starts_with("file://") { appwin::uri_to_path(p) } else { p.to_string() };
                    if p.starts_with("trash:") {
                        p = "trash:///".into();
                    } else if !p.starts_with('/') {
                        p = format!("{}/{p}", std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default());
                    }
                    let is_file = std::fs::metadata(&p).map(|m| !m.is_dir()).unwrap_or(false);
                    if select || is_file {
                        self.open_window(&fs::parent(&p), &p, None, "", cx);
                    } else {
                        self.open_window(&p, "", None, "", cx);
                    }
                    opened = true;
                    select = false;
                }
            }
        }
        if !opened {
            self.open_window(&fs::home(), "", None, "", cx);
        }
        // a FileChooser name may be ours already in this process (portal); else try for it
        if !self.portal_only && !portal::OWNED.load(Ordering::Relaxed) {
            portal::spawn(cx.tx.clone());
        }
    }
    fn async_msg(&mut self, m: Box<dyn Any + Send>, cx: &mut Cx) {
        if let Some(r) = m.downcast_ref::<portal::Request>() {
            let _ = r;
        }
        let m = match m.downcast::<portal::Request>() {
            Ok(r) => {
                self.portal_request(*r, cx);
                return;
            }
            Err(m) => m,
        };
        let Ok(b) = m.downcast::<Bg>() else { return };
        match *b {
            Bg::Listed { uid, serial, res } => self.listed(uid, serial, res, cx),
            Bg::Total(t) => {
                if let Some(j) = self.job.as_mut() {
                    j.total = t;
                }
            }
            Bg::JobDone { made, cancelled, err } => {
                let Some(j) = self.job.take() else { return };
                if !made.is_empty() && !cancelled {
                    let label = j.title.to_lowercase();
                    if j.mv {
                        self.push_undo(label, Undo::Mv(made.iter().map(|(a, b)| (b.clone(), a.clone())).collect()));
                    } else {
                        self.push_undo(label, Undo::Trash(made.iter().map(|(_, b)| b.clone()).collect()));
                    }
                }
                if j.after_cut && !cancelled {
                    self.clip.clear();
                    self.clip_cut = false;
                }
                let msg = if cancelled { "Anulowano".to_string() } else if !err.is_empty() { err } else if j.mv { "Przeniesiono".into() } else { "Skopiowano".into() };
                self.say(j.win, &msg, cx);
                // select what was made in the folder it went to
                let made_paths: Vec<String> = made.iter().map(|(_, b)| b.clone()).collect();
                for w in self.wins.iter_mut() {
                    if w.cur().path == j.dest {
                        w.cur_mut().pending_select = made_paths.clone();
                    }
                }
                self.reload_path(&j.dest);
                if j.mv {
                    let srcs: HashSet<String> = made.iter().map(|(a, _)| fs::parent(a)).collect();
                    for s in srcs {
                        self.reload_path(&s);
                    }
                }
                self.next_job(cx);
                for w in &self.wins {
                    cx.redraw(w.id);
                }
                if self.wins.is_empty() {
                    self.idle_since = Some(Instant::now());
                    cx.wake_at(Instant::now());
                }
            }
            Bg::Thumb { key, pm } => {
                self.thumb_bytes += pm.as_ref().map(|p| p.data().len()).unwrap_or(0);
                let tick = self.thumbs.get(&key).map(|t| t.1).unwrap_or(self.thumb_tick);
                if let Some((Some(old), _)) = self.thumbs.insert(key, (pm.map(Rc::new), tick)) {
                    self.thumb_bytes -= old.data().len();
                }
                self.trim_thumbs();
                for w in &self.wins {
                    cx.redraw(w.id);
                }
            }
            Bg::FolderInfo { path, n, size } => {
                self.folder_info.insert(path, (n, size));
                for w in &self.wins {
                    cx.redraw(w.id);
                }
            }
            Bg::Done { win, msg, reload, select } => {
                if let Some((l, u)) = UNDO_PENDING.with(|u| u.borrow_mut().take()) {
                    // a delete: undo restores what went to the trash
                    if let Undo::Restore(_) = u {
                        let origs: Vec<String> = self.wins.iter().find(|w| w.id == win).map(|w| w.cur().selected().iter().map(|e| e.path.clone()).collect()).unwrap_or_default();
                        if msg.starts_with("Przeniesiono") {
                            self.push_undo(l, Undo::Restore(origs));
                        }
                    }
                }
                // no trash on this filesystem (tmpfs, some mounts): offer a permanent delete
                if msg.starts_with("Nie udało się przenieść do kosza") {
                    if let Some(w) = self.win_mut(win) {
                        if !w.cur().sel.is_empty() {
                            w.ask = Some(Ask { title: "Nie można przenieść do kosza".into(), text: "Ten dysk nie ma kosza. Usunąć zaznaczone elementy trwale? Tej operacji nie można cofnąć.".into(), ok: "Usuń".into(), act: Act::DeleteNow });
                        }
                    }
                }
                self.say(win, &msg, cx);
                if reload {
                    if let Some(w) = self.win_mut(win) {
                        if !select.is_empty() {
                            w.cur_mut().pending_select = select;
                        }
                        let a = w.active;
                        self.reload(win, a);
                    }
                }
                self.folder_info.clear();
            }
            Bg::Watch => {
                let wds: Vec<i32> = std::mem::take(&mut *WATCH_HITS.lock().unwrap());
                let Some(wt) = &self.watch else { return };
                let paths: HashSet<String> = wds.iter().filter_map(|wd| wt.wds.get(wd).cloned()).collect();
                let tf = format!("{}/files", fs::trash_dir());
                let at = Instant::now() + Duration::from_millis(250);
                for w in self.wins.iter_mut() {
                    for t in w.tabs.iter_mut() {
                        let p = if t.is_trash() { tf.clone() } else { t.path.clone() };
                        if paths.contains(&p) {
                            t.reload_at = Some(at);
                        }
                    }
                }
                self.folder_info.clear();
                cx.wake_at(at);
            }
            Bg::Pick(r) => self.portal_request(r, cx),
        }
    }
}

impl Files {
    // test hooks (`mist-ui-bin files --test ...`, last window): click x y [n] [right|middle] | key NAME [ctrl] [shift] [alt]
    // | type TEXT | move x y | dump (state to $XDG_RUNTIME_DIR/mist/files-state.json)
    fn test(&mut self, a: &[String], cx: &mut Cx) {
        let Some(wid) = self.wins.last().map(|w| w.id) else { return };
        let f = |i: usize| a.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        let has = |s: &str| a.iter().any(|x| x == s);
        match a.first().map(|s| s.as_str()) {
            Some("click") => {
                let (x, y) = (f(1), f(2));
                let n = a.get(3).and_then(|s| s.parse::<u8>().ok()).unwrap_or(1);
                let b = if has("right") { RIGHT } else if has("middle") { MIDDLE } else { LEFT };
                cx.mods = appwin::Mods { ctrl: has("ctrl"), shift: has("shift"), alt: false };
                self.event(wid, Ev::Motion(x, y), cx);
                for k in 1..=n {
                    self.event(wid, Ev::Press(x, y, b, k), cx);
                    self.event(wid, Ev::Release(x, y, b), cx);
                }
            }
            Some("move") => self.event(wid, Ev::Motion(f(1), f(2)), cx),
            Some("key") => {
                let name = a.get(1).cloned().unwrap_or_default();
                let k = match name.as_str() {
                    "Return" => Keysym::Return,
                    "Escape" => Keysym::Escape,
                    "F2" => Keysym::F2,
                    "F5" => Keysym::F5,
                    "Delete" => Keysym::Delete,
                    "Down" => Keysym::Down,
                    "Up" => Keysym::Up,
                    "Left" => Keysym::Left,
                    "Right" => Keysym::Right,
                    "BackSpace" => Keysym::BackSpace,
                    "Tab" => Keysym::Tab,
                    "Home" => Keysym::Home,
                    "End" => Keysym::End,
                    s if s.chars().count() == 1 => Keysym::from_char(s.chars().next().unwrap()),
                    _ => return,
                };
                cx.mods = appwin::Mods { ctrl: has("ctrl"), shift: has("shift"), alt: has("alt") };
                let utf8 = (name.chars().count() == 1 && !has("ctrl")).then(|| name.clone());
                self.event(wid, Ev::Key(k, utf8), cx);
            }
            Some("type") => {
                cx.mods = appwin::Mods::default();
                for ch in a.get(1).cloned().unwrap_or_default().chars() {
                    self.event(wid, Ev::Key(Keysym::from_char(ch), Some(ch.to_string())), cx);
                }
            }
            Some("dump") => {
                let w = self.wins.last().unwrap();
                let t = w.cur();
                let v = serde_json::json!({
                    "windows": self.wins.len(), "tabs": w.tabs.len(), "path": t.path, "items": t.items.len(),
                    "names": t.items.iter().take(40).map(|&k| t.all[k].name.clone()).collect::<Vec<_>>(),
                    "sel": t.selected().iter().map(|e| e.name.clone()).collect::<Vec<_>>(), "filter": t.filter,
                    "focus": format!("{:?}", w.focus), "menu": w.menu.as_ref().map(|m| m.entries.iter().map(|e| e.label.clone()).collect::<Vec<_>>()),
                    "ask": w.ask.as_ref().map(|a| a.title.clone()), "toast": w.toast, "rename": w.rename.as_ref().map(|r| r.1.text.clone()),
                    "addr": w.addr.as_ref().map(|r| r.text.clone()), "icons": self.icons_view, "preview": self.preview,
                    "clip": self.clip.len(), "undo": self.undo.len(), "job": self.job.as_ref().map(|j| j.title.clone()),
                    "pick": w.pick.as_ref().map(|p| p.title.clone()), "name": w.name.text,
                    "thumbs": self.thumbs.len(), "thumb_bytes": self.thumb_bytes,
                    "thumb_px": self.thumbs.values().filter_map(|t| t.0.as_ref().map(|p| p.data().len())).sum::<usize>(),
                });
                let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
                let _ = std::fs::write(format!("{dir}/files-state.json"), v.to_string());
            }
            _ => {}
        }
        cx.mods = appwin::Mods::default();
    }
    fn drop_target(&self, wid: WinId, x: f32, y: f32) -> Option<String> {
        let w = self.wins.iter().find(|w| w.id == wid)?;
        match self.hit_at(wid, x, y).map(|h| h.0) {
            Some(Hit::Place(p)) => Some(p),
            Some(Hit::Trash) => Some("trash:///".into()),
            Some(Hit::View) => {
                if w.cur().is_trash() {
                    return None;
                }
                match self.index_at(wid, x, y).and_then(|i| w.cur().item(i)) {
                    Some(e) if e.dir => Some(e.path.clone()),
                    _ => Some(w.cur().path.clone()),
                }
            }
            _ => None,
        }
    }
    fn portal_request(&mut self, r: portal::Request, cx: &mut Cx) {
        let start = r.folder.clone().filter(|f| std::fs::metadata(f).map(|m| m.is_dir()).unwrap_or(false)).or_else(|| std::fs::read_to_string(last_dir_path()).ok().filter(|f| std::fs::metadata(f).map(|m| m.is_dir()).unwrap_or(false))).unwrap_or(fs::home());
        let pick = Pick {
            save: r.save,
            multiple: r.multiple,
            directory: r.directory,
            title: if r.title.is_empty() { if r.save { "Zapisz".into() } else { "Otwórz".into() } } else { r.title.clone() },
            accept: r.accept.clone().unwrap_or(if r.save { "Zapisz".into() } else if r.directory { "Wybierz".into() } else { "Otwórz".into() }),
            filters: r.filters.clone(),
            cur_filter: r.cur_filter,
            reply: Some(r.reply.clone()),
        };
        self.open_window(&start, r.file.as_deref().unwrap_or(""), Some(pick), &r.name, cx);
        let _ = &self.pending_picks;
    }
}

fn new_win(id: WinId, pick: Option<Pick>, name: &str, pick_save: bool) -> Win {
    Win {
            id,
            tabs: vec![],
            active: 0,
            w: W0 as f32,
            h: H0 as f32,
            hits: vec![],
            hover: None,
            mouse: (0.0, 0.0),
            focus: if pick_save { Focus::Name } else { Focus::View },
            addr: None,
            search: LineEdit::default(),
            rename: None,
            menu: None,
            ask: None,
            toast: String::new(),
            toast_until: Instant::now(),
            typed: String::new(),
            typed_at: Instant::now(),
            press: None,
            band: None,
            drop: None,
            tip: None,
            hover_at: Instant::now(),
            side_scroll: 0.0,
            pick,
            name: {
                let mut le = LineEdit::new(&name);
                let stem = name.rfind('.').filter(|&i| i > 0).unwrap_or(name.len());
                le.select(0, stem);
                le
            },
            focused: true,
    }
}

// offscreen check: `files --png out.png [path] [WxH] [icons] [preview] [sel=N]`
fn render_png(args: &[String]) -> i32 {
    let out = &args[1];
    let path = args.get(2).cloned().unwrap_or(fs::home());
    let (w, h) = args.get(3).and_then(|s| s.split_once('x')).map(|(a, b)| (a.parse().unwrap_or(W0), b.parse().unwrap_or(H0))).unwrap_or((W0, H0));
    let mut f = Files::new();
    f.icons_view = args.iter().any(|a| a == "icons");
    f.preview = args.iter().any(|a| a == "preview");
    let mut win = new_win(1, None, "", false);
    let mut t = Tab::new(1, &path);
    t.all = fs::list(&path, &f.mime).unwrap_or_default();
    t.resort(&f.mime, None);
    if let Some(n) = args.iter().find_map(|a| a.strip_prefix("sel=")).and_then(|n| n.parse::<i32>().ok()) {
        t.select_index(n, false, false);
    }
    win.tabs.push(t);
    f.wins.push(win);
    let mut c = crate::gfx::Canvas::new(w, h);
    f.paint_win(1, &mut c);
    c.pm.save_png(out).map(|_| 0).unwrap_or(1)
}

pub fn run(_args: &[String]) -> i32 {
    if _args.first().map(|s| s.as_str()) == Some("--png") {
        return render_png(_args);
    }
    // a running instance takes the arguments (a new window there)
    let args: Vec<String> = std::env::args().skip(2).collect();
    if appwin::forward("files", &args) {
        return 0;
    }
    // test hooks only talk to a running instance
    if args.first().map(|s| s.as_str()) == Some("--test") {
        return 1;
    }
    appwin::run("files", Box::new(Files::new()))
}
