// FileChooser portal backend (org.freedesktop.impl.portal.FileChooser): xdg-desktop-portal hands "Open…" /
// "Save…" dialogs of other apps (GTK, Qt, Electron, Firefox) here instead of the GTK file chooser; each one is a
// Mist Files window in picker mode. The D-Bus name is org.freedesktop.impl.portal.desktop.mist (mist.portal);
// a running Files process owns it, otherwise D-Bus starts `mist-ui-bin files --portal`.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub static OWNED: AtomicBool = AtomicBool::new(false);
static SPAWNED: AtomicBool = AtomicBool::new(false);
pub const NAME: &str = "org.freedesktop.impl.portal.desktop.mist";

pub type Reply = Option<(Vec<String>, Option<usize>)>;

pub struct Request {
    pub save: bool,
    pub multiple: bool,
    pub directory: bool,
    pub title: String,
    pub accept: Option<String>,
    pub filters: Vec<(String, Vec<String>)>, // "*.png" or "mime:image/png"
    pub cur_filter: usize,
    pub folder: Option<String>,
    pub file: Option<String>,
    pub name: String,
    pub reply: mpsc::Sender<Reply>,
}

type Tx = calloop::channel::Sender<Box<dyn std::any::Any + Send>>;
struct Chooser {
    tx: Tx,
}

fn opt_bool(o: &HashMap<String, OwnedValue>, k: &str) -> bool {
    matches!(o.get(k).map(|v| &**v), Some(Value::Bool(true)))
}
fn opt_str(o: &HashMap<String, OwnedValue>, k: &str) -> Option<String> {
    match o.get(k).map(|v| &**v) {
        Some(Value::Str(s)) => Some(s.to_string()),
        _ => None,
    }
}
// "ay" paths are NUL-terminated bytes
fn bytes_path(v: &Value) -> Option<String> {
    let Value::Array(a) = v else { return None };
    let mut b: Vec<u8> = a.iter().filter_map(|x| if let Value::U8(c) = x { Some(*c) } else { None }).collect();
    while b.last() == Some(&0) {
        b.pop();
    }
    (!b.is_empty()).then(|| String::from_utf8_lossy(&b).into_owned())
}
// (sa(us)): name, [(0 glob | 1 mime, pattern)]
fn filter(v: &Value) -> Option<(String, Vec<String>)> {
    let Value::Structure(s) = v else { return None };
    let f = s.fields();
    let Some(Value::Str(name)) = f.first() else { return None };
    let Some(Value::Array(pats)) = f.get(1) else { return None };
    let mut out = vec![];
    for p in pats.iter() {
        if let Value::Structure(ps) = p {
            if let [Value::U32(kind), Value::Str(pat)] = ps.fields() {
                out.push(if *kind == 1 { format!("mime:{}", pat.as_str()) } else { pat.to_string() });
            }
        }
    }
    Some((name.to_string(), out))
}

impl Chooser {
    async fn ask(&self, save: bool, title: String, o: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        let mut filters: Vec<(String, Vec<String>)> = vec![];
        if let Some(Value::Array(a)) = o.get("filters").map(|v| &**v) {
            filters = a.iter().filter_map(filter).collect();
        }
        let cur = o.get("current_filter").and_then(|v| filter(v));
        let cur_filter = match &cur {
            Some(c) => match filters.iter().position(|f| f == c) {
                Some(i) => i,
                None => {
                    filters.insert(0, c.clone());
                    0
                }
            },
            None => 0,
        };
        let file = o.get("current_file").and_then(|v| bytes_path(v));
        let mut name = opt_str(&o, "current_name").unwrap_or_default();
        let mut folder = o.get("current_folder").and_then(|v| bytes_path(v));
        if let Some(f) = &file {
            if folder.is_none() {
                folder = Some(super::fs::parent(f));
            }
            if save && name.is_empty() {
                name = f.rsplit('/').next().unwrap_or("").to_string();
            }
        }
        let (rtx, rrx) = mpsc::channel();
        let req = Request {
            save,
            multiple: opt_bool(&o, "multiple"),
            directory: opt_bool(&o, "directory"),
            title,
            accept: opt_str(&o, "accept_label").map(|s| s.replace('_', "")),
            filters: filters.clone(),
            cur_filter,
            folder,
            file: if save { None } else { file },
            name,
            reply: rtx,
        };
        if self.tx.send(Box::new(req)).is_err() {
            return (2, HashMap::new());
        }
        // the window answers (or is closed: cancel)
        let r = loop {
            match rrx.try_recv() {
                Ok(r) => break r,
                Err(mpsc::TryRecvError::Disconnected) => break None,
                Err(mpsc::TryRecvError::Empty) => async_io::Timer::after(std::time::Duration::from_millis(100)).await,
            };
        };
        let mut res: HashMap<String, OwnedValue> = HashMap::new();
        let Some((uris, fi)) = r else { return (1, res) };
        if let Ok(v) = OwnedValue::try_from(Value::from(uris)) {
            res.insert("uris".into(), v);
        }
        if let Some((n, pats)) = fi.and_then(|i| filters.get(i).cloned()) {
            let pats: Vec<(u32, String)> = pats.into_iter().map(|p| match p.strip_prefix("mime:") {
                Some(m) => (1, m.to_string()),
                None => (0, p),
            }).collect();
            if let Ok(v) = OwnedValue::try_from(Value::from((n, pats))) {
                res.insert("current_filter".into(), v);
            }
        }
        (0, res)
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl Chooser {
    async fn open_file(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, title: String, options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        self.ask(false, title, options).await
    }
    async fn save_file(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, title: String, options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        self.ask(true, title, options).await
    }
    // several files into one folder: a folder picker; the uris are the folder + each name
    async fn save_files(&self, _handle: OwnedObjectPath, _app_id: String, _parent_window: String, title: String, mut options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        let names: Vec<String> = match options.get("files").map(|v| &**v) {
            Some(Value::Array(a)) => a.iter().filter_map(bytes_path).collect(),
            _ => vec![],
        };
        options.insert("directory".into(), OwnedValue::from(true));
        let (code, mut res) = self.ask(false, title, options).await;
        if code == 0 {
            let dir = match res.get("uris").map(|v| &**v) {
                Some(Value::Array(a)) => a.iter().find_map(|x| if let Value::Str(s) = x { Some(s.to_string()) } else { None }),
                _ => None,
            };
            if let Some(d) = dir {
                let d = d.trim_end_matches('/').to_string();
                let uris: Vec<String> = names.iter().map(|n| format!("{d}/{}", crate::appwin::path_to_uri(n).rsplit('/').next().unwrap_or(""))).collect();
                if let Ok(v) = OwnedValue::try_from(Value::from(uris)) {
                    res.insert("uris".into(), v);
                }
            }
        }
        (code, res)
    }
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        4
    }
}

// serve the backend on the session bus (once per process)
pub fn spawn(tx: Tx) {
    if SPAWNED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("portal".into()).stack_size(256 * 1024).spawn(move || {
        let run = || -> zbus::Result<()> {
            let c = zbus::blocking::connection::Builder::session()?.serve_at("/org/freedesktop/portal/desktop", Chooser { tx })?.build()?;
            c.request_name(NAME)?;
            OWNED.store(true, Ordering::Relaxed);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        };
        if let Err(e) = run() {
            eprintln!("files portal: {e}");
        }
    });
}
