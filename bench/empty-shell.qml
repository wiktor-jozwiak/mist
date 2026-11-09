// The smallest Quickshell shell: one empty 32 px bar on every screen (bench/shells.sh, the toolkit's floor)
import Quickshell
import QtQuick

ShellRoot {
    Variants {
        model: Quickshell.screens
        PanelWindow {
            required property var modelData
            screen: modelData
            anchors { top: true; left: true; right: true }
            implicitHeight: 32
            color: "#202020"
        }
    }
}
