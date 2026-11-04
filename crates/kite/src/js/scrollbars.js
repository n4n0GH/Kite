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
(() => {
  'use strict';
  if (window.__kiteScrollbars) return;
  window.__kiteScrollbars = true;

  const ATTR = 'data-kite-scrollbar';
  // A grey that reads on light and dark pages alike.
  const CSS = `[${ATTR}] { scrollbar-color: rgb(128 128 128 / 0.75) transparent !important; }`;

  function addStyle() {
    const root = document.documentElement;
    if (!root) return false;
    const style = document.createElement('style');
    style.textContent = CSS;
    // Not in <head>: pages rebuilding it keep the rule.
    root.appendChild(style);
    return true;
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
    el.setAttribute(ATTR, '');
  }

  function scan() {
    const root = document.documentElement;
    if (!root) return;
    check(root);
    if (document.body) check(document.body);
    for (const el of document.body ? document.body.getElementsByTagName('*') : []) {
      // Cheap filter first: only elements that can scroll.
      if (el.scrollHeight <= el.clientHeight && el.scrollWidth <= el.clientWidth) continue;
      check(el);
    }
  }

  // Scans run when the page is idle, at most every second while it changes.
  let pending = false;
  let last = 0;
  function schedule() {
    if (pending) return;
    pending = true;
    const wait = Math.max(0, last + 1000 - Date.now());
    setTimeout(() => requestIdleCallback(() => {
      pending = false;
      last = Date.now();
      scan();
    }, {timeout: 1000}), wait);
  }

  function start() {
    if (!addStyle()) return;
    schedule();
    new MutationObserver(schedule).observe(document.documentElement, {childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'style']});
    // Whatever scrolls is checked right away.
    document.addEventListener('scroll', e => {
      const t = e.target === document ? document.documentElement : e.target;
      if (t instanceof Element) check(t);
    }, {capture: true, passive: true});
    window.addEventListener('load', schedule);
  }

  if (document.documentElement) start();
  else new MutationObserver((_, obs) => { if (document.documentElement) { obs.disconnect(); start(); } }).observe(document, {childList: true});
})();
