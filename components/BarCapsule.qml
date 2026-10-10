import QtQuick
import QtQuick.Layouts
import ".."

// group of top bar items: no background at all, just the content in a row (30 px, same line as the island)
Item {
    id: c
    default property alias content: row.data
    property real pad: 10
    property alias spacing: row.spacing
    implicitWidth: row.implicitWidth + 2 * pad
    implicitHeight: 30
    RowLayout { id: row; anchors.centerIn: parent; spacing: 8 }
}
