//! `~/.config/mist/comp.conf`: one directive per line, `#` comments.
//!
//!   bind  SUPER+SHIFT+Return  exec kitty        key press
//!   bindr SUPER+Super_L       exec mist-ui-bin msg mist toggle start   release, only if nothing else was pressed
//!   bindl XF86AudioMute       exec wpctl ...    also while locked
//!   output DP-3 0 0 [scale 1.25] [mode 2560x1440@165] [transform 90]   position (also ~/.config/mist/outputs,
//!                                               where comp.conf wins); transform normal|90|180|270|flipped[-90…]
//!   exec  ~/.local/bin/mist-ui                  autostart
//!   env   XCURSOR_SIZE 24
//!   keyboard pl                                 xkb layout[,layout2] [variant] [options]
//!   numlock on                                  Num Lock on at start
//!   vrr fullscreen|off                          adaptive sync only while a fullscreen window is shown
//!   swallow on                                  a window started from a terminal hides it until it closes
//!   middle_click_paste off                      no primary selection (Hyprland misc.middle_click_paste)
//!   pointer flat                                libinput accel profile: flat | adaptive
//!   explicit_sync off                           no linux-drm-syncobj (default: on where the GPU supports it)
//!   screencopy_dmabuf on                        screen casting straight into the client's GPU buffer
//!   swipe 3|4|off                               touchpad: N-finger horizontal swipe switches workspaces (default 3)
//!   workspaces vertical                         workspaces stacked top to bottom (niri): the slide and the
//!                                               swipe go up / down (default horizontal)
//!   fifo on                                     wp-fifo for Vulkan FIFO presentation (games); opt-in
//!   direct_scanout game|on|off                  a client's buffer straight to the screen (Hyprland
//!                                               render:direct_scanout): game (default) = a fullscreen game's, on
//!                                               the primary plane only; on = any window, overlay planes too
//!                                               (on NVIDIA that showed torn or stale windows); off = all composited
//!
//! Actions: exec CMD | close | kill | quit | reload | workspace N|+1|-1 | movetoworkspace N|+1|-1
//!          | maximize | fullscreen | float | focus left|right|up|down | cycle | overview | zoom in|out|off | vt N
//!          | global APP_ID:ID   (a shortcut registered through the portal, e.g. Discord push-to-talk)
use smithay::input::keyboard::{keysyms, xkb, ModifiersState};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Exec(String),
    Close,
    Kill,
    Quit,
    Reload,
    Workspace(WsArg),
    /// the window goes there and the view follows it (Hyprland movetoworkspace)
    MoveToWorkspace(WsArg),
    MoveToWorkspaceSilent(WsArg),
    /// the scratchpad workspace over the current one (Hyprland special workspace)
    ToggleSpecial,
    Maximize,
    Fullscreen,
    Float,
    Focus(Dir),
    Cycle,
    /// Win+D: minimize every shown window, again on the empty desktop: bring them back (Windows)
    ShowDesktop,
    /// Mission Control: the shown workspace's windows side by side (overview.rs)
    Overview,
    /// a key the open overview takes (keysym, Shift held): input.rs only, no bind
    OverviewKey(u32, bool),
    /// the magnifier: +1 in, -1 out, 0 off
    Zoom(i32),
    Vt(i32),
    /// Windows 11 snapping (snap.rs)
    Snap(crate::snap::Key),
    /// move the focused window to the next output (-1 / +1)
    MoveToMonitor(i32),
    /// a shortcut an app registered through the portal (`global APP_ID:ID`, or `:ID` for any app):
    /// pressed on the key press, released with the key
    Global(String),
    /// the key of a held `global` bind went up
    GlobalUp(String),
    /// swallow a key (the release of an intercepted press)
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WsArg {
    Id(u32),
    Rel(i32),
    /// the first empty one on this monitor (Hyprland emptym)
    Empty,
    /// the Nth of this monitor's workspaces (Hyprland m~N)
    Local(u32),
    Special,
}

/// Pseudo keysyms for the scroll wheel in binds (`SUPER+mouse_down`).
pub const MOUSE_UP: u32 = 0x1000_0001;
pub const MOUSE_DOWN: u32 = 0x1000_0002;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl From<&ModifiersState> for Mods {
    fn from(m: &ModifiersState) -> Self {
        Mods { ctrl: m.ctrl, alt: m.alt, shift: m.shift, logo: m.logo }
    }
}

