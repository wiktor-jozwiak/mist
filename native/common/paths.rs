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
