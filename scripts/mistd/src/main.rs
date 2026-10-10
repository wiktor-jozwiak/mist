// mistd: the native backends of the Mist shell (one binary, one mode per process).
//   mistd notify | island | calls | phone      long-running watchers, JSON lines on stdout, commands on stdin
//   mistd data weather|garmin|running|calendar|sys
//   mistd tiles <icon name | path | @glyph> ...  dock tiles -> ~/.cache/mist/tiles
//   mistd notifimg | clipstore | cliplist <dir>
mod calls;
mod common;
mod data;
mod island;
mod notify;
mod phone;
mod tiles;
mod tools;

fn main() {
    common::ignore_sigpipe_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a = |i: usize| args.get(i).map(|s| s.as_str()).unwrap_or("");
    match a(0) {
        "notify" => {
            common::pdeath();
            if let Err(e) = notify::main() {
                eprintln!("notify: {}", e);
                std::process::exit(1);
            }
        }
        "island" => {
            common::pdeath();
            island::main()
        }
        "calls" => {
            common::pdeath();
            calls::main()
        }
        "phone" => {
            common::pdeath();
            phone::main()
        }
        "data" => data::main(a(1)),
        "tiles" => tiles::main(&args[1..]),
        "notifimg" => tools::notifimg(),
        "clipstore" => tools::clipstore(),
        "cliplist" => tools::cliplist(a(1)),
        _ => {
            eprintln!("usage: mistd notify|island|calls|phone|data <src>|tiles <names>|notifimg|clipstore|cliplist <dir>");
            std::process::exit(2);
        }
    }
}