#[derive(Clone, Debug)]
pub struct Bind {
    pub mods: Mods,
    /// lower-case keysym (letters compare case-insensitively)
    pub sym: u32,
    pub release: bool,
    pub locked: bool,
    pub action: Action,
}

/// A monitor's setup: position, and optionally scale, mode (size, refresh in mHz) and transform.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputConf {
    pub name: String,
    pub pos: (i32, i32),
    pub scale: Option<f64>,
    pub mode: Option<(i32, i32, Option<i32>)>,
    pub transform: Option<smithay::utils::Transform>,
}

/// `NAME X Y [scale S] [mode WxH[@HZ]] [transform T]`
pub fn parse_output(rest: &str) -> Option<OutputConf> {
    use smithay::utils::Transform;
    let mut it = rest.split_whitespace();
    let mut o = OutputConf { name: it.next()?.into(), pos: (it.next()?.parse().ok()?, it.next()?.parse().ok()?), ..Default::default() };
    while let Some(k) = it.next() {
        let v = it.next()?;
        match k {
            "scale" => o.scale = Some(v.parse().ok().filter(|s: &f64| (0.25..=8.0).contains(s))?),
            "mode" => {
                let (size, hz) = v.split_once('@').map(|(a, b)| (a, Some(b))).unwrap_or((v, None));
                let (w, h) = size.split_once('x')?;
                let hz = hz.and_then(|h| h.parse::<f64>().ok()).map(|h| (h * 1000.0).round() as i32);
                o.mode = Some((w.parse().ok()?, h.parse().ok()?, hz));
            }
            "transform" => {
                o.transform = Some(match v {
                    "normal" | "0" => Transform::Normal,
                    "90" => Transform::_90,
                    "180" => Transform::_180,
                    "270" => Transform::_270,
                    "flipped" => Transform::Flipped,
                    "flipped-90" => Transform::Flipped90,
                    "flipped-180" => Transform::Flipped180,
                    "flipped-270" => Transform::Flipped270,
                    _ => return None,
                })
            }
            _ => return None,
        }
    }
    Some(o)
}

/// Every configured monitor: comp.conf's `output` lines, then ~/.config/mist/outputs for the others.
pub fn outputs(c: &Config) -> Vec<OutputConf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut all = c.outputs.clone();
    for o in std::fs::read_to_string(format!("{home}/.config/mist/outputs")).unwrap_or_default().lines().filter_map(parse_output) {
        if !all.iter().any(|a| a.name == o.name) {
            all.push(o);
        }
    }
    all
}

/// What may skip composition and go to a plane as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scanout {
    Off,
    /// a fullscreen game (rules.rs `game`), primary plane only (Hyprland direct_scanout = 2)
    #[default]
    Game,
    /// any client buffer, any plane (Hyprland direct_scanout = 1)
    On,
}

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub binds: Vec<Bind>,
    pub outputs: Vec<OutputConf>,
    pub exec: Vec<String>,
    pub env: Vec<(String, String)>,
    /// layout, variant, options
    pub keyboard: (String, String, String),
    /// a `keyboard` line was given
    pub keyboard_set: bool,
    pub numlock: bool,
    pub middle_click_paste: bool,
    /// hide the terminal a window was started from (Hyprland misc.enable_swallow)
    pub swallow: bool,
    /// adaptive sync while a fullscreen window is shown (Hyprland misc.vrr = 3)
    pub vrr: bool,
    /// libinput acceleration profile for mice / touchpads (None: the device default)
    pub pointer_flat: Option<bool>,
    /// linux-drm-syncobj when the GPU supports it (Hyprland render:explicit_sync auto); `explicit_sync off`
    pub no_explicit_sync: bool,
    /// screen casting into the client's dmabuf on the GPU (`screencopy_dmabuf on`), else shm read back
    pub screencopy_dmabuf: bool,
    /// fingers of the horizontal touchpad swipe that switches workspaces (`swipe 3|4|off`, 0 = off)
    pub swipe_fingers: u32,
    /// workspaces above / below each other (`workspaces vertical`): slides and swipes run up / down
    pub ws_vertical: bool,
    /// wp-fifo for Vulkan FIFO clients (`fifo on`; off until tested on each driver)
    pub fifo: bool,
    /// client buffers straight to a scanout plane: `direct_scanout game` (default), `on`, `off`
    pub direct_scanout: Scanout,
}

