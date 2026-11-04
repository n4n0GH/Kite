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

    // Form autofill and the password generator (the bridge world reports the
    // focused field; WebTab shows the suggestions and answers with these).
    signal fillAddress(string json)
    signal fillPassword(string password)
    signal autofillMenu(bool open)
    signal autofillFocused(string json)
    signal autofillBlurred()
    signal autofillKeyPressed(string key)
    signal autofillSubmitted(string json)
    // Picture-in-picture (bridge.js): the page has a video to offer, and the
    // video shown in picture-in-picture went away.
    signal videoFoundInPage()
    signal pipEndedInPage()
    function videoFound() { if (viewKind === "tab") videoFoundInPage() }
    function pipEnded() { if (viewKind === "tab") pipEndedInPage() }
    // Text typed into a form and not sent yet (keeps the tab from unloading).
    signal formEdited(bool dirty)
    function formDirty(dirty) { if (viewKind === "tab") formEdited(dirty) }
    function autofillFocus(json) { if (viewKind === "tab") autofillFocused(json) }
    function autofillBlur() { if (viewKind === "tab") autofillBlurred() }
    function autofillKey(key) { if (viewKind === "tab") autofillKeyPressed(key) }
    function autofillSubmit(json) { if (viewKind === "tab") autofillSubmitted(json) }

    function call(token, ctx, method, args, callId) {
        Extensions.call(viewKey, tabId, windowId, token, ctx, method, args, callId)
    }
    function cosmetic(url) { return Browser.cosmetic(url) }

    // Screen Wake Lock (crates/kite/src/js/wakelock.js, relayed by the bridge
    // world): WebTab prompts and answers with wakeLockAnswer.
    signal wakeLockRequested(int id)
    signal wakeLockReleased()
    signal wakeLockAnswer(int id, bool granted)
    function requestWakeLock(id) { if (viewKind === "tab") wakeLockRequested(id) }
    function releaseWakeLock() { if (viewKind === "tab") wakeLockReleased() }

    // Only this view's messages are taken (and converted) here.
    function _onDeliveryReady(view) {
        if (view !== viewKey) return
        for (let json = Extensions.takeDelivery(viewKey); json !== ""; json = Extensions.takeDelivery(viewKey)) deliver(json)
    }
    Component.onCompleted: Extensions.deliveryReady.connect(_onDeliveryReady)
    Component.onDestruction: {
        Extensions.deliveryReady.disconnect(_onDeliveryReady)
        if (viewKey !== "") Extensions.viewClosed(viewKey)
    }
}
