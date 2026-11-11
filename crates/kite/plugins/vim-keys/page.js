// Vim keys, in a page and in each of its frames. The keys are looked at before the
// page's own scripts see them; only those that mean something are kept from it.
// What the window does (tabs, history) is sent to it; link hints are shown here,
// one frame at a time, with the window taking the keyboard and saying what to show.

function run() {
    if (globalThis.__kiteVim) globalThis.__kiteVim.stop()
    delete globalThis.__kiteVim
    // The page's address decides, not a frame's own: a widget's or an ad's frame
    // is often about:blank, and a key pressed with it focused would otherwise
    // be lost (here, it's handed up: scrollTop).
    const origins = location.ancestorOrigins
    let top
    try { top = origins && origins.length ? new URL(origins[origins.length - 1]) : location } catch (e) { return }
    if (!/^(https?|file):$/.test(top.protocol)) return
    if (isExcepted(top.hostname, KITE_PLUGIN_SETTINGS.exceptions)) return

    const STEP = 60
    const PENDING_MS = 1500
    const map = bindings(KITE_PLUGIN_SETTINGS.keys)
    const inFrame = window !== window.top
    let pending = ""
    let pendingTimer = 0
    let passing = false       // insert mode: keys are the page's
    let lastClicked = null
    const kept = new Set()    // key codes whose release is kept from the page too
    const down = new Set()    // key codes seen going down here

    // ---- is something being typed into? ----
    function deepActive() {
        let el = document.activeElement
        while (el && el.shadowRoot && el.shadowRoot.activeElement) el = el.shadowRoot.activeElement
        return el
    }
    const NOT_TEXT = new Set(["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"])
    // Something being typed into. A select, or a dropdown made of a button
    // (role=combobox: a shop's size or sort picker), keeps the keyboard after it
    // was picked from with the mouse, but isn't typed into: the keys are the
    // page's only while its list is open. (Picked from and left focused, it
    // would otherwise swallow every key, with nothing to show why.)
    function typingIn(el) {
        if (document.designMode === "on") return true
        if (!el || el === document.body) return false
        if (el.isContentEditable || el.matches("textarea, [role=textbox], [role=searchbox]")) return true
        if (el.matches("input")) return !NOT_TEXT.has((el.type || "text").toLowerCase())
        return el.matches("select, [role=combobox]") && el.getAttribute("aria-expanded") === "true"
    }

    // ---- scrolling ----
    function overflows(el) {
        const y = getComputedStyle(el).overflowY
        return (y === "auto" || y === "scroll") && el.scrollHeight > el.clientHeight + 1
    }
    // (A body's overflow is usually the viewport's, unless the root has its own.)
    function scrollsItself(el) {
        return overflows(el) && (el !== document.body || getComputedStyle(document.documentElement).overflowY !== "visible")
    }
    function parentOf(el) { return el.parentElement || (el.getRootNode().host || null) }
    let largest = null
    let largestAt = 0
    // What the keys should move: around where the user last was, or the document,
    // or on pages keeping it all in one inner pane, that pane.
    function scroller() {
        for (let el = lastClicked && lastClicked.isConnected ? lastClicked : deepActive(); el && el !== document.documentElement; el = parentOf(el)) {
            if (scrollsItself(el)) return el
        }
        const doc = document.scrollingElement
        if (doc && doc.scrollHeight > doc.clientHeight + 1) return doc
        if (Date.now() - largestAt > 2000) {
            largestAt = Date.now()
            largest = null
            let area = 0
            for (const el of document.querySelectorAll("body *")) {
                const a = el.clientHeight * el.clientWidth
                if (a > area && el.scrollHeight > el.clientHeight + 1 && overflows(el)) { area = a; largest = el }
            }
        }
        return largest
    }
    const SCROLLS = {
        scrollDown: () => ({dy: STEP}),
        scrollUp: () => ({dy: -STEP}),
        scrollLeft: () => ({dx: -STEP}),
        scrollRight: () => ({dx: STEP}),
        halfPageDown: t => ({dy: Math.round(t.clientHeight / 2)}),
        halfPageUp: t => ({dy: -Math.round(t.clientHeight / 2)}),
        top: () => ({y: 0}),
        bottom: t => ({y: t.scrollHeight}),
    }
    // Smooth: each key moves where the scroller is heading, and every frame it goes
    // part of the way there, so a held key runs evenly instead of restarting.
    const SMOOTH = !!KITE_PLUGIN_SETTINGS.smoothScroll
    const EASE = 0.22
    const moving = new Map()    // scroller → {x, y, toX, toY}
    let frame = 0
    // A press starts moving at once, the same whether it's a tap or a hold: where the
    // scroller is heading runs ahead of it, which it follows, so it gathers speed
    // without a break. The release decides the rest: a tap is made up to one step,
    // and a hold just slows down from wherever it is. So nothing waits to find out
    // which it is, and it doesn't depend on the keyboard's repeat.
    const HOLD_SPEED = scrollSpeed(KITE_PLUGIN_SETTINGS.scrollSpeed)
    const TAP_MS = 90           // up to here it may still be a tap: not past one step
    const HOLDS = {scrollDown: [0, 1], scrollUp: [0, -1], scrollLeft: [-1, 0], scrollRight: [1, 0]}
    let held = null             // {code, target, way, since, last, travelled}
    function headTo(target, dx, dy, y) {
        let m = moving.get(target)
        if (!m) moving.set(target, m = {x: target.scrollLeft, y: target.scrollTop, toX: target.scrollLeft, toY: target.scrollTop})
        const maxX = target.scrollWidth - target.clientWidth, maxY = target.scrollHeight - target.clientHeight
        m.toX = Math.max(0, Math.min(maxX, m.toX + dx))
        m.toY = Math.max(0, Math.min(maxY, y !== undefined ? y : m.toY + dy))
        if (!frame) frame = requestAnimationFrame(animate)
    }
    function releaseHeld() {
        if (!held) return
        const h = held
        held = null
        const rest = STEP - h.travelled
        if (rest > 0 && h.target.isConnected) headTo(h.target, h.way[0] * rest, h.way[1] * rest)
    }
    // Where it is now is where it stays, as what's left of a release would run on.
    function stopMoving() {
        held = null
        moving.clear()
        cancelAnimationFrame(frame)
        frame = 0
    }
    function animate(now) {
        frame = 0
        if (held) {
            if (!held.target.isConnected) held = null
            else {
                const dt = Math.max(0, Math.min(100, now - held.last))
                held.last = now
                const rate = now - held.since < TAP_MS ? Math.min(HOLD_SPEED, STEP / TAP_MS * 1000) : HOLD_SPEED
                const by = rate * dt / 1000
                held.travelled += by
                headTo(held.target, held.way[0] * by, held.way[1] * by)
            }
        }
        for (const [el, m] of moving) {
            // Something else moved it (a wheel, the page): that wins.
            if (!el.isConnected || Math.abs(el.scrollTop - m.y) > 3 || Math.abs(el.scrollLeft - m.x) > 3) { moving.delete(el); continue }
            m.x += (m.toX - m.x) * EASE
            m.y += (m.toY - m.y) * EASE
            const done = Math.abs(m.toX - m.x) < 0.5 && Math.abs(m.toY - m.y) < 0.5
            if (done) { m.x = m.toX; m.y = m.toY; moving.delete(el) }
            el.scrollTo({left: m.x, top: m.y, behavior: "instant"})
        }
        if ((moving.size || held) && !frame) frame = requestAnimationFrame(animate)
    }
    function scroll(command, code) {
        const target = scroller()
        // A frame with nothing to move: the page around it.
        if (!target) {
            if (inFrame) kitePluginSend({cmd: "scrollTop", scroll: command})
            return
        }
        const by = SCROLLS[command](target)
        if (!SMOOTH) {
            if (by.y !== undefined) target.scrollTo({top: by.y, behavior: "instant"})
            else target.scrollBy({left: by.dx || 0, top: by.dy || 0, behavior: "instant"})
            return
        }
        // (A key repeating without being held here, after focus came back: a plain step.)
        if (code && HOLDS[command]) {
            const now = performance.now()
            if (held) releaseHeld()
            held = {code, target, way: HOLDS[command], since: now, last: now, travelled: 0}
            headTo(target, 0, 0)
            return
        }
        headTo(target, by.dx || 0, by.dy || 0, by.y)
    }

    // ---- link hints ----
    // What gets a hint follows Vimium, as sites are written with it in mind: pages often
    // catch clicks further up, leaving nothing on an element to say so but its class.
    function allElements(root, out) {
        for (const el of root.querySelectorAll("*")) {
            out.push(el)
            if (el.shadowRoot) allElements(el.shadowRoot, out)
        }
        return out
    }
    function crop(r) {
        const left = Math.max(0, r.left), top = Math.max(0, r.top)
        const right = Math.min(innerWidth, r.right), bottom = Math.min(innerHeight, r.bottom)
        const width = right - left, height = bottom - top
        return width >= 3 && height >= 3 ? {left, top, right, bottom, width, height} : null
    }
    // Where it can be seen in the window, if anywhere.
    function visibleRect(el) {
        const style = getComputedStyle(el)
        for (const r of el.getClientRects()) {
            if (r.width && r.height) {
                const rect = crop(r)
                if (rect && style.visibility === "visible" && style.opacity !== "0") return rect
                continue
            }
            // No size: it may still wrap what's taken out of its flow, like a floated
            // image in a link, or be inline text sized on its children.
            const noFont = r.height === 0 && style.display.startsWith("inline") && style.fontSize === "0px"
            for (const child of el.children) {
                const c = getComputedStyle(child)
                const out = c.float !== "none" || c.position === "absolute" || c.position === "fixed"
                if (!out && !(noFont && c.display.startsWith("inline"))) continue
                const rect = visibleRect(child)
                if (rect) return rect
            }
        }
        return null
    }
    function isDeclaredDisabled(el) {
        const value = el.getAttribute("aria-disabled")
        return value !== null && ["", "true"].includes(value.toLowerCase())
    }
    // Is it clickable, and how? Null if not. {reason}: what picking it does, if not a
    // click. {unsure}: a guess, dropped if something clickable is close inside it.
    function hintKind(el, context) {
        if (el.disabled || isDeclaredDisabled(el)) return null
        const tag = el.localName
        const role = el.getAttribute("role")
        const editable = el.getAttribute("contenteditable")
        let clickable = el.hasAttribute("onclick")
            || (role !== null && HINT_ROLES.includes(role.toLowerCase()))
            || (editable !== null && ["", "contenteditable", "true"].includes(editable.toLowerCase()))
            || (context.angular && NG_CLICK.some(name => el.hasAttribute(name)))
            || jsactionClicks(el.getAttribute("jsaction"))
        let reason = ""
        let unsure = false
        switch (tag) {
        case "a": case "button": case "select": case "summary": case "object": case "embed":
            clickable = true
            break
        case "textarea":
            clickable = clickable || !el.readOnly
            break
        case "input":
            clickable = clickable || !(el.type === "hidden" || (el.readOnly && !NOT_TEXT.has(el.type)))
            break
        case "label":
            // Not beside the field it's for, unless that can't be picked itself: a
            // styled checkbox is often hidden in favor of its label.
            if (!el.control) { clickable = true; unsure = true }
            else clickable = clickable || (!el.control.disabled && !isDeclaredDisabled(el.control) && !visibleRect(el.control))
            break
        case "img":
            clickable = clickable || ["zoom-in", "zoom-out"].includes(el.style.cursor)
            break
        case "body":
            if (el !== document.body || clickable) break
            if (!document.hasFocus()) {
                if (innerWidth > 3 && innerHeight > 3) { clickable = true; reason = "frame" }
            } else if (el !== context.scrolling && scrollsItself(el)) { clickable = true; reason = "scroll" }
            break
        case "div": case "ol": case "ul":
            if (!clickable && el !== context.scrolling && scrollsItself(el)) { clickable = true; reason = "scroll" }
            break
        case "details":
            clickable = true
            reason = "open"
            break
        }
        if (!clickable && classSaysButton(el.getAttribute("class"))) { clickable = true; unsure = true }
        // A span is mostly around what's really clickable.
        if (tag === "span") unsure = true
        if (!clickable) clickable = parseInt(el.getAttribute("tabindex")) >= 0
        return clickable ? {reason, unsure} : null
    }
    // The hints of an image map, one for each area: those have no box of their own.
    function mapHints(img) {
        const name = (img.getAttribute("usemap") || "").replace(/^#/, "")
        if (!name || isDeclaredDisabled(img)) return null
        const map = img.getRootNode().querySelector('map[name="' + CSS.escape(name) + '"]')
        const box = img.getClientRects()[0]
        if (!map || !box) return null
        if (!visibleRect(img)) return []
        const out = []
        for (const area of map.getElementsByTagName("area")) {
            const part = areaBox(area.shape, area.coords, box.width, box.height)
            const rect = part && crop({left: box.left + part.left, top: box.top + part.top, right: box.left + part.right, bottom: box.top + part.bottom})
            if (rect) out.push({el: area, rect, image: img, reason: "", unsure: false})
        }
        return out
    }
    // Its middle or a corner can be clicked, not being under something else.
    function reachable({el, rect, image}) {
        const root = (image || el).getRootNode()
        const ours = hit => !hit || hit === image || hit === el || el.contains(hit) || hit.contains(el)
        if (ours(root.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2))) return true
        for (const y of [rect.top + 0.1, rect.bottom - 0.1]) {
            for (const x of [rect.left + 0.1, rect.right - 0.1]) {
                if (ours(root.elementFromPoint(x, y))) return true
            }
        }
        return false
    }
    let hints = []
    let hintHost = null
    let hintNewTab = false
    function hintsCollect() {
        hintsEnd()
        // Our own scrolling mustn't move the links from under their hints, or pass
        // for the user scrolling away from them.
        stopMoving()
        const context = {
            angular: document.getElementsByClassName("ng-scope").length > 0,
            scrolling: scroller(),
        }
        const seen = []
        for (const el of allElements(document, [])) {
            const parts = el.localName === "img" ? mapHints(el) : null
            if (parts) {
                seen.push(...parts)
                continue
            }
            const kind = hintKind(el, context)
            const rect = kind && visibleRect(el)
            if (rect) seen.push({el, rect, reason: kind.reason, unsure: kind.unsure})
        }
        const found = []
        const taken = new Map()
        for (const hint of withoutWrappers(seen, parentOf)) {
            if (!reachable(hint)) continue
            // The same target twice, as a link around its button.
            let same = false
            for (let up = parentOf(hint.el); up && !same; up = parentOf(up)) {
                const r = taken.get(up)
                same = !!r && Math.abs(r.left - hint.rect.left) < 8 && Math.abs(r.top - hint.rect.top) < 8
            }
            if (same) continue
            taken.set(hint.el, hint.rect)
            found.push(hint)
        }
        hints = found
        return found.length
    }
    function hintsShow(labels, newTab) {
        hintNewTab = newTab
        hintHost = document.createElement("div")
        hintHost.style.cssText = "all:initial;position:fixed;inset:0;z-index:2147483647;pointer-events:none"
        // Closed: nothing of the page's can look in, or style it.
        const shadow = hintHost.attachShadow({mode: "closed"})
        const style = document.createElement("style")
        const look = hintTheme(KITE_PLUGIN_SETTINGS.hintColor)
        style.textContent = ":host{all:initial}"
            + "span{position:fixed;font:bold 12px/1.25 sans-serif;color:" + look.text + ";background:linear-gradient(to bottom," + look.top + "," + look.bottom + ");border:1px solid " + look.outline + ";border-radius:3px;padding:1px 3px;box-shadow:0 3px 7px rgba(0,0,0,.3);text-transform:uppercase;white-space:nowrap}"
            + "b{font-weight:bold;color:" + look.typed + "}"
        shadow.append(style)
        hints.forEach((h, i) => {
            h.label = labels[i]
            h.mark = document.createElement("span")
            h.mark.textContent = h.label
            h.mark.style.left = Math.max(0, h.rect.left) + "px"
            h.mark.style.top = Math.max(0, h.rect.top) + "px"
            shadow.append(h.mark)
        })
        document.documentElement.append(hintHost)
        addEventListener("scroll", hintsAbandoned, {capture: true, passive: true})
    }
    function hintsAbandoned() { kitePluginSend({cmd: "hintsCancel"}) }
    function hintsFilter(typed) {
        for (const h of hints) {
            const on = h.label.startsWith(typed)
            h.mark.style.display = on ? "" : "none"
            if (!on) continue
            h.mark.textContent = ""
            const done = document.createElement("b")
            done.textContent = typed
            h.mark.append(done, h.label.substring(typed.length))
        }
    }
    function hintsEnd() {
        removeEventListener("scroll", hintsAbandoned, {capture: true})
        if (hintHost) hintHost.remove()
        hintHost = null
        hints = []
    }
    function activate(el, newTab, rect) {
        const link = el.closest("a[href], area[href]")
        if (newTab && link && /^https?:/.test(link.href)) {
            kitePluginSend({cmd: "openTab", url: link.href})
            return
        }
        lastClicked = el
        // (An image map's area has no box to ask.)
        const r = rect || el.getBoundingClientRect()
        const at = {bubbles: true, cancelable: true, composed: true, view: window, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2}
        el.dispatchEvent(new PointerEvent("pointerdown", at))
        el.dispatchEvent(new MouseEvent("mousedown", at))
        if (el.matches("input, textarea, select, object, embed, [contenteditable], [tabindex]")) el.focus()
        el.dispatchEvent(new PointerEvent("pointerup", at))
        el.dispatchEvent(new MouseEvent("mouseup", at))
        // (Not everything has click(): SVG icons don't.)
        if (el.click) el.click()
        else el.dispatchEvent(new MouseEvent("click", at))
    }
    // (`shifted`: Shift was held on the last letter, which also means a new tab.)
    function hintsPick(label, shifted) {
        const h = hints.find(x => x.label === label)
        const el = h && h.el
        const newTab = hintNewTab || shifted
        hintsEnd()
        if (!el || !el.isConnected) return
        if (h.reason === "frame") window.focus()
        else if (h.reason === "scroll") lastClicked = el
        else if (h.reason === "open") {
            lastClicked = el
            el.open = !el.open
        } else activate(el, newTab, h.rect)
    }

    // ---- keys ----
    function clearPending() {
        pending = ""
        clearTimeout(pendingTimer)
    }
    // Insert mode is the page's, in all its frames (the window passes it on with
    // pass(), as Esc may be pressed in another frame than `i` was): here at once,
    // so that the keys typed right after `i` are the page's too.
    function execute(command, code) {
        if (SCROLLS[command]) scroll(command, code)
        else if (command === "insert") {
            passing = true
            kitePluginSend({cmd: "insert"})
        } else kitePluginSend({cmd: command})
    }
    function onKeyDown(e) {
        if (!e.isTrusted) return
        down.add(e.code)
        if (e.isComposing) return
        const active = deepActive()
        if (e.key === "Escape") {
            clearPending()
            passing = false
            if (typingIn(active)) {
                // After the page's own use of Esc, as closing its search box.
                setTimeout(() => active.blur())
            }
            // (The window ends insert mode in the other frames, if it was on.)
            kitePluginSend({cmd: "escape"})
            return
        }
        if (passing || typingIn(active) || e.ctrlKey || e.altKey || e.metaKey || e.key.length !== 1) return
        // The keyboard repeating a key that is keeping itself going.
        if (held && held.code === e.code) {
            e.preventDefault()
            e.stopImmediatePropagation()
            return
        }
        const r = press(map, pending, e.key)
        clearPending()
        if (!r.run && !r.pending) return
        e.preventDefault()
        e.stopImmediatePropagation()
        kept.add(e.code)
        if (r.run) execute(r.run, e.repeat ? "" : e.code)
        else {
            pending = r.pending
            pendingTimer = setTimeout(clearPending, PENDING_MS)
        }
    }
    // Also those whose press went elsewhere: a hint's last letter is typed to the
    // window, and let go of here.
    function onKeyUp(e) {
        if (held && held.code === e.code) releaseHeld()
        const seen = down.delete(e.code)
        if (!kept.delete(e.code) && seen) return
        e.preventDefault()
        e.stopImmediatePropagation()
    }
    function onPointerDown(e) { lastClicked = e.composedPath()[0] instanceof Element ? e.composedPath()[0] : null }
    addEventListener("keydown", onKeyDown, true)
    addEventListener("keyup", onKeyUp, true)
    addEventListener("pointerdown", onPointerDown, true)
    // The release won't reach us once the page has lost the keyboard.
    addEventListener("blur", releaseHeld)
    document.addEventListener("visibilitychange", releaseHeld)

    globalThis.__kiteVim = {
        hintsCollect, hintsShow, hintsFilter, hintsEnd, hintsPick, scroll,
        pass(on) { passing = on },
        stop() {
            hintsEnd()
            clearPending()
            moving.clear()
            releaseHeld()
            removeEventListener("blur", releaseHeld)
            document.removeEventListener("visibilitychange", releaseHeld)
            removeEventListener("keydown", onKeyDown, true)
            removeEventListener("keyup", onKeyUp, true)
            removeEventListener("pointerdown", onPointerDown, true)
        },
    }
}

if (typeof document !== "undefined") run()
