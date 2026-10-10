import QtQuick
import ".."

// status dot (mic in use, screen sharing, urgent)
Rectangle {
    property color c: Theme.accent
    implicitWidth: 7; implicitHeight: 7; width: 7; height: 7; radius: 3.5; color: c
}
