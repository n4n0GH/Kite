// Kite's right-click menu on pages that keep it from the user (isolated world, every frame,
// from document creation, so its listeners come before any of the page's).
//
//  * Shift+right-click is Kite's alone: the page doesn't hear of it (as in Firefox).
//  * A page that cancels the menu without showing one of its own (stock photo sites, "right
//    click is disabled" alerts) gets Kite's after all: the click is replayed (bootstrap.cpp),
//    and the replay goes past the page. A page that does show its own menu keeps it.
//  * A picture under a see-through cover (an element over it, so a right-click finds the
//    cover): the covers let clicks through for the replayed click, which then finds the
//    picture, and the menu has its entries.
//
// The last two are "web.unblockContextMenu". Up, messages go to Kite as console messages
// from this world, which pages can neither see nor (lacking the secret) imitate; Kite only
// ever replays the user's own last right-click, and only for the shown tab.
(function () {
  'use strict';
  const SECRET = '__KITE_FRAME_SECRET__';
  const AUTO = __KITE_AUTO__;
  if (globalThis.__kiteContextMenu) return;
  globalThis.__kiteContextMenu = true;

  // While a replayed click is on its way: it passes the page by.
  let passUntil = 0;
  const passing = () => performance.now() < passUntil;
  function replay() {
    passUntil = performance.now() + 2000;
    console.debug(SECRET + 'cm:');
  }

  // ---- pictures under covers ----
  const INTERACTIVE = 'a[href], button, input, select, textarea, label, summary, video, audio, canvas, iframe, embed, object, [contenteditable]:not([contenteditable="false"]), [role="button"], [role="link"]';
  const ownText = (el) => [...el.childNodes].some((n) => n.nodeType === 3 && n.data.trim() !== '');
  const covers = (r, x, y) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
  // {image, above} when what's under the pointer is only see-through covers over a picture
  // (no text of their own, nothing to click, so making them let clicks through takes
  // nothing from the page), else null.
  function coveredImage(x, y) {
    const stack = document.elementsFromPoint(x, y);
    if (!stack.length || stack[0] instanceof HTMLImageElement) return null;
    let at = stack.findIndex((el) => el instanceof HTMLImageElement);
    let image = at >= 0 ? stack[at] : null;
    // (One with `pointer-events: none` isn't in the stack.)
    if (!image) {
      for (const el of stack) {
        if (el === document.documentElement) break;
        image = [...el.querySelectorAll('img')].find((i) => covers(i.getBoundingClientRect(), x, y)) || null;
        if (image) break;
      }
    }
    if (!image || !(image.currentSrc || image.src)) return null;
    const r = image.getBoundingClientRect();
    if (r.width < 48 || r.height < 48) return null;
    const above = (at >= 0 ? stack.slice(0, at) : stack).filter((el) => !el.contains(image));
    for (const el of above) if (el.matches(INTERACTIVE) || el.closest('a[href], button') || ownText(el)) return null;
    return { image, above };
  }
  // Inline values to put back: [element, value, priority].
  let changed = [];
  function letThrough(found) {
    const set = (el, v) => {
      changed.push([el, el.style.getPropertyValue('pointer-events'), el.style.getPropertyPriority('pointer-events')]);
      el.style.setProperty('pointer-events', v, 'important');
    };
    for (const el of found.above) set(el, 'none');
    if (getComputedStyle(found.image).pointerEvents === 'none') set(found.image, 'auto');
    setTimeout(putBack, 2000);
  }
  function putBack() {
    for (const [el, v, p] of changed.reverse()) {
      if (v) el.style.setProperty('pointer-events', v, p);
      else el.style.removeProperty('pointer-events');
    }
    changed = [];
  }

  // ---- the page's own menu, or none ----
  // Whether the page showed something near the pointer (its own menu, a notice) in the
  // records: an element added or changed that is now on screen around there.
  function showedSomething(records, x, y) {
    const near = (el) => {
      if (!(el instanceof Element) || !el.isConnected) return false;
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && r.right > x - 600 && r.left < x + 600 && r.bottom > y - 600 && r.top < y + 600;
    };
    for (const r of records) {
      if (r.type === 'childList') { for (const n of r.addedNodes) if (near(n) || (n.nodeType === 3 && near(n.parentElement))) return true; }
      else if (near(r.type === 'characterData' ? r.target.parentElement : r.target)) return true;
    }
    return false;
  }

  // The rest of a passing click (or a Shift one) doesn't reach the page either: some
  // pages complain on the button going down.
  for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'auxclick']) {
    window.addEventListener(type, (e) => {
      if (e.isTrusted && e.button === 2 && (e.shiftKey || passing())) e.stopImmediatePropagation();
    }, true);
  }
  window.addEventListener('contextmenu', (e) => {
    if (!e.isTrusted) return;
    if (passing()) {
      passUntil = 0;
      e.stopImmediatePropagation();
      setTimeout(putBack, 0);
      return;
    }
    if (e.shiftKey) { e.stopImmediatePropagation(); return; }
    // (From the keyboard there's no click to replay.)
    if (!AUTO || e.button !== 2) return;
    const x = e.clientX, y = e.clientY;
    const found = coveredImage(x, y);
    if (found) {
      e.preventDefault();
      e.stopImmediatePropagation();
      letThrough(found);
      replay();
      return;
    }
    // Games and drawing apps use the button themselves.
    if (e.target instanceof HTMLCanvasElement || e.target instanceof HTMLEmbedElement || e.target instanceof HTMLObjectElement) return;
    const records = [];
    const watch = new MutationObserver((r) => { records.push(...r); });
    watch.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
    setTimeout(() => {
      records.push(...watch.takeRecords());
      watch.disconnect();
      if (!e.defaultPrevented || document.pointerLockElement || document.fullscreenElement) return;
      if (!showedSomething(records, x, y)) replay();
    }, 80);
  }, true);
})();
