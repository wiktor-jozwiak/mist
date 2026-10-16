// Mist Files without gio / python: folder listing, MIME types from shared-mime-info globs, the FreeDesktop
// home trash, copy / move with progress and cancel, drives from /proc/self/mounts, thumbnails.
use std::collections::HashMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub name: String,
    pub path: String, // absolute, or trash:///<name in Trash/files>
    pub size: u64,
    pub dir: bool,
    pub mime: String,
    pub mtime: i64,
    pub hidden: bool,
    pub link: bool,
    pub orig: String,    // trash: original path
    pub deleted: String, // trash: deletion date (ISO)
}

pub fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}
pub fn trash_dir() -> String {
    format!("{}/Trash", std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| format!("{}/.local/share", home())))
}
pub fn base(p: &str) -> String {
    if p == "/" {
        return "Dysk systemowy".into();
    }
    if p == home() {
        return "Katalog domowy".into();
    }
    if p.starts_with("trash:") {
        return "Kosz".into();
    }
    p.rsplit('/').next().unwrap_or(p).to_string()
}
pub fn parent(p: &str) -> String {
    if p == "/" || p.starts_with("trash:") {
        return String::new();
    }
    match p.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => p[..i].to_string(),
    }
}
pub fn join(d: &str, n: &str) -> String {
    if d == "/" { format!("/{n}") } else { format!("{d}/{n}") }
}
pub fn ext(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

// ---------------------------------------------------------------- MIME (shared-mime-info globs2)
pub struct MimeDb {
    ext: HashMap<String, (u32, String)>,
    literal: HashMap<String, String>,
    icons: HashMap<String, String>,
    comments: HashMap<&'static str, &'static str>,
}
impl MimeDb {
    pub fn load() -> MimeDb {
        let mut ext: HashMap<String, (u32, String)> = HashMap::new();
        let mut literal = HashMap::new();
        for dir in ["/usr/share/mime", &format!("{}/.local/share/mime", home())] {
            let Ok(s) = std::fs::read_to_string(format!("{dir}/globs2")) else { continue };
            for l in s.lines() {
                if l.starts_with('#') {
                    continue;
                }
                let mut it = l.splitn(4, ':');
                let (Some(w), Some(m), Some(g)) = (it.next(), it.next(), it.next()) else { continue };
                let w: u32 = w.parse().unwrap_or(50);
                if let Some(e) = g.strip_prefix("*.") {
                    if e.contains(['*', '?', '[']) {
                        continue;
                    }
                    let k = e.to_lowercase();
                    if ext.get(&k).map(|(ow, _)| *ow < w).unwrap_or(true) {
                        ext.insert(k, (w, m.to_string()));
                    }
                } else if !g.contains(['*', '?', '[']) {
                    literal.insert(g.to_string(), m.to_string());
                }
            }
        }
        let mut icons = HashMap::new();
        if let Ok(s) = std::fs::read_to_string("/usr/share/mime/generic-icons") {
            for l in s.lines() {
                if let Some((m, i)) = l.split_once(':') {
                    icons.insert(m.to_string(), i.to_string());
                }
            }
        }
        let comments: HashMap<&'static str, &'static str> = [
            ("inode/directory", "Folder plików"),
            ("text/plain", "Dokument tekstowy"),
            ("application/pdf", "Dokument PDF"),
            ("image/png", "Obraz PNG"),
            ("image/jpeg", "Obraz JPEG"),
            ("image/gif", "Obraz GIF"),
            ("image/webp", "Obraz WebP"),
            ("image/svg+xml", "Obraz SVG"),
            ("video/mp4", "Wideo MP4"),
            ("video/x-matroska", "Wideo MKV"),
            ("video/webm", "Wideo WebM"),
            ("audio/mpeg", "Dźwięk MP3"),
            ("audio/flac", "Dźwięk FLAC"),
            ("audio/ogg", "Dźwięk OGG"),
            ("application/zip", "Archiwum ZIP"),
            ("application/x-7z-compressed", "Archiwum 7z"),
            ("application/x-compressed-tar", "Archiwum TAR.GZ"),
            ("application/x-tar", "Archiwum TAR"),
            ("application/vnd.rar", "Archiwum RAR"),
            ("application/x-shellscript", "Skrypt powłoki"),
            ("text/x-python", "Skrypt Python"),
            ("text/x-python3", "Skrypt Python"),
            ("application/json", "Plik JSON"),
            ("text/html", "Strona HTML"),
            ("text/markdown", "Dokument Markdown"),
            ("application/x-executable", "Aplikacja"),
            ("application/x-sharedlib", "Biblioteka"),
            ("text/x-qml", "Plik QML"),
            ("text/x-c++src", "Kod C++"),
            ("text/x-csrc", "Kod C"),
            ("text/rust", "Kod Rust"),
            ("application/vnd.openxmlformats-officedocument.wordprocessingml.document", "Dokument Word"),
            ("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "Arkusz Excel"),
            ("application/x-desktop", "Skrót aplikacji"),
            ("inode/symlink", "Skrót"),
        ]
        .into_iter()
        .collect();
        MimeDb { ext, literal, icons, comments }
    }
    // MIME of a file name (dirs: inode/directory); unknown: sniffed lazily by the caller
    pub fn guess(&self, name: &str) -> Option<&str> {
        if let Some(m) = self.literal.get(name) {
            return Some(m);
        }
        let lower = name.to_lowercase();
        // longest extension first: "a.tar.gz" -> "tar.gz", then "gz"
        let mut i = 0;
        while let Some(k) = lower[i..].find('.') {
            let e = &lower[i + k + 1..];
            if i + k > 0 {
                if let Some((_, m)) = self.ext.get(e) {
                    return Some(m);
                }
            }
            i += k + 1;
        }
        None
    }
    pub fn icon_names(&self, mime: &str) -> Vec<String> {
        let mut v = vec![mime.replace('/', "-")];
        if let Some(g) = self.icons.get(mime) {
            v.push(g.clone());
        }
        v.push(format!("{}-x-generic", mime.split('/').next().unwrap_or("text")));
        v.push("text-x-generic".into());
        v
    }
    pub fn type_name(&self, e: &Entry) -> String {
        if e.dir {
            return "Folder plików".into();
        }
        if let Some(c) = self.comments.get(e.mime.as_str()) {
            return c.to_string();
        }
        let x = ext(&e.name);
        if x.is_empty() { "Plik".into() } else { format!("Plik {}", x.to_uppercase()) }
    }
}

// content sniff for names without a known glob: ELF / script / text / binary
fn sniff(path: &str, meta: &std::fs::Metadata) -> String {
    use std::io::Read;
    let mut b = [0u8; 512];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
    let b = &b[..n];
    if b.starts_with(b"\x7fELF") {
        return if meta.permissions().mode() & 0o111 != 0 { "application/x-executable".into() } else { "application/x-sharedlib".into() };
    }
    if b.starts_with(b"#!") {
        let l = String::from_utf8_lossy(&b[..b.iter().position(|&c| c == b'\n').unwrap_or(b.len())]).to_string();
        return if l.contains("python") { "text/x-python".into() } else { "application/x-shellscript".into() };
    }
    if n == 0 || (!b.contains(&0) && std::str::from_utf8(b).is_ok()) {
        return "text/plain".into();
    }
    "application/octet-stream".into()
}

// ---------------------------------------------------------------- listing
pub fn list(path: &str, mime: &MimeDb) -> Result<Vec<Entry>, String> {
    if path.starts_with("trash:") {
        return Ok(trash_list(mime));
    }
    let rd = std::fs::read_dir(path).map_err(err_pl)?;
    let mut out = Vec::new();
    for d in rd.flatten() {
        let name = d.file_name().to_string_lossy().into_owned();
        let p = join(path, &name);
        let Ok(lm) = d.metadata() else { continue };
        let link = lm.file_type().is_symlink();
        // a link shows what it points to
        let m = if link { std::fs::metadata(&p).unwrap_or(lm) } else { lm };
        let dir = m.is_dir();
        let mt = if dir {
            "inode/directory".to_string()
        } else {
            match mime.guess(&name) {
                Some(x) => x.to_string(),
                None => sniff(&p, &m),
            }
        };
        out.push(Entry { hidden: name.starts_with('.'), size: if dir { 0 } else { m.len() }, mtime: m.mtime(), name, path: p, dir, mime: mt, link, ..Default::default() });
    }
    Ok(out)
}

pub fn err_pl(e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "Brak dostępu do tego folderu".into(),
        std::io::ErrorKind::NotFound => "Ten folder nie istnieje".into(),
        _ => e.to_string(),
    }
}

// sort + filter (dirs first; natural, case-insensitive names)
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n1 = String::new();
                while let Some(c) = x.peek().copied().filter(|c| c.is_ascii_digit()) {
                    n1.push(c);
                    x.next();
                }
                let mut n2 = String::new();
                while let Some(c) = y.peek().copied().filter(|c| c.is_ascii_digit()) {
                    n2.push(c);
                    y.next();
                }
                let (t1, t2) = (n1.trim_start_matches('0'), n2.trim_start_matches('0'));
                let o = t1.len().cmp(&t2.len()).then(t1.cmp(t2));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let (k1, k2) = (fold(c), fold(d));
                if k1 != k2 {
                    return k1.cmp(&k2);
                }
                x.next();
                y.next();
            }
        }
    }
}
// Polish collation, roughly: letters by base letter (ą after a), case ignored
fn fold(c: char) -> (u32, u32) {
    let l = c.to_lowercase().next().unwrap_or(c);
    let (b, k) = match l {
        'ą' => ('a', 1),
        'ć' => ('c', 1),
        'ę' => ('e', 1),
        'ł' => ('l', 1),
        'ń' => ('n', 1),
        'ó' => ('o', 1),
        'ś' => ('s', 1),
        'ź' => ('z', 1),
        'ż' => ('z', 2),
        _ => (l, 0),
    };
    // punctuation before digits before letters (like ICU)
    let class = if b.is_alphabetic() { 2 } else if b.is_ascii_digit() { 1 } else { 0 };
    (class << 24 | b as u32, k)
}

