import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// A section's title on a Settings page (PreferencesWindow.qml, and plugins' own
// pages), as System Settings has them: on the left, a line after it. (Without the
// padding it has as a list's item: it lines up with the rest of the column.)
Kirigami.ListSectionHeader { Layout.fillWidth: true; leftPadding: 0; rightPadding: 0 }
