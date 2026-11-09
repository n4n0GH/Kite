// Selecting and copying text on pages that keep it from the user (isolated world, every frame,
// from document creation, so its listeners come before any of the page's; "web.unblockSelection").
//
//  * Copying a selection of the page's text goes past the page: it can neither cancel the copy
//    nor change what's copied ("Read more at …"). Ctrl+C for one too, for pages swallowing the key.
//  * A page that stops its text being selected (user-select: none, or cancelling selectstart or
//    the button going down) selects after all when the user clearly means to: by dragging across
//    text, or double-clicking a word. The text it was stopped on stays selectable from then on.
//
// The page's text is text an element holds itself, outside anything to click, drag or type in:
// apps that keep their controls from being selected (sliders, drag handles, lists of rows) and
// editors keep doing so, and their own copying.
(function () {
  'use strict';
  if (globalThis.__kiteTextUnblock) return;
  globalThis.__kiteTextUnblock = true;

  const CONTROLS = 'a[href], button, input, select, textarea, label, summary, option, video, audio, canvas, svg, iframe, embed, object, [draggable="true"], [contenteditable]:not([contenteditable="false"]), [role="button"], [role="link"], [role="tab"], [role="menuitem"], [role="option"], [role="slider"], [role="scrollbar"], [role="separator"], [role="checkbox"], [role="radio"], [role="switch"], [role="treeitem"], [role="gridcell"], [role="row"], [role="listbox"], [role="application"]';
  const elementOf = (n) => (n && n.nodeType === 1 ? n : n && n.parentElement);
  const editable = (n) => { const el = elementOf(n); return !!el && (el.isContentEditable || !!el.closest('input, textarea')); };
  const ownText = (el) => [...el.childNodes].some((n) => n.nodeType === 3 && n.data.trim() !== '');
  // The element of the page's text at an event's target, or null.
  function pageText(target) {
    const el = elementOf(target);
    if (!el || !ownText(el) || el.closest(CONTROLS)) return null;
    return el;
  }
  // The selection, when it's of the page's text (not in a field or editor).
  function textSelection() {
    const s = getSelection();
    if (!s || s.isCollapsed || !s.rangeCount || editable(s.anchorNode) || editable(s.focusNode)) return null;
    return s.toString().trim() ? s : null;
  }

  // ---- copying ----
  for (const type of ['copy', 'cut']) {
    window.addEventListener(type, (e) => { if (e.isTrusted && textSelection()) e.stopImmediatePropagation(); }, true);
  }
  window.addEventListener('keydown', (e) => {
    if (!e.isTrusted || !(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey || e.code !== 'KeyC') return;
    if (textSelection()) e.stopImmediatePropagation();
  }, true);

  // ---- selecting ----
  // The text the page kept from being selected, once the user went for it: selectable
  // from then on (from the outermost element making it unselectable).
  let sheet = null;
  function release(el) {
    let scope = el;
    while (scope.parentElement && getComputedStyle(scope.parentElement).userSelect === 'none') scope = scope.parentElement;
    if (getComputedStyle(scope).userSelect !== 'none') return;
    scope.setAttribute('data-kite-selectable', '');
    if (!sheet || !sheet.isConnected) {
      sheet = document.createElement('style');
      sheet.textContent = '[data-kite-selectable], [data-kite-selectable] * { -webkit-user-select: text !important; user-select: text !important; }';
      (document.head || document.documentElement).appendChild(sheet);
    }
  }
  const caretAt = (x, y) => document.caretRangeFromPoint(x, y);

  // A press on the page's text: what a drag from it, or a double-click on it, should select.
  let press = null;   // {x, y, el, caret, manual}
  window.addEventListener('mousedown', (e) => {
    press = null;
    if (!e.isTrusted || e.button !== 0 || e.ctrlKey || e.metaKey || e.altKey) return;
    const el = pageText(e.target);
    if (el) press = { x: e.clientX, y: e.clientY, el, caret: caretAt(e.clientX, e.clientY), manual: false };
  }, true);
  // Started on the page's text, a selection isn't the page's to cancel.
  window.addEventListener('selectstart', (e) => {
    if (e.isTrusted && press && elementOf(e.target) && press.el.contains(elementOf(e.target))) e.stopImmediatePropagation();
  }, true);
  window.addEventListener('dragstart', () => { press = null; }, true);
  window.addEventListener('mousemove', (e) => {
    if (!press || !(e.buttons & 1)) { press = null; return; }
    if (!press.manual) {
      if (Math.abs(e.clientX - press.x) + Math.abs(e.clientY - press.y) < 8) return;
      // Dragged across text, and nothing is being selected: the page stops it.
      const s = getSelection();
      if (!s || !s.isCollapsed || !press.caret) { press = null; return; }
      release(press.el);
      press.manual = true;
    }
    // (This time by hand: the press that began it went by without selecting.)
    const to = caretAt(e.clientX, e.clientY);
    if (to) getSelection().setBaseAndExtent(press.caret.startContainer, press.caret.startOffset, to.startContainer, to.startOffset);
  }, true);
  window.addEventListener('mouseup', () => { press = null; }, true);
  window.addEventListener('dblclick', (e) => {
    if (!e.isTrusted || e.button !== 0) return;
    const el = pageText(e.target);
    const s = getSelection();
    if (!el || !s || !s.isCollapsed) return;
    const at = caretAt(e.clientX, e.clientY);
    if (!at) return;
    release(el);
    s.collapse(at.startContainer, at.startOffset);
    s.modify('move', 'backward', 'word');
    s.modify('extend', 'forward', 'word');
  }, true);
})();