// ---------------------------------------------------------------- the trash (FreeDesktop spec, home trash)
fn trash_info(name: &str) -> (String, String) {
    let (mut p, mut d) = (String::new(), String::new());
    if let Ok(s) = std::fs::read_to_string(format!("{}/info/{name}.trashinfo", trash_dir())) {
        for l in s.lines() {
            if let Some(v) = l.strip_prefix("Path=") {
                p = crate::appwin::uri_to_path(v);
            } else if let Some(v) = l.strip_prefix("DeletionDate=") {
                d = v.to_string();
            }
        }
    }
    (p, d)
}
pub fn trash_list(mime: &MimeDb) -> Vec<Entry> {
    let files = format!("{}/files", trash_dir());
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&files) else { return out };
    for d in rd.flatten() {
        let n = d.file_name().to_string_lossy().into_owned();
        let Ok(m) = d.metadata() else { continue };
        let (orig, del) = trash_info(&n);
        let dir = m.is_dir();
        let shown = Path::new(&orig).file_name().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or(n.clone());
        let mt = if dir { "inode/directory".into() } else { mime.guess(&shown).unwrap_or("application/octet-stream").to_string() };
        out.push(Entry { size: if dir { du(&format!("{files}/{n}"), None) } else { m.len() }, path: format!("trash:///{n}"), name: shown, dir, mime: mt, mtime: m.mtime(), orig, deleted: del, ..Default::default() });
    }
    out
}
fn free_path(p: &str) -> String {
    if std::fs::symlink_metadata(p).is_err() {
        return p.to_string();
    }
    let (stem, e) = match p.rfind('.') {
        Some(i) if i > p.rfind('/').map(|j| j + 1).unwrap_or(0) => (&p[..i], &p[i..]),
        _ => (p, ""),
    };
    let mut i = 2;
    loop {
        let c = format!("{stem} ({i}){e}");
        if std::fs::symlink_metadata(&c).is_err() {
            return c;
        }
        i += 1;
    }
}
fn now_iso() -> String {
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}
// move paths into the home trash; another filesystem goes through `gio trash` (its own .Trash-uid)
pub fn trash(paths: &[String]) -> Result<(), String> {
    let t = trash_dir();
    let _ = std::fs::create_dir_all(format!("{t}/files"));
    let _ = std::fs::create_dir_all(format!("{t}/info"));
    let tdev = std::fs::metadata(format!("{t}/files")).map(|m| m.dev()).unwrap_or(0);
    let mut bad = Vec::new();
    for p in paths {
        let Ok(m) = std::fs::symlink_metadata(p) else { continue };
        if m.dev() != tdev {
            let ok = std::process::Command::new("gio").args(["trash", "--", p]).status().map(|s| s.success()).unwrap_or(false);
            if !ok {
                bad.push(p.clone());
            }
            continue;
        }
        let base = Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or("x".into());
        // a name free in files/ and info/ (the info file is created exclusively first, per spec)
        let mut n = base.clone();
        let mut i = 2;
        loop {
            let info = format!("{t}/info/{n}.trashinfo");
            if std::fs::symlink_metadata(format!("{t}/files/{n}")).is_err() {
                if let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(&info) {
                    use std::io::Write;
                    let _ = write!(f, "[Trash Info]\nPath={}\nDeletionDate={}\n", crate::appwin::path_to_uri(p).trim_start_matches("file://"), now_iso());
                    break;
                }
            }
            let (stem, e) = match base.rfind('.') {
                Some(k) if k > 0 => (&base[..k], &base[k..]),
                _ => (base.as_str(), ""),
            };
            n = format!("{stem}.{i}{e}");
            i += 1;
        }
        if std::fs::rename(p, format!("{t}/files/{n}")).is_err() {
            let _ = std::fs::remove_file(format!("{t}/info/{n}.trashinfo"));
            bad.push(p.clone());
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(format!("Nie udało się przenieść do kosza: {}", bad.len())) }
}
// names in Trash/files (trash:///<name> paths)
pub fn trash_restore(names: &[String]) -> bool {
    let t = trash_dir();
    let mut ok = true;
    for n in names {
        let (orig, _) = trash_info(n);
        if orig.is_empty() {
            ok = false;
            continue;
        }
        let _ = std::fs::create_dir_all(parent(&orig));
        let dst = free_path(&orig);
        let src = format!("{t}/files/{n}");
        if std::fs::rename(&src, &dst).is_err() {
            // another filesystem: copy back, then remove
            if copy_tree(&src, &dst, &Progress::default()).is_err() {
                ok = false;
                continue;
            }
            let _ = remove_tree(&src);
        }
        let _ = std::fs::remove_file(format!("{t}/info/{n}.trashinfo"));
    }
    ok
}
pub fn trash_remove(names: &[String]) {
    let t = trash_dir();
    for n in names {
        let _ = remove_tree(&format!("{t}/files/{n}"));
        let _ = std::fs::remove_file(format!("{t}/info/{n}.trashinfo"));
    }
}
pub fn trash_empty() {
    let t = trash_dir();
    let names: Vec<String> = std::fs::read_dir(format!("{t}/files")).map(|r| r.flatten().map(|d| d.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    trash_remove(&names);
    if let Ok(r) = std::fs::read_dir(format!("{t}/info")) {
        for d in r.flatten() {
            let _ = std::fs::remove_file(d.path());
        }
    }
}
// trash names of these original paths, newest first (undo of a delete)
pub fn trash_find(origs: &[String]) -> Vec<String> {
    let t = trash_dir();
    let mut hits: Vec<(String, String, String)> = Vec::new();
    if let Ok(r) = std::fs::read_dir(format!("{t}/files")) {
        for d in r.flatten() {
            let n = d.file_name().to_string_lossy().into_owned();
            let (o, date) = trash_info(&n);
            if origs.contains(&o) {
                hits.push((date, o, n));
            }
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    hits.into_iter().filter(|(_, o, _)| seen.insert(o.clone())).map(|(_, _, n)| n).collect()
}
pub fn trash_count() -> usize {
    std::fs::read_dir(format!("{}/files", trash_dir())).map(|r| r.count()).unwrap_or(0)
}

// ---------------------------------------------------------------- copy / move / delete
#[derive(Default, Clone)]
pub struct Progress {
    pub done: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
}

// apparent size of a tree; stops at the deadline (None = no limit)
pub fn du(p: &str, deadline: Option<std::time::Instant>) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() {
        return m.len();
    }
    let mut t = 0;
    let mut stack = vec![p.to_string()];
    while let Some(d) = stack.pop() {
        if deadline.map(|dl| std::time::Instant::now() > dl).unwrap_or(false) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                stack.push(e.path().to_string_lossy().into_owned());
            } else {
                t += m.len();
            }
        }
    }
    t
}

fn copy_file(src: &str, dst: &str, m: &std::fs::Metadata, pr: &Progress) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let f = std::fs::File::open(src)?;
    let out = std::fs::OpenOptions::new().write(true).create_new(true).mode_compat(m.permissions().mode()).open(dst)?;
    // a reflink first (btrfs / xfs: instant), else copy_file_range in chunks (progress + cancel)
    let ficlone: libc::c_ulong = 0x40049409;
    if unsafe { libc::ioctl(out.as_raw_fd(), ficlone as _, f.as_raw_fd()) } == 0 {
        pr.done.fetch_add(m.len(), Ordering::Relaxed);
    } else {
        let mut left = m.len();
        while left > 0 {
            if pr.cancel.load(Ordering::Relaxed) {
                drop(out);
                let _ = std::fs::remove_file(dst);
                return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = unsafe { libc::copy_file_range(f.as_raw_fd(), std::ptr::null_mut(), out.as_raw_fd(), std::ptr::null_mut(), left.min(8 << 20) as usize, 0) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if left == m.len() && matches!(e.raw_os_error(), Some(libc::EXDEV) | Some(libc::ENOSYS) | Some(libc::EINVAL) | Some(libc::EOPNOTSUPP)) {
                    // old kernel / special fs: plain read + write
                    drop(out);
                    let _ = std::fs::remove_file(dst);
                    let mut i = std::fs::File::open(src)?;
                    let mut o = std::fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
                    let mut buf = vec![0u8; 1 << 20];
                    use std::io::{Read, Write};
                    loop {
                        if pr.cancel.load(Ordering::Relaxed) {
                            drop(o);
                            let _ = std::fs::remove_file(dst);
                            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
                        }
                        let k = i.read(&mut buf)?;
                        if k == 0 {
                            break;
                        }
                        o.write_all(&buf[..k])?;
                        pr.done.fetch_add(k as u64, Ordering::Relaxed);
                    }
                    let _ = std::fs::set_permissions(dst, m.permissions());
                    set_times(dst, m);
                    return Ok(());
                }
                return Err(e);
            }
            if n == 0 {
                break;
            }
            left -= n as u64;
            pr.done.fetch_add(n as u64, Ordering::Relaxed);
        }
    }
    let _ = std::fs::set_permissions(dst, m.permissions());
    set_times(dst, m);
    Ok(())
}
trait ModeCompat {
    fn mode_compat(&mut self, m: u32) -> &mut Self;
}
impl ModeCompat for std::fs::OpenOptions {
    fn mode_compat(&mut self, m: u32) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt;
        self.mode(m & 0o7777 | 0o200)
    }
}
fn set_times(p: &str, m: &std::fs::Metadata) {
    let c = std::ffi::CString::new(p).unwrap_or_default();
    let ts = [libc::timespec { tv_sec: m.atime(), tv_nsec: m.atime_nsec() }, libc::timespec { tv_sec: m.mtime(), tv_nsec: m.mtime_nsec() }];
    unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), ts.as_ptr(), libc::AT_SYMLINK_NOFOLLOW) };
}

