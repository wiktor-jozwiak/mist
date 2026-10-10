pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Every data source of the shell, refreshed by mistd data (scripts/mistd) (cached in ~/.cache/mist).
Singleton {
    id: root
    property var weather: ({})
    property var garmin: ({})
    property var running: ({})
    property var calendar: ({ items: [] })
    property var sys: ({})
    property date now: new Date()
    property bool sysActive: false       // Start is open -> live CPU / RAM from /proc, disk / uptime every 30 s

    readonly property string script: Quickshell.shellDir + "/scripts/bin/mistd"

    // tick on the minute boundary only (everything shows HH:MM) -> 1 wake-up per minute
    Timer {
        id: tick; running: true; repeat: false
        interval: 60000 - (Date.now() % 60000) + 50
        onTriggered: { root.now = new Date(); interval = 60000 - (Date.now() % 60000) + 50; start() }
    }

    component Source: Process {
        id: src
        property string name
        property int every: 600
        property bool active: true
        property var target: function(v) {}
        command: [root.script, "data", name]
        stdout: StdioCollector {
            onStreamFinished: { try { const v = JSON.parse(this.text); src.target(v); if (v.stale || v.error) src.retry.restart() } catch (e) { src.retry.restart() } }
        }
        // after boot the network / InfluxDB may not be up yet: try again in 60 s instead of waiting a full period
        property Timer retry: Timer { interval: 60000; onTriggered: src.running = true }
        property Timer t: Timer {
            interval: src.every * 1000; running: src.active; repeat: true; triggeredOnStart: true
            onTriggered: src.running = true
        }
    }

    Source { name: "weather"; every: 600; target: v => root.weather = v }
    Source { name: "garmin"; every: 300; target: v => root.garmin = v }
    Source { name: "running"; every: 600; target: v => root.running = v }
    Source { name: "calendar"; every: 900; target: v => root.calendar = v }
    Source { id: sysSrc; name: "sys"; every: 30; active: root.sysActive; target: v => root.sys = v }

    // ------------------------------------------------------------ live CPU % and network speed (2 s)
    property int cpuPct: 0
    property real memUsed: 0             // GiB
    property real memTotal: 0
    property bool barStats: false        // true = sample CPU / RAM / network all the time (for a bar); Noctalia owns the bar now
    property real rxRate: 0
    property real txRate: 0
    property var _cpu: null
    property var _net: null
    // /proc files report size 0, FileView reads them empty: one long-lived bash prints a sample every 2 s
    // using builtins only (read / echo, `read -t` as sleep) -> no fork per tick; runs only while needed
    Process {
        running: root.sysActive || root.barStats
        command: ["setpriv", "--pdeathsig", "TERM", "bash", "-c", "exec 9<> <(:); while :; do read -r l < /proc/stat; echo \"$l\"; echo @@; "
            + "while IFS= read -r l; do echo \"$l\"; done < /proc/meminfo; echo @@; "
            + "while IFS= read -r l; do echo \"$l\"; done < /proc/net/dev; echo @@@; read -t 2 -u 9; done"]
        stdout: SplitParser { splitMarker: "@@@"; onRead: d => root.sample(d) }
    }
    function sample(text) {
        const parts = text.split("@@\n")
        if (parts.length < 3) return
        const mi = {}
        for (const l of parts[1].split("\n")) { const p = l.split(/:\s+/); if (p.length === 2) mi[p[0]] = parseInt(p[1]) }
        if (mi.MemTotal) { root.memTotal = mi.MemTotal / 1048576; root.memUsed = (mi.MemTotal - mi.MemAvailable) / 1048576 }
        const a = parts[0].trim().split(/\s+/).slice(1).map(Number)
        const tot = a.reduce((x, y) => x + y, 0), idle = a[3] + a[4]
        if (root._cpu) root.cpuPct = Math.round(100 * (1 - (idle - root._cpu[1]) / Math.max(1, tot - root._cpu[0])))
        root._cpu = [tot, idle]
        let rx = 0, tx = 0
        for (const l of parts[2].split("\n").slice(2)) {
            const p = l.trim().split(/[:\s]+/)
            if (p.length > 9 && p[0] !== "lo" && !p[0].startsWith("docker") && !p[0].startsWith("veth") && !p[0].startsWith("br-")) { rx += +p[1]; tx += +p[9] }
        }
        const t = Date.now()
        if (root._net) { const dt = (t - root._net[2]) / 1000; root.rxRate = (rx - root._net[0]) / dt; root.txRate = (tx - root._net[1]) / dt }
        root._net = [rx, tx, t]
    }
    function bytes(b) {
        if (b < 1024) return Math.round(b) + " B"
        if (b < 1048576) return (b / 1024).toFixed(1).replace(".", ",") + " KB"
        return (b / 1048576).toFixed(1).replace(".", ",") + " MB"
    }

    // ------------------------------------------------------------ helpers
    readonly property var monthsGen: ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"]
    readonly property var monthsNom: ["Styczeń", "Luty", "Marzec", "Kwiecień", "Maj", "Czerwiec", "Lipiec", "Sierpień", "Wrzesień", "Październik", "Listopad", "Grudzień"]
    readonly property var days: ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"]
    readonly property var daysShort: ["nd", "pon", "wt", "śr", "czw", "pt", "sob"]

    function cap(s) { return s.charAt(0).toUpperCase() + s.slice(1) }
    function two(n) { return n < 10 ? "0" + n : "" + n }
    function hm(d) { return d.getHours() + ":" + two(d.getMinutes()) }
    function longDate(d) { return cap(days[d.getDay()]) + ", " + d.getDate() + " " + monthsGen[d.getMonth()] }
    function sameDay(a, b) { return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate() }

    function eventsOn(day) {
        const s = new Date(day.getFullYear(), day.getMonth(), day.getDate()).getTime() / 1000
        const e = s + 86400
        return (calendar.items || []).filter(x => x.start < e && x.end > s)
    }
    function upcoming(n, daysAhead) {
        const t = now.getTime() / 1000
        return (calendar.items || []).filter(x => x.end > t && x.start < t + daysAhead * 86400 && !x.holiday).slice(0, n)
    }
    function when(ev) {
        if (ev.allDay) return "cały dzień"
        const s = new Date(ev.start * 1000), e = new Date(ev.end * 1000)
        return hm(s) + "–" + hm(e)
    }
    function rel(ts) {
        const d = new Date(ts * 1000)
        const a = new Date(now.getFullYear(), now.getMonth(), now.getDate())
        const b = new Date(d.getFullYear(), d.getMonth(), d.getDate())
        const n = Math.round((b - a) / 86400000)
        return n === 0 ? "dziś" : n === 1 ? "jutro" : n === 2 ? "pojutrze" : n < 7 ? "za " + n + " dni" : "za " + Math.round(n / 7) + " tyg."
    }
}
