import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets
import ".."

// Phone card of Start (KDE Connect through Phone): phone glyph filled to the battery level, name + status,
// signal / battery readout, equal-width action tiles, latest phone notifications; pairing in place when
// the phone is not paired yet. Files dropped on the card go to the phone.
Rectangle {
    id: card
    property int maxNotes: 2
    property bool more: false                            // second row of actions
    signal done()                                        // an action that leaves Start (file manager, picker, SMS)
    signal expand()                                      // open the phone panel

    implicitHeight: body.implicitHeight + 28
    radius: 22
    color: drop.containsDrag ? Theme.mAccSoft : Qt.rgba(1, 1, 1, 0.045)
    border.width: 1; border.color: drop.containsDrag ? Qt.rgba(138/255, 214/255, 208/255, 0.5) : Theme.hair
    Behavior on color { ColorAnimation { duration: 120 } }
    Rectangle {
        anchors.fill: parent; anchors.margins: 1; radius: 21
        gradient: Gradient { GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.04) } GradientStop { position: 0.4; color: Qt.rgba(1, 1, 1, 0) } }
    }

    // marketing pushes (sales, coupons, "-20 %") stay out of the card; the phone panel still lists them
    function promo(n) { return /promocj|okazj|rabat|zniżk|wyprzeda|kupon|oferta|newsletter|black friday|\bsale\b|-\d+\s?%|\d+\s?% taniej/i.test([n.app, n.title, n.text].join(" ")) }
    readonly property var shownNotes: Phone.notes.filter(n => !promo(n)).slice(0, maxNotes)

    property string hint: ""                             // hovered action / feedback, replaces the status line
    property string said: ""
    function say(t) { said = t; sayT.restart() }
    Timer { id: sayT; interval: 2400; onTriggered: card.said = "" }

    readonly property string status: {
        if (!Phone.ok) return "KDE Connect nie działa"
        if (!Phone.present) return "Brak urządzeń · otwórz KDE Connect na telefonie"
        if (Phone.pairRequested) return "Chce się sparować · klucz " + Phone.main.key
        if (Phone.pairing) return "Potwierdź na telefonie · klucz " + Phone.main.key
        if (!Phone.paired) return Phone.main.reach ? "W zasięgu · niesparowany" : "Niesparowany"
        if (!Phone.online) return "Poza zasięgiem"
        const p = []
        if (Phone.charging) p.push("ładuje")
        if (Phone.network) p.push(Phone.network)
        if (Phone.mount) p.push("pliki podłączone")
        return p.join(" · ") || "Połączono"
    }

    DropArea {
        id: drop
        anchors.fill: parent
        enabled: Phone.has("share")
        onDropped: d => { if (d.hasUrls) { Phone.share(d.urls.map(u => u.toString())); card.say(d.urls.length === 1 ? "Wysłano plik" : "Wysłano pliki (" + d.urls.length + ")") } }
    }

    Column {
        id: body
        x: 14; y: 14; width: parent.width - 28; spacing: 12

        // ------------------------------------------------ device + status
        RowLayout {
            width: parent.width; spacing: 12
            // phone glyph, filled from the bottom to the battery level
            Item {
                implicitWidth: 30; implicitHeight: 30
                Rectangle {
                    anchors.centerIn: parent; width: 20; height: 30; radius: 6
                    color: "transparent"; border.width: 1.5
                    border.color: Phone.online ? Qt.rgba(1, 1, 1, 0.55) : Qt.rgba(1, 1, 1, 0.22)
                    Rectangle {
                        visible: Phone.battery >= 0
                        anchors.bottom: parent.bottom; anchors.bottomMargin: 3; anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 6; radius: 3
                        height: Math.max(2, (parent.height - 6) * Phone.battery / 100)
                        color: Phone.batteryColor(); opacity: 0.85
                        Behavior on height { NumberAnimation { duration: 400 } }
                    }
                    Rectangle { anchors.horizontalCenter: parent.horizontalCenter; y: 3; width: 6; height: 1.5; radius: 1; color: Qt.rgba(1, 1, 1, 0.4); visible: Phone.battery < 0 || Phone.battery < 85 }
                    Icon { anchors.centerIn: parent; visible: Phone.charging; name: "bolt"; size: 11; color: Theme.ink }
                    Icon { anchors.centerIn: parent; visible: !Phone.online; name: "x"; size: 11; color: Theme.mMuted }
                }
            }
            Column {
                Layout.fillWidth: true
                Row {
                    width: parent.width; spacing: 7
                    Txt { id: pn; text: Phone.present ? Phone.name : "Telefon"; size: 13.5; w: 600; width: Math.min(implicitWidth, parent.width - (nb.visible ? nb.width + 7 : 0)) }
                    // unread phone notifications
                    Rectangle {
                        id: nb
                        visible: Phone.noteCount > 0
                        anchors.verticalCenter: pn.verticalCenter
                        width: Math.max(18, nbT.implicitWidth + 10); height: 18; radius: 9
                        color: Theme.mAccSoft; border.width: 1; border.color: Qt.rgba(138/255, 214/255, 208/255, 0.3)
                        Txt { id: nbT; anchors.centerIn: parent; text: Phone.noteCount; size: 10; w: 700; color: Theme.mAcc }
                    }
                }
                Txt {
                    width: parent.width; size: 11
                    text: card.said || card.hint || card.status
                    color: card.said ? Theme.mAcc : card.hint ? Theme.mFg2 : Phone.pairRequested || Phone.pairing ? Theme.mWarn : Theme.mMuted
                }
            }
            // signal + battery readout
            Column {
                visible: Phone.online
                Layout.alignment: Qt.AlignVCenter
                Txt { anchors.right: parent.right; visible: Phone.battery >= 0; text: Phone.battery + "%"; size: 13; w: 600; color: Phone.batteryColor(); font.features: { "tnum": 1 } }
                Icon {
                    anchors.right: parent.right
                    visible: Phone.strength >= 0
                    name: ["barsOff", "bars1", "bars2", "bars3", "bars4", "bars5"][Math.max(0, Math.min(5, Phone.strength + 1))]
                    size: 13; color: Phone.strength <= 0 ? Theme.mBad : Theme.mMuted
                }
            }
            Btn { icon: "layoutGrid"; implicitWidth: 28; implicitHeight: 28; iconSize: 14; color: hovered ? Theme.fill2 : "transparent"; onClicked: card.expand() }
        }

        // ------------------------------------------------ pairing
        Row {
            visible: Phone.present && !Phone.paired
            spacing: 8
            TextBtn { visible: Phone.pairRequested; text: "Akceptuj"; primary: true; onClicked: Phone.accept() }
            TextBtn { visible: Phone.pairRequested || Phone.pairing; text: Phone.pairing ? "Anuluj" : "Odrzuć"; onClicked: Phone.reject() }
            TextBtn { visible: !Phone.pairRequested && !Phone.pairing && Phone.main && Phone.main.reach; text: "Sparuj"; primary: true; onClicked: Phone.pair() }
        }

        // ------------------------------------------------ actions: 3 common ones, the rest behind "…"
        RowLayout {
            visible: Phone.online
            width: parent.width; spacing: 6
            Act { icon: "bellRing"; tip: "Znajdź telefon (dzwoni)"; plugin: "findmyphone"; onClicked: { Phone.ring(); card.say("Telefon dzwoni") } }
            Act { icon: "clipboard"; tip: "Wyślij schowek"; plugin: "clipboard"; onClicked: { Phone.sendClipboard(); card.say("Schowek wysłany") } }
            Act { icon: "fileUp"; tip: "Wyślij pliki (lub upuść je tutaj)"; plugin: "share"; onClicked: { Phone.pickFiles(); card.done() } }
            Act { icon: card.more ? "x" : "dots"; tip: card.more ? "Mniej" : "Więcej akcji"; plugin: ""; visible: true; active: card.more; onClicked: card.more = !card.more }
        }
        RowLayout {
            visible: Phone.online && card.more
            width: parent.width; spacing: 6
            Act { icon: "folderOpen"; tip: "Przeglądaj pliki telefonu"; plugin: "sftp"; onClicked: { Phone.browse(); card.done() } }
            Act { icon: "camera"; tip: "Zrób zdjęcie telefonem"; plugin: "photo"; onClicked: { Phone.photo(); card.say("Zrób zdjęcie na telefonie") } }
            Act { icon: "message"; tip: "Wiadomości SMS"; plugin: "sms"; onClicked: card.expand() }
            Act { icon: "screenOn"; tip: "Ekran telefonu na komputerze"; plugin: ""; visible: Phone.adbOk; onClicked: { Phone.mirror(); card.done() } }
        }

        // ------------------------------------------------ notifications
        Column {
            visible: card.shownNotes.length > 0
            width: parent.width; spacing: 4
            Repeater {
                model: card.shownNotes
                Rectangle {
                    id: nr
                    required property var modelData
                    width: parent.width; height: 46; radius: 13
                    color: nh.hovered ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(1, 1, 1, 0.035)
                    Behavior on color { ColorAnimation { duration: 120 } }
                    HoverHandler { id: nh }
                    Rectangle {
                        x: 9; anchors.verticalCenter: parent.verticalCenter; width: 28; height: 28; radius: 9; color: Theme.mAccSoft
                        ClippingRectangle {
                            anchors.fill: parent; radius: 9; color: "transparent"
                            Image { anchors.fill: parent; visible: status === Image.Ready; source: nr.modelData.icon ? "file://" + nr.modelData.icon : ""; sourceSize: Qt.size(56, 56); asynchronous: true }
                        }
                        Txt { anchors.centerIn: parent; visible: !nr.modelData.icon; text: (nr.modelData.app || "?").charAt(0).toUpperCase(); size: 12; w: 700; color: Theme.mAcc }
                    }
                    Column {
                        x: 46; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 46 - (nh.hovered ? 34 : 10)
                        Row {
                            width: parent.width; spacing: 5
                            Txt { id: na; text: nr.modelData.app; size: 10; w: 700; color: Theme.mMuted; font.capitalization: Font.AllUppercase; font.letterSpacing: 0.5; textFormat: Text.PlainText; width: Math.min(implicitWidth, parent.width * 0.45) }
                            Txt { width: parent.width - na.width - 5; text: nr.modelData.title || ""; size: 11.5; w: 600; textFormat: Text.PlainText }
                        }
                        Txt { width: parent.width; visible: !!nr.modelData.text; text: nr.modelData.text; size: 11; color: Theme.mFg2; textFormat: Text.PlainText }
                    }
                    Btn {
                        visible: nh.hovered
                        anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                        icon: "x"; implicitWidth: 24; implicitHeight: 24; iconSize: 12; color: hovered ? Theme.fill2 : "transparent"
                        onClicked: Phone.dismiss(nr.modelData.id)
                    }
                }
            }
        }
    }

    // drop overlay
    Rectangle {
        anchors.fill: parent; radius: 20
        visible: drop.containsDrag
        color: Qt.rgba(14/255, 20/255, 32/255, 0.82)
        border.width: 1.5; border.color: Theme.mAcc
        Column {
            anchors.centerIn: parent; spacing: 6
            Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "fileUp"; size: 26; color: Theme.mAcc }
            Txt { anchors.horizontalCenter: parent.horizontalCenter; text: "Upuść, aby wysłać na " + Phone.name; size: 12.5; w: 600 }
        }
    }

    component Act: Btn {
        property string tip
        property string plugin
        Layout.fillWidth: true
        implicitHeight: 36; radius: 12; iconSize: 16
        visible: Phone.has(plugin)
        color: hovered || active ? Theme.mAccSoft : Qt.rgba(1, 1, 1, 0.05)
        iconColor: hovered || active ? Theme.mAcc : Theme.mFg
        border.width: 1; border.color: hovered ? Qt.rgba(138/255, 214/255, 208/255, 0.3) : Qt.rgba(1, 1, 1, 0.05)
        onHoveredChanged: card.hint = hovered ? tip : card.hint === tip ? "" : card.hint
    }
    component TextBtn: Rectangle {
        id: tb
        property string text
        property bool primary: false
        signal clicked()
        implicitWidth: tt.implicitWidth + 24; implicitHeight: 30; radius: 10
        color: primary ? (tm.containsMouse ? Theme.mAccStrong : Theme.mAcc) : tm.containsMouse ? Theme.fill2 : Theme.fill
        Txt { id: tt; anchors.centerIn: parent; text: tb.text; size: 12; w: 600; color: tb.primary ? Theme.ink : Theme.mFg }
        MouseArea { id: tm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: tb.clicked() }
    }
}
