import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"

// macOS Cmd+Tab (Super+Tab): one icon per app in a glass strip, most recent first, name under the
// selected one. Super+Tab / Super+Shift+Tab step (Hyprland binds -> ipc switcher next|prev),
// releasing Super switches. Enter / click switch, Esc cancels, Q closes the app's windows.
Scope {
    id: sw
    property bool open: false
    property int sel: 0
    property var apps: []          // [{ cls, name, icon, wins: [{ address, ws, wsName }] }]
    property int pendingStep: 0
    property string monitor: "DP-3"
    readonly property var screenObj: Quickshell.screens.find(s => s.name === monitor) || Quickshell.screens[0]

    function step(d) {
        if (!open) { pendingStep = d; clientsP.running = true; return }
        if (apps.length) sel = (sel + d + apps.length) % apps.length
    }
    function cancel() { open = false }
    function commit() {
        const a = apps[sel]
        open = false
        if (!a) return
        const w = a.wins[0], addr = "0x" + w.address.replace(/^0x/, "")
        if (w.wsName === "special:minimized") {
            const m = Hyprland.focusedMonitor, ws = m && m.activeWorkspace ? m.activeWorkspace.id : 1
            Quickshell.execDetached(["hyprctl", "eval", "MistRestore('" + addr + "', " + ws + ")"])
        } else Quickshell.execDetached(["hyprctl", "eval", "hl.dispatch(hl.dsp.focus({ window = \"address:" + addr + "\" }))"])
    }
    function quitApp() {
        const a = apps[sel]
        if (!a) return
        for (const w of a.wins) Quickshell.execDetached(["hyprctl", "eval", "hl.dispatch(hl.dsp.focus({ window = \"address:0x" + w.address.replace(/^0x/, "") + "\" })); hl.dispatch(hl.dsp.window.close())"])
        const rest = apps.slice(); rest.splice(sel, 1); apps = rest
        if (!apps.length) open = false; else sel = Math.min(sel, apps.length - 1)
    }

    Process {
        id: clientsP
        command: ["sh", "-c", "hyprctl monitors -j; echo '\x1e'; hyprctl clients -j"]
        stdout: StdioCollector {
            onStreamFinished: {
                let cs = []
                const parts = text.split("\x1e")
                try { sw.monitor = (JSON.parse(parts[0]).find(m => m.focused) || {}).name || sw.monitor; cs = JSON.parse(parts[1]) } catch (e) {}
                cs = cs.filter(c => c.mapped && c.class).sort((a, b) => a.focusHistoryID - b.focusHistoryID)
                const by = {}, out = []
                for (const c of cs) {
                    if (!by[c.class]) {
                        const e = DesktopEntries.heuristicLookup(c.class)
                        by[c.class] = { cls: c.class, name: e ? e.name : c.class, icon: e ? e.icon : c.class, wins: [] }
                        out.push(by[c.class])
                    }
                    by[c.class].wins.push({ address: c.address, ws: c.workspace.id, wsName: c.workspace.name })
                }
                sw.apps = out
                if (!out.length) return
                // first press goes to the previous app, like macOS
                sw.sel = out.length > 1 ? (sw.pendingStep > 0 ? 1 : out.length - 1) : 0
                Ui.close()
                sw.open = true
            }
        }
    }

    IpcHandler {
        target: "switcher"
        function next(): void { sw.step(1) }
        function prev(): void { sw.step(-1) }
    }

    PanelWindow {
        id: win
        visible: sw.open
        screen: sw.screenObj
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "mist-switcher"
        WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
        color: "transparent"
        onVisibleChanged: if (visible) keys.forceActiveFocus()

        // safety net: a missed Super release must not leave the overlay stuck
        Timer { running: sw.open; interval: 8000; onTriggered: sw.commit() }

        Item {
            id: keys
            anchors.fill: parent
            focus: true
            Keys.onReleased: e => { if (e.key === Qt.Key_Super_L || e.key === Qt.Key_Super_R || e.key === Qt.Key_Meta) sw.commit() }
            Keys.onPressed: e => {
                if (e.key === Qt.Key_Escape) sw.cancel()
                else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) sw.commit()
                else if (e.key === Qt.Key_Right || e.key === Qt.Key_Tab) sw.step(1)
                else if (e.key === Qt.Key_Left || e.key === Qt.Key_Backtab) sw.step(-1)
                else if (e.key === Qt.Key_Q) sw.quitApp()
                else return
                e.accepted = true
            }
            MouseArea { anchors.fill: parent; onClicked: sw.cancel() }
        }

        Glass {
            id: strip
            anchors.centerIn: parent
            width: row.width + 24; height: 136
            radius: 30; tint: 0.72
            Row {
                id: row
                anchors.centerIn: parent
                anchors.verticalCenterOffset: -9
                spacing: 4
                Repeater {
                    model: sw.apps
                    delegate: Rectangle {
                        id: cell
                        required property var modelData
                        required property int index
                        width: 108; height: 108; radius: 22
                        color: index === sw.sel ? Qt.rgba(1, 1, 1, 0.16) : "transparent"
                        border.width: index === sw.sel ? 1 : 0; border.color: Qt.rgba(1, 1, 1, 0.14)
                        AppIcon { anchors.centerIn: parent; icon: cell.modelData.icon; size: 84; bare: true }
                        // dot for apps hidden in the minimized drawer
                        Rectangle {
                            visible: cell.modelData.wins.every(w => w.wsName === "special:minimized")
                            width: 5; height: 5; radius: 3; color: Theme.t3
                            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 4 }
                        }
                        MouseArea {
                            anchors.fill: parent; hoverEnabled: true
                            onPositionChanged: sw.sel = cell.index
                            onClicked: { sw.sel = cell.index; sw.commit() }
                        }
                    }
                }
            }
            Txt {
                anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 9 }
                text: sw.apps[sw.sel] ? sw.apps[sw.sel].name : ""
                size: 13; w: 550; color: Theme.t1
            }
        }
    }
}
