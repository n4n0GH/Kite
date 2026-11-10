import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.qqc2desktopstyle.private as StylePrivate

// A tab's shape as the application style draws it (Breeze, Oxygen, …), behind
// Kite's own icon, title and buttons ("tabs.styleDrawn", TabButton.qml). Loaded
// only when asked for, so Kite still runs where KDE's Qt Quick style isn't
// installed: the tab then keeps Kite's own look.
StylePrivate.StyleItem {
    id: item
    // The tab (TabButton.qml).
    required property var tab
    control: tab
    elementType: "tab"
    paintMargins: 0
    text: ""
    enabled: true
    selected: tab.current
    hover: tab.hovered
    hasFocus: false

    readonly property int index: tab.index
    readonly property int count: tab.browser.tabs.count
    readonly property int currentIndex: tab.browser.currentIndex
    properties: ({
        "hasFrame": true,
        "orientation": tab.placement === "bottom" ? "Bottom" : "Top",
        "tabpos": count === 1 ? "only" : index === 0 ? "beginning" : index === count - 1 ? "end" : "middle",
        "selectedpos": currentIndex === index + 1 ? "next" : currentIndex === index - 1 ? "previous" : "",
    })
}
