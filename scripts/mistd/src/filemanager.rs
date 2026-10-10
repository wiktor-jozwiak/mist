// org.freedesktop.FileManager1 for Mist Files (D-Bus activated: ~/.local/share/dbus-1/services/
// org.freedesktop.FileManager1.service). "Show in folder" from browsers, Electron apps, portals:
// ShowFolders opens the folders, ShowItems / ShowItemProperties open the parent with the item selected.
// Each request runs ~/.local/bin/mist-files (IPC to the running instance or a new one). Exits after
// 10 idle minutes; D-Bus starts it again on the next call.
use crate::common::*;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static LAST: AtomicU64 = AtomicU64::new(0);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn launch(uri: &str, select: bool) {
    LAST.store(now(), Ordering::Relaxed);
    let bin = format!("{}/.local/bin/mist-files", home());
    let mut c = Command::new(&bin);
    if select {
        c.arg("--select");
    }
    c.arg(uri).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    if let Ok(mut ch) = c.spawn() {
        // reap the launcher (it returns at once when the instance runs; else it becomes the instance)
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
    }
}

struct Fm;

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl Fm {
    fn show_folders(&self, uris: Vec<String>, _startup_id: String) {
        for u in uris {
            launch(&u, false);
        }
    }
    fn show_items(&self, uris: Vec<String>, _startup_id: String) {
        for u in uris {
            launch(&u, true);
        }
    }
    fn show_item_properties(&self, uris: Vec<String>, _startup_id: String) {
        for u in uris {
            launch(&u, true);
        }
    }
}

pub fn main() -> zbus::Result<()> {
    LAST.store(now(), Ordering::Relaxed);
    let _conn = zbus::blocking::connection::Builder::session()?
        .name("org.freedesktop.FileManager1")?
        .serve_at("/org/freedesktop/FileManager1", Fm)?
        .build()?;
    loop {
        std::thread::sleep(Duration::from_secs(30));
        if now().saturating_sub(LAST.load(Ordering::Relaxed)) > 600 {
            return Ok(());
        }
    }
}
