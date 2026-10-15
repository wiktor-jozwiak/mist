import QtQuick
import QtQuick.Effects
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Widgets
import Quickshell.Hyprland
import "components"

// Dock — port of the Windows mist-shell dock.rs (Seelen "weg" look): iOS-like light milky glass slab,
// radius 26, 46 px icons, padding 8, gap 8; [Start] | pinned + running | [Kosz].
// Shown on the bare desktop; when windows are open it slides away and comes back at the bottom edge.
PanelWindow {
    id: dock
    required property var modelData
    screen: modelData
    visible: modelData.name === "DP-3"
    anchors.bottom: true
    exclusionMode: ExclusionMode.Ignore
    implicitWidth: slab.width + 88
    implicitHeight: 62 + 46 + 30          // slab + room for magnified icons, the label, the shadow
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "mist-dock"
    color: "transparent"
    mask: Region { item: shown ? hitArea : edge }

    readonly property int size: 46
    readonly property int pad: 8
    readonly property int gap: 8

    // ---------------------------------------------------------------- model
    FileView { id: cfg; path: Quickshell.env("HOME") + "/.config/mist/apps.json"; watchChanges: true; onFileChanged: reload() }
    readonly property var conf: { try { return JSON.parse(cfg.text()) } catch (e) { return { dock: [] } } }
    function savePins(list) { const c = conf; c.dock = list; cfg.setText(JSON.stringify(c, null, 2)) }

    function entryFor(appId) {
        DesktopEntries.applications.values
        return DesktopEntries.byId(appId) || DesktopEntries.heuristicLookup(appId)
    }
    readonly property var items: {
        const tops = ToplevelManager.toplevels.values
        // one desktop-entry lookup per window, grouped by entry id (heuristicLookup is not cheap)
        const groups = new Map()
        for (const t of tops) {
            const e = entryFor(t.appId)
            const key = e ? e.id : t.appId
            if (!groups.has(key)) groups.set(key, { entry: e, wins: [] })
            groups.get(key).wins.push(t)
        }
        const out = [], seen = new Set()
        for (const id of (conf.dock || [])) {
            const e = entryFor(id)
            if (!e || seen.has(e.id)) continue
            seen.add(e.id)
            out.push({ id: id, entry: e, wins: groups.has(e.id) ? groups.get(e.id).wins : [], pinned: true })
        }
        for (const [key, g] of groups) {
            if (seen.has(key)) continue
            seen.add(key)
            out.push({ id: key, entry: g.entry, wins: g.wins, pinned: false })
        }
        return out
    }

    // ---------------------------------------------------------------- auto-hide
    readonly property var hmon: Hyprland.monitorFor(modelData)
    readonly property bool bare: !hmon || !hmon.activeWorkspace || hmon.activeWorkspace.toplevels.values.length === 0
    property bool hovered: false
    property bool menuOpen: false
    readonly property bool shown: bare || hovered || menuOpen || Ui.dbgDock || Ui.popup === "start"

    // ---------------------------------------------------------------- minimize (Hyprland has none)
    // an app asks to minimize (Zen's yellow dot, xdg set_minimized) -> park it on special:minimized
    Connections {
        target: Hyprland
        function onRawEvent(ev) {
            if (ev.name === "minimized" && ev.data.endsWith(",1")) {
                const a = ev.data.split(",")[0]
                Quickshell.execDetached(["hyprctl", "eval", "MistMinimize('" + (a.startsWith("0x") ? a : "0x" + a) + "')"])
            }
        }
    }
    function hyprOf(w) { return Hyprland.toplevels.values.find(h => h.wayland === w) }
    function isMinimized(w) { const h = hyprOf(w); return !!h && !!h.workspace && h.workspace.name === "special:minimized" }
    function restore(w) {
        const h = hyprOf(w)
        const ws = Hyprland.focusedWorkspace ? Hyprland.focusedWorkspace.id : 1
        if (h) Quickshell.execDetached(["hyprctl", "eval", "MistRestore('0x" + h.address.replace(/^0x/, "") + "', " + ws + ")"])
    }
    Timer { id: hideT; interval: 400; onTriggered: dock.hovered = false }
    Connections { target: Ui; function onDbgGrab(what, path) { if (what === "dock") slabBox.grabToImage(r => r.saveToFile(path)) } }
    onBareChanged: if (!bare) hideT.restart()

    Item {
        id: edge                                   // 2 px strip at the screen edge that reveals the dock
        anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        width: slab.width; height: 2
        HoverHandler { onHoveredChanged: if (hovered) showT.restart(); else showT.stop() }
        Timer { id: showT; interval: 80; onTriggered: dock.hovered = true }
    }
    Item {
        id: hitArea
        anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        width: slab.width; height: slab.height + 8 + 6
        HoverHandler { onHoveredChanged: if (hovered) { hideT.stop(); dock.hovered = true } else hideT.restart() }
    }

    // ---------------------------------------------------------------- slab
    Item {
        id: slabBox
        anchors.horizontalCenter: parent.horizontalCenter
        y: dock.shown ? dock.height - slab.height - 8 : dock.height + 4
        width: slab.width; height: slab.height
        Behavior on y { NumberAnimation { duration: 240; easing.type: Easing.OutCubic } }

        // liquid glass slab: lighter slate tone than the cards, stronger light
        LiquidGlass {
            id: slab
            width: row.implicitWidth + 2 * dock.pad
            height: dock.size + 2 * dock.pad
            radius: 22
            tone: Qt.rgba(96/255, 104/255, 126/255, 1); tint: 0.30; shine: 1.4
            shadowOpacity: 0.30

            Row {
                id: row
                x: dock.pad; y: dock.pad
                spacing: dock.gap
                DockItem { themeIcon: "@start"; label: "Start"; onActivate: Ui.toggle("start", dock.modelData.name) }
                Sep {}
                Repeater {
                    model: dock.items
                    DockItem {
                        required property var modelData
                        app: modelData
                        label: modelData.wins.length === 1 && !modelData.pinned ? modelData.wins[0].title : (modelData.entry ? modelData.entry.name : modelData.id)
                    }
                }
                Sep {}
                DockItem { themeIcon: "@trash"; label: "Kosz"; onActivate: Quickshell.execDetached([Quickshell.env("HOME") + "/.local/bin/mist-files", "trash:///"]) }
            }
        }
    }

    component Sep: Item {
        width: 1; height: dock.size
        Rectangle { anchors.centerIn: parent; width: 1; height: dock.size * 0.6; color: Qt.rgba(1, 1, 1, 0.18) }
    }

    component DockItem: Item {
        id: it
        property var app: null
        property string glyph: ""
        property string themeIcon: ""
        property string label: ""
        readonly property real mag: 1
        signal activate()
        readonly property bool running: !!app && app.wins.length > 0
        readonly property bool focused: running && app.wins.some(w => w.activated)
        width: dock.size; height: dock.size

        Item {
            id: icon
            anchors.fill: parent
            // hover: a small lift, like the Windows dock
            transform: [
                Scale { origin.x: dock.size / 2; origin.y: dock.size / 2; xScale: ma.pressed ? 0.96 : ma.containsMouse ? 1.07 : 1; yScale: xScale
                        Behavior on xScale { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } } },
                Translate { y: ma.pressed ? -1 : ma.containsMouse ? -3 : 0
                            Behavior on y { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } } }
            ]
            // glyph tiles (Start / Pulpit / Kosz): soft glass squircle like the Windows tile pack
            LiquidGlass {
                visible: it.glyph !== ""
                anchors.fill: parent; radius: 13
                tone: "white"; tint: 0.16; shine: 1.3; shadow: false
                Icon { anchors.centerIn: parent; name: it.glyph; size: 24; color: Qt.rgba(1, 1, 1, 0.95) }
            }
            Item {
                anchors.fill: parent
                layer.enabled: true
                layer.effect: MultiEffect { shadowEnabled: true; shadowVerticalOffset: 3; shadowBlur: 0.5; shadowColor: Qt.rgba(2/255, 5/255, 10/255, 0.45) }
                readonly property string iconName: it.themeIcon !== "" ? it.themeIcon : it.app && it.app.entry ? it.app.entry.icon : (it.app ? it.app.id : "")
                Image {
                    id: tile
                    anchors.fill: parent
                    source: Tiles.url(parent.iconName)
                    sourceSize: Qt.size(dock.size * 2, dock.size * 2)
                    cache: false; asynchronous: true; smooth: true; mipmap: true
                    onStatusChanged: if (status === Image.Error) Tiles.need(parent.iconName)
                }
                // theme icon only when there is no tile ("@start" / "@trash" always have one)
                IconImage {
                    visible: tile.status === Image.Error && it.themeIcon === ""
                    anchors.fill: parent; anchors.margins: 2
                    source: visible && !parent.iconName.startsWith("@") ? Quickshell.iconPath(parent.iconName, "application-x-executable") : ""
                }
            }
        }
        // running indicator: the macOS dot
        Rectangle {
            visible: it.running
            anchors.horizontalCenter: parent.horizontalCenter; y: dock.size + dock.pad / 2 - 2
            width: 4; height: 4; radius: 2
            color: Qt.rgba(1, 1, 1, it.focused ? 0.9 : 0.6)
        }
        // instance counter
        Rectangle {
            visible: it.running && it.app.wins.length > 1
            x: dock.size - width * 0.75; y: dock.size - 20; height: 16; width: Math.max(16, cnt.implicitWidth + 9); radius: 8
            color: Qt.rgba(58/255, 64/255, 78/255, 0.95)
            Txt { id: cnt; anchors.centerIn: parent; text: it.app ? it.app.wins.length : ""; size: 10; w: 600; color: "white" }
        }
        // label pill above
        Rectangle {
            opacity: ma.containsMouse && !dock.menuOpen ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 140 } }
            anchors.horizontalCenter: parent.horizontalCenter
            y: -dock.pad - height - 10 - dock.size * (it.mag - 1)
            width: Math.min(260, lab.implicitWidth + 20); height: 26; radius: 10
            color: "transparent"
            LiquidGlass { anchors.fill: parent; radius: 10; tint: 0.86; shadowOpacity: 0.25 }
            Txt { id: lab; anchors.centerIn: parent; width: parent.width - 20; horizontalAlignment: Text.AlignHCenter; text: it.label; size: 12; w: 500 }
        }
        MouseArea {
            id: ma
            anchors.fill: parent; hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
            onClicked: m => {
                if (!it.app) { it.activate(); return }
                if (m.button === Qt.RightButton) { menu.app = it.app; menu.anchorX = it.mapToItem(dock.contentItem, dock.size / 2, 0).x; dock.menuOpen = true; return }
                if (m.button === Qt.MiddleButton || !it.running) { if (it.app.entry) it.app.entry.execute(); return }
                // minimized windows come back first (all of them, like Windows)
                const mins = it.app.wins.filter(w => dock.isMinimized(w))
                if (mins.length) { for (const w of mins) dock.restore(w); return }
                // cycle through the app's windows; the first click focuses the most recent one
                const ws = it.app.wins
                const i = ws.findIndex(w => w.activated)
                const next = ws[(i + 1) % ws.length]
                next.activate()
                const h = dock.hyprOf(next)
                if (h) Quickshell.execDetached(["hyprctl", "eval", "hl.dispatch(hl.dsp.window.bring_to_top())"])
            }
        }
    }

    // ---------------------------------------------------------------- right-click menu (Mist sheet)
    PopupWindow {
        id: menu
        property var app: null
        property real anchorX: 0
        visible: dock.menuOpen && !!app
        anchor.window: dock
        anchor.rect.x: anchorX - implicitWidth / 2
        anchor.rect.y: slabBox.y - implicitHeight - 6
        implicitWidth: 300; implicitHeight: mcol.implicitHeight + 20
        color: "transparent"
        HyprlandFocusGrab { windows: [menu]; active: menu.visible; onCleared: dock.menuOpen = false }
        Glass {
            anchors.fill: parent; radius: 18; tint: 0.92
            Column {
                id: mcol
                x: 10; y: 10; width: parent.width - 20
                Label { text: menu.app && menu.app.entry ? menu.app.entry.name : ""; leftPadding: 10; topPadding: 4; bottomPadding: 6 }
                Repeater {
                    model: menu.app ? menu.app.wins : []
                    MenuRow { required property var modelData; icon: "desktop"; text: modelData.title; onPicked: modelData.activate() }
                }
                Rectangle { visible: menu.app && menu.app.wins.length > 0; width: parent.width; height: 1; color: Theme.hair }
                MenuRow { visible: menu.app && menu.app.wins.length > 0; icon: "minus"; text: "Minimalizuj"; onPicked: { for (const w of menu.app.wins) { const h = dock.hyprOf(w); if (h) Quickshell.execDetached(["hyprctl", "eval", "MistMinimize('0x" + h.address.replace(/^0x/, "") + "')"]) } } }
                MenuRow { icon: "plus"; text: "Nowe okno"; onPicked: if (menu.app.entry) menu.app.entry.execute() }
                MenuRow {
                    icon: "pin"; text: menu.app && menu.app.pinned ? "Odepnij od docka" : "Przypnij do docka"
                    onPicked: {
                        const pins = (dock.conf.dock || []).slice(), id = menu.app.entry ? menu.app.entry.id : menu.app.id
                        dock.savePins(menu.app.pinned ? pins.filter(p => (dock.entryFor(p) || {}).id !== id && p !== id) : pins.concat([id]))
                    }
                }
                MenuRow { visible: menu.app && menu.app.wins.length > 0; icon: "x"; text: menu.app && menu.app.wins.length > 1 ? "Zamknij wszystkie okna" : "Zamknij okno"; danger: true; onPicked: { for (const w of menu.app.wins) { const h = dock.hyprOf(w); if (h) Quickshell.execDetached(["hyprctl", "eval", "hl.dispatch(hl.dsp.focus({ window = \"address:0x" + h.address.replace(/^0x/, "") + "\" })); hl.dispatch(hl.dsp.window.close())"]) } } }
            }
        }
    }
    component MenuRow: Rectangle {
        id: mr
        property string icon
        property string text
        property bool danger: false
        signal picked()
        width: parent.width; height: 36; radius: 10
        color: mra.containsMouse ? Theme.fill2 : "transparent"
        Rectangle {
            x: 8; anchors.verticalCenter: parent.verticalCenter; width: 24; height: 24; radius: 7
            color: mr.danger ? Qt.rgba(238/255, 154/255, 170/255, 0.14) : Theme.fill
            Icon { anchors.centerIn: parent; name: mr.icon; size: 14; color: mr.danger ? Theme.danger : Theme.t2 }
        }
        Txt { x: 42; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 52; text: mr.text; size: 13; color: mr.danger ? Theme.danger : Theme.t1 }
        MouseArea { id: mra; anchors.fill: parent; hoverEnabled: true; onClicked: { mr.picked(); dock.menuOpen = false } }
    }
}
