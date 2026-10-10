import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Widgets
import "components"

// macOS Notification Center (Super+A, bar bell): sheet under the bar, same look as the Control centre.
// Header (Do Not Disturb, clear), compact date + agenda tile, then the notification history of Mist's own
// server (Notifs.qml) grouped per app inside day sections (Dzisiaj / Wczoraj / Wcześniej).
// A group shows its newest notification; "+N" expands it. ✕ on hover hides a group until it posts again.
// KDE Connect forwards phone notifications with the phone app as summary -> grouped by that app instead.
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "notifications" && Ui.screenName === modelData.name
    anchors { top: true; right: true }
    margins { top: 30; right: 6 }
    exclusionMode: ExclusionMode.Ignore
    implicitWidth: 420; implicitHeight: modelData.height - 40
    mask: Region { item: sheet }
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-popup"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
    color: "transparent"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    readonly property var entries: Notifs.history
    property string open: ""
    readonly property bool dnd: Notifs.dnd
    onVisibleChanged: if (visible) { open = ""; flick.contentY = 0; sheet.forceActiveFocus() }

    function clean(t) { return (t || "").replace(/<[^>]+>/g, "").replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").trim() }
    function isPhone(n) { return n.app_name === "KDE Connect" && !!n.summary && n.desktop_entry === "org.kde.kdeconnect.daemon" }
    // group key / title, and what one row shows
    function keyOf(n) { return isPhone(n) ? n.summary : n.app_name || "System" }
    function titleOf(n) { return isPhone(n) ? "" : clean(n.summary) }
    function bodyOf(n) { return clean(n.body) }
    // pictures are PNG files kept by Notifs (image_w = their width)
    function img(n) { return n.image || "" }
    // small images (phone app icons, avatars) become the group icon, big ones a thumbnail
    function avatar(n) { return n.image && n.image_w <= 128 ? n.image : "" }
    function thumb(n) { return n.image && n.image_w > 128 ? n.image : "" }
    function iconOf(g) {
        const n = g.items[0]
        if (g.phone) return avatar(n)
        const a = avatar(n)
        if (a) return a
        const i = n.icon || ""
        if (i.startsWith("/")) return "file://" + i
        if (i.startsWith("file:")) return i
        const e = n.desktop_entry ? (DesktopEntries.byId(n.desktop_entry) || DesktopEntries.heuristicLookup(n.desktop_entry)) : DesktopEntries.heuristicLookup(g.app)
        const name = i || (e ? e.icon : "")
        return name ? Quickshell.iconPath(name, true) : ""
    }
    function dayStart(ms) { const d = new Date(ms); d.setHours(0, 0, 0, 0); return d.getTime() }
    function ago(ms) {
        const s = (Date.now() - ms) / 1000
        if (s < 60) return "teraz"
        if (s < 3600) return Math.floor(s / 60) + " min"
        if (dayStart(ms) === dayStart(Date.now())) return Data.hm(new Date(ms))
        const d = new Date(ms)
        if (s < 7 * 86400) return Data.daysShort[d.getDay()] + " " + Data.hm(d)
        return d.getDate() + "." + (d.getMonth() + 1 < 10 ? "0" : "") + (d.getMonth() + 1)
    }

    readonly property var sections: {
        const by = {}, groups = []
        const hid = Ui.notifHidden
        for (const n of entries.slice().sort((a, b) => b.received_wall_ms - a.received_wall_ms)) {
            const k = keyOf(n)
            if (hid[k] && n.received_wall_ms <= hid[k]) continue
            if (!by[k]) { by[k] = { app: k, phone: isPhone(n), items: [] }; groups.push(by[k]) }
            by[k].items.push(n)
        }
        const today = dayStart(Date.now()), out = [
            { title: "Dzisiaj", groups: [] }, { title: "Wczoraj", groups: [] }, { title: "Wcześniej", groups: [] }]
        for (const g of groups) {
            const d = dayStart(g.items[0].received_wall_ms)
            out[d >= today ? 0 : d >= today - 86400000 ? 1 : 2].groups.push(g)
        }
        return out.filter(s => s.groups.length)
    }
    readonly property int total: sections.reduce((a, s) => a + s.groups.reduce((b, g) => b + g.items.length, 0), 0)
    function hide(app) { const h = Object.assign({}, Ui.notifHidden); h[app] = Date.now(); Ui.notifHidden = h; if (open === app) open = "" }
    function clearAll() { Notifs.clear() }

    // ------------------------------------------------------------ sheet
    Glass {
        id: sheet
        x: 10; y: 10; width: parent.width - 20
        height: Math.min(win.height - 20, col.implicitHeight + 32)
        Behavior on height { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        radius: Theme.rSheet; tint: 0.82
        clipContent: true
        focus: true
        Keys.onEscapePressed: win.open !== "" ? win.open = "" : Ui.close()

        Flickable {
            id: flick
            anchors.fill: parent; anchors.margins: 16
            contentHeight: col.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
                id: col
                width: flick.width
                spacing: 10

                // ---------------- header
                Item {
                    width: parent.width; height: 34
                    Row {
                        anchors.verticalCenter: parent.verticalCenter; spacing: 8
                        Txt { text: "Powiadomienia"; size: 17; w: 650; anchors.verticalCenter: parent.verticalCenter }
                        Rectangle {
                            visible: win.total > 0
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(20, cnt.implicitWidth + 12); height: 20; radius: 10; color: Theme.fill2
                            Txt { id: cnt; anchors.centerIn: parent; text: win.total; size: 11; w: 650; color: Theme.t2; font.features: ({ "tnum": 1 }) }
                        }
                    }
                    Row {
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 6
                        Pill {
                            visible: win.total > 0
                            icon: "trash"; text: "Wyczyść"
                            onClicked: win.clearAll()
                        }
                        Orb {
                            icon: win.dnd ? "bellZ" : "bell"; on: win.dnd
                            onClicked: Notifs.setDnd(!Notifs.dnd)
                        }
                    }
                }

                // ---------------- date + agenda
                Tile {
                    width: parent.width; height: Math.max(72, agenda.implicitHeight + 24)
                    Column {
                        id: dateBlock
                        x: 16; anchors.verticalCenter: parent.verticalCenter
                        width: 52
                        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Data.daysShort[Data.now.getDay()]; size: 11; w: 700; color: Theme.danger; font.capitalization: Font.AllUppercase; font.letterSpacing: 1 }
                        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Data.now.getDate(); size: 28; w: 600; font.family: Theme.fontRounded }
                    }
                    Rectangle { id: sep; x: dateBlock.x + dateBlock.width + 14; y: 14; width: 1; height: parent.height - 28; color: Theme.hair }
                    Column {
                        id: agenda
                        anchors.left: sep.right; anchors.leftMargin: 14; anchors.right: parent.right; anchors.rightMargin: 14
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8
                        readonly property var evs: Data.upcoming(3, 2)
                        Repeater {
                            model: agenda.evs
                            delegate: Row {
                                required property var modelData
                                spacing: 9; width: agenda.width
                                Rectangle { width: 3; height: 30; radius: 2; color: Theme.accent }
                                Column {
                                    width: parent.width - 12
                                    Txt { width: parent.width; text: modelData.title || modelData.summary || ""; size: 12.5; w: 600 }
                                    Txt { width: parent.width; text: Data.rel(modelData.start) + " · " + Data.when(modelData); size: 11; color: Theme.t2 }
                                }
                            }
                        }
                        Column {
                            visible: agenda.evs.length === 0
                            Txt { text: Data.cap(Data.days[Data.now.getDay()]) + ", " + Data.now.getDate() + " " + Data.monthsGen[Data.now.getMonth()]; size: 12.5; w: 600 }
                            Txt { text: "Brak wydarzeń dziś i jutro"; size: 11; color: Theme.t2 }
                        }
                    }
                    MouseArea { anchors.fill: parent; z: -1; cursorShape: Qt.PointingHandCursor; onClicked: Ui.toggle("calendar", win.modelData.name) }
                }

                // ---------------- empty
                Column {
                    visible: win.total === 0
                    width: parent.width; topPadding: 26; bottomPadding: 22; spacing: 8
                    Icon { anchors.horizontalCenter: parent.horizontalCenter; name: win.dnd ? "bellZ" : "bell"; size: 30; color: Theme.t4 }
                    Txt { anchors.horizontalCenter: parent.horizontalCenter; text: win.dnd ? "Nie przeszkadzać jest włączone" : "Brak powiadomień"; size: 13; color: Theme.t3 }
                }

                // ---------------- day sections
                Repeater {
                    model: win.sections
                    delegate: Column {
                        id: sec
                        required property var modelData
                        width: col.width; spacing: 8
                        Label { text: sec.modelData.title; leftPadding: 4; topPadding: 4 }
                        Repeater {
                            model: sec.modelData.groups
                            delegate: GroupCard { required property var modelData; group: modelData; width: col.width }
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ parts
    // one app: icon + newest notification; expanded = up to 6 rows separated by hairlines
    component GroupCard: Tile {
        id: gcard
        property var group
        readonly property bool expanded: win.open === group.app
        readonly property int more: group.items.length - 1
        readonly property var rows: expanded ? group.items.slice(0, 6) : group.items.slice(0, 1)
        height: inner.implicitHeight + 24
        Behavior on height { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }

        AppAvatar { id: av; x: 12; y: 12; src: win.iconOf(gcard.group); letter: gcard.group.app; size: 34 }
        Column {
            id: inner
            anchors.left: av.right; anchors.leftMargin: 11; anchors.right: parent.right; anchors.rightMargin: 12
            y: 11
            spacing: 0
            // app line
            Item {
                width: parent.width; height: 18
                Row {
                    anchors.verticalCenter: parent.verticalCenter; spacing: 6
                    width: parent.width - 70
                    Txt { id: appT; text: gcard.group.app; size: 11.5; w: 600; color: Theme.t2; width: Math.min(implicitWidth, parent.width - 30) }
                    Rectangle {
                        visible: gcard.more > 0
                        anchors.verticalCenter: parent.verticalCenter
                        width: moreT.implicitWidth + 10; height: 16; radius: 8; color: Theme.fill2
                        Txt { id: moreT; anchors.centerIn: parent; text: gcard.group.items.length; size: 10; w: 650; color: Theme.t2 }
                    }
                }
                Txt {
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    visible: !gcard.hovered
                    text: win.ago(gcard.group.items[0].received_wall_ms); size: 11; color: Theme.t3
                }
                Rectangle {
                    visible: gcard.hovered
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    width: 22; height: 22; radius: 11
                    color: xm.containsMouse ? Qt.rgba(1, 1, 1, 0.18) : Theme.fill2
                    Icon { anchors.centerIn: parent; name: "x"; size: 12; color: Theme.t1 }
                    MouseArea { id: xm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.hide(gcard.group.app) }
                }
            }
            Repeater {
                model: gcard.rows
                delegate: Item {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property string th: win.thumb(modelData)
                    width: inner.width
                    height: Math.max(txt.implicitHeight, th ? 44 : 0) + (index > 0 ? 18 : 4)
                    Rectangle { visible: row.index > 0; y: 8; width: parent.width; height: 1; color: Theme.hair }
                    Column {
                        id: txt
                        y: row.index > 0 ? 16 : 3
                        width: parent.width - (row.th ? 54 : 0)
                        spacing: 1
                        Item {
                            visible: row.index > 0
                            width: parent.width; height: 0
                            Txt { anchors.right: parent.right; y: 1; text: win.ago(row.modelData.received_wall_ms); size: 10.5; color: Theme.t3 }
                        }
                        Txt { visible: text !== ""; width: parent.width - (row.index > 0 ? 50 : 0); text: win.titleOf(row.modelData); size: 13; w: 620 }
                        Txt {
                            visible: text !== ""; width: parent.width
                            text: win.bodyOf(row.modelData); size: 12.5; color: win.titleOf(row.modelData) ? Theme.t2 : Theme.t1
                            wrapMode: Text.Wrap; maximumLineCount: gcard.expanded ? 5 : 3; elide: Text.ElideRight
                        }
                    }
                    ClippingRectangle {
                        visible: row.th !== ""
                        anchors.right: parent.right; y: txt.y
                        width: 44; height: 44; radius: 9; color: Theme.fill2
                        Image {
                            id: ti; anchors.fill: parent; source: row.th; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(88, 88); asynchronous: true; cache: false
                            Connections { target: win; function onImgRevChanged() { if (ti.status === Image.Error) { ti.source = ""; ti.source = Qt.binding(() => row.th) } } }
                        }
                    }
                }
            }
            // expand / collapse
            Item {
                visible: gcard.more > 0
                width: parent.width; height: 26
                Txt {
                    y: 8
                    text: gcard.expanded ? "Pokaż mniej" : "+" + gcard.more + " " + (gcard.more === 1 ? "powiadomienie" : gcard.more < 5 ? "powiadomienia" : "powiadomień")
                    size: 11.5; w: 600; color: Theme.accentStrong
                }
            }
        }
        MouseArea {
            anchors.fill: parent; z: -1
            enabled: gcard.more > 0
            cursorShape: Qt.PointingHandCursor
            onClicked: win.open = gcard.expanded ? "" : gcard.group.app
        }
    }
    // glass lens (white-tinted LiquidGlass, no shadow)
    component Tile: Item {
        readonly property bool hovered: th.hovered
        LiquidGlass {
            anchors.fill: parent; radius: 22
            tone: "white"; tint: th.hovered ? 0.085 : 0.06
            shadow: false; shine: 0.6
            Behavior on tint { NumberAnimation { duration: 120 } }
        }
        HoverHandler { id: th }
    }
    // app icon: theme icon / image, or a tinted letter tile when nothing resolves
    component AppAvatar: Item {
        id: aa
        property string src
        property string letter
        property real size: 34
        width: size; height: size
        readonly property var palette: [Theme.accent, Theme.violet, Theme.orange, Theme.green, Theme.pink, Theme.teal, Theme.indigo]
        readonly property color tint: palette[Math.abs([...letter].reduce((h, c) => (h * 31 + c.charCodeAt(0)) | 0, 7)) % palette.length]
        Rectangle {
            visible: !aa.src || im.status === Image.Error
            anchors.fill: parent; radius: aa.size * 0.28
            color: Qt.rgba(aa.tint.r, aa.tint.g, aa.tint.b, 0.22)
            border.width: 1; border.color: Qt.rgba(aa.tint.r, aa.tint.g, aa.tint.b, 0.3)
            Txt { anchors.centerIn: parent; text: (aa.letter || "?").charAt(0).toUpperCase(); size: aa.size * 0.44; w: 700; color: aa.tint }
        }
        ClippingRectangle {
            visible: !!aa.src && im.status !== Image.Error
            anchors.fill: parent; radius: aa.size * 0.24; color: "transparent"
            Image { id: im; anchors.fill: parent; source: aa.src; fillMode: Image.PreserveAspectFit; sourceSize: Qt.size(aa.size * 2, aa.size * 2); asynchronous: true; smooth: true; cache: false }
            // PNG mirror may appear only after mistd notifimg ran -> retry failed loads
            Connections { target: win; function onImgRevChanged() { if (im.status === Image.Error) { im.source = ""; im.source = Qt.binding(() => aa.src) } } }
        }
    }
    component Orb: Rectangle {
        id: o
        property string icon
        property bool on: false
        signal clicked()
        width: 32; height: 32; radius: 16
        color: on ? Theme.accent : om.containsMouse ? Qt.rgba(1, 1, 1, 0.18) : Theme.fill2
        Behavior on color { ColorAnimation { duration: 150 } }
        Icon { anchors.centerIn: parent; name: o.icon; size: 15; color: o.on ? "white" : Theme.t1 }
        MouseArea { id: om; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: o.clicked() }
    }
    component Pill: Rectangle {
        id: pl
        property string icon
        property string text
        signal clicked()
        width: pr.implicitWidth + 22; height: 32; radius: 16
        color: pm.containsMouse ? Qt.rgba(1, 1, 1, 0.18) : Theme.fill2
        Behavior on color { ColorAnimation { duration: 150 } }
        Row {
            id: pr; anchors.centerIn: parent; spacing: 6
            Icon { name: pl.icon; size: 13; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: pl.text; size: 12; w: 600; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea { id: pm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: pl.clicked() }
    }
}
