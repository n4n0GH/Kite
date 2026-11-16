import QtQuick
import QtQuick.Window
import QtWebEngine
import org.kde.kite

// Saves what a page shows to files: the Page Info window's Media list.
//
// QtWebEngine has no "download this address" for QML. What it has: a view that
// goes to an address either gets a download for it (what no tab shows, or what
// the server wants saved), or shows it; and what a view shows, it can save
// (WebEngineView.save: of a picture or a video, that's the file itself). So a
// view nobody sees goes to each address in turn. It is one of the page's own
// profile, so the file comes the way the page got it: from the cache if it is
// there, with the site's cookies, through the proxy, past the content blocker.
//
// The downloads of these views are ordinary ones (the downloads button, the
// Library). Main.onDownloadRequested asks claim() about each download first,
// and for the ones that are from here it says where they go.
Window {
    id: saver
    visible: false
    width: 400; height: 300
    title: "Kite media saver"
    property var app

    // What is still to do, and the one under way: {url, path, unique, quiet,
    // profile}. `path`: the file to write; `unique`: under another name if
    // that one is taken (several at once, into a folder); `quiet`: without a
    // notification of its own (one for all of them, at the end).
    property var queue: []
    property var current: null
    // The current one's download, once there is one.
    property var download: null
    // The view being emptied between two files.
    property var clearing: null
    // Of the files asked for since nothing was under way.
    property int saved: 0
    property int failed: 0
    property string folder: ""
    readonly property bool busy: current !== null || queue.length > 0
    readonly property int total: saved + failed + queue.length + (current ? 1 : 0)
    signal finished(int saved, int failed, string folder, bool quiet)

    function save(profile, items) {
        if (!busy) { saved = 0; failed = 0 }
        const quiet = items.length > 1
        if (items.length > 0) folder = items[0].path.substring(0, items[0].path.lastIndexOf("/"))
        queue = queue.concat(items.map(i => ({url: i.url, path: i.path, unique: !!i.unique, quiet: quiet, profile: profile})))
        next()
    }
    // (The one under way is a download by now, to stop like any other.)
    function stop() { queue = [] }

    property var _views: new Map()
    Component {
        id: viewComponent
        WebEngineView {
            id: view
            width: 400; height: 300
            // Nothing of the site's runs here, nor is heard.
            settings.javascriptEnabled: false
            settings.localContentCanAccessFileUrls: false
            audioMuted: true
            onNewWindowRequested: request => {}
            onLoadingChanged: info => {
                // The empty page between two files (settle) is there: on to the next.
                if (/^about:/i.test(info.url.toString())) {
                    if (saver.clearing === view && info.status !== WebEngineView.LoadStartedStatus) { saver.clearing = null; cleared.stop(); saver.next() }
                    return
                }
                if (!saver.current || saver.viewFor(saver.current.profile) !== view || saver.download) return
                if (info.status === WebEngineView.LoadSucceededStatus) {
                    // It is shown: the view saves what it shows.
                    saver.saveShown()
                } else if (info.status === WebEngineView.LoadStoppedStatus) {
                    // Stopped short of being shown, for one of two reasons: it
                    // became a download, which then comes any moment; or it is
                    // sound or video, which the view fetches a second time to
                    // play, and can save like anything else it shows.
                    if (!grace.running) grace.restart()
                } else if (info.status === WebEngineView.LoadFailedStatus) {
                    saver.settle(false)
                }
            }
        }
    }
    function viewFor(profile) {
        let v = _views.get(profile)
        if (!v) { v = viewComponent.createObject(saver.contentItem, {profile: profile}); _views.set(profile, v) }
        return v
    }

    function saveShown() {
        if (!current || download) return
        grace.stop()
        const view = viewFor(current.profile)
        // (Nothing of it got as far as being shown: there is nothing to save.)
        if (/^(about:|$)/i.test(view.url.toString())) { settle(false); return }
        patience.restart()
        view.save(target(), WebEngineDownloadRequest.SingleHtmlSaveFormat)
    }
    // The file the current one goes to: decided once, when it's about to be
    // written (by then the ones before it are there, with their names taken).
    function target() {
        if (!current.target) {
            const slash = current.path.lastIndexOf("/")
            current.target = current.unique ? Browser.uniqueDownloadPath(current.path.substring(0, slash), current.path.substring(slash + 1)) : current.path
        }
        return current.target
    }
    function next() {
        if (current || clearing) return
        if (queue.length === 0) return
        current = queue[0]
        queue = queue.slice(1)
        download = null
        patience.restart()
        viewFor(current.profile).url = current.url
    }
    function settle(ok) {
        if (!current) return
        patience.stop()
        grace.stop()
        if (ok) saved++; else failed++
        const was = current
        current = null
        download = null
        if (queue.length === 0) finished(saved, failed, folder, was.quiet)
        // An empty page before the next file: so that what the view shows is
        // never the file before (which a load that stops short would leave
        // there, to be saved under the next one's name), and nothing goes on
        // playing.
        clearing = viewFor(was.profile)
        cleared.restart()
        clearing.url = "about:blank"
    }

    // Is this download one from here? Then where it goes: {path}.
    function claim(d) {
        if (!current || download) return null
        if (d.view !== viewFor(current.profile)) return null
        download = d
        grace.stop()
        patience.stop()
        const end = () => {
            if (saver.download !== d) return
            if (d.state === WebEngineDownloadRequest.DownloadCompleted) saver.settle(true)
            else if (d.isFinished || d.state === WebEngineDownloadRequest.DownloadInterrupted || d.state === WebEngineDownloadRequest.DownloadCancelled) saver.settle(false)
        }
        d.isFinishedChanged.connect(end)
        d.stateChanged.connect(end)
        return {path: target()}
    }
    // One notification for many files, at the end, in place of one each.
    Connections {
        target: saver.app
        function onDownloadBegan(id, d) { if (d === saver.download && saver.current && saver.current.quiet) saver.app.claimedDownloads[id] = true }
    }

    // Neither shown nor a download, after all this time: on to the next one.
    Timer { id: patience; interval: 30000; onTriggered: saver.settle(false) }
    // (Should the empty page never report.)
    Timer { id: cleared; interval: 3000; onTriggered: { saver.clearing = null; saver.next() } }
    // No download came of a load that stopped: sound or video, then.
    Timer { id: grace; interval: 1500; onTriggered: saver.saveShown() }
}
