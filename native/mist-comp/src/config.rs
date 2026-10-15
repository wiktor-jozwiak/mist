//! `~/.config/mist/comp.conf`: one directive per line, `#` comments.
//!
//!   bind  SUPER+SHIFT+Return  exec kitty        key press
//!   bindr SUPER+Super_L       exec mist-ui-bin msg mist toggle start   release, only if nothing else was pressed
//!   bindl XF86AudioMute       exec wpctl ...    also while locked
//!   output DP-3 0 0                             (or ~/.config/mist/outputs)
//!   exec  ~/.local/bin/mist-ui                  autostart
//!   env   XCURSOR_SIZE 24
//!   keyboard pl                                 xkb layout[,layout2] [variant] [options]
//!   numlock on                                  Num Lock on at start
//!   vrr fullscreen|off                          adaptive sync only while a fullscreen window is shown
//!   swallow on                                  a window started from a terminal hides it until it closes
//!   middle_click_paste off                      no primary selection (Hyprland misc.middle_click_paste)
//!   pointer flat                                libinput accel profile: flat | adaptive
//!
//! Actions: exec CMD | close | kill | quit | reload | workspace N|+1|-1 | movetoworkspace N|+1|-1
//!          | maximize | fullscreen | float | focus left|right|up|down | cycle | vt N
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
    Vt(i32),
    /// Windows 11 snapping (snap.rs)
    Snap(crate::snap::Key),
    /// move the focused window to the next output (-1 / +1)
    MoveToMonitor(i32),
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

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub binds: Vec<Bind>,
    pub outputs: Vec<(String, i32, i32)>,
    pub exec: Vec<String>,
    pub env: Vec<(String, String)>,
    /// layout, variant, options
    pub keyboard: (String, String, String),
    pub numlock: bool,
    pub middle_click_paste: bool,
    /// hide the terminal a window was started from (Hyprland misc.enable_swallow)
    pub swallow: bool,
    /// adaptive sync while a fullscreen window is shown (Hyprland misc.vrr = 3)
    pub vrr: bool,
    /// libinput acceleration profile for mice / touchpads (None: the device default)
    pub pointer_flat: Option<bool>,
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
    parse(&text)
}

