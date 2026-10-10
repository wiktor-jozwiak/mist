import QtQuick
import ".."

Text {
    property string name
    property real size: 16
    text: Theme.ic[name] || ""
    color: Theme.t2
    font.family: Theme.iconFont
    font.pixelSize: size
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
}
