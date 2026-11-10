import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// What a setting does, under it, on a Settings page.
QQC2.Label {
    id: hint
    Layout.fillWidth: true
    Layout.maximumWidth: Kirigami.Units.gridUnit * 24
    wrapMode: Text.Wrap
    opacity: 0.7
    font: Kirigami.Theme.smallFont
    // Lines as high as its own font's, even where a character (an arrow) comes
    // from a fallback font with taller ones.
    lineHeightMode: Text.FixedHeight
    lineHeight: metrics.height
    FontMetrics { id: metrics; font: hint.font }
}
