// mistd nvidia: after an update the NVIDIA libraries are new while the loaded kernel module is still the old one;
// GL clients then fail (black windows, a compositor that cannot start) until a reboot. Compares
// /sys/module/nvidia/version with the versions of /usr/lib/libnvidia-glcore.so.* and says so once per pair.
// Run by integrations/nvidia/mist-nvidia-check.path whenever pacman replaces the library.
use std::path::Path;

// (loaded, installed) when the module and every installed library differ; None when they match or there is no NVIDIA
pub fn mismatch(module_version: &Path, libdir: &Path) -> Option<(String, String)> {
    let loaded = std::fs::read_to_string(module_version).ok()?.trim().to_string();
    let mut installed: Vec<String> = std::fs::read_dir(libdir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_prefix("libnvidia-glcore.so.").map(str::to_string))
        .filter(|v| v.contains('.'))
        .collect();
    installed.sort();
    if loaded.is_empty() || installed.is_empty() || installed.contains(&loaded) {
        return None;
    }
    Some((loaded, installed.pop()?))
}

pub fn main() {
    let Some((loaded, installed)) = mismatch(Path::new("/sys/module/nvidia/version"), Path::new("/usr/lib")) else { return };
    let mark = format!("{}/.cache/mist/nvidia-warned", crate::common::home());
    let pair = format!("{loaded} {installed}");
    if std::fs::read_to_string(&mark).map(|s| s.trim() == pair).unwrap_or(false) {
        return;
    }
    println!("nvidia: module {loaded}, libraries {installed}");
    let body = format!("Załadowany moduł {loaded}, biblioteki {installed}. Uruchom komputer ponownie, zanim otworzysz aplikacje z GL.");
    let (rc, _) = crate::common::run(&["notify-send", "-a", "Mist", "-u", "critical", "-i", "dialog-warning", "Sterownik NVIDIA zaktualizowany", &body], 5.0);
    if rc == 0 {
        let _ = std::fs::create_dir_all(format!("{}/.cache/mist", crate::common::home()));
        let _ = std::fs::write(&mark, pair);
    }
}

#[cfg(test)]
mod tests {
    use super::mismatch;

    #[test]
    fn module_against_libraries() {
        let d = std::env::temp_dir().join(format!("mistd-nvidia-{}", std::process::id()));
        let lib = d.join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        let ver = d.join("version");
        // no module: no NVIDIA, nothing to say
        assert_eq!(mismatch(&ver, &lib), None);
        std::fs::write(&ver, "615.78.08\n").unwrap();
        // no libraries either
        assert_eq!(mismatch(&ver, &lib), None);
        for n in ["libnvidia-glcore.so", "libnvidia-glcore.so.615.78.08"] {
            std::fs::write(lib.join(n), b"").unwrap();
        }
        assert_eq!(mismatch(&ver, &lib), None);
        std::fs::remove_file(lib.join("libnvidia-glcore.so.615.78.08")).unwrap();
        std::fs::write(lib.join("libnvidia-glcore.so.620.10.02"), b"").unwrap();
        assert_eq!(mismatch(&ver, &lib), Some(("615.78.08".into(), "620.10.02".into())));
        let _ = std::fs::remove_dir_all(&d);
    }
}
