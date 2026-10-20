//! `mist-comp migrate [hyprland.lua]`: the parts of a Hyprland Lua config mist-comp understands, as
//! comp.conf lines on stdout. Binds (`hl.bind`: exec_cmd and the common dispatchers, release / locked),
//! monitors (`hl.monitor`), environment (`hl.env`) and autostart (`hl.exec_cmd` outside a bind). Locals
//! made of string literals, `os.getenv(...)` and `..` are followed; what is not understood becomes a
//! `# not converted:` comment, never a guess.
use std::collections::HashMap;

pub fn run(args: &[String]) -> i32 {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = args.first().cloned().unwrap_or_else(|| format!("{home}/.config/hypr/hyprland.lua"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("migrate: cannot read {path}");
        return 1;
    };
    print!("{}", convert(&text, &home));
    0
}

/// The comp.conf text for a Hyprland Lua config.
pub fn convert(text: &str, home: &str) -> String {
    let mut vars: HashMap<String, String> = HashMap::new();
    vars.insert("home".into(), home.into());
    let mut out = String::from("# mist-comp comp.conf from a Hyprland config (mist-comp migrate)\n");
    // locals first, in order (later ones may use earlier ones)
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("local ") {
            if let Some((name, expr)) = rest.split_once('=') {
                let name = name.trim();
                if name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    if let Some(v) = eval(expr.trim(), &vars) {
                        vars.insert(name.into(), v);
                    }
                }
            }
        }
    }
    let mut i = 0;
    while let Some(off) = text[i..].find("hl.") {
        let at = i + off;
        let rest = &text[at..];
        // skip comments: the `hl.` sits after `--` on its line
        let line_start = text[..at].rfind('\n').map(|n| n + 1).unwrap_or(0);
        if text[line_start..at].contains("--") {
            i = at + 3;
            continue;
        }
        let call = ["hl.bind(", "hl.monitor(", "hl.env(", "hl.exec_cmd("].into_iter().find(|c| rest.starts_with(c));
        let Some(call) = call else {
            i = at + 3;
            continue;
        };
        let open = at + call.len() - 1;
        let Some(close) = matching(text, open) else { break };
        let inner = &text[open + 1..close];
        let args = split_args(inner);
        let line = match call {
            "hl.bind(" => bind(&args, &vars),
            "hl.monitor(" => monitor(inner),
            "hl.env(" => match (args.first().and_then(|a| eval(a, &vars)), args.get(1).and_then(|a| eval(a, &vars))) {
                (Some(k), Some(v)) => Ok(format!("env {k} {v}")),
                _ => Err(()),
            },
            _ => args.first().and_then(|a| eval(a, &vars)).map(|c| format!("exec {c}")).ok_or(()),
        };
        match line {
            Ok(l) => out += &format!("{l}\n"),
            Err(()) => out += &format!("# not converted: {}\n", text[at..=close].split_whitespace().collect::<Vec<_>>().join(" ")),
        }
        i = close + 1;
    }
    out
}

/// A string expression: literals, known locals, os.getenv("X"), joined with `..`.
fn eval(expr: &str, vars: &HashMap<String, String>) -> Option<String> {
    let mut out = String::new();
    for part in split_top(expr, "..") {
        let p = part.trim();
        if let Some(s) = literal(p) {
            out += &s;
        } else if let Some(name) = p.strip_prefix("os.getenv(").and_then(|r| r.strip_suffix(')')) {
            let name = literal(name.trim())?;
            // HOME: the one migrate was given (the config may be converted for another account)
            out += &if name == "HOME" { vars.get("home").cloned() } else { std::env::var(&name).ok() }.unwrap_or_default();
        } else if let Some(v) = vars.get(p) {
            out += v;
        } else {
            return None;
        }
    }
    Some(out)
}

