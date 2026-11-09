import QtQuick
import org.kde.kirigami as Kirigami

// Reader View's part in one tab, over its page (ReaderViewWindow.qml): it asks
// whether the page looks like an article once it's loaded and seen, and shows the
// controls over a page in reader view.
Item {
    id: part
    property var tab
    property var reader
    anchors.fill: parent
    z: 1

    // Twice: pages often fill in only after loading.
    Timer { id: probe; interval: 300; onTriggered: { part.reader.probe(part.tab); probeLate.restart() } }
    Timer { id: probeLate; interval: 2500; onTriggered: if (!part.tab.readerAvailable) part.reader.probe(part.tab) }
    Connections {
        target: part.tab
        // (Loaded, or moved on within the page.)
        function onLoadingChanged() { if (!part.tab.loading) probe.restart() }
        function onUrlChanged() { if (!part.tab.loading) probe.restart() }
        // Whether a page looks like an article matters once it's seen.
        function onVisibleChanged() { if (part.tab.visible && !part.tab.readerAvailable) probe.restart() }
    }
    Component.onCompleted: probe.restart()

    ReaderViewControls {
        reader: part.reader
        tab: part.tab
        visible: part.tab.isReader && !part.tab.inPip
        anchors { top: parent.top; right: parent.right; margins: Kirigami.Units.largeSpacing * 2 }
    }
}
