import QtQuick
import org.kde.kirigami as Kirigami

// The purple of private windows ("privacy.privateWindowColor") on an item: its
// Kirigami.Theme gets the colors outright, as the style's controls fix their own
// color sets and take nothing from a parent. Its inheritance goes too, so it
// owns its colors and what's inside it follows them. Off, the item's own return.
Instantiator {
    id: colors
    required property QtObject target
    property bool active: false
    // Kirigami.Theme's color names → colors (Browser.privateWindowColors()).
    property var palette: ({})
    // (inherit first: an item that inherits would keep the colors to itself.)
    model: active ? ["inherit"].concat(Object.keys(palette)) : []
    delegate: Binding {
        required property string modelData
        target: colors.target.Kirigami.Theme
        property: modelData
        value: modelData === "inherit" ? false : colors.palette[modelData]
    }
}