pub fn path() -> std::path::PathBuf {
    // the greeter runs mist-comp with its own config (no binds, no autostart)
    if let Some(p) = std::env::var_os("MIST_COMP_CONFIG") {
        return p.into();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.config/mist/comp.conf").into()
}

pub fn load() -> Config {
    let text = std::fs::read_to_string(path()).unwrap_or_else(|_| DEFAULT.to_string());
    let mut c = parse(&text);
    // the session's own comp.conf: what it leaves out comes from the defaults, so a file with only monitors
    // (or only a few binds) is not a session without shortcuts, shell or keyboard layout; and there is always
    // a way out. The greeter (MIST_COMP_CONFIG) keeps exactly its file: no binds, no autostart.
    if std::env::var_os("MIST_COMP_CONFIG").is_none() {
        fill_defaults(&mut c, &parse(DEFAULT));
    }
    // safe mode (start-mist after a crash): none of the features that depend most on the driver
    if safe_mode() {
        c.vrr = false;
        c.no_explicit_sync = true;
        c.screencopy_dmabuf = false;
        c.fifo = false;
        c.direct_scanout = Scanout::Off;
    }
    c
}

/// Sections a user config does not have at all take the defaults' (binds, autostart, environment, keyboard);
/// Ctrl+Alt+BackSpace quits unless something else is bound to quit.
pub fn fill_defaults(c: &mut Config, d: &Config) {
    if c.binds.is_empty() {
        tracing::warn!("comp.conf has no bind lines: using the default key bindings");
        c.binds = d.binds.clone();
    }
    if c.exec.is_empty() {
        tracing::warn!("comp.conf has no exec lines: using the default autostart (the Mist shell)");
        c.exec = d.exec.clone();
    }
    if c.env.is_empty() {
        c.env = d.env.clone();
    }
    if !c.keyboard_set {
        c.keyboard = d.keyboard.clone();
    }
    if !c.binds.iter().any(|b| b.action == Action::Quit) {
        if let Some(b) = parse_bind("CTRL+ALT+BackSpace quit", false, true) {
            c.binds.push(b);
        }
    }
}

/// MIST_COMP_SAFE=1: start-mist restarts a crashed compositor this way (also no blur, no animations).
pub fn safe_mode() -> bool {
    std::env::var("MIST_COMP_SAFE").is_ok_and(|v| v == "1")
}

pub fn parse(text: &str) -> Config {
    let mut c = Config { keyboard: ("us".into(), String::new(), String::new()), middle_click_paste: true, swipe_fingers: 3, ..Default::default() };
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        let ok = match word {
            "bind" | "bindr" | "bindl" => parse_bind(rest, word == "bindr", word == "bindl").map(|b| c.binds.push(b)).is_some(),
            "output" => parse_output(rest).map(|o| c.outputs.push(o)).is_some(),
            "exec" => {
                c.exec.push(rest.into());
                true
            }
            "env" => rest.split_once(char::is_whitespace).map(|(k, v)| c.env.push((k.into(), v.trim().into()))).is_some(),
            "keyboard" => {
                c.keyboard_set = true;
                let mut it = rest.split_whitespace();
                c.keyboard = (
                    it.next().unwrap_or("us").into(),
                    it.next().unwrap_or("").into(),
                    it.next().unwrap_or("").into(),
                );
                true
            }
            "vrr" => {
                c.vrr = rest == "fullscreen";
                true
            }
            "explicit_sync" => {
                c.no_explicit_sync = rest == "off";
                true
            }
            "fifo" => {
                c.fifo = rest == "on";
                true
            }
            "direct_scanout" => {
                c.direct_scanout = match rest {
                    "on" => Scanout::On,
                    "off" => Scanout::Off,
                    _ => Scanout::Game,
                };
                true
            }
            "swipe" => {
                c.swipe_fingers = if rest == "off" { 0 } else { rest.parse().unwrap_or(3) };
                true
            }
            "workspaces" => {
                c.ws_vertical = rest == "vertical";
                true
            }
            "screencopy_dmabuf" => {
                c.screencopy_dmabuf = rest == "on";
                true
            }
            "swallow" => {
                c.swallow = rest == "on";
                true
            }
            "middle_click_paste" => {
                c.middle_click_paste = rest != "off";
                true
            }
            "numlock" => {
                c.numlock = rest == "on";
                matches!(rest, "on" | "off")
            }
            "pointer" => {
                c.pointer_flat = match rest {
                    "flat" => Some(true),
                    "adaptive" => Some(false),
                    _ => None,
                };
                c.pointer_flat.is_some()
            }
            _ => false,
        };
        if !ok {
            tracing::warn!("comp.conf:{}: cannot parse {line:?}", n + 1);
        }
    }
    c
}