pub fn copy_tree(src: &str, dst: &str, pr: &Progress) -> std::io::Result<()> {
    let m = std::fs::symlink_metadata(src)?;
    if m.file_type().is_symlink() {
        let t = std::fs::read_link(src)?;
        std::os::unix::fs::symlink(t, dst)?;
    } else if m.is_dir() {
        std::fs::create_dir(dst)?;
        for e in std::fs::read_dir(src)?.flatten() {
            if pr.cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = e.file_name().to_string_lossy().into_owned();
            copy_tree(&join(src, &n), &join(dst, &n), pr)?;
        }
        let _ = std::fs::set_permissions(dst, m.permissions());
        set_times(dst, &m);
    } else if m.is_file() {
        copy_file(src, dst, &m, pr)?;
    }
    Ok(())
}
pub fn remove_tree(p: &str) -> std::io::Result<()> {
    let m = std::fs::symlink_metadata(p)?;
    if m.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) }
}
// move: a rename on one filesystem, else copy + remove (the source stays when the copy fails)
pub fn move_tree(src: &str, dst: &str, pr: &Progress) -> std::io::Result<()> {
    if std::fs::symlink_metadata(dst).is_ok() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "exists"));
    }
    match std::fs::rename(src, dst) {
        Ok(()) => {
            pr.done.fetch_add(du(dst, Some(std::time::Instant::now() + std::time::Duration::from_millis(200))), Ordering::Relaxed);
            Ok(())
        }
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => match copy_tree(src, dst, pr) {
            Ok(()) => remove_tree(src),
            Err(e) => {
                let _ = remove_tree(dst);
                Err(e)
            }
        },
        Err(e) => Err(e),
    }
}
pub fn same_fs(a: &str, b: &str) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev(),
        _ => false,
    }
}

