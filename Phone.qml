pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// The phone (KDE Connect, kdeconnectd is the backend), shared by Start, the phone panel and the Control centre.
// Bridge: native/mist-ui (phone.rs) runs mistd phone + mistd calls and follows the calls; this reads its state
// ($XDG_RUNTIME_DIR/mist/phone-state.json) and sends commands over `mist-ui-bin msg phone ...`.
// The full view is PhonePanel.qml (Ui popup "phone"). Notifications, calls and SMS of the phone reach the Island on their own: kdeconnectd posts them as desktop
// notifications (mistd notify). Media playing on the phone is an MPRIS player (mprisremote plugin),
// clipboard sync is the clipboard plugin: nothing extra here.
Singleton {
    id: root
    property bool ok: false                              // daemon answers
    property var devs: []
    property var main: null                              // {id, name, type, reach, pair, key, plugins, bat, chg, net, sig, notes, nNotes, mount}
    readonly property bool present: !!main
    readonly property bool paired: !!main && main.pair === 3
    readonly property bool online: paired && main.reach
    readonly property bool pairRequested: !!main && main.pair === 2      // the phone asks, accept here
    readonly property bool pairing: !!main && main.pair === 1            // we asked, accept on the phone
    readonly property string name: main ? main.name : ""
    readonly property int battery: online ? main.bat ?? -1 : -1
    readonly property bool charging: online && !!main.chg
    readonly property string network: online ? main.net || "" : ""
    readonly property int strength: online ? main.sig ?? -1 : -1          // 0-4
    readonly property var notes: online ? main.notes || [] : []
    readonly property int noteCount: online ? main.nNotes || 0 : 0
    readonly property string mount: online ? main.mount || "" : ""
    function has(plugin) { return online && (main.plugins || []).indexOf(plugin) >= 0 }

    readonly property string ip: main ? main.ip || "" : ""
    readonly property bool clipAuto: online && main.clipAuto !== false

    // SMS: latest message of every thread (newest first); one open conversation; contacts (vCards)
    property var threads: []                             // {thread, name, addr[], body, date, out, read}
    property var convs: ({})                             // thread -> [msgs]
    property var contacts: []                            // {name, tels[]}
    function contactName(tel) {
        const d = (tel || "").replace(/\D/g, "").slice(-9)
        if (d.length < 9) return ""
        const c = contacts.find(c => c.tels.some(t => t.replace(/\D/g, "").slice(-9) === d))
        return c ? c.name : ""
    }
    function openConv(t) { send("conv", t) }
    function closeConv(t) { send("unconv", t) }
    function sendSms(thread, text) { send("sendsms", { thread: thread, text: text }) }
    function sendSmsTo(to, text) { send("sendsms", { to: to, text: text }) }
    function reply(id, text) { send("reply", { id: id, text: text }) }
    function action(id, a) { send("action", { id: id, action: a }) }
    function media(a) { send("media", a) }                 // Play, Pause, PlayPause, Next, Previous

    // ADB (only with `adb` installed and the phone authorised; asked while the phone panel is open)
    property var adb: ({ ok: false, have: false })
    readonly property bool adbOk: online && !!adb.ok
    function adbInfo() { send("adbinfo") }
    function adbFix() { send("adbfix") }
    function adbWifi() { send("adbwifi") }
    function mirror() { send("mirror") }
    function screenshot() { send("shot") }
    function wake() { send("wake") }
    function sleepScreen() { send("sleep") }
    function setDnd(on) { send("dnd", on ? 1 : 0) }
    function openOnPhone(url) { send("open", url) }
    function installApk(path) { send("apk", path) }

    // {k: conn|disc|pairreq|paired|unpaired|plug|full|photo|shot|call|done|err, name, text, ...} (call: what = ringing|talking|missedCall, who, number)
    signal event(var e)

    function msg(args) { Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/mist-ui-bin", "msg", "phone"].concat(args)) }
    function send(c, a) { msg(["send", JSON.stringify(a === undefined ? { c: c } : { c: c, a: a })]) }
    function ring() { send("ring") }
    function ping() { send("ping") }
    function sendClipboard() { send("clip") }
    function browse() { send("browse") }
    function share(urls) { send("share", urls) }
    function shareText(t) { send("text", t) }
    function photo() { send("photo") }
    function dismiss(id) { send("dismiss", id) }
    function pair() { send("pair") }
    function accept() { send("accept") }
    function reject() { send("reject") }
    function unpair() { send("unpair") }
    // a file picker of KDE Connect itself (no portal / zenity needed)
    function pickFiles() { if (main) Quickshell.execDetached(["kdeconnect-handler", "--device", main.id]) }
    function sms() { Quickshell.execDetached(["kdeconnect-sms"]) }
    function app() { Quickshell.execDetached(["kdeconnect-app"]) }

    function batteryIcon() { return charging ? "charging" : battery >= 0 && battery <= 15 ? "battLow" : "battery" }
    function batteryColor() { return charging ? Theme.green : battery >= 0 && battery <= 15 ? Theme.danger : battery <= 30 ? Theme.yellow : Theme.t1 }

    FileView {
        path: Quickshell.env("XDG_RUNTIME_DIR") + "/mist/phone-state.json"
        watchChanges: true; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            let o
            try { o = JSON.parse(text()) } catch (e) { return }
            root.ok = !!o.ok; root.devs = o.devs || []; root.main = o.main || null
            root.threads = o.threads || []; root.convs = o.convs || ({}); root.contacts = o.contacts || []
            root.adb = o.adb || ({ ok: false, have: false })
            root.hfp = !!o.hfp; root.btCalls = o.btCalls || []
            root.callAudio = o.callAudio || ({ on: false, pc: true, mute: false, err: "" })
            root.kcCall = o.kcCall || null
        }
    }

    // ------------------------------------------------------------ calls
    // Over Bluetooth (the computer is the phone's hands-free kit, mistd calls + PipeWire telephony):
    // answer, reject, hang up, dial, keypad tones, and the call audio through this computer (mute, hand back to
    // the phone). Without the Bluetooth link KDE Connect only reports a ringing call; answering / hanging up
    // then go over ADB (mistd phone: headset key / end call), the audio stays on the phone.
    property bool hfp: false                             // the phone is connected over Bluetooth (audio gateway)
    property var btCalls: []                             // {path, state, number, name, since}
    property var callAudio: ({ on: false, pc: true, mute: false, err: "" })
    property var kcCall: null                            // call reported by KDE Connect / followed over ADB (adb: true)
    readonly property var call: {                        // the call to show: {state, who, number, since, bt, path}
        const order = ["incoming", "waiting", "active", "dialing", "alerting", "held"]
        const c = btCalls.filter(c => order.indexOf(c.state) >= 0).sort((a, b) => order.indexOf(a.state) - order.indexOf(b.state))[0]
        if (c) return Object.assign({ bt: true, who: c.name || contactName(c.number) || c.number || "Nieznany numer" }, c)
        return kcCall
    }
    readonly property bool ringing: !!call && (call.state === "incoming" || call.state === "waiting")
    // KDE Connect: ringing / talking / missedCall, never the end of a call. ADB (mistd phone call_watch): every state
    // change of the phone's calls, incoming and outgoing (ringing / dialing / talking / held / idle): a call it
    // follows is not dropped by KDE Connect's events.
    function kdeCall(o) {}                               // native (phone.rs) follows the calls
    function answer() { msg(["answer"]) }
    function hangup() { msg(["hangup"]) }
    function callNumber(n) { msg(["call", n]) }
    function callMute() { tel("mute") }
    function callAudioToggle() { tel("pc") }                // through the computer / back to the phone
    function tones(t) { tel("tones", t) }
    function tel(c, a) { msg(["tel", JSON.stringify(a === undefined ? { c: c } : { c: c, a: a })]) }
}