/// A key combination as comp.conf writes it (SUPER+SHIFT+B, XF86AudioMute): what parse_bind reads back.
pub fn combo_name(m: Mods, sym: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (on, n) in [(m.logo, "SUPER"), (m.ctrl, "CTRL"), (m.alt, "ALT"), (m.shift, "SHIFT")] {
        if on {
            parts.push(n.into());
        }
    }
    let name = xkb::keysym_get_name(xkb::Keysym::new(lower(sym)));
    parts.push(if name.chars().count() == 1 { name.to_uppercase() } else { name });
    parts.join("+")
}

fn parse_bind(rest: &str, release: bool, locked: bool) -> Option<Bind> {
    let (combo, action) = rest.split_once(char::is_whitespace)?;
    let mut mods = Mods::default();
    let mut sym = None;
    for part in combo.split('+') {
        match part.to_ascii_uppercase().as_str() {
            "SUPER" | "LOGO" | "MOD4" => mods.logo = true,
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "ALT" | "MOD1" => mods.alt = true,
            "SHIFT" => mods.shift = true,
            "MOUSE_UP" => sym = Some(MOUSE_UP),
            "MOUSE_DOWN" => sym = Some(MOUSE_DOWN),
            _ => {
                let s = xkb::keysym_from_name(part, xkb::KEYSYM_CASE_INSENSITIVE).raw();
                if s == keysyms::KEY_NoSymbol {
                    return None;
                }
                sym = Some(lower(s));
            }
        }
    }
    Some(Bind { mods, sym: sym?, release, locked, action: parse_action(action.trim())? })
}

pub fn parse_action(s: &str) -> Option<Action> {
    let (word, arg) = s.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((s, ""));
    let ws = |a: &str| -> Option<WsArg> {
        let a = a.strip_prefix("m").filter(|r| r.starts_with('+') || r.starts_with('-') || r.starts_with('~')).unwrap_or(a);
        if let Some(n) = a.strip_prefix('~') {
            return n.parse().ok().filter(|n| *n > 0).map(WsArg::Local);
        }
        match a {
            "empty" | "emptym" => return Some(WsArg::Empty),
            "special" => return Some(WsArg::Special),
            _ => {}
        }
        if a.starts_with('+') || a.starts_with('-') {
            a.parse().ok().map(WsArg::Rel)
        } else {
            a.parse().ok().filter(|n| *n > 0).map(WsArg::Id)
        }
    };
    Some(match word {
        "exec" if !arg.is_empty() => Action::Exec(arg.into()),
        "close" => Action::Close,
        "kill" => Action::Kill,
        "quit" => Action::Quit,
        "reload" => Action::Reload,
        "workspace" => Action::Workspace(ws(arg)?),
        "movetoworkspace" => Action::MoveToWorkspace(ws(arg)?),
        "movetoworkspacesilent" => Action::MoveToWorkspaceSilent(ws(arg)?),
        "togglespecial" => Action::ToggleSpecial,
        "maximize" => Action::Maximize,
        "fullscreen" => Action::Fullscreen,
        "float" => Action::Float,
        "cycle" => Action::Cycle,
        "showdesktop" => Action::ShowDesktop,
        "overview" => Action::Overview,
        "zoom" => Action::Zoom(match arg {
            "in" => 1,
            "out" => -1,
            "off" => 0,
            _ => return None,
        }),
        "vt" => Action::Vt(arg.parse().ok()?),
        "snap" => Action::Snap(match arg {
            "left" => crate::snap::Key::Left,
            "right" => crate::snap::Key::Right,
            "up" => crate::snap::Key::Up,
            "down" => crate::snap::Key::Down,
            "stretch" => crate::snap::Key::Stretch,
            "unsnap" => crate::snap::Key::Unsnap,
            "layouts" => crate::snap::Key::Layouts,
            _ => return None,
        }),
        "movetomonitor" => Action::MoveToMonitor(arg.parse().ok()?),
        "global" if !arg.is_empty() => Action::Global(arg.into()),
        "focus" => Action::Focus(match arg {
            "left" => Dir::Left,
            "right" => Dir::Right,
            "up" => Dir::Up,
            "down" => Dir::Down,
            _ => return None,
        }),
        _ => return None,
    })
}

