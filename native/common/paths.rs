// Where Mist's files are (scripts, shaders, assets, mistd), so the repo can live anywhere:
// $MIST_DIR, then ~/.local/share/mist/shell (native/install.sh links it to the checkout), then /usr/share/mist
// (a package), then the old fixed place ~/.config/quickshell/mist.
use std::sync::OnceLock;

pub fn mist_dir() -> &'static str {
    static DIR: OnceLock<String> = OnceLock::new();
    DIR.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let data = std::env::var("XDG_DATA_HOME").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| format!("{home}/.local/share"));
        let tries = [std::env::var("MIST_DIR").unwrap_or_default(), format!("{data}/mist/shell"), "/usr/share/mist".into(), format!("{home}/.config/quickshell/mist")];
        tries.iter().find(|d| !d.is_empty() && std::path::Path::new(&format!("{d}/native/assets")).is_dir()).cloned().unwrap_or_else(|| tries[3].clone())
    })
}

// a file inside it: mist_path("scripts/shot.sh")
pub fn mist_path(rel: &str) -> String {
    format!("{}/{rel}", mist_dir())
}

// the mistd backend: the checkout's build, else the one on $PATH (a package installs /usr/bin/mistd)
#[allow(dead_code)]
pub fn mistd() -> String {
    let p = mist_path("scripts/bin/mistd");
    if std::path::Path::new(&p).exists() { p } else { "mistd".into() }
}

// mist-ui-bin itself, for the commands it runs on its own (lock, msg): wherever it was installed
#[allow(dead_code)]
pub fn self_bin() -> String {
    std::env::current_exe().ok().filter(|p| p.file_name().map(|n| n == "mist-ui-bin" || n == "mist-ui").unwrap_or(false)).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| {
        let local = format!("{}/.local/bin/mist-ui-bin", std::env::var("HOME").unwrap_or_default());
        if std::path::Path::new(&local).exists() { local } else { "mist-ui-bin".into() }
    })
}

// XDG Pictures (~/.config/user-dirs.dirs), else ~/Pictures
#[allow(dead_code)]
pub fn pictures_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let cfg = std::env::var("XDG_CONFIG_HOME").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| format!("{home}/.config"));
    std::fs::read_to_string(format!("{cfg}/user-dirs.dirs")).ok().and_then(|s| {
        s.lines().find_map(|l| l.trim().strip_prefix("XDG_PICTURES_DIR=").map(|v| v.trim_matches('"').replace("$HOME", &home)))
    }).filter(|d| !d.is_empty() && *d != home).unwrap_or_else(|| format!("{home}/Pictures"))
}

// the photo wallpapers: $MIST_WALLPAPERS, else Pictures/Wallpapers ("Tapety" in Polish)
#[allow(dead_code)]
pub fn wallpaper_dir() -> String {
    std::env::var("MIST_WALLPAPERS").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| {
        let sub = if std::env::var("LANG").unwrap_or_default().starts_with("pl") { "Tapety" } else { "Wallpapers" };
        format!("{}/{sub}", pictures_dir())
    })
}

// the main monitor (dock, island, desktop, the weather sky): $MIST_PRIMARY, else "primary" in
// ~/.config/mist/settings.json, else Hyprland's first monitor. Asked once per process.
#[allow(dead_code)]
pub fn primary_monitor() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| {
        // the string value of the first "key" in a JSON text (no serde here: mist-comp shares this file)
        let field = |s: &str, key: &str| -> Option<String> {
            let r = &s[s.find(&format!("\"{key}\""))? + key.len() + 2..];
            let r = &r[r.find('"')? + 1..];
            Some(r[..r.find('"')?].to_string())
        };
        if let Some(p) = std::env::var("MIST_PRIMARY").ok().filter(|p| !p.is_empty()) {
            return p;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let cfg = std::env::var("XDG_CONFIG_HOME").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| format!("{home}/.config"));
        if let Some(p) = std::fs::read_to_string(format!("{cfg}/mist/settings.json")).ok().and_then(|s| field(&s, "primary")).filter(|p| !p.is_empty()) {
            return p;
        }
        std::process::Command::new("hyprctl").args(["monitors", "-j"]).output().ok().and_then(|o| field(&String::from_utf8_lossy(&o.stdout), "name")).unwrap_or_default()
    })
}

// $XDG_RUNTIME_DIR/mist/screen.json {"off": bool, "locked": bool}: idle writes "off" as the monitors go off and
// on, the locker "locked" as it locks and unlocks (None keeps a value); mist-wall (inotify) pauses meanwhile
#[allow(dead_code)]
pub fn screen_note(off: Option<bool>, locked: Option<bool>) {
    let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let p = format!("{dir}/screen.json");
    let old = std::fs::read_to_string(&p).unwrap_or_default();
    let was = |k: &str| old.contains(&format!("\"{k}\": true"));
    let (off, locked) = (off.unwrap_or_else(|| was("off")), locked.unwrap_or_else(|| was("locked")));
    let _ = std::fs::create_dir_all(&dir);
    let tmp = format!("{p}.tmp");
    if std::fs::write(&tmp, format!("{{\"off\": {off}, \"locked\": {locked}}}")).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

// nobody sees the desktop: the monitors are off, or the session is locked ("locked" and the locker still holds
// lock.lock, so a crashed locker does not keep the wallpaper paused)
#[allow(dead_code)]
pub fn screen_asleep() -> bool {
    let dir = format!("{}/mist", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()));
    let s = std::fs::read_to_string(format!("{dir}/screen.json")).unwrap_or_default();
    if s.contains("\"off\": true") {
        return true;
    }
    if !s.contains("\"locked\": true") {
        return false;
    }
    let Ok(f) = std::fs::OpenOptions::new().read(true).open(format!("{dir}/lock.lock")) else { return false };
    let fd = std::os::fd::AsRawFd::as_raw_fd(&f);
    if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        unsafe { libc::flock(fd, libc::LOCK_UN) };
        return false;
    }
    true
}