// free name in a folder for new items and pasted copies (Windows: "x - kopia", "x - kopia (2)", "Nowy folder (2)")
pub fn free_name(names: &[String], wanted: &str, copy: bool, is_dir: bool) -> String {
    if !names.iter().any(|n| n == wanted) {
        return wanted.to_string();
    }
    let x = if is_dir { "" } else { ext(wanted) };
    let dot = if x.is_empty() { String::new() } else { format!(".{x}") };
    let stem = &wanted[..wanted.len() - dot.len()];
    let taken = |n: &str| names.iter().any(|m| m == n);
    if copy {
        let mut n = format!("{stem} - kopia{dot}");
        let mut i = 2;
        while taken(&n) {
            n = format!("{stem} - kopia ({i}){dot}");
            i += 1;
        }
        return n;
    }
    let mut n = wanted.to_string();
    let mut i = 2;
    while taken(&n) {
        n = format!("{stem} ({i}){dot}");
        i += 1;
    }
    n
}

// ---------------------------------------------------------------- drives
#[derive(Clone, Debug)]
pub struct Drive {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub free: u64,
    pub removable: bool,
}
pub fn drives() -> Vec<Drive> {
    let s = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let mut by_src: HashMap<String, Drive> = HashMap::new();
    for l in s.lines() {
        let t: Vec<&str> = l.split(' ').collect();
        if t.len() < 3 {
            continue;
        }
        let (src, fs) = (t[0], t[2]);
        let mp = t[1].replace("\\040", " ");
        if !src.starts_with('/') || ["tmpfs", "devtmpfs", "efivarfs", "overlay", "squashfs", "proc", "sysfs"].contains(&fs) {
            continue;
        }
        if mp.starts_with("/boot") || mp.starts_with("/efi") || mp.starts_with("/var/") || mp.starts_with("/snap") {
            continue;
        }
        let c = std::ffi::CString::new(mp.clone()).unwrap_or_default();
        let mut sv: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut sv) } != 0 || sv.f_blocks == 0 {
            continue;
        }
        let size = sv.f_blocks * sv.f_frsize;
        let free = sv.f_bavail * sv.f_frsize;
        // one entry per device: "/" wins, else the shortest mount point (btrfs subvolumes share it)
        let better = match by_src.get(src) {
            None => true,
            Some(p) => mp == "/" || (p.path != "/" && mp.len() < p.path.len()),
        };
        if better {
            let nm = mp.rsplit('/').next().unwrap_or("").to_string();
            let mut ch = nm.chars();
            let cap = ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default();
            by_src.insert(src.to_string(), Drive { name: if mp == "/" { "Dysk systemowy".into() } else { cap }, removable: mp.starts_with("/run/media") || mp.starts_with("/media"), path: mp, size, free });
        }
    }
    let mut v: Vec<Drive> = by_src.into_values().collect();
    v.sort_by(|a, b| (a.path != "/").cmp(&(b.path != "/")).then(a.path.cmp(&b.path)));
    v
}