/// Letters: binds are written in either case, matched on the unshifted symbol.
pub fn lower(sym: u32) -> u32 {
    match sym {
        0x41..=0x5a => sym + 0x20,
        0xc0..=0xde if sym != 0xd7 => sym + 0x20,
        _ => sym,
    }
}

impl Config {
    pub fn find(&self, mods: Mods, sym: u32, release: bool) -> Option<&Bind> {
        let sym = lower(sym);
        self.binds.iter().find(|b| b.mods == mods && b.sym == sym && b.release == release)
    }
}

/// Used when ~/.config/mist/comp.conf does not exist (mirrors the Hyprland binds Mist uses).
pub const DEFAULT: &str = include_str!("../../assets/comp-default.conf");

#[cfg(test)]
mod tests {
    #[test]
    fn partial_config_keeps_defaults() {
        let d = super::parse(super::DEFAULT);
        // only monitors: the default binds, autostart, layout, and a way out
        let mut c = super::parse("output DP-3 0 0\n");
        super::fill_defaults(&mut c, &d);
        assert_eq!(c.binds.len(), d.binds.len());
        assert!(c.exec.iter().any(|e| e.contains("mist-ui")));
        assert_eq!(c.keyboard.0, d.keyboard.0);
        assert!(c.binds.iter().any(|b| b.action == super::Action::Quit));
        // own binds stay the only binds, plus the escape
        let mut c = super::parse("bind SUPER+Q close\nkeyboard us\n");
        super::fill_defaults(&mut c, &d);
        assert_eq!(c.binds.len(), 2);
        assert_eq!(c.keyboard.0, "us");
    }

    use super::*;

    #[test]
    fn output_lines() {
        let o = parse_output("DP-3 1920 0 scale 1.25 mode 2560x1440@164.8 transform 90").unwrap();
        assert_eq!((o.name.as_str(), o.pos, o.scale), ("DP-3", (1920, 0), Some(1.25)));
        assert_eq!(o.mode, Some((2560, 1440, Some(164800))));
        assert_eq!(o.transform, Some(smithay::utils::Transform::_90));
        assert_eq!(parse_output("HDMI-A-1 0 0").unwrap().scale, None);
        assert!(parse_output("HDMI-A-1 0 0 scale 99").is_none(), "scale out of range");
        assert!(parse_output("HDMI-A-1 0 0 bogus 1").is_none());
        assert_eq!(parse("output DP-3 0 0 scale 2\n").outputs.len(), 1);
    }

    #[test]
    fn default_parses() {
        let c = parse(DEFAULT);
        assert!(c.binds.len() > 40, "{}", c.binds.len());
        assert_eq!(c.keyboard.0, "pl");
        let b = c.find(Mods { logo: true, ..Default::default() }, keysyms::KEY_Q, false).unwrap();
        assert_eq!(b.action, Action::Close);
        // shifted letter still matches its bind
        let b = c.find(Mods { logo: true, shift: true, ..Default::default() }, keysyms::KEY_W, false).unwrap();
        assert!(matches!(b.action, Action::Exec(ref s) if s.contains("wallpaper")));
        let b = c.find(Mods { logo: true, ..Default::default() }, keysyms::KEY_Super_L, true).unwrap();
        assert!(matches!(b.action, Action::Exec(ref s) if s.contains("start")));
        assert_eq!(c.find(Mods { logo: true, ctrl: true, ..Default::default() }, keysyms::KEY_Left, false).unwrap().action, Action::Workspace(WsArg::Rel(-1)));
    }
}
