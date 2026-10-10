// Legacy (unused since Mist owns the notification server, Notifs.qml): forwards every desktop notification (eavesdropped as a D-Bus monitor, another daemon stays
// the notification server) as one short tab-separated line:
//   app \t icon \t summary \t body \t urgency \t desktop-entry \t replaces-id \t progress \t sync-key \t category
// (progress = hint "value" 0-100 or -1; sync-key = x-canonical-private-synchronous / x-dunst-stack-tag)
// Big hints (image-data, kilobytes of pixel arrays) are dropped here, so QML never parses huge JSON.
use crate::common::*;
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Value};

type Notify = (String, u32, String, String, String, Vec<String>, HashMap<String, OwnedValue>, i32);

fn s(h: &HashMap<String, OwnedValue>, k: &str) -> String {
    match h.get(k).map(|v| &**v) {
        Some(Value::Str(x)) => x.to_string(),
        _ => String::new(),
    }
}

fn int(h: &HashMap<String, OwnedValue>, k: &str) -> Option<i64> {
    match h.get(k).map(|v| &**v) {
        Some(Value::U8(x)) => Some(*x as i64),
        Some(Value::I32(x)) => Some(*x as i64),
        Some(Value::U32(x)) => Some(*x as i64),
        Some(Value::I64(x)) => Some(*x),
        Some(Value::I16(x)) => Some(*x as i64),
        Some(Value::U16(x)) => Some(*x as i64),
        _ => None,
    }
}

pub fn main() -> zbus::Result<()> {
    let conn = zbus::blocking::Connection::session()?;
    let rule = "type='method_call',interface='org.freedesktop.Notifications',member='Notify'";
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus.Monitoring"),
        "BecomeMonitor",
        &(vec![rule], 0u32),
    )?;
    for m in zbus::blocking::MessageIterator::from(&conn) {
        let Ok(m) = m else { continue };
        let h = m.header();
        if h.member().map(|x| x.as_str()) != Some("Notify") {
            continue;
        }
        let Ok((app, rid, icon, summary, body, _acts, hints, _t)) = m.body().deserialize::<Notify>() else { continue };
        let ip = s(&hints, "image-path");
        let icon = if ip.is_empty() { icon } else { ip };
        let mut sync = s(&hints, "x-canonical-private-synchronous");
        if sync.is_empty() {
            sync = s(&hints, "x-dunst-stack-tag");
        }
        if sync.is_empty() {
            sync = s(&hints, "synchronous");
        }
        out_line(
            &[
                clean(&app, 80),
                clean(&icon, 300),
                clean(&summary, 300),
                clean(&body, 300),
                int(&hints, "urgency").unwrap_or(1).to_string(),
                clean(&s(&hints, "desktop-entry"), 120),
                rid.to_string(),
                int(&hints, "value").unwrap_or(-1).to_string(),
                clean(&sync, 80),
                clean(&s(&hints, "category"), 60),
            ]
            .join("\t"),
        );
    }
    Ok(())
}
