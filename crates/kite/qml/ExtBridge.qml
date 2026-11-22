import QtQuick
import QtWebChannel
import org.kde.kite

// A view's end of the web channel, through which extension pages and the
// content-script bridge world talk to the Rust WebExtension runtime, and a
// tab's page scripts to the tab.
//
// Two objects. `channel` is the one the web channel shows to pages (as "kite"): it has
// signals and functions and nothing else. This one, its host's, says whose view it is.
// A page on the channel can write the properties of what it is shown as well as read
// them, so the view's key, tab and window on that object were the caller's to choose:
// an extension's page could name another view's key and take what was sent there.
// Nothing of this object is within the channel's reach, and `channel` holds no
// property that leads back here (nor to anything else: a property holding an object
// hands the page that object).
QtObject {
    id: bridge
    property string viewKey
    property int tabId: -1
    property int windowId: -1
    property string viewKind: "tab"
    property bool inPrivate: false

    // ---- from the page, for the host ----
    // Password manager hooks, called by the bridge world of a tab. Handlers
    // (WebTab) use the view's own URL, never anything supplied by the page.
    signal passwordFormSubmitted(string json)
    signal loginFormDetected(bool emptyPassword)
    // Form autofill and the password generator (the bridge world reports the
    // focused field; WebTab shows the suggestions and answers with the fill* below).
    signal autofillFocused(string json)
    signal autofillBlurred()
    signal autofillKeyPressed(string key)
    signal autofillSubmitted(string json)
    // Text typed into a form and not sent yet (keeps the tab from unloading).
    signal formEdited(bool dirty)
    // A call to the extension API. The host hands it to the runtime (onCalled),
    // with the page its view shows: extcall.js.
    signal called(string token, string ctx, string method, string args, int callId)
    // Screen Wake Lock (crates/kite/src/js/wakelock.js, relayed by the bridge
    // world): WebTab prompts and answers with wakeLockAnswer.
    signal wakeLockRequested(int id)
    signal wakeLockReleased()
    // Web MIDI (crates/kite/src/js/webmidi.js, relayed by the bridge world). What the page
    // sends is only what it's asking for: WebTab decides, from where the tab really is.
    signal midiRequested(string json)
    signal midiQueried(string json)
    signal midiOpRequested(string json)

    // ---- from the host, to the page ----
    function fillLogin(json) { channel.fillLogin(json) }
    function fillAddress(json) { channel.fillAddress(json) }
    function fillPassword(password) { channel.fillPassword(password) }
    function autofillMenu(open) { channel.autofillMenu(open) }
    function wakeLockAnswer(id, granted) { channel.wakeLockAnswer(id, granted) }
    function midiToPage(name, detail) { channel.midiToPage(name, detail) }

    // What a view registers with its web channel.
    readonly property QtObject channel: QtObject {
        WebChannel.id: "kite"
        // (Read by page_transport.js. Read-only: the channel can't set it.)
        readonly property string viewKind: bridge.viewKind

        signal deliver(string json)
        signal fillLogin(string json)
        signal fillAddress(string json)
        signal fillPassword(string password)
        signal autofillMenu(bool open)
        signal wakeLockAnswer(int id, bool granted)
        signal midiToPage(string name, string detail)

        function formSubmitted(json) { if (bridge.viewKind === "tab") bridge.passwordFormSubmitted(json) }
        function loginFormFound(emptyPassword) { if (bridge.viewKind === "tab") bridge.loginFormDetected(emptyPassword) }
        function formDirty(dirty) { if (bridge.viewKind === "tab") bridge.formEdited(dirty) }
        function autofillFocus(json) { if (bridge.viewKind === "tab") bridge.autofillFocused(json) }
        function autofillBlur() { if (bridge.viewKind === "tab") bridge.autofillBlurred() }
        function autofillKey(key) { if (bridge.viewKind === "tab") bridge.autofillKeyPressed(key) }
        function autofillSubmit(json) { if (bridge.viewKind === "tab") bridge.autofillSubmitted(json) }
        function call(token, ctx, method, args, callId) { bridge.called(token, ctx, method, args, callId) }
        function requestWakeLock(id) { if (bridge.viewKind === "tab") bridge.wakeLockRequested(id) }
        function releaseWakeLock() { if (bridge.viewKind === "tab") bridge.wakeLockReleased() }
        function requestMidi(json) { if (bridge.viewKind === "tab") bridge.midiRequested(json) }
        function queryMidi(json) { if (bridge.viewKind === "tab") bridge.midiQueried(json) }
        function midiOp(json) { if (bridge.viewKind === "tab") bridge.midiOpRequested(json) }
    }

    // Only this view's messages are taken (and converted) here.
    function _onDeliveryReady(view) {
        if (view !== viewKey) return
        for (let json = Extensions.takeDelivery(viewKey); json !== ""; json = Extensions.takeDelivery(viewKey)) channel.deliver(json)
    }
    Component.onCompleted: Extensions.deliveryReady.connect(_onDeliveryReady)
    Component.onDestruction: {
        Extensions.deliveryReady.disconnect(_onDeliveryReady)
        if (viewKey !== "") Extensions.viewClosed(viewKey)
    }
}
