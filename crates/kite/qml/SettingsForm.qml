import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A section's form on a Settings page. The forms of a page (the items in one
// column) line up their labels with each other.
Kirigami.FormLayout {
    id: form
    readonly property bool kiteForm: true
    Layout.fillWidth: true
    Component.onCompleted: twinFormLayouts = form.parent ? form.parent.children.filter(c => c !== form && c.kiteForm === true) : []
}
