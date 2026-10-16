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
