// Pointer lock for web pages (element.requestPointerLock()), in every frame.
//
// QtWebEngine refuses pointer lock without asking, so Kite does it: the
// compositor holds the pointer still and reports how the mouse moves
// (cpp/pointerlock.cpp), and this main-world script plays the browser's part:
// document.pointerLockElement, the pointerlockchange and pointerlockerror
// events, and mouse movement as pointermove and mousemove events (with
// movementX and movementY) on the locked element. It asks Kite through DOM
// events relayed by the isolated world (js/pointerlock_relay.js), which
// checks what matters: the user's click, and the frame it came from.
(function () {
  'use strict';
  if (!Element.prototype.requestPointerLock) return;
  const REQUEST = 'kite-pl-request', EXIT = 'kite-pl-exit', DOWN = 'kite-pl-down';
  const send = (name, detail) => document.dispatchEvent(new CustomEvent(name, { detail: JSON.stringify(detail) }));

  let locked = null;          // the element holding the lock
  let pending = null;         // {id, el, resolve, reject}
  let nextId = 1;
  let restX = 0, restY = 0;   // movement below a pixel, carried over
  // Where the pointer was and what was held, from the page's real events:
  // movement events carry them on.
  const at = { clientX: 0, clientY: 0, screenX: 0, screenY: 0, buttons: 0, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false };
  const note = (e) => {
    if (!e.isTrusted) return;
    if (e.clientX !== undefined) Object.assign(at, { clientX: e.clientX, clientY: e.clientY, screenX: e.screenX, screenY: e.screenY, buttons: e.buttons });
    Object.assign(at, { ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey });
  };
  for (const t of ['mousemove', 'mousedown', 'mouseup', 'keydown', 'keyup']) addEventListener(t, note, { capture: true, passive: true });
  // While locked, movement comes only from Kite. The browser's own move events
  // keep coming as the page redraws (Qt Quick sends the pointer's last position
  // again, with no button held), and Chromium gives them a small movement, which
  // turned games' views steadily: the page never sees them.
  const still = (e) => { if (locked && e.isTrusted) e.stopImmediatePropagation(); };
  for (const t of ['mousemove', 'pointermove', 'pointerrawupdate']) addEventListener(t, still, { capture: true });

  const changed = () => document.dispatchEvent(new Event('pointerlockchange', { bubbles: true, composed: true }));
  const failed = () => document.dispatchEvent(new Event('pointerlockerror', { bubbles: true, composed: true }));
  function release() {
    if (!locked) return false;
    locked = null;
    restX = restY = 0;
    changed();
    return true;
  }

  function move(dx, dy) {
    const el = locked;
    if (!el) return;
    if (!el.isConnected || document.hidden) { exit(); return; }
    restX += dx; restY += dy;
    const mx = Math.trunc(restX), my = Math.trunc(restY);
    if (mx === 0 && my === 0) return;
    restX -= mx; restY -= my;
    const init = {
      bubbles: true, cancelable: true, composed: true, view: window,
      clientX: at.clientX, clientY: at.clientY, screenX: at.screenX, screenY: at.screenY,
      movementX: mx, movementY: my, buttons: at.buttons, button: 0,
      ctrlKey: at.ctrlKey, shiftKey: at.shiftKey, altKey: at.altKey, metaKey: at.metaKey,
    };
    el.dispatchEvent(new PointerEvent('pointermove', { ...init, button: -1, pointerId: 1, pointerType: 'mouse', isPrimary: true, width: 1, height: 1, pressure: at.buttons ? 0.5 : 0 }));
    el.dispatchEvent(new MouseEvent('mousemove', init));
  }

  // What Kite answers (relayed by the isolated world).
  document.addEventListener(DOWN, (e) => {
    let m; try { m = JSON.parse(e.detail); } catch (x) { return; }
    if (m.m === 'move') { move(+m.a[0] || 0, +m.a[1] || 0); return; }
    if (m.m === 'answer') {
      const p = pending;
      if (!p || p.id !== m.a.id) return;
      pending = null;
      if (m.a.error) {
        failed();
        p.reject(new DOMException(m.a.error, 'NotAllowedError'));
        return;
      }
      if (!p.el.isConnected) { send(EXIT, {}); failed(); p.reject(new DOMException('The element was removed.', 'WrongDocumentError')); return; }
      const before = locked;
      locked = p.el;
      restX = restY = 0;
      if (before !== locked) changed();
      p.resolve();
      return;
    }
    if (m.m === 'lost') {
      if (pending) { const p = pending; pending = null; failed(); p.reject(new DOMException('The pointer lock request was cancelled.', 'NotAllowedError')); }
      release();
    }
  });

  function exit() {
    if (!locked) return;
    send(EXIT, {});
    // As in browsers, the change event comes after the call returns.
    const was = locked;
    setTimeout(() => { if (locked === was) release(); }, 0);
  }

  Object.defineProperty(Element.prototype, 'requestPointerLock', {
    configurable: true, writable: true,
    value: function requestPointerLock(options) {
      const el = this;
      return new Promise((resolve, reject) => {
        if (!el.isConnected) {
          setTimeout(failed, 0);
          reject(new DOMException('Target Element removed from DOM', 'WrongDocumentError'));
          return;
        }
        if (locked === el && !(options && options.unadjustedMovement)) { resolve(); return; }
        if (pending) {
          setTimeout(failed, 0);
          reject(new DOMException('Pointer lock already in the process of being requested.', 'InUseAttributeError'));
          return;
        }
        pending = { id: nextId++, el, resolve, reject };
        send(REQUEST, { id: pending.id, unadjusted: !!(options && options.unadjustedMovement) });
      });
    },
  });

  // (The element as seen from `root`: a shadow tree's host, outside it.)
  function seenFrom(root) {
    let el = locked;
    while (el && el.getRootNode() !== root) {
      const r = el.getRootNode();
      el = r instanceof ShadowRoot ? r.host : null;
    }
    return el;
  }
  Object.defineProperty(Document.prototype, 'pointerLockElement', {
    configurable: true, enumerable: true,
    get() { return this === document ? seenFrom(this) : null; },
  });
  if (typeof ShadowRoot !== 'undefined') {
    Object.defineProperty(ShadowRoot.prototype, 'pointerLockElement', {
      configurable: true, enumerable: true,
      get() { return locked && locked.getRootNode() !== document ? seenFrom(this) : null; },
    });
  }
  Object.defineProperty(Document.prototype, 'exitPointerLock', {
    configurable: true, writable: true,
    value: function exitPointerLock() { if (this === document) exit(); },
  });

  document.addEventListener('visibilitychange', () => { if (document.hidden) exit(); });
  addEventListener('pagehide', exit);
})();
