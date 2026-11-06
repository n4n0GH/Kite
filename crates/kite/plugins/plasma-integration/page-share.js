// What's asked of navigator.share (page-share-world.js), for PlasmaWindow.qml. What
// reaches the window has passed here: it was a click or a key press, not the page
// deciding by itself, and it's a page that gets to ask.
(function () {
  'use strict';
  if (globalThis.__kitePlasmaShare || !KITE_PLUGIN_SETTINGS.share || !window.isSecureContext) return;
  const frame = Math.random().toString(36).slice(2);
  // As in the specification: not what's only meaningful here, or runs something.
  const REFUSED = ['about:', 'blob:', 'data:', 'file:', 'javascript:', 'ws:', 'wss:'];
  const MOST = { title: 1000, text: 65536, url: 8192 };
  let asking = null;

  function answer(id, error, message) {
    window.dispatchEvent(new CustomEvent('kite-plasma-share-done', { detail: JSON.stringify({ id, error: error || '', message: message || '' }) }));
  }
  // Not frames of other sites: they'd need allow="web-share", which this Chromium
  // doesn't know, so there's no telling whether they were let.
  function allowed() {
    try { return window === window.top || window.top.location.origin === location.origin; } catch (e) { return false; }
  }
  window.addEventListener('kite-plasma-share', (e) => {
    let ask;
    try { ask = JSON.parse(e.detail); } catch (error) { return; }
    const { id } = ask;
    if (ask.files) return answer(id, 'TypeError', "Sharing files isn't supported");
    const data = {};
    for (const key of ['title', 'text']) if (typeof ask[key] === 'string' && ask[key]) data[key] = ask[key];
    if (typeof ask.url === 'string') {
      let url;
      try { url = new URL(ask.url, ask.base); } catch (error) { return answer(id, 'TypeError', 'Not a valid address'); }
      if (REFUSED.includes(url.protocol)) return answer(id, 'TypeError', 'Not an address that can be shared');
      data.url = url.href;
    }
    if (!data.title && !data.text && !data.url) return answer(id, 'TypeError', 'Nothing to share');
    for (const key of Object.keys(data)) if (data[key].length > MOST[key]) return answer(id, 'TypeError', 'Too long to share');
    if (!allowed()) return answer(id, 'NotAllowedError', "This frame isn't allowed to share");
    if (!navigator.userActivation || !navigator.userActivation.isActive) return answer(id, 'NotAllowedError', 'Sharing has to come from a click or a key press');
    if (asking !== null) return answer(id, 'InvalidStateError', 'Already sharing');
    asking = id;
    kitePluginSend({ cmd: 'share', frame, data });
  });
  Object.defineProperty(globalThis, '__kitePlasmaShare', { value: {
    done(forFrame, error) {
      if (forFrame !== frame || asking === null) return;
      const id = asking;
      asking = null;
      answer(id, error, error ? 'Share cancelled' : '');
    },
  } });
  window.dispatchEvent(new CustomEvent('kite-plasma-share-on'));
})();
