import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import "components"
import "components/LivePresets.js" as Presets

// The wallpaper of every monitor (Background layer, own namespace, no blur): the photo LiveCfg.image
// (decoded at the monitor's size, cross-faded when it changes) or, while LiveCfg.live names a preset
// (components/LivePresets.js; both picked in Wallpapers.qml, Super+Shift+W), the animated scene.
// The scene stops under a fullscreen window (game, video) and slows down behind windows; LiveCfg.level
// (music) gently lifts its light and pace with the loudness.
Scope {
    id: root
    required property var modelData

    readonly property var hmon: Hyprland.monitorFor(modelData)
    readonly property bool fsCover: !!hmon && !!hmon.activeWorkspace && hmon.activeWorkspace.hasFullscreen
    // windows hide (almost) the whole monitor: only their blur shows the scene
    readonly property bool hidden: Cover.monHidden(modelData)
    // the music capture runs only while the primary monitor's scene is visible
    readonly property bool seen: !fsCover && !hidden
    onSeenChanged: if (modelData.name === "DP-3") LiveCfg.shown = seen

    PanelWindow {
        screen: root.modelData
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Background
        WlrLayershell.namespace: "mist-live"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        color: "black"
        mask: Region {}

        // ---------------- photo
        Item {
            id: photo
            anchors.fill: parent
            visible: LiveCfg.live === ""
            readonly property string src: visible && LiveCfg.image ? "file://" + LiveCfg.image : ""
            readonly property size dec: Qt.size(root.modelData.width, root.modelData.height)
            onSrcChanged: {
                if (!top.source.toString()) { top.source = src; return }
                under.source = top.source           // same url + size: straight from the pixmap cache
                top.opacity = 0
                top.source = src
            }
            Component.onCompleted: top.source = src
            Image {
                id: under
                anchors.fill: parent
                fillMode: Image.PreserveAspectCrop
                sourceSize: photo.dec
                asynchronous: true
                visible: !!source.toString()
            }
            Image {
                id: top
                anchors.fill: parent
                fillMode: Image.PreserveAspectCrop
                sourceSize: photo.dec
                asynchronous: true
                onStatusChanged: if (status === Image.Ready && opacity < 1) fadeIn.restart()
                NumberAnimation on opacity { id: fadeIn; running: false; to: 1; duration: 600; easing.type: Easing.InOutQuad; onFinished: under.source = "" }
            }
        }

        // ---------------- animated scene
        Loader {
            anchors.fill: parent
            active: LiveCfg.live !== ""
            sourceComponent: LiveWall {
                preset: LiveCfg.live
                // the gallery (Wallpapers.qml) covers this screen with its own preview
                animate: !root.fsCover && !(Ui.popup === "wallpaper" && Ui.screenName === root.modelData.name)
                slow: root.hidden
                speedMul: Presets.tempo(LiveCfg.tempo)
                energy: LiveCfg.level
                Behavior on energy { NumberAnimation { duration: 400 } }
            }
        }

        // ---------------- vignette under the desktop cards (main monitor): edges and bar / dock bands
        ShaderEffect {
            anchors.fill: parent
            visible: root.modelData.name === "DP-3"
            fragmentShader: Qt.resolvedUrl("shaders/vignette.frag.qsb")
            property size size: Qt.size(width, height)
        }
    }
}
