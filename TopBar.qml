import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Widgets
import Quickshell.Services.SystemTray
import Quickshell.Services.Pipewire
import Quickshell.Bluetooth
import "components"

// Top bar: fully transparent, no capsules, just icons and text (centre stays free for the Dynamic Island).
//   left   [Start | workspaces]  [active window]
//   right  [CPU · RAM · download]  [tray]  [phone · Bluetooth · network · volume · notifications]  [power]
// Control centre / notification centre / session are Mist popups.
PanelWindow {
    id: bar
    required property var modelData
    screen: modelData
    anchors { top: true; left: true; right: true }
    implicitHeight: 34
    exclusiveZone: 34
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "mist-bar"
    color: "transparent"

    readonly property bool primary: modelData.name === "DP-3"
    Component.onCompleted: if (primary) Data.barStats = true
    readonly property var hmon: Hyprland.monitorFor(modelData)
    readonly property int activeWs: hmon && hmon.activeWorkspace ? hmon.activeWorkspace.id : 1
    readonly property bool bare: !hmon || !hmon.activeWorkspace || hmon.activeWorkspace.toplevels.values.length === 0
    function hypr(lua) { Quickshell.execDetached(["hyprctl", "eval", lua]) }
    function goWs(id) { hypr("hl.dispatch(hl.dsp.focus({ workspace = \"" + id + "\" }))") }

    PwObjectTracker { objects: [Pipewire.defaultAudioSink] }
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property real vol: sink && sink.audio ? sink.audio.volume : 0
    readonly property bool muted: !sink || !sink.audio || sink.audio.muted

    // network type (Net.qml: NetworkManager events)
    readonly property string net: Net.type

    // Do Not Disturb of Mist's notification server
    readonly property bool dnd: Notifs.dnd

    readonly property bool btOn: !!Bluetooth.defaultAdapter && Bluetooth.defaultAdapter.enabled
    readonly property var btDevs: Bluetooth.devices.values.filter(d => d.connected)

    // workspaces of this monitor (ids only, so the delegates survive a switch and the pill can animate)
    readonly property var wsIds: {
        // numbered ones in order, then named ones (e.g. "gaming", negative ids); special workspaces stay out
        const ws = Hyprland.workspaces.values.filter(w => w.monitor && w.monitor.name === bar.modelData.name && (w.id > 0 || !w.name.startsWith("special")))
        return ws.map(w => w.id).sort((a, b) => a > 0 && b > 0 ? a - b : a > 0 ? -1 : b > 0 ? 1 : a - b)
    }
    function wsCount(id) { const w = Hyprland.workspaces.values.find(x => x.id === id); return w ? w.toplevels.values.length : 0 }

    readonly property var focused: ToplevelManager.activeToplevel
    readonly property bool showWin: !bare && !!focused && (focused.screens || []).some(s => s.name === modelData.name)
    readonly property var focusedEntry: { DesktopEntries.applications.values; return focused ? DesktopEntries.byId(focused.appId) || DesktopEntries.heuristicLookup(focused.appId) : null }

    // ================================================================ left (macOS menu bar)
    //  Start (app grid) · active app name in bold · its window title, quiet; wheel = switch workspace
    Row {
        x: 10
        anchors.verticalCenter: parent.verticalCenter
        spacing: 14
        WheelHandler { onWheel: e => bar.goWs(e.angleDelta.y > 0 ? "m-1" : "m+1") }

        BarItem { icon: "apps"; iconSize: 16; tint: Theme.t1; implicitWidth: 26; onClicked: Ui.toggle("start", bar.modelData.name) }
        Txt {
            anchors.verticalCenter: parent.verticalCenter
            text: bar.showWin ? (bar.focusedEntry ? bar.focusedEntry.name : (bar.focused ? bar.focused.appId : "")) : "Pulpit"
            size: 13; w: 700; color: Theme.t1
        }
        Txt {
            visible: bar.showWin
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 420)
            text: bar.focused ? (bar.focused.title || "") : ""
            size: 13; w: 500; color: Theme.t2
        }
    }

    // ================================================================ right
    Row {
        anchors.right: parent.right; anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 0

        // system stats: quiet by default (one icon); a value shows on its own only when it is worth a look
        // (CPU > 40 %, RAM > 75 %, download > 1 MB/s); hovering the group shows all three
        BarCapsule {
            id: stats
            visible: bar.primary
            pad: 0; spacing: 2
            readonly property bool open: statsHov.hovered
            readonly property bool cpuHot: Data.cpuPct > 40
            readonly property bool memHot: Data.memTotal > 0 && Data.memUsed / Data.memTotal > 0.75
            readonly property bool netHot: Data.rxRate > 1048576
            HoverHandler { id: statsHov }
            BarItem {
                visible: !stats.open && !stats.cpuHot && !stats.memHot && !stats.netHot
                icon: "activity"; tint: Theme.t2
                onClicked: Quickshell.execDetached(["kitty", "-e", "btop"])
            }
            BarItem {
                visible: stats.open || stats.cpuHot
                icon: "cpu"; text: Data.cpuPct + "%"
                tint: Data.cpuPct > 85 ? Theme.danger : Data.cpuPct > 55 ? Theme.yellow : Theme.t2
                onClicked: Quickshell.execDetached(["kitty", "-e", "btop"])
            }
            BarItem {
                visible: stats.open || stats.memHot
                icon: "ram"; text: Data.memUsed.toFixed(1).replace(".", ",") + " GB"
                tint: Data.memTotal > 0 && Data.memUsed / Data.memTotal > 0.85 ? Theme.danger : Theme.t2
                onClicked: Quickshell.execDetached(["kitty", "-e", "btop"])
            }
            BarItem {
                visible: stats.open || stats.netHot
                icon: "down"; text: Data.bytes(Data.rxRate) + "/s"; tint: Theme.t2
                onClicked: Quickshell.execDetached(["kitty", "-e", "btop"])
            }
        }

        BarCapsule {
            visible: SystemTray.items.values.length > 0
            pad: 0; spacing: 2
            Repeater {
                model: SystemTray.items
                Item {
                    id: ti
                    required property var modelData
                    implicitWidth: 26; implicitHeight: 24
                                        IconImage { anchors.centerIn: parent; implicitSize: 16; source: ti.modelData.icon; opacity: tma.containsMouse ? 1 : 0.85 }
                    QsMenuAnchor { id: menu; menu: ti.modelData.menu; anchor.window: bar }
                    MouseArea {
                        id: tma
                        anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
                        onClicked: m => {
                            if (m.button === Qt.RightButton || ti.modelData.onlyMenu) {
                                if (!ti.modelData.hasMenu) return
                                const p = ti.mapToItem(bar.contentItem, 0, 0)
                                menu.anchor.rect.x = p.x; menu.anchor.rect.y = 32; menu.anchor.rect.width = ti.width; menu.anchor.rect.height = 1
                                menu.open()
                            } else if (m.button === Qt.MiddleButton) ti.modelData.secondaryActivate()
                            else ti.modelData.activate()
                        }
                    }
                }
            }
        }

        BarCapsule {
            pad: 0; spacing: 2
            // phone (KDE Connect): battery while linked, yellow while it asks to pair; opens the phone panel, middle click rings it
            BarItem {
                visible: bar.primary && (Phone.online || Phone.pairRequested)
                icon: Phone.pairRequested ? "mobileShare" : Phone.charging ? "charging" : "mobile"
                tint: Phone.pairRequested ? Theme.yellow : Phone.batteryColor()
                text: Phone.battery >= 0 && (Phone.battery <= 30 || Phone.charging) ? Phone.battery + "%" : ""
                onClicked: m => { if (m.button === Qt.MiddleButton) Phone.ring(); else Ui.toggle("phone", bar.modelData.name) }
            }
            BarItem {
                visible: bar.btOn
                icon: bar.btDevs.length ? "btOn" : "bluetooth"
                tint: bar.btDevs.length ? Theme.accent : Theme.t2
                text: bar.btDevs.length && bar.btDevs[0].batteryAvailable && bar.btDevs[0].battery <= 0.3 ? Math.round(bar.btDevs[0].battery * 100) + "%" : ""
                onClicked: Ui.control("bluetooth", bar.modelData.name)
            }
            BarItem {
                icon: bar.net === "ethernet" ? "ethernet" : bar.net === "wifi" ? "wifi" : "worldOff"
                tint: bar.net === "none" ? Theme.danger : Theme.t1
                onClicked: Ui.control("network", bar.modelData.name)
            }
            BarItem {
                icon: bar.muted || bar.vol === 0 ? "volumeOff" : bar.vol < 0.4 ? "volLow" : "volume"
                tint: bar.muted ? Theme.t3 : Theme.t1
                onClicked: m => { if (m.button === Qt.MiddleButton) { if (bar.sink && bar.sink.audio) bar.sink.audio.muted = !bar.sink.audio.muted } else Ui.control("sound", bar.modelData.name) }
                onScrolled: w => {
                    const a = bar.sink && bar.sink.audio
                    if (!a) return
                    if (a.muted && w.angleDelta.y > 0) a.muted = false
                    a.volume = Math.max(0, Math.min(1, Math.round((a.volume + (w.angleDelta.y > 0 ? 0.05 : -0.05)) * 100) / 100))
                }
            }
            BarItem {
                icon: bar.dnd ? "bellOff" : "bell"
                tint: bar.dnd ? Theme.t3 : Theme.t1
                onClicked: m => { if (m.button === Qt.RightButton) Notifs.setDnd(!Notifs.dnd); else Ui.toggle("notifications", bar.modelData.name) }
            }
            BarItem {
                icon: "adjust"
                tint: Ui.popup === "control" ? Theme.accent : Theme.t1
                onClicked: Ui.control("", bar.modelData.name)
            }
        }

        BarCapsule {
            pad: 0; spacing: 0
            BarItem { icon: "power"; tint: Theme.danger; implicitWidth: 24; onClicked: Ui.toggle("session", bar.modelData.name) }
        }
    }
}
