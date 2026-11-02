import QtQuick
import QtWebChannel
import org.kde.kite

// Per-view QWebChannel object ("kite") through which extension pages and the
// content-script bridge world talk to the Rust WebExtension runtime.
QtObject {
    id: bridge
    WebChannel.id: "kite"
    property string viewKey
    property int tabId: -1
    property int windowId: -1
    property string viewKind: "tab"
    signal deliver(string json)

    // Password manager hooks, called by the bridge world of a tab. Handlers
    // (WebTab) use the view's own URL, never anything supplied by the page.
    signal fillLogin(string json)
    signal passwordFormSubmitted(string json)
    signal loginFormDetected(bool emptyPassword)
    function formSubmitted(json) { if (viewKind === "tab") passwordFormSubmitted(json) }
    function loginFormFound(emptyPassword) { if (viewKind === "tab") loginFormDetected(emptyPassword) }

    function call(token, ctx, method, args, callId) {
        Extensions.call(viewKey, tabId, windowId, token, ctx, method, args, callId)
    }
    function cosmetic(url) { return Browser.cosmetic(url) }

    function _onDeliver(view, json) { if (view === viewKey) deliver(json) }
    Component.onCompleted: Extensions.deliver.connect(_onDeliver)
    Component.onDestruction: {
        Extensions.deliver.disconnect(_onDeliver)
        if (viewKey !== "") Extensions.viewClosed(viewKey)
    }
}
