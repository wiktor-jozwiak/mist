import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"

// Calendar sheet under the island: month grid (left) + day plan and upcoming (right inset panel).
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "calendar" && Ui.screenName === modelData.name
    anchors.top: true
    margins.top: 6
    exclusionMode: ExclusionMode.Normal
    implicitWidth: 830; implicitHeight: 540
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-popup"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
    color: "transparent"

    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    property date month: new Date(Data.now.getFullYear(), Data.now.getMonth(), 1)
    property date selected: Data.now
    onVisibleChanged: if (visible) { month = new Date(Data.now.getFullYear(), Data.now.getMonth(), 1); selected = Data.now; sheet.forceActiveFocus() }

    readonly property var cells: {
        const first = new Date(month.getFullYear(), month.getMonth(), 1)
        const off = (first.getDay() + 6) % 7
        const out = []
        for (let i = 0; i < 42; i++) out.push(new Date(month.getFullYear(), month.getMonth(), 1 - off + i))
        return out
    }
    function shiftMonth(n) { month = new Date(month.getFullYear(), month.getMonth() + n, 1) }
    function open(url) { Quickshell.execDetached(["xdg-open", url]) }

    Glass {
        id: sheet
        anchors.fill: parent
        anchors.margins: 10
        radius: Theme.rSheet
        tint: 0.8
        focus: true
        Keys.onPressed: e => {
            if (e.key === Qt.Key_Escape) Ui.close()
            else if (e.key === Qt.Key_Left) win.selected = new Date(win.selected.getTime() - 86400000)
            else if (e.key === Qt.Key_Right) win.selected = new Date(win.selected.getTime() + 86400000)
            else if (e.key === Qt.Key_Up) win.selected = new Date(win.selected.getTime() - 7 * 86400000)
            else if (e.key === Qt.Key_Down) win.selected = new Date(win.selected.getTime() + 7 * 86400000)
            else if (e.key === Qt.Key_PageUp) win.shiftMonth(-1)
            else if (e.key === Qt.Key_PageDown) win.shiftMonth(1)
            else if (e.key === Qt.Key_T) { win.selected = Data.now; win.month = new Date(Data.now.getFullYear(), Data.now.getMonth(), 1) }
            else return
            if (win.selected.getMonth() !== win.month.getMonth()) win.month = new Date(win.selected.getFullYear(), win.selected.getMonth(), 1)
        }

        // ------------------------------------------------ month (left)
        Column {
            id: left
            x: 30; y: 28; width: 430
            spacing: 14
            RowLayout {
                width: parent.width
                Column {
                    Layout.fillWidth: true
                    Row {
                        spacing: 8
                        Txt { text: Data.monthsNom[win.month.getMonth()]; size: 28; w: 600 }
                        Txt { text: win.month.getFullYear(); size: 28; w: 300; color: Theme.t3 }
                    }
                    Txt {
                        text: Data.longDate(Data.now) + " · " + (Data.eventsOn(Data.now).filter(e => !e.holiday).length ? Data.eventsOn(Data.now).filter(e => !e.holiday).length + " wydarzenia dziś" : "dziś wolne")
                        size: 13; color: Theme.t3
                    }
                }
                Btn { icon: "left"; color: "transparent"; onClicked: win.shiftMonth(-1) }
                Txt {
                    text: "Dziś"; size: 13; w: 600; color: Theme.t2
                    MouseArea { anchors.fill: parent; anchors.margins: -8; cursorShape: Qt.PointingHandCursor; onClicked: { win.month = new Date(Data.now.getFullYear(), Data.now.getMonth(), 1); win.selected = Data.now } }
                }
                Btn { icon: "right"; color: "transparent"; onClicked: win.shiftMonth(1) }
            }
            Grid {
                columns: 7; columnSpacing: 0; rowSpacing: 2
                Repeater {
                    model: ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"]
                    Label { required property var modelData; width: 430 / 7; horizontalAlignment: Text.AlignHCenter; text: modelData; bottomPadding: 6 }
                }
                Repeater {
                    model: win.cells
                    Item {
                        required property var modelData
                        readonly property var evs: Data.eventsOn(modelData)
                        readonly property bool other: modelData.getMonth() !== win.month.getMonth()
                        readonly property bool today: Data.sameDay(modelData, Data.now)
                        readonly property bool sel: Data.sameDay(modelData, win.selected)
                        readonly property bool holiday: evs.some(e => e.holiday)
                        width: 430 / 7; height: 50
                        Rectangle {
                            anchors.centerIn: parent; anchors.verticalCenterOffset: -5
                            width: 36; height: 36; radius: 18
                            color: sel ? Theme.accent : dayMa.containsMouse ? Theme.fill2 : evs.some(e => !e.holiday) ? Theme.fill : "transparent"
                            border.width: today && !sel ? 1.5 : 0; border.color: Theme.accent
                            Txt {
                                anchors.centerIn: parent; text: modelData.getDate(); size: 14; w: today || sel ? 700 : 500
                                color: sel ? Theme.ink : other ? Theme.t4 : holiday ? Theme.danger : (modelData.getDay() === 0 || modelData.getDay() === 6) ? Theme.t3 : Theme.t1
                            }
                        }
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 2
                            spacing: 3
                            Repeater {
                                model: Math.min(3, evs.filter(e => !e.holiday).length)
                                Rectangle { width: 4; height: 4; radius: 2; color: other ? Theme.t4 : Theme.accent }
                            }
                        }
                        MouseArea { id: dayMa; anchors.fill: parent; hoverEnabled: true; onClicked: win.selected = modelData }
                    }
                }
            }
        }
        Rectangle {
            x: 30; width: 430; height: 1; color: Theme.hair
            anchors.bottom: footer.top; anchors.bottomMargin: 14
        }
        RowLayout {
            id: footer
            x: 30; width: 430; anchors.bottom: parent.bottom; anchors.bottomMargin: 22
            Rectangle {
                implicitWidth: nl.implicitWidth + 30; implicitHeight: 36; radius: 18
                color: newMa.containsMouse ? Theme.accentSoft : Theme.fill
                Row {
                    id: nl; anchors.centerIn: parent; spacing: 6
                    Icon { name: "plus"; size: 15; color: Theme.accent }
                    Txt { text: "Nowe wydarzenie"; size: 13; w: 600; color: Theme.t1 }
                }
                MouseArea { id: newMa; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { win.open("https://calendar.google.com/calendar/r/eventedit"); Ui.close() } }
            }
            Item { Layout.fillWidth: true }
            Btn { icon: "world"; color: "transparent"; onClicked: { win.open("https://calendar.google.com/calendar/r/day/" + win.selected.getFullYear() + "/" + (win.selected.getMonth() + 1) + "/" + win.selected.getDate()); Ui.close() } }
        }

        // ------------------------------------------------ day plan + upcoming (right)
        Rectangle {
            x: 490; y: 1; width: parent.width - 491; height: parent.height - 2
            radius: Theme.rSheet - 1
            color: Theme.inset
            Rectangle { width: 1; height: parent.height; color: Theme.hair }
            Column {
                x: 26; y: 28; width: parent.width - 52
                spacing: 10
                Label { text: Data.days[win.selected.getDay()] }
                Row {
                    spacing: 8
                    Txt { text: win.selected.getDate(); size: 30; w: 600 }
                    Txt { text: Data.monthsGen[win.selected.getMonth()]; size: 18; w: 500; color: Theme.t2; anchors.baseline: parent.children[0].baseline }
                }
                Repeater {
                    model: Data.eventsOn(win.selected)
                    EventRow { required property var modelData; ev: modelData; width: parent.width }
                }
                Txt {
                    visible: Data.eventsOn(win.selected).length === 0
                    text: "Nic nie zaplanowano"; size: 13; color: Theme.t3
                }
                Rectangle { width: parent.width; height: 1; color: Theme.hair }
                Label { text: "Nadchodzące" }
                Repeater {
                    model: Data.upcoming(6, 60)
                    RowLayout {
                        required property var modelData
                        required property int index
                        readonly property date d: new Date(modelData.start * 1000)
                        readonly property bool firstOfDay: index === 0 || !Data.sameDay(d, new Date(Data.upcoming(6, 60)[index - 1].start * 1000))
                        width: parent.width; spacing: 12
                        Column {
                            Layout.preferredWidth: 30
                            opacity: firstOfDay ? 1 : 0
                            Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Data.daysShort[d.getDay()]; size: 10; w: 600; color: Theme.t3; font.capitalization: Font.AllUppercase }
                            Txt { anchors.horizontalCenter: parent.horizontalCenter; text: d.getDate(); size: 18; w: 600 }
                        }
                        EventRow { ev: modelData; Layout.fillWidth: true; showRel: true }
                    }
                }
            }
        }
    }

    component EventRow: Row {
        property var ev
        property bool showRel: false
        spacing: 10
        Rectangle { width: 3; height: 34; radius: 1.5; color: ev.holiday ? Theme.danger : Theme.accent }
        Column {
            width: parent.width - 13
            Txt { width: parent.width; text: ev.title; size: 13; w: 600 }
            Txt {
                width: parent.width
                text: (parent.parent.showRel ? Data.rel(ev.start) + " · " : "") + Data.when(ev) + (ev.where ? " · " + ev.where : "")
                size: 11; color: Theme.t3
            }
        }
    }
}
