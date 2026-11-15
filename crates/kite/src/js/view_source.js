// "Line wrap" in a page's source (view-source:), without the tab dying.
//
// The source view is a document the engine builds itself, and its checkbox has
// the engine's own listener: it wraps the lines, then asks the browser to
// remember the choice (blink.mojom.PersistentRendererPrefsService). Chrome
// answers that; QtWebEngine has nobody to, and a renderer asking for something
// that isn't there is taken for a broken one and killed.
//
// So the change never reaches that listener: it's stopped on its way down, and
// the lines are wrapped here, the same way (the table's class).
//
// A script can't ask whether its document is a source view, so it goes by the
// shape the engine gives one. A page built to look the same only loses the
// change event of its own checkbox.
(() => {
    "use strict";
    addEventListener("change", event => {
        const box = event.target;
        if (!(box instanceof HTMLInputElement) || box.type !== "checkbox") return;
        const label = box.parentElement, form = label && label.parentElement, body = document.body;
        if (!label || !form || !body || form.parentElement !== body) return;
        if (!label.classList.contains("line-wrap-control") || form.localName !== "form") return;
        if (!body.firstElementChild || !body.firstElementChild.classList.contains("line-gutter-backdrop")) return;
        const table = form.nextElementSibling;
        if (!table || table.localName !== "table") return;
        event.stopImmediatePropagation();
        table.className = box.checked ? "line-wrap" : "";
    }, true);
})();
