import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Widgets
import Quickshell.Services.Pipewire
import Quickshell.Services.Mpris
import Quickshell.Bluetooth
import "components"

// macOS-style Control centre (bar: sliders icon; volume / network / Bluetooth icons open it on their page).
//   home       tiles: network · Bluetooth · phone | Do Not Disturb · night light, sound slider, now playing
//   sound      output + input volume and devices, per-app volume (grouped by application.name)
//   network    active connection details and live throughput
//   bluetooth  power switch, paired devices (click = connect / disconnect), discovery + pairing
// Pages slide in from the right; the glass sheet animates to the page height (window is masked to it).
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "control" && Ui.screenName === modelData.name
    anchors { top: true; right: true }
    margins { top: 30; right: 6 }
    exclusionMode: ExclusionMode.Ignore
    implicitWidth: 420; implicitHeight: 780
    mask: Region { item: sheet }
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-popup"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
    color: "transparent"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    // ------------------------------------------------------------ state
    readonly property string page: Ui.page
    function go(p) { Ui.page = p }

    readonly property var sink: Pipewire.defaultAudioSink
    readonly property var source: Pipewire.defaultAudioSource
    readonly property var audioNodes: Pipewire.nodes.values.filter(n => n.audio)
    readonly property var sinks: audioNodes.filter(n => n.isSink && !n.isStream)
    readonly property var sources: audioNodes.filter(n => !n.isSink && !n.isStream)
    readonly property var streams: audioNodes.filter(n => n.isStream)
    PwObjectTracker { objects: win.audioNodes }
    // playback streams grouped per application (Spotify opens two nodes); filter-chain streams have no application.name
    readonly property var apps: {
        const by = {}, out = []
        for (const n of streams) {
            const p = n.properties || {}
            const a = p["application.name"]
            if (!a || p["media.class"] !== "Stream/Output/Audio") continue
            if (!by[a]) {
                const e = DesktopEntries.heuristicLookup(p["application.process.binary"] || a)
                by[a] = { name: a, media: "", icon: p["application.icon-name"] || (e ? e.icon : a.toLowerCase()), nodes: [] }
                out.push(by[a])
            }
            if (!by[a].media && p["media.name"] && p["media.name"] !== a) by[a].media = p["media.name"]
            by[a].nodes.push(n)
        }
        return out
    }
    function nodeName(n) { return n ? n.nickname || n.description || n.name : "" }
    function sinkIcon(n) {
        const d = nodeName(n) + " " + (n ? n.name : "")
        return /headphone|headset|słuchawk/i.test(d) ? "headphones" : /hdmi|displayport/i.test(d) ? "tv" : "speaker"
    }
    function volIcon(a) { return !a || a.muted || a.volume === 0 ? "volumeOff" : a.volume < 0.4 ? "volLow" : "volume" }

    readonly property var bt: Bluetooth.defaultAdapter
    readonly property bool btOn: !!bt && bt.enabled
    readonly property var btPaired: Bluetooth.devices.values.filter(d => d.paired)
    readonly property var btConnected: btPaired.filter(d => d.connected)
    readonly property var btFound: Bluetooth.devices.values.filter(d => !d.paired && d.name && d.name.replace(/[-:]/g, "") !== d.address.replace(/[-:]/g, ""))
    function btIcon(d) {
        const i = d.icon || ""
        return /headset|headphone|audio-card/.test(i) ? "headphones" : /keyboard/.test(i) ? "keyboard" : /mouse/.test(i) ? "mouse"
             : /gaming|joystick/.test(i) ? "gamepad" : /phone/.test(i) ? "mobile" : /speaker|audio/.test(i) ? "speaker" : "bluetooth"
    }
    function btStatus(d) {
        if (d.state === BluetoothDeviceState.Connecting) return "Łączenie…"
        if (d.state === BluetoothDeviceState.Disconnecting) return "Rozłączanie…"
        if (!d.connected) return "Nie połączono"
        return d.batteryAvailable ? "Połączono · " + Math.round(d.battery * 100) + "%" : "Połączono"
    }
    // scan only while the Bluetooth page is open
    readonly property bool scanning: visible && page === "bluetooth" && btOn
    onScanningChanged: if (bt && bt.discovering !== scanning) bt.discovering = scanning

    readonly property var players: Mpris.players.values
    readonly property var player: players.find(p => p.isPlaying) || players.find(p => p.trackTitle) || null

    readonly property bool dnd: Notifs.dnd

    // network: nmcli device details + link speed
    property var net: ({ dev: "", type: "", conn: "", ip: "", gw: "", dns: "", speed: 0, up: false })
    property bool hasWifi: false
    Process {
        id: netQ
        command: ["sh", "-c", "nmcli -t -f GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS device show; "
            + "for f in /sys/class/net/*/speed; do d=${f%/speed}; echo \"SPEED:${d##*/}:$(cat $f 2>/dev/null)\"; done"]
        stdout: StdioCollector {
            onStreamFinished: {
                const devs = [], speed = {}
                let cur = null
                for (const l of this.text.split("\n")) {
                    if (l.startsWith("SPEED:")) { const p = l.split(":"); speed[p[1]] = parseInt(p[2]) || 0; continue }
                    const i = l.indexOf(":"); if (i < 0) continue
                    const k = l.slice(0, i), v = l.slice(i + 1)
                    if (k === "GENERAL.DEVICE") { cur = { dev: v, dns: [] }; devs.push(cur) }
                    else if (!cur) continue
                    else if (k === "GENERAL.TYPE") cur.type = v
                    else if (k === "GENERAL.STATE") cur.up = v.startsWith("100")
                    else if (k === "GENERAL.CONNECTION") cur.conn = v
                    else if (k.startsWith("IP4.ADDRESS")) cur.ip = cur.ip || v
                    else if (k === "IP4.GATEWAY") cur.gw = v
                    else if (k.startsWith("IP4.DNS")) cur.dns.push(v)
                }
                const real = devs.filter(d => (d.type === "ethernet" || d.type === "wifi") && !/^(veth|docker|br-)/.test(d.dev))
                win.hasWifi = real.some(d => d.type === "wifi")
                const d = real.find(d => d.up) || real[0]
                if (d) win.net = { dev: d.dev, type: d.type, conn: d.conn || "", ip: d.ip || "", gw: d.gw || "", dns: d.dns.join(", "), speed: speed[d.dev] || 0, up: !!d.up }
            }
        }
    }
    readonly property string netTitle: net.type === "wifi" ? (net.up ? net.conn : "Wi-Fi") : "Ethernet"
    readonly property string netSub: !net.dev ? "…" : !net.up ? "Rozłączono" : net.speed > 0 ? "Połączono · " + (net.speed >= 1000 ? net.speed / 1000 + " Gb/s" : net.speed + " Mb/s") : "Połączono"

    onVisibleChanged: if (visible) { netQ.running = true; sheet.forceActiveFocus() }
    // live download / upload speed for the network page (the bar is native now and samples on its own)
    Binding { target: Data; property: "barStats"; value: win.visible }
    Component.onCompleted: { netQ.running = true; if (scanning && !bt.discovering) bt.discovering = true }
    Component.onDestruction: if (bt && bt.discovering) bt.discovering = false

    // ------------------------------------------------------------ sheet
    Glass {
        id: sheet
        x: 10; y: 10; width: parent.width - 20
        height: (win.page === "" ? home.implicitHeight : detail.implicitHeight) + 32
        Behavior on height { NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }
        radius: Theme.rSheet; tint: 0.82
        clipContent: true
        focus: true
        Keys.onEscapePressed: win.page === "" ? Ui.close() : win.go("")

        // ---------------- home
        Column {
            id: home
            x: 16 + (win.page === "" ? 0 : -60); y: 16; width: sheet.width - 32
            opacity: win.page === "" ? 1 : 0
            visible: opacity > 0
            enabled: win.page === ""
            Behavior on x { NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }
            Behavior on opacity { NumberAnimation { duration: 180 } }
            spacing: 10
            readonly property real half: (width - 10) / 2

            Row {
                spacing: 10
                // connectivity
                Tile {
                    width: home.half; height: 150
                    Column {
                        anchors.fill: parent; anchors.margins: 8
                        LinkRow {
                            icon: win.net.type === "wifi" ? "wifi" : "ethernet"; on: win.net.up
                            title: win.netTitle; sub: win.net.up ? "Połączono" : win.netSub
                            onOpen: win.go("network"); onOrb: win.go("network")
                        }
                        LinkRow {
                            icon: win.btConnected.length ? "btOn" : "bluetooth"; on: win.btOn
                            title: "Bluetooth"
                            sub: !win.btOn ? "Wyłączony" : win.btConnected.length === 1 ? win.btConnected[0].name : win.btConnected.length ? win.btConnected.length + " urządzenia" : "Włączony"
                            onOpen: win.go("bluetooth"); onOrb: if (win.bt) win.bt.enabled = !win.bt.enabled
                        }
                        LinkRow {
                            icon: Phone.charging ? "charging" : "mobile"; on: Phone.online
                            title: Phone.name || "Telefon"
                            sub: Phone.online ? (Phone.battery >= 0 ? "Bateria " + Phone.battery + "%" : "Połączony") : Phone.present ? "Poza zasięgiem" : "KDE Connect"
                            onOpen: Ui.toggle("phone", win.modelData.name); onOrb: Ui.toggle("phone", win.modelData.name)
                        }
                    }
                }
                Column {
                    spacing: 10
                    ToggleTile {
                        width: home.half; height: 70
                        icon: "bellZ"; title: "Nie przeszkadzać"; sub: win.dnd ? "Włączone" : "Wyłączone"; on: win.dnd
                        onClicked: Notifs.setDnd(!Notifs.dnd)
                    }
                    ToggleTile {
                        width: home.half; height: 70
                        icon: "moon"; title: "Nocne światło"; sub: Ui.nightLight ? "Włączone" : "Wyłączone"; on: Ui.nightLight
                        onClicked: Ui.setNightLight(!Ui.nightLight)
                    }
                }
            }

            // sound
            Tile {
                width: home.width; height: 92
                hover: false
                Column {
                    x: 14; y: 12; width: parent.width - 28; spacing: 10
                    Item {
                        width: parent.width; height: 18
                        Txt { anchors.verticalCenter: parent.verticalCenter; text: "Dźwięk"; size: 13; w: 650 }
                        Row {
                            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                            opacity: sndHead.containsMouse ? 1 : 0.75
                            Txt { width: Math.min(implicitWidth, 190); anchors.verticalCenter: parent.verticalCenter; text: win.nodeName(win.sink); size: 11.5; color: Theme.t2 }
                            Icon { anchors.verticalCenter: parent.verticalCenter; name: "chevron"; size: 14; color: Theme.t2 }
                        }
                        MouseArea { id: sndHead; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.go("sound") }
                    }
                    VolSlider { width: parent.width; node: win.sink }
                }
            }

            // now playing
            Tile {
                visible: !!win.player
                width: home.width; height: 76
                hover: false
                ClippingRectangle {
                    id: art
                    x: 12; anchors.verticalCenter: parent.verticalCenter
                    width: 52; height: 52; radius: 12; color: Theme.fill2
                    Icon { anchors.centerIn: parent; name: "music"; size: 22; color: Theme.t3; visible: artImg.status !== Image.Ready }
                    Image { id: artImg; anchors.fill: parent; source: win.player ? win.player.trackArtUrl || "" : ""; fillMode: Image.PreserveAspectCrop; asynchronous: true; sourceSize: Qt.size(104, 104) }
                }
                Column {
                    anchors.left: art.right; anchors.leftMargin: 12; anchors.right: ctl.left; anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter; spacing: 2
                    Txt { width: parent.width; text: win.player ? win.player.trackTitle || win.player.identity : ""; size: 13; w: 600 }
                    Txt { width: parent.width; text: win.player ? win.player.trackArtist || win.player.identity : ""; size: 11.5; color: Theme.t2 }
                }
                Row {
                    id: ctl
                    anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Glyph { icon: "prevF"; onClicked: win.player.previous(); enabled: !!win.player && win.player.canGoPrevious }
                    Glyph { icon: win.player && win.player.isPlaying ? "pauseF" : "playF"; size: 22; onClicked: win.player.togglePlaying() }
                    Glyph { icon: "nextF"; onClicked: win.player.next(); enabled: !!win.player && win.player.canGoNext }
                }
            }

            // footer
            Item {
                width: home.width; height: 34
                Txt { x: 4; anchors.verticalCenter: parent.verticalCenter; text: Data.sys.uptime ? "Działa " + Data.sys.uptime : ""; size: 11; color: Theme.t3 }
                Row {
                    anchors.right: parent.right; spacing: 6
                    Btn { icon: "settings"; onClicked: Ui.toggle("wallpaper", win.modelData.name) }
                    Btn { icon: "lock"; onClicked: { Ui.close(); Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/mist-qs", "-p", Quickshell.shellDir, "ipc", "call", "lock", "lock"]) } }
                    Btn { icon: "power"; iconColor: Theme.danger; onClicked: Ui.toggle("session", win.modelData.name) }
                }
            }
        }

        // ---------------- detail pages
        Item {
            id: detail
            x: 16 + (win.page === "" ? 60 : 0); y: 16; width: sheet.width - 32
            implicitHeight: head.height + 12 + (body.item ? Math.min(body.item.implicitHeight, 620) : 0)
            opacity: win.page === "" ? 0 : 1
            visible: opacity > 0
            enabled: win.page !== ""
            Behavior on x { NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }
            Behavior on opacity { NumberAnimation { duration: 180 } }

            // the last page stays loaded while it slides out
            property string shown: ""
            Connections { target: win; function onPageChanged() { if (win.page !== "") detail.shown = win.page } }
            Component.onCompleted: shown = win.page

            Item {
                id: head
                width: parent.width; height: 34
                Btn { id: back; icon: "chevLeft"; iconSize: 17; onClicked: win.go("") }
                Txt {
                    anchors.left: back.right; anchors.leftMargin: 10; anchors.verticalCenter: parent.verticalCenter
                    text: detail.shown === "sound" ? "Dźwięk" : detail.shown === "network" ? "Sieć" : "Bluetooth"
                    size: 17; w: 650
                }
                Switch {
                    visible: detail.shown === "bluetooth"
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    on: win.btOn; onToggled: if (win.bt) win.bt.enabled = !win.bt.enabled
                }
                Btn {
                    visible: detail.shown === "sound"
                    anchors.right: parent.right
                    icon: "adjust"
                    onClicked: { Ui.close(); Quickshell.execDetached(["pavucontrol"]) }
                }
            }
            Flickable {
                y: head.height + 12
                width: parent.width; height: detail.implicitHeight - y
                contentHeight: body.item ? body.item.implicitHeight : 0
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Loader {
                    id: body
                    width: parent.width
                    sourceComponent: detail.shown === "sound" ? soundPage : detail.shown === "network" ? netPage : detail.shown === "bluetooth" ? btPage : null
                }
            }
        }
    }

    // ------------------------------------------------------------ pages
    Component {
        id: soundPage
        Column {
            spacing: 8
            Label { text: "Wyjście"; leftPadding: 4 }
            VolSlider { width: parent.width; node: win.sink }
            Group {
                Repeater {
                    model: win.sinks
                    PickRow {
                        required property var modelData
                        icon: win.sinkIcon(modelData); title: win.nodeName(modelData)
                        current: !!win.sink && modelData.id === win.sink.id
                        onClicked: Pipewire.preferredDefaultAudioSink = modelData
                    }
                }
            }
            Item { width: 1; height: 6 }
            Label { text: "Wejście"; leftPadding: 4 }
            VolSlider { width: parent.width; node: win.source; mic: true }
            Group {
                visible: win.sources.length > 1
                Repeater {
                    model: win.sources
                    PickRow {
                        required property var modelData
                        icon: /camera|webcam/i.test(win.nodeName(modelData)) ? "camera" : "mic"; title: win.nodeName(modelData)
                        current: !!win.source && modelData.id === win.source.id
                        onClicked: Pipewire.preferredDefaultAudioSource = modelData
                    }
                }
            }
            Item { width: 1; height: 6 }
            Label { text: "Aplikacje"; leftPadding: 4 }
            Group {
                Txt {
                    visible: win.apps.length === 0
                    width: parent.width; height: 44; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignHCenter
                    text: "Żadna aplikacja nie odtwarza dźwięku"; size: 12; color: Theme.t3
                }
                Repeater {
                    model: win.apps
                    Item {
                        id: ar
                        required property var modelData
                        readonly property var lead: modelData.nodes[0]
                        width: parent.width; height: 64
                        AppIcon { x: 8; anchors.verticalCenter: parent.verticalCenter; icon: ar.modelData.icon; size: 36 }
                        Column {
                            x: 54; width: parent.width - 62; anchors.verticalCenter: parent.verticalCenter; spacing: 6
                            Row {
                                width: parent.width; spacing: 6
                                Txt { id: an; text: ar.modelData.name; size: 12.5; w: 600; width: Math.min(implicitWidth, parent.width * 0.5) }
                                Txt { width: parent.width - an.width - 6; text: ar.modelData.media; size: 11.5; color: Theme.t3 }
                            }
                            VolSlider { width: parent.width; node: ar.lead; slim: true; group: ar.modelData.nodes }
                        }
                    }
                }
            }
        }
    }

    Component {
        id: netPage
        Column {
            spacing: 8
            Tile {
                width: parent.width; height: 64; hover: false
                Orb { id: no; x: 12; anchors.verticalCenter: parent.verticalCenter; icon: win.net.type === "wifi" ? "wifi" : "ethernet"; on: win.net.up; interactive: false }
                Column {
                    anchors.left: no.right; anchors.leftMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 1
                    Txt { text: win.netTitle; size: 13.5; w: 650 }
                    Txt { text: win.netSub; size: 11.5; color: win.net.up ? Theme.green : Theme.t3 }
                }
            }
            Item { width: 1; height: 4 }
            Label { text: "Ruch"; leftPadding: 4 }
            Row {
                width: parent.width; spacing: 8
                Repeater {
                    model: [{ i: "down", t: "Pobieranie", v: Data.rxRate, c: Theme.accent }, { i: "up", t: "Wysyłanie", v: Data.txRate, c: Theme.green }]
                    Tile {
                        required property var modelData
                        width: (parent.width - 8) / 2; height: 62; hover: false
                        Column {
                            x: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                            Row {
                                spacing: 5
                                Icon { name: modelData.i; size: 13; color: modelData.c; anchors.verticalCenter: parent.verticalCenter }
                                Txt { text: modelData.t; size: 11; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
                            }
                            Txt { text: Data.bytes(modelData.v) + "/s"; size: 17; w: 600; font.family: Theme.fontRounded; font.features: ({ "tnum": 1 }) }
                        }
                    }
                }
            }
            Item { width: 1; height: 4 }
            Label { text: "Szczegóły"; leftPadding: 4 }
            Group {
                Repeater {
                    model: [["Połączenie", win.net.conn], ["Interfejs", win.net.dev], ["Adres IP", win.net.ip], ["Brama", win.net.gw], ["DNS", win.net.dns]].filter(r => r[1])
                    Item {
                        required property var modelData
                        width: parent.width; height: 36
                        Txt { x: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData[0]; size: 12; color: Theme.t2 }
                        Txt {
                            anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, parent.width - 120); horizontalAlignment: Text.AlignRight
                            text: modelData[1]; size: 12; w: 550
                        }
                    }
                }
            }
            Group {
                visible: win.hasWifi
                PickRow { icon: "wifi"; title: "Wi-Fi"; current: false; onClicked: Quickshell.execDetached(["sh", "-c", "[ \"$(nmcli radio wifi)\" = enabled ] && nmcli radio wifi off || nmcli radio wifi on"]) }
            }
        }
    }

    Component {
        id: btPage
        Column {
            spacing: 8
            Txt {
                visible: !win.btOn
                width: parent.width; topPadding: 24; bottomPadding: 24; horizontalAlignment: Text.AlignHCenter
                text: "Bluetooth jest wyłączony"; size: 13; color: Theme.t3
            }
            Label { visible: win.btOn && win.btPaired.length > 0; text: "Moje urządzenia"; leftPadding: 4 }
            Group {
                visible: win.btOn && win.btPaired.length > 0
                Repeater {
                    model: win.btPaired
                    DevRow {
                        required property var modelData
                        icon: win.btIcon(modelData); title: modelData.name; sub: win.btStatus(modelData); on: modelData.connected
                        busy: modelData.state === BluetoothDeviceState.Connecting || modelData.state === BluetoothDeviceState.Disconnecting
                        onClicked: modelData.connected ? modelData.disconnect() : modelData.connect()
                    }
                }
            }
            Item { width: 1; height: 4; visible: win.btOn }
            Item {
                visible: win.btOn
                width: parent.width; height: 16
                Label { text: "Inne urządzenia"; leftPadding: 4; anchors.verticalCenter: parent.verticalCenter }
                Txt {
                    anchors.right: parent.right; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter
                    text: "Wyszukiwanie…"; size: 11; color: Theme.t3
                    visible: !!win.bt && win.bt.discovering
                    SequentialAnimation on opacity { running: parent.visible; loops: Animation.Infinite; NumberAnimation { to: 0.35; duration: 700 } NumberAnimation { to: 1; duration: 700 } }
                }
            }
            Group {
                visible: win.btOn
                Txt {
                    visible: win.btFound.length === 0
                    width: parent.width; height: 44; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignHCenter
                    text: "Nie znaleziono urządzeń w pobliżu"; size: 12; color: Theme.t3
                }
                Repeater {
                    model: win.btFound
                    DevRow {
                        required property var modelData
                        icon: win.btIcon(modelData); title: modelData.name
                        sub: modelData.pairing ? "Parowanie…" : "Kliknij, aby sparować"
                        busy: modelData.pairing
                        onClicked: { modelData.trusted = true; modelData.pair() }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ parts
    // soft tile of the home grid
    // soft glass lens of the home grid (white-tinted LiquidGlass, no shadow)
    component Tile: Item {
        id: tl
        property bool hover: true
        default property alias content: inner.data
        LiquidGlass {
            anchors.fill: parent; radius: 22
            tone: "white"; tint: tl.hover && th.hovered ? 0.095 : 0.06
            shadow: false; shine: 0.6
            Behavior on tint { NumberAnimation { duration: 120 } }
        }
        HoverHandler { id: th }
        Item { id: inner; anchors.fill: parent }
    }
    // grouped rows on an inset background (detail pages)
    component Group: Item {
        default property alias content: col.data
        width: parent ? parent.width : 0
        height: col.implicitHeight + 8
        LiquidGlass { anchors.fill: parent; radius: 18; tint: 0.22; tone: Qt.rgba(4/255, 7/255, 14/255, 1); shadow: false; shine: 0.45 }
        Column { id: col; x: 4; y: 4; width: parent.width - 8 }
    }
    // round toggle: blue with a white glyph when on (macOS)
    component Orb: Rectangle {
        id: o
        property string icon
        property bool on: false
        property bool interactive: true
        signal clicked()
        width: 34; height: 34; radius: 17
        color: on ? Theme.accent : om.containsMouse && interactive ? Qt.rgba(1, 1, 1, 0.18) : Theme.fill2
        Behavior on color { ColorAnimation { duration: 150 } }
        scale: om.pressed && interactive ? 0.9 : 1
        Behavior on scale { NumberAnimation { duration: 120 } }
        Icon { anchors.centerIn: parent; name: o.icon; size: 16; color: o.on ? "white" : Theme.t1 }
        MouseArea { id: om; anchors.fill: parent; enabled: o.interactive; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: o.clicked() }
    }
    // connectivity row: orb (quick toggle) + title / status (opens the page)
    component LinkRow: Item {
        id: lr
        property string icon
        property bool on
        property string title
        property string sub
        signal open()
        signal orb()
        width: parent.width; height: 44.6
        Rectangle { anchors.fill: parent; radius: 13; color: lm.containsMouse ? Theme.fill : "transparent" }
        MouseArea { id: lm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: lr.open() }
        Orb { id: lo; x: 4; anchors.verticalCenter: parent.verticalCenter; icon: lr.icon; on: lr.on; onClicked: lr.orb() }
        Column {
            anchors.left: lo.right; anchors.leftMargin: 9; anchors.right: parent.right; anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            Txt { width: parent.width; text: lr.title; size: 12.5; w: 600 }
            Txt { width: parent.width; text: lr.sub; size: 11; color: Theme.t2 }
        }
    }
    // whole-tile toggle (Do Not Disturb, night light)
    component ToggleTile: Tile {
        id: tt
        property string icon
        property string title
        property string sub
        property bool on
        signal clicked()
        Orb { id: to; x: 10; anchors.verticalCenter: parent.verticalCenter; icon: tt.icon; on: tt.on; interactive: false }
        Column {
            anchors.left: to.right; anchors.leftMargin: 8; anchors.right: parent.right; anchors.rightMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            Txt { width: parent.width; text: tt.title; size: 12; w: 600 }
            Txt { width: parent.width; text: tt.sub; size: 11; color: Theme.t2 }
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: tt.clicked() }
    }
    // device / choice row with a check mark on the current one
    component PickRow: Item {
        id: pr
        property string icon
        property string title
        property bool current
        signal clicked()
        width: parent.width; height: 40
        Rectangle { anchors.fill: parent; radius: 12; color: pm.containsMouse ? Theme.fill : "transparent" }
        Rectangle {
            id: pi; x: 6; anchors.verticalCenter: parent.verticalCenter
            width: 28; height: 28; radius: 14
            color: pr.current ? Theme.accent : Theme.fill2
            Icon { anchors.centerIn: parent; name: pr.icon; size: 14; color: pr.current ? "white" : Theme.t2 }
        }
        Txt {
            anchors.left: pi.right; anchors.leftMargin: 10; anchors.right: tick.left; anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            text: pr.title; size: 12.5; w: pr.current ? 600 : 450
        }
        Icon { id: tick; anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; name: "tick"; size: 15; color: Theme.accent; visible: pr.current }
        MouseArea { id: pm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: pr.clicked() }
    }
    // Bluetooth device row
    component DevRow: Item {
        id: dr
        property string icon
        property string title
        property string sub
        property bool on
        property bool busy
        signal clicked()
        width: parent.width; height: 48
        Rectangle { anchors.fill: parent; radius: 12; color: dm.containsMouse ? Theme.fill : "transparent" }
        Rectangle {
            id: di; x: 6; anchors.verticalCenter: parent.verticalCenter
            width: 32; height: 32; radius: 16
            color: dr.on ? Theme.accent : Theme.fill2
            Icon { anchors.centerIn: parent; name: dr.icon; size: 15; color: dr.on ? "white" : Theme.t2 }
            SequentialAnimation on opacity { running: dr.busy; loops: Animation.Infinite; NumberAnimation { to: 0.4; duration: 500 } NumberAnimation { to: 1; duration: 500 } onStopped: di.opacity = 1 }
        }
        Column {
            anchors.left: di.right; anchors.leftMargin: 10; anchors.right: parent.right; anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            Txt { width: parent.width; text: dr.title; size: 12.5; w: dr.on ? 600 : 500 }
            Txt { width: parent.width; text: dr.sub; size: 11; color: dr.on ? Theme.green : Theme.t3 }
        }
        MouseArea { id: dm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: dr.clicked() }
    }
    // pill switch
    component Switch: Rectangle {
        id: sw
        property bool on
        signal toggled()
        width: 44; height: 26; radius: 13
        color: on ? Theme.green : Theme.fill2
        Behavior on color { ColorAnimation { duration: 150 } }
        Rectangle {
            x: sw.on ? sw.width - width - 3 : 3; anchors.verticalCenter: parent.verticalCenter
            width: 20; height: 20; radius: 10; color: "white"
            Behavior on x { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: sw.toggled() }
    }
    // bare media glyph button
    component Glyph: Item {
        id: gl
        property string icon
        property real size: 18
        signal clicked()
        width: 32; height: 32
        opacity: enabled ? (gm.containsMouse ? 1 : 0.85) : 0.3
        Icon { anchors.centerIn: parent; name: gl.icon; size: gl.size; color: Theme.t1; scale: gm.pressed ? 0.85 : 1; Behavior on scale { NumberAnimation { duration: 100 } } }
        MouseArea { id: gm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: gl.clicked() }
    }
    // fat volume slider for a Pipewire node (icon inside toggles mute; wheel works); group = nodes moved together
    component VolSlider: Item {
        id: vs
        property var node
        property var group: node ? [node] : []
        property bool mic: false
        property bool slim: false
        readonly property var a: node ? node.audio : null
        readonly property real value: a ? Math.min(1, a.volume) : 0
        readonly property bool muted: !a || a.muted
        height: slim ? 24 : 30
        opacity: a ? 1 : 0.4
        function set(v) {
            v = Math.max(0, Math.min(1, Math.round(v * 100) / 100))
            for (const n of group) if (n && n.audio) { if (n.audio.muted && v > 0) n.audio.muted = false; n.audio.volume = v }
        }
        function mute() { const m = !vs.muted; for (const n of group) if (n && n.audio) n.audio.muted = m }
        Rectangle {
            id: track
            anchors.fill: parent; radius: height / 2; color: Theme.fill2
            Rectangle {
                width: Math.max(track.height, track.width * vs.value); height: parent.height; radius: height / 2
                color: vs.muted ? Theme.t3 : Theme.t1
                Behavior on width { enabled: !vm.pressed; NumberAnimation { duration: 120 } }
            }
            Icon {
                x: (track.height - width) / 2 + 1; anchors.verticalCenter: parent.verticalCenter
                name: vs.mic ? (vs.muted ? "micOff" : "mic") : vs.muted ? "volumeOff" : vs.value < 0.4 ? "volLow" : "volume"
                size: vs.slim ? 12 : 15; color: Theme.ink
            }
            Txt {
                anchors.right: parent.right; anchors.rightMargin: vs.slim ? 9 : 12; anchors.verticalCenter: parent.verticalCenter
                text: vs.muted ? "wycisz." : Math.round(vs.value * 100) + "%"
                size: vs.slim ? 10.5 : 11.5; w: 600; font.features: ({ "tnum": 1 })
                color: vs.value > 0.88 && !vs.muted ? Theme.ink : Theme.t2
            }
        }
        MouseArea {
            id: vm
            anchors.fill: parent; cursorShape: Qt.PointingHandCursor
            function at(m) { vs.set(m.x / width) }
            onPressed: m => { if (m.x < track.height) vs.mute(); else at(m) }
            onPositionChanged: m => { if (pressed && m.x >= track.height) at(m) }
            onWheel: w => vs.set(vs.value + (w.angleDelta.y > 0 ? 0.05 : -0.05))
        }
    }
}
