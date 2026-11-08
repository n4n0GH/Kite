// Pointer lock, the isolated world's part (js/pointerlock.js is the page's):
// passes a frame's requests to Kite and Kite's answers back.
//
// Up, messages go to the browser as console messages from this world, which
// page scripts can neither see nor (lacking the secret) imitate; down, Kite
// runs __kitePlDown here through Qt's frame object. A page asks only on the
// user's click (as browsers require), or right after letting go itself.
(function () {
  'use strict';
  const SECRET = '__KITE_FRAME_SECRET__';
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  const id = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  const up = (m, a) => console.debug(SECRET + 'pl:' + JSON.stringify({ f: id, m, a }));
  const down = (m, a) => document.dispatchEvent(new CustomEvent('kite-pl-down', { detail: JSON.stringify({ m, a }) }));
  const parse = (e) => { try { return JSON.parse(e.detail); } catch (x) { return null; } };
  let state = 'none';         // 'none' | 'asking' | 'locked'
  let letGoBySelf = false;    // the page let go: it may lock again unasked

  globalThis.__kitePlId = id;
  globalThis.__kitePlDown = (m, a) => {
    if (m === 'move') {
      // A document that isn't the one that asked (the frame moved on).
      if (state !== 'locked') { up('unlock'); return; }
      down('move', a);
      return;
    }
    if (m === 'answer') {
      if (state !== 'asking') return;
      state = a.error ? 'none' : 'locked';
      if (!a.error) letGoBySelf = false;
    } else if (m === 'lost') {
      state = 'none';
    }
    down(m, a);
  };

  document.addEventListener('kite-pl-request', (e) => {
    const r = parse(e);
    if (!r || typeof r.id !== 'number') return;
    const activated = navigator.userActivation ? navigator.userActivation.isActive : true;
    if (!activated && !letGoBySelf && state !== 'locked') {
      down('answer', { id: r.id, error: 'A page can lock the mouse pointer only when you click in it.' });
      return;
    }
    state = 'asking';
    up('lock', { id: r.id, unadjusted: !!r.unadjusted });
  });
  document.addEventListener('kite-pl-exit', () => {
    if (state === 'none') return;
    if (state === 'locked') letGoBySelf = true;
    state = 'none';
    up('unlock');
  });
})();