// ---------------------------------------------------------------- user folders
pub fn xdg_dirs() -> HashMap<String, String> {
    let h = home();
    let mut m = HashMap::new();
    if let Ok(s) = std::fs::read_to_string(format!("{h}/.config/user-dirs.dirs")) {
        for l in s.lines() {
            let l = l.trim();
            if let Some(r) = l.strip_prefix("XDG_") {
                if let Some((k, v)) = r.split_once("_DIR=") {
                    m.insert(k.to_string(), v.trim_matches('"').replace("$HOME", &h));
                }
            }
        }
    }
    m
}

// ---------------------------------------------------------------- formatting
pub fn size(b: u64) -> String {
    if b < 1024 {
        return format!("{b} B");
    }
    let u = ["KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    v /= 1024.0;
    while v >= 1024.0 && i < u.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let s = if v < 10.0 { format!("{v:.1}") } else { format!("{}", v.round()) };
    format!("{} {}", s.replace('.', ","), u[i])
}
pub fn date(t: i64) -> String {
    if t == 0 {
        return String::new();
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&(t as libc::time_t), &mut tm) };
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut nt: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut nt) };
    let midnight = now - (nt.tm_hour * 3600 + nt.tm_min * 60 + nt.tm_sec) as i64;
    let hm = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
    if t >= midnight {
        format!("Dzisiaj, {hm}")
    } else if t >= midnight - 86400 {
        format!("Wczoraj, {hm}")
    } else {
        format!("{}.{:02}.{}, {hm}", tm.tm_mday, tm.tm_mon + 1, tm.tm_year + 1900)
    }
}
pub fn count_pl(n: usize) -> String {
    let w = if n == 1 {
        "element"
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        "elementy"
    } else {
        "elementów"
    };
    format!("{n} {w}")
}

