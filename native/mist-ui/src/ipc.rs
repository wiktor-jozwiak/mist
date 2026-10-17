// IPC of mist-ui: a unix socket ($XDG_RUNTIME_DIR/mist/ui.sock), one request per connection:
//   "<target>\t<function>\t<arg>\t<arg>...\n"  ->  one reply line
// `mist-ui-bin msg <target> <function> [args...]` is the client (replaces `mist-qs ... ipc call` for the
// parts that are native). Requests reach the main loop as Msg::Ipc with a reply channel.
use crate::sys::Msg;
use calloop::channel::Sender;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

// one request line at most; a client gets this long to send it (the accept loop is single-threaded)
const MAX_LINE: u64 = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(2);

pub fn path() -> String {
    if let Ok(p) = std::env::var("MIST_UI_SOCK") {
        return p;
    }
    // never a shared /tmp: without XDG_RUNTIME_DIR use the per-user runtime dir systemd makes (0700)
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    format!("{dir}/mist/ui.sock")
}

// the request line, or None when the client is too slow, sends garbage or nothing
fn read_request(s: &UnixStream) -> Option<String> {
    let _ = s.set_read_timeout(Some(READ_TIMEOUT));
    let _ = s.set_write_timeout(Some(READ_TIMEOUT));
    let mut line = String::new();
    match BufReader::new(s.take(MAX_LINE)).read_line(&mut line) {
        Ok(n) if n > 0 => Some(line),
        _ => None,
    }
}

pub struct Request {
    pub target: String,
    pub func: String,
    pub args: Vec<String>,
    pub reply: std::sync::mpsc::Sender<String>,
}

pub fn serve(tx: Sender<Msg>) {
    let p = path();
    let _ = std::fs::create_dir_all(std::path::Path::new(&p).parent().unwrap());
    let _ = std::fs::remove_file(&p);
    let Ok(l) = UnixListener::bind(&p) else { eprintln!("ipc: cannot bind {p}"); return };
    let _ = std::thread::Builder::new().name("ipc".into()).stack_size(256 * 1024).spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let Some(line) = read_request(&s) else { continue };
            let mut parts = line.trim_end_matches('\n').split('\t').map(|x| x.to_string());
            let target = parts.next().unwrap_or_default();
            let func = parts.next().unwrap_or_default();
            let args: Vec<String> = parts.collect();
            let (rtx, rrx) = std::sync::mpsc::channel();
            if tx.send(Msg::Ipc(Request { target, func, args, reply: rtx })).is_err() {
                return;
            }
            let r = rrx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or_default();
            let _ = writeln!(s, "{r}");
        }
    });
}

// client: returns the reply (exit status 1 when mist-ui does not answer)
pub fn call(args: &[String]) -> i32 {
    let Ok(mut s) = UnixStream::connect(path()) else {
        eprintln!("mist-ui is not running");
        return 1;
    };
    let _ = writeln!(s, "{}", args.join("\t"));
    let mut r = String::new();
    let _ = BufReader::new(&s).read_line(&mut r);
    let r = r.trim_end();
    if !r.is_empty() {
        println!("{r}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_line() {
        let (mut a, b) = UnixStream::pair().unwrap();
        a.write_all(b"ui\tstats\n").unwrap();
        assert_eq!(read_request(&b).as_deref(), Some("ui\tstats\n"));
    }

    #[test]
    fn oversized_line_is_capped() {
        let (mut a, b) = UnixStream::pair().unwrap();
        let w = std::thread::spawn(move || {
            let _ = a.write_all(&vec![b'x'; 200 * 1024]);
        });
        assert_eq!(read_request(&b).map(|l| l.len()), Some(MAX_LINE as usize));
        drop(b);
        w.join().unwrap();
    }

    #[test]
    fn silent_client_times_out() {
        let (_a, b) = UnixStream::pair().unwrap();
        let t = std::time::Instant::now();
        assert_eq!(read_request(&b), None);
        assert!(t.elapsed() < READ_TIMEOUT + Duration::from_secs(1));
    }
}