fn literal(p: &str) -> Option<String> {
    let q = p.chars().next()?;
    if (q != '"' && q != '\'') || p.len() < 2 || !p.ends_with(q) {
        return None;
    }
    let body = &p[1..p.len() - 1];
    let mut s = String::new();
    let mut it = body.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next()? {
                'n' => s.push('\n'),
                't' => s.push('\t'),
                o => s.push(o),
            }
        } else if c == q {
            return None; // two literals without `..`
        } else {
            s.push(c);
        }
    }
    Some(s)
}

/// The index of the bracket closing the one at `open` (quotes respected).
fn matching(text: &str, open: usize) -> Option<usize> {
    let b = text.as_bytes();
    let (mut depth, mut quote, mut i) = (0i32, 0u8, open);
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == b'\\' {
                i += 1;
            } else if c == quote {
                quote = 0;
            }
        } else {
            match c {
                b'"' | b'\'' => quote = c,
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Split at `sep` outside quotes and brackets.
fn split_top<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let b = s.as_bytes();
    let (mut depth, mut quote, mut start, mut i) = (0i32, 0u8, 0usize, 0usize);
    let mut parts = Vec::new();
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == b'\\' {
                i += 1;
            } else if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if matches!(c, b'(' | b'{' | b'[') {
            depth += 1;
        } else if matches!(c, b')' | b'}' | b']') {
            depth -= 1;
        } else if depth == 0 && s[i..].starts_with(sep) {
            parts.push(&s[start..i]);
            i += sep.len();
            start = i;
            continue;
        }
        i += 1;
    }
    parts.push(&s[start..]);
    parts
}

fn split_args(s: &str) -> Vec<&str> {
    split_top(s, ",").into_iter().map(str::trim).filter(|a| !a.is_empty()).collect()
}

/// `key = value` in a Lua table text (strings unquoted).
fn field<'a>(table: &'a str, key: &str) -> Option<&'a str> {
    let inner = table.trim().strip_prefix('{')?.strip_suffix('}')?;
    split_top(inner, ",").into_iter().find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches(['"', '\'']))
    })
}

fn bind(args: &[&str], vars: &HashMap<String, String>) -> Result<String, ()> {
    let combo = eval(args.first().ok_or(())?, vars).ok_or(())?;
    let combo: String = combo.split('+').map(|k| k.trim()).map(|k| if k.eq_ignore_ascii_case("super_l") { "Super_L" } else { k }).collect::<Vec<_>>().join("+");
    let d = args.get(1).ok_or(())?.trim();
    let action = if let Some(inner) = d.strip_prefix("hl.dsp.exec_cmd(").and_then(|r| r.strip_suffix(')')) {
        format!("exec {}", eval(inner, vars).ok_or(())?)
    } else if d.starts_with("hl.dsp.window.close(") {
        "close".into()
    } else if d.starts_with("hl.dsp.window.kill(") {
        "kill".into()
    } else if d.starts_with("hl.dsp.window.fullscreen(") {
        "fullscreen".into()
    } else if d.starts_with("hl.dsp.window.float(") || d.starts_with("hl.dsp.window.toggle_floating(") {
        "float".into()
    } else if let Some(t) = d.strip_prefix("hl.dsp.focus(").and_then(|r| r.strip_suffix(')')) {
        if let Some(ws) = field(t, "workspace") {
            format!("workspace {ws}")
        } else if let Some(dir) = field(t, "direction") {
            format!("focus {}", match dir {
                "l" | "left" => "left",
                "r" | "right" => "right",
                "u" | "up" => "up",
                "d" | "down" => "down",
                _ => return Err(()),
            })
        } else {
            return Err(());
        }
    } else if let Some(t) = d.strip_prefix("hl.dsp.window.move(").and_then(|r| r.strip_suffix(')')) {
        let ws = field(t, "workspace").ok_or(())?;
        if field(t, "follow") == Some("false") { format!("movetoworkspacesilent {ws}") } else { format!("movetoworkspace {ws}") }
    } else {
        return Err(());
    };
    let opts = args.get(2).copied().unwrap_or("");
    let kind = if field(opts, "release") == Some("true") {
        "bindr"
    } else if field(opts, "locked") == Some("true") {
        "bindl"
    } else {
        "bind"
    };
    // the action must be one comp.conf takes
    crate::config::parse_action(&action).ok_or(())?;
    Ok(format!("{kind:<5} {combo:<24} {action}"))
}

