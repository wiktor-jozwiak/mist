import QtQuick
import QtQuick.Effects
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Widgets
import "components"
import "components/Astro.js" as Astro

// Desktop layer (under windows): macOS-style clock centred (date above Liquid Glass numerals, shaders/glasstext.frag),
// app folders left, cards right (weather / Garmin / running). Every surface is LiquidGlass (CardGlass);
// inner tiles are a lighter glass step so the cards read in two depths.
Scope {
    id: desk
    required property var modelData
    readonly property bool primary: modelData.name === "DP-3"

    // animate only while the desktop of this monitor is bare (no windows on its workspace)
    readonly property var hmon: Hyprland.monitorFor(modelData)
    readonly property bool bare: !hmon || !hmon.activeWorkspace || hmon.activeWorkspace.toplevels.values.length === 0
    // the weather sky keeps moving behind ordinary windows; only a fullscreen window (game, video) stops it,
    // also one on a special workspace shown over this monitor
    readonly property bool fsCover: {
        if (!hmon) return false
        if (hmon.activeWorkspace && hmon.activeWorkspace.hasFullscreen) return true
        const sp = hmon.lastIpcObject && hmon.lastIpcObject.specialWorkspace ? hmon.lastIpcObject.specialWorkspace.name : ""
        return !!sp && Hyprland.workspaces.values.some(w => w.name === sp && w.hasFullscreen)
    }

    // card palette: shades of white + one accent taken from the wallpaper (teal) + one warning (amber);
    // red only when something is really bad. Secondary text never below 0.6 opacity.
    readonly property color fg: Qt.rgba(244/255, 247/255, 251/255, 0.97)
    readonly property color fg2: Qt.rgba(222/255, 230/255, 242/255, 0.86)
    readonly property color muted: Qt.rgba(206/255, 216/255, 232/255, 0.74)
    readonly property color faint: Qt.rgba(196/255, 208/255, 226/255, 0.6)
    readonly property color acc: "#8ad6d0"
    readonly property color warn: "#ecc48e"
    readonly property color bad: "#ee95a3"
    readonly property color ink: "#0e1a22"
    // level -> colour: fine = accent, low = amber, very low = red
    function level(v) { return v === undefined || v === null ? muted : v >= 50 ? acc : v >= 25 ? warn : bad }
    readonly property color track: Qt.rgba(200/255, 214/255, 236/255, 0.12)
    readonly property color hairC: Qt.rgba(200/255, 214/255, 236/255, 0.10)
    readonly property color cGreen: "#8ad9b2"
    readonly property color cYellow: "#ecd48e"
    readonly property color cOrange: "#efb08a"
    readonly property color cRed: "#ee95a3"
    readonly property color cBlue: "#9dbdf0"
    readonly property color cPurple: "#bba9ee"
    readonly property color cTeal: "#8ad6d6"

    readonly property real cardW: 352
    readonly property real gap: 12

    function km1(x) { return (Math.round((x || 0) * 10) / 10).toFixed(1).replace(".", ",") }
    function runsWord(n) { return n === 1 ? "bieg" : (n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 10 || n % 100 >= 20)) ? "biegi" : "biegów" }
    function thousands(n) { return ("" + Math.round(n || 0)).replace(/\B(?=(\d{3})+(?!\d))/g, " ") }
    function rgba(c, a) { return "rgba(" + Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255) + "," + a + ")" }
    function alpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }

    // ------------------------------------------------------------------ shared parts
    // lighter inner glass step
    // tinted icon chip
    component TitleRow: RowLayout {
        property string title
        property string note
        property string icon
        property color c: desk.cBlue
        width: parent.width
        spacing: 8
        CTxt { text: parent.title; size: Theme.fsXs; w: 650; color: desk.faint; Layout.fillWidth: true; font.capitalization: Font.AllUppercase; font.letterSpacing: 1.4 }
        CTxt { text: parent.note; size: Theme.fsXs; w: 550; color: desk.muted; visible: text !== ""; Layout.maximumWidth: parent.width * 0.6 }
    }
    // small inner glass tile (one step lighter than the card)
    component InnerTile: Rectangle {
        radius: 14
        color: Qt.rgba(1, 1, 1, 0.05)
        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.06)
    }
    // gradient meter with a lit tip
    component Bar: Rectangle {
        id: bar
        property real frac: 0
        property color c: desk.cGreen
        width: parent.width; height: 4; radius: 2; color: desk.track
        Rectangle {
            width: Math.max(parent.height, parent.width * Math.max(0, Math.min(1, parent.frac))); height: parent.height; radius: 2
            visible: parent.frac > 0
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: desk.alpha(bar.c, 0.45) }
                GradientStop { position: 1; color: bar.c }
            }
            Behavior on width { NumberAnimation { duration: 600; easing.type: Easing.OutCubic } }
        }
    }
    component Hair: Rectangle { width: parent.width; height: 1; color: desk.hairC }
    component Gap: Item { property real h: 10; width: 1; height: h }

    // ------------------------------------------------------------------ what the cards show (~/.config/mist/widgets.json)
    // The adjust button (shown on hover) flips a card to its settings; every choice is saved at once.
    readonly property var defaults: ({
        order: ["garmin", "weather", "running"],
        garmin: { hero: "ready", vitals: ["sleep", "hrv", "rhr", "stress"], foot: "insight" },
        weather: { metric: "temp", hours: 24, days: "show", insights: ["rain", "run", "wear", "wind", "light", "tomorrow"] },
        running: { hero: "race", show: ["strip", "workout", "p5"], foot: "last" }
    })
    property var cfg: ({})
    function opt(card, key) { const o = cfg[card]; return o && o[key] !== undefined ? o[key] : defaults[card][key] }
    function setOpt(card, key, v) {
        const n = JSON.parse(JSON.stringify(cfg))
        if (card === "order") n.order = v
        else { n[card] = n[card] || {}; n[card][key] = v }
        cfg = n
        cfgF.setText(JSON.stringify(n, null, 2))
    }
    readonly property var order: {
        const o = cfg.order
        return o && o.length === 3 && ["garmin", "weather", "running"].every(k => o.includes(k)) ? o : defaults.order
    }
    function move(card, d) {
        const o = order.slice(), i = o.indexOf(card), j = i + d
        if (i < 0 || j < 0 || j >= o.length) return
        o[i] = o[j]; o[j] = card
        setOpt("order", "", o)
    }
    FileView {
        id: cfgF; path: Quickshell.env("HOME") + "/.config/mist/widgets.json"; printErrors: false
        onLoaded: { try { desk.cfg = JSON.parse(text()) } catch (e) {} }
    }

    // round adjust button, shown while the card is hovered
    component EditBtn: Rectangle {
        id: eb
        property bool shown: false
        signal clicked()
        width: 26; height: 26; radius: 13
        color: Qt.rgba(1, 1, 1, ebHov.hovered ? 0.2 : 0.11)
        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.12)
        opacity: shown ? 1 : 0; visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 160 } }
        Icon { anchors.centerIn: parent; name: "adjust"; size: 14; color: desk.fg }
        HoverHandler { id: ebHov; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: eb.clicked() }
    }
    // one choice chip
    component OptChip: Rectangle {
        id: oc
        property string label
        property bool on: false
        signal clicked()
        width: ocT.implicitWidth + 20; height: 26; radius: 13
        color: on ? desk.alpha(desk.acc, 0.22) : Qt.rgba(1, 1, 1, ocHov.hovered ? 0.1 : 0.05)
        border.width: 1; border.color: on ? desk.alpha(desk.acc, 0.7) : Qt.rgba(1, 1, 1, 0.08)
        Behavior on color { ColorAnimation { duration: 120 } }
        CTxt { id: ocT; anchors.centerIn: parent; text: oc.label; size: Theme.fsSm; w: oc.on ? 650 : 550; color: oc.on ? desk.fg : desk.fg2 }
        HoverHandler { id: ocHov; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: oc.clicked() }
    }
    // the back of a card: its options as chip groups, card order, done.
    // groups: [{ label, key, multi, max, opts: [[value, label], ...] }]
    component EditSheet: Rectangle {
        id: es
        property string card
        property string title
        property var groups: []
        property bool open: false
        signal done()
        radius: 26
        color: Qt.rgba(9/255, 13/255, 24/255, 0.95)
        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.1)
        opacity: open ? 1 : 0; visible: opacity > 0
        scale: open ? 1 : 0.97
        Behavior on opacity { NumberAnimation { duration: 180 } }
        Behavior on scale { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        // swallow the pointer so the card underneath does not react; right-click closes
        MouseArea { anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.AllButtons }
        WheelHandler { onWheel: e => e.accepted = true }
        RowLayout {
            x: 20; y: 16; width: parent.width - 40; spacing: 6
            Caps { text: es.title; color: desk.fg2; Layout.fillWidth: true }
            Repeater {
                model: [["chevLeft", -1], ["chevron", 1]]
                Rectangle {
                    required property var modelData
                    readonly property int i: desk.order.indexOf(es.card)
                    readonly property bool can: modelData[1] < 0 ? i > 0 : i < desk.order.length - 1
                    width: 26; height: 26; radius: 13; opacity: can ? 1 : 0.3
                    color: Qt.rgba(1, 1, 1, arHov.hovered && can ? 0.14 : 0.06)
                    Icon { anchors.centerIn: parent; name: modelData[0]; size: 14; color: desk.fg }
                    HoverHandler { id: arHov; cursorShape: parent.can ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    TapHandler { onTapped: if (parent.can) desk.move(es.card, parent.modelData[1]) }
                }
            }
            OptChip { label: "Gotowe"; on: true; onClicked: es.done() }
        }
        Column {
            x: 20; y: 56; width: parent.width - 40; spacing: 12
            Repeater {
                model: es.groups
                Column {
                    required property var modelData
                    readonly property var gr: modelData
                    readonly property var cur: desk.opt(es.card, gr.key)
                    width: parent.width; spacing: 6
                    CTxt { text: parent.gr.label + (parent.gr.multi && parent.gr.max ? "  ·  maks. " + parent.gr.max : ""); size: Theme.fsXs; w: 600; color: desk.muted }
                    Flow {
                        width: parent.width; spacing: 6
                        Repeater {
                            model: parent.parent.gr.opts
                            OptChip {
                                required property var modelData
                                readonly property var g: parent.parent.gr
                                readonly property var cur: parent.parent.cur
                                label: modelData[1]
                                on: g.multi ? cur.includes(modelData[0]) : cur === modelData[0]
                                onClicked: {
                                    if (!g.multi) { desk.setOpt(es.card, g.key, modelData[0]); return }
                                    let a = cur.slice()
                                    if (on) a = a.filter(v => v !== modelData[0])
                                    else { a.push(modelData[0]); if (g.max && a.length > g.max) a.shift() }
                                    if (a.length || !g.min) desk.setOpt(es.card, g.key, a)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // (the vignette under the cards is drawn by LiveWallpaper.qml, shaders/vignette.frag)

    // ================================================================== stage
    // One centred composition, like a lock screen: date in spaced caps, the big clock, the sun's path,
    // then a bento row on one grid: Garmin | wide live-sky weather (centre) | running (all the same height).
    readonly property real bentoH: 312
    readonly property real wxW: 548
    readonly property real colW: 308
    readonly property real bentoGap: 16
    readonly property real stageTop: 118
    PanelWindow {
        id: stageWin
        screen: desk.modelData
        visible: desk.primary
        anchors.top: true
        margins.top: desk.stageTop
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Bottom
        WlrLayershell.namespace: "mist-desktop"
        color: "transparent"
        implicitWidth: bento.width + 80; implicitHeight: stage.implicitHeight + 60
        mask: Region { item: bento }

        Column {
            id: stage
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 0
            CTxt {
                anchors.horizontalCenter: parent.horizontalCenter
                text: Data.longDate(Data.now); size: 22; w: 600; color: Qt.rgba(1, 1, 1, 0.82)
                style: Text.Normal
                layer.enabled: true
                layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "black"; shadowOpacity: 0.25; shadowBlur: 0.5; shadowVerticalOffset: 1 }
            }
            CTxt {
                anchors.horizontalCenter: parent.horizontalCenter
                text: Data.hm(Data.now); size: 150; w: 620; color: Qt.rgba(1, 1, 1, 0.86)
                style: Text.Normal
                font.letterSpacing: -4
                font.features: ({ "tnum": 1 })
                topPadding: -14; bottomPadding: -20
                layer.enabled: true
                layer.effect: MultiEffect {
                    shadowEnabled: true; shadowColor: Qt.rgba(2/255, 6/255, 16/255, 1)
                    shadowOpacity: 0.22; shadowBlur: 1.0; blurMax: 48
                    shadowVerticalOffset: 6; autoPaddingEnabled: true
                }
            }
            Gap { h: 16 }
            SunArc {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !!Data.weather.sunrise
                rise: Data.weather.sunrise || ""; set: Data.weather.sunset || ""
                now: Data.now
            }
            Gap { h: 40 }
            Row {
                id: bento
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: desk.bentoGap
                Repeater {
                    model: desk.order
                    Loader {
                        required property var modelData
                        sourceComponent: modelData === "weather" ? wComp : modelData === "garmin" ? gComp : rComp
                    }
                }
                Component { id: gComp; GarminCard {} }
                Component { id: wComp; WeatherSlot {} }
                Component { id: rComp; RunningCard {} }
            }
        }
    }

    // ---------------- weather: live sky you can scrub through time.
    // Hover a day at the bottom: the timeline shows its 24 hours and the headline switches to that day.
    // Move along the timeline: temperature and details follow the hour under the pointer (the sky stays live).
    // Hover the sun times or the moon in the header for details. Leaving the card returns to "now".
    component WeatherCard: CardGlass {
        id: wc
        property point origin                 // top-left on the monitor (WeatherSlot), for Cover
        readonly property real cardH: desk.bentoH
        property int selDay: 0
        property bool pinned: false          // a clicked day / hour stays until clicked again (or 20 s away)
        property int pinHour: -1
        readonly property int hoverHour: tl.idx >= 0 ? tl.idx : pinHour
        property bool editing: false
        property int ii: 0                    // which insight the footer shows
        onSelDayChanged: pinHour = -1
        readonly property string metric: desk.opt("weather", "metric")
        readonly property int hours: desk.opt("weather", "hours")
        HoverHandler { id: hov }
        Timer { interval: wc.pinned ? 20000 : 2500; running: !hov.hovered && (wc.selDay !== 0 || wc.pinHour >= 0); onTriggered: { wc.selDay = 0; wc.pinHour = -1; wc.pinned = false } }
        // wheel: walk through the days
        WheelHandler {
            onWheel: e => {
                const n = Math.min(4, wc.days.length - 1)
                wc.selDay = Math.max(0, Math.min(n, wc.selDay + (e.angleDelta.y < 0 ? 1 : -1)))
                wc.pinned = wc.selDay > 0; wc.pinHour = -1
            }
        }

        readonly property var wx: Data.weather
        readonly property var days: wx.days || []
        readonly property var hourly: wx.hourly || []
        readonly property var today: days[0]
        readonly property real lat: wx.lat ?? 52.2297
        readonly property real lon: wx.lon ?? 21.0122
        // the hours on the timeline: the next 24 from now, or the selected day 0-23 h
        readonly property var range: {
            const H = hourly
            if (!H.length) return []
            if (selDay === 0) {
                const n = Data.now.getTime() / 1000
                const i = Math.max(0, H.findIndex(h => h.t + 3600 > n))
                return H.slice(i, i + wc.hours + 1)
            }
            const d = days[selDay]
            if (!d) return []
            const start = new Date(d.date + "T00:00:00").getTime() / 1000
            const i = H.findIndex(h => h.t >= start)
            return i < 0 ? [] : H.slice(i, i + 24)
        }
        // what the text previews: an hour under the pointer, else the selected day, else now
        readonly property var hh: hoverHour >= 0 && hoverHour < range.length ? range[hoverHour] : null
        readonly property var sd: selDay > 0 ? days[selDay] || null : null
        readonly property bool live: !hh && !sd
        readonly property int code: hh ? hh.code : sd ? sd.code : (wx.code ?? 3)

        function wmo(c) {
            const m = { 0: "Bezchmurnie", 1: "Przeważnie słonecznie", 2: "Częściowe zachmurzenie", 3: "Pochmurno",
                45: "Mgła", 48: "Szadź", 51: "Lekka mżawka", 53: "Mżawka", 55: "Gęsta mżawka", 56: "Marznąca mżawka",
                57: "Marznąca mżawka", 61: "Lekki deszcz", 63: "Deszcz", 65: "Ulewa", 66: "Marznący deszcz",
                67: "Marznący deszcz", 71: "Lekki śnieg", 73: "Śnieg", 75: "Intensywny śnieg", 77: "Ziarna śniegu",
                80: "Przelotny deszcz", 81: "Przelotne opady", 82: "Ulewy", 85: "Przelotny śnieg", 86: "Śnieżyca",
                95: "Burza", 96: "Burza z gradem", 99: "Burza z gradem" }
            return m[c] || "—"
        }
        function dir8(d) { return ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][Math.round((d || 0) / 45) % 8] }
        function dec1(x) { return (Math.round((x || 0) * 10) / 10).toString().replace(".", ",") }
        function dayName(d) { return ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"][d.getDay()] }
        function dayMon(d) {
            if (!d) return "–"
            return d.getDate() + " " + ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"][d.getMonth()]
        }
        function hhmm(s) { return s ? s.replace(/^0/, "") : "–" }
        function mins(s) { const p = (s || "0:0").split(":"); return +p[0] * 60 + +p[1] }
        function dur(m) { m = Math.max(0, Math.round(m)); return Math.floor(m / 60) + " h " + (m % 60) + " min" }

        readonly property string label: {
            if (hh) {
                const d = new Date(hh.t * 1000)
                if (selDay === 0 && hoverHour === 0) return "teraz"
                const same = d.toDateString() === Data.now.toDateString()
                return (same ? "dziś" : selDay === 0 ? "jutro" : dayName(d)) + ", " + d.getHours() + ":00"
            }
            if (sd) { const d = new Date(sd.date + "T12:00:00"); return dayName(d) + ", " + dayMon(d) }
            return ""
        }
        // next two hours in 15-minute steps: when it starts / stops
        readonly property string nowcast: {
            const q = wx.soon || []
            if (!q.length) return ""
            if ((wx.precip || 0) > 0) {
                const i = q.findIndex(v => v < 0.05)
                return i < 0 ? "pada jeszcze 2 h+" : i === 0 ? "" : "przestanie za " + i * 15 + " min"
            }
            const i = q.findIndex(v => v >= 0.05)
            return i < 0 ? "" : i === 0 ? "zaraz zacznie padać" : "opad za " + i * 15 + " min"
        }
        // one sentence that matters most: when rain starts / stops, strong gusts, else the warmest hour
        readonly property var next24: {
            const n = Data.now.getTime() / 1000
            const i = Math.max(0, hourly.findIndex(h => h.t + 3600 > n))
            return hourly.slice(i, i + 24)
        }
        function hourOf(h) { return new Date(h.t * 1000).getHours() + ":00" }
        readonly property var summary: {
            if (hh) {
                const parts = []
                if ((hh.pop || 0) >= 20 || hh.pr > 0) parts.push("opad " + (hh.pop || 0) + "%" + (hh.pr > 0 ? ", " + dec1(hh.pr) + " mm" : ""))
                parts.push("wiatr " + hh.wind + (hh.gust >= hh.wind + 10 ? "–" + hh.gust : "") + " km/h")
                return { icon: (hh.pop || 0) >= 20 ? "umbrella" : "wind", text: parts.join(" · ") }
            }
            if (sd) return { icon: sd.pop >= 30 ? "umbrella" : "wind",
                text: (sd.pop >= 30 ? "Opad " + sd.pop + "%, " + dec1(sd.rain) + " mm · " : "Bez większych opadów · ") + "porywy do " + sd.gust + " km/h" }
            const L = insights
            return L.length ? L[ii % L.length] : { icon: "", text: "" }
        }

        // ---- insights: a small stack of things worth knowing, most urgent first. Click the footer for the next one;
        // the hours an insight talks about light up on the timeline (i0..i1 index the next-24-h range).
        readonly property var insights: {
            const R = next24, en = desk.opt("weather", "insights"), out = []
            if (!R.length) return out
            const wet = h => (h.pr || 0) >= 0.1 || (h.sn || 0) > 0
            const snowy = h => (h.sn || 0) > 0 || h.temp <= 0.5
            const hr = h => new Date(h.t * 1000).getHours()
            if (en.includes("rain")) {
                let t = "", i0 = -1, i1 = -1, p = 2
                if ((wx.precip || 0) > 0 || wet(R[0])) {
                    const i = R.findIndex(h => !wet(h))
                    t = nowcast.startsWith("przestanie") ? (nowcast.charAt(0).toUpperCase() + nowcast.slice(1))
                        : i < 0 ? "Opady przez całą dobę" : "Przestanie padać ok. " + hourOf(R[i])
                    i0 = 0; i1 = i < 0 ? R.length - 1 : i - 1; p = 9
                } else {
                    const i = R.findIndex(wet)
                    if (i >= 0) {
                        let j = i; while (j + 1 < R.length && wet(R[j + 1])) j++
                        const mm = R.slice(i, j + 1).reduce((a, h) => a + (h.pr || 0), 0)
                        t = (nowcast.startsWith("opad za") ? (snowy(R[0]) ? "Śnieg" : "Deszcz") + " za " + nowcast.slice(8)
                            : (snowy(R[i]) ? "Śnieg" : "Deszcz") + " " + hr(R[i]) + "–" + (hr(R[j]) + 1) % 24 + ":00") + " · " + dec1(mm) + " mm"
                        i0 = i; i1 = j; p = i < 4 ? 8 : i < 10 ? 5 : 3
                    } else { t = "Bez opadów przez 24 h"; p = 1 }
                }
                out.push({ icon: "umbrella", c: p >= 5 ? desk.cBlue : desk.acc, text: t, i0: i0, i1: i1, p: p })
            }
            if (en.includes("run")) {
                // the best hour to run in daylight before the next 14 h are over: dry, calm, cool not cold
                let best = -1, bs = 1e9
                for (let i = 0; i < Math.min(R.length, 15); i++) {
                    const h = R[i], H = hr(h)
                    if (!h.day || H < 6 || H > 21) continue
                    const s = (h.pr || 0) * 12 + (h.pop || 0) / 8 + Math.max(0, h.wind - 12) / 2 + Math.abs(h.feels - 10) / 2 + i * 0.15
                    if (s < bs) { bs = s; best = i }
                }
                if (best >= 0) {
                    const h = R[best], w = Data.garmin.workout
                    out.push({ icon: "run", c: desk.acc, i0: best, i1: best, p: w ? 6 : 4,
                        text: (w ? "Trening" : "Bieg") + " najlepiej ok. " + hourOf(h) + " · " + Math.round(h.temp) + "°, "
                            + ((h.pop || 0) < 30 && !(h.pr > 0) ? "sucho" : "opad " + h.pop + "%") + ", wiatr " + h.wind })
                }
            }
            if (en.includes("wear")) {
                const f = Math.min(...R.slice(0, 4).map(h => h.feels)), rf = f + 10
                const coat = f <= -5 ? "Ciepła kurtka, czapka, rękawiczki" : f <= 3 ? "Kurtka i czapka" : f <= 10 ? "Kurtka" : f <= 16 ? "Bluza lub lekka kurtka" : f <= 22 ? "Długi rękaw" : "Krótki rękaw"
                const run = rf >= 18 ? "krótki rękaw" : rf >= 12 ? "długi rękaw" : rf >= 5 ? "długi rękaw + rękawiczki" : "warstwy, czapka, rękawiczki"
                out.push({ icon: "temp", c: desk.acc, i0: 0, i1: 3, p: f <= 3 ? 5 : 2, text: coat + " · na bieg " + run })
            }
            if (en.includes("wind")) {
                const W = R.slice(0, 12), g = Math.max(...W.map(h => h.gust || 0))
                if (g >= 40) {
                    const i = W.findIndex(h => h.gust === g)
                    out.push({ icon: "wind", c: g >= 60 ? desk.bad : desk.warn, i0: i, i1: i, p: g >= 60 ? 9 : g >= 50 ? 6 : 3,
                        text: "Porywy do " + g + " km/h ok. " + hourOf(W[i]) + " · z " + dir8(W[i].dir) })
                }
            }
            if (en.includes("light") && today) {
                const nm = Data.now.getHours() * 60 + Data.now.getMinutes()
                const r = mins(today.rise || wx.sunrise), s = mins(today.set || wx.sunset)
                const t1 = days[1], len0 = s - r, len1 = t1 ? mins(t1.set) - mins(t1.rise) : len0
                const dl = len1 - len0
                const tail = dl ? " · jutro " + (dl < 0 ? "krótszy" : "dłuższy") + " o " + Math.abs(dl) + " min" : ""
                out.push({ icon: nm < s ? "sunset" : "sunrise", c: desk.cYellow, i0: -1, i1: -1, p: nm < s && s - nm < 90 ? 5 : 1,
                    text: nm < r ? "Wschód " + hhmm(today.rise) + " · za " + dur(r - nm)
                        : nm < s ? "Zachód " + hhmm(today.set) + " · światła jeszcze " + dur(s - nm) + tail
                        : "Wschód jutro " + hhmm(t1 ? t1.rise : "") + tail })
            }
            if (en.includes("tomorrow") && days[1] && today) {
                const t = days[1], dt = t.max - today.max
                out.push({ icon: Theme.wxIcon(t.code, 1), c: desk.acc, i0: -1, i1: -1, p: Math.abs(dt) >= 5 ? 4 : 1,
                    text: "Jutro " + t.max + "°/" + t.min + "° · " + wmo(t.code).toLowerCase()
                        + (dt <= -2 ? ", " + -dt + "° chłodniej" : dt >= 2 ? ", " + dt + "° cieplej" : "") })
            }
            return out.sort((a, b) => b.p - a.p)
        }
        readonly property var cur: insights.length ? insights[ii % insights.length] : null
        // the hours to light up on the timeline: only while the card shows now
        readonly property var hl: live && selDay === 0 && cur && cur.i0 >= 0 ? [cur.i0, cur.i1] : [-1, -1]

        // sun & moon popovers
        readonly property var moonP: Astro.moon(Data.now, lat, lon)
        readonly property string moonName: {
            const p = moonP.phase
            return p < 0.03 || p > 0.97 ? "Nów" : p < 0.22 ? "Przybywający sierp" : p < 0.28 ? "Pierwsza kwadra"
                : p < 0.47 ? "Przybywający garb" : p < 0.53 ? "Pełnia" : p < 0.72 ? "Ubywający garb"
                : p < 0.78 ? "Ostatnia kwadra" : "Ubywający sierp"
        }
        property string lastPop: ""
        readonly property string popKind: sunHov.hovered ? "sun" : moonHov.hovered ? "moon" : popHov.hovered ? lastPop : ""
        onPopKindChanged: if (popKind) lastPop = popKind
        readonly property var moonInfo: {
            if (popKind !== "moon") return null
            const n = Data.now, tm = Astro.moonTimes(n, lat, lon)
            return { rise: tm.rise, set: tm.set, newM: Astro.nextPhase(n, 0), full: Astro.nextPhase(n, 0.5), dist: Astro.moonDistance(n) }
        }
        readonly property var sunDay: today
        readonly property var sunP: Astro.sun(Data.now, lat, lon)

        implicitWidth: desk.wxW
        padL: 0; padR: 0; padT: 0; padB: 0
        Item {
            width: parent.width; height: wc.cardH
            // a lightning strike shakes the card a little
            transform: Translate { x: sky.flash > 0.45 ? (Math.random() - 0.5) * 5 * sky.flash : 0; y: sky.flash > 0.45 ? (Math.random() - 0.5) * 3 * sky.flash : 0 }

            // the sky is always live: the real weather and time here and now (previewing an hour / a day only changes the text)
            Sky {
                id: sky
                x: 1; y: 1; width: parent.width - 2; height: parent.height - 2
                radius: 25
                hor: 0.6
                code: wc.wx.code ?? 3
                cover: wc.wx.cloud ?? -1
                precip: wc.wx.precip ?? 0
                snowNow: wc.wx.snow ?? 0
                wind: wc.wx.wind ?? 0
                gust: wc.wx.gust ?? 0
                windDir: wc.wx.windDir ?? 270
                vis: wc.wx.vis ?? 50000
                temp: wc.wx.temp ?? 10
                hum: wc.wx.hum ?? 70
                snowDepth: wc.wx.snowDepth || 0
                kp: wc.wx.kp ?? 0; lat: wc.lat; lon: wc.lon
                now: Data.now
                animate: !desk.fsCover && Ui.dbgSky !== "off"
                slow: Ui.dbgSky !== "fast" && Cover.hidden(desk.modelData.name, desk.modelData.x + wc.origin.x + sky.x, desk.modelData.y + wc.origin.y + sky.y, sky.width, sky.height)
            }
            // soft shade behind the headline (top left), so the text never fights a bright sky
            Rectangle {
                x: 1; y: 1; width: parent.width - 2; height: parent.height * 0.55; radius: 25
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Qt.rgba(4/255, 8/255, 16/255, 0.26) }
                    GradientStop { position: 0.6; color: Qt.rgba(4/255, 8/255, 16/255, 0.08) }
                    GradientStop { position: 1.0; color: Qt.rgba(4/255, 8/255, 16/255, 0.0) }
                }
            }
            // soft shade over the land so the timeline and the days read well (no panel: the sky stays whole)
            Rectangle {
                x: 1; y: parent.height * 0.40; width: parent.width - 2; height: parent.height * 0.60 - 1; radius: 25
                gradient: Gradient {
                    GradientStop { position: 0.0; color: Qt.rgba(4/255, 8/255, 16/255, 0.0) }
                    GradientStop { position: 0.35; color: Qt.rgba(4/255, 8/255, 16/255, 0.28) }
                    GradientStop { position: 1.0; color: Qt.rgba(4/255, 8/255, 16/255, 0.58) }
                }
            }

            Item {
                anchors.fill: parent
                layer.enabled: true
                layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Qt.rgba(4/255, 8/255, 18/255, 1); shadowOpacity: 0.6; shadowBlur: 0.6; shadowVerticalOffset: 1 }

                // ---- headline (follows the preview)
                EditBtn { anchors.left: city.right; anchors.leftMargin: 8; anchors.verticalCenter: city.verticalCenter; shown: hov.hovered && !wc.editing; onClicked: wc.editing = true }
                Caps {
                    id: city
                    x: desk.pad; y: desk.pad
                    text: (wc.wx.city || "Warszawa") + (wc.wx.stale ? " · dane z " + Data.hm(new Date(wc.wx.at * 1000)) : "")
                    color: desk.fg2
                }
                Hero {
                    id: bigT
                    x: desk.pad; y: desk.pad + desk.heroY
                    text: wc.hh ? Math.round(wc.hh.temp) + "°" : wc.sd ? wc.sd.max + "°" : wc.wx.temp !== undefined ? wc.wx.temp + "°" : "–"
                }
                Column {
                    anchors.left: bigT.right; anchors.leftMargin: 10
                    anchors.baseline: bigT.baseline; anchors.baselineOffset: -20
                    spacing: 2
                    CTxt {
                        visible: !!wc.label
                        text: wc.label; size: Theme.fsXs; w: 650; color: desk.acc
                        font.capitalization: Font.AllUppercase; font.letterSpacing: 0.6
                    }
                    CTxt { text: wc.live ? (wc.wx.desc || "łączenie…") : wc.wmo(wc.code); size: Theme.fsLg; w: 650; color: desk.fg; width: Math.min(implicitWidth, wc.width - bigT.width - 300) }
                    CTxt {
                        text: wc.hh ? "odczuwalna " + wc.hh.feels + "°"
                            : wc.sd ? wc.sd.max + "° / " + wc.sd.min + "°"
                            : wc.wx.feels !== undefined ? "odczuwalna " + wc.wx.feels + "°" : ""
                        size: Theme.fsXs; w: 500; color: desk.fg2
                    }
                }
                // the one thing worth knowing; click for the next one (dots = how many there are)
                Item {
                    x: desk.pad; y: desk.pad + desk.footY; width: parent.width - 2 * desk.pad; height: 24
                    id: foot
                    visible: !!wc.summary.text
                    readonly property bool stack: wc.live && wc.insights.length > 1
                    HoverHandler { cursorShape: parent.stack ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    TapHandler { enabled: parent.stack; onTapped: { wc.ii = (wc.ii + 1) % wc.insights.length; ftFade.restart() } }
                    // carousel: step to the next insight on its own, paused while the card is hovered
                    Timer { interval: 8000; repeat: true; running: foot.stack && !hov.hovered; onTriggered: { wc.ii = (wc.ii + 1) % wc.insights.length; ftFade.restart() } }
                    NumberAnimation { id: ftFade; target: ftRow; property: "opacity"; from: 0.2; to: 1; duration: 220 }
                    Row {
                        id: ftRow
                        anchors.verticalCenter: parent.verticalCenter; spacing: 8
                        Icon { anchors.verticalCenter: parent.verticalCenter; name: wc.summary.icon; size: 14; color: wc.summary.c || desk.acc }
                        CTxt { anchors.verticalCenter: parent.verticalCenter; text: wc.summary.text; size: Theme.fsMd; w: 600; color: desk.fg; width: Math.min(implicitWidth, ftRow.parent.width - dots.width - 34) }
                    }
                    Row {
                        id: dots
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 5
                        visible: parent.stack
                        Repeater {
                            model: wc.insights.length
                            Rectangle {
                                required property int index
                                width: 5; height: 5; radius: 2.5
                                color: index === wc.ii % Math.max(1, wc.insights.length) ? desk.fg : Qt.rgba(1, 1, 1, 0.3)
                            }
                        }
                    }
                }

                // what the timeline draws: temperature, precipitation or wind (shown while the card is hovered)
                Row {
                    x: parent.width - width - desk.pad; y: desk.pad + desk.bandY - 2
                    spacing: 6
                    opacity: hov.hovered && !wc.editing ? 1 : 0; visible: opacity > 0
                    Behavior on opacity { NumberAnimation { duration: 160 } }
                    Repeater {
                        model: [["temp", "Temp."], ["rain", "Opady"], ["wind", "Wiatr"]]
                        OptChip { required property var modelData; label: modelData[1]; on: wc.metric === modelData[0]; onClicked: desk.setOpt("weather", "metric", modelData[0]) }
                    }
                }

                // ---- timeline: temperature / precipitation / wind over the hours; hover to scrub, click to pin an hour
                Item {
                    id: tl
                    x: desk.pad; y: desk.pad + desk.bandY + 28; width: parent.width - 2 * desk.pad; height: 78
                    readonly property int n: wc.range.length
                    readonly property real step: n > 1 ? width / (n - 1) : width
                    readonly property int every: Math.max(3, Math.round((n - 1) / 4))
                    readonly property string m: wc.metric
                    readonly property real vmin: !n ? 0 : m === "temp" ? Math.min(...wc.range.map(h => h.temp)) : 0
                    readonly property real vmax: !n ? 1 : m === "temp" ? Math.max(...wc.range.map(h => h.temp)) : m === "rain" ? 100 : Math.max(30, ...wc.range.map(h => h.gust || h.wind))
                    function ty(v) { return 16 + (1 - (v - vmin) / Math.max(1, vmax - vmin)) * 30 }
                    function val(h) { return m === "rain" ? (h.pop || 0) : m === "wind" ? h.wind : h.temp }
                    function lab(h) { return m === "rain" ? (h.pop || 0) + "%" : m === "wind" ? "" + h.wind : Math.round(h.temp) + "°" }
                    readonly property int idx: tlHov.hovered && n ? Math.max(0, Math.min(n - 1, Math.round(tlHov.point.position.x / step))) : -1
                    HoverHandler { id: tlHov; cursorShape: Qt.SizeHorCursor }
                    TapHandler { onTapped: { wc.pinHour = wc.pinHour === tl.idx ? -1 : tl.idx; wc.pinned = wc.pinHour >= 0 } }
                    Canvas {
                        id: curve
                        anchors.fill: parent
                        property var r: wc.range
                        property string m: tl.m
                        property var hl: wc.hl
                        onRChanged: requestPaint()
                        onMChanged: requestPaint()
                        onHlChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onPaint: {
                            const x = getContext("2d"); x.reset()
                            const R = r, n = R.length
                            if (n < 2) return
                            const st = width / (n - 1), base = 60
                            // background washes, each softened over neighbouring hours so no edge is ever sharp:
                            // night (dark), rain in temperature mode (blue), the hours the footer is talking about (teal)
                            const soft = v => v.map((_, i) => {
                                let s = 0, w = 0
                                for (let j = -3; j <= 3; j++) { const k = i + j; if (k < 0 || k >= n) continue; const q = 4 - Math.abs(j); s += v[k] * q; w += q }
                                return s / w
                            })
                            const wash = (v, rgb, a) => {
                                const g = x.createLinearGradient(0, 0, width, 0)
                                for (let i = 0; i < n; i++) g.addColorStop(i / (n - 1), "rgba(" + rgb + "," + (v[i] * a).toFixed(3) + ")")
                                x.fillStyle = g; x.fillRect(0, 0, width, base)
                            }
                            wash(soft(R.map(h => h.day ? 0 : 1)), "0,0,0", 0.08)
                            if (m === "temp") wash(soft(R.map(h => Math.max(Math.min(1, (h.pr || 0) / 1.5), (h.pop || 0) >= 40 ? ((h.pop || 0) / 100 - 0.4) * 0.5 : 0))), "157,189,240", 0.22)
                            if (hl[0] >= 0) wash(soft(R.map((_, i) => i >= hl[0] && i <= hl[1] ? 1 : 0)), "138,214,208", 0.2)
                            // fade all of it in from the ground up (no visible box)
                            x.globalCompositeOperation = "destination-in"
                            const vg = x.createLinearGradient(0, 0, 0, base)
                            vg.addColorStop(0, "rgba(0,0,0,0)"); vg.addColorStop(0.55, "rgba(0,0,0,0.35)"); vg.addColorStop(1, "rgba(0,0,0,1)")
                            x.fillStyle = vg; x.fillRect(0, 0, width, base + 2)
                            x.clearRect(0, base + 2, width, height)
                            x.globalCompositeOperation = "source-over"
                            const curveOf = (vals) => {
                                x.beginPath(); x.moveTo(0, tl.ty(vals[0]))
                                for (let i = 1; i < n; i++) {
                                    const mx = (i - 0.5) * st
                                    x.bezierCurveTo(mx, tl.ty(vals[i - 1]), mx, tl.ty(vals[i]), i * st, tl.ty(vals[i]))
                                }
                            }
                            if (m === "rain") {
                                // bars = millimetres (2 mm fills the band), line = chance
                                for (let i = 0; i < n; i++) {
                                    const mm = (R[i].pr || 0) + (R[i].sn || 0), hgt = Math.min(1, mm / 2) * 44
                                    if (hgt < 0.5) continue
                                    x.fillStyle = (R[i].sn || 0) > 0 ? "rgba(230,240,255,0.85)" : "rgba(157,189,240,0.85)"
                                    x.fillRect(i * st - st * 0.32, base - hgt, Math.max(2, st * 0.64), hgt)
                                }
                                curveOf(R.map(h => h.pop || 0))
                                x.lineWidth = 1.6; x.strokeStyle = "rgba(157,189,240,0.9)"; x.setLineDash([3, 3]); x.stroke(); x.setLineDash([])
                                return
                            }
                            if (m === "wind") {
                                // band between wind and gusts, line = mean wind
                                x.beginPath(); x.moveTo(0, tl.ty(R[0].gust || R[0].wind))
                                for (let i = 1; i < n; i++) x.lineTo(i * st, tl.ty(R[i].gust || R[i].wind))
                                for (let i = n - 1; i >= 0; i--) x.lineTo(i * st, tl.ty(R[i].wind))
                                x.closePath(); x.fillStyle = "rgba(220,232,248,0.16)"; x.fill()
                                curveOf(R.map(h => h.wind))
                                x.lineWidth = 2.2; x.lineCap = "round"; x.strokeStyle = "#dce8f8"; x.stroke()
                                return
                            }
                            // temperature curve
                            curveOf(R.map(h => h.temp))
                            const g = x.createLinearGradient(0, 12, 0, 46)
                            g.addColorStop(0, "#efb08a"); g.addColorStop(1, "#9dbdf0")
                            x.lineWidth = 2.2; x.strokeStyle = g; x.lineCap = "round"; x.stroke()
                        }
                    }
                    // values every few hours, above the curve
                    Repeater {
                        model: wc.range
                        CTxt {
                            required property var modelData
                            required property int index
                            visible: index % tl.every === 0 && index > 0 && wc.hoverHour < 0 && (tl.m !== "rain" || (modelData.pop || 0) >= 10)
                            x: Math.max(0, Math.min(tl.width - width, index * tl.step - width / 2)); y: tl.ty(tl.val(modelData)) - 16
                            text: tl.lab(modelData); size: Theme.fsXs; w: 650; color: desk.fg
                        }
                    }
                    // hour labels
                    Repeater {
                        model: wc.range
                        CTxt {
                            required property var modelData
                            required property int index
                            visible: index % tl.every === 0 && (wc.hoverHour < 0 || Math.abs(index - wc.hoverHour) > tl.every / 3)
                            x: Math.max(0, Math.min(tl.width - width, index * tl.step - width / 2)); y: 63
                            text: index === 0 && wc.selDay === 0 ? "teraz" : new Date(modelData.t * 1000).getHours() + ""
                            size: Theme.fsXs; w: 550; color: desk.fg2
                        }
                    }
                    // scrubber (pinned: accent)
                    Item {
                        visible: wc.hoverHour >= 0 && wc.hoverHour < tl.n
                        x: wc.hoverHour * tl.step
                        readonly property bool pin: tl.idx < 0
                        Behavior on x { NumberAnimation { duration: 90 } }
                        Rectangle { x: -0.5; y: 4; width: 1; height: 56; color: parent.pin ? desk.acc : Qt.rgba(1, 1, 1, 0.45) }
                        Rectangle {
                            x: -4.5; y: wc.hh ? tl.ty(tl.val(wc.hh)) - 4.5 : 0; width: 9; height: 9; radius: 4.5
                            color: parent.pin ? desk.acc : desk.fg; border.width: 2; border.color: Qt.rgba(0.1, 0.14, 0.24, 0.8)
                        }
                        CTxt {
                            x: -width / 2; y: 62
                            text: wc.hh ? new Date(wc.hh.t * 1000).getHours() + ":00" : ""
                            size: Theme.fsXs; w: 700; color: parent.pin ? desk.acc : desk.fg
                        }
                    }
                }

                // ---- the next four days, top right; hover a day to preview it, click to keep it
                Row {
                    visible: desk.opt("weather", "days") === "show"
                    x: parent.width - width - 14; y: 12; width: 248; height: 84
                    Repeater {
                        model: wc.days.slice(1, 5)
                        Item {
                            required property var modelData
                            required property int index
                            readonly property int day: index + 1
                            readonly property bool sel: wc.selDay === day
                            width: parent.width / 4; height: parent.height
                            Rectangle {
                                anchors.fill: parent; anchors.margins: 2; radius: 16
                                color: Qt.rgba(1, 1, 1, parent.sel ? 0.16 : dHov.hovered ? 0.09 : 0)
                                border.width: parent.sel ? 1 : 0; border.color: wc.pinned ? desk.alpha(desk.acc, 0.8) : Qt.rgba(1, 1, 1, 0.14)
                                Behavior on color { ColorAnimation { duration: 160 } }
                            }
                            Column {
                                anchors.centerIn: parent; spacing: 4
                                CTxt { anchors.horizontalCenter: parent.horizontalCenter; text: modelData.name; size: Theme.fsXs; w: 650; color: parent.parent.sel ? desk.fg : desk.fg2; font.capitalization: Font.AllUppercase; font.letterSpacing: 0.8 }
                                Icon {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    readonly property string n: Theme.wxIcon(modelData.code, 1)
                                    name: n; size: 18
                                    color: n === "sun" || n === "cloudSun" ? desk.cYellow : n === "rain" || n === "storm" ? desk.cBlue : desk.fg
                                }
                                Row {
                                    anchors.horizontalCenter: parent.horizontalCenter; spacing: 3
                                    CTxt { text: modelData.max + "°"; size: Theme.fsMd; w: 650; color: desk.fg }
                                    CTxt { text: modelData.min + "°"; size: Theme.fsXs; w: 500; color: desk.muted; anchors.baseline: parent.children[0].baseline }
                                }
                            }
                            HoverHandler { id: dHov; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered && !wc.pinned) wc.selDay = parent.day }
                            TapHandler {
                                onTapped: {
                                    const p = wc.pinned && wc.selDay === parent.day
                                    wc.selDay = p ? 0 : parent.day; wc.pinned = !p; wc.pinHour = -1
                                }
                            }
                        }
                    }
                }
            }

            // ---- the sun and the moon in the sky are hoverable: details pop up next to them
            Item {
                id: sunSpot
                readonly property point c: Qt.point(sky.x + sky.sunS.x * sky.width, sky.y + sky.sunS.y * sky.height)
                visible: sky.sunP.alt > -1.5 && c.x > 6 && c.x < parent.width - 6 && c.y > 6 && c.y < 140
                x: c.x - 20; y: c.y - 20; width: 40; height: 40
                HoverHandler { id: sunHov }
                Rectangle {
                    anchors.centerIn: parent; width: 34; height: 34; radius: 17
                    color: "transparent"; border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.55)
                    opacity: sunHov.hovered ? 1 : 0; Behavior on opacity { NumberAnimation { duration: 150 } }
                }
            }
            Item {
                id: moonSpot
                readonly property point c: Qt.point(sky.x + sky.moonS.x * sky.width, sky.y + sky.moonS.y * sky.height)
                visible: sky.moonP.alt > -1 && c.x > 6 && c.x < parent.width - 6 && c.y > 6 && c.y < 140
                x: c.x - 18; y: c.y - 18; width: 36; height: 36
                HoverHandler { id: moonHov }
                Rectangle {
                    anchors.centerIn: parent; width: 32; height: 32; radius: 16
                    color: "transparent"; border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.55)
                    opacity: moonHov.hovered ? 1 : 0; Behavior on opacity { NumberAnimation { duration: 150 } }
                }
            }

            // ---- popover: sun or moon details, next to the body (stays while the pointer is on it)
            Rectangle {
                id: popover
                readonly property point at: wc.lastPop === "moon" ? moonSpot.c : sunSpot.c
                x: Math.max(8, Math.min(parent.width - width - 8, at.x - width / 2))
                y: at.y < 80 ? at.y + 24 : Math.max(6, at.y - height - 24)
                width: Math.min(336, popRow.implicitWidth + 24); height: popRow.implicitHeight + 22; radius: 16
                color: Qt.rgba(10/255, 14/255, 26/255, 0.86)
                border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.12)
                opacity: wc.popKind ? 1 : 0
                visible: opacity > 0
                scale: wc.popKind ? 1 : 0.96
                transformOrigin: Item.Center
                Behavior on opacity { NumberAnimation { duration: 160 } }
                Behavior on scale { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
                HoverHandler { id: popHov }
                Row {
                    id: popRow
                    x: 12; y: 11; spacing: 12
                    MoonDisc { visible: wc.lastPop === "moon"; size: 46; halo: false; phase: wc.moonP.phase; anchors.verticalCenter: parent.verticalCenter }
                    Column {
                        id: popCol
                        spacing: 3
                        CTxt {
                            text: wc.lastPop === "moon" ? wc.moonName : "Słońce"
                            size: 13.5; w: 650; color: desk.fg
                        }
                        Repeater {
                            model: wc.lastPop === "moon" ? (wc.moonInfo ? [
                                    "oświetlenie " + Math.round(wc.moonP.fraction * 100) + "% · dzień " + wc.dec1(wc.moonP.phase * 29.53),
                                    "wschód " + (wc.moonInfo.rise ? Data.hm(wc.moonInfo.rise) : "–") + " · zachód " + (wc.moonInfo.set ? Data.hm(wc.moonInfo.set) : "–"),
                                    "nów " + wc.dayMon(wc.moonInfo.newM) + " · pełnia " + wc.dayMon(wc.moonInfo.full),
                                    (wc.moonP.alt > 0 ? Math.round(wc.moonP.alt) + "° nad horyzontem" : "pod horyzontem") + " · " + desk.thousands(wc.moonInfo.dist) + " km"
                                ] : [])
                                : wc.sunDay ? (function () {
                                    const r = wc.sunDay.rise || wc.wx.sunrise, s = wc.sunDay.set || wc.wx.sunset
                                    const len = wc.mins(s) - wc.mins(r)
                                    const out = ["wschód " + wc.hhmm(r) + " · zachód " + wc.hhmm(s), "dzień trwa " + wc.dur(len)]
                                    {
                                        const nm = Data.now.getHours() * 60 + Data.now.getMinutes()
                                        out.push(wc.sunP.alt > 0 ? "słońce " + Math.round(wc.sunP.alt) + "° nad horyzontem" : "słońce " + Math.round(-wc.sunP.alt) + "° pod horyzontem")
                                        out.push(nm < wc.mins(r) ? "do wschodu " + wc.dur(wc.mins(r) - nm)
                                            : nm < wc.mins(s) ? "do zachodu " + wc.dur(wc.mins(s) - nm) + " · złota godzina od " + Math.floor((wc.mins(s) - 60) / 60) + ":" + ("0" + (wc.mins(s) - 60) % 60).slice(-2)
                                            : "wschód jutro " + wc.hhmm(wc.days[1] ? wc.days[1].rise : ""))
                                    }
                                    return out
                                })() : []
                            CTxt { required property var modelData; text: modelData; size: 11.5; w: 500; color: desk.fg2 }
                        }
                    }
                }
            }
            EditSheet {
                anchors.fill: parent
                card: "weather"; title: "Pogoda"; open: wc.editing
                onDone: wc.editing = false
                groups: [
                    { label: "Wykres", key: "metric", opts: [["temp", "Temperatura"], ["rain", "Opady"], ["wind", "Wiatr"]] },
                    { label: "Oś czasu", key: "hours", opts: [[12, "12 h"], [24, "24 h"], [48, "48 h"]] },
                    { label: "Prognoza dni", key: "days", opts: [["show", "Pokaż"], ["hide", "Ukryj"]] },
                    { label: "Podpowiedzi w stopce", key: "insights", multi: true, min: 1,
                        opts: [["rain", "Opady"], ["run", "Okno na bieg"], ["wear", "Ubiór"], ["wind", "Porywy"], ["light", "Światło dnia"], ["tomorrow", "Jutro"]] }
                ]
            }
        }
    }

    // ================================================================== card system
    // Every card has the same anatomy on one grid, so the row reads as a set:
    //   caps header (y 0) · hero number (y 26, same baseline in every card) · visual band (y 102–224)
    //   · footer = the one sentence worth knowing (y 248). Hovering a part of a card explains it in the footer.
    readonly property real pad: 20
    readonly property real heroY: 26
    readonly property real bandY: 102
    readonly property real footY: desk.bentoH - 2 * desk.pad - 24
    // The weather card lives in its own small window over its place in the bento row (layer Bottom, above the
    // stage, no blur: the sky covers the card). Its live sky redraws 30 times a second; in the stage window
    // every frame re-rendered the whole stage and made Hyprland redo the blur under it.
    component WeatherSlot: Item {
        id: slot
        implicitWidth: desk.wxW; implicitHeight: desk.bentoH
        // top-left of the card on the monitor (the stage window is centred horizontally, desk.stageTop from the top)
        readonly property point at: {
            const w = slot.Window.width, lx = slot.parent ? slot.parent.x : 0
            const p = slot.mapToItem(null, 0, 0)
            return Qt.point(Math.round((desk.modelData.width - w) / 2 + p.x + 0 * lx), Math.round(desk.stageTop + p.y))
        }
        readonly property int pad: 40          // room for the card's shadow
        PanelWindow {
            screen: desk.modelData
            visible: desk.primary
            anchors { top: true; left: true }
            margins { left: slot.at.x - slot.pad; top: slot.at.y - slot.pad }
            implicitWidth: slot.width + 2 * slot.pad; implicitHeight: slot.height + 2 * slot.pad
            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Bottom
            WlrLayershell.namespace: "mist-desktop-card"
            color: "transparent"
            mask: Region { item: card }
            WeatherCard { id: card; x: slot.pad; y: slot.pad; width: slot.width; height: slot.height; origin: slot.at }
            Connections { target: Ui; function onDbgGrab(what, path) { if (what === "weather") card.grabToImage(r => r.saveToFile(path)) } }
        }
    }

    component Caps: CTxt { size: Theme.fsXs; w: 650; color: desk.faint; font.capitalization: Font.AllUppercase; font.letterSpacing: 1.3 }
    component Hero: CTxt { size: 58; w: 300; color: desk.fg; font.letterSpacing: -2.5; font.features: ({ "tnum": 1 }) }
    component Footer: Item {
        id: ft
        property string icon
        property string text
        property color c: desk.acc
        width: parent.width; height: 24
        Rectangle { width: parent.width; height: 1; color: desk.hairC; y: -11 }
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8
            Icon { anchors.verticalCenter: parent.verticalCenter; name: ft.icon; size: 14; color: ft.c; visible: !!ft.icon }
            CTxt { anchors.verticalCenter: parent.verticalCenter; text: ft.text; size: Theme.fsMd; w: 600; color: desk.fg; width: Math.min(implicitWidth, ft.width - 24) }
        }
    }

    // ---------------- body (Garmin): how ready am I today? One number, the day's energy curve, four vitals.
    component GarminCard: CardGlass {
        id: gcard
        readonly property var g: Data.garmin
        readonly property color acc: desk.level(g.ready)
        readonly property color bbCol: desk.level(g.bb)
        readonly property var series: g.bbSeries || []
        readonly property int hrvDelta: g.hrv && g.hrv7 ? g.hrv - g.hrv7 : 0
        readonly property real sleepH: (g.sleepSec || 0) / 3600
        readonly property string sleepTxt: g.sleepSec ? Math.floor(g.sleepSec / 3600) + ":" + Data.two(Math.floor(g.sleepSec % 3600 / 60)) : "–"
        readonly property string stressWord: g.stress === undefined || g.stress === null ? "" : g.stress <= 25 ? "spokój" : g.stress <= 50 ? "niski" : g.stress <= 75 ? "średni" : "wysoki"
        property string hot: ""   // part under the pointer -> the footer explains it
        property bool editing: false
        readonly property string heroKey: desk.opt("garmin", "hero")
        // hero: [header, number, qualifier (accent), small line, colour]
        readonly property var hero: heroKey === "bb" ? ["Body Battery", g.bb ?? "–", g.bb >= 50 ? "naładowana" : g.bb >= 25 ? "średnio" : "nisko", "na 100", bbCol]
            : heroKey === "sleep" ? ["Sen", sleepTxt, g.sleepScore >= 80 ? "dobry" : g.sleepScore >= 60 ? "przeciętny" : "słaby", g.sleepScore ? "ocena " + g.sleepScore : "", desk.level(g.sleepScore)]
            : heroKey === "hrv" ? ["HRV", g.hrv ?? "–", "ms", "średnia 7 dni " + (g.hrv7 ?? "–"), hrvDelta >= -4 ? desk.acc : desk.warn]
            : ["Gotowość", g.ready ?? "–", g.level || "", "na 100", acc]
        // every vital the strip can show: [key, value, unit, label, off?]
        readonly property var vitalDefs: ({
            sleep: ["sen", sleepTxt, "", "sen", g.sleepSec && sleepH < 6.5],
            hrv: ["hrv", g.hrv ?? "–", "ms", "HRV", hrvDelta <= -5],
            rhr: ["hr", g.rhr ?? "–", "bpm", "tętno", false],
            stress: ["st", g.stress ?? "–", "", "stres", g.stress > 50],
            steps: ["steps", g.steps !== undefined ? (g.steps >= 10000 ? (g.steps / 1000).toFixed(1).replace(".", ",") + "k" : desk.thousands(g.steps)) : "–", "", "kroki", false],
            bb: ["bb", g.bb ?? "–", "", "battery", g.bb < 25],
            ready: ["ready", g.ready ?? "–", "", "gotowość", g.ready < 40]
        })
        readonly property var vitals: desk.opt("garmin", "vitals").filter(k => vitalDefs[k]).map(k => vitalDefs[k])
        HoverHandler { id: gHov }
        // the most important deviation first, else what today allows
        readonly property var insight: {
            if (hot === "bb") return { icon: "bolt", c: bbCol, text: series.length > 1 ? "Body Battery " + (g.bb ?? "–") + " · 24 h: maks " + Math.max(...series) + ", min " + Math.min(...series) : "Body Battery " + (g.bb ?? "–") }
            if (hot === "sen") return { icon: "zzz", c: desk.level(g.sleepScore), text: "Sen " + sleepTxt.replace(":", " h ") + (g.sleepScore ? " · ocena " + g.sleepScore : "") }
            if (hot === "hrv") return { icon: "activity", c: hrvDelta >= 0 ? desk.acc : desk.warn, text: "HRV " + (g.hrv ?? "–") + " ms · średnia 7 dni " + (g.hrv7 ?? "–") }
            if (hot === "hr") return { icon: "heart", c: desk.acc, text: "Tętno spoczynkowe " + (g.rhr ?? "–") + " bpm" }
            if (hot === "steps") return { icon: "walk", c: desk.acc, text: "Kroki dziś: " + desk.thousands(g.steps) }
            if (hot === "ready") return { icon: "gauge", c: acc, text: "Gotowość " + (g.ready ?? "–") + " · " + (g.level || "") }
            if (hot === "st") return { icon: "leaf", c: g.stress > 50 ? desk.warn : desk.acc, text: "Stres " + (g.stress ?? "–") + " · " + stressWord }
            if (g.error) return { icon: "alert", c: desk.warn, text: "Brak połączenia z Garmin" }
            if (g.bb !== undefined && g.bb < 25) return { icon: "bolt", c: desk.bad, text: "Mało energii — odpuść intensywność" }
            if (hrvDelta <= -5) return { icon: "activity", c: desk.warn, text: "HRV " + Math.abs(hrvDelta) + " ms poniżej średniej 7 dni" }
            if (g.sleepSec && sleepH < 6.5) return { icon: "zzz", c: desk.warn, text: "Krótka noc: " + sleepTxt.replace(":", " h ") }
            if (g.stress > 50) return { icon: "leaf", c: desk.warn, text: "Podwyższony stres — zwolnij" }
            if (g.ready >= 70) return { icon: "check", c: desk.acc, text: "Gotowy na mocny trening" }
            return { icon: "leaf", c: desk.acc, text: "Dzień na spokojne tempo" }
        }
        implicitWidth: desk.colW; implicitHeight: desk.bentoH
        glow: desk.alpha(hero[4], 0.20); glowAt: Qt.point(0.18, 0.22); glowSize: 0.7
        padL: desk.pad; padR: desk.pad; padT: desk.pad; padB: desk.pad
        Item {
            width: parent.width; height: desk.bentoH - 2 * desk.pad
            RowLayout {
                width: parent.width
                Caps { text: gcard.hero[0]; Layout.fillWidth: true }
                CTxt { opacity: gHov.hovered && !gcard.editing ? 0 : 1; Behavior on opacity { NumberAnimation { duration: 160 } }
                    text: gcard.g.stale ? "dane z " + Data.hm(new Date(gcard.g.at * 1000)) : gcard.g.steps !== undefined ? desk.thousands(gcard.g.steps) + " kroków" : ""; size: Theme.fsXs; w: 550; color: desk.muted }
            }
            EditBtn { anchors.right: parent.right; y: -6; shown: gHov.hovered && !gcard.editing; onClicked: gcard.editing = true }
            Hero { id: gHero; y: desk.heroY; text: gcard.hero[1] }
            Column {
                anchors.left: gHero.right; anchors.leftMargin: 10; anchors.baseline: gHero.baseline; anchors.baselineOffset: -20
                CTxt { text: gcard.hero[2]; size: Theme.fsLg; w: 650; color: gcard.hero[4] }
                CTxt { text: gcard.hero[3]; size: Theme.fsXs; w: 500; color: desk.muted }
            }
            // Body Battery, the last 24 h: the shape of the day's energy
            Item {
                id: bbBand
                y: desk.bandY - 14; width: parent.width; height: 70
                HoverHandler { onHoveredChanged: gcard.hot = hovered ? "bb" : "" }
                CTxt { anchors.right: parent.right; text: "Body Battery"; size: Theme.fsXs; w: 550; color: desk.muted }
                Canvas {
                    y: 16; width: parent.width; height: 54
                    property var pts: gcard.series
                    property color c: gcard.bbCol
                    onPtsChanged: requestPaint()
                    onCChanged: requestPaint()
                    onWidthChanged: requestPaint()
                    onPaint: {
                        const x = getContext("2d"); x.reset()
                        const n = pts.length, w = width - 36, h = height - 8
                        if (n < 2) return
                        const X = i => i / (n - 1) * w + 2, Y = v => 4 + h - v / 100 * h
                        x.beginPath(); x.moveTo(X(0), Y(pts[0]))
                        for (let i = 1; i < n; i++) {
                            const xm = (X(i - 1) + X(i)) / 2
                            x.bezierCurveTo(xm, Y(pts[i - 1]), xm, Y(pts[i]), X(i), Y(pts[i]))
                        }
                        x.lineWidth = 2; x.lineCap = "round"; x.strokeStyle = desk.rgba(c, 0.95); x.stroke()
                        x.lineTo(X(n - 1), height); x.lineTo(X(0), height); x.closePath()
                        const g = x.createLinearGradient(0, 0, 0, height)
                        g.addColorStop(0, desk.rgba(c, 0.22)); g.addColorStop(1, desk.rgba(c, 0))
                        x.fillStyle = g; x.fill()
                        x.shadowColor = desk.rgba(c, 0.8); x.shadowBlur = 6
                        x.fillStyle = "#ffffff"; x.beginPath(); x.arc(X(n - 1), Y(pts[n - 1]), 3.2, 0, 2 * Math.PI); x.fill()
                        x.shadowBlur = 0
                        x.font = "600 13px 'SF Pro'"; x.fillStyle = desk.rgba(c, 1); x.textBaseline = "middle"
                        x.fillText("" + pts[n - 1], X(n - 1) + 9, Math.max(10, Math.min(height - 8, Y(pts[n - 1]))))
                    }
                }
            }
            // four vitals in one quiet strip; a dot flags the one that is off
            Row {
                y: desk.bandY + 72; width: parent.width
                Repeater {
                    model: gcard.vitals
                    Item {
                        required property var modelData
                        width: parent.width / Math.max(1, gcard.vitals.length); height: 44
                        HoverHandler { onHoveredChanged: gcard.hot = hovered ? modelData[0] : (gcard.hot === modelData[0] ? "" : gcard.hot) }
                        Row {
                            spacing: 2
                            CTxt { id: vv; text: modelData[1]; size: 18; w: 550; color: gcard.hot === modelData[0] ? desk.acc : desk.fg; font.features: ({ "tnum": 1 }) }
                            CTxt { text: modelData[2]; size: Theme.fsXs; w: 500; color: desk.muted; anchors.baseline: vv.baseline; visible: text !== "" }
                        }
                        Row {
                            y: 25; spacing: 5
                            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 5; height: 5; radius: 2.5; color: desk.warn; visible: !!modelData[4] }
                            CTxt { text: modelData[3]; size: Theme.fsXs; w: 550; color: desk.muted }
                        }
                    }
                }
            }
            Footer { visible: desk.opt("garmin", "foot") === "insight" || gcard.hot !== ""; y: desk.footY; icon: gcard.insight.icon; c: gcard.insight.c; text: gcard.insight.text }
            EditSheet {
                x: -desk.pad; y: -desk.pad; width: gcard.width; height: desk.bentoH
                card: "garmin"; title: "Garmin"; open: gcard.editing
                onDone: gcard.editing = false
                groups: [
                    { label: "Główna liczba", key: "hero", opts: [["ready", "Gotowość"], ["bb", "Body Battery"], ["sleep", "Sen"], ["hrv", "HRV"]] },
                    { label: "Wartości na dole", key: "vitals", multi: true, max: 4, min: 1,
                        opts: [["sleep", "Sen"], ["hrv", "HRV"], ["rhr", "Tętno"], ["stress", "Stres"], ["steps", "Kroki"], ["bb", "Battery"], ["ready", "Gotowość"]] },
                    { label: "Stopka", key: "foot", opts: [["insight", "Podpowiedź"], ["hide", "Tylko po najechaniu"]] }
                ]
            }
        }
    }

    // ---------------- training: the race countdown, the days left to it, today's session, the last run
    component RunningCard: CardGlass {
        id: rc
        readonly property var r: Data.running
        readonly property var g: Data.garmin
        readonly property bool race: r.raceDays !== null && r.raceDays !== undefined
        readonly property var week: r.week || [0, 0, 0, 0, 0, 0, 0]
        readonly property date raceDate: new Date(Data.now.getFullYear(), Data.now.getMonth(), Data.now.getDate() + (r.raceDays || 0))
        // the strip: today .. race day (max 10), else the current week Monday .. Sunday
        readonly property var strip: {
            const out = [], t = Data.now, ti = r.today ?? ((t.getDay() + 6) % 7)
            const n = race ? Math.min(10, r.raceDays + 1) : 7
            for (let i = 0; i < n; i++) {
                const off = race ? i : i - ti
                const d = new Date(t.getFullYear(), t.getMonth(), t.getDate() + off)
                const wi = ti + off
                out.push({ d: d, today: off === 0, past: off < 0, race: race && i === n - 1 && off === r.raceDays,
                    km: wi >= 0 && wi < 7 && off <= 0 ? (week[wi] || 0) : 0 })
            }
            return out
        }
        property bool editing: false
        readonly property var show: desk.opt("running", "show")
        readonly property string heroKey: race || desk.opt("running", "hero") !== "race" ? desk.opt("running", "hero") : "week"
        readonly property string foot: desk.opt("running", "foot")
        // hero: [header, number, unit (accent), small line]
        readonly property var hero: heroKey === "year" ? ["Rok " + Data.now.getFullYear(), desk.km1(r.yearKm), "km", (r.yearN || 0) + " " + desk.runsWord(r.yearN || 0)]
            : heroKey === "week" ? ["Tydzień " + (r.isoWeek || ""), desk.km1(r.weekKm), "km", "poprzedni " + desk.km1(r.prevKm) + " km"]
            : ["Start · 5 km", r.raceDays, r.raceDays === 1 ? "dzień" : "dni", "do startu"]
        HoverHandler { id: rHov }
        readonly property var insight: {
            if (foot === "year") return { icon: "trophy", c: desk.acc, text: "Rok " + Data.now.getFullYear() + ": " + desk.km1(r.yearKm) + " km · " + (r.yearN || 0) + " " + desk.runsWord(r.yearN || 0) }
            if (foot === "week") return { icon: "calendar", c: desk.acc, text: "Tydzień " + desk.km1(r.weekKm) + " km · poprzedni " + desk.km1(r.prevKm) }
            const L = r.last
            if (!L) return { icon: "run", c: desk.muted, text: "Brak biegów w historii" }
            return { icon: "run", c: desk.muted, text: "Ostatni bieg " + desk.km1(L.km) + " km · " + L.ago }
        }
        implicitWidth: desk.colW; implicitHeight: desk.bentoH
        glow: Qt.rgba(157/255, 189/255, 240/255, 0.16); glowAt: Qt.point(0.15, 0.2); glowSize: 0.75
        padL: desk.pad; padR: desk.pad; padT: desk.pad; padB: desk.pad
        Item {
            width: parent.width; height: desk.bentoH - 2 * desk.pad
            RowLayout {
                width: parent.width
                Caps { text: rc.hero[0]; Layout.fillWidth: true }
                CTxt {
                    opacity: rHov.hovered && !rc.editing ? 0 : 1; Behavior on opacity { NumberAnimation { duration: 160 } }
                    text: rc.heroKey === "year" || rc.foot === "year" ? (rc.race && rc.heroKey !== "race" ? "start za " + rc.r.raceDays + " d" : "tydzień " + (rc.r.isoWeek || "")) : "rok " + desk.km1(rc.r.yearKm) + " km"
                    size: Theme.fsXs; w: 550; color: desk.muted; visible: rc.r.yearKm !== undefined
                }
            }
            EditBtn { anchors.right: parent.right; y: -6; shown: rHov.hovered && !rc.editing; onClicked: rc.editing = true }
            Hero { id: rHero; y: desk.heroY; text: rc.hero[1] }
            Column {
                anchors.left: rHero.right; anchors.leftMargin: 10; anchors.baseline: rHero.baseline; anchors.baselineOffset: -20
                CTxt { text: rc.hero[2]; size: Theme.fsLg; w: 650; color: desk.acc }
                CTxt { text: rc.hero[3]; size: Theme.fsXs; w: 500; color: desk.muted }
            }
            Column {
                visible: !!rc.r.p5 && rc.show.includes("p5")
                anchors.right: parent.right; anchors.baseline: rHero.baseline; anchors.baselineOffset: -20
                CTxt { anchors.right: parent.right; text: rc.r.p5 || ""; size: 24; w: 400; color: desk.fg; font.features: ({ "tnum": 1 }) }
                CTxt { anchors.right: parent.right; text: "prognoza"; size: Theme.fsXs; w: 500; color: desk.muted }
            }
            // the days that are left, one cell each; today ringed, race day flagged
            Row {
                id: strip
                visible: rc.show.includes("strip")
                y: desk.bandY + 12; width: parent.width
                readonly property real cw: width / rc.strip.length
                Repeater {
                    model: rc.strip
                    Item {
                        required property var modelData
                        width: strip.cw; height: 40
                        CTxt {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: ["N", "P", "W", "Ś", "C", "P", "S"][modelData.d.getDay()]
                            size: Theme.fsXs; w: modelData.today ? 700 : 550
                            color: modelData.today ? desk.acc : modelData.race ? desk.warn : desk.muted
                        }
                        Rectangle {
                            anchors.horizontalCenter: parent.horizontalCenter; y: 18
                            width: 20; height: 20; radius: 10
                            color: modelData.km > 0 ? desk.alpha(desk.acc, 0.7) : modelData.race ? desk.alpha(desk.warn, 0.18) : "transparent"
                            border.width: modelData.today ? 1.5 : 0; border.color: desk.acc
                            Rectangle { anchors.centerIn: parent; width: 4; height: 4; radius: 2; color: Qt.rgba(1, 1, 1, 0.28); visible: !modelData.today && !modelData.race && modelData.km <= 0 }
                            Icon { anchors.centerIn: parent; name: "flag"; size: 12; color: desk.warn; visible: modelData.race }
                        }
                    }
                }
            }
            // today's session (from the Garmin plan)
            Item {
                visible: !!rc.g.workout && rc.show.includes("workout")
                y: strip.visible ? desk.bandY + 60 : desk.bandY + 10; width: parent.width; height: 64
                Caps { text: "Dziś"; color: desk.acc }
                CTxt { y: 16; width: parent.width; text: rc.g.workout || ""; size: Theme.fsLg + 1; w: 650; color: desk.fg }
                CTxt {
                    y: 38; width: parent.width; text: rc.g.workoutDetail || ""; size: Theme.fsXs; w: 500; color: desk.muted
                    wrapMode: Text.WordWrap; maximumLineCount: 2; lineHeight: 1.1
                }
            }
            Footer { visible: rc.foot !== "hide"; y: desk.footY; icon: rc.insight.icon; c: rc.insight.c; text: rc.insight.text }
            EditSheet {
                x: -desk.pad; y: -desk.pad; width: rc.width; height: desk.bentoH
                card: "running"; title: "Bieganie"; open: rc.editing
                onDone: rc.editing = false
                groups: [
                    { label: "Główna liczba", key: "hero", opts: [["race", "Do startu"], ["week", "Tydzień"], ["year", "Rok"]] },
                    { label: "Pokaż", key: "show", multi: true, opts: [["strip", "Dni do startu"], ["workout", "Trening dnia"], ["p5", "Prognoza 5 km"]] },
                    { label: "Stopka", key: "foot", opts: [["last", "Ostatni bieg"], ["week", "Tydzień"], ["year", "Rok"], ["hide", "Ukryj"]] }
                ]
            }
        }
    }
}
