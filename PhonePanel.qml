import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Services.Mpris
import "components"

// Phone panel (Ui popup "phone"): everything about the phone in one sheet.
// left  = the phone (battery, network, ADB details), actions, media playing on the phone
// right = tabs: notifications (reply, actions, dismiss), SMS threads and conversations, contacts
// Backend: Phone (KDE Connect over D-Bus, ADB when available).
PanelWindow {
    id: win
    required property var modelData
    screen: modelData
    visible: Ui.popup === "phone" && Ui.screenName === modelData.name
    implicitWidth: 1040; implicitHeight: 760
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "mist-popup"
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
    exclusionMode: ExclusionMode.Normal
    color: "transparent"
    HyprlandFocusGrab { windows: [win]; active: win.visible; onCleared: Ui.close() }

    property string tab: Phone.noteCount > 0 ? "notes" : "sms"
    property var thread: null                           // open conversation: {thread, name, addr}
    property string composeTo: ""                       // new message to this number
    property string said: ""
    function say(t) { said = t; sayT.restart() }
    Timer { id: sayT; interval: 2600; onTriggered: win.said = "" }

    onVisibleChanged: {
        if (!visible) { if (thread) Phone.closeConv(thread.thread); return }
        Phone.send("threads"); Phone.send("contacts")
        sheet.forceActiveFocus()
    }
    // ADB details only while the panel is open (one `adb shell` per 15 s)
    Timer { interval: 15000; running: win.visible && Phone.online; repeat: true; triggeredOnStart: true; onTriggered: Phone.adbInfo() }
    Connections {
        target: Phone
        function onEvent(o) {
            if (!win.visible) return
            if (o.k === "done") win.say(o.text)
            else if (o.k === "err") win.say("⚠ " + o.text)
            else if (o.k === "shot") win.say("Zrzut ekranu telefonu w schowku")
        }
    }

    // phone media: the MPRIS player kdeconnectd makes for the phone (mprisremote plugin)
    readonly property int unread: Phone.threads.filter(t => !t.read).length
    readonly property var dismissable: Phone.notes.filter(n => n.dismiss)
    readonly property var phonePlayer: Mpris.players.values.find(p => (p.dbusName || "").indexOf("kdeconnect") >= 0 && p.trackTitle) || null

    function openThread(t) {
        if (thread) Phone.closeConv(thread.thread)
        thread = t; composeTo = ""; tab = "sms"
        Phone.openConv(t.thread)
    }
    function compose(number) { if (thread) Phone.closeConv(thread.thread); thread = null; composeTo = number; tab = "sms" }
    function when(ms) {
        const d = new Date(ms), n = Data.now
        if (Data.sameDay(d, n)) return Data.hm(d)
        const y = new Date(n.getFullYear(), n.getMonth(), n.getDate() - 1)
        if (Data.sameDay(d, y)) return "wczoraj"
        if (n - d < 6 * 86400000) return Data.daysShort[d.getDay()]
        return d.getDate() + "." + Data.two(d.getMonth() + 1) + (d.getFullYear() !== n.getFullYear() ? "." + d.getFullYear() : "")
    }
    function gb(b) { return (b / 1e9).toFixed(b < 1e10 ? 1 : 0).replace(".", ",") }
    function dur(s) { const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600); return d ? d + " d " + h + " h" : h + " h " + Math.floor(s % 3600 / 60) + " min" }
    function initials(n) { if (/^[+\d]/.test(n || "")) return "#"; return initials2(n) }
    function initials2(n) { const p = (n || "?").replace(/[^0-9A-Za-zÀ-ž ]/g, "").trim().split(/\s+/); return ((p[0] || "?")[0] + (p.length > 1 ? p[1][0] : "")).toUpperCase() }
    function hue(s) { let h = 0; for (const c of (s || "")) h = (h * 31 + c.charCodeAt(0)) % 360; return Qt.hsla(h / 360, 0.45, 0.72, 1) }

    Glass {
        id: sheet
        anchors.fill: parent; anchors.margins: 10
        radius: Theme.rSheet; tint: 0.86
        focus: true
        Keys.onEscapePressed: { if (win.thread || win.composeTo) { if (win.thread) Phone.closeConv(win.thread.thread); win.thread = null; win.composeTo = "" } else Ui.close() }

        // ================================================================ left: the phone
        Item {
            id: left
            x: 34; y: 32; width: 372; height: parent.height - 64

            // ---------------- hero
            Row {
                id: hero
                width: parent.width; spacing: 22
                // the phone itself, filled up to its charge
                Rectangle {
                    id: glyph
                    readonly property color bc: Phone.charging ? Theme.green : Phone.battery >= 0 && Phone.battery <= 15 ? Theme.danger : Phone.battery >= 0 && Phone.battery <= 30 ? Theme.yellow : Theme.mAcc
                    width: 70; height: 124; radius: 17
                    color: Theme.inset; border.width: 2; border.color: Phone.online ? Theme.rim : Theme.hair
                    clip: true
                    Rectangle {
                        anchors.bottom: parent.bottom; anchors.bottomMargin: 4; anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 8; radius: 13
                        height: Phone.battery >= 0 ? Math.max(16, (parent.height - 8) * Phone.battery / 100) : 0
                        gradient: Gradient {
                            GradientStop { position: 0; color: Qt.rgba(glyph.bc.r, glyph.bc.g, glyph.bc.b, 0.34) }
                            GradientStop { position: 1; color: Qt.rgba(glyph.bc.r, glyph.bc.g, glyph.bc.b, 0.12) }
                        }
                        Behavior on height { NumberAnimation { duration: 600; easing.type: Easing.OutCubic } }
                        SequentialAnimation on opacity {
                            running: Phone.charging; loops: Animation.Infinite
                            NumberAnimation { to: 0.55; duration: 1100; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1; duration: 1100; easing.type: Easing.InOutSine }
                            onRunningChanged: if (!running) parent.opacity = 1
                        }
                    }
                    Rectangle { anchors.horizontalCenter: parent.horizontalCenter; y: 8; width: 20; height: 4; radius: 2; color: Theme.rim }
                    Column {
                        anchors.centerIn: parent; spacing: 2
                        Icon { anchors.horizontalCenter: parent.horizontalCenter; name: Phone.online ? (Phone.charging ? "bolt" : "mobile") : "mobileOff"; size: 18; color: Phone.online ? glyph.bc : Theme.t3 }
                        Txt { anchors.horizontalCenter: parent.horizontalCenter; visible: Phone.battery >= 0; text: Phone.battery + "%"; size: 17; w: 680; font.family: Theme.fontRounded }
                    }
                }
                Column {
                    width: parent.width - glyph.width - 22; spacing: 4
                    anchors.verticalCenter: parent.verticalCenter
                    Txt { width: parent.width; text: Phone.present ? Phone.name : "Telefon"; size: 22; w: 650 }
                    Txt {
                        width: parent.width; size: 12; color: Theme.t3
                        text: Phone.adbOk ? [Phone.adb.model, "Android " + Phone.adb.android, Phone.adb.ui].filter(x => x && x !== "Android ").join(" · ")
                            : Phone.online ? "KDE Connect · " + Phone.ip : Phone.present ? (Phone.paired ? "poza zasięgiem" : "niesparowany") : "brak urządzenia"
                    }
                    Flow {
                        width: parent.width; spacing: 6; topPadding: 6
                        Pill { visible: Phone.charging; icon: "charging"; text: "ładuje"; c: Theme.green }
                        Pill { visible: Phone.online && !Phone.charging && Phone.battery >= 0 && Phone.battery <= 15; icon: "battLow"; text: "słaba bateria"; c: Theme.danger }
                        Pill { visible: Phone.strength >= 0; icon: ["barsOff", "bars1", "bars2", "bars3", "bars4", "bars5"][Math.max(0, Math.min(5, Phone.strength + 1))]; text: (Phone.adbOk && Phone.adb.operator ? Phone.adb.operator + " · " : "") + Phone.network }
                        Pill { visible: Phone.adbOk && !!Phone.adb.ssid; icon: "wifi"; text: Phone.adbOk ? Phone.adb.ssid + (Phone.adb.link ? " · " + Phone.adb.link + " Mb/s" : "") : ""; c: Phone.adbOk && Phone.adb.rssi && Phone.adb.rssi < -75 ? Theme.orange : Theme.t1 }
                        Pill { visible: Phone.adbOk && Phone.adb.temp !== null && Phone.adb.temp !== undefined; icon: "temp"; text: Phone.adbOk ? ("" + Phone.adb.temp).replace(".", ",") + "°C" : ""; c: Phone.adbOk && Phone.adb.temp > 40 ? Theme.danger : Theme.t1 }
                        Pill { visible: Phone.adbOk && Phone.adb.dnd; icon: "bellZ"; text: "nie przeszkadzać" }
                        Pill { visible: Phone.adbOk; icon: Phone.adbOk && Phone.adb.wifi ? "link" : "usb"; text: Phone.adbOk && Phone.adb.wifi ? "ADB" : "ADB · USB"; c: Theme.green }
                        Pill { visible: Phone.hfp; icon: "btLink"; text: "rozmowy na PC"; c: Theme.green }
                    }
                }
            }

            // ---------------- pairing / status
            Rectangle {
                id: pairBox
                anchors.top: hero.bottom; anchors.topMargin: 22
                visible: !Phone.online
                width: parent.width; height: visible ? pairCol.implicitHeight + 28 : 0; radius: Theme.rPanel
                color: Theme.fill; border.width: 1; border.color: Theme.hair
                Column {
                    id: pairCol
                    x: 16; y: 14; width: parent.width - 32; spacing: 10
                    Txt {
                        width: parent.width; wrapMode: Text.WordWrap; size: 13; color: Theme.t2
                        text: !Phone.ok ? "KDE Connect nie działa (kdeconnectd)." : !Phone.present ? "Otwórz KDE Connect na telefonie w tej samej sieci." :
                              Phone.pairRequested ? "Telefon chce się sparować. Sprawdź, czy klucz " + Phone.main.key + " jest taki sam na telefonie." :
                              Phone.pairing ? "Zaakceptuj parowanie na telefonie (klucz " + Phone.main.key + ")." :
                              !Phone.paired ? "Telefon w sieci, ale niesparowany." : "Telefon poza zasięgiem. Wróci sam, gdy będzie w tej samej sieci."
                    }
                    Row {
                        spacing: 8
                        TextBtn { visible: Phone.pairRequested; text: "Akceptuj"; primary: true; onClicked: Phone.accept() }
                        TextBtn { visible: Phone.pairRequested || Phone.pairing; text: Phone.pairing ? "Anuluj" : "Odrzuć"; onClicked: Phone.reject() }
                        TextBtn { visible: Phone.present && !Phone.paired && !Phone.pairRequested && !Phone.pairing; text: "Sparuj"; primary: true; onClicked: Phone.pair() }
                    }
                }
            }

            // ---------------- ADB details
            Column {
                id: stats
                anchors.top: pairBox.visible ? pairBox.bottom : hero.bottom; anchors.topMargin: 22
                width: parent.width; spacing: 12
                visible: Phone.online
                Rectangle {
                    visible: Phone.adbOk
                    width: parent.width; height: statCol.implicitHeight + 28; radius: Theme.rPanel
                    color: Theme.fill; border.width: 1; border.color: Theme.hair
                    Column {
                        id: statCol
                        x: 16; y: 14; width: parent.width - 32; spacing: 12
                Row {
                    width: parent.width; spacing: 14
                    Stat { icon: "drive"; label: "Pamięć"; value: Phone.adbOk ? win.gb(Phone.adb.storage[1]) + " / " + win.gb(Phone.adb.storage[0]) + " GB" : ""; frac: Phone.adbOk ? Phone.adb.storage[1] / (Phone.adb.storage[0] || 1) : 0; c: Theme.yellow }
                    Stat { icon: "ram"; label: "RAM"; value: Phone.adbOk ? win.gb(Phone.adb.ram[0] - Phone.adb.ram[1]) + " / " + win.gb(Phone.adb.ram[0]) + " GB" : ""; frac: Phone.adbOk ? 1 - Phone.adb.ram[1] / (Phone.adb.ram[0] || 1) : 0; c: Theme.accent }
                    Stat { icon: "heart"; label: "Bateria"; value: Phone.adbOk ? (Phone.adb.health || "—") + " · " + ("" + Phone.adb.volt).replace(".", ",") + " V" : ""; frac: Phone.battery / 100; c: Phone.batteryColor() }
                }
                Rectangle { width: parent.width; height: 1; color: Theme.hair }
                Row {
                    width: parent.width; spacing: 14
                    InfoBit { icon: "clock"; text: Phone.adbOk ? win.dur(Phone.adb.uptime) : "" }
                    InfoBit { icon: Phone.adbOk && Phone.adb.screen ? "screenOn" : "screenOff"; text: Phone.adbOk && Phone.adb.screen ? "ekran wł." : "ekran wył."; lit: Phone.adbOk && Phone.adb.screen }
                    InfoBit { icon: "network"; text: Phone.ip || (Phone.adbOk ? Phone.adb.serial.split(":")[0] : "") }
                }
                    }
                }
                // no ADB: what to do
                Rectangle {
                    visible: !Phone.adbOk
                    width: parent.width; height: adbHelp.implicitHeight + 22; radius: Theme.rPanel
                    color: Theme.fill; border.width: 1; border.color: Theme.hair
                    Row {
                        id: adbHelp
                        x: 12; y: 11; width: parent.width - 24; spacing: 10
                        Icon { name: "tool"; size: 16; color: Theme.t3 }
                        Txt {
                            width: parent.width - 26; wrapMode: Text.WordWrap; size: 11.5; color: Theme.t3; textFormat: Text.PlainText
                            text: !Phone.adb.have ? "ADB daje więcej (ekran telefonu na PC, zrzuty, dzwonienie, pamięć, temperatura): sudo pacman -S android-tools scrcpy"
                                : "ADB: podłącz telefon kablem z włączonym debugowaniem USB i zezwól na komputer. Potem „ADB przez Wi-Fi” i kabel można odłączyć."
                        }
                    }
                }
            }

            // ---------------- actions
            Label {
                id: actsLbl
                anchors.top: stats.visible ? stats.bottom : pairBox.bottom; anchors.topMargin: 22
                visible: Phone.online; text: "Szybkie akcje"
            }
            Grid {
                id: acts
                anchors.top: actsLbl.bottom; anchors.topMargin: 10
                visible: Phone.online
                width: parent.width; columns: 4; spacing: 8
                Tile { icon: "bellRing"; text: "Znajdź"; on: Phone.has("findmyphone"); onHit: { Phone.ring(); win.say("Telefon dzwoni") } }
                Tile { icon: "clipboard"; text: "Schowek"; on: Phone.has("clipboard"); onHit: { Phone.sendClipboard(); win.say("Schowek wysłany na telefon") } }
                Tile { icon: "fileUp"; text: "Wyślij pliki"; on: Phone.has("share"); onHit: { Phone.pickFiles(); Ui.close() } }
                Tile { icon: "folderOpen"; text: "Pliki"; on: Phone.has("sftp"); onHit: { Phone.browse(); Ui.close() } }
                Tile { icon: "screenOn"; text: "Ekran na PC"; on: Phone.adbOk; onHit: { Phone.mirror(); Ui.close() } }
                Tile { icon: "screenshot"; text: "Zrzut"; on: Phone.adbOk; onHit: Phone.screenshot() }
                Tile { icon: "camera"; text: "Zdjęcie"; on: Phone.has("photo"); onHit: { Phone.photo(); win.say("Zrób zdjęcie na telefonie") } }
                Tile { icon: Phone.adbOk && Phone.adb.screen ? "lock" : "bulb"; text: Phone.adbOk && Phone.adb.screen ? "Zablokuj" : "Wybudź"; on: Phone.adbOk; onHit: { Phone.adb.screen ? Phone.sleepScreen() : Phone.wake(); sleepT.restart() } }
                Tile { icon: "bellZ"; text: "Nie przeszk."; on: Phone.adbOk; lit: Phone.adbOk && Phone.adb.dnd; onHit: { Phone.setDnd(!Phone.adb.dnd); sleepT.restart() } }
                Tile { icon: "wifi"; text: "ADB Wi-Fi"; on: !!Phone.adb.have; lit: Phone.adbOk && Phone.adb.wifi; onHit: { if (lit) win.say("ADB już działa przez Wi-Fi"); else { Phone.adbWifi(); win.say("Włączam ADB przez Wi-Fi…") } } }
                Tile { icon: "shield"; text: "Uprawnienia"; on: Phone.adbOk; onHit: { Phone.adbFix(); win.say("Nadaję uprawnienia KDE Connect…") } }
                Tile { icon: "settings"; text: "KDE Connect"; on: true; onHit: { Phone.app(); Ui.close() } }
            }
            Timer { id: sleepT; interval: 1200; onTriggered: Phone.adbInfo() }

            // ---------------- send a link / text to the phone
            InputBox {
                id: pushIn
                anchors.top: acts.bottom; anchors.topMargin: 14
                visible: Phone.online && Phone.has("share") && (!media.visible || media.y - acts.y - acts.height > 70)
                width: parent.width; hint: "Wyślij link lub tekst na telefon"; sendIcon: true
                onSubmit: t => {
                    if (/^(https?:\/\/|www\.)\S+$/i.test(t)) { Phone.share(t.indexOf("://") < 0 ? "https://" + t : t); win.say("Link otwarty na telefonie") }
                    else { Phone.shareText(t); win.say("Tekst wysłany na telefon") }
                    text = ""
                }
            }

            // ---------------- media on the phone
            Rectangle {
                id: media
                visible: !!win.phonePlayer
                anchors.bottom: parent.bottom
                width: parent.width; height: 88; radius: Theme.rPanel
                color: Theme.fill; border.width: 1; border.color: Theme.hair
                clip: true
                readonly property var p: win.phonePlayer
                readonly property bool timed: !!p && p.positionSupported && p.lengthSupported && p.length > 0
                Timer { interval: 1000; running: win.visible && !!media.p && media.p.isPlaying; repeat: true; onTriggered: if (media.p) media.p.positionChanged() }
                // art as a soft backdrop
                Image {
                    anchors.fill: parent; source: media.p ? media.p.trackArtUrl || "" : ""; fillMode: Image.PreserveAspectCrop
                    sourceSize: Qt.size(64, 64); asynchronous: true; opacity: 0.12; smooth: true
                }
                Rectangle {
                    id: pArt; x: 14; anchors.verticalCenter: parent.verticalCenter; width: 60; height: 60; radius: 12; clip: true; color: Theme.fill2
                    Image { anchors.fill: parent; source: media.p ? media.p.trackArtUrl || "" : ""; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(112, 112); asynchronous: true }
                    Icon { anchors.centerIn: parent; name: "music"; visible: !media.p || !media.p.trackArtUrl }
                }
                Column {
                    anchors.left: pArt.right; anchors.leftMargin: 12; anchors.right: pBtns.left; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                    spacing: 1
                    Label { width: parent.width; size: 10; text: media.p ? "Na telefonie · " + (media.p.identity || "").split(" - ")[0] : "" }
                    Txt { width: parent.width; text: media.p ? media.p.trackTitle : ""; size: 13.5; w: 620 }
                    Txt { width: parent.width; text: media.p ? media.p.trackArtist : ""; size: 11.5; color: Theme.t2 }
                    Meter {
                        visible: media.timed; width: parent.width; implicitHeight: 3
                        value: media.timed ? media.p.position / media.p.length : 0; barColor: Theme.t1
                    }
                }
                Row {
                    id: pBtns
                    anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                    Btn { icon: "prev"; implicitWidth: 30; implicitHeight: 30; color: "transparent"; onClicked: Phone.media("Previous") }
                    Btn { icon: media.p && media.p.isPlaying ? "pause" : "play"; implicitWidth: 34; implicitHeight: 34; active: true; onClicked: Phone.media("PlayPause") }
                    Btn { icon: "next"; implicitWidth: 30; implicitHeight: 30; color: "transparent"; onClicked: Phone.media("Next") }
                }
            }
        }

        // ================================================================ right: notifications, SMS, contacts
        Rectangle {
            id: right
            x: 440; y: 1; width: parent.width - 441; height: parent.height - 2
            radius: Theme.rSheet - 1; color: Theme.inset
            Rectangle { width: 1; height: parent.height; color: Theme.hair }

            // tabs
            Row {
                id: tabs
                x: 26; y: 28; spacing: 6
                TabBtn { key: "notes"; text: "Powiadomienia" + (Phone.noteCount ? " " + Phone.noteCount : "") }
                TabBtn { key: "sms"; text: "Wiadomości" + (win.unread ? " " + win.unread : "") }
                TabBtn { key: "people"; text: "Kontakty" + (Phone.contacts.length ? " " + Phone.contacts.length : "") }
            }
            TextBtn {
                anchors.right: parent.right; anchors.rightMargin: 26; anchors.verticalCenter: tabs.verticalCenter
                visible: win.tab === "notes" && win.dismissable.length > 1
                text: "Wyczyść wszystko"; small: true
                onClicked: { for (const n of win.dismissable) Phone.dismiss(n.id); win.say("Powiadomienia usunięte") }
            }

            // toast
            Rectangle {
                id: toast
                z: 10
                readonly property bool bad: win.said.startsWith("⚠")
                anchors.horizontalCenter: parent.horizontalCenter
                y: win.said ? parent.height - height - 22 : parent.height + 10
                width: toastRow.implicitWidth + 28; height: 36; radius: 18
                color: Qt.rgba(28/255, 34/255, 46/255, 0.96); border.width: 1; border.color: Theme.rim
                opacity: win.said ? 1 : 0
                Behavior on y { NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }
                Behavior on opacity { NumberAnimation { duration: 200 } }
                Row {
                    id: toastRow; anchors.centerIn: parent; spacing: 8
                    Icon { name: toast.bad ? "alert" : "check"; size: 15; color: toast.bad ? Theme.orange : Theme.green; anchors.verticalCenter: parent.verticalCenter }
                    Txt { text: win.said.replace(/^⚠ /, ""); size: 12.5; w: 560; anchors.verticalCenter: parent.verticalCenter }
                }
            }

            Item {
                id: page
                x: 26; y: tabs.y + tabs.height + 18; width: parent.width - 52; height: parent.height - y - 24

                // ------------------------------------------------ notifications
                ListView {
                    id: noteList
                    anchors.fill: parent; clip: true; spacing: 8
                    visible: win.tab === "notes"
                    model: Phone.notes
                    boundsBehavior: Flickable.StopAtBounds
                    bottomMargin: 56
                    add: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 220 } }
                    remove: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; to: 0; duration: 180 } NumberAnimation { property: "x"; to: 60; duration: 180; easing.type: Easing.InCubic } } }
                    displaced: Transition { NumberAnimation { property: "y"; duration: 220; easing.type: Easing.OutCubic } }
                    delegate: Rectangle {
                        id: nd
                        required property var modelData
                        readonly property string body: (modelData.text || "") === (modelData.title || "") ? "" : modelData.text || ""
                        width: noteList.width; height: ndCol.implicitHeight + 26; radius: Theme.rPanel
                        color: ndm.containsMouse ? Theme.fill2 : Theme.fill
                        Behavior on color { ColorAnimation { duration: 120 } }
                        border.width: 1; border.color: Theme.hair
                        property bool replying: false
                        MouseArea { id: ndm; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                        Column {
                            id: ndCol
                            x: 14; y: 13; width: parent.width - 28; spacing: 6
                            Row {
                                width: parent.width; spacing: 12
                                Rectangle {
                                    width: 36; height: 36; radius: 10; clip: true
                                    color: Qt.rgba(win.hue(nd.modelData.app).r, win.hue(nd.modelData.app).g, win.hue(nd.modelData.app).b, 0.18)
                                    Image { id: ni; anchors.fill: parent; source: nd.modelData.icon ? "file://" + nd.modelData.icon : ""; sourceSize: Qt.size(72, 72); asynchronous: true; fillMode: Image.PreserveAspectCrop }
                                    Txt { anchors.centerIn: parent; visible: ni.status !== Image.Ready; text: (nd.modelData.app || "?").charAt(0).toUpperCase(); size: 14; w: 700; color: win.hue(nd.modelData.app) }
                                }
                                Column {
                                    width: parent.width - 48 - 30; spacing: 1
                                    anchors.verticalCenter: parent.verticalCenter
                                    Row {
                                        width: parent.width; spacing: 6
                                        Txt { id: ndApp; width: Math.min(implicitWidth, parent.width - ndAt.width - 6); text: nd.modelData.app; size: 11; w: 600; color: Theme.t2 }
                                        Txt { id: ndAt; text: "· " + win.when(nd.modelData.at * 1000); size: 11; color: Theme.t3 }
                                    }
                                    Txt { width: parent.width; text: nd.modelData.title || nd.modelData.app; size: 13.5; w: 640; textFormat: Text.PlainText }
                                }
                                Rectangle {
                                    visible: nd.modelData.dismiss
                                    width: 26; height: 26; radius: 13
                                    color: ndx.containsMouse ? Theme.fill2 : "transparent"
                                    opacity: ndm.containsMouse || ndx.containsMouse ? 1 : 0.45
                                    Behavior on opacity { NumberAnimation { duration: 120 } }
                                    Icon { anchors.centerIn: parent; name: "x"; size: 14; color: ndx.containsMouse ? Theme.t1 : Theme.t2 }
                                    MouseArea { id: ndx; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Phone.dismiss(nd.modelData.id) }
                                }
                            }
                            Txt { visible: !!nd.body; width: parent.width; text: nd.body; size: 12.5; color: Theme.t2; wrapMode: Text.Wrap; maximumLineCount: 4; textFormat: Text.PlainText; leftPadding: 48; lineHeight: 1.12 }
                            Flow {
                                visible: nd.modelData.reply || (nd.modelData.actions || []).length > 0
                                width: parent.width; spacing: 6; leftPadding: 48; topPadding: 4
                                TextBtn { visible: nd.modelData.reply && !nd.replying; text: "Odpowiedz"; small: true; onClicked: { nd.replying = true; rIn.focusInput() } }
                                Repeater {
                                    model: (nd.modelData.actions || []).slice(0, 3)
                                    TextBtn { required property var modelData; text: modelData; small: true; onClicked: { Phone.action(nd.modelData.id, modelData); win.say("Wysłano: " + modelData) } }
                                }
                            }
                            InputBox {
                                id: rIn
                                visible: nd.replying
                                width: parent.width; hint: "Odpowiedź do " + (nd.modelData.title || nd.modelData.app)
                                onSubmit: t => { Phone.reply(nd.modelData.id, t); nd.replying = false; win.say("Odpowiedź wysłana") }
                                onCancel: nd.replying = false
                            }
                        }
                    }
                    Empty {
                        parent: noteList
                        visible: noteList.count === 0
                        icon: "bell"; text: Phone.online ? "Brak powiadomień z telefonu" : "Telefon niepołączony"
                        sub: Phone.online && Phone.has("notifications") ? "Jeśli powinny tu być: daj KDE Connect dostęp do powiadomień na telefonie" + (Phone.adbOk ? " albo kliknij „Uprawnienia”." : ".") : ""
                    }
                }

                // ------------------------------------------------ SMS: thread list
                Item {
                    anchors.fill: parent
                    visible: win.tab === "sms" && !win.thread && !win.composeTo
                    TextBtn { id: newSms; text: "Nowa wiadomość"; primary: true; small: true; visible: Phone.has("sms"); onClicked: win.compose(" ") }
                    ListView {
                        id: thList
                        anchors.fill: parent; anchors.topMargin: newSms.visible ? 40 : 0
                        clip: true; spacing: 2
                        boundsBehavior: Flickable.StopAtBounds
                        model: Phone.threads
                        delegate: Rectangle {
                            id: td
                            required property var modelData
                            readonly property string nm: Phone.contactName(modelData.addr[0]) || modelData.name
                            width: thList.width; height: 58; radius: Theme.rRow
                            color: tdm.containsMouse ? Theme.fill2 : "transparent"
                            Rectangle {
                                x: 8; anchors.verticalCenter: parent.verticalCenter; width: 40; height: 40; radius: 20
                                color: Qt.rgba(win.hue(td.nm).r, win.hue(td.nm).g, win.hue(td.nm).b, 0.2)
                                Txt { anchors.centerIn: parent; text: win.initials(td.nm); size: 13; w: 700; color: win.hue(td.nm) }
                            }
                            Column {
                                x: 60; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 60 - 64
                                Txt { width: parent.width; text: td.nm; size: 13; w: td.modelData.read ? 600 : 750 }
                                Txt { width: parent.width; text: (td.modelData.out ? "Ty: " : "") + td.modelData.body + (td.modelData.att ? " 📎" : ""); size: 12; color: td.modelData.read ? Theme.t3 : Theme.t1; textFormat: Text.PlainText }
                            }
                            Column {
                                anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                                Txt { anchors.right: parent.right; text: win.when(td.modelData.date); size: 11; color: Theme.t3 }
                                Rectangle { anchors.right: parent.right; visible: !td.modelData.read; width: 8; height: 8; radius: 4; color: Theme.accent }
                            }
                            MouseArea { id: tdm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.openThread({ thread: td.modelData.thread, name: td.nm, addr: td.modelData.addr }) }
                        }
                        Empty { parent: thList; visible: thList.count === 0; icon: "message"; text: Phone.has("sms") ? "Wczytuję wiadomości…" : "SMS niedostępne"; sub: Phone.has("sms") ? "" : "Włącz wtyczkę SMS w KDE Connect na telefonie" }
                    }
                }

                // ------------------------------------------------ SMS: one conversation / a new message
                Item {
                    id: conv
                    anchors.fill: parent
                    visible: win.tab === "sms" && (!!win.thread || !!win.composeTo)
                    readonly property var msgs: win.thread ? Phone.convs[win.thread.thread] || [] : []
                    Row {
                        id: convHead
                        width: parent.width; spacing: 10
                        Btn { icon: "left"; implicitWidth: 32; implicitHeight: 32; onClicked: { if (win.thread) Phone.closeConv(win.thread.thread); win.thread = null; win.composeTo = "" } }
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 42 - (callBtn.visible ? 42 : 0)
                            Txt { width: parent.width; text: win.thread ? win.thread.name : "Nowa wiadomość"; size: 15; w: 650 }
                            Txt { width: parent.width; visible: !!win.thread; text: win.thread ? win.thread.addr.join(", ") : ""; size: 11; color: Theme.t3 }
                        }
                        Btn {
                            id: callBtn; icon: "call"; implicitWidth: 32; implicitHeight: 32
                            visible: (Phone.adbOk || Phone.hfp) && !!win.thread && win.thread.addr.length === 1 && /\d{6,}/.test(win.thread.addr[0])
                            onClicked: Phone.callNumber(win.thread.addr[0])
                        }
                    }
                    InputBox {
                        id: toIn
                        anchors.top: convHead.bottom; anchors.topMargin: 12
                        visible: !win.thread; width: parent.width; hint: "Do: numer lub imię z kontaktów"
                        text: win.composeTo.trim()
                        onSubmit: t => msgIn.focusInput()
                    }
                    // contacts matching the "to" field
                    Column {
                        id: toHints
                        anchors.top: toIn.bottom; anchors.topMargin: 6
                        visible: !win.thread && toIn.text.length > 1 && !/^\+?[\d\s-]{6,}$/.test(toIn.text)
                        width: parent.width
                        Repeater {
                            model: toHints.visible ? Phone.contacts.filter(c => c.name.toLowerCase().indexOf(toIn.text.toLowerCase()) >= 0).slice(0, 5) : []
                            Rectangle {
                                required property var modelData
                                width: toHints.width; height: 36; radius: 10; color: thm.containsMouse ? Theme.fill2 : "transparent"
                                Txt { x: 12; anchors.verticalCenter: parent.verticalCenter; text: parent.modelData.name + "  ·  " + parent.modelData.tels[0]; size: 12.5 }
                                MouseArea { id: thm; anchors.fill: parent; hoverEnabled: true; onClicked: { toIn.text = parent.modelData.tels[0]; msgIn.focusInput() } }
                            }
                        }
                    }
                    ListView {
                        id: msgList
                        anchors.top: win.thread ? convHead.bottom : toHints.visible ? toHints.bottom : toIn.bottom; anchors.topMargin: 12
                        anchors.bottom: msgIn.top; anchors.bottomMargin: 12
                        width: parent.width; clip: true; spacing: 6
                        boundsBehavior: Flickable.StopAtBounds
                        model: conv.msgs
                        onCountChanged: Qt.callLater(positionViewAtEnd)
                        delegate: Item {
                            required property var modelData
                            required property int index
                            width: msgList.width; height: bub.height + (stamp.visible ? 18 : 0)
                            Txt {
                                id: stamp
                                readonly property var prev: index > 0 ? conv.msgs[index - 1] : null
                                visible: !prev || modelData.date - prev.date > 3600000
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: win.when(modelData.date) + (Data.sameDay(new Date(modelData.date), Data.now) ? "" : " " + Data.hm(new Date(modelData.date)))
                                size: 10.5; color: Theme.t4
                            }
                            Rectangle {
                                id: bub
                                y: stamp.visible ? 18 : 0
                                anchors.right: modelData.out ? parent.right : undefined
                                width: Math.min(parent.width * 0.78, bt.implicitWidth + 26); height: bt.implicitHeight + 16
                                radius: 16
                                color: modelData.out ? Theme.accent : Theme.fill2
                                Text {
                                    id: bt
                                    x: 13; y: 8; width: Math.min(implicitWidth, msgList.width * 0.78 - 26)
                                    text: modelData.body + (modelData.att ? (modelData.body ? "\n" : "") + "📎 załącznik" : "")
                                    wrapMode: Text.Wrap; textFormat: Text.PlainText
                                    color: modelData.out ? Theme.ink : Theme.t1
                                    font.family: Theme.font; font.pixelSize: 13
                                }
                            }
                        }
                        Empty { parent: msgList; visible: !!win.thread && msgList.count === 0; icon: "message"; text: "Wczytuję rozmowę…"; sub: "" }
                    }
                    InputBox {
                        id: msgIn
                        anchors.bottom: parent.bottom
                        width: parent.width; hint: "Wiadomość SMS"; sendIcon: true
                        onSubmit: t => {
                            if (win.thread) Phone.sendSms(win.thread.thread, t)
                            else if (toIn.text.trim()) { Phone.sendSmsTo(toIn.text.trim(), t); win.composeTo = ""; win.tab = "sms" }
                            text = ""; win.say("SMS wysłany")
                        }
                    }
                }

                // ------------------------------------------------ contacts
                Item {
                    anchors.fill: parent
                    visible: win.tab === "people"
                    InputBox { id: pq; width: parent.width; hint: "Szukaj kontaktu"; onSubmit: t => {} }
                    ListView {
                        id: pList
                        anchors.fill: parent; anchors.topMargin: 52
                        clip: true; spacing: 2
                        boundsBehavior: Flickable.StopAtBounds
                        model: { const q = pq.text.toLowerCase().trim(); return q ? Phone.contacts.filter(c => c.name.toLowerCase().indexOf(q) >= 0 || c.tels.some(t => t.replace(/\s/g, "").indexOf(q) >= 0)) : Phone.contacts }
                        delegate: Rectangle {
                            id: pd
                            required property var modelData
                            width: pList.width; height: 52; radius: Theme.rRow
                            color: pdm.containsMouse ? Theme.fill2 : "transparent"
                            MouseArea { id: pdm; anchors.fill: parent; hoverEnabled: true }
                            Rectangle {
                                x: 8; anchors.verticalCenter: parent.verticalCenter; width: 36; height: 36; radius: 18
                                color: Qt.rgba(win.hue(pd.modelData.name).r, win.hue(pd.modelData.name).g, win.hue(pd.modelData.name).b, 0.2)
                                Txt { anchors.centerIn: parent; text: win.initials(pd.modelData.name); size: 12; w: 700; color: win.hue(pd.modelData.name) }
                            }
                            Column {
                                x: 56; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 56 - 84
                                Txt { width: parent.width; text: pd.modelData.name; size: 13; w: 600 }
                                Txt { width: parent.width; text: pd.modelData.tels.join(" · "); size: 11.5; color: Theme.t3 }
                            }
                            Row {
                                anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                                Btn { icon: "message"; implicitWidth: 32; implicitHeight: 32; color: "transparent"; visible: Phone.has("sms"); onClicked: win.compose(pd.modelData.tels[0]) }
                                Btn { icon: "call"; implicitWidth: 32; implicitHeight: 32; color: "transparent"; visible: Phone.adbOk || Phone.hfp; onClicked: { Phone.callNumber(pd.modelData.tels[0]); win.say("Dzwonię: " + pd.modelData.name) } }
                            }
                        }
                        Empty {
                            parent: pList; visible: pList.count === 0; icon: "phone"; text: Phone.contacts.length ? "Nic nie znaleziono" : "Brak kontaktów"
                            sub: Phone.contacts.length ? "" : "KDE Connect na telefonie potrzebuje dostępu do kontaktów" + (Phone.adbOk ? " — kliknij „Uprawnienia”." : ".")
                        }
                    }
                }
            }
        }
    }

    // ================================================================ components
    component Pill: Rectangle {
        property string icon
        property string text
        property color c: Theme.t1
        width: pr.implicitWidth + 16; height: 24; radius: 12
        color: Theme.fill; border.width: 1; border.color: Theme.hair
        Row {
            id: pr; anchors.centerIn: parent; spacing: 5
            Icon { name: parent.parent.icon; size: 13; color: parent.parent.c; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: parent.parent.text; size: 11.5; w: 600; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
        }
    }
    component Stat: Column {
        id: st
        property string icon
        property string label
        property string value
        property real frac
        property color c
        width: (parent.width - 28) / 3; spacing: 5
        Row {
            spacing: 5
            Icon { name: st.icon; size: 12; color: st.c; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: st.label; size: 11; w: 550; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
        }
        Txt { width: parent.width; text: st.value; size: 13; w: 640; font.family: Theme.fontRounded }
        Meter { width: parent.width; value: st.frac; barColor: st.c }
    }
    component InfoBit: Item {
        id: ft
        property string icon
        property string text
        property bool lit: false
        visible: !!text
        implicitWidth: ftr.implicitWidth; implicitHeight: ftr.implicitHeight
        Row {
            id: ftr; spacing: 5
            Icon { name: ft.icon; size: 12; color: ft.lit ? Theme.green : Theme.t3; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: ft.text; size: 11; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
        }
    }
    component Tile: Rectangle {
        id: tl
        property string icon
        property string text
        property bool on: true
        property bool lit: false
        signal hit()
        width: (acts.width - 24) / 4; height: 62; radius: Theme.rRow
        opacity: on ? 1 : 0.35
        color: lit ? Theme.accentSoft : tlm.containsMouse && on ? Theme.fill2 : Theme.fill
        border.width: 1; border.color: lit ? Qt.rgba(169/255, 194/255, 238/255, 0.4) : Theme.hair
        scale: tlm.pressed && on ? 0.96 : 1
        Behavior on scale { NumberAnimation { duration: 100 } }
        Column {
            anchors.centerIn: parent; spacing: 6
            Icon { anchors.horizontalCenter: parent.horizontalCenter; name: tl.icon; size: 19; color: tl.lit ? Theme.accent : Theme.t1 }
            Txt { anchors.horizontalCenter: parent.horizontalCenter; text: tl.text; size: 11; w: 550; color: Theme.t2 }
        }
        MouseArea { id: tlm; anchors.fill: parent; hoverEnabled: true; cursorShape: tl.on ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: if (tl.on) tl.hit() }
    }
    component TabBtn: Rectangle {
        id: tb
        property string key
        property string text
        width: tbt.implicitWidth + 26; height: 32; radius: 16
        color: win.tab === key ? Theme.fill2 : tbm.containsMouse ? Theme.fill : "transparent"
        border.width: win.tab === key ? 1 : 0; border.color: Theme.rim
        Txt { id: tbt; anchors.centerIn: parent; text: tb.text; size: 12.5; w: 600; color: win.tab === tb.key ? Theme.t1 : Theme.t3 }
        MouseArea { id: tbm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.tab = tb.key }
    }
    component TextBtn: Rectangle {
        id: xb
        property string text
        property bool primary: false
        property bool small: false
        signal clicked()
        width: xt.implicitWidth + (small ? 20 : 26); height: small ? 28 : 32; radius: small ? 9 : 11
        color: primary ? (xm.containsMouse ? Theme.accentStrong : Theme.accent) : xm.containsMouse ? Theme.fill2 : Theme.fill
        border.width: primary ? 0 : 1; border.color: Theme.hair
        Txt { id: xt; anchors.centerIn: parent; text: xb.text; size: xb.small ? 11.5 : 12.5; w: 600; color: xb.primary ? Theme.ink : Theme.t1 }
        MouseArea { id: xm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: xb.clicked() }
    }
    component InputBox: Rectangle {
        id: ib
        property string hint
        property alias text: ti.text
        property bool sendIcon: false
        signal submit(string t)
        signal cancel()
        function focusInput() { ti.forceActiveFocus() }
        height: 42; radius: 14
        color: Theme.fill; border.width: 1
        border.color: ti.activeFocus ? Qt.rgba(169/255, 194/255, 238/255, 0.45) : Theme.rim
        TextInput {
            id: ti
            x: 14; width: parent.width - 28 - (ib.sendIcon ? 34 : 0); anchors.verticalCenter: parent.verticalCenter
            color: Theme.t1; font.family: Theme.font; font.pixelSize: 14
            selectionColor: Theme.accentSoft; clip: true
            Keys.onReturnPressed: if (text.trim()) ib.submit(text.trim())
            Keys.onEscapePressed: e => { if (text) text = ""; else { ib.cancel(); e.accepted = false } }
            Txt { visible: !parent.text; text: ib.hint; size: 13.5; color: Theme.t4; anchors.verticalCenter: parent.verticalCenter; width: parent.width }
        }
        Btn {
            visible: ib.sendIcon
            anchors.right: parent.right; anchors.rightMargin: 5; anchors.verticalCenter: parent.verticalCenter
            icon: "send"; implicitWidth: 32; implicitHeight: 32; active: ti.text.trim() !== ""
            onClicked: if (ti.text.trim()) ib.submit(ti.text.trim())
        }
    }
    component Empty: Column {
        property string icon
        property string text
        property string sub
        anchors.centerIn: parent; width: parent.width * 0.8; spacing: 8
        Icon { anchors.horizontalCenter: parent.horizontalCenter; name: parent.icon; size: 30; color: Theme.t4 }
        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: parent.text; size: 14; w: 600; color: Theme.t3 }
        Txt { width: parent.width; visible: !!parent.sub; text: parent.sub; size: 12; color: Theme.t4; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter }
    }
}