// ---------------------------------------------------------------- md5 (thumbnail cache names, FreeDesktop spec)
pub fn md5_hex(input: &[u8]) -> String {
    let s: [u32; 64] = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let (mut a0, mut b0, mut c0, mut d0) = (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    let mut msg = input.to_vec();
    let bits = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(s[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    [a0, b0, c0, d0].iter().flat_map(|v| v.to_le_bytes()).map(|b| format!("{b:02x}")).collect()
}

// a cached FreeDesktop thumbnail of the file (any app's), or None
pub fn cached_thumb(path: &str, mtime: i64) -> Option<String> {
    let md5 = md5_hex(crate::appwin::path_to_uri(path).as_bytes());
    let base = format!("{}/thumbnails", std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", home())));
    for d in ["large", "normal", "x-large"] {
        let p = format!("{base}/{d}/{md5}.png");
        if let Ok(m) = std::fs::metadata(&p) {
            // older than the file: stale
            if m.mtime() + 1 >= mtime {
                return Some(p);
            }
        }
    }
    None
}
// make one for a video with ffmpegthumbnailer into the large cache (spec: the file is named by the URI's md5)
pub fn make_video_thumb(path: &str) -> Option<String> {
    let md5 = md5_hex(crate::appwin::path_to_uri(path).as_bytes());
    let dir = format!("{}/thumbnails/large", std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{}/.cache", home())));
    let _ = std::fs::create_dir_all(&dir);
    let out = format!("{dir}/{md5}.png");
    let ok = std::process::Command::new("ffmpegthumbnailer").args(["-i", path, "-o", &out, "-s", "256", "-t", "15%", "-c", "png"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    ok.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn md5_known() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"file:///home/jens/photos/me.png"), "c6ee772d9e49320e97ec29a7eb5b1697");
    }
    #[test]
    fn names() {
        let n: Vec<String> = ["a.txt", "a - kopia.txt", "Nowy folder"].iter().map(|s| s.to_string()).collect();
        assert_eq!(free_name(&n, "a.txt", true, false), "a - kopia (2).txt");
        assert_eq!(free_name(&n, "Nowy folder", false, true), "Nowy folder (2)");
        assert_eq!(free_name(&n, "b", false, false), "b");
    }
    #[test]
    fn natural() {
        let mut v = vec!["plik10", "plik2", "Plik1", "żaba", "zebra", "ala"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["ala", "Plik1", "plik2", "plik10", "zebra", "żaba"]);
    }
    #[test]
    fn uris() {
        let p = "/home/a b/zażółć#1.txt";
        assert_eq!(crate::appwin::uri_to_path(&crate::appwin::path_to_uri(p)), p);
        assert_eq!(parent("/a"), "/");
        assert_eq!(parent("/a/b"), "/a");
    }
}
