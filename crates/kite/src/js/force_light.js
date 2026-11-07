// "Always force light" (Site Settings): shows a site as it looks on a light
// desktop, also when the desktop is dark. QtWebEngine takes the colour scheme
// from the application and has no setting per page, so pages are told here.
// Runs in the page's own world as the document is created. It can't reach
// style sheets of other origins that don't allow it (except a <link>'s media
// attribute), or a site that's dark whatever the desktop.
(() => {
    const QUERY = /\(\s*prefers-color-scheme\s*:\s*(dark|light)\s*\)/gi;
    const light = (media) => media.replace(QUERY, (_, scheme) =>
        scheme.toLowerCase() === 'dark' ? '(max-width: -1px)' : '(min-width: 0px)');

    // What scripts ask.
    const matchMedia = window.matchMedia;
    window.matchMedia = function (media) {
        return matchMedia.call(this, light(String(media)));
    };

    // Form controls, scrollbars, light-dark() and pages without colours of their own.
    const base = new CSSStyleSheet();
    base.replaceSync(':root { color-scheme: light !important; }');
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, base];

    // Style sheets' dark and light rules.
    const fixMedia = (list) => {
        const text = list.mediaText;
        const fixed = light(text);
        if (fixed !== text) list.mediaText = fixed;
    };
    const fixRules = (rules) => {
        for (const rule of rules) {
            if (rule.media) fixMedia(rule.media);
            if (rule.styleSheet) fixSheet(rule.styleSheet);
            else if (rule.cssRules) fixRules(rule.cssRules);
        }
    };
    // If its server allows other origins, a copy of the <link> can load it
    // readably, as a new sheet (see the load listener below). The copy takes
    // its place once loaded, so the page is never without the styles.
    const requestReadable = (sheet) => {
        const link = sheet.ownerNode;
        if (!link || link.nodeName !== 'LINK' || link.crossOrigin !== null || !sheet.href) return;
        fetch(sheet.href, { mode: 'cors' }).then((r) => {
            if (!r.ok) return;
            const copy = link.cloneNode();
            copy.crossOrigin = 'anonymous';
            copy.addEventListener('load', () => link.remove(), { once: true });
            link.after(copy);
        }, () => {});
    };
    const done = new WeakSet();
    const fixSheet = (sheet) => {
        if (done.has(sheet)) return;
        done.add(sheet);
        fixMedia(sheet.media);
        // (Reading another origin's sheet throws.)
        try { fixRules(sheet.cssRules); } catch (e) { requestReadable(sheet); }
    };
    const fixAll = () => { for (const sheet of document.styleSheets) fixSheet(sheet); };

    // A <link> only has its rules once loaded; a <style> as it's added.
    const isStyle = (node) => node.nodeName === 'STYLE' || node.nodeName === 'LINK';
    document.addEventListener('load', (e) => { if (isStyle(e.target)) fixAll(); }, true);
    new MutationObserver((changes) => {
        if (changes.some((c) => isStyle(c.target) || [...c.addedNodes].some(isStyle))) fixAll();
    }).observe(document, { childList: true, subtree: true });
    fixAll();
})();
