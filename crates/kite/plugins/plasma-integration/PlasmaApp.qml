import QtQuick
import org.kde.kite

// Plasma Integration for the whole browser: what isn't any one window's.
Item {
    id: root
    property var app
    property var plugin

    function setting(key) { return !!app.pluginSetting(plugin, key) }

    // ---- downloads, as the desktop's jobs (kite_core::plasma::jobs) -----------------
    // The ones it's showing: id → where to.
    property var jobs: ({})
    Connections {
        target: root.app
        function onDownloadBegan(id, download, inPrivate) {
            if (!root.setting("downloads") || (inPrivate && !root.setting("privateWindows"))) return
            const path = download.downloadDirectory + "/" + download.downloadFileName
            root.jobs[id] = path
            root.app.claimedDownloads[id] = true
            Browser.plasmaJobStarted(id, path, download.url.toString())
        }
        function onDownloadProgress(id, download) {
            if (root.jobs[id]) Browser.plasmaJobProgress(id, download.receivedBytes, download.totalBytes)
        }
        function onDownloadPausedChanged(id, download) {
            if (root.jobs[id]) Browser.plasmaJobPaused(id, download.isPaused)
        }
        // (A complete one stays in `jobs` until it's clear whether that was shown.)
        function onDownloadEnded(id, download, state) {
            if (!root.jobs[id]) return
            // (Not interruptReasonString: a lost connection has it "canceled by the user".)
            Browser.plasmaJobFinished(id, state, download.receivedBytes, state === "interrupted" ? "The download was interrupted" : "")
            if (state !== "complete") delete root.jobs[id]
        }
    }
    Connections {
        target: Browser
        function onPlasmaJobEvent(json) {
            const e = JSON.parse(json)
            if (e.event === "notShown") {
                // Nothing here to show jobs in: it's for Kite to say.
                if (root.jobs[e.id]) root.app.notifyDownloadComplete(root.jobs[e.id])
                delete root.jobs[e.id]
                return
            }
            const download = root.app.activeDownloads[e.id]
            if (!download) return
            if (e.event === "cancel") download.cancel()
            else if (e.event === "pause") download.pause()
            else if (e.event === "resume") download.resume()
        }
    }
    // ---- search from the desktop (kite_core::plasma::krunner) ----------------------
    function configureRunner() {
        Browser.plasmaRunnerConfigure(JSON.stringify({
            tabs: setting("searchTabs"),
            bookmarks: setting("searchBookmarks"),
            // (A private session's history is nobody's business.)
            history: setting("searchHistory") && !Browser.isPrivate(),
        }))
    }
    Component.onCompleted: configureRunner()
    Connections {
        target: Browser
        function onSettingChanged(key) { if (key.startsWith("plugins." + root.plugin.id + ".search")) root.configureRunner() }
        function onPlasmaRunnerWantsTabs() {
            const tabs = []
            for (const w of root.app.windows) {
                if (w.privateMode && !root.setting("privateWindows")) continue
                for (let i = 0; i < w.tabs.count; ++i) {
                    const id = w.tabs.get(i).tabId
                    const tab = w.tabById(id)
                    if (tab) tabs.push({tab: id, title: String(tab.title), url: String(tab.url)})
                }
            }
            Browser.plasmaRunnerAnswerTabs(JSON.stringify(tabs))
        }
        function onPlasmaRunnerPicked(json) {
            const p = JSON.parse(json)
            Browser.useActivationToken(p.token)
            const present = w => { w.show(); w.raise(); w.requestActivate() }
            if (p.tab !== null) {
                const w = root.app.windows.find(x => x.tabById(p.tab))
                if (!w) return
                present(w)
                w.setCurrentIndex(w.indexOfTab(p.tab))
            } else {
                // Not into a private window, from outside.
                const active = root.app.activeWindow
                const w = active && !active.privateMode ? active : root.app.windows.find(x => !x.privateMode)
                if (!w) { root.app.newWindow({urls: [p.url]}); return }
                present(w)
                w.openTab(p.url, {})
            }
        }
    }

    function endJobs() {
        for (const id of Object.keys(jobs)) delete app.claimedDownloads[id]
        jobs = {}
        Browser.plasmaJobsForget()
    }
    // Plasma would call it an application that closed unexpectedly.
    Connections {
        target: Qt.application
        function onAboutToQuit() { root.quitting = true; root.endJobs() }
    }
    // Quitting: not being searchable is for when it's off, and for going, the service
    // goes by itself.
    property bool quitting: false
    Component.onDestruction: {
        endJobs()
        if (!quitting) Browser.plasmaRunnerConfigure("{}")
    }
}
