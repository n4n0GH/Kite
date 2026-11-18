// Overlay scrollbars everywhere ("appearance.overlayScrollbars").
//
// Kite turns on Chromium's overlay scrollbars (thin, wider on hover, hidden
// while idle; see main.rs). Pages that style their scrollbars with
// ::-webkit-scrollbar get their own, classic scrollbars instead, so this
// script hands those back to the overlay style: an element with a
// `scrollbar-color` ignores its ::-webkit-scrollbar styles.
//
// Only scrollbars the page shows are switched. Under overlay scrollbars
// only custom ones take layout space, so that's how they're found; ones a
// page hides (display: none, zero size, scrollbar-width: none) take none
// and stay hidden.
//
// They are found when they matter, not by going through the page: the
// page's own (of its root and body) as it loads, any other one when the
// pointer comes over it or it scrolls. Until then such a one keeps the look
// its page gave it. Asking every element of every page for its scroll size,
// and again whenever the page changed, cost about 4 MiB a page in memory
// and work every second on pages that never stand still.
(() => {
  'use strict';
  if (window.__kiteScrollbars) return;
  window.__kiteScrollbars = true;

  const ATTR = 'data-kite-scrollbar';
  // A grey that reads on light and dark pages alike.
  const CSS = `[${ATTR}] { scrollbar-color: rgb(128 128 128 / 0.75) transparent !important; }`;

  // The rule goes in with the first scrollbar it's for: most documents have none, and are
  // left as their pages made them.
  let style = null;
  function addStyle() {
    if (style && style.isConnected) return;
    const root = document.documentElement;
    if (!root) return;
    style = document.createElement('style');
    style.textContent = CSS;
    // Not in <head>: pages rebuilding it keep the rule.
    root.appendChild(style);
  }

  // Whether `el` hides one of its scrollbars with ::-webkit-scrollbar
  // (switching it would bring that one back).
  function hidesOne(el) {
    const s = getComputedStyle(el, '::-webkit-scrollbar');
    return s.display === 'none' || s.width === '0px' || s.height === '0px';
  }

  function check(el) {
    if (el.hasAttribute(ATTR)) return;
    // Space between the border and the content box taken by a scrollbar.
    const isRoot = el === document.documentElement;
    const vertical = isRoot ? window.innerWidth - el.clientWidth : el.offsetWidth - el.clientWidth - 2 * el.clientLeft;
    const horizontal = isRoot ? window.innerHeight - el.clientHeight : el.offsetHeight - el.clientHeight - 2 * el.clientTop;
    if (vertical <= 0 && horizontal <= 0) return;
    if (hidesOne(el)) return;
    addStyle();
    el.setAttribute(ATTR, '');
  }

  // The page's own scrollbars. (Asked again as it loads and now and then
  // after: a page may be given its style, or enough to scroll, later.)
  function checkPage() {
    const root = document.documentElement;
    if (!root) return;
    check(root);
    if (document.body) check(document.body);
  }

  // What the pointer is over, and what that is inside of: each looked at once.
  // (One that was looked at had all it is inside of looked at with it.)
  const seen = new WeakSet();
  let pageChecked = 0;
  function checkAround(target) {
    if (Date.now() - pageChecked > 1000) {
      pageChecked = Date.now();
      checkPage();
    }
    for (let el = target; el instanceof Element && !seen.has(el); el = el.parentElement) {
      seen.add(el);
      check(el);
    }
  }

  function start() {
    checkPage();
    document.addEventListener('DOMContentLoaded', checkPage);
    window.addEventListener('load', checkPage);
    // Whatever scrolls is checked right away.
    document.addEventListener('scroll', e => {
      const t = e.target === document ? document.documentElement : e.target;
      if (t instanceof Element) check(t);
    }, {capture: true, passive: true});
    document.addEventListener('pointerover', e => checkAround(e.target), {capture: true, passive: true});
  }

  // (Right away, though the document may have no root element yet: the listeners need none,
  // and the page is looked at again as it loads. Waiting for the root with a
  // MutationObserver would cost the page for good: once one has watched a document, the
  // engine looks for observers at every change made to it, also after that one is gone.)
  start();
})();
