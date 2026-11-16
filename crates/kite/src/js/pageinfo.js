// What the Page Info window shows of a page (qml/PageInfoWindow.qml): run once
// in the page's main frame, in Kite's own world, when the window asks. All of
// it is the page's to say, so the window shows it as plain text; and it is cut
// to sizes a window can show, whatever the page holds.
(() => {
    const cut = (value, max) => {
        const text = value === null || value === undefined ? "" : String(value)
        return text.length > max ? text.slice(0, max) + "…" : text
    }
    const d = document
    const info = {
        url: location.href,
        title: cut(d.title, 500),
        type: cut(d.contentType, 100),
        encoding: cut(d.characterSet, 50),
        mode: d.compatMode === "CSS1Compat" ? "standards" : "quirks",
        language: cut(d.documentElement && d.documentElement.lang, 50),
        referrer: cut(d.referrer, 2000),
        modified: "",
        protocol: "",
        size: 0,
        transferred: 0,
        meta: [],
        media: [],
    }
    // (A page whose server names no date is given the time of asking.)
    const modified = Date.parse(d.lastModified)
    if (!isNaN(modified) && Math.abs(Date.now() - modified) > 5000) info.modified = new Date(modified).toISOString()
    try {
        const load = performance.getEntriesByType("navigation")[0]
        if (load) {
            info.protocol = cut(load.nextHopProtocol, 20)
            info.size = load.decodedBodySize || 0
            info.transferred = load.transferSize || 0
        }
    } catch (e) {}

    for (const m of d.querySelectorAll("meta")) {
        if (info.meta.length >= 200) break
        const charset = m.getAttribute("charset")
        const name = m.getAttribute("name") || m.getAttribute("property") || m.getAttribute("http-equiv") || m.getAttribute("itemprop") || (charset ? "charset" : "")
        if (name) info.meta.push({name: cut(name, 100), content: cut(charset || m.getAttribute("content"), 2000)})
    }

    // Pictures, sound and video: each address once per kind. (A data: address
    // is the picture itself; a long one is left out, not cut to no picture.)
    const seen = new Set()
    const add = (kind, address, more) => {
        if (info.media.length >= 500 || !address) return
        let url
        try { url = new URL(address, d.baseURI).href } catch (e) { return }
        if (!/^(https?|data|blob|file):/i.test(url) || (url.startsWith("data:") && url.length > 20000)) return
        if (seen.has(kind + " " + url)) return
        seen.add(kind + " " + url)
        info.media.push(Object.assign({kind: kind, url: url, width: 0, height: 0, text: ""}, more || {}))
    }
    for (const l of d.querySelectorAll("link[rel~='icon' i], link[rel='apple-touch-icon' i]")) add("icon", l.href)
    for (const i of d.images) add("image", i.currentSrc || i.src, {width: i.naturalWidth, height: i.naturalHeight, text: cut(i.alt || i.title, 300)})
    for (const i of d.querySelectorAll("input[type='image' i]")) add("image", i.src, {text: cut(i.alt, 300)})
    for (const s of d.querySelectorAll("svg image")) add("image", s.href && s.href.baseVal)
    for (const v of d.querySelectorAll("video")) {
        add("video", v.currentSrc || v.src, {width: v.videoWidth, height: v.videoHeight})
        add("image", v.poster)
        for (const s of v.querySelectorAll("source")) add("video", s.src)
    }
    for (const a of d.querySelectorAll("audio")) {
        add("audio", a.currentSrc || a.src)
        for (const s of a.querySelectorAll("source")) add("audio", s.src)
    }
    for (const e of d.querySelectorAll("embed, object")) add("embedded", e.src || e.data)
    // Backgrounds: of so many elements that a long page doesn't hold the window up.
    let looked = 0
    for (const e of d.querySelectorAll("body, body *")) {
        if (++looked > 3000 || info.media.length >= 500) break
        const image = getComputedStyle(e).backgroundImage
        if (!image || image === "none") continue
        for (const m of image.matchAll(/url\((["']?)(.*?)\1\)/g)) add("background", m[2])
    }
    return JSON.stringify(info)
})()
