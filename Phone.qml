pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// The phone (KDE Connect, kdeconnectd is the backend), shared by Start, the top bar and the Dynamic Island.
// mistd phone (scripts/mistd) reads the daemon only after its D-Bus signals (no polling) and takes commands on stdin.
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

    function send(c, a) { if (watch.running) watch.write(JSON.stringify(a === undefined ? { c: c } : { c: c, a: a }) + "\n") }
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

    Process {
        id: watch; running: true; stdinEnabled: true
        command: ["setpriv", "--pdeathsig", "TERM", Quickshell.shellDir + "/scripts/bin/mistd", "phone"]
        stdout: SplitParser {
            onRead: l => {
                let o
                try { o = JSON.parse(l) } catch (e) { return }
                if (o.t === "state") { root.ok = o.ok; root.devs = o.devs || []; root.main = o.main || null }
                else if (o.t === "sms") root.threads = o.threads || []
                else if (o.t === "conv") { const c = Object.assign({}, root.convs); c[o.thread] = o.msgs || []; root.convs = c }
                else if (o.t === "contacts") root.contacts = o.list || []
                else if (o.t === "adb") root.adb = o
                else if (o.t === "ev") { if (o.k === "call") root.kdeCall(o); root.event(o) }
            }
        }
        onExited: restart.restart()
    }
    Timer { id: restart; interval: 5000; onTriggered: watch.running = true }

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
    function kdeCall(o) {
        if (o.src === "adb") {
            const st = { ringing: "incoming", talking: "active", dialing: "alerting", held: "held" }[o.what]
            if (!st) { kcCall = null; kcT.stop(); return }
            const keep = kcCall && kcCall.adb && (kcCall.path === o.id || kcCall.pending) ? kcCall : null
            kcCall = { state: st, bt: false, adb: true, path: o.id || "", since: o.since || 0,
                       who: o.who && o.who !== "Nieznany numer" ? o.who : keep ? keep.who : (o.who || "Nieznany numer"),
                       number: o.number || (keep ? keep.number : "") }
            kcT.stop()
            return
        }
        if (kcCall && kcCall.adb && !kcCall.pending) {    // ADB already follows it: KDE Connect may only name the caller
            if (o.what === "ringing" && kcCall.who === "Nieznany numer" && o.who) kcCall = Object.assign({}, kcCall, { who: o.who, number: o.number || kcCall.number })
            return
        }
        if (o.what === "ringing") { kcCall = { state: "incoming", bt: false, who: o.who || o.number || "Nieznany numer", number: o.number || "", since: 0 }; kcT.restart() }
        else kcCall = null                               // talking / missed: KDE Connect never says when it ends
    }
    Timer { id: kcT; interval: 45000; onTriggered: if (root.kcCall && (!root.kcCall.adb || root.kcCall.pending)) root.kcCall = null }
    function answer() { if (call && call.bt) tel("answer", call.path); else { send("answer"); if (!(kcCall && kcCall.adb)) kcCall = null } }
    function hangup() { if (call && call.bt) tel("hangup", call.path); else { send("hangup"); if (!(kcCall && kcCall.adb) || kcCall.pending) kcCall = null } }
    function callNumber(n) {
        if (hfp) { tel("dial", (n || "").replace(/[^\d+*#]/g, "")); return }
        send("call", n)
        if (adbOk && !call) {                            // shown at once; the phone's Telecom log takes over
            kcCall = { state: "alerting", bt: false, adb: true, pending: true, path: "", since: 0, who: contactName(n) || n, number: n }
            kcT.restart()
        }
    }
    function callMute() { tel("mute") }
    function callAudioToggle() { tel("pc") }                // through the computer / back to the phone
    function tones(t) { tel("tones", t) }
    function tel(c, a) { if (calls.running) calls.write(JSON.stringify(a === undefined ? { c: c } : { c: c, a: a }) + "\n") }
    Process {
        id: calls; running: true; stdinEnabled: true
        command: ["setpriv", "--pdeathsig", "TERM", Quickshell.shellDir + "/scripts/bin/mistd", "calls"]
        stdout: SplitParser {
            onRead: l => {
                let o
                try { o = JSON.parse(l) } catch (e) { return }
                if (o.t === "tel") { root.hfp = (o.ags || []).length > 0; root.btCalls = o.calls || [] }
                else if (o.t === "audio") root.callAudio = o
            }
        }
        onExited: callsRestart.restart()
    }
    Timer { id: callsRestart; interval: 5000; onTriggered: calls.running = true }
}
