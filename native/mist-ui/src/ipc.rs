// IPC of mist-ui: a unix socket ($XDG_RUNTIME_DIR/mist/ui.sock), one request per connection:
//   "<target>\t<function>\t<arg>\t<arg>...\n"  ->  one reply line
// `mist-ui-bin msg <target> <function> [args...]` is the client (replaces `mist-qs ... ipc call` for the
// parts that are native). Requests reach the main loop as Msg::Ipc with a reply channel.
use crate::sys::Msg;
use calloop::channel::Sender;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};

pub fn path() -> String {
    if let Ok(p) = std::env::var("MIST_UI_SOCK") {
        return p;
    }
    format!("{}/mist/ui.sock", std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into()))
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
            let mut line = String::new();
            if BufReader::new(&s).read_line(&mut line).is_err() {
                continue;
            }
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
