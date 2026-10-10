import QtQuick
import QtQuick.Effects
import Quickshell.Widgets

// macOS lock / login screen, shared by Mist's session lock (../Lock.qml) and the greetd greeter (greeter.qml):
// wallpaper, date above big Liquid Glass numerals at the top, avatar + name + password capsule at the bottom.
// No auth logic here: the host listens to submit(password) and answers with busy / failTick / message.
// Self-contained (no Mist singletons) so the directory can be copied to /usr/share/mist-greeter.
Item {
    id: root
    property url wallpaper
    property string userName: ""
    property url avatar                  // empty -> initials
    property bool primary: true          // other monitors: wallpaper + clock only
    property bool login: false           // greeter: sleep / restart / shut down row
    property bool busy: false
    property int failTick: 0             // bumped by the host on a wrong password -> shake + clear
    property string message: ""          // hint under the field (PAM / greetd text, errors)
    property string hint: login ? "Wpisz hasło, aby się zalogować" : "Naciśnij Enter, aby odblokować"
    property string font: "SF Pro"
    signal submit(string password)
    signal power(string action)          // "suspend" | "reboot" | "poweroff"

    readonly property string iconFont: tabler.name
    FontLoader { id: tabler; source: "file:///usr/share/noctalia/assets/fonts/noctalia-tabler.ttf" }
    readonly property var ic: ({ right: "", power: "", reload: "", zzz: "", loader: "" })

    // awake after the first key / mouse move: the wallpaper blurs and dims like on macOS
    property bool awake: false
    Timer { id: doze; interval: 30000; onTriggered: if (field.text === "" && !root.busy) root.awake = false }
    function wake() { awake = true; doze.restart() }

    function focusField() { if (primary) field.forceActiveFocus() }
    Component.onCompleted: focusField()

    onFailTickChanged: { field.text = ""; shake.restart(); wake() }
    onBusyChanged: if (!busy) focusField()

    // ------------------------------------------------------------------ clock (minute boundary tick)
    property date now: new Date()
    Timer {
        running: true; repeat: false
        interval: 60000 - (Date.now() % 60000) + 50
        onTriggered: { root.now = new Date(); interval = 60000 - (Date.now() % 60000) + 50; start() }
    }
    readonly property var days: ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"]
    readonly property var months: ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"]
    readonly property string dateText: { const d = root.now, w = days[d.getDay()]; return w.charAt(0).toUpperCase() + w.slice(1) + ", " + d.getDate() + " " + months[d.getMonth()] }
    readonly property string timeText: root.now.getHours() + ":" + (root.now.getMinutes() < 10 ? "0" : "") + root.now.getMinutes()

    component Label: Text {
        property real size: 14
        property int w: 500
        color: "white"
        font.family: root.font
        font.pixelSize: size
        font.weight: w
        font.variableAxes: ({ "wght": w, "opsz": Math.max(8, Math.min(36, size)) })
        renderType: Text.QtRendering
        style: Text.Raised; styleColor: Qt.rgba(0, 0, 0, 0.18)
    }
    component Glyph: Text {
        property real size: 16
        color: "white"
        font.family: root.iconFont
        font.pixelSize: size
        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
    }

    // ------------------------------------------------------------------ wallpaper
    Rectangle { anchors.fill: parent; color: "#0b0f18" }
    Image {
        id: wall
        anchors.fill: parent
        source: root.wallpaper
        fillMode: Image.PreserveAspectCrop
        asynchronous: true; cache: false
        sourceSize: Qt.size(root.width, root.height)
        visible: false
    }
    MultiEffect {
        anchors.fill: parent
        source: wall
        autoPaddingEnabled: false
        blurEnabled: true; blurMax: 64
        blur: root.awake ? 0.85 : 0.0
        saturation: root.awake ? 0.12 : 0
        brightness: root.awake ? -0.08 : 0
        opacity: wall.status === Image.Ready ? 1 : 0
        Behavior on blur { NumberAnimation { duration: 450; easing.type: Easing.OutCubic } }
        Behavior on brightness { NumberAnimation { duration: 450 } }
        Behavior on opacity { NumberAnimation { duration: 600 } }
    }
    // soft vignette top and bottom so the white text reads on bright wallpapers
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.rgba(0, 0, 0, 0.22) }
            GradientStop { position: 0.32; color: Qt.rgba(0, 0, 0, 0) }
            GradientStop { position: 0.7; color: Qt.rgba(0, 0, 0, 0) }
            GradientStop { position: 1.0; color: Qt.rgba(0, 0, 0, 0.30) }
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        property point last
        onPositionChanged: m => { if (Math.abs(m.x - last.x) + Math.abs(m.y - last.y) > 6) root.wake(); last = Qt.point(m.x, m.y) }
        onPressed: { root.wake(); root.focusField() }
    }

    // ------------------------------------------------------------------ date + glass time
    Column {
        id: clock
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round(root.height * 0.075)
        spacing: 0
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.dateText; size: 24; w: 600; opacity: 0.92
            style: Text.Normal
            layer.enabled: true
            layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "black"; shadowOpacity: 0.25; shadowBlur: 0.5; shadowVerticalOffset: 1 }
        }
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: tMask.implicitWidth; height: tMask.implicitHeight
            Label {
                id: tMask
                text: root.timeText; size: 176; w: 700
                style: Text.Normal
                font.variableAxes: ({ "wght": 700, "opsz": 36 }); font.letterSpacing: -5
                topPadding: -20; bottomPadding: -24
            }
            // glass numerals: bright lens top, clearer middle, cool lit bottom (glasstext.frag)
            ShaderEffect {
                anchors.fill: tMask
                property var src: ShaderEffectSource { sourceItem: tMask; hideSource: true }
                property color cTop: Qt.rgba(1, 1, 1, 0.97)
                property color cMid: Qt.rgba(1, 1, 1, 0.84)
                property color cBot: Qt.rgba(220/255, 232/255, 1, 0.74)
                fragmentShader: Qt.resolvedUrl("glasstext.frag.qsb")
                layer.enabled: true
                layer.effect: MultiEffect {
                    shadowEnabled: true; shadowColor: Qt.rgba(2/255, 6/255, 16/255, 1)
                    shadowOpacity: 0.16; shadowBlur: 1.0; blurMax: 40
                    shadowVerticalOffset: 6; autoPaddingEnabled: true
                }
            }
        }
    }

    // ------------------------------------------------------------------ user + password
    Column {
        id: who
        visible: root.primary
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.login ? Math.round(root.height * 0.15) : Math.round(root.height * 0.09)
        spacing: 0

        // avatar: picture or initials on the macOS grey gradient
        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 72; height: 72
            Rectangle {
                anchors.fill: parent; radius: width / 2
                gradient: Gradient {
                    GradientStop { position: 0; color: "#a9b1be" }
                    GradientStop { position: 1; color: "#6c7482" }
                }
                visible: pic.status !== Image.Ready
                Label {
                    anchors.centerIn: parent
                    text: root.userName.split(" ").filter(s => s).slice(0, 2).map(s => s.charAt(0).toUpperCase()).join("")
                    size: 28; w: 600; style: Text.Normal
                }
            }
            ClippingRectangle {
                anchors.fill: parent; radius: width / 2
                color: "transparent"
                visible: pic.status === Image.Ready
                Image { id: pic; anchors.fill: parent; source: root.avatar; fillMode: Image.PreserveAspectCrop; sourceSize: Qt.size(144, 144) }
            }
            layer.enabled: true
            layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "black"; shadowOpacity: 0.28; shadowBlur: 0.6; shadowVerticalOffset: 2 }
        }
        Item { width: 1; height: 12 }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.userName; size: 17; w: 600
        }
        Item { width: 1; height: 14 }

        // password capsule
        Item {
            id: capsule
            anchors.horizontalCenter: parent.horizontalCenter
            width: 232; height: 34
            transform: Translate { id: shakeX }
            SequentialAnimation {
                id: shake
                NumberAnimation { target: shakeX; property: "x"; to: -14; duration: 55; easing.type: Easing.OutQuad }
                NumberAnimation { target: shakeX; property: "x"; to: 12; duration: 80 }
                NumberAnimation { target: shakeX; property: "x"; to: -9; duration: 75 }
                NumberAnimation { target: shakeX; property: "x"; to: 6; duration: 70 }
                NumberAnimation { target: shakeX; property: "x"; to: -3; duration: 60 }
                NumberAnimation { target: shakeX; property: "x"; to: 0; duration: 55 }
            }

            Rectangle {
                anchors.fill: parent; radius: height / 2
                color: Qt.rgba(1, 1, 1, field.activeFocus ? 0.20 : 0.15)
                border.width: 1; border.color: Qt.rgba(1, 1, 1, field.activeFocus ? 0.34 : 0.22)
                Behavior on color { ColorAnimation { duration: 180 } }
                // lit top rim of the glass
                Rectangle {
                    anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: parent.radius; rightMargin: parent.radius; topMargin: 1 }
                    height: 1; color: Qt.rgba(1, 1, 1, 0.22)
                }
            }
            TextInput {
                id: field
                anchors { left: parent.left; right: go.left; verticalCenter: parent.verticalCenter; leftMargin: 16; rightMargin: 6 }
                echoMode: TextInput.Password
                passwordCharacter: "●"
                passwordMaskDelay: 0
                color: "white"
                font.family: root.font; font.pixelSize: text ? 11 : 14; font.letterSpacing: text ? 2.5 : 0
                selectionColor: Qt.rgba(1, 1, 1, 0.3)
                clip: true
                readOnly: root.busy
                focus: root.primary
                cursorDelegate: Rectangle { width: 1.5; color: Qt.rgba(1, 1, 1, 0.85); visible: field.activeFocus && !root.busy
                    SequentialAnimation on opacity { loops: Animation.Infinite; running: field.activeFocus
                        NumberAnimation { to: 0; duration: 500; easing.type: Easing.InQuad } NumberAnimation { to: 1; duration: 500; easing.type: Easing.OutQuad } } }
                onTextChanged: root.wake()
                Keys.onPressed: e => {
                    root.wake()
                    if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { if (text !== "" && !root.busy) root.submit(text); e.accepted = true }
                    else if (e.key === Qt.Key_Escape) { text = ""; e.accepted = true }
                }
                Label {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: field.text === ""
                    text: "Wpisz hasło"; size: 14; w: 450; opacity: 0.62; style: Text.Normal
                }
            }
            // arrow / spinner on the right, like the macOS field
            Item {
                id: go
                anchors { right: parent.right; rightMargin: 5; verticalCenter: parent.verticalCenter }
                width: 24; height: 24
                Rectangle {
                    anchors.fill: parent; radius: 12
                    color: Qt.rgba(1, 1, 1, goMouse.containsMouse ? 0.34 : 0.24)
                    opacity: field.text !== "" && !root.busy ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: 160 } }
                    Glyph { anchors.centerIn: parent; text: root.ic.right; size: 15 }
                    MouseArea { id: goMouse; anchors.fill: parent; hoverEnabled: true; enabled: parent.opacity > 0.5
                        onClicked: root.submit(field.text) }
                }
                Glyph {
                    anchors.centerIn: parent; text: root.ic.loader; size: 17; opacity: 0.85
                    visible: root.busy
                    RotationAnimation on rotation { running: root.busy; loops: Animation.Infinite; from: 0; to: 360; duration: 900 }
                }
            }
        }
        Item { width: 1; height: 12 }
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.message !== "" ? root.message : root.hint
            size: 13; w: 500
            opacity: root.message !== "" ? 0.95 : (root.awake ? 0.62 : 0)
            Behavior on opacity { NumberAnimation { duration: 300 } }
        }
    }

    // ------------------------------------------------------------------ login: power row
    Row {
        visible: root.login && root.primary
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom; anchors.bottomMargin: Math.round(root.height * 0.045)
        spacing: 36
        Repeater {
            model: [
                { a: "suspend", i: root.ic.zzz, t: "Uśpij" },
                { a: "reboot", i: root.ic.reload, t: "Uruchom ponownie" },
                { a: "poweroff", i: root.ic.power, t: "Wyłącz" }
            ]
            delegate: Column {
                required property var modelData
                spacing: 7
                width: 96
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 40; height: 40; radius: 20
                    color: Qt.rgba(1, 1, 1, pm.pressed ? 0.32 : pm.containsMouse ? 0.24 : 0.15)
                    border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.2)
                    Behavior on color { ColorAnimation { duration: 140 } }
                    Glyph { anchors.centerIn: parent; text: modelData.i; size: 19 }
                    MouseArea { id: pm; anchors.fill: parent; hoverEnabled: true; onClicked: root.power(modelData.a) }
                }
                Label {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: modelData.t; size: 12; w: 500; opacity: 0.85
                }
            }
        }
    }
}
