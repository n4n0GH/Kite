import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A row of a list on a Settings page: highlighted under the pointer (KDE's list
// row), so it's clear which row a button belongs to. Rows aren't clicked
// themselves. It holds one RowLayout, which fills it.
QQC2.ItemDelegate {
    id: listRow
    default property alias content: rowBox.data
    Layout.fillWidth: true
    hoverEnabled: true
    down: false
    highlighted: false
    topPadding: Kirigami.Units.smallSpacing
    bottomPadding: Kirigami.Units.smallSpacing
    contentItem: Item {
        id: rowBox
        implicitHeight: children.length ? children[0].implicitHeight : 0
        implicitWidth: children.length ? children[0].implicitWidth : 0
        Component.onCompleted: if (children.length) { children[0].anchors.fill = rowBox }
    }
}