/// hl.monitor({ output = "DP-3", mode = "2560x1440@165", position = "0x0", scale = 1.25, transform = 1 })
fn monitor(table: &str) -> Result<String, ()> {
    let name = field(table, "output").ok_or(())?;
    let pos = field(table, "position").unwrap_or("0x0");
    let (x, y) = pos.split_once('x').ok_or(())?;
    let mut line = format!("output {name} {} {}", x.trim().parse::<i32>().map_err(|_| ())?, y.trim().parse::<i32>().map_err(|_| ())?);
    if let Some(m) = field(table, "mode").filter(|m| !matches!(*m, "preferred" | "highrr" | "highres")) {
        line += &format!(" mode {m}");
    }
    if let Some(s) = field(table, "scale").filter(|s| *s != "auto") {
        line += &format!(" scale {}", s.parse::<f64>().map_err(|_| ())?);
    }
    if let Some(t) = field(table, "transform") {
        let t = match t {
            "0" => "normal",
            "1" => "90",
            "2" => "180",
            "3" => "270",
            "4" => "flipped",
            "5" => "flipped-90",
            "6" => "flipped-180",
            "7" => "flipped-270",
            _ => return Err(()),
        };
        line += &format!(" transform {t}");
    }
    crate::config::parse_output(line.trim_start_matches("output ")).ok_or(())?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hyprland_example() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hyprland/mist.lua")).unwrap();
        let out = super::convert(&text, "/home/u");
        assert!(out.contains("bind  SUPER+Space"), "{out}");
        assert!(out.contains("exec /home/u/.local/bin/mist-ui-bin msg mist toggle spotlight"), "{out}");
        assert!(out.contains("bindr SUPER+Super_L"), "{out}");
        assert!(out.contains("bindl XF86AudioPlay"), "{out}");
        assert!(out.contains("exec /home/u/.local/bin/mist-ui"), "autostart: {out}");
        // everything comp.conf has a reader for parses back
        let c = crate::config::parse(&out);
        assert!(c.binds.len() >= 15, "{} binds", c.binds.len());
    }

    #[test]
    fn dispatchers_and_monitors() {
        let lua = r#"
local mainMod = "SUPER"
hl.bind(mainMod .. " + Q", hl.dsp.window.close())
hl.bind(mainMod .. " + 2", hl.dsp.focus({ workspace = 2 }))
hl.bind(mainMod .. " + SHIFT + 2", hl.dsp.window.move({ workspace = 2, follow = false }))
hl.bind(mainMod .. " + H", hl.dsp.focus({ direction = "l" }))
hl.bind(mainMod .. " + K", hl.dsp.something_else())
-- hl.bind("SUPER + X", hl.dsp.window.close())
hl.monitor({ output = "DP-3", mode = "2560x1440@165", position = "1920x0", scale = 1.25 })
hl.env("XCURSOR_SIZE", "24")
"#;
        let out = super::convert(lua, "/h");
        assert!(out.contains("bind  SUPER+Q                  close"), "{out}");
        assert!(out.contains("workspace 2"), "{out}");
        assert!(out.contains("movetoworkspacesilent 2"), "{out}");
        assert!(out.contains("focus left"), "{out}");
        assert!(out.contains("# not converted: hl.bind(mainMod .. \" + K\""), "{out}");
        assert!(!out.contains("SUPER+X"), "commented out: {out}");
        assert!(out.contains("output DP-3 1920 0 mode 2560x1440@165 scale 1.25"), "{out}");
        assert!(out.contains("env XCURSOR_SIZE 24"), "{out}");
    }
}
