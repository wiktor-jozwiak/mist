import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"
import "components/LivePresets.js" as Presets

// Wallpaper gallery (Super+Shift+W): the whole screen is a live preview of the scene under the pointer
// (or the selected one), with the desktop's date + clock on top so you see how it sits behind the
// widgets. A glass shelf at the bottom holds the filters, tempo / music and a carousel of moving
// thumbnails (animated scenes, components/LivePresets.js) and photos (~/Obrazy/Tapety, LiveWallpaper.qml draws
// them). Click sets, Enter sets and closes, ← → browse, Tab next filter, Esc closes.
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "wallpaper" && Ui.screenName === modelData.name
    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-spotlight"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    color: "black"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    readonly property string dir: Quickshell.env("HOME") + "/Obrazy/Tapety"
    property var files: []
    property string current: ""
    readonly property string live: LiveCfg.live
    readonly property string applied: live ? "live:" + live : current ? "photo:" + current : ""

    // ------------------------------------------------------------ what the carousel shows
    property string filter: "all"
    readonly property var filters: [
        { id: "all", name: "Wszystkie", icon: "sparkles" },
        { id: "auto", name: "Same się dopasowują", icon: "sunMoon" },
        { id: "calm", name: "Spokojne", icon: "leaf" },
        { id: "live", name: "Żywe", icon: "bolt" },
        { id: "toon", name: "Bajki", icon: "heart" },
        { id: "photos", name: "Zdjęcia", icon: "photo" }
    ]
    readonly property var items: {
        const out = []
        if (filter !== "photos")
            for (const p of Presets.list)
                if (filter === "all" || p.cat === filter)
                    out.push({ key: "live:" + p.id, name: p.name, desc: p.desc || "", cat: p.cat, auto: !!p.auto })
        if (filter === "photos" || filter === "all")
            for (const f of files)
                out.push({ key: "photo:" + f, name: nice(f), desc: "Zdjęcie z ~/Obrazy/Tapety", cat: "photos", auto: false })
        return out
    }
    function count(id) {
        if (id === "photos") return files.length
        if (id === "all") return Presets.list.length + files.length
        return Presets.inCat(id).length
    }
    property int sel: 0
    property int hover: -1
    readonly property var shown: items[hover >= 0 ? hover : sel] || null
    function catName(c) { return c === "auto" ? "Same się dopasowują" : c === "calm" ? "Spokojne" : c === "live" ? "Żywe" : c === "toon" ? "Bajki" : "Zdjęcie" }
    function nice(p) { return p.split("/").pop().replace(/\.[^.]+$/, "").replace(/[-_]+/g, " ").replace(/^./, c => c.toUpperCase()) }

    function selectApplied() {
        const i = items.findIndex(it => it.key === applied)
        sel = Math.max(0, i)
        carousel.positionViewAtIndex(sel, ListView.Center)
    }
    onFilterChanged: { hover = -1; sel = Math.max(0, items.findIndex(it => it.key === applied)); carousel.positionViewAtIndex(sel, ListView.Center) }

    onVisibleChanged: if (visible) {
        filter = "all"
        hover = -1
        findP.running = true; current = LiveCfg.image
        keys.forceActiveFocus()
        intro.restart()
    }
    Process {
        id: findP
        command: ["sh", "-c", "find \"$1\" -maxdepth 2 -type f \\( -iname '*.png' -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.webp' \\) | sort", "sh", win.dir]
        stdout: StdioCollector { onStreamFinished: { win.files = text.trim().split("\n").filter(s => s); win.selectApplied() } }
    }

    function apply(it) {
        if (!it) return
        if (it.key.startsWith("live:")) LiveCfg.save({ live: it.key.slice(5) })
        else {
            const f = it.key.slice(6)
            current = f
            LiveCfg.save({ live: "", image: f })
        }
    }

    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: e => {
            if (e.key === Qt.Key_Escape) Ui.close()
            else if (e.key === Qt.Key_Right) { win.hover = -1; win.sel = Math.min(win.items.length - 1, win.sel + 1) }
            else if (e.key === Qt.Key_Left) { win.hover = -1; win.sel = Math.max(0, win.sel - 1) }
            else if (e.key === Qt.Key_Tab || e.key === Qt.Key_Backtab) {
                const i = win.filters.findIndex(f => f.id === win.filter)
                win.filter = win.filters[(i + (e.key === Qt.Key_Backtab ? win.filters.length - 1 : 1)) % win.filters.length].id
            }
            else if (e.key === Qt.Key_Space) win.apply(win.items[win.sel])
            else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { win.apply(win.items[win.sel]); Ui.close() }
            else return
            e.accepted = true
        }
    }

    // ------------------------------------------------------------ full-screen preview, cross-faded
    component Scene: Item {
        id: sc
        property string key: ""
        property bool on: false
        anchors.fill: parent
        opacity: on ? 1 : 0
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: 420; easing.type: Easing.InOutQuad } }
        LiveWall {
            anchors.fill: parent
            visible: sc.key.startsWith("live:")
            preset: visible ? sc.key.slice(5) : "aurora"
            animate: sc.visible
            speedMul: Presets.tempo(LiveCfg.tempo)
        }
        Image {
            anchors.fill: parent
            visible: sc.key.startsWith("photo:")
            fillMode: Image.PreserveAspectCrop; asynchronous: true
            source: visible ? "file://" + sc.key.slice(6) : ""
            sourceSize: Qt.size(win.width, win.height)
        }
    }
    property bool aFront: true
    readonly property string shownKey: shown ? shown.key : ""
    onShownKeyChanged: {
        if (!shownKey) return
        if (aFront && sceneA.key === shownKey || !aFront && sceneB.key === shownKey) return
        if (aFront) { sceneB.key = shownKey; aFront = false } else { sceneA.key = shownKey; aFront = true }
    }
    Item {
        id: stage
        anchors.fill: parent
        opacity: Math.min(1, win.enter * 1.6)
        Scene { id: sceneA; on: win.aFront }
        Scene { id: sceneB; on: !win.aFront }
    }

    // the desktop's own composition, so the preview reads like the real thing
    Column {
        id: mock
        anchors.horizontalCenter: parent.horizontalCenter
        y: 118
        opacity: win.enter
        CTxt {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Data.longDate(Data.now); size: 22; w: 600; color: Qt.rgba(1, 1, 1, 0.82)
        }
        CTxt {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Data.hm(Data.now); size: 150; w: 620; color: Qt.rgba(1, 1, 1, 0.86)
            font.letterSpacing: -4
        }
    }

    // scrim under the shelf and the title
    Rectangle {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: parent.height * 0.55
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0) }
            GradientStop { position: 0.55; color: Qt.rgba(0, 0, 0, 0.35) }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.62) }
        }
    }

    // close
    Rectangle {
        anchors { top: parent.top; right: parent.right; margins: 28 }
        width: 40; height: 40; radius: 20
        color: xm.containsMouse ? Qt.rgba(1, 1, 1, 0.22) : Qt.rgba(1, 1, 1, 0.1)
        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.14)
        Icon { anchors.centerIn: parent; name: "x"; size: 18; color: Qt.rgba(1, 1, 1, 0.9) }
        MouseArea { id: xm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Ui.close() }
    }

    // ------------------------------------------------------------ title of what is shown
    Column {
        id: title
        x: 64
        anchors.bottom: shelf.top; anchors.bottomMargin: 28
        spacing: 8
        opacity: win.enter
        Row {
            spacing: 10
            Txt {
                anchors.verticalCenter: parent.verticalCenter
                text: win.shown ? (win.catName(win.shown.cat) + (win.shown.key.startsWith("live:") ? " · animowana" : "")).toUpperCase() : ""
                size: 11.5; w: 650; color: Qt.rgba(1, 1, 1, 0.62); font.letterSpacing: 1.6
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                visible: !!win.shown && win.shown.key === win.applied
                width: setRow.implicitWidth + 16; height: 22; radius: 11
                color: Qt.rgba(138/255, 214/255, 208/255, 0.2); border.width: 1; border.color: Qt.rgba(138/255, 214/255, 208/255, 0.45)
                Row {
                    id: setRow
                    anchors.centerIn: parent; spacing: 4
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: "check"; size: 12; color: Theme.mAcc }
                    Txt { anchors.verticalCenter: parent.verticalCenter; text: "Ustawiona"; size: 11.5; w: 600; color: Theme.mAccStrong }
                }
            }
        }
        Txt {
            text: win.shown ? win.shown.name : ""
            size: 52; w: 650; color: "white"; font.letterSpacing: -1.2
            layer.enabled: true
            layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "black"; shadowOpacity: 0.35; shadowBlur: 0.8; shadowVerticalOffset: 2 }
        }
        Txt {
            width: Math.min(620, win.width - 128)
            text: win.shown ? win.shown.desc : ""
            size: 16; color: Qt.rgba(1, 1, 1, 0.78); wrapMode: Text.WordWrap
        }
    }
    // what Enter does, on the right of the title
    Row {
        anchors { right: parent.right; rightMargin: 64; bottom: shelf.top; bottomMargin: 34 }
        spacing: 10
        opacity: title.opacity
        Rectangle {
            visible: !!win.shown && win.shown.key !== win.applied
            width: apRow.implicitWidth + 32; height: 44; radius: 22
            color: apm.containsMouse ? "white" : Qt.rgba(1, 1, 1, 0.9)
            Row {
                id: apRow
                anchors.centerIn: parent; spacing: 8
                Icon { anchors.verticalCenter: parent.verticalCenter; name: "check"; size: 16; color: "#0b0f17" }
                Txt { anchors.verticalCenter: parent.verticalCenter; text: "Ustaw tapetę"; size: 14; w: 650; color: "#0b0f17" }
            }
            MouseArea { id: apm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                onClicked: { win.apply(win.shown); win.sel = Math.max(0, win.hover >= 0 ? win.hover : win.sel) } }
        }
    }

    // ------------------------------------------------------------ the shelf
    Glass {
        id: shelf
        x: 40; width: parent.width - 80
        height: 280
        y: parent.height - height - 28 + (1 - win.enter) * 140
        radius: 30; tint: 0.55
        opacity: Math.min(1, win.enter * 1.4)

        // filters
        Row {
            id: pills
            x: 22; y: 18
            spacing: 6
            Repeater {
                model: win.filters
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool on: win.filter === modelData.id
                    width: pr.implicitWidth + 26; height: 34; radius: 17
                    color: on ? Qt.rgba(1, 1, 1, 0.92) : pm.containsMouse ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(1, 1, 1, 0.07)
                    Behavior on color { ColorAnimation { duration: 140 } }
                    Row {
                        id: pr
                        anchors.centerIn: parent; spacing: 7
                        Icon { anchors.verticalCenter: parent.verticalCenter; name: modelData.icon; size: 15; color: parent.parent.on ? "#0b0f17" : Qt.rgba(1, 1, 1, 0.8) }
                        Txt { anchors.verticalCenter: parent.verticalCenter; text: modelData.name; size: 13; w: 600; color: parent.parent.on ? "#0b0f17" : Qt.rgba(1, 1, 1, 0.86) }
                        Txt { anchors.verticalCenter: parent.verticalCenter; text: win.count(modelData.id); size: 12; w: 500; color: parent.parent.on ? Qt.rgba(11/255, 15/255, 23/255, 0.5) : Qt.rgba(1, 1, 1, 0.4) }
                    }
                    MouseArea { id: pm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.filter = parent.modelData.id }
                }
            }
        }

        // tempo + music (animated scenes), folder (photos)
        Row {
            anchors { right: parent.right; rightMargin: 22 }
            y: 18
            spacing: 10
            Rectangle {
                visible: win.filter === "photos"
                width: fr.implicitWidth + 26; height: 34; radius: 17
                color: fm.containsMouse ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(1, 1, 1, 0.07)
                Row { id: fr; anchors.centerIn: parent; spacing: 7
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: "folder"; size: 15; color: Qt.rgba(1, 1, 1, 0.8) }
                    Txt { anchors.verticalCenter: parent.verticalCenter; text: "Otwórz folder"; size: 13; w: 600; color: Qt.rgba(1, 1, 1, 0.86) } }
                MouseArea { id: fm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { Ui.close(); Quickshell.execDetached(["xdg-open", win.dir]) } }
            }
            // segmented tempo
            Rectangle {
                visible: win.filter !== "photos"
                width: seg.implicitWidth + 6; height: 34; radius: 17
                color: Qt.rgba(1, 1, 1, 0.07)
                Row {
                    id: seg
                    x: 3; anchors.verticalCenter: parent.verticalCenter
                    Repeater {
                        model: Presets.tempos
                        delegate: Rectangle {
                            required property var modelData
                            readonly property bool on: LiveCfg.tempo === modelData.id
                            width: st.implicitWidth + 24; height: 28; radius: 14
                            color: on ? Qt.rgba(1, 1, 1, 0.2) : sm.containsMouse ? Qt.rgba(1, 1, 1, 0.08) : "transparent"
                            Behavior on color { ColorAnimation { duration: 140 } }
                            Txt { id: st; anchors.centerIn: parent; text: modelData.name; size: 12.5; w: parent.on ? 650 : 500; color: parent.on ? "white" : Qt.rgba(1, 1, 1, 0.6) }
                            MouseArea { id: sm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: LiveCfg.save({ tempo: parent.modelData.id }) }
                        }
                    }
                }
            }
            Rectangle {
                visible: win.filter !== "photos"
                width: mr.implicitWidth + 26; height: 34; radius: 17
                color: LiveCfg.music ? Qt.rgba(138/255, 214/255, 208/255, 0.22) : mm.containsMouse ? Qt.rgba(1, 1, 1, 0.14) : Qt.rgba(1, 1, 1, 0.07)
                border.width: LiveCfg.music ? 1 : 0; border.color: Qt.rgba(138/255, 214/255, 208/255, 0.5)
                Behavior on color { ColorAnimation { duration: 140 } }
                Row { id: mr; anchors.centerIn: parent; spacing: 7
                    Icon { anchors.verticalCenter: parent.verticalCenter; name: "music"; size: 15; color: LiveCfg.music ? Theme.mAccStrong : Qt.rgba(1, 1, 1, 0.8) }
                    Txt { anchors.verticalCenter: parent.verticalCenter; text: "Rytm muzyki"; size: 13; w: 600; color: LiveCfg.music ? Theme.mAccStrong : Qt.rgba(1, 1, 1, 0.86) } }
                MouseArea { id: mm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: LiveCfg.save({ music: !LiveCfg.music }) }
            }
        }

        // carousel
        ListView {
            id: carousel
            x: 0; y: 60; width: parent.width; height: 184
            orientation: ListView.Horizontal
            spacing: 16
            leftMargin: 22; rightMargin: 22
            clip: true
            model: win.items
            currentIndex: win.sel
            highlightFollowsCurrentItem: false
            preferredHighlightBegin: width / 2 - 120; preferredHighlightEnd: width / 2 + 120
            highlightRangeMode: ListView.ApplyRange
            highlightMoveDuration: 380
            cacheBuffer: 400
            boundsBehavior: Flickable.StopAtBounds
            onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
            delegate: Item {
                id: tile
                required property var modelData
                required property int index
                readonly property bool isSel: index === win.sel
                readonly property bool isSet: modelData.key === win.applied
                readonly property bool hov: tm.containsMouse
                width: 220; height: 184
                Item {
                    id: card
                    width: 220; height: 124
                    y: tile.isSel ? 10 : tile.hov ? 12 : 16
                    scale: tile.isSel ? 1 : tile.hov ? 0.97 : 0.93
                    Behavior on scale { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
                    Behavior on y { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
                    Item {
                        anchors.fill: parent
                        layer.enabled: true
                        layer.effect: MultiEffect { maskEnabled: true; maskSource: tmask; maskThresholdMin: 0.5; maskSpreadAtMin: 1 }
                        LiveWall {
                            anchors.fill: parent
                            visible: tile.modelData.key.startsWith("live:")
                            preset: visible ? tile.modelData.key.slice(5) : "aurora"
                            fps: 20
                            animate: win.visible
                        }
                        Image {
                            anchors.fill: parent
                            visible: tile.modelData.key.startsWith("photo:")
                            fillMode: Image.PreserveAspectCrop; asynchronous: true
                            source: visible ? "file://" + tile.modelData.key.slice(6) : ""
                            sourceSize: Qt.size(440, 248)
                        }
                        // dim the ones you are not looking at
                        Rectangle { anchors.fill: parent; color: "black"; opacity: tile.isSel || tile.hov ? 0 : 0.22
                            Behavior on opacity { NumberAnimation { duration: 200 } } }
                    }
                    Rectangle { id: tmask; anchors.fill: parent; radius: 16; visible: false; layer.enabled: true }
                    // ring: white = selected, teal = the one that is set
                    Rectangle {
                        anchors.fill: parent; anchors.margins: -4; radius: 20
                        color: "transparent"
                        border.width: tile.isSel ? 2.5 : 0
                        border.color: "white"
                    }
                    Rectangle { anchors.fill: parent; radius: 16; color: "transparent"; border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.14) }
                    Rectangle {
                        visible: tile.isSet
                        anchors { right: parent.right; top: parent.top; margins: 8 }
                        width: 22; height: 22; radius: 11
                        color: Theme.mAcc
                        Icon { anchors.centerIn: parent; name: "check"; size: 14; color: "#0b0f17" }
                    }
                    Rectangle {
                        visible: tile.modelData.auto
                        anchors { left: parent.left; top: parent.top; margins: 8 }
                        width: aut.implicitWidth + 14; height: 20; radius: 10
                        color: Qt.rgba(0, 0, 0, 0.45)
                        Txt { id: aut; anchors.centerIn: parent; text: "AUTO"; size: 9.5; w: 700; color: Qt.rgba(1, 1, 1, 0.9); font.letterSpacing: 0.8 }
                    }
                    MouseArea {
                        id: tm
                        anchors.fill: parent
                        hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                        onContainsMouseChanged: hoverT.restart()
                        onClicked: { win.sel = tile.index; win.hover = -1; win.apply(tile.modelData) }
                        onDoubleClicked: { win.apply(tile.modelData); Ui.close() }
                    }
                }
                Txt {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 148
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight
                    text: tile.modelData.name
                    size: 13; w: tile.isSel ? 650 : 500
                    color: tile.isSel ? "white" : Qt.rgba(1, 1, 1, 0.6)
                }
            }
        }
        // the preview follows the pointer after a short pause (no flicker while sweeping across)
        Timer {
            id: hoverT
            interval: 140
            onTriggered: {
                let h = -1
                for (let i = 0; i < carousel.count; i++) {
                    const it = carousel.itemAtIndex(i)
                    if (it && it.hov) { h = i; break }
                }
                win.hover = h
            }
        }
        Txt {
            anchors { right: parent.right; rightMargin: 24; bottom: parent.bottom; bottomMargin: 12 }
            text: "← →  przeglądaj   ·   Tab  kategoria   ·   Spacja  ustaw   ·   Enter  ustaw i zamknij   ·   Esc  zamknij"
            size: 11; color: Qt.rgba(1, 1, 1, 0.38)
        }
    }

    // ------------------------------------------------------------ entrance
    property real enter: 0
    NumberAnimation { id: intro; target: win; property: "enter"; from: 0; to: 1; duration: 520; easing.type: Easing.OutCubic }
}