pub fn parse(text: &str) -> Config {
    let mut c = Config { keyboard: ("us".into(), String::new(), String::new()), middle_click_paste: true, ..Default::default() };
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        let ok = match word {
            "bind" | "bindr" | "bindl" => parse_bind(rest, word == "bindr", word == "bindl").map(|b| c.binds.push(b)).is_some(),
            "output" => {
                let mut it = rest.split_whitespace();
                (|| Some(c.outputs.push((it.next()?.into(), it.next()?.parse().ok()?, it.next()?.parse().ok()?))))().is_some()
            }
            "exec" => {
                c.exec.push(rest.into());
                true
            }
            "env" => rest.split_once(char::is_whitespace).map(|(k, v)| c.env.push((k.into(), v.trim().into()))).is_some(),
            "keyboard" => {
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
pub const DEFAULT: &str = r#"
keyboard pl
numlock on
middle_click_paste off
swallow on
vrr fullscreen
pointer flat
# session environment for every child (Hyprland's environment.lua)
env XDG_CURRENT_DESKTOP mist
env XDG_SESSION_DESKTOP mist
env XDG_SESSION_TYPE wayland
env __GLX_VENDOR_LIBRARY_NAME nvidia
env LIBVA_DRIVER_NAME nvidia
env NVD_BACKEND direct
env MOZ_DISABLE_RDD_SANDBOX 1
env __GL_GSYNC_ALLOWED 1
env QT_QPA_PLATFORMTHEME qt6ct
env QT_PLUGIN_PATH ~/.local/opt/kvantum/usr/lib/qt6/plugins
env QT_QPA_PLATFORM wayland;xcb
env MOZ_ENABLE_WAYLAND 1
# autostart (real session only): portals / systemd services see the display, then the shell
exec dbus-update-activation-environment --systemd WAYLAND_DISPLAY DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE HYPRLAND_INSTANCE_SIGNATURE
exec ~/.local/bin/mist-ui
# root GUI apps (pkexec gparted...) on XWayland, like Hyprland autostart.lua
exec sh -c '[ -n "$DISPLAY" ] && xhost +SI:localuser:root'
bind  SUPER+Return       exec kitty
bind  SUPER+Q            close
bind  ALT+F4             close
bind  SUPER+Escape       kill
bind  SUPER+Up           snap up
bind  SUPER+Down         snap down
bind  SUPER+SHIFT+Up     snap stretch
bind  SUPER+SHIFT+Down   snap unsnap
bind  SUPER+Z            snap layouts
bind  SUPER+SHIFT+Left   movetomonitor -1
bind  SUPER+SHIFT+Right  movetomonitor 1
bind  SUPER+ALT+Left     focus left
bind  SUPER+ALT+Right    focus right
bind  SUPER+F            fullscreen
bind  SUPER+ALT+Space    float
bind  SUPER+Left         snap left
bind  SUPER+Right        snap right
bind  ALT+Tab            cycle
bind  SUPER+1            workspace 1
bind  SUPER+2            workspace 2
bind  SUPER+3            workspace 3
bind  SUPER+4            workspace 4
bind  SUPER+5            workspace 5
bind  SUPER+6            workspace 6
bind  SUPER+SHIFT+1      movetoworkspace 1
bind  SUPER+SHIFT+2      movetoworkspace 2
bind  SUPER+SHIFT+3      movetoworkspace 3
bind  SUPER+SHIFT+4      movetoworkspace 4
bind  SUPER+SHIFT+5      movetoworkspace 5
bind  SUPER+SHIFT+6      movetoworkspace 6
bind  SUPER+CTRL+Right   workspace +1
bind  SUPER+CTRL+Left    workspace -1
bind  SUPER+CTRL+SHIFT+Right movetoworkspace +1
bind  SUPER+CTRL+SHIFT+Left  movetoworkspace -1
bind  SUPER+Tab          exec mist-ui-bin msg switcher next
bind  SUPER+SHIFT+Tab    exec mist-ui-bin msg switcher prev
bind  SUPER+Space        exec mist-ui-bin msg mist toggle spotlight
bind  SUPER+S            exec mist-ui-bin msg mist toggle spotlight
bindr SUPER+Super_L      exec mist-ui-bin msg mist toggle start
bind  SUPER+X            exec mist-ui-bin msg mist toggle control
bind  SUPER+I            exec mist-ui-bin msg mist toggle control
bind  SUPER+R            exec mist-ui-bin msg mist toggle spotlight
bind  SUPER+Y            exec mist-ui-bin msg mist toggle phone
bind  SUPER+period       exec mist-ui-bin msg mist toggle emoji
bind  SUPER+V            exec mist-ui-bin msg mist toggle clipboard
bind  SUPER+A            exec mist-ui-bin msg mist toggle notifications
bind  SUPER+SHIFT+W      exec mist-ui-bin msg mist toggle wallpaper
bind  SUPER+ALT+C        exec mist-ui-bin msg mist toggle session
bind  SUPER+L            exec mist-ui-bin lock
bind  SUPER+E            exec mist-files
bind  SUPER+T            exec gnome-text-editor --new-window
bind  SUPER+C            exec gnome-calculator
bind  XF86Calculator     exec gnome-calculator
bind  SUPER+P            exec hyprpicker -a -n
bindl XF86MonBrightnessUp   exec ~/.config/quickshell/mist/scripts/bright.sh up
bindl XF86MonBrightnessDown exec ~/.config/quickshell/mist/scripts/bright.sh down
bind  SUPER+D            workspace empty
bind  SUPER+CTRL+Down    workspace empty
bind  SUPER+ALT+Up       focus up
bind  SUPER+ALT+Down     focus down
bind  SUPER+mouse_down   workspace m-1
bind  SUPER+mouse_up     workspace m+1
bind  SUPER+CTRL+mouse_up   workspace m-1
bind  SUPER+CTRL+mouse_down workspace m+1
bind  SUPER+SHIFT+mouse_up   movetomonitor -1
bind  SUPER+SHIFT+mouse_down movetomonitor 1
bind  SUPER+CTRL+SHIFT+mouse_up   movetoworkspace m-1
bind  SUPER+CTRL+SHIFT+mouse_down movetoworkspace m+1
bind  SUPER+ALT+1        workspace 1
bind  SUPER+CTRL+1       workspace m~1
bind  SUPER+SHIFT+CTRL+1 movetoworkspace m~1
bind  SUPER+SHIFT+ALT+1  movetoworkspacesilent m~1
bind  SUPER+ALT+2        workspace 2
bind  SUPER+CTRL+2       workspace m~2
bind  SUPER+SHIFT+CTRL+2 movetoworkspace m~2
bind  SUPER+SHIFT+ALT+2  movetoworkspacesilent m~2
bind  SUPER+ALT+3        workspace 3
bind  SUPER+CTRL+3       workspace m~3
bind  SUPER+SHIFT+CTRL+3 movetoworkspace m~3
bind  SUPER+SHIFT+ALT+3  movetoworkspacesilent m~3
bind  SUPER+grave        togglespecial
bind  SUPER+SHIFT+grave  movetoworkspacesilent special
bind  SUPER+W            exec zen-browser
bind  CTRL+SHIFT+Escape  exec kitty -e btop
bind  Print              exec ~/.config/quickshell/mist/scripts/screenshot.sh region
bind  SUPER+SHIFT+S      exec ~/.config/quickshell/mist/scripts/screenshot.sh region
bind  SUPER+Print        exec ~/.config/quickshell/mist/scripts/screenshot.sh full
bindl XF86AudioRaiseVolume exec wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+
bindl XF86AudioLowerVolume exec wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-
bindl XF86AudioMute      exec wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle
bindl XF86AudioMicMute   exec wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle
bindl XF86AudioPlay      exec mist-ui-bin msg mist media toggle
bindl XF86AudioPause     exec mist-ui-bin msg mist media toggle
bindl XF86AudioNext      exec mist-ui-bin msg mist media next
bindl XF86AudioPrev      exec mist-ui-bin msg mist media prev
bind  CTRL+ALT+BackSpace quit
"#;

#[cfg(test)]
mod tests {
    use super::*;

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
